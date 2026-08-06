use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use dashmap::DashMap;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::models::{ItemInfo, ServerMessage};
use crate::room::{now_millis, Participant, ReadyCheck, Room};

pub type RoomMap = Arc<DashMap<String, Arc<Mutex<Room>>>>;

pub fn new_room_map() -> RoomMap {
    Arc::new(DashMap::new())
}

fn validate_room_id(id: &str) -> Result<(), String> {
    if id.len() != 8 {
        return Err("Room ID must be exactly 8 characters".into());
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Room ID may only contain letters and digits".into());
    }
    Ok(())
}

/// Create a new room with the given ID, or join the existing room if the ID is already in use.
/// Returns Ok(true) if a new room was created, Ok(false) if joined an existing room.
pub async fn create_room(
    rooms: &RoomMap,
    room_id: String,
    item: ItemInfo,
    host: Participant,
) -> Result<bool, String> {
    validate_room_id(&room_id)?;
    if rooms.contains_key(&room_id) {
        join_room(rooms, &room_id, host).await?;
        return Ok(false);
    }
    let room = Room::new(room_id.clone(), item, host);
    rooms.insert(room_id, Arc::new(Mutex::new(room)));
    Ok(true)
}

pub async fn join_room(
    rooms: &RoomMap,
    room_id: &str,
    participant: Participant,
) -> Result<(), String> {
    validate_room_id(room_id)?;
    let arc = rooms
        .get(room_id)
        .ok_or_else(|| "Room not found — the host hasn't opened the room yet".to_string())?
        .clone();

    let mut room = arc.lock().await;
    let name = participant.name.clone();
    let tx = participant.tx.clone();

    if room.participants.iter().any(|p| p.name == name) {
        let _ = tx.send(ServerMessage::Error {
            message: "Username already in this room".into(),
        });
        return Err("Username taken".into());
    }

    // Auto-pause on join so all parties re-sync before resuming
    if room.playing {
        let pos = room.estimated_position();
        room.pause_now();
        room.broadcast(&ServerMessage::Pause {
            position: pos,
            sender_name: format!("{} joined", name),
        });
    } else if room.ready_check.is_some() {
        // Cancel any pending ready-check; broadcast Pause so clients clear buffering state
        let pos = room.ready_check.as_ref().unwrap().position;
        room.ready_check = None;
        room.broadcast(&ServerMessage::Pause {
            position: pos,
            sender_name: format!("{} joined", name),
        });
    }

    // Push the newcomer first so room_state includes them in the participant list
    room.participants.push(participant);

    let state = room.state_message();
    let _ = tx.send(state);

    room.broadcast_except(
        room.participants.last().unwrap().id,
        &ServerMessage::ParticipantJoined { name },
    );

    Ok(())
}

