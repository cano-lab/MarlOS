//! EPUB export via pandoc.
//!
//! Pandoc's EPUB writer is solid out of the box, so this is mostly a
//! command-line wrapper. The same source markdown that produces the
//! print PDF produces the EPUB — different rendering paths from one
//! source.
//!
//! EPUB doesn't use CSS Paged Media (ebooks reflow). The accompanying
//! CSS is small: typography, headings, blockquotes — no @page, no
//! print-only chrome.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

use super::book_config::BookConfig;
use super::pandoc::{PandocConvertOptions, PandocConverter};

#[derive(Debug, thiserror::Error)]
pub enum EpubExportError {
    #[error("pandoc binary not found — install pandoc")]
    PandocNotFound,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("pandoc exited with status {status}: {stderr}")]
    PandocFailed { status: i32, stderr: String },
    #[error("invalid book config: {0}")]
    Invalid(String),
}

/// Minimal EPUB stylesheet. Ebooks reflow, so we keep this to typography
/// and structural styling — no page rules, no fixed dimensions.
fn build_epub_css(config: &BookConfig) -> String {
    let body_font = format!(
        "\"{}\", Georgia, \"Times New Roman\", serif",
        config.typography.body_font
    );
    format!(
        r#"
body {{
  font-family: {bf};
  font-size: 1em;
  line-height: 1.5;
  margin: 0 1em;
}}

h1, h2, h3, h4 {{
  font-family: {bf};
  font-weight: 600;
  line-height: 1.2;
  margin: 1.5em 0 0.5em;
  page-break-after: avoid;
}}

h1 {{
  font-size: 2em;
  text-align: center;
  margin-top: 2em;
  font-variant: small-caps;
  letter-spacing: 0.04em;
}}

h2 {{ font-size: 1.4em; }}
h3 {{ font-size: 1.15em; font-style: italic; font-weight: 500; }}

p {{
  margin: 0;
  text-indent: 1.2em;
  text-align: justify;
  hyphens: auto;
  -webkit-hyphens: auto;
  widows: 2;
  orphans: 2;
}}

h1 + p, h2 + p, h3 + p,
section > p:first-of-type,
blockquote p:first-of-type {{
  text-indent: 0;
}}

blockquote {{
  margin: 1em 1.5em;
  font-style: italic;
}}

/* Centered blocks: pandoc `:::center` fenced divs (the "Center" tool /
   centered equations) and display math. EPUB reflows, so this is just
   text-align + auto margins — no page rules. Matches the PDF/preview. */
.center, .center > p {{
  text-align: center;
  text-indent: 0;
}}
.math.display {{
  display: block;
  text-align: center;
  text-indent: 0;
  margin: 1em 0;
}}
math[display="block"] {{
  display: block;
  margin: 1em auto;
  text-align: center;
}}

/* Figures. EPUB reflows, so the print-only bleed/page mechanics don't
   apply: .fig-fullpage just becomes a full-width image. .fig-crop still
   crops to a fixed aspect ratio (object-fit needs the constrained box
   that --fig-crop-ar supplies). */
figure {{
  margin: 1em auto;
  text-align: center;
}}
figure img {{
  max-width: 100%;
  height: auto;
}}
figcaption {{
  font-size: 0.9em;
  font-style: italic;
  margin-top: 0.4em;
  text-align: center;
}}
figure.fig-fullpage img {{ width: 100%; }}
figure.fig-crop img {{
  aspect-ratio: var(--fig-crop-ar, auto);
  width: 100%;
  height: auto;
  object-fit: cover;
  object-position: var(--fig-crop-pos, center);
}}

ul, ol {{
  margin: 0.5em 0 0.5em 1.5em;
  padding: 0;
}}

li {{ margin: 0.2em 0; }}

hr {{
  border: 0;
  height: 0;
  margin: 1em 0;
  visibility: hidden;
}}

.footnote-ref a {{
  text-decoration: none;
  font-size: 0.8em;
  vertical-align: super;
  line-height: 0;
}}

.footnotes {{
  font-size: 0.92em;
  margin-top: 2em;
  padding-top: 1em;
  border-top: 1px solid #ccc;
}}

a {{ color: #1a5490; }}

em, i {{ font-style: italic; }}
strong, b {{ font-weight: 600; }}

/* Word-anchor fixation emphasis — see the Rust transform. The
   leading half of each prose word gets <b class="word-anchor"> so the
   eye anchors faster. Math, code, headings, and math-anchor callouts
   are excluded by the transform. */
b.word-anchor {{ font-weight: 700; }}

/* Hide headings marked with .hidden-heading — used for the back-
   cover chapter so it doesn't show "Back Cover" text on the page. */
h1.hidden-heading, h2.hidden-heading, h3.hidden-heading {{
  display: none;
}}

/* Back cover (front cover is registered separately by pandoc via
   --epub-cover-image and doesn't need styling here). */
.book-cover {{
  margin: 0;
  padding: 0;
  text-align: center;
}}

.book-cover img {{
  display: block;
  max-width: 100%;
  height: auto;
}}

/* Citation superscript references in body. */
sup.note-ref {{
  font-size: 0.75em;
  vertical-align: super;
  line-height: 0;
}}
sup.note-ref a {{
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
}}
.notes-list .note-num {{
  font-weight: 500;
  margin-right: 0.3em;
}}
.notes-list .note-back {{
  text-decoration: none;
  margin-left: 0.3em;
  color: #888;
}}

/* Pandoc's EPUB nav generates an <ol> which most readers render with
   numbers (1. Title page, 2. Author's Note, ..., 5. Chapter 1) — the
   numbering doesn't match what readers expect from a book TOC. Strip
   the numbers; the H1 text inside each entry already tells the story. */
nav#toc, nav[role="doc-toc"], nav[epub|type="toc"] {{
  font-family: {bf};
  line-height: 1.6;
}}

nav#toc ol, nav[role="doc-toc"] ol, nav[epub|type="toc"] ol {{
  list-style-type: none;
  margin: 0;
  padding-left: 0;
}}

