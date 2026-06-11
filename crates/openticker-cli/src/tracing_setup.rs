use std::fs;
use std::path::PathBuf;
use std::sync::Once;
use tracing::info;
use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const DEFAULT_LOG_DIR: &str = "var/log";
const LOG_FILE_NAME: &str = "openticker-runtime.jsonl";
static TRACING_INIT: Once = Once::new();

pub(crate) fn init_tracing() {
    TRACING_INIT.call_once(|| {
        let filter = EnvFilter::try_from_env("OPENTICKER_LOG")
            .or_else(|_| EnvFilter::try_from_default_env())
            .unwrap_or_else(|_| {
                // `openticker_cli=info` keeps the operator audit trail for
                // destructive commands (close-positions, kill-switch, etc.)
                // visible by default; see `commands::confirm_destructive` and the
                // dashboard's `execute_bot_operation`.
                EnvFilter::new(
                    "openticker_runtime=debug,openticker_http=info,openticker_signals=debug,openticker_cli=info,warn",
                )
            });

        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_timer(UtcTime::rfc_3339())
            .with_target(true)
            .with_thread_names(true)
            .with_line_number(true)
            .with_writer(std::io::stderr)
            .compact();

        let log_dir = std::env::var_os("OPENTICKER_LOG_DIR")
            .map_or_else(|| PathBuf::from(DEFAULT_LOG_DIR), PathBuf::from);
        let (file_layer, file_logging_enabled) = match fs::create_dir_all(&log_dir) {
            Ok(()) => {
                let file_appender = rolling::daily(&log_dir, LOG_FILE_NAME);
                let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
                let _ = Box::leak(Box::new(guard));

                let layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_timer(UtcTime::rfc_3339())
                    .with_target(true)
                    .with_thread_names(true)
                    .with_line_number(true)
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_writer(file_writer);
                (Some(layer), true)
            }
            Err(error) => {
                eprintln!(
                    "warn: failed to create log directory `{}`: {error}",
                    log_dir.display()
                );
                (None, false)
            }
        };

        let init_result = tracing_subscriber::registry()
            .with(filter)
            .with(stderr_layer)
            .with(file_layer)
            .try_init();

        if let Err(error) = init_result {
            eprintln!("warn: failed to initialize tracing subscriber: {error}");
            return;
        }

        if file_logging_enabled {
            info!(
                log_dir = %log_dir.display(),
                file = LOG_FILE_NAME,
                "file logging enabled with daily rotation"
            );
        }
    });
}
