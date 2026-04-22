"use strict";

const FEED_LIMIT = 60;
const FOCUSED_TIMELINE_LIMIT = 80;
const REFRESH_INTERVAL_MS = 5000;
const CLOCK_INTERVAL_MS = 1000;

const DASHBOARD_SNAPSHOT_ENDPOINT = "/v1/dashboard/snapshot";
const SERVICE_ENDPOINT = "/v1/service/status";
const LEDGER_ENDPOINT = "/api/ledger";
const BOTS_ENDPOINT = "/v1/bots";
const CONNECTORS_STATUS_ENDPOINT = "/v1/connectors/status";
const CONNECTORS_MATRIX_ENDPOINT = "/v1/connectors/matrix";
const DATA_STREAMS_ENDPOINT = "/v1/data/streams";
const CONFIG_EFFECTIVE_ENDPOINT = "/v1/config/effective";
const HEALTH_ENDPOINT = "/healthz";
const READY_ENDPOINT = "/readyz";
const METRICS_ENDPOINT = "/metrics";
const OPENAPI_ENDPOINT = "/openapi.json";
const STREAM_BARS_LIMIT = 24;
const STREAM_HISTORY_LIMIT_DEFAULT = 200;
const STREAM_HISTORY_LIMIT_MAX = 1000;

const PAGE_META = {
  overview: {
    title: "Workspace Overview",
    description: "Cross-runtime posture, feed pressure, latency markers, and operator watchpoints in one view."
  },
  feeds: {
    title: "Dataplane Feeds",
    description: "Inspect registered ticker streams, buffer freshness, attached bots, in-memory dataplane bars, and on-demand connector history."
  },
  "feed-detail": {
    title: "Feed Detail",
    description: "Inspect a single dataplane stream, its latest bar state, in-memory OHLCV buffer, and connector history fetched on demand."
  },
  bots: {
    title: "Bot Supervision",
    description: "Inspect runtime bots, compare current posture, and open a dedicated bot detail page for lifecycle transitions, reconciliation review, and simulation-driven diagnostics."
  },
  portfolio: {
    title: "Portfolio + Ledger",
    description: "Inspect account, bot, and lane ownership snapshots from the runtime ledger, including allocated, committed, blocked, and tradable room."
  },
  "bot-detail": {
    title: "Bot Detail",
    description: "Lifecycle control, reconciliation review, PnL, and simulation tools for the selected bot."
  },
  activity: {
    title: "Activity Streams",
    description: "Inspect recent journal history across signals, intents, risk, orders, fills, positions, reconciliation, and events. Use Bot Detail for authoritative current position state."
  },
  cycles: {
    title: "Cycle Inspector",
    description: "Inspect one evaluated bot lane cycle end-to-end across signal, intent, risk, execution, position, capital, and reconciliation."
  },
  "cycles-detail": {
    title: "Cycle Trace Detail",
    description: "Dedicated deep-dive surface for one trace, including rationale, capital deltas, reconciliation context, and linked records."
  },
  providers: {
    title: "Provider Request Logs",
    description: "Inspect persisted request, response, normalization, and failure payloads captured at the runtime/provider boundary."
  },
  connectors: {
    title: "Connector Operations",
    description: "Inspect runtime connector state, resilience envelopes, and matrix capabilities with a selected-account side panel."
  },
  config: {
    title: "Config + API Surface",
    description: "Review effective config inventory, API readiness, metrics output, and generated OpenAPI coverage from one page."
  }
};

const STREAM_META = {
  signals: {
    label: "Signals",
    note: "Latest indicator output records from the runtime."
  },
  intents: {
    label: "Intents",
    note: "Strategy intents emitted before risk evaluation."
  },
  risk: {
    label: "Risk Decisions",
    note: "Allow or reject decisions with reasons where available."
  },
  orders: {
    label: "Orders",
    note: "Recent order submissions and runtime status transitions from the journal."
  },
  fills: {
    label: "Fills",
    note: "Recent execution acknowledgements recorded as fill history."
  },
  positions: {
    label: "Positions",
    note: "Recent position transitions and flatten reasons from the journal. Use Bot Detail for current position state."
  },
  reconciliations: {
    label: "Reconciliations",
    note: "Safety checks comparing runtime and connector state."
  },
  events: {
    label: "Events",
    note: "General service and entity events across the control plane."
  }
};

const TERMINAL_LABELS = {
  config: "Config JSON",
  metrics: "Metrics",
  openapi: "OpenAPI",
  route: "Selected Route"
};

const INITIAL_PAGE = (() => {
  const configuredPage = document.body && document.body.dataset
    ? document.body.dataset.initialPage
    : null;
  return configuredPage && PAGE_META[configuredPage] ? configuredPage : "overview";
})();
