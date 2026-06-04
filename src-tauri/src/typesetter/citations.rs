//! Citation transformation for the typesetter.
//!
//! Spec lives at `marlos-citations-handoff.md` in the book directory.
//! Summary: walk the source markdown, replace `[CITE: text]` markers
//! with superscript references, and append a single `# Notes` back-
//! matter section that groups numbered notes per chapter. The source
//! markdown stays canonical; numbering happens at render time so
//! re-ordering or adding citations doesn't require manual renumbering.
//!
//! This runs *before* pandoc — both PDF and EPUB pipelines see the
//! same already-transformed markdown.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use super::book_config::BookConfig;

#[derive(Debug, thiserror::Error)]
pub enum CitationError {
    #[error("regex compilation failed: {0}")]
    Regex(#[from] regex::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Read all manuscript files in book.toml order, concatenate them
/// (with section separators so citation/heading detection doesn't
/// merge content across files), run the citation transform, and
/// write the result to a temp markdown file. Returns the temp file
/// path along with the transform result so the caller can surface
/// warnings and note counts to the UI.
///
/// The pipeline (typesetter_book_load / typesetter_export_pdf /
/// typesetter_export_epub) feeds this temp file to pandoc instead of
/// looping over per-file pandoc calls. Single-document conversion is
/// also what makes per-chapter heading IDs deterministic across the
/// whole book.
pub fn prepare_book_markdown(
    config: &BookConfig,
) -> Result<(PathBuf, CitationTransformResult), CitationError> {
    let mut combined = String::new();
    for (i, file) in config.resolved_files().into_iter().enumerate() {
        if i > 0 {
            // Two newlines between files so headings at boundaries
            // don't accidentally merge with a preceding paragraph.
            combined.push_str("\n\n");
        }
        let content = std::fs::read_to_string(&file)?;
        combined.push_str(&content);
    }

    let result = transform_citations(&combined);

    let temp_dir = std::env::temp_dir().join("marlos-typesetter");
    std::fs::create_dir_all(&temp_dir)?;
    let temp_path = temp_dir.join(format!("book-{}.md", uuid::Uuid::new_v4()));
    std::fs::write(&temp_path, &result.transformed)?;

    Ok((temp_path, result))
}

/// Best-effort cleanup of a temp markdown file produced by
/// `prepare_book_markdown`. Failures are non-fatal — the OS will
/// reap the marlos-typesetter dir on its own eventually.
pub fn cleanup_temp_markdown(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[derive(Debug, Clone)]
pub struct CitationTransformResult {
    /// The transformed markdown — feed this to pandoc instead of the
    /// raw source.
    pub transformed: String,
    /// Non-fatal issues encountered during transformation. Surface to
    /// the UI so the writer notices unfinished verification markers,
    /// orphaned citations, etc.
    pub warnings: Vec<String>,
    /// Total `[CITE:]` markers replaced.
    pub note_count: u32,
    /// Chapters/sections that ended up with at least one note.
    pub chapters_with_notes: u32,
}

/// Identifies a chapter/section heading. The id is deterministic so
/// cross-references (sup → note → back-link) line up.
#[derive(Debug, Clone)]
struct ChapterId {
    id: String,
    title: String,
}

#[derive(Debug, Clone)]
struct Note {
    number: u32,
    note_id: String,
    /// Markdown text of the note. Passes through pandoc, so inline
    /// markdown (`*italic*`, `"quoted"`, em-dashes, etc.) is rendered.
    text: String,
}

#[derive(Debug, Clone)]
struct ChapterGroup {
    id: String,
    title: String,
    notes: Vec<Note>,
}

static CITE_RE: OnceLock<Regex> = OnceLock::new();
static VERIFY_RE: OnceLock<Regex> = OnceLock::new();
static ANY_HEADING_RE: OnceLock<Regex> = OnceLock::new();
static CHAPTER_RE: OnceLock<Regex> = OnceLock::new();
static INTERLUDE_RE: OnceLock<Regex> = OnceLock::new();
static APPENDIX_RE: OnceLock<Regex> = OnceLock::new();

fn cite_re() -> &'static Regex {
    // (?s) so . matches newlines — citations occasionally wrap.
    // Lazy `.+?` so adjacent markers don't merge.
    // Optional leading whitespace is consumed so the superscript
    // typeset hugs the preceding word ("text¹" rather than "text ¹"),
    // matching standard typographic convention for endnote markers.
    CITE_RE.get_or_init(|| Regex::new(r"(?s)\s?\[CITE:\s*(.+?)\]").unwrap())
}

fn verify_re() -> &'static Regex {
    VERIFY_RE.get_or_init(|| Regex::new(r"\[VERIFY:").unwrap())
}

/// Match a heading at any level (#, ##, ###, ...).
/// Group 1 is the leading hashes; group 2 is the heading text.
fn any_heading_re() -> &'static Regex {
    ANY_HEADING_RE.get_or_init(|| Regex::new(r"^(#{1,6})\s+(.+?)\s*$").unwrap())
}

