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

/// Classifies a US-equities timestamp into a market session.
///
/// # Known limitation: DST / EDT assumption
///
/// Equity session boundaries below are hardcoded in UTC for **Eastern Daylight
/// Time (EDT, UTC-4)**: regular hours map 9:30 AM–4:00 PM ET to 13:30–20:00
/// UTC. During Eastern Standard Time (EST, UTC-5, roughly November–March) the
/// real ET sessions shift one hour later in UTC, so this classification is off
/// by one hour in winter (e.g. 13:30 UTC is pre-market in EST, not regular
/// hours). Proper handling would require timezone-aware conversion via
/// `America/New_York` (e.g. `chrono-tz`), which is intentionally not pulled in
/// for this low-impact diagnostic classification. Callers needing exact ET
/// session boundaries year-round must account for the winter skew.
///
/// Note that the `AfterHours` variant is a catch-all for everything outside
/// pre-market and regular hours; it covers both the post-close extended session
/// (4:00 PM–8:00 PM ET) and the overnight closed window (midnight–4:00 AM ET),
/// so callers must not assume it implies only the post-close extended session.
#[must_use]
pub fn market_session_for(market: MarketType, timestamp: DateTime<Utc>) -> MarketSession {
    match market {
        MarketType::Crypto => MarketSession::Continuous,
        MarketType::Equities => {
            let hour = timestamp.hour();
            let minute = timestamp.minute();
            let minutes = hour * 60 + minute;

            // Boundaries assume EDT (UTC-4); see the function doc for the
            // one-hour EST (winter) skew limitation.
            // Regular session: 9:30 AM–4:00 PM ET == 13:30–20:00 UTC (EDT).
            if (13 * 60 + 30..20 * 60).contains(&minutes) {
                MarketSession::Regular
            // Pre-market: 4:00 AM–9:30 AM ET == 08:00–13:30 UTC (EDT).
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

    use super::{MarketSession, market_session_for};

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
