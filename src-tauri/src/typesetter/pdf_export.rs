//! Headless Chromium PDF export.
//!
//! For production-quality PDF output we bypass the OS print dialog and
//! drive a headless Chromium instance via the DevTools Protocol. Chrome's
//! native print engine has full CSS Paged Media support — `@page :left`,
//! `@page :right`, `@page named`, margin boxes, `string-set` / `string()`,
//! `counter(page, lower-roman)`, etc. — so the export path uses richer
//! typesetter CSS than the on-screen Paged.js preview can handle.
//!
//! Requires Google Chrome, Chromium, or MS Edge installed and discoverable.

use std::path::{Path, PathBuf};

use headless_chrome::{
    types::PrintToPdfOptions,
    Browser, LaunchOptionsBuilder,
};

use super::book_config::BookConfig;
use super::structure::BookStructure;

#[derive(Debug, thiserror::Error)]
pub enum PdfExportError {
    #[error("chromium error: {0}")]
    Chromium(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid output path: {0}")]
    InvalidPath(PathBuf),
}

impl From<anyhow::Error> for PdfExportError {
    fn from(e: anyhow::Error) -> Self {
        PdfExportError::Chromium(e.to_string())
    }
}

/// Build the full-fidelity CSS for export. Chromium's native print
/// engine has had on-and-off bugs with `string-set` / `string()`; we
/// avoid them entirely by generating per-chapter `@page` rules with
/// literal content. The on-screen Paged.js preview uses a simpler CSS
/// (`buildBookCss` in TS).
pub fn build_export_css(config: &BookConfig, structure: &BookStructure) -> String {
    // Trim size accepts three named presets plus any free-form
    // "WxH" string parsed as floats in inches (e.g. "5.25x8.0" for a
    // custom trim). Falls back to 6x9 on malformed input.
    let trim: (String, String) = match config.trim.size.as_str() {
        "5x8" => ("5in".into(), "8in".into()),
        "5.5x8.5" => ("5.5in".into(), "8.5in".into()),
        "6x9" => ("6in".into(), "9in".into()),
        other => {
            let mut parsed: Option<(f64, f64)> = None;
            if let Some((w_str, h_str)) = other.split_once('x') {
                if let (Ok(w), Ok(h)) = (
                    w_str.trim().parse::<f64>(),
                    h_str.trim().parse::<f64>(),
                ) {
                    if w > 0.0 && h > 0.0 {
                        parsed = Some((w, h));
                    }
                }
            }
            match parsed {
                Some((w, h)) => (format!("{}in", w), format!("{}in", h)),
                None => ("6in".into(), "9in".into()),
            }
        }
    };
    let m = &config.trim.margins_in;
    // Treat empty / whitespace body_font as "EB Garamond" so the
    // CSS doesn't end up with `font-family: "", Georgia, ...` which
    // browsers ignore the empty quotes for and silently fall back —
    // and KDP rejects anything that isn't an embedded font.
    let body_font_name = if config.typography.body_font.trim().is_empty() {
        "EB Garamond".to_string()
    } else {
        config.typography.body_font.clone()
    };
    let body_font = format!(
        "\"{}\", Georgia, \"Times New Roman\", serif",
        body_font_name
    );
    let body_size = config.typography.body_size_pt;
    let body_lead = config.typography.body_leading_pt;
    let book_title_lit = css_string_literal(&config.book.title);

    // Embed EB Garamond directly into the export CSS so Chromium has
    // the actual font file and can subset it into the PDF. Without
    // this, KDP flags glyphs (especially superscripts and math
    // symbols) as not printable because the rendering fell back to
    // a system font that wasn't embedded.
    let font_face_block = build_font_face_block();

    let running_header_style = config.typography.running_header_style.as_str();

    // Body page-number content. Front matter stays lower-roman.
    let page_number_content: &str = match config.typography.page_number_style.as_str() {
        "none" => "none",
        "roman" => "counter(page, upper-roman)",
        "lower-roman" => "counter(page, lower-roman)",
        // "arabic" or anything unrecognized
        _ => "counter(page)",
    };

    // Front-matter page-number style. "arabic" makes the whole book one
    // continuous arabic sequence (no restart at chapter 1); roman keeps
    // the traditional split (front matter roman, body restarts at 1).
    let fm_style = config.typography.front_matter_page_number_style.as_str();
    let front_matter_folio: &str = match fm_style {
        "upper-roman" => "counter(page, upper-roman)",
        "arabic" => "counter(page)",
        _ => "counter(page, lower-roman)",
    };
    let body_reset_css: &str = if fm_style == "arabic" {
        "" // continuous numbering — don't restart at chapter 1
    } else {
        "counter-reset: page 1;"
    };

    // Chapter-opener preset overrides. "modern" is the baseline; the
    // earlier rules already implement it. "traditional" appends an
    // override block (larger drop cap, smaller centered title, more
    // top whitespace, small-caps lead-in) — order matters: these
    // rules sit AFTER the modern ones in the CSS source so equal
    // specificity means traditional wins.
    let chapter_opener_extra_css =
        if config.typography.chapter_opener_style == "traditional" {
            r#"
section[data-section-type="chapter"] > h1 {
  font-size: 1.6em;
  /* margin stays 0 — the title is centered on its own opener page (set in
     the base rule); the body's drop cap opens the next page. */
  margin: 0;
}
section[data-section-type="chapter"] > p:first-of-type {
  text-indent: 0;
}
.drop-cap {
  float: left;
  font-size: 4em;
  line-height: 0.85;
  padding: 0.06em 0.1em 0 0;
  font-weight: 600;
}
.lead-in {
  font-variant: small-caps;
  letter-spacing: 0.05em;
}
"#
            .to_string()
        } else {
            String::new()
        };

    // Per-chapter @page rules with literal headers. Chromium's native
    // string()/string-set has been unreliable across versions; literal
    // content in named pages always works.
    let mut per_chapter_css = String::new();
    use crate::typesetter::structure::SectionKind;
    for section in &structure.sections {
        if section.kind != SectionKind::Chapter {
            continue;
        }
        let n = match section.number {
            Some(n) => n,
            None => continue,
        };
        // Running header text — formatted per running_header_style.
        // "title" is the historic default (chapter title only); the
        // other styles add a chapter number prefix so the reader can
        // locate themselves without remembering the title.
        let title = if section.title.is_empty() {
            String::new()
        } else {
            section.title.clone()
        };
        let header_text = match running_header_style {
            "chapter-number" => format!("Chapter {}", n),
            "chapter-number-title" => {
                if title.is_empty() {
                    format!("Chapter {}", n)
                } else {
                    format!("Chapter {} \u{00B7} {}", n, title)
                }
            }
            "compact-arabic" => {
                if title.is_empty() {
                    n.to_string()
                } else {
                    format!("{} \u{00B7} {}", n, title)
                }
            }
            "compact-roman" => {
                let r = to_upper_roman(n);
                if title.is_empty() {
                    r
                } else {
                    format!("{} \u{00B7} {}", r, title)
                }
            }
            // "title" or anything unrecognized
            _ => {
                if title.is_empty() {
                    format!("Chapter {}", n)
                } else {
                    title
                }
            }
        };
        let header_lit = css_string_literal(&header_text);
        per_chapter_css.push_str(&format!(
            r#"
section[data-section-type="chapter"][data-section-number="{n}"] {{
  page: chap-{n};
}}
/* The opener (h1) gets its own named page with NO running header so the
   centered title stands alone. The h1 carries the break-before:right
   (the section does not), so this is the chapter's single page break —
   no blank page. CSS `:first` only matches the document's first page, so
   a dedicated named page is the only way to suppress the header on every
   chapter's opener, not just chapter one's. */
section[data-section-type="chapter"][data-section-number="{n}"] > h1 {{
  page: chap-{n}-open;
}}
@page chap-{n}-open {{
  @top-center {{ content: none; }}
}}
@page chap-{n} {{
  @top-center {{
    content: {header_lit};
    font-family: {bf};
    font-size: 9pt;
    font-variant: small-caps;
    letter-spacing: 0.08em;
    color: #444;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }}
}}
"#,
            n = n,
            header_lit = header_lit,
            bf = body_font,
        ));
    }

    // Per-section running headers for non-chapter sections (front matter,
    // interludes, back matter). Same literal-content-in-a-named-page trick
    // as chapters, keyed by data-section-order. Header text = the section's
    // {header="..."} override, else its title. Front matter keeps its
    // lower-roman folio; everything else uses the configured page-number
    // style. The section's first page suppresses the header so the opener
    // reads clean (matching chapters).
    let book_title_norm = config.book.title.trim().to_lowercase();
    for section in &structure.sections {
        if section.kind == SectionKind::Chapter {
            continue;
        }
        // Skip the book-title section — it's the title page, not a header.
        if !book_title_norm.is_empty()
            && section.title.trim().to_lowercase() == book_title_norm
        {
            continue;
        }
        let head = section
            .running_header
            .clone()
            .unwrap_or_else(|| section.title.clone());
        if head.trim().is_empty() {
            continue;
        }
        let head_lit = css_string_literal(&head);
        let order = section.order;
        let folio: &str = if section.kind == SectionKind::FrontMatter {
            front_matter_folio
        } else {
            page_number_content
        };
        per_chapter_css.push_str(&format!(
            r#"
section[data-section-order="{order}"] {{ page: sec-{order}; }}
@page sec-{order} {{
  @top-center {{
    content: {head_lit};
    font-family: {bf};
    font-size: 9pt;
    font-variant: small-caps;
    letter-spacing: 0.08em;
    color: #444;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }}
  @bottom-center {{
    content: {folio};
    font-family: {bf};
    font-size: 9pt;
    color: #444;
    padding-top: 6pt;
  }}
}}
@page sec-{order}:first {{
  @top-center {{ content: none; }}
}}
"#,
            order = order,
            head_lit = head_lit,
            bf = body_font,
            folio = folio,
        ));
    }

    format!(
        r#"
{font_faces}

@page {{
  size: {tw} {th};
  margin: {mt}in {mout}in {mb}in {mout}in;
  @top-center {{
    content: {btitle};
    font-family: {bf};
    font-size: 9pt;
    font-variant: small-caps;
    letter-spacing: 0.08em;
    color: #444;
    /* Single-line runner; truncate with ellipsis if the book title or
       a long chapter title overflows the body width. */
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }}
  @bottom-center {{
    content: {pgnum};
    font-family: {bf};
    font-size: 9pt;
    color: #444;
  }}
}}

@page :left {{
  margin: {mt}in {min}in {mb}in {mout}in;
}}

@page :right {{
  margin: {mt}in {mout}in {mb}in {min}in;
}}

@page front-matter {{
  @top-center {{ content: none; }}
  @bottom-center {{
    content: {fm_folio};
    font-size: 9pt;
    color: #444;
  }}
}}

@page chapter-opener {{
  @top-center {{ content: none; }}
}}

@page cover {{
  margin: 0;
  @top-center {{ content: none; }}
  @bottom-center {{ content: none; }}
}}
/* The general @page :left / :right rules set mirrored margins; without
   these named+pseudo overrides a full-bleed cover/figure page on a
   left- or right-hand sheet inherits those margins (a white strip along
   the top + binding edge). Pin them to zero for both sides. */
@page cover:left {{
  margin: 0;
  @top-center {{ content: none; }} @top-left {{ content: none; }} @top-right {{ content: none; }}
  @bottom-center {{ content: none; }} @bottom-left {{ content: none; }} @bottom-right {{ content: none; }}
}}
@page cover:right {{
  margin: 0;
  @top-center {{ content: none; }} @top-left {{ content: none; }} @top-right {{ content: none; }}
  @bottom-center {{ content: none; }} @bottom-left {{ content: none; }} @bottom-right {{ content: none; }}
}}

html, body {{
  margin: 0;
  /* Right-side buffer so glyph bearings (italic descender slopes, the
     right edge of "y"/"f"/"j", trailing kern on small-caps) stay
     visible inside the printable area. Without it Chromium clips
     them at the @page margin even though the advance-width-based
     line layout fit. 2pt is the minimal cushion for EB Garamond. */
  padding: 0 2pt 0 0;
  background: #fff;
  color: #000;
  font-family: {bf};
  font-size: {bs}pt;
  line-height: {bl}pt;
}}

p {{
  margin: 0;
  text-align: justify;
  /* Justify spaces only — never stretch letters apart. Without this,
     Chromium can spread inter-character spacing on tight lines and
     push the last character past the body width into the margin
     (visible as the right-edge "cutoff" some letters get). */
  text-justify: inter-word;
  hyphens: auto;
  -webkit-hyphens: auto;
  text-indent: 1em;
  widows: 2;
  orphans: 2;
  /* Long unbreakable tokens (URLs, hex strings, citations with no
     spaces) wrap mid-character instead of overflowing the body. */
  overflow-wrap: break-word;
  word-wrap: break-word;
}}

h1 + p, h2 + p, h3 + p,
section > p:first-of-type {{
  text-indent: 0;
}}

/* Legacy hidden marker — harmless if still present in injected content. */
.book-title-source {{
  display: none;
}}

/* Word-anchor fixation emphasis. The HTML transform wraps the
   leading ~half of each prose word in <b class="word-anchor"> —
   render as strong weight without changing color so the unstressed
   second half is visually anchored by the leading bold "stem". */
b.word-anchor {{
  font-weight: 700;
}}

/* Cover pages: full-page IMAGE that fits within the trim, no cropping.
   object-fit: contain preserves the cover art aspect ratio. */
.book-cover {{
  page: cover;
  break-before: page;
  break-after: page;
  margin: 0;
  padding: 0;
  width: {tw};
  height: {th};
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}}

.book-cover img {{
  display: block;
  max-width: 100%;
  max-height: 100%;
  width: auto;
  height: auto;
  object-fit: contain;
}}

section[data-section-type="front-matter"] {{
  page: front-matter;
}}

section[data-section-type="interlude"],
section[data-section-type="back-matter"] {{
  break-before: page;
}}

/* Chapters do NOT break at the section level — the opener <h1> carries
   the break-before:right (below). Putting the break on the section AND
   giving the h1 a different named page is what produced the old blank
   page before every chapter. */

section[data-section-type="chapter"] > h1 {{
  /* Chapter-opener page: the title gets its own page, vertically centered;
     the body starts on the NEXT page. We force the break with break-after
     (not a conflicting named page on the h1), which avoids the old
     blank-page-before-every-chapter bug. Same centering mechanism as the
     generated title page. The chapter still opens on a fresh right page
     (break-before: right on the section) with no running header
     (@page chap-N:first). */
  break-before: right;
  page-break-before: right;
  height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  margin: 0;
  break-after: page;
  page-break-after: always;
  font-size: 2em;
  text-align: center;
  font-variant: small-caps;
  letter-spacing: 0.03em;
  font-weight: 600; /* embedded face; 500 would be synthesized */
  /* Distribute words across wrapped lines instead of letting the
     second line dangle long. Prevents two-line chapter titles from
     overflowing the page or looking imbalanced. */
  text-wrap: balance;
  line-height: 1.2;
}}

section[data-section-type="interlude"] > h1 {{
  font-size: 1.4em;
  text-align: center;
  margin: 8% 0 1.5em;
  font-style: italic;
  font-weight: 400;
}}

section[data-section-type="chapter"][data-section-number="1"] {{
  {body_reset}
}}

/* Drop cap is now a structure-parser span (.drop-cap) instead of a
   pseudo-element. Skips leading punctuation so quoted openings ("He
   said...") cap the letter, not the quote mark. Lead-in span is
   defined but only styled by the `traditional` preset (V2 ships
   `modern` styling here = no special lead-in). */
.drop-cap {{
  float: left;
  font-size: 3.2em;
  line-height: 0.85;
  padding: 0.05em 0.08em 0 0;
  font-weight: 600;
}}

.lead-in {{
  /* `modern` preset: no special styling. The `traditional` preset
     overrides this with small-caps + letter-spacing in Phase I. */
}}

h1, h2, h3, h4 {{
  font-weight: 600;
  line-height: 1.2;
  break-after: avoid;
}}

h2 {{ font-size: 1.25em; margin: 1.4em 0 0.6em; }}
h3 {{ font-size: 1.05em; margin: 1.2em 0 0.4em; font-style: italic; font-weight: 500; }}

blockquote {{ margin: 1em 1.5em; font-style: italic; }}

/* Centered line/block, written as a pandoc fenced div (:::center …
   :::). Inserted by the "Center" toolbar button. */
.center, .center > p, .center > h1, .center > h2, .center > h3 {{
  text-align: center;
  text-indent: 0;
}}

/* "Math Anchor" callout boxes — the equations/derivations the writer
   flags inline. Tagged with class="math-anchor" by the structure
   pipeline. A bordered, lightly tinted box that stays on one page. */
blockquote.math-anchor {{
  margin: 1.2em 0;
  padding: 0.6em 0.9em;
  border: 0.75pt solid #aaaaaa;
  border-left: 3pt solid #555555;
  background: #f5f5f5;
  font-style: normal;
  break-inside: avoid;
  page-break-inside: avoid;
}}
blockquote.math-anchor > :first-child {{ margin-top: 0; }}
blockquote.math-anchor > :last-child {{ margin-bottom: 0; }}
blockquote.math-anchor p {{ text-indent: 0; }}
ul, ol {{ margin: 0.5em 0 0.5em 1.5em; padding: 0; }}
li {{ margin: 0.2em 0; }}

.footnote-ref a {{ text-decoration: none; font-size: 0.8em; vertical-align: super; line-height: 0; }}
.footnotes {{ font-size: 0.92em; }}

.math {{ white-space: nowrap; }}
.math.display {{ display: block; text-align: center; margin: 1em 0; white-space: normal; }}

/* The PDF pipeline renders math as native MathML (Chromium's MathML
   engine). MathML elements inherit `font-family` from the body, but
   EB Garamond is a text face with no math coverage — partial-
   differential ∂, reduced-Planck ℏ, stretchy delimiters, accents like
   \hat — so equations come out with missing/tofu glyphs. Reset the
   math subtree to the CSS `math` generic family so Chromium falls to
   its installed OpenType math font (Cambria Math on Windows), which
   has full coverage and embeds into the PDF. */
math, math * {{
  font-family: math, "Cambria Math", "STIX Two Math", "Latin Modern Math", serif;
}}
/* pandoc emits display equations as bare <math display="block">, which
   left-aligns by default. A block <math> fills the line, so text-align
   alone won't center the equation — shrink the box to its content and
   auto-center it. The wrapping <p> also gets centered as a fallback for
   engines that treat the math box as inline-level. */
math[display="block"] {{
  display: block;
  width: fit-content;
  max-width: 100%;
  margin: 1em auto;
  text-align: center;
}}
p:has(> math[display="block"]) {{ text-align: center; text-indent: 0; }}

em, i {{ font-style: italic; }}
strong, b {{ font-weight: 600; }}

/* Section dividers in source (`---` / `***`) become <hr> elements.
   We keep the vertical breathing room the writer expected (~1em) but
   suppress the visual marker — no star ornament, no rule. */
hr {{
  border: 0;
  height: 0;
  margin: 1em 0;
  visibility: hidden;
}}

a {{ color: inherit; text-decoration: none; }}

/* Manual paragraph-spacing utility classes. Use raw HTML in your
   markdown to insert vertical breathing room between paragraphs:
     <div class="space-small"></div>      ~half line
     <div class="space-medium"></div>     ~one line
     <div class="space-large"></div>      ~two lines
     <div class="space-section"></div>    ~four lines (mini scene break)
     <div class="blank-page"></div>       full blank page
     <div class="page-break"></div>       force the next paragraph to a new page
*/
.space-small {{ height: 0.6em; }}
.space-medium {{ height: 1.2em; }}
.space-large {{ height: 2.4em; }}
.space-section {{ height: 4em; }}
.space-small, .space-medium, .space-large, .space-section {{
  break-inside: avoid;
  page-break-inside: avoid;
}}
.blank-page {{
  break-before: page;
  break-after: page;
  page-break-before: always;
  page-break-after: always;
  height: 0;
  visibility: hidden;
}}
.page-break {{
  break-before: page;
  page-break-before: always;
  height: 0;
  visibility: hidden;
}}

/* All inline super/subscripts — math exponents (10<sup>-35</sup>),
   chemical/Planck subscripts (ℓ<sub>p</sub>), and manually-typed
   footnote markers — are pinned to the embedded body font with
   lining numerals, same as citation refs below. The structure
   pipeline rewrites every precomposed Unicode super/subscript
   (⁰¹²³⁴⁵⁶⁷⁸⁹⁻ ₀₁₂…) into <sup>/<sub> with ordinary ASCII glyphs so
   this rule can pin them. Without it, Chromium falls back to a system
   font for the Unicode super/subscripts EB Garamond doesn't subset
   (⁴ ⁵ ⁻ live outside Latin-1), which makes the two digits inside one
   exponent render in different typefaces and risks KDP flagging the
   glyph as not embedded. */
sup, sub {{
  font-family: {bf};
  font-size: 0.72em;
  line-height: 0;
  font-feature-settings: "lnum" 1;
  font-variant-numeric: lining-nums;
  font-weight: 400;
}}
sup {{ vertical-align: super; }}
sub {{ vertical-align: sub; }}

/* Citation superscript references in body.
   Explicitly pin font-family + lining numerals so the digit glyph is
   guaranteed to come from the embedded EB Garamond Latin subset
   (where U+0030–0039 always live). Without this, Chromium has been
   observed to fall back to a system font for the superscript size,
   which then ends up unembedded and KDP flags the digit as not
   printable. */
sup.note-ref {{
  font-family: {bf};
  font-size: 0.75em;
  vertical-align: super;
  line-height: 0;
  /* lining-nums forces the embedded digit subset (KDP rejects glyphs
     from unembedded fallbacks). NO tabular-nums — its fixed-width
     slot center-pads narrow digits like 1, which reads as an extra
     space before the numeral. */
  font-feature-settings: "lnum" 1;
  font-variant-numeric: lining-nums;
  font-variant-position: normal;
  font-weight: 400;
}}
sup.note-ref a {{
  font-family: inherit;
  text-decoration: none;
  color: inherit;
}}

/* Notes back-matter section. */
.notes-list {{
  list-style: none;
  padding: 0;
  margin: 1em 0;
}}
.notes-list li {{
  text-indent: -2em;
  padding-left: 2em;
  margin-bottom: 0.6em;
  /* No break-inside: avoid — some bibliography entries are 80+ words
     long and must be allowed to flow across page boundaries. Forcing
     them to stay together makes the bottom of the entry clip when it
     overruns the page. */
}}
.notes-list .note-num {{
  font-weight: 600; /* not 500 — 500 isn't a face we embed, Chromium synthesizes it */
  margin-right: 0.3em;
}}
/* Hide the back-arrow (↩, U+21A9) in the print PDF. Useful only for
   digital tap-back navigation; can't be in EB Garamond's subsets
   (it's a text serif, not a symbol font), so KDP flags the arrow
   glyph — and the digit immediately preceding it gets caught up in
   the same fallback font run, producing the "some endnote numbers
   not printable" complaint. The EPUB CSS keeps the arrow because
   ereaders can act on the link. */
.notes-list .note-back {{
  display: none;
}}

/* Chapter-opener preset override (empty when modern) */
{chapter_opener_extra}

/* Generated front-matter pages — title, copyright, dedication.
   Each is a flex-centered full body block so the content sits at
   the page's vertical center regardless of how much text it
   contains. Page numbers suppressed on title and dedication
   (they're typically unprinted); copyright keeps its lower-roman
   page number per print convention. */
@page no-page-number {{
  @top-center {{ content: none; }}
  @bottom-center {{ content: none; }}
  @bottom-left {{ content: none; }}
  @bottom-right {{ content: none; }}
}}

/* Table of Contents (generated). Chapters carry a "N." prefix; other
   sections are italic with no number. Page numbers + leaders are NOT
   emitted here — Chromium's print engine ignores target-counter /
   leader(), so the folios are injected by the two-pass PDF page-number
   step instead. */
.generated-toc-page {{
  break-before: right;
  page-break-before: right;
  /* Force the body that follows the TOC onto a fresh page. */
  break-after: page;
  page-break-after: always;
}}
.gen-toc-title {{
  text-align: center;
  font-variant: small-caps;
  letter-spacing: 0.08em;
  font-size: 1.4em;
  /* Tight top margin so the 20+ entries fit on one page. */
  margin: 0 0 1em;
}}
.toc-entry {{
  display: flex;
  align-items: baseline;
  text-decoration: none;
  color: inherit;
  text-indent: 0;
  margin: 0.45em 0;
  line-height: 1.3;
}}
.toc-num {{ flex: 0 0 1.9em; }}
.toc-other {{ font-style: italic; padding-left: 1.9em; }}
.toc-text {{ flex: 0 1 auto; }}
/* Dot leader filling the gap to the page number (folios injected by the
   two-pass step; a flexbox leader works in Chromium where leader() does
   not). */
.toc-leader {{ flex: 1 1 auto; border-bottom: 0.75pt dotted #999; margin: 0 0.4em 0.28em; }}
.toc-folio {{ flex: 0 0 auto; padding-left: 0.3em; }}

/* List of Figures (generated back matter). Reuses the .toc / .toc-entry
   machinery (folios injected by the two-pass step); starts on its own
   page. .lof-num is the "Fig. N." label inside each entry. */
.generated-lof-page {{
  break-before: page;
  page-break-before: always;
}}
.gen-lof-title {{
  text-align: center;
  font-variant: small-caps;
  letter-spacing: 0.08em;
  font-size: 1.4em;
  margin: 0 0 1em;
}}
.lof-num {{ font-variant: small-caps; padding-right: 0.3em; }}
/* "Fig. N." prefix on the in-body caption. */
.fig-num {{ font-style: normal; font-variant: small-caps; padding-right: 0.25em; }}

.generated-title-page,
.generated-copyright-page,
.generated-dedication-page {{
  break-before: right;
  page-break-before: right;
  break-after: page;
  page-break-after: always;
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  text-align: center;
}}

.generated-title-page {{ page: no-page-number; }}
.generated-dedication-page {{ page: no-page-number; }}

/* Stack the cover-art credit above the centred copyright block. */
.generated-copyright-page {{ flex-direction: column; }}

.generated-copyright-page .gen-cover-art {{
  width: 100%;
  max-width: 4in;
  margin: 0 0 1.2em;
  font-size: 0.8em;
  font-style: italic;
  line-height: 1.4;
  text-align: center;
  text-indent: 0;
  color: #333;
}}

.generated-title-page .title-page-inner,
.generated-copyright-page .copyright-page-inner,
.generated-dedication-page .dedication-inner {{
  width: 100%;
  max-width: 4in;
}}

/* Title-page elements override the generic `p` rules (justify +
   text-indent) so every line is center-aligned with no leading indent. */
.generated-title-page .gen-book-title,
.generated-title-page .gen-book-subtitle,
.generated-title-page .gen-book-author {{
  text-align: center;
  text-indent: 0;
}}

.generated-title-page .gen-book-title {{
  font-size: 2.4em;
  margin: 0 0 0.5em;
  font-variant: small-caps;
  letter-spacing: 0.04em;
  font-weight: 600;
  line-height: 1.15;
  text-wrap: balance;
}}

.generated-title-page .gen-book-subtitle {{
  font-size: 1.1em;
  font-style: italic;
  margin: 0 0 2.5em;
  color: #333;
  text-wrap: balance;
}}

.generated-title-page .gen-book-author {{
  font-size: 1.05em;
  font-variant: small-caps;
  letter-spacing: 0.08em;
  margin: 0;
}}

.generated-copyright-page .copyright-page-inner {{
  font-size: 0.85em;
  line-height: 1.5;
  color: #222;
}}

.generated-copyright-page p {{
  margin: 0 0 0.6em;
  text-indent: 0;
  text-align: center;
}}

.generated-copyright-page .gen-publisher {{
  font-style: italic;
  margin-top: 1.2em;
}}

.generated-copyright-page .gen-isbn {{
  font-family: var(--font-mono, monospace);
  letter-spacing: 0.05em;
}}

.generated-dedication-page .dedication-inner {{
  font-size: 1.05em;
  font-style: italic;
  line-height: 1.5;
  text-wrap: balance;
}}

.generated-dedication-page p {{
  margin: 0;
  text-indent: 0;
  text-align: center;
}}

/* Acknowledgements page — back matter. Centered italic block under
   a small-caps heading. Unlike the dedication, the body flows
   normally across pages if it runs long (no flex-center). */
.generated-acknowledgements-page {{
  break-before: page;
}}

.generated-acknowledgements-page .gen-ack-heading {{
  text-align: center;
  font-size: 1.4em;
  font-variant: small-caps;
  letter-spacing: 0.08em;
  font-weight: 600;
  margin: 4em 0 2em;
}}

.generated-acknowledgements-page .acknowledgements-inner {{
  max-width: 4in;
  margin: 0 auto;
  font-style: italic;
  text-align: center;
  line-height: 1.6;
}}

.generated-acknowledgements-page .acknowledgements-inner p {{
  margin: 0 0 1em;
  text-indent: 0;
}}

/* === Phase J: figures + captions (inline layout only) ===
   Pandoc's implicit_figures extension wraps any paragraph that's just
   an image-with-alt in `<figure><img/><figcaption>alt</figcaption></figure>`.
   These rules style that wrapper: centered, capped at the body width,
   never split across pages, caption in italic below.
   Layout-mode classes (.float-top, .full-page) come in Phase K. */
figure {{
  break-inside: avoid;
  page-break-inside: avoid;
  margin: 1em auto;
  display: block;
  text-align: center;
  max-width: 100%;
}}

figure img {{
  max-width: 100%;
  height: auto;
  display: block;
  margin: 0 auto;
}}

figcaption {{
  font-size: 0.9em;
  font-style: italic;
  margin-top: 0.4em;
  text-align: center;
  text-wrap: balance;
  text-indent: 0;
  color: #333;
}}

/* === Phase K: figure layout modes ===
   .fig-text  — fills the full text block (body width).
   .fig-bleed — escapes the text block toward the page edge via symmetric
                negative margins (the outside margin). --fig-inset, set per
                image, pulls it back from the edge for a gutter-safe
                near-bleed; 0 = full bleed to the outside edge. Symmetric so
                the wider inside (gutter) margin keeps extra room and the
                binding seam never distorts the image. */
figure.fig-text {{ max-width: 100%; }}
figure.fig-text img {{ width: 100%; }}
figure.fig-bleed {{
  margin-left: calc(-1 * ({mout}in - var(--fig-inset, 0in)));
  margin-right: calc(-1 * ({mout}in - var(--fig-inset, 0in)));
  max-width: none;
}}
figure.fig-bleed img {{ width: 100%; }}

/* Float modes — image sits in the text, paragraphs wrap around it. */
figure.fig-float-left {{ float: left; max-width: 48%; margin: 0.2em 1.2em 0.6em 0; }}
figure.fig-float-right {{ float: right; max-width: 48%; margin: 0.2em 0 0.6em 1.2em; }}
figure.fig-float-left img, figure.fig-float-right img {{ width: 100%; }}

/* .fig-fullpage — a full-bleed plate on its own page. Uses the zero-margin
   `cover` page (same proven mechanism as the book covers): the figure is
   exactly the trim size, so it fills one page edge-to-edge without
   overflowing. --fig-fit picks cover (fill + crop) vs contain (whole
   image, may letterbox). */
figure.fig-fullpage {{
  page: cover;
  margin: 0;
  padding: 0;
  width: {tw};
  height: {th};
  max-width: none;
  overflow: hidden;
  break-before: page;
  break-after: page;
  page-break-before: always;
  page-break-after: always;
}}
figure.fig-fullpage img {{
  display: block;
  width: 100%;
  height: 100%;
  object-fit: var(--fig-fit, cover);
  object-position: var(--fig-crop-pos, center);
}}
/* Caption would fall off the bleed; hide it on the page (the figure id +
   alt text still feed the List of Figures). */
figure.fig-fullpage figcaption {{ display: none; }}

/* .fig-crop — crop a figure to a fixed aspect ratio, filling the box and
   panning to the focal point. object-fit only bites once the img has a
   constrained box, which --fig-crop-ar supplies via aspect-ratio. */
figure.fig-crop img {{
  aspect-ratio: var(--fig-crop-ar, auto);
  width: 100%;
  height: auto;
  object-fit: cover;
  object-position: var(--fig-crop-pos, center);
}}

/* Per-chapter named pages with literal headers — generated below */
{per_chapter}
"#,
        tw = trim.0,
        th = trim.1,
        mt = m.top,
        mb = m.bottom,
        min = m.inside,
        mout = m.outside,
        bf = body_font,
        bs = body_size,
        bl = body_lead,
        btitle = book_title_lit,
        per_chapter = per_chapter_css,
        chapter_opener_extra = chapter_opener_extra_css,
        font_faces = font_face_block,
        pgnum = page_number_content,
        fm_folio = front_matter_folio,
        body_reset = body_reset_css,
    )
}

/// Render an integer as upper-roman ("I", "II", "IV", ...). Saturates
/// at 3999 (largest classical roman numeral); 0 / negatives return
/// the arabic form so the running header doesn't go blank.
fn to_upper_roman(n: u32) -> String {
    if n == 0 || n > 3999 {
        return n.to_string();
    }
    let pairs: [(u32, &str); 13] = [
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"),
        (100, "C"), (90, "XC"), (50, "L"), (40, "XL"),
        (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
    ];
    let mut remaining = n;
    let mut out = String::new();
    for (value, sym) in pairs {
        while remaining >= value {
            out.push_str(sym);
            remaining -= value;
        }
    }
    out
}

/// EB Garamond WOFF2 files embedded at compile time. We bundle three
/// subsets per weight/style (latin + latin-ext + greek) so author
/// names with accented characters (Schwarzschild, Möbius, Penrose…)
/// and Greek letters in prose render with embedded glyphs — KDP
/// rejects PDFs whose visible glyphs aren't in an embedded font.
macro_rules! ebg {
    ($subset:literal, $weight:literal, $style:literal) => {
        include_bytes!(concat!(
            "../../../node_modules/@fontsource/eb-garamond/files/eb-garamond-",
            $subset, "-", $weight, "-", $style, ".woff2"
        ))
    };
}
const EBG_LATIN_400_NORMAL: &[u8] = ebg!("latin", "400", "normal");
const EBG_LATIN_400_ITALIC: &[u8] = ebg!("latin", "400", "italic");
const EBG_LATIN_600_NORMAL: &[u8] = ebg!("latin", "600", "normal");
const EBG_LATIN_600_ITALIC: &[u8] = ebg!("latin", "600", "italic");
const EBG_LATEXT_400_NORMAL: &[u8] = ebg!("latin-ext", "400", "normal");
const EBG_LATEXT_400_ITALIC: &[u8] = ebg!("latin-ext", "400", "italic");
const EBG_LATEXT_600_NORMAL: &[u8] = ebg!("latin-ext", "600", "normal");
const EBG_LATEXT_600_ITALIC: &[u8] = ebg!("latin-ext", "600", "italic");
const EBG_GREEK_400_NORMAL: &[u8] = ebg!("greek", "400", "normal");
const EBG_GREEK_400_ITALIC: &[u8] = ebg!("greek", "400", "italic");
const EBG_GREEK_600_NORMAL: &[u8] = ebg!("greek", "600", "normal");
const EBG_GREEK_600_ITALIC: &[u8] = ebg!("greek", "600", "italic");

// Accessibility fonts: OpenDyslexic, Atkinson Hyperlegible, Lexend.
// All three are SIL OFL — we ship them in-binary so the picker works
// offline at export time. Latin subset only to keep the binary small;
// extend to latin-ext if a translation needs it.
const OPENDYS_400_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/opendyslexic/files/opendyslexic-latin-400-normal.woff2"
);
const OPENDYS_400_ITALIC: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/opendyslexic/files/opendyslexic-latin-400-italic.woff2"
);
const OPENDYS_700_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/opendyslexic/files/opendyslexic-latin-700-normal.woff2"
);
const OPENDYS_700_ITALIC: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/opendyslexic/files/opendyslexic-latin-700-italic.woff2"
);
const ATKINSON_400_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/atkinson-hyperlegible/files/atkinson-hyperlegible-latin-400-normal.woff2"
);
const ATKINSON_400_ITALIC: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/atkinson-hyperlegible/files/atkinson-hyperlegible-latin-400-italic.woff2"
);
const ATKINSON_700_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/atkinson-hyperlegible/files/atkinson-hyperlegible-latin-700-normal.woff2"
);
const ATKINSON_700_ITALIC: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/atkinson-hyperlegible/files/atkinson-hyperlegible-latin-700-italic.woff2"
);
const LEXEND_400_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/lexend/files/lexend-latin-400-normal.woff2"
);
const LEXEND_600_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/lexend/files/lexend-latin-600-normal.woff2"
);
const LEXEND_700_NORMAL: &[u8] = include_bytes!(
    "../../../node_modules/@fontsource/lexend/files/lexend-latin-700-normal.woff2"
);

