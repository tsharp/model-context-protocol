//! MCP Server with bidirectional async communication.
//!
//! This module provides a transport-agnostic `McpServer` core that communicates
//! via channels. Transport-specific servers (`McpStdioServer`, `McpHttpServer`)
//! wrap this core and handle the actual I/O.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        McpServer (core)                          │
//! │                                                                  │
//! │  ┌──────────────┐     ┌──────────────┐     ┌──────────────────┐ │
//! │  │   Inbound    │     │   Message    │     │    Outbound      │ │
//! │  │   Receiver   │────▶│    Loop      │────▶│    Sender        │ │
//! │  │  (ClientIn)  │     │   (Task)     │     │  (ServerOut)     │ │
//! │  └──────────────┘     └──────────────┘     └──────────────────┘ │
//! │         ▲                    │                     │            │
//! │         │                    │                     ▼            │
//! │         │              ┌─────┴─────┐        ┌──────────────┐   │
//! │         │              │  Pending  │        │   Channels   │   │
//! │         │              │ Requests  │        │   Handle     │   │
//! │         │              │ (oneshot) │        └──────────────┘   │
//! │         │              └───────────┘               │            │
//! └─────────┼──────────────────────────────────────────┼────────────┘
//!           │                                          │
//!           │                                          ▼
//! ┌─────────┴──────────────────────────────────────────────────────┐
//! │                    Transport Server                             │
//! │            (McpStdioServer / McpHttpServer)                     │
//! │                                                                 │
//! │  ┌──────────────┐                         ┌──────────────────┐ │
//! │  │    I/O       │                         │    I/O           │ │
//! │  │   Reader     │                         │   Writer         │ │
//! │  │  (stdin/HTTP)│                         │ (stdout/HTTP)    │ │
//! │  └──────────────┘                         └──────────────────┘ │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use mcp::server::{McpServer, McpServerConfig};
//! use mcp::server::stdio::McpStdioServer;
//!
//! let config = McpServerConfig::builder()
//!     .name("my-server")
//!     .version("1.0.0")
//!     .with_tool(MyTool)
//!     .build();
//!
//! // Run via stdio transport
//! McpStdioServer::run(config).await?;
//! ```

#[cfg(feature = "http-server")]
pub mod http;
#[cfg(feature = "stdio-server")]
pub mod stdio;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};

use crate::protocol::{
    ClientInbound, JsonRpcId, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
    McpCapabilities, McpServerInfo, ServerOutbound, MCP_PROTOCOL_VERSION,
};
use crate::tool::{DynTool, McpTool, ToolCallResult, ToolProvider, ToolRegistry};

/// Server status indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ServerStatus {
    /// Server is stopped and not processing messages.
    Stopped = 0,
    /// Server is running and processing messages.
    Running = 1,
    /// Server encountered a fault and stopped.
    Faulted = 2,
}

impl From<u8> for ServerStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => ServerStatus::Stopped,
            1 => ServerStatus::Running,
            2 => ServerStatus::Faulted,
            _ => ServerStatus::Stopped,
        }
    }
}

/// Channel handles for communicating with the McpServer.
///
/// This struct provides the external interface for sending messages to
/// and receiving messages from the server.
pub struct McpServerChannels {
    /// Send inbound messages (from client) to the server.
    pub inbound_tx: mpsc::Sender<ClientInbound>,
    /// Send outbound messages (to client) directly.
    /// Used for transport-level errors (e.g., parse errors) that bypass the server.
    pub outbound_tx: mpsc::Sender<ServerOutbound>,
    /// Receive outbound messages (to client) from the server.
    pub outbound_rx: mpsc::Receiver<ServerOutbound>,
}

/// Configuration for an MCP server.
pub struct McpServerConfig {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) registry: ToolRegistry,
    pub(crate) capabilities: McpCapabilities,
}

impl McpServerConfig {
    /// Creates a new configuration builder.
    pub fn builder() -> McpServerConfigBuilder {
        McpServerConfigBuilder::new()
    }

    /// Returns the server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the server version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the tool registry for direct tool access.
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }
}

/// Builder for creating MCP server configurations.
#[derive(Default)]
pub struct McpServerConfigBuilder {
    name: String,
    version: String,
    registry: ToolRegistry,
    capabilities: McpCapabilities,
}

impl McpServerConfigBuilder {
    /// Creates a new configuration builder with defaults.
    pub fn new() -> Self {
        Self {
            name: "mcp-server".to_string(),
            version: "0.1.0".to_string(),
            registry: ToolRegistry::new(),
            capabilities: McpCapabilities {
                tools: Some(serde_json::json!({})),
                ..Default::default()
            },
        }
    }

