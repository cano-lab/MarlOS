//! PDF → markdown import. Pure Rust via the already-bundled
//! `pdfium-render` crate — no external binaries, no Python sidecar
//! (the [[feedback-marlos-rust-only]] constraint).
//!
//! What this is good at:
//!   - Linear prose books with clear paragraph breaks.
//!   - Chapter headings styled obviously ("CHAPTER 3", "Chapter 3 — Title").
//!   - Numbered section headings ("3.1 The Argument").
//!   - Stripping running headers, footers, and lone page-number lines.
//!
//! What it's bad at (and the user should hand-fix in the Source view):
//!   - Math — Unicode math glyphs from a PDF rarely reconstruct as
//!     valid TeX; expect to wrap formulas in `$...$` by hand.
//!   - Tables, figures, captions — there's no semantic figure
//!     concept in the extracted text; figures show up as missing,
//!     captions as orphan paragraphs.
//!   - Multi-column layouts — pdfium reads columns top-to-bottom,
//!     left-to-right but mistakes are common; verify chapter starts.
//!   - Footnotes — they land inline at the bottom of the page they
//!     came from; you'll want to convert them to `[CITE:]` markers.
//!
//! Heuristics that survived contact with reality:
//!   - A line is a heading candidate if it's short (≤ 60 chars),
//!     surrounded by blank lines, lacks terminal punctuation, and
//!     starts with a capital. Strong markers ("CHAPTER N", "N. Title")
//!     bypass the heuristic.
//!   - Running headers / footers: any line that appears verbatim on
//!     >= 30% of pages, and is < 80 chars, is stripped from all pages.
//!   - Page numbers: lone short numeric / roman lines (≤ 5 chars,
//!     digits or roman, no other text) at the first or last position
//!     in a page get stripped.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use pdfium_render::prelude::*;
use regex::Regex;

#[derive(Debug, thiserror::Error)]
pub enum PdfImportError {
    #[error("PDFium error: {0}")]
    Pdfium(#[from] PdfiumError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid PDF path: {0}")]
    InvalidPath(PathBuf),
}

/// Result of an import. `markdown` is the body; `notes` is a list of
/// non-fatal warnings to surface to the user ("X pages had no extractable
/// text — likely scanned images, run OCR before importing").
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub markdown: String,
    pub page_count: u32,
    pub notes: Vec<String>,
}

/// Convert a PDF file to a markdown string. Pure-Rust pipeline:
/// pdfium → page text → cleanup → heading inference → markdown.
pub fn convert(pdf_path: &Path) -> Result<ImportResult, PdfImportError> {
    if !pdf_path.is_file() {
        return Err(PdfImportError::InvalidPath(pdf_path.to_path_buf()));
    }
    let pdfium = init_pdfium()?;
    let doc = pdfium.load_pdf_from_file(pdf_path, None)?;

    // Extract per-page raw text.
    let mut pages: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let page_count = doc.pages().len() as u32;
    let mut empty_pages = 0usize;
    for page in doc.pages().iter() {
        let raw = page.text().map(|t| t.all()).unwrap_or_default();
        if raw.trim().is_empty() {
            empty_pages += 1;
        }
        pages.push(raw);
    }
    if empty_pages > 0 {
        notes.push(format!(
            "{} of {} pages had no extractable text (scanned image PDFs need OCR first).",
            empty_pages, page_count,
        ));
    }

    // Normalize, dehyphenate line breaks, split into lines.
    let mut page_lines: Vec<Vec<String>> = pages
        .into_iter()
        .map(|p| normalize_page(&p))
        .collect();

    // Strip running headers / footers (lines repeating across pages).
    let chrome: HashSet<String> = detect_repeated_chrome(&page_lines);
    if !chrome.is_empty() {
        for lines in page_lines.iter_mut() {
            lines.retain(|l| !chrome.contains(l.trim()));
        }
        notes.push(format!(
            "Stripped {} repeated header/footer line(s).",
            chrome.len()
        ));
    }

    // Strip lone page-number lines (first or last position).
    let stripped_nums = strip_page_numbers(&mut page_lines);
    if stripped_nums > 0 {
        notes.push(format!(
            "Stripped {} page-number line(s).",
            stripped_nums
        ));
    }

    // Stitch pages into one body, joining paragraphs that bleed across
    // a page break.
    let body_lines = stitch_pages(page_lines);

    // Heading inference + markdown emission.
    let markdown = render_markdown(&body_lines);

    Ok(ImportResult {
        markdown,
        page_count,
        notes,
    })
}

