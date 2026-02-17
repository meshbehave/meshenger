#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-$PWD}"
CONFIG_PATH="${2:-config.toml}"
DURATION_SECS="${3:-300}"
LOG_LEVEL="${RUST_LOG:-info}"

cd "$ROOT_DIR"

if [[ ! -f "$CONFIG_PATH" ]]; then
  echo "Config not found: $CONFIG_PATH" >&2
  exit 1
fi

if ! command -v sqlite3 >/dev/null 2>&1; then
  echo "sqlite3 is required for DB verification output" >&2
  exit 1
fi

LOG_FILE="/tmp/meshenger-live-test-$(date +%Y%m%d-%H%M%S).log"
START_TS="$(date +%s)"

if command -v rg >/dev/null 2>&1; then
  SEARCH_CMD='rg -n'
else
  SEARCH_CMD='grep -nE'
fi

echo "[test] root=$ROOT_DIR"
echo "[test] config=$CONFIG_PATH"
echo "[test] duration=${DURATION_SECS}s"
echo "[test] rust_log=$LOG_LEVEL"
echo "[test] log=$LOG_FILE"
echo "[test] start=$(date '+%Y-%m-%d %H:%M:%S %Z')"

RUST_LOG="$LOG_LEVEL" timeout "${DURATION_SECS}s" scripts/run-with-web.sh "$CONFIG_PATH" >"$LOG_FILE" 2>&1 || true

END_TS="$(date +%s)"
echo "[test] end=$(date '+%Y-%m-%d %H:%M:%S %Z')"

echo
echo "=== Startup / Traceroute Log Lines ==="
# shellcheck disable=SC2086
$SEARCH_CMD "Bot node ID|Connected and configured|Traceroute from|Traceroute detail|Traceroute packet logged|Traceroute session updated|Traceroute session update failed|Outgoing traceroute detail|Outgoing traceroute session updated|Outgoing traceroute session update failed|Sending queued traceroute|Queued traceroute" "$LOG_FILE" || true

echo
echo "=== Traceroute Packets During Window ==="
sqlite3 meshenger.db "
select id,
       datetime(timestamp,'unixepoch','localtime') as ts,
       direction,
       printf('!%08x',from_node) as from_id,
       case when to_node is null then 'broadcast' else printf('!%08x',to_node) end as to_id,
       via_mqtt,
       hop_count,
       hop_start,
       mesh_packet_id
from packets
where packet_type='traceroute'
  and timestamp between $START_TS and $END_TS
order by id desc;
"

echo
echo "=== Traceroute Sessions During Window ==="
sqlite3 meshenger.db "
select id,
       datetime(first_seen,'unixepoch','localtime') as first_seen,
       datetime(last_seen,'unixepoch','localtime') as last_seen,
       trace_key,
       printf('!%08x',src_node) as src,
       case when dst_node is null then 'broadcast' else printf('!%08x',dst_node) end as dst,
       via_mqtt,
       request_hops,
       request_hop_start,
       response_hops,
       response_hop_start,
       status,
       sample_count,
       request_packet_id,
       response_packet_id
from traceroute_sessions
where last_seen between $START_TS and $END_TS
order by id desc;
"

echo
echo "=== Traceroute Session Hops During Window ==="
sqlite3 meshenger.db "
select h.id,
       h.session_id,
       h.direction,
       h.hop_index,
       printf('!%08x',h.node_id) as node_id,
       datetime(h.observed_at,'unixepoch','localtime') as observed_at,
       h.packet_id_ref,
       h.source_kind
from traceroute_session_hops h
where h.observed_at between $START_TS and $END_TS
order by h.session_id desc, h.direction, h.hop_index;
"

echo
echo "[test] done. full log: $LOG_FILE"
echo "[test] COMPLETE"
# Terminal bell (if enabled in your terminal settings)
printf '\a'
