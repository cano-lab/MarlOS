//! Paper Templates
//!
//! Pre-defined templates for common academic paper formats.
//! Each template specifies structure, section types, and LaTeX export settings.

use serde::{Deserialize, Serialize};
use super::paper::{PaperType, SectionType};
use super::latex_export::LaTeXExportOptions;

/// A paper template defining structure and formatting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTemplate {
    /// Template ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Category (conference, journal, thesis, general)
    pub category: String,
    /// Sections this template includes (in order)
    pub sections: Vec<TemplateSection>,
    /// Suggested paper type
    pub paper_type: PaperType,
    /// LaTeX preset name (for export)
    pub latex_preset: Option<String>,
    /// Suggested word count range
    pub word_count_range: Option<(usize, usize)>,
}

/// A section defined by a template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSection {
    pub title: String,
    pub section_type: SectionType,
    pub description: String,
    pub target_words: Option<usize>,
    pub required: bool,
}

/// Get all available templates
pub fn available_templates() -> Vec<PaperTemplate> {
    vec![
        ieee_conference(),
        acm_sigconf(),
        springer_lncs(),
        apa_manuscript(),
        literature_review(),
        thesis_chapter(),
        technical_report(),
        short_paper(),
    ]
}

/// Get a template by ID
pub fn get_template(id: &str) -> Option<PaperTemplate> {
    available_templates().into_iter().find(|t| t.id == id)
}

fn ieee_conference() -> PaperTemplate {
    PaperTemplate {
        id: "ieee-conference".into(),
        name: "IEEE Conference Paper".into(),
        description: "Standard IEEE conference paper format (6-8 pages, two-column)".into(),
        category: "conference".into(),
        paper_type: PaperType::ResearchPaper,
        latex_preset: Some("ieee".into()),
        word_count_range: Some((4000, 6000)),
        sections: vec![
            TemplateSection {
                title: "Abstract".into(),
                section_type: SectionType::Abstract,
                description: "Brief summary of the paper (150-250 words)".into(),
                target_words: Some(200),
                required: true,
            },
            TemplateSection {
                title: "Introduction".into(),
                section_type: SectionType::Introduction,
                description: "Problem statement, motivation, and contributions".into(),
                target_words: Some(800),
                required: true,
            },
            TemplateSection {
                title: "Related Work".into(),
                section_type: SectionType::LiteratureReview,
                description: "Survey of relevant prior work".into(),
                target_words: Some(600),
                required: true,
            },
            TemplateSection {
                title: "Methodology".into(),
                section_type: SectionType::Methodology,
                description: "Proposed approach and system design".into(),
                target_words: Some(1200),
                required: true,
            },
            TemplateSection {
                title: "Evaluation".into(),
                section_type: SectionType::Results,
                description: "Experimental setup, results, and analysis".into(),
                target_words: Some(1200),
                required: true,
            },
            TemplateSection {
                title: "Conclusion".into(),
                section_type: SectionType::Conclusion,
                description: "Summary and future work".into(),
                target_words: Some(400),
                required: true,
            },
        ],
    }
}

fn acm_sigconf() -> PaperTemplate {
    PaperTemplate {
        id: "acm-sigconf".into(),
        name: "ACM SIGCONF".into(),
        description: "ACM conference proceedings format (10-12 pages)".into(),
        category: "conference".into(),
        paper_type: PaperType::ResearchPaper,
        latex_preset: None,
        word_count_range: Some((6000, 10000)),
        sections: vec![
            TemplateSection {
                title: "Abstract".into(),
                section_type: SectionType::Abstract,
                description: "Concise summary (up to 150 words for CCS)".into(),
                target_words: Some(150),
                required: true,
            },
            TemplateSection {
                title: "Introduction".into(),
                section_type: SectionType::Introduction,
                description: "Motivation, problem, contributions".into(),
                target_words: Some(1000),
                required: true,
            },
            TemplateSection {
                title: "Background".into(),
                section_type: SectionType::Custom("Background".into()),
                description: "Necessary background and definitions".into(),
                target_words: Some(800),
                required: false,
            },
            TemplateSection {
                title: "Related Work".into(),
                section_type: SectionType::LiteratureReview,
                description: "Comparison with existing approaches".into(),
                target_words: Some(800),
                required: true,
            },
            TemplateSection {
                title: "System Design".into(),
                section_type: SectionType::Methodology,
                description: "Architecture and implementation details".into(),
                target_words: Some(2000),
                required: true,
            },
            TemplateSection {
                title: "Evaluation".into(),
                section_type: SectionType::Results,
                description: "Benchmarks, user studies, or formal analysis".into(),
                target_words: Some(2000),
                required: true,
            },
            TemplateSection {
                title: "Discussion".into(),
                section_type: SectionType::Discussion,
                description: "Limitations, threats to validity, broader impact".into(),
                target_words: Some(600),
                required: false,
            },
            TemplateSection {
                title: "Conclusion".into(),
                section_type: SectionType::Conclusion,
                description: "Summary and future directions".into(),
                target_words: Some(400),
                required: true,
            },
        ],
    }
}

