//! Stdio Transport for connecting to MCP server processes.
//!
//! Communicates with MCP servers via standard input/output using JSON-RPC.
//! This is used to connect to MCP servers that run as child processes.
//!
//! ## Architecture
//!
//! Uses true async I/O with `tokio::process` for non-blocking communication:
//! - Separate async tasks for stdin writing and stdout reading
//! - Pending request tracking with oneshot channels for responses
//! - No mutex held across I/O operations, enabling concurrent requests

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

use crate::protocol::*;
use crate::transport::{InitializeParams, McpTransport, McpTransportError, TransportTypeId};

// =============================================================================
// True Async Stdio Transport (using tokio::process)
// =============================================================================

/// Message sent to the writer task
struct WriteRequest {
    request_line: String,
}

/// True async stdio transport using tokio::process.
/// 
/// This implementation:
/// - Uses `tokio::process::Command` for async process spawning
/// - Separate tasks for reading stdout and writing to stdin
/// - No mutex held during I/O, enabling true concurrent requests
/// - Pending request map for matching responses to requests
pub struct TokioStdioTransport {
    /// Sender for write requests
    write_tx: mpsc::Sender<WriteRequest>,
    /// Pending requests: request ID -> response sender
    pending: Arc<DashMap<i64, oneshot::Sender<Result<Value, McpTransportError>>>>,
    /// Next request ID
    next_id: AtomicI64,
    /// Whether the transport is alive
    alive: Arc<AtomicBool>,
    /// Handle to the child process (for cleanup)
    child: Arc<tokio::sync::Mutex<Child>>,
}

impl TokioStdioTransport {
    /// Spawn a new MCP server process with true async I/O.
    pub async fn spawn(command: &str, args: &[String]) -> Result<Self, McpTransportError> {
        Self::spawn_with_env(command, args, HashMap::new()).await
    }

    /// Spawn with environment variables.
    pub async fn spawn_with_env(
        command: &str,
        args: &[String],
        env: HashMap<String, String>,
    ) -> Result<Self, McpTransportError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        for (key, value) in env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| {
            McpTransportError::TransportError(format!(
                "Failed to spawn process '{}': {}",
                command, e
            ))
        })?;

        // Take ownership of stdin and stdout
        let stdin = child.stdin.take().ok_or_else(|| {
            McpTransportError::TransportError("Failed to get stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            McpTransportError::TransportError("Failed to get stdout".to_string())
        })?;

        let alive = Arc::new(AtomicBool::new(true));
        let pending: Arc<DashMap<i64, oneshot::Sender<Result<Value, McpTransportError>>>> =
            Arc::new(DashMap::new());

        // Channel for write requests
        let (write_tx, mut write_rx) = mpsc::channel::<WriteRequest>(256);

        // Spawn writer task
        let alive_writer = Arc::clone(&alive);
        let mut stdin = stdin;
        tokio::spawn(async move {
            while let Some(req) = write_rx.recv().await {
                if !alive_writer.load(Ordering::SeqCst) {
                    break;
                }
                if let Err(e) = stdin.write_all(req.request_line.as_bytes()).await {
                    eprintln!("Stdio write error: {}", e);
                    alive_writer.store(false, Ordering::SeqCst);
                    break;
                }
                if let Err(e) = stdin.flush().await {
                    eprintln!("Stdio flush error: {}", e);
                    alive_writer.store(false, Ordering::SeqCst);
                    break;
                }
            }
        });

        // Spawn reader task
        let pending_reader = Arc::clone(&pending);
        let alive_reader = Arc::clone(&alive);
        let mut reader = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        // EOF - process closed
                        alive_reader.store(false, Ordering::SeqCst);
                        break;
                    }
                    Ok(_) => {
                        // Parse response
                        match serde_json::from_str::<JsonRpcResponse>(&line) {
                            Ok(response) => {
                                if let JsonRpcId::Number(id) = &response.id {
                                    if let Some((_, tx)) = pending_reader.remove(id) {
                                        let result = match response.payload {
                                            JsonRpcPayload::Success { result } => Ok(result),
                                            JsonRpcPayload::Error { error } => {
                                                Err(McpTransportError::ServerError(format!(
                                                    "MCP Error: {}",
                                                    error
                                                )))
                                            }
                                        };
                                        let _ = tx.send(result);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to parse response: {} - line: {}", e, line.trim());
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Stdio read error: {}", e);
                        alive_reader.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            }
            
            // Clean up pending requests on shutdown - receivers will get
            // a channel closed error when the senders are dropped
            pending_reader.clear();
        });

        Ok(Self {
            write_tx,
            pending,
            next_id: AtomicI64::new(1),
            alive,
            child: Arc::new(tokio::sync::Mutex::new(child)),
        })
    }

    /// Send a request and wait for response with timeout.
    pub async fn send_request(
        &self,
        method: &str,
        params: Option<Value>,
        timeout_duration: Duration,
    ) -> Result<Value, McpTransportError> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err(McpTransportError::ConnectionClosed);
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest::new(JsonRpcId::Number(id), method.to_string(), params);
        let request_json = serde_json::to_string(&request)?;
        let request_line = format!("{}\n", request_json);

        // Create response channel and register pending request
        let (tx, rx) = oneshot::channel();
        self.pending.insert(id, tx);

        // Send write request
        if self.write_tx.send(WriteRequest { request_line }).await.is_err() {
            self.pending.remove(&id);
            return Err(McpTransportError::ConnectionClosed);
        }

        // Wait for response with timeout
        match tokio::time::timeout(timeout_duration, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                self.pending.remove(&id);
                Err(McpTransportError::ConnectionClosed)
            }
            Err(_) => {
                self.pending.remove(&id);
                Err(McpTransportError::Timeout(format!(
                    "Request timed out after {:?}",
                    timeout_duration
                )))
            }
        }
    }

    /// Check if the transport is alive.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Stop the transport and kill the process.
    pub async fn stop(&self) -> Result<(), McpTransportError> {
        self.alive.store(false, Ordering::SeqCst);
        
        // Kill the child process
        let mut child = self.child.lock().await;
        if let Err(e) = child.kill().await {
            // Process may have already exited
            if e.kind() != std::io::ErrorKind::InvalidInput {
                return Err(McpTransportError::TransportError(format!(
                    "Failed to kill process: {}",
                    e
                )));
            }
        }
        
        Ok(())
    }
}

