//! Notes MCP Server
//!
//! Demonstrates using the MCP server in-process with a `ToolProvider`
//! to group related tools together.
//!
//! ## Running
//!
//! ```sh
//! cargo run -p notes-server
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use model_context_protocol::{
    BoxFuture, DynTool, McpTool, McpToolDefinition, ToolCallResult, ToolContent, ToolProvider,
};
use serde_json::Value;

// =============================================================================
// Note Model
// =============================================================================

#[derive(Clone, Default)]
struct Note {
    id: String,
    title: String,
    content: String,
    tags: Vec<String>,
}

// =============================================================================
// Notes State (shared between tools)
// =============================================================================

struct NotesState {
    notes: RwLock<HashMap<String, Note>>,
}

impl NotesState {
    fn new() -> Self {
        Self {
            notes: RwLock::new(HashMap::new()),
        }
    }
}

// =============================================================================
// Notes Tool Provider
// =============================================================================

struct NotesProvider {
    state: Arc<NotesState>,
}

impl NotesProvider {
    fn new() -> Self {
        Self {
            state: Arc::new(NotesState::new()),
        }
    }
}

impl ToolProvider for NotesProvider {
    fn tools(&self) -> Vec<DynTool> {
        vec![
            Arc::new(NotesCreateTool {
                state: self.state.clone(),
            }),
            Arc::new(NotesReadTool {
                state: self.state.clone(),
            }),
            Arc::new(NotesListTool {
                state: self.state.clone(),
            }),
            Arc::new(NotesDeleteTool {
                state: self.state.clone(),
            }),
            Arc::new(NotesSearchTool {
                state: self.state.clone(),
            }),
        ]
    }
}

// =============================================================================
// Individual Tools
// =============================================================================

struct NotesCreateTool {
    state: Arc<NotesState>,
}

impl McpTool for NotesCreateTool {
    fn definition(&self) -> McpToolDefinition {
        McpToolDefinition::new("notes_create")
            .with_description("Create a new note")
            .with_group("notes")
            .with_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Unique note ID" },
                    "title": { "type": "string", "description": "Note title" },
                    "content": { "type": "string", "description": "Note content" },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tags for the note"
                    }
                },
                "required": ["id", "title", "content"]
            }))
    }

    fn call<'a>(&'a self, args: Value) -> BoxFuture<'a, ToolCallResult> {
        Box::pin(async move {
            let id = args["id"].as_str().ok_or("Missing 'id'")?;
            let title = args["title"].as_str().ok_or("Missing 'title'")?;
            let content = args["content"].as_str().ok_or("Missing 'content'")?;
            let tags: Vec<String> = args["tags"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let note = Note {
                id: id.to_string(),
                title: title.to_string(),
                content: content.to_string(),
                tags,
            };

            self.state
                .notes
                .write()
                .unwrap()
                .insert(id.to_string(), note);

            Ok(vec![ToolContent::text(format!("Created note: {}", id))])
        })
    }
}

struct NotesReadTool {
    state: Arc<NotesState>,
}

impl McpTool for NotesReadTool {
    fn definition(&self) -> McpToolDefinition {
        McpToolDefinition::new("notes_read")
            .with_description("Read a note by ID")
            .with_group("notes")
            .with_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Note ID to read" }
                },
                "required": ["id"]
            }))
    }

    fn call<'a>(&'a self, args: Value) -> BoxFuture<'a, ToolCallResult> {
        Box::pin(async move {
            let id = args["id"].as_str().ok_or("Missing 'id'")?;

            let notes = self.state.notes.read().unwrap();
            let note = notes.get(id).ok_or("Note not found")?;

            let output = format!(
                "# {}\n\nID: {}\nTags: {}\n\n{}",
                note.title,
                note.id,
                note.tags.join(", "),
                note.content
            );

            Ok(vec![ToolContent::text(output)])
        })
    }
}

struct NotesListTool {
    state: Arc<NotesState>,
}

impl McpTool for NotesListTool {
    fn definition(&self) -> McpToolDefinition {
        McpToolDefinition::new("notes_list")
            .with_description("List all notes, optionally filtered by tag")
            .with_group("notes")
            .with_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "tag": { "type": "string", "description": "Optional tag to filter by" }
                }
            }))
    }

    fn call<'a>(&'a self, args: Value) -> BoxFuture<'a, ToolCallResult> {
        Box::pin(async move {
            let tag_filter = args["tag"].as_str();

            let notes = self.state.notes.read().unwrap();
            let filtered: Vec<_> = notes
                .values()
                .filter(|n| {
                    tag_filter
                        .map(|t| n.tags.iter().any(|nt| nt == t))
                        .unwrap_or(true)
                })
                .collect();

            let output = if filtered.is_empty() {
                "No notes found".to_string()
            } else {
                let list: Vec<String> = filtered
                    .iter()
                    .map(|n| format!("- {} [{}]: {}", n.id, n.tags.join(", "), n.title))
                    .collect();
                format!("Notes ({}):\n{}", filtered.len(), list.join("\n"))
            };

            Ok(vec![ToolContent::text(output)])
        })
    }
}

