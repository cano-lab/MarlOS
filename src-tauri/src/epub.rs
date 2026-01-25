//! EPUB handling for reading e-books
//!
//! Provides EPUB parsing, navigation, and content extraction.

use std::path::Path;
use std::sync::Mutex;
use epub::doc::EpubDoc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

#[derive(Error, Debug)]
pub enum EpubError {
    #[error("EPUB error: {0}")]
    Epub(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("EPUB not loaded")]
    NotLoaded,
    #[error("Chapter not found: {0}")]
    ChapterNotFound(usize),
    #[error("Lock error")]
    Lock,
}

impl From<epub::doc::DocError> for EpubError {
    fn from(e: epub::doc::DocError) -> Self {
        EpubError::Epub(format!("{:?}", e))
    }
}

pub type Result<T> = std::result::Result<T, EpubError>;

/// Table of contents entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocEntry {
    pub label: String,
    pub content_path: String,
    pub play_order: usize,
    pub children: Vec<TocEntry>,
}

/// EPUB metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpubInfo {
    pub path: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub chapter_count: usize,
    pub toc: Vec<TocEntry>,
}

/// Chapter content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterContent {
    pub index: usize,
    pub title: Option<String>,
    /// HTML content of the chapter
    pub html: String,
    /// Plain text version (for search/display)
    pub text: String,
}

/// Stored EPUB state
struct LoadedEpub {
    path: String,
    info: EpubInfo,
    /// Raw EPUB bytes for reloading
    bytes: Vec<u8>,
}

/// EPUB Manager state
pub struct EpubState {
    current_epub: Option<LoadedEpub>,
}

impl EpubState {
    pub fn new() -> Self {
        Self {
            current_epub: None,
        }
    }
}

/// Thread-safe EPUB Manager
pub struct EpubManager {
    state: Mutex<EpubState>,
}

unsafe impl Send for EpubManager {}
unsafe impl Sync for EpubManager {}

