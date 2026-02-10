//! Universal Importers - Import AI conversations and knowledge from external tools
//!
//! Supports:
//! - ChatGPT (OpenAI conversation exports)
//! - Cursor (AI IDE sessions)
//! - Obsidian (Markdown vault with wikilinks)
//! - Browser Sync (Real-time ChatGPT from browser storage)
//! - Claude Code (Conversations from .claude folders in repos)

pub mod chatgpt;
pub mod cursor;
pub mod obsidian;
pub mod browser_sync;
pub mod claude_code;

pub use chatgpt::*;
pub use cursor::*;
pub use obsidian::*;
pub use browser_sync::*;
pub use claude_code::*;

use std::path::Path;
use crate::semantic_object::SemanticObject;

/// Common trait for all importers
pub trait Importer {
    /// The source name (e.g., "ChatGPT", "Cursor", "Obsidian")
    fn source_name(&self) -> &'static str;

    /// Check if this importer can handle the given path
    fn can_import(&self, path: &Path) -> bool;

    /// Import content and return SemanticObjects
    fn import(&self, path: &Path, embed: bool) -> Result<ImportResult, String>;
}

/// Result of an import operation
#[derive(Debug, Clone)]
pub struct ImportResult {
    /// Source identifier (e.g., "ChatGPT", "Cursor")
    pub source: String,
    /// Number of conversations/documents imported
    pub items_imported: usize,
    /// Number of SemanticObjects created
    pub objects_created: usize,
    /// Any warnings during import
    pub warnings: Vec<String>,
    /// The imported objects
    pub objects: Vec<SemanticObject>,
}

impl ImportResult {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            items_imported: 0,
            objects_created: 0,
            warnings: Vec::new(),
            objects: Vec::new(),
        }
    }
}

/// Auto-detect the source type and return appropriate importer
pub fn detect_source(path: &Path) -> Option<Box<dyn Importer>> {
    // Check in order of specificity

    // ChatGPT export file
    if path.is_file() && path.extension().map_or(false, |e| e == "json") {
        if let Ok(content) = std::fs::read_to_string(path) {
            if content.contains("\"mapping\"") && content.contains("\"message\"") {
                return Some(Box::new(chatgpt::ChatGptImporter::new()));
            }
        }
    }

    // Cursor workspace
    if path.is_dir() {
        let cursor_dir = path.join(".cursor");
        if cursor_dir.exists() {
            return Some(Box::new(cursor::CursorImporter::new()));
        }
    }

    // Obsidian vault
    if path.is_dir() {
        let obsidian_dir = path.join(".obsidian");
        if obsidian_dir.exists() {
            return Some(Box::new(obsidian::ObsidianImporter::new()));
        }
    }

    None
}
