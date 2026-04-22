function initializeManualSignalControls() {
  const output = elements.simulation.output;
  const container = output ? output.parentElement : null;
  if (!container) {
    return;
  }

  const panel = document.createElement("section");
  panel.className = "stack";
  panel.id = "manual-signal-panel";
  panel.innerHTML = `
    <div class="toolbar-row">
      <div class="detail-stack">
        <p class="eyebrow">Manual Signal</p>
        <p class="panel-copy">Inject a synthetic buy or sell signal into the focused bot. This bypasses indicator evaluation, but still runs strategy mapping, risk checks, execution routing, and journaling. Order quantity is derived from the bot's sizing, ledger room, and risk rules using the signal price.</p>
      </div>
    </div>
    <div class="field-grid">
      <label class="field-block">
        <span>Signal</span>
        <select id="sim-manual-signal">
          <option value="buy_confirmed">Buy Confirmed</option>
          <option value="buy_preview">Buy Preview</option>
          <option value="sell_confirmed">Sell Confirmed</option>
          <option value="sell_preview">Sell Preview</option>
        </select>
      </label>
      <label class="field-block">
        <span>Timestamp (UTC)</span>
        <input id="sim-manual-timestamp" type="datetime-local">
      </label>
      <label class="field-block">
        <span>Price</span>
        <input id="sim-manual-price" type="number" min="0" step="any" placeholder="Uses latest focused close">
      </label>
    </div>
    <div class="button-grid">
      <button type="button" class="toolbar-button primary" id="sim-manual-btn">Inject Manual Signal</button>
    </div>
  `;
  container.insertBefore(panel, output);

  elements.simulation.manualPanel = panel;
  elements.simulation.manualSignal = panel.querySelector("#sim-manual-signal");
  elements.simulation.manualTimestamp = panel.querySelector("#sim-manual-timestamp");
  elements.simulation.manualPrice = panel.querySelector("#sim-manual-price");
  elements.simulation.manualButton = panel.querySelector("#sim-manual-btn");
}

initializeManualSignalControls();

const ACTIVITY_TAB_IDS = elements.activity.tabs.map((button) => button.dataset.streamTab);

function normalizeFeedLimit(value) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return FEED_LIMIT;
  }
  return Math.min(1_000, Math.max(1, Math.round(parsed)));
}

function feedEndpoint(path, params = null) {
  const search = new URLSearchParams();
  search.set("limit", String(normalizeFeedLimit(state.feedLimit)));
  if (params && typeof params === "object") {
    for (const [key, value] of Object.entries(params)) {
      if (value === undefined || value === null || value === "") {
        continue;
      }
      search.set(key, String(value));
    }
  }
  return `${path}?${search.toString()}`;
}

function setMessage(message, tone) {
  elements.flash.textContent = message;
  elements.flash.className = "flash panel";
  if (tone === "ok" || tone === "warn" || tone === "error") {
    elements.flash.classList.add(tone);
  }
  state.lastMessageTone = tone || null;
}

async function requestJson(path, options = {}) {
  const response = await fetch(path, {
    headers: {
      accept: "application/json",
      ...(options.headers || {})
    },
    ...options
  });

  const body = await response.text();
  let payload;
  if (body.length > 0) {
    try {
      payload = JSON.parse(body);
    } catch {
      payload = { error: body };
    }
  }

  if (!response.ok) {
    const reason = payload && payload.error ? payload.error : `${response.status} ${response.statusText}`;
    throw new Error(reason);
  }

  return payload;
}

async function requestOptionalJson(path, options = {}) {
  try {
    const data = await requestJson(path, options);
    return { ok: true, data, error: "" };
  } catch (error) {
    return { ok: false, data: null, error: error.message };
  }
}

async function requestText(path, options = {}) {
  const response = await fetch(path, {
    headers: {
      accept: "text/plain",
      ...(options.headers || {})
    },
    ...options
  });

  const body = await response.text();
  if (!response.ok) {
    const reason = body && body.trim().length > 0 ? body.trim() : `${response.status} ${response.statusText}`;
    throw new Error(reason);
  }
  return body;
}

async function requestOptionalText(path, options = {}) {
  try {
    const data = await requestText(path, options);
    return { ok: true, data, error: "" };
  } catch (error) {
    return { ok: false, data: "", error: error.message };
  }
}

function numberText(value, fractionDigits = 0) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return "-";
  }
  return parsed.toLocaleString(undefined, {
    minimumFractionDigits: 0,
    maximumFractionDigits: fractionDigits
  });
}

function priceInputText(value) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return "";
  }
  const fractionDigits = parsed >= 1 ? 4 : 8;
  return parsed.toLocaleString(undefined, {
    minimumFractionDigits: 0,
    maximumFractionDigits: fractionDigits,
    useGrouping: false
  });
}

function syncManualSignalPriceFromFocusedInstance(force = false) {
  if (!elements.simulation.manualPrice) {
    return;
  }
  const summary = state.focused.summary;
  const lane = selectedFocusedLane();
  const stream = lane ? streamStatusForSymbol(summary, lane.symbol) : null;
  const latestClose = stream && stream.latest_bar
    ? Number(stream.latest_bar.close)
    : Number.NaN;
  if (!Number.isFinite(latestClose) || latestClose <= 0) {
    return;
  }
  const hasManualOverride = elements.simulation.manualPrice.dataset.manualOverride === "true";
  if (force || !elements.simulation.manualPrice.value || !hasManualOverride) {
    elements.simulation.manualPrice.value = priceInputText(latestClose);
    elements.simulation.manualPrice.dataset.manualOverride = "false";
  }
}

function currencyText(value, fractionDigits = 2) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return "-";
  }
  return `$${Math.abs(parsed).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: fractionDigits
  })}`;
}

function signedCurrencyText(value, fractionDigits = 2) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return "-";
  }
  const amount = currencyText(parsed, fractionDigits);
  if (parsed > 0) {
    return `+${amount}`;
  }
  if (parsed < 0) {
    return `-${currencyText(parsed, fractionDigits)}`;
  }
  return amount;
}

function latencyText(value, fractionDigits = 1) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return "-";
  }
  return `${numberText(parsed, fractionDigits)} ms`;
}

function whenText(timestamp) {
  const millis = Number(timestamp);
  if (!Number.isFinite(millis) || millis <= 0) {
    return "time unknown";
  }
  return new Date(millis).toLocaleString();
}

function whenFromValue(value) {
  if (value === undefined || value === null || value === "") {
    return "-";
  }
  if (typeof value === "number") {
    return whenText(value);
  }
  const parsedNumeric = Number(value);
  if (Number.isFinite(parsedNumeric) && String(value).trim() !== "") {
    return whenText(parsedNumeric);
  }
  const parsedDate = new Date(value);
  if (!Number.isNaN(parsedDate.getTime())) {
    return parsedDate.toLocaleString();
  }
  return String(value);
}

function safeJsonParse(value) {
  if (typeof value !== "string") {
    return null;
  }
  const trimmed = value.trim();
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) {
    return null;
  }
  try {
    return JSON.parse(trimmed);
  } catch {
    return null;
  }
}

function connectorKindFromClientOrderId(clientOrderId) {
  const value = String(clientOrderId || "").trim().toLowerCase();
  if (value.startsWith("alpaca-")) {
    return "alpaca";
  }
  if (value.startsWith("binance-")) {
    return "binance";
  }
  return "";
}

function connectorKindForRecord(record) {
  if (!record || typeof record !== "object") {
    return "";
  }
  const explicit = String(record.connector_kind || "").trim().toLowerCase();
  if (explicit) {
    return explicit;
  }
  return connectorKindFromClientOrderId(record.client_order_id);
}

function compactText(value, maxChars) {
  if (value === undefined || value === null) {
    return "";
  }
  const text = String(value);
  if (text.length <= maxChars) {
    return text;
  }
  return `${text.slice(0, maxChars)}...`;
}

function titleText(value) {
  if (!value) {
    return "-";
  }
  return String(value)
    .replace(/_/g, " ")
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function formatJson(value) {
  if (value === undefined || value === null) {
    return "No payload available.";
  }
  if (typeof value === "string") {
    return value;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function activityInspectorRecord(kind, record) {
  if (!record || typeof record !== "object" || kind !== "events") {
    return record;
  }
  const parsedPayload = safeJsonParse(record.payload);
  if (!parsedPayload) {
    return record;
  }
  return {
    ...record,
    payload: parsedPayload
  };
}

function stateTone(value) {
  const text = String(value || "").toLowerCase();
  if (text === "running" || text === "connected" || text === "ready" || text === "yes" || text === "true" || text === "safe") {
    return "ok";
  }
  if (text === "paused" || text === "degraded" || text === "reconciling" || text === "warn" || text === "blocked") {
    return "warn";
  }
  if (text === "stopped" || text === "disconnected" || text === "error" || text === "not_ready" || text === "no") {
    return "error";
  }
  return "info";
}

function setBadge(element, label, tone) {
  element.textContent = label;
  const variantClass = element.dataset.badgeVariant ? ` ${element.dataset.badgeVariant}` : "";
  element.className = `badge${variantClass}`;
  if (tone === "ok" || tone === "warn" || tone === "error" || tone === "info") {
    element.classList.add(tone);
  }
}

function startButtonBusy(button, pendingText) {
  if (!button) {
    return () => {};
  }
  if (!button.dataset.originalText) {
    button.dataset.originalText = button.textContent;
  }
  button.textContent = pendingText;
  button.disabled = true;
  return () => {
    button.textContent = button.dataset.originalText || button.textContent;
    delete button.dataset.originalText;
  };
}

function triggerBackgroundRefresh(preserveMessage) {
  if (state.refreshing) {
    state.refreshQueued = true;
    return;
  }
  void refreshDashboard(preserveMessage);
}

function refreshTokenCurrent(refreshToken) {
  return state.deferredRefreshToken === refreshToken;
}

function primeSelectedStreamLoading() {
  if (!selectedDataStream()) {
    state.selectedDataStreamLoading = false;
    state.selectedDataStreamHistoryLoading = false;
    return;
  }

  state.selectedDataStreamLoading = true;
  state.selectedDataStreamHistoryLoading = true;
  state.selectedDataStreamBars = [];
  state.selectedDataStreamHistory = [];
  state.selectedDataStreamBarsError = "";
  state.selectedDataStreamHistoryError = "";
}

function primeFocusedInstanceLoading() {
  if (!state.focusedBotId || state.activePage !== "bot-detail") {
    return;
  }

  state.focused.loading = true;
  state.focused.timelineLoading = true;
}

async function hydrateDeferredDashboardData(refreshToken) {
  const optionalConfigPromise = requestOptionalJson(CONFIG_EFFECTIVE_ENDPOINT);
  const optionalSurfacePromise = Promise.all([
    requestOptionalJson(HEALTH_ENDPOINT),
    requestOptionalJson(READY_ENDPOINT),
    requestOptionalText(METRICS_ENDPOINT),
    requestOptionalJson(OPENAPI_ENDPOINT)
  ]).then(([health, ready, metrics, openapi]) => ({
    health,
    ready,
    metrics,
    openapi
  }));
  const selectedStreamPromise = Promise.all([
    refreshSelectedStreamBars(),
    refreshSelectedStreamHistory()
  ]);
  const focusedPromise = state.activePage === "bot-detail"
    ? refreshFocusedInstanceReport({ preserveMessage: true, forceSummaryFetch: true })
    : Promise.resolve();

  const [optionalConfig, optionalSurface] = await Promise.all([
    optionalConfigPromise,
    optionalSurfacePromise
  ]);
  if (!refreshTokenCurrent(refreshToken)) {
    return;
  }

  updateConfigSnapshot(optionalConfig);
  updateApiSurfaceSnapshot(optionalSurface);
  renderConfigWorkspace();
  renderNavigationContext();

  await selectedStreamPromise;
  if (!refreshTokenCurrent(refreshToken)) {
    return;
  }

  renderDataStreamSection();
  renderNavigationContext();

  await focusedPromise;
  if (!refreshTokenCurrent(refreshToken)) {
    return;
  }

  renderNavigationContext();
}

function recordSortDesc(left, right) {
  const leftTime = Number(left && left.created_at_ms);
  const rightTime = Number(right && right.created_at_ms);
  const delta = (Number.isFinite(rightTime) ? rightTime : 0) - (Number.isFinite(leftTime) ? leftTime : 0);
  if (delta !== 0) {
    return delta;
  }
  const leftId = Number(left && left.id);
  const rightId = Number(right && right.id);
  return (Number.isFinite(rightId) ? rightId : 0) - (Number.isFinite(leftId) ? leftId : 0);
}

function recordBotId(record) {
  return String(record && (record.bot_id ?? record.instance_id) ? (record.bot_id ?? record.instance_id) : "");
}

function recordSymbol(record) {
  return String(record && record.symbol ? record.symbol : "").trim().toUpperCase();
}

function traceIdForRecord(record) {
  return record && record.trace_id ? String(record.trace_id).trim() : "";
}

function openTraceForRecord(record, fallbackBotId = "", fallbackSymbol = "") {
  const traceId = traceIdForRecord(record);
  if (!traceId) {
    return;
  }
  const botId = recordBotId(record) || String(fallbackBotId || "").trim();
  const symbol = recordSymbol(record) || String(fallbackSymbol || "").trim().toUpperCase();
  const search = new URLSearchParams();
  if (botId) {
    search.set("bot_id", botId);
  }
  if (symbol) {
    search.set("symbol", symbol);
  }
  window.location.assign(`/cycles/${encodeURIComponent(traceId)}?${search.toString()}`);
}

function latestRecordForBot(records, botId, symbol = "") {
  if (!Array.isArray(records) || !botId) {
    return null;
  }
  const expectedSymbol = String(symbol || "").trim().toUpperCase();
  let latest = null;
  for (const record of records) {
    if (!record || recordBotId(record) !== botId) {
      continue;
    }
    if (expectedSymbol && recordSymbol(record) && recordSymbol(record) !== expectedSymbol) {
      continue;
    }
    if (!latest || recordSortDesc(record, latest) < 0) {
      latest = record;
    }
  }
  return latest;
}

function recordIdentity(kind, record, index = 0) {
  const parts = [
    kind,
    record && (record.id ?? record.client_order_id ?? record.kind ?? record.signal ?? record.intent ?? record.scope ?? index),
    record && (record.bot_id ?? record.instance_id ?? record.entity_id ?? "service"),
    record && record.created_at_ms,
    record && record.bar_timestamp
  ];
  return parts.join("::");
}

function parsePrometheusMetrics(rawText) {
  const metrics = {};
  if (typeof rawText !== "string" || rawText.length === 0) {
    return metrics;
  }
  const lines = rawText.split(/\r?\n/);
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) {
      continue;
    }
    const firstSpace = trimmed.search(/\s/);
    if (firstSpace <= 0) {
      continue;
    }
    const key = trimmed.slice(0, firstSpace);
    const valueText = trimmed.slice(firstSpace).trim();
    const value = Number(valueText);
    if (Number.isFinite(value)) {
      metrics[key] = value;
    }
  }
  return metrics;
}

function routeInventory(openapi) {
  const routes = [];
  if (!openapi || !openapi.paths || typeof openapi.paths !== "object") {
    return routes;
  }
  for (const [path, methods] of Object.entries(openapi.paths)) {
    if (!methods || typeof methods !== "object") {
      continue;
    }
    for (const [method, descriptor] of Object.entries(methods)) {
      routes.push({
        key: `${method.toUpperCase()} ${path}`,
        method: method.toUpperCase(),
        path,
        descriptor
      });
    }
  }
  routes.sort((left, right) => {
    if (left.path !== right.path) {
      return left.path.localeCompare(right.path);
    }
    return left.method.localeCompare(right.method);
  });
  return routes;
}

function dataStreamIdentity(stream) {
  if (!stream || !stream.key) {
    return "";
  }
  return `${stream.key.account_id}::${stream.key.symbol}::${stream.key.timeframe}`;
}

function selectedDataStream() {
  return state.dataStreams.find((stream) => dataStreamIdentity(stream) === state.selectedDataStreamKey) || null;
}

function durationText(ms) {
  const parsed = Number(ms);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return "-";
  }
  if (parsed < 1_000) {
    return `${numberText(parsed, 0)} ms`;
  }
  if (parsed < 60_000) {
    return `${numberText(parsed / 1_000, 1)} s`;
  }
  return `${numberText(parsed / 60_000, 1)} min`;
}

function timeframeDurationMs(timeframe) {
  switch (String(timeframe || "").toLowerCase()) {
    case "1m":
      return 60_000;
    case "5m":
      return 5 * 60_000;
    case "15m":
      return 15 * 60_000;
    case "30m":
      return 30 * 60_000;
    case "1h":
      return 60 * 60_000;
    case "4h":
      return 4 * 60 * 60_000;
    case "1d":
      return 24 * 60 * 60_000;
    default:
      return null;
  }
}

function barAgeMs(stream) {
  if (!stream || !stream.latest_bar || !stream.latest_bar.timestamp) {
    return null;
  }
  const timestamp = new Date(stream.latest_bar.timestamp).getTime();
  if (!Number.isFinite(timestamp)) {
    return null;
  }
  return Math.max(Date.now() - timestamp, 0);
}

function dataStreamTone(stream) {
  if (!stream) {
    return "info";
  }

  const hasLatestBar = Boolean(stream.latest_bar);
  const hasSuccess = stream.last_success_ms !== undefined && stream.last_success_ms !== null;
  const hasAttempts = Number(stream.fetch_count) > 0 || (stream.last_attempt_ms !== undefined && stream.last_attempt_ms !== null);
  const hasAttachedInstances = Array.isArray(stream.attached_instances) && stream.attached_instances.length > 0;
  const staleness = Number(stream.staleness_ms);
  const polling = Number(stream.polling_interval_ms);
  const timeframeMs = timeframeDurationMs(stream && stream.key ? stream.key.timeframe : null);
  const currentBarAgeMs = barAgeMs(stream);

  if (stream.last_error && !hasSuccess) {
    return "error";
  }

  if (!hasAttempts && !hasLatestBar) {
    return "info";
  }

  if (!hasLatestBar && hasAttachedInstances) {
    return "warn";
  }

  if (Number.isFinite(currentBarAgeMs) && Number.isFinite(timeframeMs) && timeframeMs > 0) {
    const barLagMultiplier = hasAttachedInstances ? 2.25 : 3.0;
    if (currentBarAgeMs > timeframeMs * barLagMultiplier) {
      return "warn";
    }
  } else if (Number.isFinite(staleness) && Number.isFinite(polling) && polling > 0) {
    const fetchGapMultiplier = hasAttachedInstances ? 20 : 30;
    if (staleness > polling * fetchGapMultiplier) {
      return "warn";
    }
  }

  if (stream.last_error) {
    return "warn";
  }

  return "ok";
}

function dataStreamStatusLabel(stream) {
  const tone = dataStreamTone(stream);
  if (!stream) {
    return "No Stream";
  }

  const hasLatestBar = Boolean(stream.latest_bar);
  const hasSuccess = stream.last_success_ms !== undefined && stream.last_success_ms !== null;
  const hasAttempts = Number(stream.fetch_count) > 0 || (stream.last_attempt_ms !== undefined && stream.last_attempt_ms !== null);
  const hasAttachedInstances = Array.isArray(stream.attached_instances) && stream.attached_instances.length > 0;
  const currentBarAgeMs = barAgeMs(stream);

  if (tone === "error") {
    return "Fetch Error";
  }
  if (!hasAttempts && !hasLatestBar) {
    return "Idle";
  }
  if (!hasLatestBar && hasAttachedInstances) {
    return "Awaiting Bars";
  }
  if (tone === "warn" && stream.last_error && hasSuccess) {
    return "Warning";
  }
  if (tone === "warn") {
    return Number.isFinite(currentBarAgeMs) ? "Lagging Bars" : "Lagging";
  }
  return "Healthy";
}

function sparklineSvg(values, width, height, tone, fillArea) {
  const safeValues = Array.isArray(values)
    ? values.map((value) => Number(value)).filter((value) => Number.isFinite(value))
    : [];
  const variant = fillArea ? "large" : "compact";

  if (safeValues.length === 0) {
    const emptyLabel = fillArea ? "No bars" : "Awaiting bars";
    return `<svg class="sparkline-svg ${variant}" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none"><line class="grid-line" x1="0" y1="${height / 2}" x2="${width}" y2="${height / 2}"></line><text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" fill="#8da0b5" font-family="var(--mono)" font-size="11">${emptyLabel}</text></svg>`;
  }

  const paddingX = fillArea ? 10 : 8;
  const paddingY = fillArea ? 18 : 7;
  const innerWidth = Math.max(width - (paddingX * 2), 1);
  const innerHeight = Math.max(height - (paddingY * 2), 1);
  const min = Math.min(...safeValues);
  const max = Math.max(...safeValues);
  const mean = safeValues.reduce((total, value) => total + value, 0) / safeValues.length;
  const observedRange = max - min;
  const paddingFloor = Math.max(Math.abs(mean || 1) * (fillArea ? 0.0005 : 0.0015), fillArea ? 0.0001 : 0.001);
  const domainPadding = Math.max(observedRange * (fillArea ? 0.2 : 0.5), paddingFloor);
  const domainMin = observedRange === 0 ? mean - domainPadding : min - domainPadding;
  const domainMax = observedRange === 0 ? mean + domainPadding : max + domainPadding;
  const range = domainMax - domainMin || 1;

  const sourceValues = safeValues.length === 1 ? [safeValues[0], safeValues[0]] : safeValues;
  const points = sourceValues.map((value, index) => {
    const x = paddingX + ((innerWidth * index) / Math.max(sourceValues.length - 1, 1));
    const y = paddingY + innerHeight - (((value - domainMin) / range) * innerHeight);
    return [x, y];
  });

  const pointString = points.map(([x, y]) => `${x.toFixed(2)},${y.toFixed(2)}`).join(" ");
  const linePath = points.map(([x, y], index) => `${index === 0 ? "M" : "L"}${x.toFixed(2)} ${y.toFixed(2)}`).join(" ");
  const areaPath = fillArea
    ? `${linePath} L ${points[points.length - 1][0].toFixed(2)} ${(height - paddingY).toFixed(2)} L ${points[0][0].toFixed(2)} ${(height - paddingY).toFixed(2)} Z`
    : "";
  const latestPoint = points[points.length - 1];
  const dotRadius = fillArea ? 3 : 2.5;

  return `
    <svg class="sparkline-svg ${variant}" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none">
      <line class="grid-line" x1="0" y1="${(height / 2).toFixed(2)}" x2="${width}" y2="${(height / 2).toFixed(2)}"></line>
      ${fillArea ? `<path class="fill ${tone}" d="${areaPath}"></path>` : ""}
      <polyline class="line ${tone}" points="${pointString}"></polyline>
      <circle class="dot ${tone}" cx="${latestPoint[0].toFixed(2)}" cy="${latestPoint[1].toFixed(2)}" r="${dotRadius}"></circle>
    </svg>
  `;
}

function renderEmptyList(listElement, message) {
  listElement.textContent = "";
  const item = document.createElement("li");
  item.className = "empty-state";
  item.textContent = message;
  listElement.appendChild(item);
}

function renderEmptyTableRow(body, colSpan, message) {
  body.textContent = "";
  const row = document.createElement("tr");
  const cell = document.createElement("td");
  cell.colSpan = colSpan;
  cell.className = "empty-state";
  cell.textContent = message;
  row.appendChild(cell);
  body.appendChild(row);
}

function renderOhlcvRows(body, bars) {
  body.textContent = "";
  for (const bar of [...bars].reverse()) {
    const row = document.createElement("tr");
    row.innerHTML = `
      <td><div class="table-primary">${whenFromValue(bar.timestamp)}</div></td>
      <td><div class="table-primary">${numberText(bar.open, 4)}</div></td>
      <td><div class="table-primary">${numberText(bar.high, 4)}</div></td>
      <td><div class="table-primary">${numberText(bar.low, 4)}</div></td>
      <td><div class="table-primary">${numberText(bar.close, 4)}</div></td>
      <td><div class="table-primary">${numberText(bar.volume, 2)}</div></td>
    `;
    body.appendChild(row);
  }
}

function renderBarList(container, items, formatter = (value) => numberText(value, 0)) {
  container.textContent = "";
  const safeItems = Array.isArray(items) ? items : [];
  if (safeItems.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state";
    empty.textContent = "No chart data available.";
    container.appendChild(empty);
    return;
  }

  const max = Math.max(
    1,
    ...safeItems.map((item) => {
      const parsed = Number(item.value);
      return Number.isFinite(parsed) ? parsed : 0;
    })
  );

  for (const item of safeItems) {
    const parsedValue = Number(item.value);
    const value = Number.isFinite(parsedValue) ? parsedValue : 0;
    const row = document.createElement("div");
    row.className = "bar-item";

    const meta = document.createElement("div");
    meta.className = "bar-meta";
    const label = document.createElement("span");
    label.className = "bar-label";
    label.textContent = item.label;
    const strong = document.createElement("strong");
    strong.textContent = item.displayValue || formatter(value);
    meta.appendChild(label);
    meta.appendChild(strong);

    const track = document.createElement("div");
    track.className = "bar-track";
    const fill = document.createElement("div");
    fill.className = "bar-fill";
    if (item.tone === "ok" || item.tone === "warn" || item.tone === "error") {
      fill.classList.add(item.tone);
    }
    const width = value > 0 ? Math.max((value / max) * 100, 4) : 0;
    fill.style.width = `${Math.min(width, 100)}%`;
    track.appendChild(fill);

    const note = document.createElement("p");
    note.className = "bar-note";
    note.textContent = item.note || "";

    row.appendChild(meta);
    row.appendChild(track);
    row.appendChild(note);
    container.appendChild(row);
  }
}

