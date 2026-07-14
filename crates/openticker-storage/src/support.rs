use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn lock_mutex<'a, T>(mutex: &'a Mutex<T>, label: &'static str) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            // Workspace policy: recover from poisoning and log instead of
            // failing forever. A poisoned mutex only means another thread
            // panicked while holding the guard; the protected state remains
            // usable (a SQLite connection stays consistent after a Rust-side
            // panic — at worst an open transaction was rolled back).
            mutex.clear_poison();
            tracing::warn!("{label} mutex was poisoned; clearing poison and recovering the lock");
            poisoned.into_inner()
        }
    }
}

/// Last timestamp handed out by [`now_timestamp_ms`], used as a monotonic floor.
static LAST_TIMESTAMP_MS: AtomicI64 = AtomicI64::new(0);

/// Applies a monotonic floor to a raw wall-clock reading.
///
/// `raw` is the current wall-clock timestamp in milliseconds, or `None` when
/// the system clock is unavailable (e.g. it reports a time before the UNIX
/// epoch). The returned value never decreases across calls sharing the same
/// `floor` cell: a regressed or unavailable clock yields the last known good
/// timestamp instead of going backwards or collapsing to zero. Note that if
/// the clock has never been readable since process start, the floor is still 0
/// and that initial zero value will be returned.
pub(crate) fn monotonic_timestamp_ms(floor: &AtomicI64, raw: Option<i64>) -> i64 {
    match raw {
        Some(raw) => floor.fetch_max(raw, Ordering::Relaxed).max(raw),
        None => floor.load(Ordering::Relaxed),
    }
}

/// Returns the current wall-clock time in milliseconds since the UNIX epoch,
/// clamped to never go backwards within this process.
///
/// Granularity note: this feeds the `created_at_ms` fields on journal records,
/// which therefore cannot distinguish records created within the same
/// millisecond. Where exact insertion order matters, the autoincrement `id`
/// column is the tiebreaker.
pub(crate) fn now_timestamp_ms() -> i64 {
    let raw = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX));
    monotonic_timestamp_ms(&LAST_TIMESTAMP_MS, raw)
}
