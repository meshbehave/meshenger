# Filesystem Issue Tracker

This repository tracks issues as Markdown files under `issues/`.

## Directory Layout

- `issues/open/` - Newly reported, not yet started.
- `issues/in_progress/` - Actively being worked.
- `issues/resolved/` - Fixed and verified.
- `issues/rejected/` - Invalid, duplicate, or won't-fix.
- `issues/templates/` - Reusable templates.

## File Naming

Use:

`ISS-YYYYMMDD-short-kebab-slug.md`

Example:

`ISS-20260216-ping-hop-display-denominator.md`

## Required Metadata

Every issue file should include these fields near the top:

- `ID`
- `Title`
- `Status` (`open`, `in_progress`, `resolved`, `rejected`)
- `Reported`
- `Reporter`
- `Severity` (`low`, `medium`, `high`, `critical`)
- `Component`
- `Environment`

## Lifecycle

1. Create in `issues/open/` from `issues/templates/ISSUE_TEMPLATE.md`.
   - Or use `scripts/new-issue.sh --title "..."` to generate from template automatically.
2. Add an entry to `issues/index.md` in the same change.
3. Move file between status folders as work progresses.
4. Keep a `Timeline` section with timestamped updates.
5. Mark `resolved` only after fix + validation are documented.

## Helper Script

Create a new issue quickly:

```sh
scripts/new-issue.sh --title "Ping output mismatches hop max" \
  --severity medium \
  --component module/ping \
  --reporter user
```

Preview without writing files:

```sh
scripts/new-issue.sh --title "Example issue" --dry-run
```

## Search Tips

- Find all open issues: `rg -n "^Status:\s+open" issues/open`
- Find high severity: `rg -n "^Severity:\s+high|^Severity:\s+critical" issues`
- Find issues by component: `rg -n "^Component:\s+bot|^Component:\s+module/ping" issues`
