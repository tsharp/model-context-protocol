//! Text Tools MCP Server
//!
//! An MCP server demonstrating HTTP transport with function-based tools.
//!
//! All parameters must be marked with `#[param("description")]` to provide
//! descriptions for the LLM.
//!
//! ## Running
//!
//! ```sh
//! cargo run -p text-tools-server
//! ```
//!
//! ## Endpoints
//!
//! - GET  /tools - List available tools
//! - POST /call  - Call a tool directly
//! - POST /rpc   - JSON-RPC endpoint
//!
//! ## Testing
//!
//! ```sh
//! curl http://localhost:8080/tools
//! curl -X POST http://localhost:8080/call -H "Content-Type: application/json" \
//!      -d '{"name":"echo","arguments":{"message":"Hello!"}}'
//! ```

use mcp::macros::mcp_tool;
use mcp::{McpServer, McpServerConfig};

// =============================================================================
// Text Tools - just annotate functions!
// =============================================================================

/// Echo back the provided message.
#[mcp_tool(group = "text", description = "Echo back the provided message")]
fn echo(#[param("The message to echo back")] message: String) -> String {
    format!("Echo: {}", message)
}

/// Reverse the provided text.
#[mcp_tool(group = "text", description = "Reverse the provided text")]
fn reverse(#[param("The text to reverse")] text: String) -> String {
    text.chars().rev().collect()
}

/// Convert text to uppercase.
#[mcp_tool(group = "text", description = "Convert text to uppercase")]
fn uppercase(#[param("The text to convert")] text: String) -> String {
    text.to_uppercase()
}

/// Convert text to lowercase.
#[mcp_tool(group = "text", description = "Convert text to lowercase")]
fn lowercase(#[param("The text to convert")] text: String) -> String {
    text.to_lowercase()
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting HTTP MCP server on http://127.0.0.1:8080");
    println!();
    println!("Endpoints:");
    println!("  GET  /tools - List available tools");
    println!("  POST /call  - Call a tool directly");
    println!("  POST /rpc   - JSON-RPC endpoint");
    println!();

    let config = McpServerConfig::builder()
        .name("text-tools")
        .version("1.0.0")
        .with_http_transport("127.0.0.1", 8080)
        .register_tools_in_group("text") // Auto-discovers all tools with group = "text"
        .build();

    McpServer::run(config).await?;
    Ok(())
}
