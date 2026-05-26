/**
 * Book typesetter service — Phase A pandoc bridge.
 *
 * Wraps the Rust-side commands that talk to the pandoc subprocess. Phase A
 * surface is intentionally tiny: probe + convert. Later phases (book.toml
 * structure parsing, CSS Paged Media render, EPUB export) extend this
 * service rather than replace it.
 */

import { invoke } from "@tauri-apps/api/core";

export interface PandocProbe {
  available: boolean;
  path: string | null;
  version: string | null;
  error: string | null;
}

export interface PandocConvertOptions {
  from_format?: string;
  to_format?: string;
  id_prefix?: string;
  standalone?: boolean;
  metadata_file?: string;
}

export interface PandocConvertResult {
  html: string;
  stderr_warnings: string;
  pandoc_path: string;
  pandoc_version: string;
}

// Phase B types --------------------------------------------------------------

export type SectionKind = "front-matter" | "chapter" | "interlude" | "back-matter";

export interface BookSection {
  kind: SectionKind;
  order: number;
  number: number | null;
  roman_number: string | null;
  title: string;
  h1_raw: string;
  html_id: string;
}

export interface BookStructure {
  sections: BookSection[];
  chapter_count: number;
  interlude_count: number;
  front_matter_count: number;
  back_matter_count: number;
}

/** Anchor describing a content position inside a scroll container,
 *  keyed by `(section.order, paraIndex)` and measured in scrollTop
 *  pixels. paraIndex 0 = the section's heading, 1+ = subsequent
 *  paragraphs in DOM order on the pages side, blank-line-separated
 *  block order on the source side. The pair is the cross-pane key:
 *  Source's "section 5, paragraph 3" lines up with Pages's
 *  "section 5, paragraph 3" regardless of how dense the rendered
 *  pages are. Drift between counts is bounded to one section. */
export interface SectionAnchor {
  order: number;
  paraIndex: number;
  top: number;
}

/** Handle the children expose so BookMode can mirror scrolling between
 *  Source and Pages in split mode. `getAnchors` is called per-event so
 *  it always sees the current DOM / textarea state. */
export interface ScrollSurface {
  el: HTMLElement;
  getAnchors: () => SectionAnchor[];
}

export interface BookMeta {
  title: string;
  subtitle: string;
  author: string;
  isbn: string;
  cover_image: string | null;
  back_cover_image: string | null;
  language: string | null;
  /** Publisher name on the copyright page. Empty = omit. */
  publisher: string;
  /** Year on the © line. Empty = current year. */
  copyright_year: string;
  /** Name on the © line. Empty = falls back to `author`. */
  copyright_holder: string;
  /** Dedication text, prints on its own page. Empty = no dedication. */
  dedication: string;
  /** Acknowledgements text. Multi-paragraph (split on blank lines).
   *  Renders as centered italic body under an "Acknowledgements"
   *  heading at the end of the book. Empty = omit. */
  acknowledgements: string;
}

export interface BookConfig {
  book: BookMeta;
  trim: {
    size: string;
    margins_in: { inside: number; outside: number; top: number; bottom: number };
  };
  typography: {
    body_font: string;
    body_size_pt: number;
    body_leading_pt: number;
    heading_space_em: number;
    /** Format of the running header at the top of each in-chapter page.
     *  One of: "title" | "chapter-number" | "chapter-number-title" |
     *  "compact-arabic" | "compact-roman". */
    running_header_style: string;
    /** Word count for the small-caps lead-in span after the drop cap
     *  (used by the `traditional` chapter-opener preset, Phase I).
     *  Default 5. The structure parser wraps regardless of preset. */
    lead_in_word_count: number;
    /** Chapter opener visual preset. "modern" = V1 default (centered
     *  small-caps title, no special lead-in). "traditional" = larger
     *  drop cap, smaller title, more top whitespace, small-caps
     *  lead-in. */
    chapter_opener_style: string;
    /** Body page-number style. One of:
     *    "arabic" (default) — 1, 2, 3
     *    "roman"            — I, II, III
     *    "lower-roman"      — i, ii, iii
     *    "none"             — page number hidden
     *  Front matter always uses lower-roman per print convention. */
    page_number_style: string;
    /** Front-matter page-number style: "lower-roman" | "upper-roman" |
     *  "arabic" (arabic = one continuous sequence through the book). */
    front_matter_page_number_style?: string;
  };
  export: { color_mode: string; include_list_of_figures?: boolean };
  files: string[];
  root_dir?: string;
  config_path?: string;
}

export interface LoadedBook {
  config: BookConfig;
  structure: BookStructure;
  enriched_html: string;
  stderr_warnings: string;
  pandoc_version: string;
  /** Absolute path to the front cover (if set + file exists). Frontend
   *  converts via convertFileSrc for use in <img src>. */
  front_cover_path: string | null;
  back_cover_path: string | null;
}

