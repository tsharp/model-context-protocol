//! Tool traits and types for MCP servers.
//!
//! This module provides the core abstractions for defining and executing MCP tools.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

use crate::protocol::{McpToolDefinition, ToolContent};

/// A boxed future for async tool execution.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Result type for tool execution - returns content or an error message.
pub type ToolCallResult = Result<Vec<ToolContent>, String>;

// =============================================================================
// Tool Auto-Discovery via Inventory
// =============================================================================

/// A factory function that creates a tool instance.
pub type ToolFactory = fn() -> DynTool;

/// Entry for auto-discovered tools registered via `#[mcp_tool]`.
///
/// This struct is used internally by the `inventory` crate to collect
/// all tools defined with the `#[mcp_tool]` attribute at link time.
pub struct ToolEntry {
    /// Factory function to create the tool.
    pub factory: ToolFactory,
    /// The group this tool belongs to (if any).
    pub group: Option<&'static str>,
}

impl ToolEntry {
    /// Creates a new tool entry.
    pub const fn new(factory: ToolFactory, group: Option<&'static str>) -> Self {
        Self { factory, group }
    }
}

// Register ToolEntry with inventory for compile-time collection
inventory::collect!(ToolEntry);

/// Returns all auto-discovered tools.
///
/// This collects all tools registered with `#[mcp_tool]` across the crate.
pub fn all_tools() -> Vec<DynTool> {
    inventory::iter::<ToolEntry>()
        .map(|entry| (entry.factory)())
        .collect()
}

/// Returns auto-discovered tools filtered by group.
///
/// Only returns tools that have the specified group.
pub fn tools_in_group(group: &str) -> Vec<DynTool> {
    inventory::iter::<ToolEntry>()
        .filter(|entry| entry.group == Some(group))
        .map(|entry| (entry.factory)())
        .collect()
}

/// Trait for implementing MCP tools.
///
/// # Example
///
/// ```ignore
/// use mcp::{McpTool, ToolCallResult, McpToolDefinition};
/// use serde_json::Value;
///
/// struct CalculatorTool;
///
/// impl McpTool for CalculatorTool {
///     fn definition(&self) -> McpToolDefinition {
///         McpToolDefinition {
///             name: "add".to_string(),
///             description: Some("Add two numbers".to_string()),
///             group: None,
///             input_schema: serde_json::json!({
///                 "type": "object",
///                 "properties": {
///                     "a": { "type": "number" },
///                     "b": { "type": "number" }
///                 },
///                 "required": ["a", "b"]
///             }),
///         }
///     }
///
///     fn call<'a>(&'a self, args: Value) -> BoxFuture<'a, ToolCallResult> {
///         Box::pin(async move {
///             let a = args["a"].as_f64().unwrap_or(0.0);
///             let b = args["b"].as_f64().unwrap_or(0.0);
///             Ok(vec![ToolContent::text(format!("{}", a + b))])
///         })
///     }
/// }
/// ```
pub trait McpTool: Send + Sync {
    /// Returns the tool definition (name, description, input schema).
    fn definition(&self) -> McpToolDefinition;

    /// Executes the tool with the given arguments.
    fn call<'a>(&'a self, args: Value) -> BoxFuture<'a, ToolCallResult>;
}

/// A type-erased tool wrapper.
pub type DynTool = Arc<dyn McpTool>;

/// Trait for types that can provide multiple tools.
///
/// Implement this trait to group related tools together.
///
/// # Example
///
/// ```ignore
/// use mcp::{ToolProvider, McpTool};
///
/// struct MathTools;
///
/// impl ToolProvider for MathTools {
///     fn tools(&self) -> Vec<Arc<dyn McpTool>> {
///         vec![
///             Arc::new(AddTool),
///             Arc::new(SubtractTool),
///             Arc::new(MultiplyTool),
///         ]
///     }
/// }
/// ```
pub trait ToolProvider: Send + Sync {
    /// Returns a list of tools provided by this provider.
    fn tools(&self) -> Vec<DynTool>;
}

/// A simple function-based tool.
///
/// This allows creating tools from closures without implementing the `McpTool` trait.
pub struct FnTool<F>
where
    F: Fn(Value) -> BoxFuture<'static, ToolCallResult> + Send + Sync,
{
    definition: McpToolDefinition,
    handler: F,
}

impl<F> FnTool<F>
where
    F: Fn(Value) -> BoxFuture<'static, ToolCallResult> + Send + Sync,
{
    /// Creates a new function-based tool.
    pub fn new(definition: McpToolDefinition, handler: F) -> Self {
        Self {
            definition,
            handler,
        }
    }
}

impl<F> McpTool for FnTool<F>
where
    F: Fn(Value) -> BoxFuture<'static, ToolCallResult> + Send + Sync,
{
    fn definition(&self) -> McpToolDefinition {
        self.definition.clone()
    }

    fn call<'a>(&'a self, args: Value) -> BoxFuture<'a, ToolCallResult> {
        (self.handler)(args)
    }
}

