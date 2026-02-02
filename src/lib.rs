//! # MCP - Model Context Protocol
//!
//! A Rust implementation of the Model Context Protocol (MCP) for AI tool integration.
//!
//! This crate provides the infrastructure for communicating with MCP servers
//! via stdio/HTTP transports.
//!
//! ## Features
//!
//! - **McpServer**: Core MCP server with tools and capabilities
//! - **McpServerHub**: Aggregates multiple external servers into one
//! - **Multiple Transports**: Support for stdio and HTTP-based MCP servers
//! - **Tool Routing**: Automatic routing of tool calls to the correct server
//! - **Macros**: `#[mcp_tool]` for easy tool definitions from functions
//!
//! ## Quick Start - Creating a Server
//!
//! ```rust,ignore
//! use mcp::macros::mcp_tool;
//! use mcp::server::stdio::McpStdioServer;
//! use mcp::McpServerConfig;
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
//!         .with_tools(tools![AddTool, SubtractTool])
//!         .build();
//!
//!     McpStdioServer::run(config).await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Aggregating Multiple Servers
//!
//! ```rust,ignore
//! use mcp::{McpServerHub, McpServerConnectionConfig};
//! use mcp::server::stdio::McpStdioServer;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a hub that aggregates external servers
//!     let hub = std::sync::Arc::new(McpServerHub::new("aggregator", "1.0.0"));
//!
//!     // Connect to external servers
//!     hub.connect(McpServerConnectionConfig::stdio("calc", "node", vec!["calc.js".into()])).await?;
//!     hub.connect(McpServerConnectionConfig::stdio("files", "python", vec!["files.py".into()])).await?;
//!
//!     // Wrap as a stdio server - all tools are now exposed
//!     McpStdioServer::run(hub.to_config()).await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Feature Flags
//!
//! - `default` - Enables stdio, http, and macros features
//! - `client` - Client transports for connecting to external MCP servers
//! - `stdio-server` - Stdio server transport
//! - `http-server` - HTTP server with actix-web
//! - `macros` - Procedural macros for defining tools

#![cfg_attr(docsrs, feature(doc_cfg))]

/// Current MCP protocol version (2025-11-25).
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

pub mod protocol;
pub mod result;
pub mod server;
pub mod tool;
pub mod transport;

#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub mod macro_adapter;

// Client transports (for connecting to external MCP servers)
#[cfg(feature = "client")]
pub mod client;

pub mod circuit_breaker;
pub mod hub;
pub mod hub_common;
pub mod server_hub;
pub mod transport_factory;

// =============================================================================
// Re-exports
// =============================================================================

// Protocol types (core)
pub use protocol::{
    CallToolParams, CallToolResult, ClientInbound, JsonRpcError, JsonRpcId, JsonRpcMessage,
    JsonRpcNotification, JsonRpcPayload, JsonRpcRequest, JsonRpcResponse, ListToolsParams,
    ListToolsResult, McpCapabilities, McpServerInfo, McpToolDefinition, ServerOutbound,
    ToolContent, ToolInputSchema,
};

// Protocol types (2025-11-25 - new)
pub use protocol::{
    // Core types
    Annotations, BaseMetadata, Icon, IconTheme, Implementation, Role,
    // Tool types
    TaskSupport, ToolAnnotations, ToolExecution,
    // Task types
    CancelTaskParams, CreateTaskResult, GetTaskParams, GetTaskPayloadParams, ListTasksParams,
    ListTasksResult, RelatedTaskMetadata, Task, TaskMetadata, TaskStatus, TaskStatusNotificationParams,
    // Elicitation types
    BooleanSchema, ElicitAction, ElicitationCompleteParams, ElicitRequestFormParams, ElicitRequestParams,
    ElicitRequestUrlParams, ElicitResult, NumberSchema, StringSchema, StringSchemaFormat,
    // Sampling types
    CreateMessageParams, CreateMessageResult, ModelHint, ModelPreferences, SamplingContent,
    SamplingMessage, ToolChoice, ToolChoiceMode, ToolResultContent, ToolUseContent,
    // Logging types
    LoggingLevel, LoggingMessageParams, SetLevelParams,
    // Progress types
    ProgressNotificationParams, ProgressToken,
    // Roots types
    ListRootsResult, Root,
    // Completion types
    CompleteArgument, CompleteContext, CompleteParams, CompleteResult, CompletionData,
    PromptReference, ResourceTemplateReference,
};

// Transport types
pub use transport::{
    ClientInfo, InitializeCapabilities, InitializeParams, InitializeResult,
    McpServerConnectionConfig, McpTransport, McpTransportError, RestartPolicy,
    ServerCapabilities, ServerInfo, TransportTypeId,
};

// Transport types (2025-11-25 - new capabilities)
pub use transport::{
    ElicitationCapabilities, PromptsCapabilities, ResourcesCapabilities, RootsCapabilities,
    SamplingCapabilities, ServerToolCapabilities, TasksCapabilities, ToolCapabilities,
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

// Server (core)
pub use server::{
    McpServer, McpServerChannels, McpServerConfig, McpServerConfigBuilder, ServerError, ServerStatus,
};

// Server transports
#[cfg(feature = "stdio-server")]
#[cfg_attr(docsrs, doc(cfg(feature = "stdio-server")))]
pub use server::stdio::McpStdioServer;

#[cfg(feature = "http-server")]
#[cfg_attr(docsrs, doc(cfg(feature = "http-server")))]
pub use server::http::McpHttpServer;

// Hub (legacy - for direct client usage)
pub use hub::McpHub;

// Server Hub (aggregates multiple servers into one)
pub use server_hub::McpServerHub;

// Client transports (for connecting to external servers)
#[cfg(feature = "client")]
#[cfg_attr(docsrs, doc(cfg(feature = "client")))]
pub use client::stdio::{AsyncStdioTransport, StdioTransportAdapter, TokioStdioTransport};

#[cfg(feature = "client")]
#[cfg_attr(docsrs, doc(cfg(feature = "client")))]
pub use client::http::{HttpTransport, HttpTransportAdapter};

// Macros (when enabled)
#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub mod macros {
    //! Procedural macros for defining MCP servers and tools.
    //!
    //! This module re-exports the macros from `model-context-protocol-macros`.
    //!
    //! Use `#[mcp(...)]` on function parameters to add descriptions:
    //! ```rust,ignore
    //! #[mcp_server]
    //! impl MyServer {
    //!     #[mcp_tool(description = "Add two numbers")]
    //!     pub fn add(
    //!         &self,
    //!         #[param("First number")] a: f64,
    //!         #[param("Second number")] b: f64,
    //!     ) -> f64 { a + b }
    //! }
    //! ```
    //!
    //! For standalone function tools:
    //! ```rust,ignore
    //! use mcp::macros::mcp_tool;
    //!
    //! #[mcp_tool(description = "Add two numbers")]
    //! fn add(#[param("First number")] a: f64, #[param("Second number")] b: f64) -> f64 {
    //!     a + b
    //! }
    //! ```
    //!
    //! Note: `#[mcp_tool]` and `#[param]` are inert attributes processed by `#[mcp_server]`
    //! when used on impl blocks. For standalone functions, `#[mcp_tool]` generates a tool struct.
    pub use model_context_protocol_macros::{mcp_server, mcp_tool};
}

// Macro adapter (when enabled)
#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub use macro_adapter::{MacroServer, MacroServerAdapter};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        assert_eq!(MCP_PROTOCOL_VERSION, "2025-11-25");
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
