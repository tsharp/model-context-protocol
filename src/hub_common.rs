//! Common hub infrastructure shared between McpHub and McpServerHub.
//!
//! This module provides:
//! - Shared connection state management
//! - Circuit breaker integration
//! - Tool cache management
//! - Parallel tool discovery

use dashmap::DashMap;
use futures::future::join_all;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Notify};

use crate::circuit_breaker::CircuitBreaker;
use crate::protocol::McpToolDefinition;
use crate::transport::{McpServerConnectionConfig, McpTransport, McpTransportError};
use crate::transport_factory::TransportFactory;

/// Connection state for a managed server.
pub struct ManagedConnection {
    /// Original configuration (used for restarts)
    pub config: McpServerConnectionConfig,
    /// Current transport (may be replaced on restart)
    pub transport: tokio::sync::RwLock<Option<Arc<dyn McpTransport>>>,
    /// Circuit breaker for resilient connections
    pub circuit_breaker: CircuitBreaker,
    /// Number of restart attempts
    pub restart_count: AtomicU32,
    /// Whether shutdown has been requested
    pub shutdown_requested: AtomicBool,
    /// Notifier for restart events
    pub restart_notify: Notify,
    /// Broadcast channel to notify pending requests of failure
    pub failure_tx: broadcast::Sender<()>,
}

impl ManagedConnection {
    /// Create a new managed connection.
    pub fn new(config: McpServerConnectionConfig) -> Self {
        let (failure_tx, _) = broadcast::channel(16);
        Self {
            config,
            transport: tokio::sync::RwLock::new(None),
            circuit_breaker: CircuitBreaker::new(),
            restart_count: AtomicU32::new(0),
            shutdown_requested: AtomicBool::new(false),
            restart_notify: Notify::new(),
            failure_tx,
        }
    }

    /// Check if the connection is alive.
    pub async fn is_alive(&self) -> bool {
        if let Some(transport) = self.transport.read().await.as_ref() {
            transport.is_alive()
        } else {
            false
        }
    }

    /// Get the transport if available.
    pub async fn get_transport(&self) -> Option<Arc<dyn McpTransport>> {
        self.transport.read().await.clone()
    }

    /// Subscribe to failure notifications.
    pub fn subscribe_failures(&self) -> broadcast::Receiver<()> {
        self.failure_tx.subscribe()
    }

    /// Notify pending requests of failure.
    pub fn notify_failure(&self) {
        let _ = self.failure_tx.send(());
    }
}

/// Managed hub connections with shared infrastructure.
pub struct HubConnections {
    /// Server name → connection mapping
    connections: DashMap<String, Arc<ManagedConnection>>,
    /// Tool name → (server name, optional tool definition)
    tool_cache: DashMap<String, (String, Option<McpToolDefinition>)>,
}

impl Default for HubConnections {
    fn default() -> Self {
        Self::new()
    }
}

