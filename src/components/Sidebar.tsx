import { Component, createSignal } from "solid-js";
import MemoryBrowser from "./MemoryBrowser";
import "./Sidebar.css";

interface SidebarProps {
  onNewDocument: () => void;
  onOpenDocument: () => void;
  onOpenPdf: () => void;
  onOpenEpub: () => void;
  onPrint?: () => void;
  canPrint?: boolean;
  onToggleChat?: () => void;
  chatOpen?: boolean;
  onToggleResearchHub?: () => void;
  researchHubOpen?: boolean;
}

type SidebarTab = "files" | "memory";

const Sidebar: Component<SidebarProps> = (props) => {
  const [activeTab, setActiveTab] = createSignal<SidebarTab>("files");

  return (
    <aside class="sidebar">
      <div class="sidebar-tabs">
        <button
          class={`tab-btn ${activeTab() === "files" ? "active" : ""}`}
          onClick={() => setActiveTab("files")}
        >
          Files
        </button>
        <button
          class={`tab-btn ${activeTab() === "memory" ? "active" : ""}`}
          onClick={() => setActiveTab("memory")}
        >
          Memory
        </button>
      </div>

      {activeTab() === "files" ? (
        <>
          <div class="sidebar-actions">
            <button class="sidebar-btn" onClick={props.onNewDocument}>
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <path d="M8 2v12M2 8h12" stroke="currentColor" stroke-width="1.5" fill="none" />
              </svg>
              New Markdown
            </button>
            <button class="sidebar-btn" onClick={props.onOpenDocument}>
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <path d="M2 3h5l2 2h5v8H2V3z" stroke="currentColor" stroke-width="1" fill="none" />
              </svg>
              Open File
            </button>
            <button class="sidebar-btn pdf-btn" onClick={props.onOpenPdf}>
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <rect x="2" y="1" width="12" height="14" rx="1" stroke="currentColor" stroke-width="1" fill="none" />
                <text x="8" y="11" text-anchor="middle" font-size="6" font-weight="bold" fill="currentColor">PDF</text>
              </svg>
              Open PDF
            </button>
            <button class="sidebar-btn epub-btn" onClick={props.onOpenEpub}>
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <rect x="3" y="1" width="10" height="14" rx="1" stroke="currentColor" stroke-width="1" fill="none" />
                <path d="M5 4h6M5 6h6M5 8h4" stroke="currentColor" stroke-width="0.8" />
              </svg>
              Open EPUB
            </button>
            <button
              class="sidebar-btn"
              onClick={props.onPrint}
              disabled={!props.canPrint}
              title="Print (Ctrl+P)"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <rect x="3" y="8" width="10" height="6" rx="1" stroke="currentColor" stroke-width="1" fill="none" />
                <rect x="4" y="2" width="8" height="5" stroke="currentColor" stroke-width="1" fill="none" />
                <rect x="5" y="10" width="6" height="1" fill="currentColor" />
                <rect x="5" y="12" width="4" height="1" fill="currentColor" />
              </svg>
              Print
            </button>
            <button
              class={`sidebar-btn chat-btn ${props.chatOpen ? "active" : ""}`}
              onClick={props.onToggleChat}
              title="AI Chat (Ctrl+/)"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <path d="M2 2h12v9H5l-3 3V2z" stroke="currentColor" stroke-width="1" fill="none" />
                <circle cx="5" cy="6.5" r="1" fill="currentColor" />
                <circle cx="8" cy="6.5" r="1" fill="currentColor" />
                <circle cx="11" cy="6.5" r="1" fill="currentColor" />
              </svg>
              Unstuck
            </button>
            <button
              class={`sidebar-btn research-btn ${props.researchHubOpen ? "active" : ""}`}
              onClick={props.onToggleResearchHub}
              title="Research Hub (Ctrl+R)"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <circle cx="6" cy="6" r="4" stroke="currentColor" stroke-width="1.2" fill="none" />
                <path d="M9 9l4 4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                <path d="M4 6h4M6 4v4" stroke="currentColor" stroke-width="0.8" />
              </svg>
              Research
            </button>
          </div>

          <div class="sidebar-section">
            <h3 class="sidebar-section-title">Recent Files</h3>
            <div class="sidebar-empty">No recent files</div>
          </div>

          <div class="sidebar-section">
            <h3 class="sidebar-section-title">Quick Insert</h3>
            <div class="quick-insert-grid">
              <button class="quick-btn" title="Insert Table">Table</button>
              <button class="quick-btn" title="Insert Diagram">Diagram</button>
              <button class="quick-btn" title="Insert Math">Math</button>
              <button class="quick-btn" title="Insert Chart">Chart</button>
            </div>
          </div>
        </>
      ) : (
        <MemoryBrowser />
      )}
    </aside>
  );
};

export default Sidebar;
