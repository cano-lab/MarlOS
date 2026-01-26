import { Component, createSignal, createEffect, For, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import {
  SemanticObject,
  ObjectSearchResult,
  SecurityTier,
  searchObjects,
  listObjects,
  importFile,
  deleteObject,
  setTier,
  tierColor,
  tierIcon,
  contentTypeIcon,
} from "../api/objects";
import "./MemoryBrowser.css";

interface MemoryBrowserProps {
  onSelectObject?: (obj: SemanticObject) => void;
}

const MemoryBrowser: Component<MemoryBrowserProps> = (props) => {
  const [searchQuery, setSearchQuery] = createSignal("");
  const [results, setResults] = createSignal<ObjectSearchResult[]>([]);
  const [allObjects, setAllObjects] = createSignal<SemanticObject[]>([]);
  const [isSearching, setIsSearching] = createSignal(false);
  const [selectedSuid, setSelectedSuid] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [viewMode, setViewMode] = createSignal<"search" | "browse">("browse");
  const [tierFilter, setTierFilter] = createSignal<SecurityTier | "all">("all");

  // Load objects on mount
  createEffect(async () => {
    await loadObjects();
  });

  const loadObjects = async () => {
    try {
      const tier = tierFilter() === "all" ? undefined : tierFilter() as SecurityTier;
      const objects = await listObjects(undefined, tier, 50);
      setAllObjects(objects);
      setError(null);
    } catch (e) {
      console.error("Failed to load objects:", e);
      setError(`Failed to load: ${e}`);
    }
  };

  const handleSearch = async () => {
    const query = searchQuery().trim();
    if (!query) {
      setViewMode("browse");
      await loadObjects();
      return;
    }

    setIsSearching(true);
    setViewMode("search");
    try {
      const tier = tierFilter() === "all" ? undefined : tierFilter() as SecurityTier;
      const searchResults = await searchObjects(query, 20, tier);
      setResults(searchResults);
      setError(null);
    } catch (e) {
      console.error("Search failed:", e);
      setError(`Search failed: ${e}`);
    } finally {
      setIsSearching(false);
    }
  };

  const handleImport = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [
          { name: "Text", extensions: ["txt", "md", "markdown"] },
          { name: "Code", extensions: ["js", "ts", "py", "rs", "go", "java"] },
          { name: "Data", extensions: ["json", "yaml", "toml", "xml"] },
          { name: "All Files", extensions: ["*"] },
        ],
      });

      if (!path || typeof path !== "string") return;

      const obj = await importFile(path);
      console.log("Imported object:", obj.suid);
      await loadObjects();
    } catch (e) {
      console.error("Import failed:", e);
      setError(`Import failed: ${e}`);
    }
  };

  const handleDelete = async (suid: string) => {
    if (!confirm("Delete this object? This cannot be undone.")) return;

    try {
      await deleteObject(suid);
      await loadObjects();
      if (selectedSuid() === suid) {
        setSelectedSuid(null);
      }
    } catch (e) {
      console.error("Delete failed:", e);
      setError(`Delete failed: ${e}`);
    }
  };

  const handleTierChange = async (suid: string, newTier: SecurityTier) => {
    try {
      await setTier(suid, newTier, "Manual tier change by user", "user");
      await loadObjects();
    } catch (e) {
      console.error("Tier change failed:", e);
      setError(`Tier change failed: ${e}`);
    }
  };

  const formatDate = (dateStr: string) => {
    const date = new Date(dateStr);
    return date.toLocaleDateString() + " " + date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  };

  const truncate = (text: string, maxLen: number = 100) => {
    if (text.length <= maxLen) return text;
    return text.slice(0, maxLen) + "...";
  };

  return (
    <div class="memory-browser">
      <div class="memory-header">
        <h3>Memory</h3>
        <button class="import-btn" onClick={handleImport} title="Import file">
          +
        </button>
      </div>

      <div class="memory-search">
        <input
          type="text"
          class="search-input"
          placeholder="Search memories..."
          value={searchQuery()}
          onInput={(e) => setSearchQuery(e.currentTarget.value)}
          onKeyPress={(e) => e.key === "Enter" && handleSearch()}
        />
        <button class="search-btn" onClick={handleSearch} disabled={isSearching()}>
          {isSearching() ? "..." : "Go"}
        </button>
      </div>

      <div class="memory-filters">
        <select
          class="tier-filter"
          value={tierFilter()}
          onChange={(e) => {
            setTierFilter(e.currentTarget.value as SecurityTier | "all");
            loadObjects();
          }}
        >
          <option value="all">All Tiers</option>
          <option value="Open">Open</option>
          <option value="Guarded">Guarded</option>
          <option value="Sealed">Sealed</option>
        </select>
      </div>

      <Show when={error()}>
        <div class="error-message">{error()}</div>
      </Show>

      <div class="memory-list">
        <Show when={viewMode() === "search" && results().length > 0}>
          <div class="results-header">
            Search Results ({results().length})
          </div>
          <For each={results()}>
            {(result) => (
              <div
                class={`memory-item ${selectedSuid() === result.suid ? "selected" : ""}`}
                onClick={() => setSelectedSuid(result.suid)}
              >
                <div class="item-header">
                  <span class="item-icon">{contentTypeIcon(result.content_type)}</span>
                  <span class="item-title">{result.title || result.suid.slice(0, 8)}</span>
                  <span
                    class="item-tier"
                    style={{ color: tierColor(result.tier) }}
                  >
                    {tierIcon(result.tier)}
                  </span>
                </div>
                <div class="item-meta">
                  <span class="relevance">
                    {(result.relevance * 100).toFixed(0)}% match
                  </span>
                </div>
              </div>
            )}
          </For>
        </Show>

        <Show when={viewMode() === "browse" || (viewMode() === "search" && results().length === 0)}>
          <Show when={viewMode() === "search" && results().length === 0 && searchQuery()}>
            <div class="no-results">No matches for "{searchQuery()}"</div>
          </Show>

          <Show when={allObjects().length === 0}>
            <div class="empty-state">
              <p>No memories yet</p>
              <p class="hint">Import files to build your memory</p>
            </div>
          </Show>

          <For each={allObjects()}>
            {(obj) => (
              <div
                class={`memory-item ${selectedSuid() === obj.suid ? "selected" : ""}`}
                onClick={() => setSelectedSuid(obj.suid)}
              >
                <div class="item-header">
                  <span class="item-icon">{contentTypeIcon(obj.content_type)}</span>
                  <span class="item-title">{obj.title || obj.suid.slice(0, 8)}</span>
                  <span
                    class="item-tier"
                    style={{ color: tierColor(obj.tier) }}
                    title={obj.tier}
                  >
                    {tierIcon(obj.tier)}
                  </span>
                </div>
                <div class="item-preview">
                  {truncate(obj.content, 60)}
                </div>
                <div class="item-meta">
                  <span class="date">{formatDate(obj.modified_at)}</span>
                  <span class="version">v{obj.version}</span>
                </div>

                <Show when={selectedSuid() === obj.suid}>
                  <div class="item-actions">
                    <select
                      class="tier-select"
                      value={obj.tier}
                      onClick={(e) => e.stopPropagation()}
                      onChange={(e) => handleTierChange(obj.suid, e.currentTarget.value as SecurityTier)}
                    >
                      <option value="Open">Open</option>
                      <option value="Guarded">Guarded</option>
                      <option value="Sealed">Sealed</option>
                    </select>
                    <button
                      class="delete-btn"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDelete(obj.suid);
                      }}
                      title="Delete"
                    >
                      x
                    </button>
                  </div>
                </Show>
              </div>
            )}
          </For>
        </Show>
      </div>

      <div class="memory-footer">
        <span>{allObjects().length} objects</span>
      </div>
    </div>
  );
};

export default MemoryBrowser;
