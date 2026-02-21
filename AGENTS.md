# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Meshenger is a modular Meshtastic mesh bot written in Rust. It connects to a Meshtastic node via TCP and provides automated services (welcome greetings, commands, platform bridges) to mesh users.

> **Firmware requirement:** Full traceroute session correlation (RouteReply decoding, pass-through packet delivery) requires a **patched meshtastic firmware** (`meshtasticd`): [https://github.com/meshbehave/meshtastic-firmware](https://github.com/meshbehave/meshtastic-firmware). On stock firmware, traceroute sessions are recorded but RouteReply packets may not be relayed back and pass-through packets may not be delivered to the TCP client. See `issues/` for details.

## Build & Test Commands

```sh
cargo build                        # Debug build
cargo build --release              # Release build
cargo run                          # Run with default config.toml
cargo run -- /path/to/config.toml  # Custom config path
cargo test                         # Run all tests
cargo test test_name               # Run a specific test
cargo test module_name::           # Run tests in a specific module
cargo test -- --nocapture          # Show println output
cd web && npm test                 # Run frontend unit tests (Vitest)
RUST_LOG=debug cargo run           # Verbose logging
RUST_LOG=meshenger=debug cargo run  # Crate-only debug logging
scripts/run-with-web.sh            # Build web frontend, then run with config.toml
scripts/run-with-web.sh /path/to/config.toml
```

## Architecture

### Data Flow

```
Meshtastic Node (TCP)
    ↕
  bot.rs (event_loop) ←→ ModuleRegistry (modules/*.rs)
    ↕                         ↕
bridge.rs channels        db.rs (SQLite)
    ↕
bridges/ (Telegram, Discord)
```

The bot connects to a Meshtastic node via TCP in `bot.rs`, dispatching incoming packets through an event loop. Text messages are parsed for commands and routed to the appropriate module via `ModuleRegistry`. Bridges run as independent tokio tasks communicating through channels defined in `bridge.rs`.

### Module System

All modules implement the `Module` trait (`src/module.rs`). Key conventions:

- Modules register **bare command names** (e.g., `"ping"` not `"!ping"`) — the bot prepends the configurable command prefix
- Modules can handle both commands (`handle_command`) and events (`handle_event` for `MeshEvent::NodeDiscovered`, etc.)
- Module registration happens in `src/modules/mod.rs` via `build_registry()`, gated by `config.is_module_enabled("name")`
- Return `Ok(Some(vec![Response { ... }]))` to send responses, `Ok(None)` for no response

### Bridge System

Bridges connect the mesh to external platforms (Telegram via `teloxide`, Discord via `serenity`). Communication uses two channel types in `bridge.rs`:

- `MeshMessageSender` (broadcast) — bot → all bridges
- `OutgoingMessageSender` (mpsc) — bridges → bot

Echo prevention: bridge-originated messages are prefixed with source tags (`[TG:username]`, `[DC:username]`) so they aren't re-forwarded. Each bridge has its own `BridgeDirection` enum controlling forwarding directionality.

### Outgoing Message Queue

All outgoing mesh messages go through a `VecDeque<OutgoingMeshMessage>` queue in `Bot`, drained by a timer branch in the `tokio::select!` event loop. This prevents radio flooding when many messages are generated at once (e.g., deferred welcome greetings after the startup grace period).

Key components:

- **`OutgoingMeshMessage`** — struct holding text, destination, channel, and DB logging fields
- **`queue_message()`** — pushes a single message onto the queue
- **`queue_responses()`** — converts `Response` objects into queued messages (with chunking for long text)
- **`send_next_queued_message()`** — pops front message, sends either text or traceroute packet, logs to DB
- **`send_delay_ms`** config option (default 1500ms) — minimum delay between consecutive transmissions

Messages from all sources (command responses, event responses, bridge messages, optional traceroute probes) flow through the queue. Only `send_next_queued_message()` touches `api`/`router`, keeping them out of the rest of the codebase.

### Optional Traceroute Probe

`[traceroute_probe]` can periodically queue a traceroute for a recently seen RF node that has no recorded inbound RF hop metadata yet.

Safety defaults are conservative:

- `interval_secs = 900` (15 min)
- `per_node_cooldown_secs = 21600` (6 h)
- `enabled = false` by default

Candidate selection excludes the bot's own node ID in SQL to avoid no-op self-target loops.

### Error Handling

Use `Box<dyn std::error::Error + Send + Sync>` for all async error types. The `RouterError` struct in `bot.rs` exists solely because the `PacketRouter` trait requires `E: std::error::Error`.

### Dashboard

An optional web dashboard (`src/dashboard.rs`) serves metrics via an axum HTTP server. Enabled via `[dashboard] enabled = true` in config.

**Backend** (`src/dashboard.rs`): axum routes under `/api/*` return JSON. Queries go through `Db` dashboard methods. An `MqttFilter` enum (All/LocalOnly/MqttOnly) filters metrics by MQTT vs local RF. Queue depth is shared via `Arc<AtomicUsize>`. Static files from `web/dist/` are served in production via `tower_http::services::ServeDir`.

API endpoints:

- `GET /api/overview?hours=24` — node count, message in/out (text only), packet in/out (all types), bot name
- `GET /api/nodes?hours=24&mqtt=all|local|mqtt_only` — node list with MQTT/RF distinction and per-node hop summary
- `GET /api/throughput?hours=24&mqtt=all` — text message throughput (hourly or daily buckets)
- `GET /api/packet-throughput?hours=24&mqtt=all&types=text,position,telemetry` — all packet type throughput with optional type filter
- `GET /api/rssi?hours=24&mqtt=all` — RSSI distribution
- `GET /api/snr?hours=24&mqtt=all` — SNR distribution
- `GET /api/hops?hours=24&mqtt=all` — hop count distribution
- `GET /api/traceroute-requesters?hours=24&mqtt=all` — nodes that sent incoming traceroute requests to the local node (count + last seen)
- `GET /api/traceroute-events?hours=24&mqtt=all` — recent incoming traceroute events (from/to/source/hops/RSSI/SNR)
- `GET /api/traceroute-destinations?hours=24&mqtt=all` — destination summary (requests, unique requesters, RF/MQTT split, last seen, avg hops)
- `GET /api/traceroute-sessions?hours=24` — correlated traceroute sessions with per-session hop arrays; `req:` prefix = our outgoing probes, `in:` prefix = observed third-party traceroutes
- `GET /api/queue` — current outgoing queue depth
- `GET /api/events` — SSE stream; emits `refresh` events when new data arrives

Smart bucketing: queries with `hours <= 48` bucket by hour; `hours > 48` bucket by day. This keeps charts readable at longer time ranges.

**Real-time updates**: The bot sends notifications via a `tokio::sync::broadcast` channel whenever packets arrive or messages are sent. The dashboard exposes this as an SSE endpoint (`/api/events`). The frontend connects via `EventSource` and re-fetches data on each `refresh` event. Polling every 30s remains as a fallback.

**Frontend** (`web/`): React + TypeScript + Vite + Tailwind CSS v4 + Chart.js + Leaflet. Dark theme. Real-time updates via SSE with 30s polling fallback. Components: overview cards (6 — nodes, messages in/out, packets in/out, queue depth), time range selector (1d/3d/7d/30d/90d/365d/All), message throughput chart (text only), packet throughput chart (with type toggles), RSSI/SNR bar charts, hop count doughnut, traceroute traffic panel with 3 tabs (`Events` + `Destinations` + `Sessions`), node map (Leaflet with MQTT/RF marker distinction + per-node hop summary), sortable node table (with MQTT/RF badges + per-node hop summary), MQTT filter toggle. Large tables are paginated in frontend state (API remains unchanged). Traceroute session detail displays `Route` plus optional `Route Back`; when no decoded hops are available it explicitly shows `Path unavailable on this node`.

Traceroute Insights `Sessions` table semantics:
- `Request` / `Response` columns display `hop_count/hop_start` when present.
- `Samples` is the count of packet observations merged into the same traceroute session key.

**Dev workflow**: Run `cd web && npm run dev` (Vite at :5173 with proxy to :9000) alongside `cargo run`. **Prod workflow**: `cd web && npm run build` then `cargo run` — axum serves both API and SPA from port 9000.

### Database

SQLite via `rusqlite` with bundled SQLite. Core runtime tables are `nodes` and `packets`. All access goes through the `Db` struct in `db.rs`. Use in-memory SQLite (`:memory:`) for tests.

The `packets` table includes a `packet_type` column (`text`, `position`, `telemetry`, `nodeinfo`, `traceroute`, `neighborinfo`, `routing`, `other`) and RF metadata columns (`via_mqtt`, `rssi`, `snr`, `hop_count`, `hop_start`). All packet types from the Meshtastic node are logged, not just text messages. `log_packet()` accepts these fields — outgoing messages pass `"text"`/`false`/`None`.

Traceroute session correlation is request-ID based (Meshtastic protocol semantics): canonical session key format is `req:<src>:<dst>:<request_id>`, where `request_id` is the traceroute request packet ID (`MeshPacket.id`) and responses/routing updates attach via `Data.request_id`.

When available, traceroute path vectors are extracted from both `TracerouteApp` and `RoutingApp` payloads (`RouteRequest`/`RouteReply`) and persisted to `traceroute_session_hops`. The `source_kind` field indicates provenance (`route`, `route_back`, `routing_route`, `routing_route_back`).

The `nodes` table includes a `via_mqtt` column tracking whether a node was last seen via MQTT or local RF. This is populated from the `NodeInfo` protobuf's `via_mqtt` field and carried through `MeshEvent::NodeDiscovered` (including deferred events during the startup grace period).

Dashboard node rows include derived hop summary fields from RF packet history:

- `last_hop` — latest known inbound RF hop count for the node (not time-windowed)
- `min_hop` / `avg_hop` — inbound RF hop stats over the selected dashboard `hours` window (or all-time when `hours=0`)

### Configuration

TOML format (`config.toml`, gitignored). All settings have defaults via `#[serde(default = "...")]`. When adding a config option:

1. Add field to the appropriate struct in `config.rs` with a serde default
2. Add the default function
3. Document it in `config.example.toml`

## Adding a New Module

1. Create `src/modules/your_module.rs` implementing the `Module` trait
2. Add `mod your_module;` and register it in `build_registry()` in `src/modules/mod.rs`
3. Add `[modules.your_module]` config section with `enabled` and `scope` fields

## Adding a New Bridge

1. Create `src/bridges/your_bridge.rs` with a struct that takes a config, `MeshMessageReceiver`, and `OutgoingMessageSender`
2. Add `pub mod your_bridge;` and re-exports in `src/bridges/mod.rs`
3. Add config struct in `config.rs` under `BridgeConfig`
4. Spawn the bridge task in `main.rs`

## Debugging

- **"incomplete packet" errors**: Benign, suppressed via log filter
- **Connection refused**: Check TCP address in config, ensure node is accessible on port 4403
- **Rate limited**: Check `rate_limit_commands` setting (0 = disabled)

## Issue Recording (Filesystem)

Use the in-repo tracker under `issues/`:

- Read `issues/README.md` before creating/updating issues.
- Create new issues from `issues/templates/ISSUE_TEMPLATE.md`.
- Prefer `scripts/new-issue.sh --title \"...\"` to keep IDs and index entries consistent.
- Keep `issues/index.md` in sync in the same change.
- Move issue files between `issues/open`, `issues/in_progress`, `issues/resolved`, and `issues/rejected` as status changes.
- Include concrete validation evidence before moving an issue to `resolved`.

## Known Issues

### Text messages delayed during startup

When the bot connects, the Meshtastic node dumps all known NodeInfo packets over TCP before delivering any queued text messages. During the 30-second startup grace period, text messages sent by other nodes over radio are buffered behind this NodeInfo flood in the TCP stream. They arrive at the bot only after the dump completes, typically appearing as a burst all at the same timestamp.

This is a Meshtastic firmware/TCP behavior, not a bot bug. The bot code handles text messages at any time — there is no grace period check in `handle_mesh_packet`. The delay is purely at the TCP transport layer.
