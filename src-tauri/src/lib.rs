//! vibecode-granola — a local-first meeting recorder and AI notepad.
//!
//! Records system audio and your microphone, transcribes as you go, suggests
//! things to say against the intent you set for the meeting, and lets you chat
//! with the result. Everything is stored on your machine; the only things that
//! leave it are the API calls to whichever model provider you configure — which
//! can be `localhost`.

pub mod audio;
pub mod commands;
pub mod db;
pub mod error;
pub mod llm;
pub mod mcp;
pub mod meeting;
pub mod notes;
pub mod prompts;
pub mod settings;
pub mod state;
pub mod stt;

use state::AppState;
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // `RUST_LOG=vibecode_granola_lib=debug` for the recording loop's internals.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vibecode_granola_lib=info,warn".into()),
        )
        .init();

    let state = match AppState::new() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            // Without state there is no app — a blank window with silently broken
            // buttons would be worse than an honest failure.
            eprintln!("vibecode-granola could not start: {e}");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::clone(&state))
        .setup(move |_app| {
            // Bring the MCP endpoint back up if the user left it on, so Claude
            // reconnects on its own after a restart.
            if state.settings_snapshot().mcp.server_enabled {
                let state = Arc::clone(&state);
                tauri::async_runtime::spawn(async move {
                    let port = state.settings_snapshot().mcp.server_port;
                    if let Err(e) = state.start_mcp(port).await {
                        // Not fatal: the app is still a meeting recorder without it.
                        tracing::error!(error = %e, "could not start the MCP server");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // settings
            commands::settings_get,
            commands::settings_save,
            commands::model_status,
            commands::secret_set,
            commands::secret_clear,
            commands::provider_defaults,
            commands::llm_test,
            commands::stt_test,
            commands::llm_models,
            // permissions
            commands::permissions_get,
            commands::permissions_open,
            // folders & templates
            commands::folders_list,
            commands::folder_create,
            commands::folder_rename,
            commands::folder_delete,
            commands::templates_list,
            commands::template_create,
            commands::template_delete,
            // meetings
            commands::meetings_list,
            commands::meeting_get,
            commands::meeting_create,
            commands::meeting_update,
            commands::meeting_delete,
            // recording
            commands::recording_state,
            commands::meeting_start,
            commands::meeting_stop,
            // transcript
            commands::segments_list,
            commands::transcript_text,
            commands::search,
            // notes & suggestions
            commands::notes_get,
            commands::note_save,
            commands::notes_generate,
            commands::note_toggle_action,
            commands::suggestions_list,
            commands::suggest_now,
            // chat
            commands::chat_history,
            commands::chat_send,
            commands::chat_clear,
            // skills
            commands::skills_list,
            commands::skill_create,
            commands::skill_update,
            commands::skill_delete,
            commands::meeting_skills,
            commands::meeting_attach_skill,
            commands::meeting_detach_skill,
            commands::skill_runs,
            commands::skills_run_now,
            // mcp
            commands::mcp_status,
            commands::mcp_tool_call,
            commands::mcp_claude_access,
            commands::mcp_server_set_enabled,
            commands::mcp_server_restart,
            commands::mcp_regenerate_token,
            commands::mcp_set_tool_enabled,
            commands::mcp_add_connection,
            commands::mcp_remove_connection,
            commands::mcp_toggle_connection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
