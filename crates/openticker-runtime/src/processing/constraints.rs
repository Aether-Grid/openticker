use crate::{MarketType, Runtime, ServiceError, execution_constraints_are_complete};
use openticker_gateway::{Gateway, GatewayError};
use tracing::warn;

impl Runtime {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn ensure_lane_connector_execution_constraints(
        &mut self,
        lane_id: &str,
        account_id: &str,
    ) -> Result<(), ServiceError> {
        let (reconciliation_remote_snapshot, connector_kind) = {
            let account = self.catalog.accounts.get(account_id).ok_or_else(|| {
                ServiceError::InvalidConfiguration(format!(
                    "instance `{lane_id}` references unknown account `{account_id}`"
                ))
            })?;
            (account.reconciliation_remote_snapshot, account.kind.clone())
        };

        let (already_initialized, config_constraints_complete, market, symbol) = {
            let lane = self.instance(lane_id)?;
            (
                lane.connector_execution_constraints_initialized,
                execution_constraints_are_complete(&lane.config.execution_constraints),
                lane.config.market,
                lane.lane_symbol.clone(),
            )
        };

        if already_initialized {
            return Ok(());
        }

        if !reconciliation_remote_snapshot
            || (config_constraints_complete && !matches!(market, MarketType::Equities))
        {
            let lane = self.instance_mut(lane_id)?;
            lane.connector_execution_constraints_initialized = true;
            return Ok(());
        }

        {
            let repo = self.repo();
            repo.provider_operation(
                lane_id,
                "provider.symbol_constraints",
                account_id,
                connector_kind.as_str(),
                "fetch_symbol_constraints",
            )
            .record_stage(
                "requested",
                format!("requesting symbol constraints for {}", symbol.as_str()),
                serde_json::json!({
                    "request": {
                        "symbol": symbol.as_str(),
                    },
                }),
            )?;
        }

        match Gateway::new(self.connectors.clone())
            .fetch_normalized_symbol_constraints_unchecked(account_id, &symbol)
        {
            Ok(normalized_constraints) => {
                let normalized = normalized_constraints.execution_constraints.clone();
                let fractional_entry_supported = normalized_constraints.fractional_entry_supported;
                let has_numeric_constraints = normalized_constraints.has_numeric_constraints();
                let has_connector_metadata =
                    has_numeric_constraints || fractional_entry_supported.is_some();

                {
                    let lane = self.instance_mut(lane_id)?;
                    lane.connector_execution_constraints_initialized = true;
                    lane.connector_fractional_entry_supported = fractional_entry_supported;
                    if has_numeric_constraints {
                        lane.connector_execution_constraints = Some(normalized.clone());
                    } else {
                        lane.connector_execution_constraints = None;
                    }
                }

                if has_connector_metadata {
                    self.repo().append_runtime_event(
                        "order",
                        Some(lane_id),
                        "order.quantity_constraints_resolved",
                        serde_json::json!({
                            "symbol": symbol.as_str(),
                            "account_id": account_id,
                            "fractional_entry_supported": fractional_entry_supported,
                            "quantity_step": normalized.quantity_step,
                            "min_quantity": normalized.min_quantity,
                            "min_notional_usd": normalized.min_notional_usd,
                            "source": normalized_constraints.source,
                        })
                        .to_string(),
                    )?;
                }

                {
                    let repo = self.repo();
                    repo.provider_operation(
                        lane_id,
                        "provider.symbol_constraints",
                        account_id,
                        connector_kind.as_str(),
                        "fetch_symbol_constraints",
                    )
                    .record_stage(
                        "succeeded",
                        format!("resolved symbol constraints for {}", symbol.as_str()),
                        serde_json::json!({
                            "request": {
                                "symbol": symbol.as_str(),
                            },
                            "response": {
                                "fractional_entry_supported": fractional_entry_supported,
                                "quantity_step": normalized.quantity_step,
                                "min_quantity": normalized.min_quantity,
                                "min_notional_usd": normalized.min_notional_usd,
                                "source": normalized_constraints.source,
                            },
                        }),
                    )?;
                }
            }
            Err(GatewayError::LockPoisoned) => {
                return Err(ServiceError::InvalidConfiguration(
                    "connector registry lock poisoned".to_owned(),
                ));
            }
            Err(error) => {
                {
                    let lane = self.instance_mut(lane_id)?;
                    lane.connector_execution_constraints_initialized = true;
                }

                warn!(
                    lane_id = %lane_id,
                    account_id = %account_id,
                    symbol = %symbol,
                    error = %error,
                    "connector symbol constraints unavailable; continuing with config-only constraints"
                );

                self.repo().append_runtime_event(
                    "order",
                    Some(lane_id),
                    "order.quantity_constraints_unavailable",
                    format!(
                        "account_id={account_id},symbol={},reason={error}",
                        symbol.as_str()
                    ),
                )?;

                let repo = self.repo();
                let _ = repo
                    .provider_operation(
                        lane_id,
                        "provider.symbol_constraints",
                        account_id,
                        connector_kind.as_str(),
                        "fetch_symbol_constraints",
                    )
                    .record_stage(
                        "failed",
                        format!("symbol constraints unavailable for {}", symbol.as_str()),
                        serde_json::json!({
                            "request": {
                                "symbol": symbol.as_str(),
                            },
                            "error": error.to_string(),
                        }),
                    );
            }
        }

        Ok(())
    }
}
