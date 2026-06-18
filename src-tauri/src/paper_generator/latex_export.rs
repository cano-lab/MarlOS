//! LaTeX Export
//!
//! Exports papers to LaTeX format with companion .bib file generation.
//! Supports common document classes and proper citation handling via BibTeX.

use std::collections::HashMap;
use regex::Regex;

use super::{Paper, SectionType};
use crate::providers::research::{CitationStyle, Source};
use crate::reference_library::reference::Reference;

/// LaTeX document class
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaTeXDocumentClass {
    Article,
    Report,
    Book,
}

impl Default for LaTeXDocumentClass {
    fn default() -> Self {
        LaTeXDocumentClass::Article
    }
}

/// Options for LaTeX export
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LaTeXExportOptions {
    /// Document class
    pub document_class: LaTeXDocumentClass,
    /// Font size (10, 11, or 12)
    pub font_size: u8,
    /// Paper size
    pub paper_size: String,
    /// Two-column layout
    pub two_column: bool,
    /// Bibliography style (plain, unsrt, abbrv, alpha, ieeetr, acm)
    pub bib_style: String,
    /// Whether to include table of contents
    pub include_toc: bool,
    /// Whether to number sections
    pub number_sections: bool,
    /// Additional packages to include
    pub extra_packages: Vec<String>,
    /// Line spacing (1.0, 1.5, 2.0)
    pub line_spacing: f32,
}

impl Default for LaTeXExportOptions {
    fn default() -> Self {
        Self {
            document_class: LaTeXDocumentClass::Article,
            font_size: 12,
            paper_size: "a4paper".into(),
            two_column: false,
            bib_style: "plain".into(),
            include_toc: false,
            number_sections: true,
            extra_packages: Vec::new(),
            line_spacing: 1.0,
        }
    }
}

impl LaTeXExportOptions {
    /// Preset for IEEE papers
    pub fn ieee() -> Self {
        Self {
            document_class: LaTeXDocumentClass::Article,
            font_size: 10,
            paper_size: "letterpaper".into(),
            two_column: true,
            bib_style: "ieeetr".into(),
            include_toc: false,
            number_sections: true,
            extra_packages: vec!["cite".into(), "amsmath".into(), "algorithmic".into()],
            line_spacing: 1.0,
        }
    }

    /// Preset for APA papers
    pub fn apa() -> Self {
        Self {
            document_class: LaTeXDocumentClass::Article,
            font_size: 12,
            paper_size: "letterpaper".into(),
            two_column: false,
            bib_style: "apacite".into(),
            include_toc: false,
            number_sections: true,
            extra_packages: vec!["apacite".into()],
            line_spacing: 2.0,
        }
    }

    /// Preset for academic thesis
    pub fn thesis() -> Self {
        Self {
            document_class: LaTeXDocumentClass::Report,
            font_size: 12,
            paper_size: "a4paper".into(),
            two_column: false,
            bib_style: "plain".into(),
            include_toc: true,
            number_sections: true,
            extra_packages: Vec::new(),
            line_spacing: 1.5,
        }
    }
}

