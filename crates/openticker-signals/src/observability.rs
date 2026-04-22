use openticker_core::{IndicatorSignal, SignalPhase};
use tracing::{debug, info};

pub fn log_indicator_evaluation(
    indicator_id: &str,
    indicator_type: &str,
    phase: SignalPhase,
    signal: IndicatorSignal,
    close: f64,
) {
    let phase = signal_phase_label(phase);
    let signal = indicator_signal_label(signal);

    if signal == "none" {
        debug!(
            indicator.id = indicator_id,
            indicator.kind = indicator_type,
            phase,
            signal,
            close,
            "indicator evaluated"
        );
    } else {
        info!(
            indicator.id = indicator_id,
            indicator.kind = indicator_type,
            phase,
            signal,
            close,
            "indicator emitted actionable signal"
        );
    }
}

fn signal_phase_label(phase: SignalPhase) -> &'static str {
    match phase {
        SignalPhase::Preview => "preview",
        SignalPhase::Confirmed => "confirmed",
    }
}

fn indicator_signal_label(signal: IndicatorSignal) -> &'static str {
    match signal {
        IndicatorSignal::None => "none",
        IndicatorSignal::BuyPreview => "buy_preview",
        IndicatorSignal::BuyConfirmed => "buy_confirmed",
        IndicatorSignal::SellPreview => "sell_preview",
        IndicatorSignal::SellConfirmed => "sell_confirmed",
    }
}
