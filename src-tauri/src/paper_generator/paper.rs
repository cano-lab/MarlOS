//! Paper Data Structures
//!
//! Core types for representing research papers, sections, and findings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::providers::research::CitationStyle;

// ============================================================================
// Paper Status
// ============================================================================

/// Status of a paper in the generation pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperStatus {
    /// Initial state, sources being gathered
    Draft,
    /// Sources have been chunked and analyzed
    Analyzed,
    /// Outline has been generated
    Outlined,
    /// Sections are being written
    Writing,
    /// All sections written, under review
    Review,
    /// Paper is complete
    Complete,
}

impl Default for PaperStatus {
    fn default() -> Self {
        PaperStatus::Draft
    }
}

// ============================================================================
// Section Status
// ============================================================================

/// Status of an individual section
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionStatus {
    /// Section defined but not written
    Pending,
    /// Section is currently being generated
    Generating,
    /// Section has content
    Written,
    /// Section has been reviewed
    Reviewed,
    /// Section is finalized
    Final,
}

impl Default for SectionStatus {
    fn default() -> Self {
        SectionStatus::Pending
    }
}

// ============================================================================
// Section Type
// ============================================================================

/// Type of paper section
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionType {
    /// Abstract/summary
    Abstract,
    /// Introduction section
    Introduction,
    /// Literature review
    LiteratureReview,
    /// Methodology
    Methodology,
    /// Results/findings
    Results,
    /// Analysis/discussion
    Discussion,
    /// Conclusion
    Conclusion,
    /// Custom section type
    Custom(String),
}

impl SectionType {
    /// Get display name for the section type
    pub fn display_name(&self) -> &str {
        match self {
            SectionType::Abstract => "Abstract",
            SectionType::Introduction => "Introduction",
            SectionType::LiteratureReview => "Literature Review",
            SectionType::Methodology => "Methodology",
            SectionType::Results => "Results",
            SectionType::Discussion => "Discussion",
            SectionType::Conclusion => "Conclusion",
            SectionType::Custom(name) => name,
        }
    }
}

// ============================================================================
// Paper Type
// ============================================================================

/// Type of research paper
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperType {
    /// Standard research paper
    ResearchPaper,
    /// Literature review
    LiteratureReview,
    /// Argumentative essay
    ArgumentativeEssay,
    /// Expository essay
    ExpositoryEssay,
    /// Case study
    CaseStudy,
    /// Technical report
    TechnicalReport,
    /// Thesis/dissertation
    Thesis,
    /// Custom type
    Custom(String),
}

impl Default for PaperType {
    fn default() -> Self {
        PaperType::ResearchPaper
    }
}

impl PaperType {
    /// Get standard sections for this paper type
    pub fn standard_sections(&self) -> Vec<SectionType> {
        match self {
            PaperType::ResearchPaper => vec![
                SectionType::Abstract,
                SectionType::Introduction,
                SectionType::LiteratureReview,
                SectionType::Methodology,
                SectionType::Results,
                SectionType::Discussion,
                SectionType::Conclusion,
            ],
            PaperType::LiteratureReview => vec![
                SectionType::Abstract,
                SectionType::Introduction,
                SectionType::LiteratureReview,
                SectionType::Discussion,
                SectionType::Conclusion,
            ],
            PaperType::ArgumentativeEssay => vec![
                SectionType::Introduction,
                SectionType::Custom("Arguments".to_string()),
                SectionType::Custom("Counterarguments".to_string()),
                SectionType::Conclusion,
            ],
            PaperType::ExpositoryEssay => vec![
                SectionType::Introduction,
                SectionType::Custom("Body".to_string()),
                SectionType::Conclusion,
            ],
            PaperType::CaseStudy => vec![
                SectionType::Abstract,
                SectionType::Introduction,
                SectionType::Custom("Background".to_string()),
                SectionType::Methodology,
                SectionType::Results,
                SectionType::Discussion,
                SectionType::Conclusion,
            ],
            PaperType::TechnicalReport => vec![
                SectionType::Abstract,
                SectionType::Introduction,
                SectionType::Methodology,
                SectionType::Results,
                SectionType::Discussion,
                SectionType::Conclusion,
            ],
            PaperType::Thesis => vec![
                SectionType::Abstract,
                SectionType::Introduction,
                SectionType::LiteratureReview,
                SectionType::Methodology,
                SectionType::Results,
                SectionType::Discussion,
                SectionType::Conclusion,
            ],
            PaperType::Custom(_) => vec![
                SectionType::Introduction,
                SectionType::Custom("Body".to_string()),
                SectionType::Conclusion,
            ],
        }
    }
}

// ============================================================================
// Key Finding
// ============================================================================

