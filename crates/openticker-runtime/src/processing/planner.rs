use crate::repo::RuntimeRepo;
use crate::{
    AccountRiskSnapshot, IndicatorSignal, LaneRuntimeState, OhlcvBar, ProcessBarEvaluation,
    Runtime, ServiceError, SignalPhase, resolve_effective_execution_constraints,
    resolve_order_quantity_with_constraints,
};
use openticker_lane::{
    PreparedLaneEvaluation, SignalEvaluationKernelInput, build_process_bar_evaluation,
    market_data_is_stale, prepare_manual_signal_evaluation, prepare_process_bar_evaluation,
};
use openticker_trace::BudgetRoomContext;

pub(crate) struct ProcessingPlanner<'a> {
    repo: RuntimeRepo<'a>,
}

impl Runtime {
    pub(crate) fn processing_planner_mut(&mut self) -> ProcessingPlanner<'_> {
        ProcessingPlanner {
            repo: self.repo_mut(),
        }
    }
}

impl ProcessingPlanner<'_> {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn evaluate_process_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        account_risk: AccountRiskSnapshot,
    ) -> Result<ProcessBarEvaluation, ServiceError> {
        let prepared = {
            let instance = self.repo.instance_mut(instance_id)?;
            if !instance.config.enabled {
                return Err(ServiceError::InstanceDisabled(instance_id.to_owned()));
            }
            if !matches!(instance.state, LaneRuntimeState::Running) {
                return Err(ServiceError::InvalidTransition {
                    instance_id: instance_id.to_owned(),
                    state: instance.state,
                    action: "process_bar",
                });
            }
            prepare_process_bar_evaluation(instance, bar, phase)
        };

        self.finalize_signal_evaluation(
            bar,
            account_risk,
            self.repo.state.kill_switch_active,
            prepared,
        )
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn evaluate_manual_signal(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        _phase: SignalPhase,
        signal: IndicatorSignal,
        account_risk: AccountRiskSnapshot,
    ) -> Result<ProcessBarEvaluation, ServiceError> {
        let prepared = {
            let instance = self.repo.instance_mut(instance_id)?;
            if !instance.config.enabled {
                return Err(ServiceError::InstanceDisabled(instance_id.to_owned()));
            }
            if !matches!(instance.state, LaneRuntimeState::Running) {
                return Err(ServiceError::InvalidTransition {
                    instance_id: instance_id.to_owned(),
                    state: instance.state,
                    action: "process_manual_signal",
                });
            }
            prepare_manual_signal_evaluation(instance, bar, signal)
        };

        self.finalize_signal_evaluation(
            bar,
            account_risk,
            self.repo.state.kill_switch_active,
            prepared,
        )
    }

    fn finalize_signal_evaluation(
        &mut self,
        bar: &OhlcvBar,
        account_risk: AccountRiskSnapshot,
        kill_switch_active: bool,
        prepared: PreparedLaneEvaluation,
    ) -> Result<ProcessBarEvaluation, ServiceError> {
        let effective_execution_constraints = resolve_effective_execution_constraints(
            &prepared.instance_execution_constraints,
            prepared.connector_execution_constraints.as_ref(),
        );
        self.repo
            .as_read()
            .sync_account_ledger_from_instances(&prepared.account_id)?;
        let ledger_rooms = self.repo.as_read().account_ledger_rooms(
            &prepared.bot_id,
            &prepared.account_id,
            prepared.bot_budget_pct,
        )?;
        let order_quantity_resolution = resolve_order_quantity_with_constraints(
            prepared.intent,
            prepared.market,
            bar.close,
            prepared.current_position_quantity,
            ledger_rooms.remaining_bot_usd,
            ledger_rooms.remaining_account_usd,
            prepared.risk_limits,
            prepared.target_order_notional_usd,
            &effective_execution_constraints,
            prepared.connector_fractional_entry_supported == Some(true),
        );
        let stale_data =
            market_data_is_stale(bar, prepared.timeframe, prepared.risk_limits.stale_data_ms);
        Ok(build_process_bar_evaluation(SignalEvaluationKernelInput {
            signal: prepared.signal,
            signal_metadata: prepared.signal_metadata,
            signal_source: prepared.signal_source,
            intent: prepared.intent,
            strategy_rationale: prepared.strategy_rationale,
            bar_close: bar.close,
            stale_data,
            account_open_positions: account_risk.open_positions,
            account_daily_loss_pct: account_risk.daily_loss_pct,
            risk_limits: prepared.risk_limits,
            order_quantity_resolution,
            budget_room: BudgetRoomContext {
                remaining_bot_usd: ledger_rooms.remaining_bot_usd,
                remaining_account_usd: ledger_rooms.remaining_account_usd,
            },
            kill_switch_active,
            has_position_before: prepared.has_position_before,
            cooldown_active: prepared.cooldown_active,
        }))
    }
}

