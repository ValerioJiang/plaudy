# DATA-001 — Meeting audio retention: transcripts kept forever, audio purged after N days once transcribed

## Context
Retention used to delete whole history rows: with the default
(`preserve_limit`, `history_limit` = 5) the sixth meeting deleted the first
one, transcript included, because session rows are inserted `saved = 0`.
Audio is ~115 MB per meeting-hour (16 kHz s16 mono WAV) while the transcript
is a few KB. PO request 2026-11-04 ("do the retention"), PM pick from the
consent item in ATTENTION: keep meeting audio 7 days, keep transcripts.

## Files it may touch
- `handy/src-tauri/src/managers/history.rs` (one append-only migration,
  `HistoryEntry.audio_purged_at`, cleanup split, purge sweep, tests)
- `handy/src-tauri/src/settings.rs` (`SessionAudioRetention` + default)
- `handy/src-tauri/src/commands/history.rs`, `handy/src-tauri/src/lib.rs`
  (one setter command, registered)
- `handy/src/bindings.ts` (regenerated), `handy/src/stores/settingsStore.ts`
- `handy/src/components/settings/sessions/SessionsSettings.tsx`,
  `handy/src/components/settings/history/DetailPane.tsx`
- `handy/src/i18n/locales/en/translation.json`
- this story's worklog, `plan/BACKLOG.md`, `plan/ATTENTION.md`

## Interfaces it must respect
- Migrations are APPEND-ONLY (`user_version` tracking).
- Dictation rows keep the existing retention semantics exactly
  (`recording_retention_period`, `history_limit`).
- MCP stays read-only (`query_only`).

## Acceptance criteria
- [x] Session rows (source `mic`/`system`/`meeting`, or legacy
      `file_name` `session-%`) are never deleted by retention; dictation
      rows are cleaned exactly as before.
- [x] New setting `session_audio_retention`: `days7` (default) | `days30` |
      `days90` | `forever`. A sweep deletes the WAV of session rows older
      than the period **only if** `status = 'done'` and `saved = 0`, sets
      `audio_purged_at`, and emits an `Updated` history event. Failed or
      pending transcripts keep their audio whatever their age.
- [x] The sweep runs on every history insert, on setting change and at
      startup; a WAV that cannot be deleted is logged and left unmarked.
- [x] History detail: a purged row shows "Audio removed after the retention
      period. The transcript is kept." instead of a dead player.
- [x] Sessions settings: a "Keep meeting audio" dropdown.
- [ ] PO: a real purge on a transcribed meeting (recipe in the worklog; the
      file-deletion branch needs an AppHandle and is not unit-tested).
- [x] Unit tests on the pure selection queries (session vs dictation,
      status/saved/age gates, idempotence); `cargo test --lib` and MCP
      tests green.

## Out of scope
- Opus compression of kept audio; per-session "keep audio" other than the
  existing star (`saved`).

## Size
M

## Routing
Run on: the PO's session, Opus-class — a data-deleting sweep.
