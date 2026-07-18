import { Component, createMemo, createSignal, For, Show } from "solid-js";
import {
  defaultLayoutState,
  generateLayoutCss,
  lengthToInches,
  parseLayoutMarker,
  parseSections,
  type LayoutPreset,
  type LayoutState,
  type RegionAssignment,
} from "../typesetter/layout-generator";
import { trimDimensions } from "../typesetter/book-css";
import "./BookLayoutPanel.css";

/**
 * Interactive layout controls for the flat resume/custom lane: pick a
 * configuration (single / two balanced columns / sidebar), assign section
 * boxes to regions, tune type + spacing with sliders, and auto-fit the
 * content to one page. Every change regenerates custom.css deterministically
 * (see layout-generator.ts) and previews live; "Apply & save" persists it.
 * The Style chat can refine the generated CSS afterwards.
 */

export interface LayoutMeasurement {
  seq: number;
  contentPx: number;
  pagePx: number;
}

interface BookLayoutPanelProps {
  /** Flat-lane enriched HTML — section boxes are parsed from it. */
  enrichedHtml: string;
  /** The document's saved custom.css (marker state is restored from it). */
  currentCss: string;
  /** Config trim size (e.g. "letter") — page geometry for the generator. */
  trimSize: string;
  /** Latest preview measurement (null until the preview reports one). */
  measurement: () => LayoutMeasurement | null;
  /** Apply CSS to the preview only (in-memory, no file write). */
  onPreview: (css: string) => void;
  /** Persist CSS to custom.css and apply it. */
  onApply: (css: string) => void | Promise<void>;
  onClose: () => void;
}

const PRESETS: { id: LayoutPreset; name: string; desc: string }[] = [
  { id: "single", name: "Single column", desc: "Classic full-width flow" },
  { id: "two-col", name: "Two columns", desc: "Balanced newspaper columns" },
  { id: "sidebar-left", name: "Sidebar left", desc: "Narrow rail + main column" },
  { id: "sidebar-right", name: "Sidebar right", desc: "Main column + narrow rail" },
];

