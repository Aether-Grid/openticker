use crate::error::ConfigError;
use crate::loading::{GLOBAL_CONFIG_FILE, resolve_config_dir};
use crate::model::{AccountConfig, ConfigBundle, GlobalConfig, InstanceConfig, RiskProfileConfig};
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};

/// A parsed configuration file together with its on-disk location and raw TOML text.
#[derive(Debug, Clone)]
pub struct SourceFile<T> {
    pub path: PathBuf,
    pub raw: String,
    pub value: T,
}

/// The full set of configuration files backing a [`ConfigBundle`], keeping each
/// parsed entity tied to the file it came from.
///
/// Entities are located by their parsed `id`, not by filename — filenames only
/// conventionally match ids.
#[derive(Debug, Clone)]
pub struct ConfigSources {
    pub config_dir: PathBuf,
    pub accounts_dir: PathBuf,
    pub risk_dir: PathBuf,
    pub bots_dir: PathBuf,
    pub global: SourceFile<GlobalConfig>,
    pub accounts: Vec<SourceFile<AccountConfig>>,
    pub risk_profiles: Vec<SourceFile<RiskProfileConfig>>,
    pub instances: Vec<SourceFile<InstanceConfig>>,
}

/// Loads every configuration file under `dir` without validating the result.
///
/// Files within each directory are read in sorted path order, matching
/// [`load_from_dir`](crate::load_from_dir).
///
/// # Errors
///
/// Returns [`ConfigError`] when required files cannot be read or TOML cannot be parsed.
pub fn load_sources_from_dir(dir: impl AsRef<Path>) -> Result<ConfigSources, ConfigError> {
    let config_dir = dir.as_ref();

    let global: SourceFile<GlobalConfig> = read_source_file(&config_dir.join(GLOBAL_CONFIG_FILE))?;

    let accounts_dir = config_dir.join("accounts");
    let risk_dir = config_dir.join("risk");
    let bots_dir = resolve_config_dir(config_dir, &global.value.service.bot_dir);

    let accounts = read_source_dir(&accounts_dir)?;
    let risk_profiles = read_source_dir(&risk_dir)?;
    let instances = read_source_dir(&bots_dir)?;

    Ok(ConfigSources {
        config_dir: config_dir.to_path_buf(),
        accounts_dir,
        risk_dir,
        bots_dir,
        global,
        accounts,
        risk_profiles,
        instances,
    })
}

impl ConfigSources {
    /// Clones the parsed values into a [`ConfigBundle`] without validating it.
    #[must_use]
    pub fn to_bundle(&self) -> ConfigBundle {
        ConfigBundle {
            global: self.global.value.clone(),
            accounts: self
                .accounts
                .iter()
                .map(|file| file.value.clone())
                .collect(),
            risk_profiles: self
                .risk_profiles
                .iter()
                .map(|file| file.value.clone())
                .collect(),
            instances: self
                .instances
                .iter()
                .map(|file| file.value.clone())
                .collect(),
        }
    }

    /// Looks up a bot instance source by its parsed `id`.
    ///
    /// Ids are not guaranteed unique until the bundle validates; this returns
    /// the first match in sorted path order.
    #[must_use]
    pub fn instance_by_id(&self, id: &str) -> Option<&SourceFile<InstanceConfig>> {
        self.instances.iter().find(|file| file.value.id == id)
    }

    /// Looks up an account source by its parsed `id`.
    ///
    /// Ids are not guaranteed unique until the bundle validates; this returns
    /// the first match in sorted path order.
    #[must_use]
    pub fn account_by_id(&self, id: &str) -> Option<&SourceFile<AccountConfig>> {
        self.accounts.iter().find(|file| file.value.id == id)
    }

    /// Looks up a risk profile source by its parsed `id`.
    ///
    /// Ids are not guaranteed unique until the bundle validates; this returns
    /// the first match in sorted path order.
    #[must_use]
    pub fn risk_profile_by_id(&self, id: &str) -> Option<&SourceFile<RiskProfileConfig>> {
        self.risk_profiles.iter().find(|file| file.value.id == id)
    }
}

fn read_source_file<T>(path: &Path) -> Result<SourceFile<T>, ConfigError>
where
    T: DeserializeOwned,
{
    let raw = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let value = toml::from_str(&raw).map_err(|source| ConfigError::Toml {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(SourceFile {
        path: path.to_path_buf(),
        raw,
        value,
    })
}

fn read_source_dir<T>(directory: &Path) -> Result<Vec<SourceFile<T>>, ConfigError>
where
    T: DeserializeOwned,
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
        .map(|path| read_source_file::<T>(&path))
        .collect()
}
