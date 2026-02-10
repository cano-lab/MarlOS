//! Claude Code Importer - Import conversations from .claude folders in repos
//!
//! Claude Code stores conversations in:
//! - .claude/conversations/*.json within each repo
//!
//! Each conversation JSON contains the full conversation history with Claude.

use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::memory::SecurityTier;
use crate::semantic_object::{ContentType, SemanticObject};

/// Result of an import operation
#[derive(Debug, Clone)]
pub struct ClaudeCodeImportResult {
    /// The imported objects
    pub imported: Vec<SemanticObject>,
    /// Any errors during import
    pub errors: Vec<String>,
    /// Summary message
    pub summary: String,
}

/// Claude Code conversation structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeCodeConversation {
    pub id: String,
    pub title: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub messages: Vec<ClaudeCodeMessage>,
    #[serde(default)]
    pub metadata: ConversationMetadata,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeCodeMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ConversationMetadata {
    pub workspace: Option<String>,
    pub model: Option<String>,
    pub language: Option<String>,
}

impl ClaudeCodeConversation {
    /// Convert conversation to text format for storage
    pub fn to_text(&self) -> String {
        let mut text = String::new();

        // Add header
        text.push_str(&format!("# {}\n\n", self.title.clone().unwrap_or("Untitled Conversation".to_string())));
        if let Some(workspace) = &self.metadata.workspace {
            text.push_str(&format!("Workspace: {}\n", workspace));
        }
        text.push_str("\n");

        // Add messages
        for msg in &self.messages {
            text.push_str(&format!("**[{}]:**\n{}\n\n", msg.role, msg.content));
        }

        text
    }

    /// Get conversation summary
    pub fn summary(&self) -> String {
        let mut summary = String::new();

        if let Some(title) = &self.title {
            summary.push_str(&format!("Title: {}\n", title));
        }

        summary.push_str(&format!("Messages: {}\n", self.messages.len()));

        if let Some(workspace) = &self.metadata.workspace {
            summary.push_str(&format!("Workspace: {}\n", workspace));
        }

        // Get first user message as description
        if let Some(first_user) = self.messages.iter().find(|m| m.role == "user") {
            let preview: String = first_user.content.chars().take(100).collect();
            summary.push_str(&format!("Preview: {}...\n", preview));
        }

        summary
    }

    /// Get all files referenced in conversation
    pub fn all_files(&self) -> Vec<String> {
        self.messages.iter()
            .flat_map(|m| m.files.iter().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }
}

pub struct ClaudeCodeImporter {
    /// Base paths to scan for repos
    scan_paths: Vec<PathBuf>,
}

impl ClaudeCodeImporter {
    pub fn new() -> Self {
        Self {
            scan_paths: Vec::new(),
        }
    }

    /// Add a scan path
    pub fn add_scan_path(&mut self, path: PathBuf) {
        self.scan_paths.push(path);
    }

    /// Create with default scan paths for the current platform
    pub fn with_default_paths() -> Self {
        let mut importer = Self::new();

        // Add common development directories
        if let Some(home) = dirs::home_dir() {
            // Windows: X:\ARCH\Software, C:\Users\...\source, etc.
            #[cfg(windows)]
            {
                // User specified path
                let arch_path = PathBuf::from(r"X:\ARCH\Software");
                if arch_path.exists() {
                    importer.add_scan_path(arch_path);
                }

                // Common Windows source directories
                for base in &["source", "code", "dev", "projects"] {
                    let path = home.join(base);
                    if path.exists() {
                        importer.add_scan_path(path);
                    }
                }
            }

            // Unix/Linux/macOS
            #[cfg(not(windows))]
            {
                for base in &["dev", "code", "projects", "work"] {
                    let path = home.join(base);
                    if path.exists() {
                        importer.add_scan_path(path);
                    }
                }
            }
        }

        importer
    }

