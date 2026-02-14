import { Component, createSignal, createEffect, onMount, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./ResearchHub.css";
import PaperGenerator from "./PaperGenerator";

// Types matching the Rust backend
interface SourceView {
  id: string;
  title: string;
  url: string | null;
  source_type: string;
  authors: string[];
  published_date: string | null;
  accessed_date: string;
  summary: string | null;
  key_points: string[];
  tags: string[];
  reliability_score: number | null;
  notes: string | null;
  publisher: string | null;
  doi: string | null;
  citations: Record<string, string>;
}

interface FactCheckResult {
  claim: string;
  verdict: string;
  confidence: number;
  supporting_sources: string[];
  contradicting_sources: string[];
  explanation: string;
}

type ViewMode = "list" | "add" | "detail" | "citations" | "fact-check" | "connections" | "discover" | "paper-generator";
type AddMode = "url" | "manual";

interface DiscoveredSource {
  title: string;
  url: string;
  snippet: string;
  relevance_reason: string | null;
}

interface DiscoveryResult {
  query_used: string;
  sources: DiscoveredSource[];
}

// MCP Agent result type
interface McpResearchResult {
  summary: string;
  sources: McpDiscoveredSource[];
  iterations: number;
}

interface McpDiscoveredSource {
  url: string;
  title: string;
  relevance: string;
  // Academic paper fields (optional)
  authors?: string[];
  year?: number;
  citation_count?: number;
  pdf_url?: string;
  doi?: string;
  venue?: string;
  source_type?: string;
}

type SearchMode = "web" | "academic";

const SOURCE_TYPES = [
  { value: "webpage", label: "Web Page" },
  { value: "article", label: "Article" },
  { value: "paper", label: "Academic Paper" },
  { value: "book", label: "Book" },
  { value: "video", label: "Video" },
  { value: "podcast", label: "Podcast" },
  { value: "documentation", label: "Documentation" },
  { value: "code_repository", label: "Code Repository" },
];

const CITATION_STYLES = [
  { value: "apa", label: "APA" },
  { value: "mla", label: "MLA" },
  { value: "chicago", label: "Chicago" },
  { value: "harvard", label: "Harvard" },
  { value: "ieee", label: "IEEE" },
  { value: "bibtex", label: "BibTeX" },
];

// Paper view for filtering
interface PaperInfo {
  id: string;
  title: string;
  source_ids: string[];
}

interface ResearchHubProps {
  onClose?: () => void;
}

const ResearchHub: Component<ResearchHubProps> = (props) => {
  // State
  const [viewMode, setViewMode] = createSignal<ViewMode>("list");
  const [sources, setSources] = createSignal<SourceView[]>([]);
  const [selectedSource, setSelectedSource] = createSignal<SourceView | null>(null);
  const [selectedSources, setSelectedSources] = createSignal<Set<string>>(new Set());
  const [searchQuery, setSearchQuery] = createSignal("");
  const [tagFilter, setTagFilter] = createSignal("");
  const [paperFilter, setPaperFilter] = createSignal<string>(""); // Paper ID to filter by
  const [papers, setPapers] = createSignal<PaperInfo[]>([]); // All papers for filter dropdown
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [successMsg, setSuccessMsg] = createSignal<string | null>(null);

  // Add source form state
  const [addMode, setAddMode] = createSignal<AddMode>("url");
  const [urlInput, setUrlInput] = createSignal("");
  const [titleInput, setTitleInput] = createSignal("");
  const [authorsInput, setAuthorsInput] = createSignal("");
  const [publishedDateInput, setPublishedDateInput] = createSignal("");
  const [sourceTypeInput, setSourceTypeInput] = createSignal("webpage");
  const [tagsInput, setTagsInput] = createSignal("");
  const [notesInput, setNotesInput] = createSignal("");
  const [contentInput, setContentInput] = createSignal("");

  // Citation state
  const [citationStyle, setCitationStyle] = createSignal("apa");
  const [generatedCitation, setGeneratedCitation] = createSignal("");
  const [bibliography, setBibliography] = createSignal("");

  // Fact-check state
  const [factCheckClaim, setFactCheckClaim] = createSignal("");
  const [factCheckResult, setFactCheckResult] = createSignal<FactCheckResult | null>(null);

  // Connections state
  const [connectionsResult, setConnectionsResult] = createSignal<any>(null);

  // Discovery state
  const [discoveryInput, setDiscoveryInput] = createSignal("");
  const [discoveryResult, setDiscoveryResult] = createSignal<DiscoveryResult | null>(null);
  const [isDiscovering, setIsDiscovering] = createSignal(false);

  // MCP Agent state
  const [useAgent, setUseAgent] = createSignal(true); // Default to agent mode
  const [searchMode, setSearchMode] = createSignal<SearchMode>("web"); // web or academic
  const [mcpResult, setMcpResult] = createSignal<McpResearchResult | null>(null);

  // Tag editing state
  const [newTagInput, setNewTagInput] = createSignal("");

  // Time filter state
  const [timeFilter, setTimeFilter] = createSignal<string>("all"); // all, 7d, 30d, 90d

  // Pagination state
  const [currentPage, setCurrentPage] = createSignal<number>(1);
  const [pageSize] = createSignal<number>(50); // Sources per page
  const [totalSources, setTotalSources] = createSignal<number>(0);
  const [totalPages, setTotalPages] = createSignal<number>(0);

  onMount(() => {
    loadSources();
    loadTotalSources();
    loadPapers();
  });

  // Load all papers for the filter dropdown
  const loadPapers = async () => {
    try {
      const result = await invoke<any[]>("paper_list", { limit: 100 });
      // Load full paper details to get source_ids
      const paperInfos: PaperInfo[] = [];
      for (const p of result) {
        try {
          const full = await invoke<any>("paper_get", { paperId: p.id });
          paperInfos.push({
            id: full.id,
            title: full.title,
            source_ids: full.source_ids || [],
          });
        } catch {
          // Skip papers that fail to load
        }
      }
      setPapers(paperInfos);
    } catch (e) {
      console.error("Failed to load papers:", e);
    }
  };

  // Get papers that contain a specific source
  const getPapersForSource = (sourceId: string): PaperInfo[] => {
    return papers().filter(p => p.source_ids.includes(sourceId));
  };

  // Get filtered sources based on paper filter and time filter
  const getFilteredSources = (): SourceView[] => {
    let filtered = sources();

    // Apply time filter
    const time = timeFilter();
    if (time !== "all") {
      const now = new Date();
      const cutoff = new Date();
      if (time === "7d") cutoff.setDate(now.getDate() - 7);
      else if (time === "30d") cutoff.setDate(now.getDate() - 30);
      else if (time === "90d") cutoff.setDate(now.getDate() - 90);

      filtered = filtered.filter(s => new Date(s.accessed_date) >= cutoff);
    }

    // Apply paper filter
    const filter = paperFilter();
    if (!filter) return filtered;

    // Show sources not assigned to any paper
    if (filter === "__unassigned__") {
      const allAssigned = new Set(papers().flatMap(p => p.source_ids));
      return filtered.filter(s => !allAssigned.has(s.id));
    }

    const paper = papers().find(p => p.id === filter);
    if (!paper) return filtered;

    return filtered.filter(s => paper.source_ids.includes(s.id));
  };

  // Format relative time (e.g., "2 days ago", "3 weeks ago")
  const formatRelativeTime = (dateStr: string): string => {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

    if (diffDays === 0) return "Today";
    if (diffDays === 1) return "Yesterday";
    if (diffDays < 7) return `${diffDays} days ago`;
    if (diffDays < 30) return `${Math.floor(diffDays / 7)} weeks ago`;
    if (diffDays < 365) return `${Math.floor(diffDays / 30)} months ago`;
    return `${Math.floor(diffDays / 365)} years ago`;
  };

  const loadSources = async (page?: number) => {
    setIsLoading(true);
    setError(null);
    try {
      const filter = tagFilter() || undefined;
      const targetPage = page ?? currentPage();
      const offset = (targetPage - 1) * pageSize();
      const results = await invoke<SourceView[]>("research_list_sources", {
        tagFilter: filter,
        limit: pageSize(),
        offset: offset,
      });
      setSources(results);
      // Note: totalSources is set separately by loadTotalSources
      if (page) setCurrentPage(page);
    } catch (e) {
      setError(`Failed to load sources: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Load total count for pagination UI
  const loadTotalSources = async () => {
    try {
      const filter = tagFilter() || undefined;
      // Request all sources (up to 500) to get total count
      const allResults = await invoke<SourceView[]>("research_list_sources", {
        tagFilter: filter,
        limit: 500,
        offset: 0,
      });
      const total = allResults.length;
      setTotalSources(total);
      setTotalPages(Math.ceil(total / pageSize()));
    } catch (e) {
      console.error("Failed to load total count:", e);
    }
  };

  // Pagination handlers
  const handleNextPage = async () => {
    if (currentPage() < totalPages()) {
      await loadSources(currentPage() + 1);
    }
  };

  const handlePreviousPage = async () => {
    if (currentPage() > 1) {
      await loadSources(currentPage() - 1);
    }
  };

  const handlePageJump = async (page: number) => {
    if (page >= 1 && page <= totalPages() && page !== currentPage()) {
      await loadSources(page);
    }
  };

  const searchSources = async () => {
    if (!searchQuery().trim()) {
      loadSources();
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const results = await invoke<[SourceView, number][]>("research_search_sources", {
        query: searchQuery(),
        limit: 50,
      });
      setSources(results.map(([source, _score]) => source));
    } catch (e) {
      setError(`Search failed: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const addSourceFromUrl = async () => {
    if (!urlInput().trim()) {
      setError("URL is required");
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const tags = tagsInput().split(",").map(t => t.trim()).filter(t => t);
      const source = await invoke<SourceView>("research_add_from_url", {
        url: urlInput(),
        tags: tags.length > 0 ? tags : undefined,
      });
      setSuccessMsg(`Added source: ${source.title}`);
      resetForm();
      setViewMode("list");
      // Reload first page to show new source
      setCurrentPage(1);
      await loadSources(1);
      await loadTotalSources();
    } catch (e) {
      setError(`Failed to add source: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const addSourceManual = async () => {
    if (!titleInput().trim()) {
      setError("Title is required");
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const authors = authorsInput().split(",").map(a => a.trim()).filter(a => a);
      const tags = tagsInput().split(",").map(t => t.trim()).filter(t => t);

      const source = await invoke<SourceView>("research_add_manual", {
        request: {
          title: titleInput(),
          url: urlInput() || undefined,
          authors: authors.length > 0 ? authors : undefined,
          published_date: publishedDateInput() || undefined,
          source_type: sourceTypeInput(),
          tags: tags.length > 0 ? tags : undefined,
          notes: notesInput() || undefined,
          content: contentInput() || undefined,
        },
      });
      setSuccessMsg(`Added source: ${source.title}`);
      resetForm();
      setViewMode("list");
      // Reload first page to show new source
      setCurrentPage(1);
      await loadSources(1);
      await loadTotalSources();
    } catch (e) {
      setError(`Failed to add source: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const resetForm = () => {
    setUrlInput("");
    setTitleInput("");
    setAuthorsInput("");
    setPublishedDateInput("");
    setSourceTypeInput("webpage");
    setTagsInput("");
    setNotesInput("");
    setContentInput("");
  };

  const viewSource = async (sourceId: string) => {
    setIsLoading(true);
    try {
      const source = await invoke<SourceView>("research_get_source", { sourceId });
      setSelectedSource(source);
      setViewMode("detail");
    } catch (e) {
      setError(`Failed to load source: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const deleteSource = async (sourceId: string) => {
    if (!confirm("Delete this source?")) return;

    setIsLoading(true);
    try {
      await invoke("research_delete_source", { sourceId });
      setSuccessMsg("Source deleted");
      setSelectedSource(null);
      setViewMode("list");
      loadSources();
    } catch (e) {
      setError(`Failed to delete source: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const summarizeSource = async (sourceId: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const source = await invoke<SourceView>("research_summarize_source", { sourceId });
      setSelectedSource(source);
      setSuccessMsg("Summary generated");
    } catch (e) {
      setError(`Failed to summarize: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const addTag = async (tag: string) => {
    const source = selectedSource();
    if (!source || !tag.trim()) return;

    const newTag = tag.trim().toLowerCase();
    if (source.tags.includes(newTag)) {
      setError("Tag already exists");
      return;
    }

    setIsLoading(true);
    try {
      const updatedSource = await invoke<SourceView>("research_update_tags", {
        sourceId: source.id,
        tags: [...source.tags, newTag],
      });
      setSelectedSource(updatedSource);
      setNewTagInput("");
      setSuccessMsg("Tag added");
      loadSources(); // Refresh list
    } catch (e) {
      setError(`Failed to add tag: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const removeTag = async (tagToRemove: string) => {
    const source = selectedSource();
    if (!source) return;

    setIsLoading(true);
    try {
      const updatedSource = await invoke<SourceView>("research_update_tags", {
        sourceId: source.id,
        tags: source.tags.filter(t => t !== tagToRemove),
      });
      setSelectedSource(updatedSource);
      setSuccessMsg("Tag removed");
      loadSources(); // Refresh list
    } catch (e) {
      setError(`Failed to remove tag: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const generateCitation = async () => {
    const source = selectedSource();
    if (!source) return;

    setIsLoading(true);
    try {
      const citation = await invoke<string>("research_generate_citation", {
        sourceId: source.id,
        style: citationStyle(),
      });
      setGeneratedCitation(citation);
    } catch (e) {
      setError(`Failed to generate citation: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const generateBibliography = async () => {
    const selected = Array.from(selectedSources());
    if (selected.length === 0) {
      setError("Select at least one source");
      return;
    }

    setIsLoading(true);
    try {
      const bib = await invoke<string>("research_generate_bibliography", {
        sourceIds: selected,
        style: citationStyle(),
      });
      setBibliography(bib);
    } catch (e) {
      setError(`Failed to generate bibliography: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const runFactCheck = async () => {
    if (!factCheckClaim().trim()) {
      setError("Enter a claim to fact-check");
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const selected = Array.from(selectedSources());
      const result = await invoke<FactCheckResult>("research_fact_check", {
        claim: factCheckClaim(),
        sourceIds: selected.length > 0 ? selected : undefined,
      });
      setFactCheckResult(result);
    } catch (e) {
      setError(`Fact-check failed: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const findConnections = async () => {
    const selected = Array.from(selectedSources());
    if (selected.length < 2) {
      setError("Select at least 2 sources to find connections");
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<any>("research_find_connections", {
        sourceIds: selected,
      });
      setConnectionsResult(result);
    } catch (e) {
      setError(`Failed to find connections: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const discoverSources = async () => {
    if (!discoveryInput().trim()) {
      setError("Enter a research topic or description");
      return;
    }

    setIsDiscovering(true);
    setError(null);
    setDiscoveryResult(null);
    setMcpResult(null);

    try {
      if (searchMode() === "academic") {
        // Academic: always use direct search (no LLM needed)
        const result = await invoke<{ query: string; papers: any[] }>("mcp_academic_search", {
          query: discoveryInput(),
          numResults: 15,
        });
        // Convert to McpResearchResult format
        setMcpResult({
          summary: `Found ${result.papers.length} papers from Semantic Scholar and arXiv`,
          sources: result.papers.map((p: any) => ({
            url: p.url,
            title: p.title,
            relevance: p.abstract_text || "",
            authors: p.authors,
            year: p.year,
            citation_count: p.citation_count,
            pdf_url: p.pdf_url,
            doi: p.doi,
            venue: p.venue,
            source_type: p.source,
          })),
          iterations: 0,
        });
      } else if (useAgent()) {
        // Web + Agent: use LLM-powered research
        const result = await invoke<McpResearchResult>("mcp_research", {
          topic: discoveryInput(),
          notes: null,
        });
        setMcpResult(result);
      } else {
        // Web + Simple: basic web search
        const result = await invoke<DiscoveryResult>("research_discover_sources", {
          description: discoveryInput(),
          maxResults: 10,
        });
        setDiscoveryResult(result);
      }
    } catch (e) {
      setError(`Discovery failed: ${e}`);
    } finally {
      setIsDiscovering(false);
    }
  };

  const addDiscoveredSource = async (source: DiscoveredSource | McpDiscoveredSource) => {
    setIsLoading(true);
    setError(null);
    try {
      // Check if this is an academic source with metadata
      const mcpSource = source as McpDiscoveredSource;
      const isAcademic = mcpSource.source_type === "semantic_scholar" || mcpSource.source_type === "arxiv";

      if (isAcademic && mcpSource.authors) {
        // Use manual add with metadata for academic papers (avoids URL fetch)
        await invoke("research_add_manual", {
          request: {
            title: source.title,
            url: source.url,
            authors: mcpSource.authors,
            published_date: mcpSource.year ? `${mcpSource.year}` : null,
            source_type: "paper",
            tags: [mcpSource.source_type || "academic"],
            notes: mcpSource.relevance ? `Abstract: ${mcpSource.relevance}` : null,
          },
        });
      } else {
        // Use URL fetch for web sources
        await invoke("research_add_from_url", {
          url: source.url,
          tags: [],
        });
      }
      setSuccessMsg(`Added: ${source.title}`);
      setTimeout(() => setSuccessMsg(null), 2000);
      loadSources();
    } catch (e) {
      setError(`Failed to add source: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const toggleSourceSelection = (sourceId: string) => {
    const current = new Set(selectedSources());
    if (current.has(sourceId)) {
      current.delete(sourceId);
    } else {
      current.add(sourceId);
    }
    setSelectedSources(current);
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    setSuccessMsg("Copied to clipboard");
    setTimeout(() => setSuccessMsg(null), 2000);
  };

  const formatDate = (dateStr: string) => {
    try {
      return new Date(dateStr).toLocaleDateString();
    } catch {
      return dateStr;
    }
  };

  const getVerdictColor = (verdict: string) => {
    switch (verdict.toLowerCase()) {
      case "supported": return "#4caf50";
      case "contradicted": return "#f44336";
      case "partially_supported": return "#ff9800";
      default: return "#9e9e9e";
    }
  };

  // Clear messages after timeout
  createEffect(() => {
    if (successMsg()) {
      setTimeout(() => setSuccessMsg(null), 3000);
    }
  });

  return (
    <div class="research-hub">
      {/* Header */}
      <div class="research-header">
        <div class="research-title">
          <span class="research-icon">📚</span>
          <span>Research Hub</span>
        </div>
        <div class="research-actions">
          <button
            class="icon-btn"
            onClick={() => { setViewMode("add"); setAddMode("url"); }}
            title="Add Source"
          >
            +
          </button>
          <Show when={props.onClose}>
            <button class="icon-btn" onClick={props.onClose} title="Close">
              ×
            </button>
          </Show>
        </div>
      </div>

      {/* Navigation tabs */}
      <div class="research-tabs">
        <button
          class={`tab-btn ${viewMode() === "list" ? "active" : ""}`}
          onClick={() => setViewMode("list")}
        >
          Sources
        </button>
        <button
          class={`tab-btn ${viewMode() === "citations" ? "active" : ""}`}
          onClick={() => setViewMode("citations")}
        >
          Citations
        </button>
        <button
          class={`tab-btn ${viewMode() === "fact-check" ? "active" : ""}`}
          onClick={() => setViewMode("fact-check")}
        >
          Fact-Check
        </button>
        <button
          class={`tab-btn ${viewMode() === "connections" ? "active" : ""}`}
          onClick={() => setViewMode("connections")}
        >
          Connections
        </button>
        <button
          class={`tab-btn ${viewMode() === "discover" ? "active" : ""}`}
          onClick={() => setViewMode("discover")}
        >
          Discover
        </button>
        <button
          class={`tab-btn ${viewMode() === "paper-generator" ? "active" : ""}`}
          onClick={() => setViewMode("paper-generator")}
        >
          Paper
        </button>
      </div>

      {/* Error/Success messages */}
      <Show when={error()}>
        <div class="error-banner">
          <span>{error()}</span>
          <button onClick={() => setError(null)}>×</button>
        </div>
      </Show>

      <Show when={successMsg()}>
        <div class="success-banner">
          <span>{successMsg()}</span>
        </div>
      </Show>

      {/* Main content */}
      <div class="research-content">
        {/* List View */}
        <Show when={viewMode() === "list"}>
          <div class="source-list-view">
            {/* Search bar */}
            <div class="search-bar">
              <input
                type="text"
                placeholder="Search sources..."
                value={searchQuery()}
                onInput={(e) => setSearchQuery(e.currentTarget.value)}
                onKeyPress={(e) => e.key === "Enter" && searchSources()}
              />
              <button class="btn-secondary" onClick={searchSources}>
                Search
              </button>
              <button class="btn-secondary" onClick={loadSources}>
                Refresh
              </button>
            </div>

            {/* Filters row */}
            <div class="filters-row">
              <div class="tag-filter">
                <input
                  type="text"
                  placeholder="Filter by tag..."
                  value={tagFilter()}
                  onInput={(e) => setTagFilter(e.currentTarget.value)}
                  onKeyPress={(e) => e.key === "Enter" && (setCurrentPage(1), loadSources(1), loadTotalSources())}
                />
              </div>
              <div class="time-filter">
                <select
                  value={timeFilter()}
                  onChange={(e) => setTimeFilter(e.currentTarget.value)}
                >
                  <option value="all">All time</option>
                  <option value="7d">Last 7 days</option>
                  <option value="30d">Last 30 days</option>
                  <option value="90d">Last 90 days</option>
                </select>
              </div>
              <div class="paper-filter">
                <select
                  value={paperFilter()}
                  onChange={(e) => setPaperFilter(e.currentTarget.value)}
                >
                  <option value="">All papers</option>
                  <option value="__unassigned__">Unassigned</option>
                  <For each={papers()}>
                    {(paper) => (
                      <option value={paper.id}>
                        {paper.title} ({paper.source_ids.length})
                      </option>
                    )}
                  </For>
                </select>
              </div>
            </div>

            {/* Filter summary */}
            <Show when={paperFilter()}>
              <div class="filter-summary">
                <span class="filter-info">
                  {paperFilter() === "__unassigned__"
                    ? `Showing ${getFilteredSources().length} unassigned sources`
                    : `Showing ${getFilteredSources().length} sources in "${papers().find(p => p.id === paperFilter())?.title || 'Unknown'}"`}
                </span>
                <button class="btn-clear-filter" onClick={() => setPaperFilter("")}>
                  Clear filter
                </button>
              </div>
            </Show>

            {/* Source list */}
            <div class="source-list">
              <Show when={isLoading()}>
                <div class="loading-indicator">Loading...</div>
              </Show>

              <Show when={!isLoading() && getFilteredSources().length === 0 && paperFilter()}>
                <div class="empty-state">
                  <p>No sources match this filter</p>
                  <button class="btn-secondary" onClick={() => setPaperFilter("")}>
                    Clear filter
                  </button>
                </div>
              </Show>

              <Show when={!isLoading() && sources().length === 0 && !paperFilter()}>
                <div class="empty-state">
                  <p>No sources yet</p>
                  <button class="btn-primary" onClick={() => setViewMode("add")}>
                    Add your first source
                  </button>
                </div>
              </Show>

              <For each={getFilteredSources()}>
                {(source) => (
                  <div class="source-item">
                    <input
                      type="checkbox"
                      checked={selectedSources().has(source.id)}
                      onChange={() => toggleSourceSelection(source.id)}
                    />
                    <div class="source-info" onClick={() => viewSource(source.id)}>
                      <div class="source-title">{source.title}</div>
                      <div class="source-meta">
                        <span class="source-type">{source.source_type}</span>
                        <Show when={source.authors.length > 0}>
                          <span class="source-authors">
                            {source.authors.slice(0, 2).join(", ")}
                            {source.authors.length > 2 ? " et al." : ""}
                          </span>
                        </Show>
                        <Show when={source.published_date}>
                          <span class="source-date">{source.published_date}</span>
                        </Show>
                        <span class="source-added" title={`Added: ${formatDate(source.accessed_date)}`}>
                          {formatRelativeTime(source.accessed_date)}
                        </span>
                      </div>
                      {/* Paper badges - show which papers this source belongs to */}
                      <Show when={getPapersForSource(source.id).length > 0}>
                        <div class="source-papers">
                          <For each={getPapersForSource(source.id)}>
                            {(paper) => (
                              <span
                                class="paper-badge"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setPaperFilter(paper.id);
                                }}
                                title={`Filter by: ${paper.title}`}
                              >
                                📄 {paper.title.length > 20 ? paper.title.slice(0, 20) + "..." : paper.title}
                              </span>
                            )}
                          </For>
                        </div>
                      </Show>
                      <Show when={source.summary}>
                        <div class="source-summary">{source.summary}</div>
                      </Show>
                      <Show when={source.tags.length > 0}>
                        <div class="source-tags">
                          <For each={source.tags}>
                            {(tag) => <span class="tag">{tag}</span>}
                          </For>
                        </div>
                      </Show>
                    </div>
                  </div>
                )}
              </For>

              {/* Pagination controls */}
              <Show when={totalPages() > 1}>
                <div class="pagination-controls">
                  <button
                    class="pagination-btn"
                    disabled={currentPage() === 1}
                    onClick={handlePreviousPage}
                  >
                    Previous
                  </button>
                  <span class="pagination-info">
                    Page {currentPage()} of {totalPages()}
                    <Show when={totalSources() > 0}>
                      ({totalSources()} total)
                    </Show>
                  </span>
                  <button
                    class="pagination-btn"
                    disabled={currentPage() >= totalPages()}
                    onClick={handleNextPage}
                  >
                    Next
                  </button>
                </div>
              </Show>
            </div>

            {/* Selection actions */}
            <Show when={selectedSources().size > 0}>
              <div class="selection-actions">
                <span>{selectedSources().size} selected</span>
                <button class="btn-secondary" onClick={() => setSelectedSources(new Set())}>
                  Clear
                </button>
                <button class="btn-secondary" onClick={() => setViewMode("citations")}>
                  Generate Citations
                </button>
                <button class="btn-secondary" onClick={() => setViewMode("connections")}>
                  Find Connections
                </button>
              </div>
            </Show>
          </div>
        </Show>

        {/* Add Source View */}
        <Show when={viewMode() === "add"}>
          <div class="add-source-view">
            <div class="add-mode-tabs">
              <button
                class={`mode-tab ${addMode() === "url" ? "active" : ""}`}
                onClick={() => setAddMode("url")}
              >
                From URL
              </button>
              <button
                class={`mode-tab ${addMode() === "manual" ? "active" : ""}`}
                onClick={() => setAddMode("manual")}
              >
                Manual Entry
              </button>
            </div>

            <Show when={addMode() === "url"}>
              <div class="form-group">
                <label>URL *</label>
                <input
                  type="url"
                  placeholder="https://example.com/article"
                  value={urlInput()}
                  onInput={(e) => setUrlInput(e.currentTarget.value)}
                />
              </div>
              <div class="form-group">
                <label>Tags (comma-separated)</label>
                <input
                  type="text"
                  placeholder="research, ai, machine-learning"
                  value={tagsInput()}
                  onInput={(e) => setTagsInput(e.currentTarget.value)}
                />
              </div>
              <div class="form-actions">
                <button class="btn-secondary" onClick={() => setViewMode("list")}>
                  Cancel
                </button>
                <button
                  class="btn-primary"
                  onClick={addSourceFromUrl}
                  disabled={isLoading()}
                >
                  {isLoading() ? "Adding..." : "Add Source"}
                </button>
              </div>
            </Show>

            <Show when={addMode() === "manual"}>
              <div class="form-group">
                <label>Title *</label>
                <input
                  type="text"
                  placeholder="Article or Book Title"
                  value={titleInput()}
                  onInput={(e) => setTitleInput(e.currentTarget.value)}
                />
              </div>
              <div class="form-group">
                <label>URL (optional)</label>
                <input
                  type="url"
                  placeholder="https://example.com"
                  value={urlInput()}
                  onInput={(e) => setUrlInput(e.currentTarget.value)}
                />
              </div>
              <div class="form-row">
                <div class="form-group">
                  <label>Authors (comma-separated)</label>
                  <input
                    type="text"
                    placeholder="John Smith, Jane Doe"
                    value={authorsInput()}
                    onInput={(e) => setAuthorsInput(e.currentTarget.value)}
                  />
                </div>
                <div class="form-group">
                  <label>Published Date</label>
                  <input
                    type="text"
                    placeholder="2024 or 2024-01-15"
                    value={publishedDateInput()}
                    onInput={(e) => setPublishedDateInput(e.currentTarget.value)}
                  />
                </div>
              </div>
              <div class="form-row">
                <div class="form-group">
                  <label>Source Type</label>
                  <select
                    value={sourceTypeInput()}
                    onChange={(e) => setSourceTypeInput(e.currentTarget.value)}
                  >
                    <For each={SOURCE_TYPES}>
                      {(type) => <option value={type.value}>{type.label}</option>}
                    </For>
                  </select>
                </div>
                <div class="form-group">
                  <label>Tags (comma-separated)</label>
                  <input
                    type="text"
                    placeholder="research, ai"
                    value={tagsInput()}
                    onInput={(e) => setTagsInput(e.currentTarget.value)}
                  />
                </div>
              </div>
              <div class="form-group">
                <label>Notes</label>
                <textarea
                  placeholder="Your notes about this source..."
                  value={notesInput()}
                  onInput={(e) => setNotesInput(e.currentTarget.value)}
                  rows={3}
                />
              </div>
              <div class="form-group">
                <label>Content (optional - for embedding/search)</label>
                <textarea
                  placeholder="Paste relevant content or abstract..."
                  value={contentInput()}
                  onInput={(e) => setContentInput(e.currentTarget.value)}
                  rows={5}
                />
              </div>
              <div class="form-actions">
                <button class="btn-secondary" onClick={() => setViewMode("list")}>
                  Cancel
                </button>
                <button
                  class="btn-primary"
                  onClick={addSourceManual}
                  disabled={isLoading()}
                >
                  {isLoading() ? "Adding..." : "Add Source"}
                </button>
              </div>
            </Show>
          </div>
        </Show>

        {/* Detail View */}
        <Show when={viewMode() === "detail" && selectedSource()}>
          <div class="source-detail-view">
            <button class="back-btn" onClick={() => setViewMode("list")}>
              ← Back to list
            </button>

            <div class="detail-header">
              <h2>{selectedSource()!.title}</h2>
              <div class="detail-actions">
                <button
                  class="btn-secondary"
                  onClick={() => summarizeSource(selectedSource()!.id)}
                  disabled={isLoading()}
                >
                  {isLoading() ? "..." : "Summarize"}
                </button>
                <button
                  class="btn-secondary"
                  onClick={() => deleteSource(selectedSource()!.id)}
                >
                  Delete
                </button>
              </div>
            </div>

            <div class="detail-meta">
              <Show when={selectedSource()!.url}>
                <div class="meta-item">
                  <span class="meta-label">URL:</span>
                  <a href={selectedSource()!.url!} target="_blank" rel="noopener">
                    {selectedSource()!.url}
                  </a>
                </div>
              </Show>
              <Show when={selectedSource()!.authors.length > 0}>
                <div class="meta-item">
                  <span class="meta-label">Authors:</span>
                  <span>{selectedSource()!.authors.join(", ")}</span>
                </div>
              </Show>
              <Show when={selectedSource()!.published_date}>
                <div class="meta-item">
                  <span class="meta-label">Published:</span>
                  <span>{selectedSource()!.published_date}</span>
                </div>
              </Show>
              <div class="meta-item">
                <span class="meta-label">Type:</span>
                <span>{selectedSource()!.source_type}</span>
              </div>
              <div class="meta-item">
                <span class="meta-label">Added:</span>
                <span>{formatDate(selectedSource()!.accessed_date)}</span>
              </div>
            </div>

            <Show when={selectedSource()!.summary}>
              <div class="detail-section">
                <h3>Summary</h3>
                <p>{selectedSource()!.summary}</p>
              </div>
            </Show>

            <Show when={selectedSource()!.key_points.length > 0}>
              <div class="detail-section">
                <h3>Key Points</h3>
                <ul>
                  <For each={selectedSource()!.key_points}>
                    {(point) => <li>{point}</li>}
                  </For>
                </ul>
              </div>
            </Show>

            <Show when={selectedSource()!.notes}>
              <div class="detail-section">
                <h3>Notes</h3>
                <p>{selectedSource()!.notes}</p>
              </div>
            </Show>

            <div class="detail-section">
              <h3>Tags</h3>
              <div class="tags-editor">
                <div class="current-tags">
                  <Show when={selectedSource()!.tags.length === 0}>
                    <span class="no-tags">No tags yet</span>
                  </Show>
                  <For each={selectedSource()!.tags}>
                    {(tag) => (
                      <span class="editable-tag">
                        {tag}
                        <button
                          class="tag-remove-btn"
                          onClick={() => removeTag(tag)}
                          title="Remove tag"
                        >
                          ×
                        </button>
                      </span>
                    )}
                  </For>
                </div>
                <div class="add-tag-form">
                  <input
                    type="text"
                    placeholder="Add a tag..."
                    value={newTagInput()}
                    onInput={(e) => setNewTagInput(e.currentTarget.value)}
                    onKeyPress={(e) => {
                      if (e.key === "Enter") {
                        addTag(newTagInput());
                      }
                    }}
                  />
                  <button
                    class="btn-secondary"
                    onClick={() => addTag(newTagInput())}
                    disabled={!newTagInput().trim()}
                  >
                    Add
                  </button>
                </div>
              </div>
            </div>

            <div class="detail-section">
              <h3>Citations</h3>
              <div class="citation-selector">
                <select
                  value={citationStyle()}
                  onChange={(e) => setCitationStyle(e.currentTarget.value)}
                >
                  <For each={CITATION_STYLES}>
                    {(style) => <option value={style.value}>{style.label}</option>}
                  </For>
                </select>
                <button class="btn-secondary" onClick={generateCitation}>
                  Generate
                </button>
              </div>
              <Show when={generatedCitation()}>
                <div class="citation-output">
                  <pre>{generatedCitation()}</pre>
                  <button
                    class="copy-btn"
                    onClick={() => copyToClipboard(generatedCitation())}
                  >
                    Copy
                  </button>
                </div>
              </Show>
            </div>
          </div>
        </Show>

        {/* Citations View */}
        <Show when={viewMode() === "citations"}>
          <div class="citations-view">
            <h3>Generate Bibliography</h3>
            <p class="help-text">
              Select sources from the list, then generate a bibliography in your preferred style.
            </p>

            <div class="citation-controls">
              <select
                value={citationStyle()}
                onChange={(e) => setCitationStyle(e.currentTarget.value)}
              >
                <For each={CITATION_STYLES}>
                  {(style) => <option value={style.value}>{style.label}</option>}
                </For>
              </select>
              <button
                class="btn-primary"
                onClick={generateBibliography}
                disabled={isLoading() || selectedSources().size === 0}
              >
                Generate Bibliography ({selectedSources().size} sources)
              </button>
            </div>

            <Show when={bibliography()}>
              <div class="bibliography-output">
                <div class="output-header">
                  <h4>Bibliography</h4>
                  <button
                    class="copy-btn"
                    onClick={() => copyToClipboard(bibliography())}
                  >
                    Copy
                  </button>
                </div>
                <pre>{bibliography()}</pre>
              </div>
            </Show>

            {/* Source selection list */}
            <div class="source-select-list">
              <h4>Select Sources</h4>
              <For each={sources()}>
                {(source) => (
                  <div class="source-select-item">
                    <input
                      type="checkbox"
                      checked={selectedSources().has(source.id)}
                      onChange={() => toggleSourceSelection(source.id)}
                    />
                    <span>{source.title}</span>
                  </div>
                )}
              </For>
            </div>
          </div>
        </Show>

        {/* Fact-Check View */}
        <Show when={viewMode() === "fact-check"}>
          <div class="fact-check-view">
            <h3>Fact-Check a Claim</h3>
            <p class="help-text">
              Enter a claim to verify against your collected sources.
              Optionally select specific sources to check against.
            </p>

            <div class="form-group">
              <label>Claim to verify</label>
              <textarea
                placeholder="Enter a statement or claim to fact-check..."
                value={factCheckClaim()}
                onInput={(e) => setFactCheckClaim(e.currentTarget.value)}
                rows={3}
              />
            </div>

            <button
              class="btn-primary"
              onClick={runFactCheck}
              disabled={isLoading() || !factCheckClaim().trim()}
            >
              {isLoading() ? "Checking..." : "Fact-Check"}
            </button>

            <Show when={factCheckResult()}>
              <div class="fact-check-result">
                <div
                  class="verdict"
                  style={{ "border-color": getVerdictColor(factCheckResult()!.verdict) }}
                >
                  <span class="verdict-label">Verdict:</span>
                  <span
                    class="verdict-value"
                    style={{ color: getVerdictColor(factCheckResult()!.verdict) }}
                  >
                    {factCheckResult()!.verdict.replace("_", " ").toUpperCase()}
                  </span>
                  <span class="confidence">
                    ({Math.round(factCheckResult()!.confidence * 100)}% confidence)
                  </span>
                </div>

                <div class="explanation">
                  <h4>Explanation</h4>
                  <p>{factCheckResult()!.explanation}</p>
                </div>

                <Show when={factCheckResult()!.supporting_sources.length > 0}>
                  <div class="source-list-mini">
                    <h4>Supporting Sources</h4>
                    <For each={factCheckResult()!.supporting_sources}>
                      {(id) => <span class="source-ref">{id}</span>}
                    </For>
                  </div>
                </Show>

                <Show when={factCheckResult()!.contradicting_sources.length > 0}>
                  <div class="source-list-mini">
                    <h4>Contradicting Sources</h4>
                    <For each={factCheckResult()!.contradicting_sources}>
                      {(id) => <span class="source-ref">{id}</span>}
                    </For>
                  </div>
                </Show>
              </div>
            </Show>
          </div>
        </Show>

        {/* Connections View */}
        <Show when={viewMode() === "connections"}>
          <div class="connections-view">
            <h3>Find Connections</h3>
            <p class="help-text">
              Select 2 or more sources to discover themes, relationships,
              and connections between them.
            </p>

            <button
              class="btn-primary"
              onClick={findConnections}
              disabled={isLoading() || selectedSources().size < 2}
            >
              {isLoading() ? "Analyzing..." : `Find Connections (${selectedSources().size} sources)`}
            </button>

            <Show when={connectionsResult()}>
              <div class="connections-result">
                <Show when={connectionsResult().common_themes?.length > 0}>
                  <div class="themes-section">
                    <h4>Common Themes</h4>
                    <div class="themes-list">
                      <For each={connectionsResult().common_themes}>
                        {(theme: string) => <span class="theme-tag">{theme}</span>}
                      </For>
                    </div>
                  </div>
                </Show>

                <Show when={connectionsResult().synthesis}>
                  <div class="synthesis-section">
                    <h4>Synthesis</h4>
                    <p>{connectionsResult().synthesis}</p>
                  </div>
                </Show>

                <Show when={connectionsResult().connections?.length > 0}>
                  <div class="connections-list">
                    <h4>Connections</h4>
                    <For each={connectionsResult().connections}>
                      {(conn: any) => (
                        <div class="connection-item">
                          <div class="connection-header">
                            <span class="source-ref">{conn.source_a}</span>
                            <span class="relationship">{conn.relationship}</span>
                            <span class="source-ref">{conn.source_b}</span>
                          </div>
                          <p class="connection-desc">{conn.description}</p>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </Show>

            {/* Source selection */}
            <div class="source-select-list">
              <h4>Select Sources to Analyze</h4>
              <For each={sources()}>
                {(source) => (
                  <div class="source-select-item">
                    <input
                      type="checkbox"
                      checked={selectedSources().has(source.id)}
                      onChange={() => toggleSourceSelection(source.id)}
                    />
                    <span>{source.title}</span>
                  </div>
                )}
              </For>
            </div>
          </div>
        </Show>

        {/* Discover View */}
        <Show when={viewMode() === "discover"}>
          <div class="discover-view">
            <h3>Discover Sources</h3>
            <p class="help-text">
              Describe your research topic and let AI find relevant sources for you.
            </p>

            {/* Mode Toggles */}
            <div class="mode-toggles">
              {/* Search Mode Selector */}
              <div class="search-mode-selector">
                <button
                  class={`mode-btn ${searchMode() === "web" ? "active" : ""}`}
                  onClick={() => setSearchMode("web")}
                >
                  Web
                </button>
                <button
                  class={`mode-btn ${searchMode() === "academic" ? "active" : ""}`}
                  onClick={() => setSearchMode("academic")}
                >
                  Academic
                </button>
              </div>

              {/* Agent toggle only for web search */}
              <Show when={searchMode() === "web"}>
                <div class="mode-toggle">
                  <label class="toggle-label">
                    <input
                      type="checkbox"
                      checked={useAgent()}
                      onChange={(e) => setUseAgent(e.currentTarget.checked)}
                    />
                    <span class="toggle-text">
                      {useAgent() ? "Agent Mode (AI analyzes)" : "Simple Search"}
                    </span>
                  </label>
                </div>
              </Show>
            </div>

            <Show when={searchMode() === "academic"}>
              <p class="academic-hint">
                Direct search of Semantic Scholar + arXiv. No LLM required.
              </p>
            </Show>

            <div class="discover-input">
              <textarea
                placeholder={searchMode() === "academic"
                  ? "Enter keywords to search academic papers (e.g., 'machine learning transformers', 'quantum computing algorithms')"
                  : (useAgent()
                    ? "Describe what you're researching... The AI agent will search the web, analyze results, and find relevant sources."
                    : "Enter a search query...")
                }
                value={discoveryInput()}
                onInput={(e) => setDiscoveryInput(e.currentTarget.value)}
                rows={2}
              />
              <button
                class="btn-primary"
                onClick={discoverSources}
                disabled={isDiscovering() || !discoveryInput().trim()}
              >
                {isDiscovering()
                  ? (searchMode() === "academic" ? "Searching papers..." : (useAgent() ? "Agent researching..." : "Searching..."))
                  : (searchMode() === "academic" ? "Search Papers" : (useAgent() ? "Research with Agent" : "Search"))}
              </button>
            </div>

            {/* Results */}
            <Show when={mcpResult()}>
              <div class="mcp-results">
                <div class="mcp-summary">
                  <h4>{searchMode() === "academic" ? "Results" : "Research Summary"}</h4>
                  <p>{mcpResult()!.summary}</p>
                  <Show when={mcpResult()!.iterations > 0}>
                    <span class="iterations-badge">
                      {mcpResult()!.iterations} iteration{mcpResult()!.iterations !== 1 ? 's' : ''}
                    </span>
                  </Show>
                </div>

                <Show when={mcpResult()!.sources.length > 0}>
                  <h4>Discovered Sources</h4>
                  <div class="discovered-sources">
                    <For each={mcpResult()!.sources}>
                      {(source) => (
                        <div class={`discovered-source ${source.source_type === "semantic_scholar" || source.source_type === "arxiv" ? "academic-paper" : ""}`}>
                          <div class="source-header">
                            <h4 class="source-title">
                              <a href={source.url} target="_blank" rel="noopener noreferrer">
                                {source.title}
                              </a>
                            </h4>
                            <div class="source-actions">
                              <Show when={source.pdf_url}>
                                <a
                                  href={source.pdf_url}
                                  target="_blank"
                                  rel="noopener noreferrer"
                                  class="btn-pdf"
                                  title="Open PDF"
                                >
                                  PDF
                                </a>
                              </Show>
                              <button
                                class="btn-add"
                                onClick={() => addDiscoveredSource(source)}
                                disabled={isLoading()}
                                title="Add to collection"
                              >
                                + Add
                              </button>
                            </div>
                          </div>

                          {/* Academic paper metadata */}
                          <Show when={source.authors && source.authors.length > 0}>
                            <p class="paper-authors">
                              {source.authors!.slice(0, 3).join(", ")}
                              {source.authors!.length > 3 ? ` +${source.authors!.length - 3} more` : ""}
                            </p>
                          </Show>

                          <div class="paper-meta">
                            <Show when={source.year}>
                              <span class="meta-badge year">{source.year}</span>
                            </Show>
                            <Show when={source.citation_count !== undefined && source.citation_count !== null}>
                              <span class="meta-badge citations">{source.citation_count} citations</span>
                            </Show>
                            <Show when={source.venue}>
                              <span class="meta-badge venue">{source.venue}</span>
                            </Show>
                            <Show when={source.source_type}>
                              <span class={`meta-badge source-type ${source.source_type}`}>
                                {source.source_type === "semantic_scholar" ? "Semantic Scholar" :
                                 source.source_type === "arxiv" ? "arXiv" :
                                 source.source_type}
                              </span>
                            </Show>
                          </div>

                          <Show when={source.relevance}>
                            <p class="paper-abstract">
                              {source.relevance.length > 300
                                ? source.relevance.slice(0, 300) + "..."
                                : source.relevance}
                            </p>
                          </Show>

                          <Show when={source.doi}>
                            <span class="paper-doi">DOI: {source.doi}</span>
                          </Show>

                          <span class="source-url">{source.url}</span>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>

                <Show when={mcpResult()!.sources.length === 0}>
                  <p class="no-results">No sources found. Try a different topic.</p>
                </Show>
              </div>
            </Show>

            {/* Simple Search Results */}
            <Show when={discoveryResult()}>
              <div class="discovery-results">
                <div class="query-used">
                  <span class="label">Search query:</span>
                  <span class="query">{discoveryResult()!.query_used}</span>
                </div>

                <Show when={discoveryResult()!.sources.length === 0}>
                  <p class="no-results">No sources found. Try a different description.</p>
                </Show>

                <div class="discovered-sources">
                  <For each={discoveryResult()!.sources}>
                    {(source) => (
                      <div class="discovered-source">
                        <div class="source-header">
                          <h4 class="source-title">
                            <a href={source.url} target="_blank" rel="noopener noreferrer">
                              {source.title}
                            </a>
                          </h4>
                          <button
                            class="btn-add"
                            onClick={() => addDiscoveredSource(source)}
                            disabled={isLoading()}
                            title="Add to collection"
                          >
                            + Add
                          </button>
                        </div>
                        <p class="source-snippet">{source.snippet}</p>
                        <Show when={source.relevance_reason}>
                          <p class="relevance-reason">
                            <span class="label">Why relevant:</span> {source.relevance_reason}
                          </p>
                        </Show>
                        <span class="source-url">{source.url}</span>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>
          </div>
        </Show>

        {/* Paper Generator View */}
        <Show when={viewMode() === "paper-generator"}>
          <PaperGenerator onClose={() => setViewMode("list")} />
        </Show>
      </div>
    </div>
  );
};

export default ResearchHub;
