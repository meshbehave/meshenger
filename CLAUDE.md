# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Meshenger is a modular Meshtastic mesh bot written in Rust. It connects to a Meshtastic node via TCP and provides automated services (welcome greetings, commands, store-and-forward mail, platform bridges) to mesh users.

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
RUST_LOG=debug cargo run           # Verbose logging
RUST_LOG=meshenger=debug cargo run  # Crate-only debug logging
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

### Error Handling

Use `Box<dyn std::error::Error + Send + Sync>` for all async error types. The `RouterError` struct in `bot.rs` exists solely because the `PacketRouter` trait requires `E: std::error::Error`.

### Database

SQLite via `rusqlite` with bundled SQLite. Three tables: `nodes`, `messages`, `mail`. Schema auto-migrates (adds columns if missing). All access through the `Db` struct in `db.rs`. Use in-memory SQLite (`:memory:`) for tests.

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
