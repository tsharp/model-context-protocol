//! Stdio Transport for MCP Servers
//!
//! Communicates with MCP servers via standard input/output using JSON-RPC.
//! This is used for MCP servers that run as child processes.

use async_trait::async_trait;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::timeout;

use crate::protocol::*;
use crate::transport::{
    InitializeParams, McpTransport, McpTransportError, TransportTypeId,
};

/// Stdio-based MCP transport for communicating with child processes.
pub struct StdioTransport {
    process: Arc<Mutex<Child>>,
    next_id: Arc<AtomicI64>,
    alive: Arc<AtomicBool>,
}

impl StdioTransport {
    /// Spawn a new MCP server process.
    pub fn spawn(command: &str, args: &[String]) -> Result<Self, McpTransportError> {
        Self::spawn_with_env(command, args, std::collections::HashMap::new())
    }

    /// Spawn a new MCP server process with environment variables.
    pub fn spawn_with_env(
        command: &str,
        args: &[String],
        env: std::collections::HashMap<String, String>,
    ) -> Result<Self, McpTransportError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in env {
            cmd.env(key, value);
        }

        let child = cmd
            .spawn()
            .map_err(|e| McpTransportError::TransportError(format!("Failed to spawn process '{}': {}", command, e)))?;

        // Verify process is running
        let mut process = child;
        if let Some(status) = process
            .try_wait()
            .map_err(|e| McpTransportError::TransportError(format!("Process check failed: {}", e)))?
        {
            return Err(McpTransportError::TransportError(format!(
                "Process exited immediately with status: {}",
                status
            )));
        }

        Ok(Self {
            process: Arc::new(Mutex::new(process)),
            next_id: Arc::new(AtomicI64::new(1)),
            alive: Arc::new(AtomicBool::new(true)),
        })
    }

    /// Send a JSON-RPC request and wait for response (blocking).
    pub fn send_request_sync(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, McpTransportError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest::new(JsonRpcId::Number(id), method, params);

        let mut process = self
            .process
            .lock()
            .map_err(|e| McpTransportError::TransportError(format!("Lock error: {}", e)))?;

        // Get stdin
        let stdin = process
            .stdin
            .as_mut()
            .ok_or_else(|| McpTransportError::TransportError("Failed to get stdin".to_string()))?;

        // Serialize and send request
        let request_json = serde_json::to_string(&request)?;

        writeln!(stdin, "{}", request_json)
            .map_err(|e| McpTransportError::IoError(e))?;

        stdin.flush().map_err(|e| McpTransportError::IoError(e))?;

        // Read response from stdout
        let stdout = process
            .stdout
            .as_mut()
            .ok_or_else(|| McpTransportError::TransportError("Failed to get stdout".to_string()))?;

        let mut reader = BufReader::new(stdout);
        let mut response_line = String::new();

        reader
            .read_line(&mut response_line)
            .map_err(|e| McpTransportError::IoError(e))?;

        if response_line.is_empty() {
            self.alive.store(false, Ordering::SeqCst);
            return Err(McpTransportError::ConnectionClosed);
        }

        // Parse response
        let response: JsonRpcResponse = serde_json::from_str(&response_line)?;

        // Extract result or error
        match response.payload {
            JsonRpcPayload::Success { result } => Ok(result),
            JsonRpcPayload::Error { error } => {
                Err(McpTransportError::ServerError(format!("MCP Error: {}", error)))
            }
        }
    }

    /// Check if the process is still running.
    pub fn is_alive(&self) -> bool {
        if !self.alive.load(Ordering::SeqCst) {
            return false;
        }

        if let Ok(mut process) = self.process.lock() {
            let alive = process.try_wait().ok().flatten().is_none();
            self.alive.store(alive, Ordering::SeqCst);
            alive
        } else {
            false
        }
    }

    /// Stop the process.
    pub fn stop(&self) -> Result<(), McpTransportError> {
        self.alive.store(false, Ordering::SeqCst);

        let mut process = self
            .process
            .lock()
            .map_err(|e| McpTransportError::TransportError(format!("Lock error: {}", e)))?;

        process.kill().map_err(|e| McpTransportError::IoError(e))?;

        process.wait().map_err(|e| McpTransportError::IoError(e))?;

        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Async-friendly stdio transport with timeout support.
pub struct AsyncStdioTransport {
    inner: StdioTransport,
}

impl AsyncStdioTransport {
    /// Spawn a new MCP server process.
    pub fn spawn(command: &str, args: &[String]) -> Result<Self, McpTransportError> {
        Ok(Self {
            inner: StdioTransport::spawn(command, args)?,
        })
    }

    /// Spawn with environment variables.
    pub fn spawn_with_env(
        command: &str,
        args: &[String],
        env: std::collections::HashMap<String, String>,
    ) -> Result<Self, McpTransportError> {
        Ok(Self {
            inner: StdioTransport::spawn_with_env(command, args, env)?,
        })
    }

    /// Send a request with a timeout.
    pub async fn send_request_with_timeout(
        &self,
        method: &str,
        params: Option<Value>,
        timeout_duration: Duration,
    ) -> Result<Value, McpTransportError> {
        let method = method.to_string();
        let process = Arc::clone(&self.inner.process);
        let next_id = Arc::clone(&self.inner.next_id);
        let alive = Arc::clone(&self.inner.alive);

        let (tx, rx) = oneshot::channel();

        // Spawn blocking task
        tokio::task::spawn_blocking(move || {
            let id = next_id.fetch_add(1, Ordering::SeqCst);
            let request = JsonRpcRequest::new(JsonRpcId::Number(id), method, params);

            let result: Result<Value, McpTransportError> = (|| {
                let mut process = process
                    .lock()
                    .map_err(|e| McpTransportError::TransportError(format!("Lock error: {}", e)))?;

                let stdin = process
                    .stdin
                    .as_mut()
                    .ok_or_else(|| McpTransportError::TransportError("Failed to get stdin".to_string()))?;

                let request_json = serde_json::to_string(&request)?;

                writeln!(stdin, "{}", request_json)
                    .map_err(|e| McpTransportError::IoError(e))?;

                stdin.flush().map_err(|e| McpTransportError::IoError(e))?;

                let stdout = process
                    .stdout
                    .as_mut()
                    .ok_or_else(|| McpTransportError::TransportError("Failed to get stdout".to_string()))?;

                let mut reader = BufReader::new(stdout);
                let mut response_line = String::new();

                reader
                    .read_line(&mut response_line)
                    .map_err(|e| McpTransportError::IoError(e))?;

                if response_line.is_empty() {
                    alive.store(false, Ordering::SeqCst);
                    return Err(McpTransportError::ConnectionClosed);
                }

                let response: JsonRpcResponse = serde_json::from_str(&response_line)?;

                match response.payload {
                    JsonRpcPayload::Success { result } => Ok(result),
                    JsonRpcPayload::Error { error } => {
                        Err(McpTransportError::ServerError(format!("MCP Error: {}", error)))
                    }
                }
            })();

            let _ = tx.send(result);
        });

        // Wait with timeout
        match timeout(timeout_duration, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(McpTransportError::TransportError("Channel closed".to_string())),
            Err(_) => Err(McpTransportError::Timeout(format!(
                "Request timed out after {:?}",
                timeout_duration
            ))),
        }
    }

    /// Check if alive.
    pub fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }

    /// Stop the transport.
    pub fn stop(&self) -> Result<(), McpTransportError> {
        self.inner.stop()
    }
}

