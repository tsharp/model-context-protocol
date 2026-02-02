//! MCP JSON-RPC Protocol Types
//!
//! This module defines the core JSON-RPC 2.0 protocol types used for
//! communicating with Model Context Protocol (MCP) servers.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

// Re-export protocol version from lib.rs
pub use crate::MCP_PROTOCOL_VERSION;

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: JsonRpcId, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    #[serde(flatten)]
    pub payload: JsonRpcPayload,
}

impl JsonRpcResponse {
    /// Create a success response.
    pub fn success(id: impl Into<JsonRpcId>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            payload: JsonRpcPayload::Success { result },
        }
    }

    /// Create an error response.
    pub fn error(
        id: impl Into<JsonRpcId>,
        code: i32,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            payload: JsonRpcPayload::Error {
                error: JsonRpcError {
                    code,
                    message: message.into(),
                    data,
                },
            },
        }
    }

    /// Get the result if this is a success response.
    pub fn result(&self) -> Option<&Value> {
        match &self.payload {
            JsonRpcPayload::Success { result } => Some(result),
            JsonRpcPayload::Error { .. } => None,
        }
    }

    /// Returns true if this is a success response.
    pub fn is_success(&self) -> bool {
        matches!(&self.payload, JsonRpcPayload::Success { .. })
    }

    /// Returns true if this is an error response.
    pub fn is_error(&self) -> bool {
        matches!(&self.payload, JsonRpcPayload::Error { .. })
    }

    /// Get the error if this is an error response.
    pub fn error_info(&self) -> Option<&JsonRpcError> {
        match &self.payload {
            JsonRpcPayload::Success { .. } => None,
            JsonRpcPayload::Error { error } => Some(error),
        }
    }
}

/// JSON-RPC 2.0 Notification (no id, no response expected)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcNotification {
    /// Create a new notification.
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

/// Outbound message from server to client.
#[derive(Debug, Clone)]
pub enum ServerOutbound {
    /// A request that expects a response from the client.
    Request(JsonRpcRequest),
    /// A notification that does not expect a response.
    Notification(JsonRpcNotification),
    /// A response to a client request.
    Response(JsonRpcResponse),
}

impl ServerOutbound {
    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        match self {
            ServerOutbound::Request(r) => serde_json::to_string(r),
            ServerOutbound::Notification(n) => serde_json::to_string(n),
            ServerOutbound::Response(r) => serde_json::to_string(r),
        }
    }
}

/// Inbound message from client to server.
#[derive(Debug, Clone)]
pub enum ClientInbound {
    /// A request from the client that expects a response.
    Request(JsonRpcRequest),
    /// A response to a server request.
    Response(JsonRpcResponse),
    /// A notification from the client.
    Notification(JsonRpcNotification),
}

/// A JSON-RPC message that can be any of request, response, or notification.
/// Used for parsing incoming messages when the type is unknown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
}

