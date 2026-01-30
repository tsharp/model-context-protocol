# MCP Macros

Procedural macros for the Model Context Protocol (MCP) Rust implementation.

## Features

This crate provides procedural macros to reduce boilerplate when defining MCP servers:

- `#[mcp_server]` - Define server metadata and collect tools
- `#[mcp_tool]` - Mark a method as an MCP tool
- `#[mcp_internal]` - Mark a server as built-in (no stdio wrapper)

## Usage

```rust
use mcp::{mcp_server, mcp_tool, ToolResult};
use serde_json::Value;

#[mcp_server(name = "calculator", version = "1.0.0")]
pub struct CalculatorServer;

#[mcp_server]
impl CalculatorServer {
    #[mcp_tool(description = "Add two numbers")]
    pub fn add(&self, a: f64, b: f64) -> ToolResult<f64> {
        Ok(a + b)
    }
    
    #[mcp_tool(description = "Multiply two numbers")]
    pub fn multiply(&self, a: f64, b: f64) -> ToolResult<f64> {
        Ok(a * b)
    }
}
```

## Parameter Documentation

Tool parameters can be documented using doc comments:

```rust
#[mcp_tool(description = "Store a value in memory")]
pub fn memory_write(
    &self,
    /// The scope/namespace for the memory
    scope: String,
    /// The key to store the value under
    key: String,
    /// The value to store (can be any JSON value)
    value: Value,
) -> ToolResult<String> {
    // implementation
}
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
