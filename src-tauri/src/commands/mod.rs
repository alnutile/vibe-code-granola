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

/// The one-line model summary shown on the rail's status pill.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    /// e.g. "Whisper + Llama · on this Mac"
    pub label: String,
    /// Everything runs on this machine — nothing leaves it.
    pub local: bool,
    /// Something is missing (no key, no model) and recording would fail.
    pub needs_setup: bool,
}

#[tauri::command]
pub fn model_status(state: St<'_>) -> ModelStatus {
    let s = state.settings_snapshot();

    let llm_local = matches!(s.llm.provider.as_str(), "ollama" | "lmstudio");
    let stt_local = matches!(s.stt.provider.as_str(), "openai_compatible" | "whisper_cpp");
    let local = llm_local && stt_local;

    let missing_llm_key = s
        .llm_secret_key()
        .is_some_and(|k| !state.secrets.has(k));
    // Only OpenAI transcription hard-requires a key; local servers usually don't.
    let missing_stt_key =
        s.stt.provider == "openai" && !state.secrets.has(crate::settings::secrets::OPENAI_API_KEY);
    let needs_setup =
        missing_llm_key || missing_stt_key || s.llm.model.trim().is_empty();

    let label = if needs_setup {
        "Finish setup to start recording".to_string()
    } else {
        format!(
            "{} + {} · {}",
            short_model(&s.stt.model),
            short_model(&s.llm.model),
            if local { "on this Mac" } else { "hosted" }
        )
    };

    ModelStatus { label, local, needs_setup }
}

/// `anthropic/claude-sonnet-4.5` → `Claude`, `Systran/faster-whisper-small` →
/// `Whisper`. The pill has room for a recognisable name, not an identifier.
fn short_model(model: &str) -> String {
    let tail = model.rsplit('/').next().unwrap_or(model);
    let head = tail
        .split(['-', ':', '_', '.'])
        .find(|part| !part.is_empty() && part.parse::<f32>().is_err())
        .unwrap_or(tail);

    let mut chars = head.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => model.to_string(),
    }
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

/// The sidebar list. Always returns every meeting — the folder filter and the
/// per-folder counts are both derived from this one payload in the UI, which
/// keeps the counts correct while a filter is active.
#[tauri::command]
pub fn meetings_list(state: St<'_>) -> Result<Vec<MeetingListItem>> {
    state.db.list_meeting_items(None)
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
    let meeting = state.db.create_meeting(
        title.as_deref().unwrap_or("New meeting"),
        prompt.as_deref().unwrap_or(""),
        folder_id.as_deref(),
        template_id.as_deref(),
    )?;
    // Your enabled skills come along by default; drop the ones you don't want
    // from the meeting itself.
    state.db.attach_default_skills(&meeting.id)?;
    Ok(meeting)
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

/// Everything the notes tab needs: the generated write-up (once it exists) and
/// whatever the user typed themselves.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingNotes {
    pub rendered: Option<crate::notes::RenderedNotes>,
    pub user: String,
}

#[tauri::command]
pub fn notes_get(state: St<'_>, meeting_id: String) -> Result<MeetingNotes> {
    Ok(MeetingNotes {
        rendered: meeting::load_notes(&state, &meeting_id)?,
        user: state
            .db
            .get_note(&meeting_id, "user")?
            .map(|n| n.content)
            .unwrap_or_default(),
    })
}

/// Save what the user typed themselves. Kept separate from the generated notes
/// so regenerating never overwrites their own writing.
#[tauri::command]
pub fn note_save(state: St<'_>, meeting_id: String, content: String) -> Result<Note> {
    state.db.upsert_note(&meeting_id, "user", &content)
}

#[tauri::command]
pub async fn notes_generate(
    app: AppHandle,
    state: St<'_>,
    meeting_id: String,
) -> Result<crate::notes::RenderedNotes> {
    meeting::generate_notes(&app, &state, &meeting_id).await
}

