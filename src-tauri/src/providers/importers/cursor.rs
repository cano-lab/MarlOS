//! Cursor Importer - Import AI conversations from Cursor IDE
//!
//! Cursor stores conversations in:
//! - Windows: %APPDATA%/Cursor/User/workspaceStorage/
//! - macOS: ~/Library/Application Support/Cursor/User/workspaceStorage/
//! - Linux: ~/.config/Cursor/User/workspaceStorage/
//!
//! Each workspace has a state.vscdb SQLite database with conversation history.

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::memory::SecurityTier;
use crate::semantic_object::{ContentType, SemanticObject};
use super::{Importer, ImportResult};

/// Cursor conversation from database
#[derive(Debug, Clone)]
pub struct CursorConversation {
    pub id: String,
    pub workspace_path: Option<String>,
    pub messages: Vec<CursorMessage>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct CursorMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<DateTime<Utc>>,
    /// Files referenced in this message
    pub files: Vec<String>,
}

impl CursorConversation {
    pub fn get_full_text(&self) -> String {
        let mut text = String::new();
        for msg in &self.messages {
            text.push_str(&format!("[{}]: {}\n\n", msg.role, msg.content));
        }
        text
    }

    pub fn generate_summary(&self) -> String {
        let mut summary = String::new();

        if let Some(path) = &self.workspace_path {
            summary.push_str(&format!("Workspace: {}\n", path));
        }

        summary.push_str(&format!("Messages: {}\n", self.messages.len()));

        // Collect all files
        let all_files: std::collections::HashSet<&String> = self.messages.iter()
            .flat_map(|m| m.files.iter())
            .collect();
        if !all_files.is_empty() {
            let file_list: Vec<&str> = all_files.iter()
                .take(5)
                .map(|f| {
                    Path::new(f.as_str())
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(f.as_str())
                })
                .collect();
            summary.push_str(&format!("Files: {}\n", file_list.join(", ")));
        }

        summary
    }

    pub fn all_files(&self) -> Vec<String> {
        self.messages.iter()
            .flat_map(|m| m.files.iter().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }
}

/// Cursor state database entry (simplified)
#[derive(Debug, Deserialize)]
struct CursorStateEntry {
    key: String,
    value: String,
}

pub struct CursorImporter {
    /// Base path to Cursor data (if not using default)
    base_path: Option<PathBuf>,
}

impl CursorImporter {
    pub fn new() -> Self {
        Self { base_path: None }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self { base_path: Some(path) }
    }

    /// Get the default Cursor data directory for the current platform
    pub fn default_cursor_path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            std::env::var("APPDATA")
                .map(|p| PathBuf::from(p).join("Cursor").join("User").join("workspaceStorage"))
                .ok()
        }

        #[cfg(target_os = "macos")]
        {
            dirs::data_dir().map(|d| d.join("Cursor").join("User").join("workspaceStorage"))
        }

        #[cfg(target_os = "linux")]
        {
            dirs::config_dir().map(|d| d.join("Cursor").join("User").join("workspaceStorage"))
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }

    /// Find all workspace storage directories
    pub fn find_workspaces(&self) -> Result<Vec<PathBuf>, String> {
        let base = self.base_path.clone()
            .or_else(Self::default_cursor_path)
            .ok_or_else(|| "Could not determine Cursor data path".to_string())?;

        if !base.exists() {
            return Ok(Vec::new());
        }

        let mut workspaces = Vec::new();

        let entries = std::fs::read_dir(&base)
            .map_err(|e| format!("Failed to read Cursor workspaces: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Check for state.vscdb
                let db_path = path.join("state.vscdb");
                if db_path.exists() {
                    workspaces.push(path);
                }
            }
        }

        Ok(workspaces)
    }

    /// Parse conversations from a workspace
    pub fn parse_workspace(&self, workspace_path: &Path) -> Result<Vec<CursorConversation>, String> {
        let db_path = workspace_path.join("state.vscdb");

        if !db_path.exists() {
            return Err(format!("No state.vscdb found in {:?}", workspace_path));
        }

        // Open SQLite database
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("Failed to open Cursor database: {}", e))?;

