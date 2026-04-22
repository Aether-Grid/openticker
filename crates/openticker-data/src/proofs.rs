use crate::{BarBuilder, DataError, NormalizedTrade};
use chrono::{Duration, LocalResult, TimeZone, Utc};
use openticker_core::{SignalPhase, Timeframe};

fn timestamp(seconds: i64) -> chrono::DateTime<Utc> {
    match Utc.timestamp_opt(seconds, 0) {
        LocalResult::Single(value) => value,
        _ => unreachable!(),
    }
}

fn trade(offset_seconds: i64, price: f64, quantity: f64) -> NormalizedTrade {
    NormalizedTrade {
        symbol: "AAPL".to_owned(),
        timestamp: timestamp(1_735_689_600) + Duration::seconds(offset_seconds),
        price,
        quantity,
    }
}

#[kani::proof]
fn proof_same_bucket_only_emits_preview() {
    let mut builder = BarBuilder::new("AAPL", Timeframe::M1);
    assert!(builder.push_trade(&trade(5, 100.0, 1.0)).is_ok());

    let updates = builder.push_trade(&trade(40, 101.0, 2.0));
    assert!(updates.is_ok());
    let updates = match updates {
        Ok(updates) => updates,
        Err(_) => unreachable!(),
    };

    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].phase, SignalPhase::Preview);
}

#[kani::proof]
fn proof_bucket_rollover_emits_confirmed_then_preview() {
    let mut builder = BarBuilder::new("AAPL", Timeframe::M1);
    assert!(builder.push_trade(&trade(5, 100.0, 1.0)).is_ok());

    let updates = builder.push_trade(&trade(60, 99.0, 1.0));
    assert!(updates.is_ok());
    let updates = match updates {
        Ok(updates) => updates,
        Err(_) => unreachable!(),
    };

    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0].phase, SignalPhase::Confirmed);
    assert_eq!(updates[1].phase, SignalPhase::Preview);
}

#[kani::proof]
fn proof_flush_confirmed_clears_state() {
    let mut builder = BarBuilder::new("AAPL", Timeframe::M1);
    assert!(builder.push_trade(&trade(5, 100.0, 1.0)).is_ok());

    let flushed = builder.flush_confirmed();
    assert!(matches!(flushed, Some(update) if update.phase == SignalPhase::Confirmed));
    assert!(builder.flush_confirmed().is_none());
}

#[kani::proof]
fn proof_invalid_trade_inputs_reject() {
    let mut builder = BarBuilder::new("AAPL", Timeframe::M1);

    let invalid_price = builder.push_trade(&trade(5, 0.0, 1.0));
    assert!(matches!(invalid_price, Err(DataError::InvalidPrice)));

    let invalid_quantity = builder.push_trade(&trade(5, 100.0, -1.0));
    assert!(matches!(invalid_quantity, Err(DataError::InvalidQuantity)));
}
