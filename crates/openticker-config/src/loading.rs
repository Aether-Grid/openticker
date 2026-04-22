use crate::error::ConfigError;
use crate::model::{AccountConfig, ConfigBundle, GlobalConfig, InstanceConfig, RiskProfileConfig};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const GLOBAL_CONFIG_FILE: &str = "openticker.toml";

/// Loads all runtime configuration files from `config_dir` and validates the resulting bundle.
///
/// # Errors
///
/// Returns [`ConfigError`] when required files cannot be read, TOML cannot be parsed,
/// or validation fails.
pub fn load_from_dir(config_dir: impl AsRef<Path>) -> Result<ConfigBundle, ConfigError> {
    let config_dir = config_dir.as_ref();
    load_dotenv(config_dir);

    let global_path = config_dir.join(GLOBAL_CONFIG_FILE);
    let global: GlobalConfig = read_toml_file(&global_path)?;

    let accounts_dir = config_dir.join("accounts");
    let risk_dir = config_dir.join("risk");
    let bots_dir = resolve_config_dir(config_dir, &global.service.bot_dir);

    let accounts: Vec<AccountConfig> = read_toml_dir(&accounts_dir)?;
    let risk_profiles: Vec<RiskProfileConfig> = read_toml_dir(&risk_dir)?;
    let instances: Vec<InstanceConfig> = read_toml_dir(&bots_dir)?;

    let bundle = ConfigBundle {
        global,
        accounts,
        risk_profiles,
        instances,
    };
    bundle.validate()?;

    Ok(bundle)
}

fn resolve_config_dir(config_dir: &Path, configured_dir: &Path) -> PathBuf {
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

fn load_dotenv(config_dir: &Path) {
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

fn read_toml_file<T>(path: &Path) -> Result<T, ConfigError>
where
    T: for<'de> Deserialize<'de>,
{
    let raw = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&raw).map_err(|source| ConfigError::Toml {
        path: path.to_path_buf(),
        source,
    })
}

fn read_toml_dir<T>(directory: &Path) -> Result<Vec<T>, ConfigError>
where
    T: for<'de> Deserialize<'de>,
{
    let mut entries = fs::read_dir(directory)
        .map_err(|source| ConfigError::Io {
            path: directory.to_path_buf(),
            source,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| ConfigError::Io {
            path: directory.to_path_buf(),
            source,
        })?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect::<Vec<_>>();

    entries.sort();

    entries
        .into_iter()
        .map(|path| read_toml_file::<T>(&path))
        .collect()
}
