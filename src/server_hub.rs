//! McpServerHub - A hub that aggregates multiple MCP servers into a single server.
//!
//! The McpServerHub connects to multiple external MCP servers and exposes their
//! tools as a unified MCP server that can be wrapped by McpStdioServer or McpHttpServer.
//!
//! # Example
//!
//! ```rust,ignore
//! use mcp::{McpServerHub, McpServerConnectionConfig};
//! use mcp::server::stdio::McpStdioServer;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a hub that aggregates multiple servers
//!     let hub = McpServerHub::new("aggregator");
//!
//!     // Connect to external servers
//!     let calc_config = McpServerConnectionConfig::stdio(
//!         "calculator",
//!         "node",
//!         vec!["calc-server.js".into()],
//!     );
//!     hub.connect(calc_config).await?;
//!
//!     let files_config = McpServerConnectionConfig::stdio(
//!         "files",
//!         "python",
//!         vec!["files-server.py".into()],
//!     );
//!     hub.connect(files_config).await?;
//!
//!     // Run as a stdio server - all connected tools are now exposed
//!     McpStdioServer::run(hub.into_config()).await?;
//!     Ok(())
//! }
//! ```

use serde_json::Value;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::hub_common::HubConnections;
use crate::protocol::McpToolDefinition;
use crate::server::McpServerConfig;
use crate::tool::{BoxFuture, DynTool, McpTool, ToolCallResult, ToolProvider};
use crate::transport::{McpServerConnectionConfig, McpTransportError};

/// A hub that aggregates multiple MCP servers into a single MCP server.
///
/// This allows you to:
/// - Connect to multiple external MCP servers
/// - Expose all their tools through a single unified server
/// - Wrap the hub with McpStdioServer or McpHttpServer
/// - Automatically restart servers on failure
/// - Parallel tool discovery for performance
///
/// Tools from connected servers are automatically discovered and made available
/// through the hub's server interface.
pub struct McpServerHub {
    /// Hub server name
    name: String,
    /// Shared connection infrastructure
    connections: HubConnections,
    /// Default timeout for operations
    timeout: Duration,
}

impl McpServerHub {
    /// Create a new hub with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            connections: HubConnections::new(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Create a hub with a custom timeout.
    pub fn with_timeout(name: impl Into<String>, timeout: Duration) -> Self {
        Self {
            name: name.into(),
            connections: HubConnections::new(),
            timeout,
        }
    }

    /// Connect to an external MCP server.
    ///
    /// This method:
    /// 1. Creates the appropriate transport based on config
    /// 2. Initializes the connection
    /// 3. Discovers tools and creates proxy tools for them
    /// 4. Starts a restart monitor if restart policy is enabled
    pub async fn connect(
        self: &Arc<Self>,
        config: McpServerConnectionConfig,
    ) -> Result<(), McpTransportError> {
        let server_name = config.name.clone();
        let restart_enabled = config.restart_policy.enabled;

        // Connect using shared infrastructure
        let connection = self.connections.connect(config).await?;

        // Start restart monitor if enabled
        if restart_enabled {
            let hub = Arc::clone(self);
            let conn = Arc::clone(&connection);
            let name = server_name.clone();

            tokio::spawn(async move {
                hub.restart_monitor(name, conn).await;
            });
        }

        Ok(())
    }

    /// Monitor a connection and restart on failure.
    async fn restart_monitor(&self, name: String, conn: Arc<crate::hub_common::ManagedConnection>) {
        let policy = &conn.config.restart_policy;

        loop {
            // Wait for health check interval or restart notification
            tokio::select! {
                _ = conn.restart_notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    if conn.is_alive().await {
                        continue;
                    }
                }
            }

            // Check if shutdown was requested
            if conn.shutdown_requested.load(Ordering::SeqCst) {
                break;
            }

            // Check if transport is still alive (double-check)
            if conn.is_alive().await {
                continue;
            }

            // Server is dead - notify all pending requests to fail immediately
            conn.notify_failure();

            // Get current attempt count
            let attempt = conn.restart_count.fetch_add(1, Ordering::SeqCst);

            // Check if we've exceeded max attempts
            if let Some(max) = policy.max_attempts {
                if attempt >= max {
                    eprintln!(
                        "[McpServerHub] Server '{}' exceeded max restart attempts ({})",
                        name, max
                    );
                    break;
                }
            }

            // Calculate delay with exponential backoff
            let delay = policy.delay_for_attempt(attempt);

            eprintln!(
                "[McpServerHub] Server '{}' disconnected. Restarting in {}ms (attempt {}/{})",
                name,
                delay,
                attempt + 1,
                policy
                    .max_attempts
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "∞".into())
            );

