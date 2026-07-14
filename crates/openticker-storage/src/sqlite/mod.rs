pub(crate) mod migrations;

mod journal;

use crate::StorageError;
use crate::support::lock_mutex;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

#[derive(Debug)]
pub struct SqliteRuntimeJournal {
    path: PathBuf,
    pub(crate) write_connection: Mutex<Connection>,
    pub(crate) read_connections: Vec<Mutex<Connection>>,
    next_read_connection: AtomicUsize,
}

const SQLITE_DEFAULT_READ_POOL_SIZE: usize = 4;
const SQLITE_MIN_READ_POOL_SIZE: usize = 2;
const SQLITE_MAX_READ_POOL_SIZE: usize = 8;

impl SqliteRuntimeJournal {
    /// Opens a `SQLite` runtime journal and initializes required schema objects.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the parent directory cannot be created,
    /// the database cannot be opened, or schema initialization fails.
    pub fn open(path: impl AsRef<Path>, busy_timeout_ms: u64) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| StorageError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let busy_timeout = Duration::from_millis(busy_timeout_ms);
        let write_connection = Self::open_write_connection(&path, busy_timeout)?;
        let read_pool_size = Self::read_pool_size();
        let mut read_connections = Vec::with_capacity(read_pool_size);
        for _ in 0..read_pool_size {
            read_connections.push(Mutex::new(Self::open_read_connection(&path, busy_timeout)?));
        }

        Ok(Self {
            path,
            write_connection: Mutex::new(write_connection),
            read_connections,
            next_read_connection: AtomicUsize::new(0),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn open_write_connection(
        path: &Path,
        busy_timeout: Duration,
    ) -> Result<Connection, StorageError> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(busy_timeout)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        // FULL (not NORMAL) on the write connection: with WAL + NORMAL, a
        // power loss or OS crash can drop the most recent committed
        // transactions. This journal persists order/fill records for a
        // trading system, where losing acknowledged writes is worse than the
        // extra fsync cost.
        connection.pragma_update(None, "synchronous", "FULL")?;
        migrations::run(&mut connection)?;
        Ok(connection)
    }

    fn open_read_connection(
        path: &Path,
        busy_timeout: Duration,
    ) -> Result<Connection, StorageError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(busy_timeout)?;
        Ok(connection)
    }

    fn read_pool_size() -> usize {
        std::thread::available_parallelism().map_or(SQLITE_DEFAULT_READ_POOL_SIZE, |parallelism| {
            parallelism
                .get()
                .clamp(SQLITE_MIN_READ_POOL_SIZE, SQLITE_MAX_READ_POOL_SIZE)
        })
    }

    fn with_write_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, StorageError> {
        let connection = lock_mutex(&self.write_connection, "sqlite_write_connection");
        operation(&connection).map_err(StorageError::from)
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, StorageError> {
        let read_pool_len = self.read_connections.len();
        if read_pool_len == 0 {
            return self.with_write_connection(operation);
        }

        let start = self
            .next_read_connection
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        for offset in 0..read_pool_len {
            let index = (start + offset) % read_pool_len;
            // `try_lock` treats a poisoned connection the same as busy; it
            // gets skipped until the fallback slot's `lock_mutex` heals it.
            if let Ok(connection) = self.read_connections[index].try_lock() {
                return operation(&connection).map_err(StorageError::from);
            }
        }

        let fallback_index = start % read_pool_len;
        let connection = lock_mutex(
            &self.read_connections[fallback_index],
            "sqlite_read_connection",
        );
        operation(&connection).map_err(StorageError::from)
    }
}
