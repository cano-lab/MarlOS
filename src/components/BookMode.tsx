import { Component, createEffect, createSignal, For, onMount, Show } from "solid-js";
import { open, save } from "@tauri-apps/plugin-dialog";
import katex from "katex";
import "katex/dist/katex.min.css";
import {
  typesetterService,
  PandocProbe,
  LoadedBook,
  BookSection,
  SectionKind,
  ScrollSurface,
  SectionAnchor,
} from "../services/typesetter-service";
import BookPagedPreview from "./BookPagedPreview";
import BookConfigEditor from "./BookConfigEditor";
import BookEditorPane from "./BookEditorPane";
import RelevantSources from "./RelevantSources";
import "./BookMode.css";

/**
 * Pandoc with `--katex` emits `.math.inline` / `.math.display` spans wrapped
 * in `\(...\)` or `\[...\]` delimiters. We strip the delimiters and let
 * KaTeX render in place.
 */
function renderMathInElement(root: Element): { rendered: number; failed: number } {
  let rendered = 0;
  let failed = 0;
  const nodes = root.querySelectorAll(".math");
  nodes.forEach((node) => {
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
      rendered++;
    } catch (e) {
      console.warn("KaTeX render failed:", e, raw);
      failed++;
    }
  });
  return { rendered, failed };
}

interface BookModeProps {
  initialPath?: string | null;
  onClose?: () => void;
}

const DEFAULT_MANUSCRIPT =
  "X:\\ARCH\\ETE 26\\Book 1- Crumbs\\Markdown\\book.toml";

/** localStorage key for persisting the most recently loaded book path
 *  so subsequent Book Mode visits open it automatically. */
const LAST_BOOK_PATH_KEY = "marlos-typesetter-last-book-path";

function readLastBookPath(): string | null {
  try {
    return localStorage.getItem(LAST_BOOK_PATH_KEY);
  } catch {
    return null;
  }
}

function writeLastBookPath(p: string) {
  try {
    localStorage.setItem(LAST_BOOK_PATH_KEY, p);
  } catch {
    // ignore quota errors
  }
}

const SECTION_KIND_LABELS: Record<SectionKind, string> = {
  "front-matter": "Front matter",
  chapter: "Chapters",
  interlude: "Interludes",
  "back-matter": "Back matter",
};

const SECTION_KIND_ORDER: SectionKind[] = ["front-matter", "chapter", "interlude", "back-matter"];

