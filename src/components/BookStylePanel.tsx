import { Component, createSignal, Show } from "solid-js";
import { typesetterService } from "../services/typesetter-service";
import "./BookStylePanel.css";

/**
 * Custom-CSS editor for a book: describe a formatting change in plain
 * language, the AI fills/updates an editable CSS box (it sees the current
 * CSS and the document's class vocabulary), and Save writes custom.css
 * next to book.toml — which both the preview and the PDF export pick up.
 */
interface BookStylePanelProps {
  bookPath: string;
  /** The book's current custom.css (already loaded by the parent). */
  currentCss: string;
  /** Document kind — selects which selector vocabulary the AI is told
   *  about (book chapters/TOC vs the flat resume/custom elements). */
  docType?: "book" | "resume" | "custom";
  /** Persist the edited CSS and apply it (re-paginate). */
  onApply: (css: string) => void | Promise<void>;
  onClose: () => void;
}

const BookStylePanel: Component<BookStylePanelProps> = (props) => {
  const [draft, setDraft] = createSignal(props.currentCss);
  const [prompt, setPrompt] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const generate = async () => {
    const desc = prompt().trim();
    if (!desc || busy()) return;
    setBusy(true);
    setError(null);
    try {
      const css = await typesetterService.generateCustomCss(desc, draft(), props.docType);
      setDraft(css);
      setPrompt("");
    } catch (e) {
      setError(`Generation failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      await props.onApply(draft());
    } catch (e) {
      setError(`Save failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="book-style-backdrop" onClick={props.onClose}>
      <div class="book-style-panel" onClick={(e) => e.stopPropagation()}>
        <div class="book-style-header">
          <span class="book-style-title">Custom styling</span>
          <button class="book-style-x" onClick={props.onClose} title="Close">
            ✕
          </button>
        </div>

        <label class="book-style-field">
          <span>Describe the change</span>
          <textarea
            class="book-style-prompt"
            rows={2}
            placeholder="e.g. make chapter titles 1.5x bigger and add a thin rule under them; indent block quotes more"
            value={prompt()}
            onInput={(e) => setPrompt(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                void generate();
              }
            }}
          />
        </label>
        <div class="book-style-gen-row">
          <span class="book-style-hint">Ctrl/⌘+Enter to generate</span>
          <button class="book-style-gen" onClick={generate} disabled={busy() || !prompt().trim()}>
            {busy() ? "Generating…" : "✨ Generate CSS"}
          </button>
        </div>

        <label class="book-style-field book-style-css-field">
          <span>custom.css — edit freely</span>
          <textarea
            class="book-style-css"
            spellcheck={false}
            placeholder="/* Your CSS overrides the generated styling. */"
            value={draft()}
            onInput={(e) => setDraft(e.currentTarget.value)}
          />
        </label>

        <Show when={error()}>
          <div class="book-style-error">{error()}</div>
        </Show>

        <div class="book-style-actions">
          <button onClick={props.onClose}>Cancel</button>
          <button class="book-style-primary" onClick={save} disabled={saving()}>
            {saving() ? "Applying…" : "Save & apply"}
          </button>
        </div>
      </div>
    </div>
  );
};

export default BookStylePanel;