/// Registry for managing tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, DynTool>,
    /// Cached definitions for faster access
    definitions_cache: parking_lot::RwLock<Option<Vec<McpToolDefinition>>>,
}

impl ToolRegistry {
    /// Creates a new empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            definitions_cache: parking_lot::RwLock::new(None),
        }
    }

    /// Registers a tool.
    pub fn register(&mut self, tool: DynTool) {
        let name = tool.definition().name.clone();
        self.tools.insert(name, tool);
        // Invalidate cache when tools change
        *self.definitions_cache.write() = None;
    }

    /// Registers multiple tools from a provider.
    pub fn register_provider<P: ToolProvider>(&mut self, provider: P) {
        for tool in provider.tools() {
            let name = tool.definition().name.clone();
            self.tools.insert(name, tool);
        }
        // Invalidate cache when tools change
        *self.definitions_cache.write() = None;
    }

    /// Gets a tool by name.
    pub fn get(&self, name: &str) -> Option<&DynTool> {
        self.tools.get(name)
    }

    /// Returns all tool definitions (cached).
    ///
    /// Uses an Arc-wrapped cache to minimize cloning overhead.
    /// Returns a clone of the Arc, so iterating is efficient.
    pub fn definitions(&self) -> Vec<McpToolDefinition> {
        // Try to return cached definitions
        {
            let cache = self.definitions_cache.read();
            if let Some(ref defs) = *cache {
                return defs.clone();
            }
        }

        // Build and cache definitions
        let defs: Vec<McpToolDefinition> = self.tools.values().map(|t| t.definition()).collect();
        *self.definitions_cache.write() = Some(defs.clone());
        defs
    }

    /// Returns an iterator over tool definitions without cloning.
    ///
    /// More efficient than `definitions()` when you only need to iterate.
    pub fn definitions_iter(&self) -> impl Iterator<Item = McpToolDefinition> + '_ {
        self.tools.values().map(|t| t.definition())
    }

    /// Returns all tool definitions without caching (for cases where fresh data is needed).
    pub fn definitions_uncached(&self) -> Vec<McpToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Invalidates the definitions cache.
    pub fn invalidate_cache(&self) {
        *self.definitions_cache.write() = None;
    }

    /// Returns the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns true if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Calls a tool by name with the given arguments.
    pub async fn call(&self, name: &str, args: Value) -> ToolCallResult {
        match self.get(name) {
            Some(tool) => tool.call(args).await,
            None => Err(format!("Unknown tool: {}", name)),
        }
    }
}

/// Helper macro for creating tools from async functions.
///
/// # Example
///
/// ```ignore
/// use mcp::fn_tool;
///
/// let add_tool = fn_tool!(
///     "add",
///     "Add two numbers",
///     {
///         "type": "object",
///         "properties": {
///             "a": { "type": "number" },
///             "b": { "type": "number" }
///         }
///     },
///     |args| async move {
///         let a = args["a"].as_f64().unwrap_or(0.0);
///         let b = args["b"].as_f64().unwrap_or(0.0);
///         Ok(vec![ToolContent::text(format!("{}", a + b))])
///     }
/// );
/// ```
#[macro_export]
macro_rules! fn_tool {
    ($name:expr, $desc:expr, $schema:tt, $handler:expr) => {{
        use $crate::protocol::McpToolDefinition;
        use $crate::tool::FnTool;

        let definition = McpToolDefinition {
            name: $name.to_string(),
            description: Some($desc.to_string()),
            group: None,
            input_schema: serde_json::json!($schema),
        };

        FnTool::new(definition, move |args| Box::pin($handler(args)))
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ToolContent;

    struct TestTool {
        name: String,
    }

    impl McpTool for TestTool {
        fn definition(&self) -> McpToolDefinition {
            McpToolDefinition::new(&self.name)
                .with_description("Test tool")
                .with_schema(serde_json::json!({"type": "object"}))
        }

        fn call<'a>(&'a self, _args: Value) -> BoxFuture<'a, ToolCallResult> {
            Box::pin(async move { Ok(vec![ToolContent::text("ok")]) })
        }
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(TestTool {
            name: "test".to_string(),
        }));

        assert_eq!(registry.len(), 1);
        assert!(registry.get("test").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(TestTool {
            name: "tool1".to_string(),
        }));
        registry.register(Arc::new(TestTool {
            name: "tool2".to_string(),
        }));

        let defs = registry.definitions();
        assert_eq!(defs.len(), 2);
    }

    #[tokio::test]
    async fn test_registry_call() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(TestTool {
            name: "test".to_string(),
        }));

        let result = registry.call("test", serde_json::json!({})).await;
        assert!(result.is_ok());

        let result = registry.call("unknown", serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
