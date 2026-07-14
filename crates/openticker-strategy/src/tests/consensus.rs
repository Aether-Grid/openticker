use super::{metadata, metadata_capabilities, metadata_filters};
use crate::{
    ConsensusLongOnlyStrategy, ConsensusStrategy, ConsensusStrategyContext, IndicatorObservation,
};
use openticker_core::{IndicatorRole, IndicatorSignal, IndicatorSignalPolicy, TradeIntent};

#[test]
fn consensus_maps_primary_vote_to_open_long() {
    let mut strategy = ConsensusLongOnlyStrategy::default();
    let indicators = [IndicatorObservation {
        id: "primary-1",
        role: IndicatorRole::PrimarySignal,
        signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
        signal: IndicatorSignal::BuyConfirmed,
        metadata_capabilities: metadata_capabilities(),
        metadata_filters: &metadata_filters(),
        metadata: &metadata(),
        weight: 1.0,
    }];

    let decision = strategy.decide_consensus(ConsensusStrategyContext {
        indicators: &indicators,
        has_position: false,
    });
    assert_eq!(decision.intent, TradeIntent::OpenLong);
}

#[test]
fn consensus_new_accepts_valid_thresholds() {
    let strategy = ConsensusLongOnlyStrategy::new(2.0, 1.5).expect("valid thresholds");
    assert!((strategy.entry_threshold - 2.0).abs() < f64::EPSILON);
    assert!((strategy.exit_threshold - 1.5).abs() < f64::EPSILON);

    // Zero is a valid (boundary) threshold.
    assert!(ConsensusLongOnlyStrategy::new(0.0, 0.0).is_ok());
}

#[test]
fn consensus_new_rejects_negative_entry_threshold() {
    let result = ConsensusLongOnlyStrategy::new(-0.1, 1.0);
    assert!(result.is_err());
}

#[test]
fn consensus_new_rejects_negative_exit_threshold() {
    let result = ConsensusLongOnlyStrategy::new(1.0, -1.0);
    assert!(result.is_err());
}

#[test]
fn consensus_new_rejects_non_finite_thresholds() {
    assert!(ConsensusLongOnlyStrategy::new(f64::NAN, 1.0).is_err());
    assert!(ConsensusLongOnlyStrategy::new(1.0, f64::INFINITY).is_err());
}

#[test]
fn consensus_buy_requires_threshold_not_just_positive_score() {
    let mut strategy = ConsensusLongOnlyStrategy {
        entry_threshold: 1.0,
        exit_threshold: 1.0,
    };
    let indicators = [IndicatorObservation {
        id: "primary-1",
        role: IndicatorRole::PrimarySignal,
        signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
        signal: IndicatorSignal::BuyConfirmed,
        metadata_capabilities: metadata_capabilities(),
        metadata_filters: &metadata_filters(),
        metadata: &metadata(),
        weight: 0.75,
    }];

    let below_threshold = strategy.decide_consensus(ConsensusStrategyContext {
        indicators: &indicators,
        has_position: false,
    });
    assert_eq!(below_threshold.intent, TradeIntent::NoOp);

    let at_threshold = [IndicatorObservation {
        weight: 1.0,
        ..indicators[0]
    }];
    let entry = strategy.decide_consensus(ConsensusStrategyContext {
        indicators: &at_threshold,
        has_position: false,
    });
    assert_eq!(entry.intent, TradeIntent::OpenLong);
}

#[test]
fn consensus_filter_veto_blocks_entry() {
    let mut strategy = ConsensusLongOnlyStrategy::default();
    let blocked_indicators = [
        IndicatorObservation {
            id: "primary-1",
            role: IndicatorRole::PrimarySignal,
            signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
            signal: IndicatorSignal::BuyConfirmed,
            metadata_capabilities: metadata_capabilities(),
            metadata_filters: &metadata_filters(),
            metadata: &metadata(),
            weight: 1.0,
        },
        IndicatorObservation {
            id: "filter-1",
            role: IndicatorRole::Filter,
            signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
            signal: IndicatorSignal::SellConfirmed,
            metadata_capabilities: metadata_capabilities(),
            metadata_filters: &metadata_filters(),
            metadata: &metadata(),
            weight: 1.0,
        },
    ];

    let blocked = strategy.decide_consensus(ConsensusStrategyContext {
        indicators: &blocked_indicators,
        has_position: false,
    });
    assert_eq!(blocked.intent, TradeIntent::NoOp);
    assert!(
        blocked
            .rationale
            .as_deref()
            .is_some_and(|rationale| rationale.contains("filter_veto"))
    );

    let allowed_indicators = [
        blocked_indicators[0],
        IndicatorObservation {
            signal: IndicatorSignal::BuyConfirmed,
            ..blocked_indicators[1]
        },
    ];
    let allowed = strategy.decide_consensus(ConsensusStrategyContext {
        indicators: &allowed_indicators,
        has_position: false,
    });
    assert_eq!(allowed.intent, TradeIntent::OpenLong);
}

#[test]
fn preview_signal_is_ignored_when_policy_requires_confirmed() {
    let mut strategy = ConsensusLongOnlyStrategy::default();
    let filters = metadata_filters();
    let metadata = metadata();
    let indicators = [IndicatorObservation {
        id: "primary-1",
        role: IndicatorRole::PrimarySignal,
        signal_policy: IndicatorSignalPolicy::ConfirmedRequired,
        signal: IndicatorSignal::BuyPreview,
        metadata_capabilities: metadata_capabilities(),
        metadata_filters: &filters,
        metadata: &metadata,
        weight: 1.0,
    }];

    let no_intent = strategy.decide_consensus(ConsensusStrategyContext {
        indicators: &indicators,
        has_position: false,
    });
    assert_eq!(no_intent.intent, TradeIntent::NoOp);

    let preview_allowed = [IndicatorObservation {
        signal_policy: IndicatorSignalPolicy::PreviewAllowed,
        ..indicators[0]
    }];
    let open_intent = strategy.decide_consensus(ConsensusStrategyContext {
        indicators: &preview_allowed,
        has_position: false,
    });
    assert_eq!(open_intent.intent, TradeIntent::OpenLong);
}
