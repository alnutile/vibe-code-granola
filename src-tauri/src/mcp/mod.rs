//! Model Context Protocol, both directions.
//!
//! The plan is for this app to be an MCP **server** (so Claude Desktop can read
//! your meetings) and an MCP **client** (so meeting chat can call out to your
//! other tools). Neither transport is wired up yet.
//!
//! What *is* done is the part that carries the design risk: the tool surface in
//! [`tools`] is fully implemented against the database and unit-tested. Adding a
//! transport means adapting [`tools::dispatch`] to it — the tools themselves need
//! no further work, and can be exercised from the app today.

pub mod tools;

use crate::error::Result;
use crate::settings::McpSettings;

/// Current state of the MCP surface, for the Settings screen.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub server_enabled: bool,
    pub server_running: bool,
    pub server_port: u16,
    /// Tools this app exposes to MCP hosts.
    pub exposed_tools: Vec<tools::ToolDef>,
    /// Configured outbound connections and whether each is currently connected.
    pub clients: Vec<ClientStatus>,
    /// Plain-language note about what is and isn't live yet.
    pub note: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    pub name: String,
    pub enabled: bool,
    pub transport: String,
    pub connected: bool,
}

pub fn status(settings: &McpSettings) -> Result<McpStatus> {
    Ok(McpStatus {
        server_enabled: settings.server_enabled,
        // Flips to true once a transport is attached.
        server_running: false,
        server_port: settings.server_port,
        exposed_tools: tools::definitions(),
        clients: settings
            .clients
            .iter()
            .map(|c| ClientStatus {
                name: c.name.clone(),
                enabled: c.enabled,
                transport: c.transport.clone(),
                connected: false,
            })
            .collect(),
        note: "The tool surface below is implemented and callable in-process. \
               Connecting it to a stdio/HTTP transport is the remaining step."
            .into(),
    })
}
