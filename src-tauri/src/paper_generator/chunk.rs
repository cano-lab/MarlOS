//! Semantic Chunking
//!
//! Splits documents into meaningful semantic chunks for processing.
//! Targets ~500-1000 words per chunk with context overlap.

use serde::{Deserialize, Serialize};
use regex::Regex;

// ============================================================================
// Semantic Chunk
// ============================================================================

/// A semantically meaningful segment of a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticChunk {
    /// Unique chunk ID
    pub id: String,
    /// Source ID this chunk came from
    pub source_id: String,
    /// Chunk content
    pub content: String,
    /// Chunk type/category
    pub chunk_type: ChunkType,
    /// Start position in original document (char offset)
    pub start_offset: usize,
    /// End position in original document (char offset)
    pub end_offset: usize,
    /// Word count
    pub word_count: usize,
    /// Section or heading this chunk belongs to (if detected)
    pub section_heading: Option<String>,
    /// Page number (if available)
    pub page_number: Option<usize>,
    /// Overlap with previous chunk (for context continuity)
    pub overlap_prev: Option<String>,
    /// Overlap with next chunk (for context continuity)
    pub overlap_next: Option<String>,
}

impl SemanticChunk {
    pub fn new(source_id: &str, content: &str, start_offset: usize, end_offset: usize) -> Self {
        Self {
            id: format!("chunk_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string()),
            source_id: source_id.to_string(),
            content: content.to_string(),
            chunk_type: ChunkType::Paragraph,
            start_offset,
            end_offset,
            word_count: content.split_whitespace().count(),
            section_heading: None,
            page_number: None,
            overlap_prev: None,
            overlap_next: None,
        }
    }

    pub fn with_type(mut self, chunk_type: ChunkType) -> Self {
        self.chunk_type = chunk_type;
        self
    }

    pub fn with_heading(mut self, heading: &str) -> Self {
        self.section_heading = Some(heading.to_string());
        self
    }

    pub fn with_page(mut self, page: usize) -> Self {
        self.page_number = Some(page);
        self
    }
}

/// Type of chunk
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkType {
    /// Regular paragraph
    Paragraph,
    /// Section with heading
    Section,
    /// List items
    List,
    /// Table content
    Table,
    /// Code block
    Code,
    /// Quote or excerpt
    Quote,
    /// Abstract/summary
    Abstract,
    /// References/bibliography
    References,
    /// Figure/image caption
    Caption,
}

// ============================================================================
// Chunker Configuration
// ============================================================================

/// Configuration for the semantic chunker
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Target word count per chunk
    pub target_words: usize,
    /// Minimum word count (don't split below this)
    pub min_words: usize,
    /// Maximum word count (force split above this)
    pub max_words: usize,
    /// Number of overlap words for context
    pub overlap_words: usize,
    /// Whether to preserve paragraph boundaries
    pub preserve_paragraphs: bool,
    /// Whether to detect section headings
    pub detect_headings: bool,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            target_words: 750,
            min_words: 300,
            max_words: 1200,
            overlap_words: 50,
            preserve_paragraphs: true,
            detect_headings: true,
        }
    }
}

// ============================================================================
// Semantic Chunker
// ============================================================================

/// Splits documents into semantic chunks
pub struct SemanticChunker {
    config: ChunkerConfig,
}

impl SemanticChunker {
    pub fn new() -> Self {
        Self {
            config: ChunkerConfig::default(),
        }
    }

