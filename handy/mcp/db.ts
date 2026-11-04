// Read-only query layer over Plaude Local's history.db (the Tauri app owns all writes).
// Pure functions taking a bun:sqlite Database so they unit-test against a temp DB.
import { Database } from "bun:sqlite";
import { homedir } from "os";
import { join } from "path";

/** Default location of the app's SQLite DB; override with PLAUDE_DB for tests / portability. */
// Must match `identifier` in src-tauri/tauri.conf.json — no shared constant, keep in sync by hand.
// IF-W04: Tauri's app_data_dir() for that identifier is
//   macOS   ~/Library/Application Support/com.uppify.plaudy/
//   Windows %APPDATA%\com.uppify.plaudy\   (APPDATA falls back to ~/AppData/Roaming)
export function defaultDbPath(
  platform: NodeJS.Platform = process.platform,
  env: NodeJS.ProcessEnv = process.env,
): string {
  if (env.PLAUDE_DB) return env.PLAUDE_DB;
  return join(appDataDir(platform, env), "com.uppify.plaudy", "history.db");
}

function appDataDir(platform: NodeJS.Platform, env: NodeJS.ProcessEnv): string {
  if (platform === "win32") {
    return env.APPDATA ?? join(homedir(), "AppData", "Roaming");
  }
  return join(homedir(), "Library", "Application Support");
}

/**
 * Open the DB with writes disabled — this process never writes, so it can never corrupt a recording.
 * Not `readonly: true`: the app keeps history.db in WAL mode, and a readonly connection cannot
 * create the -shm/-wal sidecars, so it fails with SQLITE_CANTOPEN whenever the app is closed.
 * A read-write handle can; `query_only` then makes SQLite itself reject every write.
 */
export function openDb(path: string): Database {
  const db = new Database(path, { readwrite: true }); // no `create`: a missing DB must still throw
  db.exec("PRAGMA query_only = ON;");
  return db;
}

const SNIPPET_LEN = 160;
const CONTEXT = 60;

