/**
 * Deterministic layout generator for the flat resume/custom lane — the
 * engine behind the 📐 Layout panel. Instead of asking the AI for CSS, the
 * panel parses the document's section boxes (pandoc `--section-divs` wraps
 * every heading-bounded section in `<section id="…">`) and emits CSS from a
 * small state object, so every control is reproducible and round-trippable.
 *
 * Layout engines:
 *  - single        — plain block flow.
 *  - two-col       — CSS multi-column with `column-fill: balance`. The
 *                    header (h1 block) and any "span" sections use
 *                    `column-span: all`; "keep whole" sections get
 *                    `break-inside: avoid`.
 *  - sidebar-*     — floats. Sidebar sections `float: left/right; clear:
 *                    same` so they stack into a column; main sections get a
 *                    margin clearing the sidebar; "full" sections clear both.
 *                    Floats are the only technique that groups flat sibling
 *                    boxes into a column without grid row-lock.
 *
 * The state is embedded in the output as a `marlos:layout` JSON marker
 * comment so the panel can restore its controls from an existing
 * custom.css. Everything targets Chromium (preview iframe = PDF export).
 */

export interface SectionBox {
  /** Section element id (e.g. "chsummary"). */
  id: string;
  /** Heading text (e.g. "Summary"). */
  title: string;
  /** Approximate text length — used to default keep-whole off for big sections. */
  textLength: number;
}

export interface ParsedDoc {
  /** Selector prefix for the header block ("#chjeremie-roy" or "main.resume"). */
  headerBase: string;
  /** True when the document has an h1 header block. */
  hasHeader: boolean;
  /** h2-level section boxes in document order. */
  sections: SectionBox[];
  /** Unique selector for the trailing paragraph (e.g. a references line), if any. */
  lastParagraphSelector: string | null;
}

export type LayoutPreset = "single" | "two-col" | "sidebar-left" | "sidebar-right";
/** Sidebar presets: where each section lives. */
export type RegionAssignment = "sidebar" | "main" | "full";

export interface LayoutState {
  version: 1;
  preset: LayoutPreset;
  /** Sidebar presets: section id → region. */
  assignments: Record<string, RegionAssignment>;
  /** two-col: section id → span both columns. */
  spanFull: Record<string, boolean>;
  /** two-col: section id → break-inside: avoid. */
  keepWhole: Record<string, boolean>;
  /** Body type size in pt (7.5–12). The auto-fit knob. */
  typePt: number;
  /** Spacing multiplier (0.8–1.4). */
  density: number;
  /** Column gap / sidebar gutter in inches. */
  gapIn: number;
  /** Uniform @page margin in inches. */
  marginIn: number;
  /** Sidebar width in inches (sidebar presets). */
  sidebarIn: number;
  /** Accent colour for headings + header rule. */
  accent: string;
  /** Fix the container height to the page content box (fill the page). */
  fillPage: boolean;
  /** Pin the trailing paragraph to the page bottom (requires fillPage). */
  pinLastLine: boolean;
}

export const LAYOUT_MARKER = "marlos:layout";

/** Sections whose title suggests sidebar material in a resume. */
const SIDEBAR_HINT =
  /summary|education|skill|certif|contact|language|interest|award|profile|objective/i;

/** Sections longer than this many chars are too big for break-inside: avoid
 *  in a balanced column — it would overflow the column box. */
const KEEP_WHOLE_MAX_CHARS = 900;

