import { Component, createSignal, createEffect, onCleanup, For, Show } from "solid-js";
import type { EditorView } from "@codemirror/view";
import MarkdownEditor from "./MarkdownEditor";
import { typesetterService, BookConfig, ScrollSurface, SectionAnchor } from "../services/typesetter-service";
import "./BookEditorPane.css";

/**
 * Book-aware source editor — the CodeMirror MarkdownEditor wired to a
 * book.toml manuscript file (load/save) plus the book-specific insert
 * toolbar (page breaks, spacing, centered blocks). Replaces the old
 * BookSourceView textarea, whose raw-textarea scroll handling caused the
 * edit-jump / scroll-hang bugs. Synchronized scrolling with the Pages
 * preview is deferred (will use H1 anchors).
 */
interface BookEditorPaneProps {
  bookPath: string;
  files: string[];
  config: BookConfig | null;
  /** Bumped by the parent on book (re)load — re-read the active file
   *  from disk, but only when there are no unsaved edits. */
  reloadToken?: number;
  /** Called after a successful autosave so the parent can re-paginate. */
  onSaved: () => void | Promise<void>;
  /** Hands the CodeMirror scroll element to the parent so it can mirror
   *  scrolling with the Pages preview. Per-section anchors are stubbed
   *  until H1-anchor sync is built. */
  onScrollSurfaceReady?: (surface: ScrollSurface | null) => void;
  // --- Accepted for drop-in compatibility with the old source view;
  //     not yet wired (sync/flash deferred to the H1-anchor work). ---
  currentSectionOrder?: number;
  onSectionChange?: (order: number) => void;
  active?: boolean;
  disableSectionScrollSync?: boolean;
  pageStyled?: boolean;
  onEditAt?: (order: number, paraIndex: number) => void;
}

interface Snippet {
  label: string;
  title: string;
  snippet: string;
}

const SNIPPETS: Snippet[] = [
  { label: "↵ Page", title: 'Page break — <div class="page-break"></div>', snippet: '<div class="page-break"></div>' },
  { label: "▭ Blank", title: 'Blank page — <div class="blank-page"></div>', snippet: '<div class="blank-page"></div>' },
  { label: "½ line", title: 'Small space — <div class="space-small"></div>', snippet: '<div class="space-small"></div>' },
  { label: "1 line", title: 'Medium space — <div class="space-medium"></div>', snippet: '<div class="space-medium"></div>' },
  { label: "2 lines", title: 'Large space — <div class="space-large"></div>', snippet: '<div class="space-large"></div>' },
  { label: "Section", title: 'Section break — <div class="space-section"></div>', snippet: '<div class="space-section"></div>' },
];

const fileBaseName = (path: string) => path.split(/[/\\]/).pop() ?? path;

