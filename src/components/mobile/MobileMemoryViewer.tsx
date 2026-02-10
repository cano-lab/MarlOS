import { Component, createSignal, createEffect, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./MobileMemoryViewer.css";

interface MemoryObject {
  id: string;
  kind: string;
  content: string;
  tags: string[];
  created_at: string;
  updated_at: string;
  metadata: Record<string, unknown>;
}

interface SearchResult {
  object: MemoryObject;
  score: number;
  highlights?: string[];
}

interface MobileMemoryViewerProps {
  onSelectItem?: (item: MemoryObject) => void;
  onClose?: () => void;
}

const MobileMemoryViewer: Component<MobileMemoryViewerProps> = (props) => {
  const [query, setQuery] = createSignal("");
  const [results, setResults] = createSignal<SearchResult[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [selectedKind, setSelectedKind] = createSignal<string | null>(null);
  const [recentItems, setRecentItems] = createSignal<MemoryObject[]>([]);
  const [selectedItem, setSelectedItem] = createSignal<MemoryObject | null>(null);

  // Load recent items on mount
  createEffect(async () => {
    try {
      const recent = await invoke<MemoryObject[]>("get_recent_objects", { limit: 20 });
      setRecentItems(recent);
    } catch (e) {
      console.error("Failed to load recent items:", e);
    }
  });

  // Search when query changes
  createEffect(() => {
    const q = query();
    if (q.length >= 2) {
      performSearch(q);
    } else if (q.length === 0) {
      setResults([]);
    }
  });

  const performSearch = async (searchQuery: string) => {
    setLoading(true);
    try {
      const searchResults = await invoke<SearchResult[]>("semantic_search", {
        query: searchQuery,
        limit: 20,
        kind: selectedKind(),
      });
      setResults(searchResults);
    } catch (e) {
      console.error("Search failed:", e);
    } finally {
      setLoading(false);
    }
  };

  const formatDate = (dateStr: string) => {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

    if (diffDays === 0) {
      return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    } else if (diffDays === 1) {
      return "Yesterday";
    } else if (diffDays < 7) {
      return `${diffDays} days ago`;
    } else {
      return date.toLocaleDateString();
    }
  };

  const getKindIcon = (kind: string) => {
    switch (kind) {
      case "note":
        return "📝";
      case "decision":
        return "⚖️";
      case "conversation":
        return "💬";
      case "research":
        return "🔬";
      case "code":
        return "💻";
      case "photo":
        return "📷";
      default:
        return "📄";
    }
  };

  const getKindColor = (kind: string) => {
    switch (kind) {
      case "note":
        return "var(--accent-color, #89b4fa)";
      case "decision":
        return "var(--warning-color, #f9e2af)";
      case "conversation":
        return "var(--success-color, #a6e3a1)";
      case "research":
        return "var(--purple, #cba6f7)";
      case "code":
        return "var(--teal, #94e2d5)";
      default:
        return "var(--text-secondary, #a6adc8)";
    }
  };

  const truncateContent = (content: string, maxLength: number = 100) => {
    if (content.length <= maxLength) return content;
    return content.slice(0, maxLength) + "...";
  };

  const handleItemClick = (item: MemoryObject) => {
    setSelectedItem(item);
    props.onSelectItem?.(item);
  };

  const kindFilters = ["note", "decision", "conversation", "research", "code"];

  return (
    <div class="mobile-memory-viewer">
      <div class="viewer-header">
        <h2>Memory</h2>
        <Show when={props.onClose}>
          <button class="close-btn" onClick={props.onClose}>
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
              <path d="M6 6l12 12M6 18L18 6" stroke="currentColor" stroke-width="2" />
            </svg>
          </button>
        </Show>
      </div>

      {/* Search Bar */}
      <div class="search-container">
        <svg class="search-icon" width="20" height="20" viewBox="0 0 20 20">
          <circle cx="8" cy="8" r="5" stroke="currentColor" stroke-width="1.5" fill="none" />
          <path d="M12 12l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
        <input
          type="text"
          class="search-input"
          placeholder="Search your memory..."
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
        />
        <Show when={query()}>
          <button class="clear-search" onClick={() => setQuery("")}>
            <svg width="16" height="16" viewBox="0 0 16 16">
              <path d="M4 4l8 8M4 12l8-8" stroke="currentColor" stroke-width="2" />
            </svg>
          </button>
        </Show>
      </div>

      {/* Kind Filters */}
      <div class="kind-filters">
        <button
          class={`filter-btn ${selectedKind() === null ? "active" : ""}`}
          onClick={() => setSelectedKind(null)}
        >
          All
        </button>
        <For each={kindFilters}>
          {(kind) => (
            <button
              class={`filter-btn ${selectedKind() === kind ? "active" : ""}`}
              onClick={() => setSelectedKind(selectedKind() === kind ? null : kind)}
              style={{ "--kind-color": getKindColor(kind) }}
            >
              {getKindIcon(kind)} {kind}
            </button>
          )}
        </For>
      </div>

      {/* Loading Indicator */}
      <Show when={loading()}>
        <div class="loading-indicator">
          <div class="spinner" />
          Searching...
        </div>
      </Show>

      {/* Results List */}
      <div class="results-list">
        <Show
          when={query().length >= 2}
          fallback={
            <>
              <h3 class="section-title">Recent</h3>
              <For each={recentItems()}>
                {(item) => (
                  <div class="memory-card" onClick={() => handleItemClick(item)}>
                    <div class="card-header">
                      <span class="kind-badge" style={{ background: getKindColor(item.kind) }}>
                        {getKindIcon(item.kind)} {item.kind}
                      </span>
                      <span class="card-date">{formatDate(item.created_at)}</span>
                    </div>
                    <p class="card-content">{truncateContent(item.content)}</p>
                    <Show when={item.tags.length > 0}>
                      <div class="card-tags">
                        <For each={item.tags.slice(0, 3)}>
                          {(tag) => <span class="tag">{tag}</span>}
                        </For>
                        <Show when={item.tags.length > 3}>
                          <span class="tag more">+{item.tags.length - 3}</span>
                        </Show>
                      </div>
                    </Show>
                  </div>
                )}
              </For>
            </>
          }
        >
          <h3 class="section-title">
            {results().length} result{results().length !== 1 ? "s" : ""} for "{query()}"
          </h3>
          <For each={results()}>
            {(result) => (
              <div class="memory-card" onClick={() => handleItemClick(result.object)}>
                <div class="card-header">
                  <span class="kind-badge" style={{ background: getKindColor(result.object.kind) }}>
                    {getKindIcon(result.object.kind)} {result.object.kind}
                  </span>
                  <span class="card-score">{Math.round(result.score * 100)}% match</span>
                </div>
                <p class="card-content">{truncateContent(result.object.content)}</p>
                <Show when={result.object.tags.length > 0}>
                  <div class="card-tags">
                    <For each={result.object.tags.slice(0, 3)}>
                      {(tag) => <span class="tag">{tag}</span>}
                    </For>
                  </div>
                </Show>
              </div>
            )}
          </For>
        </Show>

        <Show when={!loading() && query().length >= 2 && results().length === 0}>
          <div class="no-results">
            <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
              <circle cx="24" cy="24" r="20" stroke="currentColor" stroke-width="2" />
              <path d="M17 17l14 14M17 31l14-14" stroke="currentColor" stroke-width="2" />
            </svg>
            <p>No memories found matching "{query()}"</p>
          </div>
        </Show>
      </div>

      {/* Detail View Modal */}
      <Show when={selectedItem()}>
        <div class="detail-overlay" onClick={() => setSelectedItem(null)}>
          <div class="detail-modal" onClick={(e) => e.stopPropagation()}>
            <div class="detail-header">
              <span class="kind-badge" style={{ background: getKindColor(selectedItem()!.kind) }}>
                {getKindIcon(selectedItem()!.kind)} {selectedItem()!.kind}
              </span>
              <button class="close-btn" onClick={() => setSelectedItem(null)}>
                <svg width="20" height="20" viewBox="0 0 20 20">
                  <path d="M5 5l10 10M5 15l10-10" stroke="currentColor" stroke-width="2" />
                </svg>
              </button>
            </div>
            <div class="detail-content">
              <p>{selectedItem()!.content}</p>
            </div>
            <Show when={selectedItem()!.tags.length > 0}>
              <div class="detail-tags">
                <For each={selectedItem()!.tags}>
                  {(tag) => <span class="tag">{tag}</span>}
                </For>
              </div>
            </Show>
            <div class="detail-meta">
              <span>Created: {formatDate(selectedItem()!.created_at)}</span>
              <span>ID: {selectedItem()!.id.slice(0, 8)}...</span>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default MobileMemoryViewer;
