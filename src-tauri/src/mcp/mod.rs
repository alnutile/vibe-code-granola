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

pub mod server;
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

/// A tool plus whether hosts are currently allowed to call it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolToggle {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

/// Every tool, with the deny-list applied.
pub fn tool_toggles(settings: &McpSettings) -> Vec<ToolToggle> {
    tools::definitions()
        .into_iter()
        .map(|t| ToolToggle {
            enabled: !settings.disabled_tools.contains(&t.name),
            name: t.name,
            description: t.description,
        })
        .collect()
}

pub fn status(settings: &McpSettings) -> Result<McpStatus> {
    Ok(McpStatus {
        server_enabled: settings.server_enabled,
        // Flips to true once a transport is attached.
        server_running: false,
        server_port: settings.server_port,
        exposed_tools: tools::definitions(),
        clients: settings
            .connections
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
