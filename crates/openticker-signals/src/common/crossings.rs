pub(crate) fn crossover(
    prev_left: Option<f64>,
    prev_right: Option<f64>,
    left: f64,
    right: f64,
) -> bool {
    // A crossover requires the left series to have been *strictly* below the
    // right series on the previous bar and to be strictly above it now. Using
    // strict `<` (rather than `<=`) means a flat/consolidation bar where the two
    // series were exactly equal does not count as having been "below"; the next
    // upward move is therefore not reported as a cross. Equal previous values
    // resolve to "no cross" — the series must have been unambiguously on the
    // lower side before we treat a move above as a crossing.
    matches!(
        (prev_left, prev_right),
        (Some(pl), Some(pr)) if pl < pr && left > right
    )
}

pub(crate) fn crossunder(
    prev_left: Option<f64>,
    prev_right: Option<f64>,
    left: f64,
    right: f64,
) -> bool {
    // Symmetric with `crossover`: a crossunder requires the left series to have
    // been *strictly* above the right series on the previous bar and to be
    // strictly below it now. Strict `>` (rather than `>=`) means an equal/flat
    // previous bar does not count as having been "above", so a subsequent
    // downward move is not reported as a cross. Equal previous values resolve to
    // "no cross".
    matches!(
        (prev_left, prev_right),
        (Some(pl), Some(pr)) if pl > pr && left < right
    )
}
