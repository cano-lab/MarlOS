import { Component, createSignal, onMount, Show, For, createMemo } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import ReferenceDetail from "./ReferenceDetail";
import ReferenceImport from "./ReferenceImport";
import "./ReferenceLibrary.css";

export interface Reference {
  id: string;
  title: string;
  authors: string[];
  year: number | null;
  doi: string | null;
  isbn: string | null;
  issn: string | null;
  url: string | null;
  journal: string | null;
  publisher: string | null;
  volume: string | null;
  issue: string | null;
  pages: string | null;
  edition: string | null;
  abstract_text: string | null;
  keywords: string[];
  notes: string | null;
  pdf_path: string | null;
  collections: string[];
  tags: string[];
  reading_status: string;
  rating: number | null;
  cite_key: string;
  ref_type: string;
  added_at: string;
  modified_at: string;
  suid: string | null;
}

interface ReferenceLibraryProps {
  onClose: () => void;
  onOpenPdf?: (path: string) => void;
}

type ViewMode = "list" | "grid";
type SortField = "added_at" | "title" | "year" | "authors";

const ReferenceLibrary: Component<ReferenceLibraryProps> = (props) => {
  const [references, setReferences] = createSignal<Reference[]>([]);
  const [collections, setCollections] = createSignal<string[]>([]);
  const [searchQuery, setSearchQuery] = createSignal("");
  const [selectedCollection, setSelectedCollection] = createSignal<string | null>(null);
  const [selectedStatus, setSelectedStatus] = createSignal<string | null>(null);
  const [selectedRef, setSelectedRef] = createSignal<Reference | null>(null);
  const [viewMode, setViewMode] = createSignal<ViewMode>("list");
  const [sortField, setSortField] = createSignal<SortField>("added_at");
  const [sortAsc, setSortAsc] = createSignal(false);
  const [showImport, setShowImport] = createSignal(false);
  const [showAddDoi, setShowAddDoi] = createSignal(false);
  const [doiInput, setDoiInput] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [totalCount, setTotalCount] = createSignal(0);

  onMount(async () => {
    await loadReferences();
    await loadCollections();
  });

  const loadReferences = async () => {
    try {
      const refs = await invoke<Reference[]>("ref_list", {
        collection: selectedCollection(),
        status: selectedStatus(),
      });
      setReferences(refs);
      const count = await invoke<number>("ref_count");
      setTotalCount(count);
    } catch (e) {
      console.error("Failed to load references:", e);
    }
  };

  const loadCollections = async () => {
    try {
      const colls = await invoke<string[]>("ref_list_collections");
      setCollections(colls);
    } catch (e) {
      console.error("Failed to load collections:", e);
    }
  };

  const handleSearch = async () => {
    if (!searchQuery().trim()) {
      await loadReferences();
      return;
    }
    try {
      const results = await invoke<Reference[]>("ref_search", { query: searchQuery() });
      setReferences(results);
    } catch (e) {
      console.error("Search failed:", e);
    }
  };

  const handleAddDoi = async () => {
    if (!doiInput().trim()) return;
    setLoading(true);
    try {
      const ref = await invoke<Reference>("ref_add_from_doi", { doi: doiInput() });
      setDoiInput("");
      setShowAddDoi(false);
      await loadReferences();
      setSelectedRef(ref);
    } catch (e) {
      alert(`Failed to resolve DOI: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await invoke("ref_delete", { id });
      if (selectedRef()?.id === id) setSelectedRef(null);
      await loadReferences();
    } catch (e) {
      console.error("Failed to delete:", e);
    }
  };

  const handleStatusChange = async (id: string, status: string) => {
    try {
      await invoke("ref_set_reading_status", { id, status });
      await loadReferences();
      if (selectedRef()?.id === id) {
        const updated = await invoke<Reference>("ref_get", { id });
        setSelectedRef(updated);
      }
    } catch (e) {
      console.error("Failed to update status:", e);
    }
  };

  const handleExportBibtex = async () => {
    try {
      const bibtex = await invoke<string>("ref_export_bibtex", { ids: null });
      await navigator.clipboard.writeText(bibtex);
      alert("BibTeX copied to clipboard!");
    } catch (e) {
      console.error("Export failed:", e);
    }
  };

  const filteredAndSorted = createMemo(() => {
    let refs = [...references()];

    // Sort
    const field = sortField();
    const asc = sortAsc();
    refs.sort((a, b) => {
      let cmp = 0;
      switch (field) {
        case "title": cmp = a.title.localeCompare(b.title); break;
        case "year": cmp = (a.year || 0) - (b.year || 0); break;
        case "authors": cmp = (a.authors[0] || "").localeCompare(b.authors[0] || ""); break;
        default: cmp = a.added_at.localeCompare(b.added_at); break;
      }
      return asc ? cmp : -cmp;
    });

    return refs;
  });

  const authorsDisplay = (authors: string[]) => {
    if (authors.length === 0) return "Unknown";
    if (authors.length === 1) return authors[0];
    if (authors.length === 2) return `${authors[0]} & ${authors[1]}`;
    return `${authors[0]} et al.`;
  };

  const statusIcon = (status: string) => {
    switch (status) {
      case "reading": return "\u{1F4D6}";
      case "read": return "\u2705";
      case "archived": return "\u{1F4E6}";
      default: return "\u{1F4CB}";
    }
  };

  return (
    <div class="reflib-overlay" onClick={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="reflib-panel">
        <div class="reflib-header">
          <h2>Reference Library</h2>
          <span class="reflib-count">{totalCount()} references</span>
          <div class="reflib-header-actions">
            <button class="reflib-btn" onClick={() => setShowAddDoi(true)}>+ DOI</button>
            <button class="reflib-btn" onClick={() => setShowImport(true)}>Import BibTeX</button>
            <button class="reflib-btn" onClick={handleExportBibtex}>Export All</button>
            <button class="reflib-close" onClick={props.onClose}>x</button>
          </div>
        </div>

        <div class="reflib-body">
          {/* Sidebar: collections + filters */}
          <div class="reflib-sidebar">
            <div class="reflib-filter-section">
              <h4>Collections</h4>
              <button
                class="reflib-filter-item"
                classList={{ active: selectedCollection() === null }}
                onClick={() => { setSelectedCollection(null); loadReferences(); }}
              >
                All References
              </button>
              <For each={collections()}>
                {(coll) => (
                  <button
                    class="reflib-filter-item"
                    classList={{ active: selectedCollection() === coll }}
                    onClick={() => { setSelectedCollection(coll); loadReferences(); }}
                  >
                    {coll}
                  </button>
                )}
              </For>
            </div>

            <div class="reflib-filter-section">
              <h4>Status</h4>
              {["unread", "reading", "read", "archived"].map((s) => (
                <button
                  class="reflib-filter-item"
                  classList={{ active: selectedStatus() === s }}
                  onClick={() => {
                    setSelectedStatus(selectedStatus() === s ? null : s);
                    loadReferences();
                  }}
                >
                  {statusIcon(s)} {s.charAt(0).toUpperCase() + s.slice(1)}
                </button>
              ))}
            </div>

            <div class="reflib-filter-section">
              <h4>Sort</h4>
              <select
                class="reflib-sort-select"
                value={sortField()}
                onChange={(e) => setSortField(e.currentTarget.value as SortField)}
              >
                <option value="added_at">Date Added</option>
                <option value="title">Title</option>
                <option value="year">Year</option>
                <option value="authors">Author</option>
              </select>
              <button
                class="reflib-sort-dir"
                onClick={() => setSortAsc(!sortAsc())}
              >
                {sortAsc() ? "\u2191 Asc" : "\u2193 Desc"}
              </button>
            </div>
          </div>

          {/* Main: search + list */}
          <div class="reflib-main">
            <div class="reflib-search">
              <input
                type="text"
                placeholder="Search by title, author, keyword, DOI..."
                value={searchQuery()}
                onInput={(e) => setSearchQuery(e.currentTarget.value)}
                onKeyDown={(e) => e.key === "Enter" && handleSearch()}
              />
              <button onClick={handleSearch}>Search</button>
              <div class="reflib-view-toggle">
                <button
                  classList={{ active: viewMode() === "list" }}
                  onClick={() => setViewMode("list")}
                  title="List view"
                >
                  =
                </button>
                <button
                  classList={{ active: viewMode() === "grid" }}
                  onClick={() => setViewMode("grid")}
                  title="Grid view"
                >
                  #
                </button>
              </div>
            </div>

            <div class={`reflib-list reflib-${viewMode()}`}>
              <Show
                when={filteredAndSorted().length > 0}
                fallback={
                  <div class="reflib-empty">
                    No references found. Add references via DOI or import a BibTeX file.
                  </div>
                }
              >
                <For each={filteredAndSorted()}>
                  {(ref_item) => (
                    <button
                      class="reflib-item"
                      classList={{ "reflib-item-selected": selectedRef()?.id === ref_item.id }}
                      onClick={() => setSelectedRef(ref_item)}
                    >
                      <div class="reflib-item-status">
                        {statusIcon(ref_item.reading_status)}
                      </div>
                      <div class="reflib-item-content">
                        <div class="reflib-item-title">{ref_item.title}</div>
                        <div class="reflib-item-meta">
                          <span class="reflib-item-authors">{authorsDisplay(ref_item.authors)}</span>
                          {ref_item.year && <span class="reflib-item-year">({ref_item.year})</span>}
                          {ref_item.journal && <span class="reflib-item-journal">{ref_item.journal}</span>}
                        </div>
                        <div class="reflib-item-tags">
                          <span class="reflib-item-type">{ref_item.ref_type}</span>
                          <span class="reflib-item-key">@{ref_item.cite_key}</span>
                        </div>
                      </div>
                    </button>
                  )}
                </For>
              </Show>
            </div>
          </div>

          {/* Detail panel */}
          <Show when={selectedRef()}>
            <ReferenceDetail
              reference={selectedRef()!}
              onClose={() => setSelectedRef(null)}
              onDelete={handleDelete}
              onStatusChange={handleStatusChange}
              onUpdate={async () => {
                await loadReferences();
                if (selectedRef()) {
                  const updated = await invoke<Reference>("ref_get", { id: selectedRef()!.id });
                  setSelectedRef(updated);
                }
              }}
              onOpenPdf={props.onOpenPdf}
            />
          </Show>
        </div>

        {/* Add by DOI modal */}
        <Show when={showAddDoi()}>
          <div class="reflib-modal-overlay" onClick={(e) => e.target === e.currentTarget && setShowAddDoi(false)}>
            <div class="reflib-modal">
              <h3>Add Reference by DOI</h3>
              <p>Paste a DOI to auto-fill all metadata from CrossRef.</p>
              <input
                type="text"
                placeholder="e.g. 10.1038/nature12373"
                value={doiInput()}
                onInput={(e) => setDoiInput(e.currentTarget.value)}
                onKeyDown={(e) => e.key === "Enter" && handleAddDoi()}
                autofocus
              />
              <div class="reflib-modal-actions">
                <button onClick={() => setShowAddDoi(false)}>Cancel</button>
                <button class="reflib-btn-primary" onClick={handleAddDoi} disabled={loading()}>
                  {loading() ? "Resolving..." : "Add"}
                </button>
              </div>
            </div>
          </div>
        </Show>

        {/* Import BibTeX modal */}
        <Show when={showImport()}>
          <ReferenceImport
            onClose={() => setShowImport(false)}
            onImported={async () => {
              setShowImport(false);
              await loadReferences();
              await loadCollections();
            }}
          />
        </Show>
      </div>
    </div>
  );
};

export default ReferenceLibrary;
