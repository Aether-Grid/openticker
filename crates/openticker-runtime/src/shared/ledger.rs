use crate::OrderLedgerOutcome;

pub(crate) use openticker_lane::current_instance_open_notional_usd;

pub(crate) fn ledger_outcome_reason_code(outcome: OrderLedgerOutcome) -> &'static str {
    match outcome {
        OrderLedgerOutcome::BotExhausted => "bot_ledger_exhausted",
        OrderLedgerOutcome::AccountExhausted => "account_ledger_exhausted",
        OrderLedgerOutcome::Dust => "ledger_dust",
    }
}
