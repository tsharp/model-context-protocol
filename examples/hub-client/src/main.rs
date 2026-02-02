//! Hub Client Example
//!
//! Demonstrates using `McpServerHub` to:
//! - Connect to external MCP servers (like the calculator-server)
//! - Discover available tools
//! - Call tools and get results
//!
//! ## Running
//!
//! First, build the calculator server:
//! ```sh
//! cargo build -p calculator-server
//! ```
//!
//! Then run this client:
//! ```sh
//! cargo run -p hub-client
//! ```
//!
//! The client will spawn the calculator server as a subprocess and communicate
//! with it via stdio.

use mcp::{McpServerHub, McpServerConnectionConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== McpServerHub Client Example ===\n");

    // Create a new hub - this manages connections to multiple MCP servers
    let hub = std::sync::Arc::new(McpServerHub::new("hub-client"));

    // Connect to the calculator server via stdio
    // The hub will spawn the server process and communicate via stdin/stdout
    println!("Connecting to calculator server...");
    
    let config = McpServerConnectionConfig::stdio(
        "calculator",                                          // Server name (for routing)
        "cargo",                                               // Command to run
        vec![
            "run".into(),
            "-p".into(), 
            "calculator-server".into(),
            "-q".into(),  // Quiet mode to suppress cargo output
        ],
    )
    .with_timeout(30)  // 30 second timeout for initialization
    .restart_on_failure();  // Auto-restart if the server crashes

    hub.connect(config).await?;
    println!("✓ Connected!\n");

    // Discover all available tools
    println!("Discovering tools...");
    let tools = hub.list_tools().await?;
    
    println!("Found {} tools:", tools.len());
    for (server, tool) in &tools {
        let desc = tool.description.as_deref().unwrap_or("No description");
        println!("  [{server}] {} - {}", tool.name, desc);
    }
    println!();

    // Call some tools!
    println!("=== Calling Calculator Tools ===\n");

    // Addition
    let result = hub.call_tool("add", serde_json::json!({
        "a": 10.0,
        "b": 5.0
    })).await?;
    println!("add(10, 5) = {}", result);

    // Subtraction
    let result = hub.call_tool("subtract", serde_json::json!({
        "a": 10.0,
        "b": 3.0
    })).await?;
    println!("subtract(10, 3) = {}", result);

    // Multiplication
    let result = hub.call_tool("multiply", serde_json::json!({
        "a": 7.0,
        "b": 6.0
    })).await?;
    println!("multiply(7, 6) = {}", result);

    // Division
    let result = hub.call_tool("divide", serde_json::json!({
        "a": 100.0,
        "b": 4.0
    })).await?;
    println!("divide(100, 4) = {}", result);

    // Division by zero (error case)
    println!("\nTesting error handling:");
    match hub.call_tool("divide", serde_json::json!({
        "a": 42.0,
        "b": 0.0
    })).await {
        Ok(result) => println!("divide(42, 0) = {}", result),
        Err(e) => println!("divide(42, 0) → Error: {}", e),
    }

    // Chain operations example
    println!("\n=== Chained Calculation: (10 + 5) * 3 ===");
    
    let sum = hub.call_tool("add", serde_json::json!({"a": 10.0, "b": 5.0})).await?;
    println!("Step 1: 10 + 5 = {}", sum);
    
    // Extract the numeric value from the result
    // The result could be a string like "15.0" or a nested structure
    let sum_value: f64 = sum.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| sum.as_f64())
        .or_else(|| {
            // Try nested array format: [{"type": "text", "text": "15.0"}]
            sum.as_array()
                .and_then(|arr| arr.first())
                .and_then(|v| v.get("text"))
                .and_then(|t| t.as_str())
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0.0);
    
    let product = hub.call_tool("multiply", serde_json::json!({
        "a": sum_value, 
        "b": 3.0
    })).await?;
    println!("Step 2: {} * 3 = {}", sum_value, product);

    // Show which server handles each tool
    println!("\n=== Tool Routing ===");
    for tool_name in ["add", "subtract", "multiply", "divide"] {
        if let Some(server) = hub.server_for_tool(tool_name) {
            println!("  {} → {}", tool_name, server);
        }
    }

    // Health check
    println!("\n=== Server Health ===");
    for (name, alive) in hub.health_check().await {
        let status = if alive { "✓ healthy" } else { "✗ unhealthy" };
        println!("  {} - {}", name, status);
    }

    // Clean shutdown
    println!("\nShutting down...");
    hub.shutdown_all().await?;
    println!("✓ Done!");

    Ok(())
}
