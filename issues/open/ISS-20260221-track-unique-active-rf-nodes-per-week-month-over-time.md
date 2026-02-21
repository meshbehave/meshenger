# ID: ISS-20260221-track-unique-active-rf-nodes-per-week-month-over-time

Title: Track unique active RF nodes per week/month over time
Status: open
Reported: 2026-02-21
Reporter: user
Severity: low
Component: db/schema/dashboard
Environment: Meshenger long-running deployment

## Symptom

There is no way to answer "how many unique RF nodes were active in week/month N"
from the current schema without scanning the entire `packets` table — a query
that will become increasingly expensive over months of data accumulation. The
`nodes` table only holds current state, not per-node RF first-appearance
history. A node first heard via MQTT but later heard via RF has its
`nodes.first_seen` set to the MQTT time, losing the RF join date.

## Expected

It should be possible to query:
- "How many unique RF-active nodes per week/month over the past year?"
- "When did each node first appear on the RF mesh (as opposed to MQTT)?"
- Queries should be fast regardless of how large `packets` grows.

## Actual

- `nodes.first_seen` exists but is MQTT-or-RF combined; no RF-specific field.
- The raw query (count DISTINCT from_node WHERE via_mqtt=0 per time bucket)
  works from `packets` today (32k rows) but becomes a full-table scan at scale.
- No dashboard endpoint or API exposes this time-series.

## Root Cause

The `nodes` table schema was designed to track current node state, not
historical RF appearance. The `packets` table carries the full history but is
optimized for per-node and per-direction lookups, not aggregate time-series.

## Fix Plan

### 1. Add `first_rf_seen` to `nodes`

Add a nullable `first_rf_seen INTEGER` column to `nodes`, set the first time a
packet with `direction='in' AND via_mqtt=0` is received from that node. Never
updated after initial set (tracks RF first appearance, not last).

Population: set in `log_packet_with_mesh_id()` or `log_incoming_packet()` when
`via_mqtt=false` and `direction='in'`, using `INSERT OR IGNORE` / `UPDATE ...
WHERE first_rf_seen IS NULL` pattern on the `nodes` table.

This allows:
```sql
-- Nodes that joined the RF mesh each month
SELECT strftime('%Y-%m', first_rf_seen, 'unixepoch') AS month,
       COUNT(*) AS new_rf_nodes
FROM nodes
WHERE first_rf_seen IS NOT NULL
GROUP BY month ORDER BY month;
```

### 2. Add a weekly/monthly RF activity summary query

A DB helper `rf_active_nodes_per_bucket(bucket: 'week'|'month')` that returns:
```sql
SELECT strftime('%Y-%W', timestamp, 'unixepoch') AS bucket,
       COUNT(DISTINCT from_node) AS unique_rf_nodes
FROM packets
WHERE direction = 'in' AND via_mqtt = 0
GROUP BY bucket ORDER BY bucket;
```

This is the correct longitudinal query. It is a full scan today — acceptable
for an on-demand historical report, but not for dashboard polling.

### 3. Dashboard endpoint (optional, lower priority)

Add `GET /api/rf-growth?bucket=week|month` returning the time-series from
step 2. Not for dashboard polling — intended as a one-off analytical query
(could be triggered manually or rendered on a separate "History" tab).

### 4. Backfill `first_rf_seen` for existing data

On schema migration, backfill `first_rf_seen` from `packets`:
```sql
UPDATE nodes SET first_rf_seen = (
    SELECT MIN(timestamp) FROM packets
    WHERE from_node = nodes.node_id
      AND direction = 'in' AND via_mqtt = 0
)
WHERE first_rf_seen IS NULL;
```
Run once during `init_schema()` migration (guard with `ALTER TABLE IF NOT
EXISTS` column check).

## Validation

- `cargo test -q` passes.
- After migration, confirm `first_rf_seen` is populated for RF nodes and NULL
  for MQTT-only nodes.
- Query the monthly new-node time-series and confirm it matches manual counts
  from `packets`.

## References

- `src/db.rs:152` — `nodes` table schema
- `src/db.rs:164` — `packets` table schema
- Live DB: 530 nodes (441 RF, 89 MQTT) after ~20h, `first_rf_seen` needed to
  distinguish RF join date from MQTT join date.

## Timeline

- 2026-02-21 — Issue created. Goal: retrospective mesh growth analysis over
  months/years of data. User wants to track how the local RF mesh has grown.