impl JsonRpcMessage {
    /// Parse a JSON string into a message.
    pub fn parse(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Convert to ClientInbound for server processing.
    pub fn into_client_inbound(self) -> ClientInbound {
        match self {
            JsonRpcMessage::Request(r) => ClientInbound::Request(r),
            JsonRpcMessage::Response(r) => ClientInbound::Response(r),
            JsonRpcMessage::Notification(n) => ClientInbound::Notification(n),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcPayload {
    Success { result: Value },
    Error { error: JsonRpcError },
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSON-RPC Error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for JsonRpcError {}

/// Standard JSON-RPC error codes
pub mod error_codes {
    /// Invalid JSON was received by the server
    pub const PARSE_ERROR: i32 = -32700;
    /// The JSON sent is not a valid Request object
    pub const INVALID_REQUEST: i32 = -32600;
    /// The method does not exist / is not available
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid method parameter(s)
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal JSON-RPC error
    pub const INTERNAL_ERROR: i32 = -32603;
    /// URL elicitation required (implementation-specific)
    pub const URL_ELICITATION_REQUIRED: i32 = -32042;
}

// =============================================================================
// Core Types (2025-11-25)
// =============================================================================

/// An optionally-sized icon that can be displayed in a user interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Icon {
    /// A standard URI pointing to an icon resource.
    /// May be an HTTP/HTTPS URL or a `data:` URI with Base64-encoded image data.
    pub src: String,

    /// Optional MIME type override if the source MIME type is missing or generic.
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,

    /// Optional array of strings that specify sizes at which the icon can be used.
    /// Each string should be in WxH format (e.g., "48x48", "96x96") or "any" for scalable formats.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sizes: Option<Vec<String>>,

    /// Optional specifier for the theme this icon is designed for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<IconTheme>,
}

/// Theme specifier for an icon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IconTheme {
    Light,
    Dark,
}

/// Base metadata with name (identifier) and title (display name) properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseMetadata {
    /// Intended for programmatic or logical use.
    pub name: String,

    /// Intended for UI and end-user contexts — optimized to be human-readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Optional annotations for the client.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Annotations {
    /// Describes who the intended audience of this object or data is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<Role>>,

    /// Describes how important this data is (0.0 = least, 1.0 = most important).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,

    /// The moment the resource was last modified, as an ISO 8601 formatted string.
    #[serde(rename = "lastModified", skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

/// The sender or recipient of messages and data in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

// =============================================================================
// MCP Server Types
// =============================================================================

/// MCP Implementation info (server or client).
/// Describes the MCP implementation with name, version, icons, and description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Implementation {
    /// Intended for programmatic or logical use.
    pub name: String,

    /// The version of the implementation.
    pub version: String,

    /// Intended for UI and end-user contexts — optimized to be human-readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// An optional human-readable description of what this implementation does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional set of sized icons that the client can display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,

    /// An optional URL of the website for this implementation.
    #[serde(rename = "websiteUrl", skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
}

/// MCP server information (legacy alias for Implementation).
pub type McpServerInfo = Implementation;

/// MCP server capabilities (legacy format).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completions: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Value>,
}

// =============================================================================
// JSON-RPC ID
// =============================================================================

/// JSON-RPC ID can be string, number, or null
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum JsonRpcId {
    String(String),
    Number(i64),
    Null,
}

impl From<i64> for JsonRpcId {
    fn from(n: i64) -> Self {
        JsonRpcId::Number(n)
    }
}

impl From<String> for JsonRpcId {
    fn from(s: String) -> Self {
        JsonRpcId::String(s)
    }
}

impl From<&str> for JsonRpcId {
    fn from(s: &str) -> Self {
        JsonRpcId::String(s.to_string())
    }
}

impl From<Value> for JsonRpcId {
    fn from(v: Value) -> Self {
        match v {
            Value::String(s) => JsonRpcId::String(s),
            Value::Number(n) => JsonRpcId::Number(n.as_i64().unwrap_or(0)),
            Value::Null => JsonRpcId::Null,
            _ => JsonRpcId::Null,
        }
    }
}

impl From<JsonRpcId> for Value {
    fn from(id: JsonRpcId) -> Self {
        match id {
            JsonRpcId::String(s) => Value::String(s),
            JsonRpcId::Number(n) => Value::Number(n.into()),
            JsonRpcId::Null => Value::Null,
        }
    }
}

impl fmt::Display for JsonRpcId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonRpcId::String(s) => write!(f, "{}", s),
            JsonRpcId::Number(n) => write!(f, "{}", n),
            JsonRpcId::Null => write!(f, "null"),
        }
    }
}

// =============================================================================
// MCP Tool Types
// =============================================================================

/// Additional properties describing a Tool to clients.
/// NOTE: all properties in ToolAnnotations are **hints**.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolAnnotations {
    /// A human-readable title for the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// If true, the tool does not modify its environment. Default: false
    #[serde(rename = "readOnlyHint", skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,

    /// If true, the tool may perform destructive updates. Default: true
    #[serde(rename = "destructiveHint", skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,

    /// If true, calling the tool repeatedly with the same arguments has no additional effect. Default: false
    #[serde(rename = "idempotentHint", skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,

    /// If true, this tool may interact with an "open world" of external entities. Default: true
    #[serde(rename = "openWorldHint", skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// Indicates whether a tool supports task-augmented execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskSupport {
    /// Tool does not support task-augmented execution (default)
    #[default]
    Forbidden,
    /// Tool may support task-augmented execution
    Optional,
    /// Tool requires task-augmented execution
    Required,
}

/// Execution-related properties for a tool.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolExecution {
    /// Indicates whether this tool supports task-augmented execution.
    #[serde(rename = "taskSupport", skip_serializing_if = "Option::is_none")]
    pub task_support: Option<TaskSupport>,
}

