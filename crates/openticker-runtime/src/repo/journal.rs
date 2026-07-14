use super::{RuntimeRepo, RuntimeRepoRead};
use crate::{
    BotEventWrite, BotSnapshotWrite, CycleTrace, CycleTraceWrite, EventWrite, FillWrite,
    IndicatorSignal, InstanceSummary, IntentWrite, LaneRuntimeState, OhlcvBar, OrderWrite,
    POSITION_QUANTITY_TOLERANCE, PositionWrite, ReconciliationAssessment, ReconciliationWrite,
    RiskDecision, RiskDecisionWrite, ServiceError, ServiceEventWrite, SignalMetadata, SignalPhase,
    SignalWrite, TradeIntent, execution_mode_to_storage, indicator_signal_label, log_runtime_event,
    signal_phase_label, trade_intent_label,
};

fn saturating_usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl RuntimeRepo<'_> {
    pub(crate) fn transition_instance(
        &mut self,
        lane_id: &str,
        target_state: LaneRuntimeState,
        event_kind: &str,
    ) -> Result<InstanceSummary, ServiceError> {
        let previous_state;
        let bot_id;
        {
            let instance = self.instance_mut(lane_id)?;
            previous_state = instance.state;
            bot_id = instance.config.id.clone();
            instance.state = target_state;
        }
        let summary = self.as_read().get_instance(&bot_id)?;

        if let Err(error) = self
            .as_read()
            .persist_instance_state(&summary)
            .and_then(|()| {
                self.as_read()
                    .append_instance_event(lane_id, event_kind, target_state, "")
            })
        {
            if let Ok(instance) = self.instance_mut(lane_id) {
                instance.state = previous_state;
            }
            return Err(error);
        }

        Ok(summary)
    }
}

