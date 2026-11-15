# abs-party

Listen to an audiobook together on a shared [Audiobookshelf](https://www.audiobookshelf.org/) instance. One person (the host) opens a room with a book, others join by link or room code, and playback stays in sync for everyone.

## Running

### Docker

```sh
docker run -d --name abs-party --restart unless-stopped -p 3456:3456 \
  -e ABS_BASE_URL=https://abs.example.com \
  0xpr03/abs-party:latest
```

Or with compose:

```yaml
services:
  abs-party:
    image: 0xpr03/abs-party:latest
    ports:
      - "3456:3456"
    environment:
      - ABS_BASE_URL=https://abs.example.com
    restart: unless-stopped
```

- Open `http://<host>:3456`, behind your HTTPS reverse proxy.
- Images: `0xpr03/abs-party` (Docker Hub), `ghcr.io/0xpr03/abs-party`; amd64 + arm64.
- Tags: `latest` (main), `<version>` (releases), `sha-<commit>`.

### Configuration

| Variable | Default | |
|---|---|---|
| `ABS_BASE_URL` | `http://localhost:13378` | Your ABS instance, no trailing slash (**required**) |
| `BIND` / `PORT` | `0.0.0.0:3456` | Listen address, or just the port |
| `STATIC_DIR` | `./static` | Built SPA |
| `RUST_LOG` | | e.g. `abs_party=info` |
| `CORS_ORIGIN` | | Only if the SPA is served from a different origin |
| `BYPASS_AUTH` | `0` | Tests only, never in production |

### From source

Development:

```sh
cd server && cargo run                    # Rust 1.85+
cd client && npm install && npm run dev   # http://localhost:5173, proxies /api and /ws to :3456
```

Tests: `cargo test` in `server/`, `npm test` in `client/`.

Docker image from source (`docker-compose.yml` builds locally; set `ABS_BASE_URL` in `.env`):

```sh
docker compose up -d --build
```

lima/nerdctl (`down` is required, `up` alone keeps the old container):

```sh
lima nerdctl compose down
lima nerdctl compose build --no-cache
lima nerdctl compose up -d
```

systemd: build `server/` (`cargo build --release`) and `client/` (`npm run build`, outputs to `server/static`), copy both to `/opt/abs-party`, then:

```sh
sudo useradd -r -s /sbin/nologin abs-party
sudo mkdir -p /etc/abs-party
sudo cp abs-party.env.example /etc/abs-party/env   # edit ABS_BASE_URL
sudo cp abs-party.service /etc/systemd/system/
sudo systemctl enable --now abs-party
```

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
