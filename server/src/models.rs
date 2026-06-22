use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Messages from client → server
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Join {
        room_id: Option<String>,
        abs_token: String,
    },
    CreateRoom {
        abs_token: String,
        item_id: String,
        item_title: String,
        item_author: String,
        library_id: String,
    },
    Play {
        position: f64,
    },
    Pause {
        position: f64,
    },
    Seek {
        position: f64,
    },
    Speed {
        rate: f64,
    },
    Ping {
        sent_at: u64,
    },
}

// Messages from server → client
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    RoomState {
        room_id: String,
        item: ItemInfo,
        position: f64,
        playing: bool,
        speed: f64,
        participants: Vec<ParticipantInfo>,
    },
    Play {
        position: f64,
        host_time: u64,
        sender_name: String,
    },
    Pause {
        position: f64,
        sender_name: String,
    },
    Seek {
        position: f64,
        sender_name: String,
    },
    Speed {
        rate: f64,
        sender_name: String,
    },
    Sync {
        position: f64,
    },
    Pong {
        sent_at: u64,
    },
    ParticipantJoined {
        name: String,
    },
    ParticipantLeft {
        name: String,
    },
    HostChanged {
        name: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ItemInfo {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub author: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ParticipantInfo {
    pub name: String,
    pub is_host: bool,
}

// ABS API response types
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsLoginResponse {
    pub user: AbsUser,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsUser {
    pub id: String,
    pub username: String,
    pub token: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AbsMeResponse {
    pub id: String,
    pub username: String,
}

pub type RoomId = String;
pub type ParticipantId = Uuid;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_join_deserializes() {
        let json = r#"{"type":"join","room_id":"abc123","abs_token":"tok"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Join { room_id: Some(_), .. }));
    }

    #[test]
    fn client_message_create_room_deserializes() {
        let json = r#"{"type":"create_room","abs_token":"t","item_id":"i1","item_title":"Book","item_author":"Auth","library_id":"l1"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::CreateRoom { .. }));
    }

    #[test]
    fn client_message_ping_deserializes() {
        let json = r#"{"type":"ping","sent_at":1234567890}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Ping { sent_at: 1234567890 }));
    }

    #[test]
    fn server_message_pong_serializes() {
        let msg = ServerMessage::Pong { sent_at: 42 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"pong""#));
        assert!(json.contains(r#""sent_at":42"#));
    }

    #[test]
    fn server_message_play_serializes() {
        let msg = ServerMessage::Play { position: 123.4, host_time: 1000, sender_name: "Alice".into() };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"play""#));
        assert!(json.contains(r#""position":123.4"#));
    }

    #[test]
    fn server_message_error_serializes() {
        let msg = ServerMessage::Error { message: "oops".into() };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"error""#));
        assert!(json.contains(r#""message":"oops""#));
    }

    #[test]
    fn item_info_roundtrips() {
        let item = ItemInfo { id: "i".into(), library_id: "l".into(), title: "T".into(), author: "A".into() };
        let json = serde_json::to_string(&item).unwrap();
        let back: ItemInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "i");
        assert_eq!(back.title, "T");
    }
}
