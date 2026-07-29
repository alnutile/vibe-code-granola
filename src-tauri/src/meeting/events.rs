//! Events pushed from Rust to the UI.
//!
//! Recording is a stream of things happening over time, so the frontend
//! subscribes rather than polls. Event names are declared once here and mirrored
//! in `src/lib/events.ts` — keep the two in step.

use crate::db::{Segment, Suggestion};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const EVENT_STATUS: &str = "meeting://status";
pub const EVENT_SEGMENT: &str = "meeting://segment";
pub const EVENT_LEVELS: &str = "meeting://levels";
pub const EVENT_SUGGESTION: &str = "meeting://suggestion";
pub const EVENT_NOTES: &str = "meeting://notes";
pub const EVENT_UPDATED: &str = "meeting://updated";
pub const EVENT_CHAT_DELTA: &str = "chat://delta";
pub const EVENT_CHAT_DONE: &str = "chat://done";
pub const EVENT_RUNS: &str = "meeting://runs";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEvent {
    pub meeting_id: String,
    pub status: String,
    /// Optional detail for the UI: "Writing up your notes…", an error, etc.
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelsEvent {
    pub meeting_id: String,
    pub mic: f32,
    pub system: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesEvent {
    pub meeting_id: String,
    pub notes: crate::notes::RenderedNotes,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatDeltaEvent {
    pub meeting_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatDoneEvent {
    pub meeting_id: String,
    pub content: String,
}

/// A failed emit means the window is gone. That is not an error worth
/// propagating into the recording loop, so every helper here swallows it.
fn emit<P: Serialize + Clone>(app: &AppHandle, event: &str, payload: P) {
    if let Err(e) = app.emit(event, payload) {
        tracing::debug!(event, error = %e, "event not delivered");
    }
}

pub fn emit_status(app: &AppHandle, meeting_id: &str, status: &str, message: Option<&str>) {
    emit(
        app,
        EVENT_STATUS,
        StatusEvent {
            meeting_id: meeting_id.to_string(),
            status: status.to_string(),
            message: message.map(str::to_string),
        },
    );
}

pub fn emit_segment(app: &AppHandle, segment: &Segment) {
    emit(app, EVENT_SEGMENT, segment.clone());
}

pub fn emit_levels(app: &AppHandle, meeting_id: &str, mic: f32, system: f32) {
    emit(
        app,
        EVENT_LEVELS,
        LevelsEvent {
            meeting_id: meeting_id.to_string(),
            mic,
            system,
        },
    );
}

pub fn emit_suggestion(app: &AppHandle, suggestion: &Suggestion) {
    emit(app, EVENT_SUGGESTION, suggestion.clone());
}

pub fn emit_notes(app: &AppHandle, meeting_id: &str, notes: &crate::notes::RenderedNotes) {
    emit(
        app,
        EVENT_NOTES,
        NotesEvent {
            meeting_id: meeting_id.to_string(),
            notes: notes.clone(),
        },
    );
}

pub fn emit_meeting_updated(app: &AppHandle, meeting_id: &str) {
    emit(app, EVENT_UPDATED, meeting_id.to_string());
}

pub fn emit_chat_delta(app: &AppHandle, meeting_id: &str, delta: &str) {
    emit(
        app,
        EVENT_CHAT_DELTA,
        ChatDeltaEvent {
            meeting_id: meeting_id.to_string(),
            delta: delta.to_string(),
        },
    );
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunsEvent {
    pub meeting_id: String,
    pub runs: Vec<crate::db::SkillRun>,
}

/// The whole list each time, not a delta — there are a handful of runs per
/// meeting, and replacing wholesale means the UI can't drift out of order.
pub fn emit_skill_runs(app: &AppHandle, meeting_id: &str, runs: &[crate::db::SkillRun]) {
    emit(
        app,
        EVENT_RUNS,
        RunsEvent {
            meeting_id: meeting_id.to_string(),
            runs: runs.to_vec(),
        },
    );
}

pub fn emit_chat_done(app: &AppHandle, meeting_id: &str, content: &str) {
    emit(
        app,
        EVENT_CHAT_DONE,
        ChatDoneEvent {
            meeting_id: meeting_id.to_string(),
            content: content.to_string(),
        },
    );
}
