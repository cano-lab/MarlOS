import { Component, createSignal, createEffect, onMount, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./PaperGenerator.css";

// ============================================================================
// Types matching Rust backend
// ============================================================================

interface PaperView {
  id: string;
  title: string;
  research_question: string;
  thesis: string | null;
  paper_type: string;
  citation_style: string;
  source_count: number;
  finding_count: number;
  section_count: number;
  status: string;
  word_count: number;
  tags: string[];
  created_at: string;
  modified_at: string;
}

interface Paper {
  id: string;
  title: string;
  research_question: string;
  thesis: string | null;
  paper_type: string;
  citation_style: string;
  source_ids: string[];
  findings: KeyFinding[];
  sections: PaperSection[];
  status: string;
  abstract_text: string | null;
  bibliography: string[];
  tags: string[];
  created_at: string;
  modified_at: string;
}

interface PaperSection {
  id: string;
  title: string;
  section_type: string;
  order: number;
  content: string;
  rendered_content: string | null;
  citations: Citation[];
  findings_used: string[];
  status: string;
  word_count: number;
  target_word_count: number | null;
  review_notes: ReviewNote[];
  modified_at: string;
}

interface SectionView {
  id: string;
  title: string;
  section_type: string;
  order: number;
  status: string;
  word_count: number;
  target_word_count: number | null;
  citation_count: number;
  review_note_count: number;
  unresolved_notes: number;
}

interface KeyFinding {
  id: string;
  finding_type: string;
  content: string;
  source_id: string;
  location: string | null;
  relevance: number;
  tags: string[];
  used: boolean;
  used_in_section: string | null;
}

interface Citation {
  source_id: string;
  page: string | null;
  number: number | null;
  formatted: string | null;
}

interface ReviewNote {
  id: string;
  issue_type: string;
  description: string;
  suggestion: string | null;
  resolved: boolean;
  created_at: string;
}

interface ReviewResult {
  section_id: string;
  quality_score: number;
  issues: ReviewIssue[];
  suggestions: string[];
  passes: boolean;
}

interface ReviewIssue {
  issue_type: string;
  description: string;
  location: string | null;
  severity: string;
}

// ChunkingProgress interface reserved for future use

interface ExtractionProgress {
  total_chunks: number;
  processed_chunks: number;
  total_findings: number;
  current_chunk: string | null;
  complete: boolean;
}

interface WritingProgress {
  total_sections: number;
  written_sections: number;
  current_section: string | null;
  total_words: number;
  complete: boolean;
}

type PaperViewMode = "list" | "create" | "detail" | "sources" | "outline" | "write" | "review" | "export";

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
}

const PAPER_TYPES = [
  { value: "research_paper", label: "Research Paper" },
  { value: "literature_review", label: "Literature Review" },
  { value: "argumentative", label: "Argumentative Essay" },
  { value: "expository", label: "Expository Essay" },
  { value: "case_study", label: "Case Study" },
  { value: "technical_report", label: "Technical Report" },
  { value: "thesis", label: "Thesis/Dissertation" },
];

const CITATION_STYLES = [
  { value: "apa", label: "APA" },
  { value: "mla", label: "MLA" },
  { value: "chicago", label: "Chicago" },
  { value: "harvard", label: "Harvard" },
  { value: "ieee", label: "IEEE" },
  { value: "bibtex", label: "BibTeX" },
];

// ============================================================================
// Paper Generator Component
// ============================================================================

interface PaperGeneratorProps {
  onClose?: () => void;
}

