use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::models::{ItemInfo, ParticipantInfo, ServerMessage};

pub struct Participant {
    pub id: Uuid,
    pub name: String,
    pub abs_token: String,
    pub abs_username: String,
    pub tx: mpsc::UnboundedSender<ServerMessage>,
}

pub struct Room {
    pub id: String,
    pub item: ItemInfo,
    pub position: f64,
    pub playing: bool,
    pub speed: f64,
    pub host_id: Uuid,
    pub participants: Vec<Participant>,
}

impl Room {
    pub fn new(id: String, item: ItemInfo, host: Participant) -> Self {
        Self {
            id,
            item,
            position: 0.0,
            playing: false,
            speed: 1.0,
            host_id: host.id,
            participants: vec![host],
        }
    }

    pub fn participant_infos(&self) -> Vec<ParticipantInfo> {
        self.participants
            .iter()
            .map(|p| ParticipantInfo {
                name: p.name.clone(),
                is_host: p.id == self.host_id,
            })
            .collect()
    }

    pub fn broadcast(&self, msg: &ServerMessage) {
        for p in &self.participants {
            let _ = p.tx.send(msg.clone());
        }
    }

    pub fn broadcast_except(&self, exclude_id: Uuid, msg: &ServerMessage) {
        for p in &self.participants {
            if p.id != exclude_id {
                let _ = p.tx.send(msg.clone());
            }
        }
    }

    pub fn send_to(&self, participant_id: Uuid, msg: ServerMessage) {
        if let Some(p) = self.participants.iter().find(|p| p.id == participant_id) {
            let _ = p.tx.send(msg);
        }
    }

    pub fn remove_participant(&mut self, id: Uuid) -> Option<String> {
        if let Some(pos) = self.participants.iter().position(|p| p.id == id) {
            let name = self.participants.remove(pos).name;
            // Transfer host if needed
            if self.host_id == id {
                if let Some(next) = self.participants.first() {
                    self.host_id = next.id;
                    let new_host_name = next.name.clone();
                    self.broadcast(&ServerMessage::HostChanged {
                        name: new_host_name,
                    });
                }
            }
            Some(name)
        } else {
            None
        }
    }

    pub fn is_empty(&self) -> bool {
        self.participants.is_empty()
    }

    pub fn is_host(&self, id: Uuid) -> bool {
        self.host_id == id
    }

    pub fn state_message(&self) -> ServerMessage {
        ServerMessage::RoomState {
            room_id: self.id.clone(),
            item: self.item.clone(),
            position: self.position,
            playing: self.playing,
            speed: self.speed,
            participants: self.participant_infos(),
        }
    }
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;
    use crate::models::ItemInfo;
    use tokio::sync::mpsc;

    pub fn make_item() -> ItemInfo {
        ItemInfo { id: "item1".into(), library_id: "lib1".into(), title: "Book".into(), author: "Author".into() }
    }

    pub fn make_participant(name: &str) -> (Participant, mpsc::UnboundedReceiver<ServerMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Participant { id: Uuid::new_v4(), name: name.into(), abs_token: "tok".into(), abs_username: name.into(), tx },
            rx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_helpers::*;
    use crate::models::ServerMessage;

    #[test]
    fn new_room_sets_host() {
        let (host, _rx) = make_participant("Alice");
        let host_id = host.id;
        let room = Room::new("r1".into(), make_item(), host);
        assert!(room.is_host(host_id));
        assert_eq!(room.participants.len(), 1);
    }

    #[test]
    fn participant_infos_marks_host() {
        let (host, _rx) = make_participant("Alice");
        let room = Room::new("r1".into(), make_item(), host);
        let infos = room.participant_infos();
        assert_eq!(infos.len(), 1);
        assert!(infos[0].is_host);
        assert_eq!(infos[0].name, "Alice");
    }

    #[test]
    fn remove_non_host_participant() {
        let (host, _host_rx) = make_participant("Alice");
        let (guest, _guest_rx) = make_participant("Bob");
        let guest_id = guest.id;
        let host_id = host.id;
        let mut room = Room::new("r1".into(), make_item(), host);
        room.participants.push(guest);

        let name = room.remove_participant(guest_id);
        assert_eq!(name, Some("Bob".into()));
        assert!(room.is_host(host_id)); // host unchanged
        assert_eq!(room.participants.len(), 1);
    }

    #[test]
    fn host_removal_transfers_to_next_participant() {
        let (host, _host_rx) = make_participant("Alice");
        let (guest, mut guest_rx) = make_participant("Bob");
        let host_id = host.id;
        let guest_id = guest.id;
        let mut room = Room::new("r1".into(), make_item(), host);
        room.participants.push(guest);

        room.remove_participant(host_id);

        // Bob should now be host
        assert!(room.is_host(guest_id));
        // Bob should have received a HostChanged message
        let msg = guest_rx.try_recv().unwrap();
        assert!(matches!(msg, ServerMessage::HostChanged { name } if name == "Bob"));
    }

    #[test]
    fn is_empty_after_all_leave() {
        let (host, _rx) = make_participant("Alice");
        let host_id = host.id;
        let mut room = Room::new("r1".into(), make_item(), host);
        assert!(!room.is_empty());
        room.remove_participant(host_id);
        assert!(room.is_empty());
    }

    #[test]
    fn state_message_reflects_room_state() {
        let (host, _rx) = make_participant("Alice");
        let mut room = Room::new("r1".into(), make_item(), host);
        room.position = 42.0;
        room.playing = true;
        room.speed = 1.5;

        let msg = room.state_message();
        assert!(matches!(msg, ServerMessage::RoomState { position, playing, speed, .. }
            if (position - 42.0).abs() < f64::EPSILON && playing && (speed - 1.5).abs() < f64::EPSILON));
    }
}
