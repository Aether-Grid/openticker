use super::{metadata, metadata_capabilities, metadata_filters};
use crate::{SingleIndicatorLongOnlyStrategy, Strategy, StrategyContext};
use openticker_core::{
    IndicatorMetadataCapabilities, IndicatorSignal, IndicatorSignalMetadataFilters,
    IndicatorSignalPolicy, SignalMetadata, SignalMetadataFilter, SignalStrength, TradeIntent,
};

#[test]
fn maps_signals_to_trade_intents() {
    let mut strategy = SingleIndicatorLongOnlyStrategy;

    assert_eq!(
        strategy
            .decide(StrategyContext {
                indicator_id: "primary-1",
                signal: IndicatorSignal::BuyConfirmed,
                signal_policy: IndicatorSignalPolicy::PreviewAllowed,
                metadata_capabilities: metadata_capabilities(),
                metadata_filters: &metadata_filters(),
                metadata: &metadata(),
                has_position: false,
            })
            .intent,
        TradeIntent::OpenLong
    );
    assert_eq!(
        strategy
            .decide(StrategyContext {
                indicator_id: "primary-1",
                signal: IndicatorSignal::BuyConfirmed,
                signal_policy: IndicatorSignalPolicy::PreviewAllowed,
                metadata_capabilities: metadata_capabilities(),
                metadata_filters: &metadata_filters(),
                metadata: &metadata(),
                has_position: true,
            })
            .intent,
        TradeIntent::AddLong
    );
    assert_eq!(
        strategy
            .decide(StrategyContext {
                indicator_id: "primary-1",
                signal: IndicatorSignal::SellConfirmed,
                signal_policy: IndicatorSignalPolicy::PreviewAllowed,
                metadata_capabilities: metadata_capabilities(),
                metadata_filters: &metadata_filters(),
                metadata: &metadata(),
                has_position: true,
            })
            .intent,
        TradeIntent::CloseLong
    );
}

#[test]
fn single_indicator_sell_without_position_is_noop() {
    let mut strategy = SingleIndicatorLongOnlyStrategy;

    let decision = strategy.decide(StrategyContext {
        indicator_id: "primary-1",
        signal: IndicatorSignal::SellConfirmed,
        signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
        metadata_capabilities: metadata_capabilities(),
        metadata_filters: &metadata_filters(),
        metadata: &metadata(),
        has_position: false,
    });

    assert_eq!(decision.intent, TradeIntent::NoOp);
}

#[test]
fn single_indicator_respects_confirmed_required_policy() {
    let mut strategy = SingleIndicatorLongOnlyStrategy;

    assert_eq!(
        strategy
            .decide(StrategyContext {
                indicator_id: "primary-1",
                signal: IndicatorSignal::BuyPreview,
                signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
                metadata_capabilities: metadata_capabilities(),
                metadata_filters: &metadata_filters(),
                metadata: &metadata(),
                has_position: false,
            })
            .intent,
        TradeIntent::NoOp
    );
    assert_eq!(
        strategy
            .decide(StrategyContext {
                indicator_id: "primary-1",
                signal: IndicatorSignal::SellPreview,
                signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
                metadata_capabilities: metadata_capabilities(),
                metadata_filters: &metadata_filters(),
                metadata: &metadata(),
                has_position: true,
            })
            .intent,
        TradeIntent::NoOp
    );
    assert_eq!(
        strategy
            .decide(StrategyContext {
                indicator_id: "primary-1",
                signal: IndicatorSignal::BuyPreview,
                signal_policy: IndicatorSignalPolicy::PreviewAllowed,
                metadata_capabilities: metadata_capabilities(),
                metadata_filters: &metadata_filters(),
                metadata: &metadata(),
                has_position: false,
            })
            .intent,
        TradeIntent::OpenLong
    );
}

#[test]
fn single_indicator_filters_entry_by_strength_when_available() {
    let mut strategy = SingleIndicatorLongOnlyStrategy;
    let metadata = SignalMetadata {
        strength: Some(SignalStrength::Normal),
        ..SignalMetadata::default()
    };
    let filters = IndicatorSignalMetadataFilters {
        entry: SignalMetadataFilter {
            allowed_strengths: vec![SignalStrength::Strong],
            ..SignalMetadataFilter::default()
        },
        exit: SignalMetadataFilter::default(),
    };

    let decision = strategy.decide(StrategyContext {
        indicator_id: "primary-1",
        signal: IndicatorSignal::BuyConfirmed,
        signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
        metadata_capabilities: IndicatorMetadataCapabilities {
            supports_strength: true,
            ..IndicatorMetadataCapabilities::default()
        },
        metadata_filters: &filters,
        metadata: &metadata,
        has_position: false,
    });

    assert_eq!(decision.intent, TradeIntent::NoOp);
}

#[test]
fn single_indicator_does_not_block_when_metadata_capability_is_missing() {
    let mut strategy = SingleIndicatorLongOnlyStrategy;
    let metadata = SignalMetadata::default();
    let filters = IndicatorSignalMetadataFilters {
        entry: SignalMetadataFilter {
            allowed_strengths: vec![SignalStrength::Strong],
            ..SignalMetadataFilter::default()
        },
        exit: SignalMetadataFilter::default(),
    };

    let decision = strategy.decide(StrategyContext {
        indicator_id: "primary-1",
        signal: IndicatorSignal::BuyConfirmed,
        signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
        metadata_capabilities: IndicatorMetadataCapabilities::default(),
        metadata_filters: &filters,
        metadata: &metadata,
        has_position: false,
    });

    assert_eq!(decision.intent, TradeIntent::OpenLong);
}