#[tauri::command]
pub fn note_toggle_action(
    app: AppHandle,
    state: St<'_>,
    meeting_id: String,
    index: usize,
) -> Result<crate::notes::RenderedNotes> {
    meeting::toggle_action(&app, &state, &meeting_id, index)
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

// --------------------------------------------------------------------- skills

#[tauri::command]
pub fn skills_list(state: St<'_>) -> Result<Vec<Skill>> {
    state.db.list_skills()
}

#[tauri::command]
pub fn skill_create(
    state: St<'_>,
    name: String,
    kind: String,
    target: Option<String>,
    prompt: String,
) -> Result<Skill> {
    if name.trim().is_empty() {
        return Err(AppError::Other("Give the skill a name.".into()));
    }
    if kind != "live" && kind != "post" {
        return Err(AppError::Other(format!("unknown skill kind: {kind}")));
    }
    state.db.create_skill(
        name.trim(),
        &kind,
        target.as_deref().unwrap_or("").trim(),
        prompt.trim(),
    )
}

#[tauri::command]
pub fn skill_update(
    state: St<'_>,
    id: String,
    name: Option<String>,
    kind: Option<String>,
    target: Option<String>,
    prompt: Option<String>,
    enabled: Option<bool>,
) -> Result<()> {
    state.db.update_skill(
        &id,
        name.as_deref(),
        kind.as_deref(),
        target.as_deref(),
        prompt.as_deref(),
        enabled,
    )
}

#[tauri::command]
pub fn skill_delete(state: St<'_>, id: String) -> Result<()> {
    state.db.delete_skill(&id)
}

#[tauri::command]
pub fn meeting_skills(state: St<'_>, meeting_id: String) -> Result<Vec<Skill>> {
    state.db.meeting_skills(&meeting_id)
}

#[tauri::command]
pub fn meeting_attach_skill(state: St<'_>, meeting_id: String, skill_id: String) -> Result<()> {
    state.db.attach_skill(&meeting_id, &skill_id)
}

#[tauri::command]
pub fn meeting_detach_skill(state: St<'_>, meeting_id: String, skill_id: String) -> Result<()> {
    state.db.detach_skill(&meeting_id, &skill_id)
}

#[tauri::command]
pub fn skill_runs(state: St<'_>, meeting_id: String) -> Result<Vec<SkillRun>> {
    state.db.list_runs(&meeting_id)
}

/// Re-run the attached post-skills on demand, without re-recording.
#[tauri::command]
pub async fn skills_run_now(app: AppHandle, state: St<'_>, meeting_id: String) -> Result<()> {
    let state = Arc::clone(&state);
    meeting::run_post_skills(&app, &state, &meeting_id).await;
    Ok(())
}

// ------------------------------------------------------------------------ MCP

#[tauri::command]
pub fn mcp_status(state: St<'_>) -> Result<mcp::McpStatus> {
    mcp::status(&state.settings_snapshot().mcp)
}

/// Everything the "Claude access" settings tab shows.
///
/// The two clients need different things. Claude Code speaks HTTP with custom
/// headers directly. Claude Desktop cannot, so it goes through the `mcp-remote`
/// stdio bridge — and the token lives in `env` rather than inline in `args`
/// because Desktop mangles spaces inside an argument, which would corrupt
/// "Bearer <token>".
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeAccess {
    pub host: String,
    pub port: u16,
    pub token: String,
    /// For `claude_desktop_config.json`.
    pub desktop_config: String,
    pub cli_command: String,
    /// Whether the endpoint is accepting connections right now.
    pub listening: bool,
    /// Whether the user has switched the server on. It can be enabled but not
    /// listening if the port was taken at startup.
    pub enabled: bool,
    pub tools: Vec<mcp::ToolToggle>,
}

#[tauri::command]
pub fn mcp_claude_access(state: St<'_>) -> Result<ClaudeAccess> {
    let s = state.settings_snapshot();
    let (host, port, token) = ("127.0.0.1", s.mcp.server_port, s.mcp.server_token.clone());
    let url = format!("http://{host}:{port}/mcp");

    Ok(ClaudeAccess {
        listening: state.mcp_listening(),
        enabled: s.mcp.server_enabled,
        desktop_config: desktop_config_json(&url, &token),
        cli_command: format!(
            "claude mcp add --transport http amble {url} --header \"Authorization: Bearer {token}\""
        ),
        tools: mcp::tool_toggles(&s.mcp),
        host: host.to_string(),
        port,
        token,
    })
}

/// Turn the local MCP endpoint on or off, and remember the choice.
#[tauri::command]
pub async fn mcp_server_set_enabled(state: St<'_>, enabled: bool) -> Result<bool> {
    let state = Arc::clone(&state);

    let mut s = state.settings_snapshot();
    s.mcp.server_enabled = enabled;
    if s.mcp.server_token.is_empty() {
        s.mcp.server_token = crate::settings::new_server_token();
    }
    let port = s.mcp.server_port;
    s.save(&state.paths.config)?;
    *state.settings.write() = s;

    if enabled {
        // A bind failure is the user's problem to fix (port in use), so it
        // propagates rather than leaving the toggle lying about its state.
        state.start_mcp(port).await?;
    } else {
        state.stop_mcp().await;
    }
    Ok(state.mcp_listening())
}

/// Rebind after a port change, so the pasted config and the live socket agree.
#[tauri::command]
pub async fn mcp_server_restart(state: St<'_>) -> Result<bool> {
    let state = Arc::clone(&state);
    let s = state.settings_snapshot();
    if s.mcp.server_enabled {
        state.start_mcp(s.mcp.server_port).await?;
    }
    Ok(state.mcp_listening())
}

/// The `claude_desktop_config.json` entry.
///
/// Written out by hand rather than through `json!` for two reasons: serde_json
/// orders keys alphabetically, and this is a block the user reads and pastes, so
/// command-then-args-then-env is worth preserving.
///
/// Two details are load-bearing and easy to "tidy" into breakage:
/// * `Authorization:${AUTH_HEADER}` has **no space** after the colon, and the
///   token lives in `env` — Claude Desktop mangles spaces inside an argument,
///   which would corrupt `Bearer <token>` if inlined here.
/// * `-y` keeps `npx` from prompting on first run, which would hang startup.
fn desktop_config_json(url: &str, token: &str) -> String {
    format!(
        r#"{{
  "mcpServers": {{
    "amble": {{
      "command": "npx",
      "args": [
        "-y",
        "mcp-remote",
        "{url}",
        "--header",
        "Authorization:${{AUTH_HEADER}}"
      ],
      "env": {{
        "AUTH_HEADER": "Bearer {token}"
      }}
    }}
  }}
}}"#
    )
}