    /// Sets the server name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets the server version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Adds a single tool to the server.
    pub fn with_tool<T: McpTool + 'static>(mut self, tool: T) -> Self {
        self.registry.register(Arc::new(tool));
        self
    }

    /// Adds a dynamic tool reference to the server.
    pub fn with_dyn_tool(mut self, tool: DynTool) -> Self {
        self.registry.register(tool);
        self
    }

    /// Adds multiple tools from a vector.
    pub fn with_tools(mut self, tools: Vec<DynTool>) -> Self {
        for tool in tools {
            self.registry.register(tool);
        }
        self
    }

    /// Adds all tools from a provider.
    pub fn with_tools_from<P: ToolProvider>(mut self, provider: P) -> Self {
        self.registry.register_provider(provider);
        self
    }

    /// Registers all auto-discovered tools.
    pub fn register_tools(mut self) -> Self {
        for tool in crate::tool::all_tools() {
            self.registry.register(tool);
        }
        self
    }

    /// Registers auto-discovered tools filtered by group.
    pub fn register_tools_in_group(mut self, group: &str) -> Self {
        for tool in crate::tool::tools_in_group(group) {
            self.registry.register(tool);
        }
        self
    }

    /// Adds capabilities to the server.
    pub fn with_capabilities(mut self, capabilities: McpCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Enables resource capabilities.
    pub fn with_resources(mut self) -> Self {
        self.capabilities.resources = Some(serde_json::json!({}));
        self
    }

    /// Enables prompt capabilities.
    pub fn with_prompts(mut self) -> Self {
        self.capabilities.prompts = Some(serde_json::json!({}));
        self
    }

    /// Enables elicitation capabilities.
    pub fn with_elicitation(mut self) -> Self {
        self.capabilities.elicitation = Some(serde_json::json!({}));
        self
    }

    /// Enables task capabilities.
    pub fn with_tasks(mut self) -> Self {
        self.capabilities.tasks = Some(serde_json::json!({}));
        self
    }

    /// Enables logging capabilities.
    pub fn with_logging(mut self) -> Self {
        self.capabilities.logging = Some(serde_json::json!({}));
        self
    }

    /// Enables completions capabilities.
    pub fn with_completions(mut self) -> Self {
        self.capabilities.completions = Some(serde_json::json!({}));
        self
    }

    /// Builds the server configuration.
    pub fn build(self) -> McpServerConfig {
        McpServerConfig {
            name: self.name,
            version: self.version,
            registry: self.registry,
            capabilities: self.capabilities,
        }
    }
}

/// Pending request tracker for server-initiated requests.
type PendingRequests = Arc<Mutex<HashMap<JsonRpcId, oneshot::Sender<JsonRpcResponse>>>>;

/// MCP Server with bidirectional async communication.
///
/// The server uses internal channels for message passing and spawns an
/// internal task to handle the message loop. Transport servers wrap this
/// core and bridge the channels to actual I/O.
///
/// # Example
///
/// ```ignore
/// use mcp::server::{McpServer, McpServerConfig};
///
/// let config = McpServerConfig::builder()
///     .name("my-server")
///     .version("1.0.0")
///     .with_tool(MyTool)
///     .build();
///
/// let (server, channels) = McpServer::new(config);
///
/// // Server is now running, use channels to communicate
/// channels.inbound_tx.send(ClientInbound::Request(request)).await?;
/// let outbound = channels.outbound_rx.recv().await;
/// ```
pub struct McpServer {
    name: String,
    version: String,
    registry: Arc<ToolRegistry>,
    capabilities: McpCapabilities,
    status: Arc<AtomicU8>,
    pending_requests: PendingRequests,
    next_request_id: Arc<AtomicU8>,
    outbound_tx: mpsc::Sender<ServerOutbound>,
    fault_reason: Arc<RwLock<Option<String>>>,
    start_time: std::time::Instant,
}

impl McpServer {
    /// Creates a new MCP server and starts the message loop.
    ///
    /// Returns the server instance and channel handles for communication.
    /// The internal message loop task is spawned automatically.
    pub fn new(config: McpServerConfig) -> (Arc<Self>, McpServerChannels) {
        let (inbound_tx, inbound_rx) = mpsc::channel::<ClientInbound>(256);
        let (outbound_tx, outbound_rx) = mpsc::channel::<ServerOutbound>(256);

        let server = Arc::new(Self {
            name: config.name,
            version: config.version,
            registry: Arc::new(config.registry),
            capabilities: config.capabilities,
            status: Arc::new(AtomicU8::new(ServerStatus::Running as u8)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            next_request_id: Arc::new(AtomicU8::new(1)),
            outbound_tx,
            fault_reason: Arc::new(RwLock::new(None)),
            start_time: std::time::Instant::now(),
        });

        // Spawn the message loop
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            server_clone.message_loop(inbound_rx).await;
        });