fn chapter_re() -> &'static Regex {
    CHAPTER_RE.get_or_init(|| {
        Regex::new(r"(?i)^Chapter\s+(\d+|[ivxlcdm]+)\b").unwrap()
    })
}

fn interlude_re() -> &'static Regex {
    INTERLUDE_RE.get_or_init(|| {
        Regex::new(r"(?i)^Interlude\s+(\d+|[ivxlcdm]+)\b").unwrap()
    })
}

fn appendix_re() -> &'static Regex {
    APPENDIX_RE.get_or_init(|| Regex::new(r"(?i)^Appendix\b").unwrap())
}

/// Header text that triggers strip-section mode. Matched case-
/// insensitively and trimmed.
const STRIP_HEADING: &str = "Note on Citation Markers";

fn parse_num_to_arabic(s: &str) -> Option<u32> {
    // Try arabic first
    if let Ok(n) = s.trim().parse::<u32>() {
        return Some(n);
    }
    // Roman numerals (lowercase or uppercase)
    let upper = s.trim().to_ascii_uppercase();
    if upper.is_empty() || upper.chars().any(|c| !"IVXLCDM".contains(c)) {
        return None;
    }
    let mut total: u32 = 0;
    let mut prev: u32 = 0;
    for c in upper.chars().rev() {
        let v = match c {
            'I' => 1, 'V' => 5, 'X' => 10, 'L' => 50,
            'C' => 100, 'D' => 500, 'M' => 1000,
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

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn derive_chapter_id(heading_text: &str) -> ChapterId {
    let trimmed = heading_text.trim();
    if let Some(caps) = chapter_re().captures(trimmed) {
        let n = caps.get(1).unwrap().as_str();
        if let Some(num) = parse_num_to_arabic(n) {
            return ChapterId {
                id: format!("ch{}", num),
                title: trimmed.to_string(),
            };
        }
    }
    if let Some(caps) = interlude_re().captures(trimmed) {
        let n = caps.get(1).unwrap().as_str();
        if let Some(num) = parse_num_to_arabic(n) {
            return ChapterId {
                id: format!("interlude{}", num),
                title: trimmed.to_string(),
            };
        }
    }
    if appendix_re().is_match(trimmed) {
        return ChapterId {
            id: "appendix".to_string(),
            title: trimmed.to_string(),
        };
    }
    ChapterId {
        id: slugify(trimmed),
        title: trimmed.to_string(),
    }
}

/// Replace every `[CITE: text]` in `line` with superscript markup,
/// pushing the noted text into `chapters[current]`. Returns the
/// transformed line.
fn transform_line<'a>(
    line: &'a str,
    chapters: &mut Vec<ChapterGroup>,
    current_chapter: Option<&str>,
    counter: &mut u32,
    total: &mut u32,
    warnings: &mut Vec<String>,
    line_num: u32,
) -> String {
    let re = cite_re();
    let mut out = String::with_capacity(line.len());
    let mut last_end = 0usize;
    for caps in re.captures_iter(line) {
        let mat = caps.get(0).unwrap();
        let text = caps.get(1).unwrap().as_str().trim().to_string();

        out.push_str(&line[last_end..mat.start()]);
        last_end = mat.end();

        let chapter_id = match current_chapter {
            Some(id) => id.to_string(),
            None => {
                warnings.push(format!(
                    "line {}: [CITE:] before any chapter heading; left in place",
                    line_num
                ));
                out.push_str(mat.as_str());
                continue;
            }
        };

        *counter += 1;
        *total += 1;
        let n = *counter;
        let note_id = format!("{}n{}", chapter_id, n);

        // Append to the matching chapter group. We always have one if
        // current_chapter is Some, since the heading branch creates
        // the group.
        if let Some(group) = chapters
            .iter_mut()
            .rev()
            .find(|g| g.id == chapter_id)
        {
            group.notes.push(Note {
                number: n,
                note_id: note_id.clone(),
                text,
            });
        }

        // Body marker. Inline (no surrounding blank lines), so it
        // stays attached to the surrounding text.
        out.push_str("<sup class=\"note-ref\" id=\"");
        out.push_str(&note_id);
        out.push_str("-back\" epub:type=\"noteref\" role=\"doc-noteref\"><a href=\"#");
        out.push_str(&note_id);
        out.push_str("\">");
        out.push_str(&n.to_string());
        out.push_str("</a></sup>");
    }
    out.push_str(&line[last_end..]);
    out
}

/// Typst-flavored variant of [`transform_citations`].
///
/// Same line-walker + ChapterGroup tracking, but body markers and
/// the Notes section are emitted as `<typst>...</typst>` HTML
/// passthrough so comrak's markdown parser preserves them verbatim
/// (the typst emitter detects this sentinel pair and forwards the
/// inner text as raw typst).
///
/// Body marker per `[CITE: text]`:
///   `<typst>#super[#link(label("ch3n7"))[7]] #label("ch3n7-back")</typst>`
/// Notes section (appended before the first `# Appendix` heading,
/// matching the HTML variant):
///   ```
///   # Notes
///   <typst-block>
///   == Chapter Title
///   /  7) note body text   #label("ch3n7")
///   </typst-block>
///   ```
pub fn transform_citations_to_typst(markdown: &str) -> CitationTransformResult {
    let mut output = String::with_capacity(markdown.len() + 1024);
    let mut warnings = Vec::new();
    let mut chapters: Vec<ChapterGroup> = Vec::new();
    let mut current_chapter_id: Option<String> = None;
    let mut counter: u32 = 0;
    let mut total: u32 = 0;
    let mut in_strip = false;

    for (idx, line) in markdown.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        if let Some(caps) = any_heading_re().captures(line) {
            let level = caps.get(1).unwrap().as_str().len();
            let heading_text = caps.get(2).unwrap().as_str().trim();
            if heading_text.eq_ignore_ascii_case(STRIP_HEADING) {
                in_strip = true;
                continue;
            }
            if in_strip {
                in_strip = false;
            }
            if level == 1 {
                let cid = derive_chapter_id(heading_text);
                current_chapter_id = Some(cid.id.clone());
                counter = 0;
                chapters.push(ChapterGroup {
                    id: cid.id,
                    title: cid.title,
                    notes: Vec::new(),
                });
                output.push_str(line);
                output.push('\n');
                continue;
            }
            let transformed = transform_line_typst(
                line,
                &mut chapters,
                current_chapter_id.as_deref(),
                &mut counter,
                &mut total,
                &mut warnings,
                line_num,
            );
            output.push_str(&transformed);
            output.push('\n');
            continue;
        }
        if in_strip {
            continue;
        }
        if verify_re().is_match(line) {
            warnings.push(format!(
                "line {}: [VERIFY:] marker still present — finish citation verification before publishing",
                line_num
            ));
        }
        let transformed = transform_line_typst(
            line,
            &mut chapters,
            current_chapter_id.as_deref(),
            &mut counter,
            &mut total,
            &mut warnings,
            line_num,
        );
        output.push_str(&transformed);
        output.push('\n');
    }

    let chapters_with_notes =
        chapters.iter().filter(|g| !g.notes.is_empty()).count() as u32;
    let mut notes_section = String::new();
    if chapters_with_notes > 0 {
        notes_section.push_str("\n\n# Notes\n\n<typst-block>\n");
        for group in &chapters {
            if group.notes.is_empty() {
                continue;
            }
            notes_section.push_str("== ");
            notes_section.push_str(&typst_escape_string(&group.title));
            notes_section.push_str("\n\n");
            for note in &group.notes {
                notes_section.push_str(&format!(
                    "{}. {} #label(\"{}\")\n\n",
                    note.number,
                    typst_escape_inline(&note.text),
                    note.note_id,
                ));
            }
        }
        notes_section.push_str("</typst-block>\n\n");
    }
    if !notes_section.is_empty() {
        static APPENDIX_INSERT_RE: OnceLock<Regex> = OnceLock::new();
        let appendix_re = APPENDIX_INSERT_RE
            .get_or_init(|| Regex::new(r"(?mi)^#\s+Appendix\b").unwrap());
        if let Some(m) = appendix_re.find(&output) {
            output.insert_str(m.start(), &notes_section);
        } else {
            output.push_str(&notes_section);
        }
    }

    CitationTransformResult {
        transformed: output,
        warnings,
        note_count: total,
        chapters_with_notes,
    }
}

/// typst-flavored sibling of [`transform_line`]. Emits `<typst>...
/// </typst>` body markers carrying typst markup that gets passed
/// through verbatim by the markdown→typst emitter.
fn transform_line_typst<'a>(
    line: &'a str,
    chapters: &mut Vec<ChapterGroup>,
    current_chapter: Option<&str>,
    counter: &mut u32,
    total: &mut u32,
    warnings: &mut Vec<String>,
    line_num: u32,
) -> String {
    let re = cite_re();
    let mut out = String::with_capacity(line.len());
    let mut last_end = 0usize;
    for caps in re.captures_iter(line) {
        let mat = caps.get(0).unwrap();
        let text = caps.get(1).unwrap().as_str().trim().to_string();

        out.push_str(&line[last_end..mat.start()]);
        last_end = mat.end();

        let chapter_id = match current_chapter {
            Some(id) => id.to_string(),
            None => {
                warnings.push(format!(
                    "line {}: [CITE:] before any chapter heading; left in place",
                    line_num
                ));
                out.push_str(mat.as_str());
                continue;
            }
        };

        *counter += 1;
        *total += 1;
        let n = *counter;
        let note_id = format!("{}n{}", chapter_id, n);

        if let Some(group) = chapters
            .iter_mut()
            .rev()
            .find(|g| g.id == chapter_id)
        {
            group.notes.push(Note {
                number: n,
                note_id: note_id.clone(),
                text,
            });
        }

        // Pass-through wrapper. The markdown→typst emitter detects
        // `<typst>` / `</typst>` and emits the contents verbatim.
        out.push_str("<typst>#super[#link(label(\"");
        out.push_str(&note_id);
        out.push_str("\"))[");
        out.push_str(&n.to_string());
        out.push_str("]]</typst>");
    }
    out.push_str(&line[last_end..]);
    out
}

