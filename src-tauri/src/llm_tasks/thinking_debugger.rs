//! Thinking Debugger - Analyzes conversations to find errors in thinking
//!
//! This module helps users learn critical thinking by showing where their
//! reasoning went wrong when interacting with AI.
//!
//! Philosophy: AI should amplify growth, but only with critical thinking.
//! Without it, AI makes you confidently wrong faster.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{extract_json, LlmTask, TaskContext};

/// A single thinking error identified in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingError {
    /// Type of error: "fact", "reasoning", or "question"
    pub error_type: String,
    /// The specific quote or moment where the error occurred
    pub location: String,
    /// What went wrong
    pub problem: String,
    /// Why this matters
    pub why_it_matters: String,
    /// What they should have done instead
    pub better_approach: String,
    /// Severity: "minor", "moderate", "critical"
    pub severity: String,
}

/// The complete analysis of a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingAnalysis {
    /// Overall assessment
    pub summary: String,
    /// List of errors found
    pub errors: Vec<ThinkingError>,
    /// What the user did well (balance feedback)
    pub strengths: Vec<String>,
    /// Score from 0-100 on critical thinking demonstrated
    pub critical_thinking_score: u8,
    /// One key thing to focus on improving
    pub focus_area: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,  // "user" or "assistant"
    pub content: String,
}

/// A chunk of conversation for analysis
#[derive(Debug, Clone)]
pub struct ConversationChunk {
    pub messages: Vec<ConversationMessage>,
    pub chunk_index: usize,
    pub total_chunks: usize,
}

/// Maximum characters per chunk (~6k tokens at 4 chars/token, leaving room for prompt)
const MAX_CHUNK_CHARS: usize = 24000;
/// Overlap between chunks to maintain context
const CHUNK_OVERLAP_MESSAGES: usize = 2;

/// Split a conversation into analyzable chunks
pub fn chunk_conversation(messages: &[ConversationMessage]) -> Vec<ConversationChunk> {
    if messages.is_empty() {
        return vec![];
    }

    // Calculate total size
    let total_chars: usize = messages.iter()
        .map(|m| m.content.len() + m.role.len() + 10)
        .sum();

    // If it fits in one chunk, return as-is
    if total_chars <= MAX_CHUNK_CHARS {
        return vec![ConversationChunk {
            messages: messages.to_vec(),
            chunk_index: 0,
            total_chunks: 1,
        }];
    }

    let mut chunks = Vec::new();
    let mut current_chunk: Vec<ConversationMessage> = Vec::new();
    let mut current_size = 0;
    let mut start_idx = 0;

    for (idx, msg) in messages.iter().enumerate() {
        let msg_size = msg.content.len() + msg.role.len() + 10;

        // If adding this message would exceed limit, save current chunk
        if current_size + msg_size > MAX_CHUNK_CHARS && !current_chunk.is_empty() {
            chunks.push(current_chunk.clone());

            // Start new chunk with overlap
            let overlap_start = if idx > CHUNK_OVERLAP_MESSAGES {
                idx - CHUNK_OVERLAP_MESSAGES
            } else {
                start_idx
            };

            current_chunk = messages[overlap_start..idx].to_vec();
            current_size = current_chunk.iter()
                .map(|m| m.content.len() + m.role.len() + 10)
                .sum();
            start_idx = overlap_start;
        }

        current_chunk.push(msg.clone());
        current_size += msg_size;
    }

    // Don't forget the last chunk
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    let total_chunks = chunks.len();
    chunks.into_iter()
        .enumerate()
        .map(|(idx, messages)| ConversationChunk {
            messages,
            chunk_index: idx,
            total_chunks,
        })
        .collect()
}

