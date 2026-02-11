import { Component, createSignal, createEffect, createMemo, Show, For, onMount, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./EmbeddingExplorer.css";

interface SearchResult {
  object: {
    id: string;
    kind: string;
    content: string;
    tags: string[];
  };
  score: number;
}

// Color mapping for embedding values
const valToColor = (val: number, min: number, max: number): string => {
  const t = (val - min) / (max - min || 1);
  if (t < 0.5) {
    const s = t * 2;
    return `rgb(${Math.round(15 + s * 20)}, ${Math.round(20 + s * 60)}, ${Math.round(80 + s * 100)})`;
  } else {
    const s = (t - 0.5) * 2;
    return `rgb(${Math.round(35 + s * 220)}, ${Math.round(80 + s * 100)}, ${Math.round(180 - s * 140)})`;
  }
};

// Vector math utilities
const magnitude = (v: number[]): number => Math.sqrt(v.reduce((s, x) => s + x * x, 0));
const dotProduct = (a: number[], b: number[]): number => a.reduce((s, x, i) => s + x * b[i], 0);
const cosineSim = (a: number[], b: number[]): number => {
  const dot = dotProduct(a, b);
  const m = magnitude(a) * magnitude(b);
  return m > 0 ? dot / m : 0;
};
const lerp = (a: number[], b: number[], t: number): number[] => a.map((v, i) => v * (1 - t) + b[i] * t);

// Hilbert curve for 2D mapping
const hilbertD2xy = (n: number, d: number): [number, number] => {
  let x = 0, y = 0, rx: number, ry: number, s: number, t = d;
  for (s = 1; s < n; s *= 2) {
    rx = 1 & Math.floor(t / 2);
    ry = 1 & (t ^ rx);
    if (ry === 0) {
      if (rx === 1) { x = s - 1 - x; y = s - 1 - y; }
      [x, y] = [y, x];
    }
    x += s * rx;
    y += s * ry;
    t = Math.floor(t / 4);
  }
  return [x, y];
};

// Heatmap visualization
const HeatmapViz: Component<{ vector: number[]; compareVector?: number[] | null }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const vector = props.vector;
    const min = Math.min(...vector), max = Math.max(...vector);
    const cols = 64, rows = Math.ceil(vector.length / cols);
    const cellW = canvasRef.width / cols;
    const cellH = canvasRef.height / (props.compareVector ? rows * 2 + 1 : rows);

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, canvasRef.width, canvasRef.height);

    vector.forEach((val, i) => {
      ctx.fillStyle = valToColor(val, min, max);
      ctx.fillRect((i % cols) * cellW, Math.floor(i / cols) * cellH, cellW + 0.5, cellH + 0.5);
    });

    if (props.compareVector) {
      const cMin = Math.min(...props.compareVector), cMax = Math.max(...props.compareVector);
      const offset = (rows + 1) * cellH;
      props.compareVector.forEach((val, i) => {
        ctx.fillStyle = valToColor(val, cMin, cMax);
        ctx.fillRect((i % cols) * cellW, offset + Math.floor(i / cols) * cellH, cellW + 0.5, cellH + 0.5);
      });
    }
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={640} height={props.compareVector ? 400 : 200} />
      <div class="viz-label">dim 0 → 64×16 = {props.vector?.length || 0} → dim {(props.vector?.length || 1) - 1}</div>
    </div>
  );
};

// Hilbert curve visualization
const HilbertViz: Component<{ vector: number[] }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const size = canvasRef.width;
    const n = 32, cellSize = size / n;
    const min = Math.min(...props.vector), max = Math.max(...props.vector);

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, size, size);

    for (let d = 0; d < Math.min(1024, props.vector.length); d++) {
      const [x, y] = hilbertD2xy(n, d);
      ctx.fillStyle = valToColor(props.vector[d], min, max);
      ctx.fillRect(x * cellSize, y * cellSize, cellSize + 0.5, cellSize + 0.5);
    }
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={512} height={512} />
      <div class="viz-label">Hilbert curve — adjacent dims stay spatially close</div>
    </div>
  );
};

