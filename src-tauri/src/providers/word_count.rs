//! Word Count Provider
//!
//! An ALWAYS_ON provider that tracks document statistics:
//! - Word count
//! - Character count
//! - Line count
//! - Paragraph count
//!
//! Updates automatically when document content changes.

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::{Provider, ProviderCategory, ProviderContext};

/// Document statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentStats {
    /// Number of words
    pub words: usize,
    /// Number of characters (including whitespace)
    pub chars: usize,
    /// Number of characters (excluding whitespace)
    pub chars_no_spaces: usize,
    /// Number of lines
    pub lines: usize,
    /// Number of paragraphs
    pub paragraphs: usize,
    /// Estimated reading time in minutes
    pub reading_time_minutes: f32,
}

/// Word count provider
pub struct WordCountProvider {
    stats: DocumentStats,
    word_regex: Regex,
}

impl WordCountProvider {
    pub fn new() -> Self {
        Self {
            stats: DocumentStats::default(),
            // Match words: sequences of alphanumeric characters, optionally with
            // apostrophes or hyphens in the middle (e.g., "don't", "well-known")
            word_regex: Regex::new(r"[A-Za-z0-9]+(?:['-][A-Za-z0-9]+)*").unwrap(),
        }
    }

    /// Get current statistics
    pub fn get_stats(&self) -> &DocumentStats {
        &self.stats
    }

    /// Update statistics from content
    fn update(&mut self, content: &str) {
        // Count words using regex
        let words: Vec<&str> = self.word_regex.find_iter(content).map(|m| m.as_str()).collect();
        let word_count = words.len();

        // Count characters
        let chars = content.len();
        let chars_no_spaces = content.chars().filter(|c| !c.is_whitespace()).count();

        // Count lines
        let lines = if content.is_empty() {
            0
        } else {
            content.lines().count()
        };

        // Count paragraphs (separated by blank lines)
        let paragraphs = content
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .count();

        // Estimate reading time (average 200 words per minute)
        let reading_time_minutes = word_count as f32 / 200.0;

        self.stats = DocumentStats {
            words: word_count,
            chars,
            chars_no_spaces,
            lines,
            paragraphs,
            reading_time_minutes,
        };

        log::trace!(
            "WordCount updated: {} words, {} chars, {} lines",
            word_count,
            chars,
            lines
        );
    }
}

impl Default for WordCountProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for WordCountProvider {
    fn name(&self) -> &str {
        "word_count"
    }

    fn category(&self) -> ProviderCategory {
        ProviderCategory::AlwaysOn
    }

    fn activate(&mut self, context: &ProviderContext) -> Result<(), String> {
        self.update(context.content);
        Ok(())
    }

    fn on_content_changed(&mut self, content: &str) {
        self.update(content);
    }

    fn get_state(&self) -> serde_json::Value {
        serde_json::to_value(&self.stats).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_count_basic() {
        let mut provider = WordCountProvider::new();
        let context = ProviderContext {
            content: "Hello world, this is a test.",
            file_path: None,
            commands: &super::super::CommandRegistry::new(),
        };

        provider.activate(&context).unwrap();
        let stats = provider.get_stats();

        assert_eq!(stats.words, 6);
        assert_eq!(stats.lines, 1);
    }

    #[test]
    fn test_word_count_multiline() {
        let mut provider = WordCountProvider::new();
        let content = "First line.\n\nSecond paragraph.\nThird line.";
        let context = ProviderContext {
            content,
            file_path: None,
            commands: &super::super::CommandRegistry::new(),
        };

        provider.activate(&context).unwrap();
        let stats = provider.get_stats();

        // "First", "line", "Second", "paragraph", "Third", "line" = 6 words
        assert_eq!(stats.words, 6);
        assert_eq!(stats.lines, 4);
        assert_eq!(stats.paragraphs, 2);
    }

    #[test]
    fn test_word_count_with_apostrophes() {
        let mut provider = WordCountProvider::new();
        let context = ProviderContext {
            content: "Don't worry, it's well-known.",
            file_path: None,
            commands: &super::super::CommandRegistry::new(),
        };

        provider.activate(&context).unwrap();
        let stats = provider.get_stats();

        // "Don't", "worry", "it's", "well-known" = 4 words
        assert_eq!(stats.words, 4);
    }

    #[test]
    fn test_empty_content() {
        let mut provider = WordCountProvider::new();
        let context = ProviderContext {
            content: "",
            file_path: None,
            commands: &super::super::CommandRegistry::new(),
        };

        provider.activate(&context).unwrap();
        let stats = provider.get_stats();

        assert_eq!(stats.words, 0);
        assert_eq!(stats.chars, 0);
        assert_eq!(stats.lines, 0);
    }
}
