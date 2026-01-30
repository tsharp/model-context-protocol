# MCP Macros

Procedural macros for the Model Context Protocol (MCP) Rust implementation.

## Features

This crate provides procedural macros to reduce boilerplate when defining MCP servers:

- `#[mcp_server]` - Define server metadata and collect tools from impl blocks
- `#[mcp_tool]` - Mark a method as an MCP tool
- `#[param]` - Document tool parameters for the LLM

## Usage

```rust
use mcp::macros::mcp_server;

#[mcp_server(name = "calculator", version = "1.0.0")]
pub struct CalculatorServer;

#[mcp_server]
impl CalculatorServer {
    #[mcp_tool("Add two numbers")]
    pub fn add(
        &self,
        #[param("First number")] a: f64,
        #[param("Second number")] b: f64,
    ) -> f64 {
        a + b
    }
    
    #[mcp_tool("Multiply two numbers")]
    pub fn multiply(
        &self,
        #[param("First factor")] a: f64,
        #[param("Second factor")] b: f64,
    ) -> f64 {
        a * b
    }
}
```

## Parameter Attributes

All tool parameters (except `&self`) must be marked with `#[param(...)]`:

```rust
// Shorthand - just the description:
#[param("The user's name")]

// Full form - with additional options:
#[param(description = "The user's name", name = "username", required = true)]
```

Options:
- `description` - Description shown to the LLM (required for good UX)
- `name` - Custom parameter name override (optional, defaults to argument name)
- `required` - Override required/optional inference (optional, defaults based on `Option<T>`)

## Tool Attributes

```rust
// Shorthand - just the description:
#[mcp_tool("Add two numbers")]

// Full form - with additional options:
#[mcp_tool(description = "Add two numbers", name = "custom_add", group = "math")]
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