function updateClock() {
  elements.clockUtc.textContent = `${new Date().toLocaleTimeString(undefined, {
    hour12: false,
    timeZone: "UTC"
  })} UTC`;
}

function updateHeaderStatus(status) {
  if (!status) {
    setBadge(elements.modeChip, "Mode Unknown", "info");
    setBadge(elements.serviceChip, "Service Unknown", "warn");
    setBadge(elements.connectorSummary, "Connectors Unknown", "info");
    setBadge(elements.lastUpdated, "Not Synced", "warn");
    elements.modeBanner.textContent = "Waiting for runtime status...";
    elements.modeBanner.className = "mode-banner";
    return;
  }

  const connectorStatuses = state.connectorStatuses;
  const connectedCount = connectorStatuses.filter((entry) => entry && entry.state === "connected").length;
  const degradedCount = connectorStatuses.filter((entry) => entry && entry.state === "degraded").length;
  const downCount = connectorStatuses.filter((entry) => entry && entry.state === "disconnected").length;

  setBadge(elements.modeChip, status.live_mode_active ? "Live Mode" : "Paper Mode", status.live_mode_active ? "error" : "ok");
  setBadge(elements.serviceChip, status.ready ? "Service Ready" : "Service Not Ready", status.ready ? "ok" : "warn");
  setBadge(
    elements.configChip,
    state.payloads.config.ok ? "Managed Config" : "Unmanaged Config",
    state.payloads.config.ok ? "ok" : "warn"
  );
  setBadge(
    elements.connectorSummary,
    `${numberText(connectedCount, 0)} up | ${numberText(degradedCount, 0)} degraded | ${numberText(downCount, 0)} down`,
    downCount > 0 ? "warn" : "info"
  );

  elements.modeBanner.textContent = status.mode_banner && String(status.mode_banner).trim().length > 0
    ? status.mode_banner
    : (status.live_mode_active ? "Live mode active - real capital may be at risk" : "Paper mode - simulated trading only");
  elements.modeBanner.className = "mode-banner";
  elements.modeBanner.classList.add(status.live_mode_active ? "live" : "paper");
}

function updateSidebarStatus(status) {
  if (!status) {
    elements.sidebarReady.textContent = "-";
    elements.sidebarMode.textContent = "-";
    elements.sidebarInstances.textContent = "-";
    elements.sidebarRunning.textContent = "-";
    elements.sidebarConnectors.textContent = "-";
    elements.sidebarFocus.textContent = state.focusedBotId || "none";
    return;
  }
  const connectedCount = state.connectorStatuses.filter((entry) => entry && entry.state === "connected").length;
  elements.sidebarReady.textContent = status.ready ? "YES" : "NO";
  elements.sidebarMode.textContent = status.live_mode_active ? "LIVE" : "PAPER";
  elements.sidebarInstances.textContent = numberText(status.total_instances, 0);
  elements.sidebarRunning.textContent = numberText(status.running_instances, 0);
  elements.sidebarConnectors.textContent = `${numberText(connectedCount, 0)}/${numberText(state.connectorStatuses.length, 0)}`;
  elements.sidebarFocus.textContent = state.focusedBotId || "none";
}

function renderWatchlist() {
  const status = state.payloads.status;
  elements.watchlist.textContent = "";
  const items = [];

  if (!status) {
    items.push({ tone: "warn", title: "No runtime snapshot", detail: "The dashboard has not loaded a service snapshot yet." });
  } else {
    if (status.kill_switch_active) {
      items.push({ tone: "error", title: "Kill switch active", detail: "New intents are blocked until the switch is cleared." });
    }
    if (status.live_mode_active) {
      items.push({ tone: "warn", title: "Live mode armed", detail: "Real venue activity may be possible through the runtime." });
    }
    if (status.reconciliation_blocked_instances > 0) {
      items.push({
        tone: "warn",
        title: "Reconciliation blocked bots",
        detail: `${numberText(status.reconciliation_blocked_instances, 0)} bot(s) are blocked from new trading.`
      });
    }
    if (status.warmup_pending_instances > 0) {
      items.push({
        tone: "warn",
        title: "Warmup pending",
        detail: `${numberText(status.warmup_pending_instances, 0)} bot(s) are still loading confirmed-bar history before trading is allowed.`
      });
    }
    if (status.warmup_failed_instances > 0) {
      items.push({
        tone: "error",
        title: "Warmup recovery required",
        detail: `${numberText(status.warmup_failed_instances, 0)} bot(s) are waiting on backfill recovery or future confirmed bars.`
      });
    }
    const downConnectors = state.connectorStatuses.filter((entry) => entry && entry.state === "disconnected");
    if (downConnectors.length > 0) {
      items.push({
        tone: "error",
        title: "Disconnected connectors",
        detail: `${numberText(downConnectors.length, 0)} connector account(s) are unavailable.`
      });
    }
    const degradedConnectors = state.connectorStatuses.filter((entry) => entry && entry.state === "degraded");
    if (degradedConnectors.length > 0) {
      items.push({
        tone: "warn",
        title: "Degraded connectors",
        detail: `${numberText(degradedConnectors.length, 0)} connector account(s) are degraded.`
      });
    }
    if (items.length === 0) {
      items.push({ tone: "ok", title: "No active watchlist items", detail: "Runtime posture is nominal from the currently visible HTTP surfaces." });
    }
  }

  for (const item of items) {
    const li = document.createElement("li");
    li.className = `watch-item ${item.tone || ""}`.trim();
    const span = document.createElement("span");
    span.textContent = item.title;
    const strong = document.createElement("strong");
    strong.textContent = item.detail;
    li.appendChild(span);
    li.appendChild(strong);
    elements.watchlist.appendChild(li);
  }
}

function updateStatus(status) {
  state.payloads.status = status;
  updateHeaderStatus(status);
  updateSidebarStatus(status);

  if (!status) {
    return;
  }

  elements.stats.total.textContent = numberText(status.total_instances, 0);
  elements.stats.running.textContent = numberText(status.running_instances, 0);
  elements.stats.paused.textContent = numberText(status.paused_instances, 0);
  elements.stats.reconciling.textContent = numberText(status.reconciling_instances, 0);
  elements.stats.stopped.textContent = numberText(status.stopped_instances, 0);
  elements.stats.blocked.textContent = numberText(status.reconciliation_blocked_instances, 0);
  elements.stats.warmupReady.textContent = numberText(status.warmup_ready_instances, 0);
  elements.stats.warmupPending.textContent = numberText(status.warmup_pending_instances, 0);
  elements.stats.warmupFailed.textContent = numberText(status.warmup_failed_instances, 0);
  elements.stats.kill.textContent = status.kill_switch_active ? "ACTIVE" : "OFF";
  elements.stats.ready.textContent = status.ready ? "YES" : "NO";
  const connectedCount = state.connectorStatuses.filter((entry) => entry && entry.state === "connected").length;
  elements.stats.connectors.textContent = `${numberText(connectedCount, 0)}/${numberText(state.connectorStatuses.length, 0)}`;
}

function updateObservability(status) {
  const observability = status && status.observability ? status.observability : {};
  const processSamples = Number(observability.process_bar_latency_samples);
  const executionSamples = Number(observability.execution_submit_latency_samples);
  const totalSamples = (Number.isFinite(processSamples) ? processSamples : 0) + (Number.isFinite(executionSamples) ? executionSamples : 0);
  elements.observability.riskRejects.textContent = numberText(observability.risk_rejects_total, 0);
  elements.observability.resilienceWindows.textContent = numberText(status.connector_resilience_windows_active, 0);
  elements.observability.processAvg.textContent = latencyText(observability.process_bar_latency_ms_avg, 2);
  elements.observability.processMax.textContent = latencyText(observability.process_bar_latency_ms_max, 0);
  elements.observability.execAvg.textContent = latencyText(observability.execution_submit_latency_ms_avg, 2);
  elements.observability.execMax.textContent = latencyText(observability.execution_submit_latency_ms_max, 0);
  setBadge(
    elements.observability.samples,
    `${numberText(processSamples, 0)} bar | ${numberText(executionSamples, 0)} exec samples`,
    totalSamples > 0 ? "ok" : "warn"
  );
}

function countOpenPositionInstances(records) {
  if (!Array.isArray(records) || records.length === 0) {
    return 0;
  }
  const latestByInstance = new Map();
  const sorted = [...records].sort(recordSortDesc);
  for (const record of sorted) {
    const botId = recordBotId(record);
    if (!record || !botId || latestByInstance.has(botId)) {
      continue;
    }
    latestByInstance.set(botId, Boolean(record.has_position));
  }
  let openCount = 0;
  for (const hasPosition of latestByInstance.values()) {
    if (hasPosition) {
      openCount += 1;
    }
  }
  return openCount;
}

function computeAuthoritativeRealizedPnl(botSummaries) {
  if (!Array.isArray(botSummaries) || botSummaries.length === 0) {
    return { realizedPnl: 0, reportingBots: 0, totalBots: 0 };
  }

  let realizedPnl = 0;
  let reportingBots = 0;
  for (const summary of botSummaries) {
    const realized = Number(summary && summary.pnl ? summary.pnl.realized_usd : NaN);
    if (!Number.isFinite(realized)) {
      continue;
    }
    realizedPnl += realized;
    reportingBots += 1;
  }

  return {
    realizedPnl,
    reportingBots,
    totalBots: botSummaries.length
  };
}

function updateExecutionStats(feeds) {
  const orders = Array.isArray(feeds && feeds.orders) ? feeds.orders : [];
  const fills = Array.isArray(feeds && feeds.fills) ? feeds.fills : [];
  const positions = Array.isArray(feeds && feeds.positions) ? feeds.positions : [];
  const ledgerPayload = state.payloads.ledger;

  const submittedOrders = orders.filter((order) => String(order && order.status ? order.status : "").toLowerCase() === "submitted").length;
  const fillTurnover = fills.reduce((total, fill) => {
    const price = Number(fill && fill.price);
    const quantity = Number(fill && fill.quantity);
    if (!Number.isFinite(price) || !Number.isFinite(quantity)) {
      return total;
    }
    return total + (price * quantity);
  }, 0);
  const currentExposure = ledgerPayload && ledgerPayload.ok
    ? (Array.isArray(ledgerPayload.data && ledgerPayload.data.accounts)
      ? ledgerPayload.data.accounts.reduce((total, account) => total + (Number(account && account.attributed_open_notional_usd) || 0), 0)
      : 0)
    : null;

  const pnl = computeAuthoritativeRealizedPnl(state.botCache);
  const openPositionInstances = Array.isArray(state.botCache)
    ? state.botCache.filter((summary) => summary && summary.position && summary.position.has_position).length
    : countOpenPositionInstances(positions);

  elements.performance.ordersSeen.textContent = numberText(orders.length, 0);
  elements.performance.ordersSubmitted.textContent = numberText(submittedOrders, 0);
  elements.performance.fillsSeen.textContent = numberText(fills.length, 0);
  elements.performance.fillTurnover.textContent = currencyText(fillTurnover, 2);
  elements.performance.currentExposure.textContent = Number.isFinite(currentExposure) ? currencyText(currentExposure, 2) : "n/a";
  elements.performance.realizedPnl.textContent = signedCurrencyText(pnl.realizedPnl, 2);
  elements.performance.openPositions.textContent = numberText(openPositionInstances, 0);

  if (pnl.totalBots > 0) {
    elements.performance.realizedNote.textContent = `Cumulative net realized P/L from runtime bot summaries (${numberText(pnl.reportingBots, 0)}/${numberText(pnl.totalBots, 0)} bots reporting).`;
  } else {
    elements.performance.realizedNote.textContent = "No bot summaries loaded yet.";
  }
}

function updateConfigSnapshot(optionalConfig) {
  state.payloads.config = optionalConfig;
  if (!optionalConfig.ok) {
    setBadge(elements.configChip, "Unmanaged Config", "warn");
    elements.diagnostics.accounts.textContent = "n/a";
    elements.diagnostics.risk.textContent = "n/a";
    elements.diagnostics.instances.textContent = "n/a";
    elements.diagnostics.environment.textContent = "n/a";
    elements.diagnostics.storage.textContent = "n/a";
    elements.diagnostics.liveGate.textContent = "n/a";
    elements.diagnostics.note.textContent = `Managed config endpoint unavailable: ${optionalConfig.error}`;
    elements.config.accounts.textContent = "n/a";
    elements.config.risk.textContent = "n/a";
    elements.config.instances.textContent = "n/a";
    elements.config.environment.textContent = "n/a";
    elements.config.storage.textContent = "n/a";
    elements.config.liveGate.textContent = "n/a";
    elements.config.note.textContent = `Managed config endpoint unavailable: ${optionalConfig.error}`;
    return;
  }

  const config = optionalConfig.data || {};
  const accounts = Array.isArray(config.accounts) ? config.accounts : [];
  const riskProfiles = Array.isArray(config.risk_profiles) ? config.risk_profiles : [];
  const instances = Array.isArray(config.bots) ? config.bots : [];
  const environment = config.global && config.global.service ? config.global.service.environment : null;
  const storageKind = config.global && config.global.storage ? config.global.storage.kind : null;
  const explicitLive = config.global && config.global.safety ? config.global.safety.require_explicit_live_enable : null;
  const completeSecrets = accounts.filter((account) => {
    if (!account || !account.secret_status) {
      return false;
    }
    return account.secret_status.api_key_present && account.secret_status.api_secret_present;
  }).length;

  setBadge(elements.configChip, "Managed Config", "ok");

  elements.diagnostics.accounts.textContent = numberText(accounts.length, 0);
  elements.diagnostics.risk.textContent = numberText(riskProfiles.length, 0);
  elements.diagnostics.instances.textContent = numberText(instances.length, 0);
  elements.diagnostics.environment.textContent = environment || "n/a";
  elements.diagnostics.storage.textContent = storageKind || "n/a";
  elements.diagnostics.liveGate.textContent = explicitLive ? "required" : "off";
  elements.diagnostics.note.textContent = `${completeSecrets}/${accounts.length || 0} account entries expose API key + secret env references.`;

  elements.config.accounts.textContent = numberText(accounts.length, 0);
  elements.config.risk.textContent = numberText(riskProfiles.length, 0);
  elements.config.instances.textContent = numberText(instances.length, 0);
  elements.config.environment.textContent = environment || "n/a";
  elements.config.storage.textContent = storageKind || "n/a";
  elements.config.liveGate.textContent = explicitLive ? "required" : "off";
  elements.config.note.textContent = `${completeSecrets}/${accounts.length || 0} account entries expose API key + secret env references.`;
}

function updateLedgerSnapshot(optionalLedger) {
  state.payloads.ledger = optionalLedger;

  const portfolioReady = elements.portfolio
    && elements.portfolio.accountsBody
    && elements.portfolio.botsBody
    && elements.portfolio.lanesBody;

  const describeException = (exception) => {
    if (!exception || typeof exception !== "object") {
      return "Unknown exception";
    }
    const kind = titleText(exception.kind || "exception");
    const owner = exception.owner && typeof exception.owner === "object"
      ? `${exception.owner.account_id || "-"}/${exception.owner.bot_id || "-"}/${exception.owner.symbol || "-"}`
      : null;
    const symbol = exception.symbol ? String(exception.symbol) : null;
    const scope = owner || symbol || "service";
    const mode = exception.blocks_new_opens ? "blocks opens" : "advisory";
    return `${kind} (${scope}, ${mode})`;
  };

  if (!optionalLedger.ok) {
    if (portfolioReady) {
      elements.portfolio.accounts.textContent = "n/a";
      elements.portfolio.bots.textContent = "n/a";
      elements.portfolio.lanes.textContent = "n/a";
      elements.portfolio.effectiveCap.textContent = "n/a";
      elements.portfolio.committed.textContent = "n/a";
      elements.portfolio.tradeable.textContent = "n/a";
      elements.portfolio.blocked.textContent = "n/a";
      elements.portfolio.unattributed.textContent = "n/a";
      elements.portfolio.note.textContent = optionalLedger.error || "Ledger surface unavailable.";
      renderEmptyTableRow(elements.portfolio.accountsBody, 10, "Ledger endpoint unavailable.");
      renderEmptyTableRow(elements.portfolio.botsBody, 9, "Ledger endpoint unavailable.");
      renderEmptyTableRow(elements.portfolio.lanesBody, 7, "Ledger endpoint unavailable.");
      elements.portfolio.terminal.textContent = `Ledger endpoint unavailable: ${optionalLedger.error || "unknown error"}`;
      setBadge(elements.portfolio.chip, "Ledger Unavailable", "warn");
    }

    return;
  }

  const ledger = optionalLedger.data || {};
  const accounts = Array.isArray(ledger.accounts) ? ledger.accounts : [];
  const bots = Array.isArray(ledger.bots) ? ledger.bots : [];
  const lanes = Array.isArray(ledger.lanes) ? ledger.lanes : [];
  const totalEffectiveCap = accounts.reduce((total, account) => total + (Number(account && account.effective_cap_usd) || 0), 0);
  const totalCommitted = accounts.reduce((total, account) => total + (Number(account && account.total_committed_notional_usd) || 0), 0);
  const totalTradeable = accounts.reduce((total, account) => total + (Number(account && account.tradeable_open_room_usd) || 0), 0);
  const totalBlocked = accounts.reduce((total, account) => total + (Number(account && account.blocked_open_room_usd) || 0), 0);
  const totalUnattributed = accounts.reduce((total, account) => total + (Number(account && account.unattributed_open_notional_usd) || 0), 0);
  const totalAvailable = totalTradeable + totalBlocked;

  if (!portfolioReady) {
    return;
  }

  elements.portfolio.accounts.textContent = numberText(accounts.length, 0);
  elements.portfolio.bots.textContent = numberText(bots.length, 0);
  elements.portfolio.lanes.textContent = numberText(lanes.length, 0);
  elements.portfolio.effectiveCap.textContent = currencyText(totalEffectiveCap, 2);
  elements.portfolio.committed.textContent = currencyText(totalCommitted, 2);
  elements.portfolio.tradeable.textContent = currencyText(totalTradeable, 2);
  elements.portfolio.blocked.textContent = currencyText(totalBlocked, 2);
  elements.portfolio.unattributed.textContent = currencyText(totalUnattributed, 2);
  elements.portfolio.note.textContent = `Effective cap ${currencyText(totalEffectiveCap, 2)} minus committed ${currencyText(totalCommitted, 2)} leaves ${currencyText(totalAvailable, 2)} available room (${currencyText(totalTradeable, 2)} tradeable + ${currencyText(totalBlocked, 2)} blocked).`;

  elements.portfolio.accountsBody.textContent = "";
  elements.portfolio.botsBody.textContent = "";
  elements.portfolio.lanesBody.textContent = "";

  if (accounts.length === 0) {
    renderEmptyTableRow(elements.portfolio.accountsBody, 10, "No account ledger rows available.");
  } else {
    for (const account of accounts) {
      const declared = Number(account && account.declared_total_usd);
      const liveBalanceValue = account ? account.live_balance_usd : null;
      const liveBalance = liveBalanceValue == null ? null : Number(liveBalanceValue);
      const effectiveCap = Number(account && account.effective_cap_usd);
      const attributed = Number(account && account.attributed_open_notional_usd);
      const reserved = Number(account && account.reserved_open_notional_usd);
      const unattributed = Number(account && account.unattributed_open_notional_usd);
      const committed = Number(account && account.total_committed_notional_usd);
      const blocked = Number(account && account.blocked_open_room_usd);
      const tradeable = Number(account && account.tradeable_open_room_usd);
      const exceptions = Array.isArray(account && account.exceptions) ? account.exceptions : [];
      const exceptionSummary = exceptions.length > 0
        ? `<div class="table-secondary">${exceptions.map(describeException).join("; ")}</div>`
        : "";
      const tr = document.createElement("tr");
      tr.innerHTML = `
        <td>
          <div class="table-primary">${account && account.id ? account.id : "-"}</div>
          ${exceptionSummary}
        </td>
        <td><div class="table-primary">${currencyText(declared, 2)}</div></td>
        <td><div class="table-primary">${Number.isFinite(liveBalance) ? currencyText(liveBalance, 2) : "not available"}</div></td>
        <td><div class="table-primary">${currencyText(effectiveCap, 2)}</div></td>
        <td><div class="table-primary">${currencyText(attributed, 2)}</div></td>
        <td><div class="table-primary">${currencyText(reserved, 2)}</div></td>
        <td><div class="table-primary">${currencyText(unattributed, 2)}</div></td>
        <td><div class="table-primary">${currencyText(committed, 2)}</div></td>
        <td><div class="table-primary">${currencyText(blocked, 2)}</div></td>
        <td><div class="table-primary">${currencyText(tradeable, 2)}</div></td>
      `;
      elements.portfolio.accountsBody.appendChild(tr);
    }
  }

  if (bots.length === 0) {
    renderEmptyTableRow(elements.portfolio.botsBody, 9, "No bot ledger rows available.");
  } else {
    for (const bot of bots) {
      const pct = Number(bot && bot.pct);
      const allocated = Number(bot && bot.allocated_usd);
      const attributed = Number(bot && bot.attributed_open_notional_usd);
      const reserved = Number(bot && bot.reserved_open_notional_usd);
      const committed = Number(bot && bot.total_committed_notional_usd);
      const blocked = Number(bot && bot.blocked_open_room_usd);
      const tradeable = Number(bot && bot.tradeable_open_room_usd);
      const tr = document.createElement("tr");
      tr.innerHTML = `
        <td><div class="table-primary">${bot && bot.id ? bot.id : "-"}</div></td>
        <td><div class="table-primary">${bot && bot.account_id ? bot.account_id : "-"}</div></td>
        <td><div class="table-primary">${numberText(pct, 2)}%</div></td>
        <td><div class="table-primary">${currencyText(allocated, 2)}</div></td>
        <td><div class="table-primary">${currencyText(attributed, 2)}</div></td>
        <td><div class="table-primary">${currencyText(reserved, 2)}</div></td>
        <td><div class="table-primary">${currencyText(committed, 2)}</div></td>
        <td><div class="table-primary">${currencyText(blocked, 2)}</div></td>
        <td><div class="table-primary">${currencyText(tradeable, 2)}</div></td>
      `;
      elements.portfolio.botsBody.appendChild(tr);
    }
  }

  if (lanes.length === 0) {
    renderEmptyTableRow(elements.portfolio.lanesBody, 7, "No lane ownership rows available.");
  } else {
    const botByKey = new Map();
    for (const bot of bots) {
      const accountId = bot && bot.account_id ? String(bot.account_id) : "";
      const botId = bot && bot.id ? String(bot.id) : "";
      if (accountId && botId) {
        botByKey.set(`${accountId}::${botId}`, bot);
      }
    }

    for (const lane of lanes) {
      const owner = lane && lane.owner && typeof lane.owner === "object" ? lane.owner : {};
      const ownerAccountId = owner.account_id ? String(owner.account_id) : "-";
      const ownerBotId = owner.bot_id ? String(owner.bot_id) : "-";
      const ownerSymbol = owner.symbol ? String(owner.symbol) : "-";
      const attributed = Number(lane && lane.attributed_open_notional_usd);
      const reserved = Number(lane && lane.reserved_open_notional_usd);
      const committed = Number(lane && lane.total_committed_notional_usd);
      const botSlice = botByKey.get(`${ownerAccountId}::${ownerBotId}`);
      const allocated = Number(botSlice && botSlice.allocated_usd);
      const usagePct = Number.isFinite(allocated) && allocated > 0
        ? `${numberText((committed / allocated) * 100, 2)}%`
        : "-";

      const tr = document.createElement("tr");
      tr.innerHTML = `
        <td><div class="table-primary">${ownerAccountId}</div></td>
        <td><div class="table-primary">${ownerBotId}</div></td>
        <td><div class="table-primary">${ownerSymbol}</div></td>
        <td><div class="table-primary">${currencyText(attributed, 2)}</div></td>
        <td><div class="table-primary">${currencyText(reserved, 2)}</div></td>
        <td><div class="table-primary">${currencyText(committed, 2)}</div></td>
        <td><div class="table-primary">${usagePct}</div></td>
      `;
      elements.portfolio.lanesBody.appendChild(tr);
    }
  }

  elements.portfolio.terminal.textContent = formatJson(ledger);
  setBadge(
    elements.portfolio.chip,
    `${currencyText(totalTradeable, 2)} tradable`,
    totalTradeable > 0 ? "ok" : "warn"
  );
}

function updateApiSurfaceSnapshot(optionalSurface) {
  state.payloads.health = optionalSurface.health;
  state.payloads.ready = optionalSurface.ready;
  state.payloads.metrics = optionalSurface.metrics;
  state.payloads.openapi = optionalSurface.openapi;
  state.parsedMetrics = optionalSurface.metrics.ok ? parsePrometheusMetrics(optionalSurface.metrics.data) : {};
  state.openapiRoutes = optionalSurface.openapi.ok ? routeInventory(optionalSurface.openapi.data) : [];

  if (optionalSurface.health.ok) {
    const healthStatus = optionalSurface.health.data && optionalSurface.health.data.status ? optionalSurface.health.data.status : "ok";
    elements.api.health.textContent = healthStatus;
  } else {
    elements.api.health.textContent = "unavailable";
  }

  if (optionalSurface.ready.ok) {
    const readyStatus = optionalSurface.ready.data && optionalSurface.ready.data.status ? optionalSurface.ready.data.status : "unknown";
    elements.api.ready.textContent = readyStatus;
  } else {
    elements.api.ready.textContent = "unavailable";
  }

  if (optionalSurface.metrics.ok) {
    const seriesCount = Object.keys(state.parsedMetrics).length;
    elements.api.metrics.textContent = `${numberText(seriesCount, 0)} series`;
  } else {
    elements.api.metrics.textContent = "disabled";
  }

  if (optionalSurface.openapi.ok) {
    elements.api.openapi.textContent = `${numberText(state.openapiRoutes.length, 0)} routes`;
  } else {
    elements.api.openapi.textContent = "disabled";
  }

  const notes = [];
  notes.push(optionalSurface.health.ok ? "healthz reachable" : `healthz error: ${optionalSurface.health.error}`);
  notes.push(optionalSurface.ready.ok ? `readyz=${optionalSurface.ready.data && optionalSurface.ready.data.status ? optionalSurface.ready.data.status : "unknown"}` : `readyz error: ${optionalSurface.ready.error}`);
  notes.push(optionalSurface.metrics.ok ? "metrics sampled" : "metrics unavailable");
  notes.push(optionalSurface.openapi.ok ? "openapi sampled" : "openapi unavailable");
  elements.api.note.textContent = notes.join(" | ");
}

