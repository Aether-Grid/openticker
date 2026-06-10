//! Config-directory file watcher that auto-reloads the runtime.
//!
//! Watches the managed config directory (and the bots directory when it is not
//! nested inside it) for `.toml` mutations and drives
//! [`crate::config_ops::reload_from_disk`] after a debounce window. The reload
//! pipeline owns no-op detection (self-write suppression), change-set
//! validation, the runtime swap, and reload-status bookkeeping; this module is
//! only responsible for turning filesystem events into debounced reload
//! triggers without ever letting a reload error tear the loop down.
//!
//! The debounce constants live here rather than in `constants.rs` because they
//! are an implementation detail of this watcher only and keeping them local
//! avoids editing a file shared with unrelated in-flight work.

use crate::config_ops::{ConfigApplyError, ReloadTrigger, reload_from_disk};
use crate::state::HttpState;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

/// Quiet period that must elapse with no further relevant event before a
/// coalesced batch of filesystem changes triggers a reload.
const CONFIG_WATCH_DEBOUNCE_MS: u64 = 500;
/// Hard upper bound on how long a continuous stream of events may defer a
/// reload. Guarantees a long-running batch (e.g. `git checkout`) still fires
/// within this window of its first event.
const CONFIG_WATCH_MAX_COALESCE_MS: u64 = 5_000;
/// Backoff applied before the watcher is recreated after the notify backend
/// errors, so a persistently failing backend cannot busy-spin.
const CONFIG_WATCH_RECREATE_BACKOFF_MS: u64 = 5_000;

/// Background watcher that auto-reloads the runtime when the managed config
/// directory changes on disk.
pub(crate) struct ConfigWatcher {
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl ConfigWatcher {
    /// Spawns the watcher loop. Returns `None` when there is no managed config
    /// directory (`config_dir` would be the service's own working config).
    ///
    /// `bots_dir`, when present and not nested inside `config_dir`, is watched
    /// in addition so bot definitions stored outside the config tree are also
    /// observed.
    // The `Option` return is part of the documented contract: callers chain
    // `state.config_dir.clone().and_then(ConfigWatcher::start)`, so the absence
    // of a managed directory short-circuits to `None` at the call site. The
    // type is therefore deliberately `Option<Self>` even though this body
    // always succeeds once a directory is supplied.
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn start(
        state: HttpState,
        config_dir: PathBuf,
        bots_dir: Option<PathBuf>,
    ) -> Option<Self> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(run_watch_loop(state, config_dir, bots_dir, shutdown_rx));
        Some(Self { shutdown_tx, task })
    }

    /// Signals the loop to stop and waits for it to drain. Dropping the loop's
    /// owned [`RecommendedWatcher`] releases the OS watch.
    pub(crate) async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.task.await;
    }
}

/// Owns the notify watcher for exactly the lifetime of the loop and bridges its
/// thread-based callback into an async mpsc channel.
async fn run_watch_loop(
    state: HttpState,
    config_dir: PathBuf,
    bots_dir: Option<PathBuf>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    info!(
        config_dir = %config_dir.display(),
        "starting config-directory file watcher"
    );

    loop {
        if *shutdown_rx.borrow() {
            break;
        }

        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();
        // The watcher is held in this scope so the OS watch lives exactly as
        // long as the inner loop; recreating it on error drops the previous
        // one first.
        let _watcher = match build_watcher(&config_dir, bots_dir.as_deref(), event_tx) {
            Ok(watcher) => watcher,
            Err(error) => {
                error!(
                    error = %error,
                    backoff_ms = CONFIG_WATCH_RECREATE_BACKOFF_MS,
                    "failed to create config file watcher; retrying after backoff"
                );
                if sleep_or_shutdown(&mut shutdown_rx, CONFIG_WATCH_RECREATE_BACKOFF_MS).await {
                    break;
                }
                continue;
            }
        };

        // Inner loop: drains debounced events until either shutdown is
        // requested or the notify channel closes (which forces a recreate).
        let channel_closed = drain_events(&state, &mut event_rx, &mut shutdown_rx).await;
        if *shutdown_rx.borrow() {
            break;
        }
        if channel_closed {
            error!(
                backoff_ms = CONFIG_WATCH_RECREATE_BACKOFF_MS,
                "config file watcher channel closed; recreating watcher after backoff"
            );
            if sleep_or_shutdown(&mut shutdown_rx, CONFIG_WATCH_RECREATE_BACKOFF_MS).await {
                break;
            }
        }
    }

    info!("config-directory file watcher stopped");
}

