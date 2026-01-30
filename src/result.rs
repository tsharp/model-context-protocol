//! Result types for MCP tool implementations.
//!
//! Provides ergonomic return types that automatically convert to `CallToolResult`.

use serde::Serialize;

use crate::protocol::{CallToolResult, ToolContent};

/// Result type for MCP tool implementations.
///
/// Converts automatically to `CallToolResult` for macro-generated code.
///
/// # Example
///
/// ```rust,ignore
/// #[mcp_tool(description = "Read a value")]
/// fn read(&self, key: String) -> ToolResult<Value> {
///     self.store.get(&key).ok_or_else(|| format!("Key not found: {}", key))
/// }
/// ```
pub type ToolResult<T> = Result<T, String>;

/// Extension trait for converting values to `CallToolResult`.
pub trait IntoCallToolResult {
    /// Convert to a `CallToolResult`.
    fn into_call_result(self) -> CallToolResult;
}

impl<T: Serialize> IntoCallToolResult for ToolResult<T> {
    fn into_call_result(self) -> CallToolResult {
        match self {
            Ok(value) => {
                let text = match serde_json::to_string_pretty(&value) {
                    Ok(s) => s,
                    Err(e) => format!("Serialization error: {}", e),
                };
                CallToolResult {
                    content: vec![ToolContent::text(text)],
                    is_error: Some(false),
                }
            }
            Err(e) => CallToolResult {
                content: vec![ToolContent::text(format!("Error: {}", e))],
                is_error: Some(true),
            },
        }
    }
}

impl IntoCallToolResult for CallToolResult {
    fn into_call_result(self) -> CallToolResult {
        self
    }
}

/// Allows `ToolResult<T>` to convert to `CallToolResult` via `.into()`.
impl<T: Serialize> From<ToolResult<T>> for CallToolResult {
    fn from(result: ToolResult<T>) -> Self {
        result.into_call_result()
    }
}

/// Convenience function to create a successful tool result.
pub fn tool_ok<T: Serialize>(value: T) -> ToolResult<T> {
    Ok(value)
}

/// Convenience function to create a failed tool result.
pub fn tool_err<T>(message: impl Into<String>) -> ToolResult<T> {
    Err(message.into())
}

/// Create a successful `CallToolResult` with text content.
pub fn success_result(text: impl Into<String>) -> CallToolResult {
    CallToolResult {
        content: vec![ToolContent::text(text)],
        is_error: Some(false),
    }
}

/// Create an error `CallToolResult` with text content.
pub fn error_result(message: impl Into<String>) -> CallToolResult {
    CallToolResult {
        content: vec![ToolContent::text(message)],
        is_error: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok_result_converts() {
        let result: ToolResult<String> = Ok("success".to_string());
        let call_result = result.into_call_result();
        assert_eq!(call_result.is_error, Some(false));
        assert_eq!(call_result.content.len(), 1);
    }

    #[test]
    fn test_err_result_converts() {
        let result: ToolResult<String> = Err("failed".to_string());
        let call_result = result.into_call_result();
        assert_eq!(call_result.is_error, Some(true));
    }

    #[test]
    fn test_tool_ok_helper() {
        let result = tool_ok("hello");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_tool_err_helper() {
        let result: ToolResult<()> = tool_err("oops");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "oops");
    }

    #[test]
    fn test_success_result() {
        let result = success_result("Operation completed");
        assert_eq!(result.is_error, Some(false));
        assert!(result.content[0]
            .as_text()
            .unwrap()
            .contains("Operation completed"));
    }

    #[test]
    fn test_error_result() {
        let result = error_result("Something went wrong");
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0]
            .as_text()
            .unwrap()
            .contains("Something went wrong"));
    }
}
