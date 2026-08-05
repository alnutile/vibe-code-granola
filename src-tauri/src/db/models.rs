//! Row types shared verbatim with the frontend.
//!
//! `camelCase` on the wire so TypeScript reads naturally; snake_case in Rust.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub is_builtin: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: String,
    pub folder_id: Option<String>,
    pub template_id: Option<String>,
    pub title: String,
    pub prompt: String,
    pub status: String,
    pub audio_dir: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A meeting as the sidebar list needs it: the row plus the two derived values
/// every card shows. Derived here rather than in the UI so the list is one query
/// instead of one-per-meeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingListItem {
    #[serde(flatten)]
    pub meeting: Meeting,
    /// First line of the generated summary. `None` until a write-up exists.
    pub snippet: Option<String>,
    /// Recorded length. `None` while recording, or if the meeting never started.
    pub duration_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub id: String,
    pub meeting_id: String,
    /// `mic` | `system` | `mixed`
    pub source: String,
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub meeting_id: String,
    /// `user` | `ai`
    pub kind: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

/// An image attached to a meeting's notes.
///
/// The bytes live on disk (`path`); only this metadata is stored in the
/// database. `path` never crosses to the frontend or an MCP host — both address
/// an image by its `reference` (`amble://image/<id>`), a stable local URL that
/// survives renames and that `get_image` resolves back to the bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteImage {
    pub id: String,
    pub meeting_id: String,
    /// User-facing label. Defaults to the original filename, editable after.
    pub name: String,
    pub mime: String,
    /// Absolute path to the bytes. Kept off the wire.
    #[serde(skip)]
    pub path: String,
    pub size_bytes: i64,
    /// `amble://image/<id>`. Derived, but sent so the UI and note text never
    /// hard-code the URL scheme.
    pub reference: String,
    pub created_at: String,
}

impl NoteImage {
    /// The stable local URL for an image id.
    pub fn reference_for(id: &str) -> String {
        format!("amble://image/{id}")
    }

    /// Markdown a note can embed to point at this image. Brackets in the name
    /// are neutralised so they can't break the `![alt](url)` syntax.
    pub fn markdown(&self) -> String {
        let alt = self.name.replace(['[', ']'], " ");
        format!("![{alt}]({})", self.reference)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub id: String,
    pub meeting_id: String,
    pub content: String,
    pub at_ms: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub meeting_id: String,
    /// `user` | `assistant`
    pub role: String,
    pub content: String,
    pub created_at: String,
}

/// A reusable prompt attached to meetings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    /// `live` — steers suggestions during the meeting.
    /// `post` — runs once after recording stops.
    pub kind: String,
    /// Where a post-skill's output is meant to go. Empty for none.
    pub target: String,
    pub prompt: String,
    pub enabled: bool,
    pub is_builtin: bool,
    pub created_at: String,
}

/// One execution of a post-skill against a finished meeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRun {
    pub id: String,
    pub meeting_id: String,
    pub skill_id: String,
    /// Copied at run time so the record survives a rename or delete.
    pub skill_name: String,
    pub target: String,
    /// `running` | `done` | `error`
    pub status: String,
    /// One-line outcome for the UI.
    pub message: String,
    /// What the model produced.
    pub output: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub meeting_id: String,
    pub meeting_title: String,
    pub segment_id: String,
    pub text: String,
    pub start_ms: i64,
    pub source: String,
}
