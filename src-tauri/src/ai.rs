//! AI Provider integration for LLM chat
//!
//! Supports OpenAI-compatible APIs (LM Studio, Ollama, OpenAI, etc.)

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AiError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Provider not configured")]
    NotConfigured,
    #[error("Provider not available: {0}")]
    NotAvailable(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Lock error")]
    Lock,
}

pub type Result<T> = std::result::Result<T, AiError>;

/// Chat message role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// A chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// AI provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider name for display
    pub name: String,
    /// API base URL (e.g., "http://localhost:4321/v1")
    pub base_url: String,
    /// API key (optional for local providers)
    pub api_key: Option<String>,
    /// Model name (optional, some providers auto-select)
    pub model: Option<String>,
    /// Temperature for generation
    pub temperature: f32,
    /// Max tokens to generate
    pub max_tokens: u32,
    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: "LM Studio".to_string(),
            base_url: "http://localhost:4321/v1".to_string(),
            api_key: None,
            model: None,
            temperature: 0.7,
            max_tokens: 4096,
            timeout_secs: 300,
        }
    }
}

/// Response from AI provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub content: String,
    pub model: String,
    pub tokens_used: Option<u32>,
    pub finish_reason: Option<String>,
}

/// OpenAI-compatible API response structures
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    total_tokens: Option<u32>,
}

/// Predefined system prompts for different tasks
pub struct SystemPrompts;

impl SystemPrompts {
    pub fn default_chat() -> &'static str {
        "You are an UNSTUCK assistant. Your job is to help the user clarify their thinking \
         and get unstuck on ideas, problems, and decisions.\n\n\
         Focus on:\n\
         - Identifying the core question or confusion\n\
         - Breaking down complex problems into clear parts\n\
         - Suggesting next steps when stuck\n\
         - Asking clarifying questions to reveal hidden assumptions\n\
         - Providing multiple perspectives without deciding for the user\n\n\
         Be direct and actionable. Help them think, don't just give answers."
    }

    pub fn intent_map() -> &'static str {
        "You are a document analyst. Analyze the given text and extract:\n\
         1. Main intent or purpose\n\
         2. Key themes and topics\n\
         3. Structure overview\n\
         4. Open questions or TODOs\n\
         Be concise and use bullet points."
    }

    pub fn explain() -> &'static str {
        "You are an expert explainer. Explain the given content clearly and concisely. \
         Break down complex concepts. Use examples when helpful."
    }

    pub fn summarize() -> &'static str {
        "You are a summarization expert. Provide a clear, concise summary of the given content. \
         Highlight the most important points. Keep it brief but complete."
    }

    pub fn brainstorm() -> &'static str {
        "You are a creative thinking partner. Help brainstorm ideas related to the topic. \
         Suggest diverse perspectives, potential approaches, and thought-provoking questions. \
         Be creative and expansive."
    }

    pub fn critique() -> &'static str {
        "You are a constructive critic. Analyze the content for:\n\
         1. Logical gaps or weaknesses\n\
         2. Missing evidence or citations needed\n\
         3. Unclear or ambiguous points\n\
         4. Suggestions for improvement\n\
         Be specific and actionable."
    }

    pub fn action_items() -> &'static str {
        "You are a task extractor. Identify all actionable items, TODOs, and next steps \
         from the content. Format as a checklist. Include any implicit actions that should be taken."
    }
}

/// AI Provider manager
pub struct AiManager {
    config: Mutex<ProviderConfig>,
    client: reqwest::Client,
}

