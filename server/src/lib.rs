pub mod abs_client;
pub mod config;
pub mod hub;
pub mod models;
pub mod proxy;
pub mod room;
pub mod ws_handler;

use std::sync::Arc;
use axum::{
    extract::{ws::WebSocketUpgrade, State},
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
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
        .nest("/api/abs-proxy", proxy::router())
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
