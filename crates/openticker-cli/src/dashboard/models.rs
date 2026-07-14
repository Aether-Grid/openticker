use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct DashboardServiceStatus {
    #[serde(default)]
    pub(super) total_instances: usize,
    #[serde(default)]
    pub(super) running_instances: usize,
    #[serde(default)]
    pub(super) paused_instances: usize,
    #[serde(default)]
    pub(super) stopped_instances: usize,
    #[serde(default)]
    pub(super) reconciling_instances: usize,
    #[serde(default)]
    pub(super) reconciliation_blocked_instances: usize,
    #[serde(default)]
    pub(super) warmup_ready_instances: usize,
    #[serde(default)]
    pub(super) warmup_pending_instances: usize,
    #[serde(default)]
    pub(super) warmup_failed_instances: usize,
    #[serde(default)]
    pub(super) kill_switch_active: bool,
    #[serde(default)]
    pub(super) ready: bool,
    #[serde(default)]
    pub(super) live_mode_active: bool,
    #[serde(default)]
    pub(super) mode_banner: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct DashboardWarmupStatus {
    #[serde(default)]
    pub(super) required_bars: usize,
    #[serde(default)]
    pub(super) loaded_bars: usize,
    #[serde(default)]
    pub(super) ready: bool,
    #[serde(default)]
    pub(super) last_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct DashboardBotPosition {
    #[serde(default)]
    pub(super) has_position: bool,
    #[serde(default)]
    pub(super) quantity: f64,
    #[serde(default)]
    pub(super) entry_price: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct DashboardBotSummary {
    #[serde(default)]
    pub(super) id: String,
    #[serde(default)]
    pub(super) state: String,
    #[serde(default)]
    pub(super) market: String,
    #[serde(default)]
    pub(super) timeframe: String,
    #[serde(default)]
    pub(super) account: String,
    #[serde(default)]
    pub(super) execution_mode: String,
    #[serde(default)]
    pub(super) position: DashboardBotPosition,
    #[serde(default)]
    pub(super) reconciliation_blocked: bool,
    #[serde(default)]
    pub(super) reconciliation_by_symbol: Vec<DashboardSymbolReconciliationSummary>,
    #[serde(default)]
    pub(super) warmup: DashboardWarmupStatus,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct DashboardSymbolReconciliationSummary {
    #[serde(default)]
    pub(super) symbol: String,
    #[serde(default)]
    pub(super) reconciliation_blocked: bool,
    #[serde(default)]
    pub(super) remote_net_qty: Option<f64>,
    #[serde(default)]
    pub(super) aggregate_managed_qty: f64,
    #[serde(default)]
    pub(super) external_delta_qty: Option<f64>,
    #[serde(default)]
    pub(super) managed_remote_open_orders: usize,
    #[serde(default)]
    pub(super) external_remote_open_orders: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct DashboardConnectorStatus {
    #[serde(default)]
    pub(super) account_id: String,
    #[serde(default)]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) mode: String,
    #[serde(default)]
    pub(super) state: String,
    #[serde(default)]
    pub(super) message: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct DashboardRiskDecisionRecord {
    #[serde(default, rename = "bot_id", alias = "instance_id")]
    pub(super) bot_id: String,
    #[serde(default)]
    pub(super) symbol: Option<String>,
    #[serde(default)]
    pub(super) intent: String,
    #[serde(default)]
    pub(super) decision: String,
    #[serde(default)]
    pub(super) reason: Option<String>,
    #[serde(default)]
    pub(super) created_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct DashboardRiskDecisionResponse {
    #[serde(default)]
    pub(super) count: usize,
    #[serde(default)]
    pub(super) items: Vec<DashboardRiskDecisionRecord>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct DashboardOrderRecord {
    #[serde(default, rename = "bot_id", alias = "instance_id")]
    pub(super) bot_id: String,
    #[serde(default)]
    pub(super) intent: String,
    #[serde(default)]
    pub(super) status: String,
    #[serde(default)]
    pub(super) price: f64,
    #[serde(default)]
    pub(super) quantity: f64,
    #[serde(default)]
    pub(super) created_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct DashboardRuntimeEvent {
    #[serde(default)]
    pub(super) scope: String,
    #[serde(default)]
    pub(super) entity_id: Option<String>,
    #[serde(default)]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) payload: String,
    #[serde(default)]
    pub(super) created_at_ms: i64,
}

#[derive(Debug, Default)]
pub(super) struct DashboardSnapshot {
    pub(super) service: DashboardServiceStatus,
    pub(super) bots: Vec<DashboardBotSummary>,
    pub(super) connectors: Vec<DashboardConnectorStatus>,
    pub(super) risk_count: usize,
    pub(super) risk_decisions: Vec<DashboardRiskDecisionRecord>,
    pub(super) orders: Vec<DashboardOrderRecord>,
    pub(super) events: Vec<DashboardRuntimeEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dashboard_models_deserialize_warmup_fields() {
        let service: DashboardServiceStatus = serde_json::from_value(json!({
            "total_instances": 2,
            "warmup_ready_instances": 1,
            "warmup_pending_instances": 1,
            "warmup_failed_instances": 0,
            "ready": false
        }))
        .unwrap();
        assert_eq!(service.total_instances, 2);
        assert_eq!(service.warmup_ready_instances, 1);
        assert_eq!(service.warmup_pending_instances, 1);
        assert_eq!(service.warmup_failed_instances, 0);

        let bot: DashboardBotSummary = serde_json::from_value(json!({
            "id": "aapl",
            "state": "paused",
            "market": "equities",
            "timeframe": "1m",
            "account": "alpaca-paper",
            "execution_mode": "paper",
            "position": {
                "has_position": true,
                "quantity": 1.5,
                "entry_price": 123.45
            },
            "reconciliation_blocked": false,
            "reconciliation_by_symbol": [
                {
                    "symbol": "AAPL",
                    "reconciliation_blocked": false,
                    "remote_net_qty": 1.0,
                    "aggregate_managed_qty": 1.5,
                    "external_delta_qty": -0.5,
                    "managed_remote_open_orders": 1,
                    "external_remote_open_orders": 0
                }
            ],
            "warmup": {
                "required_bars": 200,
                "loaded_bars": 35,
                "ready": false,
                "last_error": "startup warmup backfill unavailable"
            }
        }))
        .unwrap();
        assert_eq!(bot.warmup.required_bars, 200);
        assert_eq!(bot.warmup.loaded_bars, 35);
        assert!(!bot.warmup.ready);
        assert_eq!(
            bot.warmup.last_error.as_deref(),
            Some("startup warmup backfill unavailable")
        );
        assert!(bot.position.has_position);
        assert!((bot.position.quantity - 1.5).abs() < f64::EPSILON);
        assert_eq!(bot.position.entry_price, Some(123.45));
        assert_eq!(bot.reconciliation_by_symbol.len(), 1);
        assert_eq!(bot.reconciliation_by_symbol[0].symbol, "AAPL");
        assert_eq!(bot.reconciliation_by_symbol[0].remote_net_qty, Some(1.0));
    }
}
