use openticker_core::usize_to_f64;
use std::collections::VecDeque;
use toml::Table;

#[derive(Debug, Clone)]
pub(crate) struct Sma {
    length: usize,
    values: VecDeque<f64>,
    sum: f64,
}

impl Sma {
    pub(crate) fn new(length: usize) -> Self {
        Self {
            length,
            values: VecDeque::new(),
            sum: 0.0,
        }
    }

    pub(crate) fn update(&mut self, value: f64) -> Option<f64> {
        self.values.push_back(value);
        self.sum += value;
        if self.values.len() > self.length
            && let Some(removed) = self.values.pop_front()
        {
            self.sum -= removed;
        }

        if self.values.len() == self.length {
            Some(self.sum / usize_to_f64(self.length))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WilderRma {
    length: usize,
    samples: usize,
    seed_sum: f64,
    current: Option<f64>,
}

impl WilderRma {
    pub(crate) fn new(length: usize) -> Self {
        Self {
            length,
            samples: 0,
            seed_sum: 0.0,
            current: None,
        }
    }

    pub(crate) fn update(&mut self, value: f64) -> Option<f64> {
        if let Some(previous) = self.current {
            let length = usize_to_f64(self.length);
            let next = previous + ((value - previous) / length);
            self.current = Some(next);
            self.current
        } else {
            self.samples += 1;
            self.seed_sum += value;
            if self.samples < self.length {
                None
            } else {
                let seeded = self.seed_sum / usize_to_f64(self.length);
                self.current = Some(seeded);
                self.current
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Rsi {
    up: WilderRma,
    down: WilderRma,
    prev: Option<f64>,
}

impl Rsi {
    pub(crate) fn new(length: usize) -> Self {
        Self {
            up: WilderRma::new(length),
            down: WilderRma::new(length),
            prev: None,
        }
    }

    pub(crate) fn update(&mut self, value: f64) -> Option<f64> {
        let change = if let Some(previous) = self.prev {
            value - previous
        } else {
            self.prev = Some(value);
            return None;
        };
        self.prev = Some(value);

        let up = self.up.update(change.max(0.0));
        let down = self.down.update((-change).max(0.0));

        match (up, down) {
            (Some(_), Some(0.0)) => Some(100.0),
            (Some(0.0), Some(_)) => Some(0.0),
            (Some(up_value), Some(down_value)) => {
                let rs = up_value / down_value;
                Some(100.0 - (100.0 / (1.0 + rs)))
            }
            _ => None,
        }
    }
}

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

pub(crate) fn indicator_param_f64(params: &Table, key: &str) -> Option<f64> {
    params.get(key).and_then(|value| {
        value.as_float().or_else(|| {
            value
                .as_integer()
                .and_then(|integer| integer.to_string().parse::<f64>().ok())
        })
    })
}

#[allow(clippy::redundant_closure_for_method_calls)]
pub(crate) fn indicator_param_usize(params: &Table, key: &str) -> Option<usize> {
    params
        .get(key)
        .and_then(|value| value.as_integer())
        .and_then(|integer| usize::try_from(integer).ok())
}
