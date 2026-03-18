import { StateField, StateEffect, Extension } from "@codemirror/state";
import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate, WidgetType } from "@codemirror/view";
import { autocompleteService, AutocompleteContext, ContentType } from "../services/autocomplete-service";

// Effect to set ghost text
const setGhostText = StateEffect.define<string | null>({
  provide: () => null,
});

// Effect to set metadata about current suggestion
const setGhostMeta = StateEffect.define<{
  text: string;
  type: ContentType;
  timestamp: number;
} | null>();

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

// State field to track ghost text metadata
const ghostMetaField = StateField.define<{
  text: string;
  type: ContentType;
  timestamp: number;
} | null>({
  create: () => null,
  update(value, tr) {
    for (let e of tr.effects) {
      if (e.is(setGhostMeta)) return e.value;
    }
    return value;
  },
});

// Widget to render ghost text
class GhostTextWidget extends WidgetType {
  constructor(
    readonly text: string,
    readonly contentType: ContentType,
    readonly showHint: boolean = true
  ) {
    super();
  }

  eq(other: GhostTextWidget) {
    return other.text === this.text && other.contentType === this.contentType;
  }

  toDOM() {
    const container = document.createElement("span");
    container.className = "cm-ghost-text-container";
    container.style.display = "inline-flex";
    container.style.alignItems = "center";
    container.style.gap = "4px";

    // The ghost text itself
    const textSpan = document.createElement("span");
    textSpan.className = "cm-ghost-text";
    textSpan.textContent = this.text;
    
    // Hint about keyboard shortcuts
    if (this.showHint) {
      const hint = document.createElement("span");
      hint.className = "cm-ghost-hint";
      hint.textContent = "Tab";
      hint.title = "Tab to accept, Esc to dismiss, Ctrl+→ for word";
      container.appendChild(hint);
    }

    container.appendChild(textSpan);
    return container;
  }

  ignoreEvent() {
    return true;
  }
}

// Decoration set from ghost text field
const ghostTextDecoration = EditorView.decorations.fromField(ghostTextField, (text, state) => {
  if (!text) return Decoration.none;
  
  const cursor = state.selection.main.head;
  const meta = state.field(ghostMetaField);
  const widget = new GhostTextWidget(text, meta?.type || 'unknown', true);
  const deco = Decoration.widget({
    widget,
    side: 1,
  });
  
  return Decoration.set([deco.range(cursor)]);
});

// Debounce helper
function debounce<T extends (...args: any[]) => any>(fn: T, delay: number) {
  let timeoutId: ReturnType<typeof setTimeout>;
  return (...args: Parameters<T>) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  };
}

// Plugin to fetch suggestions
const ghostTextPlugin = ViewPlugin.fromClass(
  class {
    private fetchSuggestion: (view: EditorView) => void;
    private lastChangeTime = Date.now();
    private changeCount = 0;

    constructor(view: EditorView) {
      this.fetchSuggestion = debounce(this._fetchSuggestion.bind(this), 100);
    }

    update(update: ViewUpdate) {
      if (update.docChanged) {
        this.lastChangeTime = Date.now();
        this.changeCount += update.changes.changedRanges().length;
        
        // Clear ghost text on any change
        update.view.dispatch({ effects: setGhostText.of(null) });
        
        // Schedule new suggestion
        this.fetchSuggestion(update.view);
      }
      
      if (update.selectionSet) {
        // Clear ghost text if cursor moved significantly
        const ghost = update.state.field(ghostTextField);
        if (ghost && Math.abs(update.state.selection.main.head - update.startState.selection.main.head) > 1) {
          update.view.dispatch({ effects: setGhostText.of(null) });
        }
      }
    }

    private async _fetchSuggestion(view: EditorView) {
      try {
        const cursor = view.state.selection.main.head;
        const doc = view.state.doc.toString();
        
        const textBefore = doc.slice(0, cursor);
        const textAfter = doc.slice(cursor);
        
        // Build context
        const context: AutocompleteContext = {
          textBefore,
          textAfter,
          contentType: 'unknown',
          recentEdits: this.changeCount,
          pauseDuration: Date.now() - this.lastChangeTime,
        };

        // Detect content type
        context.contentType = autocompleteService.detectContentType({
          textBefore,
          fileExtension: this.getFileExtension(),
        });

        // Get suggestion
        const suggestion = await autocompleteService.getSuggestion(context);
        
        if (suggestion && view.state.selection.main.head === cursor) {
          view.dispatch({
            effects: [
              setGhostText.of(suggestion.text),
              setGhostMeta.of({
                text: suggestion.text,
                type: context.contentType,
                timestamp: Date.now(),
              }),
            ],
          });
        }
        
        // Reset change count after fetch
        this.changeCount = 0;
      } catch (e) {
        console.error("Ghost text fetch failed:", e);
      }
    }

    private getFileExtension(): string | undefined {
      // Try to infer from document content or context
      // In a real app, this would come from the file path
      return undefined;
    }
  }
);

// Keymap to accept ghost text
const acceptGhostText = (view: EditorView) => {
  const ghost = view.state.field(ghostTextField);
  if (!ghost) return false;
  
  autocompleteService.recordAcceptance();
  
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
  
  // Find the next word boundary
  const match = ghost.match(/^(\S+\s*)/);
  if (!match) {
    // Accept all if no word boundary found
    return acceptGhostText(view);
  }
  
  const firstWord = match[1];
  const rest = ghost.slice(firstWord.length);
  
  const cursor = view.state.selection.main.head;
  view.dispatch({
    changes: { from: cursor, insert: firstWord },
    effects: [
      setGhostText.of(rest || null),
      setGhostMeta.of(rest ? {
        text: rest,
        type: view.state.field(ghostMetaField)?.type || 'unknown',
        timestamp: Date.now(),
      } : null),
    ],
    selection: { anchor: cursor + firstWord.length },
  });
  return true;
};

const dismissGhostText = (view: EditorView) => {
  const ghost = view.state.field(ghostTextField);
  if (!ghost) return false;
  
  autocompleteService.recordRejection();
  
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
    ghostMetaField,
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
