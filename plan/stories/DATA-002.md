# DATA-002 — Full-text index (FTS5 trigram) over titles, transcripts and segments; app and MCP search use it

## Context
History search (app) and `search_sessions` (MCP) scan every row with
`LIKE '%q%'`. Fine at 3 sessions, linear in years of meetings. SQLite FTS5
with the `trigram` tokenizer indexes substrings, so the existing semantics
(case-insensitive substring, `%` `_` `\` literal) stay identical while
queries of 3+ characters use the index. Both SQLite builds support it
(app: bundled 3.50.2 with `SQLITE_ENABLE_FTS5`; MCP: Bun 3.53.4).

## Files it may touch
- `handy/src-tauri/src/managers/history.rs` (one append-only migration
  with external-content FTS tables, triggers and a rebuild; search; tests)
- `handy/mcp/db.ts`, `handy/mcp/fixture.ts`, `handy/mcp/*.test.ts`
- this story's worklog, `plan/BACKLOG.md`

## Interfaces it must respect
- Migrations APPEND-ONLY; the FTS tables are derived data kept in sync by
  triggers, never written by application code.
- MCP read-only; must still work against a DB that predates the migration
  (LIKE fallback when the FTS tables are missing).
- Search results identical to before for every existing test.

## Acceptance criteria
- [x] Migration creates `history_fts(title, transcription_text)` and
      `segments_fts(text)` (external content, trigram), insert/delete/update
      triggers (update only on the indexed columns) and rebuilds both from
      existing rows.
- [x] App search and MCP search use `MATCH` for queries of 3+ characters,
      `LIKE` below that or when the tables are missing; same results.
- [x] Tests: migrated in-memory DB finds by title, transcript, segment;
      triggers keep the index in sync on update and delete; short queries
      and metacharacters behave as before; MCP fallback on an old schema.
- [x] Timing on a synthetic 1 000-session DB, LIKE vs MATCH, in the worklog.

## Out of scope
- Relevance ranking (results stay newest-first), word/stemming search,
  semantic search.

## Size
S

## Routing
Run on: the PO's session, Opus-class.
