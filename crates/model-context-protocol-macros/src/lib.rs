//! Procedural macros for MCP server and tool definitions.
//!
//! This crate provides macros to reduce boilerplate when defining MCP servers:
//!
//! - `#[mcp_server]` - Define server metadata and collect tools
//! - `#[mcp_tool]` - Mark a method as an MCP tool
//!
//! # Example
//!
//! ```rust,ignore
//! use mcp_macros::{mcp_server, mcp_tool};
//!
//! #[mcp_server(name = "memory", version = "1.0.0")]
//! pub struct MemoryServer {
//!     store: MemoryStore,
//! }
//!
//! #[mcp_server]
//! impl MemoryServer {
//!     #[mcp_tool(description = "Store a value in memory")]
//!     pub fn memory_write(&self, scope: String, key: String, value: Value) -> ToolResult<String> {
//!         self.store.write(&scope, &key, value)
//!     }
//! }
//! ```

use proc_macro::TokenStream;

mod schema;
mod server;
mod tool;

/// Marks a struct as an MCP server or an impl block as containing MCP tools.
///
/// # On Structs
///
/// ```rust,ignore
/// #[mcp_server(name = "my-server", version = "1.0.0")]
/// pub struct MyServer { ... }
/// ```
///
/// # On Impl Blocks
///
/// ```rust,ignore
/// #[mcp_server]
/// impl MyServer {
///     #[mcp_tool(description = "...")]
///     pub fn my_tool(&self, ...) -> ToolResult<T> { ... }
/// }
/// ```
#[proc_macro_attribute]
pub fn mcp_server(attr: TokenStream, item: TokenStream) -> TokenStream {
    server::mcp_server_impl(attr.into(), item.into()).into()
}

/// Marks a method as an MCP tool.
///
/// Parameter descriptions are extracted from doc comments on parameters.
///
/// # Example
///
/// ```rust,ignore
/// #[mcp_tool(description = "Store a value in memory")]
/// pub fn memory_write(
///     &self,
///     /// The scope/namespace for the memory
///     scope: String,
///     /// The key to store the value under
///     key: String,
///     /// The value to store
///     value: Value,
/// ) -> ToolResult<String> {
///     self.store.write(&scope, &key, value)
/// }
/// ```
#[proc_macro_attribute]
pub fn mcp_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    tool::mcp_tool_impl(attr.into(), item.into()).into()
}
