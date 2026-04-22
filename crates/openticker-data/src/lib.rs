mod bar_builder;
mod error;
mod market_session;
mod normalized;

pub use bar_builder::BarBuilder;
pub use error::DataError;
pub use market_session::{MarketSession, market_session_for};
pub use normalized::{NormalizedBarUpdate, NormalizedOrderEvent, NormalizedQuote, NormalizedTrade};

#[cfg(kani)]
mod proofs;
