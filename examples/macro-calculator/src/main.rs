//! Macro-Based Calculator Server
//!
//! Demonstrates using `#[mcp_server]` and `#[mcp_tool]` macros to define
//! an MCP server with minimal boilerplate.
//!
//! ## Running
//!
//! ```sh
//! cargo run -p macro-calculator
//! ```

use mcp::{mcp_server, MacroServer, MacroServerAdapter, McpServerConfig};

// =============================================================================
// Calculator Server using Macros
// =============================================================================

/// A simple calculator MCP server.
#[mcp_server(name = "calculator", version = "1.0.0")]
pub struct Calculator;

#[mcp_server]
impl Calculator {
    /// Add two numbers together.
    #[mcp_tool(description = "Add two numbers together")]
    pub fn add(&self, a: f64, b: f64) -> f64 {
        a + b
    }

    /// Subtract the second number from the first.
    #[mcp_tool(description = "Subtract second number from first")]
    pub fn subtract(&self, a: f64, b: f64) -> f64 {
        a - b
    }

    /// Multiply two numbers.
    #[mcp_tool(description = "Multiply two numbers")]
    pub fn multiply(&self, a: f64, b: f64) -> f64 {
        a * b
    }

    /// Divide the first number by the second.
    #[mcp_tool(description = "Divide first number by second")]
    pub fn divide(&self, a: f64, b: f64) -> Result<f64, String> {
        if b == 0.0 {
            Err("Division by zero".to_string())
        } else {
            Ok(a / b)
        }
    }

    /// Calculate the power of a number.
    #[mcp_tool(description = "Calculate a raised to the power of b")]
    pub fn power(&self, base: f64, exponent: f64) -> f64 {
        base.powf(exponent)
    }

    /// Calculate the square root of a number.
    #[mcp_tool(description = "Calculate the square root of a number")]
    pub fn sqrt(&self, n: f64) -> Result<f64, String> {
        if n < 0.0 {
            Err("Cannot calculate square root of negative number".to_string())
        } else {
            Ok(n.sqrt())
        }
    }
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Macro Calculator Example ===\n");

    // Create calculator using macros
    let calc = Calculator;

    // Show the generated tools
    println!("Tools defined by macros:");
    for tool in calc.list_tools() {
        println!(
            "  - {}: {}",
            tool.name,
            tool.description.as_deref().unwrap_or("")
        );
    }

    // Test calling tools directly
    println!("\n--- Direct calls ---");
    println!(
        "add(5, 3) = {:?}",
        calc.call_tool("add", serde_json::json!({"a": 5.0, "b": 3.0}))
    );
    println!(
        "subtract(10, 4) = {:?}",
        calc.call_tool("subtract", serde_json::json!({"a": 10.0, "b": 4.0}))
    );
    println!(
        "multiply(6, 7) = {:?}",
        calc.call_tool("multiply", serde_json::json!({"a": 6.0, "b": 7.0}))
    );
    println!(
        "divide(20, 4) = {:?}",
        calc.call_tool("divide", serde_json::json!({"a": 20.0, "b": 4.0}))
    );
    println!(
        "power(2, 8) = {:?}",
        calc.call_tool("power", serde_json::json!({"base": 2.0, "exponent": 8.0}))
    );
    println!(
        "sqrt(144) = {:?}",
        calc.call_tool("sqrt", serde_json::json!({"n": 144.0}))
    );

    // Build an MCP server using the macro adapter
    println!("\n--- Using McpServerConfig builder ---");
    let config = McpServerConfig::builder()
        .name("macro-calculator")
        .version("1.0.0")
        .with_tools_from(MacroServerAdapter::new(Calculator))
        .build();

    // Show tools via the server
    println!("Server tools:");
    for tool in config.registry().definitions() {
        println!("  - {}", tool.name);
    }

    // Call tools via the registry
    println!("\n--- Server tool calls ---");
    let result = config
        .registry()
        .call("add", serde_json::json!({"a": 100.0, "b": 200.0}))
        .await?;
    println!("add(100, 200) = {}", result[0].as_text().unwrap_or("error"));

    let result = config
        .registry()
        .call("sqrt", serde_json::json!({"n": 256.0}))
        .await?;
    println!("sqrt(256) = {}", result[0].as_text().unwrap_or("error"));

    println!("\n=== Example Complete ===");

    // To run as a stdio server, uncomment:
    // McpServer::run(config).await?;

    Ok(())
}
