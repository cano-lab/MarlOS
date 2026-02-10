//! Proactive Intelligence Module
//!
//! Automatically surfaces relevant context, decisions, and patterns
//! based on current work without explicit queries.
//!
//! Features:
//! - Context triggers: Detect when past work is relevant
//! - Decision reminders: Surface past decisions when revisiting topics
//! - Pattern detection: Identify recurring themes across sessions
//! - Work continuity: Remember where you left off

pub mod context_triggers;
pub mod decision_tracker;
pub mod pattern_detector;
pub mod work_journal;

pub use context_triggers::*;
pub use decision_tracker::*;
pub use pattern_detector::*;
pub use work_journal::*;

use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::semantic_search::SemanticSearch;

/// A proactive suggestion surfaced by the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveSuggestion {
    /// Unique ID for this suggestion
    pub id: String,
    /// Type of suggestion
    pub suggestion_type: SuggestionType,
    /// Relevance score (0.0 to 1.0)
    pub relevance: f32,
    /// Title/summary of the suggestion
    pub title: String,
    /// Detailed content
    pub content: String,
    /// Why this was suggested
    pub reason: String,
    /// Source object IDs
    pub source_ids: Vec<String>,
    /// When this was generated
    pub generated_at: DateTime<Utc>,
    /// Has user seen this?
    pub seen: bool,
    /// Has user dismissed this?
    pub dismissed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionType {
    /// Related past work
    RelatedContext,
    /// A past decision that may apply
    PastDecision,
    /// A pattern detected across sessions
    DetectedPattern,
    /// Continuation of previous work
    WorkContinuity,
    /// Potential conflict with past decision
    DecisionConflict,
    /// Similar problem solved before
    SimilarSolution,
}

impl ProactiveSuggestion {
    pub fn new(
        suggestion_type: SuggestionType,
        title: &str,
        content: &str,
        reason: &str,
        relevance: f32,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            suggestion_type,
            relevance,
            title: title.to_string(),
            content: content.to_string(),
            reason: reason.to_string(),
            source_ids: Vec::new(),
            generated_at: Utc::now(),
            seen: false,
            dismissed: false,
        }
    }

    pub fn with_sources(mut self, ids: Vec<String>) -> Self {
        self.source_ids = ids;
        self
    }
}

/// The proactive engine that monitors context and generates suggestions
pub struct ProactiveEngine {
    search: Arc<SemanticSearch>,
    context_triggers: ContextTriggerEngine,
    decision_tracker: DecisionTracker,
    pattern_detector: PatternDetector,
    work_journal: WorkJournal,
    /// Active suggestions
    suggestions: Vec<ProactiveSuggestion>,
    /// Configuration
    config: ProactiveConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveConfig {
    /// Minimum relevance score to surface a suggestion
    pub min_relevance: f32,
    /// Maximum suggestions to show at once
    pub max_suggestions: usize,
    /// How far back to look for patterns (days)
    pub pattern_lookback_days: i64,
    /// Whether to track file access patterns
    pub track_file_access: bool,
    /// Whether to detect decision conflicts
    pub detect_conflicts: bool,
}

impl Default for ProactiveConfig {
    fn default() -> Self {
        Self {
            min_relevance: 0.7,
            max_suggestions: 5,
            pattern_lookback_days: 30,
            track_file_access: true,
            detect_conflicts: true,
        }
    }
}

impl ProactiveEngine {
    pub fn new(search: Arc<SemanticSearch>) -> Self {
        Self {
            search: search.clone(),
            context_triggers: ContextTriggerEngine::new(search.clone()),
            decision_tracker: DecisionTracker::new(search.clone()),
            pattern_detector: PatternDetector::new(search.clone()),
            work_journal: WorkJournal::new(search),
            suggestions: Vec::new(),
            config: ProactiveConfig::default(),
        }
    }

    pub fn with_config(mut self, config: ProactiveConfig) -> Self {
        self.config = config;
        self
    }

