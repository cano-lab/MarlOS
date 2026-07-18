import { Component, createEffect, createSignal, For, Show } from "solid-js";
import { typesetterService } from "../services/typesetter-service";
import { RESUME_TEMPLATES } from "../typesetter/resume-templates";
import { PROPOSAL_TEMPLATES, isProposalTemplate } from "../typesetter/proposal-templates";
import "./BookStylePanel.css";

/**
 * Conversational CSS styling for a document. Describe a change in plain
 * language; the AI writes/updates custom.css, which is applied to the live
 * preview immediately. Each turn builds on the current stylesheet, so you
 * can keep refining ("make the sidebar narrower", "tighten the spacing")
 * and watch the Pages preview update. The full conversation stays visible
 * as history. A raw-CSS editor is available for manual tweaks.
 *
 * Right-docked drawer (no blocking backdrop) so the preview stays visible
 * and interactive while you chat.
 */
interface BookStylePanelProps {
  bookPath: string;
  /** The document's current custom.css (already loaded by the parent). */
  currentCss: string;
  /** Document kind — selects the AI's selector vocabulary (book vs resume). */
  docType?: "book" | "resume" | "custom";
  /** Optional template / subtype identifier (e.g. a proposal template). */
  templateId?: string;
  /** Persist the CSS and apply it to the preview. Does NOT close the panel. */
  onApply: (css: string) => void | Promise<void>;
  onClose: () => void;
}

interface ChatMsg {
  id: number;
  role: "user" | "assistant";
  text: string;
  /** For assistant turns: the CSS that was applied. */
  css?: string;
  error?: boolean;
}

/** localStorage key for a document's style-chat history. */
const historyKey = (bookPath: string) => `marlos-style-chat:${bookPath}`;
const MAX_STORED = 60;

