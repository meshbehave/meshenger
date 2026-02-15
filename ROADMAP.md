# Meshenger: Progress and Future Work

Last updated: 2026-02-15

## Current progress
- Logging improvements implemented in `src/bot.rs`:
  - Incoming position logs include packet/message ID.
  - Incoming text logs include packet/message ID.
  - "No bridge receivers listening" debug includes message ID.
  - Outgoing reply logs include `reply_to_msg_id` when replying.
  - Reply-send failure logs include `reply_to_msg_id`.
- Dead code cleanup in Telegram bridge:
  - Removed unused `format_mesh_message` method.
  - Replaced with used shared helper `render_mesh_message`.
- Lint cleanup:
  - Clippy issue in weather module fixed (`80..=82`).
  - `cargo clippy --all-targets --all-features -- -D warnings` passes.

## Future work (recommended)
1. Add sent message ID correlation
- If available from Meshtastic API response, log `sent_msg_id` and correlate with `reply_to_msg_id`.

2. Logging structure hardening
- Move key logs to structured fields (JSON logger mode) for easier ingestion/search.
- Add consistent event type tags (`rx_text`, `rx_pos`, `tx_reply`, etc.).

3. Observability
- Add counters/metrics for dropped bridge messages, queue latency, and per-module response time.

4. Test coverage
- Add focused tests for logging formatting/correlation paths.

5. SQLite growth management
- Current observed baseline (2026-02-15): `meshenger.db` ~468 KB with ~9,134 `packets` rows over ~13.3 hours.
- At similar traffic, annual growth is likely in the hundreds of MB range (roughly 300 MB to 1 GB depending on message size/mix).
- Add configurable retention for high-volume tables (`packets`, optionally `mail`), for example `retention_days = 90` or `180`.
- Add periodic maintenance (`VACUUM`/`auto_vacuum`) after pruning to reclaim space and keep backups smaller.

## Quick lint check
```bash
cd /home/eanu/dev/meshenger
cargo clippy --all-targets --all-features -- -D warnings
```
