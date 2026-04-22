use super::cycle::RuntimeLaneCycleEngine;
use crate::{IndicatorSignal, OhlcvBar, ProcessBarOutcome, Runtime, ServiceError, SignalPhase};
use openticker_lane::{run_manual_signal_cycle, run_process_bar_cycle};

impl Runtime {
    /// Processes a bar for an instance and returns the resulting decision outcome.
    ///
    /// # Errors
    ///
    /// Returns an error when instance state, connectors, storage, or execution interactions fail.
    pub fn process_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
    ) -> Result<ProcessBarOutcome, ServiceError> {
        let lane_id = self.single_lane_id_for_bot(instance_id, "process_bar")?;
        self.process_bar_for_lane(&lane_id, bar, phase)
    }

    /// Processes a bar for a specific symbol lane.
    ///
    /// # Errors
    ///
    /// Returns an error when lane resolution fails or any downstream runtime,
    /// risk, storage, or execution interaction fails.
    pub fn process_bar_for_symbol(
        &mut self,
        instance_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
    ) -> Result<ProcessBarOutcome, ServiceError> {
        let lane_id = self.resolve_lane_id(instance_id, symbol)?;
        self.process_bar_for_lane(&lane_id, bar, phase)
    }

    pub(crate) fn process_bar_for_lane(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
    ) -> Result<ProcessBarOutcome, ServiceError> {
        run_process_bar_cycle(
            &mut RuntimeLaneCycleEngine { runtime: self },
            instance_id,
            bar,
            phase,
        )
    }

    /// Injects a manual signal into a running instance and executes the normal
    /// strategy, risk, execution, and journaling path against a synthetic bar.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance cannot accept the signal, the request
    /// is invalid, or downstream execution interactions fail.
    pub fn process_manual_signal(
        &mut self,
        instance_id: &str,
        signal: IndicatorSignal,
        price: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<ProcessBarOutcome, ServiceError> {
        let lane_id = self.single_lane_id_for_bot(instance_id, "process_manual_signal")?;
        self.process_manual_signal_for_lane(&lane_id, signal, price, timestamp)
    }

    /// Injects a manual signal into a specific symbol lane.
    ///
    /// # Errors
    ///
    /// Returns an error when lane resolution fails, the request is invalid, or
    /// downstream execution interactions fail.
    pub fn process_manual_signal_for_symbol(
        &mut self,
        instance_id: &str,
        symbol: &str,
        signal: IndicatorSignal,
        price: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<ProcessBarOutcome, ServiceError> {
        let lane_id = self.resolve_lane_id(instance_id, symbol)?;
        self.process_manual_signal_for_lane(&lane_id, signal, price, timestamp)
    }

    pub(crate) fn process_manual_signal_for_lane(
        &mut self,
        instance_id: &str,
        signal: IndicatorSignal,
        price: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<ProcessBarOutcome, ServiceError> {
        if signal == IndicatorSignal::None {
            return Err(ServiceError::InvalidConfiguration(
                "manual signal injection requires a non-none signal".to_owned(),
            ));
        }
        if !price.is_finite() || price <= 0.0 {
            return Err(ServiceError::InvalidConfiguration(
                "manual signal injection requires a finite positive price".to_owned(),
            ));
        }
        run_manual_signal_cycle(
            &mut RuntimeLaneCycleEngine { runtime: self },
            instance_id,
            signal,
            price,
            timestamp,
        )
    }
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