#[cfg(kani)]
mod proofs {
    use super::{SignalEvaluationKernelInput, build_process_bar_evaluation};
    use crate::{
        IndicatorSignal, OrderLedgerOutcome, OrderQuantityResolution, RiskDecision, RiskLimits,
        SignalMetadata, TradeIntent,
    };
    use openticker_lane::StrategySignalSource;
    use openticker_trace::BudgetRoomContext;

    fn limits() -> RiskLimits {
        RiskLimits {
            max_daily_loss_pct: 5.0,
            max_open_positions: 5,
            max_order_notional_usd: 10_000.0,
            max_spread_bps: 20,
            max_slippage_bps: 30,
            stale_data_ms: 3_000,
            cooldown_after_reject_ms: 1_000,
        }
    }

    fn budget_room() -> BudgetRoomContext {
        BudgetRoomContext {
            remaining_bot_usd: 1_000.0,
            remaining_account_usd: 1_000.0,
        }
    }

    #[kani::proof]
    fn proof_kernel_reject_keeps_position_state() {
        let has_position_before = kani::any();
        let evaluation = build_process_bar_evaluation(SignalEvaluationKernelInput {
            signal: IndicatorSignal::BuyConfirmed,
            signal_metadata: SignalMetadata::default(),
            signal_source: StrategySignalSource::Representative,
            intent: TradeIntent::OpenLong,
            strategy_rationale: None,
            bar_close: 100.0,
            stale_data: false,
            account_open_positions: 0,
            account_daily_loss_pct: 0.0,
            risk_limits: limits(),
            order_quantity_resolution: OrderQuantityResolution {
                quantity: 1.0,
                adjustment_reason: None,
                ledger_outcome: None,
            },
            budget_room: budget_room(),
            kill_switch_active: true,
            has_position_before,
            cooldown_active: false,
        });

        assert!(matches!(
            evaluation.risk_decision,
            RiskDecision::Reject { .. }
        ));
        assert_eq!(evaluation.next_has_position, has_position_before);
    }

    #[kani::proof]
    fn proof_kernel_open_allow_sets_next_position_true() {
        let evaluation = build_process_bar_evaluation(SignalEvaluationKernelInput {
            signal: IndicatorSignal::BuyConfirmed,
            signal_metadata: SignalMetadata::default(),
            signal_source: StrategySignalSource::Representative,
            intent: TradeIntent::OpenLong,
            strategy_rationale: None,
            bar_close: 100.0,
            stale_data: false,
            account_open_positions: 0,
            account_daily_loss_pct: 0.0,
            risk_limits: limits(),
            order_quantity_resolution: OrderQuantityResolution {
                quantity: 1.0,
                adjustment_reason: None,
                ledger_outcome: None,
            },
            budget_room: budget_room(),
            kill_switch_active: false,
            has_position_before: false,
            cooldown_active: false,
        });

        assert!(matches!(
            evaluation.risk_decision,
            RiskDecision::Allow(TradeIntent::OpenLong)
        ));
        assert!(evaluation.next_has_position);
    }

    #[kani::proof]
    fn proof_kernel_open_dust_becomes_noop() {
        let evaluation = build_process_bar_evaluation(SignalEvaluationKernelInput {
            signal: IndicatorSignal::BuyConfirmed,
            signal_metadata: SignalMetadata::default(),
            signal_source: StrategySignalSource::Representative,
            intent: TradeIntent::OpenLong,
            strategy_rationale: None,
            bar_close: 100.0,
            stale_data: false,
            account_open_positions: 0,
            account_daily_loss_pct: 0.0,
            risk_limits: limits(),
            order_quantity_resolution: OrderQuantityResolution {
                quantity: 0.0,
                adjustment_reason: Some("dust".to_owned()),
                ledger_outcome: Some(OrderLedgerOutcome::Dust),
            },
            budget_room: budget_room(),
            kill_switch_active: false,
            has_position_before: false,
            cooldown_active: false,
        });

        assert!(matches!(
            evaluation.risk_decision,
            RiskDecision::Allow(TradeIntent::NoOp)
        ));
        assert!(!evaluation.next_has_position);
    }
}
