//! Session-Based Continuity - Time Machine for Your Work
//!
//! MarlOS tracks everything you do in "sessions" - chunks of focused work.
//! Later, you can resume any session to restore your entire mental context.
//!
//! Philosophy: Organize by time and thinking, not by files and folders.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod importer;
pub mod vector_bridge;
pub use importer::*;

/// Errors specific to session management
#[derive(Debug)]
pub enum SessionError {
    Io(String),
    Serialization(String),
    NotFound(String),
    InvalidState(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::Io(e) => write!(f, "IO error: {}", e),
            SessionError::Serialization(e) => write!(f, "Serialization error: {}", e),
            SessionError::NotFound(e) => write!(f, "Not found: {}", e),
            SessionError::InvalidState(e) => write!(f, "Invalid state: {}", e),
        }
    }
}

impl std::error::Error for SessionError {}

pub type Result<T> = std::result::Result<T, SessionError>;

/// A work session - a chunk of focused time with a specific intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub intent: SessionIntent,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub activities: Vec<SessionActivity>,
    pub snapshots: Vec<SessionSnapshot>,
    pub context: SessionContext,
    pub next_steps: Vec<String>,
    pub tags: Vec<String>,
}

/// What the user was trying to accomplish
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionIntent {
    Research { topic: String },
    Writing { project: String },
    Coding { project: String },
    Learning { subject: String },
    Planning { goal: String },
    Debugging { issue: String },
    Brainstorming { theme: String },
    Other { description: String },
}

/// An activity that happened during the session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionActivity {
    pub id: String,
    pub activity_type: ActivityType,
    pub timestamp: DateTime<Utc>,
    pub duration: Option<Duration>,
    pub details: ActivityDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivityType {
    /// Read/viewed a document
    DocumentView,
    /// Took notes
    NoteTaking,
    /// Had an AI conversation
    AiChat,
    /// Browser research
    WebResearch,
    /// Wrote code
    Coding,
    /// Paper writing
    Writing,
    /// Thinking/planning
    Thinking,
    /// Other activity
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivityDetails {
    ViewedDocument {
        title: String,
        path: String,
        page_number: Option<usize>,
        duration_secs: u64,
    },
    TookNotes {
        content: String,
        related_to: Vec<String>, // IDs of related activities
    },
    AiChat {
        provider: String, // "ChatGPT", "Claude", "Local"
        topic: String,
        message_count: usize,
        summary: String,
    },
    WebResearch {
        urls: Vec<String>,
        search_queries: Vec<String>,
        findings: String,
    },
    Coding {
        files_modified: Vec<String>,
        language: String,
        commit_message: Option<String>,
    },
    Writing {
        word_count: usize,
        section: Option<String>,
    },
    Thinking {
        notes: String,
        insights: Vec<String>,
    },
    Other {
        description: String,
        metadata: HashMap<String, String>,
    },
}

/// A snapshot of the user's state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub timestamp: DateTime<Utc>,
    /// What documents were open
    pub open_documents: Vec<OpenDocument>,
    /// Browser tabs
    pub browser_tabs: Vec<BrowserTab>,
    /// Notes taken so far
    pub notes: String,
    /// Current mental state (user-reported)
    pub mental_state: Option<MentalState>,
    /// What they were working on
    pub current_focus: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenDocument {
    pub title: String,
    pub path: String,
    pub page_number: usize,
    pub scroll_position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTab {
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
}

/// The user's mental state during the session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentalState {
    /// Energy level: 1-10
    pub energy: u8,
    /// Focus level: 1-10
    pub focus: u8,
    /// Mood: "energetic", "tired", "frustrated", "flow", "curious", etc.
    pub mood: String,
    /// How they're feeling about the work
    pub sentiment: String,
}

/// Context surrounding the session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    /// Project this session belongs to (if any)
    pub project: Option<String>,
    /// Related sessions
    pub related_sessions: Vec<String>,
    /// Prerequisites/knowledge needed
    pub prerequisites: Vec<String>,
    /// Environment (OS, apps open, etc.)
    pub environment: String,
}

/// Session manager - tracks and manages sessions
pub struct SessionManager {
    current_session: Arc<Mutex<Option<Session>>>,
    session_history: Arc<Mutex<Vec<Session>>>,
    storage_path: PathBuf,
}

