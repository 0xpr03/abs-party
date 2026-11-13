use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use dashmap::DashMap;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::models::{ItemInfo, RoomSettings, ServerMessage};
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
    settings: RoomSettings,
) -> Result<bool, String> {
    validate_room_id(&room_id)?;
    if rooms.contains_key(&room_id) {
        join_room(rooms, &room_id, host).await?;
        return Ok(false);
    }
    let room = Room::new(room_id.clone(), item, host, settings);
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

    let newcomer_id = participant.id;
    let pause_for_join = room.settings.pause_on_join_leave;

    // Sync-pause: if playing, turn the join into a ready-check that includes the newcomer,
    // so everyone pauses until they've loaded and buffered, then resumes together.
    // With pause_on_join_leave off the room keeps going and the newcomer catches up alone.
    let join_check_position = if pause_for_join && room.playing {
        let pos = room.estimated_position();
        room.pause_now();
        let mut expected: HashSet<Uuid> = room.participants.iter().map(|p| p.id).collect();
        expected.insert(newcomer_id);
        room.ready_check = Some(ReadyCheck { position: pos, expected, ready: HashSet::new() });
        Some(pos)
    } else {
        None
    };

    // Push the newcomer first so room_state includes them in the participant list
    room.participants.push(participant);

    let state = room.state_message();
    let _ = tx.send(state);

    room.broadcast_except(newcomer_id, &ServerMessage::ParticipantJoined { name: name.clone() });

    if let Some(position) = join_check_position {
        room.broadcast(&ServerMessage::PlayIntent { position, sender_name: name, joined: true });
    } else if pause_for_join {
        // A play is already waiting on everyone to buffer: make it wait for the newcomer too
        if let Some(check) = room.ready_check.as_mut() {
            check.expected.insert(newcomer_id);
            let position = check.position;
            room.send_to(newcomer_id, ServerMessage::PlayIntent { position, sender_name: String::new(), joined: false });
        }
    }

    Ok(())
}