/// Export a paper to LaTeX format
pub fn export_latex(
    paper: &Paper,
    sources: &[Source],
    references: &[Reference],
    options: &LaTeXExportOptions,
) -> Result<LaTeXOutput, String> {
    let mut tex = String::new();
    let mut cite_keys: HashMap<String, String> = HashMap::new();

    // Map source IDs to cite keys
    for source in sources {
        let key = source_to_cite_key(source);
        cite_keys.insert(source.id.clone(), key);
    }
    for reference in references {
        if !reference.cite_key.is_empty() {
            cite_keys.insert(reference.id.clone(), reference.cite_key.clone());
        }
    }

    // Document class
    let class_opts = build_class_options(options);
    let class_name = match options.document_class {
        LaTeXDocumentClass::Article => "article",
        LaTeXDocumentClass::Report => "report",
        LaTeXDocumentClass::Book => "book",
    };
    tex.push_str(&format!("\\documentclass[{}]{{{}}}\n\n", class_opts, class_name));

    // Packages
    tex.push_str("% Packages\n");
    tex.push_str("\\usepackage[utf8]{inputenc}\n");
    tex.push_str("\\usepackage[T1]{fontenc}\n");
    tex.push_str("\\usepackage{amsmath,amssymb}\n");
    tex.push_str("\\usepackage{graphicx}\n");
    tex.push_str("\\usepackage{hyperref}\n");
    tex.push_str("\\usepackage{booktabs}\n");
    tex.push_str("\\usepackage{natbib}\n");

    if (options.line_spacing - 1.0).abs() > 0.01 {
        tex.push_str("\\usepackage{setspace}\n");
    }

    for pkg in &options.extra_packages {
        tex.push_str(&format!("\\usepackage{{{}}}\n", pkg));
    }
    tex.push('\n');

    // Line spacing
    if (options.line_spacing - 1.5).abs() < 0.01 {
        tex.push_str("\\onehalfspacing\n");
    } else if (options.line_spacing - 2.0).abs() < 0.01 {
        tex.push_str("\\doublespacing\n");
    }

    // Title info
    tex.push_str(&format!("\\title{{{}}}\n", escape_latex(&paper.title)));
    tex.push_str("\\author{}\n");
    tex.push_str("\\date{}\n\n");

    // Begin document
    tex.push_str("\\begin{document}\n\n");
    tex.push_str("\\maketitle\n\n");

    // Table of contents
    if options.include_toc {
        tex.push_str("\\tableofcontents\n\\newpage\n\n");
    }

    // Abstract
    if let Some(abstract_section) = paper.sections.iter()
        .find(|s| matches!(s.section_type, SectionType::Abstract))
    {
        tex.push_str("\\begin{abstract}\n");
        tex.push_str(&markdown_to_latex(&abstract_section.content, &cite_keys));
        tex.push_str("\n\\end{abstract}\n\n");
    } else if let Some(abstract_text) = &paper.abstract_text {
        tex.push_str("\\begin{abstract}\n");
        tex.push_str(&markdown_to_latex(abstract_text, &cite_keys));
        tex.push_str("\n\\end{abstract}\n\n");
    }

    // Sections
    let section_cmd = match options.document_class {
        LaTeXDocumentClass::Article => "section",
        LaTeXDocumentClass::Report | LaTeXDocumentClass::Book => "chapter",
    };

    for section in &paper.sections {
        if matches!(section.section_type, SectionType::Abstract) {
            continue;
        }

        let cmd = if !options.number_sections {
            format!("{}*", section_cmd)
        } else {
            section_cmd.to_string()
        };

        tex.push_str(&format!("\\{}{{{}}}\n\n", cmd, escape_latex(&section.title)));

        let content = section.rendered_content.as_ref().unwrap_or(&section.content);
        tex.push_str(&markdown_to_latex(content, &cite_keys));
        tex.push_str("\n\n");
    }

    // Bibliography
    tex.push_str("\\bibliographystyle{");
    tex.push_str(&options.bib_style);
    tex.push_str("}\n");
    tex.push_str("\\bibliography{references}\n\n");

    tex.push_str("\\end{document}\n");

    // Generate .bib content
    let bib = generate_bib(sources, references);

    Ok(LaTeXOutput {
        tex_content: tex,
        bib_content: bib,
    })
}

/// Output of LaTeX export
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LaTeXOutput {
    /// The .tex file content
    pub tex_content: String,
    /// The .bib file content
    pub bib_content: String,
}

// =============================================================================
// Helpers
// =============================================================================

fn build_class_options(options: &LaTeXExportOptions) -> String {
    let mut opts = vec![
        format!("{}pt", options.font_size),
        options.paper_size.clone(),
    ];
    if options.two_column {
        opts.push("twocolumn".into());
    }
    opts.join(",")
}

/// Escape special LaTeX characters
fn escape_latex(text: &str) -> String {
    text.replace('\\', "\\textbackslash{}")
        .replace('&', "\\&")
        .replace('%', "\\%")
        .replace('$', "\\$")
        .replace('#', "\\#")
        .replace('_', "\\_")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('~', "\\textasciitilde{}")
        .replace('^', "\\textasciicircum{}")
}

/// Convert markdown content to LaTeX, replacing citation markers with \cite{}
fn markdown_to_latex(content: &str, cite_keys: &HashMap<String, String>) -> String {
    let mut result = content.to_string();

    // Replace citation markers [[source_id]] or [[source_id:page]] with \cite{key}
    let citation_re = Regex::new(r"\[\[([^\]:]+)(?::([^\]]+))?\]\]").unwrap();
    result = citation_re.replace_all(&result, |caps: &regex::Captures| {
        let source_id = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let page = caps.get(2).map(|m| m.as_str().trim());
        let key = cite_keys.get(source_id)
            .cloned()
            .unwrap_or_else(|| source_id.to_string());
        match page {
            Some(p) => format!("\\cite[{}]{{{}}}", p, key),
            None => format!("\\cite{{{}}}", key),
        }
    }).to_string();

    // Bold: **text** → \textbf{text}
    let bold_re = Regex::new(r"\*\*(.+?)\*\*").unwrap();
    result = bold_re.replace_all(&result, "\\textbf{$1}").to_string();

    // Italic: *text* → \textit{text}
    let italic_re = Regex::new(r"\*(.+?)\*").unwrap();
    result = italic_re.replace_all(&result, "\\textit{$1}").to_string();

    // Inline code: `text` → \texttt{text}
    let code_re = Regex::new(r"`([^`]+)`").unwrap();
    result = code_re.replace_all(&result, "\\texttt{$1}").to_string();

    // Subsection headers: ### → \subsubsection, ## → \subsection
    let h3_re = Regex::new(r"(?m)^### (.+)$").unwrap();
    result = h3_re.replace_all(&result, "\\subsubsection{$1}").to_string();

    let h2_re = Regex::new(r"(?m)^## (.+)$").unwrap();
    result = h2_re.replace_all(&result, "\\subsection{$1}").to_string();

    // Unordered lists: - item → \begin{itemize} \item ... \end{itemize}
    result = convert_lists(&result);

    // Escape remaining special chars in non-command text (simplified — only & and %)
    // We do a targeted pass: escape & and % that aren't already preceded by backslash
    let amp_re = Regex::new(r"(?<!\\)&").unwrap();
    result = amp_re.replace_all(&result, "\\&").to_string();
    let pct_re = Regex::new(r"(?<!\\)%").unwrap();
    result = pct_re.replace_all(&result, "\\%").to_string();

    result
}

