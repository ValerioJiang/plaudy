// Shared test fixture mirroring the app's history.db shape (migrations #1/#5/#6) so the
// query layer and the spawned server are exercised against the real schema without
// depending on a populated ~/Library DB. Not a .test.ts file — bun test skips it.
import { Database } from "bun:sqlite";

const SCHEMA = `
  CREATE TABLE transcription_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_name TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    saved BOOLEAN NOT NULL DEFAULT 0,
    title TEXT NOT NULL,
    transcription_text TEXT NOT NULL,
    post_processed_text TEXT,
    post_process_prompt TEXT,
    post_process_requested BOOLEAN NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'done'
  );
  CREATE TABLE speakers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    history_id INTEGER NOT NULL,
    label TEXT NOT NULL,
    embedding BLOB
  );
  CREATE TABLE transcription_segments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    history_id INTEGER NOT NULL,
    speaker_id INTEGER,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    text TEXT NOT NULL,
    confidence REAL
  );
`;

// DATA-002 migration, copied from history.rs (keep in sync by hand, like SCHEMA above).
const FTS_SCHEMA = `
  CREATE VIRTUAL TABLE history_fts USING fts5(
    title, transcription_text,
    content='transcription_history', content_rowid='id', tokenize='trigram'
  );
  CREATE TRIGGER history_fts_ai AFTER INSERT ON transcription_history BEGIN
    INSERT INTO history_fts(rowid, title, transcription_text)
    VALUES (new.id, new.title, new.transcription_text);
  END;
  CREATE TRIGGER history_fts_ad AFTER DELETE ON transcription_history BEGIN
    INSERT INTO history_fts(history_fts, rowid, title, transcription_text)
    VALUES ('delete', old.id, old.title, old.transcription_text);
  END;
  CREATE TRIGGER history_fts_au
  AFTER UPDATE OF title, transcription_text ON transcription_history BEGIN
    INSERT INTO history_fts(history_fts, rowid, title, transcription_text)
    VALUES ('delete', old.id, old.title, old.transcription_text);
    INSERT INTO history_fts(rowid, title, transcription_text)
    VALUES (new.id, new.title, new.transcription_text);
  END;
  CREATE VIRTUAL TABLE segments_fts USING fts5(
    text, content='transcription_segments', content_rowid='id', tokenize='trigram'
  );
  CREATE TRIGGER segments_fts_ai AFTER INSERT ON transcription_segments BEGIN
    INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
  END;
  CREATE TRIGGER segments_fts_ad AFTER DELETE ON transcription_segments BEGIN
    INSERT INTO segments_fts(segments_fts, rowid, text) VALUES ('delete', old.id, old.text);
  END;
  CREATE TRIGGER segments_fts_au AFTER UPDATE OF text ON transcription_segments BEGIN
    INSERT INTO segments_fts(segments_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
  END;
`;

/**
 * A writable DB with the app schema — in-memory by default, or a file path for the
 * spawned-server tests. `fts: false` gives a DB from before the DATA-002 migration.
 */
export function createDb(
  path = ":memory:",
  { fts = true }: { fts?: boolean } = {},
): Database {
  const db = new Database(path);
  db.run(SCHEMA);
  if (fts) db.run(FTS_SCHEMA);
  return db;
}

/** Two canonical sessions: a dual-stream meeting (id 1, "Me" + one diarized remote speaker) and a solo note still transcribing (id 2). */
export function seedSessions(db: Database): void {
  db.run(
    "INSERT INTO transcription_history (id, file_name, timestamp, title, transcription_text, status) VALUES (1, 'a.wav', 1000, 'Standup', 'Parliamo del budget per il prossimo trimestre', 'done')",
  );
  db.run(
    "INSERT INTO speakers (id, history_id, label) VALUES (10, 1, 'Me'), (11, 1, 'Speaker 1')",
  );
  db.run(
    "INSERT INTO transcription_segments (history_id, speaker_id, start_ms, end_ms, text) VALUES (1, 10, 0, 1000, 'Parliamo del budget'), (1, 11, 1000, 2000, 'per il prossimo trimestre')",
  );
  db.run(
    "INSERT INTO transcription_history (id, file_name, timestamp, title, transcription_text, status) VALUES (2, 'b.wav', 2000, 'Nota', '', 'transcribing')",
  );
}
