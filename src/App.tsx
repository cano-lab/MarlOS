import { createSignal, onMount, Show, createEffect } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import MarkdownEditor from "./components/MarkdownEditor";
import Preview from "./components/Preview";
import PdfViewer from "./components/PdfViewer";
import Sidebar from "./components/Sidebar";
import Titlebar from "./components/Titlebar";
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
type AppMode = "markdown" | "pdf";

function App() {
  const [version, setVersion] = createSignal<VersionInfo | null>(null);
  const [document, setDocument] = createSignal<Document | null>(null);
  const [sidebarOpen, setSidebarOpen] = createSignal(true);
  const [viewMode, setViewMode] = createSignal<ViewMode>("split");
  const [appMode, setAppMode] = createSignal<AppMode>("markdown");
  const [pdfPath, setPdfPath] = createSignal<string | null>(null);

  onMount(async () => {
    try {
      const info = await invoke<VersionInfo>("get_version");
      setVersion(info);
    } catch (e) {
      console.error("Failed to get version:", e);
    }
  });

  const createNewDocument = async () => {
    setAppMode("markdown");
    setPdfPath(null);
    try {
      const doc = await invoke<Document>("create_document");
      setDocument(doc);
    } catch (e) {
      console.error("Failed to create document:", e);
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

      setAppMode("markdown");
      setPdfPath(null);

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

      // Also register with kernel
      try {
        await invoke("open_document", { path });
      } catch (e) {
        console.log("Kernel registration skipped:", e);
      }
    } catch (e) {
      console.error("Failed to open document:", e);
      alert(`Failed to open file: ${e}`);
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
      setAppMode("pdf");
      setPdfPath(path);
    } catch (e) {
      console.error("Failed to open PDF:", e);
      alert(`Failed to open PDF: ${e}`);
    }
  };

  const closePdf = () => {
    setAppMode("markdown");
    setPdfPath(null);
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

    } catch (e) {
      console.error("Failed to save document:", e);
      alert(`Failed to save file: ${e}`);
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
    if ((e.ctrlKey || e.metaKey) && e.key === "\\") {
      e.preventDefault();
      setSidebarOpen(!sidebarOpen());
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
    // Escape to close PDF
    if (e.key === "Escape" && appMode() === "pdf") {
      closePdf();
    }
  };

  onMount(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  });

  const getTitle = () => {
    if (appMode() === "pdf" && pdfPath()) {
      return pdfPath()!.split(/[/\\]/).pop() || "PDF";
    }
    return document()?.title || "MarlOS";
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
          />
        </Show>

        <main class="main-content">
          {/* PDF Mode */}
          <Show when={appMode() === "pdf" && pdfPath()}>
            <PdfViewer path={pdfPath()!} onClose={closePdf} />
          </Show>

          {/* Markdown Mode */}
          <Show when={appMode() === "markdown"}>
            <Show
              when={document()}
              fallback={
                <div class="welcome">
                  <h1>Welcome to MarlOS</h1>
                  <p>A semantic document editor for papers, PDFs, and research</p>
                  <div class="welcome-actions">
                    <button class="btn-primary" onClick={createNewDocument}>
                      New Markdown
                    </button>
                    <button class="btn-secondary" onClick={() => openDocument()}>
                      Open File
                    </button>
                    <button class="btn-secondary" onClick={() => openPdf()}>
                      Open PDF
                    </button>
                  </div>
                  <div class="welcome-shortcuts">
                    <h3>Keyboard Shortcuts</h3>
                    <div class="shortcut-grid">
                      <span class="shortcut-key">Ctrl+N</span><span>New Document</span>
                      <span class="shortcut-key">Ctrl+O</span><span>Open File</span>
                      <span class="shortcut-key">Ctrl+S</span><span>Save</span>
                      <span class="shortcut-key">Ctrl+\</span><span>Toggle Sidebar</span>
                      <span class="shortcut-key">Ctrl+1</span><span>Editor Only</span>
                      <span class="shortcut-key">Ctrl+2</span><span>Split View</span>
                      <span class="shortcut-key">Ctrl+3</span><span>Preview Only</span>
                    </div>
                  </div>
                  <div class="welcome-features">
                    <h3>Features</h3>
                    <ul>
                      <li><strong>Markdown Editor</strong> - Mermaid diagrams, LaTeX math, charts</li>
                      <li><strong>PDF Viewer</strong> - Accurate rendering with measurement tools</li>
                      <li><strong>Calibration</strong> - Calibrate scale for real-world measurements</li>
                    </ul>
                  </div>
                </div>
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
        </main>
      </div>

      <Show when={appMode() === "markdown"}>
        <footer class="status-bar">
          <span>
            {document()
              ? `${document()!.word_count} words`
              : "No document open"}
          </span>
          <Show when={document()?.path}>
            <span class="status-path" title={document()!.path || ""}>
              {document()!.path?.split(/[/\\]/).slice(-2).join("/")}
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
    </div>
  );
}

export default App;
