//! HTTP Transport for MCP Servers
//!
//! Communicates with MCP servers via HTTP using JSON-RPC over POST requests.
//! This is used for MCP servers that expose an HTTP endpoint.

#[cfg(feature = "http")]
use async_trait::async_trait;
#[cfg(feature = "http")]
use serde_json::Value;
#[cfg(feature = "http")]
use std::sync::atomic::{AtomicI64, Ordering};
#[cfg(feature = "http")]
use std::sync::Arc;
#[cfg(feature = "http")]
use std::time::Duration;

#[cfg(feature = "http")]
use crate::protocol::*;
#[cfg(feature = "http")]
use crate::transport::{McpTransport, McpTransportError, TransportTypeId};

/// HTTP-based MCP transport for communicating with HTTP servers.
#[cfg(feature = "http")]
pub struct HttpTransport {
    endpoint: String,
    client: reqwest::Client,
    next_id: Arc<AtomicI64>,
}

#[cfg(feature = "http")]
impl HttpTransport {
    /// Create a new HTTP transport.
    pub fn new(endpoint: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            endpoint: endpoint.into(),
            client,
            next_id: Arc::new(AtomicI64::new(1)),
        }
    }

    /// Create with custom timeout.
    pub fn with_timeout(endpoint: impl Into<String>, timeout: Duration) -> Result<Self, McpTransportError> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| McpTransportError::TransportError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            endpoint: endpoint.into(),
            client,
            next_id: Arc::new(AtomicI64::new(1)),
        })
    }

    /// Send a JSON-RPC request via HTTP POST.
    pub async fn send_request(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, McpTransportError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest::new(JsonRpcId::Number(id), method, params);

        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|e| McpTransportError::TransportError(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpTransportError::TransportError(format!(
                "HTTP error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let json_response: JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| McpTransportError::TransportError(format!("Failed to parse JSON response: {}", e)))?;

        match json_response.payload {
            JsonRpcPayload::Success { result } => Ok(result),
            JsonRpcPayload::Error { error } => {
                Err(McpTransportError::ServerError(format!("MCP Error: {}", error)))
            }
        }
    }

    /// Health check - verify the endpoint is reachable.
    pub async fn health_check(&self) -> bool {
        self.client
            .head(&self.endpoint)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Get the endpoint URL.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

/// Adapter that wraps HttpTransport and implements McpTransport.
#[cfg(feature = "http")]
pub struct HttpTransportAdapter {
    inner: HttpTransport,
}

#[cfg(feature = "http")]
impl HttpTransportAdapter {
    /// Create a new HTTP transport adapter.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            inner: HttpTransport::new(endpoint),
        }
    }

    /// Create with custom timeout.
    pub fn with_timeout(endpoint: impl Into<String>, timeout: Duration) -> Result<Self, McpTransportError> {
        Ok(Self {
            inner: HttpTransport::with_timeout(endpoint, timeout)?,
        })
    }
}

#[cfg(feature = "http")]
#[async_trait]
impl McpTransport for HttpTransportAdapter {
    async fn list_tools(&self) -> Result<Vec<ToolDefinition>, McpTransportError> {
        let result = self
            .inner
            .send_request("tools/list", Some(serde_json::json!({})))
            .await?;

        let list_result: ListToolsResult = serde_json::from_value(result)?;

        Ok(list_result.tools.into_iter().map(ToolDefinition::from).collect())
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpTransportError> {
        let params = CallToolParams {
            name: name.to_string(),
            arguments: Some(args),
        };

        let result = self
            .inner
            .send_request("tools/call", Some(serde_json::to_value(&params)?))
            .await?;

        let call_result: CallToolResult = serde_json::from_value(result)?;

        if call_result.is_error == Some(true) {
            let error_text = call_result
                .content
                .first()
                .and_then(|c| c.as_text())
                .unwrap_or("Unknown error");
            return Err(McpTransportError::ServerError(error_text.to_string()));
        }

        let text = call_result
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(Value::String(text))
    }

    async fn shutdown(&self) -> Result<(), McpTransportError> {
        // HTTP transport doesn't need explicit shutdown
        Ok(())
    }

    fn is_alive(&self) -> bool {
        // For HTTP, we'd need to do a health check
        // For now, assume alive (caller can use health_check() if needed)
        true
    }

    fn transport_type(&self) -> TransportTypeId {
        TransportTypeId::Http
    }
}

#[cfg(all(test, feature = "http"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_transport_creation() {
        let transport = HttpTransport::new("http://localhost:8080/mcp");
        assert_eq!(transport.endpoint(), "http://localhost:8080/mcp");
    }

    #[tokio::test]
    async fn test_http_transport_with_timeout() {
        let transport = HttpTransport::with_timeout(
            "http://localhost:8080/mcp",
            Duration::from_secs(10),
        );
        assert!(transport.is_ok());
    }

    #[test]
    fn test_transport_type() {
        assert_eq!(TransportTypeId::Http.to_string(), "http");
    }
}
