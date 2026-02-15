# Meshenger — Design Document

## Goal

Build a modular, extensible Meshtastic mesh bot in Rust that runs on a host machine (e.g. Raspberry Pi, laptop, server), connects to a Meshtastic node via TCP, and provides automated services to mesh users. The bot listens for incoming text commands and mesh events, dispatches them to pluggable modules, and sends responses back over the mesh.

The project prioritizes **customizability** — adding a new feature means implementing a single Rust trait, dropping a file into `src/modules/`, and registering it. No framework knowledge needed beyond the trait interface.

## Current Feature Set

| Feature | Command | Description | Scope |
|---------|---------|-------------|-------|
| Ping | `!ping` | Signal quality metrics (RSSI, SNR, hop count, MQTT indicator) | Public + DM |
| Node Info | `!nodes [n]` | Lists mesh nodes the bot has seen, with last-seen times (default 5, max 20) | Public + DM |
| Weather | `!weather` | Current conditions from Open-Meteo API — location-aware | Public + DM |
| Welcome | *(automatic)* | Sends a DM greeting when a new node is first seen (with optional whitelist) | DM only |
| Mail | `!mail` | Store-and-forward offline messaging | Public + DM |
| Uptime | `!uptime` | Bot uptime and message statistics | Public + DM |
| Help | `!help` | Lists available commands | Public + DM |

## Bridges

| Bridge | Description | Status |
|--------|-------------|--------|
| Telegram | Bidirectional message bridge to Telegram groups | Implemented |
| Discord | Bidirectional message bridge to Discord channels | Planned |

## Future Ideas

- LLM chat (Ollama integration)
- Emergency alerts (keyword detection + broadcast)
- Discord bridge
- Telemetry leaderboard
- Scheduled broadcasts
- Games (trivia, etc.)

---

## Architecture

### Project Structure

```
meshenger/
├── Cargo.toml
├── config.example.toml          # Example configuration
├── README.md                    # User documentation
├── DESIGN.md                    # This file
├── CLAUDE.md                    # Claude Code context
├── src/
│   ├── main.rs                  # Entry point, config loading, bridge startup
│   ├── bot.rs                   # Event loop, packet dispatch, bridge integration
│   ├── bridge.rs                # Bridge types and channels
│   ├── config.rs                # TOML config structs (serde)
│   ├── dashboard.rs             # Web dashboard HTTP server (axum)
│   ├── db.rs                    # SQLite setup, node/packet/mail tracking
│   ├── message.rs               # MessageContext, Response, CommandScope, MeshEvent
│   ├── module.rs                # Module trait definition + registry
│   ├── util.rs                  # Shared utility functions
│   ├── bridges/
│   │   ├── mod.rs               # Bridge re-exports
│   │   └── telegram.rs          # Telegram bridge implementation
│   └── modules/
│       ├── mod.rs               # Module registry builder
│       ├── ping.rs              # !ping — signal report
│       ├── node_info.rs         # !nodes — mesh node listing
│       ├── weather.rs           # !weather — forecast from API
│       ├── welcome.rs           # Auto-greet new nodes
│       ├── mail.rs              # !mail — store-and-forward messaging
│       ├── uptime.rs            # !uptime — bot statistics
│       └── help.rs              # !help — list commands
```

## Core Types

### Module Trait (`src/module.rs`)

```rust
#[async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn commands(&self) -> &[&str];           // bare names: &["ping"], not &["!ping"]
    fn scope(&self) -> CommandScope;          // Public | DM | Both

    async fn handle_command(
        &self,
        command: &str,
        args: &str,
        ctx: &MessageContext,
        db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>>;

    async fn handle_event(
        &self,
        event: &MeshEvent,
        db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None) // default implementation
    }
}
```

Modules register bare command names (without prefix). The bot handles prefix matching based on configuration.

### Message Types (`src/message.rs`)

