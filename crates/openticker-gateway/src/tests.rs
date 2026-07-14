use crate::{
    Gateway, GatewayError, build_connector_registry, execution_constraints_are_complete,
    normalize_symbol_constraints, resolve_effective_execution_constraints,
};
use openticker_config::{AccountConfig, ExecutionConstraintsConfig};
use openticker_connectors::ConnectorSymbolConstraints;
use openticker_core::ExecutionMode;
use std::sync::{Arc, Mutex};

fn account(kind: &str) -> AccountConfig {
    AccountConfig {
        id: "paper-account".to_owned(),
        kind: kind.to_owned(),
        mode: ExecutionMode::Paper,
        api_key_env: None,
        api_secret_env: None,
        passphrase_env: None,
        use_demo_mode: false,
        reconciliation_remote_snapshot: false,
        execution_remote_submission: None,
        reconciliation_base_url: None,
        cash_balance_assets: Vec::new(),
        total_budget_usd: 10_000.0,
    }
}

#[test]
fn normalize_symbol_constraints_filters_non_positive_values() {
    let normalized = normalize_symbol_constraints(&ConnectorSymbolConstraints {
        fractional_entry_supported: Some(true),
        quantity_step: Some(0.0),
        min_quantity: Some(-1.0),
        min_notional_usd: Some(5.0),
        source: Some("connector".to_owned()),
    });

    assert_eq!(normalized.execution_constraints.quantity_step, None);
    assert_eq!(normalized.execution_constraints.min_quantity, None);
    assert_eq!(normalized.execution_constraints.min_notional_usd, Some(5.0));
    assert_eq!(normalized.fractional_entry_supported, Some(true));
    assert_eq!(normalized.source.as_deref(), Some("connector"));
    assert!(normalized.has_numeric_constraints());
}

#[test]
fn resolve_effective_execution_constraints_prefers_instance_over_connector_values() {
    let configured = ExecutionConstraintsConfig {
        quantity_step: Some(0.01),
        min_quantity: None,
        min_notional_usd: Some(15.0),
    };
    let connector = ExecutionConstraintsConfig {
        quantity_step: Some(0.1),
        min_quantity: Some(0.001),
        min_notional_usd: Some(5.0),
    };

    let resolved = resolve_effective_execution_constraints(&configured, Some(&connector));
    assert_eq!(resolved.quantity_step, Some(0.01));
    assert_eq!(resolved.min_quantity, Some(0.001));
    assert_eq!(resolved.min_notional_usd, Some(15.0));
}

#[test]
fn execution_constraints_are_complete_requires_all_numeric_fields() {
    assert!(!execution_constraints_are_complete(
        &ExecutionConstraintsConfig {
            quantity_step: Some(0.01),
            min_quantity: Some(1.0),
            min_notional_usd: None,
        }
    ));
    assert!(execution_constraints_are_complete(
        &ExecutionConstraintsConfig {
            quantity_step: Some(0.01),
            min_quantity: Some(1.0),
            min_notional_usd: Some(5.0),
        }
    ));
}

#[test]
fn build_connector_registry_accepts_supported_account_kinds() {
    let registry = build_connector_registry(&[account("alpaca")]).unwrap();
    let gateway = Gateway::new(Arc::new(Mutex::new(registry)));

    let statuses = gateway.statuses().unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].account_id, "paper-account");
    assert_eq!(statuses[0].kind.as_str(), "alpaca");
}

#[test]
fn build_connector_registry_rejects_unsupported_account_kinds() {
    let error = build_connector_registry(&[account("not-a-real-connector")]).unwrap_err();

    assert!(matches!(
        error,
        GatewayError::UnsupportedConnectorKind {
            account_id,
            kind,
        } if account_id == "paper-account" && kind == "not-a-real-connector"
    ));
}
