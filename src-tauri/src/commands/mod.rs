//! The Rust surface the UI calls.
//!
//! Commands stay thin — validate, delegate, return. The interesting logic lives
//! in `meeting`, `audio`, `llm`, and `stt`, which keeps it unit-testable without
//! standing up a Tauri app.

use crate::audio::{self, PermissionStatus};
use crate::db::*;
use crate::error::{AppError, Result};
use crate::llm::ModelInfo;
use crate::mcp;
use crate::meeting;
use crate::settings::{Settings, SECRET_KEYS};
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, State};

type St<'a> = State<'a, Arc<AppState>>;

// ------------------------------------------------------------------- settings

/// Settings plus which secrets exist. Key *values* are never sent to the UI —
/// only whether each one is set, which is all the UI needs to render its state.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub settings: Settings,
    pub secrets_set: Vec<String>,
    pub data_dir: String,
}

#[tauri::command]
pub fn settings_get(state: St<'_>) -> Result<SettingsView> {
    let secrets_set = SECRET_KEYS
        .iter()
        .filter(|k| state.secrets.has(k))
        .map(|k| k.to_string())
        .collect();

    Ok(SettingsView {
        settings: state.settings_snapshot(),
        secrets_set,
        data_dir: state.paths.root.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn settings_save(state: St<'_>, settings: Settings) -> Result<()> {
    settings.save(&state.paths.config)?;
    *state.settings.write() = settings;
    Ok(())
}

#[tauri::command]
pub fn secret_set(state: St<'_>, key: String, value: String) -> Result<()> {
    if !SECRET_KEYS.contains(&key.as_str()) {
        return Err(AppError::Other(format!("unknown secret: {key}")));
    }
    state.secrets.set(&key, &value)
}

#[tauri::command]
pub fn secret_clear(state: St<'_>, key: String) -> Result<()> {
    state.secrets.delete(&key)
}

/// Prefill values for the Settings form when switching providers.
#[tauri::command]
pub fn provider_defaults() -> Value {
    serde_json::json!({
        "llm": {
            "openrouter": crate::llm::default_base_url("openrouter"),
            "openai": crate::llm::default_base_url("openai"),
            "ollama": crate::llm::default_base_url("ollama"),
            "lmstudio": crate::llm::default_base_url("lmstudio"),
            "custom": "",
        },
        "stt": {
            "openai": crate::stt::default_base_url("openai"),
            "openai_compatible": crate::stt::default_base_url("openai_compatible"),
            "whisper_cpp": crate::stt::default_base_url("whisper_cpp"),
        }
    })
}

#[tauri::command]
pub async fn llm_test(state: St<'_>) -> Result<String> {
    state.llm()?.test_connection().await
}

#[tauri::command]
pub async fn stt_test(state: St<'_>) -> Result<String> {
    state.stt()?.test_connection().await
}

#[tauri::command]
pub async fn llm_models(state: St<'_>) -> Result<Vec<ModelInfo>> {
    state.llm()?.list_models().await
}

// ---------------------------------------------------------------- permissions

#[tauri::command]
pub fn permissions_get() -> PermissionStatus {
    audio::permission_status()
}

#[tauri::command]
pub fn permissions_open(pane: String) -> Result<()> {
    audio::open_privacy_settings(&pane)
}

// -------------------------------------------------------------------- folders

#[tauri::command]
pub fn folders_list(state: St<'_>) -> Result<Vec<Folder>> {
    state.db.list_folders()
}

#[tauri::command]
pub fn folder_create(state: St<'_>, name: String, parent_id: Option<String>) -> Result<Folder> {
    state.db.create_folder(&name, parent_id.as_deref())
}

#[tauri::command]
pub fn folder_rename(state: St<'_>, id: String, name: String) -> Result<()> {
    state.db.rename_folder(&id, &name)
}

#[tauri::command]
pub fn folder_delete(state: St<'_>, id: String) -> Result<()> {
    state.db.delete_folder(&id)
}

// ------------------------------------------------------------------ templates

#[tauri::command]
pub fn templates_list(state: St<'_>) -> Result<Vec<Template>> {
    state.db.list_templates()
}

#[tauri::command]
pub fn template_create(state: St<'_>, name: String, prompt: String) -> Result<Template> {
    state.db.create_template(&name, &prompt)
}

#[tauri::command]
pub fn template_delete(state: St<'_>, id: String) -> Result<()> {
    state.db.delete_template(&id)
}

// ------------------------------------------------------------------- meetings

#[tauri::command]
pub fn meetings_list(state: St<'_>, folder_id: Option<String>) -> Result<Vec<Meeting>> {
    state.db.list_meetings(folder_id.as_deref())
}

#[tauri::command]
pub fn meeting_get(state: St<'_>, id: String) -> Result<Meeting> {
    state.require_meeting(&id)
}

#[tauri::command]
pub fn meeting_create(
    state: St<'_>,
    title: Option<String>,
    prompt: Option<String>,
    folder_id: Option<String>,
    template_id: Option<String>,
) -> Result<Meeting> {
    state.db.create_meeting(
        title.as_deref().unwrap_or("New meeting"),
        prompt.as_deref().unwrap_or(""),
        folder_id.as_deref(),
        template_id.as_deref(),
    )
}

/// Partial update. A `None` field is left untouched; to *clear* the folder or
/// template, pass `Some(None)` — which is what `serde` produces for an explicit
/// `null` in the payload.
#[tauri::command]
pub fn meeting_update(
    state: St<'_>,
    id: String,
    title: Option<String>,
    prompt: Option<String>,
    #[allow(clippy::option_option)] folder_id: Option<Option<String>>,
    #[allow(clippy::option_option)] template_id: Option<Option<String>>,
) -> Result<()> {
    state.db.update_meeting(
        &id,
        title.as_deref(),
        prompt.as_deref(),
        folder_id.as_ref().map(|f| f.as_deref()),
        template_id.as_ref().map(|t| t.as_deref()),
    )
}

#[tauri::command]
pub fn meeting_delete(state: St<'_>, id: String) -> Result<()> {
    if state.active_meeting_id().as_deref() == Some(id.as_str()) {
        return Err(AppError::Other(
            "Stop the recording before deleting this meeting.".into(),
        ));
    }
    state.db.delete_meeting(&id)
}

// ------------------------------------------------------------------ recording

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingState {
    pub recording: bool,
    pub meeting_id: Option<String>,
}

#[tauri::command]
pub fn recording_state(state: St<'_>) -> RecordingState {
    RecordingState {
        meeting_id: state.active_meeting_id(),
        recording: state.is_recording(),
    }
}

#[tauri::command]
pub fn meeting_start(app: AppHandle, state: St<'_>, id: String) -> Result<()> {
    meeting::start(app, Arc::clone(&state), &id)
}

#[tauri::command]
pub fn meeting_stop(app: AppHandle, state: St<'_>, id: String) -> Result<()> {
    meeting::stop(&app, &state, &id)
}

// ----------------------------------------------------------------- transcript

#[tauri::command]
pub fn segments_list(state: St<'_>, meeting_id: String) -> Result<Vec<Segment>> {
    state.db.list_segments(&meeting_id)
}

#[tauri::command]
pub fn transcript_text(state: St<'_>, meeting_id: String) -> Result<String> {
    state.db.transcript_text(&meeting_id)
}

#[tauri::command]
pub fn search(state: St<'_>, query: String, limit: Option<i64>) -> Result<Vec<SearchHit>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    state.db.search(query.trim(), limit.unwrap_or(30))
}

// ---------------------------------------------------------------------- notes

#[tauri::command]
pub fn notes_list(state: St<'_>, meeting_id: String) -> Result<Vec<Note>> {
    state.db.list_notes(&meeting_id)
}

/// Save what the user typed themselves. Kept separate from the generated notes
/// so regenerating never overwrites their own writing.
#[tauri::command]
pub fn note_save(state: St<'_>, meeting_id: String, content: String) -> Result<Note> {
    state.db.upsert_note(&meeting_id, "user", &content)
}

#[tauri::command]
pub async fn notes_generate(app: AppHandle, state: St<'_>, meeting_id: String) -> Result<String> {
    meeting::generate_notes(&app, &state, &meeting_id).await
}

// ---------------------------------------------------------------- suggestions

#[tauri::command]
pub fn suggestions_list(state: St<'_>, meeting_id: String) -> Result<Vec<Suggestion>> {
    state.db.list_suggestions(&meeting_id)
}

#[tauri::command]
pub async fn suggest_now(
    app: AppHandle,
    state: St<'_>,
    meeting_id: String,
) -> Result<Option<String>> {
    let segments = state.db.list_segments(&meeting_id)?;
    meeting::suggest_once(&app, &state, &meeting_id, &segments).await
}

// ----------------------------------------------------------------------- chat

#[tauri::command]
pub fn chat_history(state: St<'_>, meeting_id: String) -> Result<Vec<ChatMessage>> {
    state.db.list_chat_messages(&meeting_id)
}

/// Ask a question about a meeting. The answer streams back over `chat://delta`;
/// the full text is also returned when the stream completes.
#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    state: St<'_>,
    meeting_id: String,
    message: String,
) -> Result<String> {
    if message.trim().is_empty() {
        return Err(AppError::Other("Ask a question first.".into()));
    }
    meeting::chat(&app, &state, &meeting_id, message.trim()).await
}

#[tauri::command]
pub fn chat_clear(state: St<'_>, meeting_id: String) -> Result<()> {
    state.db.clear_chat(&meeting_id)
}

// ------------------------------------------------------------------------ MCP

#[tauri::command]
pub fn mcp_status(state: St<'_>) -> Result<mcp::McpStatus> {
    mcp::status(&state.settings_snapshot().mcp)
}

/// Invoke an MCP tool in-process. This is what the MCP server will call once a
/// transport is attached, and it lets you exercise the tool surface from the
/// Settings screen before that exists.
#[tauri::command]
pub fn mcp_tool_call(state: St<'_>, name: String, args: Option<Value>) -> Result<Value> {
    mcp::tools::dispatch(
        &state.db,
        &name,
        &args.unwrap_or_else(|| serde_json::json!({})),
    )
}