impl SessionManager {
    pub fn new() -> Result<Self> {
        // Get app data directory
        let data_dir = dirs::data_local_dir()
            .ok_or_else(|| SessionError::Io("Cannot get data directory".to_string()))?;

        let storage_dir = data_dir.join("marlos").join("sessions");
        fs::create_dir_all(&storage_dir)
            .map_err(|e| SessionError::Io(format!("Cannot create sessions dir: {}", e)))?;

        let storage_path = storage_dir.join("sessions.json");

        Ok(Self {
            current_session: Arc::new(Mutex::new(None)),
            session_history: Arc::new(Mutex::new(Vec::new())),
            storage_path,
        })
    }

    /// Load sessions from disk
    pub fn load(&self) -> Result<()> {
        if self.storage_path.exists() {
            let content = fs::read_to_string(&self.storage_path)
                .map_err(|e| SessionError::Serialization(format!("Cannot read sessions: {}", e)))?;

            let sessions: Vec<Session> = serde_json::from_str(&content)
                .map_err(|e| SessionError::Serialization(format!("Cannot parse sessions: {}", e)))?;

            *self.session_history.lock().unwrap() = sessions;
        }

        Ok(())
    }

    /// Save sessions to disk
    pub fn save(&self) -> Result<()> {
        let sessions = self.session_history.lock().unwrap();
        let content = serde_json::to_string_pretty(&*sessions)
            .map_err(|e| SessionError::Serialization(format!("Cannot serialize sessions: {}", e)))?;

        fs::write(&self.storage_path, content)
            .map_err(|e| SessionError::Io(format!("Cannot write sessions: {}", e)))?;

        Ok(())
    }

    /// Start a new session with intent
    pub fn start_session(
        &self,
        title: String,
        intent: SessionIntent,
        description: Option<String>,
    ) -> Result<Session> {
        let session = Session {
            id: Uuid::new_v4().to_string(),
            title,
            description,
            intent,
            started_at: Utc::now(),
            ended_at: None,
            activities: Vec::new(),
            snapshots: Vec::new(),
            context: SessionContext {
                project: None,
                related_sessions: Vec::new(),
                prerequisites: Vec::new(),
                environment: Self::detect_environment(),
            },
            next_steps: Vec::new(),
            tags: Vec::new(),
        };

        // Store as current session
        *self.current_session.lock().unwrap() = Some(session.clone());

        // Take initial snapshot
        self.take_snapshot()?;

        // Save to disk
        self.save()?;

        Ok(session)
    }

