# ID: ISS-20260217-self-node-protocol-quality-breakdown

Title: Add self-node protocol quality breakdown (count, ratio, RSSI/SNR/hops)
Status: open
Reported: 2026-02-17
Reporter: user
Severity: low
Component: dashboard+db+web
Environment: Meshenger dashboard

## Symptom

There is no focused protocol-quality panel for local-node traffic health.

## Expected

Add per-protocol quality breakdown for local node:
- Packet counts and percentages by protocol.
- Avg/min/max RSSI and SNR where available.
- Avg/min/max hops for relevant protocols.

## Actual

Current dashboard exposes broad packet throughput but not self-node protocol quality diagnostics.

## Reproduction

1. Open dashboard and inspect packet charts.
2. Try to answer which protocol has worst quality for local-node traffic.
3. Observe no dedicated protocol-quality panel.

## Root Cause

- No local-node protocol-specific aggregation endpoint.
- UI not optimized for protocol-level troubleshooting.

## Fix Plan

1. Backend
- Add endpoint: `/api/self/protocol-quality` with local-node and window filters.
- Compute counts, percentages, and quality stats per protocol.

2. Frontend
- Add `Protocol Quality` card/table with sortable metrics.
- Add quick badges for worst RSSI/SNR/hops protocol.

## Validation

- DB tests for protocol grouping and metric correctness.
- API tests for expected numeric fields and null handling.
- UI tests for sorting and formatting.

## References

- Meshmap protocol usage concept: `https://meshmap.pro/node/3954221518`
- Observability panel strategy (USE/RED): `https://grafana.com/docs/grafana-cloud/visualizations/dashboards/build-dashboards/best-practices/`

## Timeline

- 2026-02-17 12:20 - Issue created.
