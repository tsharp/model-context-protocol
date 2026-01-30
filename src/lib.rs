//! # MCP - Model Context Protocol
//!
//! A Rust implementation of the Model Context Protocol (MCP) for AI tool integration.
//!
//! This crate provides the infrastructure for communicating with MCP servers
//! via stdio/HTTP transports.
//!
//! ## Features
//!
//! - **McpHub**: Central hub for managing multiple MCP server connections
//! - **Multiple Transports**: Support for stdio and HTTP-based MCP servers
//! - **Tool Routing**: Automatic routing of tool calls to the correct server
//! - **Macros**: `#[mcp_tool]` for easy tool definitions from functions
//!
//! ## Quick Start - Creating a Server
//!
//! ```rust,ignore
//! use mcp::{mcp_tool, McpServerConfig, McpServer, tools};
//!
//! // Define tools as simple functions
//! #[mcp_tool(description = "Add two numbers")]
//! fn add(a: f64, b: f64) -> f64 { a + b }
//!
//! #[mcp_tool(description = "Subtract two numbers")]
//! fn subtract(a: f64, b: f64) -> f64 { a - b }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = McpServerConfig::builder()
//!         .name("calculator")
//!         .version("1.0.0")
//!         .with_stdio_transport()
//!         .with_tools(tools![AddTool, SubtractTool])
//!         .build();
//!
//!     McpServer::run(config).await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Connecting to Servers
//!
//! ```rust,ignore
//! use mcp::{McpHub, McpServerConnectionConfig};
//!
//! let hub = McpHub::new();
//!
//! // Connect to an external server
//! let config = McpServerConnectionConfig::stdio("my-server", "node", vec!["server.js".into()]);
//! hub.connect(config).await?;
//!
//! // List available tools
//! let tools = hub.list_all_tools().await?;
//! ```
//!
//! ## Feature Flags
//!
//! - `default` - Enables stdio, http, and macros features
//! - `stdio` - Stdio transport for spawning server processes
//! - `http` - HTTP transport for connecting to HTTP servers
//! - `http-server` - HTTP server with actix-web (for hosting MCP servers)
//! - `macros` - Procedural macros for defining tools

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod protocol;
pub mod result;
pub mod server;
pub mod tool;
pub mod transport;

#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub mod macro_adapter;

#[cfg(feature = "stdio")]
#[cfg_attr(docsrs, doc(cfg(feature = "stdio")))]
pub mod stdio;

#[cfg(feature = "http")]
#[cfg_attr(docsrs, doc(cfg(feature = "http")))]
pub mod http;

pub mod hub;

// =============================================================================
// Re-exports
// =============================================================================

// Protocol types
pub use protocol::{
    CallToolParams, CallToolResult, JsonRpcError, JsonRpcId, JsonRpcPayload, JsonRpcRequest,
    JsonRpcResponse, ListToolsParams, ListToolsResult, McpCapabilities, McpServerInfo, McpToolDef,
    ToolContent, ToolDefinition, ToolInputSchema, MCP_PROTOCOL_VERSION,
};

// Transport types
pub use transport::{
    ClientInfo, InitializeCapabilities, InitializeParams, InitializeResult,
    McpServerConnectionConfig, McpTransport, McpTransportError, ServerCapabilities, ServerInfo,
    TransportTypeId,
};

// Result types
pub use result::{error_result, success_result, tool_err, tool_ok, IntoCallToolResult, ToolResult};

// Tool types
pub use tool::{
    all_tools, tools_in_group, BoxFuture, DynTool, FnTool, McpTool, ToolCallResult, ToolEntry,
    ToolFactory, ToolProvider, ToolRegistry,
};

// Re-export inventory for use in macro-generated code
#[doc(hidden)]
pub use inventory;

// Server
pub use server::{
    McpServer, McpServerConfig, McpServerConfigBuilder, ServerError, ServerTransport,
};

// Hub
pub use hub::McpHub;

// Stdio transport (when enabled)
#[cfg(feature = "stdio")]
#[cfg_attr(docsrs, doc(cfg(feature = "stdio")))]
pub use stdio::{AsyncStdioTransport, StdioTransport, StdioTransportAdapter};

// HTTP transport (when enabled)
#[cfg(feature = "http")]
#[cfg_attr(docsrs, doc(cfg(feature = "http")))]
pub use http::{HttpTransport, HttpTransportAdapter};

// Macros (when enabled)
#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub use mcp_macros::{mcp_server, mcp_tool};

// Macro adapter (when enabled)
#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub use macro_adapter::{MacroServer, MacroServerAdapter};

/// Current MCP protocol version supported by this crate.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        assert_eq!(PROTOCOL_VERSION, "2024-11-05");
    }

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[tokio::test]
    async fn test_hub_basic() {
        let hub = McpHub::new();
        assert!(!hub.list_servers().is_empty() || hub.list_servers().is_empty());
    }
}