nav#toc ol ol, nav[role="doc-toc"] ol ol, nav[epub|type="toc"] ol ol {{
  margin-left: 1em;
}}

nav#toc li, nav[role="doc-toc"] li, nav[epub|type="toc"] li {{
  margin: 0.4em 0;
  padding: 0;
}}

nav#toc a, nav[role="doc-toc"] a, nav[epub|type="toc"] a {{
  text-decoration: none;
  color: inherit;
}}

nav#toc h1, nav[role="doc-toc"] h1, nav[epub|type="toc"] h1 {{
  font-size: 1.6em;
  text-align: center;
  margin-top: 2em;
  font-variant: small-caps;
  letter-spacing: 0.04em;
}}
"#,
        bf = body_font,
    )
}

/// Run pandoc to convert the book's markdown files into a single EPUB3.
pub async fn export_epub(
    config: &BookConfig,
    output: &Path,
) -> Result<(), EpubExportError> {
    let pandoc = PandocConverter::resolve_binary().ok_or(EpubExportError::PandocNotFound)?;

    if config.files.is_empty() {
        return Err(EpubExportError::Invalid("files list is empty".into()));
    }

    // Run citation transform + concatenate files into one temp md.
    // This makes the EPUB get the same per-chapter Notes section the
    // PDF does, with consistent superscript IDs.
    let (temp_md, _citation_result) =
        super::citations::prepare_book_markdown(config)
            .map_err(|e| EpubExportError::Invalid(e.to_string()))?;

    // Resolve cover paths. We surface a clear error if either is
    // configured but the file is missing — silent skip used to hide
    // stale paths in book.toml after the user moved the book dir.
    let resolve_cover = |rel: &Option<String>| -> Result<Option<PathBuf>, EpubExportError> {
        match rel {
            None => Ok(None),
            Some(s) if s.trim().is_empty() => Ok(None),
            Some(s) => {
                let p = PathBuf::from(s);
                let abs = if p.is_absolute() { p } else { config.root_dir.join(&p) };
                if abs.is_file() {
                    Ok(Some(abs))
                } else {
                    Err(EpubExportError::Invalid(format!(
                        "cover image configured but file not found: {} (update Settings → Covers)",
                        abs.display(),
                    )))
                }
            }
        }
    };
    let front_cover_abs = resolve_cover(&config.book.cover_image)?;
    let back_cover_abs = resolve_cover(&config.book.back_cover_image)?;

    // EPUB has no --epub-back-cover-image; inject the back cover as
    // the very last chapter of the spine so it renders as the final
    // page. The H1 is hidden via CSS so it doesn't visibly title
    // the page, but stays for navigation purposes.
    if let Some(back_path) = &back_cover_abs {
        let mut existing = std::fs::read_to_string(&temp_md)?;
        let url = format!("file:///{}", back_path.display().to_string().replace('\\', "/"));
        existing.push_str(&format!(
            "\n\n# Back Cover {{.hidden-heading}}\n\n<div class=\"book-cover book-cover-back\">\n<img src=\"{}\" alt=\"Back cover\" />\n</div>\n",
            url,
        ));
        std::fs::write(&temp_md, existing)?;
    }

    // Stage the EPUB CSS so pandoc can pull it via --css=path.
    let temp_dir = std::env::temp_dir().join("marlos-epub-export");
    std::fs::create_dir_all(&temp_dir)?;
    let css_path = temp_dir.join(format!("epub-{}.css", uuid::Uuid::new_v4()));
    std::fs::write(&css_path, build_epub_css(config))?;

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Word-anchor detour: render to HTML first so we can apply the
    // same post-pandoc transforms the PDF uses (math-anchor tagging,
    // figure-class hoisting, word emphasis), then convert that HTML to
    // EPUB. Without this we'd have to do the anchor pass at the
    // markdown level, which can't see math anchors as a block class.
    let intermediate_html_path: Option<PathBuf> = if config.export.word_anchors {
        // Native MathML so equations render in EPUB3 readers without JS
        // (KaTeX/MathJax spans need a runtime the reader won't run, so
        // the default --katex path leaves math blank). Matches the PDF.
        let opts = PandocConvertOptions {
            math_format: Some("mathml".to_string()),
            ..Default::default()
        };
        let pandoc_html = PandocConverter::convert_file(&temp_md, &opts)
            .await
            .map_err(|e| EpubExportError::Invalid(format!("pandoc md→html failed: {}", e)))?;
        let mut html = super::citations::hoist_figure_classes(
            &super::citations::tag_math_anchors(
                &super::citations::normalize_unicode_scripts(&pandoc_html.html),
            ),
        );
        // Wrap the whole document in a chapter section so the
        // word_anchors transform's body_depth gate fires — EPUB
        // doesn't use the structure analysis that PDF does, so there's
        // no per-chapter wrapping in the pandoc output. This is
        // harmless: it just marks "everything is body prose" for
        // the transform.
        html = format!(
            r#"<section data-section-type="chapter">{}</section>"#,
            html
        );
        html = super::word_anchors::apply(&html);
        let html_path = temp_dir.join(format!("epub-html-{}.html", uuid::Uuid::new_v4()));
        std::fs::write(&html_path, &html)?;
        Some(html_path)
    } else {
        None
    };

    let mut cmd = Command::new(&pandoc);

    // Pick input + from-format based on whether the word-anchor pass
    // re-routed us through HTML.
    if let Some(html_path) = &intermediate_html_path {
        cmd.arg(html_path);
        cmd.args(["--from", "html", "--to", "epub3"]);
    } else {
        // Single combined+transformed input (instead of looping over
        // config.resolved_files()).
        cmd.arg(&temp_md);
        cmd.args([
            "--from",
            // lists_without_preceding_blankline: match the PDF/preview reader
            // so breakdown lists under a lead-in line ("What's in it:") render
            // as lists instead of being folded into the paragraph.
            // -yaml_metadata_block: treat `---` as a thematic break, not a
            // YAML block (matches the PDF/preview reader).
            "markdown+smart+footnotes+pipe_tables+lists_without_preceding_blankline-yaml_metadata_block",
            "--to",
            "epub3",
        ]);
    }

    // Render math as native MathML. EPUB3 readers display MathML without
    // any script; pandoc's default (no math flag) emits raw TeX in a
    // <span class="math">, which shows as blank/garbled source in most
    // readers — the "math not appearing in the EPUB" bug. Applies to
    // both the markdown and the word-anchor HTML input paths.
    cmd.args(["--mathml"]);

    cmd.args(["--css", &css_path.display().to_string()])
        // No --toc: don't generate an in-book TOC chapter. The manuscript
        // already has a hand-formatted "Contents" section. Pandoc still
        // auto-generates nav.xhtml from H1s for the reader's navigation
        // menu (required by the EPUB3 spec) — that's separate from the
        // in-content TOC and doesn't show up as a visible chapter.
        .arg("--standalone")
        .args(["--output", &output.display().to_string()])
        .args(["--metadata", &format!("title={}", config.book.title)]);

    if !config.book.subtitle.is_empty() {
        cmd.args(["--metadata", &format!("subtitle={}", config.book.subtitle)]);
    }
    if !config.book.author.is_empty() {
        cmd.args(["--metadata", &format!("author={}", config.book.author)]);
    }
    if !config.book.isbn.is_empty() {
        cmd.args(["--metadata", &format!("identifier={}", config.book.isbn)]);
    }
    if let Some(lang) = &config.book.language {
        if !lang.is_empty() {
            cmd.args(["--metadata", &format!("lang={}", lang)]);
        }
    }

    // Front cover: pandoc registers it as the EPUB cover via --epub-
    // cover-image. Path was already validated above.
    if let Some(front) = &front_cover_abs {
        cmd.args(["--epub-cover-image", &front.display().to_string()]);
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output_result = cmd.output().await?;
    // Best-effort cleanup of staged CSS + temp markdown + intermediate HTML.
    let _ = std::fs::remove_file(&css_path);
    if let Some(p) = &intermediate_html_path {
        let _ = std::fs::remove_file(p);
    }
    super::citations::cleanup_temp_markdown(&temp_md);

    if !output_result.status.success() {
        return Err(EpubExportError::PandocFailed {
            status: output_result.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output_result.stderr).to_string(),
        });
    }

    Ok(())
}
