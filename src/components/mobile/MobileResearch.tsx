import { Component, createSignal, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./MobileResearch.css";

interface WebSearchResult {
  title: string;
  url: string;
  snippet: string;
}

interface SavedSource {
  id: string;
  title: string;
  url: string;
  content?: string;
  notes?: string;
  tags: string[];
  saved_at: string;
}

interface MobileResearchProps {
  onClose?: () => void;
  onSourceSaved?: (id: string) => void;
}

const MobileResearch: Component<MobileResearchProps> = (props) => {
  const [query, setQuery] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<WebSearchResult[]>([]);
  const [savedSources, setSavedSources] = createSignal<SavedSource[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [activeTab, setActiveTab] = createSignal<"search" | "saved">("search");
  const [savingUrl, setSavingUrl] = createSignal<string | null>(null);
  const [showSaveModal, setShowSaveModal] = createSignal(false);
  const [sourceToSave, setSourceToSave] = createSignal<WebSearchResult | null>(null);
  const [saveNotes, setSaveNotes] = createSignal("");
  const [saveTags, setSaveTags] = createSignal("");

  const performSearch = async () => {
    if (!query().trim()) return;

    setLoading(true);
    try {
      const results = await invoke<WebSearchResult[]>("web_search", {
        query: query().trim(),
        limit: 10,
      });
      setSearchResults(results);
    } catch (e) {
      console.error("Search failed:", e);
    } finally {
      setLoading(false);
    }
  };

  const loadSavedSources = async () => {
    try {
      const sources = await invoke<SavedSource[]>("get_saved_sources", { limit: 50 });
      setSavedSources(sources);
    } catch (e) {
      console.error("Failed to load saved sources:", e);
    }
  };

  const openSaveModal = (result: WebSearchResult) => {
    setSourceToSave(result);
    setSaveNotes("");
    setSaveTags("");
    setShowSaveModal(true);
  };

  const saveSource = async () => {
    const source = sourceToSave();
    if (!source) return;

    setSavingUrl(source.url);
    try {
      const tagList = saveTags()
        .split(",")
        .map((t) => t.trim())
        .filter((t) => t.length > 0);

      const id = await invoke<string>("save_research_source", {
        title: source.title,
        url: source.url,
        snippet: source.snippet,
        notes: saveNotes().trim() || null,
        tags: tagList,
      });

      props.onSourceSaved?.(id);
      setShowSaveModal(false);
      setSourceToSave(null);

      // Refresh saved sources if on that tab
      if (activeTab() === "saved") {
        loadSavedSources();
      }
    } catch (e) {
      console.error("Failed to save source:", e);
    } finally {
      setSavingUrl(null);
    }
  };

  const openUrl = (url: string) => {
    // Use Tauri shell plugin to open in browser
    invoke("open_external_url", { url }).catch(console.error);
  };

  const handleTabChange = (tab: "search" | "saved") => {
    setActiveTab(tab);
    if (tab === "saved") {
      loadSavedSources();
    }
  };

  const formatDate = (dateStr: string) => {
    const date = new Date(dateStr);
    return date.toLocaleDateString();
  };

  return (
    <div class="mobile-research">
      <div class="research-header">
        <h2>Research</h2>
        <Show when={props.onClose}>
          <button class="close-btn" onClick={props.onClose}>
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
              <path d="M6 6l12 12M6 18L18 6" stroke="currentColor" stroke-width="2" />
            </svg>
          </button>
        </Show>
      </div>

      {/* Tab Selector */}
      <div class="tab-selector">
        <button
          class={`tab-btn ${activeTab() === "search" ? "active" : ""}`}
          onClick={() => handleTabChange("search")}
        >
          <svg width="18" height="18" viewBox="0 0 18 18">
            <circle cx="7" cy="7" r="5" stroke="currentColor" stroke-width="1.5" fill="none" />
            <path d="M11 11l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
          Search
        </button>
        <button
          class={`tab-btn ${activeTab() === "saved" ? "active" : ""}`}
          onClick={() => handleTabChange("saved")}
        >
          <svg width="18" height="18" viewBox="0 0 18 18">
            <path d="M3 3h12v14l-6-4-6 4V3z" stroke="currentColor" stroke-width="1.5" fill="none" />
          </svg>
          Saved ({savedSources().length})
        </button>
      </div>

      {/* Search Tab */}
      <Show when={activeTab() === "search"}>
        <div class="search-section">
          <div class="search-bar">
            <input
              type="text"
              placeholder="Search the web..."
              value={query()}
              onInput={(e) => setQuery(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && performSearch()}
            />
            <button class="search-btn" onClick={performSearch} disabled={loading()}>
              {loading() ? (
                <div class="spinner" />
              ) : (
                <svg width="20" height="20" viewBox="0 0 20 20">
                  <path d="M17 17l-5-5" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                  <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="2" fill="none" />
                </svg>
              )}
            </button>
          </div>

          <div class="results-container">
            <Show
              when={searchResults().length > 0}
              fallback={
                <div class="empty-state">
                  <svg width="48" height="48" viewBox="0 0 48 48">
                    <circle cx="20" cy="20" r="12" stroke="currentColor" stroke-width="2" fill="none" />
                    <path d="M28 28l12 12" stroke="currentColor" stroke-width="3" stroke-linecap="round" />
                  </svg>
                  <p>Search for articles, papers, and resources</p>
                </div>
              }
            >
              <For each={searchResults()}>
                {(result) => (
                  <div class="result-card">
                    <h3 class="result-title" onClick={() => openUrl(result.url)}>
                      {result.title}
                    </h3>
                    <p class="result-url">{result.url}</p>
                    <p class="result-snippet">{result.snippet}</p>
                    <div class="result-actions">
                      <button class="action-btn" onClick={() => openUrl(result.url)}>
                        <svg width="16" height="16" viewBox="0 0 16 16">
                          <path d="M4 12L12 4M12 4H6M12 4v6" stroke="currentColor" stroke-width="1.5" fill="none" />
                        </svg>
                        Open
                      </button>
                      <button
                        class="action-btn save"
                        onClick={() => openSaveModal(result)}
                        disabled={savingUrl() === result.url}
                      >
                        <svg width="16" height="16" viewBox="0 0 16 16">
                          <path d="M2 2h12v14l-6-4-6 4V2z" stroke="currentColor" stroke-width="1.5" fill="none" />
                        </svg>
                        {savingUrl() === result.url ? "Saving..." : "Save"}
                      </button>
                    </div>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </div>
      </Show>

      {/* Saved Tab */}
      <Show when={activeTab() === "saved"}>
        <div class="saved-section">
          <Show
            when={savedSources().length > 0}
            fallback={
              <div class="empty-state">
                <svg width="48" height="48" viewBox="0 0 48 48">
                  <path d="M8 8h32v40l-16-10-16 10V8z" stroke="currentColor" stroke-width="2" fill="none" />
                </svg>
                <p>No saved sources yet</p>
                <span>Search and save articles for later</span>
              </div>
            }
          >
            <For each={savedSources()}>
              {(source) => (
                <div class="saved-card">
                  <h3 class="saved-title" onClick={() => openUrl(source.url)}>
                    {source.title}
                  </h3>
                  <p class="saved-url">{source.url}</p>
                  <Show when={source.notes}>
                    <p class="saved-notes">{source.notes}</p>
                  </Show>
                  <div class="saved-meta">
                    <span class="saved-date">{formatDate(source.saved_at)}</span>
                    <Show when={source.tags.length > 0}>
                      <div class="saved-tags">
                        <For each={source.tags.slice(0, 2)}>
                          {(tag) => <span class="tag">{tag}</span>}
                        </For>
                      </div>
                    </Show>
                  </div>
                </div>
              )}
            </For>
          </Show>
        </div>
      </Show>

      {/* Save Modal */}
      <Show when={showSaveModal() && sourceToSave()}>
        <div class="save-modal-overlay" onClick={() => setShowSaveModal(false)}>
          <div class="save-modal" onClick={(e) => e.stopPropagation()}>
            <h3>Save Source</h3>
            <p class="modal-source-title">{sourceToSave()!.title}</p>

            <div class="form-group">
              <label>Notes (optional)</label>
              <textarea
                placeholder="Why are you saving this? Key points?"
                value={saveNotes()}
                onInput={(e) => setSaveNotes(e.currentTarget.value)}
                rows={3}
              />
            </div>

            <div class="form-group">
              <label>Tags (optional)</label>
              <input
                type="text"
                placeholder="research, topic, important"
                value={saveTags()}
                onInput={(e) => setSaveTags(e.currentTarget.value)}
              />
            </div>

            <div class="modal-actions">
              <button class="cancel-btn" onClick={() => setShowSaveModal(false)}>
                Cancel
              </button>
              <button class="save-btn" onClick={saveSource} disabled={!!savingUrl()}>
                {savingUrl() ? "Saving..." : "Save"}
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default MobileResearch;
