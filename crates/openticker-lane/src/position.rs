use crate::state::LaneRuntime;
use openticker_core::{OhlcvBar, TradeIntent};
use openticker_execution::{AcceptedOrder, OrderLedgerOutcome};
use openticker_ledger::{
    FeeEntry, InventoryError, InventoryFillSide, InventoryState, LedgerOwnerPath,
    calculate_position_notional_usd, sanitize_ledger_value,
};
use openticker_risk::RiskDecision;

const POSITION_QUANTITY_TOLERANCE: f64 = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionRecordState {
    pub has_position: bool,
    pub quantity: f64,
    pub entry_price: Option<f64>,
    pub realized_pnl_usd: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessBarStateMutation {
    pub position_record: Option<PositionRecordState>,
    pub released_notional_usd: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryTransitionFailure {
    pub action: &'static str,
    pub error: InventoryError,
}

#[must_use]
pub fn ledger_owner_path(instance: &LaneRuntime) -> LedgerOwnerPath {
    LedgerOwnerPath::new(
        instance.config.account.clone(),
        instance.config.id.clone(),
        instance.lane_symbol.clone(),
    )
}

#[must_use]
pub fn inventory_state_from_runtime_fields(
    position_quantity: f64,
    entry_price: Option<f64>,
    realized_pnl_usd: f64,
) -> InventoryState {
    InventoryState::from_position_state(position_quantity, entry_price, realized_pnl_usd)
}

pub fn sync_inventory_from_runtime_fields(instance: &mut LaneRuntime) {
    instance.inventory = inventory_state_from_runtime_fields(
        instance.position_quantity,
        instance.entry_price,
        instance.realized_pnl_usd,
    );
}

pub fn sync_runtime_fields_from_inventory(
    instance: &mut LaneRuntime,
    valuation_price: Option<f64>,
) {
    // Capture the *incoming* (pre-sync) state before any field is overwritten.
    // The genuine state-consistency anomaly this boundary guards against is a
    // lane that arrived here claiming `has_position == true` while BOTH of its
    // effective quantity sources (the ledger inventory and the cached
    // `position_quantity` field) are within tolerance of zero. That divergence
    // can be produced across the public boundary — e.g. a reconciliation
    // assessment that resolves `has_position = true` together with a ~0
    // resolved quantity (see `openticker-runtime`
    // `apply_reconciliation_assessment_state`). That assessment persists the
    // divergent fields on the lane; the anomaly is then observed here at the
    // next fill-driven sync. It is exactly the scenario that previously caused
    // `effective_position_quantity` to fabricate a quantity. Detecting it here, at the single sync boundary, lets that
    // accessor stay a clean, side-effect-free `0.0` fallback while still
    // surfacing the anomaly.
    let pre_sync_has_position = instance.has_position;
    let pre_sync_inventory_quantity = instance.inventory.quantity();
    let pre_sync_cached_quantity = instance.position_quantity;
    let inconsistent_on_entry = pre_sync_has_position
        && pre_sync_inventory_quantity <= POSITION_QUANTITY_TOLERANCE
        && pre_sync_cached_quantity <= POSITION_QUANTITY_TOLERANCE;

    instance.position_quantity = instance.inventory.quantity();
    instance.has_position = instance.position_quantity > POSITION_QUANTITY_TOLERANCE;
    instance.entry_price = instance.inventory.average_cost_usd();
    instance.realized_pnl_usd = instance.inventory.realized_pnl.net_usd;
    instance.position_notional_usd = if instance.has_position {
        instance.inventory.position_notional_usd(
            valuation_price
                .filter(|price| price.is_finite() && *price > 0.0)
                .or(instance.entry_price),
        )
    } else {
        0.0
    };

    // Record the anomaly through a release-visible channel. `recovery_last_error`
    // is the idiomatic structured lane-state marker for an operator-facing
    // anomaly: it surfaces to runtime recovery summaries (see `openticker-runtime`
    // `repo::summaries`) and is the same field `mark_lane_out_of_sync_state`
    // writes. We record it once, here, at the detection point — the read-only
    // `effective_position_quantity` accessor deliberately does NOT re-record, so
    // the inconsistency has a single coherent story. The sync above has already
    // collapsed the lane to a coherent flat state (`has_position == false`,
    // quantity/notional zeroed), so the recovered position is safe; the marker
    // exists purely so the prior divergence is visible in production, where the
    // `debug_assert!` below is compiled out.
    // Last-writer-wins overwrite is intentional: this anomaly is itself a
    // flat-reset signal, and surfacing it takes precedence over any prior
    // recovery marker for this cycle.
    if inconsistent_on_entry {
        instance.recovery_last_error = Some(format!(
            "lane position-quantity invariant violated during inventory sync: \
             account={} instance={} symbol={} had has_position=true while both \
             quantity sources were ~0 (inventory_quantity={pre_sync_inventory_quantity}, \
             cached_position_quantity={pre_sync_cached_quantity}); reset to flat",
            instance.config.account, instance.config.id, instance.lane_symbol,
        ));
    }
    // Invariant the release-visible marker above guards: a synced lane must
    // never present `has_position == true` alongside a zero effective quantity.
    // Kept as a debug-only tripwire in addition to the marker, never as the
    // only signal.
    debug_assert!(
        !(instance.has_position && instance.position_quantity <= POSITION_QUANTITY_TOLERANCE),
        "lane position-quantity invariant violated after sync: has_position={} but position_quantity={}",
        instance.has_position,
        instance.position_quantity
    );
}

#[must_use]
pub fn sync_remote_position_quantity(instance: &mut LaneRuntime, remote_quantity: f64) -> bool {
    let has_position = remote_quantity > POSITION_QUANTITY_TOLERANCE;
    let local_quantity = effective_position_quantity(instance);
    let changed = instance.has_position != has_position
        || (local_quantity - remote_quantity).abs() > POSITION_QUANTITY_TOLERANCE;
    if !changed {
        return false;
    }

    instance.has_position = has_position;
    instance.position_quantity = remote_quantity;
    if has_position {
        instance.position_notional_usd = instance.entry_price.map_or(0.0, |entry_price| {
            calculate_position_notional_usd(remote_quantity, entry_price)
        });
    } else {
        instance.position_notional_usd = 0.0;
        instance.entry_price = None;
    }
    sync_inventory_from_runtime_fields(instance);

    true
}

#[must_use]
pub fn accepted_order_fee_entry(order: &AcceptedOrder) -> Option<FeeEntry> {
    let fee_asset = order.fee_asset.clone()?;
    let fee_amount = order
        .fee_amount
        .filter(|value| value.is_finite() && *value > 0.0)?;
    Some(FeeEntry {
        asset: fee_asset,
        amount: fee_amount,
        normalized_usd: order
            .fee_normalized_usd
            .filter(|value| value.is_finite() && *value > 0.0),
    })
}

/// Returns the position quantity the lane should treat as authoritative,
/// preferring the ledger inventory, then the cached `position_quantity` field.
///
/// # Invariant
///
/// Whenever `has_position` is true the lane is expected to also carry a
/// non-zero quantity in either the inventory or the `position_quantity`
/// field (see `sync_runtime_fields_from_inventory`, where the two are kept in
/// lockstep). If that invariant is ever violated — `has_position == true`
/// while both quantity sources are within `POSITION_QUANTITY_TOLERANCE` of
/// zero — this function deliberately returns `0.0` rather than fabricating a
/// quantity. Returning a fabricated non-zero value here previously corrupted
/// downstream position-notional and order-sizing math, so `0.0` is the only
/// safe answer: it makes notional zero and prevents fabricated sizing. The
/// inconsistency itself is detected and recorded at the state-sync boundary
/// (`sync_runtime_fields_from_inventory`, via the release-visible
/// `recovery_last_error` marker plus a debug-only tripwire), not here, so that
/// this read-only accessor stays a side-effect-free, panic-free `0.0` fallback
/// that never double-records the anomaly.
#[must_use]
pub fn effective_position_quantity(instance: &LaneRuntime) -> f64 {
    if instance.inventory.quantity() > POSITION_QUANTITY_TOLERANCE {
        instance.inventory.quantity()
    } else if instance.position_quantity > POSITION_QUANTITY_TOLERANCE {
        instance.position_quantity
    } else {
        // Defensive: `has_position` may be true here only if the
        // quantity-consistency invariant was violated upstream. Never
        // fabricate a quantity; return zero so notional collapses to zero.
        0.0
    }
}

#[must_use]
pub fn current_instance_open_notional_usd(instance: &LaneRuntime) -> f64 {
    sanitize_ledger_value(instance.position_notional_usd)
}

fn position_record_state(instance: &LaneRuntime) -> PositionRecordState {
    PositionRecordState {
        has_position: instance.has_position,
        quantity: effective_position_quantity(instance),
        entry_price: instance.entry_price,
        realized_pnl_usd: instance.realized_pnl_usd,
    }
}

fn apply_open_long_fill(
    instance: &mut LaneRuntime,
    bar: &OhlcvBar,
    order: &AcceptedOrder,
) -> Result<(), InventoryTransitionFailure> {
    let fill_quantity = order.quantity.max(0.0);
    if fill_quantity <= f64::EPSILON {
        return Ok(());
    }

    let previous_quantity = effective_position_quantity(instance);
    let fee_entry = accepted_order_fee_entry(order);
    sync_inventory_from_runtime_fields(instance);
    let average_cost = instance.inventory.average_cost_usd();

    if average_cost.is_some() || previous_quantity <= POSITION_QUANTITY_TOLERANCE {
        instance
            .inventory
            .apply_fill(
                InventoryFillSide::Buy,
                fill_quantity,
                order.price,
                fee_entry.as_ref(),
            )
            .map_err(|error| InventoryTransitionFailure {
                action: "open_fill",
                error,
            })?;
        sync_runtime_fields_from_inventory(instance, Some(bar.close));
    } else {
        let previous_entry_price = instance.entry_price.unwrap_or(order.price);
        let new_quantity = previous_quantity + fill_quantity;
        let new_entry_price = ((previous_quantity * previous_entry_price)
            + (fill_quantity * order.price))
            / new_quantity;

        instance.position_quantity = new_quantity;
        instance.position_notional_usd = calculate_position_notional_usd(new_quantity, bar.close);
        instance.entry_price = Some(new_entry_price);
        instance.has_position = new_quantity > f64::EPSILON;
        sync_inventory_from_runtime_fields(instance);
    }

    Ok(())
}

fn apply_close_long_fill(
    instance: &mut LaneRuntime,
    bar: &OhlcvBar,
    order: &AcceptedOrder,
) -> Result<f64, InventoryTransitionFailure> {
    let previous_quantity = effective_position_quantity(instance);
    let fee_entry = accepted_order_fee_entry(order);
    sync_inventory_from_runtime_fields(instance);
    let fill_quantity = order.quantity.max(0.0).min(previous_quantity);
    let released_notional_usd = calculate_position_notional_usd(fill_quantity, bar.close);
    let average_cost = instance.inventory.average_cost_usd();

    if let Some(entry_price) = average_cost
        && entry_price > 0.0
        && previous_quantity > f64::EPSILON
        && fill_quantity > f64::EPSILON
    {
        let pnl_pct = ((order.price - entry_price) / entry_price) * 100.0;
        if pnl_pct < 0.0 {
            let closed_fraction = (fill_quantity / previous_quantity).clamp(0.0, 1.0);
            instance.daily_loss_pct_accumulated += -pnl_pct * closed_fraction;
        }
    }

    if fill_quantity > f64::EPSILON {
        if average_cost.is_some() {
            instance
                .inventory
                .apply_fill(
                    InventoryFillSide::Sell,
                    fill_quantity,
                    order.price,
                    fee_entry.as_ref(),
                )
                .map_err(|error| InventoryTransitionFailure {
                    action: "close_fill",
                    error,
                })?;
            sync_runtime_fields_from_inventory(instance, Some(bar.close));
        } else {
            let remaining_quantity = (previous_quantity - fill_quantity).max(0.0);
            instance.position_quantity = remaining_quantity;
            instance.position_notional_usd =
                calculate_position_notional_usd(remaining_quantity, bar.close);
            instance.has_position = remaining_quantity > f64::EPSILON;
            if !instance.has_position {
                instance.entry_price = None;
            }
            sync_inventory_from_runtime_fields(instance);
        }
    }

    Ok(released_notional_usd)
}

/// Applies the fill-state transition for a lane after risk has resolved the
/// trade intent.
///
/// # Errors
///
/// Returns an inventory transition failure when the lane's local inventory
/// cannot absorb the accepted buy or sell fill.
pub fn apply_process_bar_fill_state(
    instance: &mut LaneRuntime,
    bar: &OhlcvBar,
    next_has_position: bool,
    accepted_order: Option<&AcceptedOrder>,
    order_ledger_outcome: Option<OrderLedgerOutcome>,
    risk_decision: &RiskDecision,
) -> Result<ProcessBarStateMutation, InventoryTransitionFailure> {
    let mut position_record = None;
    let mut released_notional_usd = None;

    match risk_decision {
        RiskDecision::Allow(allowed_intent) => match allowed_intent {
            TradeIntent::OpenLong | TradeIntent::AddLong => {
                if let Some(order) = accepted_order {
                    apply_open_long_fill(instance, bar, order)?;
                    position_record = Some(position_record_state(instance));
                }
                instance.cooldown_until_ms = None;
            }
            TradeIntent::CloseLong | TradeIntent::ReduceLong => {
                if let Some(order) = accepted_order {
                    released_notional_usd = Some(apply_close_long_fill(instance, bar, order)?);
                    position_record = Some(position_record_state(instance));
                }
                instance.cooldown_until_ms = None;
            }
            TradeIntent::NoOp => {
                instance.has_position = next_has_position;
            }
        },
        RiskDecision::Reject { .. } => {
            instance.has_position = next_has_position;
            if order_ledger_outcome.is_none() {
                let cooldown_ms = i64::try_from(instance.risk_limits.cooldown_after_reject_ms)
                    .unwrap_or(i64::MAX);
                instance.cooldown_until_ms =
                    Some(bar.timestamp.timestamp_millis().saturating_add(cooldown_ms));
            }
        }
    }

    Ok(ProcessBarStateMutation {
        position_record,
        released_notional_usd,
    })
}
