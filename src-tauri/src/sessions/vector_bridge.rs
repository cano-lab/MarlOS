//! Session Vector Bridge - Hybrid Q&A Chunking for Claude Code Sessions
//!
//! This module bridges session data (especially Claude Code conversations) into
//! the vector database by:
//! 1. Extracting Q&A pairs from activities
//! 2. Grouping related Q&As into topic sections
//! 3. Building adaptive context based on answer complexity
//!
//! Philosophy: Each Q&A pair or topic section becomes a searchable vector chunk.

use crate::sessions::{Session, SessionActivity, ActivityDetails, SessionIntent};
use serde::{Deserialize, Serialize};

/// A chunk of session content for vector embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionChunk {
    pub chunk_type: ChunkType,
    pub question: Option<String>,
    pub answer: String,
    pub context: SessionContext,
    pub metadata: ChunkMetadata,
}

/// Type of chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkType {
    #[serde(rename = "qa_pair")]
    QAPair,
    #[serde(rename = "topic_section")]
    TopicSection,
}

impl ChunkType {
    pub fn to_string(&self) -> String {
        match self {
            ChunkType::QAPair => "qa-pair".to_string(),
            ChunkType::TopicSection => "topic-section".to_string(),
        }
    }
}

/// Context information for the chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub session_id: String,
    pub session_title: String,
    pub project: Option<String>,
    pub intent: String,
    pub started_at: String,
}

/// Metadata about the chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub complexity: usize,
    pub has_code: bool,
    pub has_file_ops: bool,
    pub tags: Vec<String>,
}

impl SessionChunk {
    /// Convert chunk to text for embedding
    pub fn to_text(&self) -> String {
        let mut text = String::new();

        // Add Q&A format if applicable
        if let Some(ref question) = self.question {
            text.push_str(&format!("Q: {}\n\n", question));
        }

        text.push_str(&format!("{}\n", self.answer));

        // Add context
        text.push_str(&format!(
            "\n---\nContext: Session \"{}\" ({})",
            self.context.session_title,
            self.context.intent
        ));

        if let Some(ref project) = self.context.project {
            text.push_str(&format!(", Project: {}", project));
        }

        text
    }

    /// Generate title for the chunk
    pub fn title(&self) -> String {
        if let Some(ref question) = self.question {
            // Truncate question to first 50 chars
            let preview = if question.len() > 50 {
                format!("{}...", &question[..50])
            } else {
                question.clone()
            };
            format!("Q: {}", preview)
        } else {
            // Use session title + type
            format!("{} ({})", self.context.session_title, self.chunk_type.to_string())
        }
    }
}

/// A single Q&A pair extracted from activities
#[derive(Debug, Clone)]
struct QAPair {
    question: String,
    answer: String,
    has_code: bool,
    has_file_ops: bool,
    timestamp: String,
}

/// Chunk Claude Code sessions into hybrid Q&A + topic sections
pub fn chunk_session_for_vectors(session: &Session) -> Vec<SessionChunk> {
    // Step 1: Extract all Q&A pairs
    let qa_pairs = extract_qa_pairs(session);

    if qa_pairs.is_empty() {
        // No Q&A pairs found - create a single chunk from session metadata
        return vec![create_session_summary_chunk(session)];
    }

    // Step 2: Group by topic using semantic similarity (simplified - use adjacency)
    let topic_groups = group_by_topic(&qa_pairs);

    // Step 3: For each group, create chunks
    let mut chunks = Vec::new();

    for group in topic_groups {
        if group.len() == 1 {
            // Single Q&A → individual chunk
            chunks.push(create_qa_chunk(&group[0], session));
        } else if group.len() <= 3 {
            // Small group → individual chunks (flexible search)
            for qa in &group {
                chunks.push(create_qa_chunk(qa, session));
            }
        } else {
            // Large group → create both individual + topic section
            for qa in &group {
                chunks.push(create_qa_chunk(qa, session));
            }
            chunks.push(create_topic_section(&group, session));
        }
    }

    chunks
}

/// Extract Q&A pairs from session activities
fn extract_qa_pairs(session: &Session) -> Vec<QAPair> {
    let mut qa_pairs = Vec::new();

    for activity in &session.activities {
        if let ActivityDetails::AiChat {
            ref provider,
            ref summary,
            message_count,
            ..
        } = activity.details {
            // Only process AI chat activities
            if message_count < 2 {
                continue; // Need at least a question and answer
            }

            // Try to infer Q&A from summary or topic
            let topic = if !summary.is_empty() {
                summary.clone()
            } else {
                format!("{} conversation", provider)
            };

            // For now, use topic as both question and answer
            // In a real implementation, you'd parse the actual conversation
            qa_pairs.push(QAPair {
                question: format!("What was discussed in: {}?", topic),
                answer: summary.clone(),
                has_code: topic.to_lowercase().contains("code") ||
                         topic.to_lowercase().contains("function") ||
                         topic.to_lowercase().contains("implement"),
                has_file_ops: false, // Would be detected from actual content
                timestamp: activity.timestamp.to_rfc3339(),
            });
        }
    }

    qa_pairs
}

