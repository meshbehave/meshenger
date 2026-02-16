# ID: ISS-20260216-dashboard-traceroute-traffic-tabs

Title: Dashboard traceroute view should include all seen destinations with Events/Destinations tabs
Status: resolved
Reported: 2026-02-16
Reporter: user
Severity: medium
Component: dashboard+db+web
Environment: Meshenger dashboard + Meshtastic traceroute traffic

## Symptom

Existing dashboard traceroute view only covered requests addressed to local node, so users could not inspect all seen traceroute destinations.

## Expected

Dashboard should expose all seen incoming traceroute traffic with two views:

- `Events` tab for raw recent packets (time/from/to/source/hops/RSSI/SNR)
- `Destinations` tab for grouped target statistics

## Actual

Implemented:

- Added backend query/API for traceroute events (`/api/traceroute-events`).
- Added backend query/API for destination summary (`/api/traceroute-destinations`).
- Added tabbed frontend panel `TracerouteTrafficPanel` with `Events` and `Destinations` tabs.

## Reproduction

1. Generate incoming traceroute packets from one or more source nodes to various destination nodes.
2. Open dashboard and locate Traceroute Traffic panel.
3. Verify Events and Destinations tabs both show data within selected time range/MQTT filter.

## Root Cause

Previous implementation was scoped to `to_node = local_node_id` and aggregated only by requester.

## Fix Plan

1. Keep existing `to me` endpoint for compatibility.
2. Add broad traceroute event and destination summary queries.
3. Expose both through dashboard API.
4. Replace single traceroute table with tabbed panel.
5. Update docs.

## Validation

- `cargo test -q`
- `cd web && npm run -s build`

## References

- Commit: `bba63e6`
- `src/db.rs`
- `src/dashboard.rs`
- `web/src/components/TracerouteTrafficPanel.tsx`
- `web/src/App.tsx`

## Timeline

- 2026-02-16 22:05 - Enhancement requested (all seen destinations + two tabs).
- 2026-02-16 22:12 - Backend queries/endpoints implemented.
- 2026-02-16 22:18 - Frontend tabbed panel implemented.
- 2026-02-16 22:20 - Docs and tracker updated; issue resolved.
