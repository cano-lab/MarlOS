//! Paper Export
//!
//! Export papers to various formats (Markdown, DOCX).

use std::path::Path;

use crate::providers::research::CitationStyle;
use super::{Paper, PaperSection, SectionType, CitationTracker, process_paper_citations};
use crate::providers::research::Source;

// ============================================================================
// Export Format
// ============================================================================

/// Export format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// Markdown format
    Markdown,
    /// HTML format
    Html,
    /// Plain text
    PlainText,
    /// LaTeX format (with companion .bib)
    LaTeX,
    /// DOCX format (requires docx-rs)
    #[cfg(feature = "docx")]
    Docx,
}

impl Default for ExportFormat {
    fn default() -> Self {
        ExportFormat::Markdown
    }
}

// ============================================================================
// Export Options
// ============================================================================

/// Options for paper export
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Export format
    pub format: ExportFormat,
    /// Whether to include title page
    pub include_title_page: bool,
    /// Whether to include table of contents
    pub include_toc: bool,
    /// Whether to include abstract
    pub include_abstract: bool,
    /// Whether to include bibliography
    pub include_bibliography: bool,
    /// Whether to include page numbers (for supported formats)
    pub include_page_numbers: bool,
    /// Whether to include section numbers
    pub number_sections: bool,
    /// Whether to use rendered content (citations resolved) vs raw
    pub use_rendered_content: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: ExportFormat::Markdown,
            include_title_page: true,
            include_toc: true,
            include_abstract: true,
            include_bibliography: true,
            include_page_numbers: true,
            number_sections: true,
            use_rendered_content: true,
        }
    }
}

// ============================================================================
// Paper Exporter
// ============================================================================

/// Exports papers to various formats
pub struct PaperExporter;

impl PaperExporter {
    /// Export a paper to the specified format
    pub fn export(paper: &Paper, sources: &[Source], options: &ExportOptions) -> Result<String, String> {
        // Process citations if using rendered content
        let mut paper_clone = paper.clone();
        if options.use_rendered_content {
            process_paper_citations(&mut paper_clone, sources.to_vec());
        }

        match options.format {
            ExportFormat::Markdown => Self::export_markdown(&paper_clone, options),
            ExportFormat::Html => Self::export_html(&paper_clone, options),
            ExportFormat::PlainText => Self::export_plaintext(&paper_clone, options),
            ExportFormat::LaTeX => {
                // LaTeX export uses its own dedicated function with richer options
                let latex_opts = super::latex_export::LaTeXExportOptions::default();
                let output = super::latex_export::export_latex(&paper_clone, sources, &[], &latex_opts)?;
                Ok(output.tex_content)
            }
            #[cfg(feature = "docx")]
            ExportFormat::Docx => Err("DOCX export not yet implemented".to_string()),
        }
    }

