use crate::error::ConfigError;
use crate::model::ConfigBundle;
use crate::sources::load_sources_from_dir;
use std::path::{Path, PathBuf};

pub(crate) const GLOBAL_CONFIG_FILE: &str = "openticker.toml";

/// Loads all runtime configuration files from `config_dir` and validates the resulting bundle.
///
/// # Errors
///
/// Returns [`ConfigError`] when an existing `.env` file cannot be read/parsed,
/// when required files cannot be read, when TOML cannot be parsed, or when
/// validation fails.
pub fn load_from_dir(config_dir: impl AsRef<Path>) -> Result<ConfigBundle, ConfigError> {
    let config_dir = config_dir.as_ref();
    load_dotenv(config_dir)?;

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

/// Loads environment variables from a `.env` file if one is present.
///
/// Resolution order: `<config_dir>/.env`, then `<config_dir>/../.env`, then a
/// directory walk via [`dotenvy::dotenv`]. A `.env` file is optional, so a
/// missing file (`NotFound`) is silently ignored. Any other failure (e.g. a
/// permission error or a malformed line) is logged via `tracing::warn!` and
/// propagated, because silently leaving secrets unset would hide the cause of
/// downstream "required secret env var ... is not set" failures.
///
/// # Errors
///
/// Returns [`ConfigError::Dotenv`] when a `.env` file exists but cannot be read
/// or parsed.
pub(crate) fn load_dotenv(config_dir: &Path) -> Result<(), ConfigError> {
    let env_in_config_dir = config_dir.join(".env");
    if env_in_config_dir.is_file() {
        return classify_dotenv_result(&env_in_config_dir, dotenvy::from_path(&env_in_config_dir));
    }

    if let Some(parent) = config_dir.parent() {
        let env_in_project_root = parent.join(".env");
        if env_in_project_root.is_file() {
            return classify_dotenv_result(
                &env_in_project_root,
                dotenvy::from_path(&env_in_project_root),
            );
        }
    }

    // No explicit `.env` next to the config; fall back to a directory walk.
    // `dotenvy::dotenv` resolves the path it actually used, so report that on
    // failure; a `NotFound` here simply means no `.env` exists anywhere.
    match dotenvy::dotenv() {
        Ok(_) => Ok(()),
        Err(err) if err.not_found() => Ok(()),
        Err(err) => {
            tracing::warn!(error = %err, "failed to load `.env` discovered via directory walk");
            Err(ConfigError::Dotenv {
                path: PathBuf::from(".env"),
                source: err,
            })
        }
    }
}

/// Classifies the outcome of loading a specific `.env` file.
///
/// A `NotFound` error is benign (the file is optional / may have been removed
/// between the `is_file` check and the read). Any other error is logged and
/// converted into a [`ConfigError::Dotenv`] so the caller can surface it.
pub(crate) fn classify_dotenv_result(
    path: &Path,
    result: Result<(), dotenvy::Error>,
) -> Result<(), ConfigError> {
    match result {
        Ok(()) => Ok(()),
        Err(err) if err.not_found() => Ok(()),
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "failed to load `.env` file");
            Err(ConfigError::Dotenv {
                path: path.to_path_buf(),
                source: err,
            })
        }
    }
}