/// Escape a string literal for embedding inside typst double-quoted
/// strings. Only `"` and `\\` need escaping.
fn typst_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Escape a note body for embedding inside the typst Notes section,
/// where it's read as markup mode (not a string literal). Currently
/// minimal: we trust the note text was authored as plain prose.
fn typst_escape_inline(s: &str) -> String {
    // Notes can contain `[`, `]`, `#`, etc. typst's markup escape is
    // `\`. Keep curly quotes / em-dashes intact.
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '#' | '*' | '_' | '<' | '[' | ']' | '@' | '`' | '$' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

pub fn transform_citations(markdown: &str) -> CitationTransformResult {
    let mut output = String::with_capacity(markdown.len() + 1024);
    let mut warnings = Vec::new();
    let mut chapters: Vec<ChapterGroup> = Vec::new();
    let mut current_chapter_id: Option<String> = None;
    let mut counter: u32 = 0;
    let mut total: u32 = 0;
    let mut in_strip = false;

    for (idx, line) in markdown.lines().enumerate() {
        let line_num = (idx + 1) as u32;

        // Detect any heading level — the strip-section logic must work
        // regardless of whether the legend is `#`, `##`, etc., and we
        // want to continue counting citations under H2/H3 subsections
        // within a chapter without resetting the counter.
        if let Some(caps) = any_heading_re().captures(line) {
            let level = caps.get(1).unwrap().as_str().len();
            let heading_text = caps.get(2).unwrap().as_str().trim();

            // Strip-section heading (any level): begin stripping.
            if heading_text.eq_ignore_ascii_case(STRIP_HEADING) {
                in_strip = true;
                continue;
            }

            // Any other heading ends a strip mode (and we then process
            // it normally below).
            if in_strip {
                in_strip = false;
            }

            if level == 1 {
                // H1 starts a new chapter scope.
                let cid = derive_chapter_id(heading_text);
                current_chapter_id = Some(cid.id.clone());
                counter = 0;
                chapters.push(ChapterGroup {
                    id: cid.id,
                    title: cid.title,
                    notes: Vec::new(),
                });
                output.push_str(line);
                output.push('\n');
                continue;
            }

            // H2/H3/... pass through; counter doesn't reset, but
            // citations in the heading line itself still transform.
            let transformed = transform_line(
                line,
                &mut chapters,
                current_chapter_id.as_deref(),
                &mut counter,
                &mut total,
                &mut warnings,
                line_num,
            );
            output.push_str(&transformed);
            output.push('\n');
            continue;
        }

        if in_strip {
            // Skip body lines inside the stripped citation-markers legend.
            continue;
        }

        // Warn on [VERIFY:] markers in body content.
        if verify_re().is_match(line) {
            warnings.push(format!(
                "line {}: [VERIFY:] marker still present — finish citation verification before publishing",
                line_num
            ));
        }

        // Transform [CITE:] markers in this line.
        let transformed = transform_line(
            line,
            &mut chapters,
            current_chapter_id.as_deref(),
            &mut counter,
            &mut total,
            &mut warnings,
            line_num,
        );
        output.push_str(&transformed);
        output.push('\n');
    }

    // Build the # Notes back-matter section into a separate buffer
    // first, then insert it BEFORE the first `# Appendix*` heading
    // if one exists. Notes belong with the chapters they reference,
    // so they should come before the appendix(es) — not stuck at the
    // very end after all back matter. Fallback: append at end when no
    // appendix is present.
    let chapters_with_notes = chapters.iter().filter(|g| !g.notes.is_empty()).count() as u32;
    let mut notes_section = String::new();
    if chapters_with_notes > 0 {
        notes_section.push_str("\n\n# Notes\n\n");
        for group in &chapters {
            if group.notes.is_empty() {
                continue;
            }
            notes_section.push_str(&format!("## {}\n\n", group.title));
            // Raw HTML <ol> + <li> for stable IDs and back-links.
            // pandoc's `markdown_in_html_blocks` (default in our
            // invocation) processes the markdown text inside.
            notes_section.push_str("<ol class=\"notes-list\">\n\n");
            for note in &group.notes {
                notes_section.push_str("<li id=\"");
                notes_section.push_str(&note.note_id);
                notes_section.push_str("\" epub:type=\"endnote\" role=\"doc-endnote\">\n\n<span class=\"note-num\">");
                notes_section.push_str(&note.number.to_string());
                notes_section.push_str(".</span> ");
                notes_section.push_str(&note.text);
                notes_section.push_str(" <a href=\"#");
                notes_section.push_str(&note.note_id);
                notes_section.push_str("-back\" class=\"note-back\" aria-label=\"back to text\">↩</a>\n\n</li>\n\n");
            }
            notes_section.push_str("</ol>\n\n");
        }
    }
    if !notes_section.is_empty() {
        // Find the first H1 that starts with "Appendix". Multi-line
        // mode so `^` matches the start of any line.
        static APPENDIX_INSERT_RE: OnceLock<Regex> = OnceLock::new();
        let appendix_re = APPENDIX_INSERT_RE
            .get_or_init(|| Regex::new(r"(?mi)^#\s+Appendix\b").unwrap());
        if let Some(m) = appendix_re.find(&output) {
            output.insert_str(m.start(), &notes_section);
        } else {
            output.push_str(&notes_section);
        }
    }

    CitationTransformResult {
        transformed: output,
        warnings,
        note_count: total,
        chapters_with_notes,
    }
}

/// Rewrite precomposed Unicode superscript/subscript characters (e.g.
/// `10⁻³⁵`, `|ψ|²`, `K₂⁰`, `ℓₚ`) into real `<sup>`/`<sub>` markup
/// carrying ordinary ASCII glyphs. Contiguous runs of the same script
/// collapse into a single element, so `10⁻³⁵` becomes
/// `10<sup>-35</sup>` rather than three separate tags.
///
/// Why: EB Garamond's embedded subset covers the Latin-1 superscripts
/// (¹ ² ³) but not the "Superscripts and Subscripts" block (⁰ ⁴ ⁵ ⁻ …
/// ₀ ₂ ₚ …), so Chromium silently falls back to a system font for the
/// missing glyphs. The result: the two digits inside one exponent
/// render in different typefaces, and KDP may reject the fallback glyph
/// as not embedded. Routing every super/subscript through `<sup>`/`<sub>`
/// with plain digits lets the global `sup, sub` CSS rule pin them to the
/// embedded font, so they all match.
///
/// Runs *after* pandoc, on the rendered HTML, so both the body and the
/// generated Notes section are covered in one pass. Super/subscript
/// characters never appear inside HTML tags or attributes, so a flat
/// character scan is safe.
pub fn normalize_unicode_scripts(html: &str) -> String {
    #[derive(PartialEq, Clone, Copy)]
    enum Kind {
        Sup,
        Sub,
    }

    fn classify(c: char) -> Option<(Kind, char)> {
        let m = match c {
            // Superscripts (Latin-1 + Superscripts/Subscripts block).
            '\u{2070}' => (Kind::Sup, '0'),
            '\u{00B9}' => (Kind::Sup, '1'),
            '\u{00B2}' => (Kind::Sup, '2'),
            '\u{00B3}' => (Kind::Sup, '3'),
            '\u{2074}' => (Kind::Sup, '4'),
            '\u{2075}' => (Kind::Sup, '5'),
            '\u{2076}' => (Kind::Sup, '6'),
            '\u{2077}' => (Kind::Sup, '7'),
            '\u{2078}' => (Kind::Sup, '8'),
            '\u{2079}' => (Kind::Sup, '9'),
            '\u{207A}' => (Kind::Sup, '+'),
            '\u{207B}' => (Kind::Sup, '-'),
            '\u{207C}' => (Kind::Sup, '='),
            '\u{207D}' => (Kind::Sup, '('),
            '\u{207E}' => (Kind::Sup, ')'),
            '\u{207F}' => (Kind::Sup, 'n'),
            '\u{2071}' => (Kind::Sup, 'i'),
            // Subscripts.
            '\u{2080}' => (Kind::Sub, '0'),
            '\u{2081}' => (Kind::Sub, '1'),
            '\u{2082}' => (Kind::Sub, '2'),
            '\u{2083}' => (Kind::Sub, '3'),
            '\u{2084}' => (Kind::Sub, '4'),
            '\u{2085}' => (Kind::Sub, '5'),
            '\u{2086}' => (Kind::Sub, '6'),
            '\u{2087}' => (Kind::Sub, '7'),
            '\u{2088}' => (Kind::Sub, '8'),
            '\u{2089}' => (Kind::Sub, '9'),
            '\u{208A}' => (Kind::Sub, '+'),
            '\u{208B}' => (Kind::Sub, '-'),
            '\u{208C}' => (Kind::Sub, '='),
            '\u{208D}' => (Kind::Sub, '('),
            '\u{208E}' => (Kind::Sub, ')'),
            '\u{2090}' => (Kind::Sub, 'a'),
            '\u{2091}' => (Kind::Sub, 'e'),
            '\u{2092}' => (Kind::Sub, 'o'),
            '\u{2093}' => (Kind::Sub, 'x'),
            '\u{2095}' => (Kind::Sub, 'h'),
            '\u{2096}' => (Kind::Sub, 'k'),
            '\u{2097}' => (Kind::Sub, 'l'),
            '\u{2098}' => (Kind::Sub, 'm'),
            '\u{2099}' => (Kind::Sub, 'n'),
            '\u{209A}' => (Kind::Sub, 'p'),
            '\u{209B}' => (Kind::Sub, 's'),
            '\u{209C}' => (Kind::Sub, 't'),
            '\u{1D62}' => (Kind::Sub, 'i'),
            '\u{1D63}' => (Kind::Sub, 'r'),
            '\u{1D64}' => (Kind::Sub, 'u'),
            '\u{1D65}' => (Kind::Sub, 'v'),
            '\u{2C7C}' => (Kind::Sub, 'j'),
            _ => return None,
        };
        Some(m)
    }

    fn flush(out: &mut String, run: &mut Option<Kind>, buf: &mut String) {
        if let Some(k) = run.take() {
            let tag = match k {
                Kind::Sup => "sup",
                Kind::Sub => "sub",
            };
            out.push('<');
            out.push_str(tag);
            out.push('>');
            out.push_str(buf);
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
            buf.clear();
        }
    }

    let mut out = String::with_capacity(html.len() + 16);
    let mut run: Option<Kind> = None;
    let mut buf = String::new();

    for c in html.chars() {
        match classify(c) {
            Some((k, ascii)) => {
                if run != Some(k) {
                    flush(&mut out, &mut run, &mut buf);
                    run = Some(k);
                }
                buf.push(ascii);
            }
            None => {
                flush(&mut out, &mut run, &mut buf);
                out.push(c);
            }
        }
    }
    flush(&mut out, &mut run, &mut buf);
    out
}

/// Tag "Math Anchor" callout blockquotes with `class="math-anchor"` so
/// the CSS can render them as boxed asides instead of plain pull-quotes.
///
/// The manuscript writes them as blockquotes opening with
/// `**Math Anchor — ...**`, which pandoc renders as
/// `<blockquote>\n<p><strong>Math Anchor ...`. Only those blockquotes
/// get the class — ordinary blockquotes (epigraphs, dialogue) are left
/// as-is. Runs post-pandoc on the rendered HTML.
pub fn tag_math_anchors(html: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"<blockquote>(\s*<p><strong>Math Anchor)"#).unwrap()
    });
    re.replace_all(html, r#"<blockquote class="math-anchor">$1"#)
        .into_owned()
}