impl RuntimeRepoRead<'_> {
    pub(crate) fn persist_boot_state(&self) -> Result<(), ServiceError> {
        self.append_service_event(
            "service.started",
            format!("instances={}", self.state.lanes.len()),
        )?;

        for summary in self.list_instances() {
            self.persist_instance_state(&summary)?;
        }

        Ok(())
    }

    pub(crate) fn persist_instance_state(
        &self,
        summary: &InstanceSummary,
    ) -> Result<(), ServiceError> {
        self.journal
            .upsert_bot_snapshot(BotSnapshotWrite {
                bot_id: summary.id.clone(),
                state: summary.state.as_storage_value().to_owned(),
                execution_mode: execution_mode_to_storage(summary.execution_mode).to_owned(),
                enabled: summary.enabled,
            })
            .map_err(ServiceError::from)
    }

    pub(crate) fn append_instance_event(
        &self,
        lane_id: &str,
        kind: &str,
        state: LaneRuntimeState,
        detail: &str,
    ) -> Result<(), ServiceError> {
        let payload = if detail.is_empty() {
            format!("state={}", state.as_storage_value())
        } else {
            format!("state={},detail={detail}", state.as_storage_value())
        };

        self.journal
            .append_bot_event(BotEventWrite {
                bot_id: lane_id.to_owned(),
                kind: kind.to_owned(),
                payload: payload.clone(),
            })
            .map_err(ServiceError::from)?;

        self.append_runtime_event("instance", Some(lane_id), kind, payload)
    }

    pub(crate) fn append_service_event(
        &self,
        kind: &str,
        payload: String,
    ) -> Result<(), ServiceError> {
        self.journal
            .append_service_event(ServiceEventWrite {
                kind: kind.to_owned(),
                payload: payload.clone(),
            })
            .map_err(ServiceError::from)?;

        self.append_runtime_event("service", None, kind, payload)
    }

    pub(crate) fn append_signal_record_with_trace(
        &self,
        lane_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        signal: IndicatorSignal,
        metadata: &SignalMetadata,
        trace_id: Option<&str>,
    ) -> Result<(), ServiceError> {
        let instance = self.instance(lane_id)?;
        let write = SignalWrite {
            bot_id: instance.config.id.clone(),
            symbol: instance.lane_symbol.clone(),
            trace_id: trace_id.map(str::to_owned),
            bar_timestamp: bar.timestamp.to_rfc3339(),
            phase: signal_phase_label(phase).to_owned(),
            signal: indicator_signal_label(signal).to_owned(),
            close: bar.close,
            metadata_json: (!metadata.is_empty())
                .then(|| serde_json::to_string(metadata))
                .transpose()?,
        };
        self.journal
            .append_signal(write.clone())
            .map_err(ServiceError::from)?;
        self.read_models.record_signal_write(&write);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_intent_record_with_trace(
        &self,
        lane_id: &str,
        bar: &OhlcvBar,
        signal: IndicatorSignal,
        metadata: &SignalMetadata,
        intent: TradeIntent,
        strategy_rationale: Option<&str>,
        has_position_before: bool,
        trace_id: Option<&str>,
    ) -> Result<(), ServiceError> {
        let instance = self.instance(lane_id)?;
        let write = IntentWrite {
            bot_id: instance.config.id.clone(),
            symbol: instance.lane_symbol.clone(),
            trace_id: trace_id.map(str::to_owned),
            bar_timestamp: bar.timestamp.to_rfc3339(),
            signal: indicator_signal_label(signal).to_owned(),
            intent: trade_intent_label(intent).to_owned(),
            metadata_json: (!metadata.is_empty())
                .then(|| serde_json::to_string(metadata))
                .transpose()?,
            strategy_rationale: strategy_rationale.map(str::to_owned),
            has_position_before,
        };
        self.journal
            .append_intent(write.clone())
            .map_err(ServiceError::from)?;
        self.read_models.record_intent_write(&write);
        Ok(())
    }

    pub(crate) fn append_risk_decision_record_with_trace(
        &self,
        lane_id: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        decision: &RiskDecision,
        trace_id: Option<&str>,
    ) -> Result<(), ServiceError> {
        let (decision_label, reason) = match decision {
            RiskDecision::Allow(_) => ("allowed", None),
            RiskDecision::Reject { reason } => ("rejected", Some((*reason).to_owned())),
        };
        let instance = self.instance(lane_id)?;

        let write = RiskDecisionWrite {
            bot_id: instance.config.id.clone(),
            symbol: instance.lane_symbol.clone(),
            trace_id: trace_id.map(str::to_owned),
            bar_timestamp: bar.timestamp.to_rfc3339(),
            intent: trade_intent_label(intent).to_owned(),
            decision: decision_label.to_owned(),
            reason,
        };
        self.journal
            .append_risk_decision(write.clone())
            .map_err(ServiceError::from)?;
        self.read_models.record_risk_decision_write(&write);
        Ok(())
    }

    pub(crate) fn append_order_record(
        &self,
        lane_id: &str,
        client_order_id: &str,
        intent: TradeIntent,
        status: &str,
        price: f64,
        quantity: f64,
    ) -> Result<(), ServiceError> {
        self.append_order_record_with_trace(
            lane_id,
            client_order_id,
            intent,
            status,
            price,
            quantity,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_order_record_with_trace(
        &self,
        lane_id: &str,
        client_order_id: &str,
        intent: TradeIntent,
        status: &str,
        price: f64,
        quantity: f64,
        bar: Option<&OhlcvBar>,
        trace_id: Option<&str>,
    ) -> Result<(), ServiceError> {
        let instance = self.instance(lane_id)?;
        let write = OrderWrite {
            bot_id: instance.config.id.clone(),
            symbol: instance.lane_symbol.clone(),
            trace_id: trace_id.map(str::to_owned),
            bar_timestamp: bar.map(|value| value.timestamp.to_rfc3339()),
            client_order_id: client_order_id.to_owned(),
            intent: trade_intent_label(intent).to_owned(),
            status: status.to_owned(),
            price,
            quantity,
        };
        self.journal
            .append_order(write.clone())
            .map_err(ServiceError::from)?;
        self.read_models.record_order_write(&write);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_fill_record(
        &self,
        lane_id: &str,
        client_order_id: &str,
        price: f64,
        quantity: f64,
        fee_asset: Option<String>,
        fee_amount: Option<f64>,
        fee_normalized_usd: Option<f64>,
    ) -> Result<(), ServiceError> {
        self.append_fill_record_with_trace(
            lane_id,
            client_order_id,
            price,
            quantity,
            fee_asset,
            fee_amount,
            fee_normalized_usd,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_fill_record_with_trace(
        &self,
        lane_id: &str,
        client_order_id: &str,
        price: f64,
        quantity: f64,
        fee_asset: Option<String>,
        fee_amount: Option<f64>,
        fee_normalized_usd: Option<f64>,
        bar: Option<&OhlcvBar>,
        trace_id: Option<&str>,
    ) -> Result<(), ServiceError> {
        let instance = self.instance(lane_id)?;
        let write = FillWrite {
            bot_id: instance.config.id.clone(),
            symbol: instance.lane_symbol.clone(),
            trace_id: trace_id.map(str::to_owned),
            bar_timestamp: bar.map(|value| value.timestamp.to_rfc3339()),
            client_order_id: client_order_id.to_owned(),
            price,
            quantity,
            fee_asset,
            fee_amount,
            fee_normalized_usd,
        };
        self.journal
            .append_fill(write.clone())
            .map_err(ServiceError::from)?;
        self.read_models.record_fill_write(&write);
        Ok(())
    }

    pub(crate) fn append_position_record(
        &self,
        lane_id: &str,
        has_position: bool,
        quantity: f64,
        entry_price: Option<f64>,
        realized_pnl_usd: f64,
        reason: &str,
    ) -> Result<(), ServiceError> {
        self.append_position_record_with_trace(
            lane_id,
            has_position,
            quantity,
            entry_price,
            realized_pnl_usd,
            reason,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_position_record_with_trace(
        &self,
        lane_id: &str,
        has_position: bool,
        quantity: f64,
        entry_price: Option<f64>,
        realized_pnl_usd: f64,
        reason: &str,
        bar: Option<&OhlcvBar>,
        trace_id: Option<&str>,
    ) -> Result<(), ServiceError> {
        let quantity = if quantity.is_finite() && quantity > POSITION_QUANTITY_TOLERANCE {
            quantity
        } else {
            0.0
        };
        let entry_price = entry_price.filter(|price| price.is_finite() && *price > 0.0);
        let realized_pnl_usd = if realized_pnl_usd.is_finite() {
            realized_pnl_usd
        } else {
            0.0
        };
        let instance = self.instance(lane_id)?;

        let write = PositionWrite {
            bot_id: instance.config.id.clone(),
            symbol: instance.lane_symbol.clone(),
            trace_id: trace_id.map(str::to_owned),
            bar_timestamp: bar.map(|value| value.timestamp.to_rfc3339()),
            has_position,
            quantity,
            entry_price,
            realized_pnl_usd,
            reason: reason.to_owned(),
        };

        self.journal
            .append_position(write.clone())
            .map_err(ServiceError::from)?;
        self.read_models.record_position_write(&write);
        Ok(())
    }

    pub(crate) fn append_reconciliation_record(
        &self,
        lane_id: &str,
        source: &str,
        assessment: &ReconciliationAssessment,
    ) -> Result<(), ServiceError> {
        let write = ReconciliationWrite {
            bot_id: lane_id.to_owned(),
            source: source.to_owned(),
            symbol: assessment.symbol.clone(),
            safe_to_trade: assessment.safe_to_trade,
            local_open_orders: saturating_usize_to_i64(assessment.local_open_orders),
            connector_open_orders: saturating_usize_to_i64(assessment.connector_open_orders),
            local_has_position: assessment.positions.local_has_position,
            connector_has_position: assessment.positions.connector_has_position,
            reason: assessment.reason.clone(),
        };
        self.journal
            .append_reconciliation(write.clone())
            .map_err(ServiceError::from)?;
        self.read_models.record_reconciliation_write(&write);
        Ok(())
    }

    pub(crate) fn append_runtime_event(
        &self,
        scope: &str,
        entity_id: Option<&str>,
        kind: &str,
        payload: String,
    ) -> Result<(), ServiceError> {
        self.append_runtime_event_with_trace(scope, entity_id, kind, payload, None)
    }

    pub(crate) fn append_runtime_event_with_trace(
        &self,
        scope: &str,
        entity_id: Option<&str>,
        kind: &str,
        payload: String,
        trace_id: Option<&str>,
    ) -> Result<(), ServiceError> {
        let payload_for_log = payload.clone();
        let write = EventWrite {
            scope: scope.to_owned(),
            entity_id: entity_id.map(ToOwned::to_owned),
            trace_id: trace_id.map(str::to_owned),
            kind: kind.to_owned(),
            payload,
        };
        self.journal
            .append_event(write.clone())
            .map_err(ServiceError::from)?;
        self.read_models.record_runtime_event_write(&write);

        log_runtime_event(scope, entity_id, kind, &payload_for_log);
        Ok(())
    }

    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn append_provider_event(
        &self,
        lane_id: &str,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<(), ServiceError> {
        self.append_runtime_event("provider", Some(lane_id), kind, payload.to_string())
    }

    pub(crate) fn append_cycle_trace(&self, trace: &CycleTrace) -> Result<(), ServiceError> {
        let payload_json = serde_json::to_string(trace)?;
        self.journal
            .append_cycle_trace(CycleTraceWrite {
                trace_id: trace.summary.trace_id.clone(),
                bot_id: trace.summary.bot_id.clone(),
                symbol: trace.summary.symbol.clone(),
                bar_timestamp: trace.summary.bar_timestamp.clone(),
                phase: serde_json::to_string(&trace.summary.phase)?
                    .trim_matches('"')
                    .to_owned(),
                trigger_kind: serde_json::to_string(&trace.summary.trigger_kind)?
                    .trim_matches('"')
                    .to_owned(),
                signal: serde_json::to_string(&trace.summary.signal)?
                    .trim_matches('"')
                    .to_owned(),
                intent: serde_json::to_string(&trace.summary.intent)?
                    .trim_matches('"')
                    .to_owned(),
                risk_decision: serde_json::to_string(&trace.summary.risk_decision)?
                    .trim_matches('"')
                    .to_owned(),
                outcome: serde_json::to_string(&trace.summary.outcome)?
                    .trim_matches('"')
                    .to_owned(),
                payload_json,
            })
            .map_err(ServiceError::from)
    }
}
