//! Built-in LLM Task Implementations
//!
//! This module contains ready-to-use task implementations for common AI operations.
//! Use these as examples when creating your own tasks.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{extract_json, prompts, LlmTask, TaskContext, SummaryResponse};

// ============================================================================
// Summarization Tasks
// ============================================================================

/// Summarize content and extract key points
pub struct SummarizeContentTask {
    pub content: String,
    pub max_content_chars: usize,
}

impl SummarizeContentTask {
    pub fn new(content: String) -> Self {
        Self {
            content,
            max_content_chars: 8000,
        }
    }

    pub fn with_max_chars(mut self, max: usize) -> Self {
        self.max_content_chars = max;
        self
    }
}

#[async_trait]
impl LlmTask for SummarizeContentTask {
    type Output = SummaryResponse;

    fn name(&self) -> &'static str {
        "summarize_content"
    }

    fn system_prompt(&self) -> String {
        prompts::SUMMARIZER.to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let content = if self.content.len() > self.max_content_chars {
            format!("{}...[truncated]", &self.content[..self.max_content_chars])
        } else {
            self.content.clone()
        };

        format!(
            r#"Analyze the following content and provide:
1. A concise summary (2-3 sentences)
2. 3-5 key points or takeaways

Content:
{}

Respond in this exact JSON format:
{{
  "summary": "Your summary here",
  "key_points": ["point 1", "point 2", "point 3"]
}}"#,
            content
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }

    fn validate(&self) -> Result<(), String> {
        if self.content.trim().is_empty() {
            return Err("Content cannot be empty".to_string());
        }
        Ok(())
    }
}

// ============================================================================
// Fact-Checking Tasks
// ============================================================================

/// Source information for fact-checking
#[derive(Debug, Clone)]
pub struct FactCheckSource {
    pub id: String,
    pub title: String,
    pub content: String,
}

/// Fact-check a claim against provided sources
pub struct FactCheckTask {
    pub claim: String,
    pub sources: Vec<FactCheckSource>,
    pub max_source_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactCheckResult {
    pub verdict: String,
    pub confidence: f32,
    pub supporting_sources: Vec<String>,
    pub contradicting_sources: Vec<String>,
    pub explanation: String,
}

impl FactCheckTask {
    pub fn new(claim: String, sources: Vec<FactCheckSource>) -> Self {
        Self {
            claim,
            sources,
            max_source_chars: 1000,
        }
    }
}

#[async_trait]
impl LlmTask for FactCheckTask {
    type Output = FactCheckResult;

    fn name(&self) -> &'static str {
        "fact_check"
    }

    fn system_prompt(&self) -> String {
        prompts::FACT_CHECKER.to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let sources_text = self.sources.iter()
            .map(|s| {
                let content = if s.content.len() > self.max_source_chars {
                    format!("{}...", &s.content[..self.max_source_chars])
                } else {
                    s.content.clone()
                };
                format!("Source [{}]: {}\nContent: {}", s.id, s.title, content)
            })
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        format!(
            r#"Fact-check the following claim against the provided sources:

CLAIM: {}

SOURCES:
{}

Analyze whether the claim is supported, contradicted, or unverifiable based on these sources.

Respond in this exact JSON format:
{{
  "verdict": "supported|contradicted|partially_supported|unverifiable",
  "confidence": 0.0 to 1.0,
  "supporting_sources": ["source_id1", "source_id2"],
  "contradicting_sources": ["source_id3"],
  "explanation": "Detailed explanation of the verdict"
}}"#,
            self.claim, sources_text
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }

    fn validate(&self) -> Result<(), String> {
        if self.claim.trim().is_empty() {
            return Err("Claim cannot be empty".to_string());
        }
        if self.sources.is_empty() {
            return Err("At least one source is required".to_string());
        }
        Ok(())
    }
}

// ============================================================================
// Search Query Generation
// ============================================================================

/// Generate an effective search query from a description
pub struct GenerateSearchQueryTask {
    pub description: String,
}

impl GenerateSearchQueryTask {
    pub fn new(description: String) -> Self {
        Self { description }
    }
}

#[async_trait]
impl LlmTask for GenerateSearchQueryTask {
    type Output = String;