// Radial visualization
const RadialViz: Component<{ vector: number[]; compareVector?: number[] | null }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const size = canvasRef.width;
    const cx = size / 2, cy = size / 2, maxR = size * 0.42;

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, size, size);

    // Draw reference circles
    ctx.strokeStyle = "rgba(100,120,160,0.12)";
    ctx.lineWidth = 0.5;
    [0.25, 0.5, 0.75, 1].forEach(r => {
      ctx.beginPath();
      ctx.arc(cx, cy, maxR * r, 0, Math.PI * 2);
      ctx.stroke();
    });

    const drawShape = (v: number[], color: string, alpha: number) => {
      const absMax = Math.max(...v.map(Math.abs));
      ctx.beginPath();
      v.forEach((val, i) => {
        const angle = (i / v.length) * Math.PI * 2 - Math.PI / 2;
        const r = (Math.abs(val) / absMax) * maxR;
        const x = cx + Math.cos(angle) * r, y = cy + Math.sin(angle) * r;
        i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
      });
      ctx.closePath();
      ctx.fillStyle = color.replace(")", `,${alpha * 0.15})`).replace("rgb", "rgba");
      ctx.fill();
      ctx.strokeStyle = color.replace(")", `,${alpha})`).replace("rgb", "rgba");
      ctx.lineWidth = 0.8;
      ctx.stroke();
    };

    if (props.compareVector) drawShape(props.compareVector, "rgb(255,120,80)", 0.5);
    drawShape(props.vector, "rgb(80,160,255)", 0.7);
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={512} height={512} />
      <div class="viz-label">Each spoke = 1 dim, distance = absolute value</div>
    </div>
  );
};

// Waveform visualization
const WaveformViz: Component<{ vector: number[]; compareVector?: number[] | null }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const w = canvasRef.width, h = canvasRef.height;

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, w, h);

    // Center line
    ctx.strokeStyle = "rgba(100,120,160,0.2)";
    ctx.lineWidth = 0.5;
    ctx.beginPath();
    ctx.moveTo(0, h / 2);
    ctx.lineTo(w, h / 2);
    ctx.stroke();

    const drawWave = (v: number[], color: string) => {
      const absMax = Math.max(...v.map(Math.abs));
      ctx.beginPath();
      ctx.strokeStyle = color;
      ctx.lineWidth = 1.2;
      v.forEach((val, i) => {
        const x = (i / (v.length - 1)) * w;
        const y = h / 2 - (val / absMax) * (h * 0.4);
        i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
      });
      ctx.stroke();
    };

    if (props.compareVector) drawWave(props.compareVector, "rgba(255,120,80,0.4)");
    drawWave(props.vector, "rgba(80,160,255,0.85)");
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={1024} height={256} />
      <div class="viz-label">Waveform — x = dim index, y = value (all {props.vector?.length || 0} dimensions)</div>
    </div>
  );
};

// Distribution visualization
const DistributionViz: Component<{ vector: number[]; compareVector?: number[] | null }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const w = canvasRef.width, h = canvasRef.height;

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, w, h);

    const drawHist = (v: number[], color: string) => {
      const bins = 60;
      const min = Math.min(...v), max = Math.max(...v), range = max - min || 1;
      const counts = new Array(bins).fill(0);
      v.forEach(val => {
        counts[Math.min(Math.floor(((val - min) / range) * bins), bins - 1)]++;
      });
      const maxCount = Math.max(...counts);
      ctx.fillStyle = color;
      counts.forEach((count, i) => {
        ctx.fillRect((i / bins) * w, h - (count / maxCount) * h * 0.85, w / bins - 1, (count / maxCount) * h * 0.85);
      });
    };

    if (props.compareVector) drawHist(props.compareVector, "rgba(255,120,80,0.3)");
    drawHist(props.vector, "rgba(80,160,255,0.6)");

    // Stats
    const mean = props.vector.reduce((a, b) => a + b, 0) / props.vector.length;
    const std = Math.sqrt(props.vector.reduce((s, x) => s + (x - mean) ** 2, 0) / props.vector.length);
    ctx.fillStyle = "#8899aa";
    ctx.font = "11px monospace";
    ctx.fillText(`μ=${mean.toFixed(3)}  σ=${std.toFixed(3)}  mag=${magnitude(props.vector).toFixed(2)}`, 10, 16);
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={600} height={200} />
      <div class="viz-label">Value distribution across {props.vector?.length || 0} dimensions</div>
    </div>
  );
};

// Main component
interface EmbeddingExplorerProps {
  isOpen: boolean;
  onClose: () => void;
}

type ViewMode = "waveform" | "heatmap" | "hilbert" | "radial" | "distribution";