const BookEditorPane: Component<BookEditorPaneProps> = (props) => {
  const [activeIndex, setActiveIndex] = createSignal(0);
  const [content, setContent] = createSignal("");
  const [dirty, setDirty] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);
  let view: EditorView | undefined;
  let saveTimer: number | null = null;

  // Editor zoom — scales the CodeMirror font-size (and gutter) via a CSS
  // variable on the host. Persisted across sessions.
  const ZOOM_KEY = "marlos-book-editor-zoom";
  const ZOOM_MIN = 0.6;
  const ZOOM_MAX = 2.5;
  const readZoom = (): number => {
    try {
      const v = parseFloat(localStorage.getItem(ZOOM_KEY) ?? "");
      return Number.isFinite(v) && v >= ZOOM_MIN && v <= ZOOM_MAX ? v : 1;
    } catch {
      return 1;
    }
  };
  const [zoom, setZoom] = createSignal<number>(readZoom());
  createEffect(() => {
    try {
      localStorage.setItem(ZOOM_KEY, String(zoom()));
    } catch {
      /* localStorage unavailable — ignore */
    }
  });
  const round1 = (n: number) => Math.round(n * 10) / 10;
  const zoomIn = () => setZoom((z) => Math.min(ZOOM_MAX, round1(z + 0.1)));
  const zoomOut = () => setZoom((z) => Math.max(ZOOM_MIN, round1(z - 0.1)));
  const zoomReset = () => setZoom(1);

  const loadActive = async () => {
    setError(null);
    try {
      const text = await typesetterService.readBookFile(props.bookPath, activeIndex());
      setContent(text);
      setDirty(false);
    } catch (e) {
      setError(`Failed to load: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // Load on file switch / book change. (CodeMirror updates its doc from
  // the `content` prop; user typing already matches the doc so it no-ops.)
  createEffect(() => {
    void activeIndex();
    void props.bookPath;
    void loadActive();
  });

  // Re-read on an explicit reload, but never clobber unsaved edits.
  let reloadInitialized = false;
  createEffect(() => {
    void props.reloadToken;
    if (!reloadInitialized) {
      reloadInitialized = true;
      return;
    }
    if (!dirty()) void loadActive();
  });

  const flushSave = async () => {
    if (saveTimer !== null) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (!dirty()) return;
    setSaving(true);
    try {
      await typesetterService.writeBookFile(props.bookPath, activeIndex(), content());
      setDirty(false);
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
    }, 1500);
  };

  onCleanup(() => {
    if (saveTimer !== null) {
      clearTimeout(saveTimer);
      saveTimer = null;
      if (dirty()) {
        void typesetterService.writeBookFile(props.bookPath, activeIndex(), content());
      }
    }
  });

  const handleChange = (text: string) => {
    setContent(text);
    setDirty(true);
    scheduleSave();
  };

  /** H1 anchors for synchronized scrolling: each `# ` heading's vertical
   *  offset in the editor, keyed by 1-based H1 index (which matches the
   *  pages' data-section-order — the parser numbers top-level sections in
   *  document order). The parent interpolates scroll position between
   *  consecutive H1s. paraIndex 0 = the heading line. */
  const getAnchors = (): SectionAnchor[] => {
    if (!view) return [];
    const doc = view.state.doc;
    const out: SectionAnchor[] = [];
    let order = 0;
    for (let i = 1; i <= doc.lines; i++) {
      const line = doc.line(i);
      if (/^#[ \t]/.test(line.text)) {
        order++;
        try {
          out.push({ order, paraIndex: 0, top: view.lineBlockAt(line.from).top });
        } catch {
          /* line not measured yet — skip */
        }
      }
    }
    return out;
  };

  /** Splice text at the cursor (replacing any selection) via a CodeMirror
   *  transaction — keeps scroll/undo intact, unlike a textarea value reset. */
  const insertAtCursor = (text: string, caretOffset?: number) => {
    if (!view) return;
    const sel = view.state.selection.main;
    const at = sel.from + (caretOffset ?? text.length);
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert: text },
      selection: { anchor: at },
    });
    view.focus();
  };

  const insertSnippet = (snippet: string) => {
    // Block-level divs need blank lines around them so pandoc treats them
    // as their own block rather than folding into a paragraph.
    insertAtCursor(`\n\n${snippet}\n\n`);
  };

  // --- Image insert modal -------------------------------------------------
  const [showImage, setShowImage] = createSignal(false);
  const [imgPath, setImgPath] = createSignal("");
  const [imgCaption, setImgCaption] = createSignal("");
  // inline | text | bleed | float-left | float-right
  const [imgMode, setImgMode] = createSignal("inline");
  const [imgWidth, setImgWidth] = createSignal("60%");
  const [imgInset, setImgInset] = createSignal("0.25in");

  const insertImage = () => {
    const path = imgPath().trim();
    if (!path) {
      setError("Enter an image path (relative to the manuscript).");
      return;
    }
    const cap = imgCaption().trim();
    // Stable id so the (future) right-click margin editor can find this
    // figure and write its inset back here.
    const id = "fig-" + Date.now().toString(36);
    const mode = imgMode();
    let attrs = `#${id}`;
    if (mode === "text") {
      attrs += " .fig-text";
    } else if (mode === "bleed") {
      attrs += ` .fig-bleed style="--fig-inset:${imgInset().trim() || "0.25in"}"`;
    } else if (mode === "float-left") {
      attrs += ` .fig-float-left width="${imgWidth().trim() || "40%"}"`;
    } else if (mode === "float-right") {
      attrs += ` .fig-float-right width="${imgWidth().trim() || "40%"}"`;
    } else {
      attrs += ` width="${imgWidth().trim() || "60%"}"`; // inline block
    }
    // Image alone in a paragraph → pandoc implicit_figures wraps it in a
    // <figure> with the caption as <figcaption>.
    insertAtCursor(`\n\n![${cap}](${path}){${attrs}}\n\n`);
    setShowImage(false);
    setImgPath("");
    setImgCaption("");
  };

  /** Set (or clear) the running-header override on the heading the cursor
   *  is on, by editing its `{header="..."}` attribute — so the writer
   *  doesn't have to remember the markdown syntax. */
  const setRunningHeader = () => {
    if (!view) return;
    const line = view.state.doc.lineAt(view.state.selection.main.head);
    const text = line.text;
    if (!/^#{1,6}\s+/.test(text)) {
      setError("Put the cursor on a heading line (# …) to set its running header.");
      return;
    }
    // Split heading text from a trailing { ...attributes... } block.
    const m = text.match(/^(.*?)\s*\{([^}]*)\}\s*$/);
    const base = m ? m[1] : text.replace(/\s+$/, "");
    let attrs = m ? m[2] : "";
    const cur = attrs.match(/header\s*=\s*"([^"]*)"/);
    const input = window.prompt(
      "Running header for this section (leave blank to use the section title):",
      cur ? cur[1] : "",
    );
    if (input === null) return; // cancelled
    // Drop any existing header=… then re-add it if non-empty.
    attrs = attrs.replace(/\s*header\s*=\s*("[^"]*"|[^\s}]+)/, "").trim();
    const value = input.trim().replace(/"/g, "");
    if (value) attrs = `${attrs} header="${value}"`.trim();
    const newLine = attrs ? `${base} {${attrs}}` : base;
    view.dispatch({ changes: { from: line.from, to: line.to, insert: newLine } });
    view.focus();
  };

  /** Wrap the current selection (or an empty placeholder) in a pandoc
   *  `:::center` fenced div so the block renders centered. */
  const centerSelection = () => {
    if (!view) return;
    const sel = view.state.selection.main;
    const selected = view.state.sliceDoc(sel.from, sel.to);
    const inner = selected.trim().length > 0 ? selected.trim() : "";
    const wrapped = `\n\n:::center\n${inner}\n:::\n\n`;
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert: wrapped },
      // place caret on the inner line
      selection: { anchor: sel.from + 2 + ":::center\n".length + inner.length },
    });
    view.focus();
  };

  return (
    <div class="book-editor-pane">
      <div class="book-editor-toolbar">
        <Show when={props.files.length > 1}>
          <div class="book-editor-tabs">
            <For each={props.files}>
              {(f, i) => (
                <button
                  classList={{ "book-editor-tab": true, active: activeIndex() === i() }}
                  title={f}
                  onClick={() => {
                    if (dirty()) void flushSave();
                    setActiveIndex(i());
                  }}
                >
                  {fileBaseName(f)}
                </button>
              )}
            </For>
          </div>
        </Show>
        <Show when={props.files.length === 1}>
          <span class="book-editor-filename">{fileBaseName(props.files[0])}</span>
        </Show>
        <span class="book-editor-status">
          {saving() ? "Saving…" : dirty() ? "Unsaved" : "Saved"}
        </span>
        <div class="book-editor-snippets">
          <span class="book-editor-snippets-label">Insert:</span>
          <For each={SNIPPETS}>
            {(s) => (
              <button class="book-editor-snippet-btn" title={s.title} onClick={() => insertSnippet(s.snippet)}>
                {s.label}
              </button>
            )}
          </For>
          <button
            class="book-editor-snippet-btn"
            title="Center the current selection — wraps it in a :::center fenced div"
            onClick={centerSelection}
          >
            ⊟ Center
          </button>
          <button
            class="book-editor-snippet-btn"
            title="Set this section's running header (the repeating line at the top of its pages)"
            onClick={setRunningHeader}
          >
            ⊤ Header
          </button>
          <button
            class="book-editor-snippet-btn"
            title="Insert an image (figure) with a caption and layout"
            onClick={() => setShowImage(true)}
          >
            🖼 Image
          </button>
        </div>
        <div class="book-editor-zoom">
          <button class="book-editor-zoom-btn" title="Zoom out" onClick={zoomOut} disabled={zoom() <= ZOOM_MIN + 1e-6}>
            −
          </button>
          <button class="book-editor-zoom-btn book-editor-zoom-pct" title="Reset zoom" onClick={zoomReset}>
            {Math.round(zoom() * 100)}%
          </button>
          <button class="book-editor-zoom-btn" title="Zoom in" onClick={zoomIn} disabled={zoom() >= ZOOM_MAX - 1e-6}>
            +
          </button>
        </div>
      </div>
      <Show when={error()}>
        <div class="book-editor-error">{error()}</div>
      </Show>
      <Show when={showImage()}>
        <div class="book-editor-modal-backdrop" onClick={() => setShowImage(false)}>
          <div class="book-editor-modal" onClick={(e) => e.stopPropagation()}>
            <div class="bem-title">Insert image</div>
            <label class="bem-field">
              <span>Path (relative to the manuscript)</span>
              <input
                value={imgPath()}
                onInput={(e) => setImgPath(e.currentTarget.value)}
                placeholder="images/diagram.png"
              />
            </label>
            <label class="bem-field">
              <span>Caption</span>
              <input
                value={imgCaption()}
                onInput={(e) => setImgCaption(e.currentTarget.value)}
                placeholder="Caption (also used in the List of Figures)"
              />
            </label>
            <label class="bem-field">
              <span>Layout</span>
              <select value={imgMode()} onChange={(e) => setImgMode(e.currentTarget.value)}>
                <option value="inline">Inline block — centered, sized</option>
                <option value="text">Text block — full text width</option>
                <option value="bleed">Near-bleed — escapes the margins</option>
                <option value="float-left">Float left — text wraps</option>
                <option value="float-right">Float right — text wraps</option>
              </select>
            </label>
            <Show when={["inline", "float-left", "float-right"].includes(imgMode())}>
              <label class="bem-field">
                <span>Width</span>
                <input
                  value={imgWidth()}
                  onInput={(e) => setImgWidth(e.currentTarget.value)}
                  placeholder="60%"
                />
              </label>
            </Show>
            <Show when={imgMode() === "bleed"}>
              <label class="bem-field">
                <span>Inset from edge (0 = full bleed)</span>
                <input
                  value={imgInset()}
                  onInput={(e) => setImgInset(e.currentTarget.value)}
                  placeholder="0.25in"
                />
              </label>
            </Show>
            <div class="bem-actions">
              <button onClick={() => setShowImage(false)}>Cancel</button>
              <button class="bem-primary" onClick={insertImage}>
                Insert
              </button>
            </div>
          </div>
        </div>
      </Show>
      <div class="book-editor-host" style={{ "--cm-zoom": String(zoom()) }}>
        <MarkdownEditor
          content={content()}
          onChange={handleChange}
          onSave={() => void flushSave()}
          onEditorView={(v) => {
            view = v;
            // Expose the scroll element + H1 anchors so the parent can
            // mirror scrolling with the Pages preview.
            props.onScrollSurfaceReady?.({ el: v.scrollDOM, getAnchors });
          }}
        />
      </div>
    </div>
  );
};

export default BookEditorPane;
