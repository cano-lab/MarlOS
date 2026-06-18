//! Walk a comrak AST and emit typst markup.
//!
//! Covers V1 markdown surface: paragraphs, headings (#/##/###),
//! emphasis (`*`, `_`), strong (`**`), inline code, code blocks,
//! blockquotes, ordered/unordered lists, horizontal rules, line +
//! soft breaks, links, images, inline + display math, GFM tables,
//! and footnote definitions (turned into the typst Notes section).

use comrak::nodes::{AstNode, ListType, NodeValue};
use comrak::{Arena, Options, parse_document};

use crate::typesetter::book_config::BookConfig;
use crate::typesetter::structure::{BookStructure, SectionKind};

use super::escape::escape_markup;
use super::math;
use super::preamble;
use super::structure::analyze_markdown_ast;

/// Walker state. `skip_depth > 0` means we're inside a context where
/// word-anchor wrapping would be inappropriate (heading, code, math,
/// math-anchor callout, figure caption).
struct Ctx<'a> {
    out: String,
    skip_depth: u32,
    word_anchors: bool,
    /// Book configuration. Used to emit per-config preamble blocks
    /// (TOC, LoF, page-numbering split) at the right moments.
    config: Option<&'a BookConfig>,
    /// Pre-computed structure (chapter/interlude/front/back-matter
    /// classification of each level-1 heading).
    structure: Option<BookStructure>,
    /// Number of level-1 headings we've started emitting so far.
    /// Used to look up the current heading's classification in
    /// `structure.sections` (which is in document order).
    h1_seen: usize,
    /// Set to true the first time we've emitted the body-setup block
    /// (page-counter restart + body-numbering switch + running header).
    body_started: bool,
}

impl<'a> Ctx<'a> {
    fn new(word_anchors: bool, capacity: usize) -> Self {
        Self {
            out: String::with_capacity(capacity),
            skip_depth: 0,
            word_anchors,
            config: None,
            structure: None,
            h1_seen: 0,
            body_started: false,
        }
    }
}

/// Emit the body of a typst document (no preamble). Test-only entry
/// point; production callers should use [`emit_body_with_config`].
#[cfg(test)]
pub fn emit_body(md: &str) -> String {
    emit_body_with(md, false)
}

/// Backwards-compatible alias used by the spike binary's --chapter
/// timing run; turns on word anchors but doesn't take a config.
#[cfg(test)]
pub fn emit_body_with(md: &str, word_anchors: bool) -> String {
    let arena = Arena::new();
    let opts = make_options();
    let root = parse_document(&arena, md, &opts);
    let mut ctx = Ctx::new(word_anchors, md.len() * 11 / 10);
    ctx.out.push_str(math::MITEX_SHIM);
    ctx.out.push('\n');
    emit_node(root, &mut ctx);
    ctx.out
}

/// Production entry: emits the typst body for `md` parameterized by
/// `config`. Includes the M3 page chrome:
///   - TOC + LoF inserted at the top (front-matter pages).
///   - Body-setup block (page-counter restart, body numbering,
///     running header) inserted before the first chapter heading.
pub fn emit_body_with_config(md: &str, config: &BookConfig) -> String {
    let arena = Arena::new();
    let opts = make_options();
    let root = parse_document(&arena, md, &opts);
    let structure = analyze_markdown_ast(root);
    let mut ctx = Ctx::new(config.export.word_anchors, md.len() * 11 / 10);
    ctx.config = Some(config);
    ctx.structure = Some(structure);

    // Math helper shim at the very top so any `$...$` can resolve
    // their mitexsqrt/textmath/etc. references.
    ctx.out.push_str(math::MITEX_SHIM);
    ctx.out.push('\n');

    // TOC + (optional) LoF in front-matter numbering region. The
    // outlines populate from the headings + figures the walker will
    // emit below; typst resolves their page numbers in a single
    // layout pass.
    ctx.out.push_str(&preamble::front_matter_outlines(config));

    emit_node(root, &mut ctx);
    ctx.out
}

/// Comrak parser configuration matching the manuscript's flavor.
fn make_options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.footnotes = true;
    opts.extension.table = true;
    opts.extension.math_dollars = true;
    opts.extension.strikethrough = true;
    opts.extension.autolink = true;
    opts.extension.tasklist = true;
    // Smart punctuation matches pandoc's `+smart`: ASCII -> curly
    // quotes, --- -> em dash, etc.
    opts.parse.smart = true;
    opts
}