/// Resolve a PDFium instance. Same lookup chain `pdf.rs` uses: try
/// the working-directory bundled binary first, then `./lib/`, then
/// fall back to a system install.
fn init_pdfium() -> Result<Pdfium, PdfImportError> {
    let bindings = Pdfium::bind_to_library(
        Pdfium::pdfium_platform_library_name_at_path("./"),
    )
    .or_else(|_| {
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./lib/"))
    })
    .or_else(|_| Pdfium::bind_to_system_library())?;
    Ok(Pdfium::new(bindings))
}

/// Strip CRs, fix hyphenated line-wraps, and split into a Vec of
/// trimmed lines. We keep blank lines as empty strings so paragraph
/// detection downstream can see the gaps.
fn normalize_page(raw: &str) -> Vec<String> {
    let cleaned = raw.replace('\r', "");
    // Pdfium's text-extraction often inserts soft hyphens or
    // line-break hyphens (`word-\nbroken` → "word-broken"). Join
    // those before splitting into lines.
    let dehy = dehyphenate(&cleaned);
    dehy.split('\n')
        .map(|l| collapse_internal_ws(l.trim()))
        .collect()
}

fn collapse_internal_ws(s: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"[ \t\u{00A0}]+").unwrap());
    re.replace_all(s, " ").into_owned()
}

/// Join lines split by a trailing hyphen: `inter-\nesting` → `interesting`.
/// We only join when the next line starts with a lowercase letter; an
/// uppercase next-line probably means the hyphen is a real one (a
/// compound word at line end).
fn dehyphenate(s: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(\p{L})-\n(\p{Ll})").unwrap());
    re.replace_all(s, "$1$2").into_owned()
}

/// Detect short lines that repeat across many pages — these are the
/// running header, the running footer, or the book title strip. We
/// treat ≥30% repetition as the threshold (matching the typesetter's
/// own running-header behavior, which lands on every body page).
fn detect_repeated_chrome(pages: &[Vec<String>]) -> HashSet<String> {
    if pages.len() < 4 {
        // Too few pages to trust the heuristic.
        return HashSet::new();
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for lines in pages {
        // Only inspect the first and last few lines — chrome lives
        // in the page header/footer, not in body prose.
        let mut bands: Vec<&String> = Vec::new();
        bands.extend(lines.iter().take(3));
        bands.extend(lines.iter().rev().take(3));
        for line in bands {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.len() > 80 {
                continue;
            }
            *counts.entry(trimmed.to_string()).or_insert(0) += 1;
        }
    }
    let threshold = (pages.len() as f32 * 0.3).ceil() as usize;
    counts
        .into_iter()
        .filter(|(_, n)| *n >= threshold)
        .map(|(line, _)| line)
        .collect()
}

/// Drop lone page-number lines at the top or bottom of a page. The
/// numbering style can be arabic (1, 2, 3) or roman (i, ii, iii / I,
/// II, III). Returns the count stripped for the summary line.
fn strip_page_numbers(pages: &mut [Vec<String>]) -> usize {
    let mut stripped = 0usize;
    for lines in pages.iter_mut() {
        // First-position page number.
        if let Some(first) = lines.first() {
            if is_page_number(first.trim()) {
                lines.remove(0);
                stripped += 1;
            }
        }
        // Last-position page number.
        if let Some(last) = lines.last() {
            if is_page_number(last.trim()) {
                lines.pop();
                stripped += 1;
            }
        }
    }
    stripped
}

fn is_page_number(line: &str) -> bool {
    if line.is_empty() || line.len() > 6 {
        return false;
    }
    // Pure digits.
    if line.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // Lower or upper roman numerals.
    let upper = line.chars().all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'));
    let lower = line.chars().all(|c| matches!(c, 'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm'));
    upper || lower
}

/// Stitch pages together. The connective tissue is the boundary
/// between the last line of page N and the first line of page N+1:
/// if both look like the middle of a sentence (no terminal punctuation
/// at the end of page N, lowercase start at page N+1), we glue them
/// onto one line. Otherwise we insert a blank line so the
/// paragraph-detection downstream sees a break.
fn stitch_pages(pages: Vec<Vec<String>>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, lines) in pages.into_iter().enumerate() {
        if i > 0 {
            // Decide whether to glue or break.
            let prev_last = out.iter().rev().find(|l| !l.trim().is_empty()).cloned();
            let next_first = lines.iter().find(|l| !l.trim().is_empty()).cloned();
            let glue = match (prev_last, next_first) {
                (Some(p), Some(n)) => paragraph_continues(p.trim(), n.trim()),
                _ => false,
            };
            if glue {
                // Pop trailing blank lines, then concatenate the first
                // non-blank line of the new page onto the previous.
                while out.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
                    out.pop();
                }
                // Find first non-blank index in `lines`.
                let mut iter = lines.into_iter();
                while let Some(l) = iter.next() {
                    if !l.trim().is_empty() {
                        if let Some(last) = out.last_mut() {
                            last.push(' ');
                            last.push_str(l.trim());
                        } else {
                            out.push(l);
                        }
                        break;
                    }
                }
                for l in iter {
                    out.push(l);
                }
                continue;
            } else {
                // Ensure exactly one blank line between pages.
                if !out.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
                    out.push(String::new());
                }
            }
        }
        for l in lines {
            out.push(l);
        }
    }
    out
}