/// Merge multiple chunk analyses into a single result
pub fn merge_analyses(analyses: Vec<ThinkingAnalysis>) -> ThinkingAnalysis {
    if analyses.is_empty() {
        return ThinkingAnalysis {
            summary: "No content to analyze.".to_string(),
            errors: vec![],
            strengths: vec![],
            critical_thinking_score: 50,
            focus_area: "N/A".to_string(),
        };
    }

    if analyses.len() == 1 {
        return analyses.into_iter().next().unwrap();
    }

    // Collect all errors (deduplicate by location)
    let mut all_errors: Vec<ThinkingError> = Vec::new();
    let mut seen_locations = std::collections::HashSet::new();

    for analysis in &analyses {
        for error in &analysis.errors {
            let location_key = error.location.chars().take(50).collect::<String>();
            if !seen_locations.contains(&location_key) {
                seen_locations.insert(location_key);
                all_errors.push(error.clone());
            }
        }
    }

    // Collect unique strengths
    let mut all_strengths: Vec<String> = Vec::new();
    let mut seen_strengths = std::collections::HashSet::new();

    for analysis in &analyses {
        for strength in &analysis.strengths {
            let strength_key = strength.chars().take(30).collect::<String>();
            if !seen_strengths.contains(&strength_key) {
                seen_strengths.insert(strength_key);
                all_strengths.push(strength.clone());
            }
        }
    }

    // Average the scores
    let avg_score: u8 = (analyses.iter()
        .map(|a| a.critical_thinking_score as u32)
        .sum::<u32>() / analyses.len() as u32) as u8;

    // Combine summaries
    let combined_summary = if analyses.len() <= 3 {
        analyses.iter()
            .map(|a| a.summary.clone())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        // Just use the first and last for very long conversations
        format!("{} ... {}",
                analyses.first().map(|a| a.summary.as_str()).unwrap_or(""),
                analyses.last().map(|a| a.summary.as_str()).unwrap_or(""))
    };

    // Most common focus area (or first one)
    let focus_area = analyses.first()
        .map(|a| a.focus_area.clone())
        .unwrap_or_else(|| "Critical thinking".to_string());

    ThinkingAnalysis {
        summary: combined_summary,
        errors: all_errors,
        strengths: all_strengths,
        critical_thinking_score: avg_score,
        focus_area,
    }
}

/// Task to analyze a chunk of conversation for thinking errors
pub struct AnalyzeChunkTask {
    /// The chunk to analyze
    pub chunk: ConversationChunk,
    /// Optional topic/context for the conversation
    pub topic: Option<String>,
}

#[async_trait]
impl LlmTask for AnalyzeChunkTask {
    type Output = ThinkingAnalysis;

