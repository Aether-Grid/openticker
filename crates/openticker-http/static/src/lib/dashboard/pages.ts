export const pageTitle = "OpenTicker Operator Workspace";

export const pageMeta = {
  overview: {
    title: "Workspace Overview",
    description: "Cross-runtime posture, feed pressure, latency markers, and operator watchpoints in one view.",
  },
  feeds: {
    title: "Dataplane Feeds",
    description:
      "Inspect registered ticker streams, buffer freshness, attached bots, and recent OHLCV bars from the in-memory dataplane.",
  },
  "feed-detail": {
    title: "Feed Detail",
    description:
      "Inspect a single dataplane stream, its latest bar state, and the in-memory OHLCV buffer.",
  },
  bots: {
    title: "Bot Supervision",
    description:
      "Inspect runtime bots, compare current posture, and open a dedicated bot detail page for lifecycle transitions, reconciliation review, and simulation-driven diagnostics.",
  },
  portfolio: {
    title: "Portfolio + Ledger",
    description:
      "Inspect account, bot, and lane ownership snapshots from the runtime ledger, including allocated, committed, blocked, and tradable room.",
  },
  "bot-detail": {
    title: "Bot Detail",
    description:
      "Lifecycle control, reconciliation review, PnL, and simulation tools for the selected bot.",
  },
  activity: {
    title: "Activity Streams",
    description:
      "Inspect recent journal history across signals, intents, risk, orders, fills, positions, reconciliation, and events. Use Bot Detail for authoritative current position state.",
  },
  cycles: {
    title: "Cycle Inspector",
    description:
      "Inspect one evaluated bot lane cycle end-to-end across signal, intent, risk, execution, position, capital, and reconciliation.",
  },
  "cycles-detail": {
    title: "Cycle Trace Detail",
    description:
      "Dedicated deep-dive surface for one trace, including rationale, capital deltas, reconciliation context, and linked records.",
  },
  providers: {
    title: "Provider Request Logs",
    description:
      "Inspect persisted request, response, normalization, and failure payloads captured at the runtime/provider boundary.",
  },
  connectors: {
    title: "Connector Operations",
    description:
      "Inspect runtime connector state, resilience envelopes, and matrix capabilities with a selected-account side panel.",
  },
  config: {
    title: "Config + API Surface",
    description:
      "Review effective config inventory, API readiness, metrics output, and generated OpenAPI coverage from one page.",
  },
} as const;

export type DashboardPageId = keyof typeof pageMeta;

export const navItems = [
  { id: "overview", label: "Overview", href: "/" },
  { id: "bots", label: "Bots", href: "/bots" },
  { id: "portfolio", label: "Portfolio", href: "/portfolio" },
  { id: "activity", label: "Activity", href: "/activity" },
  { id: "cycles", label: "Cycles", href: "/cycles" },
  { id: "feeds", label: "Feeds", href: "/feeds" },
  { id: "providers", label: "Providers", href: "/providers" },
  { id: "connectors", label: "Connectors", href: "/connectors" },
  { id: "config", label: "Config / API", href: "/config" },
] as const;

export const activityTabs = [
  { id: "signals", label: "Signals" },
  { id: "intents", label: "Intents" },
  { id: "risk", label: "Risk" },
  { id: "orders", label: "Orders" },
  { id: "fills", label: "Fills" },
  { id: "positions", label: "Positions" },
  { id: "reconciliations", label: "Reconciliations" },
  { id: "events", label: "Events" },
] as const;

export const terminalTabs = [
  { id: "config", label: "Config JSON" },
  { id: "metrics", label: "Metrics" },
  { id: "openapi", label: "OpenAPI" },
  { id: "route", label: "Selected Route" },
] as const;