/** Parse the flat-lane enriched HTML into section boxes. */
export function parseSections(enrichedHtml: string): ParsedDoc {
  const dom = new DOMParser().parseFromString(
    `<main class="resume">${enrichedHtml}</main>`,
    "text/html",
  );
  const main = dom.querySelector("main")!;

  // Header: the first h1, which pandoc may have wrapped in an outer section.
  const h1 = main.querySelector("h1");
  const outer = h1?.closest("section[id]");
  const headerBase = outer ? `#${outer.id}` : "main.resume";

  // Boxes: sections whose first direct-child heading is an h2. When the doc
  // has no h2 sections, fall back to the shallowest sections present.
  const allSections = Array.from(main.querySelectorAll("section[id]"));
  const withLevel = allSections.map((el) => {
    const heading = el.querySelector(":scope > h1, :scope > h2, :scope > h3");
    return {
      el,
      level: heading ? Number(heading.tagName[1]) : 99,
    };
  });
  let boxes = withLevel.filter((s) => s.level === 2);
  if (!boxes.length) {
    const min = Math.min(...withLevel.map((s) => s.level));
    boxes = withLevel.filter((s) => s.level === min);
  }
  const sections: SectionBox[] = boxes.map(({ el }) => ({
    id: el.id,
    title:
      el
        .querySelector(":scope > h1, :scope > h2, :scope > h3")
        ?.textContent?.trim() || el.id,
    textLength: (el.textContent ?? "").trim().length,
  }));

  // Trailing paragraph (references line): the last <p> in document order.
  // Only exposed when we can build a tight unique selector for it.
  let lastParagraphSelector: string | null = null;
  const ps = main.querySelectorAll("p");
  const lastP = ps[ps.length - 1];
  if (lastP) {
    const parentSection = lastP.parentElement?.closest("section[id]");
    if (
      parentSection &&
      lastP.parentElement === parentSection &&
      lastP === parentSection.querySelector(":scope > p:last-of-type")
    ) {
      lastParagraphSelector = `#${parentSection.id} > p:last-of-type`;
    }
  }

  return { headerBase, hasHeader: !!h1, sections, lastParagraphSelector };
}

/** Default state for a freshly parsed document. */
export function defaultLayoutState(doc: ParsedDoc): LayoutState {
  const assignments: Record<string, RegionAssignment> = {};
  const keepWhole: Record<string, boolean> = {};
  for (const s of doc.sections) {
    assignments[s.id] = SIDEBAR_HINT.test(s.title) ? "sidebar" : "main";
    keepWhole[s.id] = s.textLength <= KEEP_WHOLE_MAX_CHARS;
  }
  return {
    version: 1,
    preset: "two-col",
    assignments,
    spanFull: {},
    keepWhole,
    typePt: 9.5,
    density: 1.0,
    gapIn: 0.32,
    marginIn: 0.5,
    sidebarIn: 2.6,
    accent: "#0e5f5c",
    fillPage: true,
    pinLastLine: !!doc.lastParagraphSelector,
  };
}

/** Restore panel state from a generated stylesheet, if it carries the marker. */
export function parseLayoutMarker(css: string): LayoutState | null {
  const m = new RegExp(`/\\*\\s*${LAYOUT_MARKER}\\s+(\\{.*?\\})\\s*\\*/`).exec(css);
  if (!m) return null;
  try {
    const state = JSON.parse(m[1]) as LayoutState;
    return state.version === 1 ? state : null;
  } catch {
    return null;
  }
}

const fmt = (n: number) => String(Math.round(n * 1000) / 1000);

/** "8.5in" → 8.5, "210mm" → 8.27, "612pt" → 8.5. */
export function lengthToInches(dim: string): number {
  const m = /^([\d.]+)\s*(in|mm|cm|pt|px)?$/.exec(dim.trim());
  if (!m) return 8.5;
  const v = parseFloat(m[1]);
  switch (m[2] || "in") {
    case "in": return v;
    case "mm": return v / 25.4;
    case "cm": return v / 2.54;
    case "pt": return v / 72;
    case "px": return v / 96;
    default: return v;
  }
}

export interface PageDims {
  widthIn: number;
  heightIn: number;
}

