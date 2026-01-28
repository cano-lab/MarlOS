//! Paper Generation Pipeline
//!
//! Orchestrates the full paper generation workflow:
//! Sources → Chunking → Extraction → Outline → Write Sections → Review → Export

use std::sync::Arc;

use crate::ai::AiManager;
use crate::semantic_search::SemanticSearch;
use crate::providers::research::Source;
use crate::llm_tasks::{TaskRunner, TaskContext};

use super::{
    Paper, PaperSection, PaperStatus, SectionStatus, SectionType,
    KeyFinding, FindingType,
    SemanticChunk, SemanticChunker, ChunkingProgress, ChunkerConfig,
    CitationTracker, process_paper_citations,
    PaperStore,
};

// ============================================================================
// Pipeline Progress Types
// ============================================================================

/// Progress information for extraction operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractionProgress {
    /// Total chunks to process
    pub total_chunks: usize,
    /// Chunks processed so far
    pub processed_chunks: usize,
    /// Total findings extracted
    pub total_findings: usize,
    /// Current chunk being processed
    pub current_chunk: Option<String>,
    /// Whether extraction is complete
    pub complete: bool,
}

impl ExtractionProgress {
    pub fn new(total_chunks: usize) -> Self {
        Self {
            total_chunks,
            processed_chunks: 0,
            total_findings: 0,
            current_chunk: None,
            complete: false,
        }
    }

    pub fn progress_percent(&self) -> f32 {
        if self.total_chunks == 0 {
            return 100.0;
        }
        (self.processed_chunks as f32 / self.total_chunks as f32) * 100.0
    }
}

/// Progress for section writing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WritingProgress {
    /// Total sections to write
    pub total_sections: usize,
    /// Sections written so far
    pub written_sections: usize,
    /// Current section being written
    pub current_section: Option<String>,
    /// Total words written
    pub total_words: usize,
    /// Whether writing is complete
    pub complete: bool,
}

impl WritingProgress {
    pub fn new(total_sections: usize) -> Self {
        Self {
            total_sections,
            written_sections: 0,
            current_section: None,
            total_words: 0,
            complete: false,
        }
    }
}

/// Result of section review
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewResult {
    /// Section ID that was reviewed
    pub section_id: String,
    /// Overall quality score (0.0 - 1.0)
    pub quality_score: f32,
    /// Issues found
    pub issues: Vec<ReviewIssue>,
    /// Suggestions for improvement
    pub suggestions: Vec<String>,
    /// Whether the section passes quality check
    pub passes: bool,
}

/// An issue found during review
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewIssue {
    /// Type of issue
    pub issue_type: String,
    /// Description
    pub description: String,
    /// Location in text (if applicable)
    pub location: Option<String>,
    /// Severity (low, medium, high)
    pub severity: String,
}

// ============================================================================
// Paper Pipeline
// ============================================================================

/// Orchestrates the full paper generation workflow
pub struct PaperPipeline {
    /// AI manager for LLM operations
    ai_manager: Arc<AiManager>,
    /// Semantic search for source retrieval
    semantic_search: Arc<SemanticSearch>,
    /// Paper store for persistence
    store: PaperStore,
    /// The paper being generated
    paper: Paper,
    /// Chunks extracted from sources
    chunks: Vec<SemanticChunk>,
    /// Sources loaded for the paper
    sources: Vec<Source>,
}

impl PaperPipeline {
    pub fn new(
        ai_manager: Arc<AiManager>,
        semantic_search: Arc<SemanticSearch>,
        paper: Paper,
    ) -> Self {
        let store = PaperStore::new(semantic_search.clone());
        Self {
            ai_manager,
            semantic_search,
            store,
            paper,
            chunks: Vec::new(),
            sources: Vec::new(),
        }
    }

    /// Get the current paper
    pub fn paper(&self) -> &Paper {
        &self.paper
    }

