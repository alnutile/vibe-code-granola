//! The local MCP server: Streamable HTTP transport over loopback.
//!
//! This is what lets Claude Desktop and Claude Code read your meetings. It speaks
//! JSON-RPC 2.0 on a single `POST /mcp` endpoint, which is the whole of MCP's
//! Streamable HTTP transport for a server that never initiates messages itself.
//!
//! Deliberately narrow:
//!
//! * **Loopback only.** Bound to `127.0.0.1`, never `0.0.0.0`. Nothing reaches it
//!   from the network even if the port is open.
//! * **Bearer token required.** Any loopback process could otherwise read every
//!   meeting you have ever recorded, so an unauthenticated request is refused
//!   before it reaches a tool.
//! * **Read-only.** Every exposed tool only reads. An MCP host cannot delete a
//!   meeting or start a recording through this.

use crate::error::{AppError, Result};
use crate::mcp::tools;
use crate::state::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::oneshot;

/// The MCP revision this server implements. Echoed back on `initialize` when the
/// client asks for it, so a client pinned to an older revision still gets an
/// answer it understands rather than a hard failure.
const PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

pub struct ServerHandle {
    pub port: u16,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl ServerHandle {
    /// Ask the server to stop and wait for the socket to close, so a restart on
    /// a new port can't race the old listener.
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.task.await;
    }
}

/// Bind the endpoint. Fails if the port is already taken, which is reported to
/// the UI rather than silently retried on another port — a moved port would
/// invalidate the config the user already pasted into Claude.
pub async fn start(state: Arc<AppState>, port: u16) -> Result<ServerHandle> {
    let app = Router::new()
        .route("/mcp", post(handle_rpc).get(method_not_allowed).delete(end_session))
        .route("/health", get(health))
        .with_state(state);

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        AppError::Other(format!(
            "Could not open port {port} for the MCP server: {e}. \
             Pick a different port in Settings → Claude access."
        ))
    })?;

    let (tx, rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let served = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = rx.await;
        });
        if let Err(e) = served.await {
            tracing::error!(error = %e, "MCP server stopped unexpectedly");
        }
    });

    tracing::info!(port, "MCP server listening on loopback");
    Ok(ServerHandle {
        port,
        shutdown: Some(tx),
        task,
    })
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true, "server": "amble" }))
}

/// The transport allows a server to open an SSE stream here for server-initiated
/// messages. This one has none to send.
async fn method_not_allowed() -> impl IntoResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({ "error": "this server does not open server-initiated streams" })),
    )
}

/// Clients may end a session with DELETE. Sessions here are stateless, so this
/// only needs to succeed.
async fn end_session() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    if expected.is_empty() {
        return false; // no token configured — refuse rather than allow everything
    }
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t.trim() == expected)
}

async fn handle_rpc(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    let settings = state.settings_snapshot();

    if !authorized(&headers, &settings.mcp.server_token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "jsonrpc": "2.0",
                "error": { "code": -32001, "message": "missing or invalid bearer token" },
                "id": Value::Null
            })),
        )
            .into_response();
    }

    let Ok(message) = serde_json::from_str::<Value>(&body) else {
        return Json(rpc_error(Value::Null, -32700, "parse error")).into_response();
    };

    // A batch is a JSON array. Notifications inside it produce no reply.
    if let Some(batch) = message.as_array() {
        let replies: Vec<Value> = batch
            .iter()
            .filter_map(|m| dispatch_message(&state, m))
            .collect();
        return if replies.is_empty() {
            StatusCode::ACCEPTED.into_response()
        } else {
            Json(Value::Array(replies)).into_response()
        };
    }

    match dispatch_message(&state, &message) {
        // Notifications and responses get no body, per JSON-RPC.
        None => StatusCode::ACCEPTED.into_response(),
        Some(reply) => Json(reply).into_response(),
    }
}

