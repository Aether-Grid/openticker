use openticker_core::OhlcvBar;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct StreamBuffer {
    retention: usize,
    bars: VecDeque<OhlcvBar>,
}

impl StreamBuffer {
    #[must_use]
    pub fn new(retention: usize) -> Self {
        Self {
            retention,
            bars: VecDeque::with_capacity(retention),
        }
    }

    pub fn set_retention(&mut self, retention: usize) {
        self.retention = retention;
        self.trim_to_retention();
    }

    #[must_use]
    pub fn push_if_newer(&mut self, bar: OhlcvBar) -> bool {
        if self
            .bars
            .back()
            .is_some_and(|latest| latest.timestamp >= bar.timestamp)
        {
            return false;
        }

        self.bars.push_back(bar);
        self.trim_to_retention();
        true
    }

    #[must_use]
    pub fn latest(&self) -> Option<OhlcvBar> {
        self.bars.back().cloned()
    }

    #[must_use]
    pub fn snapshot(&self, limit: usize) -> Vec<OhlcvBar> {
        let limit = limit.max(1);
        let skip = self.bars.len().saturating_sub(limit);
        self.bars.iter().skip(skip).cloned().collect()
    }

    #[must_use]
    pub fn sparkline(&self, limit: usize) -> Vec<f64> {
        self.snapshot(limit)
            .into_iter()
            .map(|bar| bar.close)
            .collect()
    }

    fn trim_to_retention(&mut self) {
        while self.bars.len() > self.retention {
            let _ = self.bars.pop_front();
        }
    }
}