fn springer_lncs() -> PaperTemplate {
    PaperTemplate {
        id: "springer-lncs".into(),
        name: "Springer LNCS".into(),
        description: "Springer Lecture Notes in Computer Science (12-15 pages)".into(),
        category: "conference".into(),
        paper_type: PaperType::ResearchPaper,
        latex_preset: None,
        word_count_range: Some((5000, 8000)),
        sections: vec![
            TemplateSection {
                title: "Abstract".into(),
                section_type: SectionType::Abstract,
                description: "Summary of the paper".into(),
                target_words: Some(200),
                required: true,
            },
            TemplateSection {
                title: "Introduction".into(),
                section_type: SectionType::Introduction,
                description: "Context, problem, contributions".into(),
                target_words: Some(1000),
                required: true,
            },
            TemplateSection {
                title: "Preliminaries".into(),
                section_type: SectionType::Custom("Preliminaries".into()),
                description: "Definitions and formal foundations".into(),
                target_words: Some(800),
                required: false,
            },
            TemplateSection {
                title: "Approach".into(),
                section_type: SectionType::Methodology,
                description: "Proposed method or algorithm".into(),
                target_words: Some(1500),
                required: true,
            },
            TemplateSection {
                title: "Experimental Evaluation".into(),
                section_type: SectionType::Results,
                description: "Setup, results, comparison".into(),
                target_words: Some(1500),
                required: true,
            },
            TemplateSection {
                title: "Related Work".into(),
                section_type: SectionType::LiteratureReview,
                description: "Positioning w.r.t. state of the art".into(),
                target_words: Some(600),
                required: true,
            },
            TemplateSection {
                title: "Conclusion".into(),
                section_type: SectionType::Conclusion,
                description: "Summary and outlook".into(),
                target_words: Some(400),
                required: true,
            },
        ],
    }
}

fn apa_manuscript() -> PaperTemplate {
    PaperTemplate {
        id: "apa-manuscript".into(),
        name: "APA 7th Edition Manuscript".into(),
        description: "Standard APA format for psychology/social science journals".into(),
        category: "journal".into(),
        paper_type: PaperType::ResearchPaper,
        latex_preset: Some("apa".into()),
        word_count_range: Some((5000, 8000)),
        sections: vec![
            TemplateSection {
                title: "Abstract".into(),
                section_type: SectionType::Abstract,
                description: "150-250 word summary with keywords".into(),
                target_words: Some(200),
                required: true,
            },
            TemplateSection {
                title: "Introduction".into(),
                section_type: SectionType::Introduction,
                description: "Literature review integrated with rationale".into(),
                target_words: Some(2000),
                required: true,
            },
            TemplateSection {
                title: "Method".into(),
                section_type: SectionType::Methodology,
                description: "Participants, materials, procedure".into(),
                target_words: Some(1500),
                required: true,
            },
            TemplateSection {
                title: "Results".into(),
                section_type: SectionType::Results,
                description: "Statistical analyses and findings".into(),
                target_words: Some(1200),
                required: true,
            },
            TemplateSection {
                title: "Discussion".into(),
                section_type: SectionType::Discussion,
                description: "Interpretation, limitations, implications".into(),
                target_words: Some(1500),
                required: true,
            },
        ],
    }
}

fn literature_review() -> PaperTemplate {
    PaperTemplate {
        id: "literature-review".into(),
        name: "Literature Review".into(),
        description: "Comprehensive review of existing research on a topic".into(),
        category: "general".into(),
        paper_type: PaperType::LiteratureReview,
        latex_preset: None,
        word_count_range: Some((4000, 10000)),
        sections: vec![
            TemplateSection {
                title: "Abstract".into(),
                section_type: SectionType::Abstract,
                description: "Overview of scope and findings".into(),
                target_words: Some(200),
                required: true,
            },
            TemplateSection {
                title: "Introduction".into(),
                section_type: SectionType::Introduction,
                description: "Research question and scope of review".into(),
                target_words: Some(800),
                required: true,
            },
            TemplateSection {
                title: "Search Methodology".into(),
                section_type: SectionType::Methodology,
                description: "Databases, search terms, inclusion/exclusion criteria".into(),
                target_words: Some(600),
                required: true,
            },
            TemplateSection {
                title: "Thematic Analysis".into(),
                section_type: SectionType::LiteratureReview,
                description: "Organized by themes, chronology, or methodology".into(),
                target_words: Some(4000),
                required: true,
            },
            TemplateSection {
                title: "Discussion".into(),
                section_type: SectionType::Discussion,
                description: "Gaps, trends, and future directions".into(),
                target_words: Some(1000),
                required: true,
            },
            TemplateSection {
                title: "Conclusion".into(),
                section_type: SectionType::Conclusion,
                description: "Key takeaways".into(),
                target_words: Some(400),
                required: true,
            },
        ],
    }
}

