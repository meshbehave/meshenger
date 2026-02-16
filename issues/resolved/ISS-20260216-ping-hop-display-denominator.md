# ID: ISS-20260216-ping-hop-display-denominator

Title: Ping hop display used remaining hops as denominator
Status: resolved
Reported: 2026-02-16
Reporter: user
Severity: medium
Component: module/ping
Environment: Meshenger + Meshtastic DM ping path

## Symptom

Users can send the same `!ping` command repeatedly and see responses like:

- `Hops: 0/5`
- `Hops: 1/4`

Even when no configuration changes were made between messages.

## Expected

`!ping` should display hops as used/original-max.

## Actual

`!ping` displayed used/remaining (`hop_count/hop_limit`), which made the denominator appear inconsistent.

## Reproduction

1. Send `!ping` multiple times from the same node.
2. Observe outputs like `Hops: 0/5` and `Hops: 1/4`.
3. Confirm no config changes between attempts.

## Root Cause

The old ping output used:

- numerator: `hop_count` (used hops)
- denominator: `hop_limit` (remaining hops)

`hop_limit` is not the original maximum hop budget. It can vary packet-to-packet because it reflects the remaining budget when the packet arrives.

In the bot pipeline:

- `hop_count` is computed as `hop_start - hop_limit`
- `hop_start` is the original hop budget

So `0/5` and `1/4` are both valid for an original budget of 5, depending on route/rebroadcast behavior.

## Fix Plan

`!ping` now displays:

- `Hops: {hop_count}/{hop_start}`

This makes the denominator stable as the packet's original hop budget and aligns with user expectation of "used/max".

## Files Changed

- `src/message.rs`
  - Added `hop_start` to `MessageContext`.
- `src/bot/incoming.rs`
  - Populated `MessageContext.hop_start` from incoming packet metadata.
- `src/modules/ping.rs`
  - Changed ping output from `hop_count/hop_limit` to `hop_count/hop_start`.
- `src/bot/events.rs`, `src/bot/tests.rs`, `src/modules/node_info.rs`, `src/modules/uptime.rs`
  - Updated `MessageContext` construction for new `hop_start` field.

## Validation

- `cargo test -q`
- Result: 112 passed, 0 failed

## References

- Commit: `a3444af`
- `src/modules/ping.rs`
- `src/bot/incoming.rs`
- `src/message.rs`

## Notes

`hop_limit` is still useful for diagnostics (remaining budget at receive time), but it should not be treated as the max hop setting.

## Timeline

- 2026-02-16 19:43 - Issue documented from user report and screenshot.
- 2026-02-16 19:45 - Fix implemented to display `hop_count/hop_start`.
- 2026-02-16 19:46 - Tests passed; issue moved to `resolved`.
