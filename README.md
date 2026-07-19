# abs-party

Synchronized audiobook listening for Audiobookshelf. Multiple people listen to the same book in real time: one person controls play, pause, seek, and speed; everyone else follows automatically.

Each listener streams audio directly from their own Audiobookshelf account and their progress is saved there independently. The party server is a lightweight sync bus only but proxies requests to avoid CORS issues.


## How it works

Users authenticate with their Audiobookshelf credentials (username/password or API key). The host creates a room and picks a book from a searchable library browser. Guests join by room code. The host's playback controls are broadcast over WebSocket to all participants, who each have their own ABS playback session open.

Before joining, each participant's current position is saved as an ABS bookmark (`pre_party_<datetime>`), so they can return to exactly where they were if the session moves their progress.

Rooms are in-memory and ephemeral -- they disappear when the last participant leaves.


## Stack

- Backend: Rust (axum 0.7, tokio), single binary
- Frontend: Vue 3 + Vite SPA, served as static files from the same binary
- No database


## Book picker

When creating a room the host browses their ABS library. The picker supports:

- **Sort** by last listened, title (A–Z), or date added
- **Search** by title or author — uses the ABS server-side search API so results are not limited to the first page of items
- **Pagination** for browsed (non-search) results

Switching sort mode and typing in the search box are coordinated: the active sort applies to search results, and changing the sort re-runs the current search query.


## Security notes

All ABS REST calls from the browser go through the `/api/abs-proxy/*` route on the party server rather than directly to ABS. This avoids CORS issues and lets the server enforce an allowlist of permitted endpoints. Only read and session-management calls are forwarded; write/delete/admin paths are blocked.

Display names are taken from the ABS username and cannot be overridden. Two participants with the same ABS username cannot be in the same room simultaneously.


## Environment variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `ABS_BASE_URL` | yes | `http://localhost:13378` | Base URL of your ABS instance, no trailing slash |
| `BIND` | no | `0.0.0.0:3456` | TCP address to listen on |
| `PORT` | no | -- | Alternative to BIND; sets port on `0.0.0.0` |
| `STATIC_DIR` | no | `./static` | Path to compiled Vue SPA files |
| `CORS_ORIGIN` | no | -- | Allowed cross-origin (e.g. `http://localhost:5173` for Vite dev). Omit in production. |
| `RUST_LOG` | no | -- | Log filter (e.g. `abs_party=info`) |
| `BYPASS_AUTH` | no | `0` | Set to `1` to skip ABS token validation. Never use in production. |

Copy `abs-party.env.example` and edit it before deploying.


## Docker with lima/nerdctl

Build and start:

```sh
lima nerdctl compose down
lima nerdctl compose build --no-cache
lima nerdctl compose up -d
lima nerdctl compose logs -f
```

The `down` step is required. Running `up` without it reuses the old container and silently ignores the new image.

`--no-cache` is needed when source files have changed between builds.

To set the ABS URL, either edit `docker-compose.yml` directly or create a `.env` file next to it:

```
ABS_BASE_URL=https://your-abs-instance.example.com
```


## Systemd (binary deployment)

Build the binary and Vue SPA manually, then install:

```sh
sudo useradd -r -s /sbin/nologin abs-party
sudo mkdir -p /opt/abs-party/static /etc/abs-party
sudo cp abs-party abs-party.env.example /opt/abs-party/
sudo cp abs-party.service /etc/systemd/system/
sudo cp abs-party.env.example /etc/abs-party/env
# edit /etc/abs-party/env
sudo systemctl daemon-reload
sudo systemctl enable --now abs-party
```

Logs: `journalctl -u abs-party -f`


## Development

Backend (requires Rust 1.85+):

```sh
cd server
cargo run
# or
cargo test
```

Frontend (runs on port 5173, proxies API calls to the backend on 3456):

```sh
cd client
npm install
npm run dev
```

Set `CORS_ORIGIN=http://localhost:5173` in the backend environment when running both separately.


## WebSocket protocol

The browser connects to `GET /ws`. All messages are JSON with a `type` field.

Client to server:

```
{ "type": "create_room", "abs_token": "...", "item_id": "...", "item_title": "...", "item_author": "...", "library_id": "..." }
{ "type": "join",        "abs_token": "...", "room_id": "abc12345" }
{ "type": "play",        "position": 1234.5 }
{ "type": "pause",       "position": 1234.5 }
{ "type": "seek",        "position": 1234.5 }
{ "type": "speed",       "rate": 1.25 }
{ "type": "ping",        "sent_at": 1719000000000 }
```

Server to client:

```
{ "type": "room_state",        "room_id": "...", "item": {...}, "position": 0.0, "playing": false, "speed": 1.0, "participants": [...] }
{ "type": "play",              "position": 1234.5, "host_time": 1719000000000 }
{ "type": "pause",             "position": 1234.5 }
{ "type": "seek",              "position": 1234.5 }
{ "type": "speed",             "rate": 1.25 }
{ "type": "participant_joined","name": "alice" }
{ "type": "participant_left",  "name": "alice" }
{ "type": "host_changed",      "name": "alice" }
{ "type": "pong",              "sent_at": 1719000000000 }
{ "type": "error",             "message": "..." }
```

Only the host (room creator, or whoever took over after the host left) may send play/pause/seek/speed. Guest attempts are silently dropped by the server.