pub async fn handle_play_intent(rooms: &RoomMap, room_id: &str, sender_id: Uuid, position: f64) {
    if !position.is_finite() || position < 0.0 { return; }
    if let Some(arc) = rooms.get(room_id).map(|r| r.clone()) {
        let mut room = arc.lock().await;
        let sender_name = room.participants.iter()
            .find(|p| p.id == sender_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        room.pause_now();
        room.position = position;
        let expected: HashSet<Uuid> = room.participants.iter().map(|p| p.id).collect();
        room.ready_check = Some(ReadyCheck { position, expected, ready: HashSet::new() });
        // Broadcast to ALL participants (including sender) so everyone starts buffering
        room.broadcast(&ServerMessage::PlayIntent { position, sender_name });
    }
}

pub async fn handle_ready(rooms: &RoomMap, room_id: &str, sender_id: Uuid) {
    if let Some(arc) = rooms.get(room_id).map(|r| r.clone()) {
        let mut room = arc.lock().await;
        let fire_play = if room.ready_check.is_some() {
            let check = room.ready_check.as_mut().unwrap();
            check.ready.insert(sender_id);
            check.expected.is_subset(&check.ready)
        } else {
            false
        };
        if fire_play {
            let pos = room.ready_check.as_ref().unwrap().position;
            room.ready_check = None;
            room.playing = true;
            room.position = pos;
            room.play_started_at = Some(Instant::now());
            room.broadcast(&ServerMessage::Play {
                position: pos,
                host_time: now_millis(),
                sender_name: String::new(),
            });
        }
    }
}

pub async fn handle_pause(rooms: &RoomMap, room_id: &str, sender_id: Uuid, position: f64) {
    if !position.is_finite() || position < 0.0 { return; }
    if let Some(arc) = rooms.get(room_id).map(|r| r.clone()) {
        let mut room = arc.lock().await;
        let sender_name = room.participants.iter()
            .find(|p| p.id == sender_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        room.position = position;
        room.playing = false;
        room.play_started_at = None;
        room.ready_check = None;
        let msg = ServerMessage::Pause { position, sender_name };
        room.broadcast_except(sender_id, &msg);
    }
}

pub async fn handle_seek(rooms: &RoomMap, room_id: &str, sender_id: Uuid, position: f64) {
    if !position.is_finite() || position < 0.0 { return; }
    if let Some(arc) = rooms.get(room_id).map(|r| r.clone()) {
        let mut room = arc.lock().await;
        let sender_name = room.participants.iter()
            .find(|p| p.id == sender_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        room.position = position;
        room.play_started_at = None;
        room.ready_check = None;
        let msg = ServerMessage::Seek { position, sender_name };
        room.broadcast_except(sender_id, &msg);
    }
}

pub async fn handle_speed(rooms: &RoomMap, room_id: &str, sender_id: Uuid, rate: f64) {
    if !rate.is_finite() || !(0.25_f64..=4.0_f64).contains(&rate) { return; }
    if let Some(arc) = rooms.get(room_id).map(|r| r.clone()) {
        let mut room = arc.lock().await;
        if !room.is_host(sender_id) {
            return;
        }
        let sender_name = room.participants.iter()
            .find(|p| p.id == sender_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        room.speed = rate;
        let msg = ServerMessage::Speed { rate, sender_name };
        room.broadcast_except(sender_id, &msg);
    }
}

pub async fn leave_room(rooms: &RoomMap, room_id: &str, participant_id: Uuid) {
    let should_remove = if let Some(arc) = rooms.get(room_id).map(|r| r.clone()) {
        let mut room = arc.lock().await;
        if let Some(name) = room.remove_participant(participant_id) {
            room.broadcast(&ServerMessage::ParticipantLeft { name: name.clone() });

            if !room.is_empty() {
                if room.playing {
                    let pos = room.estimated_position();
                    room.pause_now();
                    room.broadcast(&ServerMessage::Pause {
                        position: pos,
                        sender_name: format!("{} left", name),
                    });
                } else if room.ready_check.is_some() {
                    let fire_play = {
                        let check = room.ready_check.as_mut().unwrap();
                        check.expected.remove(&participant_id);
                        check.expected.is_subset(&check.ready)
                    };
                    if fire_play {
                        let pos = room.ready_check.as_ref().unwrap().position;
                        room.ready_check = None;
                        room.playing = true;
                        room.position = pos;
                        room.play_started_at = Some(Instant::now());
                        room.broadcast(&ServerMessage::Play {
                            position: pos,
                            host_time: now_millis(),
                            sender_name: String::new(),
                        });
                    }
                }
            }
        }
        room.is_empty()
    } else {
        false
    };

    if should_remove {
        rooms.remove(room_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ServerMessage;
    use crate::room::test_helpers::{make_item, make_participant};

    #[test]
    fn validate_room_id_rejects_short() {
        assert!(validate_room_id("abc").is_err());
    }

    #[test]
    fn validate_room_id_rejects_long() {
        assert!(validate_room_id("abcdefghi").is_err());
    }

    #[test]
    fn validate_room_id_rejects_invalid_chars() {
        assert!(validate_room_id("abcd!@#$").is_err());
        assert!(validate_room_id("abcd efgh").is_err());
    }

    #[test]
    fn validate_room_id_accepts_valid() {
        assert!(validate_room_id("abcd1234").is_ok());
        assert!(validate_room_id("ABCD1234").is_ok());
        assert!(validate_room_id("AbCd1234").is_ok());
    }

    #[tokio::test]
    async fn create_room_inserts_into_map() {
        let rooms = new_room_map();
        let (host, _rx) = make_participant("Alice");
        let created = create_room(&rooms, "testroom".into(), make_item(), host).await.unwrap();
        assert!(created); // new room
        assert!(rooms.contains_key("testroom"));
    }

    #[tokio::test]
    async fn create_room_joins_existing_room() {
        let rooms = new_room_map();
        let (host, _host_rx) = make_participant("Alice");
        create_room(&rooms, "testroom".into(), make_item(), host).await.unwrap();

        let (guest, mut guest_rx) = make_participant("Bob");
        let joined = create_room(&rooms, "testroom".into(), make_item(), guest).await.unwrap();
        assert!(!joined); // joined existing
        // join_room sends room_state to the newcomer
        let msg = guest_rx.recv().await.unwrap();
        assert!(matches!(msg, ServerMessage::RoomState { .. }));
    }

    #[tokio::test]
    async fn create_room_rejects_invalid_id() {
        let rooms = new_room_map();
        let (host, _rx) = make_participant("Alice");
        let result = create_room(&rooms, "bad!id".into(), make_item(), host).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn join_room_sends_state_to_newcomer() {
        let rooms = new_room_map();
        let (host, _host_rx) = make_participant("Alice");
        create_room(&rooms, "testroom".into(), make_item(), host).await.unwrap();

        let (guest, mut guest_rx) = make_participant("Bob");
        join_room(&rooms, "testroom", guest).await.unwrap();

        let msg = guest_rx.recv().await.unwrap();
        assert!(matches!(msg, ServerMessage::RoomState { .. }));
    }

    #[tokio::test]
    async fn join_room_notifies_existing_participants() {
        let rooms = new_room_map();
        let (host, mut host_rx) = make_participant("Alice");
        create_room(&rooms, "testroom".into(), make_item(), host).await.unwrap();

        let (guest, _guest_rx) = make_participant("Bob");
        join_room(&rooms, "testroom", guest).await.unwrap();

        let msg = host_rx.recv().await.unwrap();
        assert!(matches!(msg, ServerMessage::ParticipantJoined { name } if name == "Bob"));
    }

    #[tokio::test]
    async fn join_nonexistent_room_returns_error() {
        let rooms = new_room_map();
        let (guest, _rx) = make_participant("Bob");
        let result = join_room(&rooms, "nonexst", guest).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn join_room_rejects_invalid_id() {
        let rooms = new_room_map();
        let (guest, _rx) = make_participant("Bob");
        let result = join_room(&rooms, "bad!", guest).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn play_intent_broadcasts_to_all_and_ready_fires_play() {
        let rooms = new_room_map();
        let (host, mut host_rx) = make_participant("Alice");
        let host_id = host.id;
        create_room(&rooms, "testroom".into(), make_item(), host).await.unwrap();
        let room_id = "testroom";

        let (guest, mut guest_rx) = make_participant("Bob");
        let guest_id = guest.id;
        join_room(&rooms, room_id, guest).await.unwrap();
        // Room was not playing, so join only sends ParticipantJoined to host
        let _ = host_rx.recv().await; // drain ParticipantJoined
        let _ = guest_rx.recv().await; // drain room_state

        handle_play_intent(&rooms, room_id, host_id, 100.0).await;

        // Both should receive PlayIntent (broadcast to all)
        let h_msg = host_rx.recv().await.unwrap();
        let g_msg = guest_rx.recv().await.unwrap();
        assert!(matches!(h_msg, ServerMessage::PlayIntent { position, .. } if (position - 100.0).abs() < f64::EPSILON));
        assert!(matches!(g_msg, ServerMessage::PlayIntent { .. }));

        // Host sends Ready — not all ready yet (guest hasn't)
        handle_ready(&rooms, room_id, host_id).await;
        assert!(host_rx.try_recv().is_err());
        assert!(guest_rx.try_recv().is_err());

        // Guest sends Ready — now all ready, Play fires to everyone
        handle_ready(&rooms, room_id, guest_id).await;
        let h_play = host_rx.recv().await.unwrap();
        let g_play = guest_rx.recv().await.unwrap();
        assert!(matches!(h_play, ServerMessage::Play { position, .. } if (position - 100.0).abs() < f64::EPSILON));
        assert!(matches!(g_play, ServerMessage::Play { .. }));
    }

    #[tokio::test]
    async fn guest_can_send_play_intent() {
        let rooms = new_room_map();
        let (host, mut host_rx) = make_participant("Alice");
        let host_id = host.id;
        create_room(&rooms, "testroom".into(), make_item(), host).await.unwrap();
        let room_id = "testroom";

        let (guest, mut guest_rx) = make_participant("Bob");
        let guest_id = guest.id;
        join_room(&rooms, room_id, guest).await.unwrap();
        let _ = host_rx.recv().await; // drain ParticipantJoined
        let _ = guest_rx.recv().await; // drain room_state

        handle_play_intent(&rooms, room_id, guest_id, 50.0).await;

        // Both receive PlayIntent
        assert!(matches!(guest_rx.recv().await.unwrap(), ServerMessage::PlayIntent { .. }));
        assert!(matches!(host_rx.recv().await.unwrap(), ServerMessage::PlayIntent { .. }));

        // Both send Ready → all ready → Play fires to both
        handle_ready(&rooms, room_id, guest_id).await;
        handle_ready(&rooms, room_id, host_id).await;

        assert!(matches!(guest_rx.recv().await.unwrap(), ServerMessage::Play { .. }));
        assert!(matches!(host_rx.recv().await.unwrap(), ServerMessage::Play { .. }));
    }

    #[tokio::test]
    async fn leave_room_removes_empty_room() {
        let rooms = new_room_map();
        let (host, _rx) = make_participant("Alice");
        let host_id = host.id;
        create_room(&rooms, "testroom".into(), make_item(), host).await.unwrap();

        leave_room(&rooms, "testroom", host_id).await;

        assert!(!rooms.contains_key("testroom"));
    }

    #[tokio::test]
    async fn leave_room_notifies_remaining() {
        let rooms = new_room_map();
        let (host, mut host_rx) = make_participant("Alice");
        create_room(&rooms, "testroom".into(), make_item(), host).await.unwrap();
        let room_id = "testroom";

        let (guest, _guest_rx) = make_participant("Bob");
        let guest_id = guest.id;
        join_room(&rooms, room_id, guest).await.unwrap();
        let _ = host_rx.recv().await; // drain joined notification

        leave_room(&rooms, room_id, guest_id).await;

        let msg = host_rx.recv().await.unwrap();
        assert!(matches!(msg, ServerMessage::ParticipantLeft { name } if name == "Bob"));
    }
}
