import { Component, createSignal, createEffect, For, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import {
  SemanticObject,
  ObjectSearchResult,
  WaveformSearchResult,
  SecurityTier,
  searchObjects,
  searchObjectsWaveform,
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

type SearchMode = "cosine" | "waveform" | "compare";
type ViewMode = "search" | "browse";

interface ComparisonItem {
  suid: string;
  title?: string;
  content_type: string;
  tier: SecurityTier;
  cosineScore?: number;
  waveformScore?: number;
  waveformComponents?: {
    cross_correlation: number;
    spectral: number;
    multiscale: number;
  };
  onlyIn: "cosine" | "waveform" | "both";
}

const MemoryBrowser: Component<MemoryBrowserProps> = (_props) => {
  const [searchQuery, setSearchQuery] = createSignal("");
  const [results, setResults] = createSignal<ObjectSearchResult[]>([]);
  const [waveformResults, setWaveformResults] = createSignal<WaveformSearchResult[]>([]);
  const [comparisonResults, setComparisonResults] = createSignal<ComparisonItem[]>([]);
  const [allObjects, setAllObjects] = createSignal<SemanticObject[]>([]);
  const [isSearching, setIsSearching] = createSignal(false);
  const [selectedSuid, setSelectedSuid] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [viewMode, setViewMode] = createSignal<ViewMode>("browse");
  const [searchMode, setSearchMode] = createSignal<SearchMode>("cosine");
  const [tierFilter, setTierFilter] = createSignal<SecurityTier | "all">("all");
  const [saturationDetected, setSaturationDetected] = createSignal(false);

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

      if (searchMode() === "compare") {
        console.log("[MemoryBrowser] Running comparison: both cosine and waveform");

        // Run both searches in parallel
        const [cosineResults, waveformSearchResults] = await Promise.all([
          searchObjects(query, 20, tier),
          searchObjectsWaveform(query, 20, tier),
        ]);

        setResults(cosineResults);
        setWaveformResults(waveformSearchResults);

        if (waveformSearchResults.length > 0) {
          setSaturationDetected(waveformSearchResults[0].saturation_detected);
        }

        // Build comparison results
        const allSuids = new Set([
          ...cosineResults.map(r => r.suid),
          ...waveformSearchResults.map(r => r.suid),
        ]);

        const comparisons: ComparisonItem[] = Array.from(allSuids).map(suid => {
          const cosineItem = cosineResults.find(r => r.suid === suid);
          const waveformItem = waveformSearchResults.find(r => r.suid === suid);

          return {
            suid,
            title: cosineItem?.title || waveformItem?.title,
            content_type: cosineItem?.content_type || waveformItem?.content_type || "text",
            tier: cosineItem?.tier || waveformItem?.tier || "Open",
            cosineScore: cosineItem?.relevance,
            waveformScore: waveformItem?.score,
            waveformComponents: waveformItem?.waveform,
            onlyIn: cosineItem && waveformItem ? "both" :
                     cosineItem ? "cosine" : "waveform",
          };
        });

        // Sort by combined score, favoring items that appear in both
        comparisons.sort((a, b) => {
          const aScore = (a.cosineScore || 0) + (a.waveformScore || 0) + (a.onlyIn === "both" ? 0.2 : 0);
          const bScore = (b.cosineScore || 0) + (b.waveformScore || 0) + (b.onlyIn === "both" ? 0.2 : 0);
          return bScore - aScore;
        });

        setComparisonResults(comparisons);
      } else if (searchMode() === "waveform") {
        console.log("[MemoryBrowser] Using waveform similarity search");
        const waveformSearchResults = await searchObjectsWaveform(query, 20, tier);
        setWaveformResults(waveformSearchResults);

        // Check if saturation was detected
        if (waveformSearchResults.length > 0) {
          setSaturationDetected(waveformSearchResults[0].saturation_detected);
        }

        // Also convert to standard format for display
        const standardResults: ObjectSearchResult[] = waveformSearchResults.map(r => ({
          suid: r.suid,
          title: r.title,
          content_type: r.content_type,
          tier: r.tier,
          relevance: r.score,
        }));
        setResults(standardResults);
        setComparisonResults([]);
      } else {
        console.log("[MemoryBrowser] Using cosine similarity search");
        const searchResults = await searchObjects(query, 20, tier);
        setResults(searchResults);
        setWaveformResults([]);
        setComparisonResults([]);
        setSaturationDetected(false);
      }

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

      <div class="memory-mode-toggle">
        <button
          class={`mode-btn ${searchMode() === "cosine" ? "active" : ""}`}
          onClick={() => setSearchMode("cosine")}
          title="Traditional cosine similarity search"
        >
          Cosine
        </button>
        <button
          class={`mode-btn ${searchMode() === "waveform" ? "active" : ""}`}
          onClick={() => setSearchMode("waveform")}
          title="Waveform similarity: treats embeddings as signals using cross-correlation, spectral analysis, and multi-scale comparison"
        >
          〰️ Waveform
        </button>
        <button
          class={`mode-btn ${searchMode() === "compare" ? "active" : ""}`}
          onClick={() => setSearchMode("compare")}
          title="Compare: run both searches side-by-side to see differences"
        >
          ⚖️ Compare
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
            <span>Search Results ({results().length})</span>
            <Show when={searchMode() === "waveform"}>
              <span class="search-mode-badge">〰️ Waveform</span>
            </Show>
          </div>
          <Show when={saturationDetected()}>
            <div class="saturation-notice">
              ⚠️ Cosine saturation detected - waveform similarity active
            </div>
          </Show>
          <For each={results()}>
            {(result, idx) => {
              const waveformData = () => {
                if (searchMode() === "waveform" && waveformResults()[idx()]) {
                  return waveformResults()[idx()];
                }
                return null;
              };

              return (
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

                  {/* Waveform similarity components */}
                  <Show when={waveformData()}>
                    {(wf) => (
                      <div class="waveform-components">
                        <div class="waveform-bar" title="Cross-correlation (phase alignment)">
                          <span class="waveform-label">↔️</span>
                          <div class="waveform-track">
                            <div
                              class="waveform-fill correlation"
                              style={{ width: `${wf().waveform.cross_correlation * 100}%` }}
                            />
                          </div>
                          <span class="waveform-value">{(wf().waveform.cross_correlation * 100).toFixed(0)}%</span>
                        </div>
                        <div class="waveform-bar" title="Spectral similarity (frequency patterns)">
                          <span class="waveform-label">🌊</span>
                          <div class="waveform-track">
                            <div
                              class="waveform-fill spectral"
                              style={{ width: `${wf().waveform.spectral * 100}%` }}
                            />
                          </div>
                          <span class="waveform-value">{(wf().waveform.spectral * 100).toFixed(0)}%</span>
                        </div>
                        <div class="waveform-bar" title="Multi-scale similarity">
                          <span class="waveform-label">📐</span>
                          <div class="waveform-track">
                            <div
                              class="waveform-fill multiscale"
                              style={{ width: `${wf().waveform.multiscale * 100}%` }}
                            />
                          </div>
                          <span class="waveform-value">{(wf().waveform.multiscale * 100).toFixed(0)}%</span>
                        </div>
                      </div>
                    )}
                  </Show>

                  <div class="item-meta">
                    <span class="relevance">
                      {(result.relevance * 100).toFixed(0)}% match
                    </span>
                  </div>
                </div>
              );
            }}
          </For>
        </Show>

        {/* Comparison View: Side-by-side results */}
        <Show when={viewMode() === "search" && searchMode() === "compare" && comparisonResults().length > 0}>
          <div class="results-header">
            <span>Comparison ({comparisonResults().length} results)</span>
            <span class="search-mode-badge">⚖️ Side-by-side</span>
          </div>
          <Show when={saturationDetected()}>
            <div class="saturation-notice">
              ⚠️ Cosine saturation detected - waveform is finding different results
            </div>
          </Show>
          <For each={comparisonResults()}>
            {(item) => (
              <div
                class={`memory-item comparison-item ${item.onlyIn === "cosine" ? "cosine-only" : ""} ${item.onlyIn === "waveform" ? "waveform-only" : ""}`}
                onClick={() => setSelectedSuid(item.suid)}
              >
                <div class="item-header">
                  <span class="item-icon">{contentTypeIcon(item.content_type as any)}</span>
                  <span class="item-title">{item.title || item.suid.slice(0, 8)}</span>
                  <span class="comparison-badge">
                    {item.onlyIn === "both" ? "🔗 Both" :
                     item.onlyIn === "cosine" ? "📐 Cosine only" :
                     "〰️ Waveform only"}
                  </span>
                  <span
                    class="item-tier"
                    style={{ color: tierColor(item.tier) }}
                  >
                    {tierIcon(item.tier)}
                  </span>
                </div>

                {/* Side-by-side score comparison */}
                <div class="score-comparison">
                  <div class="score-side cosine-side">
                    <span class="score-label">Cosine</span>
                    <div class="score-bar">
                      <div
                        class="score-fill cosine-fill"
                        style={{ width: `${(item.cosineScore || 0) * 100}%` }}
                      />
                    </div>
                    <span class="score-value">
                      {item.cosineScore ? (item.cosineScore * 100).toFixed(0) + "%" : "—"}
                    </span>
                  </div>

                  <div class="score-side waveform-side">
                    <span class="score-label">Waveform</span>
                    <div class="score-bar">
                      <div
                        class="score-fill waveform-fill"
                        style={{ width: `${(item.waveformScore || 0) * 100}%` }}
                      />
                    </div>
                    <span class="score-value">
                      {item.waveformScore ? (item.waveformScore * 100).toFixed(0) + "%" : "—"}
                    </span>
                  </div>

                  {/* Difference indicator */}
                  <Show when={item.cosineScore && item.waveformScore}>
                    <div class="score-diff">
                      {(() => {
                        const diff = (item.waveformScore! - item.cosineScore!) * 100;
                        if (Math.abs(diff) < 5) return <span class="diff-similar">≈</span>;
                        if (diff > 0) return <span class="diff-higher" title={`Waveform +${diff.toFixed(0)}%`}>↑{diff.toFixed(0)}</span>;
                        return <span class="diff-lower" title={`Waveform ${diff.toFixed(0)}%`}>↓{Math.abs(diff).toFixed(0)}</span>;
                      })()}
                    </div>
                  </Show>
                </div>

                {/* Waveform components for waveform-only items */}
                <Show when={item.waveformComponents}>
                  {(wf) => (
                    <div class="waveform-components">
                      <div class="waveform-bar">
                        <span class="waveform-label">↔️</span>
                        <div class="waveform-track">
                          <div
                            class="waveform-fill correlation"
                            style={{ width: `${wf().cross_correlation * 100}%` }}
                          />
                        </div>
                        <span class="waveform-value">{(wf().cross_correlation * 100).toFixed(0)}%</span>
                      </div>
                      <div class="waveform-bar">
                        <span class="waveform-label">🌊</span>
                        <div class="waveform-track">
                          <div
                            class="waveform-fill spectral"
                            style={{ width: `${wf().spectral * 100}%` }}
                          />
                        </div>
                        <span class="waveform-value">{(wf().spectral * 100).toFixed(0)}%</span>
                      </div>
                      <div class="waveform-bar">
                        <span class="waveform-label">📐</span>
                        <div class="waveform-track">
                          <div
                            class="waveform-fill multiscale"
                            style={{ width: `${wf().multiscale * 100}%` }}
                          />
                        </div>
                        <span class="waveform-value">{(wf().multiscale * 100).toFixed(0)}%</span>
                      </div>
                    </div>
                  )}
                </Show>
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