    /// Scan for all .claude folders in the scan paths
    pub fn scan_claude_folders(&self) -> Result<Vec<PathBuf>, String> {
        let mut claude_dirs = Vec::new();

        for scan_path in &self.scan_paths {
            if !scan_path.exists() {
                continue;
            }

            // Walk the directory tree looking for .claude folders
            for entry in WalkDir::new(scan_path)
                .max_depth(6) // Don't go too deep
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.file_name() == Some(std::ffi::OsStr::new(".claude")) {
                    claude_dirs.push(path.to_path_buf());
                }
            }
        }

        Ok(claude_dirs)
    }

    /// Load all conversations from a .claude folder
    pub fn load_conversations_from_folder(&self, claude_dir: &Path) -> Result<Vec<ClaudeCodeConversation>, String> {
        let conversations_dir = claude_dir.join("conversations");
        if !conversations_dir.exists() {
            return Ok(Vec::new());
        }

        let mut conversations = Vec::new();

        for entry in fs::read_dir(&conversations_dir)
            .map_err(|e| format!("Cannot read conversations directory: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Cannot read directory entry: {}", e))?;
            let path = entry.path();

            // Only process .json files
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            // Read and parse the conversation
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Cannot read conversation file {:?}: {}", path, e))?;

            let conv: ClaudeCodeConversation = serde_json::from_str(&content)
                .map_err(|e| format!("Cannot parse conversation file {:?}: {}", path, e))?;

            conversations.push(conv);
        }

        Ok(conversations)
    }

    /// Import all conversations from scanned repos
    pub fn import_from_repos(&self) -> ClaudeCodeImportResult {
        let mut imported = Vec::new();
        let mut errors = Vec::new();

        let claude_folders = match self.scan_claude_folders() {
            Ok(folders) => folders,
            Err(e) => {
                return ClaudeCodeImportResult {
                    imported: Vec::new(),
                    errors: vec![e],
                    summary: String::from("Failed to scan for .claude folders"),
                };
            }
        };

        for claude_dir in &claude_folders {
            match self.load_conversations_from_folder(claude_dir) {
                Ok(conversations) => {
                    for conv in conversations {
                        let obj = self.conversation_to_object(conv, claude_dir);
                        imported.push(obj);
                    }
                }
                Err(e) => {
                    errors.push(format!("Error loading from {:?}: {}", claude_dir, e));
                }
            }
        }

        let summary = format!(
            "Scanned {} .claude folders, imported {} conversations",
            claude_folders.len(),
            imported.len()
        );

        ClaudeCodeImportResult {
            imported,
            errors,
            summary,
        }
    }

    /// Convert a Claude Code conversation to a SemanticObject
    fn conversation_to_object(&self, conv: ClaudeCodeConversation, claude_dir: &Path) -> SemanticObject {
        let text = conv.to_text();
        let summary = conv.summary();

        // Get repo name from path (parent of .claude folder)
        let repo_name = claude_dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut obj = SemanticObject::from_text(&text)
            .with_name(&conv.title.clone().unwrap_or(format!("Claude Code: {}", repo_name)))
            .with_summary(&summary)
            .with_tier(SecurityTier::Open);

        // Add tags
        obj.tags = vec![
            "claude-code".to_string(),
            "conversation".to_string(),
            repo_name.clone(),
        ];

        // Add metadata
        if let Some(ref workspace) = conv.metadata.workspace {
            obj.metadata.insert("workspace".to_string(), serde_json::json!(workspace));
        }
        if let Some(ref model) = conv.metadata.model {
            obj.metadata.insert("model".to_string(), serde_json::json!(model));
        }

        // Add files referenced to metadata
        let files = conv.all_files();
        if !files.is_empty() {
            obj.metadata.insert("files".to_string(), serde_json::json!(files));
        }

        // Store path for reference
        obj.metadata.insert("claude_dir".to_string(), serde_json::json!(claude_dir.to_string_lossy().to_string()));

        obj
    }
}

