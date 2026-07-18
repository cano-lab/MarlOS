import { Component, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import type { BookConfig } from "../services/typesetter-service";
import { trimDimensions } from "../typesetter/book-css";
import { buildResumeCss } from "../typesetter/resume-css";
import "./ResumePreview.css";

/**
 * Reliable preview for the flat resume/custom lane. Instead of Paged.js
 * (which mishandles CSS grid/multi-column), this renders the actual
 * document HTML + resume CSS inside an <iframe> using the webview's own
 * Chromium — the SAME engine that produces the PDF export. So a two-column
 * grid resume looks in the preview exactly as it will in the exported PDF.
 *
 * The page is drawn at its true trim size (a white box with the margins as
 * padding) and scaled with a CSS transform to fit the pane width.
 */
interface ResumePreviewProps {
  config: BookConfig;
  enrichedHtml: string;
  customCss?: string;
  /** When false the parent keeps us mounted but hidden; skip work. */
  active?: boolean;
  /** Fired after each render with the flowed content's bottom edge (px from
   *  the page top) and the full page height in px. The Layout panel's
   *  auto-fit loop uses this to detect overflow / underfill. */
  onMeasured?: (m: { contentPx: number; pagePx: number }) => void;
}

/** The last `margin:` declared inside any `@page {}` block wins (source
 *  order). Returns it so the preview's page box matches whatever margin the
 *  base CSS, a template, or the AI ended up setting. Falls back otherwise. */
function effectivePageMargin(css: string, fallback: string): string {
  const blocks = css.match(/@page[^{]*\{[^}]*\}/g) || [];
  let margin = fallback;
  for (const b of blocks) {
    const m = /margin\s*:\s*([^;}]+)/.exec(b);
    if (m) margin = m[1].trim();
  }
  return margin;
}

/** CSS length (in/mm/cm/pt/px) → CSS px at 96dpi. */
function toPx(dim: string): number {
  const m = /^([\d.]+)\s*(in|mm|cm|pt|px)?$/.exec(dim.trim());
  if (!m) return 816; // letter width fallback
  const v = parseFloat(m[1]);
  switch (m[2] || "px") {
    case "in": return v * 96;
    case "mm": return (v * 96) / 25.4;
    case "cm": return (v * 96) / 2.54;
    case "pt": return (v * 96) / 72;
    default: return v;
  }
}

const ResumePreview: Component<ResumePreviewProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let iframeRef: HTMLIFrameElement | undefined;
  const [scale, setScale] = createSignal(1);
  const [pageH, setPageH] = createSignal(0);

  const trim = () => trimDimensions(props.config.trim.size);
  const pageWpx = () => toPx(trim().width);

  const buildDoc = (): string => {
    const css = buildResumeCss(props.config, props.customCss);
    const m = props.config.trim.margins_in;
    const t = trim();
    // @page doesn't apply on screen, so we reproduce the page margins as
    // padding on the .page box. A template or the AI may override the
    // @page margin, so use the LAST effective @page margin from the
    // composed CSS (falling back to the config margins) — that keeps the
    // preview matching the exported PDF.
    const fallback = `${m.top}in ${m.outside}in ${m.bottom}in ${m.inside}in`;
    const pad = effectivePageMargin(css, fallback);
    // Simulate the printed page: a white box at the trim size with the
    // effective margins as padding.
    return `<!doctype html><html><head><meta charset="utf-8">
<style>
html,body{margin:0;padding:0;background:transparent;}
.page{width:${t.width};min-height:${t.height};background:#fff;box-sizing:border-box;
      padding:${pad};overflow:hidden;}
${css}
</style></head><body>
<div class="page"><main class="resume">${props.enrichedHtml}</main></div>
</body></html>`;
  };

  const fit = () => {
    if (!containerRef) return;
    const avail = containerRef.clientWidth - 48; // container padding
    if (avail <= 0) return;
    setScale(Math.max(0.25, Math.min(2, avail / pageWpx())));
  };

  const sizeToContent = () => {
    const doc = iframeRef?.contentDocument;
    if (!doc) return;
    const page = doc.querySelector(".page") as HTMLElement | null;
    const h = page ? page.getBoundingClientRect().height : doc.body.scrollHeight;
    if (h > 0) setPageH(h);
    if (props.onMeasured && page) {
      const main = doc.querySelector("main.resume") as HTMLElement | null;
      const pageTop = page.getBoundingClientRect().top;
      // Natural content bottom: with a fixed-height (fill-page) container
      // this reports the fixed box, which is why auto-fit measures with
      // fillPage off and only re-enables it in the final CSS.
      const contentPx = main
        ? main.getBoundingClientRect().bottom - pageTop
        : h;
      props.onMeasured({ contentPx, pagePx: toPx(trim().height) });
    }
  };

  // Re-render the iframe document whenever the inputs change.
  createEffect(() => {
    void props.config;
    void props.customCss;
    void props.enrichedHtml;
    if (props.active === false || !iframeRef) return;
    iframeRef.srcdoc = buildDoc();
  });

  const onLoad = () => {
    sizeToContent();
    fit();
  };

  onMount(() => {
    fit();
    const ro = new ResizeObserver(() => fit());
    if (containerRef) ro.observe(containerRef);
    onCleanup(() => ro.disconnect());
  });

  return (
    <div class="resume-preview" ref={containerRef}>
      <div
        class="resume-preview-scaler"
        style={{
          width: `${Math.round(pageWpx() * scale())}px`,
          height: `${Math.round(pageH() * scale())}px`,
        }}
      >
        <iframe
          ref={iframeRef}
          class="resume-preview-frame"
          title="Resume preview"
          onLoad={onLoad}
          style={{
            width: `${pageWpx()}px`,
            height: `${Math.max(pageH(), 100)}px`,
            transform: `scale(${scale()})`,
            "transform-origin": "top left",
          }}
        />
      </div>
    </div>
  );
};

export default ResumePreview;
