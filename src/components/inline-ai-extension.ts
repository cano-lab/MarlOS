import { StateField, StateEffect, Extension } from "@codemirror/state";
import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate, WidgetType, keymap } from "@codemirror/view";
import { autocompleteService, AutocompleteContext, ContentType } from "../services/autocomplete-service";

// Combined state for ghost text
interface GhostTextState {
  text: string;
  type: ContentType;
  timestamp: number;
}

// Effect to set ghost text
const setGhostText = StateEffect.define<GhostTextState | null>();

// State field to store ghost text and provide decorations
const ghostTextField = StateField.define<DecorationSet>({
  create: () => Decoration.none,

  update(_value, tr) {
    let ghostState: GhostTextState | null = null;

    // Check for setGhostText effect
    for (let e of tr.effects) {
      if (e.is(setGhostText)) ghostState = e.value;
    }

    // If clearing ghost text
    if (!ghostState) return Decoration.none;

    // Create decoration at cursor position
    const cursor = tr.state.selection.main.head;
    const widget = new GhostTextWidget(ghostState.text, ghostState.type, true);
    const deco = Decoration.widget({
      widget,
      side: 1,
    });

    return Decoration.set([deco.range(cursor)]);
  },

  provide: f => EditorView.decorations.from(f),
});

// State field to track the actual ghost text string (for keymap access)
const ghostTextStringField = StateField.define<GhostTextState | null>({
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

    // Hint about keyboard shortcuts
    if (this.showHint) {
      const hint = document.createElement("span");
      hint.className = "cm-ghost-hint";
      hint.textContent = "Tab";
      hint.title = "Tab for next word, Ctrl+→ for all, Esc to dismiss";
      hint.style.cssText = "font-size: 10px; opacity: 0.7; padding: 2px 4px; border-radius: 3px; background: rgba(255,255,255,0.1);";
      container.appendChild(hint);
    }

    // The ghost text itself with word-by-word styling
    const textSpan = document.createElement("span");
    textSpan.className = "cm-ghost-text";

    const words = this.text.trim().split(/\s+/);
    if (words.length > 1) {
      // Show individual words with subtle separators
      textSpan.innerHTML = words.map((word, i) => {
        const sep = i < words.length - 1 ? '<span style="opacity:0.3; margin:0 2px;">·</span>' : '';
        return `<span style="color:inherit;">${word}</span>${sep}`;
      }).join('');
    } else {
      textSpan.textContent = this.text;
    }

    textSpan.style.cssText = "color: rgba(150, 150, 150, 0.6); font-style: italic;";

    container.appendChild(textSpan);
    return container;
  }

  ignoreEvent() {
    return true;
  }
}

// Helper to check if ghost text exists
function hasGhostText(view: EditorView): boolean {
  try {
    const deco = view.state.field(ghostTextField);
    return deco !== Decoration.none && deco.size > 0;
  } catch {
    return false;
  }
}

/**
 * Detect document structure at cursor position
 */
