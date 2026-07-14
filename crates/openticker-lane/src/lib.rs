pub use openticker_instance::{default_signal_policy, representative_indicator};

mod build;
mod cycle;
mod execution;
mod manual_ops;
mod polling;
mod position;
mod reconcile;
mod recovery;
mod signals;
mod state;
mod trace;
mod warmup;

#[cfg(test)]
mod tests;

pub use build::{
    RecoveredInstanceState, RiskProfileRuntimeConfig, RuntimeLaneBuild, build_lane_runtime,
    build_runtime_indicators, build_runtime_lanes, build_runtime_strategy, lane_instance_id,
    recover_lane_state, required_warmup_bars, resolved_instance_state, resolved_risk_limits,
    resolved_target_order_notional_usd,
};
pub use cycle::{
    LaneCycleContext, LaneCycleEngine, manual_signal_phase, run_manual_signal_cycle,
    run_process_bar_cycle,
};
pub use execution::{
    LaneExecutionEngine, append_process_bar_records, apply_process_bar_state_effects,
    apply_risk_decision_effects,
};
pub use manual_ops::{
    LaneManualOpsEngine, ManualCloseContext, ManualCloseOutcome, ManualCloseSignalOutcome,
    ManualCloseSignalRisk, close_lane_position,
};
pub use polling::{
    ConfirmedBarReplayMode, ConfirmedBarReplayResult, LanePollingAdvance, LanePollingContext,
    LanePollingEngine, advance_lane_polling_once,
};
pub use position::{
    InventoryTransitionFailure, PositionRecordState, ProcessBarStateMutation,
    accepted_order_fee_entry, apply_process_bar_fill_state, current_instance_open_notional_usd,
    effective_position_quantity, inventory_state_from_runtime_fields, ledger_owner_path,
    sync_inventory_from_runtime_fields, sync_remote_position_quantity,
    sync_runtime_fields_from_inventory,
};
pub use reconcile::{ConnectorSnapshotOutcome, ReconciliationSyncOutcome};
pub use recovery::{
    RecoveryNoProgressState, RecoveryPageApplied, RecoveryStartKind, complete_lane_recovery_state,
    mark_lane_out_of_sync_state, mark_recovery_page_progress, record_recovery_no_progress_state,
    start_lane_recovery_state, validate_recovery_bars,
};
pub use signals::{
    PreparedLaneEvaluation, ProcessBarEvaluation, SignalEvaluationKernelInput,
    StrategySignalSource, apply_position_transition, apply_state_only_confirmed_bar,
    build_process_bar_evaluation, evaluate_indicator_signals, market_data_freshness,
    market_data_is_stale, prepare_manual_signal_evaluation, prepare_process_bar_evaluation,
    resolved_strategy_signal,
};
pub use state::{LaneRecoveryState, LaneRuntime, LaneRuntimeState, aggregate_bot_state};
pub use trace::build_cycle_trace;
pub use warmup::{
    InstanceWarmupState, LaneWarmupContext, LaneWarmupEngine, WarmupAdvance, WarmupProgressDetail,
    advance_warmup_state, attempt_lane_warmup_backfill, process_pending_warmup_bar,
    record_warmup_failure,
};
