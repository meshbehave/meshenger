# ID: ISS-20260221-prevent-frontend-dashboard-performance-degradation-as-db-grows

Title: Prevent frontend dashboard performance degradation as DB grows
Status: open
Reported: 2026-02-21
Reporter: user
Severity: medium
Component: db/dashboard/api
Environment: Meshenger long-running deployment

## Symptom

The dashboard reloads on every SSE `refresh` event (triggered by every incoming
packet) and falls back to polling every 30s. Several API endpoints run expensive
queries against the `packets` table with no row-count limit and no query-cost
cap. As `packets` grows to millions of rows over months, dashboard load times
will degrade silently.

## Expected

Dashboard API endpoints remain fast (sub-100ms) regardless of total DB size.
The result sets returned to the frontend are bounded and predictable.

## Actual

Unbounded or expensive endpoints identified:

| Endpoint | Issue |
|---|---|
| `GET /api/nodes` | `dashboard_nodes()` — no LIMIT; CTEs (`rf_last`, `rf_hops`) scan ALL historical RF packets per node on every call |
| `GET /api/positions` | `dashboard_positions()` — same expensive CTEs, no LIMIT, runs on every map render |
| `GET /api/traceroute-requesters` | `dashboard_traceroute_requesters()` — no LIMIT; scans `packets` by time window |
| `GET /api/traceroute-destinations` | `dashboard_traceroute_destinations()` — no LIMIT; scans `packets` by time window |
| All `?hours=0` queries | `hours=0` sets `since=0` (epoch), forcing full-table scans across all historical data |

Already bounded (no action needed):
- `traceroute-events`: LIMIT 200
- `traceroute-sessions`: LIMIT 300
- Throughput/RSSI/SNR/hops: return aggregated buckets (small result sets)

The most critical bottleneck is `dashboard_nodes` / `dashboard_positions`: the
`rf_last` and `rf_hops` CTEs use `ROW_NUMBER() OVER (PARTITION BY from_node
ORDER BY timestamp DESC)` with **no time filter**, so they must consider all
historical RF packets for every node on every API call. With 530 nodes and
years of RF data this becomes very expensive.

## Root Cause

1. `rf_last` and `rf_hops` CTEs in `dashboard_nodes()` and
   `dashboard_positions()` have no time-window constraint — they compute
   "most recent ever" by scanning all historical RF packets.
2. Several endpoints aggregate raw packet rows without row caps.
3. All DB queries run synchronously holding `Mutex<Connection>`, so a slow
   query blocks all other DB access.

## Fix Plan

### Priority 1 — Cache `last_rf_seen` and `last_hop` on `nodes`

This is the root fix for the most expensive queries. Instead of recomputing
"last RF seen" and "last RF hop count" from all of `packets` on every request,
maintain them on the `nodes` row:

- Add `last_rf_seen INTEGER` and `last_rf_hop INTEGER` columns to `nodes`.
- Update them in `log_incoming_packet()` when `via_mqtt=false`: a simple
  `UPDATE nodes SET last_rf_seen=?, last_rf_hop=? WHERE node_id=?`.
- Rewrite `dashboard_nodes()` and `dashboard_positions()` to read directly
  from `nodes` (O(n_nodes) index scan) instead of CTEs over `packets`.
- `rf_stats` CTE (min/avg hop over time window) still runs against `packets`
  but is already time-bounded by `timestamp > ?1`.

After this change, both endpoints become simple `SELECT` from `nodes` + one
bounded CTE — fast regardless of `packets` size.

### Priority 2 — Add LIMIT to unbounded list endpoints

- `dashboard_traceroute_requesters()`: add `LIMIT ?3` parameter (default 200).
- `dashboard_traceroute_destinations()`: add `LIMIT ?3` parameter (default 200).
- Neither endpoint currently has frontend pagination, so cap is safe.

### Priority 3 — Guard `hours=0` full-history queries

`hours=0` currently means "all time" and forces `since=0`. This is fine for
small windows but becomes a multi-second scan for packet-heavy endpoints.
Options:
- Cap `hours=0` at a maximum (e.g. 8760h = 1 year) for aggregate endpoints.
- Or document it as an admin-only slow query and ensure indexes cover it.
  The existing `idx_packets_rf_hops_stats` partial index already helps
  `rf_stats`; throughput queries need a covering index on `(timestamp, packet_type)`.

### Priority 4 — Index audit for throughput queries

`dashboard_throughput()` and `dashboard_packet_throughput()` filter by
`timestamp > ?1` and `packet_type`. Verify `EXPLAIN QUERY PLAN` uses an index.
Add `CREATE INDEX idx_packets_timestamp ON packets (timestamp)` if not covered.

## Validation

- `cargo test -q` passes.
- After caching `last_rf_seen`/`last_rf_hop` on nodes: `EXPLAIN QUERY PLAN`
  for `dashboard_nodes` shows no full-scan of `packets`.
- Measure `/api/nodes` response time before and after with a large synthetic
  `packets` dataset.
- Confirm `last_rf_seen` and `last_rf_hop` on `nodes` stay accurate under
  rapid packet ingestion (integration test).

## References

- `src/db.rs:702` — `dashboard_nodes()` CTE
- `src/db.rs:1012` — `dashboard_positions()` CTE
- `src/db.rs:1084` — `dashboard_traceroute_requesters()` (no LIMIT)
- `src/db.rs:1217` — `dashboard_traceroute_destinations()` (no LIMIT)
- `src/dashboard.rs:161` — `handle_nodes()`
- `src/dashboard.rs:248` — `handle_positions()`

## Timeline

- 2026-02-21 — Issue created. Current DB: 2.5 MB / 32k rows after 20h.
  `dashboard_nodes` CTE scans all RF packets on every SSE refresh — manageable
  now, problem in 6–12 months of continuous operation.
