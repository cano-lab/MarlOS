import { StateField, StateEffect, Extension } from "@codemirror/state";
import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate, WidgetType } from "@codemirror/view";
import { aiService } from "../services/ai-service";

// Effect to set ghost text
const setGhostText = StateEffect.define<string | null>();

// State field to store ghost text
const ghostTextField = StateField.define<string | null>({
  create: () => null,
  update(value, tr) {
    for (let e of tr.effects) {
      if (e.is(setGhostText)) return e.value;
    }
    return value;
  },
});

// Widget to render ghost text
class GhostTextWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }

  eq(other: GhostTextWidget) {
    return other.text === this.text;
  }

  toDOM() {
    const span = document.createElement("span");
    span.className = "cm-ghost-text";
    span.textContent = this.text;
    span.style.opacity = "0.5";
    span.style.fontStyle = "italic";
    span.style.pointerEvents = "none";
    span.style.userSelect = "none";
    return span;
  }

  ignoreEvent() {
    return true;
  }
}

// Decoration set from ghost text field
const ghostTextDecoration = EditorView.decorations.fromField(ghostTextField, (text, state) => {
  if (!text) return Decoration.none;
  
  const cursor = state.selection.main.head;
  const widget = new GhostTextWidget(text);
  const deco = Decoration.widget({
    widget,
    side: 1,
  });
  
  return Decoration.set([deco.range(cursor)]);
});

// Debounce helper
function debounce(fn: Function, delay: number) {
  let timeoutId: ReturnType<typeof setTimeout>;
  return (...args: any[]) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  };
}

// Plugin to fetch suggestions
const ghostTextPlugin = ViewPlugin.fromClass(
  class {
    private fetchSuggestion: (view: EditorView) => void;

    constructor(view: EditorView) {
      this.fetchSuggestion = debounce(this._fetchSuggestion.bind(this), 800);
      
      // Initial fetch
      if (view.state.doc.length > 0) {
        this.fetchSuggestion(view);
      }
    }

    update(update: ViewUpdate) {
      if (update.docChanged || update.selectionSet) {
        // Clear ghost text on any change
        if (update.docChanged) {
          update.view.dispatch({ effects: setGhostText.of(null) });
        }
        
        // Fetch new suggestion after debounce
        const cursor = update.state.selection.main.head;
        const line = update.state.doc.lineAt(cursor);
        const textBefore = line.text.slice(0, cursor - line.from);
        
        // Only suggest if we have meaningful text before cursor
        if (textBefore.trim().length >= 3 && !textBefore.endsWith(" ")) {
          this.fetchSuggestion(update.view);
        }
      }
    }

    private async _fetchSuggestion(view: EditorView) {
      try {
        const cursor = view.state.selection.main.head;
        const doc = view.state.doc.toString();
        
        const textBefore = doc.slice(Math.max(0, cursor - 500), cursor);
        const textAfter = doc.slice(cursor, cursor + 100);
        
        const completion = await aiService.getInlineCompletion(
          textBefore,
          textAfter,
          "Academic writing"
        );
        
        if (completion && view.state.selection.main.head === cursor) {
          view.dispatch({ effects: setGhostText.of(completion) });
        }
      } catch (e) {
        console.error("Ghost text fetch failed:", e);
      }
    }
  }
);

// Keymap to accept ghost text
const acceptGhostText = (view: EditorView) => {
  const ghost = view.state.field(ghostTextField);
  if (!ghost) return false;
  
  const cursor = view.state.selection.main.head;
  view.dispatch({
    changes: { from: cursor, insert: ghost },
    effects: setGhostText.of(null),
    selection: { anchor: cursor + ghost.length },
  });
  return true;
};

const acceptWordGhostText = (view: EditorView) => {
  const ghost = view.state.field(ghostTextField);
  if (!ghost) return false;
  
  const words = ghost.split(/(\s+)/);
  const firstWord = words[0];
  const rest = words.slice(1).join("");
  
  const cursor = view.state.selection.main.head;
  view.dispatch({
    changes: { from: cursor, insert: firstWord },
    effects: setGhostText.of(rest || null),
    selection: { anchor: cursor + firstWord.length },
  });
  return true;
};

const dismissGhostText = (view: EditorView) => {
  const ghost = view.state.field(ghostTextField);
  if (!ghost) return false;
  
  view.dispatch({ effects: setGhostText.of(null) });
  return true;
};

// Keymap extension
const ghostTextKeymap = [
  { key: "Tab", run: acceptGhostText },
  { key: "Ctrl-ArrowRight", run: acceptWordGhostText, mac: "Cmd-ArrowRight" },
  { key: "Escape", run: dismissGhostText },
];

// Export the complete extension
export function inlineAI(): Extension {
  return [
    ghostTextField,
    ghostTextDecoration,
    ghostTextPlugin,
    EditorView.domEventHandlers({
      blur: (e, view) => {
        view.dispatch({ effects: setGhostText.of(null) });
        return false;
      },
    }),
    EditorView.keymap.of(ghostTextKeymap),
  ];
}
