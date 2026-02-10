import { Component, createSignal, createEffect } from "solid-js";
import "./EmbeddingWaveform.css";

interface EmbeddingWaveformProps {
  /** The embedding vector to visualize */
  embedding: number[];
  /** Label for the waveform */
  label?: string;
  /** Width of the visualization */
  width?: number;
  /** Height of the visualization */
  height?: number;
  /** Color scheme */
  color?: string;
  /** Show grid lines */
  showGrid?: boolean;
  /** Enable hover to see values */
  interactive?: boolean;
}

const EmbeddingWaveform: Component<EmbeddingWaveformProps> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;
  const [hoverIndex, setHoverIndex] = createSignal<number | null>(null);
  const [hoverValue, setHoverValue] = createSignal<number | null>(null);

  const width = () => props.width || 400;
  const height = () => props.height || 100;
  const color = () => props.color || "#4a9eff";
  const showGrid = () => props.showGrid !== false;
  const interactive = () => props.interactive !== false;

  const drawWaveform = () => {
    const canvas = canvasRef;
    if (!canvas || !props.embedding || props.embedding.length === 0) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const w = width();
    const h = height();
    const embedding = props.embedding;
    const len = embedding.length;

    // Set canvas size
    canvas.width = w * window.devicePixelRatio;
    canvas.height = h * window.devicePixelRatio;
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    ctx.scale(window.devicePixelRatio, window.devicePixelRatio);

    // Clear
    ctx.fillStyle = "var(--bg-secondary)";
    ctx.fillRect(0, 0, w, h);

    // Find min/max for normalization
    let min = Infinity;
    let max = -Infinity;
    for (const v of embedding) {
      if (v < min) min = v;
      if (v > max) max = v;
    }
    const range = max - min || 1;

    // Draw grid
    if (showGrid()) {
      ctx.strokeStyle = "var(--border)";
      ctx.lineWidth = 0.5;

      // Horizontal center line (zero line)
      const zeroY = h - ((0 - min) / range) * h;
      ctx.beginPath();
      ctx.setLineDash([2, 2]);
      ctx.moveTo(0, zeroY);
      ctx.lineTo(w, zeroY);
      ctx.stroke();
      ctx.setLineDash([]);

      // Vertical divisions (every 128 dimensions)
      ctx.strokeStyle = "rgba(128, 128, 128, 0.2)";
      for (let i = 128; i < len; i += 128) {
        const x = (i / len) * w;
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
        ctx.stroke();
      }
    }

    // Draw waveform as filled area
    ctx.beginPath();
    ctx.moveTo(0, h);

    for (let i = 0; i < len; i++) {
      const x = (i / len) * w;
      const normalized = (embedding[i] - min) / range;
      const y = h - normalized * h;
      ctx.lineTo(x, y);
    }

    ctx.lineTo(w, h);
    ctx.closePath();

    // Gradient fill
    const gradient = ctx.createLinearGradient(0, 0, 0, h);
    gradient.addColorStop(0, color() + "80");
    gradient.addColorStop(1, color() + "10");
    ctx.fillStyle = gradient;
    ctx.fill();

    // Draw line on top
    ctx.beginPath();
    for (let i = 0; i < len; i++) {
      const x = (i / len) * w;
      const normalized = (embedding[i] - min) / range;
      const y = h - normalized * h;
      if (i === 0) {
        ctx.moveTo(x, y);
      } else {
        ctx.lineTo(x, y);
      }
    }
    ctx.strokeStyle = color();
    ctx.lineWidth = 1.5;
    ctx.stroke();

    // Draw hover indicator
    const idx = hoverIndex();
    if (idx !== null && interactive()) {
      const x = (idx / len) * w;
      const normalized = (embedding[idx] - min) / range;
      const y = h - normalized * h;

      // Vertical line
      ctx.strokeStyle = "#fff";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, h);
      ctx.stroke();

      // Dot at value
      ctx.fillStyle = "#fff";
      ctx.beginPath();
      ctx.arc(x, y, 4, 0, Math.PI * 2);
      ctx.fill();
    }
  };

  createEffect(() => {
    // Redraw when props change
    props.embedding;
    hoverIndex();
    drawWaveform();
  });

  const handleMouseMove = (e: MouseEvent) => {
    if (!interactive() || !canvasRef) return;

    const rect = canvasRef.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const ratio = x / rect.width;
    const idx = Math.floor(ratio * props.embedding.length);

    if (idx >= 0 && idx < props.embedding.length) {
      setHoverIndex(idx);
      setHoverValue(props.embedding[idx]);
    }
  };

  const handleMouseLeave = () => {
    setHoverIndex(null);
    setHoverValue(null);
  };

  // Stats
  const stats = () => {
    const emb = props.embedding;
    if (!emb || emb.length === 0) return null;

    const magnitude = Math.sqrt(emb.reduce((sum, v) => sum + v * v, 0));
    const mean = emb.reduce((sum, v) => sum + v, 0) / emb.length;
    const variance = emb.reduce((sum, v) => sum + (v - mean) ** 2, 0) / emb.length;
    const std = Math.sqrt(variance);

    return { magnitude, mean, std, dimensions: emb.length };
  };

  return (
    <div class="embedding-waveform">
      {props.label && <div class="waveform-label">{props.label}</div>}

      <div class="waveform-container">
        <canvas
          ref={canvasRef}
          onMouseMove={handleMouseMove}
          onMouseLeave={handleMouseLeave}
          class="waveform-canvas"
        />

        {interactive() && hoverIndex() !== null && (
          <div class="waveform-tooltip">
            <span class="tooltip-index">dim[{hoverIndex()}]</span>
            <span class="tooltip-value">{hoverValue()?.toFixed(4)}</span>
          </div>
        )}
      </div>

      {stats() && (
        <div class="waveform-stats">
          <span class="stat">
            <span class="stat-label">Dims</span>
            <span class="stat-value">{stats()!.dimensions}</span>
          </span>
          <span class="stat">
            <span class="stat-label">Mag</span>
            <span class="stat-value">{stats()!.magnitude.toFixed(2)}</span>
          </span>
          <span class="stat">
            <span class="stat-label">Mean</span>
            <span class="stat-value">{stats()!.mean.toFixed(4)}</span>
          </span>
          <span class="stat">
            <span class="stat-label">Std</span>
            <span class="stat-value">{stats()!.std.toFixed(4)}</span>
          </span>
        </div>
      )}
    </div>
  );
};

export default EmbeddingWaveform;