const BookMode: Component<BookModeProps> = (props) => {
  const [probe, setProbe] = createSignal<PandocProbe | null>(null);
  const [path, setPath] = createSignal<string>(
    props.initialPath ?? readLastBookPath() ?? DEFAULT_MANUSCRIPT,
  );
  const [book, setBook] = createSignal<LoadedBook | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [info, setInfo] = createSignal<string | null>(null);
  const [showRaw, setShowRaw] = createSignal(false);
  const [mathStats, setMathStats] = createSignal<{ rendered: number; failed: number } | null>(null);
  const [activeKindFilter, setActiveKindFilter] = createSignal<SectionKind | null>(null);
  const [viewMode, setViewMode] = createSignal<
    "outline" | "pages" | "source" | "split"
  >("outline");
  const [showSettings, setShowSettings] = createSignal(false);
  const [exporting, setExporting] = createSignal(false);
  // Relevant Sources side panel — ranks the user's saved Sources by
  // semantic similarity to the section they're currently in.
  const [showRelevant, setShowRelevant] = createSignal(false);
  // 1-based BookSection.order — which top-level section is currently in
  // view in either Pages or Source. Lets the views sync scroll position
  // when the user switches between them.
  const [currentSectionOrder, setCurrentSectionOrder] = createSignal(1);
  // Set when Source autosaves — defers the expensive pandoc + structure
  // reload until the user switches to a view that needs it.
  const [pagesStale, setPagesStale] = createSignal(false);
  // Bumped on every successful loadBook so the Source pane can re-read
  // its file from disk when the book is (re)loaded — otherwise an
  // on-disk change (external edit, version bump) shows in Pages but not
  // in the Source textarea, which keeps its stale buffer.
  const [reloadToken, setReloadToken] = createSignal(0);
  // Scroll surfaces exposed by the two child views — { el, getAnchors }
  // pairs that BookMode uses to mirror scrolling between them in split
  // mode with per-section interpolation.
  const [sourceSurface, setSourceSurface] = createSignal<ScrollSurface | null>(null);
  const [pagesSurface, setPagesSurface] = createSignal<ScrollSurface | null>(null);
  // Sync is now on-demand only — two buttons in the split toolbar
  // jump one pane to the other's position when clicked. The old
  // continuous mirror was removed because the constant scroll-event
  // ping-pong made small movements feel jumpy and re-firing
  // getBoundingClientRect on every scroll was a perf hot path.
  // When off, autosaves don't trigger an immediate loadBook +
  // re-pagination. Edits still save to disk; Pages just stays on
  // whatever was rendered last. Useful when typing fast and the
  // re-paginate flicker is distracting. A "Re-paginate" button
  // surfaces when pages are stale.
  const [autoPaginate, setAutoPaginate] = createSignal(true);
  // Live-edit flash: incremented every time Source emits onEditAt, so
  // BookPagedPreview's effect re-fires and re-applies the highlight.
  const [flashAnchor, setFlashAnchor] = createSignal<
    { order: number; paraIndex: number; ts: number } | null
  >(null);
  let renderedRef: HTMLElement | undefined;

  // On-demand sync helpers. Map a scrollTop on one pane to the
  // matching paragraph position on the other using the per-section
  // anchors both sides expose. Called from the toolbar buttons in
  // split mode — no automatic scroll listeners, no echo suppression,
  // no baseline offset.
  const findMatch = (
    anchors: SectionAnchor[],
    order: number,
    paraIndex: number,
  ): SectionAnchor | undefined => {
    const exact = anchors.find(
      (a) => a.order === order && a.paraIndex === paraIndex,
    );
    if (exact) return exact;
    let best: SectionAnchor | undefined;
    for (const a of anchors) {
      if (a.order !== order) continue;
      if (a.paraIndex > paraIndex) continue;
      if (!best || a.paraIndex > best.paraIndex) best = a;
    }
    if (best) return best;
    return anchors.find((a) => a.order === order && a.paraIndex === 0);
  };

  const computeMirrorTarget = (
    fromTop: number,
    from: HTMLElement,
    to: HTMLElement,
    fromAnchors: SectionAnchor[],
    toAnchors: SectionAnchor[],
  ): number => {
    const fromBottom = Math.max(1, from.scrollHeight - from.clientHeight);
    const toBottom = Math.max(1, to.scrollHeight - to.clientHeight);
    if (fromAnchors.length === 0 || toAnchors.length === 0) {
      return (fromTop / fromBottom) * toBottom;
    }
    let idx = -1;
    for (let i = 0; i < fromAnchors.length; i++) {
      if (fromAnchors[i].top <= fromTop) idx = i;
      else break;
    }
    if (idx === -1) {
      const fa = fromAnchors[0];
      const ta = findMatch(toAnchors, fa.order, fa.paraIndex) ?? toAnchors[0];
      const span = Math.max(1, fa.top);
      return (fromTop / span) * ta.top;
    }
    const fromCurr = fromAnchors[idx];
    const fromNext = fromAnchors[idx + 1];
    const toCurr = findMatch(toAnchors, fromCurr.order, fromCurr.paraIndex);
    if (!toCurr) return (fromTop / fromBottom) * toBottom;
    if (!fromNext) {
      const span = Math.max(1, fromBottom - fromCurr.top);
      const within = (fromTop - fromCurr.top) / span;
      return (
        toCurr.top + Math.max(0, Math.min(1, within)) * (toBottom - toCurr.top)
      );
    }
    const toNext = findMatch(toAnchors, fromNext.order, fromNext.paraIndex);
    if (!toNext || toNext.top <= toCurr.top) {
      const span = Math.max(1, fromBottom - fromCurr.top);
      const within = (fromTop - fromCurr.top) / span;
      return (
        toCurr.top + Math.max(0, Math.min(1, within)) * (toBottom - toCurr.top)
      );
    }
    const span = Math.max(1, fromNext.top - fromCurr.top);
    const within = (fromTop - fromCurr.top) / span;
    return (
      toCurr.top + Math.max(0, Math.min(1, within)) * (toNext.top - toCurr.top)
    );
  };

  /** Jump Pages to the position matching Source's current scrollTop. */
  const syncPagesToSource = () => {
    const src = sourceSurface();
    const pgs = pagesSurface();
    if (!src || !pgs) return;
    const target = computeMirrorTarget(
      src.el.scrollTop,
      src.el,
      pgs.el,
      src.getAnchors(),
      pgs.getAnchors(),
    );
    pgs.el.scrollTo({ top: Math.max(0, target), behavior: "smooth" });
  };

  /** Jump Source to the position matching Pages's current scrollTop. */
  const syncSourceToPages = () => {
    const src = sourceSurface();
    const pgs = pagesSurface();
    if (!src || !pgs) return;
    const target = computeMirrorTarget(
      pgs.el.scrollTop,
      pgs.el,
      src.el,
      pgs.getAnchors(),
      src.getAnchors(),
    );
    src.el.scrollTo({ top: Math.max(0, target), behavior: "smooth" });
  };

  // When the user moves out of Source and into Outline or Pages, drain
  // any pending re-pagination so they see the latest content. Split
  // mode is intentionally NOT in this list — split has its own
  // auto-paginate toggle and must respect Manual mode (a Manual save
  // flips pagesStale; we don't want this effect to drain it).
  createEffect(() => {
    const v = viewMode();
    if ((v === "outline" || v === "pages") && pagesStale()) {
      setPagesStale(false);
      void loadBook();
    }
  });

  // If the user flips auto-paginate back on while pages are stale,
  // immediately drain the pending reload so they don't have to also
  // click "Re-paginate".
  createEffect(() => {
    if (autoPaginate() && pagesStale() && viewMode() === "split") {
      setPagesStale(false);
      void loadBook();
    }
  });

  const slugTitle = () =>
    (book()?.config.book.title || "book")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "");

  const exportPdf = async () => {
    if (!book()) {
      setError("Load a book first");
      return;
    }
    let dest: string | null;
    try {
      dest = await save({
        title: "Export PDF",
        defaultPath: `${slugTitle()}.pdf`,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
    } catch (e) {
      setError(`File picker failed: ${e instanceof Error ? e.message : String(e)}`);
      return;
    }
    if (!dest) return;
    setExporting(true);
    setError(null);
    setInfo("Spawning headless Chromium and rendering — this may take 10–30s...");
    try {
      const written = await typesetterService.exportPdf(path(), dest);
      setInfo(`PDF written: ${written}`);
    } catch (e) {
      setError(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setExporting(false);
    }
  };

  const exportEpub = async () => {
    if (!book()) {
      setError("Load a book first");
      return;
    }
    let dest: string | null;
    try {
      dest = await save({
        title: "Export EPUB",
        defaultPath: `${slugTitle()}.epub`,
        filters: [{ name: "EPUB", extensions: ["epub"] }],
      });
    } catch (e) {
      setError(`File picker failed: ${e instanceof Error ? e.message : String(e)}`);
      return;
    }
    if (!dest) return;
    setExporting(true);
    setError(null);
    setInfo("Generating EPUB via pandoc...");
    try {
      const written = await typesetterService.exportEpub(path(), dest);
      setInfo(`EPUB written: ${written}`);
    } catch (e) {
      setError(`EPUB export failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setExporting(false);
    }
  };

  onMount(async () => {
    setProbe(await typesetterService.probe());
    // Auto-load the last-opened book if there's one stored. Saves
    // the user from re-clicking Load every time they enter Book Mode.
    const stored = readLastBookPath();
    if (stored && stored.trim()) {
      void loadBook();
    }
  });

  createEffect(() => {
    const b = book();
    const raw = showRaw();
    if (!b || raw || !renderedRef) return;
    queueMicrotask(() => {
      if (renderedRef) {
        const stats = renderMathInElement(renderedRef);
        if (stats.rendered > 0 || stats.failed > 0) setMathStats(stats);
      }
    });
  });

  const pickPath = async () => {
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [
          { name: "Book or markdown", extensions: ["toml", "md", "markdown", "txt"] },
        ],
      });
      if (typeof picked === "string") setPath(picked);
    } catch (e) {
      setError(`File picker failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const isMarkdownPath = (p: string) => /\.(md|markdown|txt)$/i.test(p);

  // Concurrency guard: only one loadBook can run at a time. If a second
  // call comes in while one is in flight (e.g. autosave fires twice in
  // rapid succession), set a "pending" flag and re-run once at the end
  // so the user always sees the latest disk state, without paying for
  // overlapping pagination work.
  let loadBookInFlight = false;
  let loadBookPending = false;
  const loadBook = async () => {
    if (loadBookInFlight) {
      loadBookPending = true;
      return;
    }
    loadBookInFlight = true;
    const p = path().trim();
    if (!p) {
      setError("Pick a book.toml or markdown file first");
      loadBookInFlight = false;
      return;
    }
    setBusy(true);
    setError(null);
    setInfo(null);
    try {
      const out = await typesetterService.loadBook(p);
      setBook(out);
      setMathStats(null);
      writeLastBookPath(p);
      // Tell the Source pane to re-read from disk (it doesn't otherwise
      // react to same-path reloads). Guarded on its side so it won't
      // clobber unsaved edits.
      setReloadToken((t) => t + 1);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      // If pointing at a markdown file with no book.toml, offer to init.
      if (msg.includes("not found") && isMarkdownPath(p)) {
        setError(
          `No book.toml in this directory. Click "Init book.toml" to create a starter config.`,
        );
      } else {
        setError(`Load failed: ${msg}`);
      }
    } finally {
      setBusy(false);
      loadBookInFlight = false;
      if (loadBookPending) {
        loadBookPending = false;
        // Tail-call: drain the queued request with whatever disk state
        // is current now. Doesn't await — caller already returned.
        void loadBook();
      }
    }
  };

  // Debounced auto-paginate trigger. Each autosave queues a loadBook
  // 800ms later; rapid saves coalesce into one. Combined with the
  // BookSourceView save debounce (1.5s after typing stops), this means
  // pages re-paginates ~2.3s after the writer pauses, not on every
  // 1.5-second autosave burst that fires while they're still typing.
  let autoPaginateTimer: number | null = null;
  const scheduleAutoPaginate = () => {
    if (autoPaginateTimer !== null) clearTimeout(autoPaginateTimer);
    autoPaginateTimer = window.setTimeout(() => {
      autoPaginateTimer = null;
      void loadBook();
    }, 800);
  };

  const initBook = async () => {
    const p = path().trim();
    if (!p || !isMarkdownPath(p)) {
      setError("Init only works when the path points at a markdown file");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const tomlPath = await typesetterService.initBook(p);
      setPath(tomlPath);
      setInfo(`Created ${tomlPath}. Loading...`);
      await loadBook();
    } catch (e) {
      setError(`Init failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  // Group sections by kind for the tree pane
  const sectionsByKind = (): Record<SectionKind, BookSection[]> => {
    const acc: Record<SectionKind, BookSection[]> = {
      "front-matter": [],
      chapter: [],
      interlude: [],
      "back-matter": [],
    };
    for (const s of book()?.structure.sections ?? []) acc[s.kind].push(s);
    return acc;
  };

  const filteredHtml = () => {
    const b = book();
    if (!b) return "";
    const kind = activeKindFilter();
    if (!kind) return b.enriched_html;
    // Hide top-level sections that don't match the filter via inline styles.
    // Cheap, deterministic, no DOM-walking required.
    return b.enriched_html.replace(
      /<section ([^>]*?data-section-type="([^"]+)"[^>]*)>/g,
      (_, attrs, kindAttr) => {
        const hidden = kindAttr !== kind ? ' style="display:none"' : "";
        return `<section ${attrs}${hidden}>`;
      },
    );
  };

  // Plain-text body of the section the writer is currently in,
  // extracted from enriched_html by data-section-order. Feeds the
  // Relevant Sources panel — capped because embedding models truncate
  // long inputs and the first ~2k chars carry the topic signal.
  const currentSectionText = (): string => {
    const b = book();
    if (!b) return "";
    const order = currentSectionOrder();
    try {
      const doc = new DOMParser().parseFromString(b.enriched_html, "text/html");
      const sec = doc.querySelector(`[data-section-order="${order}"]`);
      if (!sec) return "";
      return (sec.textContent ?? "").replace(/\s+/g, " ").trim().slice(0, 2000);
    } catch {
      return "";
    }
  };

  const currentSectionLabel = (): string => {
    const s = book()?.structure.sections.find(
      (sec) => sec.order === currentSectionOrder(),
    );
    return s ? s.title || s.h1_raw : "";
  };

  const scrollToSection = (htmlId: string) => {
    // Sync shared cursor so Pages/Source jump to here too.
    const sec = book()?.structure.sections.find((s) => s.html_id === htmlId);
    if (sec) setCurrentSectionOrder(sec.order);
    if (!renderedRef) return;
    const el = renderedRef.querySelector(`#${CSS.escape(htmlId)}`);
    if (el && "scrollIntoView" in el) {
      (el as HTMLElement).scrollIntoView({ behavior: "smooth", block: "start" });
      (el as HTMLElement).classList.add("book-mode-section-flash");
      setTimeout(() => (el as HTMLElement).classList.remove("book-mode-section-flash"), 1200);
    }
  };

  return (
    <div class="book-mode">
      <div class="book-mode-header">
        <h2>
          Book Mode <span class="book-mode-badge">Phase D — headers + page numbers + chapter openers</span>
        </h2>
        <Show when={props.onClose}>
          <button class="book-mode-close" onClick={props.onClose}>
            ✕
          </button>
        </Show>
      </div>

      <div class="book-mode-status">
        <Show
          when={probe()?.available}
          fallback={
            <div class="book-mode-warn">
              <strong>Pandoc not found.</strong>
              <Show when={probe()?.error}>
                <span> {probe()!.error}</span>
              </Show>
              <span>
                {" "}
                Install pandoc and restart MarlOS, or set <code>MARLOS_PANDOC</code>.
              </span>
            </div>
          }
        >
          <span class="book-mode-ok">✓ {probe()!.version}</span>
          <span class="book-mode-meta">{probe()!.path}</span>
        </Show>
      </div>

      <div class="book-mode-controls">
        <input
          class="book-mode-path"
          type="text"
          value={path()}
          onInput={(e) => setPath(e.currentTarget.value)}
          placeholder="Path to book.toml or a manuscript markdown file..."
        />
        <button class="book-mode-btn" onClick={pickPath}>
          Browse...
        </button>
        <button
          class="book-mode-btn book-mode-btn-primary"
          onClick={loadBook}
          disabled={busy() || !probe()?.available || !path().trim()}
        >
          {busy() ? "Loading..." : "Load book"}
        </button>
        <Show when={isMarkdownPath(path())}>
          <button class="book-mode-btn" onClick={initBook} disabled={busy()}>
            Init book.toml
          </button>
        </Show>
      </div>

      <Show when={error()}>
        <div class="book-mode-error">{error()}</div>
      </Show>
      <Show when={info()}>
        <div class="book-mode-info">{info()}</div>
      </Show>

      <Show when={book()}>
        <div class="book-mode-meta-bar">
          <span class="book-mode-title">
            <strong>{book()!.config.book.title || "(untitled)"}</strong>
            <Show when={book()!.config.book.subtitle}>
              <span class="book-mode-subtitle"> — {book()!.config.book.subtitle}</span>
            </Show>
          </span>
          <Show when={book()!.config.book.author}>
            <span>by {book()!.config.book.author}</span>
          </Show>
          <span class="book-mode-trim">
            {book()!.config.trim.size} • {book()!.config.typography.body_font}{" "}
            {book()!.config.typography.body_size_pt}/{book()!.config.typography.body_leading_pt}
          </span>
          <div class="book-mode-view-toggle">
            <button
              classList={{
                "book-mode-view-btn": true,
                active: viewMode() === "outline",
              }}
              onClick={() => setViewMode("outline")}
            >
              Outline
            </button>
            <button
              classList={{
                "book-mode-view-btn": true,
                active: viewMode() === "source",
              }}
              onClick={() => setViewMode("source")}
            >
              Source
            </button>
            <button
              classList={{
                "book-mode-view-btn": true,
                active: viewMode() === "pages",
              }}
              onClick={() => setViewMode("pages")}
            >
              Pages
            </button>
            <button
              classList={{
                "book-mode-view-btn": true,
                active: viewMode() === "split",
              }}
              onClick={() => setViewMode("split")}
              title="Source on the left, Pages on the right; saves auto-repaginate"
            >
              Split
            </button>
          </div>
          <Show when={viewMode() === "split"}>
            <button
              class="book-mode-btn"
              onClick={syncPagesToSource}
              title="Scroll Pages to match Source's current position"
            >
              → Pages
            </button>
            <button
              class="book-mode-btn"
              onClick={syncSourceToPages}
              title="Scroll Source to match Pages's current position"
            >
              ← Source
            </button>
            <button
              classList={{
                "book-mode-btn": true,
                "book-mode-btn-settings": true,
                active: autoPaginate(),
              }}
              onClick={() => setAutoPaginate(!autoPaginate())}
              title={
                autoPaginate()
                  ? "Auto-paginate ON — Pages refreshes after each save (~1.5s)"
                  : "Auto-paginate OFF — edits save but Pages stays put until you re-paginate"
              }
            >
              {autoPaginate() ? "⟳ Auto" : "⏸ Manual"}
            </button>
            <Show when={!autoPaginate() && pagesStale()}>
              <button
                class="book-mode-btn book-mode-btn-primary"
                onClick={() => {
                  setPagesStale(false);
                  void loadBook();
                }}
                title="Run pandoc + paginate now to reflect the latest edits"
              >
                ✱ Re-paginate
              </button>
            </Show>
          </Show>
          <button
            classList={{
              "book-mode-btn": true,
              "book-mode-btn-settings": true,
              active: showRelevant(),
            }}
            onClick={() => setShowRelevant(!showRelevant())}
            title="Show sources relevant to the section you're in"
          >
            📎 Relevant
          </button>
          <button
            classList={{
              "book-mode-btn": true,
              "book-mode-btn-settings": true,
              active: showSettings(),
            }}
            onClick={() => setShowSettings(!showSettings())}
            title="Edit book.toml"
          >
            ⚙ Settings
          </button>
          <button
            class="book-mode-btn book-mode-btn-primary"
            onClick={exportPdf}
            title="Render the book to a print-ready PDF via headless Chromium"
            disabled={exporting() || !book()}
          >
            {exporting() ? "⏳ Exporting..." : "📕 Export PDF"}
          </button>
          <button
            class="book-mode-btn"
            onClick={exportEpub}
            title="Generate an EPUB3 via pandoc (for KDP Kindle, Apple Books, etc.)"
            disabled={exporting() || !book()}
          >
            📘 Export EPUB
          </button>
          <button
            class="book-mode-btn"
            onClick={() => window.print()}
            title="Print via OS dialog (less reliable than Export PDF)"
            disabled={viewMode() !== "pages"}
          >
            🖨 Print
          </button>
        </div>

        <div class="book-mode-main-area">
          <div
            classList={{
              "book-mode-content": true,
              "book-mode-content-split": viewMode() === "split",
            }}
          >

        <Show when={viewMode() === "outline"}>
          <div class="book-mode-stats">
            <span>
              <strong>{book()!.structure.front_matter_count}</strong> front matter
            </span>
            <span>
              <strong>{book()!.structure.chapter_count}</strong> chapters
            </span>
            <span>
              <strong>{book()!.structure.interlude_count}</strong> interludes
            </span>
            <span>
              <strong>{book()!.structure.back_matter_count}</strong> back matter
            </span>
            <Show when={mathStats()}>
              <span>
                <strong>{mathStats()!.rendered}</strong> math rendered
                <Show when={mathStats()!.failed > 0}>
                  {" "}
                  (<span class="book-mode-stats-fail">{mathStats()!.failed} failed</span>)
                </Show>
              </span>
            </Show>
            <span class="book-mode-stats-spacer" />
            <button class="book-mode-btn" onClick={() => setShowRaw(!showRaw())}>
              {showRaw() ? "Show rendered" : "Show raw HTML"}
            </button>
          </div>

          <Show when={book()!.stderr_warnings.trim()}>
            <details class="book-mode-warnings">
              <summary>
                Pandoc warnings ({book()!.stderr_warnings.split("\n").filter((l) => l.trim()).length}{" "}
                lines)
              </summary>
              <pre>{book()!.stderr_warnings}</pre>
            </details>
          </Show>
        </Show>

        {/* Pages and Source stay mounted across view switches so we
            don't pay the Paged.js re-pagination cost or lose textarea
            state every time the user tabs around. CSS hides them when
            inactive. In split mode Source sits on the left, Pages on
            the right — JSX order is the visual order under flex row. */}
        <div
          classList={{
            "book-mode-view-host": true,
            "book-mode-view-host-split-left": viewMode() === "split",
            hidden: viewMode() !== "source" && viewMode() !== "split",
          }}
        >
          <BookEditorPane
            bookPath={path()}
            files={book()!.config.files}
            reloadToken={reloadToken()}
            currentSectionOrder={currentSectionOrder()}
            onSectionChange={setCurrentSectionOrder}
            active={viewMode() === "source" || viewMode() === "split"}
            disableSectionScrollSync={viewMode() === "split"}
            onScrollSurfaceReady={setSourceSurface}
            pageStyled={viewMode() === "split"}
            config={book()?.config ?? null}
            onEditAt={(order, paraIndex) =>
              setFlashAnchor({ order, paraIndex, ts: performance.now() })
            }
            onSaved={() => {
              if (viewMode() === "split" && autoPaginate()) {
                // Split + auto: debounced loadBook so rapid saves
                // during fast typing don't trigger overlapping
                // paginations and freeze the UI.
                scheduleAutoPaginate();
              } else {
                // Either solo Source (defer until view switch), or
                // split + manual (defer until "Re-paginate" click /
                // auto toggled back on). Both flag stale.
                setPagesStale(true);
              }
            }}
          />
        </div>

        <div
          classList={{
            "book-mode-view-host": true,
            "book-mode-view-host-split-right": viewMode() === "split",
            hidden: viewMode() !== "pages" && viewMode() !== "split",
          }}
        >
          <BookPagedPreview
            config={book()!.config}
            enrichedHtml={book()!.enriched_html}
            frontCoverPath={book()!.front_cover_path}
            backCoverPath={book()!.back_cover_path}
            currentSectionOrder={currentSectionOrder()}
            onSectionChange={setCurrentSectionOrder}
            active={viewMode() === "pages" || viewMode() === "split"}
            disableSectionScrollSync={viewMode() === "split"}
            onScrollSurfaceReady={setPagesSurface}
            flashAnchor={flashAnchor()}
          />
        </div>

        <Show when={viewMode() === "outline"}>
        <div class="book-mode-body">
          <aside class="book-mode-tree">
            <div class="book-mode-tree-filter">
              <button
                classList={{
                  "book-mode-chip": true,
                  "book-mode-chip-active": activeKindFilter() === null,
                }}
                onClick={() => setActiveKindFilter(null)}
              >
                All
              </button>
              <For each={SECTION_KIND_ORDER}>
                {(k) => (
                  <button
                    classList={{
                      "book-mode-chip": true,
                      [`book-mode-chip-${k}`]: true,
                      "book-mode-chip-active": activeKindFilter() === k,
                    }}
                    onClick={() => setActiveKindFilter(activeKindFilter() === k ? null : k)}
                  >
                    {k}
                  </button>
                )}
              </For>
            </div>
            <For each={SECTION_KIND_ORDER}>
              {(kind) => (
                <Show when={sectionsByKind()[kind].length > 0}>
                  <div class="book-mode-tree-group">
                    <div class={`book-mode-tree-heading book-mode-chip-${kind}`}>
                      {SECTION_KIND_LABELS[kind]} ({sectionsByKind()[kind].length})
                    </div>
                    <For each={sectionsByKind()[kind]}>
                      {(s) => (
                        <button
                          class="book-mode-tree-item"
                          onClick={() => scrollToSection(s.html_id)}
                          title={s.h1_raw}
                        >
                          <Show when={s.number !== null}>
                            <span class="book-mode-tree-num">
                              {kind === "interlude"
                                ? s.roman_number ?? s.number
                                : s.number}
                            </span>
                          </Show>
                          <span class="book-mode-tree-title">
                            {s.title || s.h1_raw}
                          </span>
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              )}
            </For>
          </aside>

          <div class="book-mode-preview">
            <Show
              when={!showRaw()}
              fallback={<pre class="book-mode-raw">{book()!.enriched_html}</pre>}
            >
              <article
                ref={renderedRef}
                class="book-mode-rendered"
                innerHTML={filteredHtml()}
              />
            </Show>
          </div>
        </div>
        </Show>

          </div>
          <Show when={showRelevant()}>
            <RelevantSources
              queryText={currentSectionText()}
              contextLabel={currentSectionLabel()}
            />
          </Show>
          <Show when={showSettings()}>
            <BookConfigEditor
              bookPath={path()}
              config={book()!.config}
              onSaved={async () => {
                await loadBook();
              }}
              onClose={() => setShowSettings(false)}
            />
          </Show>
        </div>
      </Show>
    </div>
  );
};

export default BookMode;