    /// Get mutable reference to the paper
    pub fn paper_mut(&mut self) -> &mut Paper {
        &mut self.paper
    }

    /// Add sources to the paper
    pub async fn add_sources(&mut self, source_ids: Vec<String>) -> Result<usize, String> {
        let mut added = 0;

        for source_id in source_ids {
            // Load source from semantic search
            if let Some(source) = self.load_source(&source_id).await? {
                self.sources.push(source);
                self.paper.add_source(&source_id);
                added += 1;
            }
        }

        // Save paper state
        self.store.save(&self.paper).await?;

        Ok(added)
    }

    /// Load a source by ID
    async fn load_source(&self, source_id: &str) -> Result<Option<Source>, String> {
        let tag = format!("source_id:{}", source_id);

        let store = self.semantic_search.store.read().await;
        let all_objects = store.list(1000, 0)
            .map_err(|e| format!("Failed to search for source: {}", e))?;

        let result = all_objects.into_iter()
            .find(|obj| obj.tags.contains(&"kind:source".to_string()) && obj.tags.contains(&tag));

        if let Some(obj) = result {
            // The source is stored as JSON in the summary field
            if let Some(summary) = &obj.summary {
                let source: Source = serde_json::from_str(summary)
                    .map_err(|e| format!("Failed to parse source: {}", e))?;
                return Ok(Some(source));
            }
        }

        Ok(None)
    }

    /// Chunk all sources
    pub async fn chunk_sources(&mut self) -> Result<ChunkingProgress, String> {
        let chunker = SemanticChunker::new();
        let mut progress = ChunkingProgress::new(self.sources.len());

        self.chunks.clear();

        for source in &self.sources {
            progress.current_source = Some(source.title.clone());

            if let Some(content) = &source.content {
                let source_chunks = chunker.chunk(&source.id, content);
                progress.total_chunks += source_chunks.len();
                progress.total_words += source_chunks.iter().map(|c| c.word_count).sum::<usize>();
                self.chunks.extend(source_chunks);
            }

            progress.processed_sources += 1;
        }

        progress.complete = true;
        progress.current_source = None;

        // Update paper status
        self.paper.status = PaperStatus::Analyzed;
        self.store.save(&self.paper).await?;

        Ok(progress)
    }

    /// Extract key findings from chunks
    pub async fn extract_findings(&mut self) -> Result<ExtractionProgress, String> {
        let mut progress = ExtractionProgress::new(self.chunks.len());

        // Check if AI is available
        if !self.ai_manager.is_available().await {
            return Err("AI service not available".to_string());
        }

        let runner = TaskRunner::new(&self.ai_manager);

        for chunk in &self.chunks {
            progress.current_chunk = Some(chunk.id.clone());

            // Build context for extraction
            let ctx = TaskContext::new()
                .with_data("research_question", &self.paper.research_question)
                .with_data("chunk_content", &chunk.content);

            // Extract findings using LLM
            let findings = self.extract_findings_from_chunk(&runner, chunk, &ctx).await?;

            for finding in findings {
                self.paper.add_finding(finding);
                progress.total_findings += 1;
            }

            progress.processed_chunks += 1;
        }

        progress.complete = true;
        progress.current_chunk = None;

        // Save progress
        self.store.save(&self.paper).await?;

        Ok(progress)
    }

