use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse TOML `{path}`: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("configuration validation failed: {message}")]
    Validation { message: String },
    #[error("failed to render TOML: {message}")]
    Render { message: String },
    #[error("failed to load environment file `{path}`: {source}")]
    Dotenv {
        path: PathBuf,
        #[source]
        source: dotenvy::Error,
    },
}

impl ConfigError {
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }

    pub(crate) fn render(message: impl Into<String>) -> Self {
        Self::Render {
            message: message.into(),
        }
    }
}
