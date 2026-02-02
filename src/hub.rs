//! McpHub - Central hub for MCP tool routing across multiple servers.
//!
//! The McpHub manages connections to multiple MCP servers and provides
//! unified tool discovery, execution, and routing.

use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::circuit_breaker::CircuitBreakerStats;
use crate::hub_common::HubConnections;
use crate::protocol::McpToolDefinition;
use crate::transport::{
    McpServerConnectionConfig, McpTransport, McpTransportError,
};

/// Central hub for MCP tool routing across multiple servers.
///
/// The McpHub provides:
/// - Connection management for multiple MCP servers
/// - Tool discovery and caching
/// - Automatic routing of tool calls to the correct server
/// - Circuit breaker protection for resilience
/// - Parallel tool discovery for performance
///
/// # Example
///
/// ```rust,ignore
/// use mcp::{McpHub, McpServerConnectionConfig};
///
/// let hub = McpHub::new();
///
/// // Connect to an external server
/// let config = McpServerConnectionConfig::stdio("my-server", "node", vec!["server.js".into()]);
/// hub.connect(config).await?;
///
/// // List all available tools
/// let tools = hub.list_all_tools().await?;
///
/// // Call a tool (automatically routed to correct server)
/// let result = hub.call_tool("my_tool", serde_json::json!({"arg": "value"})).await?;
/// ```
pub struct McpHub {
    /// Shared connection infrastructure
    connections: HubConnections,
    /// Default timeout for tool discovery
    discovery_timeout: Duration,
}

impl Default for McpHub {
    fn default() -> Self {
        Self::new()
    }
}

impl McpHub {
    /// Create a new empty hub.
    pub fn new() -> Self {
        Self {
            connections: HubConnections::new(),
            discovery_timeout: Duration::from_secs(30),
        }
    }

    /// Create a hub with a custom discovery timeout.
    pub fn with_discovery_timeout(timeout: Duration) -> Self {
        Self {
            connections: HubConnections::new(),
            discovery_timeout: timeout,
        }
    }

    /// Connect to an MCP server.
    ///
    /// This method:
    /// 1. Creates the appropriate transport based on config
    /// 2. Initializes the connection
    /// 3. Discovers tools and caches the mapping
    /// 4. Returns the transport for direct access if needed
    pub async fn connect(
        &self,
        config: McpServerConnectionConfig,
    ) -> Result<Arc<dyn McpTransport>, McpTransportError> {
        let conn = self.connections.connect(config).await?;
        conn.get_transport().await.ok_or(McpTransportError::ConnectionClosed)
    }

    /// Call a tool, automatically routing to the correct server.
    /// 
    /// Uses circuit breaker to prevent cascading failures - if a server is
    /// unhealthy, requests will be rejected immediately.
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpTransportError> {
        self.connections.call_tool(name, args).await
    }

    /// List all tools from all connected servers.
    pub async fn list_tools(&self) -> Result<Vec<(String, McpToolDefinition)>, McpTransportError> {
        Ok(self.connections.list_tools())
    }

    /// Get all registered tools as a flat list.
    pub async fn list_all_tools(&self) -> Result<Vec<McpToolDefinition>, McpTransportError> {
        Ok(self.connections.list_tool_definitions())
    }

    /// Discover tools from all servers in parallel.
    /// 
    /// This is faster than sequential discovery when connecting to many servers.
    pub async fn discover_tools_parallel(&self) -> Result<Vec<(String, McpToolDefinition)>, McpTransportError> {
        self.connections.discover_tools_parallel(self.discovery_timeout).await
    }

    /// Populate the tool cache by querying all servers (parallel).
    pub async fn refresh_tool_cache(&self) -> Result<(), McpTransportError> {
        self.connections.refresh_tools_parallel(self.discovery_timeout).await
    }

    /// Shutdown all connected servers.
    pub async fn shutdown_all(&self) -> Result<(), McpTransportError> {
        let mut errors = Vec::new();

        for (server_name, conn) in self.connections.iter() {
            if let Some(transport) = conn.get_transport().await {
                if let Err(e) = transport.shutdown().await {
                    errors.push(format!("{}: {}", server_name, e));
                }
            }
        }
        self.connections.clear();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(McpTransportError::TransportError(errors.join("; ")))
        }
    }

    /// Disconnect a specific server.
    pub async fn disconnect(&self, server_name: &str) -> Result<(), McpTransportError> {
        let conn = self.connections.remove(server_name)
            .ok_or_else(|| McpTransportError::ServerNotFound(server_name.to_string()))?;

        self.connections.clear_tools_for_server(server_name);

        if let Some(transport) = conn.get_transport().await {
            transport.shutdown().await?;
        }
        Ok(())
    }

    /// Get list of connected server names.
    pub fn list_servers(&self) -> Vec<String> {
        self.connections.list_servers()
    }

    /// Check if a server is connected.
    pub fn is_connected(&self, server_name: &str) -> bool {
        self.connections.is_connected(server_name)
    }

    /// Get health status of all servers (includes circuit breaker state).
    pub async fn health_check(&self) -> Vec<(String, bool)> {
        self.connections.health_check().await
    }

    /// Get the server name that provides a specific tool.
    pub fn server_for_tool(&self, tool_name: &str) -> Option<String> {
        self.connections.server_for_tool(tool_name)
    }
    
    /// Get circuit breaker statistics for a server.
    pub fn circuit_breaker_stats(&self, server_name: &str) -> Option<CircuitBreakerStats> {
        self.connections.circuit_breaker_stats(server_name)
    }
    
    /// Reset circuit breaker for a server (e.g., after manual recovery).
    pub fn reset_circuit_breaker(&self, server_name: &str) {
        self.connections.reset_circuit_breaker(server_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hub_creation() {
        let hub = McpHub::new();
        let servers = hub.list_servers();
        assert!(servers.is_empty());
    }

    #[tokio::test]
    async fn test_hub_unknown_tool() {
        let hub = McpHub::new();

        let result = hub
            .call_tool("nonexistent_tool", serde_json::json!({}))
            .await;
        assert!(matches!(result, Err(McpTransportError::UnknownTool(_))));
    }

    #[test]
    fn test_connection_config() {
        let config =
            McpServerConnectionConfig::stdio("test", "node", vec!["server.js".to_string()])
                .with_timeout(60);

        assert_eq!(config.name, "test");
        assert_eq!(config.timeout_secs, 60);
    }
}
