//! McpHub - Central hub for MCP tool routing across multiple servers.
//!
//! The McpHub manages connections to multiple MCP servers and provides
//! unified tool discovery, execution, and routing.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[cfg(feature = "http")]
use crate::http::HttpTransportAdapter;
use crate::protocol::ToolDefinition;
#[cfg(feature = "stdio")]
use crate::stdio::StdioTransportAdapter;
use crate::transport::{
    McpServerConnectionConfig, McpTransport, McpTransportError, TransportTypeId,
};

/// Central hub for MCP tool routing across multiple servers.
///
/// The McpHub provides:
/// - Connection management for multiple MCP servers
/// - Tool discovery and caching
/// - Automatic routing of tool calls to the correct server
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
    /// Server name → transport mapping
    transports: Arc<RwLock<HashMap<String, Arc<dyn McpTransport>>>>,

    /// Tool name → server name mapping for routing
    tool_cache: Arc<RwLock<HashMap<String, String>>>,
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
            transports: Arc::new(RwLock::new(HashMap::new())),
            tool_cache: Arc::new(RwLock::new(HashMap::new())),
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
        let transport: Arc<dyn McpTransport> = match config.transport {
            #[cfg(feature = "stdio")]
            TransportTypeId::Stdio => {
                let command = config.command.ok_or_else(|| {
                    McpTransportError::TransportError(
                        "Stdio transport requires command".to_string(),
                    )
                })?;

                let transport = StdioTransportAdapter::connect_with_env(
                    &command,
                    &config.args,
                    config.env,
                    Some(config.config.clone()),
                    Duration::from_secs(config.timeout_secs),
                )
                .await?;

                Arc::new(transport)
            }
            #[cfg(not(feature = "stdio"))]
            TransportTypeId::Stdio => {
                return Err(McpTransportError::NotSupported(
                    "Stdio transport not enabled. Enable the 'stdio' feature.".to_string(),
                ));
            }
            #[cfg(feature = "http")]
            TransportTypeId::Http | TransportTypeId::Sse => {
                let url = config.url.ok_or_else(|| {
                    McpTransportError::TransportError("HTTP transport requires URL".to_string())
                })?;

                let transport = HttpTransportAdapter::with_timeout(
                    url,
                    Duration::from_secs(config.timeout_secs),
                )?;

                Arc::new(transport)
            }
            #[cfg(not(feature = "http"))]
            TransportTypeId::Http | TransportTypeId::Sse => {
                return Err(McpTransportError::NotSupported(
                    "HTTP transport not enabled. Enable the 'http' feature.".to_string(),
                ));
            }
        };

        // Discover tools and cache mappings
        let tools = transport.list_tools().await?;

        {
            let mut cache = self.tool_cache.write().unwrap();
            for tool in &tools {
                cache.insert(tool.name.clone(), config.name.clone());
            }
            let mut transports = self.transports.write().unwrap();
            transports.insert(config.name.clone(), transport.clone());
        }

        Ok(transport)
    }

    /// Call a tool, automatically routing to the correct server.
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpTransportError> {
        // Look up server for this tool
        let server_name = self
            .tool_cache
            .read()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| McpTransportError::UnknownTool(name.to_string()))?;

        // Get transport
        let transport = self
            .transports
            .read()
            .unwrap()
            .get(&server_name)
            .cloned()
            .ok_or_else(|| McpTransportError::ServerNotFound(server_name.clone()))?;

        // Forward call
        transport.call_tool(name, args).await
    }

    /// List all tools from all connected servers.
    pub async fn list_tools(&self) -> Result<Vec<(String, ToolDefinition)>, McpTransportError> {
        let mut all_tools = Vec::new();

        let transports = self.transports.read().unwrap().clone();
        for (server_name, transport) in transports {
            match transport.list_tools().await {
                Ok(tools) => {
                    let mut cache = self.tool_cache.write().unwrap();
                    for tool in tools {
                        cache.insert(tool.name.clone(), server_name.clone());
                        all_tools.push((server_name.clone(), tool));
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to list tools from '{}': {}",
                        server_name, e
                    );
                }
            }
        }

        Ok(all_tools)
    }

    /// Get all registered tools as a flat list.
    pub async fn list_all_tools(&self) -> Result<Vec<ToolDefinition>, McpTransportError> {
        let tools_with_servers = self.list_tools().await?;
        Ok(tools_with_servers
            .into_iter()
            .map(|(_, tool)| tool)
            .collect())
    }

    /// Populate the tool cache by querying all servers.
    pub async fn refresh_tool_cache(&self) -> Result<(), McpTransportError> {
        let _ = self.list_tools().await?;
        Ok(())
    }

    /// Manually register a tool in the cache.
    pub fn register_tool_sync(&self, tool_name: &str, server_name: &str) {
        self.tool_cache
            .write()
            .unwrap()
            .insert(tool_name.to_string(), server_name.to_string());
    }

    /// Shutdown all connected servers.
    pub async fn shutdown_all(&self) -> Result<(), McpTransportError> {
        let mut errors = Vec::new();

        let transports = std::mem::take(&mut *self.transports.write().unwrap());
        for (server_name, transport) in transports {
            if let Err(e) = transport.shutdown().await {
                errors.push(format!("{}: {}", server_name, e));
            }
        }
        self.tool_cache.write().unwrap().clear();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(McpTransportError::TransportError(errors.join("; ")))
        }
    }

    /// Disconnect a specific server.
    pub async fn disconnect(&self, server_name: &str) -> Result<(), McpTransportError> {
        let transport = self
            .transports
            .write()
            .unwrap()
            .remove(server_name)
            .ok_or_else(|| McpTransportError::ServerNotFound(server_name.to_string()))?;

        // Remove tool cache entries for this server
        self.tool_cache
            .write()
            .unwrap()
            .retain(|_, server| server != server_name);

        transport.shutdown().await
    }

    /// Get list of connected server names.
    pub fn list_servers(&self) -> Vec<String> {
        self.transports.read().unwrap().keys().cloned().collect()
    }

    /// Check if a server is connected.
    pub fn is_connected(&self, server_name: &str) -> bool {
        self.transports.read().unwrap().contains_key(server_name)
    }

    /// Get health status of all servers.
    pub fn health_check(&self) -> Vec<(String, bool)> {
        self.transports
            .read()
            .unwrap()
            .iter()
            .map(|(name, transport)| (name.clone(), transport.is_alive()))
            .collect()
    }

    /// Get the server name that provides a specific tool.
    pub fn server_for_tool(&self, tool_name: &str) -> Option<String> {
        self.tool_cache.read().unwrap().get(tool_name).cloned()
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
