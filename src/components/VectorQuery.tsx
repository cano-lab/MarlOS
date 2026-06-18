import { createSignal, Show, For, onMount, onCleanup, type Component } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import EmbeddingSettings from "./EmbeddingSettings";
import IdeaSpace3D from "./IdeaSpace3D";
import EmbeddingWaveform from "./EmbeddingWaveform";
import EmbeddingComparison from "./EmbeddingComparison";
import EmbeddingExplorer from "./EmbeddingExplorer";
import "./VectorQuery.css";

interface VectorSearchResult {
  suid: string;
  title: string | null;
  content_type: string;
  similarity: number;
  preview: string;
  tags: string[];
  security_tier: string;
  path: string | null;
  created_at: string;
}

interface RecentSession {
  id: string;
  title: string;
  date: string;
  activities: number;
}

interface VectorQueryProps {
  onClose?: () => void;
}

const VectorQuery: Component<VectorQueryProps> = (props) => {
  const [query, setQuery] = createSignal("");
  const [results, setResults] = createSignal<VectorSearchResult[]>([]);
  const [selectedResult, setSelectedResult] = createSignal<VectorSearchResult | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [showSettings, setShowSettings] = createSignal(false);
  const [showEmbeddingSettings, setShowEmbeddingSettings] = createSignal(false);
  const [show3DSpace, setShow3DSpace] = createSignal(false);
  const [showWaveform, setShowWaveform] = createSignal(false);
  const [showComparison, setShowComparison] = createSignal(false);
  const [showWaveform3D, setShowWaveform3D] = createSignal(false);
  const [selectedEmbedding, _setSelectedEmbedding] = createSignal<number[] | null>(null);
  const [indexing, setIndexing] = createSignal(false);
  const [statusMessage, setStatusMessage] = createSignal<string | null>(null);
  const [recentSessions, setRecentSessions] = createSignal<RecentSession[]>([]);
  const [reindexProgress, setReindexProgress] = createSignal<{
    stage: string;
    message: string;
    current: number;
    total: number;
    chunks_created?: number;
  } | null>(null);

  // Load recent sessions and set up event listeners
  let unlistenReindex: UnlistenFn | null = null;

  onMount(async () => {
    await loadRecentSessions();

    // Listen for reindex progress events
    unlistenReindex = await listen<{
      stage: string;
      message: string;
      current: number;
      total: number;
      chunks_created?: number;
    }>("reindex-progress", (event) => {
      setReindexProgress(event.payload);
      if (event.payload.stage === "done") {
        setStatusMessage(event.payload.message);
        setTimeout(() => {
          setReindexProgress(null);
          setStatusMessage(null);
        }, 5000);
      }
    });
  });

  onCleanup(() => {
    if (unlistenReindex) {
      unlistenReindex();
    }
  });

  const loadRecentSessions = async () => {
    try {
      const sessions = await invoke<any[]>("get_session_history");
      const recent = sessions
        .slice(0, 5)
        .map((s: any) => ({
          id: s.id,
          title: s.title,
          date: new Date(s.started_at).toLocaleDateString(),
          activities: s.activities?.length || 0,
        }));
      setRecentSessions(recent);
    } catch (error) {
      console.error("Failed to load recent sessions:", error);
    }
  };

  const handleSearch = async () => {
    if (!query().trim()) {
      return;
    }

    setLoading(true);
    setResults([]);
    setSelectedResult(null);
    setStatusMessage(null);

    try {
      const searchResults = await invoke<VectorSearchResult[]>("vector_search", {
        query: query(),
        limit: 20,
        minScore: 0.3,
        contentType: null,
        securityTier: null,
      });

      setResults(searchResults);

      if (searchResults.length === 0) {
        setStatusMessage(`No results found. Try rephrasing your question.`);
        setTimeout(() => setStatusMessage(null), 3000);
      }
    } catch (error) {
      console.error("Search failed:", error);
      setStatusMessage(`Search failed. Try again.`);
      setTimeout(() => setStatusMessage(null), 3000);
    } finally {
      setLoading(false);
    }
  };

  const findSimilar = async (suid: string) => {
    setLoading(true);
    try {
      const similarResults = await invoke<VectorSearchResult[]>("find_similar", {
        suid,
        limit: 10,
      });
      setResults(similarResults);
      setSelectedResult(null);
    } catch (error) {
      console.error("Find similar failed:", error);
      setStatusMessage("Failed to find similar items.");
      setTimeout(() => setStatusMessage(null), 3000);
    } finally {
      setLoading(false);
    }
  };

  const indexSessions = async () => {
    setIndexing(true);
    setStatusMessage("Indexing your sessions...");

    try {
      const message = await invoke<string>("index_sessions_to_vector_db");
      setStatusMessage(message);
      setTimeout(() => setStatusMessage(null), 5000);
      await loadRecentSessions();
    } catch (error) {
      console.error("Indexing failed:", error);
      setStatusMessage(`Indexing failed: ${error}`);
      setTimeout(() => setStatusMessage(null), 5000);
    } finally {
      setIndexing(false);
    }
  };

  const clearDatabase = async () => {
    if (!confirm("Clear all indexed conversations? This can't be undone.")) {
      return;
    }

    setIndexing(true);
    setStatusMessage("Clearing database...");

    try {
      await invoke<string>("clear_vector_database");
      setStatusMessage("Database cleared. Your conversations are still safe.");
      setTimeout(() => setStatusMessage(null), 3000);
      setResults([]);
    } catch (error) {
      console.error("Clear failed:", error);
      setStatusMessage(`Failed to clear: ${error}`);
      setTimeout(() => setStatusMessage(null), 3000);
    } finally {
      setIndexing(false);
    }
  };

  const forceReindexAll = async () => {
    if (!confirm("Re-embed ALL objects with current model? This may take a while.")) {
      return;
    }

    setIndexing(true);
    setReindexProgress({ stage: "starting", message: "Starting reindex...", current: 0, total: 0 });

    try {
      const message = await invoke<string>("force_reindex_all");
      // Progress listener will handle the "done" message
      console.log("Reindex complete:", message);
    } catch (error) {
      console.error("Force reindex failed:", error);
      setStatusMessage(`Reindex failed: ${error}`);
      setReindexProgress(null);
      setTimeout(() => setStatusMessage(null), 5000);
    } finally {
      setIndexing(false);
    }
  };

  const formatDate = (dateStr: string): string => {
    try {
      const date = new Date(dateStr);
      const now = new Date();
      const diffDays = Math.floor((now.getTime() - date.getTime()) / (1000 * 60 * 60 * 24));

      if (diffDays === 0) return "Today";
      if (diffDays === 1) return "Yesterday";
      if (diffDays < 7) return `${diffDays} days ago`;
      return date.toLocaleDateString();
    } catch {
      return dateStr;
    }
  };

  return (
    <div class="vector-query conversational">
      {/* Header */}
      <div class="conversation-header">
        <h2>🔍 Ask about your past work</h2>
        <div class="header-actions">
          <button
            class={`settings-btn waveform-btn ${showWaveform() ? "active" : ""}`}
            onClick={() => { setShowWaveform(!showWaveform()); setShowComparison(false); }}
            title="Embedding Waveforms"
          >
            📊
          </button>
          <button
            class={`settings-btn compare-btn ${showComparison() ? "active" : ""}`}
            onClick={() => { setShowComparison(!showComparison()); setShowWaveform(false); }}
            title="Compare Embeddings"
          >
            ⚖️
          </button>
          <button
            class="settings-btn wave-3d-btn"
            onClick={() => setShowWaveform3D(true)}
            title="3D Waveform Visualization"
          >
            🌊
          </button>
          <button
            class="settings-btn view-3d-btn"
            onClick={() => setShow3DSpace(true)}
            title="3D Idea Space"
          >
            🌐
          </button>
          <button
            class="settings-btn embedding-btn"
            onClick={() => setShowEmbeddingSettings(true)}
            title="Embedding Settings"
          >
            🧠
          </button>
          <button
            class="settings-btn"
            onClick={() => setShowSettings(!showSettings())}
            title="Database Settings"
          >
            ⚙️
          </button>
          <button class="close-btn" onClick={props.onClose} title="Close">
            ✕
          </button>
        </div>
      </div>

      {/* Search Bar */}
      <div class="conversation-search">
        <input
          type="text"
          class="conversation-input"
          placeholder="What did we work on with vector embeddings?"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          onKeyPress={(e) => e.key === "Enter" && handleSearch()}
        />
        <button
          class="ask-btn"
          onClick={handleSearch}
          disabled={loading() || !query().trim()}
        >
          {loading() ? "..." : "Ask"}
        </button>
      </div>

      {/* Settings Panel (Collapsible) */}
      <Show when={showSettings()}>
        <div class="settings-panel">
          <h4>Database Settings</h4>
          <div class="settings-actions">
            <button
              class="settings-action-btn index-btn"
              onClick={indexSessions}
              disabled={indexing()}
            >
              {indexing() ? "Indexing..." : "📥 Index Sessions"}
            </button>
            <button
              class="settings-action-btn reindex-btn"
              onClick={forceReindexAll}
              disabled={indexing()}
            >
              {indexing() ? "Reindexing..." : "🔄 Force Reindex All"}
            </button>
            <button
              class="settings-action-btn clear-btn"
              onClick={clearDatabase}
              disabled={indexing()}
            >
              🗑️ Clear Database
            </button>
          </div>
          <p class="settings-hint">
            Use "Force Reindex All" to re-embed all objects with the current model (fixes dimension mismatches).
          </p>
        </div>
      </Show>

      {/* Reindex Progress */}
      <Show when={reindexProgress()}>
        <div class="reindex-progress">
          <div class="reindex-progress-header">
            <span class="reindex-stage">{reindexProgress()?.stage === "done" ? "✓" : "⏳"}</span>
            <span class="reindex-message">{reindexProgress()?.message}</span>
          </div>
          <Show when={reindexProgress()?.total && reindexProgress()!.total > 0}>
            <div class="reindex-progress-bar">
              <div
                class="reindex-progress-fill"
                style={{ width: `${Math.round((reindexProgress()!.current / reindexProgress()!.total) * 100)}%` }}
              />
            </div>
            <div class="reindex-progress-stats">
              <span>{reindexProgress()?.current} / {reindexProgress()?.total} objects</span>
              <Show when={reindexProgress()?.chunks_created}>
                <span> • {reindexProgress()?.chunks_created} chunks created</span>
              </Show>
            </div>
          </Show>
        </div>
      </Show>

      {/* Status Message */}
      <Show when={statusMessage() && !reindexProgress()}>
        <div class="status-message">{statusMessage()}</div>
      </Show>

      {/* Recent Sessions */}
      <Show when={results().length === 0 && !loading()}>
        <div class="recent-section">
          <h3>Recent Sessions</h3>
          <Show
            when={recentSessions().length > 0}
            fallback={<div class="empty-state">No recent sessions found</div>}
          >
            <div class="recent-list">
              <For each={recentSessions()}>
                {(session) => (
                  <div class="recent-card">
                    <div class="recent-title">{session.title}</div>
                    <div class="recent-meta">
                      <span>{session.date}</span>
                      <span>•</span>
                      <span>{session.activities} activities</span>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>

      {/* Search Results */}
      <Show when={results().length > 0}>
        <div class="conversation-results">
          <h3>Found {results().length} conversations</h3>
          <div class="conversation-list">
            <For each={results()}>
              {(result) => (
                <div
                  class="conversation-card"
                  onClick={() => setSelectedResult(result)}
                >
                  <Show when={result.title} fallback={
                    <div class="conversation-title">From your conversations</div>
                  }>
                    <div class="conversation-title">{result.title}</div>
                  </Show>

                  <div class="conversation-content">{result.preview}</div>

                  <div class="conversation-footer">
                    <span class="conversation-date">{formatDate(result.created_at)}</span>
                    <button
                      class="similar-link"
                      onClick={(e) => {
                        e.stopPropagation();
                        findSimilar(result.suid);
                      }}
                    >
                      Find more like this
                    </button>
                  </div>
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>

      {/* Detail Panel */}
      <Show when={selectedResult()}>
        <div class="detail-overlay" onClick={() => setSelectedResult(null)}>
          <div class="detail-modal" onClick={(e) => e.stopPropagation()}>
            <div class="detail-header">
              <h3>{selectedResult()!.title || "Conversation Details"}</h3>
              <button
                class="close-detail-btn"
                onClick={() => setSelectedResult(null)}
              >
                ✕
              </button>
            </div>

            <div class="detail-body">
              <div class="detail-content">{selectedResult()!.preview}</div>

              <div class="detail-meta">
                <span>{formatDate(selectedResult()!.created_at)}</span>
                <Show when={selectedResult()!.tags.length > 0}>
                  <span>•</span>
                  <div class="detail-tags">
                    <For each={selectedResult()!.tags}>
                      {(tag) => <span class="tag">{tag}</span>}
                    </For>
                  </div>
                </Show>
              </div>

              <div class="detail-actions">
                <button
                  class="action-btn"
                  onClick={() => findSimilar(selectedResult()!.suid)}
                >
                  Find similar conversations
                </button>
              </div>
            </div>
          </div>
        </div>
      </Show>

      {/* Embedding Settings Modal */}
      <EmbeddingSettings
        isOpen={showEmbeddingSettings()}
        onClose={() => setShowEmbeddingSettings(false)}
        onReindex={async () => {
          setStatusMessage("Reindexing with new embedding configuration...");
          try {
            await indexSessions();
          } catch (e) {
            setStatusMessage(`Reindex failed: ${e}`);
          }
        }}
      />

      {/* 3D Idea Space */}
      <IdeaSpace3D
        isOpen={show3DSpace()}
        onClose={() => setShow3DSpace(false)}
      />

      {/* Embedding Explorer (2D Canvas visualizations) */}
      <EmbeddingExplorer
        isOpen={showWaveform3D()}
        onClose={() => setShowWaveform3D(false)}
      />

      {/* Waveform Panel - inline display */}
      <Show when={showWaveform() && selectedEmbedding()}>
        <div class="waveform-panel">
          <EmbeddingWaveform
            embedding={selectedEmbedding()!}
            label="Selected Result"
            width={350}
            height={120}
          />
        </div>
      </Show>

      {/* Comparison Panel - inline display */}
      <Show when={showComparison()}>
        <div class="comparison-panel">
          <EmbeddingComparison />
        </div>
      </Show>
    </div>
  );
};

export default VectorQuery;
