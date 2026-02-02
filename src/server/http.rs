//! HTTP transport for MCP Server.
//!
//! This module provides `McpHttpServer` which wraps the core `McpServer`
//! and handles HTTP/SSE I/O using actix-web.
//!
//! # Example
//!
//! ```ignore
//! use mcp::server::{McpServerConfig, http::McpHttpServer};
//!
//! let config = McpServerConfig::builder()
//!     .name("my-server")
//!     .version("1.0.0")
//!     .with_tool(MyTool)
//!     .build();
//!
//! McpHttpServer::run(config, "127.0.0.1", 8080).await?;
//! ```

use std::sync::Arc;

use actix_web::{web, App, HttpResponse, HttpServer};
use tokio::sync::mpsc;

use super::{McpServer, McpServerConfig, ServerError};
use crate::protocol::{ClientInbound, JsonRpcMessage, JsonRpcResponse, JsonRpcId, ServerOutbound};

/// Application state shared across HTTP handlers.
struct AppState {
    inbound_tx: mpsc::Sender<ClientInbound>,
    server: Arc<McpServer>,
}

/// MCP Server with HTTP transport.
///
/// This server exposes HTTP endpoints for JSON-RPC communication and
/// optionally SSE for server-to-client streaming. It wraps the core
/// `McpServer` and bridges HTTP I/O to the internal channel-based
/// communication.
pub struct McpHttpServer;

impl McpHttpServer {
    /// Runs an MCP server with HTTP transport.
    ///
    /// This starts an HTTP server on the specified host and port.
    /// The function blocks until the server stops.
    ///
    /// # Endpoints
    ///
    /// - `POST /rpc` - JSON-RPC endpoint for all MCP methods
    /// - `GET /tools` - List available tools
    /// - `POST /call` - Direct tool call endpoint
    /// - `GET /sse` - Server-Sent Events for server-to-client notifications
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
    /// McpHttpServer::run(config, "127.0.0.1", 8080).await?;
    /// ```
    pub async fn run(config: McpServerConfig, host: &str, port: u16) -> Result<(), ServerError> {
        let (server, mut channels) = McpServer::new(config);
        let inbound_tx = channels.inbound_tx.clone();

        // Spawn a task to handle outbound messages (for SSE or logging)
        let _outbound_handle = tokio::spawn(async move {
            while let Some(outbound) = channels.outbound_rx.recv().await {
                // For now, just log outbound notifications
                // SSE streaming could be implemented here
                match &outbound {
                    ServerOutbound::Notification(n) => {
                        eprintln!("[MCP] Notification: {}", n.method);
                    }
                    ServerOutbound::Request(r) => {
                        eprintln!("[MCP] Server request: {}", r.method);
                    }
                    _ => {}
                }
            }
        });

        let state = web::Data::new(AppState {
            inbound_tx,
            server: Arc::clone(&server),
        });

        HttpServer::new(move || {
            let state = state.clone();

            App::new()
                .app_data(state)
                .route("/rpc", web::post().to(handle_rpc))
                .route("/tools", web::get().to(handle_tools_list))
                .route("/call", web::post().to(handle_tool_call))
                .route("/health", web::get().to(handle_health))
        })
        .bind((host, port))
        .map_err(|e| ServerError::Io(std::io::Error::new(std::io::ErrorKind::AddrInUse, e)))?
        .run()
        .await
        .map_err(|e| ServerError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
    }
}

/// Handles JSON-RPC requests.
async fn handle_rpc(state: web::Data<AppState>, body: String) -> HttpResponse {
    let message = match JsonRpcMessage::parse(&body) {
        Ok(m) => m,
        Err(e) => {
            let error_response = JsonRpcResponse::error(
                JsonRpcId::Null,
                -32700,
                format!("Parse error: {}", e),
                None,
            );
            return HttpResponse::Ok().json(error_response);
        }
    };

    // For HTTP, we need to handle request/response synchronously
    // since HTTP doesn't maintain a persistent connection
    match message {
        JsonRpcMessage::Request(request) => {
            // Call the server directly for HTTP since it's stateless
            let response = handle_request_directly(&state.server, request).await;
            HttpResponse::Ok().json(response)
        }
        JsonRpcMessage::Notification(notification) => {
            // Handle notification
            let inbound = ClientInbound::Notification(notification);
            let _ = state.inbound_tx.send(inbound).await;
            HttpResponse::NoContent().finish()
        }
        JsonRpcMessage::Response(_) => {
            HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Unexpected response message"
            }))
        }
    }
}

/// Handles a request directly using the server.
async fn handle_request_directly(
    server: &McpServer,
    request: crate::protocol::JsonRpcRequest,
) -> JsonRpcResponse {
    match request.method.as_str() {
        "initialize" => {
            JsonRpcResponse::success(
                request.id,
                serde_json::json!({
                    "protocolVersion": crate::protocol::MCP_PROTOCOL_VERSION,
                    "serverInfo": server.server_info(),
                    "capabilities": {} // TODO: expose capabilities
                }),
            )
        }
        "tools/list" => {
            let tools = server.list_tools();
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

            let name = match params.get("name").and_then(|n| n.as_str()) {
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

            let result = server.call_tool(name, arguments).await;

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

/// Handles GET /tools endpoint.
async fn handle_tools_list(state: web::Data<AppState>) -> HttpResponse {
    let tools = state.server.list_tools();
    HttpResponse::Ok().json(tools)
}

/// Request body for the /call endpoint.
#[derive(serde::Deserialize)]
struct CallToolRequest {
    name: String,
    arguments: serde_json::Value,
}

/// Handles POST /call endpoint.
async fn handle_tool_call(
    state: web::Data<AppState>,
    body: web::Json<CallToolRequest>,
) -> HttpResponse {
    let result = state.server.call_tool(&body.name, body.arguments.clone()).await;

    match result {
        Ok(content) => HttpResponse::Ok().json(content),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

/// Handles GET /health endpoint.
async fn handle_health(state: web::Data<AppState>) -> HttpResponse {
    let status = state.server.status();
    HttpResponse::Ok().json(serde_json::json!({
        "status": format!("{:?}", status),
        "name": state.server.name(),
        "version": state.server.version()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_server_module_exists() {
        // Basic module existence test
        // Full integration tests would require actix-web test utilities
    }
}
