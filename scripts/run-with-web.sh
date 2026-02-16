#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_PATH="${1:-config.toml}"

cd "$ROOT_DIR"

if [[ ! -f "$CONFIG_PATH" ]]; then
  echo "Config file not found: $CONFIG_PATH" >&2
  echo "Usage: scripts/run-with-web.sh [config-path]" >&2
  exit 1
fi

echo "[run-with-web] Building frontend (web/dist)..."
(
  cd web
  npm run build
)

echo "[run-with-web] Starting meshenger with config: $CONFIG_PATH"
exec cargo run -- "$CONFIG_PATH"