/// Recursively emit a node and its descendants.
fn emit_node<'a>(node: &'a AstNode<'a>, ctx: &mut Ctx<'_>) {
    let value = node.data.borrow().value.clone();
    match value {
        // ---- Block-level ----
        NodeValue::Document => emit_children(node, ctx),

        NodeValue::Paragraph => {
            emit_children(node, ctx);
            // Two newlines = blank line = paragraph break in typst.
            ctx.out.push_str("\n\n");
        }

        NodeValue::Heading(h) => {
            let level = h.level.min(6).max(1) as usize;
            // M3: at level 1, look up the heading's classification
            // and emit body-setup once we hit the first chapter.
            if level == 1 {
                let idx = ctx.h1_seen;
                ctx.h1_seen += 1;
                if !ctx.body_started {
                    let is_first_chapter = ctx
                        .structure
                        .as_ref()
                        .and_then(|s| s.sections.get(idx))
                        .map(|sec| {
                            matches!(
                                sec.kind,
                                SectionKind::Chapter | SectionKind::Interlude
                            )
                        })
                        .unwrap_or(false);
                    if is_first_chapter {
                        if let Some(cfg) = ctx.config {
                            ctx.out.push_str(&preamble::body_setup(cfg));
                            ctx.body_started = true;
                        }
                    }
                }
            }
            ctx.out.push_str(&"=".repeat(level));
            ctx.out.push(' ');
            ctx.skip_depth += 1; // headings never get word anchors
            emit_children(node, ctx);
            ctx.skip_depth -= 1;
            ctx.out.push_str("\n\n");
        }

        NodeValue::BlockQuote => {
            // Math-anchor detection (M2.8): a blockquote whose first
            // paragraph starts with `**Math Anchor — Title**:` (with
            // optional colon) is rendered as the boxed-callout
            // `#math-anchor(...)` helper from the preamble. Ordinary
            // blockquotes wrap in `#quote(block: true)`.
            if let Some(title) = detect_math_anchor_title(node) {
                ctx.skip_depth += 1;
                emit_math_anchor(node, &title, ctx);
                ctx.skip_depth -= 1;
            } else {
                ctx.out.push_str("#quote(block: true)[\n");
                emit_children(node, ctx);
                ctx.out.push_str("]\n\n");
            }
        }

        NodeValue::List(ref list) => {
            emit_children(node, ctx);
            // typst infers list grouping from consecutive `-` / `+ N.`
            // lines, so we just emit items and a trailing blank.
            ctx.out.push('\n');
            let _ = list; // list type comes from the items themselves
        }

        NodeValue::Item(ref list) => {
            let marker = match list.list_type {
                ListType::Bullet => "- ".to_string(),
                ListType::Ordered => format!("{}. ", list.start),
            };
            ctx.out.push_str(&marker);
            // Items may contain block-level children. We emit them
            // inline by stripping trailing blanks the children add.
            let scratch_start = ctx.out.len();
            emit_children(node, ctx);
            // Collapse runs of blank lines inside an item; typst will
            // treat them as ends-of-item which is what we want
            // between items but not inside a single one.
            let item_out = ctx.out.split_off(scratch_start);
            let normalized = item_out.trim_end_matches('\n');
            ctx.out.push_str(normalized);
            ctx.out.push('\n');
        }

        NodeValue::CodeBlock(ref cb) => {
            // typst raw block. Use the info string as the language hint.
            // Triple-tilde or triple-backtick handling: typst supports
            // `````{lang} ... ``````.
            let lang = cb.info.split_whitespace().next().unwrap_or("");
            if lang.is_empty() {
                ctx.out.push_str("```\n");
            } else {
                ctx.out.push_str(&format!("```{lang}\n"));
            }
            ctx.out.push_str(&cb.literal);
            if !cb.literal.ends_with('\n') {
                ctx.out.push('\n');
            }
            ctx.out.push_str("```\n\n");
        }

        NodeValue::ThematicBreak => {
            // Centered three-asterism, matching the convention in
            // the manuscript's `---` thematic breaks.
            ctx.out.push_str("#align(center)[\\* \\* \\*]\n\n");
        }

        NodeValue::HtmlBlock(ref hb) => {
            // Citation pass-through: `<typst-block>...</typst-block>`
            // is the Notes section + any future block-level injection.
            // Emit the inner content verbatim as typst markup.
            if let Some(inner) = strip_typst_block(&hb.literal) {
                ctx.out.push_str(inner);
                ctx.out.push_str("\n\n");
                return;
            }
            // Otherwise drop with a typst-comment breadcrumb.
            let preview = hb.literal.lines().next().unwrap_or("").trim();
            let snippet = preview.chars().take(60).collect::<String>();
            ctx.out.push_str(&format!(
                "// [marlos:skipped-html-block] {snippet}\n\n"
            ));
        }

        NodeValue::HtmlInline(ref h) => {
            // Citation pass-through: `<typst>...</typst>` carries
            // typst markup that should land verbatim. Anything else
            // gets dropped silently — the only inline HTML the
            // manuscript should contain is the citation marker.
            if let Some(inner) = strip_typst_inline(h) {
                ctx.out.push_str(inner);
            }
        }

        NodeValue::Table(_) => {
            // Comrak gives us TableRow children. We emit a typst
            // `#table` with default settings.
            ctx.out.push_str("#table(columns: auto)[\n");
            emit_children(node, ctx);
            ctx.out.push_str("]\n\n");
        }

        NodeValue::TableRow(_) => {
            emit_children(node, ctx);
        }

        NodeValue::TableCell => {
            ctx.out.push('[');
            emit_children(node, ctx);
            ctx.out.push_str("], ");
        }

        NodeValue::FootnoteDefinition(_) => {
            // M2.7 will lift these into the explicit Notes section.
            // For V1-skeleton we just drop them.
        }

        // ---- Inline-level ----
        NodeValue::Text(ref t) => {
            if ctx.word_anchors && ctx.skip_depth == 0 {
                emit_text_with_word_anchors(t, &mut ctx.out);
            } else {
                ctx.out.push_str(&escape_markup(t));
            }
        }

        NodeValue::Strong => {
            ctx.out.push_str("#strong[");
            emit_children(node, ctx);
            ctx.out.push(']');
        }

        NodeValue::Emph => {
            ctx.out.push_str("#emph[");
            emit_children(node, ctx);
            ctx.out.push(']');
        }

        NodeValue::Strikethrough => {
            ctx.out.push_str("#strike[");
            emit_children(node, ctx);
            ctx.out.push(']');
        }

        NodeValue::Code(ref c) => {
            // typst raw inline. Code spans never get word anchors.
            ctx.out.push_str("#raw(\"");
            for ch in c.literal.chars() {
                if ch == '"' || ch == '\\' {
                    ctx.out.push('\\');
                }
                ctx.out.push(ch);
            }
            ctx.out.push_str("\")");
        }

        NodeValue::Link(ref l) => {
            // #link("url")[label]
            ctx.out.push_str("#link(\"");
            for ch in l.url.chars() {
                if ch == '"' || ch == '\\' {
                    ctx.out.push('\\');
                }
                ctx.out.push(ch);
            }
            ctx.out.push_str("\")[");
            emit_children(node, ctx);
            ctx.out.push(']');
        }

        NodeValue::Image(ref l) => {
            // V1: emit as a centered figure. Real figure layout
            // (full bleed, crop, float) lands in M2.6.
            ctx.out.push_str("\n#figure(\n  image(\"");
            for ch in l.url.chars() {
                if ch == '"' || ch == '\\' {
                    ctx.out.push('\\');
                }
                ctx.out.push(ch);
            }
            ctx.out.push_str("\"),\n  caption: [");
            emit_children(node, ctx);
            ctx.out.push_str("]\n)\n\n");
        }

        NodeValue::Math(ref m) => {
            // m.dollar_math = true means originally `$...$`. We
            // convert via mitex then emit between `$…$`.
            let converted = match math::convert_inline(&m.literal) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("math conversion failed ({e}): {}", m.literal);
                    // Fall back to a placeholder so the rest of the
                    // doc still compiles.
                    "[math]".to_string()
                }
            };
            if m.display_math {
                // Center on its own line.
                ctx.out.push_str("\n$ ");
                ctx.out.push_str(&converted);
                ctx.out.push_str(" $\n");
            } else {
                ctx.out.push('$');
                ctx.out.push_str(&converted);
                ctx.out.push('$');
            }
        }

        NodeValue::SoftBreak => ctx.out.push(' '),
        NodeValue::LineBreak => ctx.out.push_str(" \\\n"),

        NodeValue::FootnoteReference(ref fr) => {
            // M2.7 wires this to typst labels. Skeleton: superscript.
            ctx.out.push_str("#super[");
            ctx.out.push_str(&escape_markup(&fr.name));
            ctx.out.push(']');
        }

        // Fall-through: drop unhandled node types; M2.10 fixture run
        // will surface any we missed via visible gaps.
        _ => emit_children(node, ctx),
    }
}