fn paragraph_continues(prev_last: &str, next_first: &str) -> bool {
    // If the previous line ends with sentence-final punctuation, the
    // paragraph is closed.
    let last_ch = prev_last.chars().rev().find(|c| !c.is_whitespace());
    let closed = matches!(last_ch, Some('.') | Some('!') | Some('?') | Some('»') | Some('"'));
    if closed {
        return false;
    }
    // If the next page starts with a heading-shaped line, don't glue.
    if is_strong_heading(next_first).is_some() {
        return false;
    }
    // If the next page starts with a lowercase letter or an em-dash,
    // it's almost certainly a continuation.
    let first_ch = next_first.chars().next();
    matches!(first_ch, Some(c) if c.is_lowercase() || c == '—' || c == ',' || c == ';')
}

/// Strong (unambiguous) heading detection. Returns Some(level) where
/// 1 = chapter, 2 = section. None means "use the soft heuristic
/// downstream".
fn is_strong_heading(line: &str) -> Option<u8> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    // "CHAPTER 3" / "Chapter 3" / "Chapter Three" / "CHAPTER III"
    static CH_RE: OnceLock<Regex> = OnceLock::new();
    let ch_re = CH_RE.get_or_init(|| {
        Regex::new(r"^(?i)chapter\s+([0-9ivxlcdm]+|[a-z\-]+)(\s*[—:.\-]\s*.+)?$").unwrap()
    });
    if ch_re.is_match(trimmed) {
        return Some(1);
    }
    // Numbered section "3.1 Title" — only when followed by a capitalized
    // word (avoids matching "3.14 is pi").
    static NUM_RE: OnceLock<Regex> = OnceLock::new();
    let num_re =
        NUM_RE.get_or_init(|| Regex::new(r"^\d+(\.\d+){1,3}\s+\p{Lu}\p{L}+").unwrap());
    if num_re.is_match(trimmed) {
        return Some(2);
    }
    None
}

/// Soft heading detection. A line is a "probable heading" when:
///   - Length 2..60
///   - No sentence-terminal punctuation
///   - Doesn't start with a lowercase letter
///   - The surrounding lines are blank
///   - It's not all numbers / not all-symbol noise
/// Returns the heading level: 1 for ALL-CAPS short lines, 2 otherwise.
fn is_soft_heading(line: &str) -> Option<u8> {
    let trimmed = line.trim();
    let len = trimmed.chars().count();
    if !(2..=60).contains(&len) {
        return None;
    }
    let last_ch = trimmed.chars().rev().find(|c| !c.is_whitespace())?;
    if matches!(last_ch, '.' | '!' | '?' | ';' | ':' | ',') {
        return None;
    }
    let first_ch = trimmed.chars().next()?;
    if first_ch.is_lowercase() {
        return None;
    }
    if !trimmed.chars().any(|c| c.is_alphabetic()) {
        return None;
    }
    // ALL CAPS short line — chapter-level.
    let alphabetic: String = trimmed.chars().filter(|c| c.is_alphabetic()).collect();
    if alphabetic.chars().all(|c| c.is_uppercase()) && alphabetic.chars().count() >= 3 {
        return Some(1);
    }
    Some(2)
}