const PaperGenerator: Component<PaperGeneratorProps> = (props) => {
  // State
  const [viewMode, setViewMode] = createSignal<PaperViewMode>("list");
  const [papers, setPapers] = createSignal<PaperView[]>([]);
  const [currentPaper, setCurrentPaper] = createSignal<Paper | null>(null);
  const [availableSources, setAvailableSources] = createSignal<SourceView[]>([]);
  const [selectedSourceIds, setSelectedSourceIds] = createSignal<Set<string>>(new Set());
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [successMsg, setSuccessMsg] = createSignal<string | null>(null);
  const [currentProgress, setCurrentProgress] = createSignal<string | null>(null);

  // Create paper form
  const [titleInput, setTitleInput] = createSignal("");
  const [researchQuestionInput, setResearchQuestionInput] = createSignal("");
  const [paperTypeInput, setPaperTypeInput] = createSignal("research_paper");
  const [citationStyleInput, setCitationStyleInput] = createSignal("apa");
  const [thesisInput, setThesisInput] = createSignal("");

  // Section editing
  const [selectedSection, setSelectedSection] = createSignal<PaperSection | null>(null);
  const [editingContent, setEditingContent] = createSignal("");
  const [reviewResult, setReviewResult] = createSignal<ReviewResult | null>(null);

  // Export options
  const [exportFormat, setExportFormat] = createSignal("markdown");
  const [includeToc, setIncludeToc] = createSignal(true);
  const [includeBibliography, setIncludeBibliography] = createSignal(true);
  const [exportedContent, setExportedContent] = createSignal<string | null>(null);

  onMount(() => {
    loadPapers();
    loadAvailableSources();
  });

  // Load papers
  const loadPapers = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<PaperView[]>("paper_list", { limit: 50 });
      setPapers(result);
    } catch (e) {
      setError(`Failed to load papers: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Load available sources
  const loadAvailableSources = async () => {
    try {
      const sources = await invoke<SourceView[]>("research_list_sources", { limit: 100 });
      setAvailableSources(sources);
    } catch (e) {
      console.error("Failed to load sources:", e);
    }
  };

  // Create new paper
  const createPaper = async () => {
    if (!titleInput().trim() || !researchQuestionInput().trim()) {
      setError("Title and research question are required");
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const paper = await invoke<PaperView>("paper_create", {
        request: {
          title: titleInput(),
          research_question: researchQuestionInput(),
          paper_type: paperTypeInput(),
          citation_style: citationStyleInput(),
          thesis: thesisInput() || undefined,
        },
      });
      setSuccessMsg(`Created paper: ${paper.title}`);
      resetCreateForm();
      loadPapers();
      setViewMode("list");
    } catch (e) {
      setError(`Failed to create paper: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const resetCreateForm = () => {
    setTitleInput("");
    setResearchQuestionInput("");
    setPaperTypeInput("research_paper");
    setCitationStyleInput("apa");
    setThesisInput("");
  };

  // Load paper detail
  const loadPaper = async (paperId: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const paper = await invoke<Paper>("paper_get", { paperId });
      setCurrentPaper(paper);
      setViewMode("detail");
    } catch (e) {
      setError(`Failed to load paper: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Delete paper
  const deletePaper = async (paperId: string) => {
    if (!confirm("Delete this paper? This cannot be undone.")) return;

    setIsLoading(true);
    try {
      await invoke("paper_delete", { paperId });
      setSuccessMsg("Paper deleted");
      loadPapers();
      setCurrentPaper(null);
      setViewMode("list");
    } catch (e) {
      setError(`Failed to delete paper: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Add sources to paper
  const addSourcesToPaper = async () => {
    const paper = currentPaper();
    if (!paper) return;

    const sourceIds = Array.from(selectedSourceIds());
    if (sourceIds.length === 0) {
      setError("Select at least one source");
      return;
    }

    setIsLoading(true);
    setCurrentProgress("Adding sources...");
    try {
      const added = await invoke<number>("paper_add_sources", {
        paperId: paper.id,
        sourceIds,
      });
      setSuccessMsg(`Added ${added} sources`);
      setSelectedSourceIds(new Set<string>());
      await loadPaper(paper.id);
    } catch (e) {
      setError(`Failed to add sources: ${e}`);
    } finally {
      setIsLoading(false);
      setCurrentProgress(null);
    }
  };

  // Extract findings
  const extractFindings = async () => {
    const paper = currentPaper();
    if (!paper) return;

    setIsLoading(true);
    setCurrentProgress("Extracting key findings from sources...");
    try {
      const progress = await invoke<ExtractionProgress>("paper_extract_findings", {
        paperId: paper.id,
      });
      setSuccessMsg(`Extracted ${progress.total_findings} findings`);
      await loadPaper(paper.id);
    } catch (e) {
      setError(`Failed to extract findings: ${e}`);
    } finally {
      setIsLoading(false);
      setCurrentProgress(null);
    }
  };

  // Generate outline
  const generateOutline = async () => {
    const paper = currentPaper();
    if (!paper) return;

    setIsLoading(true);
    setCurrentProgress("Generating paper outline...");
    try {
      await invoke<SectionView[]>("paper_generate_outline", {
        paperId: paper.id,
        thesisHint: null,
      });
      setSuccessMsg("Outline generated");
      await loadPaper(paper.id);
    } catch (e) {
      setError(`Failed to generate outline: ${e}`);
    } finally {
      setIsLoading(false);
      setCurrentProgress(null);
    }
  };

  // Write a section
  const writeSection = async (sectionId: string) => {
    const paper = currentPaper();
    if (!paper) return;

    setIsLoading(true);
    setCurrentProgress(`Writing section...`);
    try {
      const section = await invoke<PaperSection>("paper_write_section", {
        paperId: paper.id,
        sectionId,
      });
      setSuccessMsg(`Wrote section: ${section.title} (${section.word_count} words)`);
      await loadPaper(paper.id);
    } catch (e) {
      setError(`Failed to write section: ${e}`);
    } finally {
      setIsLoading(false);
      setCurrentProgress(null);
    }
  };

  // Write all sections
  const writeAllSections = async () => {
    const paper = currentPaper();
    if (!paper) return;

    setIsLoading(true);
    setCurrentProgress("Writing all sections...");
    try {
      const progress = await invoke<WritingProgress>("paper_write_all", {
        paperId: paper.id,
      });
      setSuccessMsg(`Wrote ${progress.written_sections} sections (${progress.total_words} words)`);
      await loadPaper(paper.id);
    } catch (e) {
      setError(`Failed to write sections: ${e}`);
    } finally {
      setIsLoading(false);
      setCurrentProgress(null);
    }
  };

  // Review section
  const reviewSection = async (sectionId: string) => {
    const paper = currentPaper();
    if (!paper) return;

    setIsLoading(true);
    setCurrentProgress("Reviewing section...");
    try {
      const result = await invoke<ReviewResult>("paper_review_section", {
        paperId: paper.id,
        sectionId,
        focusAreas: null,
      });
      setReviewResult(result);
      await loadPaper(paper.id);
    } catch (e) {
      setError(`Failed to review section: ${e}`);
    } finally {
      setIsLoading(false);
      setCurrentProgress(null);
    }
  };

  // Update section content
  const updateSectionContent = async () => {
    const paper = currentPaper();
    const section = selectedSection();
    if (!paper || !section) return;

    setIsLoading(true);
    try {
      await invoke("paper_update_section", {
        paperId: paper.id,
        sectionId: section.id,
        content: editingContent(),
      });
      setSuccessMsg("Section updated");
      setSelectedSection(null);
      await loadPaper(paper.id);
    } catch (e) {
      setError(`Failed to update section: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Export paper
  const exportPaper = async () => {
    const paper = currentPaper();
    if (!paper) return;

    setIsLoading(true);
    try {
      const content = await invoke<string>("paper_export", {
        paperId: paper.id,
        request: {
          format: exportFormat(),
          include_title_page: true,
          include_toc: includeToc(),
          include_bibliography: includeBibliography(),
          number_sections: true,
        },
      });
      setExportedContent(content);
    } catch (e) {
      setError(`Failed to export paper: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  // Utility functions
  const toggleSourceSelection = (sourceId: string) => {
    const current = new Set(selectedSourceIds());
    if (current.has(sourceId)) {
      current.delete(sourceId);
    } else {
      current.add(sourceId);
    }
    setSelectedSourceIds(current);
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    setSuccessMsg("Copied to clipboard");
  };

  const downloadExport = () => {
    const content = exportedContent();
    if (!content) return;

    const paper = currentPaper();
    const ext = exportFormat() === "html" ? "html" : exportFormat() === "markdown" ? "md" : "txt";
    const filename = `${paper?.title || "paper"}.${ext}`;

    const blob = new Blob([content], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
  };

  const getStatusColor = (status: string) => {
    switch (status.toLowerCase()) {
      case "draft": return "#9e9e9e";
      case "analyzed": return "#2196f3";
      case "outlined": return "#ff9800";
      case "writing": return "#9c27b0";
      case "review": return "#ff5722";
      case "complete": return "#4caf50";
      default: return "#9e9e9e";
    }
  };

  const getSectionStatusIcon = (status: string) => {
    switch (status.toLowerCase()) {
      case "pending": return "○";
      case "generating": return "◐";
      case "written": return "●";
      case "reviewed": return "◉";
      case "final": return "✓";
      default: return "○";
    }
  };

  // Clear messages
  createEffect(() => {
    if (successMsg()) {
      setTimeout(() => setSuccessMsg(null), 3000);
    }
  });

  return (
    <div class="paper-generator">
      {/* Header */}
      <div class="paper-header">
        <div class="paper-title">
          <span class="paper-icon">📝</span>
          <span>Paper Generator</span>
        </div>
        <div class="paper-actions">
          <button
            class="icon-btn"
            onClick={() => setViewMode("create")}
            title="New Paper"
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

      {/* Navigation */}
      <Show when={currentPaper()}>
        <div class="paper-nav">
          <button class="nav-back" onClick={() => { setCurrentPaper(null); setViewMode("list"); }}>
            ← Back to Papers
          </button>
          <div class="nav-tabs">
            <button class={`nav-tab ${viewMode() === "detail" ? "active" : ""}`} onClick={() => setViewMode("detail")}>
              Overview
            </button>
            <button class={`nav-tab ${viewMode() === "sources" ? "active" : ""}`} onClick={() => setViewMode("sources")}>
              Sources
            </button>
            <button class={`nav-tab ${viewMode() === "outline" ? "active" : ""}`} onClick={() => setViewMode("outline")}>
              Outline
            </button>
            <button class={`nav-tab ${viewMode() === "write" ? "active" : ""}`} onClick={() => setViewMode("write")}>
              Write
            </button>
            <button class={`nav-tab ${viewMode() === "review" ? "active" : ""}`} onClick={() => setViewMode("review")}>
              Review
            </button>
            <button class={`nav-tab ${viewMode() === "export" ? "active" : ""}`} onClick={() => setViewMode("export")}>
              Export
            </button>
          </div>
        </div>
      </Show>

      {/* Messages */}
      <Show when={error()}>
        <div class="error-banner">
          <span>{error()}</span>
          <button onClick={() => setError(null)}>×</button>
        </div>
      </Show>

      <Show when={successMsg()}>
        <div class="success-banner">{successMsg()}</div>
      </Show>

      <Show when={currentProgress()}>
        <div class="progress-banner">
          <span class="progress-spinner">⟳</span>
          <span>{currentProgress()}</span>
        </div>
      </Show>

      {/* Main Content */}
      <div class="paper-content">
        {/* Paper List View */}
        <Show when={viewMode() === "list"}>
          <div class="paper-list-view">
            <Show when={isLoading()}>
              <div class="loading">Loading papers...</div>
            </Show>

            <Show when={!isLoading() && papers().length === 0}>
              <div class="empty-state">
                <p>No papers yet</p>
                <button class="btn-primary" onClick={() => setViewMode("create")}>
                  Create your first paper
                </button>
              </div>
            </Show>

            <div class="paper-list">
              <For each={papers()}>
                {(paper) => (
                  <div class="paper-card" onClick={() => loadPaper(paper.id)}>
                    <div class="paper-card-header">
                      <h3>{paper.title}</h3>
                      <span class="status-badge" style={{ background: getStatusColor(paper.status) }}>
                        {paper.status}
                      </span>
                    </div>
                    <p class="research-question">{paper.research_question}</p>
                    <div class="paper-stats">
                      <span>{paper.source_count} sources</span>
                      <span>{paper.section_count} sections</span>
                      <span>{paper.word_count} words</span>
                    </div>
                    <div class="paper-meta">
                      <span class="paper-type">{paper.paper_type}</span>
                      <span class="citation-style">{paper.citation_style}</span>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </div>
        </Show>

        {/* Create Paper View */}
        <Show when={viewMode() === "create"}>
          <div class="create-paper-view">
            <h3>Create New Paper</h3>

            <div class="form-group">
              <label>Title *</label>
              <input
                type="text"
                placeholder="Enter paper title"
                value={titleInput()}
                onInput={(e) => setTitleInput(e.currentTarget.value)}
              />
            </div>

            <div class="form-group">
              <label>Research Question *</label>
              <textarea
                placeholder="What question does your paper address?"
                value={researchQuestionInput()}
                onInput={(e) => setResearchQuestionInput(e.currentTarget.value)}
                rows={3}
              />
            </div>

            <div class="form-row">
              <div class="form-group">
                <label>Paper Type</label>
                <select
                  value={paperTypeInput()}
                  onChange={(e) => setPaperTypeInput(e.currentTarget.value)}
                >
                  <For each={PAPER_TYPES}>
                    {(type) => <option value={type.value}>{type.label}</option>}
                  </For>
                </select>
              </div>

              <div class="form-group">
                <label>Citation Style</label>
                <select
                  value={citationStyleInput()}
                  onChange={(e) => setCitationStyleInput(e.currentTarget.value)}
                >
                  <For each={CITATION_STYLES}>
                    {(style) => <option value={style.value}>{style.label}</option>}
                  </For>
                </select>
              </div>
            </div>

            <div class="form-group">
              <label>Thesis (optional)</label>
              <textarea
                placeholder="Main argument or thesis statement"
                value={thesisInput()}
                onInput={(e) => setThesisInput(e.currentTarget.value)}
                rows={2}
              />
            </div>

            <div class="form-actions">
              <button class="btn-secondary" onClick={() => { resetCreateForm(); setViewMode("list"); }}>
                Cancel
              </button>
              <button class="btn-primary" onClick={createPaper} disabled={isLoading()}>
                Create Paper
              </button>
            </div>
          </div>
        </Show>

        {/* Paper Detail View */}
        <Show when={viewMode() === "detail" && currentPaper()}>
          <div class="paper-detail-view">
            <div class="detail-header">
              <h2>{currentPaper()!.title}</h2>
              <button class="btn-danger" onClick={() => deletePaper(currentPaper()!.id)}>
                Delete
              </button>
            </div>

            <div class="detail-info">
              <div class="info-item">
                <span class="label">Research Question:</span>
                <span>{currentPaper()!.research_question}</span>
              </div>
              <Show when={currentPaper()!.thesis}>
                <div class="info-item">
                  <span class="label">Thesis:</span>
                  <span>{currentPaper()!.thesis}</span>
                </div>
              </Show>
              <div class="info-row">
                <div class="info-item">
                  <span class="label">Type:</span>
                  <span>{currentPaper()!.paper_type}</span>
                </div>
                <div class="info-item">
                  <span class="label">Citation Style:</span>
                  <span>{currentPaper()!.citation_style}</span>
                </div>
                <div class="info-item">
                  <span class="label">Status:</span>
                  <span class="status-badge" style={{ background: getStatusColor(currentPaper()!.status) }}>
                    {currentPaper()!.status}
                  </span>
                </div>
              </div>
            </div>

            <div class="workflow-steps">
              <h4>Workflow</h4>
              <div class="steps">
                <div class={`step ${currentPaper()!.source_ids.length > 0 ? "complete" : ""}`}>
                  <span class="step-number">1</span>
                  <span class="step-label">Add Sources</span>
                  <span class="step-count">{currentPaper()!.source_ids.length}</span>
                </div>
                <div class={`step ${currentPaper()!.findings.length > 0 ? "complete" : ""}`}>
                  <span class="step-number">2</span>
                  <span class="step-label">Extract Findings</span>
                  <span class="step-count">{currentPaper()!.findings.length}</span>
                </div>
                <div class={`step ${currentPaper()!.sections.length > 0 ? "complete" : ""}`}>
                  <span class="step-number">3</span>
                  <span class="step-label">Generate Outline</span>
                  <span class="step-count">{currentPaper()!.sections.length}</span>
                </div>
                <div class={`step ${currentPaper()!.sections.filter(s => s.status !== "pending").length > 0 ? "complete" : ""}`}>
                  <span class="step-number">4</span>
                  <span class="step-label">Write Sections</span>
                </div>
                <div class={`step ${currentPaper()!.status === "complete" ? "complete" : ""}`}>
                  <span class="step-number">5</span>
                  <span class="step-label">Review & Export</span>
                </div>
              </div>
            </div>

            <Show when={currentPaper()!.findings.length > 0}>
              <div class="findings-preview">
                <h4>Key Findings ({currentPaper()!.findings.length})</h4>
                <div class="findings-list">
                  <For each={currentPaper()!.findings.slice(0, 5)}>
                    {(finding) => (
                      <div class="finding-item">
                        <span class="finding-type">{finding.finding_type}</span>
                        <span class="finding-content">
                          {finding.content.length > 100 ? finding.content.slice(0, 100) + "..." : finding.content}
                        </span>
                      </div>
                    )}
                  </For>
                  <Show when={currentPaper()!.findings.length > 5}>
                    <span class="more-items">+{currentPaper()!.findings.length - 5} more</span>
                  </Show>
                </div>
              </div>
            </Show>
          </div>
        </Show>

        {/* Sources View */}
        <Show when={viewMode() === "sources" && currentPaper()}>
          <div class="sources-view">
            <h3>Sources ({currentPaper()!.source_ids.length})</h3>

            <div class="current-sources">
              <h4>Added to Paper</h4>
              <Show when={currentPaper()!.source_ids.length === 0}>
                <p class="empty-text">No sources added yet</p>
              </Show>
              <For each={currentPaper()!.source_ids}>
                {(sourceId) => {
                  const source = availableSources().find(s => s.id === sourceId);
                  return (
                    <div class="source-item">
                      <span>{source?.title || sourceId}</span>
                    </div>
                  );
                }}
              </For>
            </div>

            <div class="add-sources">
              <h4>Add Sources</h4>
              <div class="available-sources">
                <For each={availableSources().filter(s => !currentPaper()!.source_ids.includes(s.id))}>
                  {(source) => (
                    <div class="source-select-item">
                      <input
                        type="checkbox"
                        checked={selectedSourceIds().has(source.id)}
                        onChange={() => toggleSourceSelection(source.id)}
                      />
                      <span class="source-title">{source.title}</span>
                      <span class="source-type">{source.source_type}</span>
                    </div>
                  )}
                </For>
              </div>
              <button
                class="btn-primary"
                onClick={addSourcesToPaper}
                disabled={isLoading() || selectedSourceIds().size === 0}
              >
                Add Selected ({selectedSourceIds().size})
              </button>
            </div>

            <Show when={currentPaper()!.source_ids.length > 0}>
              <div class="extract-section">
                <h4>Extract Key Findings</h4>
                <p class="help-text">
                  Analyze sources to extract quotes, data, claims, and other citable material.
                </p>
                <button
                  class="btn-primary"
                  onClick={extractFindings}
                  disabled={isLoading()}
                >
                  {currentPaper()!.findings.length > 0 ? "Re-extract Findings" : "Extract Findings"}
                </button>
              </div>
            </Show>
          </div>
        </Show>

        {/* Outline View */}
        <Show when={viewMode() === "outline" && currentPaper()}>
          <div class="outline-view">
            <div class="outline-header">
              <h3>Paper Outline</h3>
              <button
                class="btn-primary"
                onClick={generateOutline}
                disabled={isLoading() || currentPaper()!.source_ids.length === 0}
              >
                {currentPaper()!.sections.length > 0 ? "Regenerate Outline" : "Generate Outline"}
              </button>
            </div>

            <Show when={currentPaper()!.thesis}>
              <div class="thesis-box">
                <span class="label">Thesis:</span>
                <span>{currentPaper()!.thesis}</span>
              </div>
            </Show>

            <div class="sections-list">
              <For each={currentPaper()!.sections.sort((a, b) => a.order - b.order)}>
                {(section) => (
                  <div class="section-outline-item">
                    <div class="section-order">{section.order + 1}</div>
                    <div class="section-info">
                      <span class="section-title">{section.title}</span>
                      <span class="section-type">{section.section_type}</span>
                    </div>
                    <div class="section-stats">
                      <span class="word-count">{section.word_count} words</span>
                      <Show when={section.target_word_count}>
                        <span class="target">/ {section.target_word_count}</span>
                      </Show>
                    </div>
                    <span class="section-status">{getSectionStatusIcon(section.status)}</span>
                  </div>
                )}
              </For>
            </div>

            <Show when={currentPaper()!.sections.length === 0}>
              <div class="empty-outline">
                <p>No outline yet. Add sources and generate an outline to get started.</p>
              </div>
            </Show>
          </div>
        </Show>

        {/* Write View */}
        <Show when={viewMode() === "write" && currentPaper()}>
          <div class="write-view">
            <div class="write-header">
              <h3>Write Sections</h3>
              <button
                class="btn-primary"
                onClick={writeAllSections}
                disabled={isLoading() || currentPaper()!.sections.filter(s => s.status === "pending").length === 0}
              >
                Write All Pending
              </button>
            </div>

            <div class="sections-write-list">
              <For each={currentPaper()!.sections.sort((a, b) => a.order - b.order)}>
                {(section) => (
                  <div class="section-write-card">
                    <div class="section-header">
                      <h4>{section.title}</h4>
                      <span class="status-badge" style={{ background: getStatusColor(section.status) }}>
                        {section.status}
                      </span>
                    </div>

                    <div class="section-meta">
                      <span>{section.word_count} words</span>
                      <Show when={section.target_word_count}>
                        <span>Target: {section.target_word_count}</span>
                      </Show>
                      <span>{section.citations.length} citations</span>
                    </div>

                    <Show when={section.content}>
                      <div class="section-preview">
                        {section.content.slice(0, 200)}
                        {section.content.length > 200 ? "..." : ""}
                      </div>
                    </Show>

                    <div class="section-actions">
                      <Show when={section.status === "pending"}>
                        <button
                          class="btn-primary"
                          onClick={() => writeSection(section.id)}
                          disabled={isLoading()}
                        >
                          Write
                        </button>
                      </Show>
                      <Show when={section.content}>
                        <button
                          class="btn-secondary"
                          onClick={() => { setSelectedSection(section); setEditingContent(section.content); }}
                        >
                          Edit
                        </button>
                        <button
                          class="btn-secondary"
                          onClick={() => writeSection(section.id)}
                          disabled={isLoading()}
                        >
                          Rewrite
                        </button>
                      </Show>
                    </div>
                  </div>
                )}
              </For>
            </div>

            {/* Edit Modal */}
            <Show when={selectedSection()}>
              <div class="edit-modal-overlay" onClick={() => setSelectedSection(null)}>
                <div class="edit-modal" onClick={(e) => e.stopPropagation()}>
                  <h3>Edit: {selectedSection()!.title}</h3>
                  <textarea
                    value={editingContent()}
                    onInput={(e) => setEditingContent(e.currentTarget.value)}
                    rows={20}
                  />
                  <div class="modal-actions">
                    <button class="btn-secondary" onClick={() => setSelectedSection(null)}>
                      Cancel
                    </button>
                    <button class="btn-primary" onClick={updateSectionContent} disabled={isLoading()}>
                      Save Changes
                    </button>
                  </div>
                </div>
              </div>
            </Show>
          </div>
        </Show>

        {/* Review View */}
        <Show when={viewMode() === "review" && currentPaper()}>
          <div class="review-view">
            <h3>Review Sections</h3>

            <div class="sections-review-list">
              <For each={currentPaper()!.sections.filter(s => s.content)}>
                {(section) => (
                  <div class="section-review-card">
                    <div class="section-header">
                      <h4>{section.title}</h4>
                      <button
                        class="btn-secondary"
                        onClick={() => reviewSection(section.id)}
                        disabled={isLoading()}
                      >
                        Review
                      </button>
                    </div>
                    <Show when={section.review_notes.length > 0}>
                      <div class="review-notes">
                        <For each={section.review_notes}>
                          {(note) => (
                            <div class={`review-note ${note.resolved ? "resolved" : ""}`}>
                              <span class="note-type">{note.issue_type}</span>
                              <span class="note-desc">{note.description}</span>
                            </div>
                          )}
                        </For>
                      </div>
                    </Show>
                  </div>
                )}
              </For>
            </div>

            <Show when={reviewResult()}>
              <div class="review-result">
                <h4>Review Results</h4>
                <div class="quality-score">
                  <span>Quality Score:</span>
                  <span class="score">{Math.round(reviewResult()!.quality_score * 100)}%</span>
                  <span class={`pass-fail ${reviewResult()!.passes ? "pass" : "fail"}`}>
                    {reviewResult()!.passes ? "PASSES" : "NEEDS WORK"}
                  </span>
                </div>

                <Show when={reviewResult()!.issues.length > 0}>
                  <div class="issues-list">
                    <h5>Issues Found</h5>
                    <For each={reviewResult()!.issues}>
                      {(issue) => (
                        <div class={`issue-item severity-${issue.severity}`}>
                          <span class="issue-type">{issue.issue_type}</span>
                          <span class="issue-desc">{issue.description}</span>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>

                <Show when={reviewResult()!.suggestions.length > 0}>
                  <div class="suggestions-list">
                    <h5>Suggestions</h5>
                    <For each={reviewResult()!.suggestions}>
                      {(suggestion) => <p class="suggestion">{suggestion}</p>}
                    </For>
                  </div>
                </Show>
              </div>
            </Show>
          </div>
        </Show>

        {/* Export View */}
        <Show when={viewMode() === "export" && currentPaper()}>
          <div class="export-view">
            <h3>Export Paper</h3>

            <div class="export-options">
              <div class="form-group">
                <label>Format</label>
                <select value={exportFormat()} onChange={(e) => setExportFormat(e.currentTarget.value)}>
                  <option value="markdown">Markdown</option>
                  <option value="html">HTML</option>
                  <option value="text">Plain Text</option>
                </select>
              </div>

              <div class="form-row">
                <label class="checkbox-label">
                  <input
                    type="checkbox"
                    checked={includeToc()}
                    onChange={(e) => setIncludeToc(e.currentTarget.checked)}
                  />
                  Include Table of Contents
                </label>

                <label class="checkbox-label">
                  <input
                    type="checkbox"
                    checked={includeBibliography()}
                    onChange={(e) => setIncludeBibliography(e.currentTarget.checked)}
                  />
                  Include Bibliography
                </label>
              </div>

              <button class="btn-primary" onClick={exportPaper} disabled={isLoading()}>
                Generate Export
              </button>
            </div>

            <Show when={exportedContent()}>
              <div class="export-result">
                <div class="export-header">
                  <h4>Exported Content</h4>
                  <div class="export-actions">
                    <button class="btn-secondary" onClick={() => copyToClipboard(exportedContent()!)}>
                      Copy
                    </button>
                    <button class="btn-primary" onClick={downloadExport}>
                      Download
                    </button>
                  </div>
                </div>
                <pre class="export-preview">{exportedContent()}</pre>
              </div>
            </Show>
          </div>
        </Show>
      </div>
    </div>
  );
};

export default PaperGenerator;
