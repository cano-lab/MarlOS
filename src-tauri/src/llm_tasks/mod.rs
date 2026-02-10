//! Unified LLM Task System
//!
//! This module provides a template-based approach for integrating LLM capabilities
//! across the entire application. Instead of scattering AI prompts throughout the
//! codebase, all AI operations are defined as tasks with consistent interfaces.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐     ┌──────────────┐     ┌─────────────┐
//! │   LlmTask       │────▶│  TaskRunner  │────▶│  AiManager  │
//! │  (defines what) │     │ (executes)   │     │ (LLM calls) │
//! └─────────────────┘     └──────────────┘     └─────────────┘
//! ```
//!
//! # Creating a New AI Feature
//!
//! 1. Define your task struct with input data
//! 2. Implement `LlmTask` trait
//! 3. Use `TaskRunner::execute()` to run it
//!
//! # Example
//!
//! ```rust
//! use crate::llm_tasks::{LlmTask, TaskRunner, TaskContext};
//!
//! struct MyCustomTask {
//!     input: String,
//! }
//!
//! impl LlmTask for MyCustomTask {
//!     type Output = MyOutput;
//!
//!     fn name(&self) -> &'static str { "my_custom_task" }
//!     fn system_prompt(&self) -> String { "You are a helpful assistant.".into() }
//!     fn user_prompt(&self, _ctx: &TaskContext) -> String { self.input.clone() }
//!     fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
//!         // Parse the response
//!     }
//! }
//!
//! // Usage:
//! let task = MyCustomTask { input: "Hello".into() };
//! let result = TaskRunner::new(&ai_manager).execute(task).await?;
//! ```

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;

use crate::ai::AiManager;

// Re-export task implementations
pub mod tasks;
pub mod paper_tasks;
pub mod thinking_debugger;
pub use tasks::*;
pub use paper_tasks::*;
pub use thinking_debugger::*;

// ============================================================================
// Core Traits
// ============================================================================

/// Context passed to tasks during prompt generation
#[derive(Debug, Clone, Default)]
pub struct TaskContext {
    /// Additional context data (key-value pairs)
    pub data: HashMap<String, String>,
    /// Maximum tokens for response (hint to task)
    pub max_tokens: Option<usize>,
    /// Temperature override
    pub temperature: Option<f32>,
}

impl TaskContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_data(mut self, key: &str, value: &str) -> Self {
        self.data.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_max_tokens(mut self, tokens: usize) -> Self {
        self.max_tokens = Some(tokens);
        self
    }
}

/// The core trait for all LLM-powered tasks
///
/// Implement this trait to create a new AI feature. The trait defines:
/// - What system prompt to use
/// - How to build the user prompt from task data
/// - How to parse the LLM response into structured output
#[async_trait]
pub trait LlmTask: Send + Sync {
    /// The output type produced by this task
    type Output: Send;

    /// Unique name for this task (used for logging/metrics)
    fn name(&self) -> &'static str;

    /// The system prompt that sets the AI's behavior
    fn system_prompt(&self) -> String;

    /// Build the user prompt from task data and context
    fn user_prompt(&self, ctx: &TaskContext) -> String;

    /// Parse the raw LLM response into structured output
    fn parse_response(&self, response: &str) -> Result<Self::Output, String>;

    /// Optional: Validate inputs before running
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }

    /// Optional: Post-process the output
    fn post_process(&self, output: Self::Output) -> Result<Self::Output, String> {
        Ok(output)
    }
}

// ============================================================================
// Task Runner
// ============================================================================

/// Executes LLM tasks with consistent error handling and logging
pub struct TaskRunner<'a> {
    ai_manager: &'a AiManager,
    context: TaskContext,
}

impl<'a> TaskRunner<'a> {
    pub fn new(ai_manager: &'a AiManager) -> Self {
        Self {
            ai_manager,
            context: TaskContext::default(),
        }
    }

    pub fn with_context(mut self, context: TaskContext) -> Self {
        self.context = context;
        self
    }

    /// Execute an LLM task
    pub async fn execute<T: LlmTask>(&self, task: T) -> Result<T::Output, String> {
        // Validate inputs
        task.validate()?;

        // Check AI availability
        if !self.ai_manager.is_available().await {
            return Err("AI service not available".to_string());
        }

        let task_name = task.name();
        log::info!("Executing LLM task: {}", task_name);

        // Build prompts
        let system_prompt = task.system_prompt();
        let user_prompt = task.user_prompt(&self.context);

        log::debug!("Task {} - System prompt length: {}", task_name, system_prompt.len());
        log::debug!("Task {} - User prompt length: {}", task_name, user_prompt.len());

        // Call AI
        let response = self.ai_manager
            .generate(&user_prompt, Some(&system_prompt))
            .await
            .map_err(|e| format!("AI generation failed for {}: {}", task_name, e))?;

        log::debug!("Task {} - Response length: {}", task_name, response.content.len());

        // Parse response
        let output = task.parse_response(&response.content)
            .map_err(|e| format!("Failed to parse {} response: {}", task_name, e))?;

        // Post-process
        let final_output = task.post_process(output)?;

        log::info!("Task {} completed successfully", task_name);
        Ok(final_output)
    }