/// Adapter that wraps AsyncStdioTransport and implements McpTransport.
pub struct StdioTransportAdapter {
    inner: AsyncStdioTransport,
    timeout: Duration,
}

impl StdioTransportAdapter {
    /// Create and initialize a new stdio transport.
    pub async fn connect(
        command: &str,
        args: &[String],
        config: Option<Value>,
        timeout: Duration,
    ) -> Result<Self, McpTransportError> {
        Self::connect_with_env(command, args, std::collections::HashMap::new(), config, timeout).await
    }

    /// Create and initialize with environment variables.
    pub async fn connect_with_env(
        command: &str,
        args: &[String],
        env: std::collections::HashMap<String, String>,
        config: Option<Value>,
        timeout: Duration,
    ) -> Result<Self, McpTransportError> {
        let inner = AsyncStdioTransport::spawn_with_env(command, args, env)?;

        let adapter = Self { inner, timeout };

        // Send initialize request
        let init_params = InitializeParams::new(config);
        let _init_result = adapter
            .inner
            .send_request_with_timeout(
                "initialize",
                Some(serde_json::to_value(&init_params)?),
                adapter.timeout,
            )
            .await?;

        // Send initialized notification (no response expected, but we send it)
        // Some servers expect this
        let _ = adapter
            .inner
            .send_request_with_timeout("notifications/initialized", Some(serde_json::json!({})), adapter.timeout)
            .await;

        Ok(adapter)
    }
}

#[async_trait]
impl McpTransport for StdioTransportAdapter {
    async fn list_tools(&self) -> Result<Vec<ToolDefinition>, McpTransportError> {
        let result = self
            .inner
            .send_request_with_timeout("tools/list", Some(serde_json::json!({})), self.timeout)
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
            .send_request_with_timeout(
                "tools/call",
                Some(serde_json::to_value(&params)?),
                self.timeout,
            )
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
        self.inner.stop()
    }

    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }

    fn transport_type(&self) -> TransportTypeId {
        TransportTypeId::Stdio
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require actual processes to be available
    // In practice, you'd use mock servers or skip these in CI

    #[test]
    fn test_transport_type() {
        // We can't easily test spawn without a real server,
        // but we can test the type system
        assert_eq!(TransportTypeId::Stdio.to_string(), "stdio");
    }
}
