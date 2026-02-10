import { Component, createSignal, createEffect, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./ProactiveSuggestions.css";

interface Suggestion {
  id: string;
  suggestion_type: string;
  relevance: number;
  title: string;
  content: string;
  reason: string;
  source_ids: string[];
  generated_at: string;
  seen: boolean;
}

interface ProactiveSuggestionsProps {
  currentFile?: string;
  currentProject?: string;
  onSuggestionClick?: (suggestion: Suggestion) => void;
  onDismiss?: (suggestionId: string) => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
}

const ProactiveSuggestions: Component<ProactiveSuggestionsProps> = (props) => {
  const [suggestions, setSuggestions] = createSignal<Suggestion[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [dismissed, setDismissed] = createSignal<Set<string>>(new Set());

  // Fetch suggestions when file or project changes
  createEffect(async () => {
    const file = props.currentFile;
    const project = props.currentProject;

    if (!file && !project) {
      setSuggestions([]);
      return;
    }

    setLoading(true);
    setError(null);

    try {
      let newSuggestions: Suggestion[] = [];

      // Get file-based suggestions
      if (file) {
        const fileSuggestions = await invoke<Suggestion[]>("proactive_on_file_opened", {
          filePath: file,
        });
        newSuggestions = [...newSuggestions, ...fileSuggestions];
      }

      // Get session start suggestions
      if (project) {
        const sessionSuggestions = await invoke<Suggestion[]>("proactive_on_session_start", {
          projectPath: project,
        });
        newSuggestions = [...newSuggestions, ...sessionSuggestions];
      }

      // Filter out dismissed ones
      const activeSuggestions = newSuggestions.filter(
        (s) => !dismissed().has(s.id)
      );

      setSuggestions(activeSuggestions);
    } catch (e) {
      console.error("Failed to fetch suggestions:", e);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  });

  const handleDismiss = (id: string) => {
    setDismissed((prev) => new Set([...prev, id]));
    setSuggestions((prev) => prev.filter((s) => s.id !== id));
    props.onDismiss?.(id);
  };

  const getSuggestionIcon = (type: string) => {
    switch (type) {
      case "RelatedContext":
        return (
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 1L1 5v6l7 4 7-4V5L8 1z" stroke="currentColor" fill="none" stroke-width="1" />
            <circle cx="8" cy="8" r="2" fill="currentColor" />
          </svg>
        );
      case "PastDecision":
        return (
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <circle cx="8" cy="8" r="6" stroke="currentColor" fill="none" stroke-width="1" />
            <path d="M5 8l2 2 4-4" stroke="currentColor" stroke-width="1.5" fill="none" />
          </svg>
        );
      case "DecisionConflict":
        return (
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 1l7 14H1L8 1z" stroke="currentColor" fill="none" stroke-width="1" />
            <path d="M8 6v4M8 12v1" stroke="currentColor" stroke-width="1.5" />
          </svg>
        );
      case "WorkContinuity":
        return (
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M2 8h12M10 4l4 4-4 4" stroke="currentColor" stroke-width="1.5" fill="none" />
          </svg>
        );
      case "SimilarSolution":
        return (
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <rect x="2" y="2" width="5" height="5" stroke="currentColor" fill="none" stroke-width="1" />
            <rect x="9" y="9" width="5" height="5" stroke="currentColor" fill="none" stroke-width="1" />
            <path d="M7 4.5h2M11.5 7v2" stroke="currentColor" stroke-width="1" />
          </svg>
        );
      case "DetectedPattern":
        return (
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <circle cx="4" cy="8" r="2" stroke="currentColor" fill="none" stroke-width="1" />
            <circle cx="8" cy="8" r="2" stroke="currentColor" fill="none" stroke-width="1" />
            <circle cx="12" cy="8" r="2" stroke="currentColor" fill="none" stroke-width="1" />
          </svg>
        );
      default:
        return (
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <circle cx="8" cy="8" r="6" stroke="currentColor" fill="none" stroke-width="1" />
            <path d="M8 5v4M8 11v1" stroke="currentColor" stroke-width="1.5" />
          </svg>
        );
    }
  };

  const getTypeLabel = (type: string) => {
    switch (type) {
      case "RelatedContext":
        return "Related";
      case "PastDecision":
        return "Decision";
      case "DecisionConflict":
        return "Conflict";
      case "WorkContinuity":
        return "Continue";
      case "SimilarSolution":
        return "Similar";
      case "DetectedPattern":
        return "Pattern";
      default:
        return "Info";
    }
  };

  const getTypeClass = (type: string) => {
    switch (type) {
      case "DecisionConflict":
        return "warning";
      case "PastDecision":
        return "decision";
      case "WorkContinuity":
        return "continuity";
      case "SimilarSolution":
        return "similar";
      default:
        return "info";
    }
  };

  return (
    <div class={`proactive-suggestions ${props.collapsed ? "collapsed" : ""}`}>
      <div class="suggestions-header" onClick={props.onToggleCollapse}>
        <span class="header-icon">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 1C4.134 1 1 4.134 1 8s3.134 7 7 7 7-3.134 7-7-3.134-7-7-7zm0 12c-2.761 0-5-2.239-5-5s2.239-5 5-5 5 2.239 5 5-2.239 5-5 5z" />
            <path d="M8 4v5l3 2" stroke="currentColor" stroke-width="1.5" fill="none" />
          </svg>
        </span>
        <span class="header-title">Proactive Memory</span>
        <Show when={suggestions().length > 0}>
          <span class="suggestion-count">{suggestions().length}</span>
        </Show>
        <span class="collapse-icon">{props.collapsed ? "+" : "-"}</span>
      </div>

      <Show when={!props.collapsed}>
        <div class="suggestions-content">
          <Show when={loading()}>
            <div class="suggestions-loading">
              <span class="spinner"></span>
              Checking memory...
            </div>
          </Show>

          <Show when={error()}>
            <div class="suggestions-error">
              {error()}
            </div>
          </Show>

          <Show when={!loading() && suggestions().length === 0 && !error()}>
            <div class="suggestions-empty">
              No relevant suggestions for current context.
            </div>
          </Show>

          <For each={suggestions()}>
            {(suggestion) => (
              <div
                class={`suggestion-card ${getTypeClass(suggestion.suggestion_type)}`}
                onClick={() => props.onSuggestionClick?.(suggestion)}
              >
                <div class="suggestion-header">
                  <span class="suggestion-icon">
                    {getSuggestionIcon(suggestion.suggestion_type)}
                  </span>
                  <span class="suggestion-type">
                    {getTypeLabel(suggestion.suggestion_type)}
                  </span>
                  <span class="suggestion-relevance">
                    {Math.round(suggestion.relevance * 100)}%
                  </span>
                  <button
                    class="dismiss-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDismiss(suggestion.id);
                    }}
                    title="Dismiss"
                  >
                    ×
                  </button>
                </div>
                <div class="suggestion-title">{suggestion.title}</div>
                <div class="suggestion-reason">{suggestion.reason}</div>
                <Show when={suggestion.content}>
                  <div class="suggestion-content">
                    {suggestion.content.slice(0, 200)}
                    {suggestion.content.length > 200 ? "..." : ""}
                  </div>
                </Show>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

export default ProactiveSuggestions;
