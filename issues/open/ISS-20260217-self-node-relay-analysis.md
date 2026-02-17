# ID: ISS-20260217-self-node-relay-analysis

Title: Add self-node relay behavior analysis for path consistency diagnostics
Status: open
Reported: 2026-02-17
Reporter: user
Severity: medium
Component: dashboard+db+web+traceroute
Environment: Meshenger dashboard + traceroute/session data

## Symptom

Traceroute sessions are available, but relay behavior consistency and dominant relay patterns are not surfaced.

## Expected

Provide self-node relay analysis:
- Most frequent intermediate relay nodes.
- Relay-associated quality metrics (avg RSSI/SNR/hops, count).
- Optional mismatch indicator when path pattern changes sharply.

## Actual

No dedicated relay-analysis card/table exists for the local node.

## Reproduction

1. Generate repeated traceroute activity involving local node.
2. Open dashboard and compare session list across time.
3. Observe no summarized relay behavior diagnostics.

## Root Cause

- Session/hop data is captured but not aggregated into relay-centric metrics.
- Frontend has no relay-analysis component.

## Fix Plan

1. Backend
- Add endpoint: `/api/self/relay-analysis`.
- Aggregate intermediate hop frequency and quality metadata from `traceroute_session_hops`/`traceroute_sessions`.

2. Frontend
- Add `Relay Analysis` panel under traceroute insights.
- Show top relays with count and quality summary.

3. Future-proofing
- Define clear logic for what counts as intermediate relay (`exclude src/dst`).

## Validation

- DB tests for relay extraction correctness and ordering.
- API tests for exclusion rules and null handling.
- UI tests for ranking and tooltip details.
- Live test with controlled traceroute routes.

## References

- Meshmap relay analysis concept: `https://meshmap.pro/node/3954221518`
- Traceroute path-change framing: `https://grafana.com/docs/grafana-cloud/testing/synthetic-monitoring/create-checks/checks/traceroute/`

## Timeline

- 2026-02-17 12:20 - Issue created.