function renderOverviewBoards() {
  const status = state.payloads.status;
  if (!status) {
    renderBarList(elements.overview.stateBreakdown, []);
    renderBarList(elements.overview.latencyBreakdown, []);
    renderBarList(elements.overview.activityBreakdown, []);
    renderEmptyList(elements.overview.blotterList, "No blotter records yet.");
    elements.overview.blotterCount.textContent = "0 items";
    elements.overview.serviceTerminal.textContent = "Waiting for service payload.";
    return;
  }

  renderBarList(elements.overview.stateBreakdown, [
    { label: "Running", value: status.running_instances, tone: "ok", note: "Active bar processing." },
    { label: "Paused", value: status.paused_instances, tone: "warn", note: "Loaded but halted." },
    { label: "Reconciling", value: status.reconciling_instances, tone: "warn", note: "Inside reconciliation workflows." },
    { label: "Stopped", value: status.stopped_instances, tone: "error", note: "Not supervised." },
    { label: "Recon Blocked", value: status.reconciliation_blocked_instances, tone: "warn", note: "Unable to trade." }
  ]);

  renderBarList(elements.overview.latencyBreakdown, [
    { label: "Process Avg", value: state.payloads.status.observability && state.payloads.status.observability.process_bar_latency_ms_avg, note: "Average process_bar latency." },
    { label: "Process Max", value: state.payloads.status.observability && state.payloads.status.observability.process_bar_latency_ms_max, tone: "warn", note: "Max process_bar latency." },
    { label: "Exec Avg", value: state.payloads.status.observability && state.payloads.status.observability.execution_submit_latency_ms_avg, note: "Average execution latency." },
    { label: "Exec Max", value: state.payloads.status.observability && state.payloads.status.observability.execution_submit_latency_ms_max, tone: "warn", note: "Max execution latency." },
    { label: "Poll Cycle Avg", value: state.parsedMetrics.openticker_background_poll_cycle_latency_ms_avg, note: "HTTP-owned background poll loop." },
    { label: "Lock Wait Avg", value: state.parsedMetrics.openticker_background_poll_runtime_write_lock_wait_ms_avg, tone: "warn", note: "Runtime write lock wait inside polling." }
  ], (value) => latencyText(value, 2));

  const streamCounts = {
    signals: state.lastFeeds.signals.length,
    intents: state.lastFeeds.intents.length,
    risk: state.lastFeeds.risk.length,
    orders: state.lastFeeds.orders.length,
    fills: state.lastFeeds.fills.length,
    positions: state.lastFeeds.positions.length,
    reconciliations: state.lastFeeds.reconciliations.length,
    events: state.lastFeeds.events.length
  };
  renderBarList(elements.overview.activityBreakdown, [
    { label: "Signals", value: streamCounts.signals, note: "Indicator output." },
    { label: "Intents", value: streamCounts.intents, note: "Pre-risk strategy actions." },
    { label: "Risk", value: streamCounts.risk, note: "Allow / reject decisions." },
    { label: "Orders", value: streamCounts.orders, note: "Execution submissions." },
    { label: "Fills", value: streamCounts.fills, note: "Execution acknowledgements." },
    { label: "Events", value: streamCounts.events, note: "General runtime events." }
  ]);

  const blotter = [];
  for (const [kind, records] of Object.entries(state.lastFeeds)) {
    for (const record of records) {
      blotter.push({ kind, record });
    }
  }
  blotter.sort((left, right) => recordSortDesc(left.record, right.record));
  const visible = blotter.slice(0, 12);
  elements.overview.blotterList.textContent = "";
  elements.overview.blotterCount.textContent = `${numberText(visible.length, 0)} items`;
  if (visible.length === 0) {
    renderEmptyList(elements.overview.blotterList, "No blotter records yet.");
  } else {
    for (const item of visible) {
      const descriptor = describeRecord(item.kind, item.record);
      const li = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      button.className = `overview-record ${descriptor.tone || ""}`.trim();
      button.dataset.blotterKind = item.kind;
      button.dataset.blotterKey = recordIdentity(item.kind, item.record);
      const title = document.createElement("p");
      title.className = "record-title";
      title.textContent = descriptor.title;
      const detail = document.createElement("p");
      detail.className = "record-detail";
      detail.textContent = descriptor.detail;
      const meta = document.createElement("p");
      meta.className = "record-meta";
      meta.textContent = `${STREAM_META[item.kind].label} | ${descriptor.meta}`;
      button.appendChild(title);
      button.appendChild(detail);
      button.appendChild(meta);
      li.appendChild(button);
      elements.overview.blotterList.appendChild(li);
    }
  }

  elements.overview.serviceTerminal.textContent = formatJson(state.payloads.status);
}

function ensureSelectedDataStream() {
  if (!state.dataStreams.length) {
    state.selectedDataStreamKey = null;
    return;
  }

  if (state.selectedDataStreamKey && state.dataStreams.some((stream) => dataStreamIdentity(stream) === state.selectedDataStreamKey)) {
    return;
  }

  const sorted = [...state.dataStreams].sort((left, right) => {
    const tonePriority = { error: 0, warn: 1, ok: 2, info: 3 };
    const leftTone = tonePriority[dataStreamTone(left)] ?? 9;
    const rightTone = tonePriority[dataStreamTone(right)] ?? 9;
    if (leftTone !== rightTone) {
      return leftTone - rightTone;
    }
    return dataStreamIdentity(left).localeCompare(dataStreamIdentity(right));
  });

  state.selectedDataStreamKey = dataStreamIdentity(sorted[0]);
}

function selectedStreamHistoryLimit(stream) {
  if (!stream) {
    return STREAM_HISTORY_LIMIT_DEFAULT;
  }
  let required = STREAM_HISTORY_LIMIT_DEFAULT;
  const attached = Array.isArray(stream.attached_instances) ? stream.attached_instances : [];
  for (const botId of attached) {
    const summary = state.botCache.find((instance) => instance && instance.id === botId);
    const warmupRequired = Number(summary && summary.warmup && summary.warmup.required_bars);
    if (Number.isFinite(warmupRequired) && warmupRequired > 0) {
      required = Math.max(required, warmupRequired);
    }
  }
  return Math.max(STREAM_BARS_LIMIT, Math.min(required, STREAM_HISTORY_LIMIT_MAX));
}

function focusedLaneHistoryLimit(summary, lane) {
  let required = STREAM_HISTORY_LIMIT_DEFAULT;
  const laneWarmupRequired = Number(lane && lane.warmup && lane.warmup.required_bars);
  if (Number.isFinite(laneWarmupRequired) && laneWarmupRequired > 0) {
    required = Math.max(required, laneWarmupRequired);
  }
  const botWarmupRequired = Number(summary && summary.warmup && summary.warmup.required_bars);
  if (Number.isFinite(botWarmupRequired) && botWarmupRequired > 0) {
    required = Math.max(required, botWarmupRequired);
  }
  return Math.max(STREAM_BARS_LIMIT, Math.min(required, STREAM_HISTORY_LIMIT_MAX));
}

async function refreshSelectedStreamBars() {
  const stream = selectedDataStream();
  if (!stream || !stream.key) {
    state.selectedDataStreamBars = [];
    state.selectedDataStreamBarsError = "No stream selected.";
    state.selectedDataStreamLoading = false;
    return;
  }

  state.selectedDataStreamLoading = true;
  const key = dataStreamIdentity(stream);
  const account = encodeURIComponent(stream.key.account_id);
  const symbol = encodeURIComponent(stream.key.symbol);
  const timeframe = encodeURIComponent(stream.key.timeframe);
  const result = await requestOptionalJson(`/v1/data/streams/${account}/${symbol}/${timeframe}/bars?limit=${STREAM_BARS_LIMIT}`);

  if (state.selectedDataStreamKey !== key) {
    return;
  }

  state.selectedDataStreamLoading = false;
  if (result.ok && Array.isArray(result.data)) {
    state.selectedDataStreamBars = result.data;
    state.selectedDataStreamBarsError = "";
  } else {
    state.selectedDataStreamBars = [];
    state.selectedDataStreamBarsError = result.error || "Bar buffer unavailable.";
  }
}

async function refreshSelectedStreamHistory() {
  const stream = selectedDataStream();
  if (!stream || !stream.key) {
    state.selectedDataStreamHistory = [];
    state.selectedDataStreamHistoryError = "No stream selected.";
    state.selectedDataStreamHistoryLoading = false;
    return;
  }

  state.selectedDataStreamHistoryLoading = true;
  const key = dataStreamIdentity(stream);
  const account = encodeURIComponent(stream.key.account_id);
  const symbol = encodeURIComponent(stream.key.symbol);
  const timeframe = encodeURIComponent(stream.key.timeframe);
  const historyLimit = selectedStreamHistoryLimit(stream);
  const result = await requestOptionalJson(
    `/v1/data/streams/${account}/${symbol}/${timeframe}/history?limit=${historyLimit}`
  );

  if (state.selectedDataStreamKey !== key) {
    return;
  }

  state.selectedDataStreamHistoryLoading = false;
  const bars = result && result.ok && result.data && Array.isArray(result.data.bars)
    ? result.data.bars
    : null;
  if (bars) {
    state.selectedDataStreamHistory = bars;
    state.selectedDataStreamHistoryError = "";
  } else {
    state.selectedDataStreamHistory = [];
    state.selectedDataStreamHistoryError = result.error || "Connector history unavailable.";
  }
}

function focusedLaneMarketDataKey(summary, stream) {
  if (!summary || !summary.id || !stream || !stream.key) {
    return null;
  }
  return `${summary.id}::${stream.key.account_id}::${stream.key.symbol}::${stream.key.timeframe}`;
}

async function refreshFocusedLaneMarketData() {
  const summary = state.focused.summary;
  const lane = selectedFocusedLane();
  const stream = lane ? streamStatusForSymbol(summary, lane.symbol) : null;
  const marketData = state.focused.marketData;

  if (!summary || !lane) {
    marketData.key = null;
    marketData.historyLimit = null;
    marketData.loadingBuffer = false;
    marketData.loadingHistory = false;
    marketData.bufferBars = [];
    marketData.historyBars = [];
    marketData.bufferError = "Select a symbol lane to inspect market data.";
    marketData.historyError = "Select a symbol lane to inspect market data.";
    return;
  }

  if (!stream || !stream.key) {
    marketData.key = null;
    marketData.historyLimit = focusedLaneHistoryLimit(summary, lane);
    marketData.loadingBuffer = false;
    marketData.loadingHistory = false;
    marketData.bufferBars = [];
    marketData.historyBars = [];
    marketData.bufferError = "No dataplane stream is currently registered for this lane.";
    marketData.historyError = "No dataplane stream is currently registered for this lane.";
    return;
  }

  const key = focusedLaneMarketDataKey(summary, stream);
  const historyLimit = focusedLaneHistoryLimit(summary, lane);
  marketData.key = key;
  marketData.historyLimit = historyLimit;
  marketData.loadingBuffer = true;
  marketData.loadingHistory = true;
  marketData.bufferBars = [];
  marketData.historyBars = [];
  marketData.bufferError = "";
  marketData.historyError = "";

  const account = encodeURIComponent(stream.key.account_id);
  const symbol = encodeURIComponent(stream.key.symbol);
  const timeframe = encodeURIComponent(stream.key.timeframe);

  const [bufferResult, historyResult] = await Promise.all([
    requestOptionalJson(`/v1/data/streams/${account}/${symbol}/${timeframe}/bars?limit=${STREAM_BARS_LIMIT}`),
    requestOptionalJson(`/v1/data/streams/${account}/${symbol}/${timeframe}/history?limit=${historyLimit}`)
  ]);

  if (state.focused.marketData.key !== key) {
    return;
  }

  marketData.loadingBuffer = false;
  marketData.loadingHistory = false;

  if (bufferResult.ok && Array.isArray(bufferResult.data)) {
    marketData.bufferBars = bufferResult.data;
    marketData.bufferError = "";
  } else {
    marketData.bufferBars = [];
    marketData.bufferError = bufferResult.error || "Dataplane buffer unavailable.";
  }

  const historyBars = historyResult && historyResult.ok && historyResult.data && Array.isArray(historyResult.data.bars)
    ? historyResult.data.bars
    : null;
  if (historyBars) {
    marketData.historyBars = historyBars;
    marketData.historyError = "";
  } else {
    marketData.historyBars = [];
    marketData.historyError = (historyResult && historyResult.error) || "Connector history unavailable.";
  }
}

function renderDataStreamSection() {
  const streams = Array.isArray(state.dataStreams) ? state.dataStreams : [];
  const healthy = streams.filter((stream) => dataStreamTone(stream) === "ok");
  const stale = streams.filter((stream) => dataStreamTone(stream) === "warn");
  const erroring = streams.filter((stream) => dataStreamTone(stream) === "error");
  const attachedInstances = streams.reduce((total, stream) => total + (Array.isArray(stream.attached_instances) ? stream.attached_instances.length : 0), 0);
  const latestCloseStream = [...streams]
    .filter((stream) => stream && stream.latest_bar && Number.isFinite(Number(stream.latest_bar.close)))
    .sort((left, right) => Number(right.latest_bar.close) - Number(left.latest_bar.close))[0] || null;

  elements.overview.streamsCount.textContent = `${numberText(streams.length, 0)} streams`;
  elements.overview.streamsTotal.textContent = numberText(streams.length, 0);
  elements.overview.streamsHealthy.textContent = numberText(healthy.length, 0);
  elements.overview.streamsStale.textContent = numberText(stale.length, 0);
  elements.overview.streamsErroring.textContent = numberText(erroring.length, 0);
  elements.overview.streamsAttached.textContent = numberText(attachedInstances, 0);
  elements.overview.streamsLatestClose.textContent = latestCloseStream ? numberText(latestCloseStream.latest_bar.close, 4) : "-";

  elements.overview.streamList.textContent = "";
  if (streams.length === 0) {
    renderEmptyList(elements.overview.streamList, "No dataplane streams are registered.");
  } else {
    const sorted = [...streams].sort((left, right) => {
      const tonePriority = { error: 0, warn: 1, ok: 2, info: 3 };
      const leftTone = tonePriority[dataStreamTone(left)] ?? 9;
      const rightTone = tonePriority[dataStreamTone(right)] ?? 9;
      if (leftTone !== rightTone) {
        return leftTone - rightTone;
      }
      return dataStreamIdentity(left).localeCompare(dataStreamIdentity(right));
    });

    for (const stream of sorted) {
      const key = dataStreamIdentity(stream);
      const tone = dataStreamTone(stream);
      const label = dataStreamStatusLabel(stream);
      const currentBarAgeMs = barAgeMs(stream);
      const li = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      button.className = `stream-row ${tone}`.trim();
      if (key === state.selectedDataStreamKey) {
        button.classList.add("active");
      }
      button.dataset.streamKey = key;

      const head = document.createElement("div");
      head.className = "stream-row-head";
      const title = document.createElement("p");
      title.className = "stream-row-title";
      title.textContent = `${stream.key.account_id} / ${stream.key.symbol} / ${stream.key.timeframe}`;
      const chip = document.createElement("span");
      chip.className = `state-chip ${tone}`;
      chip.textContent = label;
      head.appendChild(title);
      head.appendChild(chip);

      const detail = document.createElement("p");
      detail.className = "stream-row-detail";
      const latestClose = stream.latest_bar ? numberText(stream.latest_bar.close, 4) : "-";
      detail.textContent = `latest close ${latestClose} | bar age ${durationText(currentBarAgeMs)} | fetch gap ${durationText(stream.staleness_ms)} | fetch ${numberText(stream.fetch_count, 0)} / error ${numberText(stream.error_count, 0)}`;

      const meta = document.createElement("p");
      meta.className = "stream-row-meta";
      meta.textContent = stream.last_error
        ? compactText(stream.last_error, 92)
        : `${Array.isArray(stream.attached_instances) && stream.attached_instances.length > 0 ? stream.attached_instances.join(", ") : "No attached bots"} | poll ${durationText(stream.polling_interval_ms)}`;

      const sparkline = document.createElement("div");
      sparkline.className = "sparkline-shell compact";
      sparkline.innerHTML = sparklineSvg(stream.sparkline, 240, 48, tone, false);

      button.appendChild(head);
      button.appendChild(detail);
      button.appendChild(meta);
      button.appendChild(sparkline);
      li.appendChild(button);
      elements.overview.streamList.appendChild(li);
    }
  }

  const selected = selectedDataStream();
  if (!selected) {
    setBadge(elements.overview.streamSelectedPill, "No selection", "info");
    elements.overview.streamSelectedNote.textContent = "Select a dataplane stream to inspect its recent OHLCV bars and in-memory close chart.";
    elements.overview.streamSelectedKey.textContent = "-";
    elements.overview.streamSelectedPolling.textContent = "-";
    elements.overview.streamSelectedStaleness.textContent = "-";
    elements.overview.streamSelectedFetches.textContent = "-";
    elements.overview.streamSelectedLatestBar.textContent = "-";
    elements.overview.streamSelectedAttachedCount.textContent = "-";
    elements.overview.streamSelectedAttached.textContent = "";
    elements.overview.streamChart.innerHTML = sparklineSvg([], 900, 220, "info", true);
    renderEmptyTableRow(elements.overview.streamBarsBody, 6, "No dataplane stream selected.");
    if (elements.overview.streamHistoryNote) {
      elements.overview.streamHistoryNote.textContent = "Connector History (On Demand)";
    }
    if (elements.overview.streamHistoryBody) {
      renderEmptyTableRow(elements.overview.streamHistoryBody, 6, "No dataplane stream selected.");
    }
    elements.overview.streamTerminal.textContent = "Select a stream to inspect the dataplane feed buffer here.";
    return;
  }

  const tone = dataStreamTone(selected);
  const label = dataStreamStatusLabel(selected);
  const latestBar = selected.latest_bar || null;
  const currentBarAgeMs = barAgeMs(selected);
  setBadge(elements.overview.streamSelectedPill, `${selected.key.symbol} ${selected.key.timeframe} | ${label}`, tone);
  elements.overview.streamSelectedNote.textContent = `Dataplane stream ${selected.key.account_id} / ${selected.key.symbol} / ${selected.key.timeframe}.`;
  if (elements.overview.streamHistoryNote) {
    elements.overview.streamHistoryNote.textContent = `Connector History (On Demand, up to ${numberText(selectedStreamHistoryLimit(selected), 0)} bars)`;
  }
  elements.overview.streamSelectedKey.textContent = `${selected.key.account_id} / ${selected.key.symbol} / ${selected.key.timeframe}`;
  elements.overview.streamSelectedPolling.textContent = durationText(selected.polling_interval_ms);
  elements.overview.streamSelectedStaleness.textContent = durationText(currentBarAgeMs);
  elements.overview.streamSelectedFetches.textContent = `${numberText(selected.fetch_count, 0)} / ${numberText(selected.error_count, 0)} | gap ${durationText(selected.staleness_ms)}`;
  elements.overview.streamSelectedLatestBar.textContent = latestBar ? `${whenFromValue(latestBar.timestamp)} | ${numberText(latestBar.close, 4)}` : "No bar yet";
  elements.overview.streamSelectedAttachedCount.textContent = numberText(Array.isArray(selected.attached_instances) ? selected.attached_instances.length : 0, 0);

  elements.overview.streamSelectedAttached.textContent = "";
  if (Array.isArray(selected.attached_instances) && selected.attached_instances.length > 0) {
    for (const instanceId of selected.attached_instances) {
      const chip = document.createElement("span");
      chip.className = "selection-chip";
      chip.textContent = instanceId;
      elements.overview.streamSelectedAttached.appendChild(chip);
    }
  } else {
    const chip = document.createElement("span");
    chip.className = "selection-chip";
    chip.textContent = "No attached bots";
    elements.overview.streamSelectedAttached.appendChild(chip);
  }

  const bars = Array.isArray(state.selectedDataStreamBars) ? state.selectedDataStreamBars : [];
  const historyBars = Array.isArray(state.selectedDataStreamHistory) ? state.selectedDataStreamHistory : [];
  const closes = bars.map((bar) => bar.close);
  elements.overview.streamChart.innerHTML = state.selectedDataStreamLoading
    ? sparklineSvg([], 900, 220, tone, true)
    : sparklineSvg(closes, 900, 220, tone, true);
  if (state.selectedDataStreamLoading) {
    renderEmptyTableRow(elements.overview.streamBarsBody, 6, "Loading bar buffer...");
  } else if (bars.length === 0) {
    renderEmptyTableRow(
      elements.overview.streamBarsBody,
      6,
      state.selectedDataStreamBarsError || "No bars in the selected dataplane buffer yet."
    );
  } else {
    renderOhlcvRows(elements.overview.streamBarsBody, bars);
  }

  if (elements.overview.streamHistoryBody) {
    if (state.selectedDataStreamHistoryLoading) {
      renderEmptyTableRow(elements.overview.streamHistoryBody, 6, "Loading connector history...");
    } else if (historyBars.length === 0) {
      renderEmptyTableRow(
        elements.overview.streamHistoryBody,
        6,
        state.selectedDataStreamHistoryError || "No connector history returned for this stream yet."
      );
    } else {
      renderOhlcvRows(elements.overview.streamHistoryBody, historyBars);
    }
  }

  elements.overview.streamTerminal.textContent = state.selectedDataStreamLoading || state.selectedDataStreamHistoryLoading
    ? "Loading selected stream bars and connector history..."
    : formatJson({
      stream: selected,
      buffer_bars: bars,
      connector_history_bars: historyBars,
    });
}

function botSummaryPolling(summary) {
  if (!summary || !summary.polling) {
    return "No polling metadata";
  }
  const polling = summary.polling;
  if (polling.last_error) {
    return `error | ${compactText(polling.last_error, 64)}`;
  }
  if (polling.last_success_ms) {
    return `success | ${whenText(polling.last_success_ms)}`;
  }
  if (polling.last_attempt_ms) {
    return `attempt | ${whenText(polling.last_attempt_ms)}`;
  }
  return "No polling metadata";
}

function botPollingDescriptor(summary) {
  if (!summary || !summary.polling) {
    return {
      primary: "No poll state",
      secondary: "No polling metadata"
    };
  }

  const polling = summary.polling;
  if (polling.last_error) {
    return {
      primary: "Poll Error",
      secondary: compactText(polling.last_error, 56)
    };
  }

  if (polling.last_success_ms) {
    return {
      primary: "Poll Success",
      secondary: whenText(polling.last_success_ms)
    };
  }

  if (polling.last_attempt_ms) {
    return {
      primary: "Attempted",
      secondary: whenText(polling.last_attempt_ms)
    };
  }

  return {
    primary: "No Poll State",
    secondary: "No polling metadata"
  };
}

function botWarmupDescriptor(summary) {
  const warmup = summary && summary.warmup ? summary.warmup : null;
  const symbolCount = Array.isArray(summary && summary.symbols) ? summary.symbols.length : 0;
  const readySymbols = Number(summary && summary.warmup_ready_symbols);
  if (!warmup) {
    return {
      primary: "No warmup state",
      secondary: "Unavailable"
    };
  }

  const required = Number(warmup.required_bars);
  const loaded = Number(warmup.loaded_bars);
  const progress = `${numberText(loaded, 0)} / ${numberText(required, 0)}`;
  if (warmup.ready) {
    return {
      primary: symbolCount > 1 ? `Ready | ${numberText(readySymbols, 0)} / ${numberText(symbolCount, 0)} lanes` : `Ready | ${progress}`,
      secondary: warmup.last_warmup_timestamp ? whenFromValue(warmup.last_warmup_timestamp) : "Confirmed history loaded"
    };
  }
  if (warmup.last_error) {
    return {
      primary: symbolCount > 1 ? `Pending | ${numberText(readySymbols, 0)} / ${numberText(symbolCount, 0)} lanes` : `Pending | ${progress}`,
      secondary: compactText(warmup.last_error, 56)
    };
  }
  return {
    primary: symbolCount > 1 ? `Pending | ${numberText(readySymbols, 0)} / ${numberText(symbolCount, 0)} lanes` : `Pending | ${progress}`,
    secondary: "Waiting for confirmed-bar warmup"
  };
}

function botPositionDescriptor(summary) {
  const position = summary && summary.position ? summary.position : null;
  const openSymbols = Number(summary && summary.open_symbol_count);
  if (!position) {
    return {
      primary: "Unknown",
      secondary: "No position snapshot",
      tone: "warn"
    };
  }

  const parsedQuantity = Number(position.quantity);
  const quantity = Number.isFinite(parsedQuantity) ? parsedQuantity : 0;
  const parsedEntry = position.entry_price == null ? NaN : Number(position.entry_price);
  const hasEntry = Number.isFinite(parsedEntry);

  if (position.has_position) {
    return {
      primary: openSymbols > 1 ? `Open | ${numberText(openSymbols, 0)} lanes` : `Open | ${numberText(quantity, 4)}`,
      secondary: openSymbols > 1
        ? `${currencyText(summary.aggregate_position_notional_usd, 2)} reserved`
        : (hasEntry ? `entry ${numberText(parsedEntry, 4)}` : "entry unavailable"),
      tone: "warn"
    };
  }

  return {
    primary: "Flat",
    secondary: "No open position",
    tone: "ok"
  };
}

