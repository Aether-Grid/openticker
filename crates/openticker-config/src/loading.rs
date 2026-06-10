use crate::error::ConfigError;
use crate::model::ConfigBundle;
use crate::sources::load_sources_from_dir;
use std::path::{Path, PathBuf};

pub(crate) const GLOBAL_CONFIG_FILE: &str = "openticker.toml";

/// Loads all runtime configuration files from `config_dir` and validates the resulting bundle.
///
/// # Errors
///
/// Returns [`ConfigError`] when required files cannot be read, TOML cannot be parsed,
/// or validation fails.
pub fn load_from_dir(config_dir: impl AsRef<Path>) -> Result<ConfigBundle, ConfigError> {
    let config_dir = config_dir.as_ref();
    load_dotenv(config_dir);

    let sources = load_sources_from_dir(config_dir)?;
    let bundle = sources.to_bundle();
    bundle.validate()?;

    Ok(bundle)
}

pub(crate) fn resolve_config_dir(config_dir: &Path, configured_dir: &Path) -> PathBuf {
    if configured_dir.is_absolute() {
        return configured_dir.to_path_buf();
    }

    let config_relative = config_dir.join(configured_dir);
    if config_relative.exists() {
        return config_relative;
    }

    if let Some(parent) = config_dir.parent() {
        let parent_relative = parent.join(configured_dir);
        if parent_relative.exists() || configured_dir.components().count() > 1 {
            return parent_relative;
        }
    }

    config_relative
}

pub(crate) fn load_dotenv(config_dir: &Path) {
    let env_in_config_dir = config_dir.join(".env");
    if env_in_config_dir.is_file() {
        let _ = dotenvy::from_path(&env_in_config_dir);
        return;
    }

    if let Some(parent) = config_dir.parent() {
        let env_in_project_root = parent.join(".env");
        if env_in_project_root.is_file() {
            let _ = dotenvy::from_path(&env_in_project_root);
            return;
        }
    }

    let _ = dotenvy::dotenv();
}
