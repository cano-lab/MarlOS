//! Markdown-AST → `BookStructure` (sections classified as
//! front-matter / chapter / interlude / back-matter).
//!
//! Direct AST equivalent of the existing pandoc-HTML pipeline in
//! [`crate::typesetter::structure`]. We walk the comrak document,
//! collect text + slug for each level-1 heading, then hand off to
//! the existing classifier so the chapter/interlude/roman/arabic
//! logic and front-matter-vs-back-matter positioning rules don't
//! need re-deriving here.

use comrak::nodes::{AstNode, NodeValue};

use crate::typesetter::structure::{
    BookStructure, SectionKind, classify_markdown_headings,
};

/// Walk a parsed comrak document and produce a `BookStructure`.
pub fn analyze_markdown_ast<'a>(root: &'a AstNode<'a>) -> BookStructure {
    let mut headings: Vec<(String, String)> = Vec::new();
    collect_h1s(root, &mut headings);
    let sections = classify_markdown_headings(&headings);
    let chapter_count =
        sections.iter().filter(|s| s.kind == SectionKind::Chapter).count() as u32;
    let interlude_count =
        sections.iter().filter(|s| s.kind == SectionKind::Interlude).count() as u32;
    let front_matter_count = sections
        .iter()
        .filter(|s| s.kind == SectionKind::FrontMatter)
        .count() as u32;
    let back_matter_count = sections
        .iter()
        .filter(|s| s.kind == SectionKind::BackMatter)
        .count() as u32;
    BookStructure {
        sections,
        chapter_count,
        interlude_count,
        front_matter_count,
        back_matter_count,
    }
}

/// Recursively visit the AST, pushing `(text, slug)` for every
/// level-1 heading.
fn collect_h1s<'a>(node: &'a AstNode<'a>, out: &mut Vec<(String, String)>) {
    let value = node.data.borrow().value.clone();
    if let NodeValue::Heading(h) = value {
        if h.level == 1 {
            let text = heading_text(node);
            let slug = format!("ch{}", slugify(&text));
            out.push((text, slug));
            // Don't recurse into the heading's own children — the
            // text was already collected. (We still need to recurse
            // into siblings via the caller.)
            return;
        }
    }
    for child in node.children() {
        collect_h1s(child, out);
    }
}

/// Concatenate text descendants of a node. Strips formatting
/// markup; matches what the user reads as the heading.
fn heading_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    text_into(node, &mut out);
    out.trim().to_string()
}

fn text_into<'a>(node: &'a AstNode<'a>, out: &mut String) {
    let value = node.data.borrow().value.clone();
    match value {
        NodeValue::Text(ref t) => out.push_str(t),
        NodeValue::Code(ref c) => out.push_str(&c.literal),
        NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
        _ => {
            for child in node.children() {
                text_into(child, out);
            }
        }
    }
}

/// Pandoc-equivalent slug: lowercase, ASCII letters + digits + `-`.
/// Matches `--id-prefix ch` plus `--section-divs` output so existing
/// `[CITE:]` → `chchapter-1-...` links continue to resolve.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_was_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_was_dash = false;
        } else if !prev_was_dash {
            out.push('-');
            prev_was_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use comrak::{Arena, Options, parse_document};

    fn structure_of(md: &str) -> BookStructure {
        let arena = Arena::new();
        let root = parse_document(&arena, md, &Options::default());
        analyze_markdown_ast(root)
    }

    #[test]
    fn classifies_a_simple_book() {
        let md = "\
# Author's Note

intro text

# Chapter 1: Beginnings

prose

# Chapter 2 — Middles

more prose

# Interlude I — A Pause

# Chapter 3

# Appendix
";
        let s = structure_of(md);
        assert_eq!(s.chapter_count, 3);
        assert_eq!(s.interlude_count, 1);
        assert_eq!(s.front_matter_count, 1, "Author's Note → front matter");
        assert_eq!(s.back_matter_count, 1, "Appendix after last ch → back matter");
        let ch1 = s
            .sections
            .iter()
            .find(|x| x.kind == SectionKind::Chapter && x.number == Some(1))
            .expect("chapter 1 present");
        assert_eq!(ch1.title, "Beginnings");
    }

    #[test]
    fn slug_matches_pandoc_id_prefix() {
        // Pandoc produces `chchapter-1-the-questions` when `--id-prefix=ch`.
        // Our slug should match the same shape.
        let md = "# Chapter 1: The Questions";
        let s = structure_of(md);
        assert_eq!(s.sections[0].html_id, "chchapter-1-the-questions");
    }
}
