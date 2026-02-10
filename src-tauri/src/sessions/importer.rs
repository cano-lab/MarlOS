//! Import existing conversations as Sessions
//!
//! This module bridges the gap between imported conversations (stored as SemanticObjects)
//! and the Session system.

use crate::semantic_object::{SemanticObject, Suid};
use crate::sessions::{SessionManager, Session, SessionIntent, SessionActivity, ActivityDetails, ActivityType};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Convert imported AI conversations to Sessions
pub async fn import_conversations_as_sessions(
    manager: &SessionManager,
    conversations: Vec<SemanticObject>,
) -> Result<usize, String> {
    let mut imported = 0;
    let mut skipped = 0;

    log::info!("Starting import of {} objects...", conversations.len());

    for (idx, conv) in conversations.iter().enumerate() {
        // Try to parse as conversation
        let metadata = match extract_conversation_metadata(conv) {
            Ok(m) => m,
            Err(e) => {
                skipped += 1;
                // Log every 100th skip
                if skipped % 100 == 0 {
                    log::debug!("Skipped {} objects so far (last error: {})", skipped, e);
                }
                continue;
            }
        };

        // Create a session from this conversation
        let session = create_session_from_conversation(conv.clone(), metadata)?;

        // Add to history
        manager.add_session_to_history(session)
            .map_err(|e| format!("Failed to add session: {}", e))?;
        imported += 1;

        // Log progress every 50 imports
        if imported % 50 == 0 {
            log::info!("Imported {} sessions so far...", imported);
        }
    }

    log::info!("Import complete: {} imported, {} skipped", imported, skipped);

    // Save all sessions
    manager.save()
        .map_err(|e| format!("Failed to save sessions: {}", e))?;

    Ok(imported)
}

/// Extract metadata from a conversation SemanticObject
fn extract_conversation_metadata(conv: &SemanticObject) -> Result<ConversationMetadata, String> {
    // Convert content bytes to string
    let content = conv.content_as_str()
        .unwrap_or("");

    // Check if this is marked as a conversation by tags - be very lenient
    let has_conversation_tag = conv.tags.iter()
        .any(|t| {
            let t_lower = t.to_lowercase();
            t_lower.contains("conversation")
                || t_lower.contains("claude")
                || t_lower.contains("chatgpt")
                || t_lower.contains("cursor")
                || t_lower.contains("codex")
                || t_lower.contains("copilot")
                || t_lower.contains("ai")
                || t_lower.contains("chat")
                || t_lower.contains("browser")
        });

    // Try to detect if this is a conversation by checking for conversation markers
    if !has_conversation_tag && !looks_like_conversation(content) {
        return Err("Not a conversation".to_string());
    }

    // Try to extract provider from tags or content
    let provider = detect_provider_from_content(conv, content);

    // Extract title from object name or content
    let title = conv.name.clone().unwrap_or_else(|| {
        // Try to find first line or use a default
        content.lines()
            .next()
            .unwrap_or("Imported Conversation")
            .to_string()
            .chars().take(50)
            .collect()
    });

    Ok(ConversationMetadata {
        title,
        provider,
        message_count: count_messages(content),
        date: conv.created_at,
    })
}

/// Check if content looks like a conversation
fn looks_like_conversation(content: &str) -> bool {
    let lower = content.to_lowercase();
    // Look for common conversation patterns
    lower.contains("user:")
        || lower.contains("assistant:")
        || lower.contains("system:")
        || lower.contains("claude:")
        || lower.contains("chatgpt")
        || lower.contains("human:")
        || lower.contains("you:")
}

/// Detect which AI provider from conversation content or tags
fn detect_provider_from_content(conv: &SemanticObject, content: &str) -> String {
    // Check tags first
    for tag in &conv.tags {
        let tag_lower = tag.to_lowercase();
        if tag_lower.contains("claude-code") || tag_lower.contains("claude") {
            return "Claude".to_string();
        } else if tag_lower.contains("chatgpt") {
            return "ChatGPT".to_string();
        } else if tag_lower.contains("cursor") {
            return "Cursor".to_string();
        } else if tag_lower.contains("codex") || tag_lower.contains("copilot") {
            return "Codex".to_string();
        }
    }

    // Fall back to content detection
    let lower = content.to_lowercase();
    if lower.contains("claude") || lower.contains("anthropic") {
        "Claude".to_string()
    } else if lower.contains("chatgpt") || lower.contains("gpt") {
        "ChatGPT".to_string()
    } else if lower.contains("codex") || lower.contains("copilot") {
        "Codex".to_string()
    } else if lower.contains("cursor") {
        "Cursor".to_string()
    } else {
        "Unknown".to_string()
    }
}