/// `unicode-range` per @fontsource subset definitions. Telling
/// Chromium which characters belong to which file lets the engine
/// pick the right subset per glyph and embed only what's used —
/// keeping the PDF small but guaranteeing every visible glyph is
/// in some embedded face.
const UNICODE_LATIN: &str = "U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+2074, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD";
const UNICODE_LATIN_EXT: &str = "U+0100-02AF, U+0304, U+0308, U+0329, U+1E00-1E9F, U+1EF2-1EFF, U+2020, U+20A0-20AB, U+20AD-20C0, U+2113, U+2C60-2C7F, U+A720-A7FF";
const UNICODE_GREEK: &str = "U+0370-0377, U+037A-037F, U+0384-038A, U+038C, U+038E-03A1, U+03A3-03FF";

/// Stage the bundled EB Garamond WOFF2 files in the temp dir and
/// return @font-face CSS pointing at them via file:// URLs. Chromium
/// loads them into its font cache and embeds (subsets) them into the
/// PDF — preventing KDP's "not printable" rejection that happens
/// when text gets rendered in a system font that isn't bundled.
fn build_font_face_block() -> String {
    let dir = std::env::temp_dir().join("marlos-fonts");
    let _ = std::fs::create_dir_all(&dir);

    let write = |name: &str, bytes: &[u8]| -> Option<String> {
        let path = dir.join(name);
        std::fs::write(&path, bytes).ok()?;
        Some(format!(
            "file:///{}",
            path.display().to_string().replace('\\', "/")
        ))
    };

    let face = |family: &str,
                url: Option<String>,
                weight: u32,
                style: &str,
                range: Option<&str>|
     -> String {
        match url {
            Some(u) => {
                let range_clause = range
                    .map(|r| format!(" unicode-range: {};", r))
                    .unwrap_or_default();
                format!(
                    "@font-face {{ font-family: '{}'; src: url('{}') format('woff2'); font-weight: {}; font-style: {}; font-display: swap;{} }}\n",
                    family, u, weight, style, range_clause,
                )
            }
            None => String::new(),
        }
    };

    let mut out = String::new();
    // EB Garamond — latin
    out.push_str(&face("EB Garamond", write("ebg-latin-400.woff2", EBG_LATIN_400_NORMAL), 400, "normal", Some(UNICODE_LATIN)));
    out.push_str(&face("EB Garamond", write("ebg-latin-400i.woff2", EBG_LATIN_400_ITALIC), 400, "italic", Some(UNICODE_LATIN)));
    out.push_str(&face("EB Garamond", write("ebg-latin-600.woff2", EBG_LATIN_600_NORMAL), 600, "normal", Some(UNICODE_LATIN)));
    out.push_str(&face("EB Garamond", write("ebg-latin-600i.woff2", EBG_LATIN_600_ITALIC), 600, "italic", Some(UNICODE_LATIN)));
    // EB Garamond — latin-ext
    out.push_str(&face("EB Garamond", write("ebg-latext-400.woff2", EBG_LATEXT_400_NORMAL), 400, "normal", Some(UNICODE_LATIN_EXT)));
    out.push_str(&face("EB Garamond", write("ebg-latext-400i.woff2", EBG_LATEXT_400_ITALIC), 400, "italic", Some(UNICODE_LATIN_EXT)));
    out.push_str(&face("EB Garamond", write("ebg-latext-600.woff2", EBG_LATEXT_600_NORMAL), 600, "normal", Some(UNICODE_LATIN_EXT)));
    out.push_str(&face("EB Garamond", write("ebg-latext-600i.woff2", EBG_LATEXT_600_ITALIC), 600, "italic", Some(UNICODE_LATIN_EXT)));
    // EB Garamond — greek
    out.push_str(&face("EB Garamond", write("ebg-greek-400.woff2", EBG_GREEK_400_NORMAL), 400, "normal", Some(UNICODE_GREEK)));
    out.push_str(&face("EB Garamond", write("ebg-greek-400i.woff2", EBG_GREEK_400_ITALIC), 400, "italic", Some(UNICODE_GREEK)));
    out.push_str(&face("EB Garamond", write("ebg-greek-600.woff2", EBG_GREEK_600_NORMAL), 600, "normal", Some(UNICODE_GREEK)));
    out.push_str(&face("EB Garamond", write("ebg-greek-600i.woff2", EBG_GREEK_600_ITALIC), 600, "italic", Some(UNICODE_GREEK)));

    // Accessibility fonts. No unicode-range so the file covers whatever
    // the manuscript actually uses — latin only, OK for English prose.
    out.push_str(&face("OpenDyslexic", write("od-400.woff2", OPENDYS_400_NORMAL), 400, "normal", None));
    out.push_str(&face("OpenDyslexic", write("od-400i.woff2", OPENDYS_400_ITALIC), 400, "italic", None));
    out.push_str(&face("OpenDyslexic", write("od-700.woff2", OPENDYS_700_NORMAL), 700, "normal", None));
    out.push_str(&face("OpenDyslexic", write("od-700i.woff2", OPENDYS_700_ITALIC), 700, "italic", None));

    out.push_str(&face("Atkinson Hyperlegible", write("ah-400.woff2", ATKINSON_400_NORMAL), 400, "normal", None));
    out.push_str(&face("Atkinson Hyperlegible", write("ah-400i.woff2", ATKINSON_400_ITALIC), 400, "italic", None));
    out.push_str(&face("Atkinson Hyperlegible", write("ah-700.woff2", ATKINSON_700_NORMAL), 700, "normal", None));
    out.push_str(&face("Atkinson Hyperlegible", write("ah-700i.woff2", ATKINSON_700_ITALIC), 700, "italic", None));

    // Lexend ships normal-only weights (it's a sans variable-weight
    // family designed for reading). Italics fall back via synthesis.
    out.push_str(&face("Lexend", write("lx-400.woff2", LEXEND_400_NORMAL), 400, "normal", None));
    out.push_str(&face("Lexend", write("lx-600.woff2", LEXEND_600_NORMAL), 600, "normal", None));
    out.push_str(&face("Lexend", write("lx-700.woff2", LEXEND_700_NORMAL), 700, "normal", None));
    out
}

