# ARCHITECTURE

Last reviewed: 2026-07-14

## Role

`openticker-lane` owns extracted lane-local runtime state, lane bootstrap
helpers, and helper DTOs that do not require service composition, plus
lane-cycle and manual-close workflow algorithms that run through
runtime-provided ports.

## Current Surface

Important public types:

- `LaneRuntime`
- `LaneRuntimeState`
- `LaneRecoveryState`
- `InstanceWarmupState`
- `RecoveredInstanceState`
- `RiskProfileRuntimeConfig`
- `RuntimeLaneBuild`
- `ProcessBarEvaluation`
- `StrategySignalSource`
- `ConnectorSnapshotOutcome`
- `ReconciliationSyncOutcome`
- `LaneCycleContext`
- `LaneCycleEngine`
- `LanePollingContext`
- `LanePollingEngine`
- `LaneWarmupContext`
- `LaneWarmupEngine`
- `LaneExecutionEngine`
- `LaneManualOpsEngine`
- `ManualCloseContext`
- `ManualCloseSignalOutcome`
- `ManualCloseOutcome`

Important public helpers:

- `required_warmup_bars(...)`
- `build_runtime_indicators(...)`
- `build_runtime_strategy(...)`
- `recover_lane_state(...)`
- `lane_instance_id(...)`
- `resolved_instance_state(...)`
- `resolved_risk_limits(...)`
- `resolved_target_order_notional_usd(...)`
- `build_lane_runtime(...)`
- `build_runtime_lanes(...)`
- `evaluate_indicator_signals(...)`
- `prepare_process_bar_evaluation(...)`
- `prepare_manual_signal_evaluation(...)`
- `run_process_bar_cycle(...)`
- `run_manual_signal_cycle(...)`
- `advance_lane_polling_once(...)`
- `attempt_lane_warmup_backfill(...)`
- `process_pending_warmup_bar(...)`
- `append_process_bar_records(...)`
- `apply_risk_decision_effects(...)`
- `apply_process_bar_state_effects(...)`
- `close_lane_position(...)`
- `ledger_owner_path(...)`
- `sync_inventory_from_runtime_fields(...)`
- `sync_runtime_fields_from_inventory(...)`
- `sync_remote_position_quantity(...)`
- `effective_position_quantity(...)`
- `aggregate_bot_state(...)`
- `apply_process_bar_fill_state(...)`
- `validate_recovery_bars(...)`
- `resolved_strategy_signal(...)`
- `build_process_bar_evaluation(...)`
- `market_data_is_stale(...)`

## Internal Layout

`src/lib.rs` preserves the crate-root API through re-exports. State and
construction live in `state.rs` and `build.rs`; pure signal and position work
lives in `signals.rs` and `position.rs`; lifecycle workflows are separated into
`cycle.rs`, `manual_ops.rs`, `warmup.rs`, `recovery.rs`, `polling.rs`, and
`execution.rs`; trace assembly and reconciliation DTOs live in `trace.rs` and
`reconcile.rs`.

## Boundaries

- This crate does not own connector registry access, runtime composition, or
  direct journal writes.
- Lane workflows here must depend on abstract engine ports for connector,
  ledger, and persistence effects.
