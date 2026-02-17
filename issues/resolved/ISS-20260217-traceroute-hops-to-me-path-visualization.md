# ID: ISS-20260217-traceroute-hops-to-me-path-visualization

Title: Dashboard should show hops-to-me metrics and traceroute path visualization by session
Status: resolved
Reported: 2026-02-17
Reporter: user
Severity: medium
Component: dashboard+db+bot/incoming+web
Environment: Meshenger dashboard + Meshtastic traceroute traffic (RF and MQTT)

## Symptom

Current dashboard surfaces traceroute traffic events and destination summaries, but it cannot answer:
- "How many hops does node X take to reach me?"
- "What path did this traceroute follow (request/response)?"

## Expected

Dashboard should provide:
- Node-level hops-to-me metrics (last/min/avg/max/sample count/last seen).
- A per-session traceroute path view to inspect hop sequence.
- Clear separation between request path and response path where possible.

## Actual

- We store packet-level traceroute rows with `hop_count`/`hop_start` in `packets`.
- Existing views are event list and destination summary only.
- No session correlation model and no path timeline/graph for traceroute sessions.

## Reproduction

1. Send traceroute requests among multiple nodes including to local node.
2. Open dashboard and inspect traceroute panels.
3. Observe no dedicated hops-to-me aggregate by source node and no per-session path visualization.

## Root Cause

- Data model is packet-centric only; no explicit traceroute session/path index.
- API currently exposes traceroute events and destination aggregates, not sessionized route details.
- Frontend has no path/session component.

## Fix Plan

1. Ticketing and design hardening
- Finalize API contracts and schema in this issue before coding.

2. Minimal refactor (anti-spaghetti)
- Keep `packets` as source of truth.
- Introduce traceroute-focused DB query module/functions to avoid bloating generic dashboard queries.
- Keep dashboard handlers thin and typed.

3. Schema extension for derived session/path data
- Add `traceroute_sessions` table for correlation metadata:
  - `id`, `trace_key`, `src_node`, `dst_node`, `first_seen`, `last_seen`, `status`, `sample_count`.
- Add `traceroute_session_hops` table for ordered path steps:
  - `id`, `session_id`, `direction` (`request`/`response`), `hop_index`, `node_id`, `observed_at`, `packet_id_ref`, `source_kind`.
- Avoid duplicating full packet payloads; store `packet_id_ref -> packets.id`.

4. Backend feature work
- Ingest/correlation logic for traceroute packets to build/update sessions and hops.
- Add API: `/api/hops-to-me` (aggregate by source node).
- Add API: `/api/traceroute-sessions` and `/api/traceroute-sessions/:id`.

5. Frontend feature work
- New dashboard section/tab: "Hops To Me" summary table + distribution chart.
- Session list + session details panel for path timeline (and optional graph later).

6. Documentation updates
- README dashboard/API section.
- issues/index status progression.
- Add fix commit hash under `## References` at resolution.

## Validation

- Unit tests for correlation and hop invariants.
- DB tests for:
  - hops-to-me aggregates
  - session correlation (complete/partial)
  - ordering of `traceroute_session_hops`
- API tests for new endpoints and query parameters.
- Frontend checks for loading/empty/data/error states and tab/session interactions.
- Live test (>= 5 minutes) with real traceroute traffic and DB/API/UI cross-check.

## References

- `issues/resolved/ISS-20260216-dashboard-traceroute-requesters.md`
- `issues/resolved/ISS-20260216-dashboard-traceroute-traffic-tabs.md`
- Commit: `5cb872f`

## Timeline

- 2026-02-17 07:30 - Issue created with phased plan: ticket -> minimal refactor -> feature implementation.
- 2026-02-17 12:08 - Implemented backend schema/API and frontend traceroute insights panel.
- 2026-02-17 12:09 - Validation completed (`cargo test -q`, `cd web && npm run -s build`).
- 2026-02-17 12:10 - Issue moved to resolved with fix commit reference.
