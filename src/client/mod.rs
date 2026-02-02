//! MCP Client transports for connecting to external MCP servers.
//!
//! This module provides client-side transports that implement the `McpTransport`
//! trait for connecting to and communicating with external MCP servers.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                      Your Application                           │
//! │                                                                 │
//! │  ┌───────────────────────────────────────────────────────────┐ │
//! │  │                        McpHub                              │ │
//! │  │              (manages multiple connections)                │ │
//! │  │                                                            │ │
//! │  │   ┌─────────────────┐       ┌─────────────────────────┐   │ │
//! │  │   │ StdioTransport  │       │    HttpTransport        │   │ │
//! │  │   │ (spawn process) │       │   (HTTP POST)           │   │ │
//! │  │   └─────────────────┘       └─────────────────────────┘   │ │
//! │  └───────────────────────────────────────────────────────────┘ │
//! │                            │                                    │
//! └────────────────────────────┼────────────────────────────────────┘
//!                              │
//!                              ▼
//!              ┌───────────────────────────────┐
//!              │     External MCP Servers      │
//!              │  (Node.js, Python, etc.)      │
//!              └───────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use mcp::client::StdioTransportAdapter;
//! use mcp::transport::McpTransport;
//!
//! // Connect to an MCP server process
//! let transport = StdioTransportAdapter::connect(
//!     "node",
//!     &["mcp-server.js".to_string()],
//!     None,
//!     Duration::from_secs(30),
//! ).await?;
//!
//! // List available tools
//! let tools = transport.list_tools().await?;
//!
//! // Call a tool
//! let result = transport.call_tool("my_tool", json!({"arg": "value"})).await?;
//! ```

pub mod http;
pub mod stdio;

// Re-exports
pub use stdio::{AsyncStdioTransport, StdioTransportAdapter, TokioStdioTransport};

pub use http::{HttpTransport, HttpTransportAdapter};
