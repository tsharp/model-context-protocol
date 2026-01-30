//! Macro-Based Calculator Server
//!
//! Demonstrates using `#[mcp_server]` and `#[mcp_tool]` macros to define
//! an MCP server with minimal boilerplate.
//!
//! Parameters are explicitly marked with `#[param]` to include them
//! in the tool schema with descriptions for the LLM.
//!
//! ## Running
//!

// Uncomment below to test the compile error for unmarked parameters:
// #[mcp_server]
// impl BadTest {
//     #[mcp_tool(description = "Test")]
//     pub fn test(&self, unmarked: i32) -> i32 { unmarked }
// }
// Error: "Parameter `unmarked` must be marked with #[param(\"description\")]..."
//! ```sh
//! cargo run -p macro-calculator
//! ```

use mcp::macros::mcp_server;
use mcp::{MacroServer, MacroServerAdapter, McpServerConfig};

// =============================================================================
// Calculator Server using Macros
// =============================================================================

/// A simple calculator MCP server.
#[mcp_server(name = "calculator", version = "1.0.0")]
pub struct Calculator;

#[mcp_server]
impl Calculator {
    /// Add two numbers together.
    #[mcp_tool("Add two numbers together")]
    pub fn add(
        &self,
        #[param("The first number to add")] a: f64,
        #[param("The second number to add")] b: f64,
    ) -> f64 {
        a + b
    }

    /// Subtract the second number from the first.
    #[mcp_tool("Subtract second number from first")]
    pub fn subtract(
        &self,
        #[param("The number to subtract from")] a: f64,
        #[param("The number to subtract")] b: f64,
    ) -> f64 {
        a - b
    }

    /// Multiply two numbers.
    #[mcp_tool("Multiply two numbers")]
    pub fn multiply(
        &self,
        #[param("The first factor")] a: f64,
        #[param("The second factor")] b: f64,
    ) -> f64 {
        a * b
    }

    /// Divide the first number by the second.
    #[mcp_tool("Divide first number by second")]
    pub fn divide(
        &self,
        #[param("The dividend (number to divide)")] a: f64,
        #[param("The divisor (number to divide by)")] b: f64,
    ) -> Result<f64, String> {
        if b == 0.0 {
            Err("Division by zero".to_string())
        } else {
            Ok(a / b)
        }
    }

    /// Calculate the power of a number.
    #[mcp_tool("Calculate a raised to the power of b")]
    pub fn power(
        &self,
        #[param("The base number")] base: f64,
        #[param("The exponent")] exponent: f64,
    ) -> f64 {
        base.powf(exponent)
    }

    /// Calculate the square root of a number.
    #[mcp_tool("Calculate the square root of a number")]
    pub fn sqrt(
        &self,
        #[param("The number to calculate the square root of")] n: f64,
    ) -> Result<f64, String> {
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

    // Show the generated tools with full schema
    println!("Tools defined by macros:");
    for tool in calc.list_tools() {
        println!(
            "  - {}: {}",
            tool.name,
            tool.description.as_deref().unwrap_or("")
        );
        println!("    Schema: {}", serde_json::to_string_pretty(&tool.input_schema).unwrap());
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
