//! Pattern Detector - Identify recurring themes and patterns across sessions
//!
//! Detects:
//! - Recurring problems/questions
//! - Common file access patterns
//! - Topic clusters
//! - Workflow patterns

use std::sync::Arc;
use std::collections::HashMap;
use chrono::{Duration, Utc};

use crate::semantic_search::{SemanticSearch, SearchOptions};
use crate::memory::SecurityTier;
use super::{ProactiveSuggestion, SuggestionType};

pub struct PatternDetector {
    search: Arc<SemanticSearch>,
    /// Detected patterns
    patterns: Vec<DetectedPattern>,
    /// Minimum occurrences to consider a pattern
    min_occurrences: usize,
    /// Lookback period for pattern detection
    lookback_days: i64,
}

#[derive(Debug, Clone)]
pub struct DetectedPattern {
    /// Pattern identifier
    pub id: String,
    /// Description of the pattern
    pub description: String,
    /// How many times this pattern was observed
    pub occurrences: usize,
    /// Representative examples
    pub examples: Vec<String>,
    /// When the pattern was detected
    pub detected_at: chrono::DateTime<Utc>,
    /// Confidence score
    pub confidence: f32,
    /// Pattern type
    pub pattern_type: PatternType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternType {
    /// Same question/problem appearing multiple times
    RecurringProblem,
    /// Files frequently accessed together
    FileCluster,
    /// Topics that appear together
    TopicCluster,
    /// Sequence of actions that repeats
    WorkflowPattern,
}

impl PatternDetector {
    pub fn new(search: Arc<SemanticSearch>) -> Self {
        Self {
            search,
            patterns: Vec::new(),
            min_occurrences: 3,
            lookback_days: 30,
        }
    }