/// Group Q&A pairs by topic (simplified adjacency-based grouping)
fn group_by_topic(qa_pairs: &[QAPair]) -> Vec<Vec<QAPair>> {
    if qa_pairs.is_empty() {
        return Vec::new();
    }

    // Simple approach: group adjacent Q&As that seem related
    // A more sophisticated approach would use semantic similarity
    let mut groups = Vec::new();
    let mut current_group = vec![qa_pairs[0].clone()];

    for i in 1..qa_pairs.len() {
        let prev = &current_group[current_group.len() - 1];
        let curr = &qa_pairs[i];

        // Check if related (simple keyword overlap)
        if are_related(&prev.question, &curr.question) {
            current_group.push(curr.clone());
        } else {
            // Start new group
            groups.push(current_group);
            current_group = vec![curr.clone()];
        }
    }

    groups.push(current_group);
    groups
}

/// Check if two questions are related (simple keyword-based)
fn are_related(q1: &str, q2: &str) -> bool {
    let q1_lower = q1.to_lowercase();
    let q2_lower = q2.to_lowercase();

    // Extract keywords (words longer than 4 chars)
    let keywords1: Vec<&str> = q1_lower
        .split_whitespace()
        .filter(|w| w.len() > 4)
        .collect();

    let keywords2: Vec<&str> = q2_lower
        .split_whitespace()
        .filter(|w| w.len() > 4)
        .collect();

    // Check for keyword overlap
    for kw1 in &keywords1 {
        for kw2 in &keywords2 {
            if kw1 == kw2 || kw1.contains(kw2) || kw2.contains(kw1) {
                return true;
            }
        }
    }

    false
}

/// Create a Q&A chunk
fn create_qa_chunk(qa: &QAPair, session: &Session) -> SessionChunk {
    let context = build_adaptive_context(qa, session);

    SessionChunk {
        chunk_type: ChunkType::QAPair,
        question: Some(qa.question.clone()),
        answer: qa.answer.clone(),
        context,
        metadata: ChunkMetadata {
            complexity: qa.answer.len(),
            has_code: qa.has_code,
            has_file_ops: qa.has_file_ops,
            tags: extract_tags(session, qa.has_code),
        },
    }
}

/// Create a topic section from multiple Q&A pairs
fn create_topic_section(group: &[QAPair], session: &Session) -> SessionChunk {
    // Combine all Q&As into a topic section
    let combined_answer = group.iter()
        .map(|qa| format!("Q: {}\nA: {}", qa.question, qa.answer))
        .collect::<Vec<_>>()
        .join("\n\n");

    let has_code = group.iter().any(|qa| qa.has_code);
    let has_file_ops = group.iter().any(|qa| qa.has_file_ops);
    let complexity = combined_answer.len();

    // Infer topic from first question
    let topic = extract_topic(&group[0].question);

    let context = SessionContext {
        session_id: session.id.clone(),
        session_title: session.title.clone(),
        project: session.context.project.clone(),
        intent: format_intent(&session.intent),
        started_at: session.started_at.to_rfc3339(),
    };

    SessionChunk {
        chunk_type: ChunkType::TopicSection,
        question: Some(topic),
        answer: combined_answer,
        context,
        metadata: ChunkMetadata {
            complexity,
            has_code,
            has_file_ops,
            tags: extract_tags(session, has_code),
        },
    }
}

/// Build adaptive context based on answer complexity
fn build_adaptive_context(qa: &QAPair, session: &Session) -> SessionContext {
    SessionContext {
        session_id: session.id.clone(),
        session_title: session.title.clone(),
        project: session.context.project.clone(),
        intent: format_intent(&session.intent),
        started_at: session.started_at.to_rfc3339(),
    }
}

/// Create a session summary chunk when no Q&A pairs are found
fn create_session_summary_chunk(session: &Session) -> SessionChunk {
    let summary = format!(
        "Session: {}\nIntent: {}\nDescription: {}\nActivities: {}",
        session.title,
        format_intent(&session.intent),
        session.description.as_deref().unwrap_or("No description"),
        session.activities.len()
    );

    let complexity = summary.len();

    SessionChunk {
        chunk_type: ChunkType::TopicSection,
        question: Some(format!("What happened in: {}?", session.title)),
        answer: summary,
        context: SessionContext {
            session_id: session.id.clone(),
            session_title: session.title.clone(),
            project: session.context.project.clone(),
            intent: format_intent(&session.intent),
            started_at: session.started_at.to_rfc3339(),
        },
        metadata: ChunkMetadata {
            complexity,
            has_code: false,
            has_file_ops: false,
            tags: session.tags.clone(),
        },
    }
}

