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
const SENT_LS_KEY = "marlos-research-feed-sent-to-sources";

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
  const [fetching, setFetching] = createSignal(false);
  const [fetchStatus, setFetchStatus] = createSignal<string | null>(null);
  // IDs of feed items the user has promoted into the Research Hub's
  // Sources library. Persisted so the button reflects "already sent"
  // across sessions.
  const [sentToSources, setSentToSources] = createSignal<Set<string>>(
    readLsSet(SENT_LS_KEY),
  );

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

  const runDailyFetch = async () => {
    setFetching(true);
    setError(null);
    setFetchStatus("Fetching candidates from arXiv / Semantic Scholar / CrossRef…");
    try {
      const out = await invoke<{
        total_stored: number;
        per_topic: Array<{
          topic: string;
          fetched: number;
          stored: number;
          error: string | null;
        }>;
      }>("research_run_daily_fetch", { topic: null, limitPerTopic: 5 });
      const lines = out.per_topic.map(
        (t) =>
          `${t.topic}: +${t.stored} new (of ${t.fetched} candidates)` +
          (t.error ? ` — ${t.error}` : ""),
      );
      setFetchStatus(
        `Done — ${out.total_stored} new summaries stored.\n${lines.join("\n")}`,
      );
      await load();
    } catch (e) {
      setError(`Fetch failed: ${e instanceof Error ? e.message : String(e)}`);
      setFetchStatus(null);
    } finally {
      setFetching(false);
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

  // Promote the current feed item into the Research Hub's Sources
  // library — creates a proper Source record (authors, citations,
  // tags, notes) usable by the paper generator and citation tools.
  // Distinct from the local ★ Save, which is just a quick "maybe".
  const sendToSources = async () => {
    const cur = current();
    if (!cur) return;
    if (sentToSources().has(cur.id)) return; // already promoted
    setError(null);
    try {
      await invoke("research_add_manual", {
        request: {
          url: cur.url,
          title: cur.title,
          authors: cur.authors
            ? cur.authors.split(/,\s*/).map((a) => a.trim()).filter(Boolean)
            : null,
          published_date: cur.year || cur.date || null,
          source_type: "paper",
          tags: cur.topic ? [cur.topic] : null,
          notes: null,
          content: cur.summary,
        },
      });
      const next = new Set(sentToSources());
      next.add(cur.id);
      setSentToSources(next);
      writeLsSet(SENT_LS_KEY, next);
    } catch (e) {
      setError(
        `Send to Sources failed: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
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
    } else if (e.key === "a") {
      e.preventDefault();
      void sendToSources();
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
        <button
          class="rf-btn rf-btn-primary"
          onClick={runDailyFetch}
          disabled={fetching() || loading()}
          title="Pull fresh papers from arXiv, Semantic Scholar, and CrossRef for every registered research topic"
        >
          {fetching() ? "⏳ Fetching…" : "⬇ Fetch latest"}
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
          ← → navigate · S save · A → sources · D dismiss · C copy [CITE:]
        </span>
      </div>

      <Show when={error()}>
        <div class="rf-error">{error()}</div>
      </Show>
      <Show when={fetchStatus()}>
        <div class="rf-info">{fetchStatus()}</div>
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
              <button
                classList={{
                  "rf-action-btn": true,
                  "rf-action-sent": sentToSources().has(item().id),
                }}
                onClick={sendToSources}
                disabled={sentToSources().has(item().id)}
                title="Promote into the Research Hub's Sources library"
              >
                {sentToSources().has(item().id)
                  ? "✓ In Sources"
                  : "→ Send to Sources (A)"}
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
