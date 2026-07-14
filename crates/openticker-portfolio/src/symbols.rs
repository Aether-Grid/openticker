pub(crate) fn connector_position_matches_symbol(connector_symbol: &str, symbol: &str) -> bool {
    connector_symbol == symbol
        || symbol_base_asset(symbol)
            .is_some_and(|asset| connector_symbol.eq_ignore_ascii_case(asset))
}

pub(crate) fn symbol_base_asset(symbol: &str) -> Option<&str> {
    const QUOTE_SUFFIXES: [&str; 6] = ["FDUSD", "USDT", "USDC", "BUSD", "USD", "BTC"];
    QUOTE_SUFFIXES.iter().find_map(|suffix| {
        if symbol.len() > suffix.len() && symbol.ends_with(suffix) {
            Some(&symbol[..symbol.len() - suffix.len()])
        } else {
            None
        }
    })
}
