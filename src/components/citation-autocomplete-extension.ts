import {
  Extension,
  RangeSetBuilder,
} from "@codemirror/state";
import {
  EditorView,
  Decoration,
  DecorationSet,
  WidgetType,
  ViewPlugin,
  ViewUpdate,
} from "@codemirror/view";
import {
  autocompletion,
  CompletionContext,
  CompletionResult,
  Completion,
} from "@codemirror/autocomplete";
import { invoke } from "@tauri-apps/api/core";

// Types matching the Rust Reference struct
interface RefResult {
  id: string;
  title: string;
  authors: string[];
  year: number | null;
  cite_key: string;
  journal: string | null;
}

// Cache for search results
let searchCache: Map<string, RefResult[]> = new Map();
let cacheTimeout: number | null = null;

function clearCache() {
  searchCache.clear();
}

async function searchRefs(query: string): Promise<RefResult[]> {
  if (searchCache.has(query)) return searchCache.get(query)!;

  try {
    const results = await invoke<RefResult[]>("ref_search", { query });
    searchCache.set(query, results);

    // Clear cache after 30 seconds
    if (cacheTimeout) clearTimeout(cacheTimeout);
    cacheTimeout = window.setTimeout(clearCache, 30000);

    return results;
  } catch {
    return [];
  }
}

function authorsShort(authors: string[]): string {
  if (authors.length === 0) return "Unknown";
  const last = authors[0].split(" ").pop() || authors[0];
  if (authors.length === 1) return last;
  if (authors.length === 2) {
    const last2 = authors[1].split(" ").pop() || authors[1];
    return `${last} & ${last2}`;
  }
  return `${last} et al.`;
}

/// Citation autocomplete source — triggers on `@` character
async function citationCompletionSource(
  context: CompletionContext
): Promise<CompletionResult | null> {
  // Look for @query pattern
  const word = context.matchBefore(/@[\w-]*/);
  if (!word) return null;

  // Don't trigger on just @ with no chars yet (wait for at least 1 char)
  const query = word.text.slice(1); // remove @
  if (query.length < 1) {
    // Still show list but with all refs
    const refs = await searchRefs("");
    if (refs.length === 0) return null;

    return {
      from: word.from,
      options: refs.slice(0, 20).map(refToCompletion),
      filter: false,
    };
  }

  const refs = await searchRefs(query);
  if (refs.length === 0) return null;

  return {
    from: word.from,
    options: refs.map(refToCompletion),
    filter: true,
  };
}

function refToCompletion(ref: RefResult): Completion {
  const authors = authorsShort(ref.authors);
  const yearStr = ref.year ? ` (${ref.year})` : "";
  return {
    label: `@${ref.cite_key}`,
    displayLabel: `${authors}${yearStr}`,
    detail: ref.title.length > 50 ? ref.title.slice(0, 47) + "..." : ref.title,
    info: ref.journal || undefined,
    apply: (view, _completion, from, to) => {
      // Insert [[ref_id]] marker
      view.dispatch({
        changes: { from, to, insert: `[[${ref.cite_key}]]` },
      });
    },
    boost: ref.year ? ref.year - 1900 : 0, // newer papers rank higher
  };
}

// --- Inline citation decoration ---
// Renders [[cite_key]] as (Author, Year) in the editor

class CitationWidget extends WidgetType {
  constructor(
    readonly citeKey: string,
    readonly display: string
  ) {
    super();
  }

  toDOM(): HTMLElement {
    const span = document.createElement("span");
    span.className = "cm-citation-inline";
    span.textContent = this.display;
    span.title = `Citation: @${this.citeKey}`;
    return span;
  }

  eq(other: CitationWidget): boolean {
    return this.citeKey === other.citeKey;
  }
}

// Cache for resolved citation display strings
const citationDisplayCache: Map<string, string> = new Map();

async function resolveCiteKey(key: string): Promise<string> {
  if (citationDisplayCache.has(key)) return citationDisplayCache.get(key)!;

  try {
    const results = await invoke<RefResult[]>("ref_search", { query: key });
    const match = results.find((r) => r.cite_key === key);
    if (match) {
      const display = `(${authorsShort(match.authors)}${match.year ? `, ${match.year}` : ""})`;
      citationDisplayCache.set(key, display);
      return display;
    }
  } catch {
    // ignore
  }

  const fallback = `[@${key}]`;
  citationDisplayCache.set(key, fallback);
  return fallback;
}

// ViewPlugin that manages citation decorations
const citationDecoPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    private pendingUpdate = false;

    constructor(view: EditorView) {
      this.decorations = Decoration.none;
      this.buildDecorations(view);
    }

    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) {
        this.buildDecorations(update.view);
      }
    }

    async buildDecorations(view: EditorView) {
      if (this.pendingUpdate) return;
      this.pendingUpdate = true;

      const doc = view.state.doc.toString();
      const regex = /\[\[([a-zA-Z][\w-]*)\]\]/g;
      const matches: { from: number; to: number; key: string }[] = [];

      let match;
      while ((match = regex.exec(doc)) !== null) {
        matches.push({
          from: match.index,
          to: match.index + match[0].length,
          key: match[1],
        });
      }

      if (matches.length === 0) {
        this.decorations = Decoration.none;
        this.pendingUpdate = false;
        return;
      }

      // Resolve all keys
      const displays = await Promise.all(
        matches.map(async (m) => ({
          ...m,
          display: await resolveCiteKey(m.key),
        }))
      );

      const builder = new RangeSetBuilder<Decoration>();
      for (const d of displays) {
        // Only decorate if position still valid
        if (d.to <= view.state.doc.length) {
          builder.add(
            d.from,
            d.to,
            Decoration.replace({
              widget: new CitationWidget(d.key, d.display),
            })
          );
        }
      }

      this.decorations = builder.finish();
      this.pendingUpdate = false;

      // Force view to re-render decorations
      view.dispatch({ effects: [] });
    }
  },
  {
    decorations: (v) => v.decorations,
  }
);

/// Create the citation autocomplete extension
export function citationAutocomplete(): Extension {
  return [
    autocompletion({
      override: [citationCompletionSource],
      activateOnTyping: true,
    }),
    citationDecoPlugin,
    EditorView.baseTheme({
      ".cm-citation-inline": {
        color: "#4ec9b0",
        fontStyle: "italic",
        fontSize: "0.95em",
        cursor: "pointer",
        borderBottom: "1px dotted rgba(78, 201, 176, 0.4)",
        padding: "0 2px",
      },
    }),
  ];
}
