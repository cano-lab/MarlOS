import { Component, createSignal, createEffect, onMount, onCleanup, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./EpubViewer.css";

interface TocEntry {
  label: string;
  content_path: string;
  play_order: number;
  children: TocEntry[];
}

interface EpubInfo {
  path: string;
  title: string | null;
  author: string | null;
  language: string | null;
  description: string | null;
  publisher: string | null;
  chapter_count: number;
  toc: TocEntry[];
}

interface ChapterContent {
  index: number;
  title: string | null;
  html: string;
  text: string;
  css: string[];
}

interface SearchResult {
  chapter_index: number;
  chapter_title: string | null;
  position: number;
  context: string;
}

interface EpubHighlight {
  id: string;
  bookPath: string;
  chapterIndex: number;
  selectedText: string;
  startOffset: number;
  endOffset: number;
  color: string;
  note: string;
  createdAt: number;
  updatedAt: number;
}

interface SelectionInfo {
  text: string;
  startOffset: number;
  endOffset: number;
  rect: DOMRect;
}

interface EpubDebugInfo {
  spine_count: number;
  toc_count: number;
  resource_count: number;
  spine_items: string[];
  toc_items: [string, string][];
  resources: [string, string][];
  first_chapter_preview: string;
}

interface EpubViewerProps {
  path: string;
  onClose?: () => void;
}

const HIGHLIGHT_COLORS = [
  { name: "yellow", label: "Yellow" },
  { name: "green", label: "Green" },
  { name: "blue", label: "Blue" },
  { name: "pink", label: "Pink" },
];

const EpubViewer: Component<EpubViewerProps> = (props) => {
  const [epubInfo, setEpubInfo] = createSignal<EpubInfo | null>(null);
  const [currentChapter, setCurrentChapter] = createSignal(0);
  const [chapterContent, setChapterContent] = createSignal<ChapterContent | null>(null);
  const [coverImage, setCoverImage] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [showToc, setShowToc] = createSignal(true);
  const [fontSize, setFontSize] = createSignal(16);
  const [searchQuery, setSearchQuery] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<SearchResult[]>([]);
  const [isSearching, setIsSearching] = createSignal(false);

  // Notes and highlights state
  const [highlights, setHighlights] = createSignal<EpubHighlight[]>([]);
  const [showNotesPanel, setShowNotesPanel] = createSignal(false);
  const [selection, setSelection] = createSignal<SelectionInfo | null>(null);
  const [showSelectionPopup, setShowSelectionPopup] = createSignal(false);
  const [editingNoteId, setEditingNoteId] = createSignal<string | null>(null);
  const [editingNoteText, setEditingNoteText] = createSignal("");
  const [notesFilter, setNotesFilter] = createSignal<"all" | "chapter">("all");
  const [epubCss, setEpubCss] = createSignal<string[]>([]);
  const [showDebug, setShowDebug] = createSignal(false);
  const [debugInfo, setDebugInfo] = createSignal<EpubDebugInfo | null>(null);

  let contentRef: HTMLDivElement | undefined;
  let popupRef: HTMLDivElement | undefined;

  onMount(async () => {
    await loadEpub();
    document.addEventListener("mousedown", handleDocumentMouseDown);
  });

  onCleanup(() => {
    document.removeEventListener("mousedown", handleDocumentMouseDown);
  });

  const handleDocumentMouseDown = (e: MouseEvent) => {
    // Close popup if clicking outside
    if (popupRef && !popupRef.contains(e.target as Node)) {
      setShowSelectionPopup(false);
      setSelection(null);
    }
  };

  const loadEpub = async () => {
    try {
      setLoading(true);
      setError(null);

      const info = await invoke<EpubInfo>("epub_open", { path: props.path });
      setEpubInfo(info);

      // Try to load cover
      try {
        const cover = await invoke<string | null>("epub_get_cover");
        if (cover) {
          setCoverImage(cover);
        }
      } catch (e) {
        console.log("No cover image available");
      }

      // Load highlights for this book
      await loadHighlights();

      // Load first chapter
      await loadChapter(0);
    } catch (e) {
      setError(`Failed to load EPUB: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const loadHighlights = async () => {
    try {
      const bookHighlights = await invoke<EpubHighlight[]>("epub_notes_get_for_book", {
        bookPath: props.path,
      });
      setHighlights(bookHighlights);
    } catch (e) {
      console.error("Failed to load highlights:", e);
    }
  };

  const loadChapter = async (index: number) => {
    const info = epubInfo();
    if (!info || index < 0 || index >= info.chapter_count) return;

    try {
      setLoading(true);
      const content = await invoke<ChapterContent>("epub_get_chapter", { index });
      setChapterContent(content);
      setCurrentChapter(index);

      // Load CSS from EPUB if available
      if (content.css && content.css.length > 0) {
        setEpubCss(content.css);
      }

      // Scroll to top of content
      if (contentRef) {
        contentRef.scrollTop = 0;
      }
    } catch (e) {
      setError(`Failed to load chapter: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const loadChapterByPath = async (contentPath: string) => {
    try {
      setLoading(true);
      const content = await invoke<ChapterContent>("epub_get_chapter_by_path", { contentPath });
      setChapterContent(content);
      setCurrentChapter(content.index);

      if (contentRef) {
        contentRef.scrollTop = 0;
      }
    } catch (e) {
      setError(`Failed to load chapter: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const performSearch = async () => {
    const query = searchQuery().trim();
    if (!query) {
      setSearchResults([]);
      return;
    }

    try {
      setIsSearching(true);
      const results = await invoke<SearchResult[]>("epub_search", {
        query,
        maxResults: 50,
      });
      setSearchResults(results);
    } catch (e) {
      console.error("Search error:", e);
    } finally {
      setIsSearching(false);
    }
  };

  const goToSearchResult = async (result: SearchResult) => {
    await loadChapter(result.chapter_index);
  };

  const prevChapter = () => loadChapter(currentChapter() - 1);
  const nextChapter = () => loadChapter(currentChapter() + 1);

  const fetchDebugInfo = async () => {
    try {
      const info = await invoke<EpubDebugInfo>("epub_debug_info");
      setDebugInfo(info);
      setShowDebug(true);
      console.log("EPUB Debug Info:", info);
    } catch (e) {
      console.error("Failed to get debug info:", e);
    }
  };

  const increaseFontSize = () => setFontSize(s => Math.min(s + 2, 32));
  const decreaseFontSize = () => setFontSize(s => Math.max(s - 2, 10));

  // Text selection handling
  const handleTextSelection = () => {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || !sel.rangeCount) {
      return;
    }

    const range = sel.getRangeAt(0);
    const selectedText = sel.toString().trim();

    if (!selectedText || selectedText.length < 2) {
      return;
    }

    // Check if selection is within chapter content
    const chapterEl = contentRef?.querySelector(".chapter-content");
    if (!chapterEl || !chapterEl.contains(range.commonAncestorContainer)) {
      return;
    }

    // Calculate offsets based on text content
    const content = chapterContent();
    if (!content) return;

    // Use the text content for offset calculation
    const textContent = content.text;
    const startOffset = textContent.indexOf(selectedText);
    const endOffset = startOffset + selectedText.length;

    if (startOffset === -1) {
      // If we can't find the exact text, use a simple position
      console.log("Could not find exact text position");
      return;
    }

    const rect = range.getBoundingClientRect();

    setSelection({
      text: selectedText,
      startOffset,
      endOffset,
      rect,
    });
    setShowSelectionPopup(true);
  };

  const createHighlight = async (color: string) => {
    const sel = selection();
    if (!sel) return;

    const highlight: EpubHighlight = {
      id: "",
      bookPath: props.path,
      chapterIndex: currentChapter(),
      selectedText: sel.text,
      startOffset: sel.startOffset,
      endOffset: sel.endOffset,
      color,
      note: "",
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };

    try {
      const saved = await invoke<EpubHighlight>("epub_notes_save", { highlight });
      setHighlights([...highlights(), saved]);
      setShowSelectionPopup(false);
      setSelection(null);
      window.getSelection()?.removeAllRanges();
    } catch (e) {
      console.error("Failed to create highlight:", e);
    }
  };

  const createHighlightWithNote = async () => {
    const sel = selection();
    if (!sel) return;

    // Create the highlight first with yellow color
    const highlight: EpubHighlight = {
      id: "",
      bookPath: props.path,
      chapterIndex: currentChapter(),
      selectedText: sel.text,
      startOffset: sel.startOffset,
      endOffset: sel.endOffset,
      color: "yellow",
      note: "",
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };

    try {
      const saved = await invoke<EpubHighlight>("epub_notes_save", { highlight });
      setHighlights([...highlights(), saved]);
      setShowSelectionPopup(false);
      setSelection(null);
      window.getSelection()?.removeAllRanges();

      // Open notes panel and start editing
      setShowNotesPanel(true);
      setEditingNoteId(saved.id);
      setEditingNoteText("");
    } catch (e) {
      console.error("Failed to create highlight:", e);
    }
  };

  const deleteHighlight = async (id: string) => {
    try {
      await invoke<boolean>("epub_notes_delete", { id });
      setHighlights(highlights().filter(h => h.id !== id));
      if (editingNoteId() === id) {
        setEditingNoteId(null);
      }
    } catch (e) {
      console.error("Failed to delete highlight:", e);
    }
  };

  const saveNote = async (id: string) => {
    try {
      await invoke<EpubHighlight>("epub_notes_update_note", {
        id,
        note: editingNoteText(),
      });
      setHighlights(highlights().map(h =>
        h.id === id ? { ...h, note: editingNoteText(), updatedAt: Date.now() } : h
      ));
      setEditingNoteId(null);
    } catch (e) {
      console.error("Failed to save note:", e);
    }
  };

  const startEditingNote = (highlight: EpubHighlight) => {
    setEditingNoteId(highlight.id);
    setEditingNoteText(highlight.note);
  };

  const navigateToHighlight = async (highlight: EpubHighlight) => {
    if (highlight.chapterIndex !== currentChapter()) {
      await loadChapter(highlight.chapterIndex);
    }
    // Scroll to the highlight position (approximate based on text)
    setTimeout(() => {
      const marks = contentRef?.querySelectorAll(`[data-highlight-id="${highlight.id}"]`);
      if (marks && marks.length > 0) {
        marks[0].scrollIntoView({ behavior: "smooth", block: "center" });
      }
    }, 100);
  };

  // Get highlights for current chapter
  const chapterHighlights = () =>
    highlights().filter(h => h.chapterIndex === currentChapter());

  // Get filtered highlights for notes panel
  const filteredHighlights = () => {
    if (notesFilter() === "chapter") {
      return chapterHighlights();
    }
    return highlights();
  };

  // Group highlights by chapter for display
  const highlightsByChapter = () => {
    const grouped: Record<number, EpubHighlight[]> = {};
    for (const h of filteredHighlights()) {
      if (!grouped[h.chapterIndex]) {
        grouped[h.chapterIndex] = [];
      }
      grouped[h.chapterIndex].push(h);
    }
    return grouped;
  };

  // Process chapter HTML to inject highlight markers
  const processChapterHtml = (html: string) => {
    const content = chapterContent();
    if (!content) return html;

    const chapterHL = chapterHighlights();
    if (chapterHL.length === 0) return html;

    // Sort highlights by start offset (descending to avoid offset shift issues)
    const sorted = [...chapterHL].sort((a, b) => b.startOffset - a.startOffset);

    // We need to inject marks based on the text content
    // This is a simplified approach that wraps matched text
    let processedHtml = html;

    for (const hl of sorted) {
      // Create a regex to find and wrap the highlighted text
      const escapedText = hl.selectedText.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      const regex = new RegExp(`(${escapedText})`, "g");

      let found = false;
      processedHtml = processedHtml.replace(regex, (match) => {
        if (!found) {
          found = true;
          return `<mark class="epub-highlight epub-highlight-${hl.color}" data-highlight-id="${hl.id}" title="${hl.note || "Click to edit"}">${match}</mark>`;
        }
        return match;
      });
    }

    return processedHtml;
  };

  // Handle keyboard navigation
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "ArrowLeft" || e.key === "PageUp") {
      prevChapter();
    } else if (e.key === "ArrowRight" || e.key === "PageDown") {
      nextChapter();
    }
  };

  createEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  });

  // Handle clicks on highlights
  const handleContentClick = (e: MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.classList.contains("epub-highlight")) {
      const highlightId = target.getAttribute("data-highlight-id");
      if (highlightId) {
        const hl = highlights().find(h => h.id === highlightId);
        if (hl) {
          setShowNotesPanel(true);
          startEditingNote(hl);
        }
      }
    }
  };

  // Combine EPUB CSS into a scoped style block
  const scopedCss = () => {
    const cssContent = epubCss();
    if (!cssContent || cssContent.length === 0) return "";
    // Prefix all CSS rules to scope them to .chapter-content
    return cssContent.map(css => {
      // Simple scoping: wrap in .chapter-content context
      return css.replace(/([^{}]+)\{/g, '.chapter-content $1{');
    }).join("\n");
  };

  return (
    <div class="epub-viewer">
      {/* Inject EPUB stylesheets */}
      <Show when={epubCss().length > 0}>
        <style>{scopedCss()}</style>
      </Show>
      <div class="epub-toolbar">
        <div class="toolbar-group">
          <button class="toolbar-btn" onClick={props.onClose} title="Close">
            X
          </button>
          <span class="toolbar-divider" />
          <button
            class={`toolbar-btn ${showToc() ? "active" : ""}`}
            onClick={() => setShowToc(!showToc())}
            title="Table of Contents"
          >
            &#9776;
          </button>
          <button
            class={`toolbar-btn ${showNotesPanel() ? "active" : ""}`}
            onClick={() => setShowNotesPanel(!showNotesPanel())}
            title="Notes & Highlights"
          >
            &#128221;
          </button>
          <button
            class="toolbar-btn"
            onClick={fetchDebugInfo}
            title="Debug Info"
          >
            &#128295;
          </button>
        </div>

        <div class="toolbar-group">
          <button class="toolbar-btn" onClick={prevChapter} disabled={currentChapter() === 0}>
            &#9664;
          </button>
          <span class="chapter-indicator">
            {currentChapter() + 1} / {epubInfo()?.chapter_count || 0}
          </span>
          <button
            class="toolbar-btn"
            onClick={nextChapter}
            disabled={currentChapter() >= (epubInfo()?.chapter_count || 1) - 1}
          >
            &#9654;
          </button>
        </div>

        <div class="toolbar-group">
          <span class="toolbar-label">Text:</span>
          <button class="toolbar-btn" onClick={decreaseFontSize} title="Decrease Font">
            A-
          </button>
          <span class="font-size-indicator">{fontSize()}px</span>
          <button class="toolbar-btn" onClick={increaseFontSize} title="Increase Font">
            A+
          </button>
        </div>

        <div class="toolbar-group search-group">
          <input
            type="text"
            class="search-input"
            placeholder="Search..."
            value={searchQuery()}
            onInput={(e) => setSearchQuery(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && performSearch()}
          />
          <button class="toolbar-btn" onClick={performSearch} disabled={isSearching()}>
            {isSearching() ? "..." : "\uD83D\uDD0D"}
          </button>
        </div>
      </div>

      <div class="epub-content-area">
        {/* Table of Contents Sidebar */}
        <Show when={showToc()}>
          <div class="epub-toc">
            <div class="toc-header">
              <h3>Contents</h3>
            </div>
            <div class="toc-list">
              <For each={epubInfo()?.toc || []}>
                {(entry, i) => (
                  <button
                    class={`toc-item ${currentChapter() === i() ? "active" : ""}`}
                    onClick={() => loadChapterByPath(entry.content_path)}
                    title={entry.label}
                  >
                    <span class="toc-number">{i() + 1}</span>
                    <span class="toc-label">{entry.label}</span>
                  </button>
                )}
              </For>
            </div>

            {/* Search Results */}
            <Show when={searchResults().length > 0}>
              <div class="search-results">
                <h3>Search Results ({searchResults().length})</h3>
                <div class="search-list">
                  <For each={searchResults()}>
                    {(result) => (
                      <button class="search-result-item" onClick={() => goToSearchResult(result)}>
                        <span class="result-chapter">
                          Ch. {result.chapter_index + 1}
                          {result.chapter_title && `: ${result.chapter_title}`}
                        </span>
                        <span class="result-context">...{result.context}...</span>
                      </button>
                    )}
                  </For>
                </div>
              </div>
            </Show>
          </div>
        </Show>

        {/* Main Content */}
        <div
          class="epub-content"
          ref={contentRef}
          onMouseUp={handleTextSelection}
          onClick={handleContentClick}
        >
          <Show when={loading()}>
            <div class="epub-loading">Loading...</div>
          </Show>

          <Show when={error()}>
            <div class="epub-error">{error()}</div>
          </Show>

          <Show when={!loading() && !error()}>
            {/* Book Info Header */}
            <Show when={currentChapter() === 0 && epubInfo()}>
              <div class="book-header">
                <Show when={coverImage()}>
                  <img
                    src={`data:image/jpeg;base64,${coverImage()}`}
                    alt="Cover"
                    class="book-cover"
                  />
                </Show>
                <div class="book-meta">
                  <h1 class="book-title">{epubInfo()?.title || "Untitled"}</h1>
                  <Show when={epubInfo()?.author}>
                    <p class="book-author">by {epubInfo()?.author}</p>
                  </Show>
                  <Show when={epubInfo()?.publisher}>
                    <p class="book-publisher">{epubInfo()?.publisher}</p>
                  </Show>
                  <Show when={epubInfo()?.description}>
                    <p class="book-description">{epubInfo()?.description}</p>
                  </Show>
                </div>
              </div>
            </Show>

            {/* Chapter Content */}
            <Show when={chapterContent()}>
              <div class="chapter-header">
                <Show when={chapterContent()?.title}>
                  <h2 class="chapter-title">{chapterContent()?.title}</h2>
                </Show>
              </div>
              <div
                class="chapter-content"
                style={{ "font-size": `${fontSize()}px` }}
                innerHTML={processChapterHtml(chapterContent()?.html || "")}
              />
            </Show>
          </Show>

          {/* Selection Popup */}
          <Show when={showSelectionPopup() && selection()}>
            <div
              ref={popupRef}
              class="selection-popup"
              style={{
                top: `${(selection()?.rect.bottom || 0) + window.scrollY + 8}px`,
                left: `${(selection()?.rect.left || 0) + (selection()?.rect.width || 0) / 2}px`,
              }}
            >
              <div class="popup-colors">
                <For each={HIGHLIGHT_COLORS}>
                  {(color) => (
                    <button
                      class={`popup-color-btn popup-color-${color.name}`}
                      onClick={() => createHighlight(color.name)}
                      title={color.label}
                    />
                  )}
                </For>
              </div>
              <button class="popup-note-btn" onClick={createHighlightWithNote}>
                + Note
              </button>
            </div>
          </Show>
        </div>

        {/* Notes Panel */}
        <Show when={showNotesPanel()}>
          <div class="epub-notes-panel">
            <div class="notes-header">
              <h3>Notes & Highlights</h3>
              <div class="notes-filter">
                <button
                  class={`filter-btn ${notesFilter() === "all" ? "active" : ""}`}
                  onClick={() => setNotesFilter("all")}
                >
                  All
                </button>
                <button
                  class={`filter-btn ${notesFilter() === "chapter" ? "active" : ""}`}
                  onClick={() => setNotesFilter("chapter")}
                >
                  Chapter
                </button>
              </div>
            </div>

            <div class="notes-list">
              <Show when={filteredHighlights().length === 0}>
                <div class="notes-empty">
                  <p>No highlights yet.</p>
                  <p class="notes-hint">Select text in the chapter to create highlights.</p>
                </div>
              </Show>

              <For each={Object.entries(highlightsByChapter())}>
                {([chapterIdx, chapterHighlights]) => (
                  <div class="notes-chapter-group">
                    <div class="notes-chapter-header">
                      Chapter {parseInt(chapterIdx) + 1}
                      {epubInfo()?.toc[parseInt(chapterIdx)]?.label &&
                        `: ${epubInfo()?.toc[parseInt(chapterIdx)]?.label}`}
                    </div>
                    <For each={chapterHighlights}>
                      {(highlight) => (
                        <div class={`note-item note-item-${highlight.color}`}>
                          <div class="note-text" onClick={() => navigateToHighlight(highlight)}>
                            "{highlight.selectedText.substring(0, 100)}
                            {highlight.selectedText.length > 100 ? "..." : ""}"
                          </div>

                          <Show when={editingNoteId() === highlight.id}>
                            <div class="note-editor">
                              <textarea
                                class="note-textarea"
                                value={editingNoteText()}
                                onInput={(e) => setEditingNoteText(e.currentTarget.value)}
                                placeholder="Add a note..."
                                rows={3}
                              />
                              <div class="note-editor-actions">
                                <button
                                  class="note-save-btn"
                                  onClick={() => saveNote(highlight.id)}
                                >
                                  Save
                                </button>
                                <button
                                  class="note-cancel-btn"
                                  onClick={() => setEditingNoteId(null)}
                                >
                                  Cancel
                                </button>
                              </div>
                            </div>
                          </Show>

                          <Show when={editingNoteId() !== highlight.id && highlight.note}>
                            <div class="note-content">{highlight.note}</div>
                          </Show>

                          <div class="note-actions">
                            <Show when={editingNoteId() !== highlight.id}>
                              <button
                                class="note-action-btn"
                                onClick={() => startEditingNote(highlight)}
                                title="Edit note"
                              >
                                &#9998;
                              </button>
                            </Show>
                            <button
                              class="note-action-btn note-delete-btn"
                              onClick={() => deleteHighlight(highlight.id)}
                              title="Delete highlight"
                            >
                              &#128465;
                            </button>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                )}
              </For>
            </div>
          </div>
        </Show>
      </div>

      {/* Debug Panel */}
      <Show when={showDebug() && debugInfo()}>
        <div class="epub-debug-panel">
          <div class="debug-header">
            <h3>EPUB Debug Info</h3>
            <button onClick={() => setShowDebug(false)}>Close</button>
          </div>
          <div class="debug-content">
            <p><strong>Spine count:</strong> {debugInfo()?.spine_count}</p>
            <p><strong>TOC count:</strong> {debugInfo()?.toc_count}</p>
            <p><strong>Resource count:</strong> {debugInfo()?.resource_count}</p>

            <h4>Spine Items (first 20):</h4>
            <ul>
              <For each={debugInfo()?.spine_items || []}>
                {(item, i) => <li>{i()}: {item}</li>}
              </For>
            </ul>

            <h4>TOC Items (first 20):</h4>
            <ul>
              <For each={debugInfo()?.toc_items || []}>
                {(item, i) => <li>{i()}: {item[0]} -&gt; {item[1]}</li>}
              </For>
            </ul>

            <h4>Resources (first 30):</h4>
            <ul>
              <For each={debugInfo()?.resources || []}>
                {(item) => <li>{item[0]} ({item[1]})</li>}
              </For>
            </ul>

            <h4>First Chapter Preview:</h4>
            <pre>{debugInfo()?.first_chapter_preview}</pre>
          </div>
        </div>
      </Show>

      <div class="epub-status">
        <Show when={epubInfo()}>
          <span class="book-info">
            {epubInfo()?.title || "Untitled"}
            {epubInfo()?.author && ` - ${epubInfo()?.author}`}
          </span>
        </Show>
        <span class="status-spacer" />
        <Show when={highlights().length > 0}>
          <span class="highlight-count">{highlights().length} highlight{highlights().length !== 1 ? "s" : ""}</span>
        </Show>
        <Show when={chapterContent()?.title}>
          <span class="chapter-info">{chapterContent()?.title}</span>
        </Show>
      </div>
    </div>
  );
};

export default EpubViewer;
