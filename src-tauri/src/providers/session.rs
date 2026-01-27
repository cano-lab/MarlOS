//! Session Provider - Tracks Claude Code sessions natively in MarlOS
//!
//! Features:
//! - Parses Claude Code JSONL session files
//! - Chunks conversations by turn (user + assistant + tools)
//! - Extracts file references (paths, line numbers) from tool calls
//! - Converts to SemanticObjects for vector search
//!
//! Data Model:
//! - Session → contains many ConversationChunks
//! - ConversationChunk → has FileRefs + embedding
//! - FileRef → path, lines, operation type

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::fs;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use regex::Regex;

use crate::memory::SecurityTier;
use crate::semantic_object::{ContentType, SemanticObject, Relation, RelationType};

// ============================================================================
// File Reference - Links conversations to code locations
// ============================================================================

/// A reference to a file location discussed/modified in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FileRef {
    /// File path (absolute or relative)
    pub path: String,
    /// Line range if available (start, end)
    pub lines: Option<(usize, usize)>,
    /// Type of operation performed
    pub operation: FileOperation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FileOperation {
    Read,
    Edit,
    Write,
    Grep,
    Glob,
    Delete,
    Unknown,
}

impl FileRef {
    pub fn new(path: &str, operation: FileOperation) -> Self {
        Self {
            path: path.to_string(),
            lines: None,
            operation,
        }
    }

    pub fn with_lines(mut self, start: usize, end: usize) -> Self {
        self.lines = Some((start, end));
        self
    }

    /// Get just the filename for display
    pub fn filename(&self) -> &str {
        Path::new(&self.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&self.path)
    }
}

// ============================================================================
// Conversation Chunk - A turn in the conversation with code context
// ============================================================================

/// A chunk of conversation (typically one user message + assistant response)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationChunk {
    /// Unique ID for this chunk
    pub id: String,
    /// Parent session ID
    pub session_id: String,
    /// Index within session (0-based)
    pub chunk_index: usize,
    /// User message content
    pub user_content: String,
    /// Assistant response content
    pub assistant_content: String,
    /// Combined content for embedding
    pub full_content: String,
    /// Files that were read during this chunk
    pub files_read: Vec<FileRef>,
    /// Files that were modified during this chunk
    pub files_modified: Vec<FileRef>,
    /// All unique file paths referenced
    pub all_files: Vec<String>,
    /// Tool names used in this chunk
    pub tools_used: Vec<String>,
    /// Timestamp of user message
    pub timestamp: Option<DateTime<Utc>>,
    /// Project path
    pub project_path: Option<String>,
    /// Git branch
    pub git_branch: Option<String>,
}

impl ConversationChunk {
    /// Generate a summary for this chunk
    pub fn generate_summary(&self) -> String {
        let mut summary = String::new();

        // First line of user message
        let user_preview: String = self.user_content
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(100)
            .collect();
        summary.push_str(&format!("User: {}\n", user_preview));

        // Files touched
        if !self.all_files.is_empty() {
            let file_list: Vec<&str> = self.all_files.iter()
                .take(5)
                .map(|f| Path::new(f).file_name().and_then(|s| s.to_str()).unwrap_or(f))
                .collect();
            summary.push_str(&format!("Files: {}\n", file_list.join(", ")));
        }

        // Tools used
        if !self.tools_used.is_empty() {
            let unique_tools: HashSet<_> = self.tools_used.iter().collect();
            summary.push_str(&format!("Tools: {}\n",
                unique_tools.into_iter().cloned().collect::<Vec<_>>().join(", ")));
        }

        summary
    }

    /// Get text optimized for embedding (focused on semantic content)
    pub fn get_embedding_text(&self) -> String {
        let mut text = String::new();

        // User question/request
        text.push_str(&format!("User request: {}\n\n", self.user_content));

        // Assistant response (truncated for embedding)
        let assistant_truncated: String = self.assistant_content.chars().take(2000).collect();
        text.push_str(&format!("Assistant response: {}\n\n", assistant_truncated));

        // File context
        if !self.all_files.is_empty() {
            text.push_str("Files discussed: ");
            text.push_str(&self.all_files.join(", "));
            text.push('\n');
        }

        text
    }
}

