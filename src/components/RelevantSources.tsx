import { Component, createEffect, createSignal, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./RelevantSources.css";

/** Mirrors the Rust SourceView shape (only the fields we render). */
interface SourceView {
  id: string;
  title: string;
  url: string | null;
  source_type: string;
  authors: string[];
  published_date: string | null;
  summary: string | null;
  tags: string[];
}

interface RelevantSourcesProps {
  /** Text the panel ranks sources against — typically the current
   *  book section's body text. Empty string clears results. */
  queryText: string;
  /** Optional label for what the query represents (e.g. the section
   *  title), shown in the panel header. */
  contextLabel?: string;
}

const DEBOUNCE_MS = 600;
const MAX_QUERY_CHARS = 2000;

/**
 * Ranks the user's saved Sources by semantic similarity to whatever
 * they're currently writing. Lives in Book Mode as a side panel —
 * "what's relevant to this paragraph" rather than "everything I saved".
 * Backed by the research_search_sources command (embedding search
 * over kind:source objects).
 */
const RelevantSources: Component<RelevantSourcesProps> = (props) => {
  const [results, setResults] = createSignal<Array<[SourceView, number]>>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [lastQuery, setLastQuery] = createSignal<string>("");

  let debounceTimer: number | null = null;

  const runSearch = async (text: string) => {
    const trimmed = text.trim().slice(0, MAX_QUERY_CHARS);
    if (trimmed.length < 20) {
      // Too little context to rank meaningfully.
      setResults([]);
      setLastQuery("");
      return;
    }
    if (trimmed === lastQuery()) return; // already showing this
    setLoading(true);
    setError(null);
    try {
      const out = await invoke<Array<[SourceView, number]>>(
        "research_search_sources",
        { query: trimmed, limit: 8 },
      );
      setResults(out);
      setLastQuery(trimmed);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  // Debounced re-query whenever the context text changes (e.g. the
  // writer moves to a different section).
  createEffect(() => {
    const text = props.queryText;
    if (debounceTimer !== null) clearTimeout(debounceTimer);
    debounceTimer = window.setTimeout(() => {
      debounceTimer = null;
      void runSearch(text);
    }, DEBOUNCE_MS);
  });

  /** Build a `[CITE: ...]` marker for a source and copy it to the
   *  clipboard so the writer can paste it into the manuscript. */
  const copyCite = async (s: SourceView) => {
    const authors =
      s.authors.length > 0 ? s.authors.join(", ") : "Unknown author";
    const year = s.published_date
      ? s.published_date.slice(0, 4)
      : "n.d.";
    const urlPart = s.url ? ` ${s.url}` : "";
    const cite = `[CITE: ${authors} (${year}). ${s.title}.${urlPart}]`;
    try {
      await navigator.clipboard.writeText(cite);
    } catch {
      // ignore — clipboard may be unavailable
    }
  };

  const scorePct = (score: number) => `${Math.round(score * 100)}%`;

  return (
    <div class="relevant-sources">
      <div class="rs-header">
        <span class="rs-title">Relevant Sources</span>
        <Show when={props.contextLabel}>
          <span class="rs-context" title={props.contextLabel}>
            {props.contextLabel}
          </span>
        </Show>
        <Show when={loading()}>
          <span class="rs-spinner" />
        </Show>
      </div>

      <Show when={error()}>
        <div class="rs-error">{error()}</div>
      </Show>

      <div class="rs-body">
        <Show
          when={results().length > 0}
          fallback={
            <div class="rs-empty">
              <Show
                when={lastQuery()}
                fallback={
                  <p>
                    Move your cursor into a section with some text — its
                    content gets ranked against your saved Sources here.
                  </p>
                }
              >
                <p>No sources match this section yet.</p>
              </Show>
            </div>
          }
        >
          <For each={results()}>
            {([source, score]) => (
              <div class="rs-item">
                <div class="rs-item-head">
                  <span class="rs-score" title="Semantic similarity">
                    {scorePct(score)}
                  </span>
                  <span class="rs-item-title">{source.title}</span>
                </div>
                <Show when={source.authors.length > 0}>
                  <p class="rs-item-authors">
                    {source.authors.slice(0, 4).join(", ")}
                    {source.authors.length > 4 ? " et al." : ""}
                  </p>
                </Show>
                <Show when={source.summary}>
                  <p class="rs-item-summary">
                    {source.summary!.slice(0, 220)}
                    {source.summary!.length > 220 ? "…" : ""}
                  </p>
                </Show>
                <div class="rs-item-actions">
                  <button
                    class="rs-item-btn"
                    onClick={() => copyCite(source)}
                    title="Copy a [CITE: ...] marker for this source"
                  >
                    ⎘ Copy [CITE:]
                  </button>
                  <Show when={source.url}>
                    <a
                      class="rs-item-link"
                      href={source.url!}
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      open
                    </a>
                  </Show>
                </div>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
};

export default RelevantSources;
