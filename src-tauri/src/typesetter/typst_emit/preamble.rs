//! Build the typst preamble from [`BookConfig`].
//!
//! V1 scope (M0..M3): page size, margins, body font + size + leading,
//! heading show-rules, page-numbering split (front matter vs body),
//! running headers, chapter opener pages. The TOC + LoF emission lives
//! in [`super::markdown`]'s body walker — they get inserted at the
//! first chapter heading along with the body-numbering switch.

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
    let leading_pt = (config.typography.body_leading_pt - body_size).max(2.0);

    // Front-matter numbering: defaults to lower-roman (i, ii, iii).
    // `arabic` means "one continuous arabic sequence through the whole
    // book"; in that case no counter reset at body start.
    let fm_style = config.typography.front_matter_page_number_style.as_str();
    let fm_numbering = match fm_style {
        "upper-roman" => r#""I""#,
        "arabic" => r#""1""#,
        _ => r#""i""#,
    };
    let restart_counter_at_body = fm_style != "arabic";

    // `restart_counter_at_body` informs the body-setup block in
    // [`body_setup`]; not consumed here.
    let _ = restart_counter_at_body;
    format!(
        r#"// === MarlOS book typesetter — generated preamble ===
#set page(
  width: {tw}in,
  height: {th}in,
  margin: (inside: {mi}in, outside: {mo}in, top: {mt}in, bottom: {mb}in),
  numbering: {fm_numbering},
  number-align: center + bottom,
)
#set text(font: "{body_font}", size: {body_size}pt)
// Math font: fall back to the body face. typst's default is "New
// Computer Modern Math" which we don't bundle yet; without this
// override, any `$...$` aborts the compile with "no font could be
// found". M-late: ship a real math face.
#show math.equation: set text(font: "{body_font}")
#set par(leading: {leading_pt}pt, justify: true, first-line-indent: 1em)

// --- Running header for body pages ---
// `body-header` is the function the body-setup block (emitted at the
// first chapter) installs as the page header. It queries the last
// level-1 heading that *precedes* the current page; if any level-1
// heading occurs on the current page itself, that page is a chapter
// opener and we render no header. Wrapping the query in `context`
// defers it to layout time when locations are known.
#let body-header = context {{
  let here-loc = here()
  let cur-page = here-loc.page()
  let all-h1s = query(heading.where(level: 1))
  let on-this-page = all-h1s.filter(h => h.location().page() == cur-page)
  if on-this-page.len() == 0 {{
    let preceding = all-h1s.filter(h => h.location().page() < cur-page)
    if preceding.len() > 0 {{
      align(center)[
        #set text(size: 9pt)
        #smallcaps[#preceding.last().body]
      ]
    }}
  }}
}}

// --- Chapter heading show-rule ---
// Each chapter heading triggers: pagebreak-to-odd → its own page with
// no running header and no folio (numbering: none) → vertically
// centered, small-caps title → pagebreak so the body starts fresh on
// the next page. The `set page` calls are scoped to the show-rule's
// content block, so they affect only the opener page; subsequent body
// pages inherit the outer header/footer settings.
#show heading.where(level: 1): it => [
  #pagebreak(weak: false, to: "odd")
  #set page(header: none, numbering: none)
  #v(2fr)
  #align(center)[
    #set text(size: 2em, weight: 600)
    #set par(first-line-indent: 0em)
    #smallcaps[#it.body]
  ]
  #v(3fr)
  #pagebreak()
]
#show heading.where(level: 2): it => [
  #block(above: 1.5em, below: 0.6em)[
    #set text(size: 1.2em, weight: 600, style: "italic")
    #set par(first-line-indent: 0em)
    #it.body
  ]
]
#show heading.where(level: 3): it => [
  #block(above: 1.2em, below: 0.4em)[
    #set text(size: 1em, weight: 600)
    #set par(first-line-indent: 0em)
    #it.body
  ]
]

// --- User-content helpers ---
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
        fm_numbering = fm_numbering,
        body_font = body_font,
        body_size = body_size,
        leading_pt = leading_pt,
    )
}

/// Body-setup block emitted by the walker at the first chapter.
/// Switches page numbering from front-matter style to body style,
/// optionally restarts the counter, and installs the running header.
pub fn body_setup(config: &BookConfig) -> String {
    let fm_style = config.typography.front_matter_page_number_style.as_str();
    let body_numbering = match config.typography.page_number_style.as_str() {
        "none" => "none".to_string(),
        "roman" => r#""I""#.to_string(),
        "lower-roman" => r#""i""#.to_string(),
        _ => r#""1""#.to_string(),
    };
    let mut out = String::new();
    if fm_style != "arabic" {
        // Restart the body counter at 1.
        out.push_str("#counter(page).update(1)\n");
    }
    out.push_str(&format!(
        "#set page(numbering: {body_numbering}, header: body-header)\n\n"
    ));
    out
}

/// TOC + LoF block emitted at the very top of the document, before
/// any markdown content. Both live in the front-matter numbering
/// region. LoF is gated on `config.export.include_list_of_figures`.
pub fn front_matter_outlines(config: &BookConfig) -> String {
    let mut out = String::new();
    out.push_str("#outline(title: [Contents], depth: 1, indent: auto)\n");
    out.push_str("#pagebreak(weak: false, to: \"odd\")\n");
    if config.export.include_list_of_figures {
        out.push_str(
            "#outline(title: [List of Figures], target: figure)\n\
             #pagebreak(weak: false, to: \"odd\")\n",
        );
    }
    out
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
