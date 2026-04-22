use crate::{
    ConnectorAccountStatus, ConnectorRuntimeStatus, execution_mode_is_live, mode_banner_text,
};

pub(crate) fn connector_runtime_status_from_account_status(
    status: ConnectorAccountStatus,
) -> ConnectorRuntimeStatus {
    let live_mode_active = execution_mode_is_live(status.mode);
    ConnectorRuntimeStatus {
        account_id: status.account_id,
        kind: status.kind.as_str().to_owned(),
        mode: status.mode,
        live_mode_active,
        mode_banner: mode_banner_text(live_mode_active).to_owned(),
        state: status.state,
        message: status.message,
        resilience_policy: status.resilience_policy,
        resilience_state: status.resilience_state,
    }
}

pub(crate) fn connector_resilience_window_active(status: &ConnectorRuntimeStatus) -> bool {
    status.resilience_state.next_reconnect_at_ms.is_some()
        || status.resilience_state.throttled_until_ms.is_some()
}
