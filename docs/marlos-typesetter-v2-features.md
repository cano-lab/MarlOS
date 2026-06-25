# Marlos Typesetter — V2 Features

*A working spec for two additions to the typesetter: a "traditional" chapter-opener preset (large drop cap + small-caps lead-in), and figure/image support with per-figure layout control. Builds on the V1 architecture defined in `marlos-typesetter-handoff.md`. Same audience: Jer (project owner) and a coding agent.*

*The forcing function for V1 was* Nothing Is the Impossibility *— a physics book where the current modernist chapter opener (small-caps centered title, no drop cap in preview) reads correctly. These additions exist so the same typesetter can produce a more conventionally-styled novel or essay collection without forking the CSS template.*

---

## Context

The V1 typesetter handles one chapter-opener style and renders pandoc-emitted `<img>` tags with no special CSS. Today:

- Chapter opener (in `src/typesetter/book-css.ts`): forced recto, centered title at 2em, small-caps, letter-spaced, ~4em top margin. Drop cap is defined in `src-tauri/src/typesetter/pdf_export.rs::build_export_css` (`section[data-section-type="chapter"] > p:first-of-type::first-letter`) and **only fires in the PDF export** — Paged.js v0.4 crashes on `::first-letter` mid-split, so the preview omits it.
- Images: pandoc converts `![alt](path)` to `<img>`. Nothing in the CSS handles sizing, captions, page-break behavior, or float positioning. A wide image will overflow the body column; a tall image will silently get cut off at a page break.

The V1 handoff doc explicitly anticipated both extensions:

> *Drop-cap customization. V1 ships with one drop-cap style. Extension point: drop cap is implemented as a CSS class on the chapter's first paragraph, parameterized by CSS variables...*

> *Multiple body fonts or per-chapter font customization. V1 ships with EB Garamond. Extension point: font is referenced in CSS via a single CSS custom property...*

The chapter-opener and figure work follow the same pattern: **one new config field per feature, preset values resolve to CSS class/variable combinations, the pipeline does not change.**

---

## Feature 1 — Traditional chapter opener

### Goal

Add a `chapter_opener_style` field to `BookConfig.typography`. V2 ships two presets:

- **`modern`** (current behavior, renamed) — centered small-caps title, no drop cap in preview, simple drop cap in PDF.
- **`traditional`** — larger drop cap (4-line raised cap), first ~5 words of the chapter in small caps as a lead-in, slightly different vertical spacing.

The book selects one preset for the whole manuscript via `book.toml`. Per-chapter overrides are V3.

### Visual spec — `traditional`