// ============================================================================
// Session File Types (Claude Code JSONL format)
// ============================================================================

/// Raw entry from JSONL - we parse this loosely to extract what we need
#[derive(Debug, Clone, Deserialize)]
pub struct RawEntry {
    #[serde(rename = "type")]
    pub entry_type: Option<String>,
    pub uuid: Option<String>,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    pub message: Option<serde_json::Value>,
    pub timestamp: Option<String>,
    // Tool use specific
    pub name: Option<String>,
    pub input: Option<serde_json::Value>,
    pub id: Option<String>,
    // Progress entries have data at top level
    pub data: Option<serde_json::Value>,
}

// ============================================================================
// Chunked Session Parser
// ============================================================================

/// A fully parsed session with chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkedSession {
    /// Session UUID
    pub id: String,
    /// Project path
    pub project_path: Option<String>,
    /// Git branch
    pub git_branch: Option<String>,
    /// Conversation chunks
    pub chunks: Vec<ConversationChunk>,
    /// All unique files referenced across all chunks
    pub all_files: Vec<String>,
    /// Total message count
    pub message_count: usize,
    /// Session start time
    pub started_at: Option<DateTime<Utc>>,
    /// Session end time
    pub ended_at: Option<DateTime<Utc>>,
    /// Source file path
    pub source_file: String,
}

impl ChunkedSession {
    pub fn generate_summary(&self) -> String {
        let mut summary = String::new();

        if let Some(path) = &self.project_path {
            summary.push_str(&format!("Project: {}\n", path));
        }
        if let Some(branch) = &self.git_branch {
            summary.push_str(&format!("Branch: {}\n", branch));
        }

        summary.push_str(&format!("Chunks: {}\n", self.chunks.len()));
        summary.push_str(&format!("Files touched: {}\n", self.all_files.len()));

        if let (Some(start), Some(end)) = (self.started_at, self.ended_at) {
            let duration = end - start;
            summary.push_str(&format!("Duration: {} minutes\n", duration.num_minutes()));
        }

        summary
    }
}

pub struct ChunkedSessionParser;

impl ChunkedSessionParser {
    /// Parse a JSONL session file into chunks
    pub fn parse_file(path: &Path) -> Result<ChunkedSession, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read session file: {}", e))?;

