use abs_party::{build_app, config::Config};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;
use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};

fn test_config() -> Config {
    Config::for_test()
}

// --- HTTP endpoint tests ---

#[tokio::test]
async fn health_returns_ok() {
    let app = build_app(test_config());
    let resp = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn config_endpoint_returns_abs_base_url() {
    let mut cfg = test_config();
    cfg.abs_base_url = "https://my.abs.example".into();

    let app = build_app(cfg);
    let resp = app
        .oneshot(Request::builder().uri("/api/config").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["abs_base_url"], "https://my.abs.example");
}

#[tokio::test]
async fn unknown_route_returns_404_or_fallback() {
    let app = build_app(test_config());
    let resp = app
        .oneshot(Request::builder().uri("/nonexistent-path").body(Body::empty()).unwrap())
        .await
        .unwrap();
    // Static file fallback returns 404 when static dir doesn't exist
    assert!(resp.status().is_client_error() || resp.status().is_success());
}

// --- WebSocket tests ---

async fn spawn_test_server() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = build_app(test_config());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn websocket_ping_pong() {
    let addr = spawn_test_server().await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect failed");

    ws.send(Message::Text(r#"{"type":"ping","sent_at":99999}"#.into()))
        .await
        .unwrap();

    let msg = ws.next().await.unwrap().unwrap();
    let text = msg.into_text().unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["type"], "pong");
    assert_eq!(json["sent_at"], 99999);
}

#[tokio::test]
async fn websocket_invalid_message_returns_error() {
    let addr = spawn_test_server().await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();

    ws.send(Message::Text(r#"{"type":"not_a_real_type"}"#.into()))
        .await
        .unwrap();

    let msg = ws.next().await.unwrap().unwrap();
    let json: serde_json::Value = serde_json::from_str(&msg.into_text().unwrap()).unwrap();
    assert_eq!(json["type"], "error");
}

#[tokio::test]
async fn websocket_create_and_join_room() {
    let addr = spawn_test_server().await;

    // Host creates a room
    let (mut host_ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();

    host_ws
        .send(Message::Text(
            serde_json::json!({
                "type": "create_room",
                "room_id": "abcd1234",
                "abs_token": "alice",
                "item_id": "item1",
                "item_title": "Test Book",
                "item_author": "Test Author",
                "library_id": "lib1"
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let state_msg = host_ws.next().await.unwrap().unwrap();
    let state: serde_json::Value = serde_json::from_str(&state_msg.into_text().unwrap()).unwrap();
    assert_eq!(state["type"], "room_state");
    let room_id = state["room_id"].as_str().unwrap().to_string();
    assert_eq!(room_id, "abcd1234");
    assert_eq!(state["item"]["title"], "Test Book");

    // Guest joins the room
    let (mut guest_ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();

    guest_ws
        .send(Message::Text(
            serde_json::json!({
                "type": "join",
                "room_id": room_id,
                "abs_token": "bob"
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    // Guest receives room_state
    let guest_msg = guest_ws.next().await.unwrap().unwrap();
    let guest_state: serde_json::Value =
        serde_json::from_str(&guest_msg.into_text().unwrap()).unwrap();
    assert_eq!(guest_state["type"], "room_state");

    // Host receives participant_joined notification
    let host_notif = host_ws.next().await.unwrap().unwrap();
    let notif: serde_json::Value =
        serde_json::from_str(&host_notif.into_text().unwrap()).unwrap();
    assert_eq!(notif["type"], "participant_joined");
    assert_eq!(notif["name"], "bob");
}

#[tokio::test]
async fn websocket_play_intent_and_ready_triggers_play() {
    let addr = spawn_test_server().await;

    let (mut host_ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    host_ws
        .send(Message::Text(
            serde_json::json!({"type":"create_room","room_id":"abcd1234","abs_token":"alice","item_id":"i","item_title":"B","item_author":"A","library_id":"l"})
            .to_string().into(),
        ))
        .await
        .unwrap();
    let state: serde_json::Value =
        serde_json::from_str(&host_ws.next().await.unwrap().unwrap().into_text().unwrap()).unwrap();
    let room_id = state["room_id"].as_str().unwrap().to_string();

    let (mut guest_ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    guest_ws
        .send(Message::Text(
            serde_json::json!({"type":"join","room_id":room_id,"abs_token":"bob"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let _ = guest_ws.next().await; // drain room_state
    let _ = host_ws.next().await; // drain participant_joined

    // Host sends play_intent
    host_ws
        .send(Message::Text(
            serde_json::json!({"type":"play_intent","position":75.5}).to_string().into(),
        ))
        .await
        .unwrap();

    // Both host and guest receive play_intent
    let host_intent: serde_json::Value =
        serde_json::from_str(&host_ws.next().await.unwrap().unwrap().into_text().unwrap()).unwrap();
    assert_eq!(host_intent["type"], "play_intent");
    let guest_intent: serde_json::Value =
        serde_json::from_str(&guest_ws.next().await.unwrap().unwrap().into_text().unwrap()).unwrap();
    assert_eq!(guest_intent["type"], "play_intent");
    assert!((guest_intent["position"].as_f64().unwrap() - 75.5).abs() < f64::EPSILON);

    // Both send ready
    host_ws
        .send(Message::Text(serde_json::json!({"type":"ready"}).to_string().into()))
        .await
        .unwrap();
    guest_ws
        .send(Message::Text(serde_json::json!({"type":"ready"}).to_string().into()))
        .await
        .unwrap();

    // Both should receive play
    let host_play: serde_json::Value =
        serde_json::from_str(&host_ws.next().await.unwrap().unwrap().into_text().unwrap()).unwrap();
    assert_eq!(host_play["type"], "play");
    let guest_play: serde_json::Value =
        serde_json::from_str(&guest_ws.next().await.unwrap().unwrap().into_text().unwrap()).unwrap();
    assert_eq!(guest_play["type"], "play");
    assert!((guest_play["position"].as_f64().unwrap() - 75.5).abs() < f64::EPSILON);
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send_json(ws: &mut Ws, value: serde_json::Value) {
    ws.send(Message::Text(value.to_string().into())).await.unwrap();
}

async fn recv_json(ws: &mut Ws) -> serde_json::Value {
    serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap()).unwrap()
}

#[tokio::test]
async fn websocket_room_settings_lifecycle() {
    let addr = spawn_test_server().await;

    // Host creates a host-only room
    let (mut host_ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws")).await.unwrap();
    send_json(&mut host_ws, serde_json::json!({
        "type":"create_room","room_id":"abcd1234","abs_token":"alice","item_id":"i","item_title":"B","item_author":"A","library_id":"l",
        "settings":{"host_only_controls":true,"pause_on_join_leave":false}
    })).await;
    let state = recv_json(&mut host_ws).await;
    assert_eq!(state["settings"]["host_only_controls"], true);
    assert_eq!(state["settings"]["pause_on_join_leave"], false);

    // Guest joins and sees the settings
    let (mut guest_ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws")).await.unwrap();
    send_json(&mut guest_ws, serde_json::json!({"type":"join","room_id":"abcd1234","abs_token":"bob"})).await;
    let guest_state = recv_json(&mut guest_ws).await;
    assert_eq!(guest_state["settings"]["host_only_controls"], true);
    let _ = recv_json(&mut host_ws).await; // participant_joined

    // Guest seek is denied
    send_json(&mut guest_ws, serde_json::json!({"type":"seek","position":99.0})).await;
    let denied = recv_json(&mut guest_ws).await;
    assert_eq!(denied["type"], "control_denied");
    assert_eq!(denied["playing"], false);

    // Guest cannot change settings: next thing the guest sees is the host's change, not their own
    send_json(&mut guest_ws, serde_json::json!({"type":"update_settings","settings":{"host_only_controls":false,"pause_on_join_leave":true}})).await;
    send_json(&mut host_ws, serde_json::json!({"type":"update_settings","settings":{"host_only_controls":false,"pause_on_join_leave":false}})).await;
    for ws in [&mut host_ws, &mut guest_ws] {
        let changed = recv_json(ws).await;
        assert_eq!(changed["type"], "settings_changed");
        assert_eq!(changed["settings"]["host_only_controls"], false);
        assert_eq!(changed["settings"]["pause_on_join_leave"], false);
    }

    // Guest may now seek; host receives it
    send_json(&mut guest_ws, serde_json::json!({"type":"seek","position":42.0})).await;
    let seek = recv_json(&mut host_ws).await;
    assert_eq!(seek["type"], "seek");
    assert_eq!(seek["position"], 42.0);
}

// --- ABS proxy: covers ---

/// Fake ABS that serves a cover and echoes whether it received an Authorization header.
async fn spawn_fake_abs() -> String {
    use axum::{http::HeaderMap, response::IntoResponse, routing::get, Router};
    let app = Router::new()
        .route("/api/items/:id/cover", get(|headers: HeaderMap| async move {
            let auth = if headers.contains_key("authorization") { "auth" } else { "noauth" };
            ([("content-type", "image/jpeg")], format!("cover-bytes-{auth}")).into_response()
        }))
        .route("/api/items/:id", get(|| async { "item" }))
        .route("/api/libraries/:id/search", get(|headers: HeaderMap, uri: axum::http::Uri| async move {
            let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("none").to_string();
            format!("{} {}", uri, auth)
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

async fn proxy_get(abs_url: &str, path: &str, token: Option<&str>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    proxy_request(abs_url, "GET", path, token).await
}

async fn proxy_request(abs_url: &str, method: &str, path: &str, token: Option<&str>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut cfg = test_config();
    cfg.abs_base_url = abs_url.into();
    let mut req = Request::builder().method(method).uri(format!("/api/abs-proxy/{path}"));
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let resp = build_app(cfg).oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = http_body_util::BodyExt::collect(resp.into_body()).await.unwrap().to_bytes().to_vec();
    (status, headers, body)
}

#[tokio::test]
async fn proxy_serves_cover_without_token() {
    let abs = spawn_fake_abs().await;
    let (status, headers, body) = proxy_get(&abs, "api/items/09c4e9ef-c12a-432c-b92f-88cc47add86a/cover", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/jpeg");
    assert!(headers["cache-control"].to_str().unwrap().contains("max-age"));
    assert_eq!(body, b"cover-bytes-noauth");
}

#[tokio::test]
async fn proxy_never_forwards_token_for_covers() {
    let abs = spawn_fake_abs().await;
    let (status, _, body) = proxy_get(&abs, "api/items/abc/cover", Some("secret")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"cover-bytes-noauth");
}

#[tokio::test]
async fn proxy_still_requires_token_for_other_item_paths() {
    let abs = spawn_fake_abs().await;
    let (status, _, _) = proxy_get(&abs, "api/items/abc", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn proxy_forwards_path_query_and_token() {
    let abs = spawn_fake_abs().await;
    let (status, _, body) = proxy_get(&abs, "api/libraries/lib1/search?q=fox&limit=40", Some("tok")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap(), "/api/libraries/lib1/search?q=fox&limit=40 Bearer tok");
}

#[tokio::test]
async fn proxy_blocks_paths_not_in_route_table() {
    let abs = spawn_fake_abs().await;
    for path in [
        "api/items/abc/file/1",          // raw audio download — was allowed by the old prefix match
        "api/items/abc/ebook",
        "api/libraries/a/b/c/items",     // was allowed by the old prefix/suffix match
        "api/items/abc/cover/extra",
        "api/users",
        "login",
    ] {
        let (status, _, _) = proxy_get(&abs, path, Some("tok")).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
}

/// Fake ABS that accepts anything and counts what reaches it.
async fn spawn_counting_abs() -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let app = axum::Router::new().fallback(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        async { "forwarded" }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), hits)
}

/// Sensitive endpoints taken from the ABS source (server/routers/ApiRouter.js, Auth.js,
/// Server.js, routers/PublicRouter.js). None may reach ABS, even with a valid token —
/// a guest's token is still a full ABS session, maybe an admin's.
#[tokio::test]
async fn proxy_never_forwards_admin_destructive_or_auth_endpoints() {
    let (abs, hits) = spawn_counting_abs().await;
    let id = "09c4e9ef-c12a-432c-b92f-88cc47add86a";
    let cases: &[(&str, String)] = &[
        // Login / session / account
        ("POST", "login".into()),
        ("POST", "logout".into()),
        ("POST", "auth/refresh".into()),
        ("GET", "auth/openid".into()),
        ("GET", "auth/openid/config".into()),
        ("POST", "init".into()),
        ("GET", "status".into()),
        ("PATCH", "api/me/password".into()),
        ("POST", "api/authorize".into()),
        ("GET", "api/api-keys".into()),
        ("POST", "api/api-keys".into()),
        ("DELETE", format!("api/api-keys/{id}")),
        ("GET", "api/me/sessions".into()),
        ("DELETE", format!("api/me/sessions/{id}")),
        // User administration
        ("GET", "api/users".into()),
        ("GET", "api/users/online".into()),
        ("POST", "api/users".into()),
        ("GET", format!("api/users/{id}")),
        ("PATCH", format!("api/users/{id}")),
        ("DELETE", format!("api/users/{id}")),
        ("PATCH", format!("api/users/{id}/openid-unlink")),
        // Server administration
        ("PATCH", "api/settings".into()),
        ("GET", "api/auth-settings".into()),
        ("PATCH", "api/auth-settings".into()),
        ("GET", "api/backups".into()),
        ("POST", "api/backups".into()),
        ("GET", format!("api/backups/{id}/apply")),   // a GET that restores a backup
        ("GET", format!("api/backups/{id}/download")),
        ("DELETE", format!("api/backups/{id}")),
        ("PATCH", "api/backups/path".into()),
        ("GET", "api/filesystem".into()),
        ("POST", "api/filesystem/pathexists".into()),
        ("GET", "api/logger-data".into()),
        ("GET", "api/sessions".into()),
        ("POST", "api/sessions/batch/delete".into()),
        ("DELETE", format!("api/sessions/{id}")),
        ("POST", "api/notifications".into()),
        ("GET", "api/notifications/test".into()),
        ("PATCH", "api/emails/settings".into()),
        ("POST", "api/custom-metadata-providers".into()),
        ("POST", "api/cache/purge".into()),
        ("POST", "api/watcher/update".into()),
        ("POST", "api/validate-cron".into()),
        ("GET", "api/stats/server".into()),
        // Libraries
        ("POST", "api/libraries".into()),
        ("PATCH", format!("api/libraries/{id}")),
        ("DELETE", format!("api/libraries/{id}")),
        ("DELETE", format!("api/libraries/{id}/issues")),
        ("POST", format!("api/libraries/{id}/scan")),
        ("POST", format!("api/libraries/{id}/remove-metadata")),
        ("GET", format!("api/libraries/{id}/matchall")),  // GET that rewrites metadata
        ("GET", format!("api/libraries/{id}/download")),
        ("POST", "api/libraries/order".into()),
        // Media items
        ("DELETE", format!("api/items/{id}")),
        ("PATCH", format!("api/items/{id}/media")),
        ("POST", format!("api/items/{id}/cover")),
        ("PATCH", format!("api/items/{id}/cover")),
        ("DELETE", format!("api/items/{id}/cover")),
        ("POST", format!("api/items/{id}/match")),
        ("PATCH", format!("api/items/{id}/tracks")),
        ("POST", format!("api/items/{id}/scan")),
        ("POST", format!("api/items/{id}/chapters")),
        ("GET", format!("api/items/{id}/download")),
        ("GET", format!("api/items/{id}/file/123")),
        ("DELETE", format!("api/items/{id}/file/123")),
        ("POST", "api/items/batch/delete".into()),
        ("POST", "api/items/batch/update".into()),
        ("POST", "api/items/batch/scan".into()),
        ("POST", "api/upload".into()),
        ("POST", format!("api/tools/item/{id}/encode-m4b")),
        ("POST", format!("api/tools/item/{id}/embed-metadata")),
        ("POST", "api/tools/batch/embed-metadata".into()),
        ("DELETE", format!("api/authors/{id}")),
        ("PATCH", format!("api/series/{id}")),
        ("DELETE", "api/tags/sometag".into()),
        ("DELETE", "api/genres/somegenre".into()),
        ("POST", format!("api/podcasts/{id}/download-episodes")),
        ("GET", format!("api/podcasts/{id}/clear-queue")),   // GET that mutates
        ("DELETE", format!("api/podcasts/{id}/episode/{id}")),
        // Sharing / feeds (make media public)
        ("POST", "api/share/mediaitem".into()),
        ("POST", format!("api/feeds/item/{id}/open")),
        ("PATCH", "public/share/slug/progress".into()),
        // The user's own data, beyond what the party needs
        ("PATCH", format!("api/me/progress/{id}")),
        ("DELETE", format!("api/me/progress/{id}")),
        ("PATCH", "api/me/progress/batch/update".into()),
        ("GET", format!("api/me/progress/{id}/remove-from-continue-listening")),  // GET that mutates
        ("PATCH", format!("api/me/item/{id}/bookmark")),
        ("DELETE", format!("api/me/item/{id}/bookmark/123")),
        ("POST", "api/session/local".into()),
        ("POST", "api/session/local-all".into()),
        ("DELETE", format!("api/collections/{id}")),
        ("DELETE", format!("api/playlists/{id}")),
    ];
    for (method, path) in cases {
        let (status, _, _) = proxy_request(&abs, method, path, Some("tok")).await;
        assert!(
            status == StatusCode::FORBIDDEN || status == StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path} -> {status}"
        );
    }
    assert_eq!(hits.load(Ordering::SeqCst), 0, "a blocked request reached ABS");

    // Control: the counting fake does see an allowed request
    let (status, _, _) = proxy_request(&abs, "GET", &format!("api/items/{id}"), Some("tok")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn proxy_blocks_traversal_and_odd_segments() {
    let abs = spawn_fake_abs().await;
    for path in [
        "api/items/../cover",            // :item_id would capture ".."
        "api/items/..",
        "api/items/%2e%2e/cover",
        "api/items/a%2Fb/cover",
        "api/items/./cover",
        "api/items//cover",
        "api/me/",
    ] {
        let (status, _, _) = proxy_get(&abs, path, Some("tok")).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
}

#[tokio::test]
async fn proxy_rejects_wrong_method_on_listed_paths() {
    let abs = spawn_fake_abs().await;
    for (method, path) in [
        ("POST", "api/items/abc/cover"),
        ("DELETE", "api/items/abc"),
        ("PATCH", "api/me"),
        ("GET", "api/items/abc/play"),
        ("PUT", "api/session/s1/sync"),
    ] {
        let (status, _, _) = proxy_request(&abs, method, path, Some("tok")).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{method} {path}");
    }
}
