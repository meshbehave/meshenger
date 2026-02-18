# ID: ISS-20260217-traceroute-session-parity-outbound-inbound

Title: Achieve full traceroute session parity by correlating outbound probes with inbound responses
Status: in_progress
Reported: 2026-02-17
Reporter: user
Severity: medium
Component: bot/outgoing+bot/incoming+db+dashboard
Environment: Meshenger dashboard + Meshtastic traceroute traffic

## Symptom

Traceroute session logging is asymmetric:
- Outbound auto-probes create session metadata rows.
- Incoming traceroute packets can add richer session data and hop rows.
- Outbound probes do not yet correlate with later inbound responses into one complete request/response session.

## Expected

For one traceroute flow, session model should show full parity:
- Outbound probe/request is tracked as request side of session.
- Inbound response (or related route data) is correlated into the same session.
- Session status evolves (`observed` -> `partial` -> `complete`) consistently.
- Hop rows are added for available request/response path data regardless of direction origin.

## Actual

Final scoped behavior (resolved):
- Traceroute sessions are keyed by protocol-aligned request identity: `req:<src>:<dst>:<request_id>`.
- Outbound probes and inbound/routing updates correlate via request ID without heuristic cross-session matching.
- Routing correlation only updates sessions when a matching traceroute request session exists (prevents spurious sessions).
- Session detail renders `Route` and optional `Route Back` when path vectors are available; otherwise it explicitly shows `Path unavailable on this node` (or direct-link hint for 0-hop sessions).
- Scope intentionally limited to packets observed by the currently connected node (not full-air interception of all mesh traffic).

## Reproduction

1. Run live test with `RUST_LOG=trace`.
2. Let auto-probe send traceroute.
3. Inspect logs and DB summary sections.
4. Observe outbound session row exists but no hop rows, and parity with incoming response is not guaranteed.

## Root Cause

- Outbound session identity was timestamp-based, while Meshtastic response correlation uses request packet IDs.
- Existing fallback matching (recent src/dst window) was heuristic and could mis-pair bursts.
- Session schema supports request/response fields, but identity strategy was not aligned to protocol-level request/response pairing.

## Fix Plan

1. Correlation design
- Use deterministic session identity `req:<src>:<dst>:<request_id>` for both request and response observations.
- Generate/store outbound traceroute `request_id` at send time.

2. DB/session update rules
- Merge outbound and inbound observations into one session when correlation matches.
- Preserve packet references for both sides (`request_packet_id`, `response_packet_id`).
- Promote `status` based on merged evidence.

3. Hop recording parity
- Add hop rows for whichever side includes route data.
- Ensure no duplicate hop rows for repeated updates of same packet.

4. API/UI consistency
- Session list/detail should reflect merged request+response fields and paths.
- Keep current dashboard contracts stable where possible.

5. Regression validation
- Keep test suite green after removing heuristic correlation code.
- Validate with live tests where another node sends traceroute requests/responses during the run window.

## Implementation Plan (Path Parity Phase)

1. Routing path extraction
- Parse `RoutingApp` payload variants and extract path vectors when available:
  - `RouteRequest(route.route, route.route_back)`
  - `RouteReply(route.route, route.route_back)`
- Attach extracted vectors to matched traceroute session updates keyed by `request_id`.

2. Path provenance tagging
- Preserve source provenance in `traceroute_session_hops.source_kind`:
  - direct traceroute payload: `route` / `route_back`
  - routing-derived request/reply path: `routing_route` / `routing_route_back`
- Keep dashboard response contract stable by reusing existing `source_kind` field.

3. Session status parity
- Ensure routing-derived path evidence can promote session state toward `complete` when request and response sides are both observed.

4. Regression tests (detailed)
- Unit tests for routing payload parser:
  - `RouteRequest` with non-empty route vectors.
  - `RouteReply` with non-empty route vectors.
  - decode failure / unknown variant path extraction fallback.
- DB-level tests for hop provenance and status:
  - inserting request + response path with custom source kinds produces ordered hop rows and `complete`.
  - routing-source hop rows appear with expected `source_kind`.
- Full test suite revalidation (`cargo test -q`).

## Validation

- Unit/DB tests for correlation correctness and session state transitions.
- Live test evidence with both sides present in one correlated session.
- Verify `Traceroute Sessions During Window` and `Traceroute Session Hops During Window` include merged data.
- Final scope validation confirmed expected behavior for connected-node visibility and no false self/destination sessions.

## References

