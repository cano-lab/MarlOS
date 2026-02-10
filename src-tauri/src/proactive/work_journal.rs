//! Work Journal - Track work continuity across sessions
//!
//! Features:
//! - Log file access for pattern detection
//! - Remember where you left off in a project
//! - Track session context for resumption

use std::sync::Arc;
use std::collections::HashMap;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::semantic_search::{SemanticSearch, SearchOptions};
use crate::semantic_object::{ContentType, SemanticObject};
use crate::memory::SecurityTier;
use super::{ProactiveSuggestion, SuggestionType};

pub struct WorkJournal {
    search: Arc<SemanticSearch>,
    /// In-memory file access log (would persist to DB in production)
    file_access_log: Vec<FileAccess>,
    /// Session context cache
    session_contexts: HashMap<String, SessionContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccess {
    pub file_path: String,
    pub accessed_at: DateTime<Utc>,
    pub access_type: AccessType,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccessType {
    Read,
    Write,
    Search,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    /// Project path
    pub project_path: String,
    /// Last active file
    pub last_file: Option<String>,
    /// Recent files accessed
    pub recent_files: Vec<String>,
    /// Last activity time
    pub last_activity: DateTime<Utc>,
    /// What was being worked on
    pub work_summary: Option<String>,
    /// Pending tasks/notes
    pub pending_notes: Vec<String>,
}

impl WorkJournal {
    pub fn new(search: Arc<SemanticSearch>) -> Self {
        Self {
            search,
            file_access_log: Vec::new(),
            session_contexts: HashMap::new(),
        }
    }

    /// Log a file access
    pub async fn log_file_access(&mut self, file_path: &str) {
        let access = FileAccess {
            file_path: file_path.to_string(),
            accessed_at: Utc::now(),
            access_type: AccessType::Read,
            project_path: self.extract_project_path(file_path),
        };

        self.file_access_log.push(access.clone());

        // Update session context
        if let Some(project) = &access.project_path {
            let context = self.session_contexts
                .entry(project.clone())
                .or_insert_with(|| SessionContext {
                    project_path: project.clone(),
                    last_file: None,
                    recent_files: Vec::new(),
                    last_activity: Utc::now(),
                    work_summary: None,
                    pending_notes: Vec::new(),
                });

            context.last_file = Some(file_path.to_string());
            context.last_activity = Utc::now();

            // Add to recent files (keep last 10)
            if !context.recent_files.contains(&file_path.to_string()) {
                context.recent_files.push(file_path.to_string());
                if context.recent_files.len() > 10 {
                    context.recent_files.remove(0);
                }
            }
        }

        // Trim old entries
        let cutoff = Utc::now() - Duration::days(7);
        self.file_access_log.retain(|a| a.accessed_at > cutoff);
    }

    /// Get work continuity context when starting a new session
    pub async fn get_continuity_context(&self, project_path: &str) -> Result<Vec<ProactiveSuggestion>, String> {
        let mut suggestions = Vec::new();

        // Check if we have previous context for this project
        if let Some(context) = self.session_contexts.get(project_path) {
            let time_since_last = Utc::now() - context.last_activity;

            // If it's been a while, remind about previous context
            if time_since_last.num_hours() > 1 {
                let time_str = format_duration(time_since_last);

                let mut content = String::new();

                if let Some(last_file) = &context.last_file {
                    content.push_str(&format!("Last file: {}\n", last_file));
                }

                if !context.recent_files.is_empty() {
                    content.push_str("\nRecent files:\n");
                    for file in context.recent_files.iter().rev().take(5) {
                        content.push_str(&format!("  - {}\n", file));
                    }
                }

                if let Some(summary) = &context.work_summary {
                    content.push_str(&format!("\nPrevious work: {}\n", summary));
                }

                if !context.pending_notes.is_empty() {
                    content.push_str("\nPending notes:\n");
                    for note in &context.pending_notes {
                        content.push_str(&format!("  - {}\n", note));
                    }
                }

                let suggestion = ProactiveSuggestion::new(
                    SuggestionType::WorkContinuity,
                    "Continue Previous Work",
                    &content,
                    &format!("You last worked on this project {}", time_str),
                    0.9,
                );

                suggestions.push(suggestion);
            }
        }

        // Also search for recent sessions in this project
        let session_suggestions = self.find_recent_sessions(project_path).await?;
        suggestions.extend(session_suggestions);

        Ok(suggestions)
    }

    /// Find recent sessions for a project
    async fn find_recent_sessions(&self, project_path: &str) -> Result<Vec<ProactiveSuggestion>, String> {
        let mut suggestions = Vec::new();

        // Search for sessions in this project
        let query = format!("session project {}", project_path);

        let options = SearchOptions {
            limit: 5,
            max_tier: SecurityTier::Guarded,
                        ..Default::default()
        };

        let results = self.search.search(&query, options).await
            .map_err(|e| e.to_string())?;

        // Get the most recent session
        if let Some(recent) = results.first() {
            let time_since = Utc::now() - recent.object.created_at;

            // Only suggest if it's recent but not too recent
            if time_since.num_hours() > 1 && time_since.num_days() < 7 {
                let summary = recent.object.summary.clone()
                    .or_else(|| recent.object.content_as_str().map(|s| s.chars().take(300).collect()))
                    .unwrap_or_default();

                let suggestion = ProactiveSuggestion::new(
                    SuggestionType::WorkContinuity,
                    "Recent Session",
                    &summary,
                    &format!("Your last session was {}", format_duration(time_since)),
                    0.8,
                ).with_sources(vec![recent.object.suid.to_string()]);

                suggestions.push(suggestion);
            }
        }

        Ok(suggestions)
    }

    /// Save work summary when ending a session
    pub async fn save_work_summary(&mut self, project_path: &str, summary: &str) {
        if let Some(context) = self.session_contexts.get_mut(project_path) {
            context.work_summary = Some(summary.to_string());
            context.last_activity = Utc::now();
        }
    }

    /// Add a pending note/TODO
    pub async fn add_pending_note(&mut self, project_path: &str, note: &str) {
        let context = self.session_contexts
            .entry(project_path.to_string())
            .or_insert_with(|| SessionContext {
                project_path: project_path.to_string(),
                last_file: None,
                recent_files: Vec::new(),
                last_activity: Utc::now(),
                work_summary: None,
                pending_notes: Vec::new(),
            });

        context.pending_notes.push(note.to_string());
    }

    /// Clear pending notes (when completed)
    pub fn clear_pending_notes(&mut self, project_path: &str) {
        if let Some(context) = self.session_contexts.get_mut(project_path) {
            context.pending_notes.clear();
        }
    }

    /// Get file access patterns for a project
    pub fn get_file_patterns(&self, project_path: &str) -> Vec<(String, usize)> {
        let mut counts: HashMap<String, usize> = HashMap::new();

        for access in &self.file_access_log {
            if access.project_path.as_deref() == Some(project_path) {
                *counts.entry(access.file_path.clone()).or_insert(0) += 1;
            }
        }

        let mut patterns: Vec<_> = counts.into_iter().collect();
        patterns.sort_by(|a, b| b.1.cmp(&a.1));
        patterns
    }

    /// Persist session context to the database
    pub async fn persist_context(&self, project_path: &str) -> Result<(), String> {
        if let Some(context) = self.session_contexts.get(project_path) {
            let content = serde_json::to_string_pretty(context)
                .map_err(|e| e.to_string())?;

            let mut obj = SemanticObject::new(
                content.as_bytes().to_vec(),
                ContentType::Structured { schema: "work-context".to_string() },
            );

            obj.name = Some(format!("Work Context: {}", project_path));
            obj.security_tier = SecurityTier::Guarded;
            obj.metadata.insert("project_path".to_string(), serde_json::json!(project_path));
            obj.tags.push("work-context".to_string());
            obj.tags.push("journal".to_string());

            let store = self.search.store.blocking_write();
            store.create(&obj).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn extract_project_path(&self, file_path: &str) -> Option<String> {
        let path = std::path::Path::new(file_path);

        // Walk up until we find a marker directory
        let markers = [".git", ".hg", "Cargo.toml", "package.json", "go.mod", "pyproject.toml"];

        let mut current = path.parent();
        while let Some(dir) = current {
            for marker in &markers {
                if dir.join(marker).exists() {
                    return Some(dir.to_string_lossy().to_string());
                }
            }
            current = dir.parent();
        }

        // Fall back to parent directory
        path.parent().map(|p| p.to_string_lossy().to_string())
    }
}

fn format_duration(duration: Duration) -> String {
    let hours = duration.num_hours();
    let days = duration.num_days();

    if days > 0 {
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    } else if hours > 0 {
        format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
    } else {
        "just now".to_string()
    }
}