        let session_id = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // First pass: collect all entries
        let mut entries: Vec<RawEntry> = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<RawEntry>(line) {
                entries.push(entry);
            }
        }

        // Extract project info from first user message
        let mut project_path: Option<String> = None;
        let mut git_branch: Option<String> = None;
        for entry in &entries {
            if entry.entry_type.as_deref() == Some("user") {
                if project_path.is_none() {
                    project_path = entry.cwd.clone();
                    git_branch = entry.git_branch.clone();
                    break;
                }
            }
        }

        // Second pass: build chunks
        let chunks = Self::build_chunks(&entries, &session_id, &project_path, &git_branch);

        // Collect all files
        let mut all_files: HashSet<String> = HashSet::new();
        let mut first_timestamp: Option<DateTime<Utc>> = None;
        let mut last_timestamp: Option<DateTime<Utc>> = None;

        for chunk in &chunks {
            all_files.extend(chunk.all_files.iter().cloned());
            if let Some(ts) = chunk.timestamp {
                if first_timestamp.is_none() || first_timestamp.unwrap() > ts {
                    first_timestamp = Some(ts);
                }
                if last_timestamp.is_none() || last_timestamp.unwrap() < ts {
                    last_timestamp = Some(ts);
                }
            }
        }

        let message_count = chunks.len() * 2; // Approximate

        Ok(ChunkedSession {
            id: session_id,
            project_path,
            git_branch,
            chunks,
            all_files: all_files.into_iter().collect(),
            message_count,
            started_at: first_timestamp,
            ended_at: last_timestamp,
            source_file: path.to_string_lossy().to_string(),
        })
    }

    fn build_chunks(
        entries: &[RawEntry],
        session_id: &str,
        project_path: &Option<String>,
        git_branch: &Option<String>,
    ) -> Vec<ConversationChunk> {
        let mut chunks = Vec::new();
        let mut current_user_content = String::new();
        let mut current_assistant_content = String::new();
        let mut current_files_read: Vec<FileRef> = Vec::new();
        let mut current_files_modified: Vec<FileRef> = Vec::new();
        let mut current_tools: Vec<String> = Vec::new();
        let mut current_timestamp: Option<DateTime<Utc>> = None;
        let mut chunk_index = 0;

        for entry in entries {
            match entry.entry_type.as_deref() {
                Some("user") => {
                    // If we have accumulated content, save the previous chunk
                    if !current_user_content.is_empty() || !current_assistant_content.is_empty() {
                        let chunk = Self::create_chunk(
                            session_id,
                            chunk_index,
                            &current_user_content,
                            &current_assistant_content,
                            &current_files_read,
                            &current_files_modified,
                            &current_tools,
                            current_timestamp,
                            project_path,
                            git_branch,
                        );
                        chunks.push(chunk);
                        chunk_index += 1;
                    }

                    // Start new chunk
                    current_user_content = Self::extract_message_text(&entry.message);
                    current_assistant_content.clear();
                    current_files_read.clear();
                    current_files_modified.clear();
                    current_tools.clear();
                    current_timestamp = entry.timestamp.as_ref()
                        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                        .map(|dt| dt.with_timezone(&Utc));
                }
                Some("assistant") => {
                    let text = Self::extract_message_text(&entry.message);
                    if !text.is_empty() {
                        if !current_assistant_content.is_empty() {
                            current_assistant_content.push_str("\n\n");
                        }
                        current_assistant_content.push_str(&text);
                    }

                    // Also extract tool_use from content blocks in assistant messages
                    if let Some(msg) = &entry.message {
                        let (tools, files_r, files_m) = Self::extract_tools_from_message(msg);
                        current_tools.extend(tools);
                        current_files_read.extend(files_r);
                        current_files_modified.extend(files_m);
                    }
                }
                Some("tool_use") => {
                    if let Some(name) = &entry.name {
                        current_tools.push(name.clone());

                        // Extract file references from tool input
                        if let Some(input) = &entry.input {
                            let file_refs = Self::extract_file_refs(name, input);
                            for file_ref in file_refs {
                                match file_ref.operation {
                                    FileOperation::Read | FileOperation::Grep | FileOperation::Glob => {
                                        current_files_read.push(file_ref);
                                    }
                                    FileOperation::Edit | FileOperation::Write | FileOperation::Delete => {
                                        current_files_modified.push(file_ref);
                                    }
                                    FileOperation::Unknown => {
                                        current_files_read.push(file_ref);
                                    }
                                }
                            }
                        }
                    }
                }
                Some("progress") => {
                    // Progress entries have data at top level containing nested message
                    if let Some(data) = &entry.data {
                        if let Some(nested_msg) = data.get("message") {
                            let (tools, files_r, files_m) = Self::extract_tools_from_message(nested_msg);
                            current_tools.extend(tools);
                            current_files_read.extend(files_r);
                            current_files_modified.extend(files_m);
                        }
                    }
                }
                _ => {}
            }
        }

        // Don't forget the last chunk
        if !current_user_content.is_empty() || !current_assistant_content.is_empty() {
            let chunk = Self::create_chunk(
                session_id,
                chunk_index,
                &current_user_content,
                &current_assistant_content,
                &current_files_read,
                &current_files_modified,
                &current_tools,
                current_timestamp,
                project_path,
                git_branch,
            );
            chunks.push(chunk);
        }

        chunks
    }

    fn create_chunk(
        session_id: &str,
        chunk_index: usize,
        user_content: &str,
        assistant_content: &str,
        files_read: &[FileRef],
        files_modified: &[FileRef],
        tools: &[String],
        timestamp: Option<DateTime<Utc>>,
        project_path: &Option<String>,
        git_branch: &Option<String>,
    ) -> ConversationChunk {
        // Collect all unique file paths
        let mut all_files: HashSet<String> = HashSet::new();
        for f in files_read {
            all_files.insert(f.path.clone());
        }
        for f in files_modified {
            all_files.insert(f.path.clone());
        }

        let full_content = format!(
            "[User]: {}\n\n[Assistant]: {}",
            user_content,
            assistant_content
        );

        ConversationChunk {
            id: format!("{}-chunk-{}", session_id, chunk_index),
            session_id: session_id.to_string(),
            chunk_index,
            user_content: user_content.to_string(),
            assistant_content: assistant_content.to_string(),
            full_content,
            files_read: files_read.to_vec(),
            files_modified: files_modified.to_vec(),
            all_files: all_files.into_iter().collect(),
            tools_used: tools.to_vec(),
            timestamp,
            project_path: project_path.clone(),
            git_branch: git_branch.clone(),
        }
    }

    fn extract_message_text(message: &Option<serde_json::Value>) -> String {
        if let Some(msg) = message {
            // Try simple format: { "role": "user", "content": "text" }
            if let Some(content) = msg.get("content") {
                if let Some(text) = content.as_str() {
                    return text.to_string();
                }
                // Complex format: { "content": [{ "type": "text", "text": "..." }] }
                if let Some(arr) = content.as_array() {
                    let texts: Vec<String> = arr.iter()
                        .filter_map(|block| {
                            if block.get("type")?.as_str()? == "text" {
                                block.get("text")?.as_str().map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                        .collect();
                    return texts.join("\n");
                }
            }
        }
        String::new()
    }

    fn extract_file_refs(tool_name: &str, input: &serde_json::Value) -> Vec<FileRef> {
        let mut refs = Vec::new();

        match tool_name {
            "Read" => {
                if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                    let mut file_ref = FileRef::new(path, FileOperation::Read);
                    // Extract line numbers if present
                    if let (Some(offset), Some(limit)) = (
                        input.get("offset").and_then(|v| v.as_u64()),
                        input.get("limit").and_then(|v| v.as_u64())
                    ) {
                        file_ref.lines = Some((offset as usize, (offset + limit) as usize));
                    }
                    refs.push(file_ref);
                }
            }
            "Edit" => {
                if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                    let file_ref = FileRef::new(path, FileOperation::Edit);
                    // Note: Edit doesn't have explicit line numbers, but we could
                    // potentially extract them from old_string matching
                    refs.push(file_ref);
                }
            }
            "Write" => {
                if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                    refs.push(FileRef::new(path, FileOperation::Write));
                }
            }
            "Grep" => {
                if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                    refs.push(FileRef::new(path, FileOperation::Grep));
                }
            }
            "Glob" => {
                if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                    refs.push(FileRef::new(path, FileOperation::Glob));
                }
            }
            "Bash" => {
                // Try to extract file paths from bash commands
                if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                    refs.extend(Self::extract_files_from_bash(cmd));
                }
            }
            _ => {}
        }

        refs
    }

    fn extract_files_from_bash(command: &str) -> Vec<FileRef> {
        let mut refs = Vec::new();

        // Common patterns for file paths in bash commands
        // This is a heuristic - won't catch everything

        // Match paths that look like file paths
        let path_regex = Regex::new(r#"(?:^|[\s"'])(/[^\s"']+|[A-Za-z]:\\[^\s"']+)"#).ok();

        if let Some(re) = path_regex {
            for cap in re.captures_iter(command) {
                if let Some(path) = cap.get(1) {
                    let path_str = path.as_str();
                    // Skip common non-file paths
                    if !path_str.starts_with("/dev/")
                        && !path_str.starts_with("/proc/")
                        && !path_str.starts_with("/tmp/")
                        && path_str.contains('.')
                    {
                        refs.push(FileRef::new(path_str, FileOperation::Unknown));
                    }
                }
            }
        }

        refs
    }

    /// Extract tools and file refs from a message JSON (handles nested content blocks)
    fn extract_tools_from_message(message: &serde_json::Value) -> (Vec<String>, Vec<FileRef>, Vec<FileRef>) {
        let mut tools = Vec::new();
        let mut files_read = Vec::new();
        let mut files_modified = Vec::new();

        // Check message.content for tool_use blocks
        if let Some(content) = message.get("content") {
            if let Some(arr) = content.as_array() {
                for block in arr {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                            tools.push(name.to_string());

                            // Extract file refs from input
                            if let Some(input) = block.get("input") {
                                let file_refs = Self::extract_file_refs(name, input);
                                for file_ref in file_refs {
                                    match file_ref.operation {
                                        FileOperation::Read | FileOperation::Grep | FileOperation::Glob => {
                                            files_read.push(file_ref);
                                        }
                                        FileOperation::Edit | FileOperation::Write | FileOperation::Delete => {
                                            files_modified.push(file_ref);
                                        }
                                        FileOperation::Unknown => {
                                            files_read.push(file_ref);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Also check for direct tool_use at message level (some formats)
        if message.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
            if let Some(name) = message.get("name").and_then(|n| n.as_str()) {
                tools.push(name.to_string());
                if let Some(input) = message.get("input") {
                    let file_refs = Self::extract_file_refs(name, input);
                    for file_ref in file_refs {
                        match file_ref.operation {
                            FileOperation::Read | FileOperation::Grep | FileOperation::Glob => {
                                files_read.push(file_ref);
                            }
                            FileOperation::Edit | FileOperation::Write | FileOperation::Delete => {
                                files_modified.push(file_ref);
                            }
                            FileOperation::Unknown => {
                                files_read.push(file_ref);
                            }
                        }
                    }
                }
            }
        }

        (tools, files_read, files_modified)
    }
}

// ============================================================================
// Legacy ParsedSession (for backwards compatibility)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSession {
    pub id: String,
    pub project_path: Option<String>,
    pub git_branch: Option<String>,
    pub user_messages: Vec<ParsedMessage>,
    pub assistant_messages: Vec<ParsedMessage>,
    pub message_count: usize,
    pub tool_calls: Vec<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
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
    pub fn generate_summary(&self) -> String {
        let mut summary = String::new();
        if let Some(path) = &self.project_path {
            summary.push_str(&format!("Project: {}\n", path));
        }
        if let Some(branch) = &self.git_branch {
            summary.push_str(&format!("Branch: {}\n", branch));
        }
        summary.push_str(&format!("Messages: {} user, {} assistant\n",
            self.user_messages.len(),
            self.assistant_messages.len()));
        summary
    }

    pub fn get_full_text(&self) -> String {
        let mut text = String::new();
        let mut all_messages: Vec<&ParsedMessage> = self.user_messages.iter()
            .chain(self.assistant_messages.iter())
            .collect();
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
// Session Parser (legacy, for backwards compatibility)
// ============================================================================

pub struct SessionParser;

impl SessionParser {
    pub fn parse_file(path: &Path) -> Result<ParsedSession, String> {
        // Use chunked parser and convert to legacy format
        let chunked = ChunkedSessionParser::parse_file(path)?;

        let mut user_messages = Vec::new();
        let mut assistant_messages = Vec::new();
        let mut tool_calls = Vec::new();

        for (i, chunk) in chunked.chunks.iter().enumerate() {
            if !chunk.user_content.is_empty() {
                user_messages.push(ParsedMessage {
                    uuid: format!("{}-user-{}", chunked.id, i),
                    role: "user".to_string(),
                    content: chunk.user_content.clone(),
                    timestamp: chunk.timestamp,
                    parent_uuid: None,
                });
            }
            if !chunk.assistant_content.is_empty() {
                assistant_messages.push(ParsedMessage {
                    uuid: format!("{}-assistant-{}", chunked.id, i),
                    role: "assistant".to_string(),
                    content: chunk.assistant_content.clone(),
                    timestamp: chunk.timestamp,
                    parent_uuid: None,
                });
            }
            tool_calls.extend(chunk.tools_used.iter().cloned());
        }

        let message_count = user_messages.len() + assistant_messages.len();

        Ok(ParsedSession {
            id: chunked.id,
            project_path: chunked.project_path,
            git_branch: chunked.git_branch,
            user_messages,
            assistant_messages,
            message_count,
            tool_calls,
            started_at: chunked.started_at,
            ended_at: chunked.ended_at,
            source_file: chunked.source_file,
        })
    }
}

// ============================================================================
// Session Scanner
// ============================================================================

pub struct SessionScanner {
    base_path: PathBuf,
}

impl SessionScanner {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            base_path: home.join(".claude").join("projects"),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self { base_path: path }
    }

    pub fn find_all_sessions(&self) -> Result<Vec<PathBuf>, String> {
        let mut sessions = Vec::new();

        if !self.base_path.exists() {
            return Ok(sessions);
        }

        let entries = fs::read_dir(&self.base_path)
            .map_err(|e| format!("Failed to read projects directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
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
}

impl Default for SessionScanner {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Convert Chunks to SemanticObjects
// ============================================================================

/// Convert a chunked session to SemanticObjects (one per chunk + one for session)
pub fn chunked_session_to_objects(session: &ChunkedSession) -> Vec<SemanticObject> {
    let mut objects = Vec::new();

    // Create session-level object
    let session_summary = session.generate_summary();
    let mut session_obj = SemanticObject::new(
        session_summary.as_bytes().to_vec(),
        ContentType::Structured { schema: "claude-session".to_string() },
    );
    session_obj.name = Some(format!("Session: {}", session.id));
    session_obj.summary = Some(session_summary.clone());
    session_obj.security_tier = SecurityTier::Guarded;
    session_obj.metadata.insert("session_id".to_string(), serde_json::json!(session.id));
    session_obj.metadata.insert("chunk_count".to_string(), serde_json::json!(session.chunks.len()));
    session_obj.metadata.insert("all_files".to_string(), serde_json::json!(session.all_files));
    if let Some(path) = &session.project_path {
        session_obj.metadata.insert("project_path".to_string(), serde_json::json!(path));
    }
    if let Some(branch) = &session.git_branch {
        session_obj.metadata.insert("git_branch".to_string(), serde_json::json!(branch));
    }
    session_obj.tags.push("session".to_string());
    session_obj.tags.push("claude-code".to_string());

    if let Some(started) = session.started_at {
        session_obj.created_at = started;
    }
    if let Some(ended) = session.ended_at {
        session_obj.modified_at = ended;
    }

    let session_suid = session_obj.suid;
    objects.push(session_obj);

    // Create chunk objects
    for chunk in &session.chunks {
        let embedding_text = chunk.get_embedding_text();
        let mut chunk_obj = SemanticObject::new(
            chunk.full_content.as_bytes().to_vec(),
            ContentType::Structured { schema: "conversation-chunk".to_string() },
        );

        chunk_obj.name = Some(format!("Chunk {}: {}", chunk.chunk_index,
            chunk.user_content.chars().take(50).collect::<String>()));
        chunk_obj.summary = Some(chunk.generate_summary());
        chunk_obj.security_tier = SecurityTier::Guarded;

        // Store file references in metadata
        chunk_obj.metadata.insert("chunk_id".to_string(), serde_json::json!(chunk.id));
        chunk_obj.metadata.insert("session_id".to_string(), serde_json::json!(chunk.session_id));
        chunk_obj.metadata.insert("chunk_index".to_string(), serde_json::json!(chunk.chunk_index));
        chunk_obj.metadata.insert("files_read".to_string(), serde_json::json!(chunk.files_read));
        chunk_obj.metadata.insert("files_modified".to_string(), serde_json::json!(chunk.files_modified));
        chunk_obj.metadata.insert("all_files".to_string(), serde_json::json!(chunk.all_files));
        chunk_obj.metadata.insert("tools_used".to_string(), serde_json::json!(chunk.tools_used));
        chunk_obj.metadata.insert("embedding_text".to_string(), serde_json::json!(embedding_text));

        if let Some(path) = &chunk.project_path {
            chunk_obj.metadata.insert("project_path".to_string(), serde_json::json!(path));
        }
        if let Some(branch) = &chunk.git_branch {
            chunk_obj.metadata.insert("git_branch".to_string(), serde_json::json!(branch));
        }

        // Tags
        chunk_obj.tags.push("chunk".to_string());
        chunk_obj.tags.push("conversation".to_string());
        for file in &chunk.all_files {
            // Add file-based tags for filtering
            if let Some(ext) = Path::new(file).extension().and_then(|e| e.to_str()) {
                chunk_obj.tags.push(format!("ext:{}", ext));
            }
        }

        // Link to parent session
        chunk_obj.relations.push(Relation::new(session_suid, RelationType::Contains));

        if let Some(ts) = chunk.timestamp {
            chunk_obj.created_at = ts;
            chunk_obj.modified_at = ts;
        }

        objects.push(chunk_obj);
    }

    objects
}

/// Legacy function for backwards compatibility
pub fn session_to_objects(session: &ParsedSession) -> Vec<SemanticObject> {
    let mut objects = Vec::new();

    let full_text = session.get_full_text();
    let summary = session.generate_summary();

    let mut session_obj = SemanticObject::new(
        full_text.as_bytes().to_vec(),
        ContentType::Structured { schema: "claude-session".to_string() },
    );

    session_obj.name = Some(format!("Session: {}", session.id));
    session_obj.summary = Some(summary);
    session_obj.security_tier = SecurityTier::Guarded;
    session_obj.metadata.insert("session_id".to_string(), serde_json::json!(session.id));
    if let Some(path) = &session.project_path {
        session_obj.metadata.insert("project_path".to_string(), serde_json::json!(path));
    }
    if let Some(branch) = &session.git_branch {
        session_obj.metadata.insert("git_branch".to_string(), serde_json::json!(branch));
    }
    session_obj.tags.push("session".to_string());
    session_obj.tags.push("claude-code".to_string());

    if let Some(started) = session.started_at {
        session_obj.created_at = started;
    }
    if let Some(ended) = session.ended_at {
        session_obj.modified_at = ended;
    }

    objects.push(session_obj);
    objects
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_ref_creation() {
        let file_ref = FileRef::new("/path/to/file.rs", FileOperation::Read)
            .with_lines(10, 50);

        assert_eq!(file_ref.path, "/path/to/file.rs");
        assert_eq!(file_ref.lines, Some((10, 50)));
        assert_eq!(file_ref.operation, FileOperation::Read);
        assert_eq!(file_ref.filename(), "file.rs");
    }

    #[test]
    fn test_extract_file_refs_read() {
        let input = serde_json::json!({
            "file_path": "/src/main.rs",
            "offset": 100,
            "limit": 50
        });

        let refs = ChunkedSessionParser::extract_file_refs("Read", &input);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].path, "/src/main.rs");
        assert_eq!(refs[0].lines, Some((100, 150)));
    }

    #[test]
    fn test_extract_file_refs_edit() {
        let input = serde_json::json!({
            "file_path": "X:\\ARCH\\Software\\test.rs",
            "old_string": "foo",
            "new_string": "bar"
        });

        let refs = ChunkedSessionParser::extract_file_refs("Edit", &input);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].operation, FileOperation::Edit);
    }

    #[test]
    fn test_session_scanner() {
        let scanner = SessionScanner::new();
        // Just verify it doesn't panic
        let _ = scanner.find_all_sessions();
    }
}