function oneLine(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

function headSnippet(text: string): string {
  const t = oneLine(text);
  return t.length > SNIPPET_LEN ? t.slice(0, SNIPPET_LEN) + "…" : t;
}

/** A snippet centered on the first occurrence of `query`, else the head of the text. */
function matchSnippet(text: string, query: string): string {
  const t = oneLine(text);
  const i = t.toLowerCase().indexOf(query.toLowerCase());
  if (i < 0) return headSnippet(t);
  const start = Math.max(0, i - CONTEXT);
  const end = Math.min(t.length, i + query.length + CONTEXT);
  return (
    (start > 0 ? "…" : "") + t.slice(start, end) + (end < t.length ? "…" : "")
  );
}

function speakerLabels(db: Database, historyId: number): string[] {
  const rows = db
    .query("SELECT label FROM speakers WHERE history_id = ? ORDER BY id")
    .all(historyId) as { label: string }[];
  return rows.map((r) => r.label);
}

export interface SessionSummary {
  id: number;
  title: string;
  timestamp: number;
  status: string;
  snippet: string;
  speakers: string[];
}

export function listSessions(
  db: Database,
  limit = 20,
  offset = 0,
): SessionSummary[] {
  const rows = db
    .query(
      `SELECT id, title, timestamp, status, transcription_text
         FROM transcription_history
        ORDER BY id DESC
        LIMIT ? OFFSET ?`,
    )
    .all(limit, offset) as Row[];
  return rows.map((r) => ({
    id: r.id,
    title: r.title,
    timestamp: r.timestamp,
    status: r.status,
    snippet: headSnippet(r.transcription_text ?? ""),
    speakers: speakerLabels(db, r.id),
  }));
}

export interface Segment {
  start_ms: number;
  end_ms: number;
  speaker: string | null;
  text: string;
}

export interface SessionDetail extends SessionSummary {
  text: string;
  segments: Segment[];
}

export function getSession(db: Database, id: number): SessionDetail | null {
  const r = db
    .query(
      `SELECT id, title, timestamp, status, transcription_text
         FROM transcription_history WHERE id = ?`,
    )
    .get(id) as Row | null;
  if (!r) return null;
  const segments = db
    .query(
      `SELECT s.start_ms AS start_ms, s.end_ms AS end_ms, sp.label AS speaker, s.text AS text
         FROM transcription_segments s
         LEFT JOIN speakers sp ON sp.id = s.speaker_id
        WHERE s.history_id = ?
        ORDER BY s.start_ms, s.id`,
    )
    .all(id) as Segment[];
  return {
    id: r.id,
    title: r.title,
    timestamp: r.timestamp,
    status: r.status,
    snippet: headSnippet(r.transcription_text ?? ""),
    speakers: speakerLabels(db, id),
    text: r.transcription_text ?? "",
    segments,
  };
}

export interface SearchHit {
  id: number;
  title: string;
  timestamp: number;
  snippet: string;
}

/** Shortest query the trigram index can serve (a trigram is 3 characters). */
const FTS_MIN_QUERY_CHARS = 3;

function hasTable(db: Database, name: string): boolean {
  return (
    db
      .query(
        "SELECT 1 AS ok FROM sqlite_master WHERE type = 'table' AND name = ?",
      )
      .get(name) != null
  );
}

/**
 * Find sessions whose flat transcript OR any speaker segment contains `query` (case-insensitive).
 *
 * DATA-002: queries of 3+ characters use the app's trigram FTS5 index (`history_fts`,
 * `segments_fts`), where a quoted string is a case-insensitive substring match: the same rows
 * as the LIKE scan, without reading every transcript. The LIKE scan stays for shorter queries
 * and for a DB the app has not migrated yet (this process never writes, so it cannot create
 * the index itself).
 */
export function searchSessions(
  db: Database,
  query: string,
  limit = 20,
): SearchHit[] {
  const useFts =
    [...query].length >= FTS_MIN_QUERY_CHARS &&
    hasTable(db, "history_fts") &&
    hasTable(db, "segments_fts");
  const rows = useFts ? ftsRows(db, query, limit) : likeRows(db, query, limit);
  return rows.map((r) => ({
    id: r.id,
    title: r.title,
    timestamp: r.timestamp,
    snippet: matchSnippet(r.transcription_text ?? "", query),
  }));
}

function ftsRows(db: Database, query: string, limit: number): Row[] {
  // One FTS5 string literal ("" escapes a quote), restricted to the transcript column so the
  // title is not searched: same scope as the LIKE path.
  const phrase = `transcription_text : "${query.replace(/"/g, '""')}"`;
  const segPhrase = `"${query.replace(/"/g, '""')}"`;
  return db
    .query(
      `SELECT h.id AS id, h.title AS title, h.timestamp AS timestamp,
              h.transcription_text AS transcription_text
         FROM transcription_history h
        WHERE h.id IN (SELECT rowid FROM history_fts WHERE history_fts MATCH ?1)
           OR h.id IN (SELECT s.history_id FROM transcription_segments s
                        WHERE s.id IN (SELECT rowid FROM segments_fts WHERE segments_fts MATCH ?2))
        ORDER BY h.id DESC
        LIMIT ?3`,
    )
    .all(phrase, segPhrase, limit) as Row[];
}

function likeRows(db: Database, query: string, limit: number): Row[] {
  // Escape LIKE metacharacters so "100%" or "snake_case" mean the literal text, not wildcards.
  const like = `%${query.replace(/[\\%_]/g, "\\$&")}%`;
  return db
    .query(
      `SELECT DISTINCT h.id AS id, h.title AS title, h.timestamp AS timestamp,
                       h.transcription_text AS transcription_text
         FROM transcription_history h
         LEFT JOIN transcription_segments s ON s.history_id = h.id
        WHERE h.transcription_text LIKE ? ESCAPE '\\' OR s.text LIKE ? ESCAPE '\\'
        ORDER BY h.id DESC
        LIMIT ?`,
    )
    .all(like, like, limit) as Row[];
}

interface Row {
  id: number;
  title: string;
  timestamp: number;
  status: string;
  transcription_text: string | null;
}
