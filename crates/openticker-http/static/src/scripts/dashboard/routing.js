function parseHashRoute(hashSource) {
  const hash = hashSource.replace(/^#/, "");
  if (!hash) {
    return null;
  }
  if (hash.startsWith("bots/")) {
    const botId = decodeURIComponent(hash.slice("bots/".length));
    return {
      page: botId ? "bot-detail" : "bots",
      botId: botId || null,
      feedKey: null
    };
  }
  if (hash.startsWith("feeds/")) {
    const feedKey = decodeURIComponent(hash.slice("feeds/".length));
    return {
      page: feedKey ? "feed-detail" : "feeds",
      botId: null,
      feedKey: feedKey || null
    };
  }
  if (PAGE_META[hash]) {
    return {
      page: hash,
      botId: null,
      feedKey: null
    };
  }
  return null;
}

function parsePathRoute(pathnameSource) {
  let pathname = pathnameSource || "/";
  try {
    pathname = decodeURIComponent(pathname);
  } catch (_error) {
    pathname = pathnameSource || "/";
  }

  const normalizedPath = pathname.length > 1
    ? pathname.replace(/\/+$/, "")
    : pathname;

  if (normalizedPath.startsWith("/bots/")) {
    const botId = normalizedPath.slice("/bots/".length);
    return {
      page: botId ? "bot-detail" : "bots",
      botId: botId || null,
      feedKey: null
    };
  }

  if (normalizedPath.startsWith("/feeds/")) {
    const feedKey = normalizedPath.slice("/feeds/".length);
    return {
      page: feedKey ? "feed-detail" : "feeds",
      botId: null,
      feedKey: feedKey || null
    };
  }

  if (normalizedPath.startsWith("/cycles/")) {
    return {
      page: "cycles-detail",
      botId: null,
      feedKey: null
    };
  }

  const pageByPath = {
    "/": INITIAL_PAGE,
    "/dashboard": "overview",
    "/activity": "activity",
    "/bots": "bots",
    "/cycles-detail": "cycles-detail",
    "/cycles": "cycles",
    "/portfolio": "portfolio",
    "/config": "config",
    "/connectors": "connectors",
    "/feeds": "feeds",
    "/providers": "providers"
  };

  return {
    page: PAGE_META[pageByPath[normalizedPath]] ? pageByPath[normalizedPath] : INITIAL_PAGE,
    botId: null,
    feedKey: null
  };
}

function routeFromLocation() {
  const hashRoute = parseHashRoute(window.location.hash);
  if (hashRoute) {
    return hashRoute;
  }
  return parsePathRoute(window.location.pathname);
}

function activeNavPage(page) {
  if (page === "bot-detail") {
    return "bots";
  }
  if (page === "feed-detail") {
    return "feeds";
  }
  if (page === "cycles-detail") {
    return "cycles";
  }
  return page;
}

function hashForPage(page) {
  if (page === "bot-detail" && state.focusedBotId) {
    return `bots/${encodeURIComponent(state.focusedBotId)}`;
  }
  if (page === "feed-detail" && state.selectedDataStreamKey) {
    return `feeds/${encodeURIComponent(state.selectedDataStreamKey)}`;
  }
  return activeNavPage(page);
}

function setActivePage(page, updateHash = true) {
  state.activePage = PAGE_META[page] ? page : "overview";
  const navPage = activeNavPage(state.activePage);
  for (const button of elements.navButtons) {
    button.classList.toggle("active", button.dataset.pageTarget === navPage);
  }
  for (const pageElement of elements.pages) {
    const isActive = pageElement.dataset.page === state.activePage;
    pageElement.classList.toggle("active", isActive);
    pageElement.hidden = !isActive;
  }
  elements.pageTitle.textContent = PAGE_META[state.activePage].title;
  elements.pageDescription.textContent = PAGE_META[state.activePage].description;
  const nextHash = hashForPage(state.activePage);
  if (updateHash && window.location.hash !== `#${nextHash}`) {
    window.location.hash = nextHash;
  }
}