/// Extract tags from session
fn extract_tags(session: &Session, has_code: bool) -> Vec<String> {
    let mut tags = session.tags.clone();

    if has_code {
        tags.push("code".to_string());
    }

    // Add intent-based tags
    match session.intent {
        SessionIntent::Coding { .. } => tags.push("coding".to_string()),
        SessionIntent::Research { .. } => tags.push("research".to_string()),
        SessionIntent::Writing { .. } => tags.push("writing".to_string()),
        SessionIntent::Learning { .. } => tags.push("learning".to_string()),
        SessionIntent::Debugging { .. } => tags.push("debugging".to_string()),
        _ => {}
    }

    tags
}

/// Format intent as string
fn format_intent(intent: &SessionIntent) -> String {
    match intent {
        SessionIntent::Research { topic } => format!("Research: {}", topic),
        SessionIntent::Writing { project } => format!("Writing: {}", project),
        SessionIntent::Coding { project } => format!("Coding: {}", project),
        SessionIntent::Learning { subject } => format!("Learning: {}", subject),
        SessionIntent::Planning { goal } => format!("Planning: {}", goal),
        SessionIntent::Debugging { issue } => format!("Debugging: {}", issue),
        SessionIntent::Brainstorming { theme } => format!("Brainstorming: {}", theme),
        SessionIntent::Other { description } => description.clone(),
    }
}

/// Extract topic from question
fn extract_topic(question: &str) -> String {
    // Remove common question words and extract topic
    let cleaned = question
        .replace("What was discussed in:", "")
        .replace("What is", "")
        .replace("How to", "")
        .replace("?", "")
        .trim()
        .to_string();

    if cleaned.len() > 60 {
        format!("{}...", &cleaned[..60])
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::sessions::{SessionIntent, SessionContext as BaseSessionContext, ActivityDetails, ActivityType, SessionActivity};

    #[test]
    fn test_chunk_empty_session() {
        let session = create_test_session();
        let chunks = chunk_session_for_vectors(&session);

        // Should have at least a summary chunk
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunk_to_text() {
        let chunk = SessionChunk {
            chunk_type: ChunkType::QAPair,
            question: Some("How do I fix this error?".to_string()),
            answer: "Update the import on line 5.".to_string(),
            context: SessionContext {
                session_id: "test-id".to_string(),
                session_title: "Debug Session".to_string(),
                project: Some("marlos-rust".to_string()),
                intent: "Debugging: fix error".to_string(),
                started_at: Utc::now().to_rfc3339(),
            },
            metadata: ChunkMetadata {
                complexity: 25,
                has_code: false,
                has_file_ops: false,
                tags: vec!["debugging".to_string()],
            },
        };

        let text = chunk.to_text();
        assert!(text.contains("Q: How do I fix this error?"));
        assert!(text.contains("Update the import on line 5."));
        assert!(text.contains("Debug Session"));
    }

    #[test]
    fn test_chunk_title() {
        let chunk = SessionChunk {
            chunk_type: ChunkType::QAPair,
            question: Some("How do I implement a search feature?".to_string()),
            answer: "Use binary search for sorted arrays.".to_string(),
            context: SessionContext {
                session_id: "test-id".to_string(),
                session_title: "Coding Session".to_string(),
                project: None,
                intent: "Coding: search".to_string(),
                started_at: Utc::now().to_rfc3339(),
            },
            metadata: ChunkMetadata {
                complexity: 35,
                has_code: true,
                has_file_ops: false,
                tags: vec!["coding".to_string()],
            },
        };

        let title = chunk.title();
        assert!(title.contains("Q: How do I implement"));
    }

    fn create_test_session() -> Session {
        Session {
            id: "test-session-id".to_string(),
            title: "Test Session".to_string(),
            description: Some("A test session".to_string()),
            intent: SessionIntent::Coding { project: "test-project".to_string() },
            started_at: Utc::now(),
            ended_at: None,
            activities: Vec::new(),
            snapshots: Vec::new(),
            context: BaseSessionContext {
                project: Some("test-project".to_string()),
                related_sessions: Vec::new(),
                prerequisites: Vec::new(),
                environment: "test".to_string(),
            },
            next_steps: Vec::new(),
            tags: vec!["test".to_string()],
        }
    }
}
