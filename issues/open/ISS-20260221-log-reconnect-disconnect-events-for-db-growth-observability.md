# ID: ISS-20260221-log-reconnect-disconnect-events-for-db-growth-observability

Title: Log reconnect/disconnect events for DB growth observability
Status: open
Reported: 2026-02-21
Reporter: user
Severity: low
Component: db/bot
Environment: Meshenger long-running deployment

## Symptom

DB growth spikes caused by reconnect floods are invisible. After a bad startup
night (e.g. 37 reconnects in one hour), the `packets` table showed 19,680
`nodeinfo` rows in a single hour — but there is no way to correlate that spike
to connection instability without reading raw logs. The DB alone tells you
*that* the data grew but not *why*.

## Expected

Connection lifecycle events (connect / disconnect) are recorded as rows in the
DB so that:
- DB growth spikes can be correlated to reconnect storms in retrospect.
- The number of reconnects over any time window is directly queryable.
- Long-term connection stability can be tracked alongside mesh growth.

## Actual

`Bot::run()` loops calling `connect_and_run()` indefinitely. Each connection
attempt is logged to the console (`log::info!`/`log::error!`) but nothing is
persisted to the DB. After a reconnect storm, there is no record of how many
times the bot reconnected or when.

## Root Cause

No DB write occurs at connection lifecycle transitions in `src/bot/runtime.rs`.
The `run()` loop only logs to stdout/stderr.

## Fix Plan

1. Add two new `packet_type` values to the existing `packets` table convention:
   - `"connect"` — bot successfully connected and received `MyNodeInfo`
   - `"disconnect"` — connection dropped (clean close or error)

   Using `packets` avoids a new table and keeps lifecycle events queryable
   alongside traffic data with the same time-series patterns.

2. Add a `log_lifecycle_event(node_id, kind)` helper to `Db` that inserts a
   `packets` row with:
   - `from_node = my_node_id`
   - `direction = "bot"`
   - `packet_type = kind` (e.g. `"connect"` or `"disconnect"`)
   - `text = ""`, all RF fields null, `via_mqtt = false`

3. In `src/bot/runtime.rs`:
   - Call `log_lifecycle_event(my_node_id, "connect")` in `connect_and_run()`
     after `wait_for_my_node_id()` returns successfully.
   - Call `log_lifecycle_event(my_node_id, "disconnect")` in `run()` after each
     `connect_and_run()` call returns (both Ok and Err paths).
   - For disconnect, include the reason (clean / error) in the `text` field.

4. After this change, reconnect count over a window is:
   ```sql
   SELECT count(*) FROM packets
   WHERE packet_type = 'connect'
     AND timestamp > unixepoch('now', '-1 day');
   ```
   And growth spikes can be overlaid against connect events in future tooling.

## Validation

- `cargo test -q` passes.
- Manual test: stop/restart the bot a few times, confirm `connect` and
  `disconnect` rows appear in `packets` with correct timestamps.
- Query reconnect count for the test window and confirm it matches observed
  restarts.

## References

- `src/bot/runtime.rs:105` — `Bot::run()` reconnect loop
- `src/bot/runtime.rs:124` — `connect_and_run()`
- `src/db.rs:164` — `packets` table schema

## Timeline

- 2026-02-21 — Issue created. Live DB showed 19,680 nodeinfo rows in one hour
  from reconnect storm; no way to identify this without reading logs.
