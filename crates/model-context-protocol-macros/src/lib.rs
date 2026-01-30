//! Procedural macros for MCP server and tool definitions.
//!
//! This crate provides macros to reduce boilerplate when defining MCP servers:
//!
//! - `#[mcp_server]` - Define server metadata and collect tools from impl blocks
//! - `#[mcp_tool]` - Mark a method as an MCP tool (used within `#[mcp_server]` impl blocks)
//! - `#[param(...)]` - Mark a parameter with description for the tool schema
//!
//! # Example
//!
//! ```rust,ignore
//! use mcp::macros::mcp_server;
//!
//! #[mcp_server(name = "calculator", version = "1.0.0")]
//! pub struct Calculator;
//!
//! #[mcp_server]
//! impl Calculator {
//!     #[mcp_tool("Add two numbers together")]
//!     pub fn add(
//!         &self,
//!         #[param("The first number")] a: f64,
//!         #[param("The second number")] b: f64,
//!     ) -> f64 {
//!         a + b
//!     }
//! }
//! ```
//!
//! Note: `#[mcp_tool]` and `#[param]` are inert marker attributes processed by `#[mcp_server]`.
//! They should only be used within impl blocks marked with `#[mcp_server]`.

use proc_macro::TokenStream;

mod schema;
mod server;
mod tool;

/// Marks a struct as an MCP server or an impl block as containing MCP tools.
///
/// When applied to a struct, it adds server name and version metadata.
/// When applied to an impl block, it processes methods marked with `#[mcp_tool]`
/// and generates the `MacroServer` trait implementation.
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
///     #[mcp_tool(description = "Tool description")]
///     pub fn my_tool(
///         &self,
///         #[param("Parameter description")] param: String,
///     ) -> Result<String, String> { ... }
/// }
/// ```
///
/// # Parameter Attributes
///
/// Within `#[mcp_tool]` methods, use `#[param(...)]` on parameters:
///
/// **Shorthand (recommended):**
/// - `#[param("description")]` - Just the description
///
/// **Full form:**
/// - `#[param(description = "...", name = "...", required = true)]`
///
/// Options:
/// - `description` - Description shown to the LLM
/// - `name` - Custom parameter name override (optional)
/// - `required` - Override required/optional inference (optional)
///
/// All non-self parameters must have `#[param(...)]` - unmarked parameters cause compile errors.
#[proc_macro_attribute]
pub fn mcp_server(attr: TokenStream, item: TokenStream) -> TokenStream {
    server::mcp_server_impl(attr.into(), item.into()).into()
}

/// Marks a method as an MCP tool.
///
/// **Note**: This is an inert marker attribute that should only be used within
/// impl blocks marked with `#[mcp_server]`. When used outside of `#[mcp_server]`,
/// it will generate tool metadata but won't be collected into a server.
///
/// # Example
///
/// ```rust,ignore
/// #[mcp_server]
/// impl MyServer {
///     #[mcp_tool(description = "Store a value in memory")]
///     pub fn memory_write(
///         &self,
///         #[mcp(description = "The scope/namespace")]
///         scope: String,
///         #[mcp(description = "The key to store under")]
///         key: String,
///     ) -> Result<String, String> {
///         // implementation
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn mcp_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    tool::mcp_tool_impl(attr.into(), item.into()).into()
}