function botSymbols(summary) {
  if (summary && Array.isArray(summary.symbols) && summary.symbols.length > 0) {
    return summary.symbols.filter(Boolean);
  }
  if (summary && summary.symbol) {
    return [summary.symbol];
  }
  return [];
}

function botSymbolsLabel(summary) {
  const symbols = botSymbols(summary);
  if (symbols.length === 0) {
    return "-";
  }
  return compactText(symbols.join(", "), 80);
}

function botSymbolBadges(summary, maxVisible = 10) {
  const symbols = botSymbols(summary);
  if (symbols.length === 0) {
    return {
      countLabel: "No tickers",
      markup: '<span class="ticker-badge muted">No tickers</span>'
    };
  }

  const visible = symbols.slice(0, maxVisible);
  const hidden = Math.max(symbols.length - visible.length, 0);
  const badges = visible.map((symbol) => `<span class="ticker-badge">${symbol}</span>`).join("");
  const overflow = hidden > 0 ? `<span class="ticker-badge more">+${numberText(hidden, 0)} more</span>` : "";
  const countLabel = `${numberText(symbols.length, 0)} ticker${symbols.length === 1 ? "" : "s"}`;
  return {
    countLabel,
    markup: `${badges}${overflow}`
  };
}

function focusedLanes() {
  return Array.isArray(state.focused.lanes) ? state.focused.lanes : [];
}

function syncFocusedLaneSelection() {
  const lanes = focusedLanes();
  if (lanes.length === 0) {
    state.focused.selectedLaneSymbol = null;
    return null;
  }
  const existing = lanes.find((lane) => lane && lane.symbol === state.focused.selectedLaneSymbol);
  if (existing) {
    return existing;
  }
  state.focused.selectedLaneSymbol = lanes[0].symbol;
  return lanes[0];
}

function selectedFocusedLane() {
  return syncFocusedLaneSelection();
}

function streamStatusForSymbol(summary, symbol) {
  if (!summary || !symbol) {
    return null;
  }
  return state.dataStreams.find((stream) => stream
    && stream.key
    && stream.key.account_id === summary.account
    && stream.key.symbol === symbol
    && String(stream.key.timeframe || "") === String(summary.timeframe || "")) || null;
}

function lanePollingDescriptor(summary, lane) {
  const stream = lane ? streamStatusForSymbol(summary, lane.symbol) : null;
  if (!stream) {
    return "No polling metadata";
  }
  if (stream.last_error) {
    return `error | ${compactText(stream.last_error, 42)}`;
  }
  if (stream.latest_bar && stream.latest_bar.timestamp) {
    return `bar ${whenFromValue(stream.latest_bar.timestamp)}`;
  }
  if (stream.last_success_ms) {
    return `success | ${whenText(stream.last_success_ms)}`;
  }
  if (stream.last_attempt_ms) {
    return `attempt | ${whenText(stream.last_attempt_ms)}`;
  }
  return "No polling metadata";
}

function botPnlDescriptor(summary) {
  const pnl = summary && summary.pnl ? summary.pnl : null;
  if (!pnl) {
    return {
      primary: "No PnL",
      secondary: "Unavailable",
      tone: "info"
    };
  }

  const total = Number(pnl.total_usd);
  const realized = Number(pnl.realized_usd);
  const unrealized = Number(pnl.unrealized_usd);
  const primaryValue = Number.isFinite(total) ? total : realized;
  const primary = Number.isFinite(primaryValue) ? signedCurrencyText(primaryValue, 2) : "-";
  const secondary = Number.isFinite(unrealized)
    ? `realized ${signedCurrencyText(realized, 2)} | unrealized ${signedCurrencyText(unrealized, 2)}`
    : `realized ${signedCurrencyText(realized, 2)} | waiting for mark`;
  const tone = primaryValue > 0 ? "ok" : (primaryValue < 0 ? "error" : "info");
  return { primary, secondary, tone };
}

function holdDurationText(ms) {
  const parsed = Number(ms);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return "-";
  }
  const totalMinutes = Math.round(parsed / 60_000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours > 0) {
    return `${hours}h ${numberText(minutes, 0)}m`;
  }
  return `${numberText(minutes, 0)}m`;
}

function setTonalValue(element, text, value) {
  if (!element) {
    return;
  }
  element.textContent = text;
  element.classList.remove("positive", "negative", "neutral");
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    element.classList.add("neutral");
    return;
  }
  if (parsed > 0) {
    element.classList.add("positive");
  } else if (parsed < 0) {
    element.classList.add("negative");
  } else {
    element.classList.add("neutral");
  }
}

function focusedLedgerContext(summary) {
  const payload = state.payloads.ledger;
  if (!payload || !payload.ok || !payload.data || !summary) {
    return { bot: null, account: null };
  }
  const ledger = payload.data;
  const bots = Array.isArray(ledger.bots) ? ledger.bots : [];
  const accounts = Array.isArray(ledger.accounts) ? ledger.accounts : [];
  return {
    bot: bots.find((entry) => entry && entry.id === summary.id) || null,
    account: accounts.find((entry) => entry && entry.id === summary.account) || null
  };
}

function deriveBotTrades(summary, orders, fills, symbolFilter = "") {
  const expectedSymbol = String(symbolFilter || "").trim().toUpperCase();
  const orderMap = new Map();
  for (const order of Array.isArray(orders) ? orders : []) {
    if (!order || !order.client_order_id) {
      continue;
    }
    if (expectedSymbol && recordSymbol(order) && recordSymbol(order) !== expectedSymbol) {
      continue;
    }
    if (order && order.client_order_id) {
      orderMap.set(order.client_order_id, order);
    }
  }

  const sortedFills = [...(Array.isArray(fills) ? fills : [])]
    .filter((fill) => !expectedSymbol || !recordSymbol(fill) || recordSymbol(fill) === expectedSymbol)
    .filter((fill) => fill && Number.isFinite(Number(fill.quantity)) && Number(fill.quantity) > 0)
    .sort((left, right) => Number(left.created_at_ms || 0) - Number(right.created_at_ms || 0));

  let openQuantity = 0;
  let averageEntry = 0;
  let openedAtMs = null;
  const closedTrades = [];

  for (const fill of sortedFills) {
    const order = orderMap.get(fill.client_order_id) || null;
    const intent = String(order && order.intent ? order.intent : "").toLowerCase();
    const fillQuantity = Number(fill.quantity);
    const fillPrice = Number(fill.price);
    const fillTimestamp = Number(fill.created_at_ms || 0);
    if (!Number.isFinite(fillQuantity) || !Number.isFinite(fillPrice) || fillQuantity <= 0) {
      continue;
    }

    if (intent === "open_long" || intent === "add_long") {
      const currentNotional = openQuantity * averageEntry;
      const addedNotional = fillQuantity * fillPrice;
      const nextQuantity = openQuantity + fillQuantity;
      averageEntry = nextQuantity > 0 ? (currentNotional + addedNotional) / nextQuantity : fillPrice;
      openQuantity = nextQuantity;
      if (!openedAtMs) {
        openedAtMs = fillTimestamp;
      }
      continue;
    }

    if (intent !== "reduce_long" && intent !== "close_long") {
      continue;
    }

    const closedQuantity = Math.min(openQuantity > 0 ? openQuantity : fillQuantity, fillQuantity);
    const entryPrice = Number.isFinite(averageEntry) && averageEntry > 0
      ? averageEntry
      : (summary && summary.position && summary.position.entry_price != null
        ? Number(summary.position.entry_price)
        : NaN);
    const tradePnl = Number.isFinite(entryPrice)
      ? (fillPrice - entryPrice) * closedQuantity
      : NaN;
    const entryTimestamp = openedAtMs || fillTimestamp;
    closedTrades.push({
      side: "LONG",
      entryTimeMs: entryTimestamp,
      exitTimeMs: fillTimestamp,
      entryPrice,
      exitPrice: fillPrice,
      quantity: closedQuantity,
      holdingMs: Math.max(fillTimestamp - entryTimestamp, 0),
      entryNotional: Number.isFinite(entryPrice) ? entryPrice * closedQuantity : NaN,
      exitNotional: fillPrice * closedQuantity,
      netPnl: tradePnl,
      clientOrderId: fill.client_order_id || ""
    });

    openQuantity = Math.max(openQuantity - closedQuantity, 0);
    if (openQuantity <= 1e-9) {
      openQuantity = 0;
      averageEntry = 0;
      openedAtMs = null;
    }
  }

  const sampledTrades = [...closedTrades].sort((left, right) => right.exitTimeMs - left.exitTimeMs);
  const tradeStats = sampledTrades.reduce((stats, trade) => {
    if (!Number.isFinite(trade.netPnl)) {
      return stats;
    }
    stats.closedTrades += 1;
    stats.holdMsTotal += trade.holdingMs;
    if (trade.netPnl > 0) {
      stats.winningTrades += 1;
      stats.biggestWin = stats.biggestWin == null ? trade.netPnl : Math.max(stats.biggestWin, trade.netPnl);
    }
    if (trade.netPnl < 0) {
      stats.biggestLoss = stats.biggestLoss == null ? trade.netPnl : Math.min(stats.biggestLoss, trade.netPnl);
    }
    return stats;
  }, {
    closedTrades: 0,
    winningTrades: 0,
    holdMsTotal: 0,
    biggestWin: null,
    biggestLoss: null
  });

  const openPosition = openQuantity > 1e-9 || (summary && summary.position && summary.position.has_position)
    ? {
        side: "LONG",
        quantity: openQuantity > 1e-9 ? openQuantity : Number(summary.position && summary.position.quantity),
        entryPrice: Number.isFinite(averageEntry) && averageEntry > 0
          ? averageEntry
          : Number(summary && summary.position ? summary.position.entry_price : NaN),
        entryTimeMs: openedAtMs,
        markPrice: Number(summary && summary.pnl ? summary.pnl.mark_price : NaN),
        unrealizedPnl: Number(summary && summary.pnl ? summary.pnl.unrealized_usd : NaN)
      }
    : null;

  return {
    trades: sampledTrades,
    stats: {
      closedTrades: tradeStats.closedTrades,
      winningTrades: tradeStats.winningTrades,
      averageHoldMs: tradeStats.closedTrades > 0 ? tradeStats.holdMsTotal / tradeStats.closedTrades : NaN,
      biggestWin: tradeStats.biggestWin,
      biggestLoss: tradeStats.biggestLoss
    },
    openPosition
  };
}

function renderBotTradesTable(trades) {
  elements.focus.tradesBody.textContent = "";
  const rows = Array.isArray(trades) ? trades.slice(0, 25) : [];
  elements.focus.tradesCount.textContent = `${numberText(rows.length, 0)} trade${rows.length === 1 ? "" : "s"}`;
  if (rows.length === 0) {
    renderEmptyTableRow(elements.focus.tradesBody, 10, "No closed trades in the sampled fill window for this bot yet.");
    return;
  }

  for (const trade of rows) {
    const row = document.createElement("tr");
    const pnlClass = Number(trade.netPnl) > 0 ? "positive" : (Number(trade.netPnl) < 0 ? "negative" : "neutral");
    row.innerHTML = `
      <td><div class="table-primary">${trade.side}</div></td>
      <td><div class="table-primary">${whenText(trade.entryTimeMs)}</div></td>
      <td><div class="table-primary">${whenText(trade.exitTimeMs)}</div></td>
      <td><div class="table-primary">${numberText(trade.entryPrice, 4)}</div></td>
      <td><div class="table-primary">${numberText(trade.exitPrice, 4)}</div></td>
      <td><div class="table-primary">${numberText(trade.quantity, 4)}</div></td>
      <td><div class="table-primary">${holdDurationText(trade.holdingMs)}</div></td>
      <td><div class="table-primary">${currencyText(trade.entryNotional, 2)}</div></td>
      <td><div class="table-primary">${currencyText(trade.exitNotional, 2)}</div></td>
      <td><div class="table-primary bot-table-pnl ${pnlClass}">${signedCurrencyText(trade.netPnl, 2)}</div></td>
    `;
    elements.focus.tradesBody.appendChild(row);
  }
}

function renderActivePositionCard(summary, ledgerContext, derivedTrades) {
  const position = derivedTrades.openPosition;
  const botLedger = ledgerContext.bot;
  const selectedLane = selectedFocusedLane();
  const latestPosition = latestRecordForBot(
    state.focused.feeds.positions,
    summary && summary.id,
    selectedLane ? selectedLane.symbol : ""
  );

  if (!position || !Number.isFinite(Number(position.quantity)) || Number(position.quantity) <= 0) {
    elements.focus.activePosition.innerHTML = `
      <div class="bot-position-card">
        <div class="bot-position-header">
          <div>
            <p class="eyebrow">Current Position</p>
            <h4 class="bot-position-title">Flat</h4>
          </div>
          <span class="state-chip ok">No Open Position</span>
        </div>
        <p class="bot-position-empty">This bot is flat right now. Last recorded transition: ${latestPosition ? compactText(latestPosition.reason || "unknown", 48) : "no position journal entries yet"}.</p>
      </div>
    `;
    return;
  }

  const entryPrice = Number(position.entryPrice);
  const quantity = Number(position.quantity);
  const markPrice = Number(position.markPrice);
  const openNotional = Number.isFinite(entryPrice) ? entryPrice * quantity : NaN;
  const unrealizedPnl = Number(position.unrealizedPnl);
  const ledgerOpen = Number(botLedger && botLedger.attributed_open_notional_usd);
  const ledgerRemaining = Number(botLedger && botLedger.tradeable_open_room_usd);
  const markDisplay = Number.isFinite(markPrice) ? numberText(markPrice, 4) : "waiting for mark";

  elements.focus.activePosition.innerHTML = `
      <div class="bot-position-card">
        <div class="bot-position-header">
          <div>
            <p class="eyebrow">Current Position</p>
            <h4 class="bot-position-title">LONG ${selectedLane ? selectedLane.symbol : (summary && summary.symbol ? summary.symbol : "")}</h4>
            <p class="bot-position-subtitle">Entry ${position.entryTimeMs ? whenText(position.entryTimeMs) : "time unavailable"} | Last transition ${latestPosition ? compactText(latestPosition.reason || "unknown", 42) : "n/a"}</p>
          </div>
        <span class="state-chip warn">Open</span>
      </div>
      <div class="bot-position-grid">
        <div class="bot-position-stat"><span>Entry Price</span><strong>${numberText(entryPrice, 4)}</strong></div>
        <div class="bot-position-stat"><span>Quantity</span><strong>${numberText(quantity, 4)}</strong></div>
        <div class="bot-position-stat"><span>Mark</span><strong>${markDisplay}</strong></div>
        <div class="bot-position-stat"><span>Open Notional</span><strong>${currencyText(Number.isFinite(ledgerOpen) ? ledgerOpen : openNotional, 2)}</strong></div>
        <div class="bot-position-stat"><span>Remaining Ledger Room</span><strong>${currencyText(ledgerRemaining, 2)}</strong></div>
        <div class="bot-position-stat"><span>Unrealized P&amp;L</span><strong class="bot-tonal-value ${unrealizedPnl > 0 ? "positive" : (unrealizedPnl < 0 ? "negative" : "neutral")}">${signedCurrencyText(unrealizedPnl, 2)}</strong></div>
      </div>
    </div>
  `;
}

function filteredBots() {
  const search = elements.instances.search.value.trim().toLowerCase();
  const stateFilter = state.botStateFilter;
  const marketFilter = state.botMarketFilter;
  const enabledFilter = state.botEnabledFilter;
  return state.botCache.filter((instance) => {
    if (!instance) {
      return false;
    }
    if (marketFilter !== "all" && String(instance.market || "").toLowerCase() !== marketFilter) {
      return false;
    }
    if (enabledFilter === "enabled" && !instance.enabled) {
      return false;
    }
    if (enabledFilter === "disabled" && instance.enabled) {
      return false;
    }
    if (stateFilter === "blocked" && !instance.reconciliation_blocked) {
      return false;
    }
    if (stateFilter !== "all" && stateFilter !== "blocked" && instance.state !== stateFilter) {
      return false;
    }
    if (!search) {
      return true;
    }
    const haystack = [instance.id, instance.market, instance.account, instance.timeframe, instance.mode_banner, instance.execution_mode, ...botSymbols(instance)]
      .filter(Boolean)
      .join(" ")
      .toLowerCase();
    return haystack.includes(search);
  });
}

function syncBotFilterTabs() {
  for (const button of elements.instances.stateTabs) {
    const active = button.dataset.instanceStateTab === state.botStateFilter;
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", active ? "true" : "false");
  }
  for (const button of elements.instances.marketTabs) {
    const active = button.dataset.instanceMarketTab === state.botMarketFilter;
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", active ? "true" : "false");
  }
  for (const button of elements.instances.enabledTabs) {
    const active = button.dataset.instanceEnabledTab === state.botEnabledFilter;
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", active ? "true" : "false");
  }
}

const BOT_DIRECTORY_COLUMN_COUNT = 7;

function renderBots() {
  syncBotFilterTabs();
  const filtered = filteredBots();
  elements.instances.visibleCount.textContent = `${numberText(filtered.length, 0)} / ${numberText(state.botCache.length, 0)}`;
  elements.instances.body.textContent = "";

  if (filtered.length === 0) {
    renderEmptyTableRow(elements.instances.body, BOT_DIRECTORY_COLUMN_COUNT, "No bots match the current filters.");
    return;
  }

  const sorted = [...filtered].sort((left, right) => {
    if (left.state !== right.state) {
      return String(left.state || "").localeCompare(String(right.state || ""));
    }
    return String(left.id || "").localeCompare(String(right.id || ""));
  });

  const knownBotIds = new Set(
    state.botCache
      .map((instance) => (instance && instance.id ? String(instance.id) : ""))
      .filter(Boolean)
  );
  for (const expandedId of state.expandedBotIds) {
    if (!knownBotIds.has(expandedId)) {
      state.expandedBotIds.delete(expandedId);
    }
  }

  for (const bot of sorted) {
    const botId = bot && bot.id ? String(bot.id) : "";
    const isExpanded = botId ? state.expandedBotIds.has(botId) : false;
    const pollingDescriptor = botPollingDescriptor(bot);
    const warmupDescriptor = botWarmupDescriptor(bot);
    const positionDescriptor = botPositionDescriptor(bot);
    const pnlDescriptor = botPnlDescriptor(bot);
    const tickerBadges = botSymbolBadges(bot, 6);
    const detailTickerBadges = botSymbolBadges(bot, 48);
    const symbols = botSymbols(bot);
    const symbolsList = symbols.length > 0 ? symbols.join(", ") : "No tickers configured";
    const totalLanes = symbols.length;
    const openLanes = Number(bot && bot.open_symbol_count);
    const laneSummary = Number.isFinite(openLanes)
      ? `${numberText(openLanes, 0)} open / ${numberText(totalLanes, 0)} lanes`
      : `${numberText(totalLanes, 0)} lanes`;
    const rawEntryPrice = bot && bot.position && bot.position.entry_price != null
      ? Number(bot.position.entry_price)
      : NaN;
    const entryDescriptor = Number.isFinite(rawEntryPrice) ? `entry ${numberText(rawEntryPrice, 4)}` : "entry n/a";
    const botPnl = bot && bot.pnl ? bot.pnl : null;
    const realizedPnlValue = Number(botPnl && botPnl.realized_usd);
    const unrealizedPnlValue = Number(botPnl && botPnl.unrealized_usd);
    const realizedPnlText = Number.isFinite(realizedPnlValue) ? signedCurrencyText(realizedPnlValue, 2) : "-";
    const unrealizedPnlText = Number.isFinite(unrealizedPnlValue) ? signedCurrencyText(unrealizedPnlValue, 2) : "-";
    const realizedPnlTone = realizedPnlValue > 0 ? "positive" : (realizedPnlValue < 0 ? "negative" : "neutral");
    const unrealizedPnlTone = unrealizedPnlValue > 0 ? "positive" : (unrealizedPnlValue < 0 ? "negative" : "neutral");
    const pnlClass = pnlDescriptor.tone === "ok" ? "positive" : (pnlDescriptor.tone === "error" ? "negative" : "neutral");
    const warmup = bot && bot.warmup ? bot.warmup : null;
    const warmupRequired = Number(warmup && warmup.required_bars);
    const warmupLoaded = Number(warmup && warmup.loaded_bars);
    const warmupProgress = warmup ? `${numberText(warmupLoaded, 0)} / ${numberText(warmupRequired, 0)}` : "Unavailable";
    const warmupTimestamp = warmup && warmup.last_warmup_timestamp ? whenFromValue(warmup.last_warmup_timestamp) : "No warmup timestamp";
    const warmupError = warmup && warmup.last_error ? compactText(warmup.last_error, 120) : "No warmup errors";
    const pollingSummary = botSummaryPolling(bot);
    const aggregateNotional = Number(bot && bot.aggregate_position_notional_usd);
    const expandLabel = isExpanded ? "Hide" : "Details";
    const detailRowId = botId ? `bot-row-detail-${botId.replace(/[^a-zA-Z0-9_-]/g, "-")}` : "";
    const row = document.createElement("tr");
    row.classList.add("bot-directory-main-row");
    row.dataset.instanceId = botId;
    if (botId === state.focusedBotId) {
      row.classList.add("active");
    }
    if (isExpanded) {
      row.classList.add("expanded");
    }

    row.innerHTML = `
      <td class="bot-directory-cell bot-bot-cell" data-label="Bot">
        <div class="bot-cell-title-row">
          <div class="bot-cell-title">${botId || "-"}</div>
          <span class="bot-mini-chip">${tickerBadges.countLabel}</span>
        </div>
        <div class="ticker-badge-list">${tickerBadges.markup}</div>
      </td>
      <td class="bot-directory-cell" data-label="Venue">
        <div class="bot-cell-title">${bot.market || "-"}</div>
        <p class="bot-cell-sub">acct ${bot.account || "-"} | tf ${bot.timeframe || "-"}</p>
        <p class="bot-cell-note">connector ${bot.connector || bot.data_connector || "-"}</p>
      </td>
      <td class="bot-directory-cell" data-label="Runtime">
        <div class="bot-cell-title">${bot.mode_banner || bot.execution_mode || "-"}</div>
        <div class="bot-chip-row">
          <span class="state-chip ${stateTone(bot.state)}">${titleText(bot.state || "unknown")}</span>
          <span class="state-chip ${bot.reconciliation_blocked ? "warn" : "ok"}">${bot.reconciliation_blocked ? "Blocked" : "Clear"}</span>
          <span class="bot-mini-chip">${bot.enabled ? "enabled" : "disabled"}</span>
        </div>
      </td>
      <td class="bot-directory-cell" data-label="Readiness">
        <div class="bot-chip-row bot-readiness-inline">
          <span class="bot-mini-chip">${warmupDescriptor.primary}</span>
          <span class="bot-mini-chip">${pollingDescriptor.primary}</span>
        </div>
      </td>
      <td class="bot-directory-cell" data-label="Position">
        <div class="bot-chip-row">
          <span class="state-chip ${positionDescriptor.tone}">${positionDescriptor.primary}</span>
          <span class="bot-mini-chip">${laneSummary}</span>
        </div>
      </td>
      <td class="bot-directory-cell" data-label="PnL">
        <div class="bot-pnl-primary ${pnlClass}">${pnlDescriptor.primary}</div>
        <div class="bot-chip-row bot-pnl-chip-row">
          <span class="bot-mini-chip bot-mini-chip-value ${realizedPnlTone}">R ${realizedPnlText}</span>
          <span class="bot-mini-chip bot-mini-chip-value ${unrealizedPnlTone}">U ${unrealizedPnlText}</span>
        </div>
      </td>
      <td class="bot-directory-cell bot-action-cell" data-label="Actions">
        <div class="bot-action-stack">
          <button type="button" class="instance-focus-button primary bot-open-button" data-instance-focus="${botId}">Open</button>
          <button type="button" class="instance-focus-button bot-detail-toggle" data-instance-expand="${botId}" aria-expanded="${isExpanded ? "true" : "false"}" ${detailRowId ? `aria-controls="${detailRowId}"` : ""}>${expandLabel}</button>
        </div>
      </td>
    `;
    elements.instances.body.appendChild(row);

    const detailRow = document.createElement("tr");
    detailRow.className = "bot-directory-detail-row";
    if (detailRowId) {
      detailRow.id = detailRowId;
    }
    detailRow.hidden = !isExpanded;
    detailRow.innerHTML = `
      <td class="bot-directory-detail-cell" colspan="${BOT_DIRECTORY_COLUMN_COUNT}">
        <div class="bot-row-detail-shell">
          <div class="bot-row-detail-grid">
            <article class="bot-detail-card">
              <h5>Tickers</h5>
              <div class="ticker-badge-list">${detailTickerBadges.markup}</div>
              <p class="bot-cell-note">${symbolsList}</p>
            </article>
            <article class="bot-detail-card">
              <h5>Venue</h5>
              <ul class="bot-detail-list">
                <li><span>Market</span><strong>${bot.market || "-"}</strong></li>
                <li><span>Account</span><strong>${bot.account || "-"}</strong></li>
                <li><span>Connector</span><strong>${bot.connector || bot.data_connector || "-"}</strong></li>
                <li><span>Timeframe</span><strong>${bot.timeframe || "-"}</strong></li>
              </ul>
            </article>
            <article class="bot-detail-card">
              <h5>Runtime</h5>
              <ul class="bot-detail-list">
                <li><span>Mode</span><strong>${bot.mode_banner || bot.execution_mode || "-"}</strong></li>
                <li><span>State</span><strong>${titleText(bot.state || "unknown")}</strong></li>
                <li><span>Safety</span><strong>${bot.reconciliation_blocked ? "Blocked" : "Clear"}</strong></li>
                <li><span>Eligibility</span><strong>${bot.enabled ? "Enabled" : "Disabled"}</strong></li>
              </ul>
            </article>
            <article class="bot-detail-card">
              <h5>Readiness</h5>
              <ul class="bot-detail-list">
                <li><span>Warmup</span><strong>${warmupDescriptor.primary}</strong></li>
                <li><span>Progress</span><strong>${warmupProgress}</strong></li>
                <li><span>Last Warmup</span><strong>${warmupTimestamp}</strong></li>
                <li><span>Polling</span><strong>${pollingSummary}</strong></li>
              </ul>
              <p class="bot-cell-note">${warmupError}</p>
            </article>
            <article class="bot-detail-card">
              <h5>Position</h5>
              <ul class="bot-detail-list">
                <li><span>Status</span><strong>${positionDescriptor.primary}</strong></li>
                <li><span>Entry</span><strong>${entryDescriptor}</strong></li>
                <li><span>Lanes</span><strong>${laneSummary}</strong></li>
                <li><span>Open Notional</span><strong>${currencyText(aggregateNotional, 2)}</strong></li>
              </ul>
              <p class="bot-cell-note">${positionDescriptor.secondary}</p>
            </article>
            <article class="bot-detail-card">
              <h5>P&amp;L</h5>
              <ul class="bot-detail-list">
                <li><span>Total</span><strong>${pnlDescriptor.primary}</strong></li>
                <li><span>Realized</span><strong>${realizedPnlText}</strong></li>
                <li><span>Unrealized</span><strong>${unrealizedPnlText}</strong></li>
              </ul>
              <p class="bot-cell-note">${pnlDescriptor.secondary}</p>
            </article>
          </div>
        </div>
      </td>
    `;
    elements.instances.body.appendChild(detailRow);
  }
}

