# Model Context Protocol (MCP)

[![Crates.io](https://img.shields.io/crates/v/model-context-protocol.svg)](https://crates.io/crates/model-context-protocol)
[![Documentation](https://docs.rs/model-context-protocol/badge.svg)](https://docs.rs/model-context-protocol)
[![License](https://img.shields.io/crates/l/model-context-protocol.svg)](LICENSE-MIT)

A comprehensive Rust implementation of the Model Context Protocol (MCP) for AI tool integration.

## Overview

This workspace provides a complete MCP implementation for building MCP servers and clients in Rust. The Model Context Protocol enables seamless communication between AI models and tool providers.

## Crates

| Crate | Description |
|-------|-------------|
| [`model-context-protocol`](https://crates.io/crates/model-context-protocol) | Core MCP implementation with transports, protocol types, and hub |
| [`model-context-protocol-macros`](crates/model-context-protocol-macros) | Procedural macros for declarative server/tool definitions |

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
model-context-protocol = "0.2"
```

## Quick Start

### Function-Based Tools

The simplest way to create an MCP server - just annotate functions:

```rust
use mcp::macros::mcp_tool;
use mcp::server::stdio::McpStdioServer;
use mcp::McpServerConfig;

#[mcp_tool("Add two numbers", group = "arithmetic")]
fn add(#[param("First number")] a: f64, #[param("Second number")] b: f64) -> f64 {
    a + b
}

#[mcp_tool("Subtract two numbers", group = "arithmetic")]
fn subtract(#[param("Number to subtract from")] a: f64, #[param("Number to subtract")] b: f64) -> f64 {
    a - b
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = McpServerConfig::builder()
        .name("calculator")
        .version("1.0.0")
        .register_tools_in_group("arithmetic")
        .build();

    McpStdioServer::run(config).await?;
    Ok(())
}
```

### Struct-Based Servers

For more complex servers with shared state:

```rust
use mcp::macros::mcp_server;
use mcp::server::stdio::McpStdioServer;
use mcp::{MacroServerAdapter, McpServerConfig};

#[mcp_server(name = "calculator", version = "1.0.0")]
pub struct Calculator;

#[mcp_server]
impl Calculator {
    #[mcp_tool("Add two numbers")]
    pub fn add(&self, #[param("First number")] a: f64, #[param("Second number")] b: f64) -> f64 {
        a + b
    }

    #[mcp_tool("Divide two numbers")]
    pub fn divide(
        &self,
        #[param("Dividend")] a: f64,
        #[param("Divisor")] b: f64,
    ) -> Result<f64, String> {
        if b == 0.0 {
            Err("Division by zero".to_string())
        } else {
            Ok(a / b)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = McpServerConfig::builder()
        .name("calculator")
        .version("1.0.0")
        .with_tools_from(MacroServerAdapter::new(Calculator))
        .build();

    McpStdioServer::run(config).await?;
    Ok(())
}
```

### Connecting to Servers

Use `McpHub` to manage multiple MCP server connections:

```rust
use mcp::{McpHub, McpServerConnectionConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hub = McpHub::new();

    // Connect to an external server
    let config = McpServerConnectionConfig::stdio("my-server", "node", vec!["server.js".into()]);
    hub.connect(config).await?;

    // List available tools
    let tools = hub.list_all_tools().await?;
    for tool in tools {
        println!("Tool: {}", tool.name);
    }

    Ok(())
}
```

## Features

- **JSON-RPC 2.0 Protocol**: Full implementation of the MCP JSON-RPC protocol
- **Multiple Transports**: Support for stdio and HTTP-based MCP servers
- **McpHub**: Central hub for managing multiple MCP server connections
- **Procedural Macros**: `#[mcp_server]`, `#[mcp_tool]`, and `#[param]` for declarative definitions
- **Parameter Descriptions**: `#[param("description")]` provides LLM-friendly parameter documentation
- **Tool Groups**: Organize tools by group for auto-discovery
- **Tool Routing**: Automatic routing of tool calls to the correct server
- **Error Handling**: Built-in `ToolResult<T>` for ergonomic error handling

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `stdio` | ✓ | Stdio transport for spawning server processes |
| `http` | ✓ | HTTP transport for connecting to HTTP servers |
| `macros` | ✓ | Procedural macros for defining tools and servers |
| `http-server` | | HTTP server support with actix-web |

## Examples

The workspace includes several example servers:

| Example | Description |
|---------|-------------|
| [`calculator-server`](examples/calculator-server) | Function-based tools with stdio transport |
| [`macro-calculator`](examples/macro-calculator) | Struct-based server using `#[mcp_server]` |
| [`text-tools-server`](examples/text-tools-server) | HTTP transport with text manipulation tools |
| [`notes-server`](examples/notes-server) | `ToolProvider` pattern with shared state |

Run an example:

```sh
cargo run -p calculator-server
cargo run -p text-tools-server
```

## Protocol Version

This crate implements MCP protocol version `2025-11-25`.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
