//! Validators for global config sections: storage backend and data plane.

use crate::error::ConfigError;
use crate::model::{DataPlaneConfig, StorageConfig};
use std::collections::HashSet;

pub(super) fn validate_storage(storage: &StorageConfig) -> Result<(), ConfigError> {
    if storage.kind != "sqlite" {
        return Err(ConfigError::validation(format!(
            "unsupported storage kind `{}`; only `sqlite` is currently supported",
            storage.kind
        )));
    }

    if storage.path.as_os_str().is_empty() {
        return Err(ConfigError::validation(
            "storage path cannot be empty for sqlite backend",
        ));
    }

    if storage.busy_timeout_ms == 0 {
        return Err(ConfigError::validation(
            "storage busy_timeout_ms must be greater than zero",
        ));
    }

    Ok(())
}

pub(super) fn validate_data_plane(
    data_plane: &DataPlaneConfig,
    account_ids: &HashSet<&str>,
) -> Result<(), ConfigError> {
    if data_plane.default_polling_interval_ms < 1_000 {
        return Err(ConfigError::validation(format!(
            "`data_plane.default_polling_interval_ms` must be at least 1000, got {}",
            data_plane.default_polling_interval_ms
        )));
    }

    if data_plane.default_retention < 10 {
        return Err(ConfigError::validation(format!(
            "`data_plane.default_retention` must be at least 10, got {}",
            data_plane.default_retention
        )));
    }

    for watch in &data_plane.watchlist {
        if !account_ids.contains(watch.account.as_str()) {
            return Err(ConfigError::validation(format!(
                "data-plane watchlist entry references unknown account `{}`",
                watch.account
            )));
        }

        let polling_interval_ms = watch
            .polling_interval_ms
            .unwrap_or(data_plane.default_polling_interval_ms);
        if polling_interval_ms < 1_000 {
            return Err(ConfigError::validation(format!(
                "data-plane watchlist entry `{}/{}` has invalid `polling_interval_ms` `{polling_interval_ms}`",
                watch.account, watch.symbol
            )));
        }

        let retention = watch.retention.unwrap_or(data_plane.default_retention);
        if retention < 10 {
            return Err(ConfigError::validation(format!(
                "data-plane watchlist entry `{}/{}` has invalid `retention` `{retention}`",
                watch.account, watch.symbol
            )));
        }
    }

    Ok(())
}