    /// End the current session
    pub fn end_session(&self, next_steps: Vec<String>) -> Result<Option<Session>> {
        let mut current = self.current_session.lock().unwrap();
        if let Some(mut session) = current.take() {
            session.ended_at = Some(Utc::now());
            session.next_steps = next_steps;

            // Save to history
            self.session_history.lock().unwrap().push(session.clone());

            // Persist to disk
            self.save()?;

            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    /// Record an activity during the session
    pub fn record_activity(&self, activity: SessionActivity) -> Result<()> {
        let mut current = self.current_session.lock().unwrap();
        if let Some(ref mut session) = *current {
            session.activities.push(activity);
            // Auto-save after each activity
            drop(current);
            self.save()?;
            Ok(())
        } else {
            Err(SessionError::InvalidState("No active session".to_string()))
        }
    }

    /// Take a snapshot of current state
    pub fn take_snapshot(&self) -> Result<()> {
        let mut current = self.current_session.lock().unwrap();
        if let Some(ref mut session) = *current {
            let snapshot = SessionSnapshot {
                timestamp: Utc::now(),
                open_documents: Self::get_open_documents()?,
                browser_tabs: Self::get_browser_tabs()?,
                notes: session.activities.iter()
                    .filter_map(|a| match &a.details {
                        ActivityDetails::TookNotes { content, .. } => Some(content.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n"),
                mental_state: None, // User can set this
                current_focus: session.title.clone(),
            };
            session.snapshots.push(snapshot);
            Ok(())
        } else {
            Err(SessionError::InvalidState("No active session".to_string()))
        }
    }

    /// Get current session (if any)
    pub fn get_current_session(&self) -> Option<Session> {
        self.current_session.lock().unwrap().clone()
    }

    /// Get session history
    pub fn get_session_history(&self) -> Vec<Session> {
        self.session_history.lock().unwrap().clone()
    }

    /// Resume a previous session
    pub fn resume_session(&self, session_id: &str) -> Result<Session> {
        let history = self.session_history.lock().unwrap();
        let session = history.iter()
            .find(|s| s.id == session_id)
            .ok_or_else(|| SessionError::NotFound(format!("Session {} not found", session_id)))?;

        // Restore as current session
        let mut resumed = session.clone();
        resumed.started_at = Utc::now(); // New start time
        resumed.ended_at = None; // It's active again
        resumed.id = Uuid::new_v4().to_string(); // New ID
        resumed.title = format!("(Resuming) {}", session.title);

        *self.current_session.lock().unwrap() = Some(resumed.clone());

        // Restore state from last snapshot
        if let Some(last_snapshot) = session.snapshots.last() {
            self.restore_snapshot(last_snapshot)?;
        }

        Ok(resumed)
    }

    /// Search sessions by meaning
    pub fn search_sessions(&self, query: &str) -> Result<Vec<Session>> {
        let history = self.session_history.lock().unwrap();
        let query_lower = query.to_lowercase();

        let results: Vec<Session> = history.iter()
            .filter(|s| {
                s.title.to_lowercase().contains(&query_lower)
                    || s.description.as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                    || s.tags.iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
                    || s.activities.iter()
                        .any(|a| match &a.details {
                            ActivityDetails::TookNotes { content, .. } => {
                                content.to_lowercase().contains(&query_lower)
                            },
                            ActivityDetails::AiChat { summary, .. } => {
                                summary.to_lowercase().contains(&query_lower)
                            },
                            _ => false,
                        })
            })
            .cloned()
            .collect();

        Ok(results)
    }

    /// Add a completed session directly to history (for backfilling)
    pub fn add_session_to_history(&self, session: Session) -> Result<()> {
        self.session_history.lock().unwrap().push(session);
        // Auto-save after adding
        self.save()
    }

    /// Get summarized context in chunks, with hierarchical summarization
    pub fn get_summarized_context(&self, days: usize, max_summary_length: usize) -> Result<String> {
        let cutoff_date = Utc::now() - chrono::Duration::days(days as i64);
        let sessions = self.get_session_history();

        // Filter sessions into a Vec of references for easier chunking
        let recent: Vec<&Session> = sessions.iter()
            .filter(|s| s.started_at > cutoff_date)
            .collect();

        if recent.is_empty() {
            return Ok(format!("No sessions in the last {} days", days));
        }

        // Format sessions into text for LLM processing
        let sessions_text = Self::format_sessions_for_llm(&recent, days);

        // Create a compact summary (this will be further summarized by the LLM if needed)
        let mut combined = format!("Session Summary (last {} days, {} sessions):\n\n{}",
            days, recent.len(), sessions_text);

        // If still too long, create ultra-compact summary
        let combined_chars = combined.chars().count();
        if combined_chars > max_summary_length {
            combined = Self::format_ultra_compact(&recent, days, max_summary_length);
        }

        Ok(combined)
    }

    /// Format sessions into text for LLM summarization
    pub fn format_sessions_for_llm(sessions: &[&Session], days: usize) -> String {
        let mut text = String::new();

        for session in sessions {
            let date = session.started_at.format("%Y-%m-%d %H:%M");
            let project = session.context.project.as_ref()
                .map(|p| p.as_str())
                .unwrap_or("General");

            text.push_str(&format!("[{}] {} ({})\n", date, session.title, project));

            // Add intent info
            let intent_desc = match &session.intent {
                SessionIntent::Coding { project } => format!("Coding: {}", project),
                SessionIntent::Research { topic } => format!("Research: {}", topic),
                SessionIntent::Writing { project } => format!("Writing: {}", project),
                SessionIntent::Learning { subject } => format!("Learning: {}", subject),
                SessionIntent::Planning { goal } => format!("Planning: {}", goal),
                SessionIntent::Debugging { issue } => format!("Debugging: {}", issue),
                SessionIntent::Brainstorming { theme } => format!("Brainstorming: {}", theme),
                SessionIntent::Other { description } => description.clone(),
            };
            text.push_str(&format!("  Intent: {}\n", intent_desc));

            // Add activity summary
            if !session.activities.is_empty() {
                let activity_types: std::collections::HashSet<&str> = session.activities.iter()
                    .map(|a| match a.activity_type {
                        ActivityType::AiChat => "AI Chat",
                        ActivityType::Coding => "Coding",
                        ActivityType::DocumentView => "Document View",
                        ActivityType::NoteTaking => "Note Taking",
                        ActivityType::WebResearch => "Web Research",
                        ActivityType::Writing => "Writing",
                        ActivityType::Thinking => "Thinking",
                        ActivityType::Other => "Other",
                    })
                    .collect();
                text.push_str(&format!("  Activities: {}\n", activity_types.iter().copied().collect::<Vec<_>>().join(", ")));
            }

            text.push('\n');
        }

        text
    }

    /// Format a chunk of sessions into a text summary
    fn format_chunk_summary(chunk: &[&Session], chunk_index: usize) -> String {
        let mut summary = format!("Chunk {}:\n", chunk_index + 1);

        // Group by project
        let mut by_project: std::collections::HashMap<String, Vec<&Session>> = std::collections::HashMap::new();
        for session in chunk {
            let project = session.context.project.clone()
                .unwrap_or_else(|| "General".to_string());
            by_project.entry(project).or_default().push(*session);
        }

        for (project, sessions) in by_project {
            summary.push_str(&format!("\n{} ({} sessions):\n", project, sessions.len()));
            for session in sessions.iter().take(5) {
                let date = session.started_at.format("%m-%d");
                summary.push_str(&format!("  - {} [{}]\n",
                    session.title.split(':').next().unwrap_or(&session.title),
                    date
                ));
            }
            if sessions.len() > 5 {
                summary.push_str(&format!("  - ... and {} more\n", sessions.len() - 5));
            }
        }

        summary
    }

    /// Create ultra-compact summary when context is still too large
    fn format_ultra_compact(recent: &[&Session], days: usize, max_length: usize) -> String {
        let mut summary = format!("Last {} days: {} sessions\n", days, recent.len());

        // Count unique projects
        let projects: std::collections::HashSet<&str> = recent.iter()
            .filter_map(|s| s.context.project.as_ref().map(|p| p.as_str()))
            .collect();

        // Count providers
        let mut providers: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for session in recent {
            for activity in &session.activities {
                if let ActivityDetails::AiChat { provider, .. } = &activity.details {
                    *providers.entry(provider.clone()).or_insert(0) += 1;
                }
            }
        }

        summary.push_str(&format!("Projects: {}\n", projects.len()));
        summary.push_str(&format!("AI: {}\n",
            providers.iter()
                .map(|(p, c)| format!("{}: {}", p, c))
                .collect::<Vec<_>>()
                .join(", ")
        ));

        // Truncate to max length
        if summary.chars().count() > max_length * 4 {
            summary = summary.chars().take(max_length * 4).collect::<String>();
            summary.push_str("...");
        }

        summary
    }

    /// Detect current environment
    fn detect_environment() -> String {
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
    }

    /// Get currently open documents (placeholder)
    fn get_open_documents() -> Result<Vec<OpenDocument>> {
        // TODO: Implement actual document detection
        Ok(vec![])
    }

    /// Get browser tabs (placeholder)
    fn get_browser_tabs() -> Result<Vec<BrowserTab>> {
        // TODO: Implement actual browser tab detection
        Ok(vec![])
    }

    /// Restore a snapshot (open documents, browser tabs, etc.)
    fn restore_snapshot(&self, _snapshot: &SessionSnapshot) -> Result<()> {
        // TODO: Implement actual restoration
        // - Open documents at right pages
        // - Open browser tabs
        // - Restore notes to editor
        Ok(())
    }

    /// Get vector-based semantic analysis of sessions
    /// Returns themes and patterns without using LLM
    pub fn get_vector_analysis(&self, days: usize) -> Result<SessionAnalysis> {
        let cutoff_date = Utc::now() - chrono::Duration::days(days as i64);
        let sessions = self.get_session_history();

        let recent: Vec<&Session> = sessions.iter()
            .filter(|s| s.started_at > cutoff_date)
            .collect();

        if recent.is_empty() {
            return Ok(SessionAnalysis::empty(days));
        }

        // Extract themes using simple frequency analysis
        let mut projects: HashMap<String, usize> = HashMap::new();
        let mut intents: HashMap<String, usize> = HashMap::new();
        let mut providers: HashMap<String, usize> = HashMap::new();
        let mut topics: HashMap<String, usize> = HashMap::new();

        for session in &recent {
            // Count projects
            if let Some(ref project) = session.context.project {
                *projects.entry(project.clone()).or_insert(0) += 1;
            }

            // Count intents
            let intent_name = match &session.intent {
                SessionIntent::Coding { .. } => "coding",
                SessionIntent::Research { .. } => "research",
                SessionIntent::Writing { .. } => "writing",
                SessionIntent::Learning { .. } => "learning",
                SessionIntent::Planning { .. } => "planning",
                SessionIntent::Debugging { .. } => "debugging",
                SessionIntent::Brainstorming { .. } => "brainstorming",
                SessionIntent::Other { .. } => "other",
            };
            *intents.entry(intent_name.to_string()).or_insert(0) += 1;

            // Count AI providers from activities
            for activity in &session.activities {
                if let ActivityDetails::AiChat { provider, .. } = &activity.details {
                    *providers.entry(provider.clone()).or_insert(0) += 1;
                }
            }

            // Extract topics from title and description
            let text = format!("{} {}", session.title, session.description.as_ref().unwrap_or(&String::new()));
            for word in text.split_whitespace() {
                let word = word.to_lowercase();
                // Filter out common words
                if word.len() > 4 && !is_common_word(&word) {
                    *topics.entry(word).or_insert(0) += 1;
                }
            }
        }

        // Sort by frequency
        let mut top_projects: Vec<_> = projects.into_iter().collect();
        top_projects.sort_by(|a, b| b.1.cmp(&a.1));

        let mut top_intents: Vec<_> = intents.into_iter().collect();
        top_intents.sort_by(|a, b| b.1.cmp(&a.1));

        let mut top_providers: Vec<_> = providers.into_iter().collect();
        top_providers.sort_by(|a, b| b.1.cmp(&a.1));

        let mut top_topics: Vec<_> = topics.into_iter().collect();
        top_topics.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(SessionAnalysis {
            days,
            total_sessions: recent.len(),
            top_projects: top_projects.into_iter().take(10).map(|(p, c)| (p, c)).collect(),
            top_intents: top_intents.into_iter().map(|(i, c)| (i, c)).collect(),
            top_providers: top_providers.into_iter().map(|(p, c)| (p, c)).collect(),
            top_topics: top_topics.into_iter().take(20).map(|(t, c)| (t, c)).collect(),
        })
    }
}

/// Vector-based analysis result (no LLM needed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAnalysis {
    pub days: usize,
    pub total_sessions: usize,
    pub top_projects: Vec<(String, usize)>,
    pub top_intents: Vec<(String, usize)>,
    pub top_providers: Vec<(String, usize)>,
    pub top_topics: Vec<(String, usize)>,
}

impl SessionAnalysis {
    fn empty(days: usize) -> Self {
        Self {
            days,
            total_sessions: 0,
            top_projects: Vec::new(),
            top_intents: Vec::new(),
            top_providers: Vec::new(),
            top_topics: Vec::new(),
        }
    }

    /// Format analysis as human-readable text
    pub fn format(&self) -> String {
        if self.total_sessions == 0 {
            return format!("No sessions in the last {} days", self.days);
        }

        let mut text = format!("Session Analysis ({} days, {} sessions)\n\n", self.days, self.total_sessions);

        // Top projects
        if !self.top_projects.is_empty() {
            text.push_str("Top Projects:\n");
            for (project, count) in &self.top_projects {
                text.push_str(&format!("  - {} ({} sessions)\n", project, count));
            }
            text.push('\n');
        }

        // Activity types
        if !self.top_intents.is_empty() {
            text.push_str("Activity Types:\n");
            for (intent, count) in &self.top_intents {
                text.push_str(&format!("  - {} ({} sessions)\n", intent, count));
            }
            text.push('\n');
        }

        // AI providers
        if !self.top_providers.is_empty() {
            text.push_str("AI Providers Used:\n");
            for (provider, count) in &self.top_providers {
                text.push_str(&format!("  - {} ({} conversations)\n", provider, count));
            }
            text.push('\n');
        }

        // Key topics
        if !self.top_topics.is_empty() {
            text.push_str("Key Topics:\n");
            let topics_str = self.top_topics.iter()
                .take(10)
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            text.push_str(&format!("  {}\n", topics_str));
        }

        text
    }
}

/// Check if a word is too common to be meaningful
fn is_common_word(word: &str) -> bool {
    let common = [
        "this", "that", "with", "from", "they", "them", "their", "there",
        "where", "which", "about", "would", "could", "should", "being",
        "doing", "having", "first", "after", "before", "between", "through",
        "session", "work", "working", "coding", "marlos", "project", "just",
        "also", "been", "were", "will", "have", "more", "some", "time",
    ];
    common.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_lifecycle() {
        // Would test: start session, record activity, end session
    }

    #[test]
    fn test_session_search() {
        // Would test: search sessions by content
    }
}