/// MCP Tool Definition (from tools/list response)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,

    /// Intended for UI and end-user contexts — optimized to be human-readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,

    /// Optional set of sized icons that the client can display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,

    #[serde(rename = "inputSchema", default = "default_input_schema")]
    pub input_schema: Value,

    /// An optional JSON Schema object defining the structure of the tool's output.
    #[serde(rename = "outputSchema", skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,

    /// Execution-related properties for this tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ToolExecution>,

    /// Optional additional tool information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,

    /// See General fields: `_meta` for notes on `_meta` usage.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

fn default_input_schema() -> Value {
    serde_json::json!({"type": "object"})
}

impl McpToolDefinition {
    /// Create a new tool definition.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            description: None,
            group: None,
            icons: None,
            input_schema: default_input_schema(),
            output_schema: None,
            execution: None,
            annotations: None,
            meta: None,
        }
    }

    /// Set the title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the group.
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// Set the description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the input schema.
    pub fn with_schema(mut self, schema: Value) -> Self {
        self.input_schema = schema;
        self
    }

    /// Set the output schema.
    pub fn with_output_schema(mut self, schema: Value) -> Self {
        self.output_schema = Some(schema);
        self
    }

    /// Set the annotations.
    pub fn with_annotations(mut self, annotations: ToolAnnotations) -> Self {
        self.annotations = Some(annotations);
        self
    }

    /// Set the execution properties.
    pub fn with_execution(mut self, execution: ToolExecution) -> Self {
        self.execution = Some(execution);
        self
    }

    /// Get the description, defaulting to empty string.
    pub fn description_or_default(&self) -> &str {
        self.description.as_deref().unwrap_or("")
    }

    /// Get a copy of the input schema as parameters value.
    pub fn parameters(&self) -> Value {
        self.input_schema.clone()
    }
}

/// JSON Schema for tool inputs (legacy format)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// MCP tools/list request parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListToolsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// MCP tools/list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResult {
    pub tools: Vec<McpToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// MCP tools/call request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    /// If specified, the caller is requesting task-augmented execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskMetadata>,
    /// Request metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// MCP tools/call response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResult {
    /// A list of content objects that represent the unstructured result of the tool call.
    #[serde(default)]
    pub content: Vec<ToolContent>,
    /// An optional JSON object that represents the structured result of the tool call.
    #[serde(rename = "structuredContent", skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
    /// Whether the tool call ended in an error.
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Tool execution content (text, image, audio, or resource types)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
        #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
        #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
    #[serde(rename = "audio")]
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
        #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
    #[serde(rename = "resource")]
    Resource {
        uri: String,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    #[serde(rename = "resource_link")]
    ResourceLink {
        uri: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
    },
}

impl ToolContent {
    /// Create a text content item.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            annotations: None,
            meta: None,
        }
    }

    /// Create an image content item.
    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Image {
            data: data.into(),
            mime_type: mime_type.into(),
            annotations: None,
            meta: None,
        }
    }

    /// Create an audio content item.
    pub fn audio(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Audio {
            data: data.into(),
            mime_type: mime_type.into(),
            annotations: None,
            meta: None,
        }
    }

    /// Create a resource content item.
    pub fn resource(uri: impl Into<String>, mime_type: Option<String>) -> Self {
        Self::Resource {
            uri: uri.into(),
            mime_type,
        }
    }

    /// Get text content if this is a text item.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text, .. } => Some(text),
            _ => None,
        }
    }

    /// Check if this is a text item.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }
}

// =============================================================================
// Tasks Subsystem (2025-11-25)
// =============================================================================

/// The status of a task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// The request is currently being processed
    Working,
    /// The task is waiting for input (e.g., elicitation or sampling)
    InputRequired,
    /// The request completed successfully and results are available
    Completed,
    /// The request did not complete successfully
    Failed,
    /// The request was cancelled before completion
    Cancelled,
}

/// Metadata for augmenting a request with task execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskMetadata {
    /// Requested duration in milliseconds to retain task from creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u64>,
}

/// Metadata for associating messages with a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedTaskMetadata {
    /// The task identifier this message is associated with.
    #[serde(rename = "taskId")]
    pub task_id: String,
}

