//! CSL-Lite: Simplified citation style formatting.
//!
//! Instead of full CSL XML parsing (~100 page spec), we use pre-compiled
//! formatting templates for the most common citation styles.

use serde::{Deserialize, Serialize};
use super::reference::Reference;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CslStyle {
    pub id: String,
    pub name: String,
    pub category: String,
    /// Template for in-text citation, e.g., "({{authors_short}}, {{year}})"
    pub inline_template: String,
    /// Template for bibliography entry
    pub bib_template: String,
    /// Whether to use numbered citations
    pub numbered: bool,
}

/// Get all available CSL-Lite styles
pub fn available_styles() -> Vec<CslStyle> {
    vec![
        CslStyle {
            id: "apa7".into(),
            name: "APA 7th Edition".into(),
            category: "author-date".into(),
            inline_template: "({{authors_short}}, {{year}})".into(),
            bib_template: "{{authors_last_first}}. ({{year}}). {{title}}. *{{journal}}*, *{{volume}}*({{issue}}), {{pages}}. {{doi_url}}".into(),
            numbered: false,
        },
        CslStyle {
            id: "mla9".into(),
            name: "MLA 9th Edition".into(),
            category: "author-page".into(),
            inline_template: "({{authors_short}} {{pages}})".into(),
            bib_template: "{{authors_last_first}}. \"{{title}}.\" *{{journal}}*, vol. {{volume}}, no. {{issue}}, {{year}}, pp. {{pages}}.".into(),
            numbered: false,
        },
        CslStyle {
            id: "chicago-notes".into(),
            name: "Chicago Manual (Notes)".into(),
            category: "notes".into(),
            inline_template: "{{authors_short}}, \"{{title_short}},\" {{pages}}.".into(),
            bib_template: "{{authors_last_first}}. \"{{title}}.\" {{journal}} {{volume}}, no. {{issue}} ({{year}}): {{pages}}.".into(),
            numbered: false,
        },
        CslStyle {
            id: "chicago-author-date".into(),
            name: "Chicago Author-Date".into(),
            category: "author-date".into(),
            inline_template: "({{authors_short}} {{year}}, {{pages}})".into(),
            bib_template: "{{authors_last_first}}. {{year}}. \"{{title}}.\" {{journal}} {{volume}} ({{issue}}): {{pages}}.".into(),
            numbered: false,
        },
        CslStyle {
            id: "ieee".into(),
            name: "IEEE".into(),
            category: "numeric".into(),
            inline_template: "[{{number}}]".into(),
            bib_template: "[{{number}}] {{authors_initials}}, \"{{title}},\" *{{journal}}*, vol. {{volume}}, no. {{issue}}, pp. {{pages}}, {{year}}.".into(),
            numbered: true,
        },
        CslStyle {
            id: "acm".into(),
            name: "ACM Computing Surveys".into(),
            category: "numeric".into(),
            inline_template: "[{{number}}]".into(),
            bib_template: "{{authors_last_first}}. {{year}}. {{title}}. *{{journal}}* {{volume}}, {{issue}} ({{year}}), {{pages}}.".into(),
            numbered: true,
        },
        CslStyle {
            id: "harvard".into(),
            name: "Harvard".into(),
            category: "author-date".into(),
            inline_template: "({{authors_short}}, {{year}})".into(),
            bib_template: "{{authors_last_first}} ({{year}}) '{{title}}', *{{journal}}*, {{volume}}({{issue}}), pp. {{pages}}.".into(),
            numbered: false,
        },
        CslStyle {
            id: "vancouver".into(),
            name: "Vancouver".into(),
            category: "numeric".into(),
            inline_template: "({{number}})".into(),
            bib_template: "{{authors_initials}}. {{title}}. {{journal}}. {{year}};{{volume}}({{issue}}):{{pages}}.".into(),
            numbered: true,
        },
        CslStyle {
            id: "nature".into(),
            name: "Nature".into(),
            category: "numeric".into(),
            inline_template: "{{number}}".into(),
            bib_template: "{{authors_last_first}} {{title}}. *{{journal}}* **{{volume}}**, {{pages}} ({{year}}).".into(),
            numbered: true,
        },
        CslStyle {
            id: "science".into(),
            name: "Science (AAAS)".into(),
            category: "numeric".into(),
            inline_template: "({{number}})".into(),
            bib_template: "{{authors_initials}}, {{title}}. *{{journal}}* **{{volume}}**, {{pages}} ({{year}}).".into(),
            numbered: true,
        },
        CslStyle {
            id: "springer-lncs".into(),
            name: "Springer LNCS".into(),
            category: "numeric".into(),
            inline_template: "[{{number}}]".into(),
            bib_template: "{{authors_last_first}}: {{title}}. {{journal}} {{volume}}, {{pages}} ({{year}})".into(),
            numbered: true,
        },
        CslStyle {
            id: "elsevier-harvard".into(),
            name: "Elsevier Harvard".into(),
            category: "author-date".into(),
            inline_template: "({{authors_short}}, {{year}})".into(),
            bib_template: "{{authors_last_first}}, {{year}}. {{title}}. {{journal}}, {{volume}}({{issue}}), pp.{{pages}}.".into(),
            numbered: false,
        },
    ]
}