    /// Extract findings from a single chunk using LLM
    async fn extract_findings_from_chunk(
        &self,
        runner: &TaskRunner<'_>,
        chunk: &SemanticChunk,
        _ctx: &TaskContext,
    ) -> Result<Vec<KeyFinding>, String> {
        use crate::llm_tasks::prompts;

        let system_prompt = format!(
            "{}\n\nYou are extracting key findings from research material. \
             For each piece of citable information, identify:\n\
             - quotes: Direct quotes that support the research\n\
             - data: Statistical or factual data\n\
             - claims: Important claims or assertions\n\
             - definitions: Key term definitions\n\n\
             Research Question: {}",
            prompts::RESEARCHER,
            self.paper.research_question
        );

        let user_prompt = format!(
            "Extract key findings from this content that are relevant to the research question.\n\n\
             Content:\n{}\n\n\
             Respond with JSON:\n\
             ```json\n\
             {{\n\
               \"findings\": [\n\
                 {{\n\
                   \"type\": \"quote|data|claim|definition|example|method|conclusion\",\n\
                   \"content\": \"the actual finding text\",\n\
                   \"relevance\": 0.0-1.0,\n\
                   \"tags\": [\"tag1\", \"tag2\"]\n\
                 }}\n\
               ]\n\
             }}\n\
             ```",
            chunk.content
        );

        let response = self.ai_manager
            .generate(&user_prompt, Some(&system_prompt))
            .await
            .map_err(|e| format!("Failed to extract findings: {}", e))?;

        // Parse response
        let findings = self.parse_findings_response(&response.content, &chunk.source_id, chunk.page_number)?;

        Ok(findings)
    }

    /// Parse findings from LLM response
    fn parse_findings_response(
        &self,
        response: &str,
        source_id: &str,
        page_number: Option<usize>,
    ) -> Result<Vec<KeyFinding>, String> {
        use crate::llm_tasks::extract_json;

        #[derive(serde::Deserialize)]
        struct FindingsResponse {
            findings: Vec<FindingItem>,
        }

        #[derive(serde::Deserialize)]
        struct FindingItem {
            #[serde(rename = "type")]
            finding_type: String,
            content: String,
            relevance: Option<f32>,
            tags: Option<Vec<String>>,
        }

        let parsed: FindingsResponse = extract_json(response)
            .map_err(|e| format!("Failed to parse findings JSON: {}", e))?;

        let mut findings = Vec::new();

        for item in parsed.findings {
            let finding_type = match item.finding_type.to_lowercase().as_str() {
                "quote" => FindingType::Quote,
                "data" => FindingType::Data,
                "claim" => FindingType::Claim,
                "definition" => FindingType::Definition,
                "example" => FindingType::Example,
                "method" => FindingType::Method,
                "conclusion" => FindingType::Conclusion,
                _ => FindingType::Claim,
            };

            let mut finding = KeyFinding::new(finding_type, &item.content, source_id)
                .with_relevance(item.relevance.unwrap_or(0.5))
                .with_tags(item.tags.unwrap_or_default());

            if let Some(page) = page_number {
                finding = finding.with_location(&format!("p. {}", page));
            }

            findings.push(finding);
        }

        Ok(findings)
    }

    /// Generate paper outline
    pub async fn generate_outline(&mut self, thesis_hint: Option<&str>) -> Result<Vec<PaperSection>, String> {
        if !self.ai_manager.is_available().await {
            return Err("AI service not available".to_string());
        }

        use crate::llm_tasks::prompts;

        // Build context for outline generation
        let findings_summary = self.summarize_findings();
        let sources_summary = self.summarize_sources();

        let system_prompt = format!(
            "{}\n\nYou are creating a research paper outline. \
             Generate a logical structure that:\n\
             - Addresses the research question thoroughly\n\
             - Has clear transitions between sections\n\
             - Integrates the available evidence effectively",
            prompts::RESEARCHER
        );

        let thesis_instruction = thesis_hint
            .map(|t| format!("\nProposed thesis: {}", t))
            .unwrap_or_default();

        let paper_type_instruction = format!(
            "Paper type: {:?} - use appropriate section structure",
            self.paper.paper_type
        );

        let user_prompt = format!(
            "Create an outline for a research paper.\n\n\
             Research Question: {}\n\
             {}\n\
             {}\n\n\
             Available Sources ({} total):\n{}\n\n\
             Key Findings ({} total):\n{}\n\n\
             Respond with JSON:\n\
             ```json\n\
             {{\n\
               \"thesis\": \"the main thesis statement\",\n\
               \"sections\": [\n\
                 {{\n\
                   \"title\": \"Section Title\",\n\
                   \"type\": \"abstract|introduction|literature_review|methodology|results|discussion|conclusion|custom\",\n\
                   \"description\": \"what this section will cover\",\n\
                   \"key_points\": [\"point 1\", \"point 2\"],\n\
                   \"target_words\": 500\n\
                 }}\n\
               ]\n\
             }}\n\
             ```",
            self.paper.research_question,
            thesis_instruction,
            paper_type_instruction,
            self.sources.len(),
            sources_summary,
            self.paper.findings.len(),
            findings_summary
        );

        let response = self.ai_manager
            .generate(&user_prompt, Some(&system_prompt))
            .await
            .map_err(|e| format!("Failed to generate outline: {}", e))?;

        // Parse outline response
        let (thesis, sections) = self.parse_outline_response(&response.content)?;

        // Update paper
        self.paper.thesis = Some(thesis);
        self.paper.sections = sections.clone();
        self.paper.status = PaperStatus::Outlined;

        // Save
        self.store.save(&self.paper).await?;

        Ok(sections)
    }

