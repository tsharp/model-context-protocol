//! MCP Server configuration and runtime.
//!
//! This module provides a clean API for creating and running MCP servers:
//!
//! ```ignore
//! use mcp::macros::mcp_tool;
//! use mcp::{McpServerConfig, McpServer};
//!
//! #[mcp_tool(description = "Add two numbers")]
//! fn add(a: f64, b: f64) -> f64 { a + b }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = McpServerConfig::builder()
//!         .name("calculator")
//!         .version("1.0.0")
//!         .with_stdio_transport()
//!         .with_tools(tools![AddTool])
//!         .build();
//!
//!     McpServer::run(config).await?;
//!     Ok(())
//! }
//! ```

use std::io::{self, BufRead, Write};
use std::sync::Arc;

use serde_json::Value;

use crate::protocol::{
    JsonRpcId, JsonRpcRequest, JsonRpcResponse, McpCapabilities, McpServerInfo,
    MCP_PROTOCOL_VERSION,
};
use crate::tool::{DynTool, McpTool, ToolCallResult, ToolProvider, ToolRegistry};

/// Server transport configuration.
#[derive(Debug, Clone)]
pub enum ServerTransport {
    /// No transport configured.
    None,
    /// Standard I/O transport (stdin/stdout).
    Stdio,
    /// HTTP transport with optional SSE support.
    #[cfg(feature = "http-server")]
    Http {
        /// Host to bind to.
        host: String,
        /// Port to bind to.
        port: u16,
    },
}

impl Default for ServerTransport {
    fn default() -> Self {
        Self::None
    }
}

/// Configuration for an MCP server.
///
/// Use the builder pattern to create a configuration, then pass it to `McpServer::run()`.
///
/// # Example
///
/// ```ignore
/// use mcp::{McpServerConfig, McpServer, tools};
///
/// let config = McpServerConfig::builder()
///     .name("my-server")
///     .version("1.0.0")
///     .with_stdio_transport()
///     .with_tools(tools![MyTool])
///     .build();
///
/// McpServer::run(config).await?;
/// ```
pub struct McpServerConfig {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) transport: ServerTransport,
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
    transport: ServerTransport,
    registry: ToolRegistry,
    capabilities: McpCapabilities,
}

