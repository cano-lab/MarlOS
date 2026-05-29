import { Component, createSignal, createEffect, onCleanup, onMount, For, Show } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { open } from "@tauri-apps/plugin-dialog";
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
  /** Fires on every keystroke / doc change. Used by the parent to know
   *  when the user is actively typing (and suppress scroll-mirror noise
   *  from CodeMirror's keep-cursor-in-view auto-scroll). */
  onEdit?: () => void;
  /** Hands an imperative API to the parent once mounted, so BookMode can
   *  drive source edits triggered from the Pages preview (e.g. the
   *  right-click figure margin editor). */
  onReady?: (api: BookEditorApi) => void;
}

/** Patch applied to a figure's markdown attribute block. */
export interface FigurePatch {
  /** "inline" | "text" | "bleed" | "full-page" */
  mode?: string;
  /** Bleed inset, e.g. "0.25in" (only meaningful when mode === "bleed"). */
  inset?: string;
}

export interface BookEditorApi {
  /** Rewrite the layout class + `--fig-inset` of the figure whose markdown
   *  id matches `domId` (the pandoc-prefixed DOM id is mapped back to the
   *  `#fig-…` source id). Returns false if the figure isn't in the active
   *  file. Saves immediately so the preview re-paginates. */
  updateFigureAttr: (domId: string, patch: FigurePatch) => boolean;
}

/** Pandoc figure-layout classes (one at a time). Other classes — e.g.
 *  fig-crop — are preserved when the layout changes. */
const LAYOUT_CLASSES = ["fig-text", "fig-bleed", "fig-fullpage", "fig-float-left", "fig-float-right"];

/** Tokenize a pandoc attribute block (`#id .class key="value with spaces"`)
 *  into whitespace-separated tokens, keeping quoted values intact. */
