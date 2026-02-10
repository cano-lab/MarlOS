import { Component, createSignal, createEffect, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import EmbeddingWaveform from "./EmbeddingWaveform";
import "./EmbeddingComparison.css";

interface SearchResult {
  suid: string;
  name: string;
  score: number;
  summary: string;
}

interface EmbeddingComparisonProps {
  /** First object SUID */
  suid1?: string;
  /** Second object SUID */
  suid2?: string;
  /** Full width mode */
  fullWidth?: boolean;
}

const EmbeddingComparison: Component<EmbeddingComparisonProps> = (props) => {
  const [searchQuery, setSearchQuery] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<SearchResult[]>([]);
  const [selected1, setSelected1] = createSignal<string | null>(props.suid1 || null);
  const [selected2, setSelected2] = createSignal<string | null>(props.suid2 || null);
  const [embedding1, setEmbedding1] = createSignal<number[] | null>(null);
  const [embedding2, setEmbedding2] = createSignal<number[] | null>(null);
  const [name1, setName1] = createSignal<string>("");
  const [name2, setName2] = createSignal<string>("");
  const [loading, setLoading] = createSignal(false);
  const [similarity, setSimilarity] = createSignal<number | null>(null);

  // Fetch embedding for an object
  const fetchEmbedding = async (suid: string): Promise<number[] | null> => {
    try {
      const emb = await invoke<number[] | null>("get_object_embedding", { suid });
      return emb;
    } catch (e) {
      console.error("Failed to fetch embedding:", e);
      return null;
    }
  };

  // Search for objects
  const handleSearch = async () => {
    if (!searchQuery().trim()) return;

    setLoading(true);
    try {
      const results = await invoke<SearchResult[]>("semantic_search", {
        query: searchQuery(),
        limit: 10,
      });
      setSearchResults(results);
    } catch (e) {
      console.error("Search failed:", e);
    } finally {
      setLoading(false);
    }
  };

  // Select an object
  const selectObject = async (suid: string, name: string, slot: 1 | 2) => {
    const embedding = await fetchEmbedding(suid);

    if (slot === 1) {
      setSelected1(suid);
      setEmbedding1(embedding);
      setName1(name);
    } else {
      setSelected2(suid);
      setEmbedding2(embedding);
      setName2(name);
    }

    // Calculate similarity if both embeddings are available
    if (embedding1() && embedding2()) {
      calculateSimilarity();
    }
  };

  // Calculate cosine similarity
  const calculateSimilarity = () => {
    const e1 = embedding1();
    const e2 = embedding2();
    if (!e1 || !e2 || e1.length !== e2.length) {
      setSimilarity(null);
      return;
    }

    let dotProduct = 0;
    let norm1 = 0;
    let norm2 = 0;

    for (let i = 0; i < e1.length; i++) {
      dotProduct += e1[i] * e2[i];
      norm1 += e1[i] * e1[i];
      norm2 += e2[i] * e2[i];
    }

    const sim = dotProduct / (Math.sqrt(norm1) * Math.sqrt(norm2));
    setSimilarity(sim);
  };

  createEffect(() => {
    if (embedding1() && embedding2()) {
      calculateSimilarity();
    }
  });

  // Calculate difference waveform
  const differenceEmbedding = () => {
    const e1 = embedding1();
    const e2 = embedding2();
    if (!e1 || !e2 || e1.length !== e2.length) return null;

    return e1.map((v, i) => v - e2[i]);
  };

  return (
    <div class={`embedding-comparison ${props.fullWidth ? "full-width" : ""}`}>
      <div class="comparison-header">
        <h3>Embedding Comparison</h3>
        <Show when={similarity() !== null}>
          <div class="similarity-display">
            <span class="similarity-label">Similarity:</span>
            <span
              class="similarity-value"
              style={{ color: similarity()! > 0.7 ? "#4caf50" : similarity()! > 0.4 ? "#ff9800" : "#f44336" }}
            >
              {(similarity()! * 100).toFixed(1)}%
            </span>
          </div>
        </Show>
      </div>

      {/* Search bar */}
      <div class="comparison-search">
        <input
          type="text"
          placeholder="Search for objects to compare..."
          value={searchQuery()}
          onInput={(e) => setSearchQuery(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSearch()}
        />
        <button onClick={handleSearch} disabled={loading()}>
          {loading() ? "..." : "Search"}
        </button>
      </div>

      {/* Search results */}
      <Show when={searchResults().length > 0}>
        <div class="search-results">
          <For each={searchResults()}>
            {(result) => (
              <div class="search-result-item">
                <div class="result-info">
                  <span class="result-name">{result.name}</span>
                  <span class="result-score">{(result.score * 100).toFixed(0)}%</span>
                </div>
                <div class="result-actions">
                  <button
                    class={`slot-btn ${selected1() === result.suid ? "selected" : ""}`}
                    onClick={() => selectObject(result.suid, result.name, 1)}
                  >
                    A
                  </button>
                  <button
                    class={`slot-btn ${selected2() === result.suid ? "selected" : ""}`}
                    onClick={() => selectObject(result.suid, result.name, 2)}
                  >
                    B
                  </button>
                </div>
              </div>
            )}
          </For>
        </div>
      </Show>

      {/* Waveform displays */}
      <div class="waveforms-container">
        <Show when={embedding1()}>
          <EmbeddingWaveform
            embedding={embedding1()!}
            label={`A: ${name1()}`}
            width={props.fullWidth ? 600 : 380}
            height={100}
            color="#4a9eff"
          />
        </Show>

        <Show when={embedding2()}>
          <EmbeddingWaveform
            embedding={embedding2()!}
            label={`B: ${name2()}`}
            width={props.fullWidth ? 600 : 380}
            height={100}
            color="#9e4aff"
          />
        </Show>

        <Show when={differenceEmbedding()}>
          <EmbeddingWaveform
            embedding={differenceEmbedding()!}
            label="Difference (A - B)"
            width={props.fullWidth ? 600 : 380}
            height={80}
            color="#ff9800"
          />
        </Show>
      </div>

      {/* Empty state */}
      <Show when={!embedding1() && !embedding2()}>
        <div class="empty-state">
          <p>Search for objects above to compare their embedding vectors</p>
        </div>
      </Show>
    </div>
  );
};

export default EmbeddingComparison;