/// Builds a [`RecommendedWatcher`] that forwards events into `event_tx` and
/// registers recursive watches for the config directory and, when distinct,
/// the bots directory.
fn build_watcher(
    config_dir: &Path,
    bots_dir: Option<&Path>,
    event_tx: mpsc::UnboundedSender<Event>,
) -> notify::Result<RecommendedWatcher> {
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<Event>| match result {
            // `send` on an unbounded channel is sync and lock-free, so it is
            // safe to call from notify's own callback thread. A closed receiver
            // means the loop has moved on; dropping the event is fine.
            Ok(event) => {
                let _ = event_tx.send(event);
            }
            Err(error) => {
                warn!(error = %error, "config file watcher reported an event error");
            }
        },
        notify::Config::default(),
    )?;

    watcher.watch(config_dir, RecursiveMode::Recursive)?;

    if let Some(bots_dir) = bots_dir
        && !path_is_inside(bots_dir, config_dir)
    {
        watcher.watch(bots_dir, RecursiveMode::Recursive)?;
    }

    Ok(watcher)
}

/// Receives events until shutdown or channel close. On each relevant event it
/// debounces to quiescence, then triggers a reload. Returns `true` when the
/// notify channel closed (the caller should recreate the watcher), `false` when
/// shutdown was requested.
async fn drain_events(
    state: &HttpState,
    event_rx: &mut mpsc::UnboundedReceiver<Event>,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> bool {
    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    return false;
                }
            }
            received = event_rx.recv() => {
                match received {
                    Some(event) => {
                        if !is_relevant_event(&event) {
                            continue;
                        }
                        // A relevant event arrived: wait for the directory to go
                        // quiet, coalescing any follow-up events into this one
                        // reload.
                        if debounce_to_quiescence(event_rx, shutdown_rx).await {
                            // Shutdown requested mid-debounce.
                            return false;
                        }
                        trigger_reload(state).await;
                    }
                    None => return true,
                }
            }
        }
    }
}

/// Waits until `CONFIG_WATCH_DEBOUNCE_MS` passes with no further relevant event,
/// or until the hard `CONFIG_WATCH_MAX_COALESCE_MS` cap elapses since the first
/// event of this batch. Returns `true` if shutdown was requested while waiting.
async fn debounce_to_quiescence(
    event_rx: &mut mpsc::UnboundedReceiver<Event>,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> bool {
    let batch_started_at = Instant::now();
    let coalesce_deadline = batch_started_at + Duration::from_millis(CONFIG_WATCH_MAX_COALESCE_MS);

    loop {
        let quiet_deadline = Instant::now() + Duration::from_millis(CONFIG_WATCH_DEBOUNCE_MS);
        // Fire when the quiet window elapses, but never defer past the hard cap.
        let fire_at = quiet_deadline.min(coalesce_deadline);

        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    return true;
                }
            }
            () = tokio::time::sleep_until(fire_at) => {
                return false;
            }
            received = event_rx.recv() => {
                match received {
                    // Reset the quiet window only for relevant events; ignore
                    // noise so it cannot indefinitely defer the reload.
                    Some(event) => {
                        if is_relevant_event(&event) {
                            debug!("coalescing additional config change into pending reload");
                        }
                    }
                    // Channel closed: stop debouncing and let the outer loop
                    // recreate the watcher. Fire what we have first.
                    None => return false,
                }
            }
        }
    }
}

/// Runs one reload attempt, logging the outcome. Reload errors are logged and
/// swallowed so a bad config on disk can never kill the watcher loop.
async fn trigger_reload(state: &HttpState) {
    match reload_from_disk(state, ReloadTrigger::FileWatcher).await {
        Ok(true) => info!("config file watcher applied an updated configuration"),
        Ok(false) => info!("config file watcher observed a change with no effect on the bundle"),
        Err(ConfigApplyError::NotManaged) => {
            // Should not happen: the watcher only starts with a managed dir.
            warn!("config file watcher fired without a managed config directory");
        }
        Err(ConfigApplyError::ChangeSet(violations)) => {
            warn!(
                violations = violations.len(),
                "config file watcher reload rejected by change-set validation"
            );
        }
        Err(ConfigApplyError::Load(error)) => {
            warn!(error = %error, "config file watcher reload failed to load config");
        }
        Err(ConfigApplyError::RuntimeBuild(message)) => {
            error!(error = %message, "config file watcher reload failed to rebuild runtime");
        }
    }
}

