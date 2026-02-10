import { createSignal, createEffect, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import MarkdownEditor from "./components/MarkdownEditor";
import Preview from "./components/Preview";
import PdfViewer from "./components/PdfViewer";
import EpubViewer from "./components/EpubViewer";
import PrintPreview from "./components/PrintPreview";
import ChatPanel from "./components/ChatPanel";
import ResearchHub from "./components/ResearchHub";
import Sessions from "./components/Sessions";
import VectorQuery from "./components/VectorQuery";
import Sidebar from "./components/Sidebar";
import Titlebar from "./components/Titlebar";
import HomePage, { RecentFile } from "./components/HomePage";
import LearningPanel from "./components/LearningPanel";
import ProviderSettings from "./components/ProviderSettings";
import { ToastProvider, useToast } from "./components/Toast";
import { useDocumentTimer } from "./hooks/useDocumentTimer";
import "./App.css";

interface VersionInfo {
  version: string;
  name: string;
  build: string;
}

interface Document {
  id: string;
  path: string | null;
  content: string;
  title: string;
  word_count: number;
  is_dirty: boolean;
}

type ViewMode = "editor" | "preview" | "split";
type AppMode = "home" | "markdown" | "pdf" | "epub" | "learn" | "unstuck";

function AppContent() {
  const { showToast } = useToast();

  const [version, setVersion] = createSignal<VersionInfo | null>(null);
  const [document, setDocument] = createSignal<Document | null>(null);
  const [sidebarOpen, setSidebarOpen] = createSignal(true);
  const [viewMode, setViewMode] = createSignal<ViewMode>("split");
  const [appMode, setAppMode] = createSignal<AppMode>("home");
  const [pdfPath, setPdfPath] = createSignal<string | null>(null);
  const [epubPath, setEpubPath] = createSignal<string | null>(null);
  const [showPrintPreview, setShowPrintPreview] = createSignal(false);
  const [showChat, setShowChat] = createSignal(false);  // Side panel chat (toggle with Ctrl+/)
  const [showResearchHub, setShowResearchHub] = createSignal(false);
  const [showSessions, setShowSessions] = createSignal(false);
  const [showVectorQuery, setShowVectorQuery] = createSignal(false);
  const [selectedText, setSelectedText] = createSignal<string>("");
  const [recentFiles, setRecentFiles] = createSignal<RecentFile[]>([]);
  const [showProviderSettings, setShowProviderSettings] = createSignal(false);

  // Document timer for tracking time spent
  const documentTimer = useDocumentTimer();

  const loadRecentFiles = async () => {
    try {
      const files = await invoke<RecentFile[]>("get_recent_files", { limit: 10 });
      setRecentFiles(files);
    } catch (e) {
      console.debug("Failed to load recent files:", e);
    }
  };

  onMount(async () => {
    try {
      const info = await invoke<VersionInfo>("get_version");
      setVersion(info);
    } catch (e) {
      console.error("Failed to get version:", e);
    }

    // Load recent files on mount
    await loadRecentFiles();
  });

  const goHome = async () => {
    // Stop document timer and record duration
    await documentTimer.stopTimer();

    setAppMode("home");
    setDocument(null);
    setPdfPath(null);
    setEpubPath(null);
    loadRecentFiles(); // Refresh recent files when going home
  };

  const openLearn = () => {
    setAppMode("learn");
    setDocument(null);
    setPdfPath(null);
    setEpubPath(null);
  };

  const openUnstuck = () => {
    setAppMode("unstuck");
    setDocument(null);
    setPdfPath(null);
    setEpubPath(null);
  };

  const createNewDocument = async () => {
    setAppMode("markdown");
    setPdfPath(null);
    setEpubPath(null);
    try {
      const doc = await invoke<Document>("create_document");
      setDocument(doc);
    } catch (e) {
      console.error("Failed to create document:", e);
      showToast("Failed to create document", "error");
    }
  };

  const openDocument = async (pathArg?: string) => {
    try {
      let path = pathArg;

      if (!path) {
        const selected = await open({
          multiple: false,
          filters: [
            { name: "Markdown", extensions: ["md", "markdown"] },
            { name: "Text", extensions: ["txt"] },
            { name: "PDF", extensions: ["pdf"] },
            { name: "EPUB", extensions: ["epub"] },
            { name: "All Files", extensions: ["*"] },
          ],
        });

        if (!selected || typeof selected !== "string") {
          return;
        }
        path = selected;
      }

      // Check if it's a PDF
      if (path.toLowerCase().endsWith(".pdf")) {
        openPdf(path);
        return;
      }

      // Check if it's an EPUB
      if (path.toLowerCase().endsWith(".epub")) {
        openEpub(path);
        return;
      }

      setAppMode("markdown");
      setPdfPath(null);
      setEpubPath(null);

      // Read file content directly using fs plugin
      const content = await readTextFile(path);

      // Extract title from content or filename
      const title = extractTitle(content) || path.split(/[/\\]/).pop()?.replace(/\.[^.]+$/, "") || "Untitled";

      const doc: Document = {
        id: crypto.randomUUID(),
        path,
        content,
        title,
        word_count: countWords(content),
        is_dirty: false,
      };

      setDocument(doc);

      // Start document timer
      documentTimer.startTimer(path);

      // Also register with kernel
      try {
        await invoke("open_document", { path });
      } catch (e) {
        console.log("Kernel registration skipped:", e);
      }

      // Refresh recent files after opening
      loadRecentFiles();
    } catch (e) {
      console.error("Failed to open document:", e);
      showToast(`Failed to open file: ${e}`, "error");
    }
  };

  const openPdf = async (pathArg?: string) => {
    try {
      let path = pathArg;

      if (!path) {
        const selected = await open({
          multiple: false,
          filters: [
            { name: "PDF", extensions: ["pdf"] },
          ],
        });

        if (!selected || typeof selected !== "string") {
          return;
        }
        path = selected;
      }

      setDocument(null);
      setEpubPath(null);
      setAppMode("pdf");
      setPdfPath(path);

      // Start document timer
      documentTimer.startTimer(path);

      // Record document viewing activity in session
      try {
        const filename = path.split(/[/\\]/).pop() || path;
        await invoke("record_session_activity", {
          activityType: "document_view",
          details: {
            title: filename,
            path: path,
            durationSecs: 0, // Will be updated when closed
          }
        });
      } catch (e) {
        console.debug("Failed to record activity:", e);
      }

      // Refresh recent files after opening
      loadRecentFiles();
    } catch (e) {
      console.error("Failed to open PDF:", e);
      showToast(`Failed to open PDF: ${e}`, "error");
    }
  };

  const closePdf = () => {
    goHome();
  };

  const openEpub = async (pathArg?: string) => {
    try {
      let path = pathArg;

      if (!path) {
        const selected = await open({
          multiple: false,
          filters: [
            { name: "EPUB", extensions: ["epub"] },
          ],
        });

        if (!selected || typeof selected !== "string") {
          return;
        }
        path = selected;
      }

      setDocument(null);
      setPdfPath(null);
      setAppMode("epub");
      setEpubPath(path);

      // Start document timer
      documentTimer.startTimer(path);

      // Record document viewing activity in session
      try {
        const filename = path.split(/[/\\]/).pop() || path;
        await invoke("record_session_activity", {
          activityType: "document_view",
          details: {
            title: filename,
            path: path,
            durationSecs: 0,
          }
        });
      } catch (e) {
        console.debug("Failed to record activity:", e);
      }

      // Refresh recent files after opening
      loadRecentFiles();
    } catch (e) {
      console.error("Failed to open EPUB:", e);
      showToast(`Failed to open EPUB: ${e}`, "error");
    }
  };

  const closeEpub = () => {
    goHome();
  };

  const saveDocument = async () => {
    const doc = document();
    if (!doc) return;

    try {
      let path = doc.path;

      if (!path) {
        const selected = await save({
          filters: [
            { name: "Markdown", extensions: ["md"] },
            { name: "Text", extensions: ["txt"] },
          ],
          defaultPath: doc.title + ".md",
        });

        if (!selected) return;
        path = selected;
      }

      await writeTextFile(path, doc.content);

      setDocument({
        ...doc,
        path,
        is_dirty: false,
        title: extractTitle(doc.content) || path.split(/[/\\]/).pop()?.replace(/\.[^.]+$/, "") || doc.title,
      });

      showToast("Document saved", "success");

      // Record writing activity in session
      try {
        await invoke("record_session_activity", {
          activityType: "writing",
          details: {
            wordCount: doc.word_count,
            section: doc.title,
          }
        });
      } catch (e) {
        console.debug("Failed to record activity:", e);
      }

    } catch (e) {
      console.error("Failed to save document:", e);
      showToast(`Failed to save file: ${e}`, "error");
    }
  };

  const updateContent = (content: string) => {
    const doc = document();
    if (!doc) return;
    setDocument({
      ...doc,
      content,
      is_dirty: true,
      word_count: countWords(content),
      title: extractTitle(content) || doc.title,
    });
  };

  const extractTitle = (content: string): string | null => {
    const match = content.match(/^#\s+(.+)$/m);
    return match ? match[1].trim() : null;
  };

  const countWords = (content: string): number => {
    return content.trim().split(/\s+/).filter(w => w.length > 0).length;
  };

  const openPrintPreview = () => {
    // Only open print preview if we have content to print
    if (appMode() === "pdf" && pdfPath()) {
      setShowPrintPreview(true);
    } else if (appMode() === "markdown" && document()) {
      setShowPrintPreview(true);
    }
  };

  // Keyboard shortcuts
  const handleKeyDown = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      saveDocument();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "o") {
      e.preventDefault();
      openDocument();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "n") {
      e.preventDefault();
      createNewDocument();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "p") {
      e.preventDefault();
      openPrintPreview();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "\\") {
      e.preventDefault();
      setSidebarOpen(!sidebarOpen());
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "/") {
      e.preventDefault();
      setShowChat(!showChat());
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "r") {
      e.preventDefault();
      setShowResearchHub(!showResearchHub());
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "s" && e.shiftKey) {
      e.preventDefault();
      setShowSessions(!showSessions());
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "k") {
      e.preventDefault();
      setShowVectorQuery(!showVectorQuery());
    }
    // View mode shortcuts (only in markdown mode)
    if (appMode() === "markdown") {
      if ((e.ctrlKey || e.metaKey) && e.key === "1") {
        e.preventDefault();
        setViewMode("editor");
      }
      if ((e.ctrlKey || e.metaKey) && e.key === "2") {
        e.preventDefault();
        setViewMode("split");
      }
      if ((e.ctrlKey || e.metaKey) && e.key === "3") {
        e.preventDefault();
        setViewMode("preview");
      }
    }
    // Escape to close print preview, PDF, EPUB, Research Hub, Sessions, or Vector Query
    if (e.key === "Escape") {
      if (showPrintPreview()) {
        setShowPrintPreview(false);
      } else if (showResearchHub()) {
        setShowResearchHub(false);
      } else if (showSessions()) {
        setShowSessions(false);
      } else if (showVectorQuery()) {
        setShowVectorQuery(false);
      } else if (appMode() === "pdf") {
        closePdf();
      } else if (appMode() === "epub") {
        closeEpub();
      }
    }
  };

  onMount(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  });

  const getTitle = () => {
    if (appMode() === "home") {
      return "MarlOS";
    }
    if (appMode() === "pdf" && pdfPath()) {
      return pdfPath()!.split(/[/\\]/).pop() || "PDF";
    }
    if (appMode() === "epub" && epubPath()) {
      return epubPath()!.split(/[/\\]/).pop() || "EPUB";
    }
    return document()?.title || "MarlOS";
  };

  const handleSessionError = (error: string) => {
    showToast(`Session Error: ${error}`, "error");
  };

  return (
    <div class="app">
      <Titlebar
        title={getTitle()}
        version={version()?.version || ""}
        isDirty={document()?.is_dirty || false}
        onToggleSidebar={() => setSidebarOpen(!sidebarOpen())}
        viewMode={viewMode()}
        onViewModeChange={setViewMode}
        showViewMode={appMode() === "markdown" && document() !== null}
      />

      <div class="app-body">
        <Show when={sidebarOpen()}>
          <Sidebar
            onNewDocument={createNewDocument}
            onOpenDocument={() => openDocument()}
            onOpenPdf={() => openPdf()}
            onOpenEpub={() => openEpub()}
            onPrint={openPrintPreview}
            canPrint={!!(document() || pdfPath())}
            onToggleChat={() => setShowChat(!showChat())}
            chatOpen={showChat()}
            onToggleResearchHub={() => setShowResearchHub(!showResearchHub())}
            researchHubOpen={showResearchHub()}
            onToggleSessions={() => setShowSessions(!showSessions())}
            sessionsOpen={showSessions()}
            onToggleVectorQuery={() => setShowVectorQuery(!showVectorQuery())}
            vectorQueryOpen={showVectorQuery()}
            currentFile={document()?.path || pdfPath() || epubPath() || undefined}
            currentProject={document()?.path ? document()!.path!.split(/[/\\]/).slice(0, -1).join('/') : undefined}
            onGoHome={goHome}
            recentFiles={recentFiles()}
            onOpenRecent={openDocument}
            onOpenProviderSettings={() => setShowProviderSettings(true)}
          />
        </Show>

        <main class="main-content">
          {/* Home Mode */}
          <Show when={appMode() === "home"}>
            <HomePage
              onNewMarkdown={createNewDocument}
              onOpenFile={() => openDocument()}
              onOpenRecent={openDocument}
              onToggleSessions={() => setShowSessions(!showSessions())}
              onToggleResearchHub={() => setShowResearchHub(!showResearchHub())}
              onToggleVectorSearch={() => setShowVectorQuery(!showVectorQuery())}
              onOpenLearn={openLearn}
              onOpenUnstuck={openUnstuck}
            />
          </Show>

          {/* PDF Mode */}
          <Show when={appMode() === "pdf" && pdfPath()}>
            <PdfViewer path={pdfPath()!} onClose={closePdf} />
          </Show>

          {/* EPUB Mode */}
          <Show when={appMode() === "epub" && epubPath()}>
            <EpubViewer path={epubPath()!} onClose={closeEpub} />
          </Show>

          {/* Markdown Mode */}
          <Show when={appMode() === "markdown"}>
            <Show
              when={document()}
              fallback={
                <HomePage
                  onNewMarkdown={createNewDocument}
                  onOpenFile={() => openDocument()}
                  onOpenRecent={openDocument}
                  onToggleSessions={() => setShowSessions(!showSessions())}
                  onToggleResearchHub={() => setShowResearchHub(!showResearchHub())}
                  onToggleVectorSearch={() => setShowVectorQuery(!showVectorQuery())}
                  onOpenLearn={openLearn}
                  onOpenUnstuck={openUnstuck}
                />
              }
            >
              <div class={`editor-area view-${viewMode()}`}>
                <Show when={viewMode() !== "preview"}>
                  <div class="editor-pane">
                    <MarkdownEditor
                      content={document()!.content}
                      onChange={updateContent}
                      onSave={saveDocument}
                    />
                  </div>
                </Show>
                <Show when={viewMode() === "split"}>
                  <div class="pane-divider" />
                </Show>
                <Show when={viewMode() !== "editor"}>
                  <div class="preview-pane">
                    <Preview content={document()!.content} />
                  </div>
                </Show>
              </div>
            </Show>
          </Show>

          {/* Learn Mode */}
          <Show when={appMode() === "learn"}>
            <div class="learn-view">
              <LearningPanel />
            </div>
          </Show>

          {/* Unstuck Mode - Full view chat */}
          <Show when={appMode() === "unstuck"}>
            <div class="unstuck-view">
              <ChatPanel
                contextContent={selectedText()}
                onClose={goHome}
                fullView={true}
              />
            </div>
          </Show>
        </main>

        {/* Chat Panel - Side panel (only when not in unstuck mode) */}
        <Show when={showChat() && appMode() !== "unstuck"}>
          <div class="chat-panel-container">
            <ChatPanel
              contextContent={selectedText()}
              onClose={() => setShowChat(false)}
            />
          </div>
        </Show>
      </div>

      <Show when={appMode() !== "home"}>
        <footer class="status-bar">
          <Show when={appMode() === "markdown" && document()}>
            <span>
              {document()!.word_count} words
            </span>
          </Show>
          <Show when={document()?.path || pdfPath() || epubPath()}>
            <span class="status-path" title={document()?.path || pdfPath() || epubPath() || ""}>
              {(document()?.path || pdfPath() || epubPath())?.split(/[/\\]/).slice(-2).join("/")}
            </span>
          </Show>
          <Show when={documentTimer.documentPath()}>
            <span class="status-timer" title="Time spent on this document">
              {documentTimer.formattedTime()}
            </span>
          </Show>
          <span class="status-spacer" />
          <span>
            {version()
              ? `${version()!.name} v${version()!.version}`
              : "Loading..."}
          </span>
        </footer>
      </Show>

      {/* Print Preview Dialog */}
      <Show when={showPrintPreview() && (appMode() === "markdown" || appMode() === "pdf")}>
        <PrintPreview
          type={appMode() === "pdf" ? "pdf" : "markdown"}
          content={appMode() === "markdown" ? document()?.content : undefined}
          pdfPath={appMode() === "pdf" ? pdfPath() || undefined : undefined}
          onClose={() => setShowPrintPreview(false)}
        />
      </Show>

      {/* Research Hub */}
      <Show when={showResearchHub()}>
        <ResearchHub onClose={() => setShowResearchHub(false)} />
      </Show>

      {/* Sessions */}
      <Show when={showSessions()}>
        <div class="sessions-dialog">
          <Sessions
            onClose={() => setShowSessions(false)}
            onError={handleSessionError}
            showToast={showToast}
          />
        </div>
      </Show>

      {/* Vector Query */}
      <Show when={showVectorQuery()}>
        <div class="vector-query-dialog">
          <VectorQuery onClose={() => setShowVectorQuery(false)} />
        </div>
      </Show>

      {/* Provider Settings */}
      <ProviderSettings
        isOpen={showProviderSettings()}
        onClose={() => setShowProviderSettings(false)}
      />
    </div>
  );
}

function App() {
  return (
    <ToastProvider>
      <AppContent />
    </ToastProvider>
  );
}

export default App;