    /// Parse outline from LLM response
    fn parse_outline_response(&self, response: &str) -> Result<(String, Vec<PaperSection>), String> {
        use crate::llm_tasks::extract_json;

        #[derive(serde::Deserialize)]
        struct OutlineResponse {
            thesis: String,
            sections: Vec<SectionItem>,
        }

        #[derive(serde::Deserialize)]
        struct SectionItem {
            title: String,
            #[serde(rename = "type")]
            section_type: String,
            description: Option<String>,
            key_points: Option<Vec<String>>,
            target_words: Option<usize>,
        }

        let parsed: OutlineResponse = extract_json(response)
            .map_err(|e| format!("Failed to parse outline JSON: {}", e))?;

        let mut sections = Vec::new();

        for (order, item) in parsed.sections.into_iter().enumerate() {
            let section_type = match item.section_type.to_lowercase().as_str() {
                "abstract" => SectionType::Abstract,
                "introduction" => SectionType::Introduction,
                "literature_review" => SectionType::LiteratureReview,
                "methodology" => SectionType::Methodology,
                "results" => SectionType::Results,
                "discussion" => SectionType::Discussion,
                "conclusion" => SectionType::Conclusion,
                other => SectionType::Custom(other.to_string()),
            };

            let mut section = PaperSection::new(&item.title, section_type, order);
            section.target_word_count = item.target_words;

            sections.push(section);
        }

        Ok((parsed.thesis, sections))
    }

    /// Write a single section
    pub async fn write_section(&mut self, section_id: &str) -> Result<PaperSection, String> {
        if !self.ai_manager.is_available().await {
            return Err("AI service not available".to_string());
        }

        // Find the section
        let section_idx = self.paper.sections.iter().position(|s| s.id == section_id)
            .ok_or("Section not found")?;

        // Mark as generating
        self.paper.sections[section_idx].status = SectionStatus::Generating;

        // Build context
        let section = &self.paper.sections[section_idx];
        let previous_sections = self.get_previous_sections_summary(section_idx);
        let relevant_findings = self.get_relevant_findings_for_section(section);

        use crate::llm_tasks::prompts;

        let system_prompt = format!(
            "{}\n\nYou are writing a section of a research paper. \
             Use [[source_id:page]] format for citations.\n\
             Write in an academic style appropriate for the paper type.",
            prompts::RESEARCHER
        );

        let user_prompt = format!(
            "Write the \"{}\" section for this paper.\n\n\
             Research Question: {}\n\
             Thesis: {}\n\n\
             Previous Sections Summary:\n{}\n\n\
             Relevant Findings to Incorporate:\n{}\n\n\
             Target Word Count: {} words\n\n\
             Section Type: {:?}\n\n\
             Write the section content. Use [[source_id:page]] format for citations.",
            section.title,
            self.paper.research_question,
            self.paper.thesis.as_deref().unwrap_or("(not yet defined)"),
            previous_sections,
            relevant_findings,
            section.target_word_count.unwrap_or(500),
            section.section_type
        );

        let response = self.ai_manager
            .generate(&user_prompt, Some(&system_prompt))
            .await
            .map_err(|e| format!("Failed to write section: {}", e))?;

        // Update section content
        self.paper.sections[section_idx].set_content(&response.content);
        self.paper.sections[section_idx].status = SectionStatus::Written;

        // Parse citations from content
        let tracker = CitationTracker::new(self.paper.citation_style);
        let citations = tracker.parse_citations(&response.content);
        self.paper.sections[section_idx].citations = citations;

        // Update paper status
        self.paper.update_status();

        // Save
        self.store.save(&self.paper).await?;

        Ok(self.paper.sections[section_idx].clone())
    }

