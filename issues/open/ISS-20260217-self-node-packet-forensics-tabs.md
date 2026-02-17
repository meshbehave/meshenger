# ID: ISS-20260217-self-node-packet-forensics-tabs

Title: Add self-node packet forensics tabs (sent/reported with packet drilldown)
Status: open
Reported: 2026-02-17
Reporter: user
Severity: medium
Component: dashboard+db+web
Environment: Meshenger dashboard + Meshtastic traffic

## Symptom

Dashboard has strong aggregates, but troubleshooting specific packet behavior for the local node is hard.

## Expected

For local node context, provide packet forensics tabs with clear packet-level drilldown:
- `Sent` and `Reported` tabs for local node packet activity.
- Filters by packet type, time range, and source transport (RF/MQTT).
- Direct jump to packet detail view (or expandable inline packet metadata).

## Actual

Current dashboard focuses on aggregate charts/tables and traceroute traffic, with limited packet-forensics workflow for the local node.

## Reproduction

1. Open dashboard while troubleshooting a packet anomaly.
2. Try to identify where a specific packet was sent/reported and with what RF metadata.
3. Observe no dedicated self-node sent/reported packet forensics panel with drilldown path.

## Root Cause

- API/data model exposes packet aggregates better than packet investigation views.
- Frontend lacks a local-node packet forensic component optimized for incident debugging.

## Fix Plan

1. Backend
- Add self-node packet query endpoint(s): `/api/self/packets?view=sent|reported&...`.
- Include packet fields needed for debugging: `timestamp`, `packet_type`, `from_node`, `to_node`, `via_mqtt`, `rssi`, `snr`, `hop_count`, `hop_start`, `mesh_packet_id`, and optional text preview.

2. Frontend
- Add `Self Packet Forensics` panel with tabs `Sent` and `Reported`.
- Add filter bar: type + transport + time range.
- Add packet row action for detail expansion (or link to future packet detail route).

3. Documentation
- Update README dashboard section and issue index when implemented.

## Validation

- Unit/DB tests for sent/reported local-node filtering and sort order.
- API response schema tests for expected fields.
- UI tests for tab switch, filters, empty state, and row detail behavior.
- Live run: confirm packet from logs appears in self-node forensics table with matching metadata.

## References

- Meshmap node details concept: `https://meshmap.pro/node/3954221518`
- Grafana dashboard drilldown guidance: `https://grafana.com/docs/grafana/latest/dashboards/build-dashboards/best-practices/`

## Timeline

- 2026-02-17 12:20 - Issue created.
