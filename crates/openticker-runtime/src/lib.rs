use openticker_config::{AccountConfig, ConfigBundle, DataPlaneConfig, InstanceConfig};
use openticker_connectors::{
    ConnectionState, ConnectorAccountSnapshot, ConnectorAccountStatus, ConnectorOpenOrder,
    ConnectorRegistry, ConnectorResiliencePolicy, ConnectorResilienceState,
};
use openticker_core::{
    ExecutionMode, IndicatorSignal, MarketType, OhlcvBar, SignalMetadata, SignalPhase, Timeframe,
    TradeIntent,
};
use openticker_data::{NormalizedBarUpdate, NormalizedTrade};
use openticker_dataplane::{StreamKey, StreamRegistry, StreamSource, StreamSpec};
use openticker_execution::{
    AcceptedOrder, ExecutionRequest, OrderLedgerOutcome, resolve_order_quantity_with_constraints,
};
use openticker_gateway::{
    execution_constraints_are_complete, resolve_effective_execution_constraints,
};
use openticker_ledger::{
    AccountLedger, InventoryError, LedgerOwnerPath, ReservationError,
    calculate_position_notional_usd,
};
pub use openticker_ledger::{
    AccountPortfolioSnapshot, BotPortfolioSnapshot, LanePortfolioSnapshot, LedgerException,
    LedgerExceptionKind, LedgerSnapshot,
};
use openticker_risk::{RiskDecision, RiskLimits};
use openticker_storage::{
    BotEventWrite, BotSnapshotWrite, CycleTraceRecord, CycleTraceWrite, EventWrite, FillRecord,
    FillWrite, InMemoryRuntimeJournal, IntentRecord, IntentWrite, OperatorReadModels, OrderRecord,
    OrderWrite, PositionRecord, PositionWrite, ReconciliationRecord, ReconciliationWrite,
    RiskDecisionRecord, RiskDecisionWrite, RuntimeEvent, RuntimeJournal, ServiceEventWrite,
    SignalRecord, SignalWrite, SqliteRuntimeJournal, StorageError,
};
pub use openticker_trace::{
    BudgetRoomContext as CycleBudgetRoomContext, CapitalState as CycleCapitalState, CycleOutcome,
    CycleRiskDecisionLabel, CycleTrace, CycleTraceSummary, CycleTrigger, CycleTriggerKind,
    ExecutionFillStep as CycleExecutionFillStep, ExecutionOrderStep as CycleExecutionOrderStep,
    ExecutionStep as CycleExecutionStep, IntentStep as CycleIntentStep,
    PositionStep as CyclePositionStep, ReconciliationContext as CycleReconciliationContext,
    ReconciliationSnapshot as CycleReconciliationSnapshot, RelatedEvent as CycleRelatedEvent,
    RelatedRecord as CycleRelatedRecord, RiskStep as CycleRiskStep, SignalStep as CycleSignalStep,
    TraceIdentity,
};

mod connector_gateway;
mod construction;
mod errors;
mod lifecycle;
mod manual_ops;
mod market_data;
mod model;
mod polling_supervisor;
mod portfolio_adapter;
mod processing;
mod queries;
mod reconciliation;
mod repo;
mod runtime_wiring;
mod shared;
#[cfg(test)]
mod test_support;

pub use errors::ServiceError;
pub use model::{
    ConnectorRuntimeStatus, InstancePnlStatus, InstancePositionStatus, InstanceRuntimeState,
    InstanceSummary, InstanceWarmupStatus, LaneRecoveryState, LaneRecoveryStatus, LaneRuntimeState,
    LaneSummary, LedgerPortfolioSnapshot, PollAdvanceResult, ProcessBarOutcome, ProcessBarRisk,
    ReconciliationCheck, ReconciliationReport, Runtime, RuntimeObservabilityStatus, ServiceStatus,
    SymbolReconciliationSummary,
};
pub use polling_supervisor::{RUNTIME_BACKGROUND_POLL_INTERVAL_MS, RuntimePollingSupervisor};
pub use queries::RuntimeQueryHandle;

use model::{
    AccountRiskSnapshot, ConnectorSnapshotOutcome, LaneRuntime, LatencyAccumulator, LedgerRooms,
    POSITION_QUANTITY_TOLERANCE, PROVIDER_EVENT_PAYLOAD_PREVIEW_LIMIT, ProcessBarEvaluation,
    RECONCILIATION_JOURNAL_SCAN_LIMIT, ReconciliationAssessment, ReconciliationSyncOutcome,
    RiskProfileRuntimeConfig, RuntimeCatalog, RuntimeState, ServiceObservability,
};
use shared::{
    aggregate_bot_state, apply_process_bar_fill_state, connector_resilience_window_active,
    connector_runtime_status_from_account_status, current_instance_open_notional_usd,
    effective_position_quantity, execution_mode_is_live, execution_mode_to_storage,
    indicator_signal_label, instance_matches_stream_key, inventory_transition_error,
    ledger_outcome_reason_code, ledger_owner_path, log_runtime_event, mode_banner_text,
    provider_payload_preview, saturating_duration_to_millis, signal_phase_label,
    sync_inventory_from_runtime_fields, sync_remote_position_quantity, trade_intent_label,
    unix_now_ms,
};