    /// Write all pending sections
    pub async fn write_all_sections(&mut self) -> Result<WritingProgress, String> {
        let pending_ids: Vec<String> = self.paper.sections.iter()
            .filter(|s| s.status == SectionStatus::Pending)
            .map(|s| s.id.clone())
            .collect();

        let mut progress = WritingProgress::new(pending_ids.len());

        for section_id in pending_ids {
            progress.current_section = Some(section_id.clone());

            let section = self.write_section(&section_id).await?;

            progress.written_sections += 1;
            progress.total_words += section.word_count;
        }

        progress.complete = true;
        progress.current_section = None;

        Ok(progress)
    }

    /// Review a section
    pub async fn review_section(&mut self, section_id: &str, focus_areas: Option<Vec<String>>) -> Result<ReviewResult, String> {
        if !self.ai_manager.is_available().await {
            return Err("AI service not available".to_string());
        }

        let section = self.paper.get_section(section_id)
            .ok_or("Section not found")?
            .clone();

        use crate::llm_tasks::prompts;

        let focus_instruction = focus_areas
            .map(|areas| format!("\nFocus especially on: {}", areas.join(", ")))
            .unwrap_or_default();

        let system_prompt = format!(
            "{}\n\nYou are reviewing a research paper section for quality. \
             Identify issues and provide constructive suggestions.",
            prompts::ANALYST
        );

        let user_prompt = format!(
            "Review this paper section for quality.\n\n\
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
                   \"type\": \"citation_needed|unclear_argument|missing_evidence|flow_issue|grammar|factual_concern|length_issue\",\n\
                   \"description\": \"description of the issue\",\n\
                   \"location\": \"approximate location in text (optional)\",\n\
                   \"severity\": \"low|medium|high\"\n\
                 }}\n\
               ],\n\
               \"suggestions\": [\"suggestion 1\", \"suggestion 2\"],\n\
               \"passes\": true/false\n\
             }}\n\
             ```",
            section.title,
            self.paper.research_question,
            self.paper.thesis.as_deref().unwrap_or("(not defined)"),
            focus_instruction,
            section.content
        );

        let response = self.ai_manager
            .generate(&user_prompt, Some(&system_prompt))
            .await
            .map_err(|e| format!("Failed to review section: {}", e))?;

        // Parse review response
        let mut result = self.parse_review_response(&response.content, section_id)?;

        // Add review notes to section
        if let Some(section) = self.paper.get_section_mut(section_id) {
            for issue in &result.issues {
                use super::paper::{ReviewNote, ReviewIssueType};

                let issue_type = match issue.issue_type.as_str() {
                    "citation_needed" => ReviewIssueType::CitationNeeded,
                    "unclear_argument" => ReviewIssueType::UnclearArgument,
                    "missing_evidence" => ReviewIssueType::MissingEvidence,
                    "flow_issue" => ReviewIssueType::FlowIssue,
                    "grammar" => ReviewIssueType::Grammar,
                    "factual_concern" => ReviewIssueType::FactualConcern,
                    "length_issue" => ReviewIssueType::LengthIssue,
                    _ => ReviewIssueType::Suggestion,
                };

                let note = ReviewNote::new(issue_type, &issue.description);
                section.add_review_note(note);
            }

            if result.passes {
                section.status = SectionStatus::Reviewed;
            }
        }

        // Save
        self.store.save(&self.paper).await?;

        Ok(result)
    }

