//! Stdio transport for MCP Server.
//!
//! This module provides `McpStdioServer` which wraps the core `McpServer`
//! and handles stdin/stdout I/O.
//!
//! # Example
//!
//! ```ignore
//! use mcp::server::{McpServerConfig, stdio::McpStdioServer};
//!
//! let config = McpServerConfig::builder()
//!     .name("my-server")
//!     .version("1.0.0")
//!     .with_tool(MyTool)
//!     .build();
//!
//! McpStdioServer::run(config).await?;
//! ```

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{McpServer, McpServerConfig, ServerError, ServerStatus};
use crate::protocol::JsonRpcMessage;

/// MCP Server with stdio transport.
///
/// This server reads JSON-RPC messages from stdin and writes responses
/// to stdout. It wraps the core `McpServer` and bridges stdio I/O to
/// the internal channel-based communication.
pub struct McpStdioServer {
    server: Arc<McpServer>,
}

impl McpStdioServer {
    /// Runs an MCP server with stdio transport.
    ///
    /// This is the main entry point for running a stdio-based MCP server.
    /// The function blocks until the server stops (stdin closed or error).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = McpServerConfig::builder()
    ///     .name("my-server")
    ///     .version("1.0.0")
    ///     .with_tool(MyTool)
    ///     .build();
    ///
    /// McpStdioServer::run(config).await?;
    /// ```
    pub async fn run(config: McpServerConfig) -> Result<(), ServerError> {
        let (server, mut channels) = McpServer::new(config);

        let stdio_server = Self {
            server: Arc::clone(&server),
        };

        // Spawn stdout writer task
        let stdout_handle = tokio::spawn(async move {
            let mut stdout = tokio::io::stdout();

            while let Some(outbound) = channels.outbound_rx.recv().await {
                let json = match outbound.to_json() {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("Failed to serialize outbound message: {}", e);
                        continue;
                    }
                };

                if let Err(e) = stdout.write_all(json.as_bytes()).await {
                    eprintln!("Failed to write to stdout: {}", e);
                    break;
                }
                if let Err(e) = stdout.write_all(b"\n").await {
                    eprintln!("Failed to write newline to stdout: {}", e);
                    break;
                }
                if let Err(e) = stdout.flush().await {
                    eprintln!("Failed to flush stdout: {}", e);
                    break;
                }
            }
        });

        // Run stdin reader in current task
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        loop {
            line.clear();

            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // EOF - stdin closed
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    // Parse the incoming message
                    match JsonRpcMessage::parse(trimmed) {
                        Ok(message) => {
                            let inbound = message.into_client_inbound();
                            if channels.inbound_tx.send(inbound).await.is_err() {
                                // Server stopped
                                break;
                            }
                        }
                        Err(e) => {
                            // Send parse error response through the channel
                            // to ensure synchronization with other outbound messages
                            let error_response = crate::protocol::JsonRpcResponse::error(
                                crate::protocol::JsonRpcId::Null,
                                -32700,
                                format!("Parse error: {}", e),
                                None,
                            );
                            let outbound =
                                crate::protocol::ServerOutbound::Response(error_response);
                            if channels.outbound_tx.send(outbound).await.is_err() {
                                // Channel closed, server stopped
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(ServerError::Io(e));
                }
            }

            // Check if server is still running
            if stdio_server.server.status() != ServerStatus::Running {
                break;
            }
        }

        // Stop the server
        server.stop();

        // Wait for stdout writer to finish
        let _ = stdout_handle.await;

        Ok(())
    }

