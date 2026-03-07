import { Component, createSignal, createEffect, onCleanup, Show } from "solid-js";
import { aiService } from "../../services/ai-service";
import "./GhostText.css";

interface GhostTextProps {
  targetElement: HTMLTextAreaElement | null;
  content: string;
  cursorPosition: number;
  onAccept: (text: string) => void;
  onDismiss: () => void;
}

const GhostText: Component<GhostTextProps> = (props) => {
  const [suggestion, setSuggestion] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [position, setPosition] = createSignal({ top: 0, left: 0 });
  const [error, setError] = createSignal<string | null>(null);

  let debounceTimer: number | null = null;
  let dismissTimer: number | null = null;

  const getTextAroundCursor = () => {
    const text = props.content;
    const pos = props.cursorPosition;
    
    // Get text before cursor (last 500 chars for context)
    const textBefore = text.slice(Math.max(0, pos - 500), pos);
    
    // Get text after cursor (next 100 chars)
    const textAfter = text.slice(pos, pos + 100);
    
    return { textBefore, textAfter };
  };

  const fetchSuggestion = async () => {
    if (!props.targetElement || props.cursorPosition === 0) return;
    
    setLoading(true);
    setError(null);
    
    try {
      const { textBefore, textAfter } = getTextAroundCursor();
      
      // Don't suggest if at start of line or just whitespace
      if (!textBefore.trim() || textBefore.endsWith('\n\n')) {
        setSuggestion(null);
        setLoading(false);
        return;
      }
      
      const completion = await aiService.getInlineCompletion(
        textBefore,
        textAfter,
        "Academic writing and research notes"
      );
      
      if (completion) {
        setSuggestion(completion);
        updatePosition();
      } else {
        setSuggestion(null);
      }
    } catch (e) {
      console.error('Ghost text error:', e);
      setError('Failed to get suggestion');
      setSuggestion(null);
    } finally {
      setLoading(false);
    }
  };

  const updatePosition = () => {
    if (!props.targetElement) return;
    
    const textarea = props.targetElement;
    const cursorPos = props.cursorPosition;
    
    // Create a mirror element to calculate position
    const mirror = document.createElement('div');
    mirror.style.cssText = getComputedStyle(textarea).cssText;
    mirror.style.position = 'absolute';
    mirror.style.visibility = 'hidden';
    mirror.style.whiteSpace = 'pre-wrap';
    mirror.style.wordWrap = 'break-word';
    mirror.style.overflow = 'hidden';
    mirror.style.height = 'auto';
    mirror.style.width = textarea.offsetWidth + 'px';
    
    // Get text up to cursor
    const textBeforeCursor = props.content.slice(0, cursorPos);
    const textAfterCursor = props.content.slice(cursorPos);
    
    // Add marker for cursor position
    mirror.textContent = textBeforeCursor;
    const cursorMarker = document.createElement('span');
    cursorMarker.textContent = '|';
    mirror.appendChild(cursorMarker);
    mirror.appendChild(document.createTextNode(textAfterCursor));
    
    document.body.appendChild(mirror);
    
    const markerRect = cursorMarker.getBoundingClientRect();
    const mirrorRect = mirror.getBoundingClientRect();
    const textareaRect = textarea.getBoundingClientRect();
    
    document.body.removeChild(mirror);
    
    // Calculate position relative to textarea
    const scrollTop = textarea.scrollTop;
    const scrollLeft = textarea.scrollLeft;
    
    setPosition({
      top: markerRect.top - textareaRect.top + scrollTop,
      left: markerRect.left - textareaRect.left + scrollLeft
    });
  };

  // Debounced suggestion fetch
  createEffect(() => {
    const _ = props.content; // Track content changes
    const pos = props.cursorPosition;
    
    if (debounceTimer) clearTimeout(debounceTimer);
    
    debounceTimer = window.setTimeout(() => {
      if (pos > 0 && !suggestion()) {
        fetchSuggestion();
      }
    }, 800); // 800ms debounce
  });

  // Handle keyboard shortcuts
  createEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!suggestion()) return;
      
      switch (e.key) {
        case 'Tab':
          e.preventDefault();
          props.onAccept(suggestion()!);
          break;
        case 'Escape':
          e.preventDefault();
          props.onDismiss();
          setSuggestion(null);
          break;
        case 'ArrowRight':
          if (e.ctrlKey || e.metaKey) {
            e.preventDefault();
            // Accept word by word
            const words = suggestion()!.split(' ');
            if (words.length > 0) {
              props.onAccept(words[0] + ' ');
            }
          }
          break;
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  // Auto-dismiss after 10 seconds
  createEffect(() => {
    if (suggestion()) {
      if (dismissTimer) clearTimeout(dismissTimer);
      dismissTimer = window.setTimeout(() => {
        props.onDismiss();
        setSuggestion(null);
      }, 10000);
    }
  });

  // Cleanup on unmount
  onCleanup(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
    if (dismissTimer) clearTimeout(dismissTimer);
  });

  return (
    <Show when={suggestion()}>
      <div
        class="ghost-text-container"
        style={{
          position: 'absolute',
          top: `${position().top}px`,
          left: `${position().left}px`,
          'pointer-events': 'none',
          'z-index': '1000'
        }}
      >
        <span class="ghost-text">{suggestion()}</span>
        <span class="ghost-text-hint">
          Tab to accept · Esc to dismiss · Ctrl+→ word
        </span>
        
        <Show when={loading()}>
          <span class="ghost-text-loading">...</span>
        </Show>
      </div>
    </Show>
  );
};

export default GhostText;