            tokio::time::sleep(Duration::from_millis(delay)).await;

            // Check again if shutdown was requested during sleep
            if conn.shutdown_requested.load(Ordering::SeqCst) {
                break;
            }

            // Attempt reconnection
            match self.connections.establish_connection(&conn).await {
                Ok(_) => {
                    eprintln!("[McpServerHub] Server '{}' reconnected successfully", name);
                    conn.restart_count.store(0, Ordering::SeqCst);
                }
                Err(e) => {
                    eprintln!(
                        "[McpServerHub] Server '{}' failed to reconnect: {}",
                        name, e
                    );
                }
            }
        }
    }

    /// Trigger an immediate restart for a specific server.
    pub fn trigger_restart(&self, server_name: &str) {
        if let Some(conn) = self.connections.get(server_name) {
            conn.restart_notify.notify_one();
        }
    }

    /// Call a tool by name, routing to the correct server.
    ///
    /// If the server restarts while a request is pending, the request will
    /// immediately fail with a `ServerRestarting` error rather than timing out.
    /// Uses circuit breaker to prevent cascading failures.
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpTransportError> {
        self.connections.call_tool(name, args).await
    }

    /// List all tools from all connected servers.
    pub async fn list_tools(&self) -> Result<Vec<(String, McpToolDefinition)>, McpTransportError> {
        Ok(self.connections.list_tools())
    }

    /// List all tool definitions.
    pub async fn list_all_tools(&self) -> Result<Vec<McpToolDefinition>, McpTransportError> {
        Ok(self.connections.list_tool_definitions())
    }

    /// Discover tools from all servers in parallel.
    pub async fn discover_tools_parallel(
        &self,
    ) -> Result<Vec<(String, McpToolDefinition)>, McpTransportError> {
        self.connections.discover_tools_parallel(self.timeout).await
    }

    /// Refresh tool cache by re-querying all servers (parallel).
    pub async fn refresh_tools(&self) -> Result<(), McpTransportError> {
        self.connections.refresh_tools_parallel(self.timeout).await
    }

    /// Get list of connected server names.
    pub fn list_servers(&self) -> Vec<String> {
        self.connections.list_servers()
    }

    /// Check if a server is connected.
    pub fn is_connected(&self, server_name: &str) -> bool {
        self.connections.is_connected(server_name)
    }

    /// Check if a server is connected and alive.
    pub async fn is_alive(&self, server_name: &str) -> bool {
        if let Some(conn) = self.connections.get(server_name) {
            conn.is_alive().await
        } else {
            false
        }
    }

    /// Get health status of all servers.
    pub async fn health_check(&self) -> Vec<(String, bool)> {
        self.connections.health_check().await
    }

    /// Get the server name that provides a specific tool.
    pub fn server_for_tool(&self, tool_name: &str) -> Option<String> {
        self.connections.server_for_tool(tool_name)
    }

    /// Disconnect a specific server (stops restart monitor).
    pub async fn disconnect(&self, server_name: &str) -> Result<(), McpTransportError> {
        let connection = self
            .connections
            .remove(server_name)
            .ok_or_else(|| McpTransportError::ServerNotFound(server_name.to_string()))?;

        // Signal shutdown to restart monitor
        connection.shutdown_requested.store(true, Ordering::SeqCst);
        connection.restart_notify.notify_one();

        // Clear tools for this server
        self.connections.clear_tools_for_server(server_name);

        // Shutdown transport
        if let Some(transport) = connection.get_transport().await {
            transport.shutdown().await?;
        }

        Ok(())
    }

    /// Shutdown all connected servers.
    pub async fn shutdown_all(&self) -> Result<(), McpTransportError> {
        let names: Vec<String> = self.list_servers();
        let mut errors = Vec::new();

        for name in names {
            if let Err(e) = self.disconnect(&name).await {
                errors.push(format!("{}: {}", name, e));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(McpTransportError::TransportError(errors.join("; ")))
        }
    }

    /// Convert this hub into an McpServerConfig that can be used with
    /// McpStdioServer or McpHttpServer.
    ///
    /// This creates proxy tools that route calls to the connected external servers.
    pub fn into_config(self, version: &str) -> McpServerConfig {
        let hub = Arc::new(self);
        let provider = HubToolProvider {
            hub: Arc::clone(&hub),
        };

        McpServerConfig::builder()
            .name(&hub.name)
            .version(version)
            .with_tools_from(provider)
            .build()
    }

    /// Create an McpServerConfig from this hub (keeps hub accessible).
    ///
    /// Use this when you need to keep a reference to the hub for direct access.
    pub fn to_config(self: &Arc<Self>, version: &str) -> McpServerConfig {
        let provider = HubToolProvider {
            hub: Arc::clone(self),
        };

        McpServerConfig::builder()
            .name(&self.name)
            .version(version)
            .with_tools_from(provider)
            .build()
    }

    /// Get proxy tools for all connected servers.
    ///
    /// Use this when you want to combine hub tools with local tools:
    /// ```rust,ignore
    /// let config = McpServerConfig::builder()
    ///     .name("my-server")
    ///     .version("1.0.0")
    ///     .register_tools_in_group("local")  // Local tools
    ///     .with_tools(hub.proxy_tools())     // Proxied tools
    ///     .build();
    /// ```
    pub fn proxy_tools(self: &Arc<Self>) -> Vec<DynTool> {
        let provider = HubToolProvider {
            hub: Arc::clone(self),
        };
        provider.tools()
    }

    /// Get circuit breaker statistics for a server.
    pub fn circuit_breaker_stats(
        &self,
        server_name: &str,
    ) -> Option<crate::circuit_breaker::CircuitBreakerStats> {
        self.connections.circuit_breaker_stats(server_name)
    }

    /// Reset circuit breaker for a server.
    pub fn reset_circuit_breaker(&self, server_name: &str) {
        self.connections.reset_circuit_breaker(server_name);
    }
}