fn emit_children<'a>(node: &'a AstNode<'a>, ctx: &mut Ctx<'_>) {
    for child in node.children() {
        emit_node(child, ctx);
    }
}

/// Emit a text run, wrapping the leading half of each word in
/// `#word-anchor[...]`. Skips numbers, punctuation, and short words
/// (≤1 letter). The preamble defines `#word-anchor` as a strong
/// wrapper; M2.9 of the typst port. Behavior parity with the
/// pre-existing HTML word-anchor transform.
fn emit_text_with_word_anchors(text: &str, out: &mut String) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (byte_idx, ch) = chars[i];
        if is_word_char(ch) {
            let start = byte_idx;
            let mut j = i;
            let mut letter_count = 0usize;
            while j < chars.len() && is_word_char(chars[j].1) {
                if chars[j].1.is_alphabetic() {
                    letter_count += 1;
                }
                j += 1;
            }
            let end = if j < chars.len() {
                chars[j].0
            } else {
                text.len()
            };
            let word = &text[start..end];
            if letter_count >= 2 {
                emit_anchored_word(word, out);
            } else {
                out.push_str(&super::escape::escape_markup(word));
            }
            i = j;
        } else {
            // Non-word char: escape singly and advance.
            let one: String = ch.to_string();
            out.push_str(&super::escape::escape_markup(&one));
            i += 1;
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || c == '\'' || c == '\u{2019}'
}

