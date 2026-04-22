use chrono::{DateTime, Timelike, Utc};
use openticker_core::MarketType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketSession {
    Continuous,
    PreMarket,
    Regular,
    AfterHours,
}

#[must_use]
pub fn market_session_for(market: MarketType, timestamp: DateTime<Utc>) -> MarketSession {
    match market {
        MarketType::Crypto => MarketSession::Continuous,
        MarketType::Equities => {
            let hour = timestamp.hour();
            let minute = timestamp.minute();
            let minutes = hour * 60 + minute;

            if (13 * 60 + 30..20 * 60).contains(&minutes) {
                MarketSession::Regular
            } else if (8 * 60..13 * 60 + 30).contains(&minutes) {
                MarketSession::PreMarket
            } else {
                MarketSession::AfterHours
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use openticker_core::MarketType;

    use crate::market_session::{MarketSession, market_session_for};

    #[test]
    fn classifies_market_sessions() {
        let regular = market_session_for(
            MarketType::Equities,
            DateTime::parse_from_rfc3339("2026-01-01T14:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        assert_eq!(regular, MarketSession::Regular);

        let pre_market = market_session_for(
            MarketType::Equities,
            DateTime::parse_from_rfc3339("2026-01-01T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        assert_eq!(pre_market, MarketSession::PreMarket);

        let crypto = market_session_for(
            MarketType::Crypto,
            DateTime::parse_from_rfc3339("2026-01-01T02:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        assert_eq!(crypto, MarketSession::Continuous);
    }
}
