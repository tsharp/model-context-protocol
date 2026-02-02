//! Transport factory for creating MCP transports from configuration.
//!
//! This module provides a unified interface for creating transports based on
//! `McpServerConnectionConfig`, eliminating duplicate transport creation logic
//! in hub.rs and server_hub.rs.

use std::sync::Arc;
use std::time::Duration;

use crate::client::http::HttpTransportAdapter;
use crate::client::stdio::StdioTransportAdapter;
use crate::transport::{
    McpServerConnectionConfig, McpTransport, McpTransportError, TransportTypeId,
};

/// Factory for creating MCP transports from configuration.
pub struct TransportFactory;

impl TransportFactory {
    /// Create a transport from a connection configuration.
    ///
    /// Returns a boxed transport trait object that can be used for communication.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Stdio transport is missing command
    /// - HTTP transport is missing URL
    /// - Transport creation fails
    pub async fn create(
        config: &McpServerConnectionConfig,
    ) -> Result<Arc<dyn McpTransport>, McpTransportError> {
        match config.transport {
            TransportTypeId::Stdio => Self::create_stdio(config).await,
            TransportTypeId::Http => Self::create_http(config).await,
        }
    }

    /// Create a stdio transport.
    async fn create_stdio(
        config: &McpServerConnectionConfig,
    ) -> Result<Arc<dyn McpTransport>, McpTransportError> {
        let command = config.command.as_ref().ok_or_else(|| {
            McpTransportError::TransportError("Stdio transport requires command".to_string())
        })?;

        let timeout = Duration::from_secs(config.timeout_secs);

        let transport = StdioTransportAdapter::connect_with_env(
            command,
            &config.args,
            config.env.clone(),
            Some(config.config.clone()),
            timeout,
        )
        .await?;

        Ok(Arc::new(transport))
    }

    /// Create an HTTP transport.
    async fn create_http(
        config: &McpServerConnectionConfig,
    ) -> Result<Arc<dyn McpTransport>, McpTransportError> {
        let url = config.url.as_ref().ok_or_else(|| {
            McpTransportError::TransportError("HTTP transport requires URL".to_string())
        })?;

        let timeout = Duration::from_secs(config.timeout_secs);
        let transport = HttpTransportAdapter::with_timeout(url, timeout)?;

        Ok(Arc::new(transport))
    }

    /// Check if a transport type is supported.
    pub fn is_supported(transport_type: TransportTypeId) -> bool {
        matches!(
            transport_type,
            TransportTypeId::Stdio | TransportTypeId::Http
        )
    }

    /// List supported transport types.
    pub fn supported_types() -> Vec<TransportTypeId> {
        vec![TransportTypeId::Stdio, TransportTypeId::Http]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported() {
        assert!(TransportFactory::is_supported(TransportTypeId::Stdio));
        assert!(TransportFactory::is_supported(TransportTypeId::Http));
    }

    #[test]
    fn test_supported_types() {
        let types = TransportFactory::supported_types();
        assert!(types.contains(&TransportTypeId::Stdio));
        assert!(types.contains(&TransportTypeId::Http));
    }
}