const BookLayoutPanel: Component<BookLayoutPanelProps> = (props) => {
  const doc = createMemo(() => parseSections(props.enrichedHtml));
  const dims = createMemo(() => {
    const t = trimDimensions(props.trimSize);
    return { widthIn: lengthToInches(t.width), heightIn: lengthToInches(t.height) };
  });

  // Restore controls from the marker in the saved CSS, else defaults.
  const [state, setState] = createSignal<LayoutState>(
    parseLayoutMarker(props.currentCss) ?? defaultLayoutState(doc()),
  );
  const [busy, setBusy] = createSignal(false);
  const [fitting, setFitting] = createSignal(false);
  const [status, setStatus] = createSignal<string | null>(
    parseLayoutMarker(props.currentCss)
      ? "Restored controls from custom.css."
      : "No saved layout found — starting from defaults. Applying replaces custom.css.",
  );

  const css = () => generateLayoutCss(state(), doc(), dims());

  // Live preview, debounced so sliders don't thrash the iframe.
  let previewTimer: number | undefined;
  const schedulePreview = () => {
    window.clearTimeout(previewTimer);
    previewTimer = window.setTimeout(() => props.onPreview(css()), 180);
  };

  const update = (patch: Partial<LayoutState>) => {
    setState((s) => ({ ...s, ...patch }));
    schedulePreview();
  };

  const setAssignment = (id: string, region: RegionAssignment) =>
    setState((s) => ({ ...s, assignments: { ...s.assignments, [id]: region } }));
  const setSpanFull = (id: string, v: boolean) =>
    setState((s) => ({ ...s, spanFull: { ...s.spanFull, [id]: v } }));
  const setKeepWhole = (id: string, v: boolean) =>
    setState((s) => ({ ...s, keepWhole: { ...s.keepWhole, [id]: v } }));

  /** Wait for the preview to report a measurement newer than `seq`. */
  const waitMeasure = async (seq: number, timeoutMs = 2500) => {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      await new Promise((r) => setTimeout(r, 60));
      const m = props.measurement();
      if (m && m.seq > seq) return m;
    }
    return null;
  };

  /** Binary-search the type size so the content fills one page without
   *  overflowing. Measured with fillPage off (natural flow); the final CSS
   *  re-enables the fill/pin toggles so the page still anchors. */
  const autoFit = async () => {
    if (fitting()) return;
    setFitting(true);
    setStatus("Measuring…");
    try {
      let lo = 7.5;
      let hi = 12;
      let best: number | null = null;
      let prevMid = 0;
      for (let i = 0; i < 8; i++) {
        const mid = Math.round(((lo + hi) / 2) * 4) / 4;
        if (mid === prevMid) break;
        prevMid = mid;
        const seq = props.measurement()?.seq ?? 0;
        const trial = { ...state(), typePt: mid, fillPage: false, pinLastLine: false };
        props.onPreview(generateLayoutCss(trial, doc(), dims()));
        const m = await waitMeasure(seq);
        if (!m) {
          setStatus("Can't measure — switch to the Pages view, then try again.");
          return;
        }
        if (m.contentPx > m.pagePx + 2) {
          hi = mid; // overflows — go smaller
        } else {
          best = mid;
          if (m.pagePx - m.contentPx < m.pagePx * 0.06) break; // close enough
          lo = mid; // underfull — go bigger
        }
      }
      const fitted = best ?? lo;
      setState((s) => ({ ...s, typePt: fitted }));
      props.onPreview(css());
      setStatus(
        best !== null
          ? `Fits one page at ${fitted}pt — Apply to keep it.`
          : `Still overflows at ${fitted}pt. Try wider margins, tighter spacing, or a sidebar preset.`,
      );
    } finally {
      setFitting(false);
    }
  };

  const apply = async () => {
    if (busy()) return;
    setBusy(true);
    try {
      await props.onApply(css());
      setStatus("Saved to custom.css ✓");
    } catch (e) {
      setStatus(`Save failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const Slider = (p: {
    label: string;
    min: number;
    max: number;
    step: number;
    unit: string;
    value: () => number;
    onChange: (v: number) => void;
  }) => (
    <label class="layout-slider">
      <span class="layout-slider-label">
        {p.label}
        <span class="layout-slider-value">
          {p.value()}
          {p.unit}
        </span>
      </span>
      <input
        type="range"
        min={p.min}
        max={p.max}
        step={p.step}
        value={p.value()}
        disabled={fitting()}
        onInput={(e) => p.onChange(parseFloat(e.currentTarget.value))}
      />
    </label>
  );

  const isSidebar = () => state().preset === "sidebar-left" || state().preset === "sidebar-right";

  return (
    <div class="layout-panel">
      <div class="layout-panel-header">
        <span class="layout-panel-title">📐 Layout</span>
        <button class="layout-panel-x" onClick={props.onClose} title="Close">
          ✕
        </button>
      </div>

      <div class="layout-panel-body">
        <div class="layout-group">
          <div class="layout-group-label">Configuration</div>
          <div class="layout-presets">
            <For each={PRESETS}>
              {(p) => (
                <button
                  classList={{ "layout-preset": true, active: state().preset === p.id }}
                  disabled={fitting()}
                  onClick={() => update({ preset: p.id })}
                >
                  <span class="layout-preset-name">{p.name}</span>
                  <span class="layout-preset-desc">{p.desc}</span>
                </button>
              )}
            </For>
          </div>
        </div>

        <Show when={doc().sections.length > 0 && state().preset !== "single"}>
          <div class="layout-group">
            <div class="layout-group-label">Section boxes</div>
            <For each={doc().sections}>
              {(s) => (
                <div class="layout-chip">
                  <span class="layout-chip-title" title={s.id}>
                    {s.title}
                  </span>
                  <Show when={isSidebar()}>
                    <select
                      class="layout-chip-select"
                      value={state().assignments[s.id] ?? "main"}
                      disabled={fitting()}
                      onChange={(e) => {
                        setAssignment(s.id, e.currentTarget.value as RegionAssignment);
                        schedulePreview();
                      }}
                    >
                      <option value="sidebar">Sidebar</option>
                      <option value="main">Main</option>
                      <option value="full">Full width</option>
                    </select>
                  </Show>
                  <Show when={state().preset === "two-col"}>
                    <label class="layout-chip-check">
                      <input
                        type="checkbox"
                        checked={!!state().spanFull[s.id]}
                        disabled={fitting()}
                        onChange={(e) => {
                          setSpanFull(s.id, e.currentTarget.checked);
                          schedulePreview();
                        }}
                      />
                      Span
                    </label>
                    <label class="layout-chip-check" title="Never split this section across columns">
                      <input
                        type="checkbox"
                        checked={!!state().keepWhole[s.id]}
                        disabled={fitting() || !!state().spanFull[s.id]}
                        onChange={(e) => {
                          setKeepWhole(s.id, e.currentTarget.checked);
                          schedulePreview();
                        }}
                      />
                      Keep whole
                    </label>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>

        <div class="layout-group">
          <div class="layout-group-label">Tune</div>
          <Slider
            label="Type size"
            min={7.5}
            max={12}
            step={0.25}
            unit="pt"
            value={() => state().typePt}
            onChange={(v) => update({ typePt: v })}
          />
          <Slider
            label="Spacing"
            min={0.8}
            max={1.4}
            step={0.05}
            unit="×"
            value={() => state().density}
            onChange={(v) => update({ density: v })}
          />
          <Show when={state().preset !== "single"}>
            <Slider
              label="Column gap"
              min={0.15}
              max={0.6}
              step={0.01}
              unit="in"
              value={() => state().gapIn}
              onChange={(v) => update({ gapIn: v })}
            />
          </Show>
          <Show when={isSidebar()}>
            <Slider
              label="Sidebar width"
              min={1.8}
              max={3.6}
              step={0.05}
              unit="in"
              value={() => state().sidebarIn}
              onChange={(v) => update({ sidebarIn: v })}
            />
          </Show>
          <Slider
            label="Page margin"
            min={0.3}
            max={1}
            step={0.05}
            unit="in"
            value={() => state().marginIn}
            onChange={(v) => update({ marginIn: v })}
          />
          <label class="layout-slider">
            <span class="layout-slider-label">Accent</span>
            <span class="layout-accent">
              <input
                type="color"
                value={state().accent}
                disabled={fitting()}
                onInput={(e) => update({ accent: e.currentTarget.value })}
              />
              <span class="layout-slider-value">{state().accent}</span>
            </span>
          </label>
          <label class="layout-check">
            <input
              type="checkbox"
              checked={state().fillPage}
              disabled={fitting()}
              onChange={(e) => update({ fillPage: e.currentTarget.checked })}
            />
            Fill the whole page
          </label>
          <label
            class="layout-check"
            title={
              doc().lastParagraphSelector
                ? "Pin the final paragraph (references etc.) to the page bottom, centred"
                : "No trailing paragraph found to pin"
            }
          >
            <input
              type="checkbox"
              checked={state().pinLastLine && !!doc().lastParagraphSelector}
              disabled={fitting() || !state().fillPage || !doc().lastParagraphSelector}
              onChange={(e) => update({ pinLastLine: e.currentTarget.checked })}
            />
            Pin last line to page bottom
          </label>
        </div>

        <div class="layout-group">
          <button class="layout-autofit" disabled={fitting()} onClick={() => void autoFit()}>
            {fitting() ? "Measuring…" : "Auto-fit to one page"}
          </button>
          <Show when={status()}>
            <div class="layout-status">{status()}</div>
          </Show>
        </div>
      </div>

      <div class="layout-panel-foot">
        <button class="layout-apply" disabled={busy() || fitting()} onClick={() => void apply()}>
          {busy() ? "Saving…" : "Apply & save to custom.css"}
        </button>
        <span class="layout-hint">Preview updates live; Apply persists.</span>
      </div>
    </div>
  );
};

export default BookLayoutPanel;
