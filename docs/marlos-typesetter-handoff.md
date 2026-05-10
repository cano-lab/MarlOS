# Marlos Typesetter — Handoff

*A working document for implementing book typesetting capability in Marlos. Audience: Jer (project owner) and a coding agent assisting with implementation. The document is prescriptive about architecture and feature scope, but does not reference specific modules or files in the Marlos codebase — those decisions belong to the implementer.*

*The forcing function is the publication of one specific book: **Nothing Is the Impossibility — Notes from the edge of physics** by Jérémie Roy. The typesetter must be sufficient to produce a KDP-publishable paperback PDF and EPUB of that book. Anything beyond that is future work.*

---

## Context

Marlos is an existing Tauri-based markdown editor written in Rust, with an HTML-rendering preview pipeline. The current "PDF output" is produced by the user invoking the OS print dialog from the Tauri webview, which sends the rendered HTML to a system print driver. This is not typesetting. It is browser-default rendering captured to PDF.

The typesetter project converts Marlos from a markdown editor that can be printed into a tool that can produce print-ready book interiors and ebooks.

The current target is one book, ~38,000 words, with 13 chapters, 4 interludes, front matter (Author's Note, Note on Method, Glossary), an appendix, and an eventual References section once the citation pass is complete.

---

## Architecture

The typesetter is **Option 1** in the design space: HTML + CSS Paged Media, rendered to PDF via a headless browser engine, with pandoc as a subprocess for the harder markdown-handling tasks.

The pipeline is:

Markdown → pandoc (subprocess) → structured HTML → Marlos applies book-aware CSS template → headless browser render → PDF.

Pandoc is used as a **library/subprocess only** — it is not modified, not forked, not extended. Marlos calls pandoc with explicit flags and consumes its output. This isolates the language boundary: pandoc is Haskell, Marlos remains Rust, the interface is markdown in / HTML in / file out.

The headless browser render is performed via Marlos's existing webview engine (programmatically driven, not via the user-facing print dialog) or via a separate headless engine if Marlos's webview cannot be driven for print without user interaction. Implementation detail for the agent to determine.

The CSS Paged Media spec ([https://www.w3.org/TR/css-page-3/](https://www.w3.org/TR/css-page-3/)) is the primary mechanism for book layout. It is supported by modern browser engines (Chromium, WebKit) when invoked via the print pipeline. Marlos's existing webview almost certainly supports it.

---

## V1 Feature Scope

Each item below is one discrete unit of work. A coding agent should treat them as independent tasks where possible.

### Document model

- A "book" in Marlos is a directory containing one or more markdown files plus a `book.toml` (or similar) configuration file declaring the book's metadata (title, author, subtitle, ISBN, trim size, etc.) and the order of files.

### Markdown structure recognition

- The typesetter parses each markdown file into book-level structures: front matter sections (`# Author's Note`, `# Note on Method`, etc.), chapters (`# Chapter N: Title`), interludes (`# Interlude N — Title`), back matter (`# Appendix:`, `# Glossary`, `# References`).

### Pandoc integration

- Marlos invokes pandoc as a subprocess with structured input and captures HTML output. The pandoc invocation includes flags for: footnote handling, smart typography, header IDs, table of contents JSON.

### CSS Paged Media template — body pages

- A CSS template defines page size (6×9 inches), asymmetric margins (inside 1.0", outside 0.875", top 0.75", bottom 0.75"), recto/verso awareness, body font (default: EB Garamond 11pt with 14pt leading), justified paragraphs, hyphenation enabled.

### CSS Paged Media template — running headers

- Left-page header: book title in small caps. Right-page header: chapter title in small caps. Headers suppressed on chapter-opener pages and front-matter pages.

### CSS Paged Media template — page numbers

- Page numbers in the bottom outside corner. Roman numerals for front matter. Arabic for chapters and back matter. Numbering restarts at 1 when chapter 1 begins.

### Chapter openers

- Each chapter starts on a recto (right) page. Chapter opener has extra blank space above the title, larger title type, and the first paragraph styled with a drop cap or small-caps first line. Previous page is a blank verso if needed to force the recto start.

### Interlude styling

- Interludes start on any page (no forced recto). Interlude title styled differently from chapter title (italic, smaller, less vertical space). Body of interlude uses the same body font as chapters.

### Front matter pagination

- All front matter (Author's Note, Note on Method, Glossary, table of contents) uses lowercase roman numerals.

### Table of contents generation

- The typesetter generates a TOC from the parsed structure, with page numbers resolved after layout. The TOC is itself a front-matter page.

### Widow and orphan control

- CSS rules to prevent single lines of a paragraph at the top or bottom of a page (`widows: 2; orphans: 2;`). Test on the actual manuscript and tune.

### Headless render to PDF

- Marlos drives the webview programmatically to render the styled HTML and export a print-ready PDF, bypassing the OS print dialog. The output PDF embeds fonts and uses the correct page size.

### EPUB export

- Separate pipeline: Marlos invokes pandoc with EPUB output flags, providing the cover image and metadata. EPUB does not use Paged Media (ebooks reflow). The EPUB and the PDF are produced from the same source markdown but use different rendering paths.

### KDP compliance check

- A pre-export validator confirms that the PDF meets KDP's spec: PDF version (1.4 or later acceptable), all fonts embedded, no transparency, correct trim size, bleed handled if cover-format requires it. For interior-only black-and-white text PDFs, bleed is not required.

### Book-level configuration UI

- A simple panel in Marlos exposes the book metadata fields (title, author, etc.) and trim-size selection. The configuration writes to the book's `book.toml`.

### Export action

- A single "Export Book" action triggers the full pipeline (pandoc → CSS render → headless PDF + EPUB), produces the two files in the book directory's `dist/` subfolder, and surfaces any errors to the user.

---

## Out of V1 Scope (deferred, but architected for)

The following are real concerns excluded from V1 to keep the scope shippable. Each item is paired with a note about what V1 must do *now* to make adding it later cheap rather than expensive. The principle: V1 ships with one of each thing, but the seams for the second one are already in place.

- **Multiple trim sizes beyond 6×9 inches.** V1 ships with 6×9 only. *Extension point:* trim size is read from `book.toml` and passed to the CSS template as a variable; do not hard-code page dimensions in the CSS file. Adding 5.5×8.5 later means changing one config value, not refactoring the renderer.

- **Multiple body fonts or per-chapter font customization.** V1 ships with EB Garamond. *Extension point:* font is referenced in CSS via a single CSS custom property (`--body-font`) declared at the top of the template; the `@font-face` block is parameterized by the book config. Adding a font option later means a new entry in the bundled fonts directory and a config field.

- **Configurable running headers.** V1 ships with the default left/right pattern (book title on verso, chapter title on recto). *Extension point:* the header content is generated by a single function that reads the current section's metadata; do not hard-code `string-set` values in the CSS, set them from per-section data attributes emitted by the structure parser.

- **Footnotes at page bottom (rather than chapter-end endnotes).** V1 uses endnotes only, by chapter, in a back-matter section. *Extension point:* pandoc is already invoked with footnote support; the choice between bottom-of-page and end-of-chapter is a CSS Paged Media rule (`float: footnote` vs. moving the footnote container to chapter end). Adding bottom-of-page later is a CSS template change, not a pipeline change.

- **Drop-cap customization.** V1 ships with one drop-cap style. *Extension point:* drop cap is implemented as a CSS class on the chapter's first paragraph, parameterized by CSS variables (cap height, font, color). Adding alternatives later means adding class variants, not rewriting the chapter-opener logic.

- **Color interior PDFs.** V1 ships black-and-white. *Extension point:* the export pipeline accepts a color-mode flag; the headless renderer is invoked with the corresponding color profile. The flag exists in V1, even if only one value is supported.

- **Spine-and-cover wraparound generation.** V1 builds the cover externally (GIMP/Inkscape). *Extension point:* the export pipeline produces a `manifest.json` alongside the PDF and EPUB that includes page count, trim size, and bleed dimensions. A future cover-builder feature consumes this manifest. The manifest exists in V1.

- **Index generation.** V1 has no index. *Extension point:* pandoc is invoked with `--id-prefix` flag so every heading has a stable ID. A future index-builder can consume the resulting HTML and produce an alphabetized index pointing to those IDs.

- **Bibliography management beyond a flat References list.** V1 has flat references. *Extension point:* references are stored in a structured format (CSL JSON or BibTeX) in the book directory; pandoc's `--citeproc` flag is in the invocation pipeline (even if the V1 reference list is hand-formatted). Adding citation manager integration later means turning on a flag, not building a new pipeline.

- **Live print preview pane.** V1 model is "edit, then export." *Extension point:* the export pipeline is callable from any code path, not only from a user-clicked button. A future live preview can call the same pipeline on a debounced timer.

- **Multi-book project management.** V1 treats each book as its own directory. *Extension point:* the book is identified by its directory path, not by being "the open book." Marlos can already have multiple book directories; the configuration UI just operates on one at a time. Adding a project switcher later is UI work, not data-model work.

The shared principle across all of these: **V1 always passes through the data that V2 will need.** Trim size goes through config even though there's only one value. Manifest gets written even though no consumer reads it yet. Pandoc flags for footnotes and citations are in the invocation even though V1 doesn't fully exercise them.

This costs almost nothing in V1 implementation effort. It costs a great deal not to do it, because it forces re-architecture later when the seams aren't where they need to be.

---

## Implementation Phasing

The V1 scope above can be split into shippable increments, each of which produces a useful artifact even if later phases are not built.

### Phase A — Pandoc subprocess + structured HTML

Marlos can invoke pandoc on a single markdown file and display the resulting HTML in its preview pane. No new CSS yet. Goal: prove the pandoc bridge works, eliminate dependency on Marlos's existing markdown parser for book content.

### Phase B — Book-aware structure recognition

Marlos can read a `book.toml` file, identify the markdown files in a book, parse them into front matter / chapters / interludes / back matter, and emit a single structured HTML document with section markers. Goal: produce the data model the CSS template will consume.

### Phase C — CSS Paged Media body styling

The first CSS template applies: page size, asymmetric margins, body font, justification, hyphenation. Output: a printable PDF that looks like a book interior, even without headers, page numbers, or chapter openers. This is the inflection point — after Phase C, Marlos's output stops being browser-default and starts being typeset.

### Phase D — Headers, page numbers, chapter openers

The CSS template gains running headers, page numbers (with roman/arabic switching), and chapter-opener styling. Output: a PDF that meets the basic visual standard of a published book.

### Phase E — TOC, widow/orphan, KDP validation

Generated TOC, widow/orphan control, KDP compliance checks. Output: a PDF ready for KDP upload.

### Phase F — EPUB export

Pandoc-driven EPUB generation with cover image and metadata. Output: an EPUB ready for KDP Kindle upload.

### Phase G — Configuration UI and export action

User-facing controls in Marlos for the book metadata and a single export button. Output: a typesetter that is usable by someone other than the implementer.

The book *Nothing Is the Impossibility* can be published using the output of phase F. Phase G is necessary for the typesetter to be usable as a Marlos feature beyond this one book.

---

## Technical Notes

### Pandoc invocation

A representative command for converting one markdown file to HTML suitable for the typesetter:

```
pandoc input.md \
  --from markdown+smart+footnotes+pipe_tables \
  --to html5 \
  --standalone=false \
  --section-divs \
  --id-prefix=ch \
  --metadata-file=book.toml \
  --output=fragment.html
```

The `--section-divs` flag wraps each heading-bounded section in a `<section>` element with the heading's ID as the element ID — this is what enables the typesetter to apply per-section CSS.

### CSS Paged Media spec essentials

The four most important pieces:

`@page` rule for page size and margins.
`@page :left` and `@page :right` for recto/verso asymmetry.
`@page chapter-opener` (named pages) for chapter-start styling, applied via `page: chapter-opener` on the chapter element.
`counter-reset` and `counter-increment` for page numbering, with `content: counter(page)` in `@page` margin areas.

For testing CSS Paged Media output, [Paged.js](https://pagedjs.org/) is a polyfill that runs in any browser and previews paged output without needing print-driver invocation. Useful during development even if the final render goes through the native browser print pipeline.

### Headless browser rendering

In a Tauri app, the webview is the renderer. To produce a PDF without the user invoking the print dialog: drive the webview's print API programmatically. Tauri exposes `WebviewWindow::print()` for this purpose, but the configurable variant (specifying page size, margins, output path) requires either the underlying webview's print-to-PDF API directly (Chromium DevTools Protocol's `Page.printToPDF` for tauri's webkit on most platforms; equivalent on macOS WKWebView) or a separate headless Chromium subprocess.

For V1, a separate headless Chromium subprocess (via `headless_chrome` Rust crate or similar) is the most predictable path and isolates the rendering from the editor UI. The trade-off is an extra binary dependency. Worth it for V1 reliability.

### Font handling

Body font (EB Garamond) and any heading font must be embedded in the PDF. CSS Paged Media handles this via `@font-face` declarations pointing to local font files. Marlos should bundle the fonts it uses as part of its installation, not rely on the user having them. Fonts must be licensed for embedding — EB Garamond is SIL Open Font License, fully embeddable.

### EPUB notes

Pandoc's EPUB output is good. The notable pandoc EPUB flags:

```
pandoc input.md \
  --to epub3 \
  --epub-cover-image=cover.png \
  --metadata-file=book.toml \
  --epub-title-page=true \
  --css=epub.css \
  --output=book.epub
```

EPUB CSS is a much smaller file than the print CSS — ebooks reflow, so almost no layout instructions are needed. Just typography (font sizes, line height, paragraph spacing) and structural styling (chapter headings, blockquotes, etc.).

---

## Validation Criteria for V1

The typesetter is V1-complete when all of the following are true for *Nothing Is the Impossibility*:

The exported PDF is 6×9 inches with asymmetric margins.
Every chapter starts on a recto page with a styled chapter opener.
Running headers appear on body pages and are absent on chapter-opener pages.
Page numbers appear in the bottom outside corner with roman numerals in front matter and arabic in body.
Body text is justified with hyphenation and no obvious widow/orphan violations.
The exported EPUB opens correctly in Kindle Previewer and Apple Books with the cover image and TOC.
KDP's online previewer accepts the PDF and EPUB without errors.

The book does not need to look identical to a Vellum-typeset book to count as V1-complete. It needs to look like a real book, meet KDP's spec, and not embarrass its author.

---

## What Comes After V1

The V2 conversation can begin once *Nothing Is the Impossibility* is published. At that point the typesetter has a real-world test, and feature decisions can be driven by what was painful or missing rather than what seemed plausible in advance.

Likely candidates for V2, listed but not committed:

Footnotes at page bottom (rather than chapter-end endnotes).
Bibliography integration with citation manager (Zotero, BibTeX).
Index generation.
Multiple trim-size presets.
Cover composition (front, spine, back wraparound) as a Marlos feature.
Live preview pane.
Multi-book project management.

These are all real and all worth eventually building. They are not what V1 needs.

---

## A Note for the Coding Agent

This document scopes the *what*. The *how* and the *where in the codebase* are open questions for the implementer. The implementer should expect to:

Inspect Marlos's existing markdown-to-HTML pipeline to understand where pandoc subprocess output should integrate.
Decide whether the typesetter is a new Marlos module, a plugin, or an extension of the existing rendering pipeline.
Pick a Rust crate for headless Chromium control (or justify the choice of an alternative path).
Write the CSS template iteratively against the actual *Nothing Is the Impossibility* manuscript, using its specific structures (chapters, interludes, appendix, glossary) as test cases.
Treat the manuscript as the integration test. If it produces a publishable PDF and EPUB, the typesetter is V1-done.

The book is the forcing function. The book is the test. The book is the success criterion. The typesetter exists to ship the book — and, from there, possibly to ship more books, but not yet.

---

*Last updated: when this document was generated. Manuscript at v7. Citation verification pending. Cover at front-cover-near-final, back-cover-not-started. Production timeline: 2–3 months from manuscript-final to published.*