    fn name(&self) -> &'static str {
        "generate_search_query"
    }

    fn system_prompt(&self) -> String {
        "You are a research assistant that creates effective search queries. \
         Be concise and focused."
            .to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        format!(
            r#"Convert this research description into an effective web search query.
Keep it concise (3-8 words) and focused on finding academic or authoritative sources.

Research description: "{}"

Respond with ONLY the search query, nothing else."#,
            self.description
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        Ok(response.trim().to_string())
    }

    fn validate(&self) -> Result<(), String> {
        if self.description.trim().is_empty() {
            return Err("Description cannot be empty".to_string());
        }
        Ok(())
    }
}

// ============================================================================
// Source Relevance Analysis
// ============================================================================

#[derive(Debug, Clone)]
pub struct DiscoveredSourceInfo {
    pub index: usize,
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevanceAnalysis {
    pub results: Vec<RelevanceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevanceItem {
    pub index: usize,
    pub reason: String,
}

/// Analyze relevance of discovered sources to a research topic
pub struct AnalyzeRelevanceTask {
    pub topic: String,
    pub sources: Vec<DiscoveredSourceInfo>,
}

impl AnalyzeRelevanceTask {
    pub fn new(topic: String, sources: Vec<DiscoveredSourceInfo>) -> Self {
        Self { topic, sources }
    }
}

#[async_trait]
impl LlmTask for AnalyzeRelevanceTask {
    type Output = RelevanceAnalysis;

    fn name(&self) -> &'static str {
        "analyze_relevance"
    }

    fn system_prompt(&self) -> String {
        prompts::RESEARCHER.to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let sources_list = self.sources.iter()
            .map(|s| format!("{}. {} - {}", s.index, s.title, s.snippet))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"Analyze these search results for relevance to the research topic.
For each result, provide a brief reason why it might be useful (or "low relevance" if not relevant).

Research topic: "{}"

Search results:
{}

Respond in this exact JSON format:
{{
  "results": [
    {{"index": 1, "reason": "Contains relevant data on..."}},
    {{"index": 2, "reason": "low relevance"}}
  ]
}}"#,
            self.topic, sources_list
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }
}

// ============================================================================
// Connection Finding
// ============================================================================

#[derive(Debug, Clone)]
pub struct SourceForConnection {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionsResult {
    pub common_themes: Vec<String>,
    pub connections: Vec<Connection>,
    pub synthesis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub source_a: String,
    pub source_b: String,
    pub relationship: String,
    pub description: String,
}

/// Find connections and themes across multiple sources
pub struct FindConnectionsTask {
    pub sources: Vec<SourceForConnection>,
}

impl FindConnectionsTask {
    pub fn new(sources: Vec<SourceForConnection>) -> Self {
        Self { sources }
    }
}

#[async_trait]
impl LlmTask for FindConnectionsTask {
    type Output = ConnectionsResult;

    fn name(&self) -> &'static str {
        "find_connections"
    }

