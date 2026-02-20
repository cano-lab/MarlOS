import { Component, createSignal, For, Show } from "solid-js";
import MemoryBrowser from "./MemoryBrowser";
import ProactiveSuggestions from "./ProactiveSuggestions";
import DecisionLogger from "./DecisionLogger";
import ProvidersPanel from "./ProvidersPanel";
import type { RecentFile } from "./HomePage";
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
  onToggleSessions?: () => void;
  sessionsOpen?: boolean;
  onToggleVectorQuery?: () => void;
  vectorQueryOpen?: boolean;
  onTogglePlanSpace?: () => void;
  planSpaceOpen?: boolean;
  currentFile?: string;
  currentProject?: string;
  // New props for home and recent files
  onGoHome?: () => void;
  recentFiles?: RecentFile[];
  onOpenRecent?: (path: string) => void;
  // Provider settings
  onOpenProviderSettings?: () => void;
}

type SidebarTab = "files" | "memory" | "providers";

const Sidebar: Component<SidebarProps> = (props) => {
  const [activeTab, setActiveTab] = createSignal<SidebarTab>("files");
  const [suggestionsCollapsed, setSuggestionsCollapsed] = createSignal(false);
  const [decisionLoggerOpen, setDecisionLoggerOpen] = createSignal(false);

  const formatRelativeTime = (isoString: string): string => {
    const date = new Date(isoString);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return "Just now";
    if (diffMins < 60) return `${diffMins}m`;
    if (diffHours < 24) return `${diffHours}h`;
    if (diffDays < 7) return `${diffDays}d`;
    return date.toLocaleDateString();
  };

  const getFileIcon = (fileType: string): string => {
    switch (fileType.toLowerCase()) {
      case "md":
      case "markdown":
        return "\u{1F4DD}";
      case "pdf":
        return "\u{1F4D1}";
      case "epub":
        return "\u{1F4DA}";
      default:
        return "\u{1F4C4}";
    }
  };

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
        <button
          class={`tab-btn ${activeTab() === "providers" ? "active" : ""}`}
          onClick={() => setActiveTab("providers")}
        >
          Providers
        </button>
      </div>

      {activeTab() === "files" ? (
        <>
          <div class="sidebar-actions">
            {/* Home button */}
            <Show when={props.onGoHome}>
              <button class="sidebar-btn home-btn" onClick={props.onGoHome} title="Go to Home">
                <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                  <path d="M2 8l6-5 6 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
                  <path d="M3 7v6a1 1 0 001 1h8a1 1 0 001-1V7" stroke="currentColor" stroke-width="1.5" />
                  <path d="M6 14v-4h4v4" stroke="currentColor" stroke-width="1.5" />
                </svg>
                Home
              </button>
            </Show>

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
            <button
              class={`sidebar-btn sessions-btn ${props.sessionsOpen ? "active" : ""}`}
              onClick={props.onToggleSessions}
              title="Sessions (Ctrl+Shift+S)"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <rect x="2" y="3" width="12" height="10" rx="1" stroke="currentColor" stroke-width="1" fill="none" />
                <path d="M5 6h6M5 8h6M5 10h4" stroke="currentColor" stroke-width="1" />
              </svg>
              Sessions
            </button>
            <button
              class={`sidebar-btn vector-btn ${props.vectorQueryOpen ? "active" : ""}`}
              onClick={props.onToggleVectorQuery}
              title="Vector Search"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <circle cx="6" cy="6" r="4" stroke="currentColor" stroke-width="1.2" fill="none" />
                <path d="M9 9l4 4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                <path d="M4 6h4M6 4v4" stroke="currentColor" stroke-width="0.8" />
              </svg>
              Vector Search
            </button>
            <button
              class={`sidebar-btn planspace-btn ${props.planSpaceOpen ? "active" : ""}`}
              onClick={props.onTogglePlanSpace}
              title="Plan Space (Ctrl+G)"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <circle cx="4" cy="4" r="2" stroke="currentColor" stroke-width="1" fill="none" />
                <circle cx="12" cy="4" r="2" stroke="currentColor" stroke-width="1" fill="none" />
                <circle cx="8" cy="12" r="2" stroke="currentColor" stroke-width="1" fill="none" />
                <path d="M5.5 5.5L7 10.5M10.5 5.5L9 10.5" stroke="currentColor" stroke-width="0.8" opacity="0.5" />
              </svg>
              Plan Space
            </button>
          </div>

          {/* Recent Files Section */}
          <div class="sidebar-section recent-files-section">
            <h3 class="sidebar-section-title">Recent Files</h3>
            <Show
              when={props.recentFiles && props.recentFiles.length > 0}
              fallback={<div class="sidebar-empty">No recent files</div>}
            >
              <div class="recent-files-list">
                <For each={(props.recentFiles || []).slice(0, 5)}>
                  {(file) => (
                    <button
                      class="recent-file-btn"
                      onClick={() => props.onOpenRecent?.(file.path)}
                      title={file.path}
                    >
                      <span class="recent-file-icon">{getFileIcon(file.file_type)}</span>
                      <span class="recent-file-name">{file.name}</span>
                      <span class="recent-file-time">{formatRelativeTime(file.last_opened)}</span>
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </div>

          {/* Proactive Suggestions */}
          <ProactiveSuggestions
            currentFile={props.currentFile}
            currentProject={props.currentProject}
            collapsed={suggestionsCollapsed()}
            onToggleCollapse={() => setSuggestionsCollapsed(!suggestionsCollapsed())}
          />

          {/* Log Decision Button */}
          <div class="sidebar-section decision-section">
            <button
              class="sidebar-btn decision-btn"
              onClick={() => setDecisionLoggerOpen(true)}
              title="Log a decision for future reference"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <circle cx="8" cy="8" r="6" stroke="currentColor" fill="none" stroke-width="1" />
                <path d="M5 8l2 2 4-4" stroke="currentColor" stroke-width="1.5" fill="none" />
              </svg>
              Log Decision
            </button>
          </div>
        </>
      ) : activeTab() === "memory" ? (
        <MemoryBrowser />
      ) : (
        <ProvidersPanel onOpenSettings={props.onOpenProviderSettings} />
      )}

      {/* Decision Logger Modal */}
      <DecisionLogger
        isOpen={decisionLoggerOpen()}
        onClose={() => setDecisionLoggerOpen(false)}
        defaultProject={props.currentProject}
        defaultFilePath={props.currentFile}
      />
    </aside>
  );
};

export default Sidebar;
