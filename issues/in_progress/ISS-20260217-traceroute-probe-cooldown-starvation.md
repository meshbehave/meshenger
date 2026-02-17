# ID: ISS-20260217-traceroute-probe-cooldown-starvation

Title: Auto traceroute probe can starve other candidates when top candidate is in cooldown
Status: in_progress
Reported: 2026-02-17
Reporter: user
Severity: medium
Component: bot/runtime+db
Environment: Meshenger traceroute_probe with multiple RF nodes missing hops

## Symptom

Auto traceroute probe repeatedly skips due to cooldown on one node and does not probe other eligible nodes.

## Expected

When the most-recent candidate is in cooldown, probe logic should continue scanning and pick another eligible node in the same cycle.

## Actual

Current logic chooses a single candidate (`ORDER BY last_seen DESC LIMIT 1`). If that node is in cooldown, cycle exits without probing any other node.

## Reproduction

1. Enable `[traceroute_probe]` and ensure at least two RF nodes are missing inbound RF hop metadata.
2. Trigger a probe to node A so it enters cooldown.
3. Keep node A as most recently seen candidate.
4. Observe repeated logs: `Traceroute probe skipped: !<nodeA> is in cooldown (...)` and no probes sent to node B.

## Root Cause

- `recent_rf_node_missing_hops()` returns only one node (single-row query).
- `maybe_queue_traceroute_probe()` returns immediately on cooldown check failure for that node.

## Fix Plan

1. Add DB helper returning multiple candidates ordered by recency (e.g., top 10).
2. In `maybe_queue_traceroute_probe()`, iterate candidates and pick first node that passes `can_send(...)`.
3. Use adaptive candidate windows (`10 -> 25 -> 50 -> 100`) if earlier windows are exhausted by cooldown nodes.
4. If all candidates are cooling down, log clear summary with candidate count.
5. Keep existing per-node cooldown semantics unchanged.

## Validation

- Unit test: with two candidates where top is in cooldown, second candidate is selected and queued.
- Unit test: when all candidates are in cooldown, no probe is queued and summary log appears.
- Live test: verify cooldown node does not block probes to other missing-hop nodes.

## References

- `src/bot/runtime.rs`
- `src/db.rs`
- Log evidence: `Traceroute probe skipped: !04059698 is in cooldown (21600s)`
- Commit: `4ca0858`

## Timeline

- 2026-02-17 18:40 - Issue created.
- 2026-02-17 18:48 - Implemented candidate fallback selection (skip cooling-down top candidate, probe next eligible).
- 2026-02-17 18:49 - Added DB test coverage for multi-candidate missing-hop selection order and limit.
- 2026-02-17 18:57 - Upgraded probe selection to adaptive windows (`10/25/50/100`) to avoid top-window starvation.
- 2026-02-17 18:58 - Added runtime unit tests for adaptive selection behavior (expansion, all-cooldown, empty-candidate paths).
- 2026-02-17 19:49 - Cherry-picked to `main` and recorded main commit hash.
