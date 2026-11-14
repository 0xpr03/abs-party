# Architecture

## Layout

```
server/src/
  lib.rs         route table, login/config handlers
  proxy.rs       ABS REST proxy; its route table is the allowlist
  ws_handler.rs  per-connection WebSocket loop
  hub.rs         room registry, play/pause/seek/settings handling
  room.rs        Room state, host transfer, broadcast
  models.rs      WebSocket message types, RoomSettings
client/src/
  views/         Home (login), BookPicker (create room), Room (player)
  composables/   useAbsPlayer (hls.js / direct play), useWebSocket, useSyncEngine
  api/abs.js     ABS REST calls via /api/abs-proxy
  utils/roomSettings.js  last-used room settings (localStorage)
```

## HTTP routes

| Route | |
|---|---|
| `GET /ws` | WebSocket |
| `POST /api/abs/login` | Username/password login, forwarded to ABS |
| `POST /api/abs/validate-key` | Validate an ABS API key via `/api/me` (not used by the UI) |
| `/api/abs-proxy/api/...` | ABS REST proxy. Only the routes in `proxy.rs` (others 403, wrong method 405); every segment must be `[A-Za-z0-9_-]+` |
| `GET /api/config` | `{ abs_base_url }` (runtime config for the SPA) |
| `GET /health` | |
| `*` | SPA, falls back to `index.html` |

Covers go through the proxy too: `GET api/items/<id>/cover` is the one path allowed without a Bearer token (ABS serves covers unauthenticated and `<img>` can't send headers); no token is ever forwarded for it. Audio is loaded straight from ABS: HLS (`/hls/...`, no auth) or direct-play files with `?token=`.

## Sync model

- The server keeps the authoritative `position`, `playing` and `speed` for each room and extrapolates the position while playing.
- **Play is two-phase (ready-check):** a `play_intent` makes everyone pause, seek and buffer; each client replies `ready` once its session is open and the audio has data (`utils/playable.js`, 20s fallback); once all have replied, the server broadcasts `play`.
- **Drift correction is client-side:** each client keeps a room clock (`utils/roomClock.js`) from `room_state`/`play`/`pause`/`seek`, and every 2s hard-seeks if it is more than 2.5s off. (The server never sends `sync`.)
- **Autoplay:** a browser may refuse `play()` without a user gesture (e.g. a guest opened the invite link directly). The player sets `blocked` and the room shows a "tap to listen along" button that resumes locally only.
- `pause`/`seek` are applied immediately and sent to everyone except the sender. A `seek` (or `pause`) while a ready-check is pending cancels that play; the play button has to be pressed again.
- **Host** = room creator; on leave, host passes to the next participant in join order (`host_changed`).
- **Permissions:**

  | Message | Who may send |
  |---|---|
  | `play_intent`, `pause`, `seek` | anyone; host only if `host_only_controls` (others get `control_denied` and snap back) |
  | `ready` | anyone (part of the play handshake) |
  | `update_settings` | host only (silently ignored otherwise) |
  | `speed` | host only (protocol only — no UI yet) |

- `RoomSettings.pause_on_join_leave`:
  - on: a join while playing becomes a ready-check including the newcomer (`play_intent` with `joined: true`), so everyone waits for them to load and then resumes together. A join during a pending ready-check adds the newcomer to it. A leave while playing pauses.
  - off: joins and leaves don't touch playback; a newcomer follows the room clock.
  - Either way, a leaver is dropped from a pending ready-check so it can't block play.

## WebSocket protocol

JSON with a `type` field.

Client → server:

```
create_room      { room_id, abs_token, item_id, item_title, item_author, library_id, settings? }
join             { room_id, abs_token }
play_intent      { position }
ready            {}
pause            { position }
seek             { position }
speed            { rate }                       // host only, 0.25–4.0, no UI yet
update_settings  { settings }                   // host only
ping             { sent_at }
```

`create_room` for an ID that already exists joins that room instead, and the existing settings stay.

Server → client:

```
room_state         { room_id, item, position, playing, speed, participants[{name,is_host}], settings }
play_intent        { position, sender_name, joined? }  // joined: sync-pause for a newcomer, resumes automatically
play               { position, host_time, sender_name }
pause              { position, sender_name }
seek               { position, sender_name }
speed              { rate, sender_name }
settings_changed   { settings }
control_denied     { position, playing }       // only to the rejected sender
participant_joined { name }
participant_left   { name }
host_changed       { name }
pong               { sent_at }
error              { message }                 // client treats as fatal
```

`settings` = `{ host_only_controls: bool (false), pause_on_join_leave: bool (true) }`.