/// A key finding extracted from a source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFinding {
    /// Unique finding ID
    pub id: String,
    /// Type of finding
    pub finding_type: FindingType,
    /// The actual content (quote, data point, or claim)
    pub content: String,
    /// Source ID this finding came from
    pub source_id: String,
    /// Page or location reference (if available)
    pub location: Option<String>,
    /// Relevance score (0.0 - 1.0)
    pub relevance: f32,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Whether this has been used in the paper
    pub used: bool,
    /// Which section used this finding (if any)
    pub used_in_section: Option<String>,
}

impl KeyFinding {
    pub fn new(finding_type: FindingType, content: &str, source_id: &str) -> Self {
        Self {
            id: format!("find_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string()),
            finding_type,
            content: content.to_string(),
            source_id: source_id.to_string(),
            location: None,
            relevance: 0.5,
            tags: Vec::new(),
            used: false,
            used_in_section: None,
        }
    }

    pub fn with_location(mut self, location: &str) -> Self {
        self.location = Some(location.to_string());
        self
    }

    pub fn with_relevance(mut self, relevance: f32) -> Self {
        self.relevance = relevance.clamp(0.0, 1.0);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// Type of key finding
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingType {
    /// Direct quote from source
    Quote,
    /// Statistical data or measurement
    Data,
    /// Claim or assertion made
    Claim,
    /// Definition of a term
    Definition,
    /// Example or case
    Example,
    /// Methodology description
    Method,
    /// Conclusion or implication
    Conclusion,
}

// ============================================================================
// Paper Section
// ============================================================================

/// A section of the paper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperSection {
    /// Unique section ID
    pub id: String,
    /// Section title
    pub title: String,
    /// Section type
    pub section_type: SectionType,
    /// Order in the paper (0-indexed)
    pub order: usize,
    /// Section content (raw with citation markers)
    pub content: String,
    /// Rendered content (citations resolved)
    pub rendered_content: Option<String>,
    /// Citations used in this section [[source_id:page]]
    pub citations: Vec<Citation>,
    /// Key findings used in this section
    pub findings_used: Vec<String>,
    /// Section status
    pub status: SectionStatus,
    /// Word count
    pub word_count: usize,
    /// Target word count (if specified)
    pub target_word_count: Option<usize>,
    /// Review notes
    pub review_notes: Vec<ReviewNote>,
    /// When section was last modified
    pub modified_at: DateTime<Utc>,
}

impl PaperSection {
    pub fn new(title: &str, section_type: SectionType, order: usize) -> Self {
        Self {
            id: format!("sec_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string()),
            title: title.to_string(),
            section_type,
            order,
            content: String::new(),
            rendered_content: None,
            citations: Vec::new(),
            findings_used: Vec::new(),
            status: SectionStatus::Pending,
            word_count: 0,
            target_word_count: None,
            review_notes: Vec::new(),
            modified_at: Utc::now(),
        }
    }

    /// Update content and recalculate word count
    pub fn set_content(&mut self, content: &str) {
        self.content = content.to_string();
        self.word_count = content.split_whitespace().count();
        self.modified_at = Utc::now();
        self.rendered_content = None; // Clear rendered content
    }

    /// Add a review note
    pub fn add_review_note(&mut self, note: ReviewNote) {
        self.review_notes.push(note);
    }
}

/// A citation reference in the paper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    /// Source ID being cited
    pub source_id: String,
    /// Page or location (optional)
    pub page: Option<String>,
    /// Citation number in the paper (assigned during rendering)
    pub number: Option<usize>,
    /// The formatted citation text (set during rendering)
    pub formatted: Option<String>,
}

impl Citation {
    pub fn new(source_id: &str, page: Option<&str>) -> Self {
        Self {
            source_id: source_id.to_string(),
            page: page.map(|s| s.to_string()),
            number: None,
            formatted: None,
        }
    }
}

/// A review note on a section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewNote {
    /// Note ID
    pub id: String,
    /// Type of issue
    pub issue_type: ReviewIssueType,
    /// Description of the issue
    pub description: String,
    /// Suggested fix
    pub suggestion: Option<String>,
    /// Whether this has been addressed
    pub resolved: bool,
    /// When the note was created
    pub created_at: DateTime<Utc>,
}

impl ReviewNote {
    pub fn new(issue_type: ReviewIssueType, description: &str) -> Self {
        Self {
            id: format!("note_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string()),
            issue_type,
            description: description.to_string(),
            suggestion: None,
            resolved: false,
            created_at: Utc::now(),
        }
    }

    pub fn with_suggestion(mut self, suggestion: &str) -> Self {
        self.suggestion = Some(suggestion.to_string());
        self
    }
}

/// Type of review issue
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewIssueType {
    /// Citation needed
    CitationNeeded,
    /// Unclear argument
    UnclearArgument,
    /// Missing evidence
    MissingEvidence,
    /// Flow/transition issue
    FlowIssue,
    /// Grammar/style issue
    Grammar,
    /// Factual concern
    FactualConcern,
    /// Too long/short
    LengthIssue,
    /// General suggestion
    Suggestion,
}

// ============================================================================
// Paper
// ============================================================================

/// A research paper being generated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paper {
    /// Unique paper ID
    pub id: String,
    /// Paper title
    pub title: String,
    /// Research question
    pub research_question: String,
    /// Thesis statement (may be generated)
    pub thesis: Option<String>,
    /// Paper type
    pub paper_type: PaperType,
    /// Citation style
    pub citation_style: CitationStyle,
    /// Source IDs included in this paper
    pub source_ids: Vec<String>,
    /// Key findings extracted from sources
    pub findings: Vec<KeyFinding>,
    /// Paper sections
    pub sections: Vec<PaperSection>,
    /// Paper status
    pub status: PaperStatus,
    /// Abstract (special section, often required separately)
    pub abstract_text: Option<String>,
    /// Bibliography (generated)
    pub bibliography: Vec<String>,
    /// Tags for organization
    pub tags: Vec<String>,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// When paper was created
    pub created_at: DateTime<Utc>,
    /// When paper was last modified
    pub modified_at: DateTime<Utc>,
}

impl Paper {
    pub fn new(title: &str, research_question: &str) -> Self {
        Self {
            id: format!("paper_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..12].to_string()),
            title: title.to_string(),
            research_question: research_question.to_string(),
            thesis: None,
            paper_type: PaperType::default(),
            citation_style: CitationStyle::default(),
            source_ids: Vec::new(),
            findings: Vec::new(),
            sections: Vec::new(),
            status: PaperStatus::Draft,
            abstract_text: None,
            bibliography: Vec::new(),
            tags: Vec::new(),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
        }
    }

    /// Builder: set paper type
    pub fn with_type(mut self, paper_type: PaperType) -> Self {
        self.paper_type = paper_type;
        self
    }

    /// Builder: set citation style
    pub fn with_citation_style(mut self, style: CitationStyle) -> Self {
        self.citation_style = style;
        self
    }

    /// Builder: set thesis
    pub fn with_thesis(mut self, thesis: &str) -> Self {
        self.thesis = Some(thesis.to_string());
        self
    }

    /// Add a source to the paper
    pub fn add_source(&mut self, source_id: &str) {
        if !self.source_ids.contains(&source_id.to_string()) {
            self.source_ids.push(source_id.to_string());
            self.modified_at = Utc::now();
        }
    }

    /// Add multiple sources
    pub fn add_sources(&mut self, source_ids: &[String]) {
        for id in source_ids {
            self.add_source(id);
        }
    }

    /// Add a key finding
    pub fn add_finding(&mut self, finding: KeyFinding) {
        self.findings.push(finding);
        self.modified_at = Utc::now();
    }

    /// Add a section
    pub fn add_section(&mut self, section: PaperSection) {
        self.sections.push(section);
        self.sections.sort_by_key(|s| s.order);
        self.modified_at = Utc::now();
    }

    /// Get section by ID
    pub fn get_section(&self, section_id: &str) -> Option<&PaperSection> {
        self.sections.iter().find(|s| s.id == section_id)
    }

    /// Get mutable section by ID
    pub fn get_section_mut(&mut self, section_id: &str) -> Option<&mut PaperSection> {
        self.sections.iter_mut().find(|s| s.id == section_id)
    }

    /// Update section content
    pub fn update_section_content(&mut self, section_id: &str, content: &str) -> bool {
        if let Some(section) = self.get_section_mut(section_id) {
            section.set_content(content);
            section.status = SectionStatus::Written;
            self.modified_at = Utc::now();
            true
        } else {
            false
        }
    }

    /// Get total word count
    pub fn total_word_count(&self) -> usize {
        self.sections.iter().map(|s| s.word_count).sum()
    }

    /// Get unused findings
    pub fn unused_findings(&self) -> Vec<&KeyFinding> {
        self.findings.iter().filter(|f| !f.used).collect()
    }

    /// Get findings for a specific source
    pub fn findings_for_source(&self, source_id: &str) -> Vec<&KeyFinding> {
        self.findings.iter().filter(|f| f.source_id == source_id).collect()
    }

    /// Get sections that are pending
    pub fn pending_sections(&self) -> Vec<&PaperSection> {
        self.sections.iter().filter(|s| s.status == SectionStatus::Pending).collect()
    }

    /// Initialize standard sections based on paper type
    pub fn initialize_standard_sections(&mut self) {
        let section_types = self.paper_type.standard_sections();
        for (order, section_type) in section_types.into_iter().enumerate() {
            let title = section_type.display_name().to_string();
            let section = PaperSection::new(&title, section_type, order);
            self.add_section(section);
        }
    }

    /// Update status based on current state
    pub fn update_status(&mut self) {
        let all_written = self.sections.iter().all(|s| {
            matches!(s.status, SectionStatus::Written | SectionStatus::Reviewed | SectionStatus::Final)
        });
        let all_reviewed = self.sections.iter().all(|s| {
            matches!(s.status, SectionStatus::Reviewed | SectionStatus::Final)
        });
        let all_final = self.sections.iter().all(|s| s.status == SectionStatus::Final);

        if all_final {
            self.status = PaperStatus::Complete;
        } else if all_reviewed {
            self.status = PaperStatus::Review;
        } else if all_written {
            self.status = PaperStatus::Review;
        } else if !self.sections.is_empty() && self.sections.iter().any(|s| s.status != SectionStatus::Pending) {
            self.status = PaperStatus::Writing;
        } else if !self.sections.is_empty() {
            self.status = PaperStatus::Outlined;
        } else if !self.findings.is_empty() {
            self.status = PaperStatus::Analyzed;
        }

        self.modified_at = Utc::now();
    }
}

// ============================================================================
// Frontend-Friendly View Types
// ============================================================================

/// Paper view for frontend (without large content fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperView {
    pub id: String,
    pub title: String,
    pub research_question: String,
    pub thesis: Option<String>,
    pub paper_type: PaperType,
    pub citation_style: CitationStyle,
    pub source_count: usize,
    pub finding_count: usize,
    pub section_count: usize,
    pub status: PaperStatus,
    pub word_count: usize,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

impl From<&Paper> for PaperView {
    fn from(paper: &Paper) -> Self {
        Self {
            id: paper.id.clone(),
            title: paper.title.clone(),
            research_question: paper.research_question.clone(),
            thesis: paper.thesis.clone(),
            paper_type: paper.paper_type.clone(),
            citation_style: paper.citation_style,
            source_count: paper.source_ids.len(),
            finding_count: paper.findings.len(),
            section_count: paper.sections.len(),
            status: paper.status,
            word_count: paper.total_word_count(),
            tags: paper.tags.clone(),
            created_at: paper.created_at,
            modified_at: paper.modified_at,
        }
    }
}

/// Section view for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionView {
    pub id: String,
    pub title: String,
    pub section_type: SectionType,
    pub order: usize,
    pub status: SectionStatus,
    pub word_count: usize,
    pub target_word_count: Option<usize>,
    pub citation_count: usize,
    pub review_note_count: usize,
    pub unresolved_notes: usize,
}

impl From<&PaperSection> for SectionView {
    fn from(section: &PaperSection) -> Self {
        Self {
            id: section.id.clone(),
            title: section.title.clone(),
            section_type: section.section_type.clone(),
            order: section.order,
            status: section.status,
            word_count: section.word_count,
            target_word_count: section.target_word_count,
            citation_count: section.citations.len(),
            review_note_count: section.review_notes.len(),
            unresolved_notes: section.review_notes.iter().filter(|n| !n.resolved).count(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paper_creation() {
        let paper = Paper::new("Test Paper", "What is the impact of X on Y?")
            .with_type(PaperType::ResearchPaper)
            .with_citation_style(CitationStyle::APA);

        assert!(paper.id.starts_with("paper_"));
        assert_eq!(paper.title, "Test Paper");
        assert_eq!(paper.status, PaperStatus::Draft);
    }

    #[test]
    fn test_paper_sections() {
        let mut paper = Paper::new("Test", "Question?");
        paper.initialize_standard_sections();

        assert!(!paper.sections.is_empty());
        assert_eq!(paper.sections[0].order, 0);
    }

    #[test]
    fn test_section_word_count() {
        let mut section = PaperSection::new("Test", SectionType::Introduction, 0);
        section.set_content("This is a test with ten words in it here.");
        assert_eq!(section.word_count, 10);
    }

    #[test]
    fn test_key_finding() {
        let finding = KeyFinding::new(FindingType::Quote, "This is a quote", "src_123")
            .with_location("p. 42")
            .with_relevance(0.9);

        assert!(finding.id.starts_with("find_"));
        assert_eq!(finding.location, Some("p. 42".to_string()));
        assert_eq!(finding.relevance, 0.9);
    }

    #[test]
    fn test_paper_view() {
        let mut paper = Paper::new("Test", "Question?");
        paper.add_source("src_1");
        paper.add_source("src_2");
        paper.add_finding(KeyFinding::new(FindingType::Quote, "quote", "src_1"));

        let view = PaperView::from(&paper);
        assert_eq!(view.source_count, 2);
        assert_eq!(view.finding_count, 1);
    }
}
