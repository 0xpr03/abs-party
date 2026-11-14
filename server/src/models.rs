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
        room_id: String,
        abs_token: String,
        // Typed so only real ABS IDs get in: it ends up in proxied ABS URLs on every
        // guest's client, with the guest's token. Title, author and library are looked up
        // from ABS with the host's token (older clients still send them; serde ignores them).
        item_id: Uuid,
        #[serde(default)]
        settings: RoomSettings,
    },
    UpdateSettings {
        settings: RoomSettings,
    },
    PlayIntent {
        position: f64,
    },
    Ready,
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
        settings: RoomSettings,
    },
    SettingsChanged {
        settings: RoomSettings,
    },
    /// Sent only to a participant whose play/pause/seek was rejected,
    /// so they can snap back to the authoritative room state.
    ControlDenied {
        position: f64,
        playing: bool,
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
    PlayIntent {
        position: f64,
        sender_name: String,
        /// True when this ready-check was started because `sender_name` joined a playing room;
        /// playback resumes automatically once everyone (including the newcomer) is ready.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        joined: bool,
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

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(default)]
pub struct RoomSettings {
    /// Only the host may play, pause or seek.
    pub host_only_controls: bool,
    /// Pause playback for everyone when a participant joins or leaves.
    pub pause_on_join_leave: bool,
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self { host_only_controls: false, pause_on_join_leave: true }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ItemInfo {
    pub id: Uuid,
    pub library_id: Uuid,
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

/// `GET /api/items/:id?expanded=1&include=progress` — only the fields a room needs
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsItemResponse {
    pub id: Uuid,
    pub library_id: Uuid,
    pub media: AbsItemMedia,
    /// The requesting user's progress; absent/null if they never started it
    #[serde(default)]
    pub user_media_progress: Option<AbsMediaProgress>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsMediaProgress {
    #[serde(default)]
    pub current_time: f64,
    #[serde(default)]
    pub is_finished: bool,
}

impl AbsItemResponse {
    /// Where ABS would start this user's playback session (PlaybackSessionManager:
    /// a finished item restarts from 0)
    pub fn start_position(&self) -> f64 {
        match &self.user_media_progress {
            Some(p) if !p.is_finished => p.current_time,
            _ => 0.0,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AbsItemMedia {
    pub metadata: AbsItemMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsItemMetadata {
    pub title: Option<String>,
    /// Books
    pub author_name: Option<String>,
    /// Podcasts
    pub author: Option<String>,
}

impl From<AbsItemResponse> for ItemInfo {
    fn from(r: AbsItemResponse) -> Self {
        let m = r.media.metadata;
        ItemInfo {
            id: r.id,
            library_id: r.library_id,
            title: m.title.filter(|t| !t.is_empty()).unwrap_or_else(|| "Untitled".into()),
            author: m.author_name.or(m.author).unwrap_or_default(),
        }
    }
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
        let json = r#"{"type":"create_room","room_id":"abcd1234","abs_token":"t","item_id":"09c4e9ef-c12a-432c-b92f-88cc47add86a"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::CreateRoom { settings, .. } if settings == RoomSettings::default()));
    }

    #[test]
    fn client_message_create_room_with_settings_deserializes() {
        let json = r#"{"type":"create_room","room_id":"abcd1234","abs_token":"t","item_id":"09c4e9ef-c12a-432c-b92f-88cc47add86a","settings":{"host_only_controls":true,"pause_on_join_leave":false}}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::CreateRoom { settings, .. }
            if settings.host_only_controls && !settings.pause_on_join_leave));
    }

    #[test]
    fn client_message_update_settings_deserializes() {
        let json = r#"{"type":"update_settings","settings":{"host_only_controls":true}}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::UpdateSettings { settings }
            if settings.host_only_controls && settings.pause_on_join_leave));
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
        let item = ItemInfo { id: Uuid::new_v4(), library_id: Uuid::new_v4(), title: "T".into(), author: "A".into() };
        let json = serde_json::to_string(&item).unwrap();
        let back: ItemInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, item.id);
        assert_eq!(back.title, "T");
    }

    fn create_room_json(item_id: &str) -> String {
        format!(r#"{{"type":"create_room","room_id":"abcd1234","abs_token":"t","item_id":"{item_id}"}}"#)
    }

    #[test]
    fn create_room_rejects_non_uuid_item_ids() {
        for bad in [
            "item1",
            "..",
            "..%2Fbackups%2F09c4e9ef-c12a-432c-b92f-88cc47add86a%2Fapply",
            "09c4e9ef-c12a-432c-b92f-88cc47add86a/../../users",
            "09c4e9ef-c12a-432c-b92f-88cc47add86a%2F..",
            "li_abc123",
            "",
        ] {
            assert!(serde_json::from_str::<ClientMessage>(&create_room_json(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn create_room_item_id_is_reserialized_canonically() {
        // Uuid also parses braced/urn/uppercase forms; what we send on is always plain hyphenated
        for alt in ["{09C4E9EF-C12A-432C-B92F-88CC47ADD86A}", "urn:uuid:09c4e9ef-c12a-432c-b92f-88cc47add86a", "09c4e9efc12a432cb92f88cc47add86a"] {
            let msg: ClientMessage = serde_json::from_str(&create_room_json(alt)).unwrap();
            let ClientMessage::CreateRoom { item_id, .. } = msg else { panic!() };
            assert_eq!(serde_json::to_value(item_id).unwrap(), "09c4e9ef-c12a-432c-b92f-88cc47add86a", "{alt}");
        }
    }

    #[test]
    fn abs_item_maps_book_and_podcast_metadata() {
        let book = r#"{"id":"09c4e9ef-c12a-432c-b92f-88cc47add86a","libraryId":"7f6a2b1c-3d4e-4f50-8a9b-0c1d2e3f4a5b","mediaType":"book",
            "media":{"metadata":{"title":"Dune","authorName":"Frank Herbert","authors":[{"id":"x","name":"Frank Herbert"}]},"chapters":[]}}"#;
        let item: ItemInfo = serde_json::from_str::<AbsItemResponse>(book).unwrap().into();
        assert_eq!((item.title.as_str(), item.author.as_str()), ("Dune", "Frank Herbert"));
        assert_eq!(item.library_id.to_string(), "7f6a2b1c-3d4e-4f50-8a9b-0c1d2e3f4a5b");

        let podcast = r#"{"id":"09c4e9ef-c12a-432c-b92f-88cc47add86a","libraryId":"7f6a2b1c-3d4e-4f50-8a9b-0c1d2e3f4a5b","mediaType":"podcast",
            "media":{"metadata":{"title":"","author":"Some Network"}}}"#;
        let item: ItemInfo = serde_json::from_str::<AbsItemResponse>(podcast).unwrap().into();
        assert_eq!((item.title.as_str(), item.author.as_str()), ("Untitled", "Some Network"));
    }

    #[test]
    fn abs_item_start_position_follows_abs_session_rules() {
        let with = |progress: &str| {
            let json = format!(r#"{{"id":"09c4e9ef-c12a-432c-b92f-88cc47add86a","libraryId":"7f6a2b1c-3d4e-4f50-8a9b-0c1d2e3f4a5b",
                "media":{{"metadata":{{"title":"T"}}}}{progress}}}"#);
            serde_json::from_str::<AbsItemResponse>(&json).unwrap().start_position()
        };
        assert_eq!(with(r#","userMediaProgress":{"currentTime":512.25,"isFinished":false}"#), 512.25);
        assert_eq!(with(r#","userMediaProgress":{"currentTime":9000,"isFinished":true}"#), 0.0);
        assert_eq!(with(r#","userMediaProgress":null"#), 0.0);
        assert_eq!(with(""), 0.0);
    }
}