/// Build a CSS string literal: `"escaped contents"`.
fn css_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\A "),
            '\r' => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Compose the full standalone HTML document for the export pipeline.
/// Chromium navigates to a file:// URL of this and prints to PDF using
/// the @page rules.
pub fn build_export_html(
    config: &BookConfig,
    enriched_html: &str,
    structure: &BookStructure,
    front_cover_path: Option<&Path>,
    back_cover_path: Option<&Path>,
) -> String {
    let css = build_export_css(config, structure);
    // User's custom stylesheet (next to book.toml), appended AFTER the
    // generated CSS so its rules win. Absent file = no custom styling.
    let custom_css = std::fs::read_to_string(config.root_dir.join("custom.css")).unwrap_or_default();
    let title = html_escape(&config.book.title);
    let lang = config.book.language.clone().unwrap_or_else(|| "en".to_string());

    let cover_to_html = |path: Option<&Path>, kind: &str| -> String {
        match path {
            Some(p) if p.is_file() => {
                let url = format!("file:///{}", p.display().to_string().replace('\\', "/"));
                format!(
                    r#"<section class="book-cover book-cover-{kind}"><img src="{url}" alt="{kind} cover" /></section>"#
                )
            }
            _ => String::new(),
        }
    };

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
<header class="book-title-source">{title}</header>
{front_cover}
{body}
{back_cover}
</body>
</html>
"#,
        lang = lang,
        title = title,
        css = css,
        custom_css = custom_css,
        front_cover = cover_to_html(front_cover_path, "front"),
        body = enriched_html,
        back_cover = cover_to_html(back_cover_path, "back"),
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Render a standalone HTML document to PDF via headless Chromium.
/// `paper_size` is `(width_inches, height_inches)`.
// ============================================================================
// Two-pass TOC page numbers
// ----------------------------------------------------------------------------
// Chromium's print engine ignores `target-counter` / `leader()`, so the PDF
// TOC can't generate its own page numbers. Instead we render once with
// machine-readable markers, read back where each section landed and the
// folio printed on that page, inject the folios into the TOC, and render
// the final PDF. Section markers are present in BOTH passes (out-of-flow,
// 1px transparent) so body pagination is identical; the footer markers and
// the TOC folios live in fixed-size regions (margin box / the TOC's own
// lines) so they don't shift any page break either.
// ============================================================================

/// Tag every `<section id="…">` with a unique, near-invisible marker so a
/// later pdfium text scan can tell which physical page the section starts
/// on. Applied to both the measurement and the final HTML.
pub fn inject_section_markers(html: &str) -> String {
    let re = regex::Regex::new(r#"(<section\b[^>]*\bid="([^"]+)"[^>]*>)"#).unwrap();
    re.replace_all(html, |c: &regex::Captures| {
        format!(
            // position:absolute keeps the marker out of flow so it never
            // adds a line box (which otherwise pushed full-bleed figures
            // down by ~1 line, leaving a white strip across the top). The
            // transparent text still lands in the PDF text layer on the
            // right page, so the pdfium folio scan reads it fine.
            "{}<span class=\"tocmark\" style=\"position:absolute;font-size:1px;color:transparent\">@@S:{}@@</span>",
            &c[1], &c[2]
        )
    })
    .into_owned()
}

/// Tag every `<figure id="…">` with the same near-invisible marker the
/// sections use, so the page-folio scan also resolves figure positions
/// for the List of Figures. Reuses the `@@S:id@@` format → no change to
/// `extract_section_folios`. Applied to both passes.
pub fn inject_figure_markers(html: &str) -> String {
    let re = regex::Regex::new(r#"(<figure\b[^>]*\bid="([^"]+)"[^>]*>)"#).unwrap();
    re.replace_all(html, |c: &regex::Captures| {
        format!(
            // position:absolute keeps the marker out of flow so it never
            // adds a line box (which otherwise pushed full-bleed figures
            // down by ~1 line, leaving a white strip across the top). The
            // transparent text still lands in the PDF text layer on the
            // right page, so the pdfium folio scan reads it fine.
            "{}<span class=\"tocmark\" style=\"position:absolute;font-size:1px;color:transparent\">@@S:{}@@</span>",
            &c[1], &c[2]
        )
    })
    .into_owned()
}

/// Remove the `@@S:…@@` position-marker spans from the FINAL HTML. They're
/// out-of-flow (position:absolute), so deleting them doesn't shift any page
/// break — but their transparent text was still landing in the PDF's text
/// layer (selectable / extractable as "@@S:…@@" gibberish). The measurement
/// pass keeps them; the final render must not.
pub fn strip_tocmarks(html: &str) -> String {
    let re = regex::Regex::new(r#"<span class="tocmark"[^>]*>[^<]*</span>"#).unwrap();
    re.replace_all(html, "").into_owned()
}

/// Wrap the page-number margin-box content in delimiters so the printed
/// folio (already lower-roman for front matter / arabic for body, as
/// Chromium computed it) can be read straight out of the page text.
/// Measurement pass only — never goes in the final PDF.
pub fn wrap_footer_markers(html: &str) -> String {
    html.replace(
        "content: counter(page, lower-roman);",
        "content: \"@@PG@@\" counter(page, lower-roman) \"@@PGEND@@\";",
    )
    .replace(
        "content: counter(page, upper-roman);",
        "content: \"@@PG@@\" counter(page, upper-roman) \"@@PGEND@@\";",
    )
    .replace(
        "content: counter(page);",
        "content: \"@@PG@@\" counter(page) \"@@PGEND@@\";",
    )
}

/// Read the measurement PDF: for each page, pull its printed folio and the
/// section markers it carries, mapping section id → folio string. Returns
/// None if pdfium can't be bound (caller falls back to a TOC with no page
/// numbers rather than failing the export).
pub fn extract_section_folios(
    pdf_path: &Path,
) -> Option<std::collections::HashMap<String, String>> {
    use pdfium_render::prelude::*;
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
        .or_else(|_| {
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./lib/"))
        })
        .or_else(|_| Pdfium::bind_to_system_library())
        .ok()?;
    let pdfium = Pdfium::new(bindings);
    let bytes = std::fs::read(pdf_path).ok()?;
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).ok()?;

    let pg_re = regex::Regex::new(r"@@PG@@(.*?)@@PGEND@@").unwrap();
    let sec_re = regex::Regex::new(r"@@S:(.+?)@@").unwrap();

    // Per page, in document order: the printed folio (if any) and the
    // marker ids on that page.
    let mut pages: Vec<(Option<String>, Vec<String>)> = Vec::new();
    for page in doc.pages().iter() {
        let text = page.text().map(|t| t.all()).unwrap_or_default();
        let folio = pg_re
            .captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        let ids: Vec<String> = sec_re
            .captures_iter(&text)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();
        pages.push((folio, ids));
    }

    // Full-page figures sit on the zero-margin `cover` page, which prints no
    // folio — but the page counter still advanced on it. Infer that page's
    // folio from the nearest page carrying a numeric (arabic body) folio,
    // offset by the physical-page distance. Pages between resets are
    // contiguous, so neighbour ± distance is exact.
    // Offset a known folio by `delta` pages, preserving its numbering style
    // (arabic, or upper/lower roman — the body can use roman page numbers).
    let offset_folio = |f: &str, delta: i64| -> Option<String> {
        if let Ok(n) = f.parse::<i64>() {
            let v = n + delta;
            return (v >= 0).then(|| v.to_string());
        }
        if let Some(r) = crate::typesetter::structure::parse_roman(f) {
            let v = r as i64 + delta;
            if v >= 1 {
                let roman = crate::typesetter::structure::to_roman(v as u32);
                let lower = !f.chars().any(|c| c.is_ascii_uppercase());
                return Some(if lower { roman.to_lowercase() } else { roman });
            }
        }
        None
    };

    let folio_at = |i: usize| -> Option<String> {
        if let Some(f) = &pages[i].0 {
            return Some(f.clone());
        }
        for d in 1..pages.len() {
            if i >= d {
                if let Some(f) = &pages[i - d].0 {
                    if let Some(r) = offset_folio(f, d as i64) {
                        return Some(r);
                    }
                }
            }
            if i + d < pages.len() {
                if let Some(f) = &pages[i + d].0 {
                    if let Some(r) = offset_folio(f, -(d as i64)) {
                        return Some(r);
                    }
                }
            }
        }
        None
    };

    let mut map = std::collections::HashMap::new();
    for i in 0..pages.len() {
        let folio = match folio_at(i) {
            Some(f) => f,
            None => continue,
        };
        for id in &pages[i].1 {
            map.entry(id.clone()).or_insert_with(|| folio.clone());
        }
    }
    Some(map)
}

/// Splice the resolved folio (and a dot-leader span) into each TOC entry.
/// Entries whose section wasn't found just get no folio (blank leader).
pub fn inject_toc_folios(
    html: &str,
    folios: &std::collections::HashMap<String, String>,
) -> String {
    let re = regex::Regex::new(r##"(?s)(<a class="toc-entry[^"]*" href="#([^"]+)">)(.*?)(</a>)"##)
        .unwrap();
    re.replace_all(html, |c: &regex::Captures| {
        let folio = folios.get(&c[2]).map(|s| s.as_str()).unwrap_or("");
        format!(
            "{}{}<span class=\"toc-leader\"></span><span class=\"toc-folio\">{}</span>{}",
            &c[1], &c[3], folio, &c[4]
        )
    })
    .into_owned()
}

pub fn html_to_pdf(
    html: &str,
    output: &Path,
    paper_size: (f64, f64),
) -> Result<(), PdfExportError> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Stash the HTML in a temp file so Chromium can navigate to it via
    // file://. Using a data: URL would work for small docs but the
    // manuscript HTML is ~260 KB — file:// is safer.
    let temp_dir = std::env::temp_dir().join("marlos-pdf-export");
    std::fs::create_dir_all(&temp_dir)?;
    let temp_html = temp_dir.join(format!("book-{}.html", uuid::Uuid::new_v4()));
    std::fs::write(&temp_html, html)?;

    let file_url = format!(
        "file:///{}",
        temp_html.display().to_string().replace('\\', "/")
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(PathBuf::from(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        )))
        .build()
        .map_err(|e| PdfExportError::Chromium(format!("launch options: {}", e)))?;

    let browser = Browser::new(launch_opts)?;
    let tab = browser.new_tab()?;
    tab.navigate_to(&file_url)?;
    tab.wait_until_navigated()?;

    // Belt-and-suspenders: wait for the body to render. The document is
    // small enough (everything inline) that this is near-instant.
    tab.wait_for_element("body")?;

    let pdf_options = PrintToPdfOptions {
        landscape: Some(false),
        display_header_footer: Some(false),
        print_background: Some(true),
        scale: Some(1.0),
        paper_width: Some(paper_size.0),
        paper_height: Some(paper_size.1),
        margin_top: Some(0.0),
        margin_right: Some(0.0),
        margin_bottom: Some(0.0),
        margin_left: Some(0.0),
        page_ranges: None,
        prefer_css_page_size: Some(true),
        ..Default::default()
    };

    let pdf_bytes = tab.print_to_pdf(Some(pdf_options))?;

    // Write to a sibling temp file first, then rename. This is atomic on
    // POSIX and gives a clean failure mode on Windows when the target is
    // locked by an open PDF viewer.
    let staging = output.with_extension("pdf.partial");
    if let Err(e) = std::fs::write(&staging, &pdf_bytes) {
        let _ = std::fs::remove_file(&temp_html);
        return Err(map_io_lock_error(e, &staging));
    }
    if let Err(e) = std::fs::rename(&staging, output) {
        // Rename failed — likely target is open in a viewer. Don't leak
        // the staging file.
        let _ = std::fs::remove_file(&staging);
        let _ = std::fs::remove_file(&temp_html);
        return Err(map_io_lock_error(e, output));
    }

    // Best-effort cleanup of the temp html
    let _ = std::fs::remove_file(&temp_html);

    Ok(())
}

/// On Windows, OS error 32 (ERROR_SHARING_VIOLATION) means the file is
/// open in another process — typically a PDF viewer. Wrap with a
/// human-friendly message instead of leaking the cryptic OS string.
fn map_io_lock_error(err: std::io::Error, path: &Path) -> PdfExportError {
    if err.raw_os_error() == Some(32) {
        return PdfExportError::Io(std::io::Error::new(
            err.kind(),
            format!(
                "{} is open in another app (probably a PDF viewer). \
                 Close it and re-export.",
                path.display()
            ),
        ));
    }
    PdfExportError::Io(err)
}

pub fn paper_size_from_trim(trim_size: &str) -> (f64, f64) {
    match trim_size {
        "5x8" => (5.0, 8.0),
        "5.5x8.5" => (5.5, 8.5),
        _ => (6.0, 9.0),
    }
}