impl McpServerConfigBuilder {
    /// Creates a new configuration builder with defaults.
    pub fn new() -> Self {
        Self {
            name: "mcp-server".to_string(),
            version: "0.1.0".to_string(),
            transport: ServerTransport::default(),
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

    /// Configures the server to use stdio transport.
    pub fn with_stdio_transport(mut self) -> Self {
        self.transport = ServerTransport::Stdio;
        self
    }

    /// Configures the server to use HTTP transport.
    #[cfg(feature = "http-server")]
    pub fn with_http_transport(mut self, host: impl Into<String>, port: u16) -> Self {
        self.transport = ServerTransport::Http {
            host: host.into(),
            port,
        };
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
    ///
    /// Use the `tools![]` macro to create the vector:
    ///
    /// ```ignore
    /// .with_tools(tools![AddTool, SubtractTool])
    /// ```
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
    ///
    /// This collects all tools defined with `#[mcp_tool]` in the crate
    /// and registers them with the server.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use mcp::macros::mcp_tool;
    /// use mcp::{McpServerConfig, McpServer};
    ///
    /// #[mcp_tool(description = "Add numbers", group = "math")]
    /// fn add(a: f64, b: f64) -> f64 { a + b }
    ///
    /// let config = McpServerConfig::builder()
    ///     .name("my-server")
    ///     .register_tools()  // Registers all #[mcp_tool] functions
    ///     .build();
    /// ```
    pub fn register_tools(mut self) -> Self {
        for tool in crate::tool::all_tools() {
            self.registry.register(tool);
        }
        self
    }

    /// Registers auto-discovered tools filtered by group.
    ///
    /// Only registers tools that have the specified group.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use mcp::macros::mcp_tool;
    /// use mcp::{McpServerConfig, McpServer};
    ///
    /// #[mcp_tool(description = "Add numbers", group = "math")]
    /// fn add(a: f64, b: f64) -> f64 { a + b }
    ///
    /// #[mcp_tool(description = "Echo text", group = "text")]
    /// fn echo(msg: String) -> String { msg }
    ///
    /// let config = McpServerConfig::builder()
    ///     .name("math-server")
    ///     .register_tools_in_group("math")  // Only registers "add"
    ///     .build();
    /// ```
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

    /// Builds the server configuration.
    pub fn build(self) -> McpServerConfig {
        McpServerConfig {
            name: self.name,
            version: self.version,
            transport: self.transport,
            registry: self.registry,
            capabilities: self.capabilities,
        }
    }
}

/// MCP Server runtime.
///
/// Use `McpServer::run(config)` to start a server with the given configuration.
pub struct McpServer {
    config: McpServerConfig,
    registry: Arc<ToolRegistry>,
}

impl McpServer {
    /// Runs an MCP server with the given configuration.
    ///
    /// This is the main entry point for running a server.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = McpServerConfig::builder()
    ///     .name("my-server")
    ///     .version("1.0.0")
    ///     .with_stdio_transport()
    ///     .build();
    ///
    /// McpServer::run(config).await?;
    /// ```
    pub async fn run(config: McpServerConfig) -> Result<(), ServerError> {
        let server = Self {
            registry: Arc::new(config.registry),
            config: McpServerConfig {
                name: config.name,
                version: config.version,
                transport: config.transport,
                registry: ToolRegistry::new(), // Already moved to Arc
                capabilities: config.capabilities,
            },
        };

        match server.config.transport.clone() {
            ServerTransport::None => Err(ServerError::Transport(
                "No transport configured for server".to_string(),
            )),
            #[cfg(feature = "stdio")]
            ServerTransport::Stdio => server.run_stdio().await,
            #[cfg(feature = "http-server")]
            ServerTransport::Http { host, port } => server.run_http(&host, port).await,
        }
    }

    /// Returns the server name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Returns the server version.
    pub fn version(&self) -> &str {
        &self.config.version
    }

    /// Returns server info for MCP protocol.
    pub fn server_info(&self) -> McpServerInfo {
        McpServerInfo {
            name: self.config.name.clone(),
            version: self.config.version.clone(),
        }
    }

    /// Runs the stdio transport loop.
    async fn run_stdio(&self) -> Result<(), ServerError> {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let reader = stdin.lock();
        let mut writer = stdout.lock();

        for line in reader.lines() {
            let line = line.map_err(ServerError::Io)?;
            if line.is_empty() {
                continue;
            }

            let response = self.handle_request(&line).await;
            let response_json =
                serde_json::to_string(&response).map_err(ServerError::Serialization)?;

            writeln!(writer, "{}", response_json).map_err(ServerError::Io)?;
            writer.flush().map_err(ServerError::Io)?;
        }

        Ok(())
    }

    /// Runs the HTTP transport (actix-web server).
    #[cfg(feature = "http-server")]
    async fn run_http(&self, host: &str, port: u16) -> Result<(), ServerError> {
        use actix_web::{web, App, HttpResponse, HttpServer};

        let registry = self.registry.clone();
        let name = self.config.name.clone();
        let version = self.config.version.clone();
        let capabilities = self.config.capabilities.clone();

        HttpServer::new(move || {
            let registry_tools = registry.clone();
            let registry_call = registry.clone();
            let registry_rpc = registry.clone();
            let name_rpc = name.clone();
            let version_rpc = version.clone();
            let caps_rpc = capabilities.clone();

            App::new()
                .route(
                    "/tools",
                    web::get().to(move || {
                        let r = registry_tools.clone();
                        async move {
                            let tools = r.definitions();
                            HttpResponse::Ok().json(tools)
                        }
                    }),
                )
                .route(
                    "/call",
                    web::post().to(move |body: web::Json<CallToolRequest>| {
                        let r = registry_call.clone();
                        async move {
                            let result = r.call(&body.name, body.arguments.clone()).await;
                            match result {
                                Ok(content) => HttpResponse::Ok().json(content),
                                Err(e) => {
                                    HttpResponse::InternalServerError().json(serde_json::json!({
                                        "error": e.to_string()
                                    }))
                                }
                            }
                        }
                    }),
                )
                .route(
                    "/rpc",
                    web::post().to(move |body: String| {
                        let r = registry_rpc.clone();
                        let n = name_rpc.clone();
                        let v = version_rpc.clone();
                        let c = caps_rpc.clone();
                        async move {
                            let response = handle_rpc_request_static(&body, &r, &n, &v, &c).await;
                            HttpResponse::Ok().json(response)
                        }
                    }),
                )
        })
        .bind((host, port))
        .map_err(|e| ServerError::Io(io::Error::new(io::ErrorKind::AddrInUse, e)))?
        .run()
        .await
        .map_err(|e| ServerError::Io(io::Error::new(io::ErrorKind::Other, e)))
    }

    /// Handles a JSON-RPC request.
    pub async fn handle_request(&self, request_str: &str) -> JsonRpcResponse {
        let request: JsonRpcRequest = match serde_json::from_str(request_str) {
            Ok(req) => req,
            Err(e) => {
                return JsonRpcResponse::error(
                    JsonRpcId::Null,
                    -32700,
                    format!("Parse error: {}", e),
                    None,
                );
            }
        };

        self.handle_rpc_request(request).await
    }

    /// Handles a parsed JSON-RPC request.
    pub async fn handle_rpc_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id),
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "ping" => JsonRpcResponse::success(request.id, serde_json::json!({})),
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
                "capabilities": self.config.capabilities
            }),
        )
    }

    fn handle_tools_list(&self, id: JsonRpcId) -> JsonRpcResponse {
        let tools = self.registry.definitions();
        JsonRpcResponse::success(id, serde_json::json!({ "tools": tools }))
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

    /// Calls a tool directly by name.
    pub async fn call_tool(&self, name: &str, args: Value) -> ToolCallResult {
        self.registry.call(name, args).await
    }
}

