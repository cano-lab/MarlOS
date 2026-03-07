import { Component, createEffect, onMount, onCleanup, createSignal } from "solid-js";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter, highlightSpecialChars, drawSelection, dropCursor, rectangularSelection, crosshairCursor, highlightActiveLine } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching, foldGutter, indentOnInput } from "@codemirror/language";
import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import { autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { inlineAI } from "./inline-ai-extension";
import "./MarkdownEditor.css";

interface MarkdownEditorProps {
  content: string;
  onChange: (content: string) => void;
  onSave: () => void;
}

// Dark theme for CodeMirror
const darkTheme = EditorView.theme({
  "&": {
    backgroundColor: "#1e1e1e",
    color: "#d4d4d4",
    height: "100%",
  },
  ".cm-content": {
    fontFamily: "'JetBrains Mono', 'Fira Code', 'Consolas', monospace",
    fontSize: "14px",
    lineHeight: "1.6",
    caretColor: "#569cd6",
  },
  ".cm-cursor": {
    borderLeftColor: "#569cd6",
  },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
    backgroundColor: "rgba(86, 156, 214, 0.3)",
  },
  ".cm-activeLine": {
    backgroundColor: "rgba(255, 255, 255, 0.05)",
  },
  ".cm-gutters": {
    backgroundColor: "#252526",
    color: "#858585",
    border: "none",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "rgba(255, 255, 255, 0.05)",
  },
  ".cm-lineNumbers .cm-gutterElement": {
    padding: "0 8px",
  },
  // Markdown syntax highlighting
  ".cm-header-1": { color: "#569cd6", fontSize: "1.6em", fontWeight: "bold" },
  ".cm-header-2": { color: "#4ec9b0", fontSize: "1.4em", fontWeight: "bold" },
  ".cm-header-3": { color: "#9cdcfe", fontSize: "1.2em", fontWeight: "bold" },
  ".cm-header-4": { color: "#dcdcaa", fontSize: "1.1em", fontWeight: "bold" },
  ".cm-header-5": { color: "#c586c0", fontSize: "1em", fontWeight: "bold" },
  ".cm-header-6": { color: "#ce9178", fontSize: "1em", fontWeight: "bold" },
  ".cm-link": { color: "#569cd6", textDecoration: "underline" },
  ".cm-url": { color: "#6a9955" },
  ".cm-emphasis": { fontStyle: "italic", color: "#dcdcaa" },
  ".cm-strong": { fontWeight: "bold", color: "#d7ba7d" },
  ".cm-strikethrough": { textDecoration: "line-through" },
  ".cm-code": { color: "#ce9178", backgroundColor: "rgba(255, 255, 255, 0.1)" },
  ".cm-quote": { color: "#6a9955", fontStyle: "italic" },
  ".cm-list": { color: "#569cd6" },
});