    pub fn with_config(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Chunk a document into semantic segments
    pub fn chunk(&self, source_id: &str, content: &str) -> Vec<SemanticChunk> {
        let mut chunks = Vec::new();

        // First, split into sections if headings are detected
        let sections = if self.config.detect_headings {
            self.split_into_sections(content)
        } else {
            vec![(None, content.to_string(), 0)]
        };

        for (heading, section_content, section_start) in sections {
            // Split section into paragraph-based chunks
            let section_chunks = self.chunk_section(
                source_id,
                &section_content,
                section_start,
                heading.as_deref(),
            );
            chunks.extend(section_chunks);
        }

        // Add overlap context between chunks
        self.add_overlap_context(&mut chunks);

        chunks
    }

    /// Split content into sections based on headings
    fn split_into_sections(&self, content: &str) -> Vec<(Option<String>, String, usize)> {
        // Match markdown headings (## Heading) or common text headings (all caps lines, etc.)
        let heading_re = Regex::new(r"(?m)^(#{1,6}\s+.+|[A-Z][A-Z\s]{3,}[A-Z])\s*$").unwrap();

        let mut sections = Vec::new();
        let mut last_end = 0;
        let mut last_heading: Option<String> = None;

        for cap in heading_re.find_iter(content) {
            // Save content before this heading
            if cap.start() > last_end {
                let section_content = content[last_end..cap.start()].trim().to_string();
                if !section_content.is_empty() {
                    sections.push((last_heading.clone(), section_content, last_end));
                }
            }

            // Extract heading text (remove # markers)
            let heading_text = cap.as_str().trim_start_matches('#').trim().to_string();
            last_heading = Some(heading_text);
            last_end = cap.end();
        }

        // Don't forget the last section
        if last_end < content.len() {
            let section_content = content[last_end..].trim().to_string();
            if !section_content.is_empty() {
                sections.push((last_heading, section_content, last_end));
            }
        }

        // If no sections found, return whole content
        if sections.is_empty() {
            sections.push((None, content.to_string(), 0));
        }

        sections
    }

    /// Chunk a single section
    fn chunk_section(
        &self,
        source_id: &str,
        content: &str,
        base_offset: usize,
        heading: Option<&str>,
    ) -> Vec<SemanticChunk> {
        let mut chunks = Vec::new();

        // Split into paragraphs
        let paragraphs: Vec<&str> = content.split("\n\n").collect();

        let mut current_content = String::new();
        let mut current_start = 0;
        let mut current_words = 0;

        for para in paragraphs {
            let para = para.trim();
            if para.is_empty() {
                continue;
            }

            let para_words = para.split_whitespace().count();

            // Check if adding this paragraph would exceed max
            if current_words + para_words > self.config.max_words && current_words >= self.config.min_words {
                // Create chunk from accumulated content
                if !current_content.is_empty() {
                    let chunk = SemanticChunk::new(
                        source_id,
                        current_content.trim(),
                        base_offset + current_start,
                        base_offset + current_start + current_content.len(),
                    );
                    let chunk = if let Some(h) = heading {
                        chunk.with_heading(h)
                    } else {
                        chunk
                    };
                    chunks.push(chunk);
                }

                // Start new chunk
                current_content = para.to_string();
                current_start = content.find(para).unwrap_or(0);
                current_words = para_words;
            } else {
                // Add to current chunk
                if !current_content.is_empty() {
                    current_content.push_str("\n\n");
                } else {
                    current_start = content.find(para).unwrap_or(0);
                }
                current_content.push_str(para);
                current_words += para_words;
            }

            // Check if we've reached target size
            if current_words >= self.config.target_words {
                let chunk = SemanticChunk::new(
                    source_id,
                    current_content.trim(),
                    base_offset + current_start,
                    base_offset + current_start + current_content.len(),
                );
                let chunk = if let Some(h) = heading {
                    chunk.with_heading(h)
                } else {
                    chunk
                };
                chunks.push(chunk);

                current_content = String::new();
                current_words = 0;
            }
        }

        // Don't forget remaining content
        if !current_content.is_empty() && current_words >= self.config.min_words / 2 {
            let chunk = SemanticChunk::new(
                source_id,
                current_content.trim(),
                base_offset + current_start,
                base_offset + current_start + current_content.len(),
            );
            let chunk = if let Some(h) = heading {
                chunk.with_heading(h)
            } else {
                chunk
            };
            chunks.push(chunk);
        } else if !current_content.is_empty() && !chunks.is_empty() {
            // Merge with previous chunk if too small
            if let Some(last) = chunks.last_mut() {
                last.content.push_str("\n\n");
                last.content.push_str(current_content.trim());
                last.word_count = last.content.split_whitespace().count();
                last.end_offset = base_offset + current_start + current_content.len();
            }
        } else if !current_content.is_empty() {
            // First chunk, keep it even if small
            let chunk = SemanticChunk::new(
                source_id,
                current_content.trim(),
                base_offset + current_start,
                base_offset + current_start + current_content.len(),
            );
            let chunk = if let Some(h) = heading {
                chunk.with_heading(h)
            } else {
                chunk
            };
            chunks.push(chunk);
        }

        chunks
    }

    /// Add overlap context between adjacent chunks
    fn add_overlap_context(&self, chunks: &mut [SemanticChunk]) {
        if chunks.len() < 2 {
            return;
        }

        for i in 0..chunks.len() {
            // Get overlap from previous chunk
            if i > 0 {
                let prev_content = &chunks[i - 1].content;
                let words: Vec<&str> = prev_content.split_whitespace().collect();
                if words.len() > self.config.overlap_words {
                    let overlap: String = words[words.len() - self.config.overlap_words..]
                        .join(" ");
                    chunks[i].overlap_prev = Some(overlap);
                }
            }

            // Get overlap for next chunk
            if i < chunks.len() - 1 {
                let curr_content = &chunks[i].content;
                let words: Vec<&str> = curr_content.split_whitespace().collect();
                if words.len() > self.config.overlap_words {
                    let overlap: String = words[..self.config.overlap_words.min(words.len())]
                        .join(" ");
                    chunks[i].overlap_next = Some(overlap);
                }
            }
        }
    }

    /// Detect the type of a chunk based on content
    pub fn detect_chunk_type(&self, content: &str) -> ChunkType {
        let trimmed = content.trim();

        // Check for code blocks
        if trimmed.starts_with("```") || trimmed.starts_with("    ") {
            return ChunkType::Code;
        }

        // Check for lists
        let list_pattern = Regex::new(r"^[\s]*[-*•]\s").unwrap();
        let numbered_list = Regex::new(r"^[\s]*\d+[.)]\s").unwrap();
        if list_pattern.is_match(trimmed) || numbered_list.is_match(trimmed) {
            return ChunkType::List;
        }

        // Check for tables
        if trimmed.contains("|") && trimmed.lines().count() > 1 {
            let lines: Vec<&str> = trimmed.lines().collect();
            if lines.iter().filter(|l| l.contains("|")).count() > 2 {
                return ChunkType::Table;
            }
        }

        // Check for quotes
        if trimmed.starts_with(">") || trimmed.starts_with("\"") {
            return ChunkType::Quote;
        }

        // Check for references section
        let ref_patterns = ["References", "Bibliography", "Works Cited", "Sources"];
        for pattern in ref_patterns {
            if trimmed.to_lowercase().starts_with(&pattern.to_lowercase()) {
                return ChunkType::References;
            }
        }

        // Check for abstract
        if trimmed.to_lowercase().starts_with("abstract") {
            return ChunkType::Abstract;
        }

        ChunkType::Paragraph
    }
}

impl Default for SemanticChunker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Chunking Progress
// ============================================================================

/// Progress information for chunking operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingProgress {
    /// Total sources to process
    pub total_sources: usize,
    /// Sources processed so far
    pub processed_sources: usize,
    /// Total chunks created
    pub total_chunks: usize,
    /// Total words processed
    pub total_words: usize,
    /// Current source being processed
    pub current_source: Option<String>,
    /// Whether chunking is complete
    pub complete: bool,
}

