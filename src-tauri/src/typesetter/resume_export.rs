//! Resume / custom-document export — the flat single-flow lane.
//!
//! Deliberately kept separate from the book lane (`pdf_export.rs`): a
//! resume/custom document has **no** chapters, drop caps, running headers,
//! folios, TOC, List of Figures, covers, generated front/back matter, or
//! two-pass folio measurement. It is one continuous flow at the chosen page
//! size, styled by a minimal base stylesheet plus the user's `custom.css`
//! (which the Kimi-powered Style panel edits live).
//!
//! The dispatch in `typesetter_export_pdf` routes `DocType::Book` down the
//! book path and everything else here, so book-specific chrome and
//! resume/custom layout never entangle. Only genuinely shared, lane-neutral
//! helpers are reused: font embedding (`build_font_face_block`), the
//! Chromium render (`html_to_pdf`), and page-size resolution
//! (`TrimConfig::css_dimensions`).

use crate::typesetter::book_config::BookConfig;
use crate::typesetter::pdf_export::build_font_face_block;

/// Minimal escaping for the few characters that would break out of the
/// title/text context. Local to this lane so the resume path carries no
/// dependency on the book module's private helpers.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build the base stylesheet for a resume/custom document: the page box at
/// the configured trim + margins, the body typography from config, and
/// sensible element defaults (headings, lists, rules). No book chrome. The
/// user's `custom.css` is appended *after* this by `build_resume_html`, so
/// every rule here is overridable — the Style panel's generated CSS wins by
/// source order.
pub fn build_resume_css(config: &BookConfig) -> String {
    let (tw, th) = config.trim.css_dimensions();
    let m = &config.trim.margins_in;

    let body_font_name = if config.typography.body_font.trim().is_empty() {
        "EB Garamond".to_string()
    } else {
        config.typography.body_font.clone()
    };
    let body_font = format!("\"{}\", Georgia, \"Times New Roman\", serif", body_font_name);
    let body_size = config.typography.body_size_pt;
    let body_lead = config.typography.body_leading_pt;
    let font_faces = build_font_face_block();

    // Resume margins: a resume typically wants a symmetric, tighter frame
    // than a bound book. We honour the config margins but map the book's
    // inside/outside (spine-relative) onto simple left/right — there is no
    // spine here, so both page edges are equal.
    format!(
        r#"{font_faces}

@page {{
  size: {tw} {th};
  margin: {mt}in {mr}in {mb}in {ml}in;
}}

*, *::before, *::after {{ box-sizing: border-box; }}
html, body {{ margin: 0; padding: 0; }}

body {{
  font-family: {bf};
  font-size: {bs}pt;
  line-height: {bl}pt;
  color: #111111;
  -webkit-print-color-adjust: exact;
  print-color-adjust: exact;
}}

main.resume {{ display: block; }}

/* Flat heading hierarchy — no chapter openers, drop caps, or forced
   page breaks. Headings stay with the block that follows. */
h1 {{ font-size: 1.9em; line-height: 1.1; margin: 0 0 0.1em; }}
h2 {{
  font-size: 1.15em;
  margin: 1.1em 0 0.35em;
  padding-bottom: 0.1em;
  border-bottom: 0.75pt solid #999999;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}}
h3 {{ font-size: 1em; margin: 0.7em 0 0.15em; }}
h1, h2, h3, h4 {{ break-after: avoid; }}

p {{ margin: 0.35em 0; }}
ul, ol {{ margin: 0.3em 0 0.35em 1.2em; padding: 0; }}
li {{ margin: 0.15em 0; }}
a {{ color: inherit; text-decoration: none; }}
strong {{ font-weight: 700; }}
em {{ font-style: italic; }}
hr {{ border: none; border-top: 0.75pt solid #999999; margin: 0.7em 0; }}

/* Tables occasionally used for two-column contact/skill rows. */
table {{ width: 100%; border-collapse: collapse; }}
td, th {{ text-align: left; vertical-align: top; padding: 0.1em 0.4em 0.1em 0; }}
"#,
        font_faces = font_faces,
        tw = tw,
        th = th,
        mt = fmt_in(m.top),
        mr = fmt_in(m.outside),
        mb = fmt_in(m.bottom),
        ml = fmt_in(m.inside),
        bf = body_font,
        bs = body_size,
        bl = body_lead,
    )
}

/// Trim trailing zeros from a margin value so `1.0` prints as `1`, matching
/// the book path's `fmt_float` style.
fn fmt_in(v: f32) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

/// Wrap the pandoc body HTML in a full standalone document for Chromium:
/// base resume CSS, then the user's `custom.css` (next to book.toml) so its
/// rules override. No covers, no generated front/back matter, no markers.
pub fn build_resume_html(config: &BookConfig, body_html: &str) -> String {
    let css = build_resume_css(config);
    let custom_css =
        std::fs::read_to_string(config.root_dir.join("custom.css")).unwrap_or_default();
    let title = html_escape(&config.book.title);
    let lang = config.book.language.clone().unwrap_or_else(|| "en".to_string());

    format!(
        r#"<!DOCTYPE html>
<html lang="{lang}">
<head>
<meta charset="utf-8" />
<title>{title}</title>
<style>{css}</style>
<style id="marlos-custom-css">{custom_css}</style>
</head>
<body>
<main class="resume">
{body}
</main>
</body>
</html>
"#,
        lang = lang,
        title = title,
        css = css,
        custom_css = custom_css,
        body = body_html,
    )
}