```rust
pub enum CommandScope { Public, DM, Both }

pub struct MessageContext {
    pub sender_id: u32,
    pub sender_name: String,      // from NodeInfo if known
    pub channel: u32,
    pub is_dm: bool,              // packet.to == my_node_id
    pub rssi: i32,
    pub snr: f32,
    pub hop_count: u32,
    pub hop_limit: u32,
    pub via_mqtt: bool,           // true if message came through MQTT gateway
}

pub struct Response {
    pub text: String,
    pub destination: Destination,  // Sender | Broadcast | Node(u32)
    pub channel: u32,
}

pub enum Destination { Sender, Broadcast, Node(u32) }

pub enum MeshEvent {
    NodeDiscovered { node_id: u32, long_name: String, short_name: String, via_mqtt: bool },
    PositionUpdate { node_id: u32, lat: f64, lon: f64, altitude: i32 },
}
```

### Response Chunking

Meshtastic has a ~230 byte message limit. A `send_responses()` helper in `bot.rs` will:
1. Check if text fits in one message
2. If not, split on newline boundaries or at 220 chars
3. Send chunks sequentially with a small delay between them (~1s)

## Main Event Loop (`src/bot.rs`)

```
loop (reconnection) {
    connect to meshtastic node (TCP)
    configure API with random ID
    wait for MyInfo packet to learn own node ID

    loop (event) {
        select! {
            packet = packet_rx.recv() => {
                match packet.payload_variant {
                    Packet(mp) => {
                        extract RF metadata (rssi, snr, hop_count, hop_start)
                        match portnum {
                            TextMessageApp => handle_text_message()
                                // parse command, check rate limit, dispatch to module
                                // log_packet(packet_type="text")
                            PositionApp => update position in DB
                                // log_packet(packet_type="position")
                            TelemetryApp =>
                                // log_packet(packet_type="telemetry")
                            TracerouteApp =>
                                // log_packet(packet_type="traceroute")
                            NeighborinfoApp =>
                                // log_packet(packet_type="neighborinfo")
                            RoutingApp =>
                                // log_packet(packet_type="routing")
                            _ =>
                                // log_packet(packet_type="other")
                        }
                    }
                    NodeInfo(ni) => {
                        if in grace period: defer event
                        else:
                            build MeshEvent::NodeDiscovered (with via_mqtt)
                            for each module: call handle_event()
                            queue any responses
                            upsert_node(via_mqtt) in DB
                            log_packet(packet_type="nodeinfo")
                    }
                    _ => { skip }
                }
            }
            _ = send_timer => {
                send_next_queued_message()  // drain outgoing queue
            }
            _ = grace_period_timer => {
                dispatch_deferred_events()  // process deferred NodeInfo
            }
        }
    }

    on disconnect: wait reconnect_delay_secs, then retry
}
```

### Rate Limiting

The bot includes an in-memory rate limiter using a sliding window algorithm:
- Tracks command timestamps per node ID
- Configurable max commands per window (default: 5)
- Configurable window duration (default: 60 seconds)
- Setting `rate_limit_commands = 0` disables rate limiting

## Database Schema (`src/db.rs`)

Using `rusqlite`. Three tables:

```sql
CREATE TABLE IF NOT EXISTS nodes (
    node_id       INTEGER PRIMARY KEY,  -- meshtastic node number
    short_name    TEXT NOT NULL DEFAULT '',
    long_name     TEXT NOT NULL DEFAULT '',
    first_seen    INTEGER NOT NULL,     -- unix timestamp
    last_seen     INTEGER NOT NULL,
    last_welcomed INTEGER,              -- unix timestamp of last welcome sent
    latitude      REAL,                 -- last known position
    longitude     REAL,
    via_mqtt      INTEGER NOT NULL DEFAULT 0  -- 0 = RF, 1 = MQTT
);

CREATE TABLE IF NOT EXISTS packets (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp    INTEGER NOT NULL,
    from_node    INTEGER NOT NULL,
    to_node      INTEGER,              -- NULL = broadcast
    channel      INTEGER NOT NULL,
    text         TEXT NOT NULL,         -- empty string for non-text packets
    direction    TEXT NOT NULL,         -- 'in' or 'out'
    via_mqtt     INTEGER NOT NULL DEFAULT 0,
    rssi         INTEGER,
    snr          REAL,
    hop_count    INTEGER,
    hop_start    INTEGER,
    packet_type  TEXT NOT NULL DEFAULT 'text'
    -- packet_type values: text, position, telemetry, nodeinfo,
    --   traceroute, neighborinfo, routing, other
);

CREATE TABLE IF NOT EXISTS mail (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp  INTEGER NOT NULL,
    from_node  INTEGER NOT NULL,
    to_node    INTEGER NOT NULL,
    body       TEXT NOT NULL,
    read       INTEGER NOT NULL DEFAULT 0
);
```