    /// Export to Markdown format
    pub fn export_markdown(paper: &Paper, options: &ExportOptions) -> Result<String, String> {
        let mut output = String::new();

        // Title page
        if options.include_title_page {
            output.push_str(&format!("# {}\n\n", paper.title));

            if let Some(thesis) = &paper.thesis {
                output.push_str(&format!("**Thesis:** {}\n\n", thesis));
            }

            output.push_str(&format!("**Research Question:** {}\n\n", paper.research_question));
            output.push_str("---\n\n");
        }

        // Table of Contents
        if options.include_toc && !paper.sections.is_empty() {
            output.push_str("## Table of Contents\n\n");
            for (i, section) in paper.sections.iter().enumerate() {
                let number = if options.number_sections {
                    format!("{}. ", i + 1)
                } else {
                    String::new()
                };
                // Create anchor link
                let anchor = section.title.to_lowercase().replace(" ", "-");
                output.push_str(&format!("- [{}{}](#{})\n", number, section.title, anchor));
            }
            output.push_str("\n---\n\n");
        }

        // Abstract (if present and requested)
        if options.include_abstract {
            if let Some(abstract_section) = paper.sections.iter()
                .find(|s| matches!(s.section_type, SectionType::Abstract))
            {
                output.push_str("## Abstract\n\n");
                let content = if options.use_rendered_content {
                    abstract_section.rendered_content.as_ref().unwrap_or(&abstract_section.content)
                } else {
                    &abstract_section.content
                };
                output.push_str(content);
                output.push_str("\n\n---\n\n");
            } else if let Some(abstract_text) = &paper.abstract_text {
                output.push_str("## Abstract\n\n");
                output.push_str(abstract_text);
                output.push_str("\n\n---\n\n");
            }
        }

        // Sections
        for (i, section) in paper.sections.iter().enumerate() {
            // Skip abstract if already included
            if options.include_abstract && matches!(section.section_type, SectionType::Abstract) {
                continue;
            }

            let number = if options.number_sections {
                format!("{}. ", i + 1)
            } else {
                String::new()
            };

            output.push_str(&format!("## {}{}\n\n", number, section.title));

            let content = if options.use_rendered_content {
                section.rendered_content.as_ref().unwrap_or(&section.content)
            } else {
                &section.content
            };

            output.push_str(content);
            output.push_str("\n\n");
        }

        // Bibliography
        if options.include_bibliography && !paper.bibliography.is_empty() {
            output.push_str("---\n\n## References\n\n");
            for (i, entry) in paper.bibliography.iter().enumerate() {
                // Check citation style for formatting
                match paper.citation_style {
                    CitationStyle::IEEE => {
                        // IEEE already has numbers in the entries
                        output.push_str(&format!("{}\n\n", entry));
                    }
                    _ => {
                        // Other styles: just list
                        output.push_str(&format!("{}. {}\n\n", i + 1, entry));
                    }
                }
            }
        }

        Ok(output)
    }

    /// Export to HTML format
    pub fn export_html(paper: &Paper, options: &ExportOptions) -> Result<String, String> {
        let mut output = String::new();

        // HTML header
        output.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
        output.push_str(&format!("<title>{}</title>\n", paper.title));
        output.push_str("<style>\n");
        output.push_str(include_str!("paper_style.css"));
        output.push_str("</style>\n");
        output.push_str("</head>\n<body>\n");
        output.push_str("<article class=\"paper\">\n");

        // Title page
        if options.include_title_page {
            output.push_str("<header class=\"title-page\">\n");
            output.push_str(&format!("<h1>{}</h1>\n", paper.title));

            if let Some(thesis) = &paper.thesis {
                output.push_str(&format!("<p class=\"thesis\"><strong>Thesis:</strong> {}</p>\n", thesis));
            }

            output.push_str(&format!("<p class=\"research-question\"><strong>Research Question:</strong> {}</p>\n", paper.research_question));
            output.push_str("</header>\n");
        }

        // Table of Contents
        if options.include_toc && !paper.sections.is_empty() {
            output.push_str("<nav class=\"toc\">\n<h2>Table of Contents</h2>\n<ol>\n");
            for section in &paper.sections {
                let anchor = section.title.to_lowercase().replace(" ", "-");
                output.push_str(&format!("<li><a href=\"#{}\">{}</a></li>\n", anchor, section.title));
            }
            output.push_str("</ol>\n</nav>\n");
        }

        // Sections
        for (i, section) in paper.sections.iter().enumerate() {
            let anchor = section.title.to_lowercase().replace(" ", "-");
            let number = if options.number_sections {
                format!("{}. ", i + 1)
            } else {
                String::new()
            };

            output.push_str(&format!("<section id=\"{}\">\n", anchor));
            output.push_str(&format!("<h2>{}{}</h2>\n", number, section.title));

            let content = if options.use_rendered_content {
                section.rendered_content.as_ref().unwrap_or(&section.content)
            } else {
                &section.content
            };

            // Convert markdown-like content to HTML paragraphs
            for para in content.split("\n\n") {
                if !para.trim().is_empty() {
                    output.push_str(&format!("<p>{}</p>\n", para.trim()));
                }
            }

            output.push_str("</section>\n");
        }

        // Bibliography
        if options.include_bibliography && !paper.bibliography.is_empty() {
            output.push_str("<section class=\"references\">\n<h2>References</h2>\n<ol>\n");
            for entry in &paper.bibliography {
                output.push_str(&format!("<li>{}</li>\n", entry));
            }
            output.push_str("</ol>\n</section>\n");
        }

        // Close HTML
        output.push_str("</article>\n</body>\n</html>");

        Ok(output)
    }

