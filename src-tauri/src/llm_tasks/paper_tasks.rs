//! Paper Generation LLM Tasks
//!
//! Task implementations for research paper generation pipeline.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{LlmTask, TaskContext, extract_json, prompts};

// ============================================================================
// Semantic Chunk Task
// ============================================================================

/// Task to determine optimal chunk boundaries for a document
pub struct SemanticChunkTask {
    /// Document content to chunk
    pub content: String,
    /// Target words per chunk
    pub target_words: usize,
}

/// Output from semantic chunking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkBoundaries {
    /// List of chunk boundaries (start, end character positions)
    pub boundaries: Vec<(usize, usize)>,
    /// Suggested headings for each chunk
    pub headings: Vec<Option<String>>,
}

#[async_trait]
impl LlmTask for SemanticChunkTask {
    type Output = ChunkBoundaries;

    fn name(&self) -> &'static str {
        "semantic_chunk"
    }

    fn system_prompt(&self) -> String {
        format!(
            "{}\n\nYou are analyzing document structure to find optimal semantic boundaries. \
             Identify natural break points at section, paragraph, or topic boundaries.",
            prompts::ANALYST
        )
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        format!(
            "Analyze this document and suggest optimal chunk boundaries.\n\
             Target approximately {} words per chunk.\n\
             Find natural semantic boundaries (section breaks, topic changes, etc.)\n\n\
             Document (first 5000 chars):\n{}\n\n\
             Respond with JSON:\n\
             ```json\n\
             {{\n\
               \"boundaries\": [[start1, end1], [start2, end2], ...],\n\
               \"headings\": [\"heading or null\", ...]\n\
             }}\n\
             ```",
            self.target_words,
            truncate(&self.content, 5000)
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        #[derive(Deserialize)]
        struct Response {
            boundaries: Vec<(usize, usize)>,
            headings: Vec<Option<String>>,
        }

        let parsed: Response = extract_json(response)?;
        Ok(ChunkBoundaries {
            boundaries: parsed.boundaries,
            headings: parsed.headings,
        })
    }
}

// ============================================================================
// Extract Key Findings Task
// ============================================================================

/// Task to extract key findings from a chunk of content
pub struct ExtractKeyFindingsTask {
    /// Content to extract from
    pub content: String,
    /// Research question for relevance filtering
    pub research_question: String,
    /// Source ID for attribution
    pub source_id: String,
}

/// A single extracted finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFinding {
    /// Type of finding (quote, data, claim, definition, etc.)
    pub finding_type: String,
    /// The actual content
    pub content: String,
    /// Relevance to research question (0.0 - 1.0)
    pub relevance: f32,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Location hint (if detectable)
    pub location: Option<String>,
}

/// Output from finding extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFindings {
    /// List of findings
    pub findings: Vec<ExtractedFinding>,
}

#[async_trait]
impl LlmTask for ExtractKeyFindingsTask {
    type Output = ExtractedFindings;

    fn name(&self) -> &'static str {
        "extract_key_findings"
    }

    fn system_prompt(&self) -> String {
        format!(
            "{}\n\nYou are extracting key findings from research material. \
             Identify quotes, data points, claims, definitions, and other citable content. \
             Rate relevance to the research question.",
            prompts::RESEARCHER
        )
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        format!(
            "Extract key findings relevant to the research question.\n\n\
             Research Question: {}\n\n\
             Content:\n{}\n\n\
             For each finding, identify:\n\
             - type: quote, data, claim, definition, example, method, or conclusion\n\
             - content: the exact text or paraphrased finding\n\
             - relevance: 0.0-1.0 score based on relevance to research question\n\
             - tags: categorization tags\n\n\
             Respond with JSON:\n\
             ```json\n\
             {{\n\
               \"findings\": [\n\
                 {{\n\
                   \"finding_type\": \"quote|data|claim|definition|example|method|conclusion\",\n\
                   \"content\": \"the finding text\",\n\
                   \"relevance\": 0.8,\n\
                   \"tags\": [\"tag1\", \"tag2\"],\n\
                   \"location\": \"optional location hint\"\n\
                 }}\n\
               ]\n\
             }}\n\
             ```",
            self.research_question,
            self.content
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        let parsed: ExtractedFindings = extract_json(response)?;
        Ok(parsed)
    }
}

