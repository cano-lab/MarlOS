//! LLM Client - Abstraction for calling local and cloud LLMs
//!
//! Supports:
//! - Ollama (local) - default
//! - LM Studio (local)
//! - OpenAI API (cloud) - future
//! - Anthropic API (cloud) - future

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum LlmProvider {
    /// Ollama running locally (default port 11434)
    Ollama { base_url: String, model: String },
    /// LM Studio running locally (default port 4321)
    LmStudio { base_url: String },
    /// OpenAI-compatible API
    OpenAi { base_url: String, api_key: String, model: String },
    /// Anthropic Claude API
    Anthropic { api_key: String, model: String },
}

impl Default for LlmProvider {
    fn default() -> Self {
        LlmProvider::Ollama {
            base_url: "http://localhost:11434".to_string(),
            model: "llama3.2".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub model: String,
    pub response_time_ms: u64,
    pub tokens_used: Option<u64>,
}

#[derive(Debug)]
pub enum LlmError {
    NetworkError(String),
    ParseError(String),
    ApiError(String),
    Timeout,
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::NetworkError(e) => write!(f, "Network error: {}", e),
            LlmError::ParseError(e) => write!(f, "Parse error: {}", e),
            LlmError::ApiError(e) => write!(f, "API error: {}", e),
            LlmError::Timeout => write!(f, "Request timed out"),
        }
    }
}

/// Ollama API request format
#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: i32,
}

/// Ollama API response format
#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
    model: String,
    #[serde(default)]
    eval_count: Option<u64>,
}

/// OpenAI-compatible API request
#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    temperature: f32,
    max_tokens: i32,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

/// OpenAI-compatible API response
#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    model: String,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessageResponse,
}

#[derive(Deserialize)]
struct OpenAiMessageResponse {
    content: String,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    total_tokens: u64,
}

/// Anthropic API request
#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: i32,
    messages: Vec<AnthropicMessage>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

/// Anthropic API response
#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    model: String,
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

pub struct LlmClient {
    provider: LlmProvider,
    timeout: Duration,
}

impl LlmClient {
    pub fn new(provider: LlmProvider) -> Self {
        Self {
            provider,
            timeout: Duration::from_secs(300), // 5 minute timeout for large local models
        }
    }

    pub fn ollama(model: &str) -> Self {
        Self::new(LlmProvider::Ollama {
            base_url: "http://localhost:11434".to_string(),
            model: model.to_string(),
        })
    }

    pub fn lm_studio() -> Self {
        Self::new(LlmProvider::LmStudio {
            base_url: "http://localhost:4321".to_string(),
        })
    }

    pub fn anthropic(api_key: &str, model: &str) -> Self {
        Self::new(LlmProvider::Anthropic {
            api_key: api_key.to_string(),
            model: model.to_string(),
        })
    }

    pub fn claude_sonnet(api_key: &str) -> Self {
        Self::anthropic(api_key, "claude-sonnet-4-20250514")
    }

    pub fn claude_haiku(api_key: &str) -> Self {
        Self::anthropic(api_key, "claude-haiku-4-20250514")
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Send a prompt to the LLM and get a response
    pub fn complete(&self, prompt: &str) -> Result<LlmResponse, LlmError> {
        let start = Instant::now();

        let result = match &self.provider {
            LlmProvider::Ollama { base_url, model } => {
                self.call_ollama(base_url, model, prompt)
            }
            LlmProvider::LmStudio { base_url } => {
                self.call_openai_compatible(base_url, "", "local-model", prompt)
            }
            LlmProvider::OpenAi { base_url, api_key, model } => {
                self.call_openai_compatible(base_url, api_key, model, prompt)
            }
            LlmProvider::Anthropic { api_key, model } => {
                self.call_anthropic(api_key, model, prompt)
            }
        };

        result.map(|mut r| {
            r.response_time_ms = start.elapsed().as_millis() as u64;
            r
        })
    }

    fn call_ollama(&self, base_url: &str, model: &str, prompt: &str) -> Result<LlmResponse, LlmError> {
        let url = format!("{}/api/generate", base_url);

        let request = OllamaRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            stream: false,
            options: OllamaOptions {
                temperature: 0.1, // Low temperature for deterministic responses
                num_predict: 1024,
            },
        };

        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| LlmError::NetworkError(e.to_string()))?;