    /// Export to plain text format
    pub fn export_plaintext(paper: &Paper, options: &ExportOptions) -> Result<String, String> {
        let mut output = String::new();

        // Title
        if options.include_title_page {
            output.push_str(&paper.title.to_uppercase());
            output.push_str("\n");
            output.push_str(&"=".repeat(paper.title.len()));
            output.push_str("\n\n");

            if let Some(thesis) = &paper.thesis {
                output.push_str(&format!("Thesis: {}\n\n", thesis));
            }

            output.push_str(&format!("Research Question: {}\n\n", paper.research_question));
            output.push_str(&"-".repeat(40));
            output.push_str("\n\n");
        }

        // Sections
        for (i, section) in paper.sections.iter().enumerate() {
            let number = if options.number_sections {
                format!("{}. ", i + 1)
            } else {
                String::new()
            };

            let header = format!("{}{}", number, section.title);
            output.push_str(&header);
            output.push_str("\n");
            output.push_str(&"-".repeat(header.len()));
            output.push_str("\n\n");

            let content = if options.use_rendered_content {
                section.rendered_content.as_ref().unwrap_or(&section.content)
            } else {
                &section.content
            };

            output.push_str(content);
            output.push_str("\n\n");
        }

        // Bibliography
        if options.include_bibliography && !paper.bibliography.is_empty() {
            output.push_str(&"-".repeat(40));
            output.push_str("\n\nREFERENCES\n\n");
            for (i, entry) in paper.bibliography.iter().enumerate() {
                output.push_str(&format!("[{}] {}\n\n", i + 1, entry));
            }
        }

        Ok(output)
    }

    /// Export to file
    pub fn export_to_file(
        paper: &Paper,
        sources: &[Source],
        path: &Path,
        options: &ExportOptions,
    ) -> Result<usize, String> {
        let content = Self::export(paper, sources, options)?;

        std::fs::write(path, &content)
            .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(content.len())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper_generator::{PaperSection, SectionType, SectionStatus};

    fn create_test_paper() -> Paper {
        let mut paper = Paper::new("Test Paper", "What is the impact of X?");
        paper.thesis = Some("X has significant impact on Y.".to_string());

        let mut intro = PaperSection::new("Introduction", SectionType::Introduction, 0);
        intro.set_content("This paper examines the impact of X on Y.");
        paper.add_section(intro);

        let mut conclusion = PaperSection::new("Conclusion", SectionType::Conclusion, 1);
        conclusion.set_content("In conclusion, X significantly affects Y.");
        paper.add_section(conclusion);

        paper.bibliography = vec![
            "Smith, J. (2024). Test Article.".to_string(),
        ];

        paper
    }

    #[test]
    fn test_markdown_export() {
        let paper = create_test_paper();
        let options = ExportOptions::default();

        let result = PaperExporter::export_markdown(&paper, &options);
        assert!(result.is_ok());

        let markdown = result.unwrap();
        assert!(markdown.contains("# Test Paper"));
        assert!(markdown.contains("## Introduction"));
        assert!(markdown.contains("## References"));
    }

    #[test]
    fn test_html_export() {
        let paper = create_test_paper();
        let options = ExportOptions::default();

        let result = PaperExporter::export_html(&paper, &options);
        assert!(result.is_ok());

        let html = result.unwrap();
        assert!(html.contains("<h1>Test Paper</h1>"));
        assert!(html.contains("<h2>"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn test_plaintext_export() {
        let paper = create_test_paper();
        let options = ExportOptions::default();

        let result = PaperExporter::export_plaintext(&paper, &options);
        assert!(result.is_ok());

        let text = result.unwrap();
        assert!(text.contains("TEST PAPER"));
        assert!(text.contains("REFERENCES"));
    }

    #[test]
    fn test_export_options() {
        let paper = create_test_paper();
        let mut options = ExportOptions::default();
        options.include_bibliography = false;
        options.include_toc = false;

        let result = PaperExporter::export_markdown(&paper, &options);
        assert!(result.is_ok());

        let markdown = result.unwrap();
        assert!(!markdown.contains("## Table of Contents"));
        assert!(!markdown.contains("## References"));
    }
}
