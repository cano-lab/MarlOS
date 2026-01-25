import { Component, createSignal, createEffect, onMount, Show, For } from "solid-js";
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
}

interface SearchResult {
  chapter_index: number;
  chapter_title: string | null;
  position: number;
  context: string;
}

interface EpubViewerProps {
  path: string;
  onClose?: () => void;
}

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

  let contentRef: HTMLDivElement | undefined;

  onMount(async () => {
    await loadEpub();
  });

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

      // Load first chapter
      await loadChapter(0);
    } catch (e) {
      setError(`Failed to load EPUB: ${e}`);
    } finally {
      setLoading(false);
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
    // TODO: Scroll to position within chapter
  };

  const prevChapter = () => loadChapter(currentChapter() - 1);
  const nextChapter = () => loadChapter(currentChapter() + 1);

  const increaseFontSize = () => setFontSize(s => Math.min(s + 2, 32));
  const decreaseFontSize = () => setFontSize(s => Math.max(s - 2, 10));

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

  return (
    <div class="epub-viewer">
      <div class="epub-toolbar">
        <div class="toolbar-group">
          <button class="toolbar-btn" onClick={props.onClose} title="Close">
            ✕
          </button>
          <span class="toolbar-divider" />
          <button
            class={`toolbar-btn ${showToc() ? "active" : ""}`}
            onClick={() => setShowToc(!showToc())}
            title="Table of Contents"
          >
            ☰
          </button>
        </div>

        <div class="toolbar-group">
          <button class="toolbar-btn" onClick={prevChapter} disabled={currentChapter() === 0}>
            ◀
          </button>
          <span class="chapter-indicator">
            {currentChapter() + 1} / {epubInfo()?.chapter_count || 0}
          </span>
          <button
            class="toolbar-btn"
            onClick={nextChapter}
            disabled={currentChapter() >= (epubInfo()?.chapter_count || 1) - 1}
          >
            ▶
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
            {isSearching() ? "..." : "🔍"}
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
        <div class="epub-content" ref={contentRef}>
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
                innerHTML={chapterContent()?.html || ""}
              />
            </Show>
          </Show>
        </div>
      </div>

      <div class="epub-status">
        <Show when={epubInfo()}>
          <span class="book-info">
            {epubInfo()?.title || "Untitled"}
            {epubInfo()?.author && ` - ${epubInfo()?.author}`}
          </span>
        </Show>
        <span class="status-spacer" />
        <Show when={chapterContent()?.title}>
          <span class="chapter-info">{chapterContent()?.title}</span>
        </Show>
      </div>
    </div>
  );
};

export default EpubViewer;