/** Emit the full custom.css for a layout state. */
export function generateLayoutCss(
  state: LayoutState,
  doc: ParsedDoc,
  page: PageDims,
): string {
  const d = state.density;
  const m = state.marginIn;
  const contentH = page.heightIn - 2 * m;
  const out: string[] = [];

  out.push(`/* ${LAYOUT_MARKER} ${JSON.stringify(state)} */`);
  out.push(`/* Generated by the 📐 Layout panel. Reopen the panel to tweak the`);
  out.push(`   controls; edit by hand or via Style chat and the marker is left behind. */`);
  out.push(``);
  out.push(`@page {`);
  out.push(`  size: ${fmt(page.widthIn)}in ${fmt(page.heightIn)}in;`);
  out.push(`  margin: ${fmt(m)}in;`);
  out.push(`}`);
  out.push(``);

  /* ---------- container ---------- */
  const isSidebar = state.preset === "sidebar-left" || state.preset === "sidebar-right";
  const container = doc.hasHeader ? doc.headerBase : "main.resume";
  const side = state.preset === "sidebar-right" ? "right" : "left";

  out.push(`main.resume {`);
  if (state.fillPage) {
    // A hair under the true content height so rounding never spills to page 2.
    out.push(`  height: ${fmt(contentH - 0.05)}in;`);
    out.push(`  position: relative;`);
    if (state.pinLastLine && doc.lastParagraphSelector) {
      out.push(`  padding-bottom: 0.38in;`);
    }
  }
  if (isSidebar) {
    // Full-height sidebar band painted as a gradient — the sections float over it.
    const band = state.sidebarIn + state.gapIn / 2;
    const dir = side === "left" ? "90deg" : "270deg";
    out.push(`  background: linear-gradient(${dir}, #f4f6f8 0 ${fmt(band)}in, transparent ${fmt(band)}in);`);
  }
  out.push(`  font-size: ${fmt(state.typePt)}pt;`);
  out.push(`  line-height: 1.35;`);
  out.push(`  color: #1f2430;`);
  out.push(`  box-sizing: border-box;`);
  out.push(`}`);
  out.push(``);

  /* ---------- layout engine ---------- */
  if (state.preset === "two-col") {
    out.push(`${container} {`);
    out.push(`  column-count: 2;`);
    out.push(`  column-gap: ${fmt(state.gapIn)}in;`);
    out.push(`  column-fill: balance;`);
    out.push(`}`);
    out.push(``);
    if (doc.hasHeader) {
      out.push(`/* Header spans both columns. */`);
      out.push(`${doc.headerBase} > h1,`);
      out.push(`${doc.headerBase} > h1 + p,`);
      out.push(`${doc.headerBase} > h1 + p + p {`);
      out.push(`  column-span: all;`);
      out.push(`}`);
      out.push(``);
    }
    for (const s of doc.sections) {
      const rules: string[] = [];
      if (state.spanFull[s.id]) rules.push(`column-span: all;`);
      if (state.keepWhole[s.id] && !state.spanFull[s.id]) rules.push(`break-inside: avoid;`);
      if (rules.length) {
        out.push(`#${s.id} { ${rules.join(" ")} }`);
      }
    }
    out.push(``);
  } else if (isSidebar) {
    const gutter = state.sidebarIn + state.gapIn;
    const sidebarIds = doc.sections.filter((s) => state.assignments[s.id] === "sidebar");
    const mainIds = doc.sections.filter((s) => state.assignments[s.id] !== "sidebar");
    const fullIds = doc.sections.filter((s) => state.assignments[s.id] === "full");
    if (sidebarIds.length) {
      out.push(`/* Sidebar sections stack into a floated column. */`);
      out.push(sidebarIds.map((s) => `#${s.id}`).join(`,\n`) + ` {`);
      out.push(`  float: ${side};`);
      out.push(`  clear: ${side};`);
      out.push(`  width: ${fmt(state.sidebarIn)}in;`);
      out.push(`  padding: 0 ${fmt(state.gapIn / 2)}in;`);
      out.push(`  box-sizing: border-box;`);
      out.push(`}`);
      out.push(``);
    }
    if (mainIds.length) {
      out.push(mainIds.map((s) => `#${s.id}`).join(`,\n`) + ` {`);
      out.push(`  margin-${side}: ${fmt(gutter)}in;`);
      out.push(`}`);
      out.push(``);
    }
    if (fullIds.length) {
      out.push(fullIds.map((s) => `#${s.id}`).join(`,\n`) + ` {`);
      out.push(`  clear: both;`);
      out.push(`  margin-${side}: 0;`);
      out.push(`}`);
      out.push(``);
    }
    // Contain the floats so the container's background band wraps them.
    out.push(`${container}::after {`);
    out.push(`  content: "";`);
    out.push(`  display: block;`);
    out.push(`  clear: both;`);
    out.push(`}`);
    out.push(``);
  }

  /* ---------- header typography ---------- */
  if (doc.hasHeader) {
    out.push(`${doc.headerBase} > h1 {`);
    out.push(`  font-size: ${fmt(state.typePt * 2.2)}pt;`);
    out.push(`  font-weight: 700;`);
    out.push(`  letter-spacing: 0.01em;`);
    out.push(`  line-height: 1.05;`);
    out.push(`  color: #14181f;`);
    out.push(`  margin: 0;`);
    out.push(`}`);
    out.push(``);
    out.push(`${doc.headerBase} > h1 + p {`);
    out.push(`  font-size: ${fmt(state.typePt)}pt;`);
    out.push(`  letter-spacing: 0.04em;`);
    out.push(`  color: #33475b;`);
    out.push(`  margin: ${fmt(0.05 * d)}in 0 0;`);
    out.push(`}`);
    out.push(``);
    out.push(`${doc.headerBase} > h1 + p + p {`);
    out.push(`  font-size: ${fmt(state.typePt * 0.85)}pt;`);
    out.push(`  color: #62707f;`);
    out.push(`  margin: ${fmt(0.04 * d)}in 0 ${fmt(0.16 * d)}in;`);
    out.push(`  padding-bottom: ${fmt(0.1 * d)}in;`);
    out.push(`  border-bottom: 2pt solid ${state.accent};`);
    out.push(`}`);
    out.push(``);
  }

  /* ---------- section typography ---------- */
  out.push(`main.resume h2 {`);
  out.push(`  font-size: ${fmt(state.typePt * 0.85)}pt;`);
  out.push(`  font-weight: 700;`);
  out.push(`  text-transform: uppercase;`);
  out.push(`  letter-spacing: 0.14em;`);
  out.push(`  color: ${state.accent};`);
  out.push(`  margin: ${fmt(0.14 * d)}in 0 ${fmt(0.05 * d)}in;`);
  out.push(`  padding-bottom: ${fmt(0.025 * d)}in;`);
  out.push(`  border-bottom: 0.75pt solid #d4dce3;`);
  out.push(`  break-after: avoid;`);
  out.push(`}`);
  out.push(``);
  out.push(`main.resume h3 {`);
  out.push(`  font-size: ${fmt(state.typePt)}pt;`);
  out.push(`  font-weight: 700;`);
  out.push(`  color: #14181f;`);
  out.push(`  line-height: 1.2;`);
  out.push(`  margin: ${fmt(0.1 * d)}in 0 ${fmt(0.015 * d)}in;`);
  out.push(`  break-after: avoid;`);
  out.push(`}`);
  out.push(``);
  out.push(`main.resume h3 + p {`);
  out.push(`  font-size: ${fmt(state.typePt * 0.85)}pt;`);
  out.push(`  color: #62707f;`);
  out.push(`  margin-bottom: ${fmt(0.04 * d)}in;`);
  out.push(`}`);
  out.push(``);
  out.push(`main.resume p { margin: 0 0 ${fmt(0.04 * d)}in; }`);
  // Lists need generous left padding: outside markers hang left of the item
  // text, and Chromium clips column overflow at the column edge in paged
  // media — too little padding and the PDF shears the bullets in half.
  out.push(`main.resume ul { margin: ${fmt(0.03 * d)}in 0 ${fmt(0.05 * d)}in; padding-left: 0.22in; list-style-position: outside; }`);
  out.push(`main.resume li { margin-bottom: ${fmt(0.03 * d)}in; }`);
  out.push(`main.resume ul li:last-child { margin-bottom: 0; }`);
  out.push(`main.resume a { color: inherit; text-decoration: none; }`);
  out.push(`main.resume hr { display: none; }`);
  out.push(`main.resume p, main.resume li { orphans: 2; widows: 2; }`);
  out.push(``);

  /* ---------- pinned trailing line ---------- */
  if (state.fillPage && state.pinLastLine && doc.lastParagraphSelector) {
    out.push(`/* Trailing line (references etc.) pinned to the page bottom. */`);
    out.push(`${doc.lastParagraphSelector} {`);
    out.push(`  position: absolute;`);
    out.push(`  left: 0;`);
    out.push(`  right: 0;`);
    out.push(`  bottom: 0;`);
    out.push(`  margin: 0;`);
    out.push(`  padding-top: 0.07in;`);
    out.push(`  border-top: 0.75pt solid #d4dce3;`);
    out.push(`  text-align: center;`);
    out.push(`  font-size: ${fmt(state.typePt * 0.85)}pt;`);
    out.push(`  color: #62707f;`);
    out.push(`}`);
  }

  return out.join("\n");
}