// ============================================================================
// Generate Outline Task
// ============================================================================

/// Task to generate a paper outline
pub struct GenerateOutlineTask {
    /// Research question
    pub research_question: String,
    /// Paper type
    pub paper_type: String,
    /// Thesis hint (optional)
    pub thesis_hint: Option<String>,
    /// Summary of available sources
    pub sources_summary: String,
    /// Summary of key findings
    pub findings_summary: String,
}

/// Generated outline section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineSection {
    /// Section title
    pub title: String,
    /// Section type
    pub section_type: String,
    /// What the section will cover
    pub description: String,
    /// Key points to address
    pub key_points: Vec<String>,
    /// Target word count
    pub target_words: usize,
}

/// Output from outline generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedOutline {
    /// Thesis statement
    pub thesis: String,
    /// Sections in order
    pub sections: Vec<OutlineSection>,
}

#[async_trait]
impl LlmTask for GenerateOutlineTask {
    type Output = GeneratedOutline;

    fn name(&self) -> &'static str {
        "generate_outline"
    }

    fn system_prompt(&self) -> String {
        format!(
            "{}\n\nYou are creating a research paper outline. \
             Generate a logical structure that addresses the research question, \
             has clear transitions, and effectively integrates available evidence.",
            prompts::RESEARCHER
        )
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let thesis_part = self.thesis_hint.as_ref()
            .map(|t| format!("\nProposed thesis: {}", t))
            .unwrap_or_default();

        format!(
            "Create an outline for a {} paper.\n\n\
             Research Question: {}\n{}\n\n\
             Available Sources:\n{}\n\n\
             Key Findings:\n{}\n\n\
             Respond with JSON:\n\
             ```json\n\
             {{\n\
               \"thesis\": \"the main thesis statement\",\n\
               \"sections\": [\n\
                 {{\n\
                   \"title\": \"Section Title\",\n\
                   \"section_type\": \"abstract|introduction|literature_review|methodology|results|discussion|conclusion|custom\",\n\
                   \"description\": \"what this section covers\",\n\
                   \"key_points\": [\"point 1\", \"point 2\"],\n\
                   \"target_words\": 500\n\
                 }}\n\
               ]\n\
             }}\n\
             ```",
            self.paper_type,
            self.research_question,
            thesis_part,
            self.sources_summary,
            self.findings_summary
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        let parsed: GeneratedOutline = extract_json(response)?;
        Ok(parsed)
    }
}

// ============================================================================
// Write Section Task
// ============================================================================

/// Task to write a paper section
pub struct WriteSectionTask {
    /// Section title
    pub section_title: String,
    /// Section type
    pub section_type: String,
    /// Research question
    pub research_question: String,
    /// Thesis
    pub thesis: String,
    /// Summary of previous sections (for context)
    pub previous_sections: String,
    /// Relevant findings to incorporate
    pub relevant_findings: String,
    /// Target word count
    pub target_words: usize,
}

/// Written section content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrittenSection {
    /// The section content (with citation markers)
    pub content: String,
    /// Word count achieved
    pub word_count: usize,
}

#[async_trait]
impl LlmTask for WriteSectionTask {
    type Output = WrittenSection;

    fn name(&self) -> &'static str {
        "write_section"
    }

    fn system_prompt(&self) -> String {
        format!(
            "{}\n\nYou are writing a section of a research paper. \
             Use [[source_id:page]] format for citations. \
             Write in an academic style with clear arguments and evidence. \
             Ensure smooth transitions and logical flow.",
            prompts::RESEARCHER
        )
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        format!(
            "Write the \"{}\" section ({}).\n\n\
             Research Question: {}\n\
             Thesis: {}\n\n\
             Previous Sections Summary:\n{}\n\n\
             Findings to Incorporate:\n{}\n\n\
             Target: approximately {} words.\n\n\
             Write the section content. Use [[source_id:page]] for citations.",
            self.section_title,
            self.section_type,
            self.research_question,
            self.thesis,
            if self.previous_sections.is_empty() { "(This is the first section)" } else { &self.previous_sections },
            self.relevant_findings,
            self.target_words
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        // For writing tasks, the response is the content itself, not JSON
        let content = response.trim().to_string();
        let word_count = content.split_whitespace().count();

        Ok(WrittenSection {
            content,
            word_count,
        })
    }
}

