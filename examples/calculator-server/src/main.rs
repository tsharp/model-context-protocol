//! Calculator MCP Server
//!
//! A simple calculator MCP server demonstrating function-based tools
//! with the `#[mcp_tool]` macro.
//!
//! ## Running
//!
//! ```sh
//! cargo run -p calculator-server
//! ```
//!
//! ## Testing
//!
//! Send JSON-RPC requests via stdin:
//! ```json
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//! {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
//! {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add","arguments":{"a":5,"b":3}}}
//! ```

use mcp::{mcp_tool, McpServer, McpServerConfig};

// =============================================================================
// Calculator Tools - just annotate functions!
// =============================================================================

/// Add two numbers together.
#[mcp_tool(description = "Add two numbers together", group = "arithmetic")]
fn add(a: f64, b: f64) -> f64 {
    a + b
}

/// Subtract the second number from the first.
#[mcp_tool(
    description = "Subtract second number from first",
    group = "arithmetic"
)]
fn subtract(a: f64, b: f64) -> f64 {
    a - b
}

/// Multiply two numbers.
#[mcp_tool(description = "Multiply two numbers", group = "arithmetic")]
fn multiply(a: f64, b: f64) -> f64 {
    a * b
}

/// Divide the first number by the second.
#[mcp_tool(description = "Divide first number by second", group = "arithmetic")]
fn divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 {
        Err("Division by zero".to_string())
    } else {
        Ok(a / b)
    }
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = McpServerConfig::builder()
        .name("calculator")
        .version("1.0.0")
        .with_stdio_transport()
        .register_tools_in_group("arithmetic") // Auto-discovers all tools with group = "arithmetic"
        .build();

    McpServer::run(config).await?;
    Ok(())
}