function renderDifferenceList(differences) {
  elements.focus.differences.textContent = "";
  if (!Array.isArray(differences) || differences.length === 0) {
    const item = document.createElement("li");
    item.className = "difference-item";
    item.textContent = "No reconciliation differences in the latest visible check.";
    elements.focus.differences.appendChild(item);
    return;
  }
  for (const difference of differences) {
    const item = document.createElement("li");
    item.className = "difference-item";
    item.textContent = difference;
    elements.focus.differences.appendChild(item);
  }
}

function renderFocusedLaneList(summary) {
  const lanes = focusedLanes();
  const selectedLane = selectedFocusedLane();
  elements.focus.laneList.textContent = "";
  elements.focus.laneSelect.textContent = "";

  if (!summary || lanes.length === 0) {
    setBadge(elements.focus.lanePill, "No lane selected", "info");
    elements.focus.laneNote.textContent = "Open a bot to inspect its per-symbol lanes.";
    elements.focus.laneSelect.disabled = true;
    elements.focus.laneSelect.innerHTML = '<option value="">No symbol lanes</option>';
    elements.focus.laneList.innerHTML = '<div class="empty-state">Per-symbol lane state appears here after you open a bot.</div>';
    return;
  }

  for (const lane of lanes) {
    const option = document.createElement("option");
    option.value = lane.symbol;
    option.textContent = `${lane.symbol} | ${titleText(lane.state || "unknown")}`;
    if (selectedLane && lane.symbol === selectedLane.symbol) {
      option.selected = true;
    }
    elements.focus.laneSelect.appendChild(option);
  }
  elements.focus.laneSelect.disabled = false;

  const selectedSymbol = selectedLane ? selectedLane.symbol : "";
  setBadge(elements.focus.lanePill, selectedSymbol ? `Lane ${selectedSymbol}` : "Lane Unknown", selectedLane && selectedLane.reconciliation_blocked ? "warn" : "info");
  elements.focus.laneNote.textContent = selectedLane
    ? `${selectedLane.symbol} selected for tick, bar simulation, and manual-signal actions.`
    : `${numberText(lanes.length, 0)} lanes available.`;

  for (const lane of lanes) {
    const stream = streamStatusForSymbol(summary, lane.symbol);
    const button = document.createElement("button");
    button.type = "button";
    button.className = `lane-button ${lane.reconciliation_blocked ? "blocked" : ""}`.trim();
    if (selectedLane && lane.symbol === selectedLane.symbol) {
      button.classList.add("active");
    }
    button.dataset.laneSymbol = lane.symbol;
    const latestBarTime = stream && stream.latest_bar ? whenFromValue(stream.latest_bar.timestamp) : "No bar yet";
    const latestBarClose = stream && stream.latest_bar && Number.isFinite(Number(stream.latest_bar.close))
      ? numberText(Number(stream.latest_bar.close), 4)
      : "-";
    button.innerHTML = `
      <div class="lane-head">
        <div>
          <p class="lane-title">${lane.symbol || "-"}</p>
          <p class="lane-meta">${titleText(lane.state || "unknown")}</p>
        </div>
        <span class="state-chip ${lane.reconciliation_blocked ? "warn" : stateTone(lane.state)}">${lane.reconciliation_blocked ? "Blocked" : titleText(lane.state || "unknown")}</span>
      </div>
      <div class="lane-stat-grid">
        <div class="lane-stat"><span>Position</span><strong>${lane.has_position ? `Open | ${numberText(lane.quantity, 4)}` : "Flat"}</strong></div>
        <div class="lane-stat"><span>Entry</span><strong>${lane.entry_price != null ? numberText(Number(lane.entry_price), 4) : "-"}</strong></div>
        <div class="lane-stat"><span>Realized</span><strong>${signedCurrencyText(lane.realized_pnl_usd, 2)}</strong></div>
        <div class="lane-stat"><span>Warmup</span><strong>${lane.warmup && lane.warmup.ready ? "Ready" : `${numberText(lane.warmup && lane.warmup.loaded_bars, 0)} / ${numberText(lane.warmup && lane.warmup.required_bars, 0)}`}</strong></div>
        <div class="lane-stat"><span>Last Bar</span><strong>${latestBarClose}</strong></div>
        <div class="lane-stat"><span>Polling</span><strong>${latestBarTime}</strong></div>
      </div>
    `;
    elements.focus.laneList.appendChild(button);
  }
}

function renderFocusedLaneMarketData(summary) {
  if (!elements.focus.laneBufferBody || !elements.focus.laneHistoryBody) {
    return;
  }

  const lane = selectedFocusedLane();
  const stream = lane ? streamStatusForSymbol(summary, lane.symbol) : null;
  const marketData = state.focused.marketData;
  const historyLimit = lane
    ? focusedLaneHistoryLimit(summary, lane)
    : STREAM_HISTORY_LIMIT_DEFAULT;

  if (!summary || !lane) {
    if (elements.focus.laneBufferNote) {
      elements.focus.laneBufferNote.textContent = "Current Buffer (In-Memory)";
    }
    if (elements.focus.laneHistoryNote) {
      elements.focus.laneHistoryNote.textContent = "Connector History (On Demand)";
    }
    renderEmptyTableRow(elements.focus.laneBufferBody, 6, "Select a symbol lane to inspect market data.");
    renderEmptyTableRow(elements.focus.laneHistoryBody, 6, "Select a symbol lane to inspect market data.");
    return;
  }

  if (elements.focus.laneBufferNote) {
    elements.focus.laneBufferNote.textContent = `Current Buffer (In-Memory, up to ${numberText(STREAM_BARS_LIMIT, 0)} bars)`;
  }
  if (elements.focus.laneHistoryNote) {
    elements.focus.laneHistoryNote.textContent = `Connector History (On Demand, up to ${numberText(historyLimit, 0)} bars)`;
  }

  if (!stream || !stream.key) {
    renderEmptyTableRow(elements.focus.laneBufferBody, 6, "No dataplane stream is currently registered for this lane.");
    renderEmptyTableRow(elements.focus.laneHistoryBody, 6, "No dataplane stream is currently registered for this lane.");
    return;
  }

  const expectedKey = focusedLaneMarketDataKey(summary, stream);
  const isStaleSelection = marketData.key && expectedKey && marketData.key !== expectedKey;
  const bufferBars = isStaleSelection
    ? []
    : (Array.isArray(marketData.bufferBars) ? marketData.bufferBars : []);
  const historyBars = isStaleSelection
    ? []
    : (Array.isArray(marketData.historyBars) ? marketData.historyBars : []);
  const loadingBuffer = marketData.loadingBuffer || isStaleSelection;
  const loadingHistory = marketData.loadingHistory || isStaleSelection;

  if (loadingBuffer) {
    renderEmptyTableRow(elements.focus.laneBufferBody, 6, "Loading dataplane buffer...");
  } else if (bufferBars.length === 0) {
    renderEmptyTableRow(
      elements.focus.laneBufferBody,
      6,
      marketData.bufferError || "No bars in the selected lane buffer yet."
    );
  } else {
    renderOhlcvRows(elements.focus.laneBufferBody, bufferBars);
  }

  if (loadingHistory) {
    renderEmptyTableRow(elements.focus.laneHistoryBody, 6, "Loading connector history...");
  } else if (historyBars.length === 0) {
    renderEmptyTableRow(
      elements.focus.laneHistoryBody,
      6,
      marketData.historyError || "No connector history returned for this lane yet."
    );
  } else {
    renderOhlcvRows(elements.focus.laneHistoryBody, historyBars);
  }
}

function renderFocusedTimeline() {
  elements.focus.timeline.textContent = "";
  if (state.focused.timelineLoading) {
    renderEmptyList(elements.focus.timeline, "Loading focused timeline...");
    elements.focus.timelineCount.textContent = "loading";
    return;
  }
  const records = Array.isArray(state.focused.timeline) ? state.focused.timeline : [];
  elements.focus.timelineCount.textContent = `${numberText(records.length, 0)} / ${numberText(FOCUSED_TIMELINE_LIMIT, 0)}`;
  if (records.length === 0) {
    renderEmptyList(elements.focus.timeline, "Open a bot to load its timeline here.");
    return;
  }

  const sorted = [...records].sort(recordSortDesc);
  let line = sorted.length;
  for (const record of sorted) {
    const li = document.createElement("li");
    const kind = String(record && record.kind ? record.kind : "").toLowerCase();
    const payload = String(record && record.payload ? record.payload : "").toLowerCase();
    let tone = "";
    if (kind.includes("failed") || kind.includes("rejected") || payload.includes("error=")) {
      tone = "error";
    } else if (kind.includes("blocked") || kind.includes("paused") || kind.includes("degraded")) {
      tone = "warn";
    }
    li.className = `timeline-record ${tone}`.trim();

    const head = document.createElement("div");
    head.className = "timeline-head";
    const title = document.createElement("p");
    title.className = "timeline-kind";
    title.textContent = `${String(line).padStart(3, "0")}> ${record.scope || "runtime"} :: ${record.kind || "event"}`;
    const meta = document.createElement("span");
    meta.className = "timeline-meta";
    meta.textContent = whenText(record.created_at_ms);
    head.appendChild(title);
    head.appendChild(meta);

    const detail = document.createElement("p");
    detail.className = "timeline-payload";
    detail.textContent = compactText(record.payload, 220);

    li.appendChild(head);
    li.appendChild(detail);
    elements.focus.timeline.appendChild(li);
    line -= 1;
  }
}

function renderFocusedInstance() {
  const summary = state.focused.summary;
  const report = state.focused.report;
  elements.sidebarFocus.textContent = state.focusedBotId || "none";

  if (!summary) {
    elements.focus.id.textContent = "none";
    elements.focus.heroNote.textContent = "Portfolio-style performance and execution view for the selected bot.";
    elements.focus.state.textContent = "-";
    elements.focus.warmup.textContent = "-";
    elements.focus.warmupError.textContent = "-";
    elements.focus.mode.textContent = "-";
    elements.focus.account.textContent = "-";
    elements.focus.market.textContent = "-";
    elements.focus.tickers.textContent = "-";
    elements.focus.connector.textContent = "-";
    elements.focus.accountCap.textContent = "-";
    elements.focus.accountFree.textContent = "-";
    elements.focus.botAllocated.textContent = "-";
    elements.focus.botRemaining.textContent = "-";
    elements.focus.position.textContent = "-";
    elements.focus.entry.textContent = "-";
    setTonalValue(elements.focus.realizedPnl, "-", NaN);
    setTonalValue(elements.focus.unrealizedPnl, "-", NaN);
    setTonalValue(elements.focus.totalPnl, "-", NaN);
    elements.focus.markPrice.textContent = "-";
    elements.focus.openNotional.textContent = "-";
    elements.focus.closedTrades.textContent = "-";
    elements.focus.winRate.textContent = "-";
    setTonalValue(elements.focus.biggestWin, "-", NaN);
    setTonalValue(elements.focus.biggestLoss, "-", NaN);
    elements.focus.avgHold.textContent = "-";
    elements.focus.lastPoll.textContent = "-";
    elements.focus.lastEval.textContent = "-";
    elements.focus.lastIntent.textContent = "-";
    elements.focus.lastRisk.textContent = "-";
    elements.focus.lastOrder.textContent = "-";
    elements.focus.lastFill.textContent = "-";
    if (elements.focus.openTrace) {
      elements.focus.openTrace.disabled = true;
    }
    elements.focus.activePosition.innerHTML = `<div class="empty-state">Open a bot to see its active position card here.</div>`;
    elements.focus.tradesCount.textContent = "0 trades";
    elements.focus.tradesNote.textContent = "Derived from recent fills for this bot. Fees are not currently tracked by the runtime journal.";
    renderEmptyTableRow(elements.focus.tradesBody, 10, "Open a bot to inspect its recent trades.");
    elements.focus.checkAt.textContent = "-";
    elements.focus.safe.textContent = "-";
    elements.focus.reason.textContent = state.focused.loading
      ? "Loading bot context..."
      : "Open a bot from the bots table to load detail here.";
    setBadge(elements.focus.safePill, state.focused.loading ? "Loading..." : "No bot selected", "info");
    renderDifferenceList([]);
    renderFocusedLaneList(null);
    renderFocusedLaneMarketData(null);
    renderFocusedTimeline();
    updateSimulationTarget();
    renderNavigationContext();
    return;
  }

  const connector = state.connectorStatuses.find((entry) => entry && entry.account_id === summary.account);
  const symbolsLabel = botSymbolsLabel(summary);
  const selectedLane = selectedFocusedLane();
  const laneSymbol = selectedLane ? selectedLane.symbol : "";
  const laneReport = report && Array.isArray(report.lanes)
    ? report.lanes.find((entry) => entry && entry.symbol === laneSymbol) || null
    : null;
  const latest = laneReport || (report && report.latest ? report.latest : null);
  const latestIntent = latestRecordForBot(state.lastFeeds.intents, summary.id, laneSymbol);
  const latestRisk = latestRecordForBot(state.lastFeeds.risk, summary.id, laneSymbol);
  const latestOrder = latestRecordForBot(state.focused.feeds.orders, summary.id, laneSymbol);
  const latestFill = latestRecordForBot(state.focused.feeds.fills, summary.id, laneSymbol);
  const latestTraceRecord = latestFill || latestOrder || latestRisk || latestIntent || null;
  const latestOrderConnector = connectorKindForRecord(latestOrder);
  const latestFillConnector = connectorKindForRecord(latestFill);
  const warmup = summary.warmup || null;
  const position = botPositionDescriptor(summary);
  const positionEntry = selectedLane
    && selectedLane.entry_price != null
    && Number.isFinite(Number(selectedLane.entry_price))
    ? numberText(Number(selectedLane.entry_price), 4)
    : (summary.position
      && summary.position.entry_price != null
      && Number.isFinite(Number(summary.position.entry_price))
      ? numberText(Number(summary.position.entry_price), 4)
      : "-");
  const pnl = summary && summary.pnl ? summary.pnl : null;
  const ledgerContext = focusedLedgerContext(summary);
  const derivedTrades = deriveBotTrades(summary, state.focused.feeds.orders, state.focused.feeds.fills, laneSymbol);
  const accountCap = Number(ledgerContext.account && (ledgerContext.account.live_balance_usd ?? ledgerContext.account.effective_cap_usd));
  const accountFree = Number(ledgerContext.account && ledgerContext.account.tradeable_open_room_usd);
  const botAllocated = Number(ledgerContext.bot && ledgerContext.bot.allocated_usd);
  const botRemaining = Number(ledgerContext.bot && ledgerContext.bot.tradeable_open_room_usd);
  const botOpenNotional = Number(ledgerContext.bot && ledgerContext.bot.attributed_open_notional_usd);
  const sampledTrades = derivedTrades.trades;
  const tradeStats = derivedTrades.stats;
  const winRate = tradeStats.closedTrades > 0
    ? (tradeStats.winningTrades / tradeStats.closedTrades) * 100
    : NaN;
  const selectedStream = selectedLane ? streamStatusForSymbol(summary, selectedLane.symbol) : null;
  const laneMarkPrice = selectedStream && selectedStream.latest_bar && Number.isFinite(Number(selectedStream.latest_bar.close))
    ? `${numberText(Number(selectedStream.latest_bar.close), 4)} | ${whenFromValue(selectedStream.latest_bar.timestamp)}`
    : null;
  const markPrice = laneMarkPrice || (pnl && Number.isFinite(Number(pnl.mark_price))
    ? `${numberText(Number(pnl.mark_price), 4)}${pnl.mark_timestamp ? ` | ${whenFromValue(pnl.mark_timestamp)}` : ""}`
    : "-");

  elements.focus.id.textContent = summary.id || "-";
  elements.focus.heroNote.textContent = `${symbolsLabel} | ${summary.market || "market unknown"} | ${summary.account || "account unknown"} | analytics from persisted P&L plus recent fills.`;
  elements.focus.state.textContent = titleText(summary.state || "unknown");
  elements.focus.warmup.textContent = warmup
    ? `${warmup.ready ? "ready" : "pending"} | ${numberText(warmup.loaded_bars, 0)} / ${numberText(warmup.required_bars, 0)}${warmup.last_warmup_timestamp ? ` | ${whenFromValue(warmup.last_warmup_timestamp)}` : ""}`
    : "-";
  elements.focus.warmupError.textContent = warmup && warmup.last_error
    ? compactText(warmup.last_error, 96)
    : (warmup && warmup.ready ? "none" : "-");
  elements.focus.mode.textContent = summary.mode_banner || summary.execution_mode || "-";
  elements.focus.account.textContent = summary.account || "-";
  elements.focus.market.textContent = summary.market || "-";
  elements.focus.tickers.textContent = symbolsLabel;
  elements.focus.connector.textContent = connector ? `${connector.kind} | ${connector.mode_banner || connector.state}` : "unknown";
  elements.focus.accountCap.textContent = Number.isFinite(accountCap) ? currencyText(accountCap, 2) : "not available";
  elements.focus.accountFree.textContent = Number.isFinite(accountFree) ? currencyText(accountFree, 2) : "-";
  elements.focus.botAllocated.textContent = Number.isFinite(botAllocated) ? currencyText(botAllocated, 2) : "-";
  elements.focus.botRemaining.textContent = Number.isFinite(botRemaining) ? currencyText(botRemaining, 2) : "-";
  elements.focus.position.textContent = selectedLane
    ? `${selectedLane.has_position ? "Open" : "Flat"}${selectedLane.has_position ? ` | ${numberText(selectedLane.quantity, 4)}` : ""}`
    : position.primary;
  elements.focus.entry.textContent = positionEntry;
  setTonalValue(elements.focus.realizedPnl, pnl ? signedCurrencyText(pnl.realized_usd, 2) : "-", pnl ? pnl.realized_usd : NaN);
  setTonalValue(elements.focus.unrealizedPnl, pnl ? signedCurrencyText(pnl.unrealized_usd, 2) : "-", pnl ? pnl.unrealized_usd : NaN);
  setTonalValue(
    elements.focus.totalPnl,
    pnl ? signedCurrencyText(Number.isFinite(Number(pnl.total_usd)) ? pnl.total_usd : pnl.realized_usd, 2) : "-",
    pnl ? (Number.isFinite(Number(pnl.total_usd)) ? pnl.total_usd : pnl.realized_usd) : NaN
  );
  elements.focus.markPrice.textContent = markPrice;
  elements.focus.openNotional.textContent = Number.isFinite(botOpenNotional) ? currencyText(botOpenNotional, 2) : "-";
  elements.focus.closedTrades.textContent = numberText(tradeStats.closedTrades, 0);
  elements.focus.winRate.textContent = Number.isFinite(winRate) ? `${numberText(winRate, 1)}%` : "-";
  setTonalValue(elements.focus.biggestWin, Number.isFinite(tradeStats.biggestWin) ? signedCurrencyText(tradeStats.biggestWin, 2) : "-", tradeStats.biggestWin);
  setTonalValue(elements.focus.biggestLoss, Number.isFinite(tradeStats.biggestLoss) ? signedCurrencyText(tradeStats.biggestLoss, 2) : "-", tradeStats.biggestLoss);
  elements.focus.avgHold.textContent = Number.isFinite(tradeStats.averageHoldMs) ? holdDurationText(tradeStats.averageHoldMs) : "-";
  elements.focus.lastPoll.textContent = selectedLane ? lanePollingDescriptor(summary, selectedLane) : botSummaryPolling(summary);
  elements.focus.lastEval.textContent = latestIntent ? whenText(latestIntent.created_at_ms) : "-";
  elements.focus.lastIntent.textContent = latestIntent ? `${latestIntent.intent} | signal ${latestIntent.signal}` : "-";
  elements.focus.lastRisk.textContent = latestRisk ? `${latestRisk.decision}${latestRisk.reason ? ` | ${compactText(latestRisk.reason, 68)}` : ""}` : "-";
  elements.focus.lastOrder.textContent = latestOrder ? `${latestOrderConnector ? `${latestOrderConnector} | ` : ""}${latestOrder.status} | ${numberText(latestOrder.quantity, 4)} @ ${numberText(latestOrder.price, 4)}` : "-";
  elements.focus.lastFill.textContent = latestFill ? `${latestFillConnector ? `${latestFillConnector} | ` : ""}${numberText(latestFill.quantity, 4)} @ ${numberText(latestFill.price, 4)}` : "-";
  if (elements.focus.openTrace) {
    const latestTraceId = traceIdForRecord(latestTraceRecord);
    elements.focus.openTrace.disabled = !latestTraceId;
    elements.focus.openTrace.dataset.traceId = latestTraceId;
    elements.focus.openTrace.dataset.botId = summary.id || "";
    elements.focus.openTrace.dataset.symbol = laneSymbol || recordSymbol(latestTraceRecord);
  }
  renderActivePositionCard(summary, ledgerContext, derivedTrades);
  elements.focus.tradesNote.textContent = sampledTrades.length > 0
    ? `Showing latest ${numberText(Math.min(25, sampledTrades.length), 0)} closes derived from ${numberText(state.focused.feeds.fills.length, 0)} fills for ${laneSymbol || "this bot"}. Fees are not currently tracked by the runtime journal.`
    : "Derived from recent fills for this bot. Fees are not currently tracked by the runtime journal.";
  renderBotTradesTable(sampledTrades);
  elements.focus.checkAt.textContent = latest ? whenText(latest.created_at_ms) : "no checks";
  elements.focus.safe.textContent = latest ? (latest.safe_to_trade ? "safe" : "blocked") : "unknown";
  elements.focus.reason.textContent = state.focused.loading
      ? "Loading reconciliation detail and timeline..."
      : (latest ? compactText(latest.reason || "No reconciliation reason recorded.", 200) : "No reconciliation record exists for this bot yet.");
  if (state.focused.loading) {
    setBadge(elements.focus.safePill, "Loading...", "info");
  } else {
    setBadge(elements.focus.safePill, latest ? (latest.safe_to_trade ? "Safe To Trade" : "Blocked") : titleText(summary.state || "unknown"), latest ? (latest.safe_to_trade ? "ok" : "warn") : stateTone(summary.state));
  }
  renderDifferenceList(latest ? latest.differences || [] : []);
  renderFocusedLaneList(summary);
  renderFocusedLaneMarketData(summary);
  renderFocusedTimeline();
  updateSimulationTarget();
  renderNavigationContext();
}

function updateSimulationTarget() {
  const lane = selectedFocusedLane();
  const target = state.focusedBotId
    ? `${state.focusedBotId}${lane ? ` / ${lane.symbol}` : ""}`
    : "none";
  elements.simulation.target.textContent = target;
  const disabled = !state.focusedBotId || state.acting;
  elements.simulation.barPhase.disabled = disabled;
  elements.simulation.barTimestamp.disabled = disabled;
  elements.simulation.barOpen.disabled = disabled;
  elements.simulation.barHigh.disabled = disabled;
  elements.simulation.barLow.disabled = disabled;
  elements.simulation.barClose.disabled = disabled;
  elements.simulation.barVolume.disabled = disabled;
  elements.simulation.tradeTimestamp.disabled = disabled;
  elements.simulation.tradeSymbol.disabled = disabled;
  elements.simulation.tradePrice.disabled = disabled;
  elements.simulation.tradeQuantity.disabled = disabled;
  elements.simulation.barButton.disabled = disabled;
  elements.simulation.tradeButton.disabled = disabled;
  if (elements.simulation.manualSignal) {
    elements.simulation.manualSignal.disabled = disabled;
  }
  if (elements.simulation.manualTimestamp) {
    elements.simulation.manualTimestamp.disabled = disabled;
  }
  if (elements.simulation.manualPrice) {
    elements.simulation.manualPrice.disabled = disabled;
  }
  if (elements.simulation.manualButton) {
    elements.simulation.manualButton.disabled = disabled;
  }
  if (elements.simulation.tradeSymbol && lane) {
    const hasOverride = elements.simulation.tradeSymbol.dataset.manualOverride === "true";
    if (!elements.simulation.tradeSymbol.value.trim() || !hasOverride) {
      elements.simulation.tradeSymbol.value = lane.symbol;
      elements.simulation.tradeSymbol.dataset.manualOverride = "false";
    }
  }
  for (const button of elements.focus.actions.querySelectorAll("button")) {
    button.disabled = disabled;
  }
}

