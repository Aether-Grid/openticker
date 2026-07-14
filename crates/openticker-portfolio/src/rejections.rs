use openticker_ledger::{AccountLedger, LedgerException};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LedgerRejectionPayload {
    pub intent: String,
    pub decision: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committed_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocated_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_cap_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradeable_room_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_room_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exceptions: Option<Vec<LedgerException>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LedgerRejectionEventPayload {
    pub intent: String,
    pub decision: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committed_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocated_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_cap_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradeable_room_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_room_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exceptions: Option<Vec<LedgerException>>,
    pub symbol: String,
    pub bar_timestamp: String,
}

#[must_use]
pub fn bot_ledger_rejection_payload(
    ledger: &AccountLedger,
    account_id: &str,
    bot_id: &str,
    bot_pct: f64,
    intent_label: &str,
    reason_code: &str,
) -> LedgerRejectionPayload {
    let bot_snapshot = ledger.bot_snapshot(account_id, bot_id, bot_pct);
    LedgerRejectionPayload {
        intent: intent_label.to_owned(),
        decision: "rejected".to_owned(),
        reason: reason_code.to_owned(),
        committed_usd: Some(bot_snapshot.total_committed_notional_usd),
        allocated_usd: Some(bot_snapshot.allocated_usd),
        effective_cap_usd: None,
        tradeable_room_usd: Some(bot_snapshot.tradeable_open_room_usd),
        blocked_room_usd: Some(bot_snapshot.blocked_open_room_usd),
        exceptions: None,
    }
}

#[must_use]
pub fn account_ledger_rejection_payload(
    ledger: &AccountLedger,
    account_id: &str,
    intent_label: &str,
    reason_code: &str,
) -> LedgerRejectionPayload {
    let account_snapshot = ledger.account_snapshot(account_id);
    LedgerRejectionPayload {
        intent: intent_label.to_owned(),
        decision: "rejected".to_owned(),
        reason: reason_code.to_owned(),
        committed_usd: Some(account_snapshot.total_committed_notional_usd),
        allocated_usd: None,
        effective_cap_usd: Some(account_snapshot.effective_cap_usd),
        tradeable_room_usd: Some(account_snapshot.tradeable_open_room_usd),
        blocked_room_usd: Some(account_snapshot.blocked_open_room_usd),
        exceptions: Some(account_snapshot.exceptions),
    }
}

#[must_use]
pub fn dust_ledger_rejection_payload(intent_label: &str) -> LedgerRejectionPayload {
    LedgerRejectionPayload {
        intent: intent_label.to_owned(),
        decision: "skipped".to_owned(),
        reason: "ledger_dust".to_owned(),
        committed_usd: None,
        allocated_usd: None,
        effective_cap_usd: None,
        tradeable_room_usd: None,
        blocked_room_usd: None,
        exceptions: None,
    }
}

#[must_use]
pub fn ledger_rejection_event_payload(
    payload: LedgerRejectionPayload,
    symbol: &str,
    bar_timestamp: &str,
) -> LedgerRejectionEventPayload {
    LedgerRejectionEventPayload {
        intent: payload.intent,
        decision: payload.decision,
        reason: payload.reason,
        committed_usd: payload.committed_usd,
        allocated_usd: payload.allocated_usd,
        effective_cap_usd: payload.effective_cap_usd,
        tradeable_room_usd: payload.tradeable_room_usd,
        blocked_room_usd: payload.blocked_room_usd,
        exceptions: payload.exceptions,
        symbol: symbol.to_owned(),
        bar_timestamp: bar_timestamp.to_owned(),
    }
}
