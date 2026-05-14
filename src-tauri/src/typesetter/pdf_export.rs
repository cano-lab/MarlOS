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
    content: counter(page);
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
    content: counter(page, lower-roman);
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

section[data-section-type="chapter"],
section[data-section-type="interlude"] {{
  break-before: page;
}}

section[data-section-type="chapter"] {{
  break-before: right;
}}

section[data-section-type="chapter"] > h1 {{
  page: chapter-opener;
  font-size: 2em;
  text-align: center;
  margin: 4em 0 2em;
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
  counter-reset: page 1;
}}

section[data-section-type="chapter"] > p:first-of-type::first-letter {{
  float: left;
  font-size: 3.2em;
  line-height: 0.85;
  padding: 0.05em 0.08em 0 0;
  font-weight: 600;
}}

h1, h2, h3, h4 {{
  font-weight: 600;
  line-height: 1.2;
  break-after: avoid;
}}

h2 {{ font-size: 1.25em; margin: 1.4em 0 0.6em; }}
h3 {{ font-size: 1.05em; margin: 1.2em 0 0.4em; font-style: italic; font-weight: 500; }}

blockquote {{ margin: 1em 1.5em; font-style: italic; }}
ul, ol {{ margin: 0.5em 0 0.5em 1.5em; padding: 0; }}
li {{ margin: 0.2em 0; }}

.footnote-ref a {{ text-decoration: none; font-size: 0.8em; vertical-align: super; line-height: 0; }}
.footnotes {{ font-size: 0.92em; }}

.math {{ white-space: nowrap; }}
.math.display {{ display: block; text-align: center; margin: 1em 0; white-space: normal; }}

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
        font_faces = font_face_block,
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

    let face = |url: Option<String>, weight: u32, style: &str, range: &str| -> String {
        match url {
            Some(u) => format!(
                "@font-face {{ font-family: 'EB Garamond'; src: url('{}') format('woff2'); font-weight: {}; font-style: {}; font-display: swap; unicode-range: {}; }}\n",
                u, weight, style, range,
            ),
            None => String::new(),
        }
    };

    let mut out = String::new();
    // latin
    out.push_str(&face(write("ebg-latin-400.woff2", EBG_LATIN_400_NORMAL), 400, "normal", UNICODE_LATIN));
    out.push_str(&face(write("ebg-latin-400i.woff2", EBG_LATIN_400_ITALIC), 400, "italic", UNICODE_LATIN));
    out.push_str(&face(write("ebg-latin-600.woff2", EBG_LATIN_600_NORMAL), 600, "normal", UNICODE_LATIN));
    out.push_str(&face(write("ebg-latin-600i.woff2", EBG_LATIN_600_ITALIC), 600, "italic", UNICODE_LATIN));
    // latin-ext
    out.push_str(&face(write("ebg-latext-400.woff2", EBG_LATEXT_400_NORMAL), 400, "normal", UNICODE_LATIN_EXT));
    out.push_str(&face(write("ebg-latext-400i.woff2", EBG_LATEXT_400_ITALIC), 400, "italic", UNICODE_LATIN_EXT));
    out.push_str(&face(write("ebg-latext-600.woff2", EBG_LATEXT_600_NORMAL), 600, "normal", UNICODE_LATIN_EXT));
    out.push_str(&face(write("ebg-latext-600i.woff2", EBG_LATEXT_600_ITALIC), 600, "italic", UNICODE_LATIN_EXT));
    // greek
    out.push_str(&face(write("ebg-greek-400.woff2", EBG_GREEK_400_NORMAL), 400, "normal", UNICODE_GREEK));
    out.push_str(&face(write("ebg-greek-400i.woff2", EBG_GREEK_400_ITALIC), 400, "italic", UNICODE_GREEK));
    out.push_str(&face(write("ebg-greek-600.woff2", EBG_GREEK_600_NORMAL), 600, "normal", UNICODE_GREEK));
    out.push_str(&face(write("ebg-greek-600i.woff2", EBG_GREEK_600_ITALIC), 600, "italic", UNICODE_GREEK));
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
