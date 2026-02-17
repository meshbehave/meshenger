# Meshenger

Your mesh network's personal messenger, butler, and weatherman — all rolled into one tiny Rust bot.

Meshenger connects to a [Meshtastic](https://meshtastic.org/) node via TCP and hangs out on your mesh network, greeting newcomers, answering commands, and bridging conversations to Telegram and Discord.

## What It Does

**Greets people** — New node pops up on the mesh? Meshenger sends them a welcome DM. Someone comes back after a long absence? Welcome back message. It's the friendly doorman your mesh never knew it needed.

**Runs commands** — Users on the mesh can interact with the bot:

| Command          | What it does                                                                    |
| ---------------- | ------------------------------------------------------------------------------- |
| `!ping`          | Signal quality report (RSSI, SNR, hop count, MQTT indicator)                    |
| `!nodes [count]` | List recently seen nodes (default 5, max 20)                                    |
| `!weather`       | Current weather — uses your GPS position if known, otherwise a default location |
| `!uptime`        | Bot uptime and message stats                                                    |
| `!help`          | List available commands                                                         |

**Bridges to chat platforms** — Bidirectional message bridging to Telegram and Discord. Mesh users see `[TG:alice]` or `[DC:bob]` prefixed messages, and chat platform users see formatted mesh messages. No more checking two apps.

**Tracks everything** — Every packet type (text, position, telemetry, traceroute, etc.) is logged with RF metadata. Nodes are tagged as MQTT or local RF based on their transport method.

**Dashboard** — Optional web dashboard with real-time metrics: message/packet throughput charts, RSSI/SNR distributions, hop counts, traceroute traffic tabs (recent events and destination summary across all seen traceroute packets), traceroute insights (hops-to-me aggregates by source and per-session path view), node table/map with MQTT/RF badges and per-node hop summary (last/avg/min), configurable time ranges (1d to 1y), and MQTT filtering. Traceroute session correlation uses Meshtastic request/response IDs (`request.id` ↔ `response.request_id`) via canonical session key `req:<src>:<dst>:<request_id>`. Path hops are recorded from traceroute/routing payload vectors when firmware provides them; session detail renders `Route` and optional `Route Back` (or `Path unavailable on this node` when absent).

In the Traceroute Insights `Sessions` table: `Request`/`Response` columns show `hop_count/hop_start` when available, and `Samples` is the number of packet observations merged into the same session key.

**Optional auto traceroute probe** — Can periodically send a low-frequency traceroute to recently-seen RF nodes that still lack hop metadata, reusing the bot's existing outgoing queue and pacing.

## Quick Start

1. Copy and edit the config:

```sh
cp config.example.toml config.toml
# Edit config.toml with your node address, location, etc.
```

2. Build and run:

```sh
cargo build --release
./target/release/meshenger
# or with a custom config path:
./target/release/meshenger /path/to/config.toml
```

3. Watch the logs:

```sh
RUST_LOG=info ./target/release/meshenger     # normal
RUST_LOG=debug ./target/release/meshenger    # verbose
```

### One-Command Run (Build Web + Start Bot)

If you want one command that builds the dashboard frontend and then starts the bot:

```sh
scripts/run-with-web.sh
# or custom config:
scripts/run-with-web.sh /path/to/config.toml
```

## Run With Docker Compose

1. Create your config file:

```sh
cp config.example.toml config.toml
# Edit config.toml (especially [connection].address)
```

2. Start the container:

```sh
docker compose up -d --build
```

3. Follow logs:

```sh
docker compose logs -f meshenger
```

Notes:

- The container runs as a non-root user (`meshenger`).
- Container user/group IDs are set from host `UID/GID` (fallback `1000:1000`) at build time.
- `config.toml` is mounted read-only at `/config/config.toml`.
- A host bind mount (`./data`) is mounted at `/data` for easy backup.
- With default `db_path = "meshenger.db"`, the SQLite DB is stored in `/data/meshenger.db`.
- Docker logs are capped at 100MB per container (`json-file` driver, `max-size=100m`).

## Configuration

Everything lives in `config.toml`. See [`config.example.toml`](config.example.toml) for all options with comments.

### The Basics

```toml
[connection]
address = "192.168.2.17:4403"    # Your Meshtastic node's TCP address

[bot]
name = "Meshenger"
command_prefix = "!"             # Change to @, /, etc.
rate_limit_commands = 5          # Per user, per window (0 = disabled)

[welcome]
enabled = true
message = "Welcome to the mesh, {name}! Send !help for commands."
welcome_back_message = "Welcome back, {name}!"
absence_threshold_hours = 48

[weather]
latitude = 25.0330
longitude = 121.5654
units = "metric"                 # or "imperial"
```

### Dashboard

```toml
[dashboard]
enabled = true
bind_address = "0.0.0.0:9000"   # Address for the dashboard web server
```

Run `cd web && npm run build` once to build the frontend, then access the dashboard at `http://localhost:9000`. For development, run `cd web && npm run dev` for hot-reload at `:5173` with API proxy to `:9000`.

### Auto Traceroute Probe

```toml
[traceroute_probe]
enabled = false
interval_secs = 900             # one probe cycle every 15 minutes
interval_jitter_pct = 0.20      # adds 0..20% random delay per cycle
recent_seen_within_secs = 3600  # only consider RF nodes seen in the last hour
per_node_cooldown_secs = 21600  # don't probe the same node again for 6 hours
mesh_channel = 0
```

The probe targets the most recently seen local RF node that still has no inbound RF hop metadata. Probes are queued and sent via the same outgoing queue as normal bot messages, so send pacing remains centralized.
`interval_jitter_pct` randomizes each cycle in `[interval_secs, interval_secs * (1 + jitter)]` to reduce synchronized, machine-like periodic traffic.

### Modules

Every feature can be toggled on/off and scoped to `public` channels, `dm` only, or `both`:

```toml
[modules.ping]
enabled = true
scope = "both"
```

### Telegram Bridge

```toml
[bridge.telegram]
enabled = true
bot_token = "123456789:ABCdefGHIjklMNOpqrsTUVwxyz"
chat_id = -1001234567890
mesh_channel = 0              # Meshtastic channel index (0-7), 0 = PRIMARY
direction = "both"            # "both", "to_telegram", "to_mesh"
format = "[{name}] {message}" # placeholders: {name}, {id}, {message}, {channel}
```

Create a bot via [@BotFather](https://t.me/botfather), add it to your group, then grab the chat ID with:

```sh
curl https://api.telegram.org/bot<TOKEN>/getUpdates
```

### Discord Bridge

```toml
[bridge.discord]
enabled = true
bot_token = "MTIzNDU2Nzg5.AbCdEf.GhIjKlMnOpQrStUvWxYz"
channel_id = 1234567890123456789
mesh_channel = 0                    # Meshtastic channel index (0-7), 0 = PRIMARY
direction = "both"
format = "**{name}**: {message}"
```

Setup: create a bot at the [Discord Developer Portal](https://discord.com/developers/applications), enable **MESSAGE CONTENT INTENT**, then invite it with:

```
https://discord.com/oauth2/authorize?client_id=YOUR_APP_ID&scope=bot&permissions=3072
```

## Adding Your Own Module

Meshenger is modular by design. To add a new command:

1. Create `src/modules/your_module.rs` implementing the `Module` trait
2. Register it in `src/modules/mod.rs`
3. Add `[modules.your_module]` to your config

See the existing modules for examples — `ping.rs` is the simplest starting point.

## Requirements

- Rust 2021 edition
- A Meshtastic node reachable via TCP (typically port 4403)
- Internet access for `!weather` (uses [Open-Meteo](https://open-meteo.com/), free, no API key needed)

## Issue Tracking

Issues are tracked in-repo under `issues/` (filesystem-based).

- Start with `issues/README.md`
- Use `issues/templates/ISSUE_TEMPLATE.md`
- Optional helper: `scripts/new-issue.sh --title "..."`
- Keep `issues/index.md` updated when adding/changing issue status
- Move issue files across status folders: `open`, `in_progress`, `resolved`, `rejected`
- When resolving an issue, add the fixing commit hash under `## References` as `Commit: <hash>`

## License

MIT
