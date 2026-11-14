//! `/api/abs-proxy/*`: forwards the browser's ABS REST calls (avoids CORS and the ABS
//! nginx's User-Agent/Accept filtering). The route table below *is* the allowlist —
//! anything not listed gets 403, a listed path with the wrong method gets 405.

use axum::{
    body::Bytes,
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/me", get(forward))
        .route("/api/me/progress/:item_id", get(forward))
        .route("/api/me/item/:item_id/bookmark", post(forward))
        .route("/api/libraries", get(forward))
        .route("/api/libraries/:library_id/items", get(forward))
        .route("/api/libraries/:library_id/search", get(forward))
        .route("/api/items/:item_id", get(forward))
        .route("/api/items/:item_id/cover", get(forward_cover))
        .route("/api/items/:item_id/play", post(forward))
        .route("/api/session/:session_id/sync", post(forward))
        .route("/api/session/:session_id/close", post(forward))
        .route_layer(middleware::from_fn(reject_unsafe_segments))
        .fallback(|| async { (StatusCode::FORBIDDEN, "Not a permitted ABS endpoint") })
}

/// The router matches raw segments: it doesn't resolve `..` or decode `%2e`, so `:item_id`
/// would happily capture `..` — and reqwest's URL parser then resolves it on the way out
/// (`api/items/../cover` reaches ABS as `/api/cover`). An nginx in front of us doesn't help:
/// a plain `proxy_pass` forwards the client's raw path. `%2F` is passed on encoded, but a
/// decoding proxy in front of ABS could turn it into real segments.
/// IDs (UUIDs, `li_…`) and our literal segments only ever use these characters.
async fn reject_unsafe_segments(req: Request, next: Next) -> Response {
    let safe = req.uri().path().split('/').skip(1).all(|seg| {
        !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    });
    if !safe {
        return (StatusCode::FORBIDDEN, "Not a permitted ABS endpoint").into_response();
    }
    next.run(req).await
}

async fn forward(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    let Some(token) = token else {
        return (StatusCode::UNAUTHORIZED, "Missing Bearer token").into_response();
    };
    send_upstream(&state, Some(token), method, &uri, body).await
}

/// ABS serves covers without auth (its Auth.js ignorePatterns) and an `<img src>` can't
/// send our Bearer header, so covers go through tokenless — and we never forward one.
async fn forward_cover(State(state): State<AppState>, uri: Uri) -> Response {
    let mut resp = send_upstream(&state, None, Method::GET, &uri, Bytes::new()).await;
    if resp.status().is_success() {
        // Covers rarely change; spare ABS a refetch per grid render
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"));
    }
    resp
}

/// `uri` is relative to the nest point, i.e. already the ABS path (`/api/...`).
async fn send_upstream(
    state: &AppState,
    token: Option<&str>,
    method: Method,
    uri: &Uri,
    body: Bytes,
) -> Response {
    let base = state.config.abs_base_url.trim_end_matches('/');
    let url = match uri.query() {
        Some(q) if !q.is_empty() => format!("{base}{}?{q}", uri.path()),
        _ => format!("{base}{}", uri.path()),
    };
    let method = reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET);

    match state.abs.proxy_raw(token, method, &url, body).await {
        Ok((status, bytes, content_type)) => {
            let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            let mut resp = (status, bytes).into_response();
            if let Some(ct) = content_type.and_then(|ct| HeaderValue::from_str(&ct).ok()) {
                resp.headers_mut().insert(header::CONTENT_TYPE, ct);
            }
            resp
        }
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}
