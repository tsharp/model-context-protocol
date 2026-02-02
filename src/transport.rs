//! McpTransport - Abstract transport interface for MCP servers.
//!
//! This module defines the core transport trait that all MCP communication
//! methods must implement, enabling uniform handling of stdio, HTTP, and
//! other transport types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

use crate::protocol::McpToolDefinition;

/// Abstract transport interface for MCP server communication.
///
/// All MCP transports (stdio, HTTP, SSE) implement this trait to provide
/// a uniform interface for tool discovery, execution, and shutdown.
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Get the list of available tools from the server.
    async fn list_tools(&self) -> Result<Vec<McpToolDefinition>, McpTransportError>;

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
}

impl fmt::Display for TransportTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportTypeId::Stdio => write!(f, "stdio"),
            TransportTypeId::Http => write!(f, "http"),
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

    #[error("Server '{0}' is restarting")]
    ServerRestarting(String),
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

/// Restart policy for server connections.
///
/// Controls how the hub handles server disconnections and failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartPolicy {
    /// Whether to automatically restart on failure
    pub enabled: bool,
    /// Maximum number of restart attempts (None = unlimited)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    /// Delay between restart attempts in milliseconds
    pub delay_ms: u64,
    /// Exponential backoff multiplier (1.0 = no backoff)
    pub backoff_multiplier: f64,
    /// Maximum delay between restarts in milliseconds
    pub max_delay_ms: u64,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            max_attempts: None,
            delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        }
    }
}

impl RestartPolicy {
    /// No automatic restarts (default).
    pub fn none() -> Self {
        Self::default()
    }

    /// Always restart with default settings (unlimited retries).
    pub fn always() -> Self {
        Self {
            enabled: true,
            max_attempts: None,
            delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        }
    }

    /// Restart up to N times before giving up.
    pub fn max_retries(attempts: u32) -> Self {
        Self {
            enabled: true,
            max_attempts: Some(attempts),
            delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        }
    }

    /// Set initial delay between restarts (milliseconds).
    pub fn with_delay_ms(mut self, ms: u64) -> Self {
        self.delay_ms = ms;
        self
    }

    /// Set backoff multiplier (e.g., 2.0 doubles delay each attempt).
    pub fn with_backoff(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    /// Set maximum delay cap (milliseconds).
    pub fn with_max_delay_ms(mut self, ms: u64) -> Self {
        self.max_delay_ms = ms;
        self
    }

    /// Calculate delay for a given attempt number (0-indexed).
    pub fn delay_for_attempt(&self, attempt: u32) -> u64 {
        if self.backoff_multiplier > 1.0 {
            let exp_delay = (self.delay_ms as f64) * self.backoff_multiplier.powi(attempt as i32);
            (exp_delay as u64).min(self.max_delay_ms)
        } else {
            self.delay_ms
        }
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

    /// Restart policy for handling server failures
    #[serde(default)]
    pub restart_policy: RestartPolicy,
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
            restart_policy: RestartPolicy::none(),
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
            restart_policy: RestartPolicy::none(),
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

    /// Set a custom restart policy.
    pub fn with_restart(mut self, policy: RestartPolicy) -> Self {
        self.restart_policy = policy;
        self
    }

    /// Enable restart on failure with default settings (unlimited retries).
    pub fn restart_on_failure(self) -> Self {
        self.with_restart(RestartPolicy::always())
    }

    /// Enable restart with a maximum number of attempts.
    pub fn restart_max_attempts(self, attempts: u32) -> Self {
        self.with_restart(RestartPolicy::max_retries(attempts))
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
            protocol_version: crate::MCP_PROTOCOL_VERSION.to_string(),
            capabilities: InitializeCapabilities::default(),
            client_info: ClientInfo::new("mcp-rust", env!("CARGO_PKG_VERSION")),
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

/// Client capabilities for initialization (2025-11-25).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InitializeCapabilities {
    /// Experimental, non-standard capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Value>,

    /// Present if the client supports listing roots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapabilities>,

    /// Present if the client supports sampling from an LLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingCapabilities>,

    /// Present if the client supports elicitation from the server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<ElicitationCapabilities>,

    /// Present if the client supports task-augmented requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<TasksCapabilities>,

    /// Tool-related capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolCapabilities>,
}

/// Roots capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RootsCapabilities {
    /// Whether the client supports notifications for changes to the roots list.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Sampling capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SamplingCapabilities {
    /// Whether the client supports context inclusion via includeContext parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,

    /// Whether the client supports tool use via tools and toolChoice parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Value>,
}

/// Elicitation capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ElicitationCapabilities {
    /// Whether the client supports form-based elicitation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<Value>,

    /// Whether the client supports URL-based elicitation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<Value>,
}

/// Tasks capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TasksCapabilities {
    /// Whether this party supports tasks/list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Value>,

    /// Whether this party supports tasks/cancel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel: Option<Value>,

    /// Specifies which request types can be augmented with tasks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests: Option<Value>,
}

/// Server capabilities returned during initialization (2025-11-25).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerCapabilities {
    /// Experimental, non-standard capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Value>,

    /// Present if the server supports sending log messages to the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<Value>,

    /// Present if the server supports argument autocompletion suggestions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completions: Option<Value>,

    /// Present if the server offers any prompt templates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapabilities>,

    /// Present if the server offers any resources to read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapabilities>,

    /// Present if the server offers any tools to call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ServerToolCapabilities>,

    /// Present if the server supports task-augmented requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<TasksCapabilities>,
}

/// Prompts capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptsCapabilities {
    /// Whether this server supports notifications for changes to the prompt list.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Resources capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourcesCapabilities {
    /// Whether this server supports subscribing to resource updates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,

    /// Whether this server supports notifications for changes to the resource list.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
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

/// Client information for initialization (2025-11-25).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Intended for programmatic or logical use.
    pub name: String,

    /// The version of the client implementation.
    pub version: String,

    /// Intended for UI and end-user contexts — optimized to be human-readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// An optional human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional set of sized icons that can be displayed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<crate::protocol::Icon>>,

    /// An optional URL of the website for this implementation.
    #[serde(rename = "websiteUrl", skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
}

impl ClientInfo {
    /// Create a new ClientInfo with just name and version.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            title: None,
            description: None,
            icons: None,
            website_url: None,
        }
    }
}

/// Server information returned during initialization (2025-11-25).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Intended for programmatic or logical use.
    pub name: String,

    /// The version of the server implementation.
    pub version: String,

    /// Intended for UI and end-user contexts — optimized to be human-readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// An optional human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional set of sized icons that can be displayed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<crate::protocol::Icon>>,

    /// An optional URL of the website for this implementation.
    #[serde(rename = "websiteUrl", skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
}

impl ServerInfo {
    /// Create a new ServerInfo with just name and version.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            title: None,
            description: None,
            icons: None,
            website_url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_type_display() {
        assert_eq!(TransportTypeId::Stdio.to_string(), "stdio");
        assert_eq!(TransportTypeId::Http.to_string(), "http");
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
        assert_eq!(params.protocol_version, "2025-11-25");
        assert_eq!(params.client_info.name, "mcp-rust");
    }
}
