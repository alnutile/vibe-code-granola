-- vibecode-granola local store.
--
-- Everything the app knows lives here, on your machine. No cloud sync, no
-- account. Audio files sit next to this database in the app data directory.

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Folders group meetings, and nest via parent_id (like Granola's folders).
CREATE TABLE IF NOT EXISTS folders (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    parent_id   TEXT REFERENCES folders(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL
);

-- Note templates: the "what should the AI write up" prompt, reusable across
-- meetings. Seeded with a few defaults on first run.
CREATE TABLE IF NOT EXISTS templates (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    prompt      TEXT NOT NULL,
    is_builtin  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

-- A meeting is one recording session plus everything derived from it.
-- status: 'idle' | 'recording' | 'processing' | 'done' | 'error'
CREATE TABLE IF NOT EXISTS meetings (
    id            TEXT PRIMARY KEY,
    folder_id     TEXT REFERENCES folders(id) ON DELETE SET NULL,
    template_id   TEXT REFERENCES templates(id) ON DELETE SET NULL,
    title         TEXT NOT NULL,
    -- The user's intent for this meeting. Steers live suggestions and the
    -- final write-up, and is prepended to chat context.
    prompt        TEXT NOT NULL DEFAULT '',
    status        TEXT NOT NULL DEFAULT 'idle',
    audio_dir     TEXT,
    started_at    TEXT,
    ended_at      TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_meetings_folder  ON meetings(folder_id);
CREATE INDEX IF NOT EXISTS idx_meetings_created ON meetings(created_at DESC);

-- One row per transcribed chunk. `source` keeps mic and system audio apart so
-- the UI can attribute lines to "you" vs "them" without a diarization model.
-- source: 'mic' | 'system' | 'mixed'
CREATE TABLE IF NOT EXISTS transcript_segments (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    source      TEXT NOT NULL,
    text        TEXT NOT NULL,
    start_ms    INTEGER NOT NULL,
    end_ms      INTEGER NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_segments_meeting ON transcript_segments(meeting_id, start_ms);

-- Full-text search over the transcript, kept in sync by triggers below.
CREATE VIRTUAL TABLE IF NOT EXISTS transcript_fts USING fts5(
    text,
    content = 'transcript_segments',
    content_rowid = 'rowid'
);

CREATE TRIGGER IF NOT EXISTS transcript_fts_insert AFTER INSERT ON transcript_segments BEGIN
    INSERT INTO transcript_fts(rowid, text) VALUES (new.rowid, new.text);
END;

CREATE TRIGGER IF NOT EXISTS transcript_fts_delete AFTER DELETE ON transcript_segments BEGIN
    INSERT INTO transcript_fts(transcript_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
END;

CREATE TRIGGER IF NOT EXISTS transcript_fts_update AFTER UPDATE ON transcript_segments BEGIN
    INSERT INTO transcript_fts(transcript_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
    INSERT INTO transcript_fts(rowid, text) VALUES (new.rowid, new.text);
END;

-- Notes attached to a meeting.
-- kind: 'user'    — what you typed yourself during the call
--       'ai'      — the generated write-up
CREATE TABLE IF NOT EXISTS notes (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,
    content     TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_notes_meeting ON notes(meeting_id);

-- Live in-meeting nudges generated against the meeting prompt.
CREATE TABLE IF NOT EXISTS suggestions (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    content     TEXT NOT NULL,
    at_ms       INTEGER NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_suggestions_meeting ON suggestions(meeting_id, at_ms);

-- Chat with a meeting's transcript + notes. role: 'user' | 'assistant'
CREATE TABLE IF NOT EXISTS chat_messages (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chat_meeting ON chat_messages(meeting_id, created_at);
