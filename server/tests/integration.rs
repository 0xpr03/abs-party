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