        // Try to extract workspace folder from the hash folder name
        let workspace_folder = self.resolve_workspace_path(workspace_path, &conn);

        // Query for AI conversation data
        // Cursor stores data in ItemTable with various key patterns
        let mut conversations = Vec::new();

        // Look for composer/chat history entries
        let keys_to_check = [
            "aiChat.panelChats",
            "composer.composerData",
            "chat.history",
        ];

        for key_pattern in keys_to_check {
            if let Ok(mut stmt) = conn.prepare("SELECT key, value FROM ItemTable WHERE key LIKE ?") {
                let pattern = format!("%{}%", key_pattern);
                if let Ok(rows) = stmt.query_map([&pattern], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                    ))
                }) {
                    for row in rows.flatten() {
                        if let Some(conv) = self.parse_conversation_entry(&row.0, &row.1, &workspace_folder) {
                            conversations.push(conv);
                        }
                    }
                }
            }
        }

        Ok(conversations)
    }

    fn resolve_workspace_path(&self, workspace_dir: &Path, conn: &rusqlite::Connection) -> Option<String> {
        // Try to find workspace.json or similar
        let workspace_json = workspace_dir.join("workspace.json");
        if workspace_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&workspace_json) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(folder) = json.get("folder").and_then(|f| f.as_str()) {
                        return Some(folder.to_string());
                    }
                }
            }
        }

        // Try database for workspace info
        if let Ok(mut stmt) = conn.prepare("SELECT value FROM ItemTable WHERE key = 'workspaceIdentifier'") {
            if let Ok(path) = stmt.query_row([], |row| row.get::<_, String>(0)) {
                return Some(path);
            }
        }

        None
    }

    fn parse_conversation_entry(
        &self,
        _key: &str,
        value: &str,
        workspace_path: &Option<String>,
    ) -> Option<CursorConversation> {
        // Try to parse as JSON
        let json: serde_json::Value = serde_json::from_str(value).ok()?;

        // Handle different formats
        if let Some(chats) = json.as_array() {
            // Array of chat entries
            for chat in chats {
                if let Some(conv) = self.parse_chat_object(chat, workspace_path) {
                    return Some(conv);
                }
            }
        } else if json.is_object() {
            return self.parse_chat_object(&json, workspace_path);
        }

        None
    }

    fn parse_chat_object(
        &self,
        obj: &serde_json::Value,
        workspace_path: &Option<String>,
    ) -> Option<CursorConversation> {
        let mut messages = Vec::new();

        // Try different message array keys
        let message_keys = ["messages", "bubbles", "turns", "history"];

        for key in message_keys {
            if let Some(arr) = obj.get(key).and_then(|v| v.as_array()) {
                for msg in arr {
                    if let Some(parsed) = self.parse_message(msg) {
                        messages.push(parsed);
                    }
                }
            }
        }

        if messages.is_empty() {
            return None;
        }

        let id = obj.get("id")
            .or_else(|| obj.get("chatId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let created_at = obj.get("createdAt")
            .or_else(|| obj.get("timestamp"))
            .and_then(|v| v.as_i64())
            .and_then(|ts| DateTime::from_timestamp(ts / 1000, 0));

        Some(CursorConversation {
            id,
            workspace_path: workspace_path.clone(),
            messages,
            created_at,
            updated_at: created_at,
        })
    }

    fn parse_message(&self, msg: &serde_json::Value) -> Option<CursorMessage> {
        let role = msg.get("role")
            .or_else(|| msg.get("type"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let content = msg.get("text")
            .or_else(|| msg.get("content"))
            .or_else(|| msg.get("message"))
            .and_then(|v| {
                if let Some(s) = v.as_str() {
                    Some(s.to_string())
                } else if let Some(arr) = v.as_array() {
                    // Handle content blocks
                    let texts: Vec<String> = arr.iter()
                        .filter_map(|block| {
                            block.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                        })
                        .collect();
                    Some(texts.join("\n"))
                } else {
                    None
                }
            })?;

        // Extract file references
        let mut files = Vec::new();
        if let Some(context) = msg.get("context").or_else(|| msg.get("codeContext")) {
            if let Some(arr) = context.as_array() {
                for item in arr {
                    if let Some(path) = item.get("path").or_else(|| item.get("uri"))
                        .and_then(|p| p.as_str())
                    {
                        files.push(path.to_string());
                    }
                }
            }
        }

        let timestamp = msg.get("timestamp")
            .or_else(|| msg.get("createdAt"))
            .and_then(|v| v.as_i64())
            .and_then(|ts| DateTime::from_timestamp(ts / 1000, 0));

        Some(CursorMessage {
            role,
            content,
            timestamp,
            files,
        })
    }

    /// Convert conversation to SemanticObjects
    pub fn conversation_to_objects(&self, conv: &CursorConversation) -> Vec<SemanticObject> {
        let mut objects = Vec::new();

        let full_text = conv.get_full_text();
        let summary = conv.generate_summary();

        let mut obj = SemanticObject::new(
            full_text.as_bytes().to_vec(),
            ContentType::Structured { schema: "cursor-conversation".to_string() },
        );

        let title = conv.workspace_path.as_ref()
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("workspace")
                    .to_string()
            })
            .unwrap_or_else(|| "Cursor Session".to_string());

        obj.name = Some(format!("Cursor: {}", title));
        obj.summary = Some(summary);
        obj.security_tier = SecurityTier::Guarded;
        obj.metadata.insert("source".to_string(), serde_json::json!("cursor"));
        obj.metadata.insert("conversation_id".to_string(), serde_json::json!(conv.id));
        obj.metadata.insert("message_count".to_string(), serde_json::json!(conv.messages.len()));
        obj.metadata.insert("files".to_string(), serde_json::json!(conv.all_files()));
        if let Some(path) = &conv.workspace_path {
            obj.metadata.insert("workspace_path".to_string(), serde_json::json!(path));
        }
        obj.tags.push("cursor".to_string());
        obj.tags.push("conversation".to_string());
        obj.tags.push("ai".to_string());
        obj.tags.push("ide".to_string());

        if let Some(created) = conv.created_at {
            obj.created_at = created;
        }
        if let Some(updated) = conv.updated_at {
            obj.modified_at = updated;
        }

        objects.push(obj);
        objects
    }
}

impl Default for CursorImporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Importer for CursorImporter {
    fn source_name(&self) -> &'static str {
        "Cursor"
    }

    fn can_import(&self, path: &Path) -> bool {
        // Can import a workspace directory with .cursor folder
        if path.is_dir() {
            return path.join(".cursor").exists();
        }

        // Can also import a state.vscdb file directly
        if path.is_file() {
            return path.file_name().map_or(false, |n| n == "state.vscdb");
        }

        false
    }

    fn import(&self, path: &Path, _embed: bool) -> Result<ImportResult, String> {
        let mut result = ImportResult::new("Cursor");

        // If given a workspace directory, find the storage location
        let workspaces = if path.is_dir() && path.join(".cursor").exists() {
            // This is a project, find corresponding storage
            self.find_workspaces()?
                .into_iter()
                .filter(|w| {
                    // Match workspace to project (heuristic)
                    self.resolve_workspace_path(w, &rusqlite::Connection::open(w.join("state.vscdb")).ok().as_ref().unwrap_or(&rusqlite::Connection::open_in_memory().unwrap()))
                        .map_or(false, |wp| wp.contains(&path.to_string_lossy().to_string()))
                })
                .collect()
        } else {
            // Given a workspace storage directory directly
            vec![path.to_path_buf()]
        };

        for workspace in workspaces {
            match self.parse_workspace(&workspace) {
                Ok(conversations) => {
                    result.items_imported += conversations.len();
                    for conv in conversations {
                        let objects = self.conversation_to_objects(&conv);
                        result.objects_created += objects.len();
                        result.objects.extend(objects);
                    }
                }
                Err(e) => {
                    result.warnings.push(format!("Failed to parse workspace {:?}: {}", workspace, e));
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_path() {
        let path = CursorImporter::default_cursor_path();
        // Just verify it returns something on supported platforms
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        assert!(path.is_some());
    }

    #[test]
    fn test_importer_detection() {
        let importer = CursorImporter::new();
        assert_eq!(importer.source_name(), "Cursor");
    }
}