        let response = client
            .post(&url)
            .json(&request)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else if e.is_connect() {
                    LlmError::NetworkError(format!("Cannot connect to Ollama at {}. Is it running?", base_url))
                } else {
                    LlmError::NetworkError(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(LlmError::ApiError(format!("HTTP {}", response.status())));
        }

        let ollama_response: OllamaResponse = response
            .json()
            .map_err(|e| LlmError::ParseError(e.to_string()))?;

        Ok(LlmResponse {
            content: ollama_response.response,
            model: ollama_response.model,
            response_time_ms: 0, // Will be filled in by caller
            tokens_used: ollama_response.eval_count,
        })
    }

    fn call_openai_compatible(
        &self,
        base_url: &str,
        api_key: &str,
        model: &str,
        prompt: &str,
    ) -> Result<LlmResponse, LlmError> {
        let url = format!("{}/v1/chat/completions", base_url);

        let request = OpenAiRequest {
            model: model.to_string(),
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature: 0.1,
            max_tokens: 1024,
        };

        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| LlmError::NetworkError(e.to_string()))?;

        let mut req = client.post(&url).json(&request);

        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req.send().map_err(|e| {
            if e.is_timeout() {
                LlmError::Timeout
            } else if e.is_connect() {
                LlmError::NetworkError(format!("Cannot connect to {}. Is the server running?", base_url))
            } else {
                LlmError::NetworkError(e.to_string())
            }
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let openai_response: OpenAiResponse = response
            .json()
            .map_err(|e| LlmError::ParseError(e.to_string()))?;

        let content = openai_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(LlmResponse {
            content,
            model: openai_response.model,
            response_time_ms: 0,
            tokens_used: openai_response.usage.map(|u| u.total_tokens),
        })
    }

    fn call_anthropic(
        &self,
        api_key: &str,
        model: &str,
        prompt: &str,
    ) -> Result<LlmResponse, LlmError> {
        let url = "https://api.anthropic.com/v1/messages";

        let request = AnthropicRequest {
            model: model.to_string(),
            max_tokens: 1024,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| LlmError::NetworkError(e.to_string()))?;

        let response = client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::NetworkError(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(LlmError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        let anthropic_response: AnthropicResponse = response
            .json()
            .map_err(|e| LlmError::ParseError(e.to_string()))?;

        let content = anthropic_response
            .content
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        let tokens = anthropic_response
            .usage
            .map(|u| u.input_tokens + u.output_tokens);

        Ok(LlmResponse {
            content,
            model: anthropic_response.model,
            response_time_ms: 0,
            tokens_used: tokens,
        })
    }

    /// List available models (Ollama only)
    pub fn list_models(&self) -> Result<Vec<String>, LlmError> {
        match &self.provider {
            LlmProvider::Ollama { base_url, .. } => {
                let url = format!("{}/api/tags", base_url);

                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .map_err(|e| LlmError::NetworkError(e.to_string()))?;

                let response = client.get(&url).send().map_err(|e| {
                    if e.is_connect() {
                        LlmError::NetworkError("Cannot connect to Ollama. Is it running?".to_string())
                    } else {
                        LlmError::NetworkError(e.to_string())
                    }
                })?;

                #[derive(Deserialize)]
                struct TagsResponse {
                    models: Vec<ModelInfo>,
                }

                #[derive(Deserialize)]
                struct ModelInfo {
                    name: String,
                }

                let tags: TagsResponse = response
                    .json()
                    .map_err(|e| LlmError::ParseError(e.to_string()))?;

                Ok(tags.models.into_iter().map(|m| m.name).collect())
            }
            _ => Err(LlmError::ApiError("Model listing only supported for Ollama".to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = LlmClient::ollama("llama3.2");
        assert!(matches!(client.provider, LlmProvider::Ollama { .. }));
    }
}
