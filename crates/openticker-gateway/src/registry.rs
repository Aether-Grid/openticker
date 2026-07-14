use crate::error::GatewayError;
use openticker_config::AccountConfig;
use openticker_connectors::{ConnectorKind, ConnectorRegistry};

/// Builds a connector registry from validated runtime account config.
///
/// # Errors
///
/// Returns [`GatewayError::UnsupportedConnectorKind`] when an account refers to
/// an unknown connector kind, or connector-specific initialization failures
/// when the registry cannot be created.
pub fn build_connector_registry(
    accounts: &[AccountConfig],
) -> Result<ConnectorRegistry, GatewayError> {
    let connector_accounts = accounts
        .iter()
        .map(|account| {
            let kind = ConnectorKind::parse(account.kind.as_str()).ok_or_else(|| {
                GatewayError::UnsupportedConnectorKind {
                    account_id: account.id.clone(),
                    kind: account.kind.clone(),
                }
            })?;

            Ok(openticker_connectors::ConnectorAccount {
                account_id: account.id.clone(),
                kind,
                mode: account.mode,
                use_demo_mode: account.use_demo_mode,
                api_key_env: account.api_key_env.clone(),
                api_secret_env: account.api_secret_env.clone(),
                passphrase_env: account.passphrase_env.clone(),
                reconciliation_remote_snapshot: account.reconciliation_remote_snapshot,
                execution_remote_submission: account.execution_remote_submission_enabled(),
                reconciliation_base_url: account.reconciliation_base_url.clone(),
            })
        })
        .collect::<Result<Vec<_>, GatewayError>>()?;

    ConnectorRegistry::from_accounts(connector_accounts).map_err(Into::into)
}