struct NotesDeleteTool {
    state: Arc<NotesState>,
}

impl McpTool for NotesDeleteTool {
    fn definition(&self) -> McpToolDefinition {
        McpToolDefinition::new("notes_delete")
            .with_description("Delete a note by ID")
            .with_group("notes")
            .with_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Note ID to delete" }
                },
                "required": ["id"]
            }))
    }

    fn call<'a>(&'a self, args: Value) -> BoxFuture<'a, ToolCallResult> {
        Box::pin(async move {
            let id = args["id"].as_str().ok_or("Missing 'id'")?;

            let removed = self.state.notes.write().unwrap().remove(id);

            let output = match removed {
                Some(_) => format!("Deleted note: {}", id),
                None => format!("Note not found: {}", id),
            };

            Ok(vec![ToolContent::text(output)])
        })
    }
}

struct NotesSearchTool {
    state: Arc<NotesState>,
}

impl McpTool for NotesSearchTool {
    fn definition(&self) -> McpToolDefinition {
        McpToolDefinition::new("notes_search")
            .with_description("Search notes by content")
            .with_group("notes")
            .with_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" }
                },
                "required": ["query"]
            }))
    }

    fn call<'a>(&'a self, args: Value) -> BoxFuture<'a, ToolCallResult> {
        Box::pin(async move {
            let query = args["query"].as_str().ok_or("Missing 'query'")?;
            let query_lower = query.to_lowercase();

            let notes = self.state.notes.read().unwrap();
            let matches: Vec<_> = notes
                .values()
                .filter(|n| {
                    n.title.to_lowercase().contains(&query_lower)
                        || n.content.to_lowercase().contains(&query_lower)
                })
                .collect();

            let output = if matches.is_empty() {
                format!("No notes matching '{}'", query)
            } else {
                let list: Vec<String> = matches
                    .iter()
                    .map(|n| format!("- {}: {}", n.id, n.title))
                    .collect();
                format!("Found {} matches:\n{}", matches.len(), list.join("\n"))
            };

            Ok(vec![ToolContent::text(output)])
        })
    }
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Notes MCP Server Example ===\n");

    // Build server configuration with tools from a provider (like WithToolsFromAssembly)
    let config = model_context_protocol::McpServerConfig::builder()
        .name("notes-server")
        .version("1.0.0")
        .with_tools_from(NotesProvider::new())
        .build();

    // List available tools
    println!("Available tools:");
    for tool in config.registry().definitions() {
        println!(
            "  - {}: {}",
            tool.name,
            tool.description.as_deref().unwrap_or("No description")
        );
    }

    println!("\n--- Demo: Creating and managing notes ---\n");

    // Create some notes - use the registry directly for in-process testing
    let result = config
        .registry()
        .call(
            "notes_create",
            serde_json::json!({
                "id": "meeting-notes",
                "title": "Team Meeting Notes",
                "content": "Discussed project timeline and deliverables.\n- Sprint ends Friday\n- Demo on Monday",
                "tags": ["work", "meetings"]
            }),
        )
        .await?;
    println!("Create note: {:?}", result[0].as_text().unwrap_or(""));

    let result = config
        .registry()
        .call(
            "notes_create",
            serde_json::json!({
                "id": "shopping-list",
                "title": "Shopping List",
                "content": "- Milk\n- Eggs\n- Bread\n- Butter",
                "tags": ["personal", "todo"]
            }),
        )
        .await?;
    println!("Create note: {:?}", result[0].as_text().unwrap_or(""));

    let result = config
        .registry()
        .call(
            "notes_create",
            serde_json::json!({
                "id": "project-ideas",
                "title": "Project Ideas",
                "content": "1. Build a CLI task manager\n2. Create a blog with Rust\n3. MCP server for home automation",
                "tags": ["work", "ideas"]
            }),
        )
        .await?;
    println!("Create note: {:?}", result[0].as_text().unwrap_or(""));

    // List all notes
    println!("\n--- All Notes ---");
    let result = config
        .registry()
        .call("notes_list", serde_json::json!({}))
        .await?;
    println!("{:?}", result[0].as_text().unwrap_or(""));

    // List work notes
    println!("\n--- Work Notes Only ---");
    let result = config
        .registry()
        .call("notes_list", serde_json::json!({"tag": "work"}))
        .await?;
    println!("{:?}", result[0].as_text().unwrap_or(""));

    // Read a specific note
    println!("\n--- Reading 'meeting-notes' ---");
    let result = config
        .registry()
        .call("notes_read", serde_json::json!({"id": "meeting-notes"}))
        .await?;
    println!("{:?}", result[0].as_text().unwrap_or(""));

    // Search notes
    println!("\n--- Search for 'project' ---");
    let result = config
        .registry()
        .call("notes_search", serde_json::json!({"query": "project"}))
        .await?;
    println!("{:?}", result[0].as_text().unwrap_or(""));

    println!("\n=== Example Complete ===");

    // To run as a stdio server, uncomment:
    // McpServer::run(config).await?;

    Ok(())
}