/// Convert markdown list blocks to LaTeX itemize environments
fn convert_lists(content: &str) -> String {
    let mut output = String::new();
    let mut in_list = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            if !in_list {
                output.push_str("\\begin{itemize}\n");
                in_list = true;
            }
            output.push_str(&format!("  \\item {}\n", &trimmed[2..]));
        } else {
            if in_list {
                output.push_str("\\end{itemize}\n");
                in_list = false;
            }
            output.push_str(line);
            output.push('\n');
        }
    }

    if in_list {
        output.push_str("\\end{itemize}\n");
    }

    // Remove trailing newline to match input behavior
    if output.ends_with('\n') && !content.ends_with('\n') {
        output.pop();
    }

    output
}

/// Generate a BibTeX cite key from a Source
fn source_to_cite_key(source: &Source) -> String {
    let author = source.authors.first()
        .map(|a| a.split_whitespace().last().unwrap_or("unknown").to_lowercase())
        .unwrap_or_else(|| "unknown".to_string());

    let year = source.published_date.as_ref()
        .and_then(|d| d.split('-').next())
        .unwrap_or("nd")
        .to_string();

    let title_word = source.title.split_whitespace()
        .find(|w| w.len() > 3)
        .map(|w| w.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .unwrap_or_else(|| "untitled".to_string());

    format!("{}{}{}", author, year, title_word)
}

/// Generate .bib file content from sources and references
fn generate_bib(sources: &[Source], references: &[Reference]) -> String {
    let mut bib = String::new();
    bib.push_str("% Bibliography generated by MarlOS\n");
    bib.push_str("% https://github.com/canolab/marlos\n\n");

    // From Source objects
    for source in sources {
        let key = source_to_cite_key(source);
        bib.push_str("@article{");
        bib.push_str(&key);
        bib.push_str(",\n");

        bib.push_str(&format!("  title = {{{}}},\n", source.title));

        if !source.authors.is_empty() {
            bib.push_str(&format!("  author = {{{}}},\n", source.authors.join(" and ")));
        }

        if let Some(date) = &source.published_date {
            if let Some(year) = date.split('-').next() {
                bib.push_str(&format!("  year = {{{}}},\n", year));
            }
        }

        if let Some(url) = &source.url {
            bib.push_str(&format!("  url = {{{}}},\n", url));
        }

        bib.push_str("}\n\n");
    }

    // From Reference objects (richer metadata)
    for reference in references {
        let key = if reference.cite_key.is_empty() {
            format!("ref_{}", &reference.id[..8.min(reference.id.len())])
        } else {
            reference.cite_key.clone()
        };

        let entry_type = match reference.ref_type {
            crate::reference_library::reference::ReferenceType::Book => "book",
            crate::reference_library::reference::ReferenceType::InProceedings => "inproceedings",
            crate::reference_library::reference::ReferenceType::InCollection => "incollection",
            crate::reference_library::reference::ReferenceType::PhdThesis => "phdthesis",
            crate::reference_library::reference::ReferenceType::MastersThesis => "mastersthesis",
            crate::reference_library::reference::ReferenceType::TechReport => "techreport",
            _ => "article",
        };

        bib.push_str(&format!("@{}{{{},\n", entry_type, key));
        bib.push_str(&format!("  title = {{{}}},\n", reference.title));

        if !reference.authors.is_empty() {
            bib.push_str(&format!("  author = {{{}}},\n", reference.authors.join(" and ")));
        }

        if let Some(year) = reference.year {
            bib.push_str(&format!("  year = {{{}}},\n", year));
        }

        if let Some(journal) = &reference.journal {
            bib.push_str(&format!("  journal = {{{}}},\n", journal));
        }

        if let Some(publisher) = &reference.publisher {
            bib.push_str(&format!("  publisher = {{{}}},\n", publisher));
        }

        if let Some(volume) = &reference.volume {
            bib.push_str(&format!("  volume = {{{}}},\n", volume));
        }

        if let Some(issue) = &reference.issue {
            bib.push_str(&format!("  number = {{{}}},\n", issue));
        }

        if let Some(pages) = &reference.pages {
            bib.push_str(&format!("  pages = {{{}}},\n", pages));
        }

        if let Some(doi) = &reference.doi {
            bib.push_str(&format!("  doi = {{{}}},\n", doi));
        }

        if let Some(isbn) = &reference.isbn {
            bib.push_str(&format!("  isbn = {{{}}},\n", isbn));
        }

        if let Some(url) = &reference.url {
            bib.push_str(&format!("  url = {{{}}},\n", url));
        }

        if let Some(abstract_text) = &reference.abstract_text {
            bib.push_str(&format!("  abstract = {{{}}},\n", abstract_text));
        }

        bib.push_str("}\n\n");
    }

    bib
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper_generator::{PaperSection, SectionType};

    fn make_test_paper() -> Paper {
        let mut paper = Paper::new("Deep Learning for NLP", "How does attention improve NLP?");
        paper.thesis = Some("Attention mechanisms significantly improve NLP tasks.".into());

        let mut abs = PaperSection::new("Abstract", SectionType::Abstract, 0);
        abs.set_content("This paper surveys attention in NLP.");
        paper.add_section(abs);

        let mut intro = PaperSection::new("Introduction", SectionType::Introduction, 1);
        intro.set_content("Attention was introduced by [[src_1]]. It has been widely adopted [[src_2:p. 15]].");
        paper.add_section(intro);

        let mut conc = PaperSection::new("Conclusion", SectionType::Conclusion, 2);
        conc.set_content("We conclude that attention is effective.");
        paper.add_section(conc);

        paper
    }

    fn make_test_sources() -> Vec<Source> {
        let mut s1 = Source::new("Attention Is All You Need", None);
        s1.id = "src_1".into();
        s1.authors = vec!["Ashish Vaswani".into()];
        s1.published_date = Some("2017".into());

        let mut s2 = Source::new("BERT: Pre-training of Transformers", None);
        s2.id = "src_2".into();
        s2.authors = vec!["Jacob Devlin".into()];
        s2.published_date = Some("2019".into());

        vec![s1, s2]
    }

    #[test]
    fn test_latex_export_basic() {
        let paper = make_test_paper();
        let sources = make_test_sources();
        let options = LaTeXExportOptions::default();

        let result = export_latex(&paper, &sources, &[], &options).unwrap();

        assert!(result.tex_content.contains("\\documentclass"));
        assert!(result.tex_content.contains("\\title{Deep Learning for NLP}"));
        assert!(result.tex_content.contains("\\begin{abstract}"));
        assert!(result.tex_content.contains("\\section{Introduction}"));
        assert!(result.tex_content.contains("\\cite{vaswani2017attention}"));
        assert!(result.tex_content.contains("\\cite[p. 15]{devlin2019bert}"));
        assert!(result.tex_content.contains("\\bibliography{references}"));
    }

    #[test]
    fn test_bib_generation() {
        let sources = make_test_sources();
        let bib = generate_bib(&sources, &[]);

        assert!(bib.contains("@article{vaswani2017attention"));
        assert!(bib.contains("title = {Attention Is All You Need}"));
        assert!(bib.contains("author = {Ashish Vaswani}"));
        assert!(bib.contains("year = {2017}"));
    }

    #[test]
    fn test_escape_latex() {
        assert_eq!(escape_latex("10% increase & $5"), "10\\% increase \\& \\$5");
        assert_eq!(escape_latex("section_1"), "section\\_1");
    }

    #[test]
    fn test_markdown_to_latex() {
        let keys = HashMap::new();
        let input = "This is **bold** and *italic* and `code`.";
        let output = markdown_to_latex(input, &keys);

        assert!(output.contains("\\textbf{bold}"));
        assert!(output.contains("\\textit{italic}"));
        assert!(output.contains("\\texttt{code}"));
    }

    #[test]
    fn test_list_conversion() {
        let input = "Before list:\n- Item one\n- Item two\nAfter list.";
        let output = convert_lists(input);

        assert!(output.contains("\\begin{itemize}"));
        assert!(output.contains("\\item Item one"));
        assert!(output.contains("\\end{itemize}"));
    }

    #[test]
    fn test_ieee_preset() {
        let opts = LaTeXExportOptions::ieee();
        assert_eq!(opts.font_size, 10);
        assert!(opts.two_column);
        assert_eq!(opts.bib_style, "ieeetr");
    }
}
