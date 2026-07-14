use crate::constants::{DASHBOARD_HTML, UI_DIST};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

pub(crate) async fn dashboard_handler() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

pub(crate) async fn ui_asset_handler(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    // Defense in depth: parent-directory components must never reach the
    // embedded-asset lookup, where they could confuse path matching.
    if path.split('/').any(|segment| segment == "..") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Some(file) = UI_DIST.get_file(path) {
        let mime = guess_ui_asset_mime(path);
        let cache_control = ui_asset_cache_header(path);
        (
            [
                (axum::http::header::CONTENT_TYPE, mime),
                (axum::http::header::CACHE_CONTROL, cache_control),
            ],
            file.contents(),
        )
            .into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

pub(crate) async fn favicon_handler() -> Response {
    if let Some(file) = UI_DIST.get_file("favicon.ico") {
        (
            [(axum::http::header::CONTENT_TYPE, "image/x-icon")],
            file.contents(),
        )
            .into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

fn guess_ui_asset_mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn ui_asset_cache_header(path: &str) -> &'static str {
    // Nuxt emits content-hashed files under _nuxt/ and _fonts/; these are safe to cache aggressively.
    if path.starts_with("_nuxt/") || path.starts_with("_fonts/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=300"
    }
}
