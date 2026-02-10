//! Text Chunking for Semantic Search
//!
//! Splits large documents into semantic chunks for better embedding quality.
//! Each chunk is small enough to embed well (~2000 chars) while maintaining
//! semantic coherence (splitting at natural boundaries like paragraphs, functions).

use regex::Regex;

/// Find the nearest valid UTF-8 character boundary at or before the given byte position
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    // Walk backwards to find a char boundary
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Configuration for chunking
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Target chunk size in characters
    pub target_size: usize,
    /// Minimum chunk size (don't create tiny chunks)
    pub min_size: usize,
    /// Maximum chunk size (hard limit)
    pub max_size: usize,
    /// Overlap between chunks for context continuity
    pub overlap: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            target_size: 2000,
            min_size: 200,
            max_size: 4000,
            overlap: 100,
        }
    }
}

/// A chunk of text with position info
#[derive(Debug, Clone)]
pub struct TextChunk {
    /// The chunk text
    pub text: String,
    /// Start position in original text (byte offset)
    pub start: usize,
    /// End position in original text (byte offset)
    pub end: usize,
    /// Chunk index (0-based)
    pub index: usize,
    /// Optional context hint (e.g., function name, section header)
    pub context: Option<String>,
}

/// Chunk text based on file type
pub fn chunk_text(text: &str, file_ext: Option<&str>, config: &ChunkConfig) -> Vec<TextChunk> {
    // Skip small files that don't need chunking
    if text.len() <= config.max_size {
        return vec![TextChunk {
            text: text.to_string(),
            start: 0,
            end: text.len(),
            index: 0,
            context: None,
        }];
    }

    match file_ext {
        Some("rs") => chunk_rust(text, config),
        Some("py") => chunk_python(text, config),
        Some("js" | "ts" | "jsx" | "tsx") => chunk_javascript(text, config),
        Some("go") => chunk_go(text, config),
        Some("md") => chunk_markdown(text, config),
        _ => chunk_generic(text, config),
    }
}

/// Chunk Rust code by functions/impl blocks
fn chunk_rust(text: &str, config: &ChunkConfig) -> Vec<TextChunk> {
    // Pattern for function/impl/struct/enum definitions
    let patterns = [
        r"(?m)^(pub\s+)?(async\s+)?fn\s+\w+",           // functions
        r"(?m)^(pub\s+)?impl\s+",                        // impl blocks
        r"(?m)^(pub\s+)?struct\s+\w+",                   // structs
        r"(?m)^(pub\s+)?enum\s+\w+",                     // enums
        r"(?m)^(pub\s+)?mod\s+\w+",                      // modules
        r"(?m)^(pub\s+)?trait\s+\w+",                    // traits
    ];

    chunk_by_patterns(text, &patterns, config)
}

/// Chunk Python code by functions/classes
fn chunk_python(text: &str, config: &ChunkConfig) -> Vec<TextChunk> {
    let patterns = [
        r"(?m)^(async\s+)?def\s+\w+",      // functions
        r"(?m)^class\s+\w+",               // classes
        r"(?m)^@\w+",                       // decorators (start of function/class)
    ];

    chunk_by_patterns(text, &patterns, config)
}

/// Chunk JavaScript/TypeScript
fn chunk_javascript(text: &str, config: &ChunkConfig) -> Vec<TextChunk> {
    let patterns = [
        r"(?m)^(export\s+)?(async\s+)?function\s+\w+",  // named functions
        r"(?m)^(export\s+)?class\s+\w+",                // classes
        r"(?m)^(export\s+)?(const|let|var)\s+\w+\s*=\s*(async\s+)?\(",  // arrow functions
        r"(?m)^(export\s+)?interface\s+\w+",            // interfaces
        r"(?m)^(export\s+)?type\s+\w+",                 // type aliases
    ];

    chunk_by_patterns(text, &patterns, config)
}

