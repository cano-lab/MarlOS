//! Document handling for MarlOS
//!
//! Manages markdown documents with:
//! - File I/O
//! - Change tracking
//! - Metadata extraction

use std::path::{Path, PathBuf};
use std::fs;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DocumentError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Document not found: {0}")]
    NotFound(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

pub type Result<T> = std::result::Result<T, DocumentError>;

/// A document with content and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Unique identifier
    pub id: String,
    /// File path (if saved)
    pub path: Option<PathBuf>,
    /// Document content (markdown)
    pub content: String,
    /// Document title (extracted from content or filename)
    pub title: String,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Last modification time
    pub modified_at: DateTime<Utc>,
    /// Word count
    pub word_count: usize,
    /// Whether document has unsaved changes
    pub is_dirty: bool,
}

impl Document {
    /// Create a new empty document
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            path: None,
            content: String::new(),
            title: "Untitled".to_string(),
            created_at: now,
            modified_at: now,
            word_count: 0,
            is_dirty: false,
        }
    }

    /// Open a document from a file
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(DocumentError::NotFound(path.display().to_string()));
        }

        let content = fs::read_to_string(path)?;
        let title = Self::extract_title(&content)
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string()
            });

        let metadata = fs::metadata(path)?;
        let modified_at = metadata
            .modified()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());

        let word_count = Self::count_words(&content);

        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            path: Some(path.to_path_buf()),
            content,
            title,
            created_at: modified_at, // Use file time as approximation
            modified_at,
            word_count,
            is_dirty: false,
        })
    }

    /// Save the document to its path
    pub fn save(&mut self) -> Result<()> {
        let path = self.path.as_ref()
            .ok_or_else(|| DocumentError::InvalidPath("No path set".to_string()))?;

        fs::write(path, &self.content)?;
        self.modified_at = Utc::now();
        self.is_dirty = false;

        log::info!("Saved document: {:?}", path);
        Ok(())
    }

    /// Save the document to a new path
    pub fn save_as<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();
        fs::write(path, &self.content)?;

        self.path = Some(path.to_path_buf());
        self.modified_at = Utc::now();
        self.is_dirty = false;

        // Update title from new filename
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if self.title == "Untitled" {
                self.title = stem.to_string();
            }
        }

        log::info!("Saved document as: {:?}", path);
        Ok(())
    }

    /// Update document content
    pub fn set_content(&mut self, content: String) {
        self.content = content;
        self.word_count = Self::count_words(&self.content);
        self.title = Self::extract_title(&self.content)
            .unwrap_or_else(|| self.title.clone());
        self.is_dirty = true;
    }

    /// Extract title from markdown content (first H1)
    fn extract_title(content: &str) -> Option<String> {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("# ") {
                return Some(trimmed[2..].trim().to_string());
            }
        }
        None
    }

    /// Count words in content
    fn count_words(content: &str) -> usize {
        content.split_whitespace().count()
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_document() {
        let doc = Document::new();
        assert_eq!(doc.title, "Untitled");
        assert!(!doc.is_dirty);
    }

    #[test]
    fn test_extract_title() {
        let content = "# My Title\n\nSome content here.";
        let title = Document::extract_title(content);
        assert_eq!(title, Some("My Title".to_string()));
    }

    #[test]
    fn test_word_count() {
        let content = "Hello world this is a test";
        assert_eq!(Document::count_words(content), 6);
    }
}
