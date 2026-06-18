/**
 * Fine-Tuning Data Export Module
 *
 * Exports conversation data from MarlOS SemanticObjects for use in fine-tuning.
 * Supports filtering by provider, date range, and content type.
 */

use crate::semantic_object::{SemanticObject, ContentType};
use crate::semantic_search::SemanticSearch;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::Arc;
use tauri::State;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExportFilters {
    pub content_types: Option<Vec<String>>,
    pub security_tiers: Option<Vec<String>>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub min_message_count: Option<usize>,
    pub limit: Option<usize>,
}

impl Default for ExportFilters {
    fn default() -> Self {
        Self {
            content_types: None,
            security_tiers: Some(vec!["open".to_string()]), // Only export open tier by default
            start_date: None,
            end_date: None,
            min_message_count: Some(1),
            limit: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExportStats {
    pub objects_processed: usize,
    pub conversations_exported: usize,
    pub total_messages: usize,
    pub output_path: String,
    pub export_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatConversation {
    pub messages: Vec<ChatMessage>,
    pub metadata: ConversationMetadata,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationMetadata {
    pub suid: String,
    pub name: Option<String>,
    pub content_type: String,
    pub provider: Option<String>,
    pub created_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

/**
 * Extract conversations from a SemanticObject for fine-tuning
 */
fn extract_conversation(obj: &SemanticObject) -> Option<ChatConversation> {
    let content = obj.content.as_ref()?;
    let text_content = String::from_utf8(content.clone()).ok()?;

    // Try to parse as structured conversation data
    // This handles various formats from different providers

    let metadata = ConversationMetadata {
        suid: obj.suid.to_string(),
        name: obj.name.clone(),
        content_type: format!("{:?}", obj.content_type),
        provider: obj.tags.iter()
            .find(|t| t.starts_with("provider:") || *t == "chatgpt" || *t == "claude" || *t == "cursor")
            .cloned(),
        created_at: obj.created_at,
        tags: obj.tags.clone(),
    };

    // Parse messages based on content type
    let messages = parse_messages_from_content(&text_content, &obj.tags);

    if messages.is_empty() {
        return None;
    }

    Some(ChatConversation {
        messages,
        metadata,
    })
}

/**
 * Parse messages from content text based on format
 */
fn parse_messages_from_content(content: &str, _tags: &[String]) -> Vec<ChatMessage> {
    // Check for structured JSON format
    if let Ok(json_data) = serde_json::from_str::<serde_json::Value>(content) {
        return parse_structured_messages(json_data);
    }

    // Parse from text format (common in Claude exports)
    // Format: "user: message\nassistant: response"
    parse_text_messages(content)
}

/**
 * Parse messages from structured JSON
 */
fn parse_structured_messages(json: serde_json::Value) -> Vec<ChatMessage> {
    let mut messages = Vec::new();

    // Handle ChatGPT format
    if let Some(mapping) = json.get("mapping") {
        if let Some(mapping_obj) = mapping.as_object() {
            for (_key, value) in mapping_obj {
                if let Some(msg) = value.get("message") {
                    if let Some(role) = msg.get("role").and_then(|r| r.as_str()) {
                        if let Some(content) = extract_content_from_parts(msg.get("content")) {
                            messages.push(ChatMessage {
                                role: role.to_string(),
                                content,
                            });
                        }
                    }
                }
            }
        }
    }

    // Handle array format
    if let Some(arr) = json.as_array() {
        for item in arr {
            if let (Some(role), Some(content)) = (
                item.get("role").and_then(|r| r.as_str()),
                item.get("content").and_then(|c| c.as_str())
            ) {
                messages.push(ChatMessage {
                    role: role.to_string(),
                    content: content.to_string(),
                });
            }
        }
    }

    messages
}

/**
 * Extract text content from message parts
 */
fn extract_content_from_parts(content: Option<&serde_json::Value>) -> Option<String> {
    let content = content?;

    // Handle parts array
    if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
        let mut text = String::new();
        for part in parts {
            if let Some(s) = part.as_str() {
                text.push_str(s);
            }
        }
        if !text.is_empty() {
            return Some(text);
        }
    }

    // Handle direct text
    content.as_str().map(|s| s.to_string())
}

/**
 * Parse messages from plain text format
 */
fn parse_text_messages(content: &str) -> Vec<ChatMessage> {
    let mut messages = Vec::new();
    let mut current_role: Option<String> = None;
    let mut current_content = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Check for role markers
        if trimmed.starts_with("user:") || trimmed.starts_with("User:") {
            if let Some(role) = current_role.take() {
                if !current_content.trim().is_empty() {
                    messages.push(ChatMessage {
                        role,
                        content: current_content.trim().to_string(),
                    });
                }
            }
            current_role = Some("user".to_string());
            let after_colon = trimmed.trim_start_matches("user:").trim_start_matches("User:");
            current_content = after_colon.to_string();
        } else if trimmed.starts_with("assistant:") || trimmed.starts_with("Assistant:") || trimmed.starts_with("claude:") {
            if let Some(role) = current_role.take() {
                if !current_content.trim().is_empty() {
                    messages.push(ChatMessage {
                        role,
                        content: current_content.trim().to_string(),
                    });
                }
            }
            current_role = Some("assistant".to_string());
            let after_colon = trimmed.trim_start_matches("assistant:")
                .trim_start_matches("Assistant:")
                .trim_start_matches("claude:");
            current_content = after_colon.to_string();
        } else if current_role.is_some() {
            current_content.push('\n');
            current_content.push_str(line);
        }
    }

    // Don't forget the last message
    if let Some(role) = current_role {
        if !current_content.trim().is_empty() {
            messages.push(ChatMessage {
                role,
                content: current_content.trim().to_string(),
            });
        }
    }

    messages
}

/**
 * Export conversations to JSONL format for fine-tuning
 */
#[tauri::command]
pub async fn export_conversations_for_finetuning(
    filters: Option<ExportFilters>,
    output_path: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<ExportStats, String> {
    let filters = filters.unwrap_or_default();
    let start_time = Utc::now();

    // Ensure output directory exists
    if let Some(parent) = Path::new(&output_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    // Get object store via SemanticSearch
    let store = search.store.read().await;

    // Query objects
    let objects = store.list(filters.limit.unwrap_or(10000), 0)
        .map_err(|e| format!("Failed to query objects: {}", e))?;

    let objects_processed = objects.len();
    let mut conversations_exported = 0;
    let mut total_messages = 0;

    // Open output file
    let file = File::create(&output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    let mut writer = BufWriter::new(file);

    for obj in objects {
        // Skip if min_message_count is set and we can't extract enough messages
        if let Some(conv) = extract_conversation(&obj) {
            if let Some(min_count) = filters.min_message_count {
                if conv.messages.len() < min_count {
                    continue;
                }
            }

            // Write as JSONL line
            let json_line = serde_json::to_string(&conv)
                .map_err(|e| format!("Failed to serialize conversation: {}", e))?;
            use std::io::Write;
            writeln!(writer, "{}", json_line)
                .map_err(|e| format!("Failed to write to output file: {}", e))?;

            conversations_exported += 1;
            total_messages += conv.messages.len();
        }
    }

    use std::io::Write;
    writer.flush()
        .map_err(|e| format!("Failed to flush output file: {}", e))?;

    Ok(ExportStats {
        objects_processed,
        conversations_exported,
        total_messages,
        output_path,
        export_time: start_time,
    })
}

/**
 * Get available export providers and counts
 */
#[tauri::command]
pub async fn get_export_stats(
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<ExportProviderStats, String> {
    let store = search.store.read().await;

    let objects = store.list(10000, 0)
        .map_err(|e| format!("Failed to query objects: {}", e))?;

    let mut stats = ExportProviderStats::default();

    for obj in objects {
        // Count by provider
        let provider = obj.tags.iter()
            .find(|t| t.starts_with("provider:") || *t == "chatgpt" || *t == "claude" || *t == "cursor")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let provider_name = if provider.starts_with("provider:") {
            provider.trim_start_matches("provider:").to_string()
        } else {
            provider
        };

        *stats.by_provider.entry(provider_name).or_insert(0) += 1;
        stats.total_objects += 1;

        // Count messages
        if let Some(conv) = extract_conversation(&obj) {
            stats.total_messages += conv.messages.len();
            stats.conversations += 1;
        }
    }

    Ok(stats)
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ExportProviderStats {
    pub total_objects: usize,
    pub total_messages: usize,
    pub conversations: usize,
    pub by_provider: std::collections::HashMap<String, usize>,
}