/// Helper for HTTP handler - handles RPC without &self
#[cfg(feature = "http-server")]
async fn handle_rpc_request_static(
    request_str: &str,
    registry: &ToolRegistry,
    name: &str,
    version: &str,
    capabilities: &McpCapabilities,
) -> JsonRpcResponse {
    let request: JsonRpcRequest = match serde_json::from_str(request_str) {
        Ok(req) => req,
        Err(e) => {
            return JsonRpcResponse::error(
                JsonRpcId::Null,
                -32700,
                format!("Parse error: {}", e),
                None,
            );
        }
    };

    match request.method.as_str() {
        "initialize" => JsonRpcResponse::success(
            request.id,
            serde_json::json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "serverInfo": {
                    "name": name,
                    "version": version
                },
                "capabilities": capabilities
            }),
        ),
        "tools/list" => {
            let tools = registry.definitions();
            JsonRpcResponse::success(request.id, serde_json::json!({ "tools": tools }))
        }
        "tools/call" => {
            let params = match request.params {
                Some(p) => p,
                None => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        "Missing params".to_string(),
                        None,
                    );
                }
            };

            let tool_name = match params.get("name").and_then(|n| n.as_str()) {
                Some(n) => n,
                None => {
                    return JsonRpcResponse::error(
                        request.id,
                        -32602,
                        "Missing tool name".to_string(),
                        None,
                    );
                }
            };

            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let result = registry.call(tool_name, arguments).await;

            match result {
                Ok(content) => JsonRpcResponse::success(
                    request.id,
                    serde_json::json!({
                        "content": content,
                        "isError": false
                    }),
                ),
                Err(e) => JsonRpcResponse::success(
                    request.id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": e.to_string() }],
                        "isError": true
                    }),
                ),
            }
        }
        "ping" => JsonRpcResponse::success(request.id, serde_json::json!({})),
        _ => JsonRpcResponse::error(
            request.id,
            -32601,
            format!("Method not found: {}", request.method),
            None,
        ),
    }
}

/// Request body for the /call HTTP endpoint.
#[cfg(feature = "http-server")]
#[derive(serde::Deserialize)]
struct CallToolRequest {
    name: String,
    arguments: Value,
}

/// Errors that can occur when running an MCP server.
#[derive(Debug)]
pub enum ServerError {
    /// I/O error.
    Io(io::Error),
    /// JSON serialization error.
    Serialization(serde_json::Error),
    /// Transport error.
    Transport(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::Io(e) => write!(f, "I/O error: {}", e),
            ServerError::Serialization(e) => write!(f, "Serialization error: {}", e),
            ServerError::Transport(e) => write!(f, "Transport error: {}", e),
        }
    }
}

impl std::error::Error for ServerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ServerError::Io(e) => Some(e),
            ServerError::Serialization(e) => Some(e),
            ServerError::Transport(_) => None,
        }
    }
}

/// Creates a vector of tools from tool types.
///
/// This macro makes it easy to register multiple tools at once:
///
/// ```ignore
/// use mcp::macros::mcp_tool;
/// use mcp::{McpServerConfig, McpServer, tools};
///
/// #[mcp_tool(description = "Add numbers")]
/// fn add(a: f64, b: f64) -> f64 { a + b }
///
/// #[mcp_tool(description = "Subtract numbers")]
/// fn subtract(a: f64, b: f64) -> f64 { a - b }
///
/// let config = McpServerConfig::builder()
///     .name("calc")
///     .with_tools(tools![AddTool, SubtractTool])
///     .build();
/// ```
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
    use crate::protocol::{McpToolDef, ToolContent};
    use crate::tool::BoxFuture;

    struct EchoTool;

    impl McpTool for EchoTool {
        fn definition(&self) -> McpToolDef {
            McpToolDef {
                name: "echo".to_string(),
                description: Some("Echo the input".to_string()),
                group: None,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    }
                }),
            }
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
            .with_stdio_transport()
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
}