/// Emit one word as `#word-anchor[head]tail`, splitting at the
/// `ceil(letters / 2)` boundary. The head + tail are both escaped
/// for typst markup mode.
fn emit_anchored_word(word: &str, out: &mut String) {
    let total: usize = word.chars().filter(|c| c.is_alphabetic()).count();
    let cut = (total + 1) / 2;
    let mut seen = 0usize;
    let mut split_at = word.len();
    for (idx, ch) in word.char_indices() {
        if ch.is_alphabetic() {
            seen += 1;
            if seen == cut {
                split_at = idx + ch.len_utf8();
                break;
            }
        }
    }
    out.push_str("#word-anchor[");
    out.push_str(&super::escape::escape_markup(&word[..split_at]));
    out.push(']');
    out.push_str(&super::escape::escape_markup(&word[split_at..]));
}

/// Recognize `<typst>...</typst>` and return the inner string.
/// Used by the citation transform's body markers — the comrak parser
/// preserves the wrapping HTML tag as `HtmlInline` text and we
/// forward the contents to the typst output verbatim.
fn strip_typst_inline(s: &str) -> Option<&str> {
    let s = s.trim();
    let inner = s.strip_prefix("<typst>")?.strip_suffix("</typst>")?;
    Some(inner)
}

/// Block-level sibling: `<typst-block>...</typst-block>`. Used by
/// the citation Notes section + future block-level injections.
fn strip_typst_block(s: &str) -> Option<&str> {
    let s = s.trim();
    // The block content may span multiple lines and contain blank
    // lines, so we match start/end tags ignoring inner content.
    let inner = s.strip_prefix("<typst-block>")?.strip_suffix("</typst-block>")?;
    Some(inner.trim())
}

/// If `bq` is a blockquote whose first paragraph opens with
/// `**Math Anchor — Title**` (with optional `:` after Title), return
/// the Title. Otherwise None. Matches the manuscript's convention.
fn detect_math_anchor_title<'a>(bq: &'a AstNode<'a>) -> Option<String> {
    let first_para = bq.children().find(|n| {
        matches!(n.data.borrow().value, NodeValue::Paragraph)
    })?;
    let first_strong = first_para.children().find(|n| {
        matches!(n.data.borrow().value, NodeValue::Strong)
    })?;
    // Concatenate the strong run's text.
    let mut strong_text = String::new();
    for c in first_strong.children() {
        if let NodeValue::Text(ref t) = c.data.borrow().value {
            strong_text.push_str(t);
        }
    }
    let trimmed = strong_text.trim();
    let prefix = "Math Anchor";
    if !trimmed.starts_with(prefix) {
        return None;
    }
    // After "Math Anchor", expect " — Title" or " - Title" or just ":".
    let rest = &trimmed[prefix.len()..];
    let rest = rest
        .trim_start_matches(|c: char| matches!(c, ' ' | '\t' | '—' | '-' | '–'))
        .trim_end_matches(':')
        .trim();
    if rest.is_empty() {
        // Plain "Math Anchor" with no title.
        Some(String::new())
    } else {
        Some(rest.to_string())
    }
}