Key queries:
- `upsert_node(id, short, long, via_mqtt)` — INSERT OR UPDATE, set last_seen and via_mqtt
- `is_node_new(id) -> bool` — check if node exists
- `is_node_absent(id, threshold_hours) -> bool` — check if last_seen is older than threshold
- `mark_welcomed(id)` — set last_welcomed to now
- `get_all_nodes() -> Vec<Node>` — for !nodes command
- `get_node_name(id) -> String` — resolve node ID to display name
- `find_node_by_name(name) -> Option<u32>` — find node by hex ID, decimal ID, or name
- `update_position(id, lat, lon)` — store node's last known position
- `get_node_position(id) -> Option<(lat, lon)>` — retrieve node's position
- `log_packet(...)` — record incoming/outgoing packets with type and RF metadata
- `message_count(direction) -> u64` — count text messages by direction
- `node_count() -> u64` — count known nodes
- `store_mail(from, to, body) -> id` — store offline mail
- `get_unread_mail(node_id) -> Vec<MailMessage>` — get unread mail for a node
- `count_unread_mail(node_id) -> u64` — count unread mail
- `mark_mail_read(id)` — mark mail as read
- `delete_mail(id, owner)` — delete mail owned by node
- `dashboard_overview(hours, filter, bot_name)` — message/packet counts for dashboard
- `dashboard_nodes(filter)` — node list with via_mqtt for dashboard
- `dashboard_throughput(hours, filter)` — text message throughput (smart bucketing)
- `dashboard_packet_throughput(hours, filter, types)` — all packet type throughput

## Module Designs

### Ping (`!ping`) — scope: Both
- Responds with: `Pong! RSSI: {rssi} SNR: {snr} Hops: {hop_count}/{hop_limit}`
- Appends "(via MQTT)" if `ctx.via_mqtt` is true
- Pulls all data from `MessageContext`, no DB needed

### Node Info (`!nodes [count]`) — scope: Both
- Queries DB for all seen nodes
- Optional argument: number of nodes to show (default 5, max 20)
- Format: `Nodes seen: 42\n!abc1234 NodeName (2m ago)\n...`
- Shows "...and N more" if list is truncated

### Weather (`!weather`) — scope: Both
- Uses Open-Meteo API (free, no API key needed)
- **Location-aware**: checks if sender has a known position in DB
  - If yes: uses sender's position, shows "(your location)" in response
  - If no: uses default location from config.toml
- Responds with: temp, conditions, humidity, wind
- HTTP request via `reqwest` crate
- WMO weather code mapping to human-readable descriptions
- Format: `Weather (your location): 24°C Partly Cloudy\nHumidity: 65% Wind: 12km/h`

### Welcome (`welcome`) — scope: DM (outgoing only)
- No commands — purely event-driven via `handle_event()`
- On `MeshEvent::NodeDiscovered`:
  - **Whitelist check**: if whitelist is configured, only greet listed nodes
  - If node is brand new (not in DB) → send welcome DM
  - If node is known but `last_seen` was more than `absence_threshold` ago → send welcome-back DM
  - Otherwise → skip (already active, no spam)
- Update `last_seen` and `last_welcomed` timestamps in DB
- Whitelist supports hex (`!ebb0a1ce`) and decimal (`3954221518`) node IDs
- All parameters configurable in config.toml

### Mail (`!mail`) — scope: Both
Store-and-forward offline messaging system.

**Subcommands:**
- `!mail send <recipient> <message>` — Send mail to a user
- `!mail read` — Read and mark unread mail as read
- `!mail list` — Show count of unread mail
- `!mail delete <id>` — Delete a mail message