/// Data associated with a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// The task identifier.
    #[serde(rename = "taskId")]
    pub task_id: String,

    /// Current task state.
    pub status: TaskStatus,

    /// Optional human-readable message describing the current task state.
    #[serde(rename = "statusMessage", skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,

    /// ISO 8601 timestamp when the task was created.
    #[serde(rename = "createdAt")]
    pub created_at: String,

    /// ISO 8601 timestamp when the task was last updated.
    #[serde(rename = "lastUpdatedAt")]
    pub last_updated_at: String,

    /// Actual retention duration from creation in milliseconds, null for unlimited.
    pub ttl: Option<u64>,

    /// Suggested polling interval in milliseconds.
    #[serde(rename = "pollInterval", skip_serializing_if = "Option::is_none")]
    pub poll_interval: Option<u64>,
}

/// A response to a task-augmented request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskResult {
    pub task: Task,
}

/// Parameters for tasks/get request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskParams {
    /// The task identifier to query.
    #[serde(rename = "taskId")]
    pub task_id: String,
}

/// Parameters for tasks/result request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskPayloadParams {
    /// The task identifier to retrieve results for.
    #[serde(rename = "taskId")]
    pub task_id: String,
}

/// Parameters for tasks/cancel request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskParams {
    /// The task identifier to cancel.
    #[serde(rename = "taskId")]
    pub task_id: String,
}

/// Parameters for tasks/list request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListTasksParams {
    /// An opaque token representing the current pagination position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Response to a tasks/list request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTasksResult {
    pub tasks: Vec<Task>,
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Parameters for notifications/tasks/status notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusNotificationParams {
    /// The task identifier.
    #[serde(rename = "taskId")]
    pub task_id: String,

    /// Current task state.
    pub status: TaskStatus,

    /// Optional human-readable message describing the current task state.
    #[serde(rename = "statusMessage", skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,

    /// ISO 8601 timestamp when the task was created.
    #[serde(rename = "createdAt")]
    pub created_at: String,

    /// ISO 8601 timestamp when the task was last updated.
    #[serde(rename = "lastUpdatedAt")]
    pub last_updated_at: String,

    /// Actual retention duration from creation in milliseconds.
    pub ttl: Option<u64>,

    /// Suggested polling interval in milliseconds.
    #[serde(rename = "pollInterval", skip_serializing_if = "Option::is_none")]
    pub poll_interval: Option<u64>,
}

// =============================================================================
// Elicitation Subsystem (2025-11-25)
// =============================================================================

/// Parameters for elicitation/create request with form mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitRequestFormParams {
    /// The elicitation mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    /// The message to present to the user describing what information is being requested.
    pub message: String,

    /// A restricted subset of JSON Schema for the form fields.
    #[serde(rename = "requestedSchema")]
    pub requested_schema: Value,

    /// Task metadata if requesting task-augmented execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskMetadata>,

    /// Request metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Parameters for elicitation/create request with URL mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitRequestUrlParams {
    /// The elicitation mode.
    pub mode: String,

    /// The message to present to the user explaining why the interaction is needed.
    pub message: String,

    /// The ID of the elicitation, which must be unique within the context of the server.
    #[serde(rename = "elicitationId")]
    pub elicitation_id: String,

    /// The URL that the user should navigate to.
    pub url: String,

    /// Task metadata if requesting task-augmented execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskMetadata>,

    /// Request metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Parameters for elicitation/create request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ElicitRequestParams {
    Form(ElicitRequestFormParams),
    Url(ElicitRequestUrlParams),
}

/// User action in response to an elicitation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ElicitAction {
    /// User submitted the form/confirmed the action
    Accept,
    /// User explicitly declined the action
    Decline,
    /// User dismissed without making an explicit choice
    Cancel,
}

/// The client's response to an elicitation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitResult {
    /// The user action in response to the elicitation.
    pub action: ElicitAction,

    /// The submitted form data, only present when action is "accept" and mode was "form".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
}

/// Parameters for notifications/elicitation/complete notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationCompleteParams {
    /// The ID of the elicitation that completed.
    #[serde(rename = "elicitationId")]
    pub elicitation_id: String,
}

// =============================================================================
// Primitive Schema Definitions for Elicitation (2025-11-25)
// =============================================================================

/// String schema for elicitation forms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringSchema {
    #[serde(rename = "type")]
    pub schema_type: String, // Always "string"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "minLength", skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(rename = "maxLength", skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<StringSchemaFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Format options for string schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StringSchemaFormat {
    Email,
    Uri,
    Date,
    DateTime,
}

/// Number schema for elicitation forms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumberSchema {
    #[serde(rename = "type")]
    pub schema_type: String, // "number" or "integer"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<f64>,
}

