use openticker_core::{IndicatorSignal, IndicatorSignalPolicy, TradeIntent};

use crate::{
    context::StrategyContext,
    decision::StrategyDecision,
    metadata::{metadata_filter_block_reason, signal_label},
    traits::Strategy,
};

#[derive(Debug, Default)]
pub struct SingleIndicatorLongOnlyStrategy;

impl Strategy for SingleIndicatorLongOnlyStrategy {
    fn decide(&mut self, context: StrategyContext<'_>) -> StrategyDecision {
        let effective_signal = match (context.signal_policy, context.signal) {
            (
                IndicatorSignalPolicy::ConfirmedRequired,
                IndicatorSignal::BuyPreview | IndicatorSignal::SellPreview,
            ) => {
                return StrategyDecision::no_op(format!(
                    "indicator={} preview_suppressed_by_policy",
                    context.indicator_id
                ));
            }
            (_, signal) => signal,
        };

        if let Some(reason) = metadata_filter_block_reason(
            context.metadata_capabilities,
            context.metadata_filters,
            context.metadata,
            effective_signal,
        ) {
            return StrategyDecision::no_op(format!("indicator={} {reason}", context.indicator_id));
        }

        let intent = match effective_signal {
            IndicatorSignal::BuyPreview | IndicatorSignal::BuyConfirmed => {
                if context.has_position {
                    TradeIntent::AddLong
                } else {
                    TradeIntent::OpenLong
                }
            }
            IndicatorSignal::SellPreview | IndicatorSignal::SellConfirmed => {
                if context.has_position {
                    TradeIntent::CloseLong
                } else {
                    TradeIntent::NoOp
                }
            }
            IndicatorSignal::None => TradeIntent::NoOp,
        };

        StrategyDecision::new(
            intent,
            format!(
                "indicator={} effective_signal={}",
                context.indicator_id,
                signal_label(effective_signal)
            ),
        )
    }
}
