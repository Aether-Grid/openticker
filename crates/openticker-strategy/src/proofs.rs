use crate::{
    ConsensusLongOnlyStrategy, ConsensusStrategy, ConsensusStrategyContext, IndicatorObservation,
    SingleIndicatorLongOnlyStrategy, Strategy, StrategyContext,
};
use openticker_core::{
    IndicatorMetadataCapabilities, IndicatorRole, IndicatorSignal, IndicatorSignalMetadataFilters,
    IndicatorSignalPolicy, SignalMetadata, TradeIntent,
};

fn default_metadata() -> SignalMetadata {
    SignalMetadata::default()
}

fn default_filters() -> IndicatorSignalMetadataFilters {
    IndicatorSignalMetadataFilters::default()
}

fn default_capabilities() -> IndicatorMetadataCapabilities {
    IndicatorMetadataCapabilities::default()
}

fn preview_signal(selector: bool) -> IndicatorSignal {
    if selector {
        IndicatorSignal::BuyPreview
    } else {
        IndicatorSignal::SellPreview
    }
}

fn sell_signal(preview: bool) -> IndicatorSignal {
    if preview {
        IndicatorSignal::SellPreview
    } else {
        IndicatorSignal::SellConfirmed
    }
}

fn any_signal(selector: u8) -> IndicatorSignal {
    match selector % 5 {
        0 => IndicatorSignal::None,
        1 => IndicatorSignal::BuyPreview,
        2 => IndicatorSignal::BuyConfirmed,
        3 => IndicatorSignal::SellPreview,
        _ => IndicatorSignal::SellConfirmed,
    }
}

fn any_policy(selector: bool) -> IndicatorSignalPolicy {
    if selector {
        IndicatorSignalPolicy::PreviewAllowed
    } else {
        IndicatorSignalPolicy::ConfirmedRequired
    }
}

#[kani::proof]
fn proof_single_indicator_confirmed_required_suppresses_preview() {
    let mut strategy = SingleIndicatorLongOnlyStrategy;
    let filters = default_filters();
    let metadata = default_metadata();

    let decision = strategy.decide(StrategyContext {
        indicator_id: "primary-1",
        signal: preview_signal(kani::any()),
        signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
        metadata_capabilities: default_capabilities(),
        metadata_filters: &filters,
        metadata: &metadata,
        has_position: kani::any(),
    });

    assert_eq!(decision.intent, TradeIntent::NoOp);
}

#[kani::proof]
fn proof_single_indicator_sell_without_position_is_noop() {
    let mut strategy = SingleIndicatorLongOnlyStrategy;
    let filters = default_filters();
    let metadata = default_metadata();

    let decision = strategy.decide(StrategyContext {
        indicator_id: "primary-1",
        signal: sell_signal(kani::any()),
        signal_policy: any_policy(kani::any()),
        metadata_capabilities: default_capabilities(),
        metadata_filters: &filters,
        metadata: &metadata,
        has_position: false,
    });

    assert_eq!(decision.intent, TradeIntent::NoOp);
}

#[kani::proof]
fn proof_single_indicator_stays_long_only() {
    let mut strategy = SingleIndicatorLongOnlyStrategy;
    let filters = default_filters();
    let metadata = default_metadata();
    let decision = strategy.decide(StrategyContext {
        indicator_id: "primary-1",
        signal: any_signal(kani::any()),
        signal_policy: any_policy(kani::any()),
        metadata_capabilities: default_capabilities(),
        metadata_filters: &filters,
        metadata: &metadata,
        has_position: kani::any(),
    });

    assert!(matches!(
        decision.intent,
        TradeIntent::NoOp | TradeIntent::OpenLong | TradeIntent::AddLong | TradeIntent::CloseLong
    ));
}

#[kani::proof]
fn proof_consensus_filter_veto_cannot_open() {
    let mut strategy = ConsensusLongOnlyStrategy::default();
    let primary_filters = default_filters();
    let filter_filters = default_filters();
    let primary_metadata = default_metadata();
    let filter_metadata = default_metadata();
    let indicators = [
        IndicatorObservation {
            id: "primary-1",
            role: IndicatorRole::PrimarySignal,
            signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
            signal: IndicatorSignal::BuyConfirmed,
            metadata_capabilities: default_capabilities(),
            metadata_filters: &primary_filters,
            metadata: &primary_metadata,
            weight: 1.0,
        },
        IndicatorObservation {
            id: "filter-1",
            role: IndicatorRole::Filter,
            signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
            signal: IndicatorSignal::SellConfirmed,
            metadata_capabilities: default_capabilities(),
            metadata_filters: &filter_filters,
            metadata: &filter_metadata,
            weight: 1.0,
        },
    ];

    let decision = strategy.decide_consensus(ConsensusStrategyContext {
        indicators: &indicators,
        has_position: kani::any(),
    });

    assert_eq!(decision.intent, TradeIntent::NoOp);
}

#[kani::proof]
fn proof_consensus_preview_primary_respects_policy() {
    let mut strategy = ConsensusLongOnlyStrategy::default();
    let filters = default_filters();
    let metadata = default_metadata();
    let indicators = [IndicatorObservation {
        id: "primary-1",
        role: IndicatorRole::PrimarySignal,
        signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
        signal: preview_signal(kani::any()),
        metadata_capabilities: default_capabilities(),
        metadata_filters: &filters,
        metadata: &metadata,
        weight: 1.0,
    }];

    let decision = strategy.decide_consensus(ConsensusStrategyContext {
        indicators: &indicators,
        has_position: kani::any(),
    });

    assert_eq!(decision.intent, TradeIntent::NoOp);
}

#[kani::proof]
fn proof_consensus_stays_long_only() {
    let mut strategy = ConsensusLongOnlyStrategy::default();
    let filters = default_filters();
    let metadata = default_metadata();
    let indicators = [IndicatorObservation {
        id: "primary-1",
        role: IndicatorRole::PrimarySignal,
        signal_policy: any_policy(kani::any()),
        signal: any_signal(kani::any()),
        metadata_capabilities: default_capabilities(),
        metadata_filters: &filters,
        metadata: &metadata,
        weight: 1.0,
    }];

    let decision = strategy.decide_consensus(ConsensusStrategyContext {
        indicators: &indicators,
        has_position: kani::any(),
    });

    assert!(matches!(
        decision.intent,
        TradeIntent::NoOp | TradeIntent::OpenLong | TradeIntent::AddLong | TradeIntent::CloseLong
    ));
}