// ============================================================================
// Review Section Task
// ============================================================================

/// Task to review a paper section
pub struct ReviewSectionTask {
    /// Section title
    pub section_title: String,
    /// Section content
    pub content: String,
    /// Research question
    pub research_question: String,
    /// Thesis
    pub thesis: String,
    /// Specific focus areas (optional)
    pub focus_areas: Vec<String>,
}

/// An issue found during review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewIssue {
    /// Type of issue
    pub issue_type: String,
    /// Description
    pub description: String,
    /// Location in text
    pub location: Option<String>,
    /// Severity
    pub severity: String,
}

/// Output from section review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionReview {
    /// Overall quality score (0.0 - 1.0)
    pub quality_score: f32,
    /// Issues found
    pub issues: Vec<ReviewIssue>,
    /// Suggestions for improvement
    pub suggestions: Vec<String>,
    /// Whether the section passes review
    pub passes: bool,
}

#[async_trait]
impl LlmTask for ReviewSectionTask {
    type Output = SectionReview;

    fn name(&self) -> &'static str {
        "review_section"
    }

    fn system_prompt(&self) -> String {
        format!(
            "{}\n\nYou are reviewing a research paper section for quality. \
             Check for: citation needs, unclear arguments, missing evidence, \
             flow issues, grammar problems, factual concerns, and length issues. \
             Provide constructive, specific feedback.",
            prompts::ANALYST
        )
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        let focus = if self.focus_areas.is_empty() {
            String::new()
        } else {
            format!("\nFocus especially on: {}", self.focus_areas.join(", "))
        };

        format!(
            "Review this paper section.\n\n\
             Section: {}\n\
             Research Question: {}\n\
             Thesis: {}\n{}\n\n\
             Content:\n{}\n\n\
             Respond with JSON:\n\
             ```json\n\
             {{\n\
               \"quality_score\": 0.0-1.0,\n\
               \"issues\": [\n\
                 {{\n\
                   \"issue_type\": \"citation_needed|unclear_argument|missing_evidence|flow_issue|grammar|factual_concern|length_issue\",\n\
                   \"description\": \"what's wrong\",\n\
                   \"location\": \"where in the text (optional)\",\n\
                   \"severity\": \"low|medium|high\"\n\
                 }}\n\
               ],\n\
               \"suggestions\": [\"suggestion 1\", \"suggestion 2\"],\n\
               \"passes\": true/false\n\
             }}\n\
             ```",
            self.section_title,
            self.research_question,
            self.thesis,
            focus,
            self.content
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        let parsed: SectionReview = extract_json(response)?;
        Ok(parsed)
    }
}

// ============================================================================
// Generate Abstract Task
// ============================================================================

/// Task to generate paper abstract
pub struct GenerateAbstractTask {
    /// Paper title
    pub title: String,
    /// Research question
    pub research_question: String,
    /// Thesis
    pub thesis: String,
    /// Summary of sections
    pub sections_summary: String,
    /// Target word count
    pub target_words: usize,
}

/// Generated abstract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedAbstract {
    /// Abstract content
    pub content: String,
    /// Word count
    pub word_count: usize,
    /// Keywords extracted
    pub keywords: Vec<String>,
}

#[async_trait]
impl LlmTask for GenerateAbstractTask {
    type Output = GeneratedAbstract;