async function refreshFocusedInstanceReport(options = {}) {
  if (!state.focusedBotId) {
    state.focused.summary = null;
    state.focused.lanes = [];
    state.focused.report = null;
    state.focused.selectedLaneSymbol = null;
    state.focused.marketData = {
      key: null,
      historyLimit: null,
      loadingBuffer: false,
      loadingHistory: false,
      bufferBars: [],
      historyBars: [],
      bufferError: "Select a symbol lane to inspect market data.",
      historyError: "Select a symbol lane to inspect market data."
    };
    state.focused.feeds = {
      orders: [],
      fills: [],
      positions: []
    };
    state.focused.timeline = [];
    state.focused.timelineLoading = false;
    state.focused.loading = false;
    renderFocusedInstance();
    return;
  }

  const encodedId = encodeURIComponent(state.focusedBotId);
  const cachedSummary = state.botCache.find((instance) => instance && instance.id === state.focusedBotId);
  if (cachedSummary) {
    state.focused.summary = cachedSummary;
  }
  state.focused.loading = true;

  state.focused.timelineLoading = true;
  renderFocusedInstance();

  const snapshotResult = await requestOptionalJson(`/v1/bots/${encodedId}/snapshot`);

  if (!snapshotResult.ok || !snapshotResult.data || typeof snapshotResult.data !== "object") {
    state.focusedBotId = null;
    state.focused.summary = null;
    state.focused.lanes = [];
    state.focused.report = null;
    state.focused.selectedLaneSymbol = null;
    state.focused.marketData = {
      key: null,
      historyLimit: null,
      loadingBuffer: false,
      loadingHistory: false,
      bufferBars: [],
      historyBars: [],
      bufferError: "Open a bot to inspect lane market data.",
      historyError: "Open a bot to inspect lane market data."
    };
    state.focused.feeds = {
      orders: [],
      fills: [],
      positions: []
    };
    state.focused.timeline = [];
    state.focused.timelineLoading = false;
    state.focused.loading = false;
    setActivePage("bots");
    renderFocusedInstance();
    if (!options.preserveMessage) {
      setMessage(`Bot detail failed: ${snapshotResult.error || "bot unavailable"}`, "warn");
    }
    return;
  }

  const snapshot = snapshotResult.data;
  const detailPayload = snapshot.detail && typeof snapshot.detail === "object" ? snapshot.detail : {};
  const detailLanes = Array.isArray(detailPayload.lanes) ? detailPayload.lanes : [];
  state.focused.summary = { ...detailPayload };
  delete state.focused.summary.lanes;
  state.focused.lanes = detailLanes;
  state.focused.report = snapshot.report && typeof snapshot.report === "object" ? snapshot.report : null;
  state.focused.feeds = {
    orders: Array.isArray(snapshot.orders) ? snapshot.orders : [],
    fills: Array.isArray(snapshot.fills) ? snapshot.fills : [],
    positions: Array.isArray(snapshot.positions) ? snapshot.positions : []
  };
  state.focused.timeline = Array.isArray(snapshot.timeline) ? snapshot.timeline : [];
  state.focused.timelineLoading = false;
  state.focused.loading = false;
  syncFocusedLaneSelection();
  const marketDataPromise = refreshFocusedLaneMarketData();
  syncManualSignalPriceFromFocusedInstance();
  renderFocusedInstance();
  renderBots();
  await marketDataPromise;
  renderFocusedInstance();
}

function ensureAutoFocus() {
  if (state.focusedBotId) {
    return;
  }
  if (!state.botCache.length || state.hasAutoFocused) {
    return;
  }
  const preferred = state.botCache.find((instance) => instance && instance.state === "running") || state.botCache[0];
  if (preferred && preferred.id) {
    state.focusedBotId = preferred.id;
    state.hasAutoFocused = true;
  }
}

function describeRecord(kind, record) {
  const botId = recordBotId(record) || record.entity_id || "service";
  if (kind === "signals") {
    return {
      title: `${botId} :: ${record.signal}`,
      detail: `phase ${record.phase} | close ${numberText(record.close, 4)}`,
      meta: `${record.bar_timestamp || "bar time unknown"} | ${whenText(record.created_at_ms)}`,
      tone: ""
    };
  }
  if (kind === "intents") {
    return {
      title: `${botId} :: ${record.intent}`,
      detail: `signal ${record.signal} | has position before ${record.has_position_before}`,
      meta: `${record.bar_timestamp || "bar time unknown"} | ${whenText(record.created_at_ms)}`,
      tone: ""
    };
  }
  if (kind === "risk") {
    return {
      title: `${botId} :: ${record.decision}`,
      detail: record.reason ? `${record.intent} | ${record.reason}` : record.intent,
      meta: `${record.bar_timestamp || "bar time unknown"} | ${whenText(record.created_at_ms)}`,
      tone: record.decision === "rejected" ? "error" : ""
    };
  }
  if (kind === "orders") {
    const notional = Number(record.price) * Number(record.quantity);
    const connectorKind = connectorKindForRecord(record);
    return {
      title: `${botId} :: ${connectorKind ? `${connectorKind} ` : ""}${record.intent}`,
      detail: `${record.status} | qty ${numberText(record.quantity, 4)} @ ${numberText(record.price, 4)} | notional ${numberText(notional, 2)}`,
      meta: `${connectorKind ? `${connectorKind} | ` : ""}${compactText(record.client_order_id, 24)} | ${whenText(record.created_at_ms)}`,
      tone: ""
    };
  }
  if (kind === "fills") {
    const notional = Number(record.price) * Number(record.quantity);
    const connectorKind = connectorKindForRecord(record);
    return {
      title: `${botId} :: ${connectorKind ? `${connectorKind} ` : ""}fill ${compactText(record.client_order_id, 18)}`,
      detail: `qty ${numberText(record.quantity, 4)} @ ${numberText(record.price, 4)} | notional ${numberText(notional, 2)}`,
      meta: `${connectorKind ? `${connectorKind} | ` : ""}${whenText(record.created_at_ms)}`,
      tone: ""
    };
  }
  if (kind === "positions") {
    return {
      title: `${botId} :: ${record.has_position ? "position open" : "flat"}`,
      detail: compactText(record.reason || "no reason", 120),
      meta: whenText(record.created_at_ms),
      tone: ""
    };
  }
  if (kind === "reconciliations") {
    return {
      title: `${botId} :: ${record.safe_to_trade ? "safe" : "blocked"}`,
      detail: `${record.source} | local orders ${record.local_open_orders} | connector orders ${record.connector_open_orders}`,
      meta: `${compactText(record.reason, 120)} | ${whenText(record.created_at_ms)}`,
      tone: record.safe_to_trade ? "" : "warn"
    };
  }
  const parsedPayload = safeJsonParse(record.payload);
  if (parsedPayload && record.kind === "order.submitted") {
    return {
      title: `${record.entity_id || "service"} :: ${(parsedPayload.connector_kind || "connector")} order submitted`,
      detail: `${parsedPayload.intent || "intent unknown"} | ${(parsedPayload.symbol || "symbol unknown")} | qty ${numberText(parsedPayload.quantity, 4)} @ ${numberText(parsedPayload.price, 4)}`,
      meta: `${compactText(parsedPayload.client_order_id, 24)} | ${parsedPayload.account_id || "account unknown"} | ${whenText(record.created_at_ms)}`,
      tone: ""
    };
  }
  if (parsedPayload && record.kind === "order.filled") {
    return {
      title: `${record.entity_id || "service"} :: ${(parsedPayload.connector_kind || "connector")} fill`,
      detail: `${parsedPayload.symbol || "symbol unknown"} | qty ${numberText(parsedPayload.quantity, 4)} @ ${numberText(parsedPayload.price, 4)}`,
      meta: `${compactText(parsedPayload.client_order_id, 24)} | ${parsedPayload.account_id || "account unknown"} | ${whenText(record.created_at_ms)}`,
      tone: ""
    };
  }
  return {
    title: `${record.scope} :: ${record.kind}`,
    detail: compactText(record.payload, 160),
    meta: `${record.entity_id || "service"} | ${whenText(record.created_at_ms)}`,
    tone: ""
  };
}

function firstNonEmptyString(...values) {
  for (const value of values) {
    if (value === undefined || value === null) {
      continue;
    }
    const text = String(value).trim();
    if (text) {
      return text;
    }
  }
  return "";
}

function activityPayload(record) {
  const parsed = safeJsonParse(record && record.payload);
  return parsed && typeof parsed === "object" ? parsed : null;
}

function activityContext(kind, record, descriptor) {
  const payload = kind === "events" ? activityPayload(record) : null;
  const owner = payload && payload.owner && typeof payload.owner === "object"
    ? payload.owner
    : null;
  const connector = kind === "events"
    ? firstNonEmptyString(payload && payload.connector_kind)
    : connectorKindForRecord(record);
  const botId = kind === "events"
    ? firstNonEmptyString(
      payload && payload.bot_id,
      payload && payload.instance_id,
      owner && owner.bot_id,
      record && record.entity_id
    )
    : recordBotId(record);
  const symbolRaw = kind === "events"
    ? firstNonEmptyString(
      payload && payload.symbol,
      payload && payload.lane_symbol,
      owner && owner.symbol
    )
    : recordSymbol(record);
  const symbol = symbolRaw ? symbolRaw.toUpperCase() : "";
  const accountId = kind === "events"
    ? firstNonEmptyString(payload && payload.account_id, owner && owner.account_id)
    : "";

  let origin;
  if (kind === "events") {
    origin = firstNonEmptyString(payload && payload.source, connector, record && record.scope);
  } else if (kind === "reconciliations") {
    origin = firstNonEmptyString(record && record.source, "runtime");
  } else if (kind === "orders" || kind === "fills") {
    origin = firstNonEmptyString(connector, "execution");
  } else {
    origin = "runtime";
  }

  const recordKind = kind === "events"
    ? firstNonEmptyString(record && record.kind, record && record.scope, "event")
    : firstNonEmptyString(
      record && record.intent,
      record && record.signal,
      record && record.decision,
      record && record.status,
      record && record.reason,
      kind
    );

  const searchText = [
    descriptor ? descriptor.title : "",
    descriptor ? descriptor.detail : "",
    descriptor ? descriptor.meta : "",
    botId,
    symbol,
    accountId,
    connector,
    origin,
    recordKind,
    record && record.kind,
    record && record.scope,
    record && record.reason,
    record && record.client_order_id,
    record && record.payload
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();

  return {
    botId,
    symbol,
    accountId,
    connector,
    origin,
    recordKind,
    searchText
  };
}

function uniqueSortedValues(values) {
  return [...new Set(values.filter(Boolean))].sort((left, right) => left.localeCompare(right));
}

function syncActivitySelectOptions(selectElement, values, allLabel, selectedValue) {
  if (!selectElement) {
    return "";
  }

  const options = uniqueSortedValues(values);
  const resolvedValue = options.includes(selectedValue) ? selectedValue : "";
  selectElement.textContent = "";

  const allOption = document.createElement("option");
  allOption.value = "";
  allOption.textContent = allLabel;
  selectElement.appendChild(allOption);

  for (const value of options) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = value;
    selectElement.appendChild(option);
  }

  selectElement.value = resolvedValue;
  return resolvedValue;
}

function activityFiltersSummary(totalCount, visibleCount) {
  const active = [];
  const windowSize = numberText(state.feedLimit, 0);
  const query = String(state.activityFilters.search || "").trim();
  if (query) {
    active.push(`search \"${query}\"`);
  }
  if (state.activityFilters.botId) {
    active.push(`bot ${state.activityFilters.botId}`);
  }
  if (state.activityFilters.symbol) {
    active.push(`ticker ${state.activityFilters.symbol}`);
  }
  if (state.activityFilters.origin) {
    active.push(`origin ${state.activityFilters.origin}`);
  }
  if (!active.length) {
    return `Showing all ${numberText(totalCount, 0)} records in the selected stream (window ${windowSize}).`;
  }
  return `Showing ${numberText(visibleCount, 0)} of ${numberText(totalCount, 0)} records (window ${windowSize}) filtered by ${active.join(", ")}.`;
}

function resetActivitySelectionDetails() {
  if (!elements.activity.selectedStream) {
    return;
  }
  if (elements.activity.openTrace) {
    elements.activity.openTrace.disabled = true;
    elements.activity.openTrace.dataset.traceId = "";
    elements.activity.openTrace.dataset.botId = "";
    elements.activity.openTrace.dataset.symbol = "";
  }
  elements.activity.selectedStream.textContent = "-";
  elements.activity.selectedBot.textContent = "-";
  elements.activity.selectedSymbol.textContent = "-";
  elements.activity.selectedOrigin.textContent = "-";
  elements.activity.selectedKind.textContent = "-";
  elements.activity.selectedAccount.textContent = "-";
}

function currentActivityRecords() {
  const records = Array.isArray(state.lastFeeds[state.selectedActivityKind]) ? state.lastFeeds[state.selectedActivityKind] : [];
  return [...records].sort(recordSortDesc);
}

function renderActivityWorkspace() {
  for (const tabButton of elements.activity.tabs) {
    const stream = tabButton.dataset.streamTab;
    tabButton.classList.toggle("active", stream === state.selectedActivityKind);
    const countTarget = document.getElementById(`${stream}-count`);
    if (!countTarget) {
      continue;
    }
    const count = Array.isArray(state.lastFeeds[stream]) ? state.lastFeeds[stream].length : 0;
    countTarget.textContent = numberText(count, 0);
  }

  const currentMeta = STREAM_META[state.selectedActivityKind];
  state.feedLimit = normalizeFeedLimit(state.feedLimit);

  if (elements.activity.feedLimit && elements.activity.feedLimit.value !== String(state.feedLimit)) {
    elements.activity.feedLimit.value = String(state.feedLimit);
  }

  const records = currentActivityRecords();
  const decorated = records.map((record, index) => {
    const key = recordIdentity(state.selectedActivityKind, record, index);
    const descriptor = describeRecord(state.selectedActivityKind, record);
    const context = activityContext(state.selectedActivityKind, record, descriptor);
    return { key, record, descriptor, context };
  });

  if (elements.activity.search && elements.activity.search.value !== state.activityFilters.search) {
    elements.activity.search.value = state.activityFilters.search;
  }

  state.activityFilters.botId = syncActivitySelectOptions(
    elements.activity.botFilter,
    decorated.map((entry) => entry.context.botId),
    "All bots",
    state.activityFilters.botId
  );
  state.activityFilters.symbol = syncActivitySelectOptions(
    elements.activity.symbolFilter,
    decorated.map((entry) => entry.context.symbol),
    "All tickers",
    state.activityFilters.symbol
  );
  state.activityFilters.origin = syncActivitySelectOptions(
    elements.activity.originFilter,
    decorated.map((entry) => entry.context.origin),
    "All origins",
    state.activityFilters.origin
  );

  const searchQuery = String(state.activityFilters.search || "").trim().toLowerCase();
  const filtered = decorated.filter((entry) => {
    if (state.activityFilters.botId && entry.context.botId !== state.activityFilters.botId) {
      return false;
    }
    if (state.activityFilters.symbol && entry.context.symbol !== state.activityFilters.symbol) {
      return false;
    }
    if (state.activityFilters.origin && entry.context.origin !== state.activityFilters.origin) {
      return false;
    }
    if (searchQuery && !entry.context.searchText.includes(searchQuery)) {
      return false;
    }
    return true;
  });

  elements.activity.listTitle.textContent = currentMeta.label;
  elements.activity.listNote.textContent = currentMeta.note;
  elements.activity.listCount.textContent = `${numberText(filtered.length, 0)} of ${numberText(records.length, 0)} records (window ${numberText(state.feedLimit, 0)})`;
  if (elements.activity.filterNote) {
    elements.activity.filterNote.textContent = activityFiltersSummary(records.length, filtered.length);
  }

  renderBarList(elements.activity.summaryChart, activityTabsFromState());

  const focusedHits = state.focusedBotId
    ? filtered.filter((entry) => entry.context.botId === state.focusedBotId).length
    : 0;
  elements.activity.streamLabel.textContent = currentMeta.label;
  elements.activity.visibleCount.textContent = numberText(filtered.length, 0);
  elements.activity.latestTime.textContent = filtered.length > 0 ? whenText(filtered[0].record.created_at_ms) : "-";
  elements.activity.focusedCount.textContent = numberText(focusedHits, 0);

  const filteredRecords = filtered.map((entry) => entry.record);
  state.selectedActivityKey = ensureSelectionKey(state.selectedActivityKey, filteredRecords, state.selectedActivityKind);

  elements.activity.list.textContent = "";
  if (filtered.length === 0) {
    const emptyMessage = records.length === 0
      ? `No ${currentMeta.label.toLowerCase()} records yet.`
      : "No records match the current activity filters.";
    renderEmptyList(elements.activity.list, emptyMessage);
  } else {
    for (const entry of filtered) {
      const { key, descriptor, context } = entry;
      const li = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      button.className = `record-button ${descriptor.tone || ""}`.trim();
      if (key === state.selectedActivityKey) {
        button.classList.add("active");
      }
      button.dataset.activityKey = key;
      const title = document.createElement("p");
      title.className = "record-title";
      title.textContent = descriptor.title;
      const detail = document.createElement("p");
      detail.className = "record-detail";
      detail.textContent = descriptor.detail;
      const meta = document.createElement("p");
      meta.className = "record-meta";
      meta.textContent = descriptor.meta;
      button.appendChild(title);
      button.appendChild(detail);
      button.appendChild(meta);

      const tags = [
        context.botId ? `bot:${context.botId}` : "",
        context.symbol ? `ticker:${context.symbol}` : "",
        context.origin ? `origin:${context.origin}` : "",
        context.accountId ? `account:${context.accountId}` : ""
      ].filter(Boolean);
      if (tags.length > 0) {
        const tagsWrap = document.createElement("div");
        tagsWrap.className = "record-tags";
        for (const tagText of tags) {
          const tag = document.createElement("span");
          tag.className = "record-tag";
          tag.textContent = tagText;
          tagsWrap.appendChild(tag);
        }
        button.appendChild(tagsWrap);
      }

      li.appendChild(button);
      elements.activity.list.appendChild(li);
    }
  }

  const selected = filtered.find((entry) => entry.key === state.selectedActivityKey) || null;
  if (!selected) {
    elements.activity.selectedTitle.textContent = "Selected Record";
    elements.activity.selectedNote.textContent = "Choose a record on the left to load contextual detail.";
    setBadge(elements.activity.selectedMeta, "No selection", "info");
    resetActivitySelectionDetails();
    elements.activity.terminal.textContent = "Select a record from the active stream to inspect it here.";
    return;
  }

  const descriptor = selected.descriptor;
  const context = selected.context;
  elements.activity.selectedTitle.textContent = descriptor.title;
  elements.activity.selectedNote.textContent = descriptor.detail;
  setBadge(elements.activity.selectedMeta, descriptor.meta, descriptor.tone || "info");
  if (elements.activity.selectedStream) {
    elements.activity.selectedStream.textContent = currentMeta.label;
    elements.activity.selectedBot.textContent = context.botId || "-";
    elements.activity.selectedSymbol.textContent = context.symbol || "-";
    elements.activity.selectedOrigin.textContent = context.origin || "-";
    elements.activity.selectedKind.textContent = context.recordKind || "-";
    elements.activity.selectedAccount.textContent = context.accountId || "-";
  }
  if (elements.activity.openTrace) {
    const traceId = traceIdForRecord(selected.record);
    elements.activity.openTrace.disabled = !traceId;
    elements.activity.openTrace.dataset.traceId = traceId;
    elements.activity.openTrace.dataset.botId = context.botId || "";
    elements.activity.openTrace.dataset.symbol = context.symbol || "";
  }
  elements.activity.terminal.textContent = formatJson(activityInspectorRecord(state.selectedActivityKind, selected.record));
}

function providerEventPayload(record) {
  const parsed = safeJsonParse(record && record.payload);
  return parsed && typeof parsed === "object" ? parsed : null;
}

function providerEventStage(record) {
  const payload = providerEventPayload(record);
  if (payload && payload.stage) {
    return String(payload.stage);
  }
  const kind = String(record && record.kind ? record.kind : "");
  if (kind.endsWith(".requested") || kind.endsWith(".received")) {
    return kind.split(".").pop();
  }
  if (kind.endsWith(".succeeded") || kind.endsWith(".failed") || kind.endsWith(".normalized") || kind.endsWith(".ignored")) {
    return kind.split(".").pop();
  }
  return "unknown";
}

function providerEventOperation(record) {
  const payload = providerEventPayload(record);
  if (payload && payload.operation) {
    return String(payload.operation);
  }
  const kind = String(record && record.kind ? record.kind : "");
  if (kind.includes("market_stream")) {
    return "market_stream";
  }
  if (kind.includes("account_snapshot")) {
    return "fetch_account_snapshot";
  }
  if (kind.includes("symbol_constraints")) {
    return "fetch_symbol_constraints";
  }
  if (kind.includes("order_submission")) {
    return "submit_order";
  }
  if (kind.includes("market_data.latest")) {
    return "fetch_latest_bar";
  }
  if (kind.includes("market_data.history")) {
    return "fetch_recent_bars";
  }
  return "provider_event";
}

function providerEventTone(record) {
  const stage = providerEventStage(record);
  if (stage === "failed") {
    return "error";
  }
  if (stage === "ignored") {
    return "warn";
  }
  if (stage === "requested" || stage === "received") {
    return "info";
  }
  return "ok";
}

function describeProviderEvent(record) {
  const payload = providerEventPayload(record);
  const connector = payload && payload.connector_kind ? String(payload.connector_kind) : "connector";
  const operation = providerEventOperation(record);
  const stage = providerEventStage(record);
  const summary = payload && payload.summary
    ? String(payload.summary)
    : compactText(record && record.payload ? record.payload : "", 180);
  const account = payload && payload.account_id ? String(payload.account_id) : (record && record.entity_id ? String(record.entity_id) : "service");
  return {
    title: `${connector} :: ${titleText(operation)} :: ${titleText(stage)}`,
    detail: summary,
    meta: `${account} | ${whenText(record && record.created_at_ms)}`,
    tone: providerEventTone(record)
  };
}

function renderProviderWorkspace() {
  const records = Array.isArray(state.providerEvents) ? [...state.providerEvents].sort(recordSortDesc) : [];
  const requested = records.filter((record) => {
    const stage = providerEventStage(record);
    return stage === "requested" || stage === "received";
  });
  const succeeded = records.filter((record) => {
    const stage = providerEventStage(record);
    return stage === "succeeded" || stage === "normalized";
  });
  const failed = records.filter((record) => providerEventStage(record) === "failed");
  const connectors = new Set(
    records
      .map((record) => {
        const payload = providerEventPayload(record);
        return payload && payload.connector_kind ? String(payload.connector_kind) : "";
      })
      .filter(Boolean)
  );

  const operationCounts = new Map();
  for (const record of records) {
    const operation = providerEventOperation(record);
    operationCounts.set(operation, (operationCounts.get(operation) || 0) + 1);
  }

  elements.provider.total.textContent = numberText(records.length, 0);
  elements.provider.requested.textContent = numberText(requested.length, 0);
  elements.provider.succeeded.textContent = numberText(succeeded.length, 0);
  elements.provider.failed.textContent = numberText(failed.length, 0);
  elements.provider.connectors.textContent = numberText(connectors.size, 0);
  renderBarList(
    elements.provider.summaryChart,
    [...operationCounts.entries()]
      .map(([operation, count]) => ({
        label: operation,
        value: count,
        note: `${numberText(count, 0)} event(s)`,
        tone: operation === "submit_order" ? "ok" : "info"
      }))
      .sort((left, right) => Number(right.value) - Number(left.value))
  );

  elements.provider.list.textContent = "";
  elements.provider.listCount.textContent = `${numberText(records.length, 0)} records`;
  if (records.length === 0) {
    renderEmptyList(elements.provider.list, "No provider request logs have been recorded yet.");
  } else {
    state.selectedProviderKey = ensureSelectionKey(state.selectedProviderKey, records, "provider");
    for (const record of records) {
      const key = recordIdentity("provider", record);
      const descriptor = describeProviderEvent(record);
      const li = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      button.className = `record-button ${descriptor.tone || ""}`.trim();
      if (key === state.selectedProviderKey) {
        button.classList.add("active");
      }
      button.dataset.providerKey = key;
      const title = document.createElement("p");
      title.className = "record-title";
      title.textContent = descriptor.title;
      const detail = document.createElement("p");
      detail.className = "record-detail";
      detail.textContent = descriptor.detail;
      const meta = document.createElement("p");
      meta.className = "record-meta";
      meta.textContent = descriptor.meta;
      button.appendChild(title);
      button.appendChild(detail);
      button.appendChild(meta);
      li.appendChild(button);
      elements.provider.list.appendChild(li);
    }
  }

  const selected = records.find((record) => recordIdentity("provider", record) === state.selectedProviderKey) || null;
  if (!selected) {
    elements.provider.selectedTitle.textContent = "Selected Provider Event";
    elements.provider.selectedNote.textContent = "Choose a provider event on the left to inspect the normalized request and response payload here.";
    setBadge(elements.provider.selectedMeta, "No selection", "info");
    elements.provider.selectedOperation.textContent = "-";
    elements.provider.selectedStage.textContent = "-";
    elements.provider.selectedConnector.textContent = "-";
    elements.provider.selectedAccount.textContent = "-";
    elements.provider.selectedEntity.textContent = "-";
    elements.provider.selectedCreated.textContent = "-";
    elements.provider.terminal.textContent = "Select a provider event to inspect it here.";
    return;
  }

  const descriptor = describeProviderEvent(selected);
  const payload = providerEventPayload(selected);
  elements.provider.selectedTitle.textContent = descriptor.title;
  elements.provider.selectedNote.textContent = descriptor.detail;
  setBadge(elements.provider.selectedMeta, titleText(providerEventStage(selected)), descriptor.tone || "info");
  elements.provider.selectedOperation.textContent = titleText(providerEventOperation(selected));
  elements.provider.selectedStage.textContent = titleText(providerEventStage(selected));
  elements.provider.selectedConnector.textContent = payload && payload.connector_kind ? String(payload.connector_kind) : "-";
  elements.provider.selectedAccount.textContent = payload && payload.account_id ? String(payload.account_id) : "-";
  elements.provider.selectedEntity.textContent = selected.entity_id || "service";
  elements.provider.selectedCreated.textContent = whenText(selected.created_at_ms);
  elements.provider.terminal.textContent = payload ? formatJson(payload) : formatJson(selected);
}

