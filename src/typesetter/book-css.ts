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
  return TRIM_PRESETS[size] ?? TRIM_PRESETS["6x9"];
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

  // Suppress unused-variable warnings while bisecting.
  void mIn; void bodyFont; void fontSize; void lineHeight;

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
    content: counter(page);
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
  string-set: chapter-name content();
  font-size: 2em;
  text-align: center;
  margin: 4em 0 2em;
  font-variant: small-caps;
  letter-spacing: 0.03em;
  font-weight: 600;
  text-wrap: balance;
  line-height: 1.2;
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
