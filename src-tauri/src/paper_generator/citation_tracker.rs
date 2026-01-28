//! Citation Tracker
//!
//! Tracks citations in paper content and converts them to proper formats.
//! Citation markers use the format: [[source_id:page]]

use regex::Regex;
use std::collections::HashMap;

use crate::providers::research::{CitationStyle, CitationGenerator, Source};
use super::{Citation, Paper};

// ============================================================================
// Citation Tracker
// ============================================================================

/// Tracks and manages citations within a paper
pub struct CitationTracker {
    /// Citation style to use
    style: CitationStyle,
    /// Mapping from source_id to citation number
    citation_numbers: HashMap<String, usize>,
    /// Counter for assigning citation numbers
    next_number: usize,
    /// Sources indexed by ID
    sources: HashMap<String, Source>,
}

impl CitationTracker {
    pub fn new(style: CitationStyle) -> Self {
        Self {
            style,
            citation_numbers: HashMap::new(),
            next_number: 1,
            sources: HashMap::new(),
        }
    }

    /// Add a source for citation tracking
    pub fn add_source(&mut self, source: Source) {
        self.sources.insert(source.id.clone(), source);
    }

    /// Add multiple sources
    pub fn add_sources(&mut self, sources: Vec<Source>) {
        for source in sources {
            self.add_source(source);
        }
    }

    /// Parse citation markers from content
    /// Returns list of citations found: [[source_id:page]] or [[source_id]]
    pub fn parse_citations(&self, content: &str) -> Vec<Citation> {
        let citation_re = Regex::new(r"\[\[([^\]:]+)(?::([^\]]+))?\]\]").unwrap();

        let mut citations = Vec::new();
        for cap in citation_re.captures_iter(content) {
            let source_id = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            let page = cap.get(2).map(|m| m.as_str().trim());

            if !source_id.is_empty() {
                citations.push(Citation::new(source_id, page));
            }
        }

        citations
    }

    /// Assign numbers to citations (for numbered citation styles)
    pub fn assign_numbers(&mut self, citations: &mut [Citation]) {
        for citation in citations {
            let number = self.citation_numbers
                .entry(citation.source_id.clone())
                .or_insert_with(|| {
                    let n = self.next_number;
                    self.next_number += 1;
                    n
                });
            citation.number = Some(*number);
        }
    }

    /// Render a single citation in the current style
    pub fn render_citation(&self, citation: &Citation) -> String {
        let source = match self.sources.get(&citation.source_id) {
            Some(s) => s,
            None => return format!("[{}]", citation.source_id),
        };

        match self.style {
            CitationStyle::APA => self.render_apa(source, citation),
            CitationStyle::MLA => self.render_mla(source, citation),
            CitationStyle::Chicago => self.render_chicago(source, citation),
            CitationStyle::Harvard => self.render_harvard(source, citation),
            CitationStyle::IEEE => self.render_ieee(citation),
            CitationStyle::BibTeX => self.render_bibtex(citation),
        }
    }

    /// Render content with citations replaced by formatted citations
    pub fn render_content(&mut self, content: &str) -> String {
        let citation_re = Regex::new(r"\[\[([^\]:]+)(?::([^\]]+))?\]\]").unwrap();

        let result = citation_re.replace_all(content, |caps: &regex::Captures| {
            let source_id = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            let page = caps.get(2).map(|m| m.as_str().trim());

            let mut citation = Citation::new(source_id, page);

            // Assign number if not already assigned
            let number = self.citation_numbers
                .entry(source_id.to_string())
                .or_insert_with(|| {
                    let n = self.next_number;
                    self.next_number += 1;
                    n
                });
            citation.number = Some(*number);

            self.render_citation(&citation)
        });

        result.to_string()
    }

    /// Generate bibliography/references section
    pub fn generate_bibliography(&self) -> Vec<String> {
        // Get citations in order
        let mut ordered: Vec<(&String, &usize)> = self.citation_numbers.iter().collect();
        ordered.sort_by_key(|(_, &num)| num);

        let mut bibliography = Vec::new();

        for (source_id, number) in ordered {
            if let Some(source) = self.sources.get(source_id) {
                let citation = CitationGenerator::generate(source, self.style);
                let entry = match self.style {
                    CitationStyle::IEEE => format!("[{}] {}", number, citation),
                    _ => citation,
                };
                bibliography.push(entry);
            }
        }

        bibliography
    }

    /// Get ordered list of sources used
    pub fn get_used_sources(&self) -> Vec<&Source> {
        let mut ordered: Vec<(&String, &usize)> = self.citation_numbers.iter().collect();
        ordered.sort_by_key(|(_, &num)| num);

        ordered.iter()
            .filter_map(|(id, _)| self.sources.get(*id))
            .collect()
    }

    /// Reset citation numbers (for re-processing)
    pub fn reset_numbers(&mut self) {
        self.citation_numbers.clear();
        self.next_number = 1;
    }

    // === Style-specific rendering ===

