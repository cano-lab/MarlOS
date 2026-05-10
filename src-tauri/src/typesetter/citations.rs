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
    CITE_RE.get_or_init(|| Regex::new(r"(?s)\[CITE:\s*(.+?)\]").unwrap())
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

    // Emit the # Notes back-matter section.
    let chapters_with_notes = chapters.iter().filter(|g| !g.notes.is_empty()).count() as u32;
    if chapters_with_notes > 0 {
        output.push_str("\n\n# Notes\n\n");
        for group in &chapters {
            if group.notes.is_empty() {
                continue;
            }
            output.push_str(&format!("## {}\n\n", group.title));
            // Raw HTML <ol> + <li> for stable IDs and back-links.
            // pandoc's `markdown_in_html_blocks` (default in our
            // invocation) processes the markdown text inside.
            output.push_str("<ol class=\"notes-list\">\n\n");
            for note in &group.notes {
                output.push_str("<li id=\"");
                output.push_str(&note.note_id);
                output.push_str("\" epub:type=\"endnote\" role=\"doc-endnote\">\n\n<span class=\"note-num\">");
                output.push_str(&note.number.to_string());
                output.push_str(".</span> ");
                output.push_str(&note.text);
                output.push_str(" <a href=\"#");
                output.push_str(&note.note_id);
                output.push_str("-back\" class=\"note-back\" aria-label=\"back to text\">↩</a>\n\n</li>\n\n");
            }
            output.push_str("</ol>\n\n");
        }
    }

    CitationTransformResult {
        transformed: output,
        warnings,
        note_count: total,
        chapters_with_notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn cite_before_chapter_warns_and_passes_through() {
        let md = "Some preamble [CITE: orphan] here.\n# Chapter 1\n";
        let r = transform_citations(md);
        assert!(r.warnings.iter().any(|w| w.contains("before any chapter")));
        assert!(r.transformed.contains("[CITE: orphan]"));
    }
}
