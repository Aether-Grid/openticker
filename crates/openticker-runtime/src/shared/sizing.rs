#[cfg(test)]
mod tests {
    use crate::{IndicatorSignal, SignalPhase, TradeIntent};
    use openticker_lane::{StrategySignalSource, resolved_strategy_signal};

    #[test]
    fn resolved_strategy_signal_falls_back_to_intent_when_representative_none() {
        assert_eq!(
            resolved_strategy_signal(
                IndicatorSignal::None,
                TradeIntent::OpenLong,
                SignalPhase::Confirmed,
            ),
            (
                IndicatorSignal::BuyConfirmed,
                StrategySignalSource::IntentFallback,
            )
        );
        assert_eq!(
            resolved_strategy_signal(
                IndicatorSignal::None,
                TradeIntent::CloseLong,
                SignalPhase::Preview,
            ),
            (
                IndicatorSignal::SellPreview,
                StrategySignalSource::IntentFallback,
            )
        );
    }

    #[test]
    fn resolved_strategy_signal_preserves_non_none_representative() {
        assert_eq!(
            resolved_strategy_signal(
                IndicatorSignal::SellConfirmed,
                TradeIntent::OpenLong,
                SignalPhase::Confirmed,
            ),
            (
                IndicatorSignal::SellConfirmed,
                StrategySignalSource::Representative,
            )
        );
    }

    #[test]
    fn resolved_strategy_signal_keeps_none_without_fallback_when_intent_no_op() {
        assert_eq!(
            resolved_strategy_signal(
                IndicatorSignal::None,
                TradeIntent::NoOp,
                SignalPhase::Confirmed,
            ),
            (IndicatorSignal::None, StrategySignalSource::Representative)
        );
    }
}

#[cfg(kani)]
mod proofs {
    use crate::TradeIntent;
    use openticker_lane::apply_position_transition;

    #[kani::proof]
    fn proof_position_transition_cannot_create_position_from_close_or_noop() {
        let intent = match kani::any::<u8>() % 3 {
            0 => TradeIntent::NoOp,
            1 => TradeIntent::ReduceLong,
            _ => TradeIntent::CloseLong,
        };

        assert!(!apply_position_transition(false, intent));
        assert!(apply_position_transition(
            kani::any(),
            TradeIntent::OpenLong
        ));
        assert!(apply_position_transition(kani::any(), TradeIntent::AddLong));
    }
}
