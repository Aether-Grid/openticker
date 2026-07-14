use openticker_connectors::{ConnectorAccountSnapshot, ConnectorOpenOrder};
use openticker_storage::OrderRecord;
use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalOpenOrderIdentity {
    pub bot_id: String,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedRemoteOpenOrder {
    pub bot_id: String,
    pub order: ConnectorOpenOrder,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClassifiedRemoteOpenOrders {
    pub managed_orders: Vec<ManagedRemoteOpenOrder>,
    pub external_orders: Vec<ConnectorOpenOrder>,
    pub unsafe_reasons: Vec<String>,
}

#[must_use]
pub fn open_orders_for_symbol(
    snapshot: &ConnectorAccountSnapshot,
    symbol: &str,
) -> Vec<ConnectorOpenOrder> {
    snapshot
        .open_orders
        .iter()
        .filter(|order| order.symbol == symbol && !order_status_terminal(order.status.as_str()))
        .cloned()
        .collect()
}

#[must_use]
pub fn local_open_order_ids<S: BuildHasher>(
    orders: &[OrderRecord],
    filled_client_order_ids: &HashSet<String, S>,
) -> Vec<String> {
    orders
        .iter()
        .filter(|order| {
            !order_status_terminal(order.status.as_str())
                && !filled_client_order_ids.contains(order.client_order_id.as_str())
        })
        .map(|order| order.client_order_id.clone())
        .collect()
}

#[must_use]
pub fn classify_remote_open_orders<S: BuildHasher, T: BuildHasher>(
    account_id: &str,
    symbol: &str,
    orders: &[ConnectorOpenOrder],
    matches_by_client_order_id: &HashMap<String, Vec<LocalOpenOrderIdentity>, S>,
    eligible_bot_ids: &HashSet<String, T>,
) -> ClassifiedRemoteOpenOrders {
    let mut classified = ClassifiedRemoteOpenOrders::default();

    for order in orders {
        let Some(matches) = matches_by_client_order_id.get(order.client_order_id.as_str()) else {
            classified.external_orders.push(order.clone());
            continue;
        };
        if matches.is_empty() {
            classified.external_orders.push(order.clone());
            continue;
        }

        let mut identities = matches.clone();
        identities.sort();
        identities.dedup();

        if identities.len() != 1 || identities[0].symbol.is_none() {
            classified.unsafe_reasons.push(format!(
                "client_order_id={} matched multiple local orders ({})",
                order.client_order_id,
                describe_local_open_order_identities(matches)
            ));
            continue;
        }

        let Some(identity) = identities.pop() else {
            continue;
        };
        let Some(local_symbol) = identity.symbol else {
            continue;
        };
        if local_symbol != symbol {
            classified.unsafe_reasons.push(format!(
                "client_order_id={} matched local symbol {} but remote symbol is {}",
                order.client_order_id, local_symbol, symbol,
            ));
            continue;
        }

        if !eligible_bot_ids.contains(identity.bot_id.as_str()) {
            classified.unsafe_reasons.push(format!(
                "client_order_id={} matched bot {} but not account {} symbol {}",
                order.client_order_id, identity.bot_id, account_id, symbol,
            ));
            continue;
        }

        classified.managed_orders.push(ManagedRemoteOpenOrder {
            bot_id: identity.bot_id,
            order: order.clone(),
        });
    }

    classified
}

fn order_status_terminal(status: &str) -> bool {
    matches!(
        status,
        "filled" | "cancelled" | "rejected" | "expired" | "done" | "cancel_requested"
    )
}

fn describe_local_open_order_identities(identities: &[LocalOpenOrderIdentity]) -> String {
    let mut described = identities
        .iter()
        .map(|identity| {
            format!(
                "{}:{}",
                identity.bot_id,
                identity.symbol.as_deref().unwrap_or("<unknown-symbol>")
            )
        })
        .collect::<Vec<_>>();
    described.sort();
    described.dedup();
    described.join(",")
}