fn thesis_chapter() -> PaperTemplate {
    PaperTemplate {
        id: "thesis".into(),
        name: "Thesis / Dissertation".into(),
        description: "Full thesis structure with chapters".into(),
        category: "thesis".into(),
        paper_type: PaperType::Thesis,
        latex_preset: Some("thesis".into()),
        word_count_range: Some((20000, 80000)),
        sections: vec![
            TemplateSection {
                title: "Abstract".into(),
                section_type: SectionType::Abstract,
                description: "Thesis summary (300-500 words)".into(),
                target_words: Some(400),
                required: true,
            },
            TemplateSection {
                title: "Introduction".into(),
                section_type: SectionType::Introduction,
                description: "Research context, motivation, objectives, thesis structure".into(),
                target_words: Some(3000),
                required: true,
            },
            TemplateSection {
                title: "Literature Review".into(),
                section_type: SectionType::LiteratureReview,
                description: "Comprehensive survey of the field".into(),
                target_words: Some(8000),
                required: true,
            },
            TemplateSection {
                title: "Methodology".into(),
                section_type: SectionType::Methodology,
                description: "Research design, data collection, tools".into(),
                target_words: Some(5000),
                required: true,
            },
            TemplateSection {
                title: "Results".into(),
                section_type: SectionType::Results,
                description: "Findings and data presentation".into(),
                target_words: Some(5000),
                required: true,
            },
            TemplateSection {
                title: "Discussion".into(),
                section_type: SectionType::Discussion,
                description: "Analysis, interpretation, implications".into(),
                target_words: Some(4000),
                required: true,
            },
            TemplateSection {
                title: "Conclusion".into(),
                section_type: SectionType::Conclusion,
                description: "Summary, contributions, future work".into(),
                target_words: Some(2000),
                required: true,
            },
        ],
    }
}

fn technical_report() -> PaperTemplate {
    PaperTemplate {
        id: "tech-report".into(),
        name: "Technical Report".into(),
        description: "Internal or external technical documentation".into(),
        category: "general".into(),
        paper_type: PaperType::TechnicalReport,
        latex_preset: None,
        word_count_range: Some((3000, 15000)),
        sections: vec![
            TemplateSection {
                title: "Executive Summary".into(),
                section_type: SectionType::Abstract,
                description: "Key findings and recommendations".into(),
                target_words: Some(300),
                required: true,
            },
            TemplateSection {
                title: "Introduction".into(),
                section_type: SectionType::Introduction,
                description: "Problem statement and objectives".into(),
                target_words: Some(500),
                required: true,
            },
            TemplateSection {
                title: "Technical Approach".into(),
                section_type: SectionType::Methodology,
                description: "Methods, tools, and procedures used".into(),
                target_words: Some(2000),
                required: true,
            },
            TemplateSection {
                title: "Results".into(),
                section_type: SectionType::Results,
                description: "Data, measurements, observations".into(),
                target_words: Some(2000),
                required: true,
            },
            TemplateSection {
                title: "Analysis".into(),
                section_type: SectionType::Discussion,
                description: "Interpretation of results".into(),
                target_words: Some(1500),
                required: true,
            },
            TemplateSection {
                title: "Recommendations".into(),
                section_type: SectionType::Conclusion,
                description: "Actionable recommendations".into(),
                target_words: Some(500),
                required: true,
            },
        ],
    }
}

fn short_paper() -> PaperTemplate {
    PaperTemplate {
        id: "short-paper".into(),
        name: "Short Paper / Workshop".into(),
        description: "4-page workshop or short paper format".into(),
        category: "conference".into(),
        paper_type: PaperType::ResearchPaper,
        latex_preset: None,
        word_count_range: Some((2000, 3500)),
        sections: vec![
            TemplateSection {
                title: "Abstract".into(),
                section_type: SectionType::Abstract,
                description: "Brief summary (100 words)".into(),
                target_words: Some(100),
                required: true,
            },
            TemplateSection {
                title: "Introduction".into(),
                section_type: SectionType::Introduction,
                description: "Concise motivation and contribution".into(),
                target_words: Some(500),
                required: true,
            },
            TemplateSection {
                title: "Approach".into(),
                section_type: SectionType::Methodology,
                description: "Core idea and method".into(),
                target_words: Some(800),
                required: true,
            },
            TemplateSection {
                title: "Preliminary Results".into(),
                section_type: SectionType::Results,
                description: "Initial findings".into(),
                target_words: Some(600),
                required: true,
            },
            TemplateSection {
                title: "Conclusion".into(),
                section_type: SectionType::Conclusion,
                description: "Summary and next steps".into(),
                target_words: Some(200),
                required: true,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_templates() {
        let templates = available_templates();
        assert!(templates.len() >= 8);
        assert!(templates.iter().any(|t| t.id == "ieee-conference"));
        assert!(templates.iter().any(|t| t.id == "apa-manuscript"));
    }

    #[test]
    fn test_get_template() {
        let t = get_template("ieee-conference").unwrap();
        assert_eq!(t.name, "IEEE Conference Paper");
        assert!(!t.sections.is_empty());
    }

    #[test]
    fn test_template_sections() {
        let t = get_template("thesis").unwrap();
        assert!(t.sections.len() >= 7);
        assert!(t.sections.iter().all(|s| s.required));
    }
}