impl EpubManager {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(EpubState::new()),
        }
    }

    /// Open an EPUB file
    pub fn open(&self, path: &str) -> Result<EpubInfo> {
        let path_obj = Path::new(path);
        let bytes = std::fs::read(path_obj)?;

        // Open EPUB from bytes
        let doc = EpubDoc::from_reader(std::io::Cursor::new(&bytes))
            .map_err(|e| EpubError::Epub(format!("{:?}", e)))?;

        // Extract metadata
        let title = doc.mdata("title").map(|m| m.value.clone());
        let author = doc.mdata("creator").map(|m| m.value.clone());
        let language = doc.mdata("language").map(|m| m.value.clone());
        let description = doc.mdata("description").map(|m| m.value.clone());
        let publisher = doc.mdata("publisher").map(|m| m.value.clone());

        // Get chapter count from spine
        let chapter_count = doc.get_num_chapters();

        // Build table of contents
        let toc = self.build_toc(&doc);

        let info = EpubInfo {
            path: path.to_string(),
            title,
            author,
            language,
            description,
            publisher,
            chapter_count,
            toc,
        };

        // Store for later use
        let mut state = self.state.lock().map_err(|_| EpubError::Lock)?;
        state.current_epub = Some(LoadedEpub {
            path: path.to_string(),
            info: info.clone(),
            bytes,
        });

        log::info!("Opened EPUB: {} ({} chapters)", path, chapter_count);

        Ok(info)
    }

    /// Build table of contents from EPUB
    fn build_toc(&self, doc: &EpubDoc<std::io::Cursor<&Vec<u8>>>) -> Vec<TocEntry> {
        let mut entries = Vec::new();

        // Get TOC from navpoints
        for (i, nav) in doc.toc.iter().enumerate() {
            entries.push(TocEntry {
                label: nav.label.clone(),
                content_path: nav.content.to_string_lossy().to_string(),
                play_order: nav.play_order.unwrap_or(i),
                children: Vec::new(), // TODO: Handle nested TOC
            });
        }

        entries
    }

    /// Get current EPUB info
    pub fn get_info(&self) -> Result<EpubInfo> {
        let state = self.state.lock().map_err(|_| EpubError::Lock)?;
        state.current_epub.as_ref()
            .map(|e| e.info.clone())
            .ok_or(EpubError::NotLoaded)
    }

    /// Get chapter content by index
    pub fn get_chapter(&self, index: usize) -> Result<ChapterContent> {
        let state = self.state.lock().map_err(|_| EpubError::Lock)?;
        let loaded = state.current_epub.as_ref().ok_or(EpubError::NotLoaded)?;

        // Reload EPUB to get chapter
        let mut doc = EpubDoc::from_reader(std::io::Cursor::new(&loaded.bytes))
            .map_err(|e| EpubError::Epub(format!("{:?}", e)))?;

        if index >= doc.get_num_chapters() {
            return Err(EpubError::ChapterNotFound(index));
        }

        // Navigate to the chapter
        doc.set_current_chapter(index);

        // Get chapter content
        let html = doc.get_current_str()
            .map(|(content, _mime)| content)
            .unwrap_or_default();

        // Convert HTML to plain text for search/display
        let text = html2text::from_read(html.as_bytes(), 80);

        // Try to get chapter title from TOC
        let title = loaded.info.toc.get(index).map(|t| t.label.clone());

        Ok(ChapterContent {
            index,
            title,
            html,
            text,
        })
    }

    /// Get chapter by content path (from TOC)
    pub fn get_chapter_by_path(&self, content_path: &str) -> Result<ChapterContent> {
        let state = self.state.lock().map_err(|_| EpubError::Lock)?;
        let loaded = state.current_epub.as_ref().ok_or(EpubError::NotLoaded)?;

        // Find index in TOC
        let index = loaded.info.toc.iter()
            .position(|t| t.content_path == content_path)
            .unwrap_or(0);

        // Use get_chapter with the found index
        drop(state); // Release lock before calling get_chapter
        self.get_chapter(index)
    }

    /// Search for text in the EPUB
    pub fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchResult>> {
        let state = self.state.lock().map_err(|_| EpubError::Lock)?;
        let loaded = state.current_epub.as_ref().ok_or(EpubError::NotLoaded)?;

        let mut results = Vec::new();
        let query_lower = query.to_lowercase();

        // Reload EPUB
        let mut doc = EpubDoc::from_reader(std::io::Cursor::new(&loaded.bytes))
            .map_err(|e| EpubError::Epub(format!("{:?}", e)))?;

        for i in 0..doc.get_num_chapters() {
            doc.set_current_chapter(i);

            if let Some((content, _mime)) = doc.get_current_str() {
                let text = html2text::from_read(content.as_bytes(), 80);
                let text_lower = text.to_lowercase();

                // Find all occurrences
                let mut start = 0;
                while let Some(pos) = text_lower[start..].find(&query_lower) {
                    let actual_pos = start + pos;

                    // Extract context (50 chars before and after)
                    let context_start = actual_pos.saturating_sub(50);
                    let context_end = (actual_pos + query.len() + 50).min(text.len());
                    let context = text[context_start..context_end].to_string();

                    results.push(SearchResult {
                        chapter_index: i,
                        chapter_title: loaded.info.toc.get(i).map(|t| t.label.clone()),
                        position: actual_pos,
                        context,
                    });

                    if results.len() >= max_results {
                        return Ok(results);
                    }

                    start = actual_pos + query.len();
                }
            }
        }

        Ok(results)
    }

    /// Get cover image as base64
    pub fn get_cover(&self) -> Result<Option<String>> {
        let state = self.state.lock().map_err(|_| EpubError::Lock)?;
        let loaded = state.current_epub.as_ref().ok_or(EpubError::NotLoaded)?;

        let mut doc = EpubDoc::from_reader(std::io::Cursor::new(&loaded.bytes))
            .map_err(|e| EpubError::Epub(format!("{:?}", e)))?;

        if let Some((cover_bytes, _mime_type)) = doc.get_cover() {
            let base64_cover = BASE64.encode(&cover_bytes);
            Ok(Some(base64_cover))
        } else {
            Ok(None)
        }
    }
}

impl Default for EpubManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chapter_index: usize,
    pub chapter_title: Option<String>,
    pub position: usize,
    pub context: String,
}