/// Walk the stitched body lines, decide heading vs paragraph for each,
/// and emit markdown.
fn render_markdown(lines: &[String]) -> String {
    let mut out = String::new();
    // Index of the next non-blank line, used to look ahead for the
    // "surrounded by blanks" heading test.
    let n = lines.len();
    let is_blank = |idx: i64| -> bool {
        if idx < 0 || idx as usize >= n {
            return true;
        }
        lines[idx as usize].trim().is_empty()
    };

    let mut paragraph: Vec<String> = Vec::new();
    let flush = |para: &mut Vec<String>, out: &mut String| {
        if para.is_empty() {
            return;
        }
        let joined = para.join(" ");
        out.push_str(joined.trim());
        out.push_str("\n\n");
        para.clear();
    };

    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut paragraph, &mut out);
            continue;
        }

        // Strong heading: emit immediately.
        if let Some(level) = is_strong_heading(line) {
            flush(&mut paragraph, &mut out);
            let prefix = if level == 1 { "# " } else { "## " };
            out.push_str(prefix);
            out.push_str(line);
            out.push_str("\n\n");
            continue;
        }

        // Soft heading: only fire when surrounded by blanks on both
        // sides — otherwise short capitalized lines (figure captions,
        // dedications) get mis-promoted.
        let surrounded =
            is_blank(i as i64 - 1) && is_blank(i as i64 + 1);
        if surrounded {
            if let Some(level) = is_soft_heading(line) {
                flush(&mut paragraph, &mut out);
                let prefix = if level == 1 { "# " } else { "## " };
                out.push_str(prefix);
                // Title-case ALL-CAPS so the manuscript reads naturally.
                let titled = if level == 1 && line.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()) {
                    to_title_case(line)
                } else {
                    line.to_string()
                };
                out.push_str(&titled);
                out.push_str("\n\n");
                continue;
            }
        }

        // Body line: accumulate into the current paragraph.
        paragraph.push(line.to_string());
    }
    flush(&mut paragraph, &mut out);

    // Trim leading blanks; collapse triple-blanks.
    static MULTIBLANK: OnceLock<Regex> = OnceLock::new();
    let mb = MULTIBLANK.get_or_init(|| Regex::new(r"\n{3,}").unwrap());
    mb.replace_all(out.trim_start(), "\n\n").into_owned()
}

fn to_title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut at_word_start = true;
    for c in s.chars() {
        if c.is_whitespace() || c == '-' || c == '—' {
            out.push(c);
            at_word_start = true;
        } else if at_word_start {
            for u in c.to_uppercase() {
                out.push(u);
            }
            at_word_start = false;
        } else {
            for l in c.to_lowercase() {
                out.push(l);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_chapter_heading() {
        assert_eq!(is_strong_heading("Chapter 3"), Some(1));
        assert_eq!(is_strong_heading("CHAPTER VII"), Some(1));
        assert_eq!(is_strong_heading("Chapter 3 — The Argument"), Some(1));
        assert_eq!(is_strong_heading("chapter and verse"), None);
    }

    #[test]
    fn detects_numbered_section() {
        assert_eq!(is_strong_heading("3.1 Background"), Some(2));
        assert_eq!(is_strong_heading("3.14 is pi"), None);
    }

    #[test]
    fn soft_heading_requires_blanks() {
        // Surrounded-by-blank test happens in render; here we just
        // check the line-shape heuristic.
        assert_eq!(is_soft_heading("The Argument"), Some(2));
        assert_eq!(is_soft_heading("AN ENTIRELY SHORT TITLE"), Some(1));
        assert_eq!(is_soft_heading("This is a full sentence."), None);
        assert_eq!(is_soft_heading("running over the line edge"), None);
    }

    #[test]
    fn page_number_recognition() {
        assert!(is_page_number("12"));
        assert!(is_page_number("iv"));
        assert!(is_page_number("XII"));
        assert!(!is_page_number("Page 12"));
        assert!(!is_page_number("12345678")); // too long
    }

    #[test]
    fn dehyphenates_line_wraps() {
        let s = "inter-\nesting";
        assert_eq!(dehyphenate(s), "interesting");
        // Don't join if next line starts upper — could be a real compound.
        let s2 = "Anglo-\nSaxon";
        assert_eq!(dehyphenate(s2), "Anglo-\nSaxon");
    }

    #[test]
    fn renders_a_minimal_book() {
        let lines: Vec<String> = vec![
            "Chapter 1".into(),
            "".into(),
            "This is the opening paragraph.".into(),
            "It runs onto a second line.".into(),
            "".into(),
            "And a second paragraph.".into(),
            "".into(),
            "3.1 A Section".into(),
            "".into(),
            "Section body.".into(),
        ];
        let md = render_markdown(&lines);
        assert!(md.starts_with("# Chapter 1\n\n"));
        assert!(md.contains("This is the opening paragraph. It runs onto a second line.\n\n"));
        assert!(md.contains("## 3.1 A Section\n\n"));
        assert!(md.contains("Section body."));
    }
}
