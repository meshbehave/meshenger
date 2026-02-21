# ID: ISS-20260221-enable-sqlite-wal-mode-and-periodic-pragma-optimize

Title: Enable SQLite WAL mode and periodic PRAGMA optimize
Status: resolved
Reported: 2026-02-21
Reporter: user
Severity: low
Component: db
Environment: Meshenger on Intel N5150 / 512 GB SSD

## Symptom

SQLite is opened with default journal mode (DELETE/rollback journal). As the
`packets` table grows to millions of rows over months of continuous operation,
concurrent reads during dashboard API calls will block on any active write, and
query planner statistics will drift from reality causing suboptimal plans.

## Expected

- WAL journal mode: reads never block writes, writes never block reads.
- Query planner statistics stay accurate as table sizes grow, keeping dashboard
  API queries fast without manual intervention.

## Actual

- `Db::open()` calls `Connection::open(path)` with no PRAGMAs set — defaults
  apply (journal_mode=DELETE, no optimize scheduled).
- No periodic maintenance runs.

## Root Cause

DB initialization in `Db::open()` (`src/db.rs:139`) only opens the connection
and runs `init_schema()`. No connection-level PRAGMAs are set.

## Fix Plan

1. In `Db::open()`, after `Connection::open()` and before `init_schema()`, run:
   ```sql
   PRAGMA journal_mode=WAL;
   PRAGMA synchronous=NORMAL;
   PRAGMA optimize;
   ```
   - `journal_mode=WAL`: enables WAL mode. Persistent — survives reconnects.
   - `synchronous=NORMAL`: safe with WAL (no data loss on OS crash, only power
     loss; acceptable for a mesh logging workload). Reduces fsync overhead.
   - `optimize` at open: updates query planner statistics for all tables that
     have changed significantly since last optimize. Fast on small DBs, still
     fast on large ones (only analyzes changed tables).

2. No periodic timer needed — SQLite docs recommend running `PRAGMA optimize`
   at connection close or periodically. Running it at open is sufficient for a
   long-lived single-connection process like Meshenger. Can revisit if needed.

3. In-memory DBs used in tests are unaffected — WAL mode is silently ignored
   for `:memory:` connections.

## Validation

- `cargo test -q` passes.
- After applying, open the DB with `sqlite3 meshenger.db "PRAGMA journal_mode;"`
  and confirm it returns `wal`.
- Confirm `meshenger.db-wal` and `meshenger.db-shm` sidecar files appear on
  next run.

## References

- `src/db.rs:139` — `Db::open()`
- SQLite WAL docs: https://www.sqlite.org/wal.html
- SQLite optimize docs: https://www.sqlite.org/pragma.html#pragma_optimize

## Timeline

- 2026-02-21 — Issue created after DB growth analysis (2.5 MB / 32k rows after
  20h, dominated by nodeinfo packets; long-term retention desired).
- 2026-02-21 — Implemented: WAL + synchronous=NORMAL + startup optimize in
  `Db::open()`; `Db::optimize()` method added; event loop refactored from two
  duplicate `select!` blocks into one unified block with `if bridge_active`
  guard; periodic optimize timer added (6h interval). All 119 tests pass.