/// Boolean schema for elicitation forms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BooleanSchema {
    #[serde(rename = "type")]
    pub schema_type: String, // Always "boolean"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
}

/// Single-select enum option with title.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitledEnumOption {
    /// The enum value.
    #[serde(rename = "const")]
    pub const_value: String,
    /// Display label for this option.
    pub title: String,
}

/// Multi-select enum item schema (untitled).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSelectEnumItems {
    #[serde(rename = "type")]
    pub schema_type: String, // Always "string"
    /// Array of enum values to choose from.
    #[serde(rename = "enum")]
    pub enum_values: Vec<String>,
}

/// Multi-select enum item schema (titled).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitledMultiSelectEnumItems {
    /// Array of enum options with values and display labels.
    #[serde(rename = "anyOf")]
    pub any_of: Vec<TitledEnumOption>,
}

// =============================================================================
// Sampling Types (2025-11-25)
// =============================================================================

/// Controls tool selection behavior for sampling requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoiceMode {
    /// Model decides whether to use tools (default)
    #[default]
    Auto,
    /// Model MUST use at least one tool before completing
    Required,
    /// Model MUST NOT use any tools
    None,
}

/// Controls tool selection behavior for sampling requests.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolChoice {
    /// Controls the tool use ability of the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ToolChoiceMode>,
}

/// A request from the assistant to call a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseContent {
    /// A unique identifier for this tool use.
    pub id: String,
    /// The name of the tool to call.
    pub name: String,
    /// The arguments to pass to the tool.
    pub input: Value,
    /// Optional metadata about the tool use.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// The result of a tool use, provided by the user back to the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultContent {
    /// The ID of the tool use this result corresponds to.
    #[serde(rename = "toolUseId")]
    pub tool_use_id: String,
    /// The unstructured result content of the tool use.
    pub content: Vec<ToolContent>,
    /// An optional structured result object.
    #[serde(rename = "structuredContent", skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
    /// Whether the tool use resulted in an error.
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Optional metadata about the tool result.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Sampling message content block types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SamplingContent {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
        #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
        #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
    #[serde(rename = "audio")]
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
        #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
        #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        #[serde(rename = "toolUseId")]
        tool_use_id: String,
        content: Vec<ToolContent>,
        #[serde(rename = "structuredContent", skip_serializing_if = "Option::is_none")]
        structured_content: Option<Value>,
        #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
}

/// Describes a message issued to or received from an LLM API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingMessage {
    pub role: Role,
    pub content: Vec<SamplingContent>,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Hints to use for model selection.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelHint {
    /// A hint for a model name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The server's preferences for model selection.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelPreferences {
    /// Optional hints to use for model selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<ModelHint>>,

    /// How much to prioritize cost when selecting a model (0.0 - 1.0).
    #[serde(rename = "costPriority", skip_serializing_if = "Option::is_none")]
    pub cost_priority: Option<f64>,

    /// How much to prioritize sampling speed (0.0 - 1.0).
    #[serde(rename = "speedPriority", skip_serializing_if = "Option::is_none")]
    pub speed_priority: Option<f64>,

    /// How much to prioritize intelligence and capabilities (0.0 - 1.0).
    #[serde(rename = "intelligencePriority", skip_serializing_if = "Option::is_none")]
    pub intelligence_priority: Option<f64>,
}

/// Parameters for sampling/createMessage request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageParams {
    pub messages: Vec<SamplingMessage>,

    /// The server's preferences for which model to select.
    #[serde(rename = "modelPreferences", skip_serializing_if = "Option::is_none")]
    pub model_preferences: Option<ModelPreferences>,

    /// An optional system prompt the server wants to use for sampling.
    #[serde(rename = "systemPrompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// A request to include context from one or more MCP servers.
    #[serde(rename = "includeContext", skip_serializing_if = "Option::is_none")]
    pub include_context: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// The requested maximum number of tokens to sample.
    #[serde(rename = "maxTokens")]
    pub max_tokens: u32,

    #[serde(rename = "stopSequences", skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,

    /// Optional metadata to pass through to the LLM provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,

    /// Tools that the model may use during generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<McpToolDefinition>>,

    /// Controls how the model uses tools.
    #[serde(rename = "toolChoice", skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,

    /// Task metadata if requesting task-augmented execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskMetadata>,

    /// Request metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Standard stop reasons for sampling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    /// Natural end of the assistant's turn
    EndTurn,
    /// A stop sequence was encountered
    StopSequence,
    /// Maximum token limit was reached
    MaxTokens,
    /// The model wants to use one or more tools
    ToolUse,
    /// Provider-specific stop reason
    #[serde(untagged)]
    Other(String),
}