**Recipient lookup** (in order):
1. Hex node ID (with or without `!` prefix)
2. Decimal node ID
3. Case-insensitive match on short_name or long_name

**Event handling**: When a node is discovered, the mail module checks for unread mail and notifies them: "You have N unread mail message(s). Send !mail read to view."

### Uptime (`!uptime`) — scope: Both
- Tracks bot start time
- Shows:
  - Uptime duration (Xd Xh Xm Xs format)
  - Messages received count
  - Messages sent count
  - Total nodes seen

### Help (`!help`) — scope: Both
- Auto-generated from module registry
- Lists all enabled commands with one-line descriptions
- Format: `!ping - Signal report\n!nodes - Mesh node listing\n!weather - Weather forecast`
- Generated in bot.rs since the help module doesn't have direct access to the registry

## Bridge Architecture

Bridges connect the mesh network to external platforms (Telegram, Discord, etc.). They run as background tasks alongside the main bot event loop.

### Communication Channels

```
┌─────────────┐     broadcast      ┌─────────────────┐
│   Bot       │ ──────────────────►│   Bridges       │
│ (mesh msgs) │   MeshBridgeMsg    │ (Telegram, etc) │
└─────────────┘                    └─────────────────┘
      ▲                                    │
      │         mpsc                       │
      └────────────────────────────────────┘
              OutgoingBridgeMsg
```

- **MeshMessageSender** (broadcast): Bot sends mesh messages to all bridges
- **OutgoingMessageSender** (mpsc): Bridges send messages back to bot for mesh broadcast

### Message Types

```rust
/// Mesh → Bridge (broadcast to all bridges)
pub struct MeshBridgeMessage {
    pub sender_id: u32,
    pub sender_name: String,
    pub text: String,
    pub channel: u32,
    pub is_dm: bool,
}

/// Bridge → Mesh (sent to bot for broadcast)
pub struct OutgoingBridgeMessage {
    pub text: String,
    pub channel: u32,
    pub source: String, // "telegram", "discord", etc.
}
```

### Echo Prevention

Messages are tagged with their source to prevent echo loops:
- Mesh → Telegram: Format as `[NodeName] message`
- Telegram → Mesh: Format as `[TG:username] message`
- Bot skips broadcasting messages that start with `[TG:` or `[DC:` back to bridges

### Telegram Bridge

Uses the `teloxide` crate for Telegram Bot API.

**Configuration:**
```toml
[bridge.telegram]
enabled = true
bot_token = "123456789:ABC..."
chat_id = -1001234567890
mesh_channel = 0           # 0 = all channels
direction = "both"         # both, to_telegram, to_mesh
format = "[{name}] {message}"
```

**Direction options:**
- `both` — Bidirectional bridging
- `to_telegram` — Only forward mesh messages to Telegram
- `to_mesh` — Only forward Telegram messages to mesh

**Format placeholders:**
- `{name}` — Sender's node name
- `{id}` — Sender's node ID (hex)
- `{message}` — Message text
- `{channel}` — Mesh channel number

## Dashboard

An optional web dashboard provides real-time metrics and node tracking.

### Backend (`src/dashboard.rs`)

axum HTTP server serving JSON APIs and static frontend files. Key features:
- **MQTT filtering**: `MqttFilter` enum (All/LocalOnly/MqttOnly) on most endpoints
- **Time range**: `hours` parameter on all time-based endpoints
- **Smart bucketing**: hourly buckets for ≤48h, daily for >48h
- **Queue depth**: shared via `Arc<AtomicUsize>` from the bot's outgoing queue

### Frontend (`web/`)

React + TypeScript + Vite + Tailwind CSS v4 + Chart.js SPA.

Components:
- **TimeRangeSelector** — toggle: 1d / 3d / 7d / 30d / 90d / 365d / All
- **OverviewCards** — 6 cards: Total Nodes, Messages In/Out, Packets In/Out, Queue Depth (labels reflect selected time range)
- **ThroughputChart** — text message throughput (line chart)
- **PacketThroughputChart** — all packet types with type toggle filters (All/Text/Position/Telemetry/Other)
- **RssiChart / SnrChart** — RF quality distribution bar charts
- **HopsChart** — hop count doughnut chart
- **NodeTable** — sortable table with MQTT/RF source badges, filterable by MQTT status
- **MqttFilter** — global toggle for MQTT vs local RF filtering