/// Sleeps for `millis`, returning `true` early if shutdown is requested while
/// waiting.
async fn sleep_or_shutdown(shutdown_rx: &mut watch::Receiver<bool>, millis: u64) -> bool {
    tokio::select! {
        _ = shutdown_rx.changed() => *shutdown_rx.borrow(),
        () = tokio::time::sleep(Duration::from_millis(millis)) => false,
    }
}

/// True when `event` is a create/modify/remove/rename touching at least one
/// `.toml` path whose file name does not begin with `.` (which skips the
/// atomic-write temp files `.{name}.tmp-{pid}-{seq}` and other dotfiles).
fn is_relevant_event(event: &Event) -> bool {
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return false;
    }
    event.paths.iter().any(|path| is_relevant_path(path))
}

/// True when `path` names a non-dotfile `.toml` file.
fn is_relevant_path(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if file_name.starts_with('.') {
        return false;
    }
    Path::new(file_name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
}

/// Whether `candidate` resolves to the same directory as `base` or a path
/// nested within it. Canonicalizes both sides when possible (so symlinked or
/// `..`-containing paths compare correctly) and falls back to a lexical prefix
/// check when either path does not yet exist on disk.
fn path_is_inside(candidate: &Path, base: &Path) -> bool {
    let canonical_candidate = candidate.canonicalize();
    let canonical_base = base.canonicalize();
    match (canonical_candidate, canonical_base) {
        (Ok(candidate), Ok(base)) => candidate.starts_with(&base),
        _ => candidate.starts_with(base),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::HttpState;
    use notify::event::{CreateKind, ModifyKind};
    use openticker_config::{
        AccountConfig, BudgetConfig, ConfigBundle, DataPlaneConfig, ExecutionConstraintsConfig,
        GlobalConfig, HttpConfig, IndicatorInstanceConfig, InstanceConfig, InstanceRiskConfig,
        ObservabilityConfig, RiskOverrides, RiskProfileConfig, SafetyConfig, ServiceConfig,
        SignalMode, StorageConfig, load_from_dir, render_new_document,
    };
    use openticker_core::{ExecutionMode, IndicatorSignalMetadataFilters, MarketType, Timeframe};
    use openticker_runtime::Runtime;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    // Generous ceiling: macOS FSEvents can lag several seconds before the first
    // callback for a freshly created watch fires.
    const POLL_TIMEOUT: Duration = Duration::from_secs(12);
    const POLL_STEP: Duration = Duration::from_millis(50);

    fn fixture_bundle() -> ConfigBundle {
        ConfigBundle {
            global: GlobalConfig {
                service: ServiceConfig {
                    environment: "test".to_owned(),
                    data_dir: "./var".into(),
                    bot_dir: "./bots".into(),
                },
                http: HttpConfig {
                    enabled: true,
                    bind: "127.0.0.1:8080".to_owned(),
                    request_log: true,
                    openapi_enabled: true,
                    openapi_path: "/openapi.json".to_owned(),
                },
                storage: StorageConfig {
                    kind: "sqlite".to_owned(),
                    path: PathBuf::from("./runtime.db"),
                    busy_timeout_ms: 5_000,
                    prune_removed_bots_on_startup: false,
                },
                observability: ObservabilityConfig {
                    log_level: "info".to_owned(),
                    metrics_enabled: true,
                    metrics_path: "/metrics".to_owned(),
                },
                safety: SafetyConfig {
                    require_explicit_live_enable: true,
                    default_start_paused_if_recovery_uncertain: true,
                },
                data_plane: DataPlaneConfig::default(),
            },
            accounts: vec![AccountConfig {
                id: "alpaca-paper".to_owned(),
                kind: "alpaca".to_owned(),
                mode: ExecutionMode::Paper,
                api_key_env: Some("PATH".to_owned()),
                api_secret_env: Some("PATH".to_owned()),
                passphrase_env: None,
                use_demo_mode: false,
                reconciliation_remote_snapshot: false,
                execution_remote_submission: None,
                reconciliation_base_url: None,
                cash_balance_assets: Vec::new(),
                total_budget_usd: 20_000.0,
            }],
            risk_profiles: vec![RiskProfileConfig {
                id: "equities-default".to_owned(),
                max_daily_loss_pct: 2.0,
                max_open_positions: 2,
                target_order_notional_usd: Some(1_000.0),
                max_order_notional_usd: 1_000.0,
                max_spread_bps: 20,
                max_slippage_bps: 30,
                stale_data_ms: 3_000,
                cooldown_after_reject_ms: 1_000,
            }],
            instances: vec![InstanceConfig {
                id: "aapl".to_owned(),
                enabled: true,
                market: MarketType::Equities,
                symbols: vec!["AAPL".to_owned()],
                timeframe: Timeframe::M1,
                account: "alpaca-paper".to_owned(),
                data_connector: "alpaca".to_owned(),
                execution_connector: "alpaca".to_owned(),
                strategy: "single_indicator_signal".to_owned(),
                signal_mode: SignalMode::ConfirmedOnly,
                polling_enabled: true,
                polling_interval_ms: 1_000,
                indicators: vec![IndicatorInstanceConfig {
                    id: "trend-1".to_owned(),
                    indicator_type: "sma_crossover".to_owned(),
                    enabled: true,
                    role: None,
                    signal_policy: None,
                    weight: None,
                    metadata_filters: IndicatorSignalMetadataFilters::default(),
                    params: toml::Table::new(),
                }],
                execution_constraints: ExecutionConstraintsConfig::default(),
                budget: BudgetConfig { pct: 100.0 },
                risk: InstanceRiskConfig {
                    profile: "equities-default".to_owned(),
                    overrides: RiskOverrides::default(),
                },
                warmup_target_bars: None,
                allow_live: false,
            }],
        }
    }

    /// Removes the materialized config directory on drop.
    struct ConfigDirGuard {
        path: PathBuf,
    }

    impl Drop for ConfigDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// Materializes a managed config directory (openticker.toml + accounts/ +
    /// risk/ + bots/) with storage nested inside so tests stay isolated. This
    /// mirrors the `fixture_config_dir` integration helper, replicated here so
    /// in-crate tests (which cannot see `tests/common`) can reach `pub(crate)`
    /// watcher internals without widening the public API.
    fn fixture_config_dir(prefix: &str) -> (ConfigDirGuard, PathBuf) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("openticker-http-cfgwatch-{prefix}-{timestamp}"));
        std::fs::create_dir_all(dir.join("accounts")).expect("accounts dir");
        std::fs::create_dir_all(dir.join("risk")).expect("risk dir");
        std::fs::create_dir_all(dir.join("bots")).expect("bots dir");

        let mut bundle = fixture_bundle();
        bundle.global.service.bot_dir = "./bots".into();
        bundle.global.storage.path = dir.join("runtime.db");

        write_rendered(&dir.join("openticker.toml"), &bundle.global);
        write_rendered(
            &dir.join("accounts").join("alpaca-paper.toml"),
            &bundle.accounts[0],
        );
        write_rendered(
            &dir.join("risk").join("equities-default.toml"),
            &bundle.risk_profiles[0],
        );
        write_rendered(&dir.join("bots").join("aapl.toml"), &bundle.instances[0]);

        (ConfigDirGuard { path: dir.clone() }, dir)
    }

    fn write_rendered<T: serde::Serialize>(path: &Path, value: &T) {
        let rendered = render_new_document(value).expect("fixture entity renders");
        std::fs::write(path, rendered).expect("fixture file written");
    }

    fn load_state(dir: &Path) -> HttpState {
        let bundle = load_from_dir(dir).expect("fixture config dir loads");
        let runtime = Runtime::from_config_with_storage(&bundle).expect("runtime builds");
        HttpState::with_config(runtime, dir.to_path_buf(), bundle)
    }

    /// Polls `predicate` until it returns true or the timeout elapses.
    async fn wait_until<F>(mut predicate: F) -> bool
    where
        F: FnMut() -> bool,
    {
        let deadline = Instant::now() + POLL_TIMEOUT;
        while Instant::now() < deadline {
            if predicate() {
                return true;
            }
            tokio::time::sleep(POLL_STEP).await;
        }
        predicate()
    }

    async fn last_outcome(state: &HttpState) -> Option<&'static str> {
        state
            .reload_status
            .read()
            .await
            .back()
            .map(|status| status.outcome)
    }

    async fn last_trigger(state: &HttpState) -> Option<&'static str> {
        state
            .reload_status
            .read()
            .await
            .back()
            .map(|status| status.trigger)
    }

    fn generation(state: &HttpState) -> u64 {
        state.reload_generation.load(Ordering::SeqCst)
    }

    /// Pause long enough for the spawned watcher task to register its OS watch
    /// before a test mutates the directory. The backend (`FSEvents` on macOS,
    /// inotify on Linux) only reports events that occur after it is armed, so a
    /// write racing watch registration would be missed.
    async fn allow_watcher_to_arm() {
        tokio::time::sleep(Duration::from_millis(600)).await;
    }

    #[test]
    fn relevant_event_filtering_skips_dotfiles_and_non_toml() {
        let make = |path: &str| Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![PathBuf::from(path)],
            attrs: notify::event::EventAttributes::default(),
        };

        assert!(is_relevant_event(&make("/cfg/bots/aapl.toml")));
        // Atomic-write temp file: leading dot must be skipped.
        assert!(!is_relevant_event(&make("/cfg/bots/.aapl.toml.tmp-42-7")));
        assert!(!is_relevant_event(&make("/cfg/bots/.hidden.toml")));
        assert!(!is_relevant_event(&make("/cfg/runtime.db")));
        // Access-only events are ignored even when the path is relevant.
        assert!(!is_relevant_event(&Event {
            kind: EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![PathBuf::from("/cfg/bots/aapl.toml")],
            attrs: notify::event::EventAttributes::default(),
        }));
        // A create touching a relevant path qualifies.
        assert!(is_relevant_event(&Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("/cfg/bots/new.toml")],
            attrs: notify::event::EventAttributes::default(),
        }));
    }

    #[test]
    fn path_inside_detection_handles_missing_paths() {
        assert!(path_is_inside(Path::new("/cfg/bots"), Path::new("/cfg")));
        assert!(path_is_inside(Path::new("/cfg"), Path::new("/cfg")));
        assert!(!path_is_inside(Path::new("/other/bots"), Path::new("/cfg")));
    }

    #[tokio::test]
    async fn manual_disk_edit_triggers_reload() {
        let (_guard, dir) = fixture_config_dir("disk-edit");
        let state = load_state(&dir);
        let watcher = ConfigWatcher::start(state.clone(), dir.clone(), state.bots_dir.clone())
            .expect("watcher starts");
        allow_watcher_to_arm().await;

        let baseline_generation = generation(&state);

        // Bump the polling interval on the bot file (a reloadable change).
        let bot_path = dir.join("bots").join("aapl.toml");
        let mut instance = fixture_bundle().instances[0].clone();
        instance.polling_interval_ms = 2_500;
        write_rendered(&bot_path, &instance);

        let applied = wait_until(|| generation(&state) > baseline_generation).await;
        assert!(applied, "watcher should have applied the disk edit");
        assert_eq!(last_trigger(&state).await, Some("file_watcher"));
        assert_eq!(last_outcome(&state).await, Some("reloaded"));

        let bundle = state.config_bundle.read().await;
        let interval = bundle
            .as_ref()
            .expect("bundle present")
            .instances
            .iter()
            .find(|candidate| candidate.id == "aapl")
            .expect("aapl instance present")
            .polling_interval_ms;
        assert_eq!(interval, 2_500, "effective bundle reflects the disk change");
        drop(bundle);

        watcher.shutdown().await;
    }

    #[tokio::test]
    async fn invalid_toml_records_failure_and_keeps_old_config() {
        let (_guard, dir) = fixture_config_dir("invalid-toml");
        let state = load_state(&dir);
        let watcher = ConfigWatcher::start(state.clone(), dir.clone(), state.bots_dir.clone())
            .expect("watcher starts");
        allow_watcher_to_arm().await;

        let baseline_generation = generation(&state);

        let bot_path = dir.join("bots").join("aapl.toml");
        std::fs::write(&bot_path, "this is not = valid = toml = at all\n[[[")
            .expect("invalid toml written");

        let failed = wait_until(|| {
            state
                .reload_status
                .try_read()
                .ok()
                .and_then(|history| history.back().map(|status| status.outcome == "failed"))
                .unwrap_or(false)
        })
        .await;
        assert!(failed, "watcher should record a failed reload");
        assert_eq!(last_trigger(&state).await, Some("file_watcher"));

        // The runtime keeps serving the previous (valid) bundle.
        assert_eq!(
            generation(&state),
            baseline_generation,
            "generation must not advance on a failed reload"
        );
        let bundle = state.config_bundle.read().await;
        assert!(
            bundle.as_ref().is_some_and(|bundle| {
                bundle
                    .instances
                    .iter()
                    .any(|instance| instance.id == "aapl")
            }),
            "old config must remain in memory after a failed reload"
        );
        drop(bundle);

        watcher.shutdown().await;
    }

    #[tokio::test]
    async fn self_write_of_identical_bundle_records_no_change() {
        let (_guard, dir) = fixture_config_dir("self-write");
        let state = load_state(&dir);
        let watcher = ConfigWatcher::start(state.clone(), dir.clone(), state.bots_dir.clone())
            .expect("watcher starts");
        allow_watcher_to_arm().await;

        let baseline_generation = generation(&state);

        // Re-serialize the current on-disk instance and write the identical
        // bytes back, mimicking a dashboard self-write of an unchanged bundle.
        let bot_path = dir.join("bots").join("aapl.toml");
        let current = std::fs::read_to_string(&bot_path).expect("current bot file readable");
        std::fs::write(&bot_path, current).expect("identical bytes rewritten");

        let no_change = wait_until(|| {
            state
                .reload_status
                .try_read()
                .ok()
                .and_then(|history| history.back().map(|status| status.outcome == "no_change"))
                .unwrap_or(false)
        })
        .await;
        assert!(no_change, "an unchanged bundle should record no_change");
        assert_eq!(last_trigger(&state).await, Some("file_watcher"));
        assert_eq!(
            generation(&state),
            baseline_generation,
            "a no_change reload must not bump the generation"
        );

        watcher.shutdown().await;
    }

    #[tokio::test]
    async fn rapid_edits_are_coalesced_into_a_bounded_number_of_reloads() {
        let (_guard, dir) = fixture_config_dir("coalesce");
        let state = load_state(&dir);
        let watcher = ConfigWatcher::start(state.clone(), dir.clone(), state.bots_dir.clone())
            .expect("watcher starts");
        allow_watcher_to_arm().await;

        let baseline_generation = generation(&state);

        // Five distinct writes well within the debounce window.
        let bot_path = dir.join("bots").join("aapl.toml");
        for interval in [1_100u64, 1_200, 1_300, 1_400, 1_500] {
            let mut instance = fixture_bundle().instances[0].clone();
            instance.polling_interval_ms = interval;
            write_rendered(&bot_path, &instance);
            tokio::time::sleep(Duration::from_millis(60)).await;
        }

        // Wait until the final value is applied.
        let applied = wait_until(|| generation(&state) > baseline_generation).await;
        assert!(
            applied,
            "coalesced edits should produce at least one reload"
        );

        // Allow any stragglers to settle past the debounce window before
        // sampling the final generation delta.
        tokio::time::sleep(Duration::from_millis(
            CONFIG_WATCH_DEBOUNCE_MS + CONFIG_WATCH_MAX_COALESCE_MS + 500,
        ))
        .await;

        let advanced = generation(&state) - baseline_generation;
        assert!(
            advanced <= 2,
            "debounce should coalesce 5 rapid edits into at most 2 reloads, saw {advanced}"
        );

        let bundle = state.config_bundle.read().await;
        let interval = bundle
            .as_ref()
            .expect("bundle present")
            .instances
            .iter()
            .find(|candidate| candidate.id == "aapl")
            .expect("aapl instance present")
            .polling_interval_ms;
        assert_eq!(interval, 1_500, "the last write wins after coalescing");
        drop(bundle);

        watcher.shutdown().await;
    }
}