function activityTabsFromState() {
  return ACTIVITY_TAB_IDS.map((tabId) => ({
    label: tabId,
    value: Array.isArray(state.lastFeeds[tabId]) ? state.lastFeeds[tabId].length : 0,
    note: STREAM_META[tabId].label,
    tone: tabId === state.selectedActivityKind ? "ok" : "info"
  }));
}

function ensureSelectionKey(currentKey, records, kind) {
  const ids = records.map((record) => recordIdentity(kind, record));
  if (currentKey && ids.includes(currentKey)) {
    return currentKey;
  }
  return ids[0] || null;
}

function connectorResilienceSummary(connector) {
  const resilience = connector && connector.resilience_state ? connector.resilience_state : {};
  const consecutiveFailures = Number(resilience.consecutive_failures);
  const parts = [`failures ${numberText(consecutiveFailures, 0)}`];
  if (resilience.next_reconnect_at_ms) {
    parts.push(`next ${whenFromValue(resilience.next_reconnect_at_ms)}`);
  } else {
    parts.push("next clear");
  }
  if (resilience.throttled_until_ms) {
    parts.push(`throttle ${whenFromValue(resilience.throttled_until_ms)}`);
  } else {
    parts.push("throttle clear");
  }
  return parts.join(" | ");
}

function renderConnectorWorkspace() {
  const connectors = Array.isArray(state.connectorStatuses) ? state.connectorStatuses : [];
  const connected = connectors.filter((entry) => entry && entry.state === "connected");
  const degraded = connectors.filter((entry) => entry && entry.state === "degraded");
  const down = connectors.filter((entry) => entry && entry.state === "disconnected");
  const resilienceWindows = connectors.filter((entry) => {
    if (!entry || !entry.resilience_state) {
      return false;
    }
    return Boolean(entry.resilience_state.next_reconnect_at_ms || entry.resilience_state.throttled_until_ms);
  });
  const kinds = new Set(connectors.map((entry) => entry && entry.kind).filter(Boolean));

  elements.connectors.total.textContent = numberText(connectors.length, 0);
  elements.connectors.up.textContent = numberText(connected.length, 0);
  elements.connectors.degraded.textContent = numberText(degraded.length, 0);
  elements.connectors.down.textContent = numberText(down.length, 0);
  elements.connectors.windows.textContent = numberText(resilienceWindows.length, 0);
  elements.connectors.kinds.textContent = numberText(kinds.size, 0);

  renderBarList(elements.connectors.stateChart, [
    { label: "Connected", value: connected.length, tone: "ok", note: "Healthy adapters." },
    { label: "Degraded", value: degraded.length, tone: "warn", note: "Degraded adapters." },
    { label: "Disconnected", value: down.length, tone: "error", note: "Unavailable adapters." },
    { label: "Resilience Windows", value: resilienceWindows.length, tone: "warn", note: "Reconnect or throttle windows." }
  ]);

  elements.connectors.body.textContent = "";
  if (connectors.length === 0) {
    renderEmptyTableRow(elements.connectors.body, 6, "No connector runtime status is available.");
  } else {
    if (!state.selectedConnectorAccountId || !connectors.some((connector) => connector && connector.account_id === state.selectedConnectorAccountId)) {
      state.selectedConnectorAccountId = connectors[0].account_id;
    }
    for (const connector of connectors) {
      const resiliencePrimary = `${numberText(connector && connector.resilience_state ? connector.resilience_state.consecutive_failures : 0, 0)} failures`;
      const resilienceSecondary = connectorResilienceSummary(connector);
      const row = document.createElement("tr");
      row.dataset.connectorAccountId = connector.account_id || "";
      if (connector.account_id === state.selectedConnectorAccountId) {
        row.classList.add("active");
      }
      row.innerHTML = `
        <td><div class="table-primary">${connector.account_id || "-"}</div></td>
        <td><div class="table-primary">${connector.kind || "-"}</div></td>
        <td><div class="table-primary">${connector.mode_banner || connector.mode || "-"}</div></td>
        <td><span class="state-chip ${stateTone(connector.state)}">${titleText(connector.state || "unknown")}</span></td>
        <td>
          <div class="table-primary">${resiliencePrimary}</div>
          <div class="table-secondary">${compactText(resilienceSecondary, 68)}</div>
        </td>
        <td><div class="table-primary">${compactText(connector.message || "-", 72)}</div></td>
      `;
      elements.connectors.body.appendChild(row);
    }
  }

  elements.connectors.matrixGrid.textContent = "";
  if (!Array.isArray(state.connectorMatrix) || state.connectorMatrix.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state";
    empty.textContent = "No connector matrix entries available.";
    elements.connectors.matrixGrid.appendChild(empty);
  } else {
    for (const entry of state.connectorMatrix) {
      const card = document.createElement("article");
      card.className = "matrix-card";
      const title = document.createElement("h4");
      title.textContent = `${entry.kind || "unknown"} | ${entry.role || "role unknown"}`;
      const modes = document.createElement("p");
      modes.textContent = `paper ${entry.supports_paper ? "yes" : "no"} | live ${entry.supports_live ? "yes" : "no"} | demo ${entry.supports_demo ? "yes" : "no"}`;
      const resilience = entry && entry.resilience ? entry.resilience : {};
      const envelope = document.createElement("p");
      envelope.textContent = `backoff ${numberText(resilience.reconnect_base_backoff_ms, 0)}-${numberText(resilience.reconnect_max_backoff_ms, 0)}ms | jitter ${numberText(resilience.reconnect_jitter_bps, 0)}bps`;
      const policy = document.createElement("p");
      policy.textContent = `keepalive ${resilience.requires_ping_pong_keepalive ? "required" : "optional"} | single-key ${resilience.single_connection_per_api_key ? "yes" : "no"}`;
      card.appendChild(title);
      card.appendChild(modes);
      card.appendChild(envelope);
      card.appendChild(policy);
      elements.connectors.matrixGrid.appendChild(card);
    }
  }

  renderConnectorInspector();
}

function renderConnectorInspector() {
  const selected = state.connectorStatuses.find((entry) => entry && entry.account_id === state.selectedConnectorAccountId) || null;
  if (!selected) {
    setBadge(elements.connectors.selectedPill, "No selection", "info");
    elements.connectors.selectedId.textContent = "-";
    elements.connectors.selectedState.textContent = "-";
    elements.connectors.selectedMode.textContent = "-";
    elements.connectors.selectedResilience.textContent = "-";
    elements.connectors.selectedMessage.textContent = "-";
    elements.connectors.selectedMatrix.textContent = "";
    elements.connectors.terminal.textContent = "Select a connector row to inspect it here.";
    return;
  }

  const matches = state.connectorMatrix.filter((entry) => entry && entry.kind === selected.kind);
  setBadge(elements.connectors.selectedPill, selected.account_id || "connector", stateTone(selected.state));
  elements.connectors.selectedId.textContent = selected.account_id || "-";
  elements.connectors.selectedState.textContent = titleText(selected.state || "unknown");
  elements.connectors.selectedMode.textContent = selected.mode_banner || selected.mode || "-";
  elements.connectors.selectedResilience.textContent = connectorResilienceSummary(selected);
  elements.connectors.selectedMessage.textContent = selected.message || "-";
  elements.connectors.selectedMatrix.textContent = "";
  if (matches.length === 0) {
    const empty = document.createElement("span");
    empty.className = "selection-chip";
    empty.textContent = "No matching matrix entries.";
    elements.connectors.selectedMatrix.appendChild(empty);
  } else {
    for (const entry of matches) {
      const chip = document.createElement("span");
      chip.className = "selection-chip";
      chip.textContent = `${entry.role || "role"} | paper ${entry.supports_paper ? "yes" : "no"} | live ${entry.supports_live ? "yes" : "no"}`;
      elements.connectors.selectedMatrix.appendChild(chip);
    }
  }
  elements.connectors.terminal.textContent = formatJson({ connector: selected, matrix_matches: matches });
}

function renderConfigWorkspace() {
  const configResult = state.payloads.config;
  const routes = state.openapiRoutes;

  elements.config.riskList.textContent = "";
  elements.config.accountsBody.textContent = "";
  elements.config.instancesBody.textContent = "";
  elements.config.apiSurfaceBody.textContent = "";
  elements.config.routesBody.textContent = "";
  elements.config.routesCount.textContent = `${numberText(routes.length, 0)} routes`;

  const apiRows = [
    { label: "/healthz", status: state.payloads.health.ok ? "ok" : "error", detail: state.payloads.health.ok ? formatJson(state.payloads.health.data) : state.payloads.health.error },
    { label: "/readyz", status: state.payloads.ready.ok ? "ok" : "error", detail: state.payloads.ready.ok ? formatJson(state.payloads.ready.data) : state.payloads.ready.error },
    { label: "/metrics", status: state.payloads.metrics.ok ? "ok" : "warn", detail: state.payloads.metrics.ok ? `${numberText(Object.keys(state.parsedMetrics).length, 0)} series visible` : state.payloads.metrics.error },
    { label: "/openapi.json", status: state.payloads.openapi.ok ? "ok" : "warn", detail: state.payloads.openapi.ok ? `${numberText(routes.length, 0)} routes flattened` : state.payloads.openapi.error }
  ];

  for (const row of apiRows) {
    const tr = document.createElement("tr");
    tr.innerHTML = `
      <td><div class="table-primary">${row.label}</div></td>
      <td><span class="state-chip ${stateTone(row.status)}">${titleText(row.status)}</span></td>
      <td><div class="table-primary">${compactText(row.detail, 140)}</div></td>
    `;
    elements.config.apiSurfaceBody.appendChild(tr);
  }

  if (!configResult.ok || !configResult.data) {
    renderEmptyTableRow(elements.config.accountsBody, 5, "Managed config is unavailable for this runtime.");
    renderEmptyTableRow(elements.config.instancesBody, 5, "Managed config is unavailable for this runtime.");
    const chip = document.createElement("span");
    chip.className = "selection-chip";
    chip.textContent = "No managed risk profiles available.";
    elements.config.riskList.appendChild(chip);
  } else {
    const config = configResult.data;
    const accounts = Array.isArray(config.accounts) ? config.accounts : [];
    const riskProfiles = Array.isArray(config.risk_profiles) ? config.risk_profiles : [];
    const instances = Array.isArray(config.bots) ? config.bots : [];

    if (accounts.length === 0) {
      renderEmptyTableRow(elements.config.accountsBody, 5, "No managed accounts in the effective config.");
    } else {
      for (const account of accounts) {
        const secretStatus = account && account.secret_status
          ? `${account.secret_status.api_key_present ? "key" : "no key"} | ${account.secret_status.api_secret_present ? "secret" : "no secret"}`
          : "unknown";
        const tr = document.createElement("tr");
        tr.innerHTML = `
          <td><div class="table-primary">${account.id || "-"}</div></td>
          <td><div class="table-primary">${account.kind || "-"}</div></td>
          <td><div class="table-primary">${account.mode || "-"}</div></td>
          <td><div class="table-primary">${secretStatus}</div></td>
          <td><div class="table-primary">${account.execution_remote_submission ? "remote" : "local"}</div></td>
        `;
        elements.config.accountsBody.appendChild(tr);
      }
    }

    if (instances.length === 0) {
      renderEmptyTableRow(elements.config.instancesBody, 5, "No managed bots in the effective config.");
    } else {
      for (const instance of instances) {
        const tr = document.createElement("tr");
        tr.innerHTML = `
          <td><div class="table-primary">${instance.id || "-"}</div></td>
          <td><div class="table-primary">${instance.account || "-"}</div></td>
          <td><div class="table-primary">${instance.market || "-"}</div></td>
          <td><div class="table-primary">${instance.timeframe || "-"}</div></td>
          <td><div class="table-primary">${Array.isArray(instance.symbols) ? instance.symbols.join(", ") : compactText(instance.symbols, 80) || "-"}</div></td>
        `;
        elements.config.instancesBody.appendChild(tr);
      }
    }

    if (riskProfiles.length === 0) {
      const chip = document.createElement("span");
      chip.className = "selection-chip";
      chip.textContent = "No risk profiles available.";
      elements.config.riskList.appendChild(chip);
    } else {
      for (const profile of riskProfiles) {
        const chip = document.createElement("span");
        chip.className = "selection-chip";
        chip.textContent = profile.id || compactText(formatJson(profile), 60);
        elements.config.riskList.appendChild(chip);
      }
    }
  }

  if (routes.length === 0) {
    renderEmptyTableRow(elements.config.routesBody, 3, "OpenAPI route inventory is unavailable.");
  } else {
    if (!state.selectedRouteKey || !routes.some((route) => route.key === state.selectedRouteKey)) {
      state.selectedRouteKey = routes[0].key;
    }
    for (const route of routes) {
      const tr = document.createElement("tr");
      if (route.key === state.selectedRouteKey) {
        tr.classList.add("active");
      }
      tr.dataset.routeKey = route.key;
      tr.innerHTML = `
        <td><span class="method-chip ${route.method.toLowerCase()}">${route.method}</span></td>
        <td><div class="table-primary">${route.path}</div></td>
        <td><div class="table-primary">${route.descriptor && route.descriptor.operationId ? route.descriptor.operationId : "-"}</div></td>
      `;
      elements.config.routesBody.appendChild(tr);
    }
  }

  renderTerminalWorkspace();
}

function renderTerminalWorkspace() {
  for (const button of elements.config.terminalTabs) {
    button.classList.toggle("active", button.dataset.terminalTab === state.terminalMode);
  }
  elements.config.terminalLabel.textContent = TERMINAL_LABELS[state.terminalMode] || "Terminal";

  if (state.terminalMode === "metrics") {
    elements.config.terminal.textContent = state.payloads.metrics.ok ? state.payloads.metrics.data : `Metrics unavailable: ${state.payloads.metrics.error}`;
    return;
  }

  if (state.terminalMode === "openapi") {
    elements.config.terminal.textContent = state.payloads.openapi.ok ? formatJson(state.payloads.openapi.data) : `OpenAPI unavailable: ${state.payloads.openapi.error}`;
    return;
  }

  if (state.terminalMode === "route") {
    const selected = state.openapiRoutes.find((route) => route.key === state.selectedRouteKey) || null;
    elements.config.terminal.textContent = selected ? formatJson(selected) : "Select a route from the table to inspect it here.";
    return;
  }

  elements.config.terminal.textContent = state.payloads.config.ok ? formatJson(state.payloads.config.data) : `Managed config unavailable: ${state.payloads.config.error}`;
}

function describeFocusFilterState() {
  if (!state.focusedBotId) {
    return "No focused bot";
  }
  return `Focused bot ${state.focusedBotId}`;
}

function renderNavigationContext() {
  const page = PAGE_META[state.activePage];
  const detailBotId = state.focused.summary && state.focused.summary.id
    ? state.focused.summary.id
    : state.focusedBotId;
  const selectedFeed = selectedDataStream();
  const detailFeedLabel = selectedFeed && selectedFeed.key
    ? `${selectedFeed.key.account_id} / ${selectedFeed.key.symbol} / ${selectedFeed.key.timeframe}`
    : (state.selectedDataStreamKey ? state.selectedDataStreamKey.replace(/::/g, " / ") : null);
  elements.pageTitle.textContent = state.activePage === "bot-detail" && detailBotId
    ? `Bot Detail :: ${detailBotId}`
    : state.activePage === "feed-detail" && detailFeedLabel
      ? `Feed Detail :: ${detailFeedLabel}`
    : page.title;
  elements.pageDescription.textContent = state.activePage === "activity"
    ? `${page.description} ${describeFocusFilterState()}.`
    : state.activePage === "bot-detail" && detailBotId
      ? `Lifecycle control, reconciliation review, PnL, and simulation tools for bot ${detailBotId}.`
      : state.activePage === "feed-detail" && detailFeedLabel
        ? `Dataplane stream ${detailFeedLabel} with current freshness, attached bots, in-memory bars, and connector history.`
    : page.description;
}

function setControlsDisabled(disabled) {
  state.acting = disabled;
  for (const control of document.querySelectorAll("[data-busy-lock]")) {
    control.disabled = disabled;
  }
  updateSimulationTarget();
}

function actionPendingText(action) {
  switch (action) {
    case "start":
      return "Starting...";
    case "stop":
      return "Stopping...";
    case "pause":
      return "Pausing...";
    case "resume":
      return "Resuming...";
    case "tick":
      return "Ticking...";
    case "reconcile":
      return "Reconciling...";
    case "cancel-open-orders":
      return "Cancelling...";
    case "close-positions":
      return "Closing...";
    default:
      return "Working...";
  }
}

function toLocalDateTimeInputValue(date) {
  const parsed = date instanceof Date ? date : new Date(date);
  if (Number.isNaN(parsed.getTime())) {
    return "";
  }
  const shifted = new Date(parsed.getTime() - (parsed.getTimezoneOffset() * 60000));
  return shifted.toISOString().slice(0, 19);
}

function toUtcIsoFromLocalInput(value) {
  if (!value) {
    return null;
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return null;
  }
  return parsed.toISOString();
}

function readFiniteNumber(value, label, options = {}) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    throw new Error(`${label} must be a valid number.`);
  }
  if (options.gt !== undefined && !(parsed > options.gt)) {
    throw new Error(`${label} must be greater than ${options.gt}.`);
  }
  if (options.gte !== undefined && !(parsed >= options.gte)) {
    throw new Error(`${label} must be greater than or equal to ${options.gte}.`);
  }
  return parsed;
}

function collectSimulateBarInput() {
  if (!state.focusedBotId) {
    throw new Error("Open a bot before simulating a bar.");
  }
  const selectedLane = selectedFocusedLane();
  const timestamp = toUtcIsoFromLocalInput(elements.simulation.barTimestamp.value);
  if (!timestamp) {
    throw new Error("Provide a valid UTC timestamp for the bar.");
  }
  const open = readFiniteNumber(elements.simulation.barOpen.value, "Open");
  const high = readFiniteNumber(elements.simulation.barHigh.value, "High");
  const low = readFiniteNumber(elements.simulation.barLow.value, "Low");
  const close = readFiniteNumber(elements.simulation.barClose.value, "Close");
  const volume = readFiniteNumber(elements.simulation.barVolume.value, "Volume", { gte: 0 });

  if (high < Math.max(open, close)) {
    throw new Error("High must be greater than or equal to open and close.");
  }
  if (low > Math.min(open, close)) {
    throw new Error("Low must be less than or equal to open and close.");
  }

  return {
    instanceId: state.focusedBotId,
    payload: {
      bar: { timestamp, open, high, low, close, volume },
      phase: elements.simulation.barPhase.value || "confirmed",
      symbol: selectedLane ? selectedLane.symbol : undefined
    }
  };
}

function collectSimulateTradeInput() {
  if (!state.focusedBotId) {
    throw new Error("Open a bot before simulating a trade.");
  }
  const symbol = elements.simulation.tradeSymbol.value.trim().toUpperCase();
  if (!symbol) {
    throw new Error("Provide a trade symbol.");
  }
  const timestamp = toUtcIsoFromLocalInput(elements.simulation.tradeTimestamp.value);
  if (!timestamp) {
    throw new Error("Provide a valid UTC timestamp for the trade.");
  }
  const price = readFiniteNumber(elements.simulation.tradePrice.value, "Price", { gt: 0 });
  const quantity = readFiniteNumber(elements.simulation.tradeQuantity.value, "Quantity", { gt: 0 });
  return {
    instanceId: state.focusedBotId,
    payload: {
      trade: { symbol, timestamp, price, quantity }
    }
  };
}

function collectManualSignalInput() {
  if (!state.focusedBotId) {
    throw new Error("Open a bot before injecting a manual signal.");
  }
  const signal = elements.simulation.manualSignal ? elements.simulation.manualSignal.value : "buy_confirmed";
  const timestampValue = elements.simulation.manualTimestamp ? elements.simulation.manualTimestamp.value : "";
  const timestamp = toUtcIsoFromLocalInput(timestampValue);
  if (!timestamp) {
    throw new Error("Provide a valid UTC timestamp for the manual signal.");
  }
  const focusedSummary = state.focused.summary;
  const focusedPolling = focusedSummary && focusedSummary.polling ? focusedSummary.polling : null;
  const price = readFiniteNumber(
    elements.simulation.manualPrice && elements.simulation.manualPrice.value.trim()
      ? elements.simulation.manualPrice.value
      : (focusedPolling && Number.isFinite(Number(focusedPolling.last_polled_bar_close))
          ? String(focusedPolling.last_polled_bar_close)
          : ""),
    "Manual signal price",
    { gt: 0 }
  );
  return {
    instanceId: state.focusedBotId,
    payload: {
      signal,
      price,
      timestamp,
      symbol: selectedFocusedLane() ? selectedFocusedLane().symbol : undefined
    }
  };
}

function setSimulationOutput(title, payload, tone) {
  const prefix = title ? `${title}\n` : "";
  const body = payload === undefined ? "" : formatJson(payload);
  elements.simulation.output.textContent = `${prefix}${body}`.trim();
  setBadge(elements.simulation.badge, title || "Simulation", tone || "info");
}

async function runBarSimulation() {
  if (state.acting) {
    return;
  }
  const stopBusy = startButtonBusy(elements.simulation.barButton, "Submitting...");
  try {
    const { instanceId, payload } = collectSimulateBarInput();
    setControlsDisabled(true);
    setBadge(elements.simulation.badge, `Simulating ${instanceId}`, "warn");
    const response = await requestJson(`/v1/bots/${encodeURIComponent(instanceId)}/simulate-bar`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload)
    });
    setMessage(`Bar simulation accepted for ${instanceId}.`, "ok");
    setSimulationOutput(`Bar simulation response for ${instanceId}`, { request: payload, response }, "ok");
  } catch (error) {
    setMessage(error.message, "error");
    setSimulationOutput("Bar simulation error", error.message, "error");
  } finally {
    stopBusy();
    setControlsDisabled(false);
  }
  triggerBackgroundRefresh(true);
}

async function runTradeSimulation() {
  if (state.acting) {
    return;
  }
  const stopBusy = startButtonBusy(elements.simulation.tradeButton, "Submitting...");
  try {
    const { instanceId, payload } = collectSimulateTradeInput();
    setControlsDisabled(true);
    setBadge(elements.simulation.badge, `Simulating ${instanceId}`, "warn");
    const response = await requestJson(`/v1/bots/${encodeURIComponent(instanceId)}/simulate-trade`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload)
    });
    setMessage(`Trade simulation accepted for ${instanceId}.`, "ok");
    setSimulationOutput(`Trade simulation response for ${instanceId}`, { request: payload, response }, "ok");
  } catch (error) {
    setMessage(error.message, "error");
    setSimulationOutput("Trade simulation error", error.message, "error");
  } finally {
    stopBusy();
    setControlsDisabled(false);
  }
  triggerBackgroundRefresh(true);
}

async function runManualSignal() {
  if (state.acting) {
    return;
  }
  const stopBusy = startButtonBusy(elements.simulation.manualButton, "Injecting...");
  try {
    const { instanceId, payload } = collectManualSignalInput();
    setControlsDisabled(true);
    setBadge(elements.simulation.badge, `Injecting ${instanceId}`, "warn");
    const response = await requestJson(`/v1/bots/${encodeURIComponent(instanceId)}/manual-signal`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload)
    });
    setMessage(`Manual signal accepted for ${instanceId}.`, "ok");
    setSimulationOutput(`Manual signal response for ${instanceId}`, { request: payload, response }, "ok");
  } catch (error) {
    setMessage(error.message, "error");
    setSimulationOutput("Manual signal error", error.message, "error");
  } finally {
    stopBusy();
    setControlsDisabled(false);
  }
  triggerBackgroundRefresh(true);
}

async function runGlobalAction(path, successMessage, button, pendingText = "Working...") {
  if (state.acting) {
    return;
  }
  const stopBusy = startButtonBusy(button, pendingText);
  try {
    setControlsDisabled(true);
    await requestJson(path, { method: "POST" });
    setMessage(successMessage, "ok");
  } catch (error) {
    setMessage(error.message, "error");
  } finally {
    stopBusy();
    setControlsDisabled(false);
  }
  triggerBackgroundRefresh(true);
}

function upsertBotSummary(summary) {
  if (!summary || typeof summary !== "object" || typeof summary.id !== "string") {
    return;
  }

  const next = [];
  let replaced = false;
  for (const instance of state.botCache) {
    if (instance && instance.id === summary.id) {
      next.push(summary);
      replaced = true;
    } else {
      next.push(instance);
    }
  }
  if (!replaced) {
    next.push(summary);
  }
  state.botCache = next;

  if (state.focusedBotId === summary.id) {
    state.focused.summary = {
      ...(state.focused.summary || {}),
      ...summary
    };
    state.focused.loading = false;
  }
}

