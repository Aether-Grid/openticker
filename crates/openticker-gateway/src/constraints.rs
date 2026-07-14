use openticker_config::ExecutionConstraintsConfig;
use openticker_connectors::ConnectorSymbolConstraints;

#[derive(Debug, Clone, Default)]
pub struct NormalizedSymbolConstraints {
    pub execution_constraints: ExecutionConstraintsConfig,
    pub fractional_entry_supported: Option<bool>,
    pub source: Option<String>,
}

impl NormalizedSymbolConstraints {
    #[must_use]
    pub fn has_numeric_constraints(&self) -> bool {
        self.execution_constraints.quantity_step.is_some()
            || self.execution_constraints.min_quantity.is_some()
            || self.execution_constraints.min_notional_usd.is_some()
    }
}

#[must_use]
pub fn execution_constraints_are_complete(constraints: &ExecutionConstraintsConfig) -> bool {
    constraints.quantity_step.is_some()
        && constraints.min_quantity.is_some()
        && constraints.min_notional_usd.is_some()
}

#[must_use]
pub fn resolve_effective_execution_constraints(
    configured_constraints: &ExecutionConstraintsConfig,
    connector_constraints: Option<&ExecutionConstraintsConfig>,
) -> ExecutionConstraintsConfig {
    ExecutionConstraintsConfig {
        quantity_step: configured_constraints
            .quantity_step
            .or_else(|| connector_constraints.and_then(|constraints| constraints.quantity_step)),
        min_quantity: configured_constraints
            .min_quantity
            .or_else(|| connector_constraints.and_then(|constraints| constraints.min_quantity)),
        min_notional_usd: configured_constraints
            .min_notional_usd
            .or_else(|| connector_constraints.and_then(|constraints| constraints.min_notional_usd)),
    }
}

#[must_use]
pub fn normalize_symbol_constraints(
    connector_constraints: &ConnectorSymbolConstraints,
) -> NormalizedSymbolConstraints {
    NormalizedSymbolConstraints {
        execution_constraints: ExecutionConstraintsConfig {
            quantity_step: sanitize_positive_constraint(connector_constraints.quantity_step),
            min_quantity: sanitize_positive_constraint(connector_constraints.min_quantity),
            min_notional_usd: sanitize_positive_constraint(connector_constraints.min_notional_usd),
        },
        fractional_entry_supported: connector_constraints.fractional_entry_supported,
        source: connector_constraints.source.clone(),
    }
}

fn sanitize_positive_constraint(value: Option<f64>) -> Option<f64> {
    value.filter(|candidate| candidate.is_finite() && *candidate > 0.0)
}
