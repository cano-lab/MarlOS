/**
 * Phase C/D — book-page CSS template.
 *
 * The doc principle: V1 ships with one set of values, but they all flow
 * through CSS variables driven by `book.toml`. Adding a second trim size,
 * a second body font, or a second margin profile in V2 is a config change,
 * not a CSS rewrite.
 *
 * Phase C scope: 6×9 page, asymmetric margins, body font, justification,
 * hyphenation, widow/orphan, basic heading hierarchy.
 *
 * Phase D additions: running headers (book title verso, chapter title
 * recto), page numbers (lowercase roman in front matter, arabic from
 * chapter 1 onward, in the bottom outside corner), forced-recto chapter
 * starts, chapter-opener styling (large centered title, drop cap),
 * interlude styling (italic, smaller, no forced recto).
 */

import type { BookConfig } from "../services/typesetter-service";

export interface TrimDimensions {
  /** width in CSS units, e.g. "6in" */
  width: string;
  height: string;
}

const TRIM_PRESETS: Record<string, TrimDimensions> = {
  "6x9": { width: "6in", height: "9in" },
  "5x8": { width: "5in", height: "8in" },
  "5.5x8.5": { width: "5.5in", height: "8.5in" },
};

export function trimDimensions(size: string): TrimDimensions {
  const preset = TRIM_PRESETS[size];
  if (preset) return preset;
  // Free-form "WxH" parsed as floats in inches (e.g. "5.25x8.0" for a
  // custom trim entered through the BookConfigEditor's Custom panel).
  const m = /^(\d+(?:\.\d+)?)x(\d+(?:\.\d+)?)$/.exec(size);
  if (m) {
    const w = parseFloat(m[1]);
    const h = parseFloat(m[2]);
    if (w > 0 && h > 0) return { width: `${w}in`, height: `${h}in` };
  }
  return TRIM_PRESETS["6x9"];
}

/**
 * Build the print CSS string for a given book config. Returns one
 * self-contained `<style>`-able string — fonts are *referenced* via
 * @fontsource which the calling component is responsible for importing.
 */