function collectBulkBotTargets(action) {
  const targets = [];
  const skipped = [];
  for (const instance of state.botCache) {
    if (!instance || !instance.id) {
      continue;
    }

    if (action === "start") {
      if (!instance.enabled) {
        skipped.push({ id: instance.id, reason: "disabled" });
        continue;
      }
      if (instance.reconciliation_blocked) {
        skipped.push({ id: instance.id, reason: "blocked" });
        continue;
      }
      if (instance.state === "reconciling") {
        skipped.push({ id: instance.id, reason: "reconciling" });
        continue;
      }
    }

    if (action === "pause") {
      if (!instance.enabled) {
        skipped.push({ id: instance.id, reason: "disabled" });
        continue;
      }
      if (instance.state !== "running") {
        skipped.push({ id: instance.id, reason: String(instance.state || "not_running") });
        continue;
      }
    }

    if (action === "resume") {
      if (!instance.enabled) {
        skipped.push({ id: instance.id, reason: "disabled" });
        continue;
      }
      if (instance.reconciliation_blocked) {
        skipped.push({ id: instance.id, reason: "blocked" });
        continue;
      }
      if (instance.state === "reconciling") {
        skipped.push({ id: instance.id, reason: "reconciling" });
        continue;
      }
      if (instance.state !== "paused") {
        skipped.push({ id: instance.id, reason: String(instance.state || "not_paused") });
        continue;
      }
    }

    if (action === "reconcile" && !instance.enabled) {
      skipped.push({ id: instance.id, reason: "disabled" });
      continue;
    }

    if (action === "tick") {
      if (!instance.enabled) {
        skipped.push({ id: instance.id, reason: "disabled" });
        continue;
      }
      if (Array.isArray(instance.symbols) && instance.symbols.length > 1) {
        skipped.push({ id: instance.id, reason: "choose_symbol" });
        continue;
      }
      if (instance.state !== "running") {
        skipped.push({ id: instance.id, reason: String(instance.state || "not_running") });
        continue;
      }
    }

    targets.push(instance);
  }
  return { targets, skipped };
}

function bulkActionLabel(action) {
  switch (action) {
    case "start":
      return "Start All Bots";
    case "stop":
      return "Stop All Bots";
    case "pause":
      return "Pause All Running";
    case "resume":
      return "Resume All Paused";
    case "reconcile":
      return "Reconcile All Bots";
    case "tick":
      return "Tick All Running";
    case "cancel-open-orders":
      return "Cancel Orders For All";
    default:
      return `${titleText(action)} All`;
  }
}

function bulkActionPendingText(action, completed, total) {
  switch (action) {
    case "start":
      return `Starting ${completed}/${total}...`;
    case "stop":
      return `Stopping ${completed}/${total}...`;
    case "pause":
      return `Pausing ${completed}/${total}...`;
    case "resume":
      return `Resuming ${completed}/${total}...`;
    case "reconcile":
      return `Reconciling ${completed}/${total}...`;
    case "tick":
      return `Ticking ${completed}/${total}...`;
    case "cancel-open-orders":
      return `Cancelling ${completed}/${total}...`;
    default:
      return `Working ${completed}/${total}...`;
  }
}

function summarizeBulkSkipped(skipped) {
  if (!Array.isArray(skipped) || skipped.length === 0) {
    return "";
  }
  const preview = skipped
    .slice(0, 3)
    .map((entry) => `${entry.id} (${entry.reason})`)
    .join(", ");
  const suffix = skipped.length > 3 ? ` +${skipped.length - 3} more` : "";
  return `Skipped: ${preview}${suffix}.`;
}

async function runBulkBotAction(action, button) {
  if (state.acting) {
    return;
  }

  const { targets, skipped } = collectBulkBotTargets(action);
  if (targets.length === 0) {
    const skippedSummary = summarizeBulkSkipped(skipped);
    setMessage(
      `${bulkActionLabel(action)} has no eligible targets.${skippedSummary ? ` ${skippedSummary}` : ""}`,
      "warn"
    );
    return;
  }

  const confirmationLines = [
    `${bulkActionLabel(action)} for ${numberText(targets.length, 0)} bot(s)?`,
    skipped.length > 0 ? `${numberText(skipped.length, 0)} bot(s) will be skipped.` : "",
    action === "stop"
      ? "This issues stop requests across every loaded bot."
      : action === "pause"
        ? "This pauses every currently running eligible bot."
        : action === "resume"
          ? "This resumes every currently paused eligible bot."
      : action === "reconcile"
        ? "This runs reconciliation across every eligible loaded bot."
        : action === "tick"
          ? "This manually polls and dispatches one bar for every currently running eligible bot."
          : action === "cancel-open-orders"
            ? "This records a manual cancel-orders request across every loaded bot."
        : "This issues start requests across every eligible loaded bot."
  ].filter(Boolean);
  if (!window.confirm(confirmationLines.join("\n"))) {
    return;
  }

  const stopBusy = startButtonBusy(button, bulkActionPendingText(action, 0, targets.length));
  const failures = [];
  let succeeded = 0;

  try {
    setControlsDisabled(true);
    for (let index = 0; index < targets.length; index += 1) {
      const target = targets[index];
      button.textContent = bulkActionPendingText(action, index + 1, targets.length);
      try {
        const response = await requestJson(`/v1/bots/${encodeURIComponent(target.id)}/${action}`, {
          method: "POST"
        });
        upsertBotSummary(response);
        succeeded += 1;
      } catch (error) {
        failures.push(`${target.id}: ${error.message}`);
      }
    }

    renderBots();
    renderFocusedInstance();

    const outcome = `${bulkActionLabel(action)} finished: ${numberText(succeeded, 0)} succeeded, ${numberText(skipped.length, 0)} skipped, ${numberText(failures.length, 0)} failed.`;
    const skippedSummary = summarizeBulkSkipped(skipped);
    const failureSummary = failures.length > 0
      ? ` Failures: ${compactText(failures.join(" | "), 220)}`
      : "";
    const detailSummary = failures.length > 0
      ? failureSummary
      : (skippedSummary ? ` ${skippedSummary}` : "");
    setMessage(`${outcome}${detailSummary}`, failures.length > 0 ? "warn" : "ok");
  } finally {
    stopBusy();
    setControlsDisabled(false);
  }

  triggerBackgroundRefresh(true);
}

async function runInstanceAction(action, button) {
  if (state.acting) {
    return;
  }
  if (!state.focusedBotId) {
    setMessage("Open a bot before running a contextual action.", "warn");
    return;
  }
  const instanceId = state.focusedBotId;
  const stopBusy = startButtonBusy(button, actionPendingText(action));
  try {
    setControlsDisabled(true);
    const selectedLane = selectedFocusedLane();
    const suffix = action === "tick" && selectedLane
      ? `?symbol=${encodeURIComponent(selectedLane.symbol)}`
      : "";
    const response = await requestJson(`/v1/bots/${encodeURIComponent(instanceId)}/${action}${suffix}`, { method: "POST" });
    upsertBotSummary(response);
    renderFocusedInstance();
    renderBots();
    setMessage(`${titleText(action)} submitted for ${instanceId}.`, "ok");
  } catch (error) {
    setMessage(error.message, "error");
  } finally {
    stopBusy();
    setControlsDisabled(false);
  }
  triggerBackgroundRefresh(true);
}

function bindActions() {
  elements.refreshButton.addEventListener("click", () => {
    refreshDashboard(false);
  });

  elements.reloadButton.addEventListener("click", () => {
    runGlobalAction("/v1/config/reload", "Config reload request sent.", elements.reloadButton, "Reloading...");
  });

  elements.killOnButton.addEventListener("click", () => {
    runGlobalAction("/v1/risk/kill-switch", "Kill switch enabled.", elements.killOnButton, "Arming...");
  });

  elements.killOffButton.addEventListener("click", () => {
    runGlobalAction("/v1/risk/clear-kill-switch", "Kill switch cleared.", elements.killOffButton, "Clearing...");
  });

  elements.simulation.barButton.addEventListener("click", () => {
    runBarSimulation();
  });

  elements.simulation.tradeButton.addEventListener("click", () => {
    runTradeSimulation();
  });

  if (elements.simulation.manualButton) {
    elements.simulation.manualButton.addEventListener("click", () => {
      runManualSignal();
    });
  }

  if (elements.simulation.manualPrice) {
    elements.simulation.manualPrice.addEventListener("input", () => {
      elements.simulation.manualPrice.dataset.manualOverride = elements.simulation.manualPrice.value.trim()
        ? "true"
        : "false";
    });
  }

  elements.simulation.tradeSymbol.addEventListener("blur", () => {
    if (!elements.simulation.tradeSymbol.value.trim() && state.focusedBotId) {
      const lane = selectedFocusedLane();
      elements.simulation.tradeSymbol.value = lane
        ? lane.symbol
        : String(state.focusedBotId).replace(/[^a-zA-Z0-9]/g, "").toUpperCase();
      elements.simulation.tradeSymbol.dataset.manualOverride = "false";
    }
  });
  elements.simulation.tradeSymbol.addEventListener("input", () => {
    elements.simulation.tradeSymbol.dataset.manualOverride = elements.simulation.tradeSymbol.value.trim()
      ? "true"
      : "false";
  });

  elements.instances.search.addEventListener("input", () => {
    renderBots();
  });

  elements.instances.bulkStartButton.addEventListener("click", () => {
    runBulkBotAction("start", elements.instances.bulkStartButton);
  });

  elements.instances.bulkStopButton.addEventListener("click", () => {
    runBulkBotAction("stop", elements.instances.bulkStopButton);
  });

  elements.instances.bulkPauseButton.addEventListener("click", () => {
    runBulkBotAction("pause", elements.instances.bulkPauseButton);
  });

  elements.instances.bulkResumeButton.addEventListener("click", () => {
    runBulkBotAction("resume", elements.instances.bulkResumeButton);
  });

  elements.instances.bulkReconcileButton.addEventListener("click", () => {
    runBulkBotAction("reconcile", elements.instances.bulkReconcileButton);
  });

  elements.instances.bulkTickButton.addEventListener("click", () => {
    runBulkBotAction("tick", elements.instances.bulkTickButton);
  });

  elements.instances.bulkCancelOrdersButton.addEventListener("click", () => {
    runBulkBotAction("cancel-open-orders", elements.instances.bulkCancelOrdersButton);
  });

  for (const button of elements.instances.stateTabs) {
    button.addEventListener("click", () => {
      state.botStateFilter = button.dataset.instanceStateTab || "all";
      renderBots();
    });
  }

  for (const button of elements.instances.marketTabs) {
    button.addEventListener("click", () => {
      state.botMarketFilter = button.dataset.instanceMarketTab || "all";
      renderBots();
    });
  }

  for (const button of elements.instances.enabledTabs) {
    button.addEventListener("click", () => {
      state.botEnabledFilter = button.dataset.instanceEnabledTab || "all";
      renderBots();
    });
  }

  elements.instances.body.addEventListener("click", (event) => {
    const expandButton = event.target.closest("[data-instance-expand]");
    if (expandButton) {
      const instanceId = expandButton.dataset.instanceExpand || "";
      if (instanceId) {
        if (state.expandedBotIds.has(instanceId)) {
          state.expandedBotIds.delete(instanceId);
        } else {
          state.expandedBotIds.add(instanceId);
        }
        renderBots();
      }
      return;
    }

    const button = event.target.closest("[data-instance-focus]");
    if (!button) {
      return;
    }
    state.focusedBotId = button.dataset.instanceFocus;
    state.focused.selectedLaneSymbol = null;
    if (elements.simulation.manualPrice) {
      elements.simulation.manualPrice.dataset.manualOverride = "false";
    }
    if (elements.simulation.tradeSymbol) {
      elements.simulation.tradeSymbol.dataset.manualOverride = "false";
    }
    const cachedSummary = state.botCache.find((instance) => instance && instance.id === state.focusedBotId);
    if (cachedSummary) {
      state.focused.summary = cachedSummary;
      syncManualSignalPriceFromFocusedInstance();
    }
    state.focused.report = null;
    state.focused.lanes = [];
    state.focused.timeline = [];
    state.focused.timelineLoading = true;
    state.focused.loading = true;
    setActivePage("bot-detail");
    renderFocusedInstance();
    renderBots();
    void refreshFocusedInstanceReport({ forceSummaryFetch: !cachedSummary, preserveMessage: true });
  });

  if (elements.botDetailBack) {
    elements.botDetailBack.addEventListener("click", () => {
      setActivePage("bots");
      renderNavigationContext();
    });
  }

  if (elements.focus.laneSelect) {
    elements.focus.laneSelect.addEventListener("change", () => {
      state.focused.selectedLaneSymbol = elements.focus.laneSelect.value || null;
      if (elements.simulation.tradeSymbol) {
        elements.simulation.tradeSymbol.dataset.manualOverride = "false";
      }
      syncManualSignalPriceFromFocusedInstance(true);
      const marketDataPromise = refreshFocusedLaneMarketData();
      renderFocusedInstance();
      void marketDataPromise.then(() => {
        renderFocusedInstance();
      });
    });
  }

  if (elements.focus.laneList) {
    elements.focus.laneList.addEventListener("click", (event) => {
      const button = event.target.closest("[data-lane-symbol]");
      if (!button) {
        return;
      }
      state.focused.selectedLaneSymbol = button.dataset.laneSymbol || null;
      if (elements.simulation.tradeSymbol) {
        elements.simulation.tradeSymbol.dataset.manualOverride = "false";
      }
      syncManualSignalPriceFromFocusedInstance(true);
      const marketDataPromise = refreshFocusedLaneMarketData();
      renderFocusedInstance();
      void marketDataPromise.then(() => {
        renderFocusedInstance();
      });
    });
  }

  if (elements.feedDetailBack) {
    elements.feedDetailBack.addEventListener("click", () => {
      setActivePage("feeds");
      renderNavigationContext();
    });
  }

  elements.overview.streamList.addEventListener("click", (event) => {
    const button = event.target.closest("[data-stream-key]");
    if (!button) {
      return;
    }
    state.selectedDataStreamKey = button.dataset.streamKey;
    state.selectedDataStreamBars = [];
    state.selectedDataStreamBarsError = "";
    state.selectedDataStreamLoading = true;
    state.selectedDataStreamHistory = [];
    state.selectedDataStreamHistoryError = "";
    state.selectedDataStreamHistoryLoading = true;
    setActivePage("feed-detail");
    renderDataStreamSection();
    void Promise.all([refreshSelectedStreamBars(), refreshSelectedStreamHistory()]).then(() => {
      renderDataStreamSection();
      renderNavigationContext();
    });
  });

  elements.focus.actions.addEventListener("click", (event) => {
    const button = event.target.closest("[data-instance-action]");
    if (!button) {
      return;
    }
    runInstanceAction(button.dataset.instanceAction, button);
  });

  if (elements.focus.openTrace) {
    elements.focus.openTrace.addEventListener("click", () => {
      const traceId = elements.focus.openTrace.dataset.traceId || "";
      if (!traceId) {
        return;
      }
      openTraceForRecord(
        {
          trace_id: traceId,
          bot_id: elements.focus.openTrace.dataset.botId || state.focusedBotId || "",
          symbol: elements.focus.openTrace.dataset.symbol || "",
        },
        elements.focus.openTrace.dataset.botId || state.focusedBotId || "",
        elements.focus.openTrace.dataset.symbol || "",
      );
    });
  }

  elements.activity.tabs.forEach((button) => {
    button.addEventListener("click", () => {
      state.selectedActivityKind = button.dataset.streamTab;
      state.selectedActivityKey = null;
      renderActivityWorkspace();
    });
  });

  if (elements.activity.search) {
    elements.activity.search.addEventListener("input", () => {
      state.activityFilters.search = elements.activity.search.value || "";
      state.selectedActivityKey = null;
      renderActivityWorkspace();
    });
  }

  if (elements.activity.feedLimit) {
    elements.activity.feedLimit.addEventListener("change", () => {
      const nextLimit = normalizeFeedLimit(elements.activity.feedLimit.value);
      if (nextLimit === state.feedLimit) {
        return;
      }
      state.feedLimit = nextLimit;
      state.selectedActivityKey = null;
      renderActivityWorkspace();
      setMessage(`Refreshing feeds with a ${numberText(nextLimit, 0)} record window...`, "warn");
      triggerBackgroundRefresh(true);
    });
  }

  if (elements.activity.botFilter) {
    elements.activity.botFilter.addEventListener("change", () => {
      state.activityFilters.botId = elements.activity.botFilter.value || "";
      state.selectedActivityKey = null;
      renderActivityWorkspace();
    });
  }

  if (elements.activity.symbolFilter) {
    elements.activity.symbolFilter.addEventListener("change", () => {
      state.activityFilters.symbol = elements.activity.symbolFilter.value || "";
      state.selectedActivityKey = null;
      renderActivityWorkspace();
    });
  }

  if (elements.activity.originFilter) {
    elements.activity.originFilter.addEventListener("change", () => {
      state.activityFilters.origin = elements.activity.originFilter.value || "";
      state.selectedActivityKey = null;
      renderActivityWorkspace();
    });
  }

  if (elements.activity.clearFilters) {
    elements.activity.clearFilters.addEventListener("click", () => {
      state.activityFilters.search = "";
      state.activityFilters.botId = "";
      state.activityFilters.symbol = "";
      state.activityFilters.origin = "";
      state.selectedActivityKey = null;
      renderActivityWorkspace();
    });
  }

  if (elements.activity.openTrace) {
    elements.activity.openTrace.addEventListener("click", () => {
      const traceId = elements.activity.openTrace.dataset.traceId || "";
      if (!traceId) {
        return;
      }
      openTraceForRecord(
        {
          trace_id: traceId,
          bot_id: elements.activity.openTrace.dataset.botId || "",
          symbol: elements.activity.openTrace.dataset.symbol || "",
        },
        elements.activity.openTrace.dataset.botId || "",
        elements.activity.openTrace.dataset.symbol || "",
      );
    });
  }

  elements.activity.list.addEventListener("click", (event) => {
    const button = event.target.closest("[data-activity-key]");
    if (!button) {
      return;
    }
    state.selectedActivityKey = button.dataset.activityKey;
    renderActivityWorkspace();
  });

  elements.provider.list.addEventListener("click", (event) => {
    const button = event.target.closest("[data-provider-key]");
    if (!button) {
      return;
    }
    state.selectedProviderKey = button.dataset.providerKey;
    renderProviderWorkspace();
  });

  elements.overview.blotterList.addEventListener("click", (event) => {
    const button = event.target.closest("[data-blotter-kind][data-blotter-key]");
    if (!button) {
      return;
    }
    state.selectedActivityKind = button.dataset.blotterKind;
    state.selectedActivityKey = button.dataset.blotterKey;
    setActivePage("activity");
    renderNavigationContext();
    renderActivityWorkspace();
  });

  elements.connectors.body.addEventListener("click", (event) => {
    const row = event.target.closest("tr[data-connector-account-id]");
    if (!row) {
      return;
    }
    state.selectedConnectorAccountId = row.dataset.connectorAccountId;
    renderConnectorWorkspace();
  });

  elements.config.routesBody.addEventListener("click", (event) => {
    const row = event.target.closest("tr[data-route-key]");
    if (!row) {
      return;
    }
    state.selectedRouteKey = row.dataset.routeKey;
    state.terminalMode = "route";
    renderConfigWorkspace();
  });

  elements.config.terminalTabs.forEach((button) => {
    button.addEventListener("click", () => {
      state.terminalMode = button.dataset.terminalTab;
      renderTerminalWorkspace();
    });
  });

  elements.navButtons.forEach((button) => {
    button.addEventListener("click", (event) => {
      if (event.defaultPrevented) {
        return;
      }
      if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
        return;
      }
      event.preventDefault();
      const href = button.getAttribute("href");
      if (href) {
        window.location.assign(href);
        return;
      }
      setActivePage(button.dataset.pageTarget);
      renderNavigationContext();
    });
  });

  elements.autoRefresh.addEventListener("change", restartAutoRefresh);

  window.addEventListener("hashchange", () => {
    const route = routeFromLocation();
    if (route.botId) {
      state.focusedBotId = route.botId;
    }
    if (route.feedKey) {
      state.selectedDataStreamKey = route.feedKey;
    }
    setActivePage(route.page, false);
    renderNavigationContext();
    if (route.page === "bot-detail" && state.focusedBotId) {
      void refreshFocusedInstanceReport({ forceSummaryFetch: true, preserveMessage: true });
    }
    if (route.page === "feed-detail" && state.selectedDataStreamKey) {
      state.selectedDataStreamLoading = true;
      state.selectedDataStreamHistoryLoading = true;
      renderDataStreamSection();
      void Promise.all([refreshSelectedStreamBars(), refreshSelectedStreamHistory()]).then(() => {
        renderDataStreamSection();
        renderNavigationContext();
      });
    }
  });
}

function restartAutoRefresh() {
  if (state.timer) {
    window.clearInterval(state.timer);
    state.timer = null;
  }
  if (elements.autoRefresh.checked) {
    state.timer = window.setInterval(() => {
      if (!state.acting) {
        refreshDashboard(true);
      }
    }, REFRESH_INTERVAL_MS);
  }
}

async function refreshDashboard(preserveMessage) {
  if (state.refreshing) {
    return;
  }
  state.refreshing = true;
  const refreshToken = state.deferredRefreshToken + 1;
  state.deferredRefreshToken = refreshToken;
  const stopRefreshBusy = startButtonBusy(elements.refreshButton, "Refreshing...");
  setBadge(elements.lastUpdated, "Syncing...", "warn");
  try {
    state.feedLimit = normalizeFeedLimit(state.feedLimit);
    const payload = await requestJson(feedEndpoint(DASHBOARD_SNAPSHOT_ENDPOINT));

    state.botCache = Array.isArray(payload.instances) ? payload.instances : [];
    state.dataStreams = Array.isArray(payload.dataStreams) ? payload.dataStreams : [];
    state.connectorStatuses = Array.isArray(payload.connectorsStatus) ? payload.connectorsStatus : [];
    state.connectorMatrix = Array.isArray(payload.connectorsMatrix) ? payload.connectorsMatrix : [];
    state.providerEvents = Array.isArray(payload.providerEvents) ? payload.providerEvents : [];
    ensureSelectedDataStream();
    primeSelectedStreamLoading();

    const riskItems = Array.isArray(payload.risk)
      ? payload.risk
      : (payload.risk && Array.isArray(payload.risk.items) ? payload.risk.items : []);
    state.feedMeta.riskCount = payload.risk && Number.isFinite(Number(payload.risk.count))
      ? Number(payload.risk.count)
      : riskItems.length;

    state.lastFeeds = {
      signals: Array.isArray(payload.signals) ? payload.signals : [],
      intents: Array.isArray(payload.intents) ? payload.intents : [],
      risk: riskItems,
      orders: Array.isArray(payload.orders) ? payload.orders : [],
      fills: Array.isArray(payload.fills) ? payload.fills : [],
      positions: Array.isArray(payload.positions) ? payload.positions : [],
      reconciliations: Array.isArray(payload.reconciliations) ? payload.reconciliations : [],
      events: Array.isArray(payload.events) ? payload.events : []
    };

    updateLedgerSnapshot(payload && payload.ledger
      ? { ok: true, data: payload.ledger, error: "" }
      : { ok: false, data: null, error: "Ledger not sampled yet." });
    updateStatus(payload.status || null);
    updateObservability(payload.status || null);
    updateExecutionStats(state.lastFeeds);
    renderOverviewBoards();
    renderWatchlist();
    ensureAutoFocus();
    primeFocusedInstanceLoading();
    renderBots();
    if (state.activePage === "bot-detail") {
      renderFocusedInstance();
    }
    renderActivityWorkspace();
    renderProviderWorkspace();
    renderConnectorWorkspace();
    renderConfigWorkspace();
    renderDataStreamSection();
    renderNavigationContext();
    void hydrateDeferredDashboardData(refreshToken);

    setBadge(elements.lastUpdated, `Updated ${new Date().toLocaleTimeString()}`, "info");

    if (!preserveMessage) {
      setMessage("Trading dashboard synchronized.", "ok");
    }
  } catch (error) {
    setMessage(error.message || "Refresh failed.", "error");
  } finally {
    state.refreshing = false;
    stopRefreshBusy();
    if (state.refreshQueued) {
      state.refreshQueued = false;
      triggerBackgroundRefresh(true);
    }
  }
}

function initialize() {
  const route = routeFromLocation();
  if (route.botId) {
    state.focusedBotId = route.botId;
  }
  if (route.feedKey) {
    state.selectedDataStreamKey = route.feedKey;
  }
  setActivePage(route.page, false);
  renderNavigationContext();
  bindActions();
  elements.simulation.barTimestamp.value = toLocalDateTimeInputValue(new Date());
  elements.simulation.tradeTimestamp.value = toLocalDateTimeInputValue(new Date());
  if (elements.simulation.manualTimestamp) {
    elements.simulation.manualTimestamp.value = toLocalDateTimeInputValue(new Date());
  }
  elements.simulation.tradeSymbol.value = "AAPL";
  elements.simulation.tradeSymbol.dataset.manualOverride = "false";
  if (elements.simulation.manualPrice) {
    elements.simulation.manualPrice.value = "";
    elements.simulation.manualPrice.dataset.manualOverride = "false";
  }
  updateClock();
  state.clockTimer = window.setInterval(updateClock, CLOCK_INTERVAL_MS);
  restartAutoRefresh();
  refreshDashboard(false);
}

initialize();