    fn name(&self) -> &'static str {
        "analyze_thinking_chunk"
    }

    fn system_prompt(&self) -> String {
        r#"You are a critical thinking coach. Your job is to analyze conversations between a human and an AI, identifying where the human's thinking went wrong.

You are DIRECT and HONEST. Not mean, but clear - like a good teacher who respects the student enough to tell them the truth.

You look for three types of errors:

1. WRONG FACTS - The human accepted something false without questioning it
   - Believed incorrect information
   - Didn't verify claims
   - Accepted statistics or data without source

2. WRONG REASONING - The human's logic was flawed
   - Non-sequiturs (conclusion doesn't follow from premises)
   - Confirmation bias (only accepting info that supports their view)
   - False dichotomies (assuming only two options exist)
   - Hasty generalizations
   - Correlation/causation confusion

3. WRONG QUESTIONS - The human asked the wrong thing entirely
   - Should have asked a more fundamental question first
   - Missed the real issue
   - Asked leading questions that assumed the answer
   - Didn't go deep enough

Be specific. Quote the exact moment where thinking broke down.
Don't soften the feedback - clarity helps learning.
Also acknowledge what they did well, but don't use this to dilute criticism.

Always respond with valid JSON."#.to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let topic_context = self.topic.as_ref()
            .map(|t| format!("\n\nTopic/Context: {}\n", t))
            .unwrap_or_default();

        let chunk_note = if self.chunk.total_chunks > 1 {
            format!("\n(Analyzing part {} of {} of this conversation)\n",
                    self.chunk.chunk_index + 1, self.chunk.total_chunks)
        } else {
            String::new()
        };

        let conversation_text: String = self.chunk.messages.iter()
            .map(|msg| format!("[{}]: {}", msg.role.to_uppercase(), msg.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        format!(r#"Analyze this conversation for thinking errors.{}{}

CONVERSATION:
{}

Respond with this JSON structure:
{{
    "summary": "Brief overall assessment of the thinking quality",
    "errors": [
        {{
            "error_type": "fact|reasoning|question",
            "location": "Quote the exact text where error occurred",
            "problem": "What went wrong",
            "why_it_matters": "Why this error is significant",
            "better_approach": "What they should have done",
            "severity": "minor|moderate|critical"
        }}
    ],
    "strengths": ["Things the user did well"],
    "critical_thinking_score": 0-100,
    "focus_area": "One key thing to work on"
}}"#, topic_context, chunk_note, conversation_text)
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }
}

/// Legacy task for backwards compatibility - now just wraps single chunk
pub struct AnalyzeThinkingTask {
    pub conversation: Vec<ConversationMessage>,
    pub topic: Option<String>,
}

#[async_trait]
impl LlmTask for AnalyzeThinkingTask {
    type Output = ThinkingAnalysis;

    fn name(&self) -> &'static str {
        "analyze_thinking"
    }

    fn system_prompt(&self) -> String {
        AnalyzeChunkTask {
            chunk: ConversationChunk {
                messages: vec![],
                chunk_index: 0,
                total_chunks: 1,
            },
            topic: None,
        }.system_prompt()
    }

    fn user_prompt(&self, ctx: &TaskContext) -> String {
        // Create single chunk and delegate
        let chunk = ConversationChunk {
            messages: self.conversation.clone(),
            chunk_index: 0,
            total_chunks: 1,
        };
        AnalyzeChunkTask {
            chunk,
            topic: self.topic.clone(),
        }.user_prompt(ctx)
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }
}

/// Quick task to check a single claim/statement (for thinking debugger)
pub struct ClaimFactCheckTask {
    pub claim: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimFactCheckResult {
    pub claim: String,
    pub verdict: String,  // "true", "false", "partially_true", "unverifiable"
    pub confidence: f32,
    pub explanation: String,
    pub what_to_check: Vec<String>,
}

#[async_trait]
impl LlmTask for ClaimFactCheckTask {
    type Output = ClaimFactCheckResult;

    fn name(&self) -> &'static str {
        "fact_check"
    }

    fn system_prompt(&self) -> String {
        r#"You are a fact-checker. Evaluate claims objectively.

Be honest about what you know vs. what you're uncertain about.
If something is unverifiable, say so.
Provide specific things the user could check to verify.

Always respond with valid JSON."#.to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let context = self.context.as_ref()
            .map(|c| format!("\nContext: {}", c))
            .unwrap_or_default();

        format!(r#"Fact-check this claim:

"{}"{}

Respond with:
{{
    "claim": "The claim being checked",
    "verdict": "true|false|partially_true|unverifiable",
    "confidence": 0.0-1.0,
    "explanation": "Why this verdict",
    "what_to_check": ["Specific things to verify this claim"]
}}"#, self.claim, context)
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }
}

/// Task to suggest better questions
pub struct BetterQuestionsTask {
    pub original_question: String,
    pub topic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetterQuestionsResult {
    pub original: String,
    pub problem_with_original: String,
    pub better_questions: Vec<BetterQuestion>,
    pub fundamental_question: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetterQuestion {
    pub question: String,
    pub why_better: String,
}

#[async_trait]
impl LlmTask for BetterQuestionsTask {
    type Output = BetterQuestionsResult;

    fn name(&self) -> &'static str {
        "better_questions"
    }

    fn system_prompt(&self) -> String {
        r#"You are a Socratic teacher. You help people ask better questions.

A good question:
- Gets to the root of the issue
- Doesn't assume the answer
- Opens up thinking rather than closing it down
- Challenges assumptions
- Seeks understanding, not just information

Always respond with valid JSON."#.to_string()
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let topic = self.topic.as_ref()
            .map(|t| format!("\nTopic: {}", t))
            .unwrap_or_default();

        format!(r#"This question was asked:

"{}"{}

What's wrong with this question, and what would be better?

Respond with:
{{
    "original": "The original question",
    "problem_with_original": "What's limiting about this question",
    "better_questions": [
        {{
            "question": "A better question to ask",
            "why_better": "Why this opens up better thinking"
        }}
    ],
    "fundamental_question": "The most fundamental question they should start with"
}}"#, self.original_question, topic)
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        extract_json(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_message() {
        let msg = ConversationMessage {
            role: "user".to_string(),
            content: "What is the capital of France?".to_string(),
        };
        assert_eq!(msg.role, "user");
    }

    #[test]
    fn test_chunk_small_conversation() {
        let messages = vec![
            ConversationMessage { role: "user".to_string(), content: "Hello".to_string() },
            ConversationMessage { role: "assistant".to_string(), content: "Hi there!".to_string() },
        ];

        let chunks = chunk_conversation(&messages);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].messages.len(), 2);
    }

    #[test]
    fn test_merge_single_analysis() {
        let analysis = ThinkingAnalysis {
            summary: "Good thinking".to_string(),
            errors: vec![],
            strengths: vec!["Clear questions".to_string()],
            critical_thinking_score: 80,
            focus_area: "Keep it up".to_string(),
        };

        let merged = merge_analyses(vec![analysis.clone()]);
        assert_eq!(merged.critical_thinking_score, 80);
    }
}
