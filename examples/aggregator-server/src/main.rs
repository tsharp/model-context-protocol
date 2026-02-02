//! Aggregator Server Example
//!
//! Demonstrates using `McpServerHub` as an MCP server that aggregates
//! multiple external MCP servers into a single unified interface, along
//! with embedded local tools.
//!
//! This example:
//! 1. Creates an McpServerHub
//! 2. Connects to the calculator-server (external)
//! 3. Adds local embedded tools (greet, reverse, timestamp)
//! 4. Wraps everything with McpStdioServer
//!
//! The result is an MCP server that combines local and proxied tools.
//!
//! ## Building
//!
//! First, build the calculator server:
//! ```sh
//! cargo build -p calculator-server
//! ```
//!
//! Then build this aggregator:
//! ```sh
//! cargo build -p aggregator-server
//! ```
//!
//! ## Testing
//!
//! Run the aggregator and send JSON-RPC requests via stdin:
//! ```sh
//! cargo run -p aggregator-server
//! ```
//!
//! Then send:
//! ```json
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//! {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
//! {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"greet","arguments":{"name":"World"}}}
//! {"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"add","arguments":{"a":10,"b":5}}}
//! ```
//!
//! ## Use Case
//!
//! This pattern is useful when you want to:
//! - Aggregate multiple specialized MCP servers into one
//! - Add local tools alongside external server tools
//! - Create a unified tool interface for clients
//! - Chain MCP servers together

use mcp::macros::mcp_tool;
use mcp::server::stdio::McpStdioServer;
use mcp::{McpServerConfig, McpServerConnectionConfig, McpServerHub};
use std::sync::Arc;

// =============================================================================
// Local Embedded Tools
// =============================================================================

/// Greet someone by name.
#[mcp_tool(description = "Greet someone with a friendly message", group = "utils")]
fn greet(#[param("The name to greet")] name: String) -> String {
    format!("Hello, {}! Welcome to the aggregator server.", name)
}

/// Reverse a string.
#[mcp_tool(description = "Reverse the characters in a string", group = "utils")]
fn reverse(#[param("The string to reverse")] text: String) -> String {
    text.chars().rev().collect()
}

/// Get the current timestamp.
#[mcp_tool(description = "Get the current Unix timestamp", group = "utils")]
fn timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Count words in text.
#[mcp_tool(description = "Count the number of words in a text", group = "utils")]
fn word_count(#[param("The text to count words in")] text: String) -> usize {
    text.split_whitespace().count()
}

/// Convert text to uppercase.
#[mcp_tool(description = "Convert text to uppercase", group = "utils")]
fn to_upper(#[param("The text to convert")] text: String) -> String {
    text.to_uppercase()
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a hub that will aggregate external servers
    let hub = Arc::new(McpServerHub::new("aggregator"));

    // Connect to the calculator server (external) with restart on failure
    // Note: In production, you might read server configs from a file
    let calc_config = McpServerConnectionConfig::stdio(
        "calculator",
        "cargo",
        vec![
            "run".into(),
            "-p".into(),
            "calculator-server".into(),
            "-q".into(),
        ],
    )
    .with_timeout(30)
    .restart_on_failure();  // Auto-restart if the server crashes

    hub.connect(calc_config).await?;

    // Log connected servers to stderr (stdout is for MCP protocol)
    eprintln!("[aggregator] Connected to calculator server");

    // Build config with both hub tools (proxied) and local tools
    let config = McpServerConfig::builder()
        .name("aggregator")
        .version("1.0.0")
        // Add local embedded tools
        .register_tools_in_group("utils")
        // Add proxied tools from connected servers
        .with_tools(hub.proxy_tools())
        .build();

    eprintln!("[aggregator] Available tools:");
    for tool in config.registry().definitions() {
        eprintln!(
            "  - {} : {}",
            tool.name,
            tool.description.as_deref().unwrap_or("(no description)")
        );
    }

    eprintln!("[aggregator] Starting stdio server...");
    McpStdioServer::run(config).await?;

    // Cleanup
    hub.shutdown_all().await?;

    Ok(())
}
