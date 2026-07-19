use abs_party::{build_app, config::Config};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

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
async fn websocket_play_command_relayed_to_guest() {
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

    // Host sends play
    host_ws
        .send(Message::Text(
            serde_json::json!({"type":"play","position":75.5}).to_string().into(),
        ))
        .await
        .unwrap();

    // Guest should receive play
    let play_msg: serde_json::Value =
        serde_json::from_str(&guest_ws.next().await.unwrap().unwrap().into_text().unwrap())
            .unwrap();
    assert_eq!(play_msg["type"], "play");
    assert!((play_msg["position"].as_f64().unwrap() - 75.5).abs() < f64::EPSILON);

    // Host should NOT receive its own play back — give a short window
    tokio::select! {
        msg = host_ws.next() => {
            if let Some(Ok(m)) = msg {
                let json: serde_json::Value = serde_json::from_str(&m.into_text().unwrap()).unwrap();
                // The only acceptable unsolicited message for the host here would be nothing
                panic!("host unexpectedly received: {json}");
            }
        }
        _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
    }
}
