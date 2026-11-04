# DATA-002 — worklog

Written for someone who has never seen the conversation that produced this
story. Update it whenever you finish a step, hit a dead end, or get blocked,
and commit it with the code.

## State
Started 2026-11-04T14:30Z in the main checkout with DATA-001 (same
`history.rs`; the FTS migration is #9, after DATA-001's #8, so commit
DATA-001 first or both together). Done 2026-11-04T15:20Z, **self-reported**.
Nothing waits on the PO.

## Done
- `managers/history.rs` migration #9: `history_fts(title,
  transcription_text)` and `segments_fts(text)`, FTS5 external-content tables
  with `tokenize='trigram'`; insert / delete / update triggers (update only
  `OF` the indexed columns, so status and star flips do not touch the index);
  `'rebuild'` of both at the end so rows older than the migration are indexed.
- Why trigram: a quoted FTS5 string under the trigram tokenizer is a
  case-insensitive substring match, i.e. the old `LIKE '%q%'` semantics with
  an index. No change to what a search returns; `%`, `_`, `\`, `"` stay
  literal.
- App search (`search_entries_conn`): queries of 3+ characters use
  `history_fts MATCH` (title + transcript, as before); shorter queries, or a DB
  without the table, use the LIKE scan. `fts_phrase()` quotes the query and
  doubles `"`.
- MCP (`mcp/db.ts`): same rule; scope kept identical to the old LIKE (the
  transcript column `transcription_text : "q"` plus `segments_fts`, not the
  title). `hasTable()` falls back to LIKE on a DB the app has not migrated
  (the MCP is read-only and cannot create the index). `fixture.ts` gets the
  FTS DDL (copied by hand, like its schema) and `createDb(path, { fts })`.
- SQLite builds: app bundles 3.50.2 with `-DSQLITE_ENABLE_FTS5`
  (libsqlite3-sys 0.35 build.rs); Bun 1.4.4 ships 3.53.4. Trigram exists
  since 3.34. Both verified live.
- Tests, Rust (real migrations): `fts_finds_by_title_and_transcript_case_
  insensitively_newest_first`, `fts_follows_updates_and_deletes` (the
  finalize path fills the transcript with an UPDATE), `segment_index_follows_
  inserts_and_the_cascade_delete`, `rows_that_predate_the_index_are_found_
  after_the_migration`, `fts_and_the_like_fallback_return_the_same_rows`
  (9 queries incl. `100%`, `e_c`, `C:\temp`, `"doppie"`, `PERCHÉ`; identical
  for ASCII; for non-ASCII letters FTS may find strictly more because the
  trigram folds Unicode case and LIKE folds ASCII only). Existing search tests
  unchanged and green.
- Tests, MCP: `FTS and the LIKE fallback (pre-migration DB) return the same
  sessions` (10 queries), `the FTS index follows a transcript filled in later
  and a deleted session`. `bun test`: **18 pass, 0 fail** (was 16).
- Timing, MCP `searchSessions`, 1 000 synthetic one-hour meetings (~55 KB of
  transcript and 600 segments each, in-memory DB):

  | Query | LIKE scan | Trigram index |
  |-------|-----------|---------------|
  | rare word, 1 hit | 16 879 ms | 1.6 ms |
  | two common words, 0 hits | 24 848 ms | 98.6 ms |
  | absent word | 29 747 ms | 0.1 ms |

  Loading cost: 0.5 s without the index vs 20.7 s with it for the 1 000
  meetings in one transaction, i.e. ~20 ms of indexing per meeting-hour at
  finalize. The LIKE path is slow mostly because of the segments LEFT JOIN.
  Bench script: `scratchpad/bench.ts` of this session (not committed).
- Real DB on the laptop: migrated 7 -> 9 (both stories), `user_version` 9,
  `history_fts` and
  `segments_fts` present, 3 documents indexed.

## In flight
Nothing half-changed.

## Tried and rejected
- `unicode61` word tokenizer with prefix queries: rejected, changes results
  (no mid-word match, punctuation dropped), breaks the existing literal-
  metacharacter tests, and would need its own query grammar.
- Relevance ranking (bm25): out of scope; results stay newest-first in both
  callers, as before.
- Union of FTS and LIKE for safety: pointless, the equivalence test shows
  they agree.

## Blocked on
Nothing.

## Next action
Commit (message start `DATA-002:`) after DATA-001.
