import { Component, createSignal } from "solid-js";
import "./Sidebar.css";

interface SidebarProps {
  onNewDocument: () => void;
  onOpenDocument: () => void;
  onOpenPdf: () => void;
  onPrint?: () => void;
  canPrint?: boolean;
}

const Sidebar: Component<SidebarProps> = (props) => {
  const [searchQuery, setSearchQuery] = createSignal("");

  return (
    <aside class="sidebar">
      <div class="sidebar-header">
        <input
          type="text"
          class="search-input"
          placeholder="Search memories..."
          value={searchQuery()}
          onInput={(e) => setSearchQuery(e.currentTarget.value)}
        />
      </div>

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
      </div>

      <div class="sidebar-section">
        <h3 class="sidebar-section-title">Recent Files</h3>
        <div class="sidebar-empty">No recent files</div>
      </div>

      <div class="sidebar-section">
        <h3 class="sidebar-section-title">Memory Search</h3>
        <div class="sidebar-empty">
          Search to explore your document memory
        </div>
      </div>

      <div class="sidebar-section">
        <h3 class="sidebar-section-title">Quick Insert</h3>
        <div class="quick-insert-grid">
          <button class="quick-btn" title="Insert Table">📊 Table</button>
          <button class="quick-btn" title="Insert Diagram">◇ Diagram</button>
          <button class="quick-btn" title="Insert Math">∑ Math</button>
          <button class="quick-btn" title="Insert Chart">📈 Chart</button>
        </div>
      </div>
    </aside>
  );
};

export default Sidebar;
