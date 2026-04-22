use crate::{InventoryError, ServiceError};

pub(crate) use openticker_lane::{
    apply_process_bar_fill_state, effective_position_quantity, ledger_owner_path,
    sync_inventory_from_runtime_fields, sync_remote_position_quantity,
};

pub(crate) fn inventory_transition_error(
    instance_id: &str,
    action: &str,
    error: InventoryError,
) -> ServiceError {
    ServiceError::InvalidConfiguration(format!(
        "instance `{instance_id}` inventory transition `{action}` failed: {error:?}"
    ))
}
