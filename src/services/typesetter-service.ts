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
  };
  export: { color_mode: string };
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
};