/// Pandoc's `implicit_figures` puts the image's attributes (classes,
/// inline style) on the inner `<img>`, but the figure-layout CSS targets
/// the `<figure>` wrapper — block-level margin escapes, page breaks, and
/// the `--fig-*` custom properties only work there. Hoist the `fig-*`
/// classes and `--fig-*` style declarations from the `<img>` up onto its
/// parent `<figure>` so `figure.fig-bleed`, `figure.fig-fullpage`,
/// `figure.fig-crop`, etc. match. Width and other img styles stay put.
pub fn hoist_figure_classes(html: &str) -> String {
    static FIG_RE: OnceLock<Regex> = OnceLock::new();
    static CLASS_RE: OnceLock<Regex> = OnceLock::new();
    static STYLE_RE: OnceLock<Regex> = OnceLock::new();
    let fig_re = FIG_RE
        .get_or_init(|| Regex::new(r#"(?s)(<figure\b)([^>]*)(>\s*<img\b)([^>]*?)(/?>)"#).unwrap());
    let class_re = CLASS_RE.get_or_init(|| Regex::new(r#"class="([^"]*)""#).unwrap());
    let style_re = STYLE_RE.get_or_init(|| Regex::new(r#"style="([^"]*)""#).unwrap());

    fig_re
        .replace_all(html, |c: &regex::Captures| {
            let img_attrs = &c[4];
            let classes: Vec<&str> = class_re
                .captures(img_attrs)
                .map(|m| {
                    m.get(1)
                        .unwrap()
                        .as_str()
                        .split_whitespace()
                        .filter(|cls| cls.starts_with("fig-"))
                        .collect()
                })
                .unwrap_or_default();
            let props: Vec<&str> = style_re
                .captures(img_attrs)
                .map(|m| {
                    m.get(1)
                        .unwrap()
                        .as_str()
                        .split(';')
                        .map(|d| d.trim())
                        .filter(|d| d.starts_with("--fig-"))
                        .collect()
                })
                .unwrap_or_default();
            if classes.is_empty() && props.is_empty() {
                return c[0].to_string();
            }
            // Figure normally carries only an id; append class/style.
            let mut fig_attrs = c[2].trim_end().to_string();
            if !classes.is_empty() {
                fig_attrs.push_str(&format!(" class=\"{}\"", classes.join(" ")));
            }
            if !props.is_empty() {
                fig_attrs.push_str(&format!(" style=\"{}\"", props.join("; ")));
            }
            format!("{}{}{}{}{}", &c[1], fig_attrs, &c[3], img_attrs, &c[5])
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hoists_fig_class_and_props_to_figure() {
        // Exactly the shape pandoc's implicit_figures emits: layout class
        // + custom props on the <img>, only id on the <figure>.
        let html = "<figure id=\"chfig-a\">\n<img src=\"x.png\" class=\"fig-bleed\"\n\
style=\"--fig-inset:0.25in;width:60.0%\"\nalt=\"cap\" />\n\
<figcaption>cap</figcaption>\n</figure>";
        let out = hoist_figure_classes(html);
        // Class + custom prop now on the figure (so figure.fig-bleed matches).
        assert!(out.contains("<figure id=\"chfig-a\" class=\"fig-bleed\" style=\"--fig-inset:0.25in\">"));
        // width stays an img concern.
        assert!(out.contains("width:60.0%"));
    }

    #[test]
    fn hoist_leaves_plain_figures_alone() {
        let html = "<figure id=\"chfig-z\">\n<img src=\"x.png\" alt=\"cap\" />\n</figure>";
        assert_eq!(hoist_figure_classes(html), html);
    }

    #[test]
    fn replaces_single_marker() {
        let md = "# Chapter 1: Hook\n\nText [CITE: Smith 2020] more.\n";
        let r = transform_citations(md);
        assert!(r.transformed.contains(r#"<sup class="note-ref""#));
        assert!(r.transformed.contains(r#"href="#ch1n1""#));
        assert_eq!(r.note_count, 1);
    }

    #[test]
    fn per_chapter_counter_resets() {
        let md = "\
# Chapter 1: A
[CITE: a] [CITE: b]
# Chapter 2: B
[CITE: c]
";
        let r = transform_citations(md);
        assert!(r.transformed.contains("ch1n1"));
        assert!(r.transformed.contains("ch1n2"));
        assert!(r.transformed.contains("ch2n1"));
        assert_eq!(r.note_count, 3);
    }

    #[test]
    fn strips_note_on_citation_markers() {
        let md = "\
# Note on Citation Markers
This explains the [CITE:] convention.
With multiple lines.

# Author's Note
Hello.
";
        let r = transform_citations(md);
        assert!(!r.transformed.contains("Note on Citation Markers"));
        assert!(r.transformed.contains("Author's Note"));
    }

    #[test]
    fn warns_on_verify_markers() {
        let md = "# Chapter 1\nFoo [VERIFY: needs check] bar.\n";
        let r = transform_citations(md);
        assert!(!r.warnings.is_empty());
        assert!(r.warnings[0].contains("VERIFY"));
    }

    #[test]
    fn preserves_markdown_in_note_text() {
        let md = "# Chapter 1\n[CITE: Smith, *Title*, 2020]\n";
        let r = transform_citations(md);
        // Note section should contain the markdown italic markers
        // (pandoc will process them via markdown_in_html_blocks).
        assert!(r.transformed.contains("*Title*"));
    }

    #[test]
    fn interlude_uses_interlude_id() {
        let md = "# Interlude II — The Student\n[CITE: foo]\n";
        let r = transform_citations(md);
        assert!(r.transformed.contains("interlude2n1"));
    }

    #[test]
    fn normalizes_unicode_superscripts() {
        // Runs of the same script collapse into one tag; ASCII glyphs.
        assert_eq!(normalize_unicode_scripts("10\u{207B}\u{00B3}\u{2075}"), "10<sup>-35</sup>");
        // Latin-1 superscript digit also normalized.
        assert_eq!(normalize_unicode_scripts("|\u{03C8}|\u{00B2}"), "|\u{03C8}|<sup>2</sup>");
        // Mixed sub/superscript split into separate tags (K subscript-2 superscript-0).
        assert_eq!(normalize_unicode_scripts("K\u{2082}\u{2070}"), "K<sub>2</sub><sup>0</sup>");
        // Subscript letter (Planck ℓ_p).
        assert_eq!(normalize_unicode_scripts("\u{2113}\u{209A}"), "\u{2113}<sub>p</sub>");
        // No scripts → unchanged.
        assert_eq!(normalize_unicode_scripts("plain text"), "plain text");
    }

    #[test]
    fn tags_math_anchor_blockquotes_only() {
        let html = "<blockquote>\n<p><strong>Math Anchor — Newton</strong>: x</p>\n</blockquote>\
                    <blockquote>\n<p>An ordinary epigraph.</p>\n</blockquote>";
        let out = tag_math_anchors(html);
        assert!(out.contains(r#"<blockquote class="math-anchor">"#));
        // The ordinary blockquote stays untagged.
        assert_eq!(out.matches(r#"class="math-anchor""#).count(), 1);
        assert!(out.contains("<blockquote>\n<p>An ordinary epigraph."));
    }

    #[test]
    fn cite_before_chapter_warns_and_passes_through() {
        let md = "Some preamble [CITE: orphan] here.\n# Chapter 1\n";
        let r = transform_citations(md);
        assert!(r.warnings.iter().any(|w| w.contains("before any chapter")));
        assert!(r.transformed.contains("[CITE: orphan]"));
    }
}
