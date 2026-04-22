use crate::{ExecutionMode, IndicatorSignal, SignalPhase, TradeIntent};

pub(crate) fn execution_mode_to_storage(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Paper => "paper",
        ExecutionMode::Live => "live",
    }
}

pub(crate) fn execution_mode_is_live(mode: ExecutionMode) -> bool {
    matches!(mode, ExecutionMode::Live)
}

pub(crate) fn mode_banner_text(live_mode_active: bool) -> &'static str {
    if live_mode_active {
        "LIVE MODE ACTIVE - real capital may be at risk"
    } else {
        "PAPER MODE - non-live execution path"
    }
}

pub(crate) fn signal_phase_label(phase: SignalPhase) -> &'static str {
    match phase {
        SignalPhase::Preview => "preview",
        SignalPhase::Confirmed => "confirmed",
    }
}

pub(crate) fn indicator_signal_label(signal: IndicatorSignal) -> &'static str {
    match signal {
        IndicatorSignal::None => "none",
        IndicatorSignal::BuyPreview => "buy_preview",
        IndicatorSignal::BuyConfirmed => "buy_confirmed",
        IndicatorSignal::SellPreview => "sell_preview",
        IndicatorSignal::SellConfirmed => "sell_confirmed",
    }
}

pub(crate) fn trade_intent_label(intent: TradeIntent) -> &'static str {
    match intent {
        TradeIntent::NoOp => "no_op",
        TradeIntent::OpenLong => "open_long",
        TradeIntent::AddLong => "add_long",
        TradeIntent::ReduceLong => "reduce_long",
        TradeIntent::CloseLong => "close_long",
    }
}