function detectDocumentStructure(doc: string, cursor: number) {
  const lines = doc.split('\n');
  let currentLine = 0;
  let charCount = 0;

  // Find current line number
  for (let i = 0; i < lines.length; i++) {
    const lineLength = lines[i].length + 1; // +1 for newline
    if (charCount + lineLength > cursor) {
      currentLine = i;
      break;
    }
    charCount += lineLength;
  }

  const currentLineText = lines[currentLine] || '';
  void charCount; // lineStart

  // Check if in code block
  let inCodeBlock = false;
  for (let i = 0; i <= currentLine; i++) {
    if (lines[i].trim().startsWith('```')) {
      inCodeBlock = !inCodeBlock;
    }
  }

  // Check if in blockquote
  const inQuote = currentLineText.trim().startsWith('>');

  // Check if in list
  const listMatch = currentLineText.match(/^(\s*)([-*+]|\d+\.)(\s+\[[ x]\])?\s/);
  const inList = !!listMatch;

  // Get list type
  let listType: 'bullet' | 'numbered' | 'task' | undefined;
  if (listMatch) {
    if (listMatch[3]) listType = 'task';
    else if (listMatch[2].includes('.')) listType = 'numbered';
    else listType = 'bullet';
  }

  // Find current section heading
  let currentHeading: string | undefined;
  for (let i = currentLine - 1; i >= 0; i--) {
    const headingMatch = lines[i].match(/^(#{1,6})\s+(.+)$/);
    if (headingMatch) {
      currentHeading = headingMatch[2];
      break;
    }
  }

  return {
    inList,
    inCodeBlock,
    inQuote,
    listType,
    currentHeading,
    currentLine,
  };
}

// Helper to get ghost text state
function getGhostTextState(view: EditorView): GhostTextState | null {
  try {
    return view.state.field(ghostTextStringField);
  } catch {
    return null;
  }
}

// Helper to accept one word of ghost text (for external use)
export function acceptOneWordGhostText(view: EditorView): boolean {
  const ghostState = getGhostTextState(view);
  if (!ghostState) return false;

  const match = ghostState.text.match(/^(\S+\s*)/);
  if (!match) {
    // Accept all if no word boundary found
    return acceptGhostText(view);
  }

  let toAccept = match[0]; // Includes trailing space from regex
  const rest = ghostState.text.slice(toAccept.length);

  const cursor = view.state.selection.main.head;

  // Check if we need to add a space before the word
  const doc = view.state.doc.toString();
  const lastChar = doc.slice(cursor - 1, cursor);

  // If cursor is not after a space and not at start, add a space
  if (lastChar && lastChar !== ' ' && lastChar !== '\n') {
    toAccept = ' ' + toAccept;
  }

  view.dispatch({
    changes: { from: cursor, insert: toAccept },
    effects: setGhostText.of(rest ? {
      text: rest,
      type: ghostState.type,
      timestamp: Date.now(),
    } : null),
    selection: { anchor: cursor + toAccept.length },
  });
  return true;
}

// Plugin to fetch suggestions
const ghostTextPlugin = ViewPlugin.fromClass(
  class {
    private fetchTimer: ReturnType<typeof setTimeout> | null = null;
    private lastChangeTime = Date.now();
    private isTyping = false;

    update(update: ViewUpdate) {
      if (update.docChanged) {
        // User is actively typing
        this.isTyping = true;
        this.lastChangeTime = Date.now();

        // Clear any pending fetch
        if (this.fetchTimer) {
          clearTimeout(this.fetchTimer);
          this.fetchTimer = null;
        }

        // Clear ghost text immediately while typing
        if (hasGhostText(update.view)) {
          update.view.dispatch({ effects: setGhostText.of(null) });
        }

        // Schedule fetch after user pauses (800ms of no typing)
        this.fetchTimer = setTimeout(() => {
          this.isTyping = false;
          this._fetchSuggestion(update.view);
        }, 800);
      }

      if (update.selectionSet) {
        // Clear ghost text if cursor moved significantly while paused
        if (!this.isTyping && hasGhostText(update.view)) {
          const cursorMove = Math.abs(update.state.selection.main.head - update.startState.selection.main.head);
          if (cursorMove > 1) {
            update.view.dispatch({ effects: setGhostText.of(null) });
          }
        }
      }
    }

    private async _fetchSuggestion(view: EditorView) {
      // Double-check we're still in a paused state
      if (this.isTyping) return;

      try {
        const cursor = view.state.selection.main.head;
        const doc = view.state.doc.toString();

        const textBefore = doc.slice(0, cursor);
        const textAfter = doc.slice(cursor);

        // Detect document structure for better suggestions
        const structure = detectDocumentStructure(doc, cursor);

        // Build context
        const context: AutocompleteContext = {
          textBefore,
          textAfter,
          contentType: 'unknown',
          recentEdits: 0,
          pauseDuration: Date.now() - this.lastChangeTime,

          // Enhanced context
          currentSection: structure.currentHeading,
          inList: structure.inList,
          inCodeBlock: structure.inCodeBlock,
          inQuote: structure.inQuote,
        };

        // Detect content type
        context.contentType = autocompleteService.detectContentType({
          textBefore,
          fileExtension: this.getFileExtension(),
        });

        // Get suggestion
        const suggestion = await autocompleteService.getSuggestion(context);

        // Only show if cursor hasn't moved significantly and user is still paused
        if (suggestion && !this.isTyping) {
          const currentCursor = view.state.selection.main.head;
          const cursorMoved = Math.abs(currentCursor - cursor) > 10;

          if (!cursorMoved) {
            view.dispatch({
              effects: setGhostText.of({
                text: suggestion.text,
                type: context.contentType,
                timestamp: Date.now(),
              }),
            });
          }
        }
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
  const ghostState = getGhostTextState(view);
  if (!ghostState) return false;

  autocompleteService.recordAcceptance();

  const cursor = view.state.selection.main.head;
  view.dispatch({
    changes: { from: cursor, insert: ghostState.text },
    effects: setGhostText.of(null),
    selection: { anchor: cursor + ghostState.text.length },
  });
  return true;
};

const acceptWordGhostText = (view: EditorView) => {
  const ghostState = getGhostTextState(view);

  // If no ghost text, let the default indent behavior happen
  if (!ghostState) return false;

  // Find the next word boundary
  const match = ghostState.text.match(/^(\S+\s*)/);
  if (!match) {
    // Accept all if no word boundary found
    return acceptGhostText(view);
  }

  const firstWord = match[1];
  const rest = ghostState.text.slice(firstWord.length);

  const cursor = view.state.selection.main.head;
  view.dispatch({
    changes: { from: cursor, insert: firstWord },
    effects: setGhostText.of(rest ? {
      text: rest,
      type: ghostState.type,
      timestamp: Date.now(),
    } : null),
    selection: { anchor: cursor + firstWord.length },
  });
  return true;
};

const dismissGhostText = (view: EditorView) => {
  if (!hasGhostText(view)) return false;

  autocompleteService.recordRejection();
  view.dispatch({ effects: setGhostText.of(null) });
  return true;
};

// Keymap extension
const ghostTextKeymap = [
  { key: "Tab", run: acceptWordGhostText },           // Tab = accept one word
  { key: "Ctrl-ArrowRight", run: acceptGhostText, mac: "Cmd-ArrowRight" },  // Ctrl+Right = accept all
  { key: "Escape", run: dismissGhostText },           // Esc = dismiss
];

// Export the complete extension
export function inlineAI(): Extension {
  return [
    ghostTextField,
    ghostTextStringField,
    ghostTextPlugin,
    EditorView.domEventHandlers({
      blur: (_e, view) => {
        view.dispatch({ effects: setGhostText.of(null) });
        return false;
      },
    }),
    keymap.of(ghostTextKeymap),
  ];
}