    /// Find similar problems from the past
    pub async fn find_similar_problems(&self, query: &str) -> Result<Vec<ProactiveSuggestion>, String> {
        let mut suggestions = Vec::new();

        // Search for similar past queries/problems
        let options = SearchOptions {
            limit: 10,
            max_tier: SecurityTier::Guarded,
            min_score: 0.8,
            ..Default::default()
        };

        let results = self.search.search(query, options).await
            .map_err(|e| e.to_string())?;

        // Group similar results to detect patterns
        let mut grouped: HashMap<String, Vec<_>> = HashMap::new();

        for result in results {
            // Use first 50 chars of content as grouping key (rough clustering)
            let key = result.object.content_as_str()
                .map(|s| s.chars().take(50).collect::<String>())
                .unwrap_or_default();

            grouped.entry(key).or_default().push(result);
        }

        // Find groups with multiple occurrences
        for (_, group) in grouped.iter().filter(|(_, g)| g.len() >= 2) {
            let first = &group[0];

            let title = format!("Similar problem solved {} times", group.len());

            let content = first.object.summary.clone()
                .or_else(|| first.object.content_as_str().map(|s| s.chars().take(300).collect()))
                .unwrap_or_default();

            let suggestion = ProactiveSuggestion::new(
                SuggestionType::SimilarSolution,
                &title,
                &content,
                &format!("You've encountered this {} times before", group.len()),
                first.score,
            ).with_sources(
                group.iter().map(|r| r.object.suid.to_string()).collect()
            );

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }

    /// Get active patterns that should be surfaced
    pub async fn get_active_patterns(&self) -> Result<Vec<ProactiveSuggestion>, String> {
        let mut suggestions = Vec::new();

        // Analyze recent sessions for patterns
        self.detect_patterns().await?;

        for pattern in &self.patterns {
            if pattern.occurrences >= self.min_occurrences {
                let suggestion = ProactiveSuggestion::new(
                    SuggestionType::DetectedPattern,
                    &format!("Pattern: {}", pattern.description),
                    &pattern.examples.join("\n"),
                    &format!("Detected {} times in recent sessions", pattern.occurrences),
                    pattern.confidence,
                );

                suggestions.push(suggestion);
            }
        }

        Ok(suggestions)
    }

    /// Detect file access patterns
    pub async fn detect_file_patterns(&self) -> Result<Vec<FilePattern>, String> {
        let mut patterns = Vec::new();

        // Get recent sessions
        let options = SearchOptions {
            limit: 100,
            max_tier: SecurityTier::Guarded,
                        ..Default::default()
        };

        let results = self.search.search("session files", options).await
            .map_err(|e| e.to_string())?;

        // Extract file lists from sessions
        let mut file_cooccurrence: HashMap<(String, String), usize> = HashMap::new();

        for result in &results {
            let files: Vec<String> = result.object.metadata
                .get("all_files")
                .or_else(|| result.object.metadata.get("files"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| f.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            // Count co-occurrences
            for i in 0..files.len() {
                for j in (i + 1)..files.len() {
                    let key = if files[i] < files[j] {
                        (files[i].clone(), files[j].clone())
                    } else {
                        (files[j].clone(), files[i].clone())
                    };
                    *file_cooccurrence.entry(key).or_insert(0) += 1;
                }
            }
        }

        // Find frequently co-occurring files
        for ((file1, file2), count) in file_cooccurrence {
            if count >= self.min_occurrences {
                patterns.push(FilePattern {
                    files: vec![file1, file2],
                    cooccurrence_count: count,
                });
            }
        }

        Ok(patterns)
    }

    /// Internal pattern detection
    async fn detect_patterns(&self) -> Result<(), String> {
        // Get recent content
        let cutoff = Utc::now() - Duration::days(self.lookback_days);

        let options = SearchOptions {
            limit: 200,
            max_tier: SecurityTier::Guarded,
            ..Default::default()
        };

        let results = self.search.search("work session problem question", options).await
            .map_err(|e| e.to_string())?;

        // Filter by date
        let recent: Vec<_> = results.into_iter()
            .filter(|r| r.object.created_at > cutoff)
            .collect();

        // Simple keyword-based pattern detection
        let mut keyword_counts: HashMap<String, usize> = HashMap::new();

        for result in &recent {
            let content = result.object.content_as_str().unwrap_or("");

            // Extract significant words (simple approach)
            for word in content.split_whitespace() {
                let clean = word.to_lowercase()
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>();

                if clean.len() > 4 && !is_common_word(&clean) {
                    *keyword_counts.entry(clean).or_insert(0) += 1;
                }
            }
        }

        // Find recurring keywords
        let _recurring: Vec<_> = keyword_counts.into_iter()
            .filter(|(_, count)| *count >= self.min_occurrences)
            .collect();

        // TODO: Convert to patterns and store in self.patterns

        Ok(())
    }

    /// Get pattern statistics
    pub fn get_stats(&self) -> PatternStats {
        PatternStats {
            total_patterns: self.patterns.len(),
            recurring_problems: self.patterns.iter()
                .filter(|p| p.pattern_type == PatternType::RecurringProblem)
                .count(),
            file_clusters: self.patterns.iter()
                .filter(|p| p.pattern_type == PatternType::FileCluster)
                .count(),
            topic_clusters: self.patterns.iter()
                .filter(|p| p.pattern_type == PatternType::TopicCluster)
                .count(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FilePattern {
    pub files: Vec<String>,
    pub cooccurrence_count: usize,
}

#[derive(Debug, Clone)]
pub struct PatternStats {
    pub total_patterns: usize,
    pub recurring_problems: usize,
    pub file_clusters: usize,
    pub topic_clusters: usize,
}

/// Check if a word is too common to be a pattern indicator
fn is_common_word(word: &str) -> bool {
    const COMMON: &[&str] = &[
        "the", "and", "for", "that", "this", "with", "from", "have", "will",
        "what", "when", "where", "which", "while", "about", "after", "before",
        "being", "between", "could", "does", "doing", "during", "each", "either",
        "function", "return", "import", "export", "const", "class", "public",
        "private", "static", "void", "string", "number", "boolean", "async",
        "await", "error", "result", "value", "data", "file", "path", "name",
    ];

    COMMON.contains(&word)
}
