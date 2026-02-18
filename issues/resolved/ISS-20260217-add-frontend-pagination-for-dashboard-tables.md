# ID: ISS-20260217-add-frontend-pagination-for-dashboard-tables

Title: Add frontend pagination for dashboard tables
Status: resolved
Reported: 2026-02-17
Reporter: user
Severity: medium
Component: dashboard+web
Environment: Dashboard web frontend (`web/`) with large node/traceroute result sets

## Symptom

Dashboard tables can become long and difficult to scan when many rows are present.
Scrolling through large unpaginated tables degrades usability.

## Expected

Frontend should paginate large tables while keeping backend APIs unchanged.

## Actual

Before fix, tables rendered all rows at once:
- Nodes table
- Traceroute Traffic `Events` and `Destinations`
- Traceroute Insights `Hops To Me` and `Sessions`

## Reproduction

1. Open dashboard with many node/traceroute rows.
2. Navigate to Node table or traceroute panels.
3. Observe all rows rendered without page controls.

## Root Cause

Frontend table components did not implement client-side pagination state.

## Fix Plan

1. Add reusable frontend pagination controls.
2. Apply pagination to large tables only (no API changes).
3. Update documentation to clarify frontend pagination behavior.

## Validation

- `npm run build` succeeds after pagination changes.
- Tables show `Prev/Next`, current page, row range, and page-size selector.
- Tab switches reset pagination to page 1 to avoid empty views.

## References

- `web/src/components/PaginationControls.tsx`
- `web/src/components/NodeTable.tsx`
- `web/src/components/TracerouteTrafficPanel.tsx`
- `web/src/components/TracerouteInsightsPanel.tsx`

## Timeline

- 2026-02-17 23:40 - Issue created.
- 2026-02-17 23:42 - Added reusable `PaginationControls` component and integrated into Node and traceroute tables.
- 2026-02-17 23:42 - Updated docs (`README.md`, `AGENTS.md`) to note frontend-only pagination scope.
- 2026-02-17 23:42 - Validated with `npm run build`.
