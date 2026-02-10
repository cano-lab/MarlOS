//! Decision Tracker - Surface past decisions when relevant
//!
//! Features:
//! - Find decisions related to current work
//! - Detect potential conflicts with past decisions
//! - Remind about decisions when revisiting topics

use std::sync::Arc;
use chrono::{Duration, Utc};

use crate::semantic_search::{SemanticSearch, SearchOptions};
use crate::memory::SecurityTier;
use super::{ProactiveSuggestion, SuggestionType};

pub struct DecisionTracker {
    search: Arc<SemanticSearch>,
    /// How far back to look for decisions (days)
    lookback_days: i64,
    /// Minimum similarity for a decision to be relevant
    min_relevance: f32,
}

impl DecisionTracker {
    pub fn new(search: Arc<SemanticSearch>) -> Self {
        Self {
            search,
            lookback_days: 90,
            min_relevance: 0.7,
        }
    }

    /// Find decisions relevant to a file being worked on
    pub async fn find_relevant_decisions(&self, file_path: &str) -> Result<Vec<ProactiveSuggestion>, String> {
        let mut suggestions = Vec::new();

        // Build query from file context
        let query = format!("decision about {}", file_path);

        let options = SearchOptions {
            limit: 5,
            max_tier: SecurityTier::Guarded,
            min_score: self.min_relevance,
            ..Default::default()
        };

        let results = self.search.search(&query, options).await
            .map_err(|e| e.to_string())?;

        for result in results {
            // Extract decision details from metadata
            let topic = result.object.metadata
                .get("topic")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown topic");

            let choice = result.object.metadata
                .get("choice")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown choice");

            let reasoning = result.object.metadata
                .get("reasoning")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let title = format!("Past Decision: {}", topic);
            let content = format!(
                "Choice: {}\n\nReasoning: {}",
                choice,
                reasoning.chars().take(300).collect::<String>()
            );

            // Calculate how old the decision is
            let age_days = (Utc::now() - result.object.created_at).num_days();
            let age_str = if age_days == 0 {
                "today".to_string()
            } else if age_days == 1 {
                "yesterday".to_string()
            } else if age_days < 30 {
                format!("{} days ago", age_days)
            } else {
                format!("{} months ago", age_days / 30)
            };

            let suggestion = ProactiveSuggestion::new(
                SuggestionType::PastDecision,
                &title,
                &content,
                &format!("You made this decision {} while working on related code", age_str),
                result.score,
            ).with_sources(vec![result.object.suid.to_string()]);

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }

    /// Find decisions relevant to a query/topic
    pub async fn find_for_query(&self, query: &str) -> Result<Vec<ProactiveSuggestion>, String> {
        let mut suggestions = Vec::new();

        let options = SearchOptions {
            limit: 3,
            max_tier: SecurityTier::Guarded,
            min_score: self.min_relevance,
            ..Default::default()
        };

        let results = self.search.search(query, options).await
            .map_err(|e| e.to_string())?;

        for result in results {
            let topic = result.object.metadata
                .get("topic")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown topic");

            let choice = result.object.metadata
                .get("choice")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown choice");

            let reasoning = result.object.metadata
                .get("reasoning")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let title = format!("Related Decision: {}", topic);
            let content = format!(
                "Choice: {}\n\nReasoning: {}",
                choice,
                reasoning.chars().take(300).collect::<String>()
            );

            let suggestion = ProactiveSuggestion::new(
                SuggestionType::PastDecision,
                &title,
                &content,
                "This past decision may be relevant to your current question",
                result.score,
            ).with_sources(vec![result.object.suid.to_string()]);

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }

    /// Find related decisions for a specific topic
    pub async fn find_related_decisions(&self, topic: &str) -> Result<Vec<ProactiveSuggestion>, String> {
        self.find_for_query(&format!("decision {}", topic)).await
    }

    /// Check if a proposed decision conflicts with past decisions
    pub async fn check_conflicts(
        &self,
        topic: &str,
        proposed_choice: &str,
    ) -> Result<Vec<ProactiveSuggestion>, String> {
        let mut suggestions = Vec::new();

        // Search for past decisions on the same topic
        let query = format!("decision about {}", topic);

        let options = SearchOptions {
            limit: 10,
            max_tier: SecurityTier::Guarded,
            min_score: 0.8, // Higher threshold for conflict detection
            ..Default::default()
        };

        let results = self.search.search(&query, options).await
            .map_err(|e| e.to_string())?;

        for result in results {
            let past_choice = result.object.metadata
                .get("choice")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let past_topic = result.object.metadata
                .get("topic")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Check if this might be a conflict
            // (simple heuristic: different choice for similar topic)
            if self.might_conflict(proposed_choice, past_choice) && result.score > 0.85 {
                let reasoning = result.object.metadata
                    .get("reasoning")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let title = format!("Potential Conflict: {}", past_topic);
                let content = format!(
                    "You previously chose: {}\n\nNow proposing: {}\n\nPast reasoning: {}",
                    past_choice,
                    proposed_choice,
                    reasoning.chars().take(200).collect::<String>()
                );

                let suggestion = ProactiveSuggestion::new(
                    SuggestionType::DecisionConflict,
                    &title,
                    &content,
                    "This may conflict with a previous decision on the same topic",
                    result.score,
                ).with_sources(vec![result.object.suid.to_string()]);

                suggestions.push(suggestion);
            }
        }

        Ok(suggestions)
    }

    /// Simple heuristic to detect if two choices might conflict
    fn might_conflict(&self, choice1: &str, choice2: &str) -> bool {
        let c1 = choice1.to_lowercase();
        let c2 = choice2.to_lowercase();

        // Exact match is not a conflict
        if c1 == c2 {
            return false;
        }

        // Check for opposite patterns
        let opposites = [
            ("use", "don't use"),
            ("enable", "disable"),
            ("add", "remove"),
            ("yes", "no"),
            ("allow", "deny"),
            ("include", "exclude"),
            ("sync", "async"),
            ("static", "dynamic"),
        ];

        for (a, b) in opposites {
            if (c1.contains(a) && c2.contains(b)) || (c1.contains(b) && c2.contains(a)) {
                return true;
            }
        }

        // If choices are substantially different (low word overlap), might be conflict
        let words1: std::collections::HashSet<&str> = c1.split_whitespace().collect();
        let words2: std::collections::HashSet<&str> = c2.split_whitespace().collect();

        let intersection = words1.intersection(&words2).count();
        let union = words1.union(&words2).count();

        if union > 0 {
            let jaccard = intersection as f32 / union as f32;
            // Low overlap suggests different choices
            jaccard < 0.3
        } else {
            false
        }
    }

    /// Get decision statistics
    pub async fn get_stats(&self) -> Result<DecisionStats, String> {
        let cutoff = Utc::now() - Duration::days(self.lookback_days);

        // This would ideally query the database directly
        // For now, use search with a broad query
        let options = SearchOptions {
            limit: 1000,
            max_tier: SecurityTier::Guarded,
            ..Default::default()
        };

        let results = self.search.search("decision", options).await
            .map_err(|e| e.to_string())?;

        let total = results.len();
        let recent = results.iter()
            .filter(|r| r.object.created_at > cutoff)
            .count();

        // Count by topic (simple word extraction)
        let mut topics: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for result in &results {
            if let Some(topic) = result.object.metadata.get("topic").and_then(|v| v.as_str()) {
                let first_word = topic.split_whitespace().next().unwrap_or(topic);
                *topics.entry(first_word.to_lowercase()).or_insert(0) += 1;
            }
        }

        let top_topics: Vec<(String, usize)> = {
            let mut v: Vec<_> = topics.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            v.into_iter().take(5).collect()
        };

        Ok(DecisionStats {
            total_decisions: total,
            recent_decisions: recent,
            top_topics,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DecisionStats {
    pub total_decisions: usize,
    pub recent_decisions: usize,
    pub top_topics: Vec<(String, usize)>,
}