// =============================================================================
// Async Stdio Transport (legacy wrapper, now uses TokioStdioTransport)
// =============================================================================

/// Async-friendly stdio transport with timeout support.
/// 
/// This is now a wrapper around `TokioStdioTransport` for backwards compatibility.
pub struct AsyncStdioTransport {
    inner: Arc<TokioStdioTransport>,
}

impl AsyncStdioTransport {
    /// Spawn a new MCP server process.
    pub async fn spawn(command: &str, args: &[String]) -> Result<Self, McpTransportError> {
        Ok(Self {
            inner: Arc::new(TokioStdioTransport::spawn(command, args).await?),
        })
    }

    /// Spawn with environment variables.
    pub async fn spawn_with_env(
        command: &str,
        args: &[String],
        env: HashMap<String, String>,
    ) -> Result<Self, McpTransportError> {
        Ok(Self {
            inner: Arc::new(TokioStdioTransport::spawn_with_env(command, args, env).await?),
        })
    }

    /// Send a request with a timeout.
    pub async fn send_request_with_timeout(
        &self,
        method: &str,
        params: Option<Value>,
        timeout_duration: Duration,
    ) -> Result<Value, McpTransportError> {
        self.inner.send_request(method, params, timeout_duration).await
    }

    /// Check if alive.
    pub fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }

    /// Stop the transport.
    pub async fn stop(&self) -> Result<(), McpTransportError> {
        self.inner.stop().await
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
        Self::connect_with_env(command, args, HashMap::new(), config, timeout).await
    }

    /// Create and initialize with environment variables.
    pub async fn connect_with_env(
        command: &str,
        args: &[String],
        env: HashMap<String, String>,
        config: Option<Value>,
        timeout: Duration,
    ) -> Result<Self, McpTransportError> {
        let inner = AsyncStdioTransport::spawn_with_env(command, args, env).await?;

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

        // Send initialized notification
        let _ = adapter
            .inner
            .send_request_with_timeout(
                "notifications/initialized",
                Some(serde_json::json!({})),
                adapter.timeout,
            )
            .await;

        Ok(adapter)
    }
}

#[async_trait]
impl McpTransport for StdioTransportAdapter {
    async fn list_tools(&self) -> Result<Vec<McpToolDefinition>, McpTransportError> {
        let result = self
            .inner
            .send_request_with_timeout("tools/list", Some(serde_json::json!({})), self.timeout)
            .await?;

        let list_result: ListToolsResult = serde_json::from_value(result)?;
        Ok(list_result.tools)
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpTransportError> {
        let params = CallToolParams {
            name: name.to_string(),
            arguments: Some(args),
            task: None,
            meta: None,
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
        self.inner.stop().await
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

    #[test]
    fn test_transport_type() {
        assert_eq!(TransportTypeId::Stdio.to_string(), "stdio");
    }
}