    fn name(&self) -> &'static str {
        "generate_abstract"
    }

    fn system_prompt(&self) -> String {
        format!(
            "{}\n\nYou are writing a research paper abstract. \
             Summarize the purpose, methodology, findings, and conclusions concisely. \
             Include key terms for discoverability.",
            prompts::SUMMARIZER
        )
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        format!(
            "Write an abstract for this paper.\n\n\
             Title: {}\n\
             Research Question: {}\n\
             Thesis: {}\n\n\
             Section Summaries:\n{}\n\n\
             Target: {} words maximum.\n\n\
             Respond with JSON:\n\
             ```json\n\
             {{\n\
               \"content\": \"the abstract text\",\n\
               \"keywords\": [\"keyword1\", \"keyword2\"]\n\
             }}\n\
             ```",
            self.title,
            self.research_question,
            self.thesis,
            self.sections_summary,
            self.target_words
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        #[derive(Deserialize)]
        struct Response {
            content: String,
            keywords: Vec<String>,
        }

        let parsed: Response = extract_json(response)?;
        let word_count = parsed.content.split_whitespace().count();

        Ok(GeneratedAbstract {
            content: parsed.content,
            word_count,
            keywords: parsed.keywords,
        })
    }
}

// ============================================================================
// Improve Section Task
// ============================================================================

/// Task to improve a section based on review feedback
pub struct ImproveSectionTask {
    /// Section title
    pub section_title: String,
    /// Current content
    pub content: String,
    /// Issues to address
    pub issues: Vec<String>,
    /// Suggestions to incorporate
    pub suggestions: Vec<String>,
}

/// Improved section content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovedSection {
    /// The improved content
    pub content: String,
    /// Changes made
    pub changes: Vec<String>,
    /// Word count
    pub word_count: usize,
}

#[async_trait]
impl LlmTask for ImproveSectionTask {
    type Output = ImprovedSection;

    fn name(&self) -> &'static str {
        "improve_section"
    }

    fn system_prompt(&self) -> String {
        format!(
            "{}\n\nYou are improving a research paper section based on review feedback. \
             Address the identified issues while maintaining the original style and arguments. \
             Preserve any citations and improve clarity.",
            prompts::RESEARCHER
        )
    }

    fn user_prompt(&self, _ctx: &TaskContext) -> String {
        format!(
            "Improve this section based on review feedback.\n\n\
             Section: {}\n\n\
             Current Content:\n{}\n\n\
             Issues to Address:\n{}\n\n\
             Suggestions:\n{}\n\n\
             Respond with JSON:\n\
             ```json\n\
             {{\n\
               \"content\": \"the improved section content\",\n\
               \"changes\": [\"change 1\", \"change 2\"]\n\
             }}\n\
             ```",
            self.section_title,
            self.content,
            self.issues.iter().map(|i| format!("- {}", i)).collect::<Vec<_>>().join("\n"),
            self.suggestions.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n")
        )
    }

    fn parse_response(&self, response: &str) -> Result<Self::Output, String> {
        #[derive(Deserialize)]
        struct Response {
            content: String,
            changes: Vec<String>,
        }

        let parsed: Response = extract_json(response)?;
        let word_count = parsed.content.split_whitespace().count();

        Ok(ImprovedSection {
            content: parsed.content,
            changes: parsed.changes,
            word_count,
        })
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_findings_prompt() {
        let task = ExtractKeyFindingsTask {
            content: "Sample content".to_string(),
            research_question: "What is X?".to_string(),
            source_id: "src_123".to_string(),
        };

        let prompt = task.user_prompt(&TaskContext::default());
        assert!(prompt.contains("What is X?"));
        assert!(prompt.contains("Sample content"));
    }

    #[test]
    fn test_generate_outline_prompt() {
        let task = GenerateOutlineTask {
            research_question: "How does X affect Y?".to_string(),
            paper_type: "research_paper".to_string(),
            thesis_hint: Some("X increases Y.".to_string()),
            sources_summary: "Source 1, Source 2".to_string(),
            findings_summary: "Finding A, Finding B".to_string(),
        };

        let prompt = task.user_prompt(&TaskContext::default());
        assert!(prompt.contains("research_paper"));
        assert!(prompt.contains("X increases Y"));
    }

    #[test]
    fn test_write_section_prompt() {
        let task = WriteSectionTask {
            section_title: "Introduction".to_string(),
            section_type: "introduction".to_string(),
            research_question: "What is X?".to_string(),
            thesis: "X is important.".to_string(),
            previous_sections: String::new(),
            relevant_findings: "Finding 1".to_string(),
            target_words: 500,
        };

        let prompt = task.user_prompt(&TaskContext::default());
        assert!(prompt.contains("Introduction"));
        assert!(prompt.contains("500 words"));
    }
}