/// Count messages in conversation content
fn count_messages(content: &str) -> usize {
    content.matches("user:").count()
        + content.matches("assistant:").count()
        + content.matches("system:").count()
        + content.matches("human:").count()
        + content.matches("claude:").count()
}

/// Create a Session from a conversation SemanticObject
fn create_session_from_conversation(
    conv: SemanticObject,
    metadata: ConversationMetadata,
) -> Result<Session, String> {
    use crate::sessions::{SessionContext};

    let content = conv.content_as_str().unwrap_or("");

    // Determine intent based on provider
    let intent = match metadata.provider.as_str() {
        "Claude" | "ChatGPT" | "Codex" | "Cursor" => SessionIntent::Coding {
            project: "Imported".to_string()
        },
        _ => SessionIntent::Other {
            description: "Imported conversation".to_string()
        },
    };

    // Create AI chat activity
    let activity = SessionActivity {
        id: uuid::Uuid::new_v4().to_string(),
        activity_type: ActivityType::AiChat,
        timestamp: metadata.date,
        duration: None,
        details: ActivityDetails::AiChat {
            provider: metadata.provider.clone(),
            topic: metadata.title.clone(),
            message_count: metadata.message_count,
            summary: content.chars().take(200).collect(),
        },
    };

    Ok(Session {
        id: conv.suid.to_string(),
        title: metadata.title,
        description: Some(format!("Imported from {}", metadata.provider)),
        intent,
        started_at: metadata.date,
        ended_at: Some(metadata.date), // Assume ended immediately for imports
        activities: vec![activity],
        snapshots: vec![],
        context: SessionContext {
            project: None,
            related_sessions: vec![],
            prerequisites: vec![],
            environment: "Imported".to_string(),
        },
        next_steps: vec![],
        tags: vec!["imported".to_string(), metadata.provider.to_lowercase()],
    })
}

struct ConversationMetadata {
    title: String,
    provider: String,
    message_count: usize,
    date: DateTime<Utc>,
}

/// Convert a Claude Code session file to a MarlOS Session
pub fn claude_session_to_marlos_session(
    claude_session: crate::providers::ParsedSession,
    session_path: &std::path::Path,
) -> Result<Session, String> {
    use crate::sessions::SessionContext;

    // Get project name from path or session
    let project_name = claude_session.project_path.clone()
        .unwrap_or_else(|| {
            session_path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown Project")
                .to_string()
        });

    // Use the session's timestamp or file modification time
    let started_at = claude_session.started_at.unwrap_or_else(|| Utc::now());
    let ended_at = claude_session.ended_at;

    // Create AI chat activity from the session
    let activity = SessionActivity {
        id: uuid::Uuid::new_v4().to_string(),
        activity_type: ActivityType::AiChat,
        timestamp: started_at,
        duration: None,
        details: ActivityDetails::AiChat {
            provider: "Claude".to_string(),
            topic: format!("Session in {}", project_name),
            message_count: claude_session.message_count,
            summary: format!("{} user messages, {} assistant messages",
                claude_session.user_messages.len(),
                claude_session.assistant_messages.len()),
        },
    };

    // Add coding activity if there were tool calls
    let mut activities = vec![activity];
    if !claude_session.tool_calls.is_empty() {
        activities.push(SessionActivity {
            id: uuid::Uuid::new_v4().to_string(),
            activity_type: ActivityType::Coding,
            timestamp: started_at,
            duration: None,
            details: ActivityDetails::Coding {
                files_modified: claude_session.tool_calls,
                language: "Various".to_string(),
                commit_message: Some(format!("Session: {}", project_name)),
            },
        });
    }

    Ok(Session {
        id: uuid::Uuid::new_v4().to_string(),
        title: format!("Claude Code: {}", project_name),
        description: Some(format!("Session with {} messages", claude_session.message_count)),
        intent: SessionIntent::Coding {
            project: project_name.clone(),
        },
        started_at,
        ended_at,
        activities,
        snapshots: vec![],
        context: SessionContext {
            project: Some(project_name),
            related_sessions: vec![],
            prerequisites: vec![],
            environment: format!("Claude Code Session: {:?}", session_path),
        },
        next_steps: vec![],
        tags: vec!["claude-code".to_string(), "imported".to_string()],
    })
}

