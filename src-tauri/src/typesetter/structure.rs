//! Book structure recognition.
//!
//! Operates on pandoc-emitted HTML (`<section class="level1">` elements
//! with H1 headings) and produces:
//!  - a structured book model (front matter / chapters / interludes /
//!    back matter), and
//!  - the same HTML enriched with `data-section-type`, `data-section-number`,
//!    and `data-section-title` attributes on each top-level `<section>`,
//!    so the Phase C CSS template can target them.
//!
//! Classification rules:
//!  - `Chapter <num>[: title]` (or em-dash) → chapter
//!  - `Interlude <num>[— title]` → interlude
//!  - Anything appearing **before** the first chapter → front matter
//!  - Anything appearing **after** the last chapter (and not an interlude)
//!    → back matter
//!  - Numbers may be Arabic (1, 2, 3) or Roman (I, II, III)
//!
//! This is not perfect but matches the manuscript's structure exactly.

use std::sync::OnceLock;

use chrono::Datelike;
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use super::book_config::BookMeta;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SectionKind {
    FrontMatter,
    Chapter,
    Interlude,
    BackMatter,
}

impl SectionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SectionKind::FrontMatter => "front-matter",
            SectionKind::Chapter => "chapter",
            SectionKind::Interlude => "interlude",
            SectionKind::BackMatter => "back-matter",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSection {
    pub kind: SectionKind,
    /// 1-based ordering across all top-level sections.
    pub order: usize,
    /// 1-based chapter number for Chapter sections; 1-based interlude
    /// number for Interlude sections; None otherwise.
    pub number: Option<u32>,
    /// Roman numeral version of the number (used for interludes & front
    /// matter pagination), if applicable.
    pub roman_number: Option<String>,
    pub title: String,
    /// The H1 text exactly as it appears.
    pub h1_raw: String,
    /// Pandoc's section id, e.g. `chchapter-1-the-questions...`.
    pub html_id: String,
    /// Optional running-header override from a `{header="..."}` heading
    /// attribute (pandoc emits it as `data-header`). When None, the
    /// running header falls back to the section title.
    #[serde(default)]
    pub running_header: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookStructure {
    pub sections: Vec<BookSection>,
    pub chapter_count: u32,
    pub interlude_count: u32,
    pub front_matter_count: u32,
    pub back_matter_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredHtml {
    pub structure: BookStructure,
    /// Pandoc HTML with data-section-* attributes added to each top-level
    /// `<section>`.
    pub enriched_html: String,
}

#[derive(Debug, thiserror::Error)]
pub enum StructureError {
    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),
    #[error("invalid HTML (no top-level sections found)")]
    NoSections,
}

static CHAPTER_RE: OnceLock<Regex> = OnceLock::new();
static INTERLUDE_RE: OnceLock<Regex> = OnceLock::new();
static SECTION_OPEN_RE: OnceLock<Regex> = OnceLock::new();

fn chapter_re() -> &'static Regex {
    CHAPTER_RE.get_or_init(|| {
        // (?s) makes `.` match newlines — H1 text may be wrapped across
        // lines after pandoc reformats. We normalize whitespace before
        // matching anyway, but the flag is belt-and-suspenders.
        Regex::new(
            r"(?six)
            ^\s*
            (?:chapter|ch\.?)\s+
            (?P<num>\d+|[ivxlcdm]+)
            \s*[:\-—–]?\s*
            (?P<title>.*?)
            \s*$
        ",
        )
        .expect("chapter regex")
    })
}

fn interlude_re() -> &'static Regex {
    INTERLUDE_RE.get_or_init(|| {
        Regex::new(
            r"(?six)
            ^\s*
            interlude\s+
            (?P<num>\d+|[ivxlcdm]+)
            \s*[:\-—–]?\s*
            (?P<title>.*?)
            \s*$
        ",
        )
        .expect("interlude regex")
    })
}

/// Collapse runs of whitespace (including newlines) to a single space and
/// trim — needed because pandoc may wrap long headings across lines.
fn normalize_h1(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(c);
            last_was_space = false;
        }
    }
    out.trim().to_string()
}

