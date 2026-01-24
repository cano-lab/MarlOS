import { Component, Show } from "solid-js";
import "./Titlebar.css";

type ViewMode = "editor" | "preview" | "split";

interface TitlebarProps {
  title: string;
  version: string;
  isDirty: boolean;
  onToggleSidebar: () => void;
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  showViewMode?: boolean;
}

const Titlebar: Component<TitlebarProps> = (props) => {
  return (
    <header class="titlebar" data-tauri-drag-region>
      <div class="titlebar-left">
        <button class="titlebar-btn" onClick={props.onToggleSidebar} title="Toggle Sidebar (Ctrl+\)">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M2 3h12v1.5H2V3zm0 4h12v1.5H2V7zm0 4h12v1.5H2V11z" />
          </svg>
        </button>
      </div>

      <div class="titlebar-center" data-tauri-drag-region>
        <span class="titlebar-title">
          {props.title}
          {props.isDirty && <span class="dirty-indicator">*</span>}
        </span>
      </div>

      <div class="titlebar-right">
        <Show when={props.showViewMode !== false}>
          <div class="view-mode-toggle">
            <button
              class={`view-btn ${props.viewMode === "editor" ? "active" : ""}`}
              onClick={() => props.onViewModeChange("editor")}
              title="Editor Only (Ctrl+1)"
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor">
                <rect x="1" y="2" width="12" height="10" rx="1" stroke="currentColor" fill="none" stroke-width="1.5"/>
              </svg>
            </button>
            <button
              class={`view-btn ${props.viewMode === "split" ? "active" : ""}`}
              onClick={() => props.onViewModeChange("split")}
              title="Split View (Ctrl+2)"
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor">
                <rect x="1" y="2" width="12" height="10" rx="1" stroke="currentColor" fill="none" stroke-width="1.5"/>
                <line x1="7" y1="2" x2="7" y2="12" stroke="currentColor" stroke-width="1.5"/>
              </svg>
            </button>
            <button
              class={`view-btn ${props.viewMode === "preview" ? "active" : ""}`}
              onClick={() => props.onViewModeChange("preview")}
              title="Preview Only (Ctrl+3)"
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor">
                <rect x="1" y="2" width="12" height="10" rx="1" fill="currentColor" opacity="0.3"/>
                <rect x="1" y="2" width="12" height="10" rx="1" stroke="currentColor" fill="none" stroke-width="1.5"/>
              </svg>
            </button>
          </div>
        </Show>
        <span class="titlebar-version">v{props.version}</span>
      </div>
    </header>
  );
};

export default Titlebar;