        let channels = McpServerChannels {
            inbound_tx,
            outbound_tx: server.outbound_tx.clone(),
            outbound_rx,
        };

        (server, channels)
    }

    /// Returns the current server status.
    pub fn status(&self) -> ServerStatus {
        ServerStatus::from(self.status.load(Ordering::SeqCst))
    }

    /// Returns the fault reason if the server is in a faulted state.
    pub async fn fault_reason(&self) -> Option<String> {
        self.fault_reason.read().await.clone()
    }

    /// Stops the server gracefully.
    pub fn stop(&self) {
        self.status.store(ServerStatus::Stopped as u8, Ordering::SeqCst);
    }

    /// Returns the server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the server version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the server uptime.
    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// Returns server info for MCP protocol.
    pub fn server_info(&self) -> McpServerInfo {
        McpServerInfo {
            name: self.name.clone(),
            version: self.version.clone(),
            title: None,
            description: None,
            icons: None,
            website_url: None,
        }
    }

    /// Sends a request to the client and waits for the response.
    ///
    /// This is used for server-to-client requests like sampling or elicitation.
    /// Times out after the specified duration to prevent resource leaks.
    pub async fn send_request(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
        timeout: std::time::Duration,
    ) -> Result<JsonRpcResponse, ServerError> {
        let id = JsonRpcId::Number(
            self.next_request_id.fetch_add(1, Ordering::SeqCst) as i64,
        );

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: id.clone(),
            method: method.into(),
            params,
        };

        // Create oneshot channel for response
        let (response_tx, response_rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id.clone(), response_tx);
        }

        // Send request
        self.outbound_tx
            .send(ServerOutbound::Request(request))
            .await
            .map_err(|_| ServerError::ChannelClosed)?;

        // Wait for response with timeout
        match tokio::time::timeout(timeout, response_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                // Channel closed - clean up pending request
                self.pending_requests.lock().await.remove(&id);
                Err(ServerError::ChannelClosed)
            }
            Err(_) => {
                // Timeout - clean up pending request
                self.pending_requests.lock().await.remove(&id);
                Err(ServerError::ResponseTimeout)
            }
        }
    }

    /// Sends a request with a default 30-second timeout.
    pub async fn send_request_default_timeout(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse, ServerError> {
        self.send_request(method, params, std::time::Duration::from_secs(30)).await
    }

    /// Sends a notification to the client (no response expected).
    pub async fn send_notification(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
    ) -> Result<(), ServerError> {
        let notification = JsonRpcNotification::new(method, params);
        self.outbound_tx
            .send(ServerOutbound::Notification(notification))
            .await
            .map_err(|_| ServerError::ChannelClosed)
    }

    /// Sends a progress notification.
    pub async fn send_progress(
        &self,
        token: impl Into<String>,
        progress: f64,
        message: Option<String>,
    ) -> Result<(), ServerError> {
        let params = serde_json::json!({
            "progressToken": token.into(),
            "progress": progress,
            "message": message
        });
        self.send_notification("notifications/progress", Some(params))
            .await
    }

    /// Sends a log message notification.
    pub async fn send_log(
        &self,
        level: &str,
        message: impl Into<String>,
        logger: Option<&str>,
        data: Option<Value>,
    ) -> Result<(), ServerError> {
        let mut params = serde_json::json!({
            "level": level,
            "message": message.into()
        });
        if let Some(l) = logger {
            params["logger"] = serde_json::json!(l);
        }
        if let Some(d) = data {
            params["data"] = d;
        }
        self.send_notification("notifications/message", Some(params))
            .await
    }

    /// Calls a tool directly by name (for embedded usage).
    pub async fn call_tool(&self, name: &str, args: Value) -> ToolCallResult {
        self.registry.call(name, args).await
    }

    /// Lists all available tools.
    pub fn list_tools(&self) -> Vec<crate::protocol::McpToolDefinition> {
        self.registry.definitions()
    }

    /// Gets the tool registry.
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// The internal message loop that processes inbound messages.
    async fn message_loop(self: Arc<Self>, mut inbound_rx: mpsc::Receiver<ClientInbound>) {
        while self.status() == ServerStatus::Running {
            match inbound_rx.recv().await {
                Some(message) => {
                    if let Err(e) = self.handle_inbound(message).await {
                        // Set fault status
                        self.status.store(ServerStatus::Faulted as u8, Ordering::SeqCst);
                        *self.fault_reason.write().await = Some(e.to_string());
                        break;
                    }
                }
                None => {
                    // Channel closed, stop gracefully
                    self.status.store(ServerStatus::Stopped as u8, Ordering::SeqCst);
                    break;
                }
            }
        }
    }

    /// Handles an inbound message from the client.
    async fn handle_inbound(&self, message: ClientInbound) -> Result<(), ServerError> {
        match message {
            ClientInbound::Request(request) => {
                let response = self.handle_rpc_request(request).await;
                self.outbound_tx
                    .send(ServerOutbound::Response(response))
                    .await
                    .map_err(|_| ServerError::ChannelClosed)?;
            }
            ClientInbound::Response(response) => {
                // Handle response to a server-initiated request
                let mut pending = self.pending_requests.lock().await;
                if let Some(tx) = pending.remove(&response.id) {
                    let _ = tx.send(response);
                }
            }
            ClientInbound::Notification(notification) => {
                // Handle client notifications
                self.handle_notification(notification).await?;
            }
        }
        Ok(())
    }

    /// Handles a JSON-RPC request from the client.
    async fn handle_rpc_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id),
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "ping" => JsonRpcResponse::success(request.id, serde_json::json!({})),
            "health/check" => self.handle_health_check(request.id),
            _ => JsonRpcResponse::error(
                request.id,
                -32601,
                format!("Method not found: {}", request.method),
                None,
            ),
        }
    }

    fn handle_initialize(&self, id: JsonRpcId) -> JsonRpcResponse {
        JsonRpcResponse::success(
            id,
            serde_json::json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "serverInfo": self.server_info(),
                "capabilities": self.capabilities
            }),
        )
    }

    fn handle_tools_list(&self, id: JsonRpcId) -> JsonRpcResponse {
        let tools = self.registry.definitions();
        JsonRpcResponse::success(id, serde_json::json!({ "tools": tools }))
    }

    fn handle_health_check(&self, id: JsonRpcId) -> JsonRpcResponse {
        let uptime_secs = self.start_time.elapsed().as_secs();
        let status = self.status();
        JsonRpcResponse::success(
            id,
            serde_json::json!({
                "status": match status {
                    ServerStatus::Running => "healthy",
                    ServerStatus::Stopped => "stopped",
                    ServerStatus::Faulted => "unhealthy",
                },
                "uptime_seconds": uptime_secs,
                "server_name": self.name,
                "server_version": self.version,
                "tool_count": self.registry.definitions().len()
            }),
        )
    }

    async fn handle_tools_call(&self, id: JsonRpcId, params: Option<Value>) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return JsonRpcResponse::error(id, -32602, "Missing params".to_string(), None);
            }
        };

        let name = match params.get("name").and_then(|n| n.as_str()) {
            Some(n) => n,
            None => {
                return JsonRpcResponse::error(id, -32602, "Missing tool name".to_string(), None);
            }
        };

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        let result = self.registry.call(name, arguments).await;

        match result {
            Ok(content) => JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "content": content,
                    "isError": false
                }),
            ),
            Err(e) => JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "content": [{ "type": "text", "text": e.to_string() }],
                    "isError": true
                }),
            ),
        }
    }

    /// Handles a notification from the client.
    async fn handle_notification(&self, notification: JsonRpcNotification) -> Result<(), ServerError> {
        match notification.method.as_str() {
            "notifications/cancelled" => {
                // Cancellation is acknowledged but not yet implemented.
                // Future versions may track in-flight requests for cancellation.
                eprintln!("[MCP] Received cancellation notification (not yet implemented)");
            }
            "notifications/initialized" => {
                // Client finished initialization - no action needed
            }
            method => {
                // Unknown notification - log for debugging
                eprintln!("[MCP] Unknown notification method: {}", method);
            }
        }
        Ok(())
    }
}