impl AiManager {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            config: Mutex::new(ProviderConfig::default()),
            client,
        }
    }

    /// Update provider configuration
    pub fn set_config(&self, config: ProviderConfig) -> Result<()> {
        let mut cfg = self.config.lock().map_err(|_| AiError::Lock)?;
        *cfg = config;
        Ok(())
    }

    /// Get current configuration
    pub fn get_config(&self) -> Result<ProviderConfig> {
        let cfg = self.config.lock().map_err(|_| AiError::Lock)?;
        Ok(cfg.clone())
    }

    /// Check if provider is available (async)
    pub async fn is_available(&self) -> bool {
        let config = match self.config.lock() {
            Ok(c) => c.clone(),
            Err(_) => return false,
        };

        let url = format!("{}/models", config.base_url);
        let mut request = self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5));

        // Include API key if configured
        if let Some(api_key) = &config.api_key {
            if !api_key.is_empty() {
                request = request.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        match request.send().await {
            Ok(r) => {
                let status = r.status();
                log::info!("AI availability check: {} -> {}", url, status);
                // Accept 200 OK or 404 (endpoint not found but server reachable)
                status.is_success() || status.as_u16() == 404
            }
            Err(e) => {
                log::warn!("AI availability check failed: {}", e);
                false
            }
        }
    }

    /// List all available models from the provider
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let config = self.config.lock().map_err(|_| AiError::Lock)?.clone();
        let url = format!("{}/models", config.base_url);
        let mut request = self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(10));

        if let Some(api_key) = &config.api_key {
            if !api_key.is_empty() {
                request = request.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        let response = request.send().await
            .map_err(|e| AiError::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AiError::Api(format!("Models endpoint returned {}", response.status())));
        }

        let body: serde_json::Value = response.json().await
            .map_err(|e| AiError::Http(format!("Failed to parse models response: {}", e)))?;

        let models = body.get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| AiError::Api("No models data in response".into()))?;

        Ok(models.iter()
            .filter_map(|m| {
                let id = m.get("id").and_then(|v| v.as_str())?;
                // Check metadata type field
                if let Some(model_type) = m.get("type").and_then(|v| v.as_str()) {
                    if model_type == "embedding" || model_type == "text-embedding" {
                        return None;
                    }
                }
                // Filter by name pattern
                if Self::is_embedding_model(id) {
                    return None;
                }
                Some(id.to_string())
            })
            .collect())
    }

    /// Check if a model ID looks like an embedding model
    fn is_embedding_model(id: &str) -> bool {
        let id_lower = id.to_lowercase();
        // Common embedding model name patterns
        id_lower.contains("embed")
            || id_lower.contains("e5-")
            || id_lower.contains("bge-")
            || id_lower.contains("minilm")
            || id_lower.contains("gte-")
            || id_lower.contains("snowflake")
            || id_lower.contains("mxbai")
            || id_lower.contains("jina")
            || id_lower.contains("sentence-transform")
            || id_lower.contains("text-embedding")
            || id_lower.contains("nomic-embed")
            || id_lower.contains("nomic-ai")
            // LM Studio sometimes uses type prefixes
            || id_lower.starts_with("embedding")
    }

    /// Query available models from the provider and pick the best chat model.
    /// Prefers non-embedding models; uses broad pattern matching to filter.
    async fn detect_chat_model(&self, config: &ProviderConfig) -> Option<String> {
        let url = format!("{}/models", config.base_url);
        let mut request = self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(10));

        if let Some(api_key) = &config.api_key {
            if !api_key.is_empty() {
                request = request.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        let response = match request.send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return None,
        };

        let body: serde_json::Value = match response.json().await {
            Ok(v) => v,
            Err(_) => return None,
        };

        let models = body.get("data")?.as_array()?;

        // Collect model IDs, preferring chat/instruct models over embedding models
        let mut chat_models: Vec<String> = Vec::new();
        let mut all_models: Vec<String> = Vec::new();

        for model in models {
            if let Some(id) = model.get("id").and_then(|v| v.as_str()) {
                all_models.push(id.to_string());

                // Check metadata type field (LM Studio provides this)
                if let Some(model_type) = model.get("type").and_then(|v| v.as_str()) {
                    if model_type == "embedding" || model_type == "text-embedding" {
                        log::info!("Skipping embedding model (by type): {}", id);
                        continue;
                    }
                }

                // Skip by name pattern
                if Self::is_embedding_model(id) {
                    log::info!("Skipping embedding model (by name): {}", id);
                    continue;
                }
                chat_models.push(id.to_string());
            }
        }

        log::info!("Model detection: {} total, {} chat candidates: {:?}",
            all_models.len(), chat_models.len(), chat_models);

        // Only return chat models — do NOT fall back to embedding models
        if chat_models.is_empty() {
            log::warn!("No chat models found! All {} models appear to be embedding models: {:?}",
                all_models.len(), all_models);
            return None;
        }
        let selected = chat_models.first().cloned();
        if let Some(ref model) = selected {
            log::info!("Auto-detected chat model: {}", model);
        }
        selected
    }

    /// Send a chat completion request (async)
    pub async fn chat(&self, messages: Vec<Message>, system_prompt: Option<&str>) -> Result<AiResponse> {
        let config = self.config.lock().map_err(|_| AiError::Lock)?.clone();

        let url = format!("{}/chat/completions", config.base_url);
        log::info!("AI request to: {}", url);

        // Auto-detect model if not specified
        let model = match &config.model {
            Some(m) if !m.is_empty() => Some(m.clone()),
            _ => self.detect_chat_model(&config).await,
        };

        if model.is_none() {
            return Err(AiError::NotAvailable(
                "No chat model found. Only embedding models are loaded in LM Studio. \
                 Please load a chat/instruct model (e.g. Qwen, Llama, Mistral) in LM Studio.".to_string()
            ));
        }

        // Build messages with optional system prompt
        let mut all_messages: Vec<serde_json::Value> = Vec::new();

        if let Some(prompt) = system_prompt {
            all_messages.push(serde_json::json!({
                "role": "system",
                "content": prompt
            }));
        }

        for msg in &messages {
            all_messages.push(serde_json::json!({
                "role": match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                "content": msg.content
            }));
        }

        // Kimi K2 quirks (kimi-k2.x on the /coding endpoint):
        //  * In its default reasoning mode it burns the entire token budget
        //    on hidden reasoning and often returns empty `content`, and it
        //    rejects any temperature != 1.
        //  * Sending `thinking: {type: "disabled"}` switches it to a direct,
        //    non-reasoning answer (fast — seconds vs minutes) but then
        //    requires temperature == 0.6 (temperature is validated per-mode).
        // For app tasks (CSS generation, code ops, summaries) we want the
        // direct answer, so disable thinking and pin temp to 0.6. Only when
        // the model name says Kimi — other providers are untouched.
        let is_kimi = model
            .as_deref()
            .map(|m| m.to_lowercase().contains("kimi"))
            .unwrap_or(false);
        let effective_temp = if is_kimi { 0.6 } else { config.temperature };

        let mut payload = serde_json::json!({
            "messages": all_messages,
            "temperature": effective_temp,
            "max_tokens": config.max_tokens,
            "stream": false
        });

        if is_kimi {
            payload["thinking"] = serde_json::json!({ "type": "disabled" });
        }

        if let Some(model) = &model {
            payload["model"] = serde_json::json!(model);
        }

        let mut request = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .json(&payload);

        if let Some(api_key) = &config.api_key {
            if !api_key.is_empty() {
                request = request.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        let response = request
            .send()
            .await
            .map_err(|e| {
                log::error!("AI request failed: {}", e);
                AiError::Http(e.to_string())
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            log::error!("AI API error {}: {}", status, text);
            return Err(AiError::Api(format!("{}: {}", status, text)));
        }

        let result: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| AiError::Http(format!("Failed to parse response: {}", e)))?;

        let choice = result.choices.first()
            .ok_or_else(|| AiError::Api("No response choices".to_string()))?;

        let content = choice.message.content.clone().unwrap_or_default();

        Ok(AiResponse {
            content,
            model: result.model.unwrap_or_default(),
            tokens_used: result.usage.and_then(|u| u.total_tokens),
            finish_reason: choice.finish_reason.clone(),
        })
    }

    /// Simple generate with just a prompt (async)
    pub async fn generate(&self, prompt: &str, system_prompt: Option<&str>) -> Result<AiResponse> {
        let messages = vec![Message {
            role: Role::User,
            content: prompt.to_string(),
        }];
        self.chat(messages, system_prompt).await
    }

    /// Run a predefined task on content (async)
    pub async fn run_task(&self, task: &str, content: &str) -> Result<AiResponse> {
        let system_prompt = match task {
            "intent_map" => SystemPrompts::intent_map(),
            "explain" => SystemPrompts::explain(),
            "summarize" => SystemPrompts::summarize(),
            "brainstorm" => SystemPrompts::brainstorm(),
            "critique" => SystemPrompts::critique(),
            "action_items" => SystemPrompts::action_items(),
            _ => SystemPrompts::default_chat(),
        };

        self.generate(content, Some(system_prompt)).await
    }
}

impl Default for AiManager {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for AiManager {}
unsafe impl Sync for AiManager {}
