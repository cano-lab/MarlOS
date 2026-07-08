import { Component, createSignal, For, Show } from "solid-js";
import { typesetterService } from "../services/typesetter-service";
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

let msgSeq = 0;

const BookStylePanel: Component<BookStylePanelProps> = (props) => {
  const [messages, setMessages] = createSignal<ChatMsg[]>([
    {
      id: msgSeq++,
      role: "assistant",
      text:
        props.docType && props.docType !== "book"
          ? "Describe how you want the resume to look — e.g. “two columns with a skills sidebar, name 24pt bold, section headings in small caps”. I'll write the CSS and apply it to the preview. Keep refining and I'll build on it."
          : "Describe a styling change — e.g. “make chapter titles bigger with a thin rule under them”. I'll update custom.css and apply it to the preview. Keep refining and I'll build on it.",
    },
  ]);
  const [input, setInput] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [draftCss, setDraftCss] = createSignal(props.currentCss);
  const [showEditor, setShowEditor] = createSignal(false);

  let listRef: HTMLDivElement | undefined;
  const scrollToBottom = () =>
    queueMicrotask(() => listRef?.scrollTo({ top: listRef.scrollHeight, behavior: "smooth" }));

  const push = (m: Omit<ChatMsg, "id">) => {
    setMessages((prev) => [...prev, { ...m, id: msgSeq++ }]);
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
          <Show when={props.docType && props.docType !== "book"}>
            <span class="style-chat-badge">{props.docType}</span>
          </Show>
        </span>
        <button class="style-chat-x" onClick={props.onClose} title="Close">
          ✕
        </button>
      </div>

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