/// Tool provider that creates proxy tools for all tools in the hub.
struct HubToolProvider {
    hub: Arc<McpServerHub>,
}

impl ToolProvider for HubToolProvider {
    fn tools(&self) -> Vec<DynTool> {
        self.hub
            .connections
            .list_tools()
            .into_iter()
            .map(|(_, def)| {
                let tool: DynTool = Arc::new(ProxyTool {
                    name: def.name.clone(),
                    definition: def,
                    hub: Arc::clone(&self.hub),
                });
                tool
            })
            .collect()
    }
}

/// A proxy tool that forwards calls to an external MCP server via the hub.
struct ProxyTool {
    name: String,
    definition: McpToolDefinition,
    hub: Arc<McpServerHub>,
}

impl McpTool for ProxyTool {
    fn definition(&self) -> McpToolDefinition {
        self.definition.clone()
    }

    fn call<'a>(&'a self, args: Value) -> BoxFuture<'a, ToolCallResult> {
        let name = self.name.clone();
        let hub = Arc::clone(&self.hub);

        Box::pin(async move {
            match hub.call_tool(&name, args).await {
                Ok(value) => {
                    // Convert the Value to ToolContent
                    if let Some(s) = value.as_str() {
                        Ok(vec![crate::protocol::ToolContent::text(s)])
                    } else {
                        Ok(vec![crate::protocol::ToolContent::text(value.to_string())])
                    }
                }
                Err(e) => Err(e.to_string()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hub_creation() {
        let hub = McpServerHub::new("test-hub");
        assert_eq!(hub.name, "test-hub");
        assert!(hub.list_servers().is_empty());
    }

    #[tokio::test]
    async fn test_hub_into_config() {
        let hub = McpServerHub::new("test-hub");
        let config = hub.into_config("1.0.0");
        assert_eq!(config.name(), "test-hub");
        assert_eq!(config.version(), "1.0.0");
    }

    #[tokio::test]
    async fn test_hub_unknown_tool() {
        let hub = McpServerHub::new("test");
        let result = hub.call_tool("nonexistent", serde_json::json!({})).await;
        assert!(matches!(result, Err(McpTransportError::UnknownTool(_))));
    }
}