pub async fn handle_play_intent(rooms: &RoomMap, room_id: &str, sender_id: Uuid, position: f64) {
    if !position.is_finite() || position < 0.0 { return; }
    if let Some(arc) = rooms.get(room_id).map(|r| r.clone()) {
        let mut room = arc.lock().await;
        if !room.may_control(sender_id) {
            room.deny_control(sender_id);
            return;
        }
        let sender_name = room.participants.iter()
            .find(|p| p.id == sender_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        room.pause_now();
        room.position = position;
        let expected: HashSet<Uuid> = room.participants.iter().map(|p| p.id).collect();
        room.ready_check = Some(ReadyCheck { position, expected, ready: HashSet::new() });
        // Broadcast to ALL participants (including sender) so everyone starts buffering
        room.broadcast(&ServerMessage::PlayIntent { position, sender_name, joined: false });
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
        if !room.may_control(sender_id) {
            room.deny_control(sender_id);
            return;
        }
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
        if !room.may_control(sender_id) {
            room.deny_control(sender_id);
            return;
        }
        let sender_name = room.participants.iter()
            .find(|p| p.id == sender_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        room.position = position;
        // Keep the clock running if we're playing, otherwise estimated_position freezes here
        room.play_started_at = if room.playing { Some(Instant::now()) } else { None };
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

pub async fn handle_update_settings(rooms: &RoomMap, room_id: &str, sender_id: Uuid, settings: RoomSettings) {
    if let Some(arc) = rooms.get(room_id).map(|r| r.clone()) {
        let mut room = arc.lock().await;
        if !room.is_host(sender_id) {
            return;
        }
        room.settings = settings;
        room.broadcast(&ServerMessage::SettingsChanged { settings });
    }
}

pub async fn leave_room(rooms: &RoomMap, room_id: &str, participant_id: Uuid) {
    let should_remove = if let Some(arc) = rooms.get(room_id).map(|r| r.clone()) {
        let mut room = arc.lock().await;
        if let Some(name) = room.remove_participant(participant_id) {
            room.broadcast(&ServerMessage::ParticipantLeft { name: name.clone() });

            if !room.is_empty() {
                if room.playing && room.settings.pause_on_join_leave {
                    let pos = room.estimated_position();
                    room.pause_now();
                    room.broadcast(&ServerMessage::Pause {
                        position: pos,
                        sender_name: format!("{} left", name),
                    });
                } else if room.ready_check.is_some() {
                    // Don't wait on the leaver's ready
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
        let created = create_room(&rooms, "testroom".into(), make_item(), host, RoomSettings::default()).await.unwrap();
        assert!(created); // new room
        assert!(rooms.contains_key("testroom"));
    }

    #[tokio::test]
    async fn create_room_joins_existing_room() {
        let rooms = new_room_map();
        let (host, _host_rx) = make_participant("Alice");
        create_room(&rooms, "testroom".into(), make_item(), host, RoomSettings::default()).await.unwrap();

        let (guest, mut guest_rx) = make_participant("Bob");
        let joined = create_room(&rooms, "testroom".into(), make_item(), guest, RoomSettings::default()).await.unwrap();
        assert!(!joined); // joined existing
        // join_room sends room_state to the newcomer
        let msg = guest_rx.recv().await.unwrap();
        assert!(matches!(msg, ServerMessage::RoomState { .. }));
    }

    #[tokio::test]
    async fn create_room_rejects_invalid_id() {
        let rooms = new_room_map();
        let (host, _rx) = make_participant("Alice");
        let result = create_room(&rooms, "bad!id".into(), make_item(), host, RoomSettings::default()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn join_room_sends_state_to_newcomer() {
        let rooms = new_room_map();
        let (host, _host_rx) = make_participant("Alice");
        create_room(&rooms, "testroom".into(), make_item(), host, RoomSettings::default()).await.unwrap();

        let (guest, mut guest_rx) = make_participant("Bob");
        join_room(&rooms, "testroom", guest).await.unwrap();

        let msg = guest_rx.recv().await.unwrap();
        assert!(matches!(msg, ServerMessage::RoomState { .. }));
    }

    #[tokio::test]
    async fn join_room_notifies_existing_participants() {
        let rooms = new_room_map();
        let (host, mut host_rx) = make_participant("Alice");
        create_room(&rooms, "testroom".into(), make_item(), host, RoomSettings::default()).await.unwrap();

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
        create_room(&rooms, "testroom".into(), make_item(), host, RoomSettings::default()).await.unwrap();
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
        create_room(&rooms, "testroom".into(), make_item(), host, RoomSettings::default()).await.unwrap();
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
        create_room(&rooms, "testroom".into(), make_item(), host, RoomSettings::default()).await.unwrap();

        leave_room(&rooms, "testroom", host_id).await;

        assert!(!rooms.contains_key("testroom"));
    }

    #[tokio::test]
    async fn leave_room_notifies_remaining() {
        let rooms = new_room_map();
        let (host, mut host_rx) = make_participant("Alice");
        create_room(&rooms, "testroom".into(), make_item(), host, RoomSettings::default()).await.unwrap();
        let room_id = "testroom";

        let (guest, _guest_rx) = make_participant("Bob");
        let guest_id = guest.id;
        join_room(&rooms, room_id, guest).await.unwrap();
        let _ = host_rx.recv().await; // drain joined notification

        leave_room(&rooms, room_id, guest_id).await;

        let msg = host_rx.recv().await.unwrap();
        assert!(matches!(msg, ServerMessage::ParticipantLeft { name } if name == "Bob"));
    }

    /// Room "testroom" with host Alice and guest Bob, all setup messages drained.
    async fn two_person_room(
        settings: RoomSettings,
    ) -> (RoomMap, Uuid, tokio::sync::mpsc::UnboundedReceiver<ServerMessage>, Uuid, tokio::sync::mpsc::UnboundedReceiver<ServerMessage>) {
        let rooms = new_room_map();
        let (host, mut host_rx) = make_participant("Alice");
        let host_id = host.id;
        create_room(&rooms, "testroom".into(), make_item(), host, settings).await.unwrap();
        let (guest, mut guest_rx) = make_participant("Bob");
        let guest_id = guest.id;
        join_room(&rooms, "testroom", guest).await.unwrap();
        while host_rx.try_recv().is_ok() {}
        while guest_rx.try_recv().is_ok() {}
        (rooms, host_id, host_rx, guest_id, guest_rx)
    }

    fn host_only() -> RoomSettings {
        RoomSettings { host_only_controls: true, pause_on_join_leave: true }
    }

    #[tokio::test]
    async fn create_room_stores_settings_in_room_state() {
        let rooms = new_room_map();
        let (host, _rx) = make_participant("Alice");
        let settings = RoomSettings { host_only_controls: true, pause_on_join_leave: false };
        create_room(&rooms, "testroom".into(), make_item(), host, settings).await.unwrap();
        let (guest, mut guest_rx) = make_participant("Bob");
        join_room(&rooms, "testroom", guest).await.unwrap();
        let msg = guest_rx.recv().await.unwrap();
        assert!(matches!(msg, ServerMessage::RoomState { settings: s, .. } if s == settings));
    }

    #[tokio::test]
    async fn rejoining_existing_room_keeps_original_settings() {
        let rooms = new_room_map();
        let (host, _rx) = make_participant("Alice");
        create_room(&rooms, "testroom".into(), make_item(), host, host_only()).await.unwrap();
        let (guest, _guest_rx) = make_participant("Bob");
        create_room(&rooms, "testroom".into(), make_item(), guest, RoomSettings::default()).await.unwrap();
        let room = rooms.get("testroom").unwrap().clone();
        assert_eq!(room.lock().await.settings, host_only());
    }

    #[tokio::test]
    async fn host_only_rejects_guest_play_pause_seek() {
        let (rooms, _host_id, mut host_rx, guest_id, mut guest_rx) = two_person_room(host_only()).await;

        handle_play_intent(&rooms, "testroom", guest_id, 10.0).await;
        handle_pause(&rooms, "testroom", guest_id, 20.0).await;
        handle_seek(&rooms, "testroom", guest_id, 30.0).await;

        // Guest gets a ControlDenied for each attempt with the unchanged room position
        for _ in 0..3 {
            let msg = guest_rx.recv().await.unwrap();
            assert!(matches!(msg, ServerMessage::ControlDenied { position, playing: false } if position == 0.0));
        }
        // Host saw nothing, and room state is untouched
        assert!(host_rx.try_recv().is_err());
        let room = rooms.get("testroom").unwrap().clone();
        let room = room.lock().await;
        assert_eq!(room.position, 0.0);
        assert!(room.ready_check.is_none());
    }

    #[tokio::test]
    async fn host_only_still_allows_host_controls() {
        let (rooms, host_id, _host_rx, _guest_id, mut guest_rx) = two_person_room(host_only()).await;

        handle_seek(&rooms, "testroom", host_id, 42.0).await;
        assert!(matches!(guest_rx.recv().await.unwrap(), ServerMessage::Seek { position, .. } if position == 42.0));

        handle_play_intent(&rooms, "testroom", host_id, 42.0).await;
        assert!(matches!(guest_rx.recv().await.unwrap(), ServerMessage::PlayIntent { .. }));
    }

    #[tokio::test]
    async fn guests_control_when_host_only_disabled() {
        let (rooms, _host_id, mut host_rx, guest_id, _guest_rx) = two_person_room(RoomSettings::default()).await;
        handle_seek(&rooms, "testroom", guest_id, 42.0).await;
        assert!(matches!(host_rx.recv().await.unwrap(), ServerMessage::Seek { position, .. } if position == 42.0));
    }

    #[tokio::test]
    async fn host_can_update_settings_and_all_are_notified() {
        let (rooms, host_id, mut host_rx, guest_id, mut guest_rx) = two_person_room(RoomSettings::default()).await;

        handle_update_settings(&rooms, "testroom", host_id, host_only()).await;
        for rx in [&mut host_rx, &mut guest_rx] {
            assert!(matches!(rx.recv().await.unwrap(), ServerMessage::SettingsChanged { settings } if settings == host_only()));
        }

        // New setting takes effect immediately
        handle_seek(&rooms, "testroom", guest_id, 5.0).await;
        assert!(matches!(guest_rx.recv().await.unwrap(), ServerMessage::ControlDenied { .. }));
    }

    #[tokio::test]
    async fn guest_cannot_update_settings() {
        let (rooms, _host_id, mut host_rx, guest_id, mut guest_rx) = two_person_room(RoomSettings::default()).await;

        handle_update_settings(&rooms, "testroom", guest_id, host_only()).await;

        assert!(host_rx.try_recv().is_err());
        assert!(guest_rx.try_recv().is_err());
        let room = rooms.get("testroom").unwrap().clone();
        assert_eq!(room.lock().await.settings, RoomSettings::default());
    }

    #[tokio::test]
    async fn host_only_follows_host_transfer() {
        let (rooms, host_id, _host_rx, guest_id, mut guest_rx) = two_person_room(host_only()).await;
        leave_room(&rooms, "testroom", host_id).await;
        while guest_rx.try_recv().is_ok() {}

        // Bob is now host and may control
        handle_seek(&rooms, "testroom", guest_id, 7.0).await;
        assert!(guest_rx.try_recv().is_err()); // seek is not echoed to sender, and no denial
        let room = rooms.get("testroom").unwrap().clone();
        assert_eq!(room.lock().await.position, 7.0);
    }

    async fn start_playing(rooms: &RoomMap, ids: &[Uuid]) {
        handle_play_intent(rooms, "testroom", ids[0], 0.0).await;
        for id in ids {
            handle_ready(rooms, "testroom", *id).await;
        }
    }

    #[tokio::test]
    async fn join_sync_pauses_until_newcomer_ready_then_resumes() {
        let (rooms, host_id, mut host_rx, guest_id, mut guest_rx) = two_person_room(RoomSettings::default()).await;
        start_playing(&rooms, &[host_id, guest_id]).await;
        while host_rx.try_recv().is_ok() {}
        while guest_rx.try_recv().is_ok() {}

        let (carol, mut carol_rx) = make_participant("Carol");
        let carol_id = carol.id;
        join_room(&rooms, "testroom", carol).await.unwrap();

        // Newcomer gets a paused room_state, then everyone gets a join ready-check
        assert!(matches!(carol_rx.recv().await.unwrap(), ServerMessage::RoomState { playing: false, .. }));
        assert!(matches!(host_rx.recv().await.unwrap(), ServerMessage::ParticipantJoined { .. }));
        assert!(matches!(guest_rx.recv().await.unwrap(), ServerMessage::ParticipantJoined { .. }));
        for rx in [&mut host_rx, &mut guest_rx, &mut carol_rx] {
            assert!(matches!(rx.recv().await.unwrap(),
                ServerMessage::PlayIntent { joined: true, sender_name, .. } if sender_name == "Carol"));
        }
        let room = rooms.get("testroom").unwrap().clone();
        assert!(!room.lock().await.playing);

        // Existing listeners are ready, but play waits for the newcomer
        handle_ready(&rooms, "testroom", host_id).await;
        handle_ready(&rooms, "testroom", guest_id).await;
        assert!(host_rx.try_recv().is_err());

        handle_ready(&rooms, "testroom", carol_id).await;
        for rx in [&mut host_rx, &mut guest_rx, &mut carol_rx] {
            assert!(matches!(rx.recv().await.unwrap(), ServerMessage::Play { .. }));
        }
        assert!(room.lock().await.playing);
    }

    #[tokio::test]
    async fn join_while_paused_does_not_start_ready_check() {
        let (rooms, _host_id, mut host_rx, _guest_id, _guest_rx) = two_person_room(RoomSettings::default()).await;
        let (carol, _carol_rx) = make_participant("Carol");
        join_room(&rooms, "testroom", carol).await.unwrap();
        assert!(matches!(host_rx.recv().await.unwrap(), ServerMessage::ParticipantJoined { .. }));
        assert!(host_rx.try_recv().is_err());
        let room = rooms.get("testroom").unwrap().clone();
        assert!(room.lock().await.ready_check.is_none());
    }

    #[tokio::test]
    async fn join_during_pending_play_waits_for_newcomer() {
        let (rooms, host_id, mut host_rx, guest_id, _guest_rx) = two_person_room(RoomSettings::default()).await;
        handle_play_intent(&rooms, "testroom", host_id, 30.0).await;
        handle_ready(&rooms, "testroom", host_id).await;
        while host_rx.try_recv().is_ok() {}

        let (carol, mut carol_rx) = make_participant("Carol");
        let carol_id = carol.id;
        join_room(&rooms, "testroom", carol).await.unwrap();
        assert!(matches!(carol_rx.recv().await.unwrap(), ServerMessage::RoomState { .. }));
        assert!(matches!(carol_rx.recv().await.unwrap(), ServerMessage::PlayIntent { position, .. } if position == 30.0));

        handle_ready(&rooms, "testroom", guest_id).await;
        assert!(!matches!(host_rx.try_recv(), Ok(ServerMessage::Play { .. })));
        handle_ready(&rooms, "testroom", carol_id).await;
        assert!(matches!(carol_rx.recv().await.unwrap(), ServerMessage::Play { position, .. } if position == 30.0));
    }

    #[tokio::test]
    async fn join_during_pending_play_does_not_wait_when_pause_disabled() {
        let settings = RoomSettings { host_only_controls: false, pause_on_join_leave: false };
        let (rooms, host_id, _host_rx, guest_id, _guest_rx) = two_person_room(settings).await;
        handle_play_intent(&rooms, "testroom", host_id, 30.0).await;
        handle_ready(&rooms, "testroom", host_id).await;

        let (carol, mut carol_rx) = make_participant("Carol");
        join_room(&rooms, "testroom", carol).await.unwrap();
        assert!(matches!(carol_rx.recv().await.unwrap(), ServerMessage::RoomState { .. }));
        assert!(carol_rx.try_recv().is_err()); // not part of the ready-check

        handle_ready(&rooms, "testroom", guest_id).await;
        assert!(matches!(carol_rx.recv().await.unwrap(), ServerMessage::Play { .. }));
    }

    #[tokio::test]
    async fn join_and_leave_keep_playing_when_pause_disabled() {
        let settings = RoomSettings { host_only_controls: false, pause_on_join_leave: false };
        let (rooms, host_id, mut host_rx, guest_id, _guest_rx) = two_person_room(settings).await;
        start_playing(&rooms, &[host_id, guest_id]).await;
        while host_rx.try_recv().is_ok() {}

        let (carol, mut carol_rx) = make_participant("Carol");
        let carol_id = carol.id;
        join_room(&rooms, "testroom", carol).await.unwrap();
        assert!(matches!(host_rx.recv().await.unwrap(), ServerMessage::ParticipantJoined { .. }));
        assert!(matches!(carol_rx.recv().await.unwrap(), ServerMessage::RoomState { playing: true, .. }));

        leave_room(&rooms, "testroom", carol_id).await;
        assert!(matches!(host_rx.recv().await.unwrap(), ServerMessage::ParticipantLeft { .. }));
        assert!(host_rx.try_recv().is_err());

        let room = rooms.get("testroom").unwrap().clone();
        assert!(room.lock().await.playing);
    }

    #[tokio::test]
    async fn leave_pauses_playback_by_default() {
        let (rooms, host_id, mut host_rx, guest_id, _guest_rx) = two_person_room(RoomSettings::default()).await;
        start_playing(&rooms, &[host_id, guest_id]).await;
        while host_rx.try_recv().is_ok() {}

        leave_room(&rooms, "testroom", guest_id).await;
        assert!(matches!(host_rx.recv().await.unwrap(), ServerMessage::ParticipantLeft { .. }));
        assert!(matches!(host_rx.recv().await.unwrap(), ServerMessage::Pause { .. }));
    }

    #[tokio::test]
    async fn seek_while_playing_keeps_position_advancing() {
        let (rooms, host_id, _host_rx, guest_id, _guest_rx) = two_person_room(RoomSettings::default()).await;
        start_playing(&rooms, &[host_id, guest_id]).await;

        handle_seek(&rooms, "testroom", host_id, 100.0).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let room = rooms.get("testroom").unwrap().clone();
        let room = room.lock().await;
        assert!(room.playing);
        assert!(room.estimated_position() > 100.0);
    }
}
