import { Component, createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { convertFileSrc } from "@tauri-apps/api/core";
import katex from "katex";
// EB Garamond weights — registered globally; ok to import multiple times.
import "@fontsource/eb-garamond/400.css";
import "@fontsource/eb-garamond/400-italic.css";
import "@fontsource/eb-garamond/600.css";
import "@fontsource/eb-garamond/600-italic.css";
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
  let mountRef: HTMLDivElement | undefined;
  let cancelled = false;
  let suppressReportUntil = 0;
  let scrollRoot: HTMLElement | null = null;
  let scrollListener: (() => void) | null = null;
  let scrollRaf: number | null = null;
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
      const css = buildBookCss(props.config);

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
      invalidateAnchors(); // fresh DOM → fresh positions
      // Restore the user's view: prefer the most recent edit anchor
      // (so a freshly-inserted snippet stays in view), fall back to
      // the parent-shared currentSectionOrder.
      queueMicrotask(restoreScrollAfterPagination);
    } catch (e) {
      if (!cancelled) {
        setError(e instanceof Error ? e.message : String(e));
        setStatus("error");
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
  onMount(() => {
    if (typeof ResizeObserver !== "undefined" && mountRef) {
      resizeObserver = new ResizeObserver(() => invalidateAnchors());
      resizeObserver.observe(mountRef);
    }
  });

  onCleanup(() => {
    cancelled = true;
    teardownScrollListener();
    resizeObserver?.disconnect();
    resizeObserver = null;
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

  /** Resolve a flashAnchor (order, paraIndex) to its `<p>` (or section
   *  heading at paraIndex 0) in the paginated DOM. */
  const flashTarget = (
    fa: { order: number; paraIndex: number },
  ): HTMLElement | null => {
    if (!mountRef) return null;
    const sec = mountRef.querySelector<HTMLElement>(
      `[data-section-order="${fa.order}"]`,
    );
    if (!sec) return null;
    if (fa.paraIndex <= 0) return sec;
    const paragraphs = sec.querySelectorAll<HTMLElement>("p");
    const owned: HTMLElement[] = [];
    paragraphs.forEach((p) => {
      if (p.closest("[data-section-order]") === sec) owned.push(p);
    });
    return owned[fa.paraIndex - 1] ?? null;
  };

  // Live edit flash: when the writer types in Source, the parent passes
  // the (order, paraIndex) of the cursor's paragraph. We find the
  // matching <p> in the paginated DOM and re-trigger the flash
  // animation. Re-triggering by remove+reflow+add lets continuous
  // typing keep the highlight lit without piling up timers.
  let lastFlashedEl: HTMLElement | null = null;
  let lastFlashTimer: number | null = null;
  createEffect(() => {
    const fa = props.flashAnchor;
    if (!fa) return;
    if (status() !== "ready") return;
    const target = flashTarget(fa);
    if (!target) return;
    if (lastFlashedEl && lastFlashedEl !== target) {
      lastFlashedEl.classList.remove("book-paged-flash");
    }
    if (lastFlashTimer !== null) {
      clearTimeout(lastFlashTimer);
      lastFlashTimer = null;
    }
    target.classList.remove("book-paged-flash");
    // Force reflow so re-adding the class restarts the animation.
    void target.offsetWidth;
    target.classList.add("book-paged-flash");
    lastFlashedEl = target;
    lastFlashTimer = window.setTimeout(() => {
      target?.classList.remove("book-paged-flash");
      lastFlashTimer = null;
    }, 1400);
  });

  /** Restore the user's view position after a fresh pagination. Pages
   *  re-paginates from scratch (the temp DOM is wiped every time), so
   *  scrollTop resets to 0. We use the most recent edit anchor to
   *  bring the user back to where they were typing — with a small
   *  amount of headroom above so any inserted page-break / space
   *  divs in front of the target paragraph remain visible. */
  const restoreScrollAfterPagination = () => {
    if (!mountRef) return;
    const fa = props.flashAnchor;
    if (fa) {
      const target = flashTarget(fa);
      if (target) {
        const rect = target.getBoundingClientRect();
        const rootRect = mountRef.getBoundingClientRect();
        const top = rect.top - rootRect.top + mountRef.scrollTop - 80;
        mountRef.scrollTop = Math.max(0, top);
        return;
      }
    }
    if (typeof props.currentSectionOrder === "number") {
      scrollToOrder(props.currentSectionOrder);
    }
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
      </div>

      <Show when={error()}>
        <div class="book-paged-error-banner">
          <strong>Pagination error:</strong> {error()}
        </div>
      </Show>

      <div
        class="book-paged-stage"
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
