import { Component, createSignal, createEffect, onCleanup, For, Show } from "solid-js";
import type { EditorView } from "@codemirror/view";
import MarkdownEditor from "./MarkdownEditor";
import { typesetterService, BookConfig, ScrollSurface } from "../services/typesetter-service";
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
        </div>
      </div>
      <Show when={error()}>
        <div class="book-editor-error">{error()}</div>
      </Show>
      <div class="book-editor-host">
        <MarkdownEditor
          content={content()}
          onChange={handleChange}
          onSave={() => void flushSave()}
          onEditorView={(v) => {
            view = v;
            // Expose the scroll element so the parent can mirror scroll.
            // getAnchors is stubbed until H1-anchor sync lands.
            props.onScrollSurfaceReady?.({ el: v.scrollDOM, getAnchors: () => [] });
          }}
        />
      </div>
    </div>
  );
};

export default BookEditorPane;
