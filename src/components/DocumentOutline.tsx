import { Component, createMemo, Show, For } from "solid-js";
import { EditorView } from "@codemirror/view";
import "./DocumentOutline.css";

interface HeadingItem {
  level: number;
  text: string;
  line: number;
}

interface DocumentOutlineProps {
  content: string;
  editorView?: EditorView;
  onClose: () => void;
}

const DocumentOutline: Component<DocumentOutlineProps> = (props) => {
  const headings = createMemo<HeadingItem[]>(() => {
    const lines = props.content.split("\n");
    const result: HeadingItem[] = [];
    let inCodeBlock = false;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i].trim();

      // Track code blocks to avoid matching headings inside them
      if (line.startsWith("```")) {
        inCodeBlock = !inCodeBlock;
        continue;
      }
      if (inCodeBlock) continue;

      const match = line.match(/^(#{1,6})\s+(.+)$/);
      if (match) {
        result.push({
          level: match[1].length,
          text: match[2].replace(/\*\*|__|~~|`/g, ""), // strip inline formatting
          line: i + 1, // 1-based
        });
      }
    }
    return result;
  });

  const scrollToLine = (lineNumber: number) => {
    const view = props.editorView;
    if (!view) return;

    const line = view.state.doc.line(Math.min(lineNumber, view.state.doc.lines));
    view.dispatch({
      selection: { anchor: line.from },
      effects: EditorView.scrollIntoView(line.from, { y: "start", yMargin: 50 }),
    });
    view.focus();
  };

  // Find the minimum heading level to normalize indentation
  const minLevel = createMemo(() => {
    const h = headings();
    return h.length > 0 ? Math.min(...h.map(h => h.level)) : 1;
  });

  return (
    <div class="document-outline">
      <div class="outline-header">
        <span class="outline-title">Outline</span>
        <button class="outline-close" onClick={props.onClose} title="Close outline">x</button>
      </div>
      <Show
        when={headings().length > 0}
        fallback={<div class="outline-empty">No headings found</div>}
      >
        <nav class="outline-nav">
          <For each={headings()}>
            {(heading) => (
              <button
                class={`outline-item outline-level-${heading.level}`}
                style={{ "padding-left": `${(heading.level - minLevel() + 1) * 12}px` }}
                onClick={() => scrollToLine(heading.line)}
                title={`Line ${heading.line}`}
              >
                <span class="outline-marker">{"#".repeat(heading.level)}</span>
                <span class="outline-text">{heading.text}</span>
              </button>
            )}
          </For>
        </nav>
      </Show>
    </div>
  );
};

export default DocumentOutline;
