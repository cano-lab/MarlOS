//! Build the typst preamble from [`BookConfig`].
//!
//! V1 scope: page size, margins, body font + size + leading, page
//! numbering, heading show-rules. The chapter-opener / running
//! header / TOC / LoF / footnote section logic lives in
//! [`super::markdown`]'s body emission (M3 work). Custom
//! user-supplied typst (`custom.typ`) is concatenated later in M4.

use crate::typesetter::book_config::BookConfig;

/// Emit the global preamble block. The body emitter appends content
/// after this, so don't terminate with a hard `#pagebreak` here.
pub fn build(config: &BookConfig) -> String {
    let trim = paper_size_for(&config.trim.size);
    let m = &config.trim.margins_in;
    let body_font = if config.typography.body_font.trim().is_empty() {
        "EB Garamond"
    } else {
        config.typography.body_font.as_str()
    };
    let body_size = config.typography.body_size_pt.max(6.0);
    // typst's `leading` is the inter-line spacing *added* between
    // baselines. Pandoc-CSS's "leading" is the full baseline-to-
    // baseline distance, so subtract the font size to map.
    let leading_pt = (config.typography.body_leading_pt - body_size).max(2.0);

    // Body-page number style. Front matter handling lands in M3
    // when chapter classification is wired through.
    let numbering = match config.typography.page_number_style.as_str() {
        "none" => "none",
        "roman" => "\"I\"",
        "lower-roman" => "\"i\"",
        _ => "\"1\"",
    };

    format!(
        r#"// === MarlOS book typesetter — generated preamble ===
#set page(
  width: {tw}in,
  height: {th}in,
  margin: (inside: {mi}in, outside: {mo}in, top: {mt}in, bottom: {mb}in),
  numbering: {numbering},
)
#set text(font: "{body_font}", size: {body_size}pt)
// Math font: fall back to the body face. typst's default is "New
// Computer Modern Math" which we don't bundle yet (M3 candidate);
// without this override, any `$...$` aborts the compile with
// "no font could be found". Math glyphs render in whatever text
// face is bound here — adequate for prose-heavy books, imperfect
// for heavy mathematical typesetting.
#show math.equation: set text(font: "{body_font}")
#set par(leading: {leading_pt}pt, justify: true, first-line-indent: 1em)
#show heading.where(level: 1): it => {{
  // Chapter title — own page, centered-ish, small-caps.
  pagebreak(weak: false, to: "odd")
  block(width: 100%, above: 4em, below: 2em)[
    #set text(size: 2em, weight: 600)
    #set par(first-line-indent: 0em)
    #align(center)[#smallcaps(it.body)]
  ]
}}
#show heading.where(level: 2): it => {{
  block(above: 1.5em, below: 0.6em)[
    #set text(size: 1.2em, weight: 600, style: "italic")
    #set par(first-line-indent: 0em)
    #it.body
  ]
}}
#show heading.where(level: 3): it => {{
  block(above: 1.2em, below: 0.4em)[
    #set text(size: 1em, weight: 600)
    #set par(first-line-indent: 0em)
    #it.body
  ]
}}

// User-content helpers (M2.8 wires the bodies):
#let word-anchor(body) = strong(body)
#let math-anchor(title: "", body) = block(
  fill: luma(245), inset: 10pt, radius: 4pt, breakable: false,
  width: 100%,
)[
  #set par(first-line-indent: 0em)
  #if title != "" [ *#title* \ ]
  #body
]
#let drop-cap(it) = text(weight: 700, size: 3em)[#it]
#let lead-in(it) = smallcaps(it)
"#,
        tw = trim.0,
        th = trim.1,
        mi = m.inside,
        mo = m.outside,
        mt = m.top,
        mb = m.bottom,
        numbering = numbering,
        body_font = body_font,
        body_size = body_size,
        leading_pt = leading_pt,
    )
}

/// `(width_in, height_in)` for a trim-size identifier. V1 supports
/// "6x9" plus a couple of common alternatives; anything else
/// silently falls back to 6×9 (matching the old chromium behavior).
fn paper_size_for(size: &str) -> (f32, f32) {
    let parsed = size
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)));
    match parsed {
        Some((w, h)) if w > 0.0 && h > 0.0 => (w, h),
        _ => (6.0, 9.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> BookConfig {
        BookConfig {
            book: Default::default(),
            trim: Default::default(),
            typography: Default::default(),
            export: Default::default(),
            files: vec!["x.md".into()],
            root_dir: std::path::PathBuf::from("."),
            config_path: std::path::PathBuf::from("./book.toml"),
        }
    }

    #[test]
    fn emits_page_and_text_blocks() {
        let out = build(&cfg());
        assert!(out.contains("#set page"));
        assert!(out.contains("6in"));
        assert!(out.contains("9in"));
        assert!(out.contains("EB Garamond"));
        assert!(out.contains("11pt"));
    }

    #[test]
    fn empty_body_font_falls_back() {
        let mut c = cfg();
        c.typography.body_font = "".into();
        let out = build(&c);
        assert!(out.contains("font: \"EB Garamond\""));
    }
}
