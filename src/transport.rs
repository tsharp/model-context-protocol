//! McpTransport - Abstract transport interface for MCP servers.
//!
//! This module defines the core transport trait that all MCP communication
//! methods must implement, enabling uniform handling of stdio, HTTP, and
//! other transport types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

use crate::protocol::ToolDefinition;

/// Abstract transport interface for MCP server communication.
///
/// All MCP transports (stdio, HTTP, SSE) implement this trait to provide
/// a uniform interface for tool discovery, execution, and shutdown.
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Get the list of available tools from the server.
    async fn list_tools(&self) -> Result<Vec<ToolDefinition>, McpTransportError>;

    /// Execute a tool with the given arguments.
    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpTransportError>;

    /// Perform a clean shutdown of the transport.
    async fn shutdown(&self) -> Result<(), McpTransportError>;

    /// Check if the transport is still connected/alive.
    fn is_alive(&self) -> bool;

    /// Get the transport type identifier.
    fn transport_type(&self) -> TransportTypeId;
}

/// Transport type identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportTypeId {
    /// Standard I/O transport (recommended)
    Stdio,
    /// HTTP/REST transport
    Http,
    /// Server-Sent Events transport
    Sse,
}

impl fmt::Display for TransportTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportTypeId::Stdio => write!(f, "stdio"),
            TransportTypeId::Http => write!(f, "http"),
            TransportTypeId::Sse => write!(f, "sse"),
        }
    }
}

/// MCP transport errors.
#[derive(Debug, thiserror::Error)]
pub enum McpTransportError {
    #[error("Unknown tool: {0}")]
    UnknownTool(String),

    #[error("Server not found: {0}")]
    ServerNotFound(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Not supported: {0}")]
    NotSupported(String),

    #[error("Connection closed")]
    ConnectionClosed,
}

impl From<String> for McpTransportError {
    fn from(s: String) -> Self {
        McpTransportError::TransportError(s)
    }
}

impl From<&str> for McpTransportError {
    fn from(s: &str) -> Self {
        McpTransportError::TransportError(s.to_string())
    }
}

/// Configuration for connecting to an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConnectionConfig {
    /// Server name (identifier)
    pub name: String,

    /// Transport type
    pub transport: TransportTypeId,

    /// Command to run (for stdio)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Command arguments (for stdio)
    #[serde(default)]
    pub args: Vec<String>,

    /// URL endpoint (for HTTP/SSE)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Server-specific configuration
    #[serde(default)]
    pub config: Value,

    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Environment variables to set for stdio transport
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

fn default_timeout() -> u64 {
    30
}

impl McpServerConnectionConfig {
    /// Create a stdio server configuration.
    pub fn stdio(name: impl Into<String>, command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            name: name.into(),
            transport: TransportTypeId::Stdio,
            command: Some(command.into()),
            args,
            url: None,
            config: Value::Object(serde_json::Map::new()),
            timeout_secs: default_timeout(),
            env: std::collections::HashMap::new(),
        }
    }

    /// Create an HTTP server configuration.
    pub fn http(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: TransportTypeId::Http,
            command: None,
            args: Vec::new(),
            url: Some(url.into()),
            config: Value::Object(serde_json::Map::new()),
            timeout_secs: default_timeout(),
            env: std::collections::HashMap::new(),
        }
    }

    /// Set server-specific configuration.
    pub fn with_config(mut self, config: Value) -> Self {
        self.config = config;
        self
    }

    /// Set connection timeout.
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Add an environment variable (for stdio transport).
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

/// Initialize request for MCP protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,

    pub capabilities: InitializeCapabilities,

    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
}

impl InitializeParams {
    pub fn new(config: Option<Value>) -> Self {
        Self {
            protocol_version: "2024-11-05".to_string(),
            capabilities: InitializeCapabilities::default(),
            client_info: ClientInfo {
                name: "mcp-rust".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            config,
        }
    }
}

/// Initialize response from MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,

    pub capabilities: ServerCapabilities,

    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

/// Client capabilities for initialization.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InitializeCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolCapabilities>,
}

/// Server capabilities returned during initialization.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ServerToolCapabilities>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Value>,
}

/// Server tool capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerToolCapabilities {
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Tool-related capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolCapabilities {
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Client information for initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// Server information returned during initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_type_display() {
        assert_eq!(TransportTypeId::Stdio.to_string(), "stdio");
        assert_eq!(TransportTypeId::Http.to_string(), "http");
        assert_eq!(TransportTypeId::Sse.to_string(), "sse");
    }

    #[test]
    fn test_connection_config_stdio() {
        let config =
            McpServerConnectionConfig::stdio("test", "node", vec!["server.js".to_string()])
                .with_timeout(60);

        assert_eq!(config.name, "test");
        assert_eq!(config.transport, TransportTypeId::Stdio);
        assert_eq!(config.command, Some("node".to_string()));
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_connection_config_http() {
        let config = McpServerConnectionConfig::http("api", "http://localhost:8080/mcp");

        assert_eq!(config.name, "api");
        assert_eq!(config.transport, TransportTypeId::Http);
        assert_eq!(config.url, Some("http://localhost:8080/mcp".to_string()));
    }

    #[test]
    fn test_initialize_params() {
        let params = InitializeParams::new(None);
        assert_eq!(params.protocol_version, "2024-11-05");
        assert_eq!(params.client_info.name, "mcp-rust");
    }
}