#[tauri::command]
pub fn mcp_regenerate_token(state: St<'_>) -> Result<String> {
    let token = crate::settings::new_server_token();
    let mut s = state.settings_snapshot();
    s.mcp.server_token = token.clone();
    s.save(&state.paths.config)?;
    *state.settings.write() = s;
    Ok(token)
}

/// Turn one MCP tool on or off for hosts. Stored as a deny-list so tools added
/// in a later version are exposed by default rather than silently hidden.
#[tauri::command]
pub fn mcp_set_tool_enabled(state: St<'_>, name: String, enabled: bool) -> Result<()> {
    let mut s = state.settings_snapshot();
    s.mcp.disabled_tools.retain(|t| t != &name);
    if !enabled {
        s.mcp.disabled_tools.push(name);
    }
    s.save(&state.paths.config)?;
    *state.settings.write() = s;
    Ok(())
}

#[tauri::command]
pub fn mcp_add_connection(
    state: St<'_>,
    name: String,
    transport: String,
    target: String,
) -> Result<crate::settings::McpConnection> {
    if name.trim().is_empty() || target.trim().is_empty() {
        return Err(AppError::Other("A connection needs a name and a target.".into()));
    }

    let conn = crate::settings::McpConnection {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        enabled: true,
        transport,
        target: target.trim().to_string(),
    };

    let mut s = state.settings_snapshot();
    s.mcp.connections.push(conn.clone());
    s.save(&state.paths.config)?;
    *state.settings.write() = s;
    Ok(conn)
}

#[tauri::command]
pub fn mcp_remove_connection(state: St<'_>, id: String) -> Result<()> {
    let mut s = state.settings_snapshot();
    s.mcp.connections.retain(|c| c.id != id);
    s.save(&state.paths.config)?;
    *state.settings.write() = s;
    Ok(())
}

#[tauri::command]
pub fn mcp_toggle_connection(state: St<'_>, id: String, enabled: bool) -> Result<()> {
    let mut s = state.settings_snapshot();
    if let Some(c) = s.mcp.connections.iter_mut().find(|c| c.id == id) {
        c.enabled = enabled;
    }
    s.save(&state.paths.config)?;
    *state.settings.write() = s;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktop_config_is_valid_json_with_the_bridge_wiring_intact() {
        let text = desktop_config_json("http://127.0.0.1:8787/mcp", "amb_lcl_abc123");
        let parsed: Value = serde_json::from_str(&text).expect("must be valid JSON to paste");
        let entry = &parsed["mcpServers"]["amble"];

        assert_eq!(entry["command"], "npx");
        assert_eq!(
            entry["args"],
            serde_json::json!([
                "-y",
                "mcp-remote",
                "http://127.0.0.1:8787/mcp",
                "--header",
                "Authorization:${AUTH_HEADER}"
            ])
        );
        // The token must reach the bridge through `env`, never inlined into an
        // arg — Claude Desktop mangles spaces inside arguments.
        assert_eq!(entry["env"]["AUTH_HEADER"], "Bearer amb_lcl_abc123");
        assert!(
            !text.contains("Authorization: Bearer"),
            "a space after the colon breaks Claude Desktop"
        );
    }

    #[test]
    fn model_names_shorten_to_something_recognisable() {
        assert_eq!(short_model("anthropic/claude-sonnet-4.5"), "Claude");
        assert_eq!(short_model("Systran/faster-whisper-small"), "Faster");
        assert_eq!(short_model("llama3.1:8b"), "Llama3");
        assert_eq!(short_model("gpt-4o-mini-transcribe"), "Gpt");
    }
}
