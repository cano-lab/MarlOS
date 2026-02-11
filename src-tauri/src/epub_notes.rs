//! EPUB Notes - Highlight and annotation storage for e-books
//!
//! Provides persistent storage for text highlights and notes in EPUB files.
//! Stores data in ~/.local/share/marlos/epub_notes.json

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use uuid::Uuid;

/// A single highlight with optional note
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpubHighlight {
    pub id: String,
    pub book_path: String,           // EPUB file path (book identifier)
    pub chapter_index: usize,
    pub selected_text: String,       // The highlighted text
    pub start_offset: usize,         // Character offset in chapter text
    pub end_offset: usize,
    pub color: String,               // Highlight color (yellow, green, blue, pink)
    pub note: String,                // User's note (can be empty for pure highlights)
    pub created_at: i64,             // Unix timestamp ms
    pub updated_at: i64,
}

impl EpubHighlight {
    /// Create a new highlight
    pub fn new(
        book_path: String,
        chapter_index: usize,
        selected_text: String,
        start_offset: usize,
        end_offset: usize,
        color: String,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: Uuid::new_v4().to_string(),
            book_path,
            chapter_index,
            selected_text,
            start_offset,
            end_offset,
            color,
            note: String::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// Store format for EPUB notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpubNotesStore {
    pub version: u32,
    pub highlights: Vec<EpubHighlight>,
}

impl Default for EpubNotesStore {
    fn default() -> Self {
        Self {
            version: 1,
            highlights: Vec::new(),
        }
    }
}

/// EPUB Notes Manager - handles persistence and queries
pub struct EpubNotesManager {
    data: RwLock<EpubNotesStore>,
    storage_path: PathBuf,
}

impl EpubNotesManager {
    /// Create a new notes manager, loading from disk if available
    pub fn new() -> Self {
        let storage_path = Self::get_storage_path();
        let data = Self::load_from_disk(&storage_path).unwrap_or_default();

        Self {
            data: RwLock::new(data),
            storage_path,
        }
    }

    /// Get the storage path for epub_notes.json
    fn get_storage_path() -> PathBuf {
        let data_dir = if cfg!(target_os = "windows") {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("marlos")
        } else {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from(".local/share"))
                .join("marlos")
        };

        // Ensure directory exists
        let _ = fs::create_dir_all(&data_dir);

        data_dir.join("epub_notes.json")
    }

    /// Load notes data from disk
    fn load_from_disk(path: &PathBuf) -> Option<EpubNotesStore> {
        let content = fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save notes data to disk
    fn save_to_disk(&self) -> Result<(), String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        let content = serde_json::to_string_pretty(&*data)
            .map_err(|e| e.to_string())?;
        fs::write(&self.storage_path, content)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Save a new highlight (or update existing)
    pub fn save(&self, mut highlight: EpubHighlight) -> Result<EpubHighlight, String> {
        {
            let mut data = self.data.write().map_err(|e| e.to_string())?;

            // Check if this is an update (existing ID)
            if let Some(existing) = data.highlights.iter_mut().find(|h| h.id == highlight.id) {
                highlight.updated_at = chrono::Utc::now().timestamp_millis();
                highlight.created_at = existing.created_at; // Preserve original creation time
                *existing = highlight.clone();
            } else {
                // New highlight - generate ID if empty
                if highlight.id.is_empty() {
                    highlight.id = Uuid::new_v4().to_string();
                }
                let now = chrono::Utc::now().timestamp_millis();
                highlight.created_at = now;
                highlight.updated_at = now;
                data.highlights.push(highlight.clone());
            }
        }

        self.save_to_disk()?;
        log::info!("Saved highlight: {}", highlight.id);
        Ok(highlight)
    }

    /// Load a highlight by ID
    pub fn load(&self, id: &str) -> Result<Option<EpubHighlight>, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.highlights.iter().find(|h| h.id == id).cloned())
    }

    /// Delete a highlight by ID
    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let deleted = {
            let mut data = self.data.write().map_err(|e| e.to_string())?;
            let initial_len = data.highlights.len();
            data.highlights.retain(|h| h.id != id);
            data.highlights.len() < initial_len
        };

        if deleted {
            self.save_to_disk()?;
            log::info!("Deleted highlight: {}", id);
        }
        Ok(deleted)
    }

    /// Get all highlights for a specific book
    pub fn get_for_book(&self, book_path: &str) -> Result<Vec<EpubHighlight>, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.highlights.iter()
            .filter(|h| h.book_path == book_path)
            .cloned()
            .collect())
    }

    /// Get highlights for a specific book and chapter
    pub fn get_for_chapter(&self, book_path: &str, chapter_index: usize) -> Result<Vec<EpubHighlight>, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.highlights.iter()
            .filter(|h| h.book_path == book_path && h.chapter_index == chapter_index)
            .cloned()
            .collect())
    }

    /// List all highlights
    pub fn list_all(&self) -> Result<Vec<EpubHighlight>, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.highlights.clone())
    }

    /// Update just the note for a highlight
    pub fn update_note(&self, id: &str, note: String) -> Result<Option<EpubHighlight>, String> {
        let result = {
            let mut data = self.data.write().map_err(|e| e.to_string())?;
            if let Some(highlight) = data.highlights.iter_mut().find(|h| h.id == id) {
                highlight.note = note;
                highlight.updated_at = chrono::Utc::now().timestamp_millis();
                Some(highlight.clone())
            } else {
                None
            }
        };

        if result.is_some() {
            self.save_to_disk()?;
            log::info!("Updated note for highlight: {}", id);
        }
        Ok(result)
    }

    /// Update the color of a highlight
    pub fn update_color(&self, id: &str, color: String) -> Result<Option<EpubHighlight>, String> {
        let result = {
            let mut data = self.data.write().map_err(|e| e.to_string())?;
            if let Some(highlight) = data.highlights.iter_mut().find(|h| h.id == id) {
                highlight.color = color;
                highlight.updated_at = chrono::Utc::now().timestamp_millis();
                Some(highlight.clone())
            } else {
                None
            }
        };

        if result.is_some() {
            self.save_to_disk()?;
            log::info!("Updated color for highlight: {}", id);
        }
        Ok(result)
    }
}

impl Default for EpubNotesManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_creation() {
        let highlight = EpubHighlight::new(
            "/path/to/book.epub".to_string(),
            0,
            "Some highlighted text".to_string(),
            100,
            121,
            "yellow".to_string(),
        );

        assert!(!highlight.id.is_empty());
        assert_eq!(highlight.book_path, "/path/to/book.epub");
        assert_eq!(highlight.chapter_index, 0);
        assert_eq!(highlight.color, "yellow");
        assert!(highlight.note.is_empty());
    }

    #[test]
    fn test_store_default() {
        let store = EpubNotesStore::default();
        assert_eq!(store.version, 1);
        assert!(store.highlights.is_empty());
    }
}