impl HubConnections {
    /// Create a new connection manager.
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            tool_cache: DashMap::new(),
        }
    }

    /// Establish a connection to a server.
    pub async fn connect(&self, config: McpServerConnectionConfig) -> Result<Arc<ManagedConnection>, McpTransportError> {
        let server_name = config.name.clone();
        let connection = Arc::new(ManagedConnection::new(config));
        
        // Establish initial connection
        self.establish_connection(&connection).await?;
        
        // Store connection
        self.connections.insert(server_name, Arc::clone(&connection));
        
        Ok(connection)
    }

    /// Establish or re-establish a connection.
    pub async fn establish_connection(&self, conn: &ManagedConnection) -> Result<(), McpTransportError> {
        let config = &conn.config;
        let server_name = config.name.clone();

        // Use transport factory for unified transport creation
        let transport = TransportFactory::create(config).await?;

        // Discover tools and cache them (clear old tools first)
        let tools = transport.list_tools().await?;

        // Remove old tools for this server, then add new ones
        self.tool_cache.retain(|_, (srv, _)| srv != &server_name);
        for tool in tools {
            self.tool_cache.insert(tool.name.clone(), (server_name.clone(), Some(tool)));
        }

        // Store transport
        *conn.transport.write().await = Some(transport);

        Ok(())
    }

    /// Get a connection by server name.
    pub fn get(&self, server_name: &str) -> Option<Arc<ManagedConnection>> {
        self.connections.get(server_name).map(|r| r.value().clone())
    }

    /// Remove a connection.
    pub fn remove(&self, server_name: &str) -> Option<Arc<ManagedConnection>> {
        self.connections.remove(server_name).map(|(_, v)| v)
    }

    /// Get server name for a tool.
    pub fn server_for_tool(&self, tool_name: &str) -> Option<String> {
        self.tool_cache.get(tool_name).map(|r| r.value().0.clone())
    }

    /// Get tool definition by name.
    pub fn get_tool_definition(&self, tool_name: &str) -> Option<McpToolDefinition> {
        self.tool_cache.get(tool_name).and_then(|r| r.value().1.clone())
    }

    /// List all server names.
    pub fn list_servers(&self) -> Vec<String> {
        self.connections.iter().map(|r| r.key().clone()).collect()
    }

    /// List all tools with their server names.
    pub fn list_tools(&self) -> Vec<(String, McpToolDefinition)> {
        self.tool_cache.iter()
            .filter_map(|r| r.value().1.clone().map(|def| (r.value().0.clone(), def)))
            .collect()
    }

    /// List all tool definitions.
    pub fn list_tool_definitions(&self) -> Vec<McpToolDefinition> {
        self.tool_cache.iter()
            .filter_map(|r| r.value().1.clone())
            .collect()
    }

    /// Check if a server is connected.
    pub fn is_connected(&self, server_name: &str) -> bool {
        self.connections.contains_key(server_name)
    }

    /// Clear tool cache for a server.
    pub fn clear_tools_for_server(&self, server_name: &str) {
        self.tool_cache.retain(|_, (srv, _)| srv != server_name);
    }

    /// Clear all connections and tool cache.
    pub fn clear(&self) {
        self.connections.clear();
        self.tool_cache.clear();
    }

    /// Iterate over all connections.
    pub fn iter(&self) -> impl Iterator<Item = (String, Arc<ManagedConnection>)> + '_ {
        self.connections.iter().map(|r| (r.key().clone(), r.value().clone()))
    }

    /// Call a tool with circuit breaker and failure handling.
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpTransportError> {
        let server_name = self.server_for_tool(name)
            .ok_or_else(|| McpTransportError::UnknownTool(name.to_string()))?;

        let connection = self.get(&server_name)
            .ok_or_else(|| McpTransportError::ServerNotFound(server_name.clone()))?;

        // Check circuit breaker
        if !connection.circuit_breaker.allow_request() {
            return Err(McpTransportError::ServerError(format!(
                "Server '{}' circuit breaker is open - server is unhealthy",
                server_name
            )));
        }

        // Subscribe to failure notifications before getting transport
        let mut failure_rx = connection.subscribe_failures();

        let transport = connection.get_transport().await
            .ok_or_else(|| McpTransportError::ConnectionClosed)?;

        // Race between the actual tool call and a failure notification
        let result = tokio::select! {
            result = transport.call_tool(name, args) => result,
            _ = failure_rx.recv() => {
                Err(McpTransportError::ServerRestarting(server_name.clone()))
            }
        };

        // Record result in circuit breaker
        match &result {
            Ok(_) => connection.circuit_breaker.record_success(),
            Err(_) => connection.circuit_breaker.record_failure(),
        }

        result
    }

    /// Discover tools from all servers in parallel.
    /// 
    /// This is much faster than sequential discovery when connecting to many servers.
    pub async fn discover_tools_parallel(&self, timeout: Duration) -> Result<Vec<(String, McpToolDefinition)>, McpTransportError> {
        let connections: Vec<_> = self.iter().collect();
        
        // Create futures for each server's tool discovery
        let futures: Vec<_> = connections.into_iter().map(|(server_name, conn)| {
            let server_name = server_name.clone();
            async move {
                let result = tokio::time::timeout(timeout, async {
                    if let Some(transport) = conn.get_transport().await {
                        transport.list_tools().await
                    } else {
                        Err(McpTransportError::ConnectionClosed)
                    }
                }).await;
                
                match result {
                    Ok(Ok(tools)) => (server_name, Ok(tools)),
                    Ok(Err(e)) => (server_name, Err(e)),
                    Err(_) => (server_name.clone(), Err(McpTransportError::Timeout(
                        format!("Tool discovery for '{}' timed out", server_name)
                    ))),
                }
            }
        }).collect();

        // Run all discoveries in parallel
        let results = join_all(futures).await;

        // Collect results and update cache
        let mut all_tools = Vec::new();
        
        for (server_name, result) in results {
            match result {
                Ok(tools) => {
                    for tool in tools {
                        self.tool_cache.insert(tool.name.clone(), (server_name.clone(), Some(tool.clone())));
                        all_tools.push((server_name.clone(), tool));
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to discover tools from '{}': {}", server_name, e);
                }
            }
        }

        Ok(all_tools)
    }

    /// Refresh tool cache from all servers in parallel.
    pub async fn refresh_tools_parallel(&self, timeout: Duration) -> Result<(), McpTransportError> {
        // Clear existing cache
        self.tool_cache.clear();
        // Discover tools
        let _ = self.discover_tools_parallel(timeout).await?;
        Ok(())
    }

    /// Get health status of all servers.
    pub async fn health_check(&self) -> Vec<(String, bool)> {
        let connections: Vec<_> = self.iter().collect();
        let mut results = Vec::new();

        for (name, conn) in connections {
            let transport_alive = conn.is_alive().await;
            let circuit_ok = conn.circuit_breaker.allow_request();
            results.push((name, transport_alive && circuit_ok));
        }

        results
    }

    /// Get circuit breaker statistics for a server.
    pub fn circuit_breaker_stats(&self, server_name: &str) -> Option<crate::circuit_breaker::CircuitBreakerStats> {
        self.get(server_name).map(|c| c.circuit_breaker.stats())
    }

    /// Reset circuit breaker for a server.
    pub fn reset_circuit_breaker(&self, server_name: &str) {
        if let Some(conn) = self.get(server_name) {
            conn.circuit_breaker.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::RestartPolicy;

    #[test]
    fn test_restart_policy_delay() {
        let policy = RestartPolicy {
            enabled: true,
            max_attempts: Some(5),
            delay_ms: 1000,
            max_delay_ms: 30_000,
            backoff_multiplier: 2.0,
        };

        assert_eq!(policy.delay_for_attempt(0), 1000);
        assert_eq!(policy.delay_for_attempt(1), 2000);
        assert_eq!(policy.delay_for_attempt(2), 4000);
        assert_eq!(policy.delay_for_attempt(5), 30_000); // Capped
    }

    #[test]
    fn test_hub_connections_creation() {
        let conns = HubConnections::new();
        assert!(conns.list_servers().is_empty());
        assert!(conns.list_tool_definitions().is_empty());
    }
}
