import { Component, createSignal, onMount, onCleanup } from "solid-js";
import type { Widget } from "./PlanSpace";
import "./NoteWidget.css";

interface NoteWidgetProps {
  widget: Widget;
  onUpdate: (updates: Partial<Widget>) => void;
}

const NoteWidget: Component<NoteWidgetProps> = (props) => {
  const [isEditing, setIsEditing] = createSignal(false);
  const [content, setContent] = createSignal("");
  const [selectedColor, setSelectedColor] = createSignal<string>("#fef3c7");

  const colors = [
    { name: "yellow", value: "#fef3c7" },
    { name: "blue", value: "#dbeafe" },
    { name: "green", value: "#d1fae5" },
    { name: "pink", value: "#fce7f3" },
    { name: "purple", value: "#e9d5ff" },
    { name: "orange", value: "#fed7aa" },
  ];

  onMount(() => {
    // Initialize content from widget data
    if (props.widget.data.type === "note") {
      setContent(props.widget.data.content || "");
      setSelectedColor(props.widget.data.color || "#fef3c7");
    }
  });

  const handleSave = () => {
    const updates: Partial<Widget> = {
      data: {
        type: "note",
        content: content(),
        color: selectedColor(),
      },
      lastTouched: new Date(),
    };
    props.onUpdate(updates);
    setIsEditing(false);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    // Save on Ctrl+Enter or Cmd+Enter
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      handleSave();
    }
    // Cancel on Escape
    if (e.key === "Escape") {
      setIsEditing(false);
      // Revert content
      if (props.widget.data.type === "note") {
        setContent(props.widget.data.content || "");
      }
    }
  };

  const handleClickOutside = (e: MouseEvent) => {
    const target = e.target as HTMLElement;
    const noteEl = target.closest(".note-widget");
    if (!noteEl && isEditing()) {
      handleSave();
    }
  };

  onMount(() => {
    document.addEventListener("mousedown", handleClickOutside);
  });

  onCleanup(() => {
    document.removeEventListener("mousedown", handleClickOutside);
  });

  return (
    <div
      class="note-widget"
      style={{ background: selectedColor() }}
      onClick={(e) => {
        e.stopPropagation();
        setIsEditing(true);
      }}
      onMouseDown={(e) => {
        // Prevent drag when clicking inside note widget
        e.stopPropagation();
      }}
    >
      {isEditing() ? (
        <div class="note-widget-editing" onClick={(e) => e.stopPropagation()}>
          <textarea
            class="note-widget-textarea"
            value={content()}
            onInput={(e) => setContent(e.currentTarget.value)}
            onKeyDown={handleKeyDown}
            placeholder="Write your note here..."
            onMouseDown={(e) => e.stopPropagation()}
            autoFocus
          />
          <div class="note-widget-toolbar">
            <div class="note-widget-colors">
              {colors.map((color) => (
                <button
                  class={`note-widget-color ${selectedColor() === color.value ? "active" : ""}`}
                  style={{ background: color.value }}
                  onClick={(e) => {
                    e.stopPropagation();
                    setSelectedColor(color.value);
                  }}
                  onMouseDown={(e) => e.stopPropagation()}
                  title={color.name}
                />
              ))}
            </div>
            <div class="note-widget-actions">
              <span class="note-widget-hint">Ctrl+Enter to save</span>
            </div>
          </div>
        </div>
      ) : (
        <div class="note-widget-content">
          {content() ? (
            <p class="note-widget-text">{content()}</p>
          ) : (
            <p class="note-widget-placeholder">Click to add a note...</p>
          )}
        </div>
      )}
    </div>
  );
};

export default NoteWidget;