impl ChunkingProgress {
    pub fn new(total_sources: usize) -> Self {
        Self {
            total_sources,
            processed_sources: 0,
            total_chunks: 0,
            total_words: 0,
            current_source: None,
            complete: false,
        }
    }

    pub fn progress_percent(&self) -> f32 {
        if self.total_sources == 0 {
            return 100.0;
        }
        (self.processed_sources as f32 / self.total_sources as f32) * 100.0
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_chunking() {
        let chunker = SemanticChunker::with_config(ChunkerConfig {
            target_words: 50,
            min_words: 20,
            max_words: 100,
            overlap_words: 5,
            preserve_paragraphs: true,
            detect_headings: true,
        });

        let content = "This is the first paragraph with some content to test the chunking functionality.

This is the second paragraph that should be grouped with the first if it doesn't exceed limits.

## New Section

This is a new section with its own content that should be in a separate chunk.";

        let chunks = chunker.chunk("src_123", content);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_heading_detection() {
        let chunker = SemanticChunker::new();
        let content = "## Introduction

Some intro content here.

## Methods

Method description here.";

        let chunks = chunker.chunk("src_123", content);

        // Should have chunks with headings
        assert!(chunks.iter().any(|c| c.section_heading.is_some()));
    }

    #[test]
    fn test_chunk_type_detection() {
        let chunker = SemanticChunker::new();

        assert_eq!(chunker.detect_chunk_type("- Item 1\n- Item 2"), ChunkType::List);
        assert_eq!(chunker.detect_chunk_type("```rust\ncode\n```"), ChunkType::Code);
        assert_eq!(chunker.detect_chunk_type("> This is a quote"), ChunkType::Quote);
        assert_eq!(chunker.detect_chunk_type("Regular paragraph text"), ChunkType::Paragraph);
    }

    #[test]
    fn test_overlap_context() {
        let chunker = SemanticChunker::with_config(ChunkerConfig {
            target_words: 20,
            min_words: 10,
            max_words: 30,
            overlap_words: 3,
            preserve_paragraphs: true,
            detect_headings: false,
        });

        let content = "First paragraph with enough words to form a chunk on its own.

Second paragraph with different content that forms another chunk.";

        let chunks = chunker.chunk("src_123", content);

        if chunks.len() > 1 {
            // Second chunk should have overlap from first
            assert!(chunks[1].overlap_prev.is_some());
        }
    }
}
