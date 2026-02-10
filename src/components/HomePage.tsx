import { Component, createSignal, onMount, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./HomePage.css";

export interface RecentFile {
  path: string;
  name: string;
  file_type: string;
  last_opened: string;
}

export interface ActiveAiSession {
  id: string;
  project_name: string;
  project_path: string;
  slug: string | null;
  git_branch: string | null;
  last_active: string;
  tool: string;
}

interface HomePageProps {
  onNewMarkdown: () => void;
  onOpenFile: () => void;
  onOpenRecent: (path: string) => void;
  onToggleSessions: () => void;
  onToggleResearchHub: () => void;
  onToggleVectorSearch: () => void;
  onOpenLearn?: () => void;
  onOpenUnstuck?: () => void;
}

const HomePage: Component<HomePageProps> = (props) => {
  const [recentFiles, setRecentFiles] = createSignal<RecentFile[]>([]);
  const [aiSessions, setAiSessions] = createSignal<ActiveAiSession[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [loadingAi, setLoadingAi] = createSignal(true);

  onMount(async () => {
    // Load both in parallel
    const [filesPromise, sessionsPromise] = [
      invoke<RecentFile[]>("get_recent_files", { limit: 12 }),
      invoke<ActiveAiSession[]>("get_active_ai_sessions", { limit: 6, hoursAgo: 48 }),
    ];

    try {
      const files = await filesPromise;
      setRecentFiles(files);
    } catch (e) {
      console.error("Failed to load recent files:", e);
    } finally {
      setLoading(false);
    }

    try {
      const sessions = await sessionsPromise;
      setAiSessions(sessions);
    } catch (e) {
      console.error("Failed to load AI sessions:", e);
    } finally {
      setLoadingAi(false);
    }
  });

  const formatRelativeTime = (isoString: string): string => {
    const date = new Date(isoString);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return "Just now";
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    if (diffDays < 30) return `${Math.floor(diffDays / 7)}w ago`;
    return date.toLocaleDateString();
  };

  const getFileIcon = (fileType: string): string => {
    switch (fileType.toLowerCase()) {
      case "md":
      case "markdown":
        return "\u{1F4DD}"; // Memo
      case "pdf":
        return "\u{1F4D1}"; // Bookmark tabs (PDF-like)
      case "epub":
        return "\u{1F4DA}"; // Books
      case "txt":
        return "\u{1F4C4}"; // Page facing up
      default:
        return "\u{1F4C4}"; // Page facing up
    }
  };

  const getFileTypeLabel = (fileType: string): string => {
    switch (fileType.toLowerCase()) {
      case "md":
      case "markdown":
        return "Markdown";
      case "pdf":
        return "PDF";
      case "epub":
        return "EPUB";
      case "txt":
        return "Text";
      default:
        return fileType.toUpperCase();
    }
  };

  const getToolIcon = (tool: string): string => {
    switch (tool) {
      case "claude-code":
        return "\u{1F916}"; // Robot
      case "cursor":
        return "\u{2328}"; // Keyboard
      default:
        return "\u{1F4BB}"; // Laptop
    }
  };

  const getToolLabel = (tool: string): string => {
    switch (tool) {
      case "claude-code":
        return "Claude Code";
      case "cursor":
        return "Cursor";
      default:
        return tool;
    }
  };

  const formatSlug = (slug: string | null): string => {
    if (!slug) return "";
    // Convert slug like "merry-yawning-squid" to "Merry Yawning Squid"
    return slug
      .split("-")
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(" ");
  };

  const openInTerminal = async (session: ActiveAiSession) => {
    try {
      // Use --resume flag with session ID to continue the specific session
      await invoke("open_terminal", {
        path: session.project_path,
        command: `claude --resume ${session.id}`,
      });
    } catch (e) {
      console.error("Failed to open terminal:", e);
      // Fallback to clipboard
      navigator.clipboard.writeText(`cd "${session.project_path}" && claude --resume ${session.id}`);
    }
  };

  return (
    <div class="home-page">
      <header class="home-header">
        <h1>Welcome back</h1>
        <p class="home-subtitle">Pick up where you left off</p>
      </header>

      {/* Active AI Coding Sessions */}
      <Show when={!loadingAi() && aiSessions().length > 0}>
        <section class="home-section ai-sessions-section">
          <h2 class="section-title">
            <span class="section-icon">{"\u{1F916}"}</span>
            Continue Coding Session
          </h2>
          <div class="ai-sessions-grid">
            <For each={aiSessions()}>
              {(session) => (
                <button
                  class="ai-session-card"
                  onClick={() => openInTerminal(session)}
                  title={`${session.project_path}\nClick to resume session`}
                >
                  <div class="ai-session-header">
                    <span class="ai-tool-icon">{getToolIcon(session.tool)}</span>
                    <span class="ai-tool-label">{getToolLabel(session.tool)}</span>
                    <span class="ai-session-time">{formatRelativeTime(session.last_active)}</span>
                  </div>
                  <div class="ai-session-project">{session.project_name}</div>
                  <Show when={session.slug}>
                    <div class="ai-session-slug">{formatSlug(session.slug)}</div>
                  </Show>
                  <Show when={session.git_branch}>
                    <div class="ai-session-branch">
                      <span class="branch-icon">{"\u{1F33F}"}</span>
                      {session.git_branch}
                    </div>
                  </Show>
                </button>
              )}
            </For>
          </div>
        </section>
      </Show>

      <div class="home-row">
        <section class="home-section home-section-flex">
          <h2 class="section-title">Recent Files</h2>
          <Show
            when={!loading()}
            fallback={
              <div class="loading-state">Loading recent files...</div>
            }
          >
            <Show
              when={recentFiles().length > 0}
              fallback={
                <div class="empty-state">
                  <p>No recent files yet. Open a document to get started.</p>
                </div>
              }
            >
              <div class="recent-files-grid">
                <For each={recentFiles()}>
                  {(file) => (
                    <button
                      class="recent-file-card"
                      onClick={() => props.onOpenRecent(file.path)}
                      title={file.path}
                    >
                      <span class="file-icon">{getFileIcon(file.file_type)}</span>
                      <span class="file-name">{file.name}</span>
                      <span class="file-meta">
                        <span class="file-type">{getFileTypeLabel(file.file_type)}</span>
                        <span class="file-time">{formatRelativeTime(file.last_opened)}</span>
                      </span>
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </Show>
        </section>

        <section class="home-section home-section-actions">
          <h2 class="section-title">Quick Actions</h2>
          <div class="quick-actions-list">
            <Show when={props.onOpenUnstuck}>
              <button class="action-btn action-unstuck" onClick={props.onOpenUnstuck}>
                <span class="action-icon">{"\u{1F4AC}"}</span>
                <span class="action-label">Unstuck</span>
              </button>
            </Show>
            <button class="action-btn action-primary" onClick={props.onNewMarkdown}>
              <span class="action-icon">+</span>
              <span class="action-label">New Markdown</span>
            </button>
            <button class="action-btn" onClick={props.onOpenFile}>
              <span class="action-icon">{"\u{1F4C2}"}</span>
              <span class="action-label">Open File</span>
            </button>
            <button class="action-btn" onClick={props.onToggleSessions}>
              <span class="action-icon">{"\u{1F4CB}"}</span>
              <span class="action-label">Sessions</span>
            </button>
            <button class="action-btn" onClick={props.onToggleResearchHub}>
              <span class="action-icon">{"\u{1F50D}"}</span>
              <span class="action-label">Research Hub</span>
            </button>
            <button class="action-btn" onClick={props.onToggleVectorSearch}>
              <span class="action-icon">{"\u{1F9E0}"}</span>
              <span class="action-label">Vector Search</span>
            </button>
            <Show when={props.onOpenLearn}>
              <button class="action-btn action-learn" onClick={props.onOpenLearn}>
                <span class="action-icon">{"\u{1F4DA}"}</span>
                <span class="action-label">Learn</span>
              </button>
            </Show>
          </div>
        </section>
      </div>

      <section class="home-section home-shortcuts">
        <h2 class="section-title">Keyboard Shortcuts</h2>
        <div class="shortcuts-grid">
          <div class="shortcut-item">
            <kbd>Ctrl+N</kbd>
            <span>New Document</span>
          </div>
          <div class="shortcut-item">
            <kbd>Ctrl+O</kbd>
            <span>Open File</span>
          </div>
          <div class="shortcut-item">
            <kbd>Ctrl+S</kbd>
            <span>Save</span>
          </div>
          <div class="shortcut-item">
            <kbd>Ctrl+P</kbd>
            <span>Print</span>
          </div>
          <div class="shortcut-item">
            <kbd>Ctrl+/</kbd>
            <span>AI Chat</span>
          </div>
          <div class="shortcut-item">
            <kbd>Ctrl+K</kbd>
            <span>Vector Search</span>
          </div>
        </div>
      </section>
    </div>
  );
};

export default HomePage;