export function buildBookCss(config: BookConfig): string {
  const trim = trimDimensions(config.trim.size);
  const margins = config.trim.margins_in;
  const fontSize = `${config.typography.body_size_pt}pt`;
  const lineHeight = `${config.typography.body_leading_pt}pt`;
  const bodyFont = `"${config.typography.body_font}", Georgia, "Times New Roman", serif`;

  // Paged.js v0.4.x doesn't reliably resolve CSS custom properties inside
  // @page rules — `margin: var(--margin-top) ...` silently falls back to
  // 0 and you end up with full-bleed text on a 6×9 page. So we inline the
  // physical layout directly into @page descriptors. Variables remain
  // useful for body-level properties that cascade normally.
  const mTop = `${margins.top}in`;
  const mBot = `${margins.bottom}in`;
  const mIn = `${margins.inside}in`;
  const mOut = `${margins.outside}in`;

  // mIn used in asymmetric verso/recto margin rules below.
  void mIn;

  // Body page-number content. Front matter still uses lower-roman.
  const pageNumberContent =
    config.typography.page_number_style === "none"
      ? "none"
      : config.typography.page_number_style === "roman"
        ? "counter(page, upper-roman)"
        : config.typography.page_number_style === "lower-roman"
          ? "counter(page, lower-roman)"
          : "counter(page)";

  // Running header style — the chapter strip at the top of each page.
  // The H1's string-set captures the running header text; @top-center
  // pulls it via string(). Counter increments per chapter so we can
  // bake an arabic or roman number into the captured string.
  const headerStyle = config.typography.running_header_style ?? "title";
  let stringSetExpr: string;
  switch (headerStyle) {
    case "chapter-number":
      stringSetExpr = `"Chapter " counter(chapter-num)`;
      break;
    case "chapter-number-title":
      stringSetExpr = `"Chapter " counter(chapter-num) " \\00B7 " content()`;
      break;
    case "compact-arabic":
      stringSetExpr = `counter(chapter-num) " \\00B7 " content()`;
      break;
    case "compact-roman":
      stringSetExpr = `counter(chapter-num, upper-roman) " \\00B7 " content()`;
      break;
    default:
      // "title" — historic behavior
      stringSetExpr = `content()`;
      break;
  }
  const chapterCounterCss =
    headerStyle === "title"
      ? ""
      : `
body { counter-reset: chapter-num; }
section[data-section-type="chapter"] { counter-increment: chapter-num; }`;

  // Rebuild step 4: asymmetric verso/recto margins via @page :left/:right
  // (standalone pseudo-classes work in Paged.js v0.4 — only the named+
  // pseudo combo `@page name:pseudo` is broken). Default @page still
  // carries running header + page number so they apply uniformly.
  return `
@page {
  size: ${trim.width} ${trim.height};
  /* Margin lives in :left/:right below — leaving it off the default
     rule lets the asymmetric values win the cascade in Paged.js,
     which otherwise applies the universal @page margin first and
     never picks up the :left/:right override. Without this, the
     on-screen page count is too low (body is 4.25" instead of
     4.125") and disagrees with the PDF export. */
  @top-center {
    content: string(chapter-name);
    font-size: 9pt;
    font-variant: small-caps;
    letter-spacing: 0.12em;
    color: #444;
  }
  @bottom-center {
    content: ${pageNumberContent};
    font-family: var(--body-font);
    font-size: 9pt;
    color: #444;
    padding-top: 6pt;
  }
}

/* Verso (left) — inside margin on the RIGHT (toward the spine). */
@page :left {
  margin: ${mTop} ${mIn} ${mBot} ${mOut};
}

/* Recto (right) — inside margin on the LEFT. */
@page :right {
  margin: ${mTop} ${mOut} ${mBot} ${mIn};
}

/* Cover pages: full bleed, no margins, no header, no page number. */
@page cover {
  margin: 0;
  @top-center { content: none; }
  @top-left { content: none; }
  @top-right { content: none; }
  @bottom-center { content: none; }
  @bottom-left { content: none; }
  @bottom-right { content: none; }
}

.book-title-source {
  display: none;
}

.book-cover {
  page: cover;
  break-before: page;
  break-after: page;
  margin: 0;
  padding: 0;
}

.book-cover img {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: cover;
}

section[data-section-type="chapter"] {
  /* Forced-recto chapter starts (the page count source-of-truth that
     the PDF export uses). Paged.js handles the standalone right
     keyword fine — only the named+pseudo combo (e.g. @page name:left)
     is what was breaking. */
  break-before: right;
  page-break-before: right;
}

section[data-section-type="interlude"] {
  break-before: page;
}

/* Each top-level back-matter heading (Notes, Appendix, About the
   Author, etc.) starts a new page. Without this they flow together
   and the appendix lands halfway down the last Notes page. */
section[data-section-type="back-matter"] {
  break-before: page;
}

/* Chapter opener: suppress the running chapter-name header on the
   opener page. The H1 itself carries the 4em top margin — we don't
   add page-level margin here, otherwise the chapter-opener pages
   would have less body height and the preview page count would
   diverge from the PDF. */
@page chapter-opener {
  @top-center { content: none; }
}

section[data-section-type="chapter"] > h1 {
  page: chapter-opener;
  string-set: chapter-name ${stringSetExpr};
  font-size: 2em;
  text-align: center;
  margin: 4em 0 2em;
  font-variant: small-caps;
  letter-spacing: 0.03em;
  font-weight: 600;
  text-wrap: balance;
  line-height: 1.2;
}

/* Running header for non-chapter sections (Author's Note, Glossary,
   interludes, back matter): feed the section's title into the same
   chapter-name string the @top-center header reads, so those pages get
   a running header too (the PDF pipeline also honors a {header="..."}
   override per section; the preview uses the title). */
section[data-section-type="front-matter"] > h1,
section[data-section-type="interlude"] > h1,
section[data-section-type="back-matter"] > h1 {
  string-set: chapter-name content();
}

/* Body-level font/size/leading — pulled from book.toml typography so
   preview density matches PDF body. */
html, body {
  font-family: ${bodyFont};
  font-size: ${fontSize};
  line-height: ${lineHeight};
}

p {
  margin: 0;
  text-indent: 1em;
}

/* Display equations ($$...$$) — pandoc emits an inline <span class="math
   display"> that KaTeX renders into. Without making the wrapper a block,
   KaTeX's own centering has nothing to center against and the equation
   hugs the left margin. Force the block + center so all display
   equations sit centered (matches the PDF pipeline). Inline math
   ($...$) stays in the text flow. */
.math.display {
  display: block;
  text-align: center;
  text-indent: 0;
  margin: 1em 0;
}

/* Several rules from pdf_export.rs::build_export_css are intentionally
   PDF-only because Paged.js v0.4's chunker throws
   "item doesn't belong to list" when it encounters them mid-split:
     - text-align: justify
     - hyphens: auto
     - widows / orphans
     - h1, h2, h3, h4 { break-after: avoid }
     - h1 + p, h2 + p, h3 + p, section > p:first-of-type { text-indent: 0 }
     - section[chapter] > p:first-of-type::first-letter (drop cap)
   Chromium native print resolves these against the original DOM and
   handles them cleanly — preview gets a slightly looser typography
   but no crashes. */

h2 { font-size: 1.25em; margin: 1.4em 0 0.6em; font-weight: 600; }
h3 { font-size: 1.05em; margin: 1.2em 0 0.4em; font-style: italic; font-weight: 500; }

section[data-section-type="interlude"] > h1 {
  font-size: 1.4em;
  text-align: center;
  margin: 3em 0 1.5em;
  font-style: italic;
  font-weight: 400;
}

blockquote { margin: 1em 1.5em; font-style: italic; }

/* Centered line/block. Written in markdown as a pandoc fenced div:
     :::center
     $\vec{F} = m\vec{a}$
     :::
   The "⊟ Center" toolbar button wraps the current line(s) in this.
   Works for equations, single lines, or whole paragraphs. */
.center, .center > p, .center > h1, .center > h2, .center > h3 {
  text-align: center;
  text-indent: 0;
}

/* "Math Anchor" callout boxes — tagged with class="math-anchor" by the
   structure pipeline. Bordered, lightly tinted box that stays on one
   page. Mirrors pdf_export.rs::build_export_css. */
blockquote.math-anchor {
  margin: 1.2em 0;
  padding: 0.6em 0.9em;
  border: 1px solid #aaaaaa;
  border-left: 3px solid #555555;
  background: #f5f5f5;
  font-style: normal;
  break-inside: avoid;
  page-break-inside: avoid;
}
blockquote.math-anchor > :first-child { margin-top: 0; }
blockquote.math-anchor > :last-child { margin-bottom: 0; }
blockquote.math-anchor p { text-indent: 0; }
ul, ol { margin: 0.5em 0 0.5em 1.5em; padding: 0; }
li { margin: 0.2em 0; }

/* Section dividers (markdown --- / ***) — removed from flow entirely
   so neither the 3-star ornament nor a default rule line shows. Use
   a <div class="space-medium"> snippet for a visible scene break. */
hr {
  display: none;
}

/* Generated front-matter pages (title / copyright / dedication).
   Matched against pdf_export.rs so preview and PDF look the same. */
@page no-page-number {
  @top-center { content: none; }
  @bottom-center { content: none; }
  @bottom-left { content: none; }
  @bottom-right { content: none; }
}

/* Table of Contents (generated). Chapters carry a "N." prefix; other
   sections (front matter, interludes, back matter) are italic with no
   number. Dot leaders + page numbers come from target-counter, which
   Paged.js supports — the Chromium PDF path fills these via a separate
   pass and so omits them here. Front-matter entries use lower-roman
   folios to match their pages. */
.generated-toc-page {
  break-before: right;
  page-break-before: right;
  /* Force the body that follows the TOC onto a fresh page. */
  break-after: page;
  page-break-after: always;
}
.gen-toc-title {
  text-align: center;
  font-variant: small-caps;
  letter-spacing: 0.08em;
  font-size: 1.4em;
  /* Tight top margin so the 20+ entries fit on one page. */
  margin: 0 0 1em;
}
.toc-entry {
  display: block;
  text-decoration: none;
  color: inherit;
  text-indent: 0;
  margin: 0.45em 0;
  line-height: 1.3;
}
.toc-num { display: inline-block; min-width: 1.9em; }
.toc-other { font-style: italic; padding-left: 1.9em; }
.toc-entry::after {
  content: leader('.') target-counter(attr(href), page);
}
.toc-fm.toc-entry::after {
  content: leader('.') target-counter(attr(href), page, lower-roman);
}

/* List of Figures (generated back matter) — reuses .toc-entry, so the
   leader + folio come from the ::after rule above. Starts on its own
   page; .lof-num / .fig-num are the "Fig. N." labels. */
.generated-lof-page {
  break-before: page;
  page-break-before: always;
}
.gen-lof-title {
  text-align: center;
  font-variant: small-caps;
  letter-spacing: 0.08em;
  font-size: 1.4em;
  margin: 0 0 1em;
}
.lof-num { font-variant: small-caps; padding-right: 0.3em; }
.fig-num { font-style: normal; font-variant: small-caps; padding-right: 0.25em; }

.generated-title-page,
.generated-copyright-page,
.generated-dedication-page {
  break-before: right;
  page-break-before: right;
  break-after: page;
  page-break-after: always;
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  text-align: center;
}

.generated-title-page { page: no-page-number; }
.generated-dedication-page { page: no-page-number; }

.generated-title-page .title-page-inner,
.generated-copyright-page .copyright-page-inner,
.generated-dedication-page .dedication-inner {
  width: 100%;
  max-width: 4in;
}

/* Title-page elements override the generic <p> rules so every line is
   center-aligned with no first-line indent. */
.generated-title-page .gen-book-title,
.generated-title-page .gen-book-subtitle,
.generated-title-page .gen-book-author {
  text-align: center;
  text-indent: 0;
}

.generated-title-page .gen-book-title {
  font-size: 2.4em;
  margin: 0 0 0.5em;
  font-variant: small-caps;
  letter-spacing: 0.04em;
  font-weight: 600;
  line-height: 1.15;
}

.generated-title-page .gen-book-subtitle {
  font-size: 1.1em;
  font-style: italic;
  margin: 0 0 2.5em;
  color: #333;
}

.generated-title-page .gen-book-author {
  font-size: 1.05em;
  font-variant: small-caps;
  letter-spacing: 0.08em;
  margin: 0;
}

.generated-copyright-page .copyright-page-inner {
  font-size: 0.85em;
  line-height: 1.5;
  color: #222;
}

.generated-copyright-page p {
  margin: 0 0 0.6em;
  text-indent: 0;
  text-align: center;
}

.generated-copyright-page .gen-publisher {
  font-style: italic;
  margin-top: 1.2em;
}

.generated-copyright-page .gen-isbn {
  font-family: var(--font-mono, monospace);
  letter-spacing: 0.05em;
}

.generated-dedication-page .dedication-inner {
  font-size: 1.05em;
  font-style: italic;
  line-height: 1.5;
}

.generated-dedication-page p {
  margin: 0;
  text-indent: 0;
  text-align: center;
}

/* Acknowledgements page — back matter. Centered italic body under a
   small-caps heading. Flows across pages naturally if the text is long. */
.generated-acknowledgements-page {
  break-before: page;
}

.generated-acknowledgements-page .gen-ack-heading {
  text-align: center;
  font-size: 1.4em;
  font-variant: small-caps;
  letter-spacing: 0.08em;
  font-weight: 600;
  margin: 4em 0 2em;
}

.generated-acknowledgements-page .acknowledgements-inner {
  max-width: 4in;
  margin: 0 auto;
  font-style: italic;
  text-align: center;
  line-height: 1.6;
}

.generated-acknowledgements-page .acknowledgements-inner p {
  margin: 0 0 1em;
  text-indent: 0;
}

/* Phase J: figures + captions (inline only). Same rules as PDF; the
   wrapper comes from pandoc's implicit_figures extension. */
figure {
  break-inside: avoid;
  page-break-inside: avoid;
  margin: 1em auto;
  display: block;
  text-align: center;
  max-width: 100%;
}

figure img {
  max-width: 100%;
  height: auto;
  display: block;
  margin: 0 auto;
}

figcaption {
  font-size: 0.9em;
  font-style: italic;
  margin-top: 0.4em;
  text-align: center;
  text-wrap: balance;
  text-indent: 0;
  color: #333;
}

/* Phase K: figure layout modes — mirrors pdf_export.rs.
   .fig-text fills the text block; .fig-bleed escapes toward the page edge
   (symmetric outside-margin negative margins), with --fig-inset pulling it
   back for a gutter-safe near-bleed. */
figure.fig-text { max-width: 100%; }
figure.fig-text img { width: 100%; }
figure.fig-bleed {
  margin-left: calc(-1 * (${mOut} - var(--fig-inset, 0in)));
  margin-right: calc(-1 * (${mOut} - var(--fig-inset, 0in)));
  max-width: none;
}
figure.fig-bleed img { width: 100%; }

/* Float modes — image sits in the text, paragraphs wrap around it. */
figure.fig-float-left { float: left; max-width: 48%; margin: 0.2em 1.2em 0.6em 0; }
figure.fig-float-right { float: right; max-width: 48%; margin: 0.2em 0 0.6em 1.2em; }
figure.fig-float-left img, figure.fig-float-right img { width: 100%; }

/* .fig-fullpage — full-bleed plate on its own page; .fig-crop — crop to a
   fixed aspect ratio. Mirrors pdf_export.rs. Uses the zero-margin 'cover'
   page (same mechanism as the book covers) so the image fills exactly one
   trim-sized page instead of overflowing onto the next. */
figure.fig-fullpage {
  page: cover;
  margin: 0;
  padding: 0;
  max-width: none;
  overflow: hidden;
  break-before: page;
  break-after: page;
  page-break-before: always;
  page-break-after: always;
}
figure.fig-fullpage img {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: var(--fig-fit, cover);
  object-position: var(--fig-crop-pos, center);
}
figure.fig-fullpage figcaption { display: none; }

figure.fig-crop img {
  aspect-ratio: var(--fig-crop-ar, auto);
  width: 100%;
  height: auto;
  object-fit: cover;
  object-position: var(--fig-crop-pos, center);
}


/* Manual paragraph-spacing utility classes — use raw HTML in markdown:
     <div class="space-small"></div>      ~half line
     <div class="space-medium"></div>     ~one line
     <div class="space-large"></div>      ~two lines
     <div class="space-section"></div>    ~four lines
     <div class="blank-page"></div>       full blank page
     <div class="page-break"></div>       new page from here on
*/
.space-small  { height: 0.6em; break-inside: avoid; }
.space-medium { height: 1.2em; break-inside: avoid; }
.space-large  { height: 2.4em; break-inside: avoid; }
.space-section { height: 4em;   break-inside: avoid; }
.blank-page {
  break-before: page;
  break-after: page;
  height: 0;
  visibility: hidden;
}
.page-break {
  break-before: page;
  height: 0;
  visibility: hidden;
}

/* All inline super/subscripts (math exponents, chemical subscripts,
   manual footnote markers). The structure pipeline rewrites precomposed
   Unicode super/subscripts into <sup>/<sub> with ASCII glyphs; pinning
   the body font + lining numerals here keeps every digit consistent and
   matches the PDF export pipeline. */
sup, sub {
  font-family: ${bodyFont};
  font-size: 0.72em;
  line-height: 0;
  font-variant-numeric: lining-nums;
}
sup { vertical-align: super; }
sub { vertical-align: sub; }

/* Citation superscript references in body. */
sup.note-ref {
  font-size: 0.75em;
  vertical-align: super;
  line-height: 0;
}

sup.note-ref a {
  text-decoration: none;
  color: inherit;
}

/* Notes back-matter section. */
.notes-list {
  list-style: none;
  padding: 0;
  margin: 1em 0;
}

.notes-list li {
  text-indent: -2em;
  padding-left: 2em;
  margin-bottom: 0.6em;
  /* Permissive break rules — some bibliography entries (e.g. Chapter 8
     note 4 with 10+ author names) are nearly a full page tall and
     Paged.js silently drops them if widow/orphan or break rules
     can't be satisfied. */
  widows: 1;
  orphans: 1;
  break-inside: auto;
  page-break-inside: auto;
}

.notes-list .note-num {
  font-weight: 500;
  margin-right: 0.3em;
}

.notes-list .note-back {
  text-decoration: none;
  margin-left: 0.3em;
  color: #666;
}

${chapterCounterCss}
`.trim();
}

/**
 * Build a stand-alone HTML document suitable for paginating with Paged.js
 * or printing via window.print(). The caller is responsible for ensuring
 * the EB Garamond fonts are loaded via @fontsource imports in the parent
 * app — Paged.js will inherit them through the same document.
 */
export function buildBookDocument(config: BookConfig, bodyHtml: string): string {
  const css = buildBookCss(config);
  const title = config.book.title || "Book";
  // The .book-title-source marker is the source of the `book-title`
  // CSS named string used by the verso running header.
  return `<!DOCTYPE html>
<html lang="${config.book.language ?? "en"}">
<head>
<meta charset="utf-8" />
<title>${escapeHtml(title)}</title>
<style>${css}</style>
</head>
<body>
<header class="book-title-source">${escapeHtml(title)}</header>
${bodyHtml}
</body>
</html>`;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
