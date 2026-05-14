import { Component, createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./ResearchFeed.css";

interface ResearchFeedItem {
  id: string;
  title: string;
  topic: string;
  date: string;
  doi: string | null;
  arxiv_id: string | null;
  url: string | null;
  authors: string | null;
  year: string | null;
  venue: string | null;
  summary: string;
}

const SAVED_LS_KEY = "marlos-research-feed-saved";
const DISMISSED_LS_KEY = "marlos-research-feed-dismissed";

function readLsSet(key: string): Set<string> {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return new Set();
    const arr = JSON.parse(raw);
    return new Set(Array.isArray(arr) ? arr : []);
  } catch {
    return new Set();
  }
}

function writeLsSet(key: string, set: Set<string>) {
  try {
    localStorage.setItem(key, JSON.stringify(Array.from(set)));
  } catch {
    // ignore quota errors
  }
}

/**
 * Swipe-through feed of research summaries collected by the daily
 * research cron jobs. One card at a time — arrow keys / buttons to
 * move through, S to save, D to dismiss. Save/dismiss state is
 * persisted locally (no backend round-trip yet) so the user's
 * curation accumulates across sessions.
 */
const ResearchFeed: Component = () => {
  const [items, setItems] = createSignal<ResearchFeedItem[]>([]);
  const [index, setIndex] = createSignal(0);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [saved, setSaved] = createSignal<Set<string>>(readLsSet(SAVED_LS_KEY));
  const [dismissed, setDismissed] = createSignal<Set<string>>(readLsSet(DISMISSED_LS_KEY));
  const [hideDismissed, setHideDismissed] = createSignal(true);
  const [showOnlySaved, setShowOnlySaved] = createSignal(false);

  const filteredItems = (): ResearchFeedItem[] => {
    let result = items();
    if (hideDismissed()) result = result.filter((it) => !dismissed().has(it.id));
    if (showOnlySaved()) result = result.filter((it) => saved().has(it.id));
    return result;
  };

  const current = (): ResearchFeedItem | null => {
    const list = filteredItems();
    if (list.length === 0) return null;
    return list[Math.min(index(), list.length - 1)] ?? null;
  };

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const out = await invoke<ResearchFeedItem[]>("research_feed_list", {
        topic: null,
        limit: 200,
      });
      setItems(out);
      setIndex(0);
    } catch (e) {
      setError(`Failed to load feed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLoading(false);
    }
  };

  const next = () => {
    const list = filteredItems();
    if (list.length === 0) return;
    setIndex((i) => Math.min(i + 1, list.length - 1));
  };
  const prev = () => {
    setIndex((i) => Math.max(0, i - 1));
  };

  const toggleSave = () => {
    const cur = current();
    if (!cur) return;
    const next = new Set(saved());
    if (next.has(cur.id)) next.delete(cur.id);
    else next.add(cur.id);
    setSaved(next);
    writeLsSet(SAVED_LS_KEY, next);
  };

  const dismiss = () => {
    const cur = current();
    if (!cur) return;
    const next = new Set(dismissed());
    next.add(cur.id);
    setDismissed(next);
    writeLsSet(DISMISSED_LS_KEY, next);
    // Advance — hideDismissed will re-filter, so the current() rolls
    // forward automatically if the card we just dismissed was visible.
  };

  const copyCite = async () => {
    const cur = current();
    if (!cur) return;
    // Build a `[CITE: ...]` marker the writer can paste into a chapter.
    const authors = cur.authors ?? "Unknown author";
    const year = cur.year ?? "n.d.";
    const venue = cur.venue ?? "";
    const venuePart = venue ? `, ${venue}` : "";
    const idPart = cur.doi
      ? ` doi:${cur.doi}`
      : cur.arxiv_id
        ? ` arXiv:${cur.arxiv_id}`
        : "";
    const cite = `[CITE: ${authors} (${year}). ${cur.title}${venuePart}.${idPart}]`;
    try {
      await navigator.clipboard.writeText(cite);
    } catch {
      // ignore — selection fallback would go here
    }
  };

  const handleKey = (e: KeyboardEvent) => {
    if (e.target && (e.target as HTMLElement).matches("input, textarea")) return;
    if (e.key === "ArrowRight" || e.key === "j") {
      e.preventDefault();
      next();
    } else if (e.key === "ArrowLeft" || e.key === "k") {
      e.preventDefault();
      prev();
    } else if (e.key === "s") {
      e.preventDefault();
      toggleSave();
    } else if (e.key === "d") {
      e.preventDefault();
      dismiss();
      next();
    } else if (e.key === "c") {
      e.preventDefault();
      void copyCite();
    }
  };

  createEffect(() => {
    void load();
  });

  window.addEventListener("keydown", handleKey);
  onCleanup(() => window.removeEventListener("keydown", handleKey));

  const total = () => filteredItems().length;

  return (
    <div class="research-feed">
      <div class="research-feed-toolbar">
        <button class="rf-btn" onClick={load} disabled={loading()}>
          {loading() ? "Loading…" : "↻ Refresh"}
        </button>
        <span class="rf-counter">
          <Show when={total() > 0} fallback="0 / 0">
            {Math.min(index() + 1, total())} / {total()}
          </Show>
        </span>
        <label class="rf-toggle">
          <input
            type="checkbox"
            checked={hideDismissed()}
            onChange={(e) => setHideDismissed(e.currentTarget.checked)}
          />
          Hide dismissed
        </label>
        <label class="rf-toggle">
          <input
            type="checkbox"
            checked={showOnlySaved()}
            onChange={(e) => setShowOnlySaved(e.currentTarget.checked)}
          />
          Saved only ({saved().size})
        </label>
        <span class="rf-help">
          ← → navigate · S save · D dismiss · C copy [CITE:]
        </span>
      </div>

      <Show when={error()}>
        <div class="rf-error">{error()}</div>
      </Show>

      <Show
        when={current()}
        fallback={
          <div class="rf-empty">
            <Show
              when={loading()}
              fallback={
                <Show
                  when={items().length === 0}
                  fallback={<p>Nothing matches the current filters.</p>}
                >
                  <p>
                    No research summaries yet. Run the daily research cron
                    or use the MCP <code>daily_research_fetch</code> tool to
                    populate the feed.
                  </p>
                </Show>
              }
            >
              <p>Loading…</p>
            </Show>
          </div>
        }
      >
        {(item) => (
          <div class="rf-card">
            <div class="rf-card-header">
              <div class="rf-card-meta">
                <span class="rf-topic">{item().topic || "(no topic)"}</span>
                <Show when={item().date}>
                  <span class="rf-date">{item().date}</span>
                </Show>
                <Show when={item().year}>
                  <span class="rf-year">{item().year}</span>
                </Show>
                <Show when={item().venue}>
                  <span class="rf-venue">{item().venue}</span>
                </Show>
              </div>
              <h2 class="rf-title">{item().title}</h2>
              <Show when={item().authors}>
                <p class="rf-authors">{item().authors}</p>
              </Show>
            </div>

            <div class="rf-card-body">
              <p class="rf-summary">{item().summary}</p>
            </div>

            <div class="rf-card-actions">
              <button class="rf-action-btn" onClick={prev}>
                ← Prev (←)
              </button>
              <button
                classList={{
                  "rf-action-btn": true,
                  "rf-action-saved": saved().has(item().id),
                }}
                onClick={toggleSave}
              >
                {saved().has(item().id) ? "★ Saved (S)" : "☆ Save (S)"}
              </button>
              <button class="rf-action-btn" onClick={copyCite}>
                ⎘ Copy [CITE:] (C)
              </button>
              <button class="rf-action-btn rf-action-dismiss" onClick={() => { dismiss(); next(); }}>
                ✕ Dismiss (D)
              </button>
              <button class="rf-action-btn rf-action-next" onClick={next}>
                Next (→)
              </button>
            </div>

            <div class="rf-card-footer">
              <Show when={item().doi}>
                <a
                  href={`https://doi.org/${item().doi}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  class="rf-link"
                >
                  doi:{item().doi}
                </a>
              </Show>
              <Show when={item().arxiv_id}>
                <a
                  href={`https://arxiv.org/abs/${item().arxiv_id}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  class="rf-link"
                >
                  arXiv:{item().arxiv_id}
                </a>
              </Show>
              <Show when={item().url}>
                <a
                  href={item().url!}
                  target="_blank"
                  rel="noopener noreferrer"
                  class="rf-link"
                >
                  source
                </a>
              </Show>
            </div>
          </div>
        )}
      </Show>

      <Show when={items().length > 0 && total() > 0}>
        <div class="rf-thumbs">
          <For each={filteredItems().slice(Math.max(0, index() - 2), index() + 8)}>
            {(it, i) => {
              const absoluteIdx = () => Math.max(0, index() - 2) + i();
              return (
                <button
                  classList={{
                    "rf-thumb": true,
                    "rf-thumb-active": absoluteIdx() === index(),
                  }}
                  onClick={() => setIndex(absoluteIdx())}
                  title={it.title}
                >
                  <Show when={saved().has(it.id)}>
                    <span class="rf-thumb-star">★</span>
                  </Show>
                  <span class="rf-thumb-title">{it.title}</span>
                </button>
              );
            }}
          </For>
        </div>
      </Show>
    </div>
  );
};

export default ResearchFeed;
