#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT_DIR/issues/templates/ISSUE_TEMPLATE.md"
INDEX="$ROOT_DIR/issues/index.md"

usage() {
  cat << 'USAGE'
Create a filesystem issue entry and append it to issues/index.md.

Usage:
  scripts/new-issue.sh --title "Issue title" [options]

Options:
  --title TEXT           Required issue title
  --severity LEVEL       low|medium|high|critical (default: medium)
  --component NAME       Component label (default: unknown)
  --reporter NAME        Reporter label (default: unknown)
  --status-dir DIR       open|in_progress|resolved|rejected (default: open)
  --date YYYY-MM-DD      Report date (default: today)
  --dry-run              Print planned file path and ID only
  -h, --help             Show this help
USAGE
}

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "Missing required file: $path" >&2
    exit 1
  fi
}

slugify() {
  local input="$1"
  local slug
  slug="$(printf '%s' "$input" \
    | tr '[:upper:]' '[:lower:]' \
    | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//; s/-{2,}/-/g')"
  if [[ -z "$slug" ]]; then
    echo "untitled"
  else
    echo "$slug"
  fi
}

escape_table_cell() {
  printf '%s' "$1" | sed 's/|/\\|/g'
}

escape_sed_replacement() {
  printf '%s' "$1" | sed -e 's/[&|\\]/\\&/g'
}

title=""
severity="medium"
component="unknown"
reporter="unknown"
status_dir="open"
reported_date="$(date +%F)"
dry_run="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --title)
      title="${2:-}"
      shift 2
      ;;
    --severity)
      severity="${2:-}"
      shift 2
      ;;
    --component)
      component="${2:-}"
      shift 2
      ;;
    --reporter)
      reporter="${2:-}"
      shift 2
      ;;
    --status-dir)
      status_dir="${2:-}"
      shift 2
      ;;
    --date)
      reported_date="${2:-}"
      shift 2
      ;;
    --dry-run)
      dry_run="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$title" ]]; then
  echo "--title is required" >&2
  usage >&2
  exit 1
fi

case "$severity" in
  low|medium|high|critical) ;;
  *)
    echo "Invalid --severity: $severity" >&2
    exit 1
    ;;
esac

case "$status_dir" in
  open|in_progress|resolved|rejected) ;;
  *)
    echo "Invalid --status-dir: $status_dir" >&2
    exit 1
    ;;
esac

if ! date -d "$reported_date" +%F >/dev/null 2>&1; then
  echo "Invalid --date: $reported_date (expected YYYY-MM-DD)" >&2
  exit 1
fi

require_file "$TEMPLATE"
require_file "$INDEX"

slug="$(slugify "$title")"
id="ISS-$(date -d "$reported_date" +%Y%m%d)-$slug"
issue_file="$ROOT_DIR/issues/$status_dir/$id.md"
relative_issue_file="issues/$status_dir/$id.md"

if [[ -e "$issue_file" ]]; then
  echo "Issue file already exists: $relative_issue_file" >&2
  exit 1
fi

if [[ "$dry_run" == "true" ]]; then
  echo "ID: $id"
  echo "File: $relative_issue_file"
  exit 0
fi

mkdir -p "$ROOT_DIR/issues/$status_dir"

title_esc="$(escape_sed_replacement "$title")"
status_dir_esc="$(escape_sed_replacement "$status_dir")"
reported_date_esc="$(escape_sed_replacement "$reported_date")"
reporter_esc="$(escape_sed_replacement "$reporter")"
severity_esc="$(escape_sed_replacement "$severity")"
component_esc="$(escape_sed_replacement "$component")"
created_line_esc="$(escape_sed_replacement "$(date '+%Y-%m-%d %H:%M') - Issue created.")"

sed \
  -e "s|^# ID:.*|# ID: $id|" \
  -e "s|^Title:.*|Title: $title_esc|" \
  -e "s|^Status:.*|Status: $status_dir_esc|" \
  -e "s|^Reported:.*|Reported: $reported_date_esc|" \
  -e "s|^Reporter:.*|Reporter: $reporter_esc|" \
  -e "s|^Severity:.*|Severity: $severity_esc|" \
  -e "s|^Component:.*|Component: $component_esc|" \
  -e "s/^Environment:.*/Environment: TBD/" \
  -e "s|^- YYYY-MM-DD HH:MM - Issue created\.|- $created_line_esc|" \
  "$TEMPLATE" > "$issue_file"

escaped_title="$(escape_table_cell "$title")"
escaped_component="$(escape_table_cell "$component")"

printf '| %s | %s | %s | %s | %s | %s | `%s` |\n' \
  "$id" \
  "$escaped_title" \
  "$status_dir" \
  "$severity" \
  "$escaped_component" \
  "$reported_date" \
  "$relative_issue_file" >> "$INDEX"

echo "Created: $relative_issue_file"
echo "Updated: issues/index.md"
