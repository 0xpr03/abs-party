use std::sync::Arc;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::abs_client::AbsClient;
use crate::hub::{self};
use crate::models::{AbsMeResponse, ClientMessage, ItemInfo, ServerMessage};
use crate::room::Participant;
use crate::AppState;

pub async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    let participant_id = Uuid::new_v4();
    let mut current_room: Option<String> = None;

    // Forward outbound messages from channel → websocket
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let text = match serde_json::to_string(&msg) {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ws_tx.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(raw)) = ws_rx.next().await {
        let text = match raw {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };

        let msg: ClientMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                let _ = tx.send(ServerMessage::Error {
                    message: format!("Invalid message: {e}"),
                });
                continue;
            }
        };

        match msg {
            ClientMessage::Join { room_id, abs_token } => {
                let me = match validate_token(&state.abs, &abs_token, state.config.bypass_auth).await {
                    Ok(m) => m,
                    Err(e) => { let _ = tx.send(ServerMessage::Error { message: e }); continue; }
                };

                let rid = match room_id {
                    Some(r) => r,
                    None => {
                        let _ = tx.send(ServerMessage::Error { message: "room_id required for join".into() });
                        continue;
                    }
                };

                let participant = Participant { id: participant_id, name: me.username.clone(), abs_token, abs_username: me.username, tx: tx.clone() };

                match hub::join_room(&state.rooms, &rid, participant).await {
                    Ok(()) => current_room = Some(rid),
                    Err(e) => { let _ = tx.send(ServerMessage::Error { message: e }); }
                }
            }

            ClientMessage::CreateRoom { room_id, abs_token, item_id, item_title, item_author, library_id } => {
                let me = match validate_token(&state.abs, &abs_token, state.config.bypass_auth).await {
                    Ok(m) => m,
                    Err(e) => { let _ = tx.send(ServerMessage::Error { message: e }); continue; }
                };

                let item = ItemInfo { id: item_id, library_id, title: item_title, author: item_author };
                let participant = Participant { id: participant_id, name: me.username.clone(), abs_token, abs_username: me.username, tx: tx.clone() };

                match hub::create_room(&state.rooms, room_id.clone(), item, participant).await {
                    Ok(true) => {
                        // New room created — send initial room_state
                        current_room = Some(room_id.clone());
                        if let Some(arc) = state.rooms.get(&room_id).map(|r| r.clone()) {
                            let room = arc.lock().await;
                            let _ = tx.send(room.state_message());
                        }
                    }
                    Ok(false) => {
                        // Joined existing room — join_room already sent room_state
                        current_room = Some(room_id);
                    }
                    Err(e) => {
                        let _ = tx.send(ServerMessage::Error { message: e });
                    }
                }
            }

            ClientMessage::Play { position } => {
                if let Some(ref rid) = current_room {
                    hub::handle_play(&state.rooms, rid, participant_id, position).await;
                }
            }
            ClientMessage::Pause { position } => {
                if let Some(ref rid) = current_room {
                    hub::handle_pause(&state.rooms, rid, participant_id, position).await;
                }
            }
            ClientMessage::Seek { position } => {
                if let Some(ref rid) = current_room {
                    hub::handle_seek(&state.rooms, rid, participant_id, position).await;
                }
            }
            ClientMessage::Speed { rate } => {
                if let Some(ref rid) = current_room {
                    hub::handle_speed(&state.rooms, rid, participant_id, rate).await;
                }
            }
            ClientMessage::Ping { sent_at } => {
                let _ = tx.send(ServerMessage::Pong { sent_at });
            }
        }
    }

    if let Some(rid) = current_room {
        hub::leave_room(&state.rooms, &rid, participant_id).await;
    }
}

async fn validate_token(
    abs: &Arc<AbsClient>,
    token: &str,
    bypass: bool,
) -> Result<AbsMeResponse, String> {
    if bypass {
        return Ok(AbsMeResponse { id: token.into(), username: token.into() });
    }
    abs.me(token).await.map_err(|_| "ABS token validation failed".into())
}