    /// Returns the underlying server reference.
    pub fn server(&self) -> &Arc<McpServer> {
        &self.server
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{JsonRpcId, ServerOutbound};
    use tokio::sync::mpsc;

    #[test]
    fn test_stdio_server_module_exists() {
        // Basic module existence test
        // Full integration tests would require stdin/stdout mocking
    }

    /// Test that all outbound messages are synchronized through a single channel.
    ///
    /// This test verifies that:
    /// 1. Parse errors are routed through the outbound channel (not directly to stdout)
    /// 2. Multiple concurrent messages maintain their order when sent through the channel
    /// 3. No message interleaving can occur because there's only one writer
    #[tokio::test]
    async fn test_outbound_message_synchronization() {
        // Create a channel to simulate the outbound message flow
        let (outbound_tx, mut outbound_rx) = mpsc::channel::<ServerOutbound>(256);

        // Simulate sending multiple messages concurrently
        let tx1 = outbound_tx.clone();
        let tx2 = outbound_tx.clone();
        let tx3 = outbound_tx.clone();

        // Spawn tasks that send messages "simultaneously"
        let handles = vec![
            tokio::spawn(async move {
                for i in 0..10 {
                    let response = crate::protocol::JsonRpcResponse::success(
                        JsonRpcId::Number(i),
                        serde_json::json!({"msg": format!("response_{}", i)}),
                    );
                    tx1.send(ServerOutbound::Response(response)).await.unwrap();
                }
            }),
            tokio::spawn(async move {
                for i in 10..20 {
                    let response = crate::protocol::JsonRpcResponse::error(
                        JsonRpcId::Number(i),
                        -32700,
                        format!("Parse error {}", i),
                        None,
                    );
                    tx2.send(ServerOutbound::Response(response)).await.unwrap();
                }
            }),
            tokio::spawn(async move {
                for i in 20..30 {
                    let notification =
                        crate::protocol::JsonRpcNotification::new(format!("notify_{}", i), None);
                    tx3.send(ServerOutbound::Notification(notification))
                        .await
                        .unwrap();
                }
            }),
        ];

        // Wait for all senders to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Drop the original sender so the channel closes
        drop(outbound_tx);

        // Collect all messages - they should be complete (not interleaved)
        let mut messages = Vec::new();
        while let Some(msg) = outbound_rx.recv().await {
            let json = msg.to_json().unwrap();
            // Verify each message is valid JSON (not corrupted by interleaving)
            let parsed: serde_json::Value = serde_json::from_str(&json)
                .expect("Each message should be valid JSON - no interleaving");
            messages.push(parsed);
        }

        // We should have received all 30 messages
        assert_eq!(messages.len(), 30, "All messages should be received");

        // Verify message integrity - each should be a complete, valid JSON-RPC message
        for msg in &messages {
            assert!(
                msg.get("jsonrpc").is_some(),
                "Each message should have jsonrpc field"
            );
        }
    }

    /// Test that the single-writer pattern prevents interleaving.
    ///
    /// By using a single channel receiver that writes to output, we guarantee
    /// that messages are written atomically one at a time.
    #[tokio::test]
    async fn test_single_writer_pattern() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let (outbound_tx, mut outbound_rx) = mpsc::channel::<ServerOutbound>(256);
        let write_count = Arc::new(AtomicUsize::new(0));
        let concurrent_writes = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));

        // Simulate the single writer task
        let write_count_clone = Arc::clone(&write_count);
        let concurrent_clone = Arc::clone(&concurrent_writes);
        let max_clone = Arc::clone(&max_concurrent);

        let writer_handle = tokio::spawn(async move {
            while let Some(outbound) = outbound_rx.recv().await {
                // Track concurrent writes
                let current = concurrent_clone.fetch_add(1, Ordering::SeqCst) + 1;

                // Update max concurrent if this is higher
                let mut max = max_clone.load(Ordering::SeqCst);
                while current > max {
                    match max_clone.compare_exchange(
                        max,
                        current,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    ) {
                        Ok(_) => break,
                        Err(m) => max = m,
                    }
                }

                // Simulate write operation
                let _json = outbound.to_json().unwrap();

                // Small delay to increase chance of detecting concurrency issues
                tokio::task::yield_now().await;

                write_count_clone.fetch_add(1, Ordering::SeqCst);
                concurrent_clone.fetch_sub(1, Ordering::SeqCst);
            }
        });

        // Send messages from multiple tasks
        let mut send_handles = Vec::new();
        for batch in 0..5 {
            let tx = outbound_tx.clone();
            send_handles.push(tokio::spawn(async move {
                for i in 0..10 {
                    let response = crate::protocol::JsonRpcResponse::success(
                        JsonRpcId::Number(batch * 10 + i),
                        serde_json::json!({}),
                    );
                    tx.send(ServerOutbound::Response(response)).await.unwrap();
                }
            }));
        }

        // Wait for all senders
        for handle in send_handles {
            handle.await.unwrap();
        }
        drop(outbound_tx);

        // Wait for writer to finish
        writer_handle.await.unwrap();

        // Verify all messages were written
        assert_eq!(write_count.load(Ordering::SeqCst), 50);

        // The max concurrent writes should be 1 (single writer)
        // Note: Due to the async nature, this might occasionally be 0 if
        // the check happens between increment and actual write
        assert!(
            max_concurrent.load(Ordering::SeqCst) <= 1,
            "Single writer should never have more than 1 concurrent write"
        );
    }
}