/// Convert an Andor Hub session to a MarlOS Session
pub fn andor_session_to_marlos_session(
    andor_session: crate::andor_client::Session,
) -> Result<Session, String> {
    use crate::sessions::SessionContext;

    // Parse timestamps
    let started_at = andor_session.created_at
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now());

    let ended_at = andor_session.completed_at
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    // Get project name
    let project_name = andor_session.project.clone()
        .or_else(|| andor_session.repo_path.clone())
        .unwrap_or_else(|| "Unknown Project".to_string());

    // Create AI chat activity
    let activity = SessionActivity {
        id: uuid::Uuid::new_v4().to_string(),
        activity_type: ActivityType::AiChat,
        timestamp: started_at,
        duration: None,
        details: ActivityDetails::AiChat {
            provider: "Andor".to_string(),
            topic: andor_session.summary.clone().unwrap_or_else(|| format!("Session in {}", project_name)),
            message_count: andor_session.message_count.unwrap_or(0) as usize,
            summary: andor_session.summary.clone().unwrap_or_default(),
        },
    };

    // Build tags
    let mut tags = vec!["andor".to_string(), "synced".to_string()];
    if let Some(session_type) = &andor_session.session_type {
        tags.push(session_type.to_lowercase());
    }

    Ok(Session {
        id: andor_session.session_id.clone(),
        title: andor_session.summary.clone().unwrap_or_else(|| format!("Session: {}", project_name)),
        description: andor_session.project.clone(),
        intent: SessionIntent::Coding {
            project: project_name.clone(),
        },
        started_at,
        ended_at,
        activities: vec![activity],
        snapshots: vec![],
        context: SessionContext {
            project: Some(project_name),
            related_sessions: vec![],
            prerequisites: vec![],
            environment: andor_session.repo_path.unwrap_or_else(|| "Andor Hub".to_string()),
        },
        next_steps: vec![],
        tags,
    })
}

/// Convert a ChatGPT SemanticObject to a Session
pub fn chatgpt_object_to_session(
    obj: crate::semantic_object::SemanticObject,
) -> Result<Session, String> {
    use crate::sessions::SessionContext;

    let content = obj.content_as_str().unwrap_or("");

    // Extract title from name or content
    let title = obj.name.clone().unwrap_or_else(|| {
        content.lines()
            .next()
            .unwrap_or("ChatGPT Conversation")
            .chars().take(50)
            .collect()
    });

    // Count messages
    let message_count = content.matches("user:").count()
        + content.matches("assistant:").count()
        + content.matches("system:").count();

    // Detect model from content
    let model = if content.contains("gpt-4") {
        "GPT-4"
    } else if content.contains("gpt-3.5") || content.contains("gpt-35") {
        "GPT-3.5"
    } else {
        "ChatGPT"
    };

    // Create AI chat activity
    let activity = SessionActivity {
        id: uuid::Uuid::new_v4().to_string(),
        activity_type: ActivityType::AiChat,
        timestamp: obj.created_at,
        duration: None,
        details: ActivityDetails::AiChat {
            provider: "ChatGPT".to_string(),
            topic: title.clone(),
            message_count,
            summary: content.chars().take(200).collect(),
        },
    };

    Ok(Session {
        id: obj.suid.to_string(),
        title,
        description: obj.summary,
        intent: SessionIntent::Other {
            description: format!("ChatGPT conversation ({})", model),
        },
        started_at: obj.created_at,
        ended_at: Some(obj.modified_at),
        activities: vec![activity],
        snapshots: vec![],
        context: SessionContext {
            project: None,
            related_sessions: vec![],
            prerequisites: vec![],
            environment: format!("ChatGPT ({})", model),
        },
        next_steps: vec![],
        tags: vec!["chatgpt".to_string(), model.to_lowercase(), "imported".to_string()],
    })
}