### Configuration

```toml
[dashboard]
enabled = true
port = 9000
```

## Configuration (`config.example.toml`)

```toml
[connection]
address = "192.168.2.17:4403"    # TCP address of meshtastic node
# reconnect_delay_secs = 5      # Seconds to wait before reconnecting

[bot]
name = "Meshenger"
db_path = "meshenger.db"
# command_prefix = "!"          # Command prefix (default: !)
# rate_limit_commands = 5       # Max commands per window (0 = disabled)
# rate_limit_window_secs = 60   # Window duration in seconds

[welcome]
enabled = true
message = "Welcome to the mesh, {name}! Send !help for commands."
welcome_back_message = "Welcome back, {name}!"
absence_threshold_hours = 48
# Optional: only greet these nodes. Omit or leave empty to greet everyone.
# Accepts hex (!ebb0a1ce) or decimal (3954221518) node IDs.
# whitelist = ["!ebb0a1ce", "!9f1a7a2d"]

[weather]
latitude = 25.0330
longitude = 121.5654
units = "metric"                  # metric or imperial

[modules.ping]
enabled = true
scope = "both"

[modules.nodes]
enabled = true
scope = "both"

[modules.weather]
enabled = true
scope = "both"

[modules.welcome]
enabled = true
scope = "dm"

[modules.mail]
enabled = true
scope = "both"

[modules.uptime]
enabled = true
scope = "both"

[modules.help]
enabled = true
scope = "both"
```

## Dependencies

```toml
meshtastic = { version = "0.1", features = ["tokio"] }
tokio = { version = "1", features = ["full"] }
rusqlite = { version = "0.31", features = ["bundled"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
async-trait = "0.1"
log = "0.4"
env_logger = "0.11"
chrono = "0.4"
```

## Hardcoding Policy

Nothing configurable is hardcoded. All of the following come from `config.toml`:
- Connection address (TCP host:port) and reconnect delay
- Bot name, database path, command prefix
- Rate limiting parameters
- Welcome messages (new + returning), absence threshold, whitelist
- Weather location (lat/lon), units
- Per-module enabled/disabled and scope

## Adding a New Module

1. Create `src/modules/your_module.rs`
2. Implement the `Module` trait:
   - `name()` — unique identifier
   - `description()` — one-line description for help
   - `commands()` — bare command names (without prefix)
   - `scope()` — where commands work
   - `handle_command()` — process commands
   - `handle_event()` — respond to mesh events (optional)
3. Add `mod your_module;` to `src/modules/mod.rs`
4. Register it in `build_registry()` with a config check
5. Add a `[modules.your_module]` section to `config.toml`

### Example Module

```rust
use async_trait::async_trait;
use crate::db::Db;
use crate::message::{CommandScope, MessageContext, Response, Destination};
use crate::module::Module;

pub struct EchoModule;

#[async_trait]
impl Module for EchoModule {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Echo back your message" }
    fn commands(&self) -> &[&str] { &["echo"] }
    fn scope(&self) -> CommandScope { CommandScope::Both }

    async fn handle_command(
        &self,
        _command: &str,
        args: &str,
        ctx: &MessageContext,
        _db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Some(vec![Response {
            text: args.to_string(),
            destination: Destination::Sender,
            channel: ctx.channel,
        }]))
    }
}
```

## Development Notes

### Logging
- Log level controlled via `RUST_LOG` environment variable
- Default filter: `info,meshtastic::connections::stream_buffer=off`
- The stream_buffer filter suppresses benign "incomplete packet" errors from the meshtastic crate

### Error Handling
- All error types use `Box<dyn std::error::Error + Send + Sync>` for async compatibility
- Custom `RouterError` type for PacketRouter trait implementation

### Testing
1. `cargo build` — must compile cleanly
2. `cargo run` — connect to node
3. Send commands from another Meshtastic device/app
4. Check logs for expected behavior
