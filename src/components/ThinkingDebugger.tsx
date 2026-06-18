import { Component, createSignal, For, Show, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./ThinkingDebugger.css";

interface ThinkingError {
  error_type: string;
  location: string;
  problem: string;
  why_it_matters: string;
  better_approach: string;
  severity: string;
}

interface ThinkingAnalysis {
  summary: string;
  errors: ThinkingError[];
  strengths: string[];
  critical_thinking_score: number;
  focus_area: string;
}

interface ConversationMessage {
  role: string;
  content: string;
}

interface SessionInfo {
  id: string;
  project_path: string | null;
  git_branch: string | null;
  chunk_count: number;
  message_count: number;
  started_at: string | null;
  ended_at: string | null;
  preview: string;
  source_file: string;
}

interface ThinkingDebuggerProps {
  conversation?: ConversationMessage[];
  topic?: string;
  onClose?: () => void;
}

const ThinkingDebugger: Component<ThinkingDebuggerProps> = (props) => {
  const [analysis, setAnalysis] = createSignal<ThinkingAnalysis | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [loadingSessions, setLoadingSessions] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [sessions, setSessions] = createSignal<SessionInfo[]>([]);
  const [selectedSession, setSelectedSession] = createSignal<SessionInfo | null>(null);
  const [conversation, setConversation] = createSignal<ConversationMessage[]>([]);
  const [topicInput, setTopicInput] = createSignal(props.topic || "");

  // Load sessions on mount
  onMount(async () => {
    await loadSessions();
  });

  const loadSessions = async () => {
    setLoadingSessions(true);
    try {
      const result = await invoke<SessionInfo[]>("list_sessions", { limit: 20 });
      setSessions(result);
    } catch (e) {
      console.error("Failed to load sessions:", e);
    } finally {
      setLoadingSessions(false);
    }
  };

  const selectSession = async (session: SessionInfo) => {
    setSelectedSession(session);
    setLoading(true);
    setError(null);
    setAnalysis(null);

    try {
      const messages = await invoke<ConversationMessage[]>("get_session_conversation", {
        sessionId: session.id,
      });
      setConversation(messages);
    } catch (e) {
      setError(`Failed to load session: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const analyzeConversation = async () => {
    const conv = conversation();
    if (conv.length === 0) {
      setError("No conversation loaded. Please select a session first.");
      return;
    }

    setLoading(true);
    setError(null);

    try {
      const result = await invoke<ThinkingAnalysis>("analyze_thinking", {
        conversation: conv,
        topic: topicInput() || null,
      });
      setAnalysis(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const getSeverityColor = (severity: string) => {
    switch (severity) {
      case "critical": return "var(--error-color, #f38ba8)";
      case "moderate": return "var(--warning-color, #f9e2af)";
      case "minor": return "var(--text-secondary, #a6adc8)";
      default: return "var(--text-secondary)";
    }
  };

  const getErrorTypeIcon = (type: string) => {
    switch (type) {
      case "fact": return "❌";
      case "reasoning": return "🔀";
      case "question": return "❓";
      default: return "⚠️";
    }
  };

  const getErrorTypeLabel = (type: string) => {
    switch (type) {
      case "fact": return "Wrong Fact";
      case "reasoning": return "Wrong Reasoning";
      case "question": return "Wrong Question";
      default: return "Error";
    }
  };

  const getScoreColor = (score: number) => {
    if (score >= 80) return "var(--success-color, #a6e3a1)";
    if (score >= 60) return "var(--warning-color, #f9e2af)";
    return "var(--error-color, #f38ba8)";
  };

  const formatDate = (dateStr: string | null) => {
    if (!dateStr) return "";
    try {
      const date = new Date(dateStr);
      return date.toLocaleDateString() + " " + date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    } catch {
      return "";
    }
  };

  const getProjectName = (path: string | null) => {
    if (!path) return "Unknown Project";
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1] || path;
  };

  return (
    <div class="thinking-debugger">
      <div class="debugger-header">
        <h2>🔍 Thinking Debugger</h2>
        <Show when={props.onClose}>
          <button class="close-btn" onClick={props.onClose}>×</button>
        </Show>
      </div>

      {/* Session List */}
      <Show when={!selectedSession() && !analysis()}>
        <div class="session-list-container">
          <p class="intro-text">Select a session to analyze your thinking:</p>

          <Show when={loadingSessions()}>
            <div class="loading-state">
              <div class="spinner" />
              <p>Loading sessions...</p>
            </div>
          </Show>

          <Show when={!loadingSessions() && sessions().length === 0}>
            <div class="empty-state">
              <p>No Claude Code sessions found.</p>
              <p class="subtitle">Start a conversation in Claude Code to analyze it here.</p>
            </div>
          </Show>

          <Show when={sessions().length > 0}>
            <div class="session-list">
              <For each={sessions()}>
                {(session) => (
                  <button class="session-card" onClick={() => selectSession(session)}>
                    <div class="session-project">{getProjectName(session.project_path)}</div>
                    <div class="session-preview">{session.preview || "Empty session"}</div>
                    <div class="session-meta">
                      <span>{session.chunk_count} exchanges</span>
                      <Show when={session.git_branch}>
                        <span class="branch-tag">{session.git_branch}</span>
                      </Show>
                      <span class="session-date">{formatDate(session.started_at)}</span>
                    </div>
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>

      {/* Session Selected - Ready to Analyze */}
      <Show when={selectedSession() && !analysis() && !loading()}>
        <div class="analyze-ready">
          <div class="selected-session-info">
            <h3>{getProjectName(selectedSession()!.project_path)}</h3>
            <p>{conversation().length} messages loaded</p>
            <Show when={selectedSession()!.git_branch}>
              <span class="branch-tag">{selectedSession()!.git_branch}</span>
            </Show>
          </div>

          <div class="topic-input-section">
            <label>Topic/Context (optional):</label>
            <input
              type="text"
              class="topic-input"
              value={topicInput()}
              onInput={(e) => setTopicInput(e.currentTarget.value)}
              placeholder="e.g., debugging, architecture, learning React..."
            />
          </div>

          <button class="analyze-btn" onClick={analyzeConversation}>
            Analyze My Thinking
          </button>

          <button class="back-btn" onClick={() => { setSelectedSession(null); setConversation([]); }}>
            ← Choose Different Session
          </button>
        </div>
      </Show>

      {/* Loading */}
      <Show when={loading()}>
        <div class="loading-state">
          <div class="spinner" />
          <p>Analyzing your thinking...</p>
        </div>
      </Show>

      {/* Error */}
      <Show when={error()}>
        <div class="error-state">
          <p>Error: {error()}</p>
          <button onClick={() => setError(null)}>Dismiss</button>
        </div>
      </Show>

      {/* Analysis Results */}
      <Show when={analysis()}>
        <div class="analysis-results">
          {/* Score */}
          <div class="score-section">
            <div class="score-circle" style={{ "border-color": getScoreColor(analysis()!.critical_thinking_score) }}>
              <span class="score-value" style={{ color: getScoreColor(analysis()!.critical_thinking_score) }}>
                {analysis()!.critical_thinking_score}
              </span>
              <span class="score-label">Score</span>
            </div>
            <div class="score-summary">
              <p>{analysis()!.summary}</p>
            </div>
          </div>

          {/* Focus Area */}
          <div class="focus-section">
            <h3>🎯 Focus On</h3>
            <p>{analysis()!.focus_area}</p>
          </div>

          {/* Errors */}
          <Show when={analysis()!.errors.length > 0}>
            <div class="errors-section">
              <h3>Where You Went Wrong</h3>
              <For each={analysis()!.errors}>
                {(err) => (
                  <div class="error-card" style={{ "border-left-color": getSeverityColor(err.severity) }}>
                    <div class="error-header">
                      <span class="error-type">
                        {getErrorTypeIcon(err.error_type)} {getErrorTypeLabel(err.error_type)}
                      </span>
                      <span class="error-severity" style={{ color: getSeverityColor(err.severity) }}>
                        {err.severity}
                      </span>
                    </div>
                    <div class="error-location">
                      <em>"{err.location}"</em>
                    </div>
                    <div class="error-problem">
                      <strong>Problem:</strong> {err.problem}
                    </div>
                    <div class="error-why">
                      <strong>Why it matters:</strong> {err.why_it_matters}
                    </div>
                    <div class="error-better">
                      <strong>Better approach:</strong> {err.better_approach}
                    </div>
                  </div>
                )}
              </For>
            </div>
          </Show>

          {/* Strengths */}
          <Show when={analysis()!.strengths.length > 0}>
            <div class="strengths-section">
              <h3>✓ What You Did Well</h3>
              <ul>
                <For each={analysis()!.strengths}>
                  {(strength) => <li>{strength}</li>}
                </For>
              </ul>
            </div>
          </Show>

          {/* Actions */}
          <div class="result-actions">
            <button class="analyze-again-btn" onClick={analyzeConversation}>
              Analyze Again
            </button>
            <button class="back-btn" onClick={() => { setAnalysis(null); setSelectedSession(null); setConversation([]); }}>
              ← Analyze Different Session
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default ThinkingDebugger;