/// Chunk Go code
fn chunk_go(text: &str, config: &ChunkConfig) -> Vec<TextChunk> {
    let patterns = [
        r"(?m)^func\s+(\(\w+\s+\*?\w+\)\s+)?\w+",  // functions and methods
        r"(?m)^type\s+\w+\s+struct",               // structs
        r"(?m)^type\s+\w+\s+interface",            // interfaces
    ];

    chunk_by_patterns(text, &patterns, config)
}

/// Chunk Markdown by headers
fn chunk_markdown(text: &str, config: &ChunkConfig) -> Vec<TextChunk> {
    let patterns = [
        r"(?m)^#{1,6}\s+.+$",   // headers
        r"(?m)^---+$",          // horizontal rules
    ];

    chunk_by_patterns(text, &patterns, config)
}

/// Generic chunking by paragraphs
fn chunk_generic(text: &str, config: &ChunkConfig) -> Vec<TextChunk> {
    chunk_by_separator(text, "\n\n", config)
}

/// Chunk text by regex patterns (split at pattern matches)
fn chunk_by_patterns(text: &str, patterns: &[&str], config: &ChunkConfig) -> Vec<TextChunk> {
    // Find all split points
    let mut split_points: Vec<usize> = vec![0];

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            for mat in re.find_iter(text) {
                split_points.push(mat.start());
            }
        }
    }

    split_points.sort();
    split_points.dedup();
    split_points.push(text.len());

    // Create chunks from split points, merging small ones
    let mut chunks = Vec::new();
    let mut current_start = 0;
    let mut current_end = 0;
    let mut chunk_index = 0;

    for i in 1..split_points.len() {
        let point = split_points[i];
        let segment_len = point - current_end;
        let current_len = point - current_start;

        // If adding this segment would exceed max, finalize current chunk
        if current_len > config.max_size && current_end > current_start {
            let safe_start = floor_char_boundary(text, current_start);
            let safe_end = floor_char_boundary(text, current_end);
            let chunk_text = &text[safe_start..safe_end];
            if chunk_text.trim().len() >= config.min_size {
                chunks.push(TextChunk {
                    text: chunk_text.to_string(),
                    start: safe_start,
                    end: safe_end,
                    index: chunk_index,
                    context: extract_context(chunk_text),
                });
                chunk_index += 1;
            }
            // Start new chunk with overlap (ensure char boundary)
            let overlap_pos = if current_end > config.overlap {
                current_end - config.overlap
            } else {
                current_end
            };
            current_start = floor_char_boundary(text, overlap_pos);
        }

        current_end = point;

        // If we've reached target size and this is a natural break, finalize
        if current_len >= config.target_size {
            let safe_start = floor_char_boundary(text, current_start);
            let safe_end = floor_char_boundary(text, current_end);
            let chunk_text = &text[safe_start..safe_end];
            if chunk_text.trim().len() >= config.min_size {
                chunks.push(TextChunk {
                    text: chunk_text.to_string(),
                    start: safe_start,
                    end: safe_end,
                    index: chunk_index,
                    context: extract_context(chunk_text),
                });
                chunk_index += 1;
            }
            let overlap_pos = if current_end > config.overlap {
                current_end - config.overlap
            } else {
                current_end
            };
            current_start = floor_char_boundary(text, overlap_pos);
        }
    }

    // Don't forget the last chunk
    if current_start < text.len() {
        let safe_start = floor_char_boundary(text, current_start);
        let chunk_text = &text[safe_start..];
        if chunk_text.trim().len() >= config.min_size {
            chunks.push(TextChunk {
                text: chunk_text.to_string(),
                start: safe_start,
                end: text.len(),
                index: chunk_index,
                context: extract_context(chunk_text),
            });
        }
    }

    // If no chunks were created, fall back to generic
    if chunks.is_empty() {
        return chunk_by_separator(text, "\n\n", config);
    }

    chunks
}

