# ID: ISS-20260217-self-node-location-history-timeline

Title: Add self-node location history timeline and map playback
Status: open
Reported: 2026-02-17
Reporter: user
Severity: low
Component: dashboard+db+web
Environment: Meshenger dashboard with position packets

## Symptom

Dashboard map shows current points but lacks a self-node location history timeline view for movement/debug context.

## Expected

For local node:
- Historical location trail in selected time window.
- Optional timeline/playback to inspect movement and signal changes over time.

## Actual

No dedicated local-node location history timeline/playback panel.

## Reproduction

1. Run node with changing positions over time.
2. Open dashboard map.
3. Observe missing local-node timeline/history playback workflow.

## Root Cause

- Position data ingestion exists, but history-oriented API/UI for local node is missing.

## Fix Plan

1. Backend
- Add endpoint: `/api/self/location-history?hours=...`.
- Return ordered position points with timestamps and optional quality overlays.

2. Frontend
- Add `Location History` panel with map polyline + time scrubber.
- Add simple controls: play/pause, speed, window.

3. Safety/UX
- Handle sparse/no-GPS data gracefully.

## Validation

- DB tests for time-window filtering and order.
- API tests for empty and populated histories.
- UI tests for playback controls and map rendering.

## References

- Meshmap location history concept: `https://meshmap.pro/node/3954221518`

## Timeline

- 2026-02-17 12:20 - Issue created.