const EmbeddingExplorer: Component<EmbeddingExplorerProps> = (props) => {
  const [searchQuery, setSearchQuery] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<SearchResult[]>([]);
  const [selectedSuid, setSelectedSuid] = createSignal<string | null>(null);
  const [selectedName, setSelectedName] = createSignal<string>("");
  const [embedding, setEmbedding] = createSignal<number[] | null>(null);
  const [compareEmbedding, setCompareEmbedding] = createSignal<number[] | null>(null);
  const [compareName, setCompareName] = createSignal<string>("");
  const [loading, setLoading] = createSignal(false);
  const [view, setView] = createSignal<ViewMode>("waveform");
  const [showCompare, setShowCompare] = createSignal(false);
  const [morphT, setMorphT] = createSignal(0);
  const [showMorph, setShowMorph] = createSignal(false);

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

  const selectObject = async (suid: string, name: string, isCompare: boolean = false) => {
    setLoading(true);
    try {
      const emb = await invoke<number[] | null>("get_object_embedding", { suid });
      if (emb && emb.length > 0) {
        if (isCompare) {
          setCompareEmbedding(emb);
          setCompareName(name);
        } else {
          setSelectedSuid(suid);
          setSelectedName(name);
          setEmbedding(emb);
        }
      }
    } catch (e) {
      console.error("Failed to fetch embedding:", e);
    } finally {
      setLoading(false);
    }
  };

  const displayVector = createMemo(() => {
    const emb = embedding();
    const comp = compareEmbedding();
    if (!emb) return null;
    if (showMorph() && comp) {
      return lerp(emb, comp, morphT());
    }
    return emb;
  });

  const stats = createMemo(() => {
    const v = displayVector();
    if (!v) return null;
    const min = Math.min(...v);
    const max = Math.max(...v);
    const mean = v.reduce((a, b) => a + b, 0) / v.length;
    const mag = magnitude(v);
    return { min, max, mean, mag, dims: v.length };
  });

  const similarity = createMemo(() => {
    const a = embedding();
    const b = compareEmbedding();
    if (a && b) return cosineSim(a, b);
    return null;
  });

  const VIEWS: ViewMode[] = ["waveform", "heatmap", "hilbert", "radial", "distribution"];

  return (
    <Show when={props.isOpen}>
      <div class="embedding-explorer-overlay">
        <div class="embedding-explorer-modal">
          <div class="explorer-header">
            <h2>
              <span class="accent">vec</span>[{embedding()?.length || "?"}]
              <span class="subtitle">embedding explorer</span>
            </h2>
            <button class="close-btn" onClick={props.onClose}>×</button>
          </div>

          <div class="explorer-content">
            {/* Sidebar */}
            <div class="explorer-sidebar">
              {/* Search */}
              <div class="sidebar-section">
                <h4>Select Object</h4>
                <div class="search-row">
                  <input
                    type="text"
                    placeholder="Search..."
                    value={searchQuery()}
                    onInput={(e) => setSearchQuery(e.currentTarget.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                  />
                  <button onClick={handleSearch} disabled={loading()}>
                    {loading() ? "..." : "Go"}
                  </button>
                </div>

                <Show when={searchResults().length > 0}>
                  <div class="search-results-list">
                    <For each={searchResults()}>
                      {(result) => (
                        <div class="result-item">
                          <span class="result-name">{result.object.content.slice(0, 40)}...</span>
                          <div class="result-actions">
                            <button
                              class={`slot-btn ${selectedSuid() === result.object.id ? "selected" : ""}`}
                              onClick={() => selectObject(result.object.id, result.object.content.slice(0, 30))}
                            >
                              A
                            </button>
                            <Show when={showCompare() || showMorph()}>
                              <button
                                class={`slot-btn compare ${compareEmbedding() && compareName() === result.object.content.slice(0, 30) ? "selected" : ""}`}
                                onClick={() => selectObject(result.object.id, result.object.content.slice(0, 30), true)}
                              >
                                B
                              </button>
                            </Show>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>

              {/* Mode */}
              <div class="sidebar-section">
                <h4>Mode</h4>
                <div class="mode-buttons">
                  <button
                    class={showCompare() ? "active compare" : ""}
                    onClick={() => { setShowCompare(!showCompare()); setShowMorph(false); }}
                  >
                    Compare
                  </button>
                  <button
                    class={showMorph() ? "active morph" : ""}
                    onClick={() => { setShowMorph(!showMorph()); setShowCompare(false); }}
                  >
                    Morph
                  </button>
                </div>
              </div>

              {/* Morph Slider */}
              <Show when={showMorph() && embedding() && compareEmbedding()}>
                <div class="sidebar-section morph-section">
                  <div class="morph-slider">
                    <span class="morph-label a">A</span>
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step="0.01"
                      value={morphT()}
                      onInput={(e) => setMorphT(parseFloat(e.currentTarget.value))}
                    />
                    <span class="morph-label b">B</span>
                  </div>
                  <div class="morph-value">t = {morphT().toFixed(2)}</div>
                </div>
              </Show>

              {/* Stats */}
              <Show when={stats()}>
                <div class="sidebar-section stats-section">
                  <h4>Stats</h4>
                  <div class="stat-row"><span>Dims:</span><span>{stats()!.dims}</span></div>
                  <div class="stat-row"><span>Min:</span><span>{stats()!.min.toFixed(4)}</span></div>
                  <div class="stat-row"><span>Max:</span><span>{stats()!.max.toFixed(4)}</span></div>
                  <div class="stat-row"><span>Mean:</span><span>{stats()!.mean.toFixed(4)}</span></div>
                  <div class="stat-row"><span>Magnitude:</span><span>{stats()!.mag.toFixed(3)}</span></div>
                  <Show when={similarity() !== null}>
                    <div class="stat-row similarity">
                      <span>A↔B:</span>
                      <span>{similarity()!.toFixed(4)}</span>
                    </div>
                  </Show>
                </div>
              </Show>

              {/* Selected items */}
              <Show when={selectedName()}>
                <div class="sidebar-section selected-section">
                  <div class="selected-item a">
                    <span class="label">A:</span>
                    <span class="name">{selectedName()}</span>
                  </div>
                  <Show when={compareName()}>
                    <div class="selected-item b">
                      <span class="label">B:</span>
                      <span class="name">{compareName()}</span>
                    </div>
                  </Show>
                </div>
              </Show>
            </div>

            {/* Main content */}
            <div class="explorer-main">
              {/* View tabs */}
              <div class="view-tabs">
                <For each={VIEWS}>
                  {(v) => (
                    <button
                      class={view() === v ? "active" : ""}
                      onClick={() => setView(v)}
                    >
                      {v}
                    </button>
                  )}
                </For>
              </div>

              {/* Visualization */}
              <div class="viz-area">
                <Show when={displayVector()} fallback={
                  <div class="empty-state">
                    <p>Search and select an object to visualize its embedding vector</p>
                    <p class="hint">All {1024} dimensions will be displayed</p>
                  </div>
                }>
                  <Show when={view() === "waveform"}>
                    <WaveformViz
                      vector={displayVector()!}
                      compareVector={(showCompare() && !showMorph()) ? compareEmbedding() : null}
                    />
                  </Show>
                  <Show when={view() === "heatmap"}>
                    <HeatmapViz
                      vector={displayVector()!}
                      compareVector={(showCompare() && !showMorph()) ? compareEmbedding() : null}
                    />
                  </Show>
                  <Show when={view() === "hilbert"}>
                    <HilbertViz vector={displayVector()!} />
                  </Show>
                  <Show when={view() === "radial"}>
                    <RadialViz
                      vector={displayVector()!}
                      compareVector={(showCompare() && !showMorph()) ? compareEmbedding() : null}
                    />
                  </Show>
                  <Show when={view() === "distribution"}>
                    <DistributionViz
                      vector={displayVector()!}
                      compareVector={(showCompare() && !showMorph()) ? compareEmbedding() : null}
                    />
                  </Show>
                </Show>
              </div>

              {/* Info panel */}
              <div class="info-panel">
                <Show when={view() === "waveform"}>
                  The vector as a waveform. X = dimension index (0-{embedding()?.length || 1024}), Y = value.
                  Every single dimension is displayed. Patterns reveal the structure of the embedding.
                </Show>
                <Show when={view() === "heatmap"}>
                  Each pixel = one dimension. Color = value (blue=low, purple=high).
                  64×16 grid maps all {embedding()?.length || 1024} dimensions spatially.
                </Show>
                <Show when={view() === "hilbert"}>
                  Hilbert curve maps 1D→2D preserving locality. Adjacent dimensions stay near each other,
                  revealing clusters that a linear layout would hide.
                </Show>
                <Show when={view() === "radial"}>
                  {embedding()?.length || 1024} spokes from center. Distance = absolute value.
                  The shape is the vector's fingerprint. Similar vectors make similar shapes.
                </Show>
                <Show when={view() === "distribution"}>
                  Value distribution across all dimensions. Most embeddings are approximately Gaussian.
                  The distribution shape reveals the vector's statistical character.
                </Show>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default EmbeddingExplorer;