function tokenizeAttrs(s: string): string[] {
  const toks: string[] = [];
  let i = 0;
  while (i < s.length) {
    while (i < s.length && /\s/.test(s[i])) i++;
    if (i >= s.length) break;
    let tok = "";
    while (i < s.length && !/\s/.test(s[i])) {
      if (s[i] === '"') {
        tok += s[i++];
        while (i < s.length && s[i] !== '"') tok += s[i++];
        if (i < s.length) tok += s[i++]; // closing quote
      } else {
        tok += s[i++];
      }
    }
    toks.push(tok);
  }
  return toks;
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
  {
    label: "ƒ Math anchor",
    title: "Math Anchor — boxed blockquote with a centered display equation",
    snippet:
      "> **Math Anchor — Title**: Lead-in sentence describing what the equation is.\n" +
      ">\n" +
      "> $$equation$$\n" +
      ">\n" +
      "> Explanation: define the variables, say why the equation matters, what it means in plain language.",
  },
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
    props.onEdit?.();
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

  /** Rebuild a figure's attribute block from a patch: swap the layout
   *  class and set/clear `--fig-inset`, preserving id, width, other
   *  classes (fig-crop), and unrelated style props. */
  const applyFigurePatch = (attrs: string, patch: FigurePatch): string => {
    let id: string | undefined;
    const classes: string[] = [];
    const kv: [string, string][] = [];
    for (const t of tokenizeAttrs(attrs)) {
      if (t.startsWith("#")) id = t.slice(1);
      else if (t.startsWith(".")) classes.push(t.slice(1));
      else {
        const eq = t.indexOf("=");
        if (eq > 0) kv.push([t.slice(0, eq), t.slice(eq + 1)]);
      }
    }
    // Layout class: drop the old one, add the new (inline = none).
    const others = classes.filter((c) => !LAYOUT_CLASSES.includes(c));
    const layout =
      patch.mode === "text"
        ? ["fig-text"]
        : patch.mode === "bleed"
          ? ["fig-bleed"]
          : patch.mode === "full-page"
            ? ["fig-fullpage"]
            : [];
    const newClasses = [...others, ...layout];

    // Style custom properties (parse "k:v; k:v" inside the quoted value).
    const styleIdx = kv.findIndex(([k]) => k === "style");
    const rawStyle = styleIdx >= 0 ? kv[styleIdx][1].replace(/^"|"$/g, "") : "";
    const props = new Map<string, string>();
    for (const decl of rawStyle.split(";")) {
      const c = decl.indexOf(":");
      if (c > 0) props.set(decl.slice(0, c).trim(), decl.slice(c + 1).trim());
    }
    if (patch.mode === "bleed" && patch.inset != null) {
      props.set("--fig-inset", patch.inset);
    } else if (patch.mode !== "bleed") {
      props.delete("--fig-inset");
    }
    const newStyle = [...props].map(([k, v]) => `${k}:${v}`).join("; ");
    if (newStyle) {
      const entry: [string, string] = ["style", `"${newStyle}"`];
      if (styleIdx >= 0) kv[styleIdx] = entry;
      else kv.push(entry);
    } else if (styleIdx >= 0) {
      kv.splice(styleIdx, 1);
    }

    const parts: string[] = [];
    if (id) parts.push(`#${id}`);
    for (const c of newClasses) parts.push(`.${c}`);
    for (const [k, v] of kv) parts.push(`${k}=${v}`);
    return parts.join(" ");
  };

  onMount(() => {
    props.onReady?.({ updateFigureAttr });
  });

  const updateFigureAttr = (domId: string, patch: FigurePatch): boolean => {
    if (!view) return false;
    // DOM ids are pandoc-prefixed with the id-prefix "ch"; the markdown
    // source id is the rest (e.g. "chfig-abc" → "fig-abc").
    const mdId = domId.replace(/^ch/, "");
    const text = view.state.doc.toString();
    // Each image-with-attrs: ![alt](path){ …attrs… }
    const re = /!\[[^\]]*\]\([^)]*\)\{([^}]*)\}/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
      const attrs = m[1];
      if (!tokenizeAttrs(attrs).includes(`#${mdId}`)) continue;
      const from = m.index + m[0].indexOf("{") + 1;
      const to = from + attrs.length;
      view.dispatch({
        changes: { from, to, insert: applyFigurePatch(attrs, patch) },
      });
      handleChange(view.state.doc.toString());
      void flushSave(); // save now so the preview re-paginates promptly
      return true;
    }
    setError(`Couldn't find figure ${mdId} in this file to update.`);
    return false;
  };

  // --- Image insert modal -------------------------------------------------
  const [showImage, setShowImage] = createSignal(false);
  const [imgPath, setImgPath] = createSignal("");
  const [imgCaption, setImgCaption] = createSignal("");
  // inline | text | bleed | full-page | float-left | float-right
  const [imgMode, setImgMode] = createSignal("inline");
  const [imgWidth, setImgWidth] = createSignal("60%");
  const [imgInset, setImgInset] = createSignal("0.25in");
  // Full-page fill: cover (fill+crop) vs contain (whole image, letterbox).
  const [imgFit, setImgFit] = createSignal("cover");
  // Crop: a fixed aspect ratio (blank = no crop) + focal point. The image
  // fills the cropped box and pans to the focal point. Works on any mode.
  const [imgCropAR, setImgCropAR] = createSignal("");
  const [imgFocal, setImgFocal] = createSignal("center");

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
    const classes: string[] = [];
    const styles: string[] = [];
    let dims = "";
    if (mode === "text") {
      classes.push("fig-text");
    } else if (mode === "bleed") {
      classes.push("fig-bleed");
      styles.push(`--fig-inset:${imgInset().trim() || "0.25in"}`);
    } else if (mode === "full-page") {
      classes.push("fig-fullpage");
      styles.push(`--fig-fit:${imgFit()}`);
    } else if (mode === "float-left") {
      classes.push("fig-float-left");
      dims = ` width="${imgWidth().trim() || "40%"}"`;
    } else if (mode === "float-right") {
      classes.push("fig-float-right");
      dims = ` width="${imgWidth().trim() || "40%"}"`;
    } else {
      dims = ` width="${imgWidth().trim() || "60%"}"`; // inline block
    }
    // Crop to a fixed aspect ratio (any mode except full-page, which
    // already crops to the page via --fig-fit).
    const ar = imgCropAR().trim();
    if (ar && mode !== "full-page") {
      classes.push("fig-crop");
      styles.push(`--fig-crop-ar:${ar}`);
    }
    // Focal point drives crop panning and full-page positioning.
    const focal = imgFocal().trim();
    if (focal && focal !== "center" && ((ar && mode !== "full-page") || mode === "full-page")) {
      styles.push(`--fig-crop-pos:${focal}`);
    }
    let attrs = `#${id}`;
    for (const c of classes) attrs += ` .${c}`;
    attrs += dims;
    if (styles.length) attrs += ` style="${styles.join("; ")}"`;
    // Image alone in a paragraph → pandoc implicit_figures wraps it in a
    // <figure> with the caption as <figcaption>.
    insertAtCursor(`\n\n![${cap}](${path}){${attrs}}\n\n`);
    setShowImage(false);
    setImgPath("");
    setImgCaption("");
  };

  /** Native file picker for the image — no need to remember/type paths.
   *  Stores it relative to the manuscript dir when possible (portable),
   *  else absolute. Seeds the caption from the filename. */
  const browseForImage = async () => {
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [
          { name: "Image", extensions: ["png", "jpg", "jpeg", "gif", "webp", "svg", "tiff", "tif"] },
        ],
      });
      if (typeof picked !== "string") return;
      const dir = props.bookPath.replace(/[\\/][^\\/]+$/, "").replace(/\\/g, "/");
      const abs = picked.replace(/\\/g, "/");
      const relative =
        dir && abs.toLowerCase().startsWith(dir.toLowerCase() + "/")
          ? abs.slice(dir.length + 1)
          : picked;
      setImgPath(relative);
      if (imgCaption().trim() === "") {
        const base = picked.split(/[\\/]/).pop() ?? "";
        setImgCaption(base.replace(/\.[^.]+$/, "").replace(/[-_]+/g, " ").trim());
      }
    } catch (e) {
      setError(`Image picker failed: ${e instanceof Error ? e.message : String(e)}`);
    }
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
              <span>Image file</span>
              <div class="bem-path-row">
                <input
                  value={imgPath()}
                  onInput={(e) => setImgPath(e.currentTarget.value)}
                  placeholder="images/diagram.png"
                />
                <button type="button" class="bem-browse" onClick={browseForImage}>
                  Browse…
                </button>
              </div>
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
                <option value="full-page">Full page — fills the whole page (bleed)</option>
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
            <Show when={imgMode() === "full-page"}>
              <label class="bem-field">
                <span>Fill</span>
                <select value={imgFit()} onChange={(e) => setImgFit(e.currentTarget.value)}>
                  <option value="cover">Cover — fills the page, crops overflow</option>
                  <option value="contain">Contain — whole image, may letterbox</option>
                </select>
              </label>
            </Show>
            <Show when={imgMode() !== "full-page"}>
              <label class="bem-field">
                <span>Crop to aspect ratio (blank = no crop)</span>
                <input
                  value={imgCropAR()}
                  onInput={(e) => setImgCropAR(e.currentTarget.value)}
                  placeholder="e.g. 3/2, 1/1, 16/9"
                />
              </label>
            </Show>
            <Show when={imgCropAR().trim() !== "" || imgMode() === "full-page"}>
              <label class="bem-field">
                <span>Focal point (which part to keep)</span>
                <select value={imgFocal()} onChange={(e) => setImgFocal(e.currentTarget.value)}>
                  <option value="center">Center</option>
                  <option value="top">Top</option>
                  <option value="bottom">Bottom</option>
                  <option value="left">Left</option>
                  <option value="right">Right</option>
                  <option value="top left">Top-left</option>
                  <option value="top right">Top-right</option>
                  <option value="bottom left">Bottom-left</option>
                  <option value="bottom right">Bottom-right</option>
                </select>
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
