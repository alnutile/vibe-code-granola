//! SQLite-backed local store.
//!
//! One connection behind a mutex. Meeting workloads are tiny (a few writes per
//! transcribed chunk), so a pool would be ceremony without benefit — and a single
//! writer sidesteps SQLite's write-lock contention entirely.

pub mod models;

use crate::error::Result;
use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

pub use models::*;

const SCHEMA: &str = include_str!("schema.sql");

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.seed_builtin_templates()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.seed_builtin_templates()?;
        Ok(db)
    }

    // ----------------------------------------------------------------- templates

    /// Insert the starter templates once. Keyed on `is_builtin` so a user who
    /// deletes one doesn't get it resurrected on next launch.
    fn seed_builtin_templates(&self) -> Result<()> {
        let conn = self.conn.lock();
        let already: i64 =
            conn.query_row("SELECT COUNT(*) FROM templates WHERE is_builtin = 1", [], |r| {
                r.get(0)
            })?;
        if already > 0 {
            return Ok(());
        }

        for (name, prompt) in crate::prompts::BUILTIN_TEMPLATES {
            conn.execute(
                "INSERT INTO templates (id, name, prompt, is_builtin, created_at)
                 VALUES (?1, ?2, ?3, 1, ?4)",
                params![new_id(), name, prompt, now()],
            )?;
        }
        Ok(())
    }

    pub fn list_templates(&self) -> Result<Vec<Template>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, prompt, is_builtin, created_at FROM templates ORDER BY is_builtin DESC, name",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Template {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    prompt: r.get(2)?,
                    is_builtin: r.get::<_, i64>(3)? != 0,
                    created_at: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn create_template(&self, name: &str, prompt: &str) -> Result<Template> {
        let t = Template {
            id: new_id(),
            name: name.to_string(),
            prompt: prompt.to_string(),
            is_builtin: false,
            created_at: now(),
        };
        self.conn.lock().execute(
            "INSERT INTO templates (id, name, prompt, is_builtin, created_at) VALUES (?1, ?2, ?3, 0, ?4)",
            params![t.id, t.name, t.prompt, t.created_at],
        )?;
        Ok(t)
    }

    pub fn delete_template(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .execute("DELETE FROM templates WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_template(&self, id: &str) -> Result<Option<Template>> {
        let conn = self.conn.lock();
        let t = conn
            .query_row(
                "SELECT id, name, prompt, is_builtin, created_at FROM templates WHERE id = ?1",
                params![id],
                |r| {
                    Ok(Template {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        prompt: r.get(2)?,
                        is_builtin: r.get::<_, i64>(3)? != 0,
                        created_at: r.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(t)
    }

    // ------------------------------------------------------------------- folders

    pub fn create_folder(&self, name: &str, parent_id: Option<&str>) -> Result<Folder> {
        let f = Folder {
            id: new_id(),
            name: name.to_string(),
            parent_id: parent_id.map(str::to_string),
            created_at: now(),
        };
        self.conn.lock().execute(
            "INSERT INTO folders (id, name, parent_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![f.id, f.name, f.parent_id, f.created_at],
        )?;
        Ok(f)
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT id, name, parent_id, created_at FROM folders ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Folder {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    parent_id: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn rename_folder(&self, id: &str, name: &str) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE folders SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    pub fn delete_folder(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .execute("DELETE FROM folders WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ------------------------------------------------------------------ meetings

    pub fn create_meeting(
        &self,
        title: &str,
        prompt: &str,
        folder_id: Option<&str>,
        template_id: Option<&str>,
    ) -> Result<Meeting> {
        let ts = now();
        let m = Meeting {
            id: new_id(),
            folder_id: folder_id.map(str::to_string),
            template_id: template_id.map(str::to_string),
            title: title.to_string(),
            prompt: prompt.to_string(),
            status: "idle".into(),
            audio_dir: None,
            started_at: None,
            ended_at: None,
            created_at: ts.clone(),
            updated_at: ts,
        };
        self.conn.lock().execute(
            "INSERT INTO meetings
               (id, folder_id, template_id, title, prompt, status, audio_dir, started_at, ended_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                m.id, m.folder_id, m.template_id, m.title, m.prompt, m.status,
                m.audio_dir, m.started_at, m.ended_at, m.created_at, m.updated_at
            ],
        )?;
        Ok(m)
    }

    pub fn get_meeting(&self, id: &str) -> Result<Option<Meeting>> {
        let conn = self.conn.lock();
        let m = conn
            .query_row(
                "SELECT id, folder_id, template_id, title, prompt, status, audio_dir,
                        started_at, ended_at, created_at, updated_at
                 FROM meetings WHERE id = ?1",
                params![id],
                Self::map_meeting,
            )
            .optional()?;
        Ok(m)
    }

    pub fn list_meetings(&self, folder_id: Option<&str>) -> Result<Vec<Meeting>> {
        let conn = self.conn.lock();
        // Two prepared statements rather than a dynamic WHERE, so both stay
        // parameterized and cacheable.
        let mut stmt;
        let rows = match folder_id {
            Some(fid) => {
                stmt = conn.prepare(
                    "SELECT id, folder_id, template_id, title, prompt, status, audio_dir,
                            started_at, ended_at, created_at, updated_at
                     FROM meetings WHERE folder_id = ?1 ORDER BY created_at DESC",
                )?;
                stmt.query_map(params![fid], Self::map_meeting)?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            }
            None => {
                stmt = conn.prepare(
                    "SELECT id, folder_id, template_id, title, prompt, status, audio_dir,
                            started_at, ended_at, created_at, updated_at
                     FROM meetings ORDER BY created_at DESC",
                )?;
                stmt.query_map([], Self::map_meeting)?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        Ok(rows)
    }

    fn map_meeting(r: &rusqlite::Row<'_>) -> rusqlite::Result<Meeting> {
        Ok(Meeting {
            id: r.get(0)?,
            folder_id: r.get(1)?,
            template_id: r.get(2)?,
            title: r.get(3)?,
            prompt: r.get(4)?,
            status: r.get(5)?,
            audio_dir: r.get(6)?,
            started_at: r.get(7)?,
            ended_at: r.get(8)?,
            created_at: r.get(9)?,
            updated_at: r.get(10)?,
        })
    }

    pub fn update_meeting_status(&self, id: &str, status: &str) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE meetings SET status = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, status, now()],
        )?;
        Ok(())
    }

    pub fn mark_meeting_started(&self, id: &str, audio_dir: &str) -> Result<()> {
        let ts = now();
        self.conn.lock().execute(
            "UPDATE meetings SET status = 'recording', started_at = ?2, audio_dir = ?3, updated_at = ?2
             WHERE id = ?1",
            params![id, ts, audio_dir],
        )?;
        Ok(())
    }

    pub fn mark_meeting_ended(&self, id: &str, status: &str) -> Result<()> {
        let ts = now();
        self.conn.lock().execute(
            "UPDATE meetings SET status = ?2, ended_at = ?3, updated_at = ?3 WHERE id = ?1",
            params![id, status, ts],
        )?;
        Ok(())
    }

    /// Partial update — every field is optional so the frontend can PATCH one
    /// thing (a title rename) without round-tripping the whole record.
    pub fn update_meeting(
        &self,
        id: &str,
        title: Option<&str>,
        prompt: Option<&str>,
        folder_id: Option<Option<&str>>,
        template_id: Option<Option<&str>>,
    ) -> Result<()> {
        let conn = self.conn.lock();
        if let Some(t) = title {
            conn.execute("UPDATE meetings SET title = ?2 WHERE id = ?1", params![id, t])?;
        }
        if let Some(p) = prompt {
            conn.execute("UPDATE meetings SET prompt = ?2 WHERE id = ?1", params![id, p])?;
        }
        if let Some(f) = folder_id {
            conn.execute(
                "UPDATE meetings SET folder_id = ?2 WHERE id = ?1",
                params![id, f],
            )?;
        }
        if let Some(t) = template_id {
            conn.execute(
                "UPDATE meetings SET template_id = ?2 WHERE id = ?1",
                params![id, t],
            )?;
        }
        conn.execute(
            "UPDATE meetings SET updated_at = ?2 WHERE id = ?1",
            params![id, now()],
        )?;
        Ok(())
    }

    pub fn delete_meeting(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --------------------------------------------------------------- transcripts

    pub fn add_segment(
        &self,
        meeting_id: &str,
        source: &str,
        text: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Segment> {
        let s = Segment {
            id: new_id(),
            meeting_id: meeting_id.to_string(),
            source: source.to_string(),
            text: text.to_string(),
            start_ms,
            end_ms,
            created_at: now(),
        };
        self.conn.lock().execute(
            "INSERT INTO transcript_segments (id, meeting_id, source, text, start_ms, end_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![s.id, s.meeting_id, s.source, s.text, s.start_ms, s.end_ms, s.created_at],
        )?;
        Ok(s)
    }

    pub fn list_segments(&self, meeting_id: &str) -> Result<Vec<Segment>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, meeting_id, source, text, start_ms, end_ms, created_at
             FROM transcript_segments WHERE meeting_id = ?1 ORDER BY start_ms, created_at",
        )?;
        let rows = stmt
            .query_map(params![meeting_id], |r| {
                Ok(Segment {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    source: r.get(2)?,
                    text: r.get(3)?,
                    start_ms: r.get(4)?,
                    end_ms: r.get(5)?,
                    created_at: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Transcript rendered for a model prompt: `[mm:ss] Speaker: text`.
    pub fn transcript_text(&self, meeting_id: &str) -> Result<String> {
        let segments = self.list_segments(meeting_id)?;
        Ok(render_transcript(&segments))
    }

    /// Full-text search across every meeting. Returns the matching segment plus
    /// its meeting title so results are navigable.
    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<SearchHit>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT m.id, m.title, s.id, s.text, s.start_ms, s.source
             FROM transcript_fts f
             JOIN transcript_segments s ON s.rowid = f.rowid
             JOIN meetings m ON m.id = s.meeting_id
             WHERE transcript_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![query, limit], |r| {
                Ok(SearchHit {
                    meeting_id: r.get(0)?,
                    meeting_title: r.get(1)?,
                    segment_id: r.get(2)?,
                    text: r.get(3)?,
                    start_ms: r.get(4)?,
                    source: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // --------------------------------------------------------------------- notes

    /// Notes are upserted per (meeting, kind): one user note and one AI write-up
    /// per meeting, each edited in place.
    pub fn upsert_note(&self, meeting_id: &str, kind: &str, content: &str) -> Result<Note> {
        let conn = self.conn.lock();
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM notes WHERE meeting_id = ?1 AND kind = ?2",
                params![meeting_id, kind],
                |r| r.get(0),
            )
            .optional()?;

        let ts = now();
        match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE notes SET content = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, content, ts],
                )?;
                Ok(Note {
                    id,
                    meeting_id: meeting_id.into(),
                    kind: kind.into(),
                    content: content.into(),
                    created_at: ts.clone(),
                    updated_at: ts,
                })
            }
            None => {
                let n = Note {
                    id: new_id(),
                    meeting_id: meeting_id.into(),
                    kind: kind.into(),
                    content: content.into(),
                    created_at: ts.clone(),
                    updated_at: ts,
                };
                conn.execute(
                    "INSERT INTO notes (id, meeting_id, kind, content, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![n.id, n.meeting_id, n.kind, n.content, n.created_at, n.updated_at],
                )?;
                Ok(n)
            }
        }
    }

    pub fn list_notes(&self, meeting_id: &str) -> Result<Vec<Note>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, meeting_id, kind, content, created_at, updated_at
             FROM notes WHERE meeting_id = ?1 ORDER BY kind",
        )?;
        let rows = stmt
            .query_map(params![meeting_id], |r| {
                Ok(Note {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    kind: r.get(2)?,
                    content: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_note(&self, meeting_id: &str, kind: &str) -> Result<Option<Note>> {
        Ok(self
            .list_notes(meeting_id)?
            .into_iter()
            .find(|n| n.kind == kind))
    }

    // --------------------------------------------------------------- suggestions

    pub fn add_suggestion(&self, meeting_id: &str, content: &str, at_ms: i64) -> Result<Suggestion> {
        let s = Suggestion {
            id: new_id(),
            meeting_id: meeting_id.into(),
            content: content.into(),
            at_ms,
            created_at: now(),
        };
        self.conn.lock().execute(
            "INSERT INTO suggestions (id, meeting_id, content, at_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![s.id, s.meeting_id, s.content, s.at_ms, s.created_at],
        )?;
        Ok(s)
    }

    pub fn list_suggestions(&self, meeting_id: &str) -> Result<Vec<Suggestion>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, meeting_id, content, at_ms, created_at
             FROM suggestions WHERE meeting_id = ?1 ORDER BY at_ms",
        )?;
        let rows = stmt
            .query_map(params![meeting_id], |r| {
                Ok(Suggestion {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    content: r.get(2)?,
                    at_ms: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ---------------------------------------------------------------------- chat

    pub fn add_chat_message(&self, meeting_id: &str, role: &str, content: &str) -> Result<ChatMessage> {
        let m = ChatMessage {
            id: new_id(),
            meeting_id: meeting_id.into(),
            role: role.into(),
            content: content.into(),
            created_at: now(),
        };
        self.conn.lock().execute(
            "INSERT INTO chat_messages (id, meeting_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![m.id, m.meeting_id, m.role, m.content, m.created_at],
        )?;
        Ok(m)
    }

    pub fn list_chat_messages(&self, meeting_id: &str) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, meeting_id, role, content, created_at
             FROM chat_messages WHERE meeting_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt
            .query_map(params![meeting_id], |r| {
                Ok(ChatMessage {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    role: r.get(2)?,
                    content: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn clear_chat(&self, meeting_id: &str) -> Result<()> {
        self.conn.lock().execute(
            "DELETE FROM chat_messages WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        Ok(())
    }
}

/// `[mm:ss] Speaker: text`, one line per segment.
///
/// Speaker labels come from the capture source rather than a diarization model —
/// mic is you, system audio is everyone else on the call.
pub fn render_transcript(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|s| {
            let secs = s.start_ms / 1000;
            let speaker = match s.source.as_str() {
                "mic" => "You",
                "system" => "Them",
                _ => "Speaker",
            };
            format!("[{:02}:{:02}] {}: {}", secs / 60, secs % 60, speaker, s.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_round_trips_with_segments_and_notes() {
        let db = Db::open_in_memory().unwrap();
        let m = db.create_meeting("Standup", "Track blockers", None, None).unwrap();

        db.add_segment(&m.id, "mic", "morning all", 0, 2000).unwrap();
        db.add_segment(&m.id, "system", "hey there", 2000, 4000).unwrap();

        let segments = db.list_segments(&m.id).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(
            db.transcript_text(&m.id).unwrap(),
            "[00:00] You: morning all\n[00:02] Them: hey there"
        );

        db.upsert_note(&m.id, "ai", "first").unwrap();
        db.upsert_note(&m.id, "ai", "second").unwrap();
        let notes = db.list_notes(&m.id).unwrap();
        assert_eq!(notes.len(), 1, "upsert should replace, not append");
        assert_eq!(notes[0].content, "second");
    }

    #[test]
    fn deleting_a_meeting_cascades_to_its_children() {
        let db = Db::open_in_memory().unwrap();
        let m = db.create_meeting("Temp", "", None, None).unwrap();
        db.add_segment(&m.id, "mic", "hello", 0, 1000).unwrap();
        db.add_chat_message(&m.id, "user", "what happened?").unwrap();

        db.delete_meeting(&m.id).unwrap();

        assert!(db.list_segments(&m.id).unwrap().is_empty());
        assert!(db.list_chat_messages(&m.id).unwrap().is_empty());
    }

    #[test]
    fn full_text_search_finds_segments_across_meetings() {
        let db = Db::open_in_memory().unwrap();
        let a = db.create_meeting("Budget review", "", None, None).unwrap();
        let b = db.create_meeting("Design sync", "", None, None).unwrap();
        db.add_segment(&a.id, "mic", "we need to cut the hosting spend", 0, 1000)
            .unwrap();
        db.add_segment(&b.id, "mic", "the sidebar needs more contrast", 0, 1000)
            .unwrap();

        let hits = db.search("hosting", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].meeting_title, "Budget review");
    }
}