    fn system_prompt(&self) -> String {
        prompts::ANALYST.to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let sources_text = self.sources.iter()
            .map(|s| format!(
                "Source [{}]: {}\nSummary: {}",
                s.id, s.title, s.summary
            ))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        format!(
            r#"Analyze these sources and find connections between them:

{}

Identify:
1. Common themes across all sources
2. Specific connections between pairs of sources
3. A synthesis that ties them together

Respond in this exact JSON format:
{{
  "common_themes": ["theme1", "theme2"],
  "connections": [
    {{
      "source_a": "source_id",
      "source_b": "source_id",
      "relationship": "supports|contradicts|extends|relates",
      "description": "How they connect"
    }}
  ],
  "synthesis": "Overall synthesis of all sources"
}}"#,
            sources_text
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }

    fn validate(&self) -> Result<(), String> {
        if self.sources.len() < 2 {
            return Err("At least 2 sources are required".to_string());
        }
        Ok(())
    }
}

// ============================================================================
// Code Analysis Tasks
// ============================================================================

/// Analyze code and suggest improvements
pub struct AnalyzeCodeTask {
    pub code: String,
    pub language: String,
    pub focus: Option<String>, // e.g., "performance", "security", "readability"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAnalysisResult {
    pub summary: String,
    pub issues: Vec<CodeIssue>,
    pub suggestions: Vec<CodeSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeIssue {
    pub severity: String, // "error", "warning", "info"
    pub line: Option<usize>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSuggestion {
    pub category: String,
    pub description: String,
    pub code_snippet: Option<String>,
}

impl AnalyzeCodeTask {
    pub fn new(code: String, language: String) -> Self {
        Self {
            code,
            language,
            focus: None,
        }
    }

    pub fn with_focus(mut self, focus: &str) -> Self {
        self.focus = Some(focus.to_string());
        self
    }
}

#[async_trait]
impl LlmTask for AnalyzeCodeTask {
    type Output = CodeAnalysisResult;

    fn name(&self) -> &'static str {
        "analyze_code"
    }

    fn system_prompt(&self) -> String {
        prompts::CODE_ASSISTANT.to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let focus_text = self.focus.as_ref()
            .map(|f| format!("\nFocus area: {}", f))
            .unwrap_or_default();

        format!(
            r#"Analyze this {} code:{}

```{}
{}
```

Provide:
1. A brief summary of what the code does
2. Any issues or bugs found
3. Suggestions for improvement

Respond in this exact JSON format:
{{
  "summary": "What the code does",
  "issues": [
    {{"severity": "error|warning|info", "line": 10, "description": "Issue description"}}
  ],
  "suggestions": [
    {{"category": "performance|security|readability|best-practice", "description": "Suggestion", "code_snippet": "optional improved code"}}
  ]
}}"#,
            self.language, focus_text, self.language, self.code
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }

    fn validate(&self) -> Result<(), String> {
        if self.code.trim().is_empty() {
            return Err("Code cannot be empty".to_string());
        }
        Ok(())
    }
}

// ============================================================================
// Generic Question Answering
// ============================================================================

/// Answer a question with optional context
pub struct AnswerQuestionTask {
    pub question: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerResult {
    pub answer: String,
    pub confidence: f32,
    pub sources_used: Vec<String>,
}

impl AnswerQuestionTask {
    pub fn new(question: String) -> Self {
        Self {
            question,
            context: None,
        }
    }

    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }
}

#[async_trait]
impl LlmTask for AnswerQuestionTask {
    type Output = AnswerResult;

    fn name(&self) -> &'static str {
        "answer_question"
    }

    fn system_prompt(&self) -> String {
        "You are a knowledgeable assistant. Answer questions accurately and concisely. \
         If you're not sure, say so. Always respond with valid JSON."
            .to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let context_section = self.context.as_ref()
            .map(|c| format!("\n\nContext:\n{}", c))
            .unwrap_or_default();

        format!(
            r#"Question: {}{}

Provide a clear, accurate answer.

Respond in this exact JSON format:
{{
  "answer": "Your answer here",
  "confidence": 0.0 to 1.0,
  "sources_used": ["any sources or context you referenced"]
}}"#,
            self.question, context_section
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }
}

// ============================================================================
// Text Classification
// ============================================================================

/// Classify text into categories
pub struct ClassifyTextTask {
    pub text: String,
    pub categories: Vec<String>,
    pub allow_multiple: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub categories: Vec<CategoryScore>,
    pub primary_category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryScore {
    pub category: String,
    pub confidence: f32,
}

impl ClassifyTextTask {
    pub fn new(text: String, categories: Vec<String>) -> Self {
        Self {
            text,
            categories,
            allow_multiple: false,
        }
    }

    pub fn allow_multiple(mut self) -> Self {
        self.allow_multiple = true;
        self
    }
}

#[async_trait]
impl LlmTask for ClassifyTextTask {
    type Output = ClassificationResult;

    fn name(&self) -> &'static str {
        "classify_text"
    }

    fn system_prompt(&self) -> String {
        prompts::ANALYST.to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let categories_list = self.categories.join(", ");
        let multiple_note = if self.allow_multiple {
            "The text may belong to multiple categories."
        } else {
            "Choose the single best category."
        };

        format!(
            r#"Classify this text into one or more of these categories: {}

{}

Text:
{}

Respond in this exact JSON format:
{{
  "categories": [
    {{"category": "category_name", "confidence": 0.0 to 1.0}}
  ],
  "primary_category": "most likely category"
}}"#,
            categories_list, multiple_note, self.text
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }

    fn validate(&self) -> Result<(), String> {
        if self.text.trim().is_empty() {
            return Err("Text cannot be empty".to_string());
        }
        if self.categories.is_empty() {
            return Err("At least one category is required".to_string());
        }
        Ok(())
    }
}

// ============================================================================
// Session Summarization
//===========================================================================

/// Summarize a chunk of work sessions
pub struct SummarizeSessionsTask {
    pub sessions_text: String,
    pub days_spanned: usize,
    pub total_session_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryResult {
    pub summary: String,
    pub key_projects: Vec<String>,
    pub key_topics: Vec<String>,
    pub activity_summary: String,
}

impl SummarizeSessionsTask {
    pub fn new(sessions_text: String, days_spanned: usize, total_session_count: usize) -> Self {
        Self {
            sessions_text,
            days_spanned,
            total_session_count,
        }
    }
}

#[async_trait]
impl LlmTask for SummarizeSessionsTask {
    type Output = SessionSummaryResult;

    fn name(&self) -> &'static str {
        "summarize_sessions"
    }

    fn system_prompt(&self) -> String {
        "You are an analytical assistant that summarizes work sessions. \
         Focus on identifying key themes, projects, and patterns. \
         Be concise and structured."
            .to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        format!(
            r#"Analyze these work sessions from the past {} days (total: {} sessions):

{}

Provide:
1. A concise summary of the work (2-3 sentences)
2. Key projects worked on
3. Main topics/themes
4. Activity patterns (e.g., "mostly coding with some research")

Respond in this exact JSON format:
{{
  "summary": "Overall summary of work done",
  "key_projects": ["project1", "project2"],
  "key_topics": ["topic1", "topic2", "topic3"],
  "activity_summary": "Brief description of activity patterns"
}}"#,
            self.days_spanned, self.total_session_count, self.sessions_text
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }

    fn validate(&self) -> Result<(), String> {
        if self.sessions_text.trim().is_empty() {
            return Err("Sessions text cannot be empty".to_string());
        }
        Ok(())
    }
}

/// Summarize multiple session summaries into one
pub struct SummarizeSummariesTask {
    pub summaries_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedSummaryResult {
    pub overview: String,
    pub projects_worked_on: Vec<String>,
    pub main_themes: Vec<String>,
}

impl SummarizeSummariesTask {
    pub fn new(summaries_text: String) -> Self {
        Self { summaries_text }
    }
}

#[async_trait]
impl LlmTask for SummarizeSummariesTask {
    type Output = CombinedSummaryResult;

    fn name(&self) -> &'static str {
        "summarize_summaries"
    }

    fn system_prompt(&self) -> String {
        "You are an analytical assistant that synthesizes multiple summaries into a coherent overview. \
         Extract the most important information and create a clean, organized summary."
            .to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        format!(
            r#"Synthesize these session summaries into one coherent overview:

{}

Provide:
1. A high-level overview (2-3 sentences)
2. Projects worked on (deduplicated list)
3. Main themes across all sessions

Respond in this exact JSON format:
{{
  "overview": "High-level overview of all work",
  "projects_worked_on": ["project1", "project2"],
  "main_themes": ["theme1", "theme2", "theme3"]
}}"#,
            self.summaries_text
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }

    fn validate(&self) -> Result<(), String> {
        if self.summaries_text.trim().is_empty() {
            return Err("Summaries text cannot be empty".to_string());
        }
        Ok(())
    }
}
