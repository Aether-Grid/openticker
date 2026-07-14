use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to create storage directory `{path}`: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("storage backend `{kind}` is not supported")]
    UnsupportedBackend { kind: String },
    #[error(
        "sqlite schema version {actual} is incompatible with expected version {expected}; recreate the database"
    )]
    IncompatibleSchemaVersion { actual: i64, expected: i64 },
    #[error("internal storage error: {0}")]
    Internal(String),
    #[error("sqlite migration {version} `{name}` failed: {source}")]
    Migration {
        version: i64,
        name: &'static str,
        #[source]
        source: rusqlite::Error,
    },
    #[error("sqlite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}
