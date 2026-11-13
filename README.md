# abs-party

Listen to an audiobook together on a shared [Audiobookshelf](https://www.audiobookshelf.org/) instance. One person (the host) opens a room with a book, others join by link or room code, and playback stays in sync for everyone.

## How it works

- **Log in with your ABS account.** The party server forwards the login to ABS; there are no separate accounts.
- **Everyone streams from ABS themselves.** Each listener has their own ABS playback session, so progress is saved to their own account. The party server never touches audio.
- **The party server is a sync bus.** A WebSocket per listener relays play/pause/seek; play waits until everyone has buffered, and each listener corrects itself if it drifts more than 2.5s from the room.
- **ABS API calls go through a reverse proxy** (`/api/abs-proxy/*`) to avoid CORS. Only an allowlist of read and playback-session endpoints is forwarded.
- **Safety bookmarks:** when you open a room, your own ABS position is saved as a bookmark (`pre_party_<datetime>`); clicking Leave saves another (`leave_<datetime>`). The room sidebar lists your bookmarks for the book; clicking one seeks everyone there.
- **Who controls playback:** by default *everyone* in the room can play, pause and seek, and it applies to all listeners. The host can restrict this to themselves.
- **The host** is whoever created the room. If they leave or lose their connection, the longest-present listener becomes host and keeps it (reconnecting doesn't give it back).
- **Room settings** (only the host can change them; set when creating the room, toggleable in the room; your last choice is remembered for your next room):

  | Setting | Default | |
  |---|---|---|
  | Only host controls playback | off | Guests can't play, pause or seek |
  | Pause on join / leave | on | A join pauses everyone until the newcomer has loaded and buffered, then playback resumes on its own. A leave pauses. When off, playback continues and newcomers catch up on their own |
- **Rooms live in memory** and disappear when the last person leaves. No database.

Stack: Rust (axum) server that also serves the Vue 3 SPA. See [ARCHITECTURE.md](ARCHITECTURE.md) for internals.

## Running

Configuration is through environment variables:

| Variable | Default | |
|---|---|---|
| `ABS_BASE_URL` | `http://localhost:13378` | Your ABS instance, no trailing slash (**required**) |
| `BIND` / `PORT` | `0.0.0.0:3456` | Listen address, or just the port |
| `STATIC_DIR` | `./static` | Built SPA |
| `RUST_LOG` | | e.g. `abs_party=info` |
| `CORS_ORIGIN` | | Only if the SPA is served from a different origin |
| `BYPASS_AUTH` | `0` | Tests only, never in production |

### Docker

```sh
ABS_BASE_URL=https://abs.example.com docker compose up -d --build
```

Then open `http://<host>:3456` (put it behind your usual HTTPS reverse proxy).

### systemd

Build the server (`cargo build --release` in `server/`) and the client (`npm run build` in `client/`, which outputs to `server/static`), copy both to `/opt/abs-party`, then:

```sh
sudo useradd -r -s /sbin/nologin abs-party
sudo mkdir -p /etc/abs-party
sudo cp abs-party.env.example /etc/abs-party/env   # edit ABS_BASE_URL
sudo cp abs-party.service /etc/systemd/system/
sudo systemctl enable --now abs-party
```

### Docker with lima/nerdctl

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

### Development

```sh
cd server && cargo run                    # Rust 1.85+
cd client && npm install && npm run dev   # http://localhost:5173, proxies /api and /ws to :3456
```

Tests: `cargo test` in `server/`, `npm test` in `client/`.