| Element | Behavior |
|---|---|
| Forced recto start | Same as `modern` (`break-before: right`). |
| Top whitespace | ~6em (slightly more than modern's 4em — pushes the title down toward the classic "title at 1/3 page" position without overcommitting). |
| Chapter title | Centered, 1.6em (smaller than modern's 2em — the drop cap below is now the visual anchor). Small-caps preserved. |
| Drop cap | First letter of first paragraph: 4-line raised cap (vs. modern's 3.2em float-left). Same body font; weight 600. |
| Small-caps lead-in | First ~5 words after the drop cap rendered in `font-variant: small-caps; letter-spacing: 0.05em;`. |
| First paragraph indent | Suppressed (`text-indent: 0`) — same as modern. |
| Ornamental rule | None in V2. Deferred to a third preset (`ornamental`) in V3 if wanted. |

### Why a 5-word lead-in, not first-line

CSS Paged Media can target `::first-line`, but `::first-line` reflows when the paragraph re-wraps at different page widths — what's 5 words on a 6×9 trim is 7 words on a 5.5×8.5 trim. A fixed word-count lead-in is more typographically stable across trim sizes. Implementation: the structure parser wraps the first N words of the first paragraph in `<span class="lead-in">` so CSS can target it directly. N defaults to 5; configurable via `BookConfig.typography.lead_in_word_count` (V2 ships with the default only; the field exists for the seam).

### Drop-cap implementation note

V1's drop cap is `::first-letter` on the first `<p>`. That's fragile (mid-page-split Paged.js crash; doesn't handle opening-quote-then-letter cases like `"Hello..."` where the cap lands on the quote mark).

V2 introduces a structure-parser pass: when building enriched HTML, detect the first paragraph of each chapter and wrap its first letter (skipping leading punctuation/quotes) in `<span class="drop-cap">`. The CSS targets `.drop-cap`, not `::first-letter`. This:

- Fixes the quote-mark bug.
- Renders identically in Paged.js preview and PDF export.
- Lets `traditional` and `modern` share the wrap step, differing only in CSS values.

Lead-in span is the same idea: structure parser wraps the first N words after the drop-cap span in `<span class="lead-in">`.

### Config schema

In `BookConfig.typography`:

```ts
{
  body_font: string;
  body_size_pt: number;
  body_leading_pt: number;
  heading_space_em: number;
  running_header_style: string;
  // V2 additions
  chapter_opener_style: "modern" | "traditional";  // default: "modern" (back-compat)
  lead_in_word_count: number;                       // default: 5 (only used when style = "traditional")
}
```

Both fields default-on-load to preserve existing books' rendering when `book.toml` lacks them.

### CSS surface

`src/typesetter/book-css.ts::buildBookCss` and `src-tauri/src/typesetter/pdf_export.rs::build_export_css` both gain a block that switches on `chapter_opener_style`. Drop the existing `section[data-section-type="chapter"] > p:first-of-type::first-letter` rule from `pdf_export.rs` — `.drop-cap` replaces it.

### UI

`src/components/BookConfigEditor.tsx` adds a `<select>` for chapter opener (two options) under the Typography section, next to `running_header_style`. The lead-in word count is **not** exposed in V2 UI — it lives in `book.toml` only. Adds the UI later when a second user wants a different value.

### Extension point for V3

The preset enum is the seam. Adding `ornamental` later means: new enum value, new CSS block, new structure-parser variant (the ornament glyph or rule). No new pipeline.

---

## Feature 2 — Figures and images

### Goal

Markdown `![alt](path)` becomes a properly-typeset figure with optional caption, per-figure size, and per-figure page-layout behavior. Three layout modes ship in V2:

- **Inline** (default) — figure flows at its position in the text; max-width caps to body column; allows a page break before the figure if it doesn't fit on the current page.
- **Float top/bottom** — figure floats to the top or bottom of its current page (or the next page if it doesn't fit) via CSS `float: top` / `float: bottom`. Body text fills around it.
- **Full page** — figure gets its own page; no body text on that page. Surrounding body text breaks before and after.

### Markdown syntax

Pandoc's attribute syntax (already in the pipeline; needs `attributes` extension enabled in the pandoc invocation):

```markdown
![A diagram of the standard model](images/standard-model.png){width=4in .float-top}

![](images/full-bleed-diagram.svg){.full-page}

![Energy levels in hydrogen](images/hydrogen.png)
```

Class attributes that V2 recognizes:

| Class | Effect |
|---|---|
| `.inline` (or no class) | Default behavior. |
| `.float-top` | Float to top of page. |
| `.float-bottom` | Float to bottom of page. |
| `.full-page` | Take a full page. |

Width attribute (`width=4in`, `width=80%`, `width=300px`) sets the figure width. Default is `max-width: 100%` of body column. Heights are computed; manual `height=` is supported but discouraged for print.

Alt text becomes the caption when non-empty. An empty alt (`![]`) produces a figure with no caption — useful for decorative or full-bleed images. To suppress the caption while keeping alt text (for EPUB accessibility), wrap with `{aria-label="..." .no-caption}`.

### Figure auto-numbering

Each figure gets a number scoped to its chapter: "Figure 3.1", "Figure 3.2", "Figure 4.1", etc. The caption renders as:

> **Figure 3.2.** Energy levels in hydrogen.

Numbering implementation: the structure parser walks chapters, counts figures per chapter, writes `data-figure-number="3.2"` on the `<figure>` element. CSS pseudo-element renders `attr(data-figure-number)` as the bold prefix. No JavaScript at render time.

Front-matter figures are unnumbered (a Note on Method with a diagram, for example, just says the caption text). Back-matter figures continue chapter numbering's last value with a letter suffix ("Figure A.1" for appendix) — V2 hardcodes the appendix prefix as "A"; per-back-matter-section prefixes deferred.

### Image source paths

Pandoc resolves image paths relative to the markdown file. The structure parser, after pandoc runs, rewrites `src` attributes to absolute paths (or `file://` URLs) so the headless Chromium subprocess can load them without a server. EPUB export hands raw paths to pandoc, which copies images into the EPUB package.

A book directory now typically looks like:

```
my-book/
  book.toml
  ch01-intro.md
  ch02-method.md
  images/
    fig-3-2-hydrogen.png
    fig-3-3-standard-model.svg
  dist/
    my-book.pdf
    my-book.epub
```

Images directory name is convention, not enforcement. The structure parser walks whatever paths the markdown uses.

### Page-break behavior

CSS Paged Media rules per layout mode:

```css
figure.inline {
  break-inside: avoid;       /* never split a figure across pages */
  margin: 1em auto;
  display: block;
  text-align: center;
  max-width: 100%;
}

figure.inline img {
  max-width: 100%;
  height: auto;
}

figure.float-top  { float: top;    }
figure.float-bottom { float: bottom; }

figure.full-page {
  page: full-figure;
  break-before: page;
  break-after: page;
}

@page full-figure {
  @top-center { content: none; }       /* no running header on figure-only pages */
  @bottom-center { content: counter(page); }  /* keep page number */
}

figure figcaption::before {
  content: "Figure " attr(data-figure-number) ". ";
  font-weight: 600;
}

figcaption {
  font-size: 0.9em;
  font-style: italic;
  margin-top: 0.4em;
  text-align: center;
  text-wrap: balance;
}
```

`float: top` and `float: bottom` are CSS Paged Media features. Chromium's native print engine supports them; Paged.js v0.4's chunker does not. **Implication for the preview pane:** floated figures render inline in preview but at the correct top/bottom in the exported PDF. The preview's page count for floated-figure-heavy chapters will be off by ~1 page per float. Document this in `book-css.ts`'s comment block alongside the existing list of preview-vs-PDF divergences.

### SVG handling

SVGs work as `<img src="...svg">` in Chromium and EPUB readers. No special handling in V2. The known issue: an SVG with embedded `<style>` whose selectors clash with the book CSS can produce unexpected styling. Mitigation deferred — flag in handoff that SVGs should be self-contained or stripped of `<style>` before adding.

### Mermaid integration

`marked-mermaid` already converts inline mermaid code blocks to SVG in the live editor preview. For the typesetter pipeline, mermaid blocks are processed during the pandoc → structured HTML step: a post-pandoc pass detects ````mermaid` code blocks (preserved by pandoc as `<pre><code class="language-mermaid">`), runs them through `mermaid.render`, and replaces with `<figure class="inline"><svg>...</svg><figcaption>...</figcaption></figure>`. Caption comes from a trailing comment line `<!-- caption: ... -->` in the mermaid block. No caption = no figcaption.

This is mostly already-built infrastructure (the frontend has mermaid as a dep) — the new work is the Rust side either calling out to a Node helper or, more cleanly, deferring mermaid rendering to a frontend pre-process step that runs before the headless Chromium PDF capture.

### Config schema

No new `BookConfig` field. Figures are markdown-driven, not config-driven. The closest thing to a global config is the caption prefix word ("Figure"), which V2 hardcodes — adding a translation field (`BookConfig.typography.figure_caption_prefix`) is a one-liner deferred until someone produces a French or Spanish book.

---

## Out of V2 scope (deferred)

- **Per-chapter chapter-opener override.** All chapters share one preset. Extension point: the structure parser already emits `data-section-number` — a `data-opener-style` attribute can join it.
- **Ornamental chapter-opener preset** (fleurons, rules, third-style typography). Add as third enum value.
- **List of Figures front matter page.** Auto-generated from the parsed figures. Same pattern as TOC: a post-render pass walks `<figure data-figure-number>` and emits a list. Add when a book needs it.
- **Side captions** (caption to the right of the figure, common in scholarly publishing). Floats are tricky; deferred.
- **Image color management for print** (CMYK conversion, ICC profile embedding). KDP accepts sRGB PDFs for B&W interiors, so V2 sidesteps this. Color interiors (already a deferred V2 in the V1 handoff) will need this.
- **Figure references in text** (`See Figure 3.2`). Pandoc's `pandoc-crossref` filter handles this. Adding it means a flag on the pandoc invocation + the filter binary as a dependency. Worth doing when the first book needs it.
- **Per-figure markdown captions distinct from alt text.** Today alt = caption. Markdown syntax extension for separate alt and caption deferred.

---

## Implementation phasing

### Phase H — Drop-cap and lead-in spans in structure parser

The structure parser wraps the first letter of every chapter's first paragraph in `<span class="drop-cap">` (skipping leading punctuation), and the next N words in `<span class="lead-in">`. CSS for both is in `book-css.ts` and `pdf_export.rs`. The existing `::first-letter` rule comes out. Output: `modern` chapter openers continue to render correctly with the new spans (no visual change to the physics book).

### Phase I — Traditional chapter-opener preset

Add `chapter_opener_style` config field. Add the `traditional` CSS block. Add UI selector in `BookConfigEditor.tsx`. Output: a second preset is usable.

### Phase J — Figure CSS and `.inline` default

Add `<figure>`/`<figcaption>` CSS for inline figures with page-break-avoid, max-width clamp, caption styling. No layout-mode classes yet. Output: markdown images render as typeset figures (centered, captioned, never split).

### Phase K — Figure layout modes

Add `.float-top`, `.float-bottom`, `.full-page` CSS. Document the preview-vs-PDF divergence. Output: per-figure layout control works in PDF export.

### Phase L — Figure auto-numbering

Structure parser counts figures per chapter, writes `data-figure-number`. CSS renders the prefix. Output: numbered captions match the bound-book convention.

### Phase M — Mermaid pipeline integration

Pre-render mermaid blocks to SVG before headless Chromium capture. Caption extraction from trailing `<!-- caption: -->`. Output: mermaid blocks in manuscripts render as figures in the PDF.

H–J are the minimum useful set — they let a non-physics manuscript with a few images export to a real-looking PDF. K–M are quality-of-life adds.

---

## Validation criteria for V2

V2 is complete when:

- A book with `chapter_opener_style = "traditional"` exports to PDF with a 4-line drop cap and small-caps lead-in on every chapter, with no `::first-letter` quote-mark bug.
- A book with `chapter_opener_style = "modern"` (or no field set) renders **identically** to V1 output. Pixel-diff the existing physics-book PDF against a V2-rebuilt copy and confirm only the drop-cap implementation changed (and only if the cap behavior was buggy on a quoted opening).
- `![caption](path.png)` renders as `<figure><img><figcaption>Figure N.M. caption</figcaption></figure>` in the exported PDF and EPUB.
- `{width=4in}` clamps figure width to 4 inches.
- `{.float-top}`, `{.float-bottom}`, `{.full-page}` produce the expected page layout in the PDF export.
- A mermaid code block with `<!-- caption: ... -->` renders as a numbered figure in the PDF.
- All V1 validation criteria still pass (physics book exports cleanly, KDP previewer accepts the PDF and EPUB).

---

## A note for the coding agent

The V1 handoff said *"the book is the forcing function, the book is the test."* That's still true. V2 needs a second forcing function — pick a real manuscript with figures and traditional styling and use it as the integration test. If the physics book stops rendering correctly during V2 work, stop and fix it before adding more.

Implementation order: Phase H first, even though it produces no visible change. The drop-cap and lead-in spans are infrastructure both presets depend on. Skipping H and adding the `traditional` preset directly leaves the codebase with two `::first-letter` styles and the quote-mark bug intact.

The CSS lives in two places (`book-css.ts` for preview, `pdf_export.rs::build_export_css` for export) and they drift. V2 work should keep them in sync, and a future cleanup should consider extracting the shared subset to a single source — but that's not V2 work.

---

## TODO — Cover-art credit on the Typst path

The `book.cover_art` field (freeform front-cover credit — artwork title, artist,
medium) was added in the legacy HTML/CSS path: it renders anchored top-left on the
generated copyright page (`structure.rs::build_generated_front_matter` +
`.gen-cover-art` in `book-css.ts`, plus the editor field in `BookConfigEditor.tsx`).

The Typst backend (`typst_emit/`) does **not** generate title/copyright/dedication
pages yet — only page setup, headers, chapter openers, and the TOC/LoF outlines. When
those generated front-matter pages are ported to Typst, wire `cover_art` into the
copyright page there too (the field already exists on `BookMeta`, so no schema change
is needed). Keep the placement consistent: anchored top-left, small italic.