/// Match the opening `<section ...>` tag for top-level (level1) sections.
fn section_open_re() -> &'static Regex {
    SECTION_OPEN_RE.get_or_init(|| {
        Regex::new(
            r#"(?xs)
            <section
            \s+
            (?P<attrs>[^>]*?id="(?P<id>[^"]+)"[^>]*?class="[^"]*\blevel1\b[^"]*"[^>]*)
            >
        "#,
        )
        .expect("section regex")
    })
}

/// Convert a Roman numeral string to u32. Lower- or upper-case. Returns
/// None for non-Roman input.
pub(crate) fn parse_roman(s: &str) -> Option<u32> {
    let s = s.trim().to_ascii_uppercase();
    if s.is_empty() || s.chars().any(|c| !"IVXLCDM".contains(c)) {
        return None;
    }
    let mut total: u32 = 0;
    let mut prev: u32 = 0;
    for c in s.chars().rev() {
        let v = match c {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            'C' => 100,
            'D' => 500,
            'M' => 1000,
            _ => return None,
        };
        if v < prev {
            total = total.checked_sub(v)?;
        } else {
            total = total.checked_add(v)?;
        }
        prev = v;
    }
    Some(total)
}

pub(crate) fn to_roman(n: u32) -> String {
    let mut out = String::new();
    let pairs: &[(u32, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut n = n;
    for &(v, sym) in pairs {
        while n >= v {
            out.push_str(sym);
            n -= v;
        }
    }
    out
}

fn parse_number(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok().or_else(|| parse_roman(s))
}

#[derive(Debug, Clone)]
struct RawHeading {
    h1_raw: String,
    html_id: String,
    /// Space-separated CSS classes on the `<section>` (from pandoc
    /// header attributes, e.g. `# Title {.chapter}`). Lets the writer
    /// force a section to count as a numbered chapter / interlude even
    /// when its heading text doesn't start with "Chapter" / "Interlude".
    classes: String,
    /// `data-header` attribute (from `{header="..."}`) — a running-header
    /// override for this section.
    data_header: Option<String>,
}

/// Walk the pandoc HTML and extract one entry per top-level section.
fn extract_top_level_sections(html: &str) -> Vec<RawHeading> {
    let doc = Html::parse_fragment(html);
    let section_sel = Selector::parse(r#"section.level1"#).expect("selector");
    let h1_sel = Selector::parse("h1").expect("h1 selector");

    let mut out = Vec::new();
    for section in doc.select(&section_sel) {
        let id = section.value().attr("id").unwrap_or("").to_string();
        let classes = section.value().attr("class").unwrap_or("").to_string();
        let data_header = section
            .value()
            .attr("data-header")
            .map(|s| s.to_string())
            .filter(|s| !s.trim().is_empty());
        let h1 = section
            .select(&h1_sel)
            .next()
            .map(|h| h.text().collect::<Vec<_>>().join(""))
            .unwrap_or_default();
        out.push(RawHeading {
            h1_raw: h1,
            html_id: id,
            classes,
            data_header,
        });
    }
    out
}

/// Classify and number each section. Position-aware: front matter is
/// "before first chapter," back matter is "after last chapter."
/// Markdown-AST-friendly entry to the classifier. Takes
/// `(heading_text, html_id)` pairs for top-level (level-1) headings
/// in document order and returns the classified `BookSection`s. The
/// pandoc-specific class/data-header attributes (`{.chapter}`,
/// `{header="..."}`) are dropped on this path because comrak doesn't
/// parse them — flagged as a known gap if a manuscript starts using
/// either. M2.5 of the typst-port plan.
pub fn classify_markdown_headings(headings: &[(String, String)]) -> Vec<BookSection> {
    let raw: Vec<RawHeading> = headings
        .iter()
        .map(|(text, id)| RawHeading {
            h1_raw: text.clone(),
            html_id: id.clone(),
            classes: String::new(),
            data_header: None,
        })
        .collect();
    classify_sections(&raw)
}

pub fn classify_sections(headings: &[RawHeading]) -> Vec<BookSection> {
    let chapter_re = chapter_re();
    let interlude_re = interlude_re();

    // First pass: tentative classification.
    #[derive(Clone)]
    enum Tentative {
        Chapter(u32, String),
        Interlude(u32, String),
        Plain(String), // any other H1 — could be front or back matter
    }

    let mut tentative: Vec<Tentative> = Vec::with_capacity(headings.len());
    let mut first_chapter_idx: Option<usize> = None;
    let mut last_chapter_idx: Option<usize> = None;

    for (i, h) in headings.iter().enumerate() {
        let normalized = normalize_h1(&h.h1_raw);
        let has_class = |c: &str| h.classes.split_whitespace().any(|x| x == c);
        if let Some(caps) = chapter_re.captures(&normalized) {
            let num = caps
                .name("num")
                .and_then(|m| parse_number(m.as_str()))
                .unwrap_or(0);
            let title = caps
                .name("title")
                .map(|m| normalize_h1(m.as_str()))
                .unwrap_or_default();
            tentative.push(Tentative::Chapter(num, title));
            first_chapter_idx.get_or_insert(i);
            last_chapter_idx = Some(i);
        } else if has_class("chapter") {
            // Forced chapter via `# Title {.chapter}` — no "Chapter N"
            // prefix, so it takes the next sequential number and the
            // whole heading is the title.
            tentative.push(Tentative::Chapter(0, normalized));
            first_chapter_idx.get_or_insert(i);
            last_chapter_idx = Some(i);
        } else if let Some(caps) = interlude_re.captures(&normalized) {
            let num = caps
                .name("num")
                .and_then(|m| parse_number(m.as_str()))
                .unwrap_or(0);
            let title = caps
                .name("title")
                .map(|m| normalize_h1(m.as_str()))
                .unwrap_or_default();
            tentative.push(Tentative::Interlude(num, title));
        } else if has_class("interlude") {
            tentative.push(Tentative::Interlude(0, normalized));
        } else {
            tentative.push(Tentative::Plain(normalized));
        }
    }

    let mut chapter_seen = 0u32;
    let mut interlude_seen = 0u32;
    let mut sections = Vec::with_capacity(headings.len());
    for (i, (raw, t)) in headings.iter().zip(tentative.into_iter()).enumerate() {
        let order = i + 1;
        match t {
            Tentative::Chapter(num, title) => {
                chapter_seen += 1;
                let n = if num > 0 { num } else { chapter_seen };
                sections.push(BookSection {
                    kind: SectionKind::Chapter,
                    order,
                    number: Some(n),
                    roman_number: Some(to_roman(n)),
                    title,
                    h1_raw: raw.h1_raw.clone(),
                    html_id: raw.html_id.clone(),
                    running_header: raw.data_header.clone(),
                });
            }
            Tentative::Interlude(num, title) => {
                interlude_seen += 1;
                let n = if num > 0 { num } else { interlude_seen };
                sections.push(BookSection {
                    kind: SectionKind::Interlude,
                    order,
                    number: Some(n),
                    roman_number: Some(to_roman(n)),
                    title,
                    h1_raw: raw.h1_raw.clone(),
                    html_id: raw.html_id.clone(),
                    running_header: raw.data_header.clone(),
                });
            }
            Tentative::Plain(title) => {
                let kind = match (first_chapter_idx, last_chapter_idx) {
                    (Some(first), _) if i < first => SectionKind::FrontMatter,
                    (_, Some(last)) if i > last => SectionKind::BackMatter,
                    // No chapters at all → treat as front matter
                    (None, None) => SectionKind::FrontMatter,
                    _ => SectionKind::FrontMatter, // unreachable but safe
                };
                sections.push(BookSection {
                    kind,
                    order,
                    number: None,
                    roman_number: None,
                    title,
                    h1_raw: raw.h1_raw.clone(),
                    html_id: raw.html_id.clone(),
                    running_header: raw.data_header.clone(),
                });
            }
        }
    }
    sections
}

/// Inject `data-section-type`, `data-section-number`, and
/// `data-section-title` attributes into every matching top-level `<section>`
/// opening tag in the pandoc HTML. Sections are matched by their `id`.
pub fn enrich_html(html: &str, sections: &[BookSection]) -> String {
    let re = section_open_re();
    let mut by_id = std::collections::HashMap::new();
    for s in sections {
        by_id.insert(s.html_id.clone(), s);
    }
    re.replace_all(html, |caps: &regex::Captures| {
        let id = caps.name("id").map(|m| m.as_str()).unwrap_or("");
        let attrs = caps.name("attrs").map(|m| m.as_str()).unwrap_or("");
        match by_id.get(id) {
            Some(section) => {
                let mut extra = format!(
                    r#" data-section-type="{}" data-section-order="{}""#,
                    section.kind.as_str(),
                    section.order
                );
                if let Some(n) = section.number {
                    extra.push_str(&format!(r#" data-section-number="{}""#, n));
                }
                if let Some(roman) = &section.roman_number {
                    extra.push_str(&format!(r#" data-section-roman="{}""#, roman));
                }
                let safe_title = section
                    .title
                    .replace('&', "&amp;")
                    .replace('"', "&quot;");
                extra.push_str(&format!(r#" data-section-title="{}""#, safe_title));
                format!("<section {}{}>", attrs, extra)
            }
            None => caps[0].to_string(),
        }
    })
    .into_owned()
}

pub fn analyze(html: &str) -> Result<StructuredHtml, StructureError> {
    analyze_with_options(html, 5)
}

/// Same as `analyze` but lets the caller specify how many words after
/// the drop cap get the lead-in span. 0 disables lead-in (drop cap span
/// still wrapped). Pulled out so the typesetter can pass through the
/// typography.lead_in_word_count config value.
pub fn analyze_with_options(
    html: &str,
    lead_in_word_count: usize,
) -> Result<StructuredHtml, StructureError> {
    let headings = extract_top_level_sections(html);
    if headings.is_empty() {
        return Err(StructureError::NoSections);
    }
    let sections = classify_sections(&headings);
    let chapter_count = sections.iter().filter(|s| s.kind == SectionKind::Chapter).count() as u32;
    let interlude_count = sections.iter().filter(|s| s.kind == SectionKind::Interlude).count() as u32;
    let front_matter_count = sections.iter().filter(|s| s.kind == SectionKind::FrontMatter).count() as u32;
    let back_matter_count = sections.iter().filter(|s| s.kind == SectionKind::BackMatter).count() as u32;
    let mut enriched_html = enrich_html(html, &sections);
    enriched_html = wrap_chapter_openers(&enriched_html, lead_in_word_count);

    Ok(StructuredHtml {
        structure: BookStructure {
            sections,
            chapter_count,
            interlude_count,
            front_matter_count,
            back_matter_count,
        },
        enriched_html,
    })
}

/// Build HTML for the generated title / copyright / dedication pages
/// from the book.toml metadata. Returns an empty string if there's
/// nothing to render (no title and no copyright holder/author).
///
/// Pages are tagged `data-section-type="front-matter"` so the existing
/// front-matter @page rule applies (lower-roman page numbers, no
/// running header). They also carry `data-front-page="title|copyright
/// |dedication"` and a unique class so CSS can target each one
/// individually (suppress page numbers on title page, center the
/// dedication, etc.).
///
/// Intentionally not added to the `BookStructure.sections` list —
/// these are "virtual" pages that don't appear in the outline tree;
/// the writer didn't author them.
pub fn build_generated_front_matter(book: &BookMeta) -> String {
    let mut out = String::new();

    // ---- Title page ----
    if !book.title.trim().is_empty() {
        out.push_str(
            r#"<section data-section-type="front-matter" data-front-page="title" class="generated-title-page">
  <div class="title-page-inner">
"#,
        );
        out.push_str(&format!(
            "    <h1 class=\"gen-book-title\">{}</h1>\n",
            escape_html(&book.title)
        ));
        if !book.subtitle.trim().is_empty() {
            out.push_str(&format!(
                "    <p class=\"gen-book-subtitle\">{}</p>\n",
                escape_html(&book.subtitle)
            ));
        }
        if !book.author.trim().is_empty() {
            out.push_str(&format!(
                "    <p class=\"gen-book-author\">{}</p>\n",
                escape_html(&book.author)
            ));
        }
        out.push_str("  </div>\n</section>\n");
    }

    // ---- Copyright page ----
    let holder = if book.copyright_holder.trim().is_empty() {
        book.author.trim()
    } else {
        book.copyright_holder.trim()
    };
    let year = if book.copyright_year.trim().is_empty() {
        chrono::Utc::now().year().to_string()
    } else {
        book.copyright_year.trim().to_string()
    };
    if !holder.is_empty() {
        out.push_str(
            r#"<section data-section-type="front-matter" data-front-page="copyright" class="generated-copyright-page">
  <div class="copyright-page-inner">
"#,
        );
        out.push_str(&format!(
            "    <p>Copyright \u{00A9} {} {}</p>\n",
            escape_html(&year),
            escape_html(holder)
        ));
        out.push_str("    <p>All rights reserved.</p>\n");
        if !book.publisher.trim().is_empty() {
            out.push_str(&format!(
                "    <p class=\"gen-publisher\">{}</p>\n",
                escape_html(book.publisher.trim())
            ));
        }
        if !book.isbn.trim().is_empty() {
            out.push_str(&format!(
                "    <p class=\"gen-isbn\">ISBN {}</p>\n",
                escape_html(book.isbn.trim())
            ));
        }
        out.push_str("  </div>\n</section>\n");
    }

    // ---- Dedication ----
    if !book.dedication.trim().is_empty() {
        out.push_str(
            r#"<section data-section-type="front-matter" data-front-page="dedication" class="generated-dedication-page">
  <div class="dedication-inner">
"#,
        );
        out.push_str(&format!(
            "    <p>{}</p>\n",
            escape_html(book.dedication.trim())
        ));
        out.push_str("  </div>\n</section>\n");
    }

    out
}

/// HTML for generated BACK-matter pages — currently just the
/// acknowledgements page. Returns empty string if nothing to render.
/// Caller appends this to the enriched_html (after all chapters /
/// authored back matter / notes).
pub fn build_generated_back_matter(book: &BookMeta) -> String {
    let mut out = String::new();
    let ack = book.acknowledgements.trim();
    if ack.is_empty() {
        return out;
    }
    out.push_str(
        r#"<section data-section-type="back-matter" data-back-page="acknowledgements" class="generated-acknowledgements-page">
  <h2 class="gen-ack-heading">Acknowledgements</h2>
  <div class="acknowledgements-inner">
"#,
    );
    // Split on blank lines so the writer can compose multi-paragraph
    // acks in the textarea. Each chunk becomes its own <p>.
    for para in ack.split("\n\n") {
        let trimmed = para.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Preserve intra-paragraph soft line breaks as <br>.
        let with_breaks = escape_html(trimmed).replace('\n', "<br>");
        out.push_str(&format!("    <p>{}</p>\n", with_breaks));
    }
    out.push_str("  </div>\n</section>\n");
    out
}

/// Scan the body HTML for captioned figures, number them in document
/// order, prefix each `<figcaption>` with "Fig. N." (so body and back-
/// matter numbering agree), and build the generated "List of Figures"
/// back-matter section. Entries link to each figure's id; page folios are
/// filled by the two-pass step (they reuse the `toc-entry` machinery).
///
/// Returns `(numbered_html, lof_section)`. `lof_section` is "" when the
/// document has no captioned figures. The caller appends `lof_section`
/// after the other generated back matter.
///
/// When `include` is false the document is returned unchanged (no caption
/// numbering, no list) — the `[export] include_list_of_figures` toggle.
pub fn build_list_of_figures(html: &str, include: bool) -> (String, String) {
    use std::cell::RefCell;
    if !include {
        return (html.to_string(), String::new());
    }
    static FIG_RE: OnceLock<Regex> = OnceLock::new();
    // 1: "<figure …id=\""  2: id  3: "\"…><…><figcaption…>"  4: caption  5: "</figcaption>"
    let re = FIG_RE.get_or_init(|| {
        Regex::new(
            r#"(?s)(<figure\b[^>]*\bid=")([^"]+)("[^>]*>.*?<figcaption\b[^>]*>)(.*?)(</figcaption>)"#,
        )
        .unwrap()
    });

    let entries = RefCell::new(String::new());
    let counter = RefCell::new(0usize);

    let numbered = re
        .replace_all(html, |c: &regex::Captures| {
            let mut n = counter.borrow_mut();
            *n += 1;
            let num = *n;
            let id = &c[2];
            let caption = c[4].trim();
            // Back-matter entry: caption HTML preserved (it may carry <em>).
            entries.borrow_mut().push_str(&format!(
                "    <a class=\"toc-entry lof-entry\" href=\"#{}\">\
                 <span class=\"toc-text\"><span class=\"lof-num\">Fig. {}.</span> {}</span></a>\n",
                id, num, caption,
            ));
            // Body caption gets the matching number prefix.
            format!(
                "{}{}{}<span class=\"fig-num\">Fig. {}.</span> {}{}",
                &c[1], id, &c[3], num, caption, &c[5],
            )
        })
        .into_owned();

    let entries = entries.into_inner();
    if entries.is_empty() {
        return (numbered, String::new());
    }
    let lof = format!(
        "<section data-section-type=\"back-matter\" data-back-page=\"list-of-figures\" \
         class=\"generated-lof-page\">\n  <h1 class=\"gen-lof-title\">List of Figures</h1>\n  \
         <nav class=\"toc lof\">\n{}  </nav>\n</section>\n",
        entries,
    );
    (numbered, lof)
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build a Table of Contents page from the parsed structure.
///
/// Chapters get a `N.` number prefix; every other section (front
/// matter, interludes, back matter) renders italic with no number
/// (styled via the `toc-other` class). Each entry links to the
/// section's `html_id` so the preview pipeline can fill page numbers
/// with `target-counter`; front-matter entries also carry `toc-fm` so
/// their (lower-roman) folios can be styled distinctly from body pages.
/// The book-title H1 is skipped — it lives on the generated title page.
/// Returns "" when there are no sections.
///
/// The caller inserts this right after the generated front matter
/// (title / copyright / dedication) and before the body.
pub fn build_toc(structure: &BookStructure, book_title: &str) -> String {
    let title_norm = normalize_h1(book_title);
    let mut entries = String::new();
    for sec in &structure.sections {
        // Skip the book-title section — it's the title page, not an entry.
        if !title_norm.is_empty() && normalize_h1(&sec.h1_raw) == title_norm {
            continue;
        }
        let href = escape_html(&sec.html_id);
        match (&sec.kind, sec.number) {
            (SectionKind::Chapter, Some(n)) => entries.push_str(&format!(
                "    <a class=\"toc-entry toc-chapter\" href=\"#{}\">\
                 <span class=\"toc-num\">{}.</span>\
                 <span class=\"toc-text\">{}</span></a>\n",
                href,
                n,
                escape_html(&sec.title),
            )),
            // Front matter, interludes, back matter — italic, no number.
            _ => {
                let fm = if sec.kind == SectionKind::FrontMatter {
                    " toc-fm"
                } else {
                    ""
                };
                entries.push_str(&format!(
                    "    <a class=\"toc-entry toc-other{}\" href=\"#{}\">\
                     <span class=\"toc-text\">{}</span></a>\n",
                    fm,
                    href,
                    escape_html(&sec.h1_raw),
                ));
            }
        }
    }
    if entries.is_empty() {
        return String::new();
    }
    format!(
        "<section data-section-type=\"front-matter\" data-front-page=\"toc\" \
         class=\"generated-toc-page\">\n  <h1 class=\"gen-toc-title\">Contents</h1>\n  \
         <nav class=\"toc\">\n{}  </nav>\n</section>\n",
        entries,
    )
}

/// Wrap the first letter of every chapter's opening paragraph in a
/// `<span class="drop-cap">`, and the next `lead_in_word_count` words
/// in a `<span class="lead-in">`. Leading punctuation (quotes, dashes,
/// ellipses, opening parenthesis) is preserved BEFORE the drop cap so
/// it renders at normal size — fixes the "`"H`ello..." pseudo-element
/// bug where the cap landed on the quote mark.
///
/// Paragraphs that start with inline HTML (`<em>`, `<a>`, etc.) are
/// left untouched in V2 — a rare-enough case to defer.
fn wrap_chapter_openers(html: &str, lead_in_word_count: usize) -> String {
    // Match each chapter section opener followed (eventually) by the
    // first `<p>` element's content. The lazy `.*?` between captures
    // 1 and 2 swallows the H1 + any whitespace; capture 2 isolates
    // the paragraph's inner text for span insertion.
    static CAP_RE: OnceLock<Regex> = OnceLock::new();
    let re = CAP_RE.get_or_init(|| {
        // Greedy `[^<]+` after the <p> open tag consumes everything up
        // to the first nested inline tag (or the closing `</p>`).
        // Look-around isn't supported by Rust's regex crate; this
        // achieves the same effect because `[^<]+` naturally stops at
        // the next `<`.
        Regex::new(
            r#"(?s)(<section [^>]*data-section-type="chapter"[^>]*>.*?<p[^>]*>)([^<]+)"#,
        )
        .unwrap()
    });

    re.replace_all(html, |caps: &regex::Captures| {
        let opener = &caps[1];
        let p_text = &caps[2];
        match wrap_opener_text(p_text, lead_in_word_count) {
            Some(wrapped) => format!("{}{}", opener, wrapped),
            None => caps[0].to_string(),
        }
    })
    .into_owned()
}

/// Wraps the drop cap + lead-in spans on a paragraph's leading text
/// run (the text before the first inline tag). Returns None when no
/// suitable leading letter is found.
fn wrap_opener_text(text: &str, lead_in_word_count: usize) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut idx = 0;
    let mut prefix = String::new();

    // Skip leading whitespace and common opening punctuation. Stop at
    // the first alphabetic character — that's the drop cap.
    while idx < chars.len() {
        let c = chars[idx];
        if c.is_alphabetic() {
            break;
        }
        // Allow whitespace and common opening punctuation BEFORE the cap.
        if c.is_whitespace()
            || matches!(
                c,
                '"' | '\''
                    | '\u{201C}' // “
                    | '\u{201D}' // ”
                    | '\u{2018}' // ‘
                    | '\u{2019}' // ’
                    | '\u{00AB}' // «
                    | '\u{00BB}' // »
                    | '—' | '–' | '-'
                    | '…' | '.' | ','
                    | '(' | '['
            )
        {
            prefix.push(c);
            idx += 1;
            continue;
        }
        // Numbers, symbols, etc. — bail; no drop cap for this paragraph.
        return None;
    }

    if idx >= chars.len() {
        return None;
    }
    let drop_cap = chars[idx];
    idx += 1;

    // Lead-in: rest of the first word + (lead_in_word_count - 1) more
    // words. A word boundary is a transition from non-whitespace to
    // whitespace; we count completed words. Stops early if an inline
    // tag (`<`) appears — keeps the HTML well-formed.
    let mut lead_in = String::new();
    let mut completed_words: usize = 0;
    let mut in_word = true; // continuation of the drop-cap word

    while idx < chars.len() {
        let c = chars[idx];
        if c == '<' {
            break;
        }
        if c.is_whitespace() {
            if in_word {
                completed_words += 1;
                in_word = false;
                if completed_words >= lead_in_word_count {
                    break;
                }
            }
        } else {
            in_word = true;
        }
        lead_in.push(c);
        idx += 1;
    }
    // If we ended mid-word at the text boundary, count it as completed
    // (otherwise short opening paragraphs lose their lead-in entirely).
    if in_word && completed_words < lead_in_word_count {
        // Trailing word was unterminated by whitespace — that's fine,
        // it's included in `lead_in` already.
    }

    let tail: String = chars[idx..].iter().collect();

    Some(format!(
        "{}<span class=\"drop-cap\">{}</span><span class=\"lead-in\">{}</span>{}",
        prefix, drop_cap, lead_in, tail
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_round_trip() {
        for n in [1, 4, 9, 14, 40, 90, 400, 1994] {
            assert_eq!(parse_roman(&to_roman(n)), Some(n));
        }
    }

    #[test]
    fn list_of_figures_numbers_and_links() {
        // Shape pandoc's implicit_figures emits (id on <figure>, after
        // hoist the layout class sits on the figure too).
        let html = "\
<figure id=\"chfig-a\" class=\"fig-bleed\">\n<img src=\"x.png\" alt=\"First\" />\n\
<figcaption aria-hidden=\"true\">First <em>caption</em></figcaption>\n</figure>\n\
<p>Body.</p>\n\
<figure id=\"chfig-b\">\n<img src=\"y.png\" alt=\"Second\" />\n\
<figcaption aria-hidden=\"true\">Second caption</figcaption>\n</figure>\n";
        let (numbered, lof) = build_list_of_figures(html, true);
        // Body captions get the matching number prefix.
        assert!(numbered.contains("<span class=\"fig-num\">Fig. 1.</span> First <em>caption</em>"));
        assert!(numbered.contains("<span class=\"fig-num\">Fig. 2.</span> Second caption"));
        // LoF lists both, in order, linking to the figure ids.
        assert!(lof.contains("List of Figures"));
        assert!(lof.contains("href=\"#chfig-a\""));
        assert!(lof.contains("href=\"#chfig-b\""));
        assert!(lof.contains("<span class=\"lof-num\">Fig. 1.</span> First <em>caption</em>"));
    }

    #[test]
    fn list_of_figures_empty_when_no_figures() {
        let (numbered, lof) = build_list_of_figures("<p>No figures here.</p>", true);
        assert_eq!(numbered, "<p>No figures here.</p>");
        assert!(lof.is_empty());
    }

    #[test]
    fn list_of_figures_toggle_off_is_noop() {
        let html = "<figure id=\"chfig-a\">\n<img src=\"x.png\" alt=\"c\" />\n\
<figcaption>Cap</figcaption>\n</figure>";
        let (numbered, lof) = build_list_of_figures(html, false);
        assert_eq!(numbered, html); // no caption numbering
        assert!(lof.is_empty());
    }

    #[test]
    fn classifies_chapter_and_interlude() {
        let html = r##"
            <section id="ch1" class="level1"><h1>Chapter 1: The Hook</h1></section>
            <section id="ch2" class="level1"><h1>Interlude I — The Student</h1></section>
            <section id="ch3" class="level1"><h1>Chapter 2: The Crack</h1></section>
        "##;
        let s = analyze(html).expect("analyze");
        assert_eq!(s.structure.chapter_count, 2);
        assert_eq!(s.structure.interlude_count, 1);
        assert_eq!(s.structure.sections[0].number, Some(1));
        assert_eq!(s.structure.sections[1].number, Some(1));
        assert_eq!(s.structure.sections[2].number, Some(2));
    }

    #[test]
    fn front_matter_before_first_chapter() {
        let html = r##"
            <section id="fa" class="level1"><h1>Author's Note</h1></section>
            <section id="ch1" class="level1"><h1>Chapter 1: Start</h1></section>
            <section id="bb" class="level1"><h1>Appendix: Extras</h1></section>
        "##;
        let s = analyze(html).expect("analyze");
        assert_eq!(s.structure.sections[0].kind, SectionKind::FrontMatter);
        assert_eq!(s.structure.sections[1].kind, SectionKind::Chapter);
        assert_eq!(s.structure.sections[2].kind, SectionKind::BackMatter);
    }

    #[test]
    fn enriches_with_data_attributes() {
        let html = r##"<section id="ch1" class="level1"><h1>Chapter 1: A</h1></section>"##;
        let out = analyze(html).expect("analyze");
        assert!(out.enriched_html.contains(r#"data-section-type="chapter""#));
        assert!(out.enriched_html.contains(r#"data-section-number="1""#));
    }
}