/// The client's response to a sampling/createMessage request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageResult {
    pub role: Role,
    pub content: Vec<SamplingContent>,
    /// The name of the model that generated the message.
    pub model: String,
    /// The reason why sampling stopped, if known.
    #[serde(rename = "stopReason", skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

// =============================================================================
// Logging Types (2025-11-25)
// =============================================================================

/// The severity of a log message (RFC-5424 syslog severities).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoggingLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

/// Parameters for logging/setLevel request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLevelParams {
    /// The level of logging that the client wants to receive from the server.
    pub level: LoggingLevel,
}

/// Parameters for notifications/message notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingMessageParams {
    /// The severity of this log message.
    pub level: LoggingLevel,
    /// An optional name of the logger issuing this message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
    /// The data to be logged.
    pub data: Value,
}

// =============================================================================
// Progress Notifications (2025-11-25)
// =============================================================================

/// Parameters for notifications/progress notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressNotificationParams {
    /// The progress token which was given in the initial request.
    #[serde(rename = "progressToken")]
    pub progress_token: ProgressToken,

    /// The progress thus far.
    pub progress: f64,

    /// Total number of items to process, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,

    /// An optional message describing the current progress.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// A progress token, used to associate progress notifications with the original request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProgressToken {
    String(String),
    Number(i64),
}

// =============================================================================
// Roots Types (2025-11-25)
// =============================================================================

/// Represents a root directory or file that the server can operate on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    /// The URI identifying the root. Must start with file:// for now.
    pub uri: String,

    /// An optional name for the root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Request metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Response to a roots/list request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRootsResult {
    pub roots: Vec<Root>,
}

// =============================================================================
// Completion Types (2025-11-25)
// =============================================================================

/// A reference to a prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptReference {
    #[serde(rename = "type")]
    pub ref_type: String, // "ref/prompt"
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// A reference to a resource or resource template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTemplateReference {
    #[serde(rename = "type")]
    pub ref_type: String, // "ref/resource"
    pub uri: String,
}

/// Parameters for completion/complete request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteParams {
    pub r#ref: Value, // Can be PromptReference or ResourceTemplateReference
    pub argument: CompleteArgument,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<CompleteContext>,
}

/// Argument information for completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteArgument {
    /// The name of the argument.
    pub name: String,
    /// The value of the argument to use for completion matching.
    pub value: String,
}

/// Additional context for completions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteContext {
    /// Previously-resolved variables in a URI template or prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// Response to a completion/complete request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteResult {
    pub completion: CompletionData,
}

/// Completion data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionData {
    /// An array of completion values. Must not exceed 100 items.
    pub values: Vec<String>,
    /// The total number of completion options available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    /// Indicates whether there are additional completion options.
    #[serde(rename = "hasMore", skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_request_serialization() {
        let req = JsonRpcRequest::new(JsonRpcId::Number(1), "tools/list", None);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""id":1"#));
        assert!(json.contains(r#""method":"tools/list""#));
    }

    #[test]
    fn test_jsonrpc_response_success() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();

        assert!(matches!(resp.payload, JsonRpcPayload::Success { .. }));
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();

        assert!(matches!(resp.payload, JsonRpcPayload::Error { .. }));
    }

    #[test]
    fn test_tool_content_text() {
        let content = ToolContent::text("Hello, world!");
        assert_eq!(content.as_text(), Some("Hello, world!"));

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains(r#""text":"Hello, world!""#));
    }

    #[test]
    fn test_jsonrpc_id_display() {
        assert_eq!(JsonRpcId::Number(42).to_string(), "42");
        assert_eq!(JsonRpcId::String("test".to_string()).to_string(), "test");
        assert_eq!(JsonRpcId::Null.to_string(), "null");
    }

    #[test]
    fn test_mcp_tool_deserialization() {
        let json = r#"{
            "name": "test_tool",
            "description": "A test tool",
            "input_schema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                },
                "required": ["name"]
            }
        }"#;

        let tool: McpToolDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.description, Some("A test tool".to_string()));
        assert_eq!(tool.input_schema["type"], "object");
    }
}
