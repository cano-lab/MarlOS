//! Walk a comrak AST and emit typst markup.
//!
//! Covers V1 markdown surface: paragraphs, headings (#/##/###),
//! emphasis (`*`, `_`), strong (`**`), inline code, code blocks,
//! blockquotes, ordered/unordered lists, horizontal rules, line +
//! soft breaks, links, images, inline + display math, GFM tables,
//! and footnote definitions (turned into the typst Notes section).

use comrak::nodes::{AstNode, ListType, NodeValue};
use comrak::{Arena, Options, parse_document};

use super::escape::escape_markup;
use super::math;

/// Emit the body of a typst document (no preamble).
pub fn emit_body(md: &str) -> String {
    let arena = Arena::new();
    let opts = make_options();
    let root = parse_document(&arena, md, &opts);
    let mut out = String::with_capacity(md.len() * 11 / 10);
    // Math helpers go at the very top so any `$...$` emitted later
    // can resolve their `mitexsqrt`/`textmath`/etc. references.
    out.push_str(math::MITEX_SHIM);
    out.push('\n');
    emit_node(root, &mut out);
    out
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
fn emit_node<'a>(node: &'a AstNode<'a>, out: &mut String) {
    let value = node.data.borrow().value.clone();
    match value {
        // ---- Block-level ----
        NodeValue::Document => emit_children(node, out),

        NodeValue::Paragraph => {
            emit_children(node, out);
            // Two newlines = blank line = paragraph break in typst.
            out.push_str("\n\n");
        }

        NodeValue::Heading(h) => {
            let level = h.level.min(6).max(1) as usize;
            out.push_str(&"=".repeat(level));
            out.push(' ');
            emit_children(node, out);
            out.push_str("\n\n");
        }

        NodeValue::BlockQuote => {
            // typst has no native blockquote; we wrap in a #quote
            // block. The math-anchor detector (M2.8) will replace
            // this for matching blockquotes.
            out.push_str("#quote(block: true)[\n");
            emit_children(node, out);
            out.push_str("]\n\n");
        }

        NodeValue::List(ref list) => {
            emit_children(node, out);
            // typst infers list grouping from consecutive `-` / `+ N.`
            // lines, so we just emit items and a trailing blank.
            out.push('\n');
            let _ = list; // list type comes from the items themselves
        }

        NodeValue::Item(ref list) => {
            let marker = match list.list_type {
                ListType::Bullet => "- ".to_string(),
                ListType::Ordered => format!("{}. ", list.start),
            };
            out.push_str(&marker);
            // Items may contain block-level children. We emit them
            // inline by stripping trailing blanks the children add.
            let scratch_start = out.len();
            emit_children(node, out);
            // Collapse runs of blank lines inside an item; typst will
            // treat them as ends-of-item which is what we want
            // between items but not inside a single one.
            let item_out = out.split_off(scratch_start);
            let normalized = item_out.trim_end_matches('\n');
            out.push_str(normalized);
            out.push('\n');
        }

        NodeValue::CodeBlock(ref cb) => {
            // typst raw block. Use the info string as the language hint.
            // Triple-tilde or triple-backtick handling: typst supports
            // `````{lang} ... ``````.
            let lang = cb.info.split_whitespace().next().unwrap_or("");
            if lang.is_empty() {
                out.push_str("```\n");
            } else {
                out.push_str(&format!("```{lang}\n"));
            }
            out.push_str(&cb.literal);
            if !cb.literal.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n\n");
        }

        NodeValue::ThematicBreak => {
            // Centered three-asterism, matching the convention in
            // the manuscript's `---` thematic breaks.
            out.push_str("#align(center)[\\* \\* \\*]\n\n");
        }

        NodeValue::HtmlBlock(ref hb) => {
            // We can't render raw HTML in typst output. Drop a typst
            // comment noting the block was skipped so the writer can
            // grep for it.
            let preview = hb.literal.lines().next().unwrap_or("").trim();
            let snippet = preview.chars().take(60).collect::<String>();
            out.push_str(&format!(
                "// [marlos:skipped-html-block] {snippet}\n\n"
            ));
        }

        NodeValue::HtmlInline(_) => {
            // Silently drop inline HTML for V1; the manuscript uses
            // it only for `<sup class="note-ref">` which the
            // citation transform will replace with typst macros
            // before this walker sees the markdown.
        }

        NodeValue::Table(_) => {
            // Comrak gives us TableRow children. We emit a typst
            // `#table` with default settings.
            out.push_str("#table(columns: auto)[\n");
            emit_children(node, out);
            out.push_str("]\n\n");
        }

        NodeValue::TableRow(_) => {
            emit_children(node, out);
        }

        NodeValue::TableCell => {
            out.push('[');
            emit_children(node, out);
            out.push_str("], ");
        }

        NodeValue::FootnoteDefinition(_) => {
            // M2.7 will lift these into the explicit Notes section.
            // For V1-skeleton we just drop them.
        }

        // ---- Inline-level ----
        NodeValue::Text(ref t) => {
            out.push_str(&escape_markup(t));
        }

        NodeValue::Strong => {
            out.push_str("#strong[");
            emit_children(node, out);
            out.push(']');
        }

        NodeValue::Emph => {
            out.push_str("#emph[");
            emit_children(node, out);
            out.push(']');
        }

        NodeValue::Strikethrough => {
            out.push_str("#strike[");
            emit_children(node, out);
            out.push(']');
        }

        NodeValue::Code(ref c) => {
            // typst raw inline: backtick-delimited.
            out.push_str("#raw(\"");
            for ch in c.literal.chars() {
                if ch == '"' || ch == '\\' {
                    out.push('\\');
                }
                out.push(ch);
            }
            out.push_str("\")");
        }

        NodeValue::Link(ref l) => {
            // #link("url")[label]
            out.push_str("#link(\"");
            for ch in l.url.chars() {
                if ch == '"' || ch == '\\' {
                    out.push('\\');
                }
                out.push(ch);
            }
            out.push_str("\")[");
            emit_children(node, out);
            out.push(']');
        }

        NodeValue::Image(ref l) => {
            // V1: emit as a centered figure. Real figure layout
            // (full bleed, crop, float) lands in M2.6.
            out.push_str("\n#figure(\n  image(\"");
            for ch in l.url.chars() {
                if ch == '"' || ch == '\\' {
                    out.push('\\');
                }
                out.push(ch);
            }
            out.push_str("\"),\n  caption: [");
            emit_children(node, out);
            out.push_str("]\n)\n\n");
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
                out.push_str("\n$ ");
                out.push_str(&converted);
                out.push_str(" $\n");
            } else {
                out.push('$');
                out.push_str(&converted);
                out.push('$');
            }
        }

        NodeValue::SoftBreak => out.push(' '),
        NodeValue::LineBreak => out.push_str(" \\\n"),

        NodeValue::FootnoteReference(ref fr) => {
            // M2.7 wires this to typst labels. Skeleton: superscript.
            out.push_str("#super[");
            out.push_str(&escape_markup(&fr.name));
            out.push(']');
        }

        // Fall-through: drop unhandled node types; M2.10 fixture run
        // will surface any we missed via visible gaps.
        _ => emit_children(node, out),
    }
}

fn emit_children<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for child in node.children() {
        emit_node(child, out);
    }
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
