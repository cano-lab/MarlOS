import { Component, createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { convertFileSrc } from "@tauri-apps/api/core";
import katex from "katex";
// EB Garamond weights — registered globally; ok to import multiple times.
import "@fontsource/eb-garamond/400.css";
import "@fontsource/eb-garamond/400-italic.css";
import "@fontsource/eb-garamond/600.css";
import "@fontsource/eb-garamond/600-italic.css";
// Accessibility fonts. Loaded eagerly so the body-font picker can
// switch instantly — the browser already has the families when
// Paged.js repaginates with the new font-family.
import "@fontsource/opendyslexic/400.css";
import "@fontsource/opendyslexic/400-italic.css";
import "@fontsource/opendyslexic/700.css";
import "@fontsource/atkinson-hyperlegible/400.css";
import "@fontsource/atkinson-hyperlegible/400-italic.css";
import "@fontsource/atkinson-hyperlegible/700.css";
import "@fontsource/lexend/400.css";
import "@fontsource/lexend/600.css";
import "@fontsource/lexend/700.css";
import { buildBookCss } from "../typesetter/book-css";
import type { BookConfig, SectionAnchor, ScrollSurface } from "../services/typesetter-service";
import "./BookPagedPreview.css";

interface BookPagedPreviewProps {
  config: BookConfig;
  enrichedHtml: string;
  frontCoverPath?: string | null;
  backCoverPath?: string | null;
  /** 1-based BookSection.order to scroll into view on mount/prop change. */
  currentSectionOrder?: number;
  /** Reports the topmost visible section's order as the user scrolls. */
  onSectionChange?: (order: number) => void;
  /** When false, the component stays mounted but skips pagination —
   *  lets the parent keep us hidden via CSS without triggering the
   *  expensive Paged.js work. Repagination resumes as soon as
   *  `active` flips to true with stale content. */
  active?: boolean;
  /** When true, suppress the programmatic scroll-to-currentSectionOrder
   *  effect. Used in split mode where the parent does fractional scroll
   *  mirroring — section-boundary snaps would fight that. */
  disableSectionScrollSync?: boolean;
  /** Hands a scroll surface (the .book-paged-stage element + per-section
   *  anchor query) back to the parent once mounted, so the parent can
   *  mirror scrolling between Source and Pages in split mode. */
  onScrollSurfaceReady?: (surface: ScrollSurface | null) => void;
  /** When this prop changes, flash the matching paragraph in the
   *  paginated DOM. `ts` is a freshness token so consecutive edits in
   *  the same paragraph still re-trigger the animation. */
  flashAnchor?: { order: number; paraIndex: number; ts: number } | null;
  /** Fired after each successful pagination, once the DOM is mounted and
   *  the scroll position restored. Lets the parent re-sync the split
   *  panes (the reflow moves the anchors). */
  onPaginated?: () => void;
  /** Right-click on a figure in the preview. Carries the figure's DOM id
   *  (pandoc-prefixed, e.g. "chfig-…"), current layout mode + inset read
   *  from the element, its caption, and the cursor position so the parent
   *  can pop up a margin editor that writes back to the markdown source. */
  onFigureContextMenu?: (info: {
    id: string;
    mode: string;
    inset: string;
    caption: string;
    x: number;
    y: number;
  }) => void;
  /** User's custom stylesheet (custom.css), appended after the generated
   *  CSS. Re-paginates when it changes. */
  customCss?: string;
}

/**
 * Phase C visual preview using Paged.js's Previewer API.
 *
 * Paged.js paginates content into `.pagedjs_page` elements based on the
 * `@page` rules in the supplied stylesheet. We mount the output into a
 * scoped container so it doesn't fight with the rest of the MarlOS UI.
 */
const BookPagedPreview: Component<BookPagedPreviewProps> = (props) => {
  const [status, setStatus] = createSignal<"idle" | "rendering" | "ready" | "error">("idle");
  const [pageCount, setPageCount] = createSignal(0);
  const [error, setError] = createSignal<string | null>(null);
  const [elapsedMs, setElapsedMs] = createSignal(0);

  // Visual zoom on the paginated preview — scales the rendered pages via
  // CSS `zoom` (which affects layout, so scrollbars adjust). Doesn't
  // re-paginate; the underlying Paged.js layout is unchanged. Persisted.
  const PZOOM_KEY = "marlos-book-preview-zoom";
  const PZOOM_MIN = 0.4;
  const PZOOM_MAX = 2.5;
  const readPZoom = (): number => {
    try {
      const v = parseFloat(localStorage.getItem(PZOOM_KEY) ?? "");
      return Number.isFinite(v) && v >= PZOOM_MIN && v <= PZOOM_MAX ? v : 1;
    } catch {
      return 1;
    }
  };
  const [pZoom, setPZoom] = createSignal<number>(readPZoom());
  createEffect(() => {
    try {
      localStorage.setItem(PZOOM_KEY, String(pZoom()));
    } catch {
      /* localStorage unavailable */
    }
  });
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const pZoomIn = () => setPZoom((z) => Math.min(PZOOM_MAX, r1(z + 0.1)));
  const pZoomOut = () => setPZoom((z) => Math.max(PZOOM_MIN, r1(z - 0.1)));
  const pZoomReset = () => setPZoom(1);
  // Pagination is serialized: only one Paged.js preview() can be in
  // flight at a time, otherwise the second's `mountRef.innerHTML = ""`
  // wipes the first's working DOM mid-layout and Paged.js crashes with
  // "Cannot read properties of null (reading 'getBoundingClientRect')".
  // The createEffect bumps `pendingRequest`; the renderer drains it
  // when it finishes. Multiple bumps coalesce — only the latest props
  // get rendered.
  let pagingActive = false;
  let pendingRequest = false;
  let renderedHtml = "";
  // The html whose pagination already got one automatic retry, so a
  // persistent (non-transient) failure doesn't loop.
  let retriedHtml = "";
  let mountRef: HTMLDivElement | undefined;
  let cancelled = false;
  let suppressReportUntil = 0;
  let scrollRoot: HTMLElement | null = null;
  let scrollListener: (() => void) | null = null;
  let scrollRaf: number | null = null;
  // Scroll position captured just before a re-paginate wipes the DOM,
  // so we can hold the reader's place across the re-flow (see
  // restoreScrollAfterPagination).
  let preScrollTop = 0;
  // Last order this view reported via onSectionChange. Used to dedup
  // round-trips: if the parent's currentSectionOrder ends up matching
  // this, we know it's our own report bouncing back and skip the
  // programmatic scrollIntoView (which would snap the user to the
  // section boundary, fighting their own scroll).
  let lastReportedOrder: number | null = null;

  const scrollToOrder = (order: number) => {
    if (!mountRef) return;
    const target = mountRef.querySelector(
      `[data-section-order="${order}"]`,
    ) as HTMLElement | null;
    if (!target) return;
    suppressReportUntil = performance.now() + 600;
    target.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  /** Cached anchor list — recomputing this on every scroll event is
   *  prohibitively expensive (hundreds of getBoundingClientRect calls
   *  force layout each time, the source of the visible scroll jitter).
   *  Invalidated on every successful pagination and on resize. */
  let cachedAnchors: SectionAnchor[] | null = null;
  const invalidateAnchors = () => {
    cachedAnchors = null;
  };

  /** Compute per-paragraph scrollTop anchors from the paginated DOM.
   *  paraIndex 0 = the section's heading itself; 1+ = each `<p>` inside
   *  the section subtree, in DOM order. Cached — see `invalidateAnchors`. */
  const getAnchors = (): SectionAnchor[] => {
    if (cachedAnchors) return cachedAnchors;
    if (!mountRef) return [];
    const sections = mountRef.querySelectorAll<HTMLElement>("[data-section-order]");
    if (sections.length === 0) return [];
    const rootRect = mountRef.getBoundingClientRect();
    const baseScroll = mountRef.scrollTop;
    const result: SectionAnchor[] = [];
    sections.forEach((sec) => {
      const orderStr = sec.getAttribute("data-section-order");
      const order = orderStr ? parseInt(orderStr, 10) : NaN;
      if (Number.isNaN(order)) return;
      // Section heading itself.
      const secRect = sec.getBoundingClientRect();
      result.push({
        order,
        paraIndex: 0,
        top: secRect.top - rootRect.top + baseScroll,
      });
      // Every <p> inside this section subtree, ignoring nested
      // [data-section-order] (Paged.js may not produce nested ones,
      // but defend anyway). Pandoc emits one <p> per markdown
      // paragraph; lists/code/blockquotes are not <p>, so the index
      // matches what we count on the source side.
      const paragraphs = sec.querySelectorAll<HTMLElement>("p");
      paragraphs.forEach((p, idx) => {
        // Skip <p>s nested inside footnote / aside / nested-section
        // wrappers we don't count on the source side.
        if (p.closest("[data-section-order]") !== sec) return;
        const rect = p.getBoundingClientRect();
        result.push({
          order,
          paraIndex: idx + 1,
          top: rect.top - rootRect.top + baseScroll,
        });
      });
    });
    result.sort((a, b) =>
      a.order !== b.order ? a.order - b.order : a.paraIndex - b.paraIndex,
    );
    cachedAnchors = result;
    return result;
  };

  /** Find the scroll ancestor of mountRef (.book-paged-stage with
   *  overflow:auto in our layout). */
  const findScrollRoot = (): HTMLElement | null => {
    let n: HTMLElement | null = mountRef ?? null;
    while (n && n !== document.body) {
      const overflow = getComputedStyle(n).overflowY;
      if (overflow === "auto" || overflow === "scroll") return n;
      n = n.parentElement;
    }
    return null;
  };

  /** On every scroll frame, find the section whose top is closest to
   *  (and at or above) the viewport top, and report its order to the
   *  parent. Direct query is more reliable than IntersectionObserver
   *  for this — the observer only fires for entries whose state
   *  changed in that frame, so during fast scrolling the actually-
   *  topmost section can be missed. */
  const reportTopSection = () => {
    if (!mountRef || !scrollRoot) return;
    if (performance.now() < suppressReportUntil) return;
    const sections = mountRef.querySelectorAll<HTMLElement>(
      "[data-section-order]",
    );
    if (sections.length === 0) return;
    const rootTop = scrollRoot.getBoundingClientRect().top;
    let bestOrder = 1;
    let bestDistance = -Infinity;
    sections.forEach((s) => {
      const rect = s.getBoundingClientRect();
      // Distance from section's top to scroll-root's top. Negative
      // means the section is above the root top (already scrolled past
      // its top edge); 0 means flush; positive means below.
      const dist = rect.top - rootTop;
      // Pick the section with the highest dist that's still <= 0
      // (closest above or flush).
      if (dist <= 0 && dist > bestDistance) {
        bestDistance = dist;
        const orderStr = s.getAttribute("data-section-order");
        const order = orderStr ? parseInt(orderStr, 10) : NaN;
        if (!Number.isNaN(order)) bestOrder = order;
      }
    });
    if (bestOrder !== lastReportedOrder) {
      lastReportedOrder = bestOrder;
      props.onSectionChange?.(bestOrder);
    }
  };

  const setupScrollListener = () => {
    teardownScrollListener();
    scrollRoot = findScrollRoot();
    if (!scrollRoot) return;
    scrollListener = () => {
      if (scrollRaf !== null) cancelAnimationFrame(scrollRaf);
      scrollRaf = requestAnimationFrame(() => {
        scrollRaf = null;
        reportTopSection();
      });
    };
    scrollRoot.addEventListener("scroll", scrollListener, { passive: true });
  };

  const teardownScrollListener = () => {
    if (scrollListener && scrollRoot) {
      scrollRoot.removeEventListener("scroll", scrollListener);
    }
    scrollListener = null;
    scrollRaf = null;
  };

  const renderPaged = async () => {
    if (pagingActive) {
      // A pagination is already in flight — record that another pass
      // is wanted and let the in-flight one drain it on completion.
      pendingRequest = true;
      return;
    }
    if (!mountRef) return;
    pagingActive = true;
    pendingRequest = false;
    cancelled = false;
    setStatus("rendering");
    setError(null);
    setPageCount(0);
    // Capture the html we're about to render so we can mark it as
    // "rendered" on success and decide whether the queued pass needs
    // to run.
    const htmlForThisRun = props.enrichedHtml;
    // Remember where the reader was before we throw away the DOM, so we
    // can restore it after Paged.js re-flows (otherwise scrollTop resets
    // to 0 and any auto-scroll would yank the view away).
    preScrollTop = mountRef.scrollTop;
    mountRef.innerHTML = "";

    const t0 = performance.now();
    try {
      // Lazy-load Paged.js — its Babel-compiled bundle is heavy, no point
      // pulling it in for users who never open Book Mode.
      const { Previewer } = await import("pagedjs");
      if (cancelled) return;

      // CSS rebuild step 1: visual baseline — @page + body + p +
      // headings, no break-before, no margin boxes, no named pages,
      // no string-set. If this paginates cleanly, we add features
      // back in chunks.
      const css = buildBookCss(props.config, props.customCss);

      // Prepend the book-title marker so future Phase D string-set
      // rules can populate the verso header. Hidden via display:none.
      const title = (props.config.book.title || "Book")
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");

      const frontCoverHtml = props.frontCoverPath
        ? `<section class="book-cover book-cover-front"><img src="${convertFileSrc(
            props.frontCoverPath,
          )}" alt="Front cover" /></section>\n`
        : "";
      const backCoverHtml = props.backCoverPath
        ? `\n<section class="book-cover book-cover-back"><img src="${convertFileSrc(
            props.backCoverPath,
          )}" alt="Back cover" /></section>`
        : "";

      const content =
        `<header class="book-title-source">${title}</header>\n` +
        frontCoverHtml +
        props.enrichedHtml +
        backCoverHtml;

      const cssBlob = new Blob([css], { type: "text/css" });
      const cssUrl = URL.createObjectURL(cssBlob);

      const previewer = new (Previewer as any)();
      let flow: any;
      try {
        flow = await previewer.preview(content, [cssUrl], mountRef);
      } finally {
        URL.revokeObjectURL(cssUrl);
      }

      if (cancelled) return;
      setPageCount(flow?.total ?? mountRef.querySelectorAll(".pagedjs_page").length);
      setStatus("ready");

      // Re-render KaTeX inside the paginated DOM. Paged.js cloned the HTML,
      // so the math spans need a fresh render pass.
      renderMathInTree(mountRef);

      setupScrollListener();
      renderedHtml = htmlForThisRun;
      retriedHtml = ""; // success — let a future html retry if it needs to
      invalidateAnchors(); // fresh DOM → fresh positions
      // Restore the user's view: prefer the most recent edit anchor
      // (so a freshly-inserted snippet stays in view), fall back to
      // the parent-shared currentSectionOrder. Then notify the parent so
      // it can re-sync the split panes against the fresh anchors.
      queueMicrotask(() => {
        restoreScrollAfterPagination();
        props.onPaginated?.();
      });
    } catch (e) {
      // Paged.js occasionally throws a transient layout race
      // ("Cannot read properties of null (reading 'getBoundingClientRect')")
      // that succeeds on a second pass once layout settles. Auto-retry
      // such a failure ONCE (per html) before surfacing it.
      const transient =
        e instanceof Error && /getBoundingClientRect/.test(e.message);
      if (!cancelled && transient && retriedHtml !== htmlForThisRun) {
        retriedHtml = htmlForThisRun;
        // Don't let the drain also fire; we schedule the retry ourselves
        // after a short delay so the DOM/layout can settle.
        renderedHtml = htmlForThisRun;
        console.warn("Paged.js layout race — retrying pagination once");
        setTimeout(() => {
          if (!cancelled) void renderPaged();
        }, 80);
      } else {
        if (!cancelled) {
          // ProgressEvent (from a failed fetch inside Paged.js) stringifies
          // as "[object ProgressEvent]" by default — extract the failing
          // URL from target so the error message points at the cause.
          let msg: string;
          if (e instanceof Error) {
            msg = e.message;
          } else if (e && typeof e === "object" && "target" in e) {
            const target = (e as { target?: unknown }).target as
              | { url?: string; src?: string; href?: string; tagName?: string }
              | undefined;
            const url =
              target?.url ?? target?.src ?? target?.href ?? "(unknown URL)";
            const tag = target?.tagName ?? "fetch";
            msg = `${e.constructor.name} from ${tag} loading ${url}`;
          } else {
            msg = String(e);
          }
          setError(msg);
          setStatus("error");
          console.error("Pagination error:", e);
        }
        // Mark this html as "attempted (failed)" so the drain doesn't
        // queue another doomed attempt (the old pagination-loop source).
        // A NEW enrichedHtml from a future edit still triggers a fresh try.
        renderedHtml = htmlForThisRun;
      }
    } finally {
      setElapsedMs(Math.round(performance.now() - t0));
      pagingActive = false;
      // Drain a queued request, if any, OR if props changed mid-flight
      // such that what's now in the DOM no longer matches what was
      // requested.
      if (pendingRequest || props.enrichedHtml !== renderedHtml) {
        pendingRequest = false;
        // Microtask gap so the calling effect can resolve cleanly
        // before we kick off the next pass.
        queueMicrotask(() => {
          void renderPaged();
        });
      }
    }
  };

  createEffect(() => {
    // Subscribe to the inputs that, when changed, require fresh
    // pagination. renderPaged itself reads the latest props at call
    // time, so we don't capture them here.
    void props.config;
    void props.customCss; // re-paginate when the custom stylesheet changes
    const html = props.enrichedHtml;
    const isActive = props.active !== false; // default true if undefined

    // Skip pagination when the view is hidden — the parent keeps us
    // mounted for state preservation but doesn't need a typeset
    // preview. We'll repaginate the moment `active` flips back to
    // true with stale content.
    if (!isActive) return;
    if (html === renderedHtml && !pagingActive) return;
    void renderPaged();
  });

  // Anchor cache also invalidates when the stage element resizes —
  // page geometry depends on viewport width via Paged.js's measured
  // sheet sizing.
  let resizeObserver: ResizeObserver | null = null;

  /** Right-click a figure → ask the parent to pop a margin editor. Read
   *  the current layout mode + inset straight off the element so the
   *  popover opens pre-filled. Delegated on the stage so it survives the
   *  DOM wipe every re-pagination does. */
  const onFigureRightClick = (e: MouseEvent) => {
    if (!props.onFigureContextMenu) return;
    const fig = (e.target as HTMLElement | null)?.closest(
      "figure[id]",
    ) as HTMLElement | null;
    if (!fig) return;
    e.preventDefault();
    const cls = fig.className || "";
    const mode = cls.includes("fig-fullpage")
      ? "full-page"
      : cls.includes("fig-bleed")
        ? "bleed"
        : cls.includes("fig-text")
          ? "text"
          : "inline";
    const inset = fig.style.getPropertyValue("--fig-inset").trim() || "0.25in";
    const caption = fig.querySelector("figcaption")?.textContent?.trim() ?? "";
    props.onFigureContextMenu({ id: fig.id, mode, inset, caption, x: e.clientX, y: e.clientY });
  };

  onMount(() => {
    if (typeof ResizeObserver !== "undefined" && mountRef) {
      resizeObserver = new ResizeObserver(() => invalidateAnchors());
      resizeObserver.observe(mountRef);
    }
    mountRef?.addEventListener("contextmenu", onFigureRightClick);
  });

  onCleanup(() => {
    cancelled = true;
    teardownScrollListener();
    resizeObserver?.disconnect();
    resizeObserver = null;
    mountRef?.removeEventListener("contextmenu", onFigureRightClick);
  });

  // Jump to currentSectionOrder when it changes from the parent (e.g. on
  // tab switch from Source) OR when we become visible after being
  // hidden. scrollIntoView on elements inside a display:none container
  // is a no-op, so we wait for active=true before applying.
  //
  // The lastReportedOrder dedup is what makes scrolling not jumpy:
  // when the user scrolls and we update currentSectionOrder ourselves,
  // the parent signal change fires this effect — but the value matches
  // what we just reported, so we skip the scrollIntoView that would
  // snap the user to the section boundary.
  createEffect(() => {
    const order = props.currentSectionOrder;
    const active = props.active !== false;
    if (typeof order !== "number") return;
    if (!active) return;
    if (status() !== "ready") return;
    if (props.disableSectionScrollSync) return;
    if (order === lastReportedOrder) return; // our own report bouncing back
    lastReportedOrder = order;
    queueMicrotask(() => scrollToOrder(order));
  });

  /** Restore the user's view position after a fresh pagination. Pages
   *  re-paginates from scratch (the temp DOM is wiped every time), so
   *  scrollTop resets to 0.
   *
   *  AUTO-SCROLL TEMPORARILY DISABLED. The previous behavior chased the
   *  most-recent edit anchor and, when it couldn't resolve one, fell
   *  back to `scrollToOrder(currentSectionOrder)`. Inserting a page
   *  break (a `<div class="page-break">`, not a paragraph) can't resolve
   *  to a paragraph anchor, so it hit that fallback and jumped the view
   *  — often to the bottom of the document. Until we design a proper
   *  edit-follow sync, just hold the reader's previous scroll position
   *  across the re-flow so editing/inserting never moves the page.
   *
   *  The old anchor-follow logic (using `flashAnchor` + `flashTarget`)
   *  is preserved in git history for when we revisit this. */
  const restoreScrollAfterPagination = () => {
    if (!mountRef) return;
    mountRef.scrollTop = preScrollTop;
  };

  return (
    <div class="book-paged">
      <div class="book-paged-toolbar">
        <Show
          when={status() === "rendering"}
          fallback={
            <span class="book-paged-status">
              <Show when={status() === "ready"}>
                {pageCount()} pages • paginated in {elapsedMs()}ms
              </Show>
              <Show when={status() === "error"}>
                <span class="book-paged-error">Pagination failed</span>
              </Show>
            </span>
          }
        >
          <span class="book-paged-spinner" />
          <span>Paginating...</span>
        </Show>
        <button class="book-paged-btn" onClick={renderPaged}>
          Re-paginate
        </button>
        <div class="book-paged-zoom">
          <button class="book-paged-zoom-btn" title="Zoom out" onClick={pZoomOut} disabled={pZoom() <= PZOOM_MIN + 1e-6}>
            −
          </button>
          <button class="book-paged-zoom-btn book-paged-zoom-pct" title="Reset zoom" onClick={pZoomReset}>
            {Math.round(pZoom() * 100)}%
          </button>
          <button class="book-paged-zoom-btn" title="Zoom in" onClick={pZoomIn} disabled={pZoom() >= PZOOM_MAX - 1e-6}>
            +
          </button>
        </div>
      </div>

      <Show when={error()}>
        <div class="book-paged-error-banner">
          <strong>Pagination error:</strong> {error()}
        </div>
      </Show>

      <div
        class="book-paged-stage"
        style={{ "--paged-zoom": String(pZoom()) }}
        ref={(el) => {
          mountRef = el;
          props.onScrollSurfaceReady?.(el ? { el, getAnchors } : null);
        }}
      />
    </div>
  );
};

function renderMathInTree(root: Element) {
  const nodes = root.querySelectorAll(".math");
  nodes.forEach((node) => {
    if ((node as HTMLElement).querySelector(".katex")) return; // already rendered
    const display = node.classList.contains("display");
    const raw = (node.textContent ?? "").trim();
    const stripped = raw
      .replace(/^\\\(/, "")
      .replace(/\\\)$/, "")
      .replace(/^\\\[/, "")
      .replace(/\\\]$/, "")
      .trim();
    try {
      katex.render(stripped, node as HTMLElement, {
        displayMode: display,
        throwOnError: false,
      });
    } catch (e) {
      console.warn("KaTeX render failed in paged preview:", e);
    }
  });
}

export default BookPagedPreview;