export const typesetterService = {
  probe: (): Promise<PandocProbe> => invoke<PandocProbe>("typesetter_pandoc_probe"),

  convertFile: (
    path: string,
    options?: PandocConvertOptions,
  ): Promise<PandocConvertResult> =>
    invoke<PandocConvertResult>("typesetter_pandoc_convert_file", {
      path,
      options: options ?? null,
    }),

  convertString: (
    markdown: string,
    options?: PandocConvertOptions,
  ): Promise<PandocConvertResult> =>
    invoke<PandocConvertResult>("typesetter_pandoc_convert_str", {
      markdown,
      options: options ?? null,
    }),

  // Phase B
  loadBook: (bookPath: string): Promise<LoadedBook> =>
    invoke<LoadedBook>("typesetter_book_load", { bookPath }),

  initBook: (markdownPath: string): Promise<string> =>
    invoke<string>("typesetter_book_init", { markdownPath }),

  saveBook: (bookPath: string, config: BookConfig): Promise<string> =>
    invoke<string>("typesetter_book_save", { bookPath, config }),

  /**
   * Phase E — render the loaded book to a PDF via headless Chromium.
   * Returns the output path on success. The Rust side spawns Chromium,
   * navigates to a temp HTML file with full Phase D CSS, and uses the
   * native print engine to produce the PDF — bypassing the OS print
   * dialog and bypassing Paged.js.
   */
  exportPdf: (bookPath: string, outputPath: string): Promise<string> =>
    invoke<string>("typesetter_export_pdf", { bookPath, outputPath }),

  /** Phase F — render the book to an EPUB3 via pandoc. */
  exportEpub: (bookPath: string, outputPath: string): Promise<string> =>
    invoke<string>("typesetter_export_epub", { bookPath, outputPath }),

  /** Source-view: read manuscript file `i` from book.toml's files list. */
  readBookFile: (bookPath: string, fileIndex: number): Promise<string> =>
    invoke<string>("typesetter_read_book_file", { bookPath, fileIndex }),

  /** Source-view: write manuscript file `i` from book.toml's files list. */
  writeBookFile: (bookPath: string, fileIndex: number, content: string): Promise<void> =>
    invoke<void>("typesetter_write_book_file", { bookPath, fileIndex, content }),

  /** Read the book's custom stylesheet (custom.css next to book.toml). */
  readCustomCss: (bookPath: string): Promise<string> =>
    invoke<string>("typesetter_read_custom_css", { bookPath }),

  /** Write the book's custom stylesheet (empty = delete). */
  writeCustomCss: (bookPath: string, css: string): Promise<void> =>
    invoke<void>("typesetter_write_custom_css", { bookPath, css }),

  /** Ask the AI to produce an updated custom.css from a plain-language
   *  description, given the current CSS and the document's class
   *  vocabulary. Returns the complete stylesheet (markdown fences stripped). */
  generateCustomCss: async (description: string, currentCss: string): Promise<string> => {
    const system = STYLE_SYSTEM_PROMPT;
    const prompt =
      `Current custom.css (may be empty):\n\`\`\`css\n${currentCss || ""}\n\`\`\`\n\n` +
      `Change request: ${description}\n\n` +
      `Return the COMPLETE updated custom.css — keep existing rules that still apply, ` +
      `modify or add as needed. Output ONLY CSS, no commentary, no markdown fences.`;
    const res = await invoke<{ content: string }>("ai_generate", {
      prompt,
      systemPrompt: system,
    });
    return stripCssFences(res.content);
  },
};

/** Strip ```css … ``` fences and stray prose the model sometimes adds. */
function stripCssFences(s: string): string {
  const fence = s.match(/```(?:css)?\s*([\s\S]*?)```/i);
  return (fence ? fence[1] : s).trim();
}

/** System prompt describing the typeset document's selector vocabulary so
 *  the model emits valid, correctly-scoped CSS that overrides the defaults. */
const STYLE_SYSTEM_PROMPT = `You write CSS for a print book typeset with CSS Paged Media (rendered by Chromium for PDF and Paged.js for the on-screen preview). Your CSS is appended AFTER the generated stylesheet, so it overrides defaults by source order — avoid !important unless necessary.

Use only these selectors (this is the document's structure):
- Text: body, p, h1, h2, h3, blockquote, em, strong, a
- Chapters: section[data-section-type="chapter"], and its opener title section[data-section-type="chapter"] > h1 (centered on its own page); first body letter .drop-cap, opening words .lead-in
- Other sections: section[data-section-type="interlude"] > h1, section[data-section-type="front-matter"], section[data-section-type="back-matter"]
- Table of Contents: .generated-toc-page, .gen-toc-title, .toc-entry, .toc-num, .toc-text, .toc-other, .toc-leader, .toc-folio
- List of Figures: .generated-lof-page, .gen-lof-title, .lof-entry, .lof-num; in-body figure number .fig-num
- Figures: figure, figcaption, figure.fig-text, figure.fig-bleed, figure.fig-fullpage, figure.fig-float-left, figure.fig-float-right
- Title page: .generated-title-page, .gen-book-title, .gen-book-subtitle, .gen-book-author
- Math: math, .math.display, blockquote.math-anchor
- Centered blocks: .center
- Page boxes: @page, @page :left, @page :right (margin boxes @top-center / @bottom-center hold the running header and folio)

Rules: use pt/in/em units (this is print, not screen — avoid px for type). Keep changes minimal and targeted to the request. Do not invent selectors or class names outside this list. Do not include @font-face or external @import. Output ONLY the CSS.`;
