//! ChatGPT Importer - Import conversations from OpenAI ChatGPT exports
//!
//! ChatGPT exports are JSON files with the following structure:
//! - Each conversation has a "mapping" field with message nodes
//! - Messages are linked by parent/child relationships
//! - Content can be text, images, or code blocks

use std::collections::HashMap;
use std::path::Path;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::memory::SecurityTier;
use crate::semantic_object::{ContentType, SemanticObject};
use super::{Importer, ImportResult};

/// ChatGPT conversation export format
#[derive(Debug, Deserialize)]
pub struct ChatGptExport(Vec<ChatGptConversation>);

#[derive(Debug, Deserialize)]
pub struct ChatGptConversation {
    pub id: String,
    pub title: Option<String>,
    pub create_time: Option<f64>,
    pub update_time: Option<f64>,
    pub mapping: HashMap<String, MessageNode>,
    pub current_node: Option<String>,
    pub conversation_template_id: Option<String>,
    pub gizmo_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessageNode {
    pub id: String,
    pub message: Option<ChatGptMessage>,
    pub parent: Option<String>,
    pub children: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatGptMessage {
    pub id: String,
    pub author: Author,
    pub create_time: Option<f64>,
    pub content: MessageContent,
    pub status: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct Author {
    pub role: String,
    pub name: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct MessageContent {
    pub content_type: String,
    pub parts: Option<Vec<serde_json::Value>>,
    pub text: Option<String>,
}

impl MessageContent {
    /// Extract text content from message
    pub fn get_text(&self) -> String {
        // First try parts array
        if let Some(parts) = &self.parts {
            let texts: Vec<String> = parts.iter()
                .filter_map(|p| {
                    // Parts can be strings or objects
                    if let Some(s) = p.as_str() {
                        Some(s.to_string())
                    } else if let Some(obj) = p.as_object() {
                        // Some parts are objects with text field
                        obj.get("text")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            if !texts.is_empty() {
                return texts.join("\n");
            }
        }

        // Fall back to text field
        self.text.clone().unwrap_or_default()
    }
}

/// Parsed ChatGPT message for internal use
#[derive(Debug, Clone)]
pub struct ParsedChatGptMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<DateTime<Utc>>,
}

/// Parsed ChatGPT conversation
#[derive(Debug, Clone)]
pub struct ParsedChatGptConversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<ParsedChatGptMessage>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub model: Option<String>,
}

impl ParsedChatGptConversation {
    /// Generate full conversation text
    pub fn get_full_text(&self) -> String {
        let mut text = String::new();
        for msg in &self.messages {
            text.push_str(&format!("[{}]: {}\n\n", msg.role, msg.content));
        }
        text
    }

    /// Generate a summary
    pub fn generate_summary(&self) -> String {
        let mut summary = format!("ChatGPT: {}\n", self.title);
        summary.push_str(&format!("Messages: {}\n", self.messages.len()));

        // First user message as preview
        if let Some(first_user) = self.messages.iter().find(|m| m.role == "user") {
            let preview: String = first_user.content.chars().take(100).collect();
            summary.push_str(&format!("Topic: {}...\n", preview));
        }

        summary
    }
}

pub struct ChatGptImporter {
    /// Whether to chunk long conversations
    pub chunk_conversations: bool,
    /// Maximum characters per chunk
    pub max_chunk_size: usize,
}

impl ChatGptImporter {
    pub fn new() -> Self {
        Self {
            chunk_conversations: true,
            max_chunk_size: 8000,
        }
    }

    /// Parse a ChatGPT export file
    pub fn parse_export(&self, path: &Path) -> Result<Vec<ParsedChatGptConversation>, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read ChatGPT export: {}", e))?;

        let export: Vec<ChatGptConversation> = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse ChatGPT JSON: {}", e))?;

        let mut conversations = Vec::new();

        for conv in export {
            if let Some(parsed) = self.parse_conversation(&conv) {
                conversations.push(parsed);
            }
        }

        Ok(conversations)
    }

    fn parse_conversation(&self, conv: &ChatGptConversation) -> Option<ParsedChatGptConversation> {
        // Build message order by following the tree
        let mut messages = Vec::new();
        let mut visited = std::collections::HashSet::new();

        // Find root (node with no parent or parent not in mapping)
        let root_id = conv.mapping.iter()
            .find(|(_, node)| {
                node.parent.as_ref().map_or(true, |p| !conv.mapping.contains_key(p))
            })
            .map(|(id, _)| id.clone())?;

        self.collect_messages_dfs(&conv.mapping, &root_id, &mut messages, &mut visited);

        if messages.is_empty() {
            return None;
        }

        // Convert timestamps
        let created_at = conv.create_time.map(|t| {
            DateTime::from_timestamp(t as i64, 0)
                .unwrap_or_else(|| Utc::now())
        });
        let updated_at = conv.update_time.map(|t| {
            DateTime::from_timestamp(t as i64, 0)
                .unwrap_or_else(|| Utc::now())
        });

        // Extract model from first assistant message metadata
        let model = conv.mapping.values()
            .filter_map(|n| n.message.as_ref())
            .filter(|m| m.author.role == "assistant")
            .filter_map(|m| m.metadata.as_ref())
            .filter_map(|meta| meta.get("model_slug"))
            .filter_map(|m| m.as_str())
            .next()
            .map(|s| s.to_string());

        Some(ParsedChatGptConversation {
            id: conv.id.clone(),
            title: conv.title.clone().unwrap_or_else(|| "Untitled".to_string()),
            messages,
            created_at,
            updated_at,
            model,
        })
    }

    fn collect_messages_dfs(
        &self,
        mapping: &HashMap<String, MessageNode>,
        node_id: &str,
        messages: &mut Vec<ParsedChatGptMessage>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if visited.contains(node_id) {
            return;
        }
        visited.insert(node_id.to_string());

        if let Some(node) = mapping.get(node_id) {
            // Add this node's message if it has content
            if let Some(msg) = &node.message {
                let content = msg.content.get_text();
                if !content.trim().is_empty() && msg.author.role != "system" {
                    let timestamp = msg.create_time.map(|t| {
                        DateTime::from_timestamp(t as i64, 0)
                            .unwrap_or_else(|| Utc::now())
                    });

                    messages.push(ParsedChatGptMessage {
                        role: msg.author.role.clone(),
                        content,
                        timestamp,
                    });
                }
            }

            // Visit children (follow first child for linear path)
            for child_id in &node.children {
                self.collect_messages_dfs(mapping, child_id, messages, visited);
            }
        }
    }

    /// Convert a parsed conversation to SemanticObjects
    pub fn conversation_to_objects(&self, conv: &ParsedChatGptConversation) -> Vec<SemanticObject> {
        let mut objects = Vec::new();

        let full_text = conv.get_full_text();
        let summary = conv.generate_summary();

        // If chunking is enabled and conversation is long, split it
        if self.chunk_conversations && full_text.len() > self.max_chunk_size {
            objects.extend(self.chunk_conversation(conv));
        } else {
            // Single object for the whole conversation
            let mut obj = SemanticObject::new(
                full_text.as_bytes().to_vec(),
                ContentType::Structured { schema: "chatgpt-conversation".to_string() },
            );

            obj.name = Some(format!("ChatGPT: {}", conv.title));
            obj.summary = Some(summary);
            obj.security_tier = SecurityTier::Guarded;
            obj.metadata.insert("source".to_string(), serde_json::json!("chatgpt"));
            obj.metadata.insert("conversation_id".to_string(), serde_json::json!(conv.id));
            obj.metadata.insert("message_count".to_string(), serde_json::json!(conv.messages.len()));
            if let Some(model) = &conv.model {
                obj.metadata.insert("model".to_string(), serde_json::json!(model));
            }
            obj.tags.push("chatgpt".to_string());
            obj.tags.push("conversation".to_string());
            obj.tags.push("ai".to_string());

            if let Some(created) = conv.created_at {
                obj.created_at = created;
            }
            if let Some(updated) = conv.updated_at {
                obj.modified_at = updated;
            }

            objects.push(obj);
        }

        objects
    }

    fn chunk_conversation(&self, conv: &ParsedChatGptConversation) -> Vec<SemanticObject> {
        let mut objects = Vec::new();
        let mut current_chunk = String::new();
        let mut chunk_messages = Vec::new();
        let mut chunk_index = 0;

        for msg in &conv.messages {
            let msg_text = format!("[{}]: {}\n\n", msg.role, msg.content);

            if current_chunk.len() + msg_text.len() > self.max_chunk_size && !current_chunk.is_empty() {
                // Save current chunk
                objects.push(self.create_chunk_object(
                    conv,
                    chunk_index,
                    &current_chunk,
                    &chunk_messages,
                ));
                chunk_index += 1;
                current_chunk.clear();
                chunk_messages.clear();
            }

            current_chunk.push_str(&msg_text);
            chunk_messages.push(msg.clone());
        }

        // Don't forget the last chunk
        if !current_chunk.is_empty() {
            objects.push(self.create_chunk_object(
                conv,
                chunk_index,
                &current_chunk,
                &chunk_messages,
            ));
        }

        objects
    }

    fn create_chunk_object(
        &self,
        conv: &ParsedChatGptConversation,
        chunk_index: usize,
        content: &str,
        messages: &[ParsedChatGptMessage],
    ) -> SemanticObject {
        let mut obj = SemanticObject::new(
            content.as_bytes().to_vec(),
            ContentType::Structured { schema: "chatgpt-chunk".to_string() },
        );

        // Generate chunk summary
        let first_user = messages.iter()
            .find(|m| m.role == "user")
            .map(|m| m.content.chars().take(50).collect::<String>())
            .unwrap_or_default();

        obj.name = Some(format!("ChatGPT: {} (chunk {})", conv.title, chunk_index));
        obj.summary = Some(format!("Chunk {} of {}: {}...", chunk_index, conv.title, first_user));
        obj.security_tier = SecurityTier::Guarded;
        obj.metadata.insert("source".to_string(), serde_json::json!("chatgpt"));
        obj.metadata.insert("conversation_id".to_string(), serde_json::json!(conv.id));
        obj.metadata.insert("chunk_index".to_string(), serde_json::json!(chunk_index));
        obj.metadata.insert("message_count".to_string(), serde_json::json!(messages.len()));
        obj.tags.push("chatgpt".to_string());
        obj.tags.push("conversation".to_string());
        obj.tags.push("chunk".to_string());

        // Use timestamp from first message in chunk
        if let Some(first_msg) = messages.first() {
            if let Some(ts) = first_msg.timestamp {
                obj.created_at = ts;
                obj.modified_at = ts;
            }
        }

        obj
    }
}

impl Default for ChatGptImporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Importer for ChatGptImporter {
    fn source_name(&self) -> &'static str {
        "ChatGPT"
    }

    fn can_import(&self, path: &Path) -> bool {
        if !path.is_file() || path.extension().map_or(true, |e| e != "json") {
            return false;
        }

        // Check if it looks like a ChatGPT export
        if let Ok(content) = std::fs::read_to_string(path) {
            // ChatGPT exports have "mapping" and are arrays of conversations
            content.contains("\"mapping\"") && content.starts_with('[')
        } else {
            false
        }
    }

    fn import(&self, path: &Path, _embed: bool) -> Result<ImportResult, String> {
        let mut result = ImportResult::new("ChatGPT");

        let conversations = self.parse_export(path)?;
        result.items_imported = conversations.len();

        for conv in conversations {
            let objects = self.conversation_to_objects(&conv);
            result.objects_created += objects.len();
            result.objects.extend(objects);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_content_extraction() {
        let content = MessageContent {
            content_type: "text".to_string(),
            parts: Some(vec![serde_json::json!("Hello, world!")]),
            text: None,
        };
        assert_eq!(content.get_text(), "Hello, world!");

        let content2 = MessageContent {
            content_type: "text".to_string(),
            parts: None,
            text: Some("Fallback text".to_string()),
        };
        assert_eq!(content2.get_text(), "Fallback text");
    }

    #[test]
    fn test_importer_detection() {
        let importer = ChatGptImporter::new();
        assert_eq!(importer.source_name(), "ChatGPT");
    }
}