/// Handle one JSON-RPC message. `None` means "nothing to send back".
fn dispatch_message(state: &Arc<AppState>, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str)?;
    let id = msg.get("id").cloned();

    // No `id` means a notification: acknowledge by staying silent.
    let Some(id) = id else {
        tracing::debug!(method, "mcp notification");
        return None;
    };

    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

    let result = match method {
        "initialize" => Ok(initialize_result(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list(state)),
        "tools/call" => tools_call(state, &params),
        // Declared capabilities are tools only; the rest are answered honestly
        // as empty rather than erroring, which some clients probe for.
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        other => Err(AppError::NotFound(format!("unknown method: {other}"))),
    };

    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err(AppError::NotFound(m)) => rpc_error(id, -32601, &m),
        Err(e) => rpc_error(id, -32603, &e.to_string()),
    })
}

fn initialize_result(params: &Value) -> Value {
    // Answer in the client's revision when we know it, so a client pinned to an
    // older one still gets something it can parse.
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = match requested {
        Some(v) if SUPPORTED_VERSIONS.contains(&v) => v,
        _ => PROTOCOL_VERSION,
    };

    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "amble", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Amble stores this user's meeting recordings, transcripts and notes, \
                         all local to their Mac. Search before assuming a meeting is absent; \
                         transcripts label the user as \"You\" and everyone else as \"Them\"."
    })
}

fn tools_list(state: &Arc<AppState>) -> Value {
    let disabled = state.settings_snapshot().mcp.disabled_tools;
    let tools: Vec<Value> = tools::definitions()
        .into_iter()
        .filter(|t| !disabled.contains(&t.name))
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
            })
        })
        .collect();
    json!({ "tools": tools })
}

fn tools_call(state: &Arc<AppState>, params: &Value) -> Result<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Other("tools/call needs a tool name".into()))?;

    // A tool switched off in Settings must be unreachable, not merely unlisted.
    if state.settings_snapshot().mcp.disabled_tools.iter().any(|t| t == name) {
        return Err(AppError::NotFound(format!(
            "the tool {name} is turned off in Amble's settings"
        )));
    }

    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

    // A failing tool is reported as a *result* with `isError`, not a JSON-RPC
    // error: the model should see what went wrong and adapt, rather than the
    // host treating it as a transport fault.
    Ok(match tools::dispatch(&state.db, name, &args) {
        Ok(tools::ToolOutput::Json(value)) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value)? }],
            "isError": false
        }),
        // Image (and any future non-text) tools return ready-made content blocks.
        Ok(tools::ToolOutput::Content(items)) => json!({
            "content": items,
            "isError": false
        }),
        Err(e) => json!({
            "content": [{ "type": "text", "text": e.to_string() }],
            "isError": true
        }),
    })
}

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::AUTHORIZATION;

    fn headers_with(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, value.parse().unwrap());
        h
    }

    #[test]
    fn only_a_matching_bearer_token_is_accepted() {
        assert!(authorized(&headers_with("Bearer abc123"), "abc123"));
        assert!(!authorized(&headers_with("Bearer wrong"), "abc123"));
        assert!(!authorized(&headers_with("abc123"), "abc123"), "scheme is required");
        assert!(!authorized(&HeaderMap::new(), "abc123"));
    }

    #[test]
    fn an_unconfigured_token_refuses_everyone() {
        // Otherwise a blank token in config.json would mean "no auth at all".
        assert!(!authorized(&headers_with("Bearer "), ""));
        assert!(!authorized(&HeaderMap::new(), ""));
    }

    #[test]
    fn initialize_echoes_a_supported_client_version() {
        let r = initialize_result(&json!({ "protocolVersion": "2024-11-05" }));
        assert_eq!(r["protocolVersion"], "2024-11-05");
        assert!(r["capabilities"]["tools"].is_object());
        assert_eq!(r["serverInfo"]["name"], "amble");
    }

    #[test]
    fn initialize_falls_back_to_our_version_for_an_unknown_one() {
        let r = initialize_result(&json!({ "protocolVersion": "1999-01-01" }));
        assert_eq!(r["protocolVersion"], PROTOCOL_VERSION);
    }
}