const BookStylePanel: Component<BookStylePanelProps> = (props) => {
  const isProposal = () => props.templateId && isProposalTemplate(props.templateId);
  const isResume = () => props.docType === "resume";
  const isFlat = () => props.docType && props.docType !== "book";

  const templates = () => {
    if (isProposal()) return PROPOSAL_TEMPLATES;
    if (isFlat()) return RESUME_TEMPLATES;
    return [];
  };

  const badgeLabel = () => {
    if (isProposal()) return "proposal";
    return props.docType ?? "custom";
  };

  const introText = () => {
    if (isProposal()) {
      return "Describe how you want the proposal to look — e.g. “tight budget table with right-aligned numbers”, “make the specific aims section stand out with a left rule”, or “use a two-column layout for the concept note”. I'll write the CSS and apply it to the preview.";
    }
    if (isResume()) {
      return "Describe how you want the resume to look — e.g. “two columns with a skills sidebar, name 24pt bold, section headings in small caps”. I'll write the CSS and apply it to the preview. Keep refining and I'll build on it.";
    }
    return "Describe a styling change — e.g. “make chapter titles bigger with a thin rule under them”. I'll update custom.css and apply it to the preview. Keep refining and I'll build on it.";
  };

  const loadHistory = (): ChatMsg[] => {
    try {
      const raw = localStorage.getItem(historyKey(props.bookPath));
      if (raw) {
        const arr = JSON.parse(raw);
        if (Array.isArray(arr) && arr.length) return arr as ChatMsg[];
      }
    } catch {
      /* ignore corrupt/blocked storage */
    }
    return [{ id: 0, role: "assistant", text: introText() }];
  };

  const initial = loadHistory();
  // Per-instance id counter, seeded past any restored ids so keys stay unique.
  let idCounter = initial.reduce((mx, m) => Math.max(mx, m.id), 0) + 1;
  const [messages, setMessages] = createSignal<ChatMsg[]>(initial);

  // Persist (capped) so the history survives closing the panel to export,
  // then reopening. Keyed per document path.
  createEffect(() => {
    try {
      localStorage.setItem(
        historyKey(props.bookPath),
        JSON.stringify(messages().slice(-MAX_STORED)),
      );
    } catch {
      /* ignore quota/blocked storage */
    }
  });

  const clearHistory = () => {
    setMessages([{ id: idCounter++, role: "assistant", text: introText() }]);
  };

  const [input, setInput] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [draftCss, setDraftCss] = createSignal(props.currentCss);
  const [showEditor, setShowEditor] = createSignal(false);

  let listRef: HTMLDivElement | undefined;
  const scrollToBottom = () =>
    queueMicrotask(() => listRef?.scrollTo({ top: listRef.scrollHeight, behavior: "smooth" }));

  const push = (m: Omit<ChatMsg, "id">) => {
    setMessages((prev) => [...prev, { ...m, id: idCounter++ }]);
    scrollToBottom();
  };

  const send = async () => {
    const text = input().trim();
    if (!text || busy()) return;
    setInput("");
    push({ role: "user", text });
    setBusy(true);
    try {
      // Each turn sees the cumulative CSS, so refinements build on each
      // other reliably regardless of conversation length.
      const css = await typesetterService.generateCustomCss(
        text,
        draftCss(),
        props.docType,
        props.templateId,
      );
      setDraftCss(css);
      await props.onApply(css); // persist + repaginate the preview (stays open)
      push({ role: "assistant", text: "Applied to the preview.", css });
    } catch (e) {
      push({
        role: "assistant",
        text: `Generation failed: ${e instanceof Error ? e.message : String(e)}`,
        error: true,
      });
    } finally {
      setBusy(false);
    }
  };

  const applyTemplate = async (id: string) => {
    if (busy()) return;
    const tpl = templates().find((t) => t.id === id);
    if (!tpl) return;
    push({ role: "user", text: `Start from the “${tpl.name}” template.` });
    setBusy(true);
    try {
      setDraftCss(tpl.css);
      await props.onApply(tpl.css);
      push({
        role: "assistant",
        text: `Applied the “${tpl.name}” template — ${tpl.description} Ask me to tweak it from here.`,
        css: tpl.css,
      });
    } catch (e) {
      push({
        role: "assistant",
        text: `Couldn't apply template: ${e instanceof Error ? e.message : String(e)}`,
        error: true,
      });
    } finally {
      setBusy(false);
    }
  };

  const applyEditedCss = async () => {
    setBusy(true);
    try {
      await props.onApply(draftCss());
      push({ role: "assistant", text: "Applied your manual CSS edits to the preview." });
    } catch (e) {
      push({
        role: "assistant",
        text: `Apply failed: ${e instanceof Error ? e.message : String(e)}`,
        error: true,
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="style-chat">
      <div class="style-chat-header">
        <span class="style-chat-title">
          🎨 Style chat
          <Show when={isFlat()}>
            <span class="style-chat-badge">{badgeLabel()}</span>
          </Show>
        </span>
        <div class="style-chat-header-actions">
          <button
            class="style-chat-clear"
            onClick={clearHistory}
            title="Clear chat history"
            disabled={busy()}
          >
            Clear
          </button>
          <button class="style-chat-x" onClick={props.onClose} title="Close">
            ✕
          </button>
        </div>
      </div>

      <Show when={isFlat()}>
        <div class="style-chat-templates">
          <span class="style-chat-templates-label">Start from:</span>
          <For each={templates()}>
            {(tpl) => (
              <button
                class="style-chat-tpl"
                title={tpl.description}
                disabled={busy()}
                onClick={() => void applyTemplate(tpl.id)}
              >
                {tpl.name}
              </button>
            )}
          </For>
        </div>
      </Show>

      <div class="style-chat-msgs" ref={listRef}>
        <For each={messages()}>
          {(m) => (
            <div
              classList={{
                "style-msg": true,
                "style-msg-user": m.role === "user",
                "style-msg-assistant": m.role === "assistant",
                "style-msg-error": !!m.error,
              }}
            >
              <div class="style-msg-text">{m.text}</div>
              <Show when={m.css}>
                <details class="style-msg-css">
                  <summary>View CSS</summary>
                  <pre>{m.css}</pre>
                </details>
              </Show>
            </div>
          )}
        </For>
        <Show when={busy()}>
          <div class="style-msg style-msg-assistant style-msg-pending">
            <span class="style-typing">
              <i /><i /><i />
            </span>
            <span>Writing CSS…</span>
          </div>
        </Show>
      </div>

      <div class="style-chat-input">
        <textarea
          rows={2}
          placeholder="Describe a change… (Enter to send, Shift+Enter for a newline)"
          value={input()}
          disabled={busy()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <button
          class="style-chat-send"
          onClick={() => void send()}
          disabled={busy() || !input().trim()}
        >
          {busy() ? "…" : "Send"}
        </button>
      </div>

      <div class="style-chat-foot">
        <button class="style-chat-link" onClick={() => setShowEditor(!showEditor())}>
          {showEditor() ? "Hide CSS editor" : "Edit CSS directly"}
        </button>
        <span class="style-chat-hint">Changes apply to the preview live.</span>
      </div>

      <Show when={showEditor()}>
        <div class="style-chat-editor">
          <textarea
            class="style-chat-css"
            spellcheck={false}
            value={draftCss()}
            onInput={(e) => setDraftCss(e.currentTarget.value)}
            placeholder="/* custom.css — edit freely, then Apply */"
          />
          <button
            class="style-chat-send"
            onClick={() => void applyEditedCss()}
            disabled={busy()}
          >
            Apply edits
          </button>
        </div>
      </Show>
    </div>
  );
};

export default BookStylePanel;