/// Chunk by a simple separator (paragraphs, etc.)
fn chunk_by_separator(text: &str, sep: &str, config: &ChunkConfig) -> Vec<TextChunk> {
    let segments: Vec<&str> = text.split(sep).collect();
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut current_start = 0;
    let mut chunk_index = 0;
    let mut pos = 0;

    for (i, segment) in segments.iter().enumerate() {
        let segment_len = segment.len() + if i < segments.len() - 1 { sep.len() } else { 0 };

        // Would this segment push us over max?
        if !current_chunk.is_empty() && current_chunk.len() + segment_len > config.max_size {
            // Finalize current chunk
            if current_chunk.trim().len() >= config.min_size {
                chunks.push(TextChunk {
                    text: current_chunk.clone(),
                    start: current_start,
                    end: pos,
                    index: chunk_index,
                    context: None,
                });
                chunk_index += 1;
            }
            current_chunk.clear();
            current_start = pos;
        }

        current_chunk.push_str(segment);
        if i < segments.len() - 1 {
            current_chunk.push_str(sep);
        }
        pos += segment_len;

        // If we've reached target, try to finalize
        if current_chunk.len() >= config.target_size {
            if current_chunk.trim().len() >= config.min_size {
                chunks.push(TextChunk {
                    text: current_chunk.clone(),
                    start: current_start,
                    end: pos,
                    index: chunk_index,
                    context: None,
                });
                chunk_index += 1;
            }
            current_chunk.clear();
            current_start = pos;
        }
    }

    // Last chunk
    if !current_chunk.is_empty() && current_chunk.trim().len() >= config.min_size {
        chunks.push(TextChunk {
            text: current_chunk,
            start: current_start,
            end: text.len(),
            index: chunk_index,
            context: None,
        });
    }

    // If still no chunks, just return the whole thing
    if chunks.is_empty() {
        return vec![TextChunk {
            text: text.to_string(),
            start: 0,
            end: text.len(),
            index: 0,
            context: None,
        }];
    }

    chunks
}

/// Extract context hint from chunk (function name, header, etc.)
fn extract_context(text: &str) -> Option<String> {
    let first_line = text.lines().next()?;

    // Rust function
    if let Ok(re) = Regex::new(r"(pub\s+)?(async\s+)?fn\s+(\w+)") {
        if let Some(cap) = re.captures(first_line) {
            return cap.get(3).map(|m| format!("fn {}", m.as_str()));
        }
    }

    // Python function/class
    if let Ok(re) = Regex::new(r"(async\s+)?def\s+(\w+)|class\s+(\w+)") {
        if let Some(cap) = re.captures(first_line) {
            if let Some(m) = cap.get(2) {
                return Some(format!("def {}", m.as_str()));
            }
            if let Some(m) = cap.get(3) {
                return Some(format!("class {}", m.as_str()));
            }
        }
    }

    // Markdown header
    if first_line.starts_with('#') {
        return Some(first_line.trim_start_matches('#').trim().to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_file_no_chunk() {
        let text = "fn main() { println!(\"hello\"); }";
        let chunks = chunk_text(text, Some("rs"), &ChunkConfig::default());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, text);
    }

    #[test]
    fn test_rust_chunking() {
        let text = r#"
fn foo() {
    // do foo
}

fn bar() {
    // do bar
}

fn baz() {
    // do baz
}
"#.repeat(100); // Make it large enough to need chunking

        let chunks = chunk_text(&text, Some("rs"), &ChunkConfig {
            target_size: 500,
            min_size: 50,
            max_size: 1000,
            overlap: 50,
        });

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.text.len() <= 1000);
        }
    }

    #[test]
    fn test_markdown_chunking() {
        let text = r#"
# Header 1

Some content here.

## Header 2

More content.

### Header 3

Even more content.
"#.repeat(50);

        let chunks = chunk_text(&text, Some("md"), &ChunkConfig {
            target_size: 300,
            min_size: 50,
            max_size: 600,
            overlap: 30,
        });

        assert!(chunks.len() > 1);
    }
}