    /// Called when user starts working on a file
    pub async fn on_file_opened(&mut self, file_path: &str) -> Vec<ProactiveSuggestion> {
        let mut suggestions = Vec::new();

        // Check for related past work
        if let Ok(context_suggestions) = self.context_triggers.trigger_for_file(file_path).await {
            suggestions.extend(context_suggestions);
        }

        // Check for relevant decisions
        if let Ok(decision_suggestions) = self.decision_tracker.find_relevant_decisions(file_path).await {
            suggestions.extend(decision_suggestions);
        }

        // Log file access for pattern detection
        if self.config.track_file_access {
            self.work_journal.log_file_access(file_path).await;
        }

        self.filter_and_store(suggestions)
    }

    /// Called when user types a query or prompt
    pub async fn on_query(&mut self, query: &str) -> Vec<ProactiveSuggestion> {
        let mut suggestions = Vec::new();

        // Check for related past work
        if let Ok(context_suggestions) = self.context_triggers.trigger_for_query(query).await {
            suggestions.extend(context_suggestions);
        }

        // Check for relevant decisions
        if let Ok(decision_suggestions) = self.decision_tracker.find_for_query(query).await {
            suggestions.extend(decision_suggestions);
        }

        // Check for similar past solutions
        if let Ok(pattern_suggestions) = self.pattern_detector.find_similar_problems(query).await {
            suggestions.extend(pattern_suggestions);
        }

        self.filter_and_store(suggestions)
    }

    /// Called when starting a new session
    pub async fn on_session_start(&mut self, project_path: Option<&str>) -> Vec<ProactiveSuggestion> {
        let mut suggestions = Vec::new();

        // Check for work continuity
        if let Some(path) = project_path {
            if let Ok(continuity) = self.work_journal.get_continuity_context(path).await {
                suggestions.extend(continuity);
            }
        }

        // Surface any detected patterns
        if let Ok(patterns) = self.pattern_detector.get_active_patterns().await {
            suggestions.extend(patterns);
        }

        self.filter_and_store(suggestions)
    }

    /// Called when a decision is being made
    pub async fn on_decision_context(&mut self, topic: &str, proposed_choice: &str) -> Vec<ProactiveSuggestion> {
        let mut suggestions = Vec::new();

        // Check for conflicting past decisions
        if self.config.detect_conflicts {
            if let Ok(conflicts) = self.decision_tracker.check_conflicts(topic, proposed_choice).await {
                suggestions.extend(conflicts);
            }
        }

        // Find related past decisions
        if let Ok(related) = self.decision_tracker.find_related_decisions(topic).await {
            suggestions.extend(related);
        }

        self.filter_and_store(suggestions)
    }

    /// Mark a suggestion as seen
    pub fn mark_seen(&mut self, suggestion_id: &str) {
        if let Some(s) = self.suggestions.iter_mut().find(|s| s.id == suggestion_id) {
            s.seen = true;
        }
    }

    /// Dismiss a suggestion
    pub fn dismiss(&mut self, suggestion_id: &str) {
        if let Some(s) = self.suggestions.iter_mut().find(|s| s.id == suggestion_id) {
            s.dismissed = true;
        }
    }

    /// Get all active (unseen, undismissed) suggestions
    pub fn get_active_suggestions(&self) -> Vec<&ProactiveSuggestion> {
        self.suggestions.iter()
            .filter(|s| !s.dismissed)
            .take(self.config.max_suggestions)
            .collect()
    }

    /// Get suggestions of a specific type
    pub fn get_suggestions_by_type(&self, suggestion_type: SuggestionType) -> Vec<&ProactiveSuggestion> {
        self.suggestions.iter()
            .filter(|s| s.suggestion_type == suggestion_type && !s.dismissed)
            .collect()
    }

    fn filter_and_store(&mut self, new_suggestions: Vec<ProactiveSuggestion>) -> Vec<ProactiveSuggestion> {
        // Filter by minimum relevance
        let filtered: Vec<ProactiveSuggestion> = new_suggestions
            .into_iter()
            .filter(|s| s.relevance >= self.config.min_relevance)
            .collect();

        // Add to stored suggestions
        self.suggestions.extend(filtered.clone());

        // Sort by relevance
        self.suggestions.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());

        // Keep only top suggestions
        self.suggestions.truncate(self.config.max_suggestions * 2);

        filtered
    }
}
