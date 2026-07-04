pub mod abs_client;
pub mod config;
pub mod hub;
pub mod models;
pub mod room;
pub mod ws_handler;

use std::sync::Arc;
use axum::{
    body::Bytes,
    extract::{ws::WebSocketUpgrade, Path, State},
    http::{HeaderMap, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Json, Router,
};
use tower_http::{
    cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer},
    services::{ServeDir, ServeFile},
};

use abs_client::AbsClient;
use config::Config;
use hub::{new_room_map, RoomMap};

#[derive(Clone)]
pub struct AppState {
    pub rooms: RoomMap,
    pub abs: Arc<AbsClient>,
    pub config: Config,
}

fn build_cors(config: &Config) -> CorsLayer {
    match &config.cors_origin {
        Some(origin) => {
            let value = origin
                .parse::<axum::http::HeaderValue>()
                .expect("CORS_ORIGIN is not a valid header value");
            CorsLayer::new()
                .allow_origin(AllowOrigin::exact(value))
                .allow_headers(AllowHeaders::list([
                    header::AUTHORIZATION,
                    header::CONTENT_TYPE,
                ]))
                .allow_methods(AllowMethods::list([Method::GET, Method::POST]))
        }
        None => CorsLayer::new(),
    }
}

pub fn build_app(config: Config) -> Router {
    let state = AppState {
        rooms: new_room_map(),
        abs: Arc::new(AbsClient::new(config.abs_base_url.clone())),
        config: config.clone(),
    };

    Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/api/abs/login", post(abs_login_proxy))
        .route("/api/abs/validate-key", post(abs_validate_key_proxy))
        .route("/api/abs-proxy/*path", any(abs_proxy))
        .route("/api/config", get(api_config))
        .route("/health", get(|| async { "ok" }))
        .fallback_service(
            ServeDir::new(&config.static_dir)
                .append_index_html_on_directories(true)
                .fallback(ServeFile::new(format!("{}/index.html", &config.static_dir))),
        )
        .layer(build_cors(&config))
        .with_state(state)
}

pub async fn run() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("abs_party=info".parse().unwrap()),
        )
        .init();

    let config = Config::from_env();
    let bind = config.bind.clone();
    let app = build_app(config);

    tracing::info!("abs-party listening on {bind}");
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .expect("bind failed");
    axum::serve(listener, app).await.unwrap();
}

// --- Handlers ---

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| ws_handler::handle_socket(socket, state))
}

#[derive(serde::Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

async fn abs_login_proxy(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Response {
    match state.abs.login(&body.username, &body.password).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
struct ValidateKeyBody {
    api_key: String,
}

async fn abs_validate_key_proxy(
    State(state): State<AppState>,
    Json(body): Json<ValidateKeyBody>,
) -> Response {
    match state.abs.me(&body.api_key).await {
        Ok(user) => Json(serde_json::json!({
            "token": body.api_key,
            "user": { "id": user.id, "username": user.username, "token": body.api_key }
        }))
        .into_response(),
        Err(e) => {
            tracing::warn!("API key validation failed: {e:#}");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": format!("Validation failed: {e}") })),
            )
                .into_response()
        }
    }
}

async fn api_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "abs_base_url": state.config.abs_base_url
    }))
}

/// Allowlist of (method, path-pattern) pairs the party app legitimately needs.
/// Anything not matching is blocked — the proxy must not be a general ABS gateway.
fn proxy_path_allowed(method: &Method, path: &str) -> bool {
    // Reject any path segment that tries to escape upward.
    if path.contains("..") {
        return false;
    }
    match method.as_str() {
        "GET" => matches_any(path, &[
            "api/me",
            "api/libraries",
        ]) || path_matches_prefix_suffix(path, "api/libraries/", "/items")
          || path_matches_prefix_suffix(path, "api/libraries/", "/search")
          || path.starts_with("api/items/")   // item detail + cover
          || path.starts_with("api/me/progress/"),
        "POST" => {
            (path.starts_with("api/me/item/") && path.ends_with("/bookmark"))
                || (path.starts_with("api/items/") && path.ends_with("/play"))
                || (path.starts_with("api/session/")
                    && (path.ends_with("/sync") || path.ends_with("/close")))
        }
        _ => false, // block DELETE, PATCH, PUT, etc.
    }
}

fn matches_any(path: &str, exact: &[&str]) -> bool {
    exact.iter().any(|&p| path == p)
}

fn path_matches_prefix_suffix(path: &str, prefix: &str, suffix: &str) -> bool {
    path.starts_with(prefix) && path.ends_with(suffix)
}

async fn abs_proxy(
    State(state): State<AppState>,
    Path(path): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !proxy_path_allowed(&method, &path) {
        return (StatusCode::FORBIDDEN, "Not a permitted ABS endpoint").into_response();
    }

    let token = match headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
    {
        Some(t) => t.to_string(),
        None => {
            return (StatusCode::UNAUTHORIZED, "Missing Bearer token").into_response();
        }
    };

    let base = state.config.abs_base_url.trim_end_matches('/');
    let url = match uri.query() {
        Some(q) if !q.is_empty() => format!("{}/{}?{}", base, path, q),
        _ => format!("{}/{}", base, path),
    };

    let reqwest_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .unwrap_or(reqwest::Method::GET);

    match state.abs.proxy_raw(&token, reqwest_method, &url, body).await {
        Ok((status, resp_bytes, content_type)) => {
            let status_code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            let mut builder = axum::response::Response::builder().status(status_code);
            if let Some(ct) = content_type {
                builder = builder.header(header::CONTENT_TYPE, ct);
            }
            builder
                .body(axum::body::Body::from(resp_bytes))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}