- Parent requirement (resolved): `issues/resolved/ISS-20260217-traceroute-hops-to-me-path-visualization.md`
- Related log script: `scripts/run-meshenger-live-test.sh`
- Live test evidence log: `/tmp/meshenger-live-test-20260217-195129.log`
- Commit: `9ba3824`
- Reverted commit (as discussed): `9ba38241d27e9dc2f9ce287bdc83bc6289b0a290`

## Timeline

- 2026-02-17 20:07 - Follow-up issue created and set to in_progress for full outbound/inbound parity.
- 2026-02-17 20:08 - Confirmed current state via live test: outbound session rows present, no parity correlation yet.
- 2026-02-17 20:22 - Added DB lookup for recent session key (`find_recent_traceroute_session_key`) to support inbound->outbound session correlation.
- 2026-02-17 20:24 - Updated incoming traceroute handling to merge packets addressed to local node into recent outbound sessions (response-side attribution).
- 2026-02-17 20:26 - Added test coverage for session-key lookup and revalidated full suite (`cargo test -q`: 118 passed).
- 2026-02-17 21:36 - Added routing-based request-id correlation (`Data.request_id`) to attach firmware-originated routing responses to traceroute sessions.
- 2026-02-17 21:37 - Added DB lookup by traceroute request mesh packet id and test coverage; full suite revalidated (`cargo test -q`: 119 passed).
- 2026-02-17 21:45 - Added routing fallback correlation (same source/destination within 180s) when request_id cannot be matched to stored traceroute mesh IDs.
- 2026-02-17 21:46 - Revalidated after fallback correlation change (`cargo test -q`: 119 passed).
- 2026-02-17 21:51 - Reworked traceroute session identity to protocol-aligned key `req:<src>:<dst>:<request_id>`.
- 2026-02-17 21:51 - Outgoing traceroute send now assigns/stores explicit request mesh ID and transmits packet with that ID.
- 2026-02-17 21:51 - Removed heuristic fallback correlation and obsolete DB lookup helpers.
- 2026-02-17 21:51 - Revalidated suite after strict-correlation refactor (`cargo test -q`: 117 passed).
- 2026-02-17 22:04 - Live test `/tmp/meshenger-live-test-20260217-215927.log` showed false routing correlations creating spurious sessions (`req:699c7dcc:...`) from `Routing.error_reason` packets.
- 2026-02-17 22:04 - Routing correlation now requires pre-existing traceroute request session lookup by `request_id` (request packet id), otherwise skips update.
- 2026-02-17 22:04 - Added DB lookup helper `find_traceroute_session_by_request_mesh_id` with strict-first (`request_packet_id`) plus canonical key fallback for outbound observed sessions lacking `request_packet_id`.
- 2026-02-17 22:04 - Added test coverage for request-id session lookup; suite revalidated (`cargo test -q`: 118 passed).
- 2026-02-17 22:08 - Live test `/tmp/meshenger-live-test-20260217-220520.log` confirmed spurious self/destination sessions are gone; only expected request sessions remain for Obvia->Vividea plus outbound probe session.
- 2026-02-17 22:23 - Planned Path Parity Phase: routing path extraction + provenance tagging + targeted parser/DB regression tests before next live verification.
- 2026-02-17 22:23 - Implemented routing path extraction for `RoutingApp` (`RouteRequest`/`RouteReply`) and attached parsed path vectors to correlated traceroute session updates.
- 2026-02-17 22:23 - Added hop provenance tagging for routing-derived paths via `source_kind` (`routing_route`, `routing_route_back`) while preserving existing direct traceroute values (`route`, `route_back`).
- 2026-02-17 22:23 - Added parser tests in `src/bot/incoming.rs` and DB regression tests for hop source kinds/status completion; suite revalidated (`cargo test -q`: 122 passed).
- 2026-02-17 23:01 - Frontend session detail updated to render phone-style `Route` with optional `Route Back`; when hop rows are absent it now shows `Path unavailable on this node`.
- 2026-02-17 23:26 - Added session table guidance text (session-key + Request/Response/Samples definitions) in frontend/docs.
- 2026-02-17 23:26 - Issue closed as resolved under connected-node traceroute visibility scope.
- 2026-02-18 07:01 - Reopened to in_progress and reverted code introduced by `9ba38241d27e9dc2f9ce287bdc83bc6289b0a290` as per discussion: behavior is too flaky in live operation (request-side hop fields mostly blank and session history not reliably showing usable hop metadata).
