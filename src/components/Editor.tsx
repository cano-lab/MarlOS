import { Component, onMount } from "solid-js";
import "./Editor.css";

interface EditorProps {
  content: string;
  onChange: (content: string) => void;
  onSave: () => void;
}

const Editor: Component<EditorProps> = (props) => {
  let textareaRef: HTMLTextAreaElement | undefined;

  onMount(() => {
    if (textareaRef) {
      textareaRef.focus();
    }
  });

  const handleKeyDown = (e: KeyboardEvent) => {
    // Ctrl/Cmd + S to save
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      props.onSave();
    }

    // Tab handling for indentation
    if (e.key === "Tab" && textareaRef) {
      e.preventDefault();
      const start = textareaRef.selectionStart;
      const end = textareaRef.selectionEnd;
      const value = textareaRef.value;

      // Insert tab at cursor
      const newValue = value.substring(0, start) + "  " + value.substring(end);
      props.onChange(newValue);

      // Restore cursor position
      requestAnimationFrame(() => {
        if (textareaRef) {
          textareaRef.selectionStart = textareaRef.selectionEnd = start + 2;
        }
      });
    }
  };

  return (
    <div class="editor">
      <textarea
        ref={textareaRef}
        class="editor-textarea"
        value={props.content}
        onInput={(e) => props.onChange(e.currentTarget.value)}
        onKeyDown={handleKeyDown}
        placeholder="Start writing..."
        spellcheck={false}
      />
    </div>
  );
};

export default Editor;
