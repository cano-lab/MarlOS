import { Component, createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import {
  typesetterService,
  SectionAnchor,
  ScrollSurface,
  BookConfig,
} from "../services/typesetter-service";
import "./BookSourceView.css";

interface BookSourceViewProps {
  bookPath: string;
  files: string[];
  /** 1-based BookSection.order to jump to on mount/prop change. */
  currentSectionOrder?: number;
  /** Reports the topmost visible section's order as the user scrolls. */
  onSectionChange?: (order: number) => void;
  /** When false, the view is hidden via CSS (display:none). Programmatic
   *  scroll on hidden elements is a no-op, so we defer scroll-to-section
   *  until activation. */
  active?: boolean;
  /** When true, suppress the programmatic scroll-to-currentSectionOrder
   *  effect. Used in split mode where the parent does fractional scroll
   *  mirroring — section-boundary snaps would fight that. */
  disableSectionScrollSync?: boolean;
  /** Hands a scroll surface (textarea + per-section anchor query) back
   *  to the parent once mounted, so the parent can mirror scrolling
   *  between Source and Pages in split mode. */
  onScrollSurfaceReady?: (surface: ScrollSurface | null) => void;
  /** When true, restyle the textarea to match the typeset page body
   *  (EB Garamond, body-pt/leading-pt, body-width column on a gray
   *  surround). Source still edits markdown — the styling just makes
   *  the column read like a page. Used in split mode. */
  pageStyled?: boolean;
  /** Required when `pageStyled` is true. Pulls trim + typography to
   *  size the column to body width and match the leading. */
  config?: BookConfig | null;
  /** Fires on every input event with the (order, paraIndex) of the
   *  paragraph the cursor is currently inside. Used in split mode so
   *  the parent can flash the matching paragraph on the Pages side as
   *  the writer types. */
  onEditAt?: (order: number, paraIndex: number) => void;
  /** Called after a successful auto-save so BookMode can re-run the
   *  pandoc + structure + paginate pipeline against the new content. */
  onSaved: () => void | Promise<void>;
}

const SAVE_DEBOUNCE_MS = 1500;
const ZOOM_LS_KEY = "marlos-source-zoom";
const ZOOM_MIN = 0.6;
const ZOOM_MAX = 3.0;
const ZOOM_STEP = 0.1;

function readZoom(): number {
  try {
    const v = parseFloat(localStorage.getItem(ZOOM_LS_KEY) ?? "");
    if (Number.isFinite(v) && v >= ZOOM_MIN && v <= ZOOM_MAX) return v;
  } catch {
    // ignore
  }
  return 1.0;
}

function writeZoom(z: number) {
  try {
    localStorage.setItem(ZOOM_LS_KEY, String(z));
  } catch {
    // ignore quota errors
  }
}

/**
 * Live-edit view for Book Mode. Loads each manuscript file via the
 * Tauri commands, debounce-saves edits to disk, and notifies the
 * parent so it can re-paginate. No CodeMirror — a textarea is the
 * cheapest reliable surface for now and lets us prove the loop
 * end-to-end. Replace with the existing MarkdownEditor later if
 * the writing experience needs richer affordances.
 */
const BookSourceView: Component<BookSourceViewProps> = (props) => {
  const [activeIndex, setActiveIndex] = createSignal(0);
  const [content, setContent] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [savedAt, setSavedAt] = createSignal<Date | null>(null);
  const [dirty, setDirty] = createSignal(false);
  // Zoom multiplier applied to font-size + line-height. Persisted to
  // localStorage so the writer's choice survives reloads.
  const [zoom, setZoomSignal] = createSignal(readZoom());
  const setZoom = (z: number) => {
    const clamped = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, z));
    setZoomSignal(clamped);
    writeZoom(clamped);
  };
  const zoomIn = () => setZoom(zoom() + ZOOM_STEP);
  const zoomOut = () => setZoom(zoom() - ZOOM_STEP);
  const zoomReset = () => setZoom(1.0);

  let saveTimer: number | null = null;
  let editorRef: HTMLTextAreaElement | undefined;
  let scrollSyncRaf: number | null = null;
  // Suppress reporting onSectionChange while we're programmatically
  // scrolling (e.g. on view-switch). Otherwise we'd echo our own
  // jump back to the parent.
  let suppressReportUntil = 0;
  // Track the last order this view reported so we can skip a
  // round-trip scrollToOrder when the parent's currentSectionOrder
  // ends up matching our own report.
  let lastReportedOrder: number | null = null;

  /** Compute the line index of every H1 in the buffer. */
  const h1LineNumbers = (text: string): number[] => {
    const out: number[] = [];
    const lines = text.split("\n");
    for (let i = 0; i < lines.length; i++) {
      if (/^#\s+\S/.test(lines[i])) out.push(i);
    }
    return out;
  };

  /**
   * Map a source-line index to a fraction-of-scrollHeight position.
   * The textarea soft-wraps long lines, so `lineNumber × lineHeight`
   * is wrong — a 600-character paragraph occupies many visual lines.
   * Instead we use `scrollHeight` (which already includes wrap) as
   * the canonical total, and treat each source line as a fixed
   * fraction `1 / total_source_lines`. The mapping is approximate
   * but consistent: as long as wrap is roughly uniform across the
   * manuscript, scroll-to-line lands within a line or two.
   */
  const sourceLineCount = (text: string): number => {
    const n = text.split("\n").length;
    return n > 0 ? n : 1;
  };

  /** Scroll the textarea so the Nth H1 (1-based) lands near the top. */
  const scrollToOrder = (order: number) => {
    if (!editorRef) return;
    const text = content();
    const lines = h1LineNumbers(text);
    if (lines.length === 0) return;
    const idx = Math.max(0, Math.min(order - 1, lines.length - 1));
    const ratio = lines[idx] / sourceLineCount(text);
    suppressReportUntil = performance.now() + 600;
    editorRef.scrollTop = Math.floor(ratio * editorRef.scrollHeight);
  };

  /** Cached canvas measurer so we don't re-create it on every getAnchors
   *  call. The 2D context is reused; only ctx.font and the input lines
   *  change between calls. */
  let measureCanvas: HTMLCanvasElement | null = null;
  let measureCtx: CanvasRenderingContext2D | null = null;
  const getMeasureCtx = (): CanvasRenderingContext2D | null => {
    if (measureCtx) return measureCtx;
    measureCanvas = document.createElement("canvas");
    measureCtx = measureCanvas.getContext("2d");
    return measureCtx;
  };

  /** For each source line, compute its visual scrollTop in the textarea.
   *
   *  Soft-wrapping makes `lineIdx × lineHeight` wrong for proportional
   *  text and for lines that exceed the wrap width. We replicate the
   *  textarea's wrap by measuring each line with canvas.measureText
   *  (using the textarea's exact font + inner width) and counting
   *  visual rows = ceil(lineWidth / innerWidth). Monospace-friendly
   *  but works for proportional too — measureText is kerning-aware.
   *
   *  Returned array is parallel to text.split("\n") — index i is the
   *  scrollTop where source line i begins. Empty lines still take
   *  one visual row. */
  const computeLineTops = (text: string): number[] => {
    if (!editorRef) return [];
    const lines = text.split("\n");
    const cs = getComputedStyle(editorRef);
    const fontSize = parseFloat(cs.fontSize) || 12;
    let lineHeight = parseFloat(cs.lineHeight);
    if (!Number.isFinite(lineHeight) || lineHeight <= 0) {
      lineHeight = fontSize * 1.5;
    }
    const padTop = parseFloat(cs.paddingTop) || 0;
    const padLeft = parseFloat(cs.paddingLeft) || 0;
    const padRight = parseFloat(cs.paddingRight) || 0;
    const innerWidth = (editorRef.clientWidth || 0) - padLeft - padRight;
    const ctx = getMeasureCtx();
    if (!ctx || innerWidth <= 0) {
      // Fallback: assume one visual row per source line.
      return lines.map((_, i) => padTop + i * lineHeight);
    }
    ctx.font = `${cs.fontStyle} ${cs.fontWeight} ${fontSize}px ${cs.fontFamily}`;
    const tops: number[] = new Array(lines.length);
    let visualRow = 0;
    for (let i = 0; i < lines.length; i++) {
      tops[i] = padTop + visualRow * lineHeight;
      const line = lines[i];
      if (line.length === 0) {
        visualRow += 1;
      } else {
        // measureText doesn't account for word-break behavior — the
        // textarea wraps at word boundaries when possible — but for
        // typical paragraph-per-line manuscripts this overcounts by
        // at most ~1 row per paragraph, well under a section's worth
        // of drift.
        const w = ctx.measureText(line).width;
        const rows = Math.max(1, Math.ceil(w / innerWidth));
        visualRow += rows;
      }
    }
    return tops;
  };

  /** True if `line` looks like the start of a markdown paragraph that
   *  pandoc will emit as <p>. Excludes headings, lists, blockquotes,
   *  fenced code, indented code, and raw HTML blocks (`<div ...>`,
   *  `<hr>`, etc.) — these become <ul>/<pre>/raw-html and are NOT
   *  counted on the pages side either. Keeps the source-side paraIndex
   *  aligned with the pages-side paraIndex even when the writer
   *  inserts space/page-break div snippets. */
  const isParagraphStart = (line: string): boolean => {
    const t = line.trim();
    if (t === "") return false;
    if (/^#{1,6}\s+/.test(t)) return false; // heading
    if (/^[-*+]\s/.test(t)) return false; // unordered list item
    if (/^\d+\.\s/.test(t)) return false; // ordered list item
    if (/^>\s?/.test(t)) return false; // blockquote
    if (/^```/.test(t)) return false; // fence
    if (/^    /.test(line)) return false; // indented code (4 spaces)
    if (/^\t/.test(line)) return false; // indented code (tab)
    if (/^</.test(t)) return false; // raw HTML block (div, hr, table, etc.)
    return true;
  };

  /** Cached anchor list — recomputing this on every scroll event is
   *  expensive (O(N) measureText calls + a full line walk for an
   *  N-line manuscript). We invalidate on content change and on
   *  textarea resize. */
  let cachedAnchors: SectionAnchor[] | null = null;
  let cachedAnchorsKey: string | null = null;
  const anchorsKey = (): string => {
    const w = editorRef?.clientWidth ?? 0;
    return `${content().length}|${w}`;
  };
  const invalidateAnchors = () => {
    cachedAnchors = null;
    cachedAnchorsKey = null;
  };

  /** Compute per-paragraph scrollTop anchors. paraIndex 0 = the section
   *  heading line; 1+ = each blank-separated paragraph block following
   *  it. Pairs by `(order, paraIndex)` with the pages-side anchors so
   *  cross-pane sync resolves to the matching paragraph rather than the
   *  matching fraction-of-section. Cached — see `invalidateAnchors`. */
  const getAnchors = (): SectionAnchor[] => {
    const key = anchorsKey();
    if (cachedAnchors && cachedAnchorsKey === key) return cachedAnchors;
    if (!editorRef) return [];
    const text = content();
    const lines = text.split("\n");
    if (lines.length === 0) {
      cachedAnchors = [];
      cachedAnchorsKey = key;
      return cachedAnchors;
    }
    const tops = computeLineTops(text);

    const result: SectionAnchor[] = [];
    let order = 0; // 0 = before any H1 — those lines have no anchor
    let paraIndex = 0;
    let inParagraph = false;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      // # H1 starts a new section. Emit the section heading anchor at
      // paraIndex 0, then reset paraIndex.
      if (/^#\s+\S/.test(line)) {
        order += 1;
        paraIndex = 0;
        inParagraph = false;
        result.push({ order, paraIndex: 0, top: tops[i] ?? 0 });
        continue;
      }
      if (order === 0) continue; // pre-amble, no section yet

      const t = line.trim();
      if (t === "") {
        inParagraph = false;
        continue;
      }
      if (!inParagraph) {
        if (isParagraphStart(line)) {
          paraIndex += 1;
          result.push({ order, paraIndex, top: tops[i] ?? 0 });
        }
        inParagraph = true;
      }
    }
    cachedAnchors = result;
    cachedAnchorsKey = key;
    return result;
  };

  /** Find which H1 is closest to (and not below) the current scroll. */
  const reportCurrentSection = () => {
    if (!editorRef) return;
    if (performance.now() < suppressReportUntil) return;
    const text = content();
    const lines = h1LineNumbers(text);
    if (lines.length === 0) return;
    const sh = editorRef.scrollHeight || 1;
    const ratio = editorRef.scrollTop / sh;
    const topSourceLine = Math.floor(ratio * sourceLineCount(text));
    let active = 0;
    for (let i = 0; i < lines.length; i++) {
      if (lines[i] <= topSourceLine) active = i;
      else break;
    }
    const order = active + 1;
    if (order !== lastReportedOrder) {
      lastReportedOrder = order;
      props.onSectionChange?.(order);
    }
  };

  const handleScroll = () => {
    // Sync gutter scroll immediately (no RAF — it must visually follow
    // the textarea exactly, no lag).
    if (gutterRef && editorRef && gutterRef.scrollTop !== editorRef.scrollTop) {
      gutterRef.scrollTop = editorRef.scrollTop;
    }
    if (scrollSyncRaf !== null) cancelAnimationFrame(scrollSyncRaf);
    scrollSyncRaf = requestAnimationFrame(() => {
      scrollSyncRaf = null;
      reportCurrentSection();
    });
  };

  const loadActive = async () => {
    setLoading(true);
    setError(null);
    try {
      const text = await typesetterService.readBookFile(props.bookPath, activeIndex());
      setContent(text);
      // The textarea is uncontrolled (no `value={content()}` binding) —
      // we set its value imperatively here on programmatic load so
      // user-typing doesn't pay the cost of a value re-assignment
      // through Solid on every keystroke (which can reset scroll
      // position on a 200KB textarea).
      if (editorRef) editorRef.value = text;
      setDirty(false);
    } catch (e) {
      setError(`Failed to load: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLoading(false);
    }
  };

  const flushSave = async () => {
    if (saveTimer !== null) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (!dirty()) return;
    setSaving(true);
    setError(null);
    try {
      await typesetterService.writeBookFile(props.bookPath, activeIndex(), content());
      setDirty(false);
      setSavedAt(new Date());
      await props.onSaved();
    } catch (e) {
      setError(`Save failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const scheduleSave = () => {
    if (saveTimer !== null) clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      saveTimer = null;
      void flushSave();
    }, SAVE_DEBOUNCE_MS);
  };

  /** Walk the buffer up to the cursor position, applying the same
   *  paragraph-counting rules as `getAnchors`, and return the
   *  `(order, paraIndex)` of the paragraph the cursor is currently
   *  inside. Used to drive the live "flash matching paragraph in
   *  Pages" indicator. */
  const cursorAnchor = (): { order: number; paraIndex: number } | null => {
    if (!editorRef) return null;
    const pos = editorRef.selectionStart ?? 0;
    const text = content();
    const cursorLine = text.slice(0, pos).split("\n").length - 1;
    const lines = text.split("\n");
    const upTo = Math.min(cursorLine, lines.length - 1);

    let order = 0;
    let paraIndex = 0;
    let inParagraph = false;
    let last: { order: number; paraIndex: number } | null = null;

    for (let i = 0; i <= upTo; i++) {
      const line = lines[i];
      if (/^#\s+\S/.test(line)) {
        order += 1;
        paraIndex = 0;
        inParagraph = false;
        last = { order, paraIndex: 0 };
        continue;
      }
      if (order === 0) continue;

      if (line.trim() === "") {
        inParagraph = false;
        continue;
      }
      if (!inParagraph) {
        if (isParagraphStart(line)) {
          paraIndex += 1;
          last = { order, paraIndex };
        }
        inParagraph = true;
      }
    }
    return last;
  };

  // RAF-throttle the onEditAt emission. Holding down a key fires input
  // at ~60Hz; we only need one emission per frame, and only when the
  // cursor's anchor actually changes (or on the first input within a
  // paragraph after a quiet period — handled by the receiver via
  // animation-restart, not here).
  let editAtRaf: number | null = null;
  let lastEmittedAnchor: { order: number; paraIndex: number } | null = null;
  const reportEditAt = () => {
    if (!props.onEditAt) return;
    if (editAtRaf !== null) return;
    editAtRaf = requestAnimationFrame(() => {
      editAtRaf = null;
      const a = cursorAnchor();
      if (!a) return;
      // Always emit — receiver restarts the flash animation each call
      // so continuous typing keeps the highlight lit.
      lastEmittedAnchor = a;
      props.onEditAt!(a.order, a.paraIndex);
    });
  };
  void lastEmittedAnchor;

  const handleInput = (e: InputEvent & { currentTarget: HTMLTextAreaElement }) => {
    setContent(e.currentTarget.value);
    setDirty(true);
    invalidateAnchors();
    scheduleSave();
    reportEditAt();
  };

  // Width-change invalidation. Soft-wrap recomputes when the textarea
  // resizes (e.g., the user drags the split-pane divider, or the
  // window resizes), which moves every line's scrollTop.
  let textareaResizeObserver: ResizeObserver | null = null;
  const attachResizeObserver = (el: HTMLElement | null) => {
    textareaResizeObserver?.disconnect();
    textareaResizeObserver = null;
    if (!el || typeof ResizeObserver === "undefined") return;
    textareaResizeObserver = new ResizeObserver(() => {
      invalidateAnchors();
      // Width change → wraps shift → line tops shift → re-render gutter.
      scheduleGutterRender(150);
    });
    textareaResizeObserver.observe(el);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void flushSave();
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      // Zoom shortcuts. `+` requires Shift on US layouts, so accept
      // both `+` and `=` for zoom-in; `-` for zoom-out; `0` to reset.
      if (e.key === "+" || e.key === "=") {
        e.preventDefault();
        zoomIn();
      } else if (e.key === "-" || e.key === "_") {
        e.preventDefault();
        zoomOut();
      } else if (e.key === "0") {
        e.preventDefault();
        zoomReset();
      }
    }
  };

  // Reload when file selection changes
  createEffect(() => {
    void activeIndex();
    void loadActive();
  });

  // Jump to the parent-shared current section whenever:
  //   - the parent updates currentSectionOrder while we're visible, OR
  //   - we transition from hidden to visible (active flips true).
  // Scrolling a display:none textarea is a no-op, so we MUST wait for
  // visibility before applying the scroll position.
  //
  // The lastReportedOrder check prevents the feedback loop: when the
  // user scrolls and we report the new section, the parent signal
  // updates and this effect fires — but the value matches what we
  // already reported, so we skip the programmatic scroll-to.
  createEffect(() => {
    const order = props.currentSectionOrder;
    const active = props.active !== false;
    if (typeof order !== "number") return;
    if (!active) return;
    if (loading()) return;
    if (props.disableSectionScrollSync) return;
    if (order === lastReportedOrder) return;
    lastReportedOrder = order;
    queueMicrotask(() => scrollToOrder(order));
  });

  onCleanup(() => {
    if (saveTimer !== null) {
      // Best-effort: synchronous save on unmount isn't possible with
      // async invoke, so just flush asynchronously and let it land.
      const c = content();
      if (dirty()) {
        void typesetterService.writeBookFile(props.bookPath, activeIndex(), c);
      }
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    textareaResizeObserver?.disconnect();
    textareaResizeObserver = null;
    if (gutterRenderTimer !== null) {
      clearTimeout(gutterRenderTimer);
      gutterRenderTimer = null;
    }
  });

  const fileBaseName = (path: string) => path.split(/[/\\]/).pop() ?? path;

  /** Insert a snippet at the cursor (or replace the selection). Wraps
   *  the snippet in surrounding blank lines so block-level divs land
   *  on their own paragraph in pandoc's eyes. After insertion, places
   *  the cursor on the line after the snippet, marks the buffer dirty,
   *  schedules an autosave, and emits the live-edit flash event. */
  const insertSnippet = (snippet: string) => {
    if (!editorRef) return;
    const ta = editorRef;
    const start = ta.selectionStart ?? ta.value.length;
    const end = ta.selectionEnd ?? start;
    const before = ta.value.slice(0, start);
    const after = ta.value.slice(end);
    // Make sure there's a blank line before and after, since pandoc
    // treats consecutive non-blank lines as one paragraph.
    const needsBlankBefore = before.length > 0 && !before.endsWith("\n\n");
    const needsBlankAfter = after.length > 0 && !after.startsWith("\n\n");
    const prefix = needsBlankBefore ? (before.endsWith("\n") ? "\n" : "\n\n") : "";
    const suffix = needsBlankAfter ? (after.startsWith("\n") ? "\n" : "\n\n") : "";
    const inserted = `${prefix}${snippet}${suffix}`;
    const next = before + inserted + after;
    ta.value = next;
    setContent(next);
    setDirty(true);
    invalidateAnchors();
    const cursor = before.length + inserted.length;
    ta.focus();
    ta.setSelectionRange(cursor, cursor);

    // Scroll the textarea so the snippet sits near the upper third of
    // the visible area — otherwise the cursor jumps off-screen on
    // long manuscripts and the writer loses their place.
    const snippetLine = (before + prefix).split("\n").length - 1;
    const tops = computeLineTops(next);
    const snippetTop = tops[snippetLine] ?? 0;
    ta.scrollTop = Math.max(0, snippetTop - ta.clientHeight / 3);

    scheduleSave();
    reportEditAt();
  };

  /** Compute the body-text width (in inches) for the configured trim
   *  size and margins. Matches `@page` body in the rendered PDF, so
   *  the page-styled column reads at the same density as actual
   *  pages. */
  const bodyWidthIn = (): number | null => {
    const c = props.config;
    if (!c) return null;
    let trimW: number;
    switch (c.trim.size) {
      case "5x8":
        trimW = 5;
        break;
      case "5.5x8.5":
        trimW = 5.5;
        break;
      case "6x9":
        trimW = 6;
        break;
      default: {
        // Free-form "WxH" inches (Custom trim option).
        const m = /^(\d+(?:\.\d+)?)x(\d+(?:\.\d+)?)$/.exec(c.trim.size);
        trimW = m ? parseFloat(m[1]) : 6;
        if (!Number.isFinite(trimW) || trimW <= 0) trimW = 6;
      }
    }
    const inner = trimW - c.trim.margins_in.inside - c.trim.margins_in.outside;
    return inner > 1 ? inner : null;
  };

  /** Inline style applied to the textarea. In paged mode, pulls font /
   *  size / leading from the user's typography config and column
   *  width from the trim + margins. Zoom is NOT applied here — it's
   *  applied via CSS `zoom` on the shared zoom-area wrapper around
   *  the gutter+textarea, so everything (column, font, margins,
   *  gutter alignment) scales together as a PDF-page would. */
  const pageStyledStyle = (): Record<string, string> | undefined => {
    if (!props.pageStyled || !props.config) return undefined;
    const t = props.config.typography;
    const font =
      t.body_font && t.body_font.trim().length > 0
        ? t.body_font
        : "EB Garamond";
    const w = bodyWidthIn();
    return {
      "font-family": `"${font}", Georgia, "Times New Roman", serif`,
      "font-size": `${t.body_size_pt}pt`,
      "line-height": `${t.body_leading_pt}pt`,
      width: w ? `${w}in` : "100%",
    };
  };

  // ----- Line number gutter --------------------------------------------------
  // Gutter is rendered via direct innerHTML write rather than a Solid
  // <For>. Reactively rendering 5000+ absolutely-positioned divs costs
  // O(N) Solid reactions on every keystroke; one innerHTML string is
  // a single browser parse. We also debounce the recompute (150ms
  // after typing stops) so fast typing doesn't pay the
  // canvas.measureText cost on every keystroke.
  let gutterRef: HTMLDivElement | undefined;
  let gutterRenderTimer: number | null = null;

  // Regex matching lines that are pure snippet divs (page-break,
  // blank-page, space-*). Used by the gutter render to flag those
  // lines with a distinct color + icon — the textarea itself can't
  // render HTML so we can't colorize the div text inline.
  const SNIPPET_LINE_RE =
    /^\s*<div class="(page-break|blank-page|space-small|space-medium|space-large|space-section)">\s*<\/div>\s*$/;

  const renderGutterNow = () => {
    if (!gutterRef || !editorRef) return;
    const text = content();
    const tops = computeLineTops(text);
    if (tops.length === 0) {
      gutterRef.innerHTML = "";
      return;
    }
    const lines = text.split("\n");
    const spacerHeight = tops[tops.length - 1] ?? 0;
    let html =
      '<div class="book-source-gutter-spacer" style="height:' +
      spacerHeight +
      'px"></div>';
    for (let i = 0; i < tops.length; i++) {
      const snippetMatch = (lines[i] ?? "").match(SNIPPET_LINE_RE);
      if (snippetMatch) {
        // Pick a glyph that hints at the snippet kind.
        const kind = snippetMatch[1];
        const glyph =
          kind === "page-break"
            ? "↵" // ↵ return arrow
            : kind === "blank-page"
              ? "▭" // ▭ rectangle
              : "—"; // — em-dash for space snippets
        html +=
          '<div class="book-source-gutter-num book-source-gutter-num-snippet" data-kind="' +
          kind +
          '" style="top:' +
          tops[i] +
          'px"><span class="book-source-gutter-glyph">' +
          glyph +
          "</span>" +
          (i + 1) +
          "</div>";
      } else {
        html +=
          '<div class="book-source-gutter-num" style="top:' +
          tops[i] +
          'px">' +
          (i + 1) +
          "</div>";
      }
    }
    gutterRef.innerHTML = html;
  };

  const scheduleGutterRender = (delayMs = 150) => {
    if (gutterRenderTimer !== null) clearTimeout(gutterRenderTimer);
    gutterRenderTimer = window.setTimeout(() => {
      gutterRenderTimer = null;
      renderGutterNow();
    }, delayMs);
  };

  // Schedule a debounced gutter re-render whenever the content or
  // layout-affecting props change. Zoom is included because font-size
  // change shifts every line's scrollTop.
  createEffect(() => {
    void content();
    void props.pageStyled;
    void props.config;
    void zoom();
    scheduleGutterRender(150);
  });

  const SNIPPETS: Array<{ label: string; title: string; snippet: string }> = [
    {
      label: "½ line",
      title: 'Small space (~½ line) — <div class="space-small"></div>',
      snippet: '<div class="space-small"></div>',
    },
    {
      label: "1 line",
      title: 'Medium space (~1 line) — <div class="space-medium"></div>',
      snippet: '<div class="space-medium"></div>',
    },
    {
      label: "2 lines",
      title: 'Large space (~2 lines) — <div class="space-large"></div>',
      snippet: '<div class="space-large"></div>',
    },
    {
      label: "Section",
      title: 'Section break (~4 lines) — <div class="space-section"></div>',
      snippet: '<div class="space-section"></div>',
    },
    {
      label: "↵ Page",
      title: 'Page break — <div class="page-break"></div>',
      snippet: '<div class="page-break"></div>',
    },
    {
      label: "▭ Blank",
      title: 'Blank page — <div class="blank-page"></div>',
      snippet: '<div class="blank-page"></div>',
    },
  ];

  return (
    <div class="book-source">
      <div class="book-source-toolbar">
        <Show when={props.files.length > 1}>
          <div class="book-source-tabs">
            <For each={props.files}>
              {(f, i) => (
                <button
                  classList={{
                    "book-source-tab": true,
                    active: activeIndex() === i(),
                  }}
                  onClick={() => {
                    if (dirty()) void flushSave();
                    setActiveIndex(i());
                  }}
                  title={f}
                >
                  {fileBaseName(f)}
                </button>
              )}
            </For>
          </div>
        </Show>
        <Show when={props.files.length === 1}>
          <span class="book-source-filename">{fileBaseName(props.files[0])}</span>
        </Show>
        <span class="book-source-status">
          <Show when={saving()}>saving...</Show>
          <Show when={!saving() && dirty()}>● unsaved</Show>
          <Show when={!saving() && !dirty() && savedAt()}>
            ✓ saved {savedAt()!.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
          </Show>
        </span>
        <button class="book-source-btn" onClick={flushSave} disabled={!dirty() || saving()}>
          Save now (Ctrl+S)
        </button>
        <div class="book-source-zoom">
          <button
            class="book-source-btn book-source-zoom-btn"
            onClick={zoomOut}
            title="Zoom out (Ctrl+−)"
            disabled={zoom() <= ZOOM_MIN + 1e-6}
          >
            −
          </button>
          <button
            class="book-source-btn book-source-zoom-btn"
            onClick={zoomReset}
            title="Reset zoom (Ctrl+0)"
          >
            {Math.round(zoom() * 100)}%
          </button>
          <button
            class="book-source-btn book-source-zoom-btn"
            onClick={zoomIn}
            title="Zoom in (Ctrl+=)"
            disabled={zoom() >= ZOOM_MAX - 1e-6}
          >
            +
          </button>
        </div>
      </div>

      <div class="book-source-snippets">
        <span class="book-source-snippets-label">Insert:</span>
        <For each={SNIPPETS}>
          {(s) => (
            <button
              class="book-source-snippet-btn"
              title={s.title}
              onClick={() => insertSnippet(s.snippet)}
            >
              {s.label}
            </button>
          )}
        </For>
      </div>

      <Show when={error()}>
        <div class="book-source-error">{error()}</div>
      </Show>

      <Show
        when={!loading()}
        fallback={<div class="book-source-loading">Loading...</div>}
      >
        <div
          classList={{
            "book-source-shell": true,
            "book-source-shell-paged": props.pageStyled === true,
          }}
        >
          <div
            class="book-source-zoom-area"
            style={zoom() === 1.0 ? undefined : { zoom: `${zoom()}` }}
          >
            <div
              class="book-source-gutter"
              ref={(el) => {
                gutterRef = el ?? undefined;
                if (el) queueMicrotask(renderGutterNow);
              }}
            />
            <textarea
              ref={(el) => {
                editorRef = el;
                attachResizeObserver(el);
                if (el) el.value = content();
                props.onScrollSurfaceReady?.(el ? { el, getAnchors } : null);
              }}
              classList={{
                "book-source-editor": true,
                "book-source-editor-paged": props.pageStyled === true,
              }}
              style={pageStyledStyle()}
              onInput={handleInput}
              onKeyDown={handleKeyDown}
              onScroll={handleScroll}
              spellcheck={false}
              autocomplete="off"
            />
          </div>
        </div>
      </Show>
    </div>
  );
};

export default BookSourceView;
