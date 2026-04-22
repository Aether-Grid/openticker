use crate::PROVIDER_EVENT_PAYLOAD_PREVIEW_LIMIT;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeEventLevel {
    Debug,
    Info,
    Warn,
    Error,
}

fn runtime_event_level(kind: &str) -> RuntimeEventLevel {
    if kind.ends_with(".failed") {
        return RuntimeEventLevel::Error;
    }

    if kind.starts_with("provider.") {
        return RuntimeEventLevel::Debug;
    }

    if kind.contains("rejected") || kind.contains("blocked") || kind.contains("unresolved") {
        return RuntimeEventLevel::Warn;
    }

    if matches!(kind, "intent.generated" | "poll.bar_received") {
        return RuntimeEventLevel::Debug;
    }

    if kind.starts_with("service.")
        || kind.starts_with("instance.")
        || kind.starts_with("poll.")
        || kind.starts_with("reconcile.")
        || kind.starts_with("risk.")
        || kind.starts_with("order.")
        || kind.starts_with("position.")
        || kind.starts_with("signal.")
    {
        return RuntimeEventLevel::Info;
    }

    RuntimeEventLevel::Debug
}

pub(crate) fn log_runtime_event(scope: &str, entity_id: Option<&str>, kind: &str, payload: &str) {
    let entity_id = entity_id.unwrap_or("-");
    match runtime_event_level(kind) {
        RuntimeEventLevel::Debug => debug!(scope, entity_id, kind, payload, "runtime event"),
        RuntimeEventLevel::Info => info!(scope, entity_id, kind, payload, "runtime event"),
        RuntimeEventLevel::Warn => warn!(scope, entity_id, kind, payload, "runtime event"),
        RuntimeEventLevel::Error => error!(scope, entity_id, kind, payload, "runtime event"),
    }
}

pub(crate) fn provider_payload_preview(payload: &str) -> serde_json::Value {
    let (text, truncated) = truncate_event_text(payload, PROVIDER_EVENT_PAYLOAD_PREVIEW_LIMIT);
    serde_json::json!({
        "text": text,
        "truncated": truncated,
        "original_bytes": payload.len(),
    })
}

pub(crate) fn truncate_event_text(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }

    let mut cut = max_bytes;
    while cut > 0 && !value.is_char_boundary(cut) {
        cut = cut.saturating_sub(1);
    }

    let preview = if cut == 0 { "" } else { &value[..cut] };
    (format!("{preview}..."), true)
}
