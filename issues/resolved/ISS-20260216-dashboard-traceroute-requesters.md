# ID: ISS-20260216-dashboard-traceroute-requesters

Title: Dashboard should show which nodes requested traceroute to my node
Status: resolved
Reported: 2026-02-16
Reporter: user
Severity: medium
Component: dashboard+bot/incoming+db
Environment: Meshenger dashboard + Meshtastic traceroute traffic

## Symptom

Dashboard currently shows packet-level metrics, but does not show which node requested traceroute to the attached node.

## Expected

Dashboard should provide a view of incoming traceroute requests addressed to the local node, grouped by requester node.

## Actual

Implemented:

- Incoming packet logging now persists `to_node` for non-text packets (including traceroute).
- Added DB aggregation for incoming traceroute requesters addressed to local node.
- Added dashboard API endpoint: `/api/traceroute-requesters`.
- Added frontend table showing requester node, source (RF/MQTT), count, and last request time.

## Reproduction

1. Send traceroute requests from one or more nodes to the local node.
2. Open dashboard and inspect existing pages/cards.
3. Observe there is no "requested traceroute to me" view by requester node.

## Root Cause

- Incoming packet logger helper in `src/bot/incoming.rs` does not persist packet destination (`to_node`) for non-text packet types.
- Dashboard has no query/API/UI dedicated to incoming traceroute requesters.

## Fix Plan

1. Persist incoming packet destination for traceroute packets (`to_node` from mesh packet).
2. Add DB query to aggregate incoming traceroute packets addressed to local node by requester (`from_node`).
3. Add dashboard API endpoint for traceroute requester stats (with hours window and MQTT filter support if applicable).
4. Add dashboard UI table/card for requester node, request count, and last-seen timestamp.
5. Ensure behavior is documented and tested.

## Acceptance Criteria

- Dashboard shows a list of nodes that requested traceroute to the local node.
- Each row includes requester node ID, readable name (when available), request count, and latest request timestamp.
- Time window filtering works consistently with other dashboard views.
- Data persists across restart (read from SQLite history).
- At least one backend test covers aggregation logic.

## Non-goals (for first iteration)

- Full traceroute path visualization.
- Distinguishing request/response payload semantics beyond destination-based attribution.

## Validation

- Automated:
  - `cargo test -q` (includes new DB test for requester aggregation/filtering)
  - `cd web && npm run -s build`
- Manual:
  - Send traceroute requests from at least two nodes to local node and confirm dashboard table rows/counts update.

## References

- Commit: `70f259c`
- `src/bot/incoming.rs`
- `src/db.rs`
- `src/dashboard.rs`
- `web/src/*`

## Timeline

- 2026-02-16 21:01 - Feature request recorded before implementation.
- 2026-02-16 21:10 - Backend logging/query/API implemented for traceroute requester stats.
- 2026-02-16 21:12 - Frontend traceroute requester table added.
- 2026-02-16 21:15 - Docs updated and issue marked resolved.