/// Errors that can occur when running an MCP server.
#[derive(Debug)]
pub enum ServerError {
    /// I/O error.
    Io(std::io::Error),
    /// JSON serialization error.
    Serialization(serde_json::Error),
    /// Channel was closed unexpectedly.
    ChannelClosed,
    /// Timeout waiting for response.
    ResponseTimeout,
    /// Transport error.
    Transport(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::Io(e) => write!(f, "I/O error: {}", e),
            ServerError::Serialization(e) => write!(f, "Serialization error: {}", e),
            ServerError::ChannelClosed => write!(f, "Channel closed"),
            ServerError::ResponseTimeout => write!(f, "Response timeout"),
            ServerError::Transport(e) => write!(f, "Transport error: {}", e),
        }
    }
}

impl std::error::Error for ServerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ServerError::Io(e) => Some(e),
            ServerError::Serialization(e) => Some(e),
            ServerError::ChannelClosed => None,
            ServerError::ResponseTimeout => None,
            ServerError::Transport(_) => None,
        }
    }
}

/// Creates a vector of tools from tool types.
#[macro_export]
macro_rules! tools {
    () => {
        Vec::new()
    };
    ($($tool:expr),+ $(,)?) => {
        vec![
            $(std::sync::Arc::new($tool) as $crate::DynTool),+
        ]
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{McpToolDefinition, ToolContent};
    use crate::tool::BoxFuture;

    struct EchoTool;

    impl McpTool for EchoTool {
        fn definition(&self) -> McpToolDefinition {
            McpToolDefinition::new("echo")
                .with_description("Echo the input")
                .with_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    }
                }))
        }

        fn call<'a>(&'a self, args: Value) -> BoxFuture<'a, ToolCallResult> {
            Box::pin(async move {
                let message = args
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("no message");
                Ok(vec![ToolContent::text(message)])
            })
        }
    }

    #[test]
    fn test_config_builder() {
        let config = McpServerConfig::builder()
            .name("test-server")
            .version("1.0.0")
            .with_tool(EchoTool)
            .build();

        assert_eq!(config.name(), "test-server");
        assert_eq!(config.version(), "1.0.0");
        assert_eq!(config.registry.len(), 1);
    }

    #[test]
    fn test_tools_macro() {
        let tools = tools![EchoTool];
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn test_config_with_tools() {
        let config = McpServerConfig::builder()
            .name("test-server")
            .version("1.0.0")
            .with_tools(tools![EchoTool])
            .build();

        assert_eq!(config.registry.len(), 1);
    }

    #[tokio::test]
    async fn test_server_creation() {
        let config = McpServerConfig::builder()
            .name("test-server")
            .version("1.0.0")
            .with_tool(EchoTool)
            .build();

        let (server, _channels) = McpServer::new(config);

        assert_eq!(server.name(), "test-server");
        assert_eq!(server.version(), "1.0.0");
        assert_eq!(server.status(), ServerStatus::Running);
    }

    #[tokio::test]
    async fn test_server_bidirectional() {
        let config = McpServerConfig::builder()
            .name("bidir-test")
            .version("1.0.0")
            .with_tool(EchoTool)
            .build();

        let (server, mut channels) = McpServer::new(config);

        // Send a request through the channel
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: JsonRpcId::Number(1),
            method: "tools/list".to_string(),
            params: None,
        };

        channels
            .inbound_tx
            .send(ClientInbound::Request(request))
            .await
            .unwrap();

        // Receive the response
        let outbound = channels.outbound_rx.recv().await.unwrap();
        match outbound {
            ServerOutbound::Response(response) => {
                assert!(response.is_success());
            }
            _ => panic!("Expected response"),
        }

        // Server should still be running
        assert_eq!(server.status(), ServerStatus::Running);
    }

    #[tokio::test]
    async fn test_server_stop() {
        let config = McpServerConfig::builder()
            .name("stop-test")
            .version("1.0.0")
            .build();

        let (server, _channels) = McpServer::new(config);

        assert_eq!(server.status(), ServerStatus::Running);

        server.stop();

        // Give the message loop time to notice
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert_eq!(server.status(), ServerStatus::Stopped);
    }

    #[tokio::test]
    async fn test_server_tool_call() {
        let config = McpServerConfig::builder()
            .name("tool-test")
            .version("1.0.0")
            .with_tool(EchoTool)
            .build();

        let (_server, mut channels) = McpServer::new(config);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: JsonRpcId::Number(1),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "echo",
                "arguments": { "message": "hello world" }
            })),
        };

        channels
            .inbound_tx
            .send(ClientInbound::Request(request))
            .await
            .unwrap();

        let outbound = channels.outbound_rx.recv().await.unwrap();
        match outbound {
            ServerOutbound::Response(response) => {
                assert!(response.is_success());
                let result = response.result().unwrap();
                assert!(result.to_string().contains("hello world"));
            }
            _ => panic!("Expected response"),
        }
    }
}
