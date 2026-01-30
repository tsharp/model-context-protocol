# Model Context Protocol (MCP)

A comprehensive Rust implementation of the Model Context Protocol (MCP) for AI tool integration.

## Overview

This workspace provides a complete MCP implementation for building MCP servers and clients in Rust. The Model Context Protocol enables seamless communication between AI models and tool providers.

## Crates

| Crate | Description |
|-------|-------------|
| [`mcp`](crates/mcp) | Core MCP implementation with transports, protocol types, and hub |
| [`mcp-macros`](crates/mcp-macros) | Procedural macros for declarative server/tool definitions |
| `model-context-protocol` | Re-exports from `mcp` for convenience |

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
mcp = "0.1.0"
# or use the re-export crate
model-context-protocol = "0.1.0"
```

## Quick Start

```rust
use mcp::{McpHub, McpServerConnectionConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create hub with built-in memory server
    let hub = McpHub::new();
    
    // List available tools
    let tools = hub.list_all_tools().await?;
    for tool in tools {
        println!("Tool: {} - {}", tool.name, tool.description);
    }
    
    // Call a tool
    let result = hub.call_tool("memory_write", serde_json::json!({
        "scope": "session",
        "key": "greeting",
        "value": "Hello, World!"
    })).await?;
    
    Ok(())
}
```

## Defining Custom Servers

Use macros for declarative server definitions:

```rust
use mcp::{mcp_server, mcp_tool, ToolResult};

#[mcp_server(name = "calculator", version = "1.0.0")]
pub struct CalculatorServer;

#[mcp_server]
impl CalculatorServer {
    #[mcp_tool(description = "Add two numbers")]
    pub fn add(&self, a: f64, b: f64) -> ToolResult<f64> {
        Ok(a + b)
    }
}
```

## Features

- **JSON-RPC 2.0 Protocol**: Full implementation of the MCP JSON-RPC protocol
- **Multiple Transports**: Support for stdio and HTTP-based MCP servers
- **Built-in Memory Server**: In-memory key-value store with optional persistence
- **McpHub**: Central hub for managing multiple MCP server connections
- **Procedural Macros**: `#[mcp_server]` and `#[mcp_tool]` for declarative definitions
- **Tool Routing**: Automatic routing of tool calls to the correct server

## Feature Flags

The `mcp` crate supports these feature flags:

- `default` - Enables all features
- `stdio` - Stdio transport for spawning server processes
- `http` - HTTP transport for connecting to HTTP servers  
- `memory` - Built-in memory server with persistence
- `macros` - Procedural macros for defining servers

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
