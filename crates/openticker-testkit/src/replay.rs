use openticker_core::{OhlcvBar, SignalPhase};
use openticker_signals::IndicatorEngine;
use openticker_signals::sma_crossover::{
    SmaCrossoverError, SmaCrossoverIndicator, SmaCrossoverParams, SmaCrossoverSnapshot,
};

/// Replays bars through the `sma_crossover` indicator and returns all snapshots.
///
/// # Errors
///
/// Returns an error when `fast_length` or `slow_length` is invalid.
pub fn replay_sma_crossover(
    bars: &[OhlcvBar],
    params: Option<(usize, usize)>,
    phase: SignalPhase,
) -> Result<Vec<SmaCrossoverSnapshot>, SmaCrossoverError> {
    let params = match params {
        Some((fast_length, slow_length)) => SmaCrossoverParams::new(fast_length, slow_length)?,
        None => SmaCrossoverParams::default(),
    };

    let mut indicator = SmaCrossoverIndicator::new(params);
    Ok(bars
        .iter()
        .map(|bar| indicator.update(bar, phase))
        .collect::<Vec<_>>())
}