/// Render the blockquote as a `#math-anchor(title: "...")[ body ]`.
/// Strips the recognized `**Math Anchor — Title**: ` lead-in so it
/// doesn't appear twice (the title is shown by the helper instead).
fn emit_math_anchor<'a>(bq: &'a AstNode<'a>, title: &str, ctx: &mut Ctx<'_>) {
    ctx.out.push_str("#math-anchor(title: \"");
    for ch in title.chars() {
        if ch == '"' || ch == '\\' {
            ctx.out.push('\\');
        }
        ctx.out.push(ch);
    }
    ctx.out.push_str("\")[\n");
    // Emit children but skip the leading `**Math Anchor ...**` strong
    // run on the first paragraph + any immediately-following `:` text.
    let mut consumed_strong = false;
    for child in bq.children() {
        if !consumed_strong {
            if let NodeValue::Paragraph = child.data.borrow().value {
                let inner_out_start = ctx.out.len();
                let mut skip_first_strong = true;
                let mut skip_following_colon = true;
                for inline in child.children() {
                    let v = inline.data.borrow().value.clone();
                    if skip_first_strong {
                        if matches!(v, NodeValue::Strong) {
                            skip_first_strong = false;
                            consumed_strong = true;
                            continue;
                        }
                        // Non-strong before strong: bail, render normally.
                        if !matches!(v, NodeValue::Text(ref t) if t.trim().is_empty()) {
                            skip_first_strong = false;
                            skip_following_colon = false;
                            emit_node(inline, ctx);
                            continue;
                        }
                        emit_node(inline, ctx);
                        continue;
                    }
                    if skip_following_colon {
                        skip_following_colon = false;
                        if let NodeValue::Text(ref t) = v {
                            // Trim a leading ": " or whitespace.
                            let stripped = t.trim_start();
                            let stripped = stripped.strip_prefix(':').unwrap_or(stripped);
                            let stripped = stripped.trim_start();
                            ctx.out.push_str(&super::escape::escape_markup(stripped));
                            continue;
                        }
                    }
                    emit_node(inline, ctx);
                }
                // Close out the paragraph.
                if ctx.out.len() > inner_out_start {
                    ctx.out.push_str("\n\n");
                }
                continue;
            }
        }
        emit_node(child, ctx);
    }
    ctx.out.push_str("]\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_and_heading() {
        let out = emit_body("# Hi\n\nHello world.");
        assert!(out.contains("= Hi"));
        assert!(out.contains("Hello world"));
    }

    #[test]
    fn bold_italic_inline_code() {
        let out = emit_body("This is **bold** and *italic* and `code`.");
        assert!(out.contains("#strong[bold]"));
        assert!(out.contains("#emph[italic]"));
        assert!(out.contains("#raw(\"code\")"));
    }

    #[test]
    fn escapes_typst_specials() {
        let out = emit_body("Price is $5 and #5.");
        // Both `$` and `#` get backslash-escaped in prose runs.
        assert!(out.contains("\\$5"));
        assert!(out.contains("\\#5"));
    }

    #[test]
    fn inline_math_runs_through_mitex() {
        let out = emit_body("Hawking: $T = \\frac{\\hbar c^3}{8\\pi G M k_B}$.");
        assert!(out.contains("frac"));
        assert!(out.contains("planck.reduce"));
    }

    #[test]
    fn lists_emit_dash_or_number() {
        let out = emit_body("- a\n- b\n\n1. one\n2. two\n");
        assert!(out.contains("- "));
        assert!(out.contains("1. "));
    }

    #[test]
    fn link_inline_emits_typst_link() {
        let out = emit_body("[Anthropic](https://anthropic.com).");
        assert!(out.contains("#link(\"https://anthropic.com\")[Anthropic]"));
    }
}
