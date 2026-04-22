use openticker_core::{
    AccountId, BotLaneKey, CoreError, InstanceId, SignalPhase, Timeframe, TradeIntent,
};

#[test]
fn trade_intent_serde_roundtrip_is_stable() {
    let cases = [
        (TradeIntent::NoOp, "no_op"),
        (TradeIntent::OpenLong, "open_long"),
        (TradeIntent::AddLong, "add_long"),
        (TradeIntent::ReduceLong, "reduce_long"),
        (TradeIntent::CloseLong, "close_long"),
    ];

    for (intent, label) in cases {
        assert_eq!(
            serde_json::to_string(&intent).unwrap(),
            format!("\"{label}\"")
        );
        let decoded: TradeIntent = serde_json::from_str(&format!("\"{label}\"")).unwrap();
        assert_eq!(decoded, intent);
    }
}

#[test]
fn signal_phase_serde_roundtrip_is_stable() {
    let cases = [
        (SignalPhase::Preview, "preview"),
        (SignalPhase::Confirmed, "confirmed"),
    ];

    for (phase, label) in cases {
        assert_eq!(
            serde_json::to_string(&phase).unwrap(),
            format!("\"{label}\"")
        );
        let decoded: SignalPhase = serde_json::from_str(&format!("\"{label}\"")).unwrap();
        assert_eq!(decoded, phase);
    }
}

#[test]
fn timeframe_serde_labels_are_stable() {
    let cases = [
        (Timeframe::M1, "1m"),
        (Timeframe::M5, "5m"),
        (Timeframe::M15, "15m"),
        (Timeframe::M30, "30m"),
        (Timeframe::H1, "1h"),
        (Timeframe::H4, "4h"),
        (Timeframe::D1, "1d"),
    ];

    for (timeframe, label) in cases {
        assert_eq!(
            serde_json::to_string(&timeframe).unwrap(),
            format!("\"{label}\"")
        );
        let decoded: Timeframe = serde_json::from_str(&format!("\"{label}\"")).unwrap();
        assert_eq!(decoded, timeframe);
    }
}

#[test]
fn timeframe_duration_matches_expected_minutes() {
    let cases = [
        (Timeframe::M1, 1_u64),
        (Timeframe::M5, 5_u64),
        (Timeframe::M15, 15_u64),
        (Timeframe::M30, 30_u64),
        (Timeframe::H1, 60_u64),
        (Timeframe::H4, 240_u64),
        (Timeframe::D1, 1_440_u64),
    ];

    for (timeframe, expected_minutes) in cases {
        assert_eq!(timeframe.duration().as_secs(), expected_minutes * 60);
    }
}

#[test]
fn identifier_parsing_rejects_blank_values() {
    assert_eq!(
        InstanceId::parse("  ").unwrap_err(),
        CoreError::InvalidIdentifier("instance id")
    );
    assert_eq!(
        AccountId::parse("\n").unwrap_err(),
        CoreError::InvalidIdentifier("account id")
    );
    assert_eq!(
        BotLaneKey::parse("bot", "   ").unwrap_err(),
        CoreError::InvalidIdentifier("symbol")
    );
    assert_eq!(
        BotLaneKey::parse("  ", "AAPL").unwrap_err(),
        CoreError::InvalidIdentifier("bot id")
    );
}