/// Get a specific style by ID
pub fn get_style(id: &str) -> Option<CslStyle> {
    available_styles().into_iter().find(|s| s.id == id)
}

/// Render an inline citation for a reference
pub fn render_inline(style: &CslStyle, reference: &Reference, number: Option<usize>, page: Option<&str>) -> String {
    let mut result = style.inline_template.clone();
    result = result.replace("{{authors_short}}", &authors_short(&reference.authors));
    result = result.replace("{{year}}", &reference.year.map(|y| y.to_string()).unwrap_or_default());
    result = result.replace("{{number}}", &number.map(|n| n.to_string()).unwrap_or_default());
    result = result.replace("{{pages}}", page.unwrap_or(""));
    result = result.replace("{{title_short}}", &title_short(&reference.title));
    // Clean up empty fields
    result = result.replace("(, )", "").replace("()", "").replace(", )", ")").replace("( ,", "(");
    result.trim().to_string()
}

/// Render a bibliography entry for a reference
pub fn render_bibliography(style: &CslStyle, reference: &Reference, number: Option<usize>) -> String {
    let mut result = style.bib_template.clone();
    result = result.replace("{{authors_last_first}}", &authors_last_first(&reference.authors));
    result = result.replace("{{authors_initials}}", &authors_initials(&reference.authors));
    result = result.replace("{{authors_short}}", &authors_short(&reference.authors));
    result = result.replace("{{title}}", &reference.title);
    result = result.replace("{{title_short}}", &title_short(&reference.title));
    result = result.replace("{{journal}}", reference.journal.as_deref().unwrap_or(""));
    result = result.replace("{{publisher}}", reference.publisher.as_deref().unwrap_or(""));
    result = result.replace("{{volume}}", reference.volume.as_deref().unwrap_or(""));
    result = result.replace("{{issue}}", reference.issue.as_deref().unwrap_or(""));
    result = result.replace("{{pages}}", reference.pages.as_deref().unwrap_or(""));
    result = result.replace("{{year}}", &reference.year.map(|y| y.to_string()).unwrap_or("n.d.".into()));
    result = result.replace("{{number}}", &number.map(|n| n.to_string()).unwrap_or_default());
    result = result.replace("{{doi_url}}", &reference.doi.as_ref().map(|d| format!("https://doi.org/{}", d)).unwrap_or_default());
    // Clean up empty fields
    result = result.replace(", ,", ",").replace("()", "").replace("  ", " ").replace(" .", ".");
    result.trim().to_string()
}

// --- Helper functions ---

fn authors_short(authors: &[String]) -> String {
    if authors.is_empty() { return "Unknown".into(); }
    let last = last_name(&authors[0]);
    match authors.len() {
        1 => last,
        2 => format!("{} & {}", last, last_name(&authors[1])),
        _ => format!("{} et al.", last),
    }
}

fn authors_last_first(authors: &[String]) -> String {
    if authors.is_empty() { return "Unknown".into(); }
    authors.iter().map(|a| {
        let parts: Vec<&str> = a.split_whitespace().collect();
        if parts.len() > 1 {
            let last = parts.last().unwrap();
            let first = parts[..parts.len()-1].join(" ");
            format!("{}, {}", last, first)
        } else {
            a.clone()
        }
    }).collect::<Vec<_>>().join(", ")
}

fn authors_initials(authors: &[String]) -> String {
    if authors.is_empty() { return "Unknown".into(); }
    authors.iter().map(|a| {
        let parts: Vec<&str> = a.split_whitespace().collect();
        if parts.len() > 1 {
            let last = parts.last().unwrap();
            let initials: String = parts[..parts.len()-1].iter()
                .map(|p| format!("{}.", p.chars().next().unwrap_or(' ')))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{} {}", initials, last)
        } else {
            a.clone()
        }
    }).collect::<Vec<_>>().join(", ")
}

fn last_name(name: &str) -> String {
    name.split_whitespace().last().unwrap_or(name).to_string()
}

fn title_short(title: &str) -> String {
    if title.len() <= 40 { title.to_string() }
    else {
        let truncated = &title[..37];
        // Break at last word boundary
        match truncated.rfind(' ') {
            Some(i) => format!("{}...", &truncated[..i]),
            None => format!("{}...", truncated),
        }
    }
}