    /// Execute a task that returns JSON
    pub async fn execute_json<T, O>(&self, task: T) -> Result<O, String>
    where
        T: LlmTask<Output = O>,
        O: DeserializeOwned + Send,
    {
        self.execute(task).await
    }
}

// ============================================================================
// JSON Parsing Utilities
// ============================================================================

/// Helper to extract and parse JSON from LLM responses
pub fn extract_json<T: DeserializeOwned>(response: &str) -> Result<T, String> {
    // Try to find JSON in the response (LLMs often add explanatory text)
    let json_str = extract_json_string(response)?;

    serde_json::from_str(&json_str)
        .map_err(|e| format!("JSON parse error: {} in: {}", e, truncate(&json_str, 200)))
}

/// Extract the JSON portion from a response that may contain other text
pub fn extract_json_string(response: &str) -> Result<String, String> {
    // Find the first { or [
    let start = response.find('{')
        .or_else(|| response.find('['))
        .ok_or("No JSON found in response")?;

    let is_array = response.chars().nth(start) == Some('[');
    let _end_char = if is_array { ']' } else { '}' };

    // Find matching closing bracket
    let mut depth = 0;
    let mut end = start;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, c) in response[start..].char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' | '[' if !in_string => depth += 1,
            '}' | ']' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    end = start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    if depth != 0 {
        return Err("Unbalanced JSON brackets".to_string());
    }

    Ok(response[start..end].to_string())
}

/// Truncate a string for display
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

// ============================================================================
// Common Response Types
// ============================================================================

/// A simple text response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextResponse {
    pub text: String,
}

/// A response with a summary and key points
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryResponse {
    pub summary: String,
    pub key_points: Vec<String>,
}

/// A response with a verdict and explanation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictResponse {
    pub verdict: String,
    pub confidence: f32,
    pub explanation: String,
}

/// A list of items response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse<T> {
    pub items: Vec<T>,
}

/// A response with structured analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResponse {
    pub analysis: String,
    pub findings: Vec<Finding>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub category: String,
    pub description: String,
    pub severity: Option<String>,
}

// ============================================================================
// Prompt Templates
// ============================================================================

/// Common prompt templates that can be reused across tasks
pub mod prompts {
    /// System prompt for JSON-only responses
    pub const JSON_RESPONDER: &str =
        "You are a helpful assistant. Always respond with valid JSON only, no additional text.";

    /// System prompt for analysis tasks
    pub const ANALYST: &str =
        "You are an expert analyst. Provide thorough, objective analysis. \
         Always respond with valid JSON.";

    /// System prompt for summarization
    pub const SUMMARIZER: &str =
        "You are a skilled summarizer. Extract the key information concisely. \
         Always respond with valid JSON.";

    /// System prompt for fact-checking
    pub const FACT_CHECKER: &str =
        "You are a fact-checker. Evaluate claims objectively based on evidence. \
         Always respond with valid JSON.";

    /// System prompt for research assistance
    pub const RESEARCHER: &str =
        "You are a research assistant. Help find, analyze, and synthesize information. \
         Always respond with valid JSON.";

    /// System prompt for code tasks
    pub const CODE_ASSISTANT: &str =
        "You are an expert programmer. Write clean, efficient, well-documented code. \
         Follow best practices and conventions.";

    /// Build a prompt that requests JSON output in a specific format
    pub fn json_format_instruction(format_example: &str) -> String {
        format!(
            "\n\nRespond in this exact JSON format:\n```json\n{}\n```",
            format_example
        )
    }

    /// Build a prompt section for providing context
    pub fn context_section(label: &str, content: &str, max_chars: usize) -> String {
        let truncated = if content.len() > max_chars {
            format!("{}...[truncated]", &content[..max_chars])
        } else {
            content.to_string()
        };
        format!("\n\n{}:\n{}", label, truncated)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_simple() {
        let response = r#"Here is the result: {"name": "test", "value": 42}"#;
        let json: serde_json::Value = extract_json(response).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["value"], 42);
    }

    #[test]
    fn test_extract_json_nested() {
        let response = r#"{"outer": {"inner": "value"}, "array": [1, 2, 3]}"#;
        let json: serde_json::Value = extract_json(response).unwrap();
        assert_eq!(json["outer"]["inner"], "value");
    }

    #[test]
    fn test_extract_json_with_strings() {
        let response = r#"{"text": "contains { brackets } inside"}"#;
        let json: serde_json::Value = extract_json(response).unwrap();
        assert_eq!(json["text"], "contains { brackets } inside");
    }

    #[test]
    fn test_extract_json_array() {
        let response = r#"Results: [{"a": 1}, {"b": 2}]"#;
        let json: serde_json::Value = extract_json(response).unwrap();
        assert!(json.is_array());
        assert_eq!(json[0]["a"], 1);
    }
}