    fn render_apa(&self, source: &Source, citation: &Citation) -> String {
        let author = if source.authors.is_empty() {
            source.title.chars().take(20).collect::<String>()
        } else if source.authors.len() == 1 {
            // Last name only
            source.authors[0].split_whitespace().last()
                .unwrap_or(&source.authors[0]).to_string()
        } else if source.authors.len() == 2 {
            let a1 = source.authors[0].split_whitespace().last().unwrap_or(&source.authors[0]);
            let a2 = source.authors[1].split_whitespace().last().unwrap_or(&source.authors[1]);
            format!("{} & {}", a1, a2)
        } else {
            let a1 = source.authors[0].split_whitespace().last().unwrap_or(&source.authors[0]);
            format!("{} et al.", a1)
        };

        let year = source.published_date.as_ref()
            .and_then(|d| d.split('-').next())
            .unwrap_or("n.d.");

        let page_str = citation.page.as_ref()
            .map(|p| format!(", {}", p))
            .unwrap_or_default();

        format!("({}, {}{})", author, year, page_str)
    }

    fn render_mla(&self, source: &Source, citation: &Citation) -> String {
        let author = if source.authors.is_empty() {
            "".to_string()
        } else {
            source.authors[0].split_whitespace().last()
                .unwrap_or(&source.authors[0]).to_string()
        };

        let page_str = citation.page.as_ref()
            .map(|p| format!(" {}", p))
            .unwrap_or_default();

        if author.is_empty() {
            format!("(\"{}\"{}", source.title, page_str)
        } else {
            format!("({}{})", author, page_str)
        }
    }

    fn render_chicago(&self, source: &Source, citation: &Citation) -> String {
        // Chicago uses footnotes, simplified here
        self.render_apa(source, citation)
    }

    fn render_harvard(&self, source: &Source, citation: &Citation) -> String {
        // Harvard is similar to APA
        self.render_apa(source, citation)
    }

    fn render_ieee(&self, citation: &Citation) -> String {
        // IEEE uses numbers
        match citation.number {
            Some(n) => format!("[{}]", n),
            None => format!("[?]"),
        }
    }

    fn render_bibtex(&self, citation: &Citation) -> String {
        // BibTeX citations are handled by LaTeX
        format!("\\cite{{{}}}", citation.source_id)
    }
}

/// Process a paper and render all citations
pub fn process_paper_citations(paper: &mut Paper, sources: Vec<Source>) {
    let mut tracker = CitationTracker::new(paper.citation_style);
    tracker.add_sources(sources);

    // Process each section
    for section in &mut paper.sections {
        // Parse citations from raw content
        let citations = tracker.parse_citations(&section.content);
        section.citations = citations;

        // Render content with formatted citations
        let rendered = tracker.render_content(&section.content);
        section.rendered_content = Some(rendered);
    }

    // Generate bibliography
    paper.bibliography = tracker.generate_bibliography();
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::research::SourceType;

    fn make_test_source(id: &str, title: &str, author: &str) -> Source {
        let mut source = Source::new(title, None);
        source.id = id.to_string();
        source.authors = vec![author.to_string()];
        source.published_date = Some("2024".to_string());
        source
    }

    #[test]
    fn test_parse_citations() {
        let tracker = CitationTracker::new(CitationStyle::APA);
        let content = "This is a claim [[src_123]] and another [[src_456:p. 42]].";

        let citations = tracker.parse_citations(content);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].source_id, "src_123");
        assert_eq!(citations[0].page, None);
        assert_eq!(citations[1].source_id, "src_456");
        assert_eq!(citations[1].page, Some("p. 42".to_string()));
    }

    #[test]
    fn test_render_apa() {
        let mut tracker = CitationTracker::new(CitationStyle::APA);
        tracker.add_source(make_test_source("src_123", "Test Article", "John Smith"));

        let citation = Citation::new("src_123", Some("p. 15"));
        let rendered = tracker.render_citation(&citation);

        assert!(rendered.contains("Smith"));
        assert!(rendered.contains("2024"));
        assert!(rendered.contains("p. 15"));
    }

    #[test]
    fn test_render_ieee() {
        let mut tracker = CitationTracker::new(CitationStyle::IEEE);
        tracker.add_source(make_test_source("src_123", "Test Article", "John Smith"));

        let mut citation = Citation::new("src_123", None);
        citation.number = Some(1);

        let rendered = tracker.render_citation(&citation);
        assert_eq!(rendered, "[1]");
    }

    #[test]
    fn test_render_content() {
        let mut tracker = CitationTracker::new(CitationStyle::APA);
        tracker.add_source(make_test_source("src_123", "Test Article", "John Smith"));

        let content = "This is a claim [[src_123]].";
        let rendered = tracker.render_content(content);

        assert!(rendered.contains("Smith"));
        assert!(rendered.contains("2024"));
        assert!(!rendered.contains("[["));
    }

    #[test]
    fn test_bibliography() {
        let mut tracker = CitationTracker::new(CitationStyle::APA);
        tracker.add_source(make_test_source("src_1", "First Article", "Alice Brown"));
        tracker.add_source(make_test_source("src_2", "Second Article", "Bob White"));

        // Cite in specific order
        tracker.render_content("First [[src_2]] then [[src_1]]");

        let bib = tracker.generate_bibliography();
        assert_eq!(bib.len(), 2);
    }
}
