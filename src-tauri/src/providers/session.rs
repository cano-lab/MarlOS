//! Session Provider - Tracks Claude Code sessions natively in MarlOS
//!
//! Watches ~/.claude/projects/ for JSONL session files and imports them
//! as SemanticObjects for unified search and memory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::memory::SecurityTier;
use crate::semantic_object::{ContentType, SemanticObject, Suid, Relation, RelationType};

// ============================================================================
// Session File Types (Claude Code JSONL format)
// ============================================================================

/// A single entry in the JSONL session file
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum SessionEntry {
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "system")]
    System(SystemMessage),
    #[serde(rename = "tool_use")]
    ToolUse(ToolUseEntry),
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultEntry),
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserMessage {
    pub uuid: String,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    pub message: MessageContent,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantMessage {
    pub uuid: String,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub message: MessageContent,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SystemMessage {
    pub uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub message: Option<MessageContent>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolUseEntry {
    pub uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub name: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolResultEntry {
    pub uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Simple { role: String, content: String },
    Complex { role: String, content: Vec<ContentBlock> },
}

impl MessageContent {
    pub fn get_text(&self) -> String {
        match self {
            MessageContent::Simple { content, .. } => content.clone(),
            MessageContent::Complex { content, .. } => {
                content.iter()
                    .filter_map(|block| {
                        match block {
                            ContentBlock::Text { text } => Some(text.clone()),
                            _ => None,
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }

    pub fn role(&self) -> &str {
        match self {
            MessageContent::Simple { role, .. } => role,
            MessageContent::Complex { role, .. } => role,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: Option<String>, name: Option<String> },
    #[serde(rename = "tool_result")]
    ToolResult { tool_use_id: Option<String> },
    #[serde(other)]
    Other,
}

// ============================================================================
// Session Index (sessions-index.json)
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct SessionsIndex {
    pub version: i32,
    pub entries: Vec<serde_json::Value>,
    #[serde(rename = "originalPath")]
    pub original_path: Option<String>,
}

// ============================================================================
// Parsed Session
// ============================================================================

/// A fully parsed Claude Code session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSession {
    /// Session UUID (from filename)
    pub id: String,
    /// Project path (from cwd or originalPath)
    pub project_path: Option<String>,
    /// Git branch
    pub git_branch: Option<String>,
    /// All user messages
    pub user_messages: Vec<ParsedMessage>,
    /// All assistant messages
    pub assistant_messages: Vec<ParsedMessage>,
    /// Total message count
    pub message_count: usize,
    /// Tool calls made
    pub tool_calls: Vec<String>,
    /// Session start time
    pub started_at: Option<DateTime<Utc>>,
    /// Session end time (last message)
    pub ended_at: Option<DateTime<Utc>>,
    /// Source file path
    pub source_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedMessage {
    pub uuid: String,
    pub role: String,
    pub content: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub parent_uuid: Option<String>,
}

impl ParsedSession {
    /// Generate a summary of the session
    pub fn generate_summary(&self) -> String {
        let mut summary = String::new();

        // Add project info
        if let Some(path) = &self.project_path {
            summary.push_str(&format!("Project: {}\n", path));
        }
        if let Some(branch) = &self.git_branch {
            summary.push_str(&format!("Branch: {}\n", branch));
        }

        summary.push_str(&format!("Messages: {} user, {} assistant\n",
            self.user_messages.len(),
            self.assistant_messages.len()));

        if !self.tool_calls.is_empty() {
            let unique_tools: std::collections::HashSet<_> = self.tool_calls.iter().collect();
            summary.push_str(&format!("Tools used: {}\n", unique_tools.into_iter().cloned().collect::<Vec<_>>().join(", ")));
        }

        // First user message as context
        if let Some(first) = self.user_messages.first() {
            let preview: String = first.content.chars().take(200).collect();
            summary.push_str(&format!("\nFirst message: {}...\n", preview));
        }

        summary
    }

    /// Get all text content for embedding
    pub fn get_full_text(&self) -> String {
        let mut text = String::new();

        // Combine user and assistant messages in order
        let mut all_messages: Vec<&ParsedMessage> = self.user_messages.iter()
            .chain(self.assistant_messages.iter())
            .collect();

        // Sort by timestamp if available
        all_messages.sort_by(|a, b| {
            match (&a.timestamp, &b.timestamp) {
                (Some(ta), Some(tb)) => ta.cmp(tb),
                _ => std::cmp::Ordering::Equal,
            }
        });

        for msg in all_messages {
            text.push_str(&format!("[{}]: {}\n\n", msg.role, msg.content));
        }

        text
    }
}

// ============================================================================
// Session Parser
// ============================================================================

pub struct SessionParser;

impl SessionParser {
    /// Parse a JSONL session file
    pub fn parse_file(path: &Path) -> Result<ParsedSession, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read session file: {}", e))?;

        let session_id = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut user_messages = Vec::new();
        let mut assistant_messages = Vec::new();
        let mut tool_calls = Vec::new();
        let mut project_path: Option<String> = None;
        let mut git_branch: Option<String> = None;
        let mut first_timestamp: Option<DateTime<Utc>> = None;
        let mut last_timestamp: Option<DateTime<Utc>> = None;

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }

            // Try to parse as a session entry
            match serde_json::from_str::<SessionEntry>(line) {
                Ok(entry) => {
                    match entry {
                        SessionEntry::User(msg) => {
                            // Extract project info from first user message
                            if project_path.is_none() {
                                project_path = msg.cwd.clone();
                                git_branch = msg.git_branch.clone();
                            }

                            let timestamp = msg.timestamp.as_ref()
                                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                                .map(|dt| dt.with_timezone(&Utc));

                            Self::update_timestamps(&mut first_timestamp, &mut last_timestamp, timestamp);

                            user_messages.push(ParsedMessage {
                                uuid: msg.uuid,
                                role: "user".to_string(),
                                content: msg.message.get_text(),
                                timestamp,
                                parent_uuid: msg.parent_uuid,
                            });
                        }
                        SessionEntry::Assistant(msg) => {
                            let timestamp = msg.timestamp.as_ref()
                                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                                .map(|dt| dt.with_timezone(&Utc));

                            Self::update_timestamps(&mut first_timestamp, &mut last_timestamp, timestamp);

                            assistant_messages.push(ParsedMessage {
                                uuid: msg.uuid,
                                role: "assistant".to_string(),
                                content: msg.message.get_text(),
                                timestamp,
                                parent_uuid: msg.parent_uuid,
                            });
                        }
                        SessionEntry::ToolUse(tool) => {
                            if let Some(name) = tool.name {
                                tool_calls.push(name);
                            }
                        }
                        _ => {}
                    }
                }
                Err(_) => {
                    // Skip unparseable lines (progress updates, etc.)
                    continue;
                }
            }
        }

        let message_count = user_messages.len() + assistant_messages.len();

        Ok(ParsedSession {
            id: session_id,
            project_path,
            git_branch,
            user_messages,
            assistant_messages,
            message_count,
            tool_calls,
            started_at: first_timestamp,
            ended_at: last_timestamp,
            source_file: path.to_string_lossy().to_string(),
        })
    }

    fn update_timestamps(
        first: &mut Option<DateTime<Utc>>,
        last: &mut Option<DateTime<Utc>>,
        timestamp: Option<DateTime<Utc>>,
    ) {
        if let Some(ts) = timestamp {
            if first.is_none() || first.unwrap() > ts {
                *first = Some(ts);
            }
            if last.is_none() || last.unwrap() < ts {
                *last = Some(ts);
            }
        }
    }
}

// ============================================================================
// Session Scanner - Finds all sessions in ~/.claude/projects/
// ============================================================================

pub struct SessionScanner {
    base_path: PathBuf,
}

impl SessionScanner {
    /// Create a new scanner with the default Claude projects path
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            base_path: home.join(".claude").join("projects"),
        }
    }

    /// Create with custom base path
    pub fn with_path(path: PathBuf) -> Self {
        Self { base_path: path }
    }

    /// Find all session JSONL files
    pub fn find_all_sessions(&self) -> Result<Vec<PathBuf>, String> {
        let mut sessions = Vec::new();

        if !self.base_path.exists() {
            return Ok(sessions);
        }

        // Iterate through project directories
        let entries = fs::read_dir(&self.base_path)
            .map_err(|e| format!("Failed to read projects directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                // Look for .jsonl files in this project directory
                if let Ok(files) = fs::read_dir(&path) {
                    for file in files {
                        if let Ok(file) = file {
                            let file_path = file.path();
                            if file_path.extension().map_or(false, |ext| ext == "jsonl") {
                                sessions.push(file_path);
                            }
                        }
                    }
                }
            }
        }

        Ok(sessions)
    }

    /// Find sessions modified after a given time
    pub fn find_sessions_since(&self, since: DateTime<Utc>) -> Result<Vec<PathBuf>, String> {
        let all = self.find_all_sessions()?;
        let since_ts = since.timestamp();

        Ok(all.into_iter()
            .filter(|path| {
                path.metadata()
                    .and_then(|m| m.modified())
                    .map(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64 > since_ts)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            })
            .collect())
    }

    /// Get project info from sessions-index.json
    pub fn get_project_info(&self, project_dir: &Path) -> Option<SessionsIndex> {
        let index_path = project_dir.join("sessions-index.json");
        if index_path.exists() {
            fs::read_to_string(&index_path)
                .ok()
                .and_then(|content| serde_json::from_str(&content).ok())
        } else {
            None
        }
    }
}

impl Default for SessionScanner {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Session to SemanticObject Conversion
// ============================================================================

/// Convert a parsed session to SemanticObjects
pub fn session_to_objects(session: &ParsedSession) -> Vec<SemanticObject> {
    let mut objects = Vec::new();

    // Create the main session object
    let full_text = session.get_full_text();
    let summary = session.generate_summary();

    let mut session_obj = SemanticObject::new(
        full_text.as_bytes().to_vec(),
        ContentType::Structured { schema: "claude-session".to_string() },
    );

    session_obj.name = Some(format!("Session: {}", session.id));
    session_obj.summary = Some(summary);
    session_obj.security_tier = SecurityTier::Guarded; // Sessions may contain sensitive code

    // Add metadata
    session_obj.metadata.insert(
        "session_id".to_string(),
        serde_json::json!(session.id),
    );
    if let Some(path) = &session.project_path {
        session_obj.metadata.insert(
            "project_path".to_string(),
            serde_json::json!(path),
        );
    }
    if let Some(branch) = &session.git_branch {
        session_obj.metadata.insert(
            "git_branch".to_string(),
            serde_json::json!(branch),
        );
    }
    session_obj.metadata.insert(
        "message_count".to_string(),
        serde_json::json!(session.message_count),
    );
    session_obj.metadata.insert(
        "source_file".to_string(),
        serde_json::json!(session.source_file),
    );

    // Tags for discoverability
    session_obj.tags.push("session".to_string());
    session_obj.tags.push("claude-code".to_string());
    if let Some(branch) = &session.git_branch {
        session_obj.tags.push(format!("branch:{}", branch));
    }

    // Set timestamps
    if let Some(started) = session.started_at {
        session_obj.created_at = started;
    }
    if let Some(ended) = session.ended_at {
        session_obj.modified_at = ended;
    }

    let session_suid = session_obj.suid;
    objects.push(session_obj);

    // Optionally create separate objects for significant messages
    // (for now, we just embed the full session)
    // In the future, we could extract individual messages for fine-grained search

    objects
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_content_simple() {
        let json = r#"{"role": "user", "content": "Hello world"}"#;
        let msg: MessageContent = serde_json::from_str(json).unwrap();
        assert_eq!(msg.get_text(), "Hello world");
        assert_eq!(msg.role(), "user");
    }

    #[test]
    fn test_message_content_complex() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "text", "text": "First part"},
                {"type": "text", "text": "Second part"}
            ]
        }"#;
        let msg: MessageContent = serde_json::from_str(json).unwrap();
        assert_eq!(msg.get_text(), "First part\nSecond part");
    }

    #[test]
    fn test_session_scanner_path() {
        let scanner = SessionScanner::new();
        // Just verify it doesn't panic
        let _ = scanner.find_all_sessions();
    }
}
