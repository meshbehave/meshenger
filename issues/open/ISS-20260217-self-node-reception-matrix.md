# ID: ISS-20260217-self-node-reception-matrix

Title: Add self-node reception matrix for packet receive/report visibility
Status: open
Reported: 2026-02-17
Reporter: user
Severity: medium
Component: dashboard+db+web
Environment: Meshenger dashboard + multi-node mesh traffic

## Symptom

It is difficult to assess who observed local-node packets and how consistently across packet history.

## Expected

Provide a local-node reception matrix view:
- Rows: recent local-node packets.
- Columns: observers/receivers (or gateway/source categories depending on available data).
- Cells: received/not-received and optional quality indicator.

## Actual

No matrix-style local-node reception visibility exists in current dashboard.

## Reproduction

1. Send multiple packets from local node.
2. Open dashboard to compare packet visibility patterns.
3. Observe no matrix to inspect per-packet receive/report coverage.

## Root Cause

- Existing views are aggregates and list tables, not matrix-form visibility analysis.
- Data is present at packet level but not shaped for matrix use.

## Fix Plan

1. Backend
- Add endpoint: `/api/self/reception-matrix`.
- Return a bounded window with packet IDs and observer keys.
- Include metadata for tooltip/detail (timestamp/type/rssi/snr/hops where available).

2. Frontend
- Add `Reception Matrix` tab under self diagnostics.
- Provide compact matrix rendering with hover details and packet linkouts.

3. Performance
- Cap row/column cardinality and use pagination/windowing.

## Validation

- DB/API tests for matrix shape and deterministic ordering.
- UI tests for render performance, hover/detail, and empty-state.
- Live test to verify new packets appear in matrix window.

## References

- Meshmap matrix-style idea: `https://meshmap.pro/node/3954221518`
- Trace table + detail workflow concept: `https://grafana.com/docs/learning-journeys/visualization-traces/`

## Timeline

- 2026-02-17 12:20 - Issue created.