const MarkdownEditor: Component<MarkdownEditorProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let editorView: EditorView | undefined;
  const [wordCount, setWordCount] = createSignal(0);
  const [lineCount, setLineCount] = createSignal(0);
  const [cursorPos, setCursorPos] = createSignal({ line: 1, col: 1 });

  const countWords = (text: string) => {
    return text.trim().split(/\s+/).filter(w => w.length > 0).length;
  };

  onMount(() => {
    if (!containerRef) return;

    const updateListener = EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        const content = update.state.doc.toString();
        props.onChange(content);
        setWordCount(countWords(content));
        setLineCount(update.state.doc.lines);
      }
      if (update.selectionSet) {
        const pos = update.state.selection.main.head;
        const line = update.state.doc.lineAt(pos);
        setCursorPos({ line: line.number, col: pos - line.from + 1 });
      }
    });

    const saveKeymap = keymap.of([
      {
        key: "Mod-s",
        run: () => {
          props.onSave();
          return true;
        },
      },
    ]);

    const state = EditorState.create({
      doc: props.content,
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightSpecialChars(),
        history(),
        foldGutter(),
        drawSelection(),
        dropCursor(),
        EditorState.allowMultipleSelections.of(true),
        indentOnInput(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        bracketMatching(),
        closeBrackets(),
        autocompletion(),
        rectangularSelection(),
        crosshairCursor(),
        highlightActiveLine(),
        highlightSelectionMatches(),
        keymap.of([
          ...closeBracketsKeymap,
          ...defaultKeymap,
          ...searchKeymap,
          ...historyKeymap,
          ...completionKeymap,
          indentWithTab,
        ]),
        saveKeymap,
        markdown({ base: markdownLanguage }),
        darkTheme,
        updateListener,
        EditorView.lineWrapping,
        inlineAI(), // Add inline AI ghost text
      ],
    });

    editorView = new EditorView({
      state,
      parent: containerRef,
    });

    // Initial counts
    setWordCount(countWords(props.content));
    setLineCount(state.doc.lines);

    editorView.focus();
  });

  // Update content when props change externally
  createEffect(() => {
    if (editorView && props.content !== editorView.state.doc.toString()) {
      editorView.dispatch({
        changes: {
          from: 0,
          to: editorView.state.doc.length,
          insert: props.content,
        },
      });
    }
  });

  onCleanup(() => {
    editorView?.destroy();
  });

  return (
    <div class="markdown-editor">
      <div class="editor-toolbar">
        <div class="toolbar-group">
          <button class="toolbar-btn toolbar-btn-save" title="Save (Ctrl+S)" onClick={props.onSave}>
            💾 Save
          </button>
        </div>
        <div class="toolbar-separator" />
        <div class="toolbar-group">
          <button class="toolbar-btn" title="Bold (Ctrl+B)" onClick={() => insertMarkdown("**", "**")}>
            <strong>B</strong>
          </button>
          <button class="toolbar-btn" title="Italic (Ctrl+I)" onClick={() => insertMarkdown("*", "*")}>
            <em>I</em>
          </button>
          <button class="toolbar-btn" title="Strikethrough" onClick={() => insertMarkdown("~~", "~~")}>
            <s>S</s>
          </button>
          <button class="toolbar-btn" title="Code" onClick={() => insertMarkdown("`", "`")}>
            {"</>"}
          </button>
        </div>
        <div class="toolbar-separator" />
        <div class="toolbar-group">
          <button class="toolbar-btn" title="Heading 1" onClick={() => insertAtLineStart("# ")}>H1</button>
          <button class="toolbar-btn" title="Heading 2" onClick={() => insertAtLineStart("## ")}>H2</button>
          <button class="toolbar-btn" title="Heading 3" onClick={() => insertAtLineStart("### ")}>H3</button>
        </div>
        <div class="toolbar-separator" />
        <div class="toolbar-group">
          <button class="toolbar-btn" title="Bullet List" onClick={() => insertAtLineStart("- ")}>•</button>
          <button class="toolbar-btn" title="Numbered List" onClick={() => insertAtLineStart("1. ")}>1.</button>
          <button class="toolbar-btn" title="Task List" onClick={() => insertAtLineStart("- [ ] ")}>☑</button>
          <button class="toolbar-btn" title="Quote" onClick={() => insertAtLineStart("> ")}>❝</button>
        </div>
        <div class="toolbar-separator" />
        <div class="toolbar-group">
          <button class="toolbar-btn" title="Link" onClick={() => insertMarkdown("[", "](url)")}>🔗</button>
          <button class="toolbar-btn" title="Image" onClick={() => insertMarkdown("![", "](url)")}>🖼</button>
          <button class="toolbar-btn" title="Table" onClick={insertTable}>📊</button>
        </div>
        <div class="toolbar-separator" />
        <div class="toolbar-group">
          <button class="toolbar-btn" title="Mermaid Diagram" onClick={insertMermaid}>◇</button>
          <button class="toolbar-btn" title="Math (LaTeX)" onClick={insertMath}>∑</button>
          <button class="toolbar-btn" title="Chart" onClick={insertChart}>📈</button>
        </div>
      </div>
      <div class="editor-container" ref={containerRef} />
      <div class="editor-status">
        <span>Ln {cursorPos().line}, Col {cursorPos().col}</span>
        <span>{lineCount()} lines</span>
        <span>{wordCount()} words</span>
        <span>Markdown</span>
      </div>
    </div>
  );

  function insertMarkdown(before: string, after: string) {
    if (!editorView) return;
    const { from, to } = editorView.state.selection.main;
    const selected = editorView.state.sliceDoc(from, to);
    editorView.dispatch({
      changes: { from, to, insert: before + selected + after },
      selection: { anchor: from + before.length, head: from + before.length + selected.length },
    });
    editorView.focus();
  }

  function insertAtLineStart(prefix: string) {
    if (!editorView) return;
    const { from } = editorView.state.selection.main;
    const line = editorView.state.doc.lineAt(from);
    editorView.dispatch({
      changes: { from: line.from, insert: prefix },
      selection: { anchor: line.from + prefix.length },
    });
    editorView.focus();
  }

  function insertTable() {
    const table = `
| Header 1 | Header 2 | Header 3 |
|----------|----------|----------|
| Cell 1   | Cell 2   | Cell 3   |
| Cell 4   | Cell 5   | Cell 6   |
`;
    insertAtCursor(table);
  }

  function insertMermaid() {
    const diagram = `
\`\`\`mermaid
graph TD
    A[Start] --> B{Decision}
    B -->|Yes| C[Action 1]
    B -->|No| D[Action 2]
    C --> E[End]
    D --> E
\`\`\`
`;
    insertAtCursor(diagram);
  }

  function insertMath() {
    const math = `
$$
\\int_{-\\infty}^{\\infty} e^{-x^2} dx = \\sqrt{\\pi}
$$
`;
    insertAtCursor(math);
  }

  function insertChart() {
    const chart = `
\`\`\`chart
type: bar
data:
  labels: [Jan, Feb, Mar, Apr, May]
  datasets:
    - label: Sales
      data: [12, 19, 3, 5, 2]
      backgroundColor: rgba(86, 156, 214, 0.5)
\`\`\`
`;
    insertAtCursor(chart);
  }

  function insertAtCursor(text: string) {
    if (!editorView) return;
    const { from } = editorView.state.selection.main;
    editorView.dispatch({
      changes: { from, insert: text },
      selection: { anchor: from + text.length },
    });
    editorView.focus();
  }
};

export default MarkdownEditor;
