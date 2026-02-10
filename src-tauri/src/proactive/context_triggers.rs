//! Context Triggers - Detect when past work is relevant to current context
//!
//! Monitors:
//! - File paths being accessed
//! - Queries and prompts
//! - Code patterns
//! - Topic discussions

use std::sync::Arc;
use std::path::Path;

use crate::semantic_search::{SemanticSearch, SearchOptions};
use crate::memory::SecurityTier;
use super::{ProactiveSuggestion, SuggestionType};

pub struct ContextTriggerEngine {
    search: Arc<SemanticSearch>,
    /// Minimum similarity score to trigger
    min_similarity: f32,
    /// Maximum results per trigger
    max_results: usize,
}

impl ContextTriggerEngine {
    pub fn new(search: Arc<SemanticSearch>) -> Self {
        Self {
            search,
            min_similarity: 0.75,
            max_results: 5,
        }
    }

    /// Trigger context suggestions based on a file being opened
    pub async fn trigger_for_file(&self, file_path: &str) -> Result<Vec<ProactiveSuggestion>, String> {
        let mut suggestions = Vec::new();

        // Extract meaningful parts from the file path
        let path = Path::new(file_path);
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // Build a context query from the file
        let query = self.build_file_context_query(file_path);

        // Search for related past work
        let options = SearchOptions {
            limit: self.max_results,
            max_tier: SecurityTier::Guarded,
            min_score: self.min_similarity,
            ..Default::default()
        };

        let results = self.search.search(&query, options).await
            .map_err(|e| e.to_string())?;

        for result in results {
            // Skip if it's about the exact same file (not useful)
            let is_same_file = result.object.metadata
                .get("files")
                .or_else(|| result.object.metadata.get("all_files"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().any(|f| {
                    f.as_str().map_or(false, |s| s.contains(filename))
                }))
                .unwrap_or(false);

            if is_same_file && result.score > 0.95 {
                continue;
            }

            let title = result.object.name.clone()
                .unwrap_or_else(|| "Related work".to_string());

            let preview = result.object.content_as_str()
                .map(|s| s.chars().take(200).collect::<String>())
                .unwrap_or_default();

            let suggestion = ProactiveSuggestion::new(
                SuggestionType::RelatedContext,
                &title,
                &preview,
                &format!("You worked on something similar involving {}", filename),
                result.score,
            ).with_sources(vec![result.object.suid.to_string()]);

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }

    /// Trigger context suggestions based on a query/prompt
    pub async fn trigger_for_query(&self, query: &str) -> Result<Vec<ProactiveSuggestion>, String> {
        let mut suggestions = Vec::new();

        // Skip very short queries
        if query.len() < 10 {
            return Ok(suggestions);
        }

        let options = SearchOptions {
            limit: self.max_results,
            max_tier: SecurityTier::Guarded,
            min_score: self.min_similarity,
            ..Default::default()
        };

        let results = self.search.search(query, options).await
            .map_err(|e| e.to_string())?;

        for result in results {
            let title = result.object.name.clone()
                .unwrap_or_else(|| "Related discussion".to_string());

            let preview = result.object.summary.clone()
                .or_else(|| {
                    result.object.content_as_str()
                        .map(|s| s.chars().take(300).collect::<String>())
                })
                .unwrap_or_default();

            // Determine what kind of content this is
            let content_kind = self.determine_content_kind(&result.object);

            let suggestion = ProactiveSuggestion::new(
                SuggestionType::RelatedContext,
                &title,
                &preview,
                &format!("Found relevant {} from past work", content_kind),
                result.score,
            ).with_sources(vec![result.object.suid.to_string()]);

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }

    /// Trigger for code patterns (e.g., detecting similar implementations)
    pub async fn trigger_for_code(&self, code_snippet: &str) -> Result<Vec<ProactiveSuggestion>, String> {
        let mut suggestions = Vec::new();

        // Extract key patterns from code
        let patterns = self.extract_code_patterns(code_snippet);
        if patterns.is_empty() {
            return Ok(suggestions);
        }

        let query = patterns.join(" ");

        let options = SearchOptions {
            limit: 3,
            max_tier: SecurityTier::Guarded,
            min_score: 0.8,
            ..Default::default()
        };

        let results = self.search.search(&query, options).await
            .map_err(|e| e.to_string())?;

        for result in results {
            let title = result.object.name.clone()
                .unwrap_or_else(|| "Similar code".to_string());

            let preview = result.object.content_as_str()
                .map(|s| s.chars().take(300).collect::<String>())
                .unwrap_or_default();

            let suggestion = ProactiveSuggestion::new(
                SuggestionType::SimilarSolution,
                &title,
                &preview,
                "Found similar code pattern from past sessions",
                result.score,
            ).with_sources(vec![result.object.suid.to_string()]);

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }

    fn build_file_context_query(&self, file_path: &str) -> String {
        let path = Path::new(file_path);

        let mut query_parts = Vec::new();

        // Add filename
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            query_parts.push(name.to_string());
        }

        // Add parent directory name (often meaningful, e.g., "components", "services")
        if let Some(parent) = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) {
            if !["src", "lib", "app"].contains(&parent) {
                query_parts.push(parent.to_string());
            }
        }

        // Add extension-based context
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext {
                "rs" => query_parts.push("Rust".to_string()),
                "ts" | "tsx" => query_parts.push("TypeScript".to_string()),
                "py" => query_parts.push("Python".to_string()),
                "go" => query_parts.push("Go".to_string()),
                _ => {}
            }
        }

        query_parts.join(" ")
    }

    fn determine_content_kind(&self, object: &crate::semantic_object::SemanticObject) -> &'static str {
        // Check tags first
        if object.tags.contains(&"session".to_string()) {
            return "session";
        }
        if object.tags.contains(&"decision".to_string()) {
            return "decision";
        }
        if object.tags.contains(&"note".to_string()) {
            return "note";
        }
        if object.tags.contains(&"chatgpt".to_string()) || object.tags.contains(&"cursor".to_string()) {
            return "AI conversation";
        }

        // Check content type
        match &object.content_type {
            crate::semantic_object::ContentType::Code { .. } => "code",
            crate::semantic_object::ContentType::Markdown => "document",
            _ => "content",
        }
    }

    fn extract_code_patterns(&self, code: &str) -> Vec<String> {
        let mut patterns = Vec::new();

        // Extract function names (simple heuristic)
        for line in code.lines() {
            let trimmed = line.trim();

            // Rust/Go function
            if trimmed.starts_with("fn ") || trimmed.starts_with("func ") {
                if let Some(name) = trimmed.split_whitespace().nth(1) {
                    let clean_name = name.trim_start_matches('(').split('(').next().unwrap_or(name);
                    patterns.push(clean_name.to_string());
                }
            }

            // Python function/class
            if trimmed.starts_with("def ") || trimmed.starts_with("class ") {
                if let Some(name) = trimmed.split_whitespace().nth(1) {
                    let clean_name = name.split('(').next().unwrap_or(name);
                    patterns.push(clean_name.to_string());
                }
            }

            // TypeScript/JavaScript function
            if trimmed.starts_with("function ") || trimmed.contains("const ") && trimmed.contains("=>") {
                if let Some(name) = trimmed.split_whitespace().nth(1) {
                    let clean_name = name.split('(').next().unwrap_or(name);
                    patterns.push(clean_name.to_string());
                }
            }
        }

        // Extract import/use statements
        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("use ") || trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                // Extract the module/package name
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    patterns.push(parts[1].trim_end_matches(';').to_string());
                }
            }
        }

        patterns
    }
}