    /// Parse review response
    fn parse_review_response(&self, response: &str, section_id: &str) -> Result<ReviewResult, String> {
        use crate::llm_tasks::extract_json;

        #[derive(serde::Deserialize)]
        struct ReviewResponse {
            quality_score: f32,
            issues: Vec<IssueItem>,
            suggestions: Vec<String>,
            passes: bool,
        }

        #[derive(serde::Deserialize)]
        struct IssueItem {
            #[serde(rename = "type")]
            issue_type: String,
            description: String,
            location: Option<String>,
            severity: String,
        }

        let parsed: ReviewResponse = extract_json(response)
            .map_err(|e| format!("Failed to parse review JSON: {}", e))?;

        Ok(ReviewResult {
            section_id: section_id.to_string(),
            quality_score: parsed.quality_score,
            issues: parsed.issues.into_iter().map(|i| ReviewIssue {
                issue_type: i.issue_type,
                description: i.description,
                location: i.location,
                severity: i.severity,
            }).collect(),
            suggestions: parsed.suggestions,
            passes: parsed.passes,
        })
    }

    /// Update a section with new content
    pub async fn update_section(&mut self, section_id: &str, content: &str) -> Result<bool, String> {
        let updated = self.paper.update_section_content(section_id, content);
        if updated {
            self.store.save(&self.paper).await?;
        }
        Ok(updated)
    }

    /// Finalize the paper (process citations, generate bibliography)
    pub async fn finalize(&mut self) -> Result<(), String> {
        // Process citations for all sections
        process_paper_citations(&mut self.paper, self.sources.clone());

        // Update status
        self.paper.status = PaperStatus::Complete;

        // Save
        self.store.save(&self.paper).await?;

        Ok(())
    }

    // === Helper methods ===

    fn summarize_findings(&self) -> String {
        self.paper.findings.iter()
            .take(20) // Limit for prompt size
            .map(|f| format!("- [{}] {} (from {})",
                format!("{:?}", f.finding_type).to_lowercase(),
                truncate(&f.content, 100),
                f.source_id
            ))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn summarize_sources(&self) -> String {
        self.sources.iter()
            .map(|s| format!("- {} ({}): {}",
                s.title,
                s.id,
                s.summary.as_deref().unwrap_or("No summary")
            ))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn get_previous_sections_summary(&self, current_idx: usize) -> String {
        self.paper.sections.iter()
            .take(current_idx)
            .filter(|s| !s.content.is_empty())
            .map(|s| format!("## {}\n{}", s.title, truncate(&s.content, 200)))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn get_relevant_findings_for_section(&self, section: &PaperSection) -> String {
        // Filter findings based on section type
        let type_keywords: Vec<&str> = match &section.section_type {
            SectionType::Introduction => vec!["background", "context", "overview"],
            SectionType::LiteratureReview => vec!["previous", "research", "study"],
            SectionType::Methodology => vec!["method", "approach", "procedure"],
            SectionType::Results => vec!["finding", "result", "data"],
            SectionType::Discussion => vec!["implication", "significance", "comparison"],
            SectionType::Conclusion => vec!["conclusion", "summary", "future"],
            _ => vec![],
        };

        self.paper.findings.iter()
            .filter(|f| !f.used)
            .take(10)
            .map(|f| format!("- [{}:{}] {} (relevance: {:.1})",
                f.source_id,
                f.location.as_deref().unwrap_or(""),
                truncate(&f.content, 150),
                f.relevance
            ))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests would require mocking AI and semantic search
}
