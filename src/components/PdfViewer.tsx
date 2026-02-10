import { Component, createSignal, createEffect, onMount, onCleanup, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./PdfViewer.css";

interface PageInfo {
  index: number;
  width_points: number;
  height_points: number;
  width_inches: number;
  height_inches: number;
  width_mm: number;
  height_mm: number;
  rotation: number;
}

interface PdfInfo {
  path: string;
  page_count: number;
  pages: PageInfo[];
  title: string | null;
  author: string | null;
}

interface RenderedPage {
  page_index: number;
  image_data: string;
  width_px: number;
  height_px: number;
  dpi: number;
  scale: number;
}

interface Measurement {
  points: number;
  inches: number;
  mm: number;
  cm: number;
}

interface Point {
  x: number;
  y: number;
}

type MeasureTool = "none" | "distance" | "area" | "calibrate";
type Unit = "mm" | "cm" | "inches" | "points";

interface PdfViewerProps {
  path: string;
  onClose?: () => void;
}

const PdfViewer: Component<PdfViewerProps> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;
  let overlayRef: SVGSVGElement | undefined;
  let containerRef: HTMLDivElement | undefined;

  const [pdfInfo, setPdfInfo] = createSignal<PdfInfo | null>(null);
  const [currentPage, setCurrentPage] = createSignal(0);
  const [zoom, setZoom] = createSignal(100);
  const [dpi, setDpi] = createSignal(200); // Higher DPI for sharper text
  const [renderedPage, setRenderedPage] = createSignal<RenderedPage | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  // Measurement state
  const [measureTool, setMeasureTool] = createSignal<MeasureTool>("none");
  const [unit, setUnit] = createSignal<Unit>("mm");
  const [measurePoints, setMeasurePoints] = createSignal<Point[]>([]);
  const [currentMeasurement, setCurrentMeasurement] = createSignal<Measurement | null>(null);
  const [measurements, setMeasurements] = createSignal<{ points: Point[]; measurement: Measurement; type: MeasureTool }[]>([]);
  const [mousePos, setMousePos] = createSignal<Point | null>(null);

  // Calibration state
  const [calibrationFactor, setCalibrationFactor] = createSignal(1.0);
  const [calibrationInput, setCalibrationInput] = createSignal("");

  // Pan state
  const [pan, setPan] = createSignal({ x: 0, y: 0 });
  const [isPanning, setIsPanning] = createSignal(false);
  const [panStart, setPanStart] = createSignal({ x: 0, y: 0 });

  onMount(async () => {
    await loadPdf();
  });

  const loadPdf = async () => {
    try {
      setLoading(true);
      setError(null);

      const info = await invoke<PdfInfo>("pdf_open", { path: props.path });
      setPdfInfo(info);

      await renderCurrentPage();
    } catch (e) {
      setError(`Failed to load PDF: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const renderCurrentPage = async () => {
    try {
      const rendered = await invoke<RenderedPage>("pdf_render_page", {
        pageIndex: currentPage(),
        dpi: dpi() * (zoom() / 100),
      });
      setRenderedPage(rendered);

      // Draw to canvas
      if (canvasRef && rendered) {
        const ctx = canvasRef.getContext("2d");
        if (ctx) {
          canvasRef.width = rendered.width_px;
          canvasRef.height = rendered.height_px;

          const img = new Image();
          img.onload = () => {
            ctx.drawImage(img, 0, 0);
          };
          img.src = `data:image/png;base64,${rendered.image_data}`;
        }
      }
    } catch (e) {
      setError(`Failed to render page: ${e}`);
    }
  };

  createEffect(() => {
    const _ = [currentPage(), zoom(), dpi()];
    if (pdfInfo()) {
      renderCurrentPage();
    }
  });

  const handleCanvasClick = async (e: MouseEvent) => {
    if (measureTool() === "none" || !canvasRef) return;

    const rect = canvasRef.getBoundingClientRect();
    const x = (e.clientX - rect.left) * (canvasRef.width / rect.width);
    const y = (e.clientY - rect.top) * (canvasRef.height / rect.height);

    const newPoints = [...measurePoints(), { x, y }];
    setMeasurePoints(newPoints);

    if (measureTool() === "distance" && newPoints.length === 2) {
      await measureDistance(newPoints);
    } else if (measureTool() === "area" && e.detail === 2) {
      // Double-click to complete polygon
      await measureArea(newPoints);
    } else if (measureTool() === "calibrate" && newPoints.length === 2) {
      // Show calibration dialog
      const dist = Math.sqrt(
        Math.pow(newPoints[1].x - newPoints[0].x, 2) +
        Math.pow(newPoints[1].y - newPoints[0].y, 2)
      );
      const realDist = prompt("Enter the real-world distance of this line (in current units):");
      if (realDist) {
        const factor = parseFloat(realDist) / dist;
        setCalibrationFactor(factor);
        setMeasurePoints([]);
        setMeasureTool("none");
      }
    }
  };

  const handleCanvasMouseMove = (e: MouseEvent) => {
    if (!canvasRef) return;

    const rect = canvasRef.getBoundingClientRect();
    const x = (e.clientX - rect.left) * (canvasRef.width / rect.width);
    const y = (e.clientY - rect.top) * (canvasRef.height / rect.height);
    setMousePos({ x, y });
  };

  const measureDistance = async (points: Point[]) => {
    const rendered = renderedPage();
    if (!rendered || points.length < 2) return;

    try {
      const measurement = await invoke<Measurement>("pdf_measure_distance", {
        pageIndex: currentPage(),
        x1Px: points[0].x,
        y1Px: points[0].y,
        x2Px: points[1].x,
        y2Px: points[1].y,
        renderScale: rendered.scale,
      });

      // Apply calibration
      const calibrated = applyCalibration(measurement);
      setCurrentMeasurement(calibrated);

      setMeasurements([...measurements(), { points, measurement: calibrated, type: "distance" }]);
      setMeasurePoints([]);
    } catch (e) {
      console.error("Measurement error:", e);
    }
  };

  const measureArea = async (points: Point[]) => {
    const rendered = renderedPage();
    if (!rendered || points.length < 3) return;

    try {
      const measurement = await invoke<Measurement>("pdf_measure_area", {
        pageIndex: currentPage(),
        pointsPx: points.map(p => [p.x, p.y]),
        renderScale: rendered.scale,
      });

      const calibrated = applyCalibration(measurement);
      setCurrentMeasurement(calibrated);

      setMeasurements([...measurements(), { points, measurement: calibrated, type: "area" }]);
      setMeasurePoints([]);
    } catch (e) {
      console.error("Area measurement error:", e);
    }
  };

  const applyCalibration = (m: Measurement): Measurement => {
    const f = calibrationFactor();
    return {
      points: m.points * f,
      inches: m.inches * f,
      mm: m.mm * f,
      cm: m.cm * f,
    };
  };

  const formatMeasurement = (m: Measurement | null, isArea: boolean = false): string => {
    if (!m) return "-";
    const u = unit();
    const suffix = isArea ? "²" : "";

    switch (u) {
      case "mm": return `${m.mm.toFixed(2)} mm${suffix}`;
      case "cm": return `${m.cm.toFixed(2)} cm${suffix}`;
      case "inches": return `${m.inches.toFixed(3)} in${suffix}`;
      case "points": return `${m.points.toFixed(1)} pt${suffix}`;
    }
  };

  const clearMeasurements = () => {
    setMeasurements([]);
    setMeasurePoints([]);
    setCurrentMeasurement(null);
  };

  const zoomIn = () => setZoom(z => Math.min(z + 25, 400));
  const zoomOut = () => setZoom(z => Math.max(z - 25, 25));
  const zoomFit = () => {
    if (containerRef && pdfInfo()) {
      const page = pdfInfo()!.pages[currentPage()];
      const containerWidth = containerRef.clientWidth - 100;
      const containerHeight = containerRef.clientHeight - 100;
      const pageWidth = page.width_points * (dpi() / 72);
      const pageHeight = page.height_points * (dpi() / 72);

      const scaleX = containerWidth / pageWidth;
      const scaleY = containerHeight / pageHeight;
      const scale = Math.min(scaleX, scaleY);

      setZoom(Math.round(scale * 100));
    }
  };

  const prevPage = () => setCurrentPage(p => Math.max(0, p - 1));
  const nextPage = () => setCurrentPage(p => Math.min((pdfInfo()?.page_count || 1) - 1, p + 1));

  return (
    <div class="pdf-viewer">
      <div class="pdf-toolbar">
        <div class="toolbar-group">
          <button class="toolbar-btn" onClick={props.onClose} title="Close">
            ✕
          </button>
          <span class="toolbar-divider" />
          <button class="toolbar-btn" onClick={prevPage} disabled={currentPage() === 0}>
            ◀
          </button>
          <span class="page-indicator">
            {currentPage() + 1} / {pdfInfo()?.page_count || 0}
          </span>
          <button class="toolbar-btn" onClick={nextPage} disabled={currentPage() >= (pdfInfo()?.page_count || 1) - 1}>
            ▶
          </button>
        </div>

        <div class="toolbar-group">
          <button class="toolbar-btn" onClick={zoomOut} title="Zoom Out">−</button>
          <span class="zoom-indicator">{zoom()}%</span>
          <button class="toolbar-btn" onClick={zoomIn} title="Zoom In">+</button>
          <button class="toolbar-btn" onClick={zoomFit} title="Fit to Window">⊡</button>
        </div>

        <div class="toolbar-group">
          <span class="toolbar-label">Measure:</span>
          <button
            class={`toolbar-btn ${measureTool() === "distance" ? "active" : ""}`}
            onClick={() => { setMeasureTool(measureTool() === "distance" ? "none" : "distance"); setMeasurePoints([]); }}
            title="Distance Tool"
          >
            📏
          </button>
          <button
            class={`toolbar-btn ${measureTool() === "area" ? "active" : ""}`}
            onClick={() => { setMeasureTool(measureTool() === "area" ? "none" : "area"); setMeasurePoints([]); }}
            title="Area Tool (double-click to complete)"
          >
            ⬡
          </button>
          <button
            class={`toolbar-btn ${measureTool() === "calibrate" ? "active" : ""}`}
            onClick={() => { setMeasureTool(measureTool() === "calibrate" ? "none" : "calibrate"); setMeasurePoints([]); }}
            title="Calibrate Scale"
          >
            ⚖
          </button>
          <button class="toolbar-btn" onClick={clearMeasurements} title="Clear Measurements">
            🗑
          </button>
        </div>

        <div class="toolbar-group">
          <span class="toolbar-label">Unit:</span>
          <select
            class="unit-select"
            value={unit()}
            onChange={(e) => setUnit(e.currentTarget.value as Unit)}
          >
            <option value="mm">mm</option>
            <option value="cm">cm</option>
            <option value="inches">inches</option>
            <option value="points">points</option>
          </select>
        </div>
      </div>

      <div class="pdf-content" ref={containerRef}>
        <Show when={loading()}>
          <div class="pdf-loading">Loading PDF...</div>
        </Show>

        <Show when={error()}>
          <div class="pdf-error">{error()}</div>
        </Show>

        <Show when={!loading() && !error() && renderedPage()}>
          <div
            class="pdf-canvas-container"
            style={{
              transform: `translate(${pan().x}px, ${pan().y}px)`,
            }}
          >
            <canvas
              ref={canvasRef}
              class="pdf-canvas"
              onClick={handleCanvasClick}
              onMouseMove={handleCanvasMouseMove}
              style={{ cursor: measureTool() !== "none" ? "crosshair" : "default" }}
            />

            {/* Measurement overlay */}
            <svg
              ref={overlayRef}
              class="pdf-overlay"
              width={renderedPage()?.width_px || 0}
              height={renderedPage()?.height_px || 0}
            >
              {/* Draw saved measurements */}
              <For each={measurements()}>
                {(m) => (
                  <>
                    <Show when={m.type === "distance"}>
                      <line
                        x1={m.points[0].x}
                        y1={m.points[0].y}
                        x2={m.points[1].x}
                        y2={m.points[1].y}
                        stroke="#ff6b6b"
                        stroke-width="2"
                      />
                      <text
                        x={(m.points[0].x + m.points[1].x) / 2}
                        y={(m.points[0].y + m.points[1].y) / 2 - 10}
                        fill="#ff6b6b"
                        font-size="14"
                        font-weight="bold"
                        text-anchor="middle"
                      >
                        {formatMeasurement(m.measurement)}
                      </text>
                    </Show>
                    <Show when={m.type === "area"}>
                      <polygon
                        points={m.points.map(p => `${p.x},${p.y}`).join(" ")}
                        fill="rgba(255, 107, 107, 0.2)"
                        stroke="#ff6b6b"
                        stroke-width="2"
                      />
                      <text
                        x={m.points.reduce((s, p) => s + p.x, 0) / m.points.length}
                        y={m.points.reduce((s, p) => s + p.y, 0) / m.points.length}
                        fill="#ff6b6b"
                        font-size="14"
                        font-weight="bold"
                        text-anchor="middle"
                      >
                        {formatMeasurement(m.measurement, true)}
                      </text>
                    </Show>
                  </>
                )}
              </For>

              {/* Draw current measurement in progress */}
              <For each={measurePoints()}>
                {(point, i) => (
                  <>
                    <circle cx={point.x} cy={point.y} r="5" fill="#569cd6" />
                    <Show when={i() > 0}>
                      <line
                        x1={measurePoints()[i() - 1].x}
                        y1={measurePoints()[i() - 1].y}
                        x2={point.x}
                        y2={point.y}
                        stroke="#569cd6"
                        stroke-width="2"
                        stroke-dasharray="5,5"
                      />
                    </Show>
                  </>
                )}
              </For>

              {/* Draw line to cursor */}
              <Show when={measurePoints().length > 0 && mousePos() && measureTool() !== "none"}>
                <line
                  x1={measurePoints()[measurePoints().length - 1].x}
                  y1={measurePoints()[measurePoints().length - 1].y}
                  x2={mousePos()!.x}
                  y2={mousePos()!.y}
                  stroke="#569cd6"
                  stroke-width="1"
                  stroke-dasharray="3,3"
                  opacity="0.7"
                />
              </Show>
            </svg>
          </div>
        </Show>
      </div>

      <div class="pdf-status">
        <Show when={pdfInfo()}>
          <span>
            Page: {pdfInfo()!.pages[currentPage()].width_mm.toFixed(1)} × {pdfInfo()!.pages[currentPage()].height_mm.toFixed(1)} mm
          </span>
        </Show>
        <Show when={currentMeasurement()}>
          <span class="measurement-result">
            Last: {formatMeasurement(currentMeasurement(), measureTool() === "area")}
          </span>
        </Show>
        <Show when={calibrationFactor() !== 1.0}>
          <span class="calibration-indicator">
            Calibrated: {calibrationFactor().toFixed(4)}×
          </span>
        </Show>
        <span class="status-spacer" />
        <span class="dpi-control">
          DPI:
          <input
            type="range"
            min="100"
            max="300"
            step="25"
            value={dpi()}
            onInput={(e) => setDpi(parseInt(e.currentTarget.value))}
            title={`${dpi()} DPI - Higher = Sharper`}
          />
          <span class="dpi-value">{dpi()}</span>
        </span>
      </div>
    </div>
  );
};

export default PdfViewer;
