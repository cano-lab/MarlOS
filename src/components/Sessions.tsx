import { Component, createSignal, onMount, For, Show } from 'solid-js';
import { invoke } from '@tauri-apps/api/core';
import './Sessions.css';

// Session types matching the Rust backend
export interface Session {
  id: string;
  title: string;
  description?: string;
  intent: SessionIntent;
  started_at: string;
  ended_at?: string;
  activities: SessionActivity[];
  snapshots: SessionSnapshot[];
  context: SessionContext;
  next_steps: string[];
  tags: string[];
}

export interface SessionIntent {
  type: 'Research' | 'Writing' | 'Coding' | 'Learning' | 'Planning' | 'Debugging' | 'Brainstorming' | 'Other';
  topic?: string;
  project?: string;
  goal?: string;
  issue?: string;
  theme?: string;
  subject?: string;
  description?: string;
}

export interface SessionActivity {
  id: string;
  activity_type: 'DocumentView' | 'NoteTaking' | 'AiChat' | 'WebResearch' | 'Coding' | 'Writing' | 'Thinking' | 'Other';
  timestamp: string;
  duration?: number;
  details: ActivityDetails;
}

export interface ActivityDetails {
  viewed_document?: { title: string; path: string; page_number?: number; duration_secs: number };
  took_notes?: { content: string; related_to: string[] };
  ai_chat?: { provider: string; topic: string; message_count: number; summary: string };
  web_research?: { urls: string[]; search_queries: string[]; findings: string };
  coding?: { files_modified: string[]; language: string; commit_message?: string };
  writing?: { word_count: number; section?: string };
  thinking?: { notes: string; insights: string[] };
  other?: { description: string; metadata: Record<string, string> };
}

export interface SessionSnapshot {
  timestamp: string;
  open_documents: Array<{ title: string; path: string; page_number: number; scroll_position: number }>;
  browser_tabs: Array<{ url: string; title: string; favicon?: string }>;
  notes: string;
  mental_state?: { energy: number; focus: number; mood: string; sentiment: string };
  current_focus: string;
}

export interface SessionContext {
  project?: string;
  related_sessions: string[];
  prerequisites: string[];
  environment: string;
}

interface SessionsProps {
  onClose?: () => void;
  onError?: (error: string) => void;
  showToast?: (message: string, type: 'success' | 'info' | 'warning' | 'error') => void;
}

export const Sessions: Component<SessionsProps> = (props) => {
  const [currentSession, setCurrentSession] = createSignal<Session | null>(null);
  const [sessionHistory, setSessionHistory] = createSignal<Session[]>([]);
  const [selectedSession, setSelectedSession] = createSignal<Session | null>(null);
  const [viewMode, setViewMode] = createSignal<'list' | 'new' | 'detail' | 'backfill'>('list');
  const [searchQuery, setSearchQuery] = createSignal('');

  // Filter state
  const [filterIntent, setFilterIntent] = createSignal<SessionIntent['type'] | 'all'>('all');
  const [filterProvider, setFilterProvider] = createSignal<string>('all');
  const [filterDateFrom, setFilterDateFrom] = createSignal<string>('');
  const [filterDateTo, setFilterDateTo] = createSignal<string>('');

  // New session form state
  const [newTitle, setNewTitle] = createSignal('');
  const [newDescription, setNewDescription] = createSignal('');
  const [newIntentType, setNewIntentType] = createSignal<SessionIntent['type']>('Research');
  const [newIntentTopic, setNewIntentTopic] = createSignal('');

  // Backfill session form state
  const [backfillTitle, setBackfillTitle] = createSignal('');
  const [backfillDescription, setBackfillDescription] = createSignal('');
  const [backfillIntentType, setBackfillIntentType] = createSignal<SessionIntent['type']>('Research');
  const [backfillIntentTopic, setBackfillIntentTopic] = createSignal('');
  const [backfillStartTime, setBackfillStartTime] = createSignal('');
  const [backfillEndTime, setBackfillEndTime] = createSignal('');
  const [backfillActivities, setBackfillActivities] = createSignal('');

  // End session state
  const [nextSteps, setNextSteps] = createSignal('');

  const [loading, setLoading] = createSignal(false);

  const showMessage = (message: string, type: 'success' | 'info' | 'warning' | 'error' = 'info') => {
    if (props.showToast) {
      props.showToast(message, type);
    } else if (type === 'error') {
      props.onError?.(message);
    } else {
      console.log(`[Sessions] ${type}: ${message}`);
    }
  };

  const debugListObjects = async () => {
    try {
      const result = await invoke<string>('debug_list_objects');
      console.log('[Sessions] Objects in database:\n', result);
      // Show a brief toast, full details in console
      const lines = result.split('\n');
      const count = lines.length;
      showMessage(`Found ${count} objects in database (see console for details)`, 'info');
    } catch (error) {
      console.error('Failed to list objects:', error);
      showMessage(String(error), 'error');
    }
  };

  const importClaudeCode = async () => {
    try {
      setLoading(true);
      const result = await invoke<string>('import_claude_code_conversations');
      console.log('[Sessions] Claude Code import result:', result);
      showMessage(`${result}. Now click "Import Conversations" to create sessions.`, 'success');
    } catch (error) {
      console.error('Failed to import Claude Code conversations:', error);
      showMessage(String(error), 'error');
    } finally {
      setLoading(false);
    }
  };

  const importClaudeCodeSessions = async () => {
    try {
      setLoading(true);
      const result = await invoke<string>('import_claude_code_sessions');
      console.log('[Sessions] Claude Code sessions import result:', result);
      await loadSessions();
      showMessage(result, 'success');
    } catch (error) {
      console.error('Failed to import Claude Code sessions:', error);
      showMessage(String(error), 'error');
    } finally {
      setLoading(false);
    }
  };

  const syncFromAndor = async () => {
    try {
      setLoading(true);
      const result = await invoke<string>('sync_from_andor');
      console.log('[Sessions] Andor sync result:', result);
      await loadSessions();
      showMessage(result, 'success');
    } catch (error) {
      console.error('Failed to sync from Andor:', error);
      showMessage(String(error), 'error');
    } finally {
      setLoading(false);
    }
  };

  const importChatGPT = async () => {
    try {
      setLoading(true);
      const result = await invoke<string>('import_chatgpt_export');
      console.log('[Sessions] ChatGPT import result:', result);
      await loadSessions();
      showMessage(result, 'success');
    } catch (error) {
      console.error('Failed to import ChatGPT conversations:', error);
      showMessage(String(error), 'error');
    } finally {
      setLoading(false);
    }
  };

  const importConversations = async () => {
    try {
      setLoading(true);
      const result = await invoke<string>('import_conversations_as_sessions');
      await loadSessions();
      showMessage(result, 'success');
    } catch (error) {
      console.error('Failed to import conversations:', error);
      showMessage(String(error), 'error');
    } finally {
      setLoading(false);
    }
  };

  onMount(() => {
    loadSessions();
    console.log('[Sessions] Component mounted');
    console.log('[Sessions] Initial filters:', {
      intent: filterIntent(),
      provider: filterProvider(),
      dateFrom: filterDateFrom(),
      dateTo: filterDateTo(),
      search: searchQuery()
    });
  });

  const loadSessions = async () => {
    try {
      setLoading(true);
      const [current, history] = await Promise.all([
        invoke<Session | null>('get_current_session'),
        invoke<Session[]>('get_session_history'),
      ]);
      setCurrentSession(current);
      setSessionHistory(history);
    } catch (error) {
      console.error('Failed to load sessions:', error);
      props.onError?.(String(error));
    } finally {
      setLoading(false);
    }
  };

  // Get filtered sessions based on current filters
  const filteredSessions = () => {
    let sessions = sessionHistory();

    console.log('[Sessions] Total sessions:', sessions.length);
    console.log('[Sessions] Session intents:', sessions.map(s => s.intent.type));

    // Filter by intent type
    if (filterIntent() !== 'all') {
      const before = sessions.length;
      sessions = sessions.filter(s => s.intent.type === filterIntent());
      console.log(`[Sessions] Filtered by intent "${filterIntent()}": ${before} -> ${sessions.length}`);
    }

    // Filter by AI provider
    if (filterProvider() !== 'all') {
      const before = sessions.length;
      sessions = sessions.filter(s => {
        // Check if any AI chat activity used this provider
        return s.activities.some(a => {
          if (a.activity_type === 'AiChat' && a.details.ai_chat) {
            const provider = a.details.ai_chat!.provider.toLowerCase();
            return provider.includes(filterProvider().toLowerCase());
          }
          return false;
        });
      });
      console.log(`[Sessions] Filtered by provider "${filterProvider()}": ${before} -> ${sessions.length}`);
    }

    // Filter by date range
    if (filterDateFrom()) {
      const fromDate = new Date(filterDateFrom());
      sessions = sessions.filter(s => new Date(s.started_at) >= fromDate);
    }

    if (filterDateTo()) {
      const toDate = new Date(filterDateTo());
      toDate.setHours(23, 59, 59, 999); // End of day
      sessions = sessions.filter(s => new Date(s.started_at) <= toDate);
    }

    // Apply text search query
    if (searchQuery().trim()) {
      const query = searchQuery().toLowerCase();
      sessions = sessions.filter(s => {
        return s.title.toLowerCase().includes(query)
          || s.description?.toLowerCase().includes(query)
          || s.tags.some(t => t.toLowerCase().includes(query))
          || s.activities.some(a => {
            if (a.details.ai_chat) {
              return a.details.ai_chat!.summary.toLowerCase().includes(query)
                || a.details.ai_chat!.topic.toLowerCase().includes(query);
            }
            if (a.details.took_notes) {
              return a.details.took_notes!.content.toLowerCase().includes(query);
            }
            return false;
          });
      });
    }

    console.log('[Sessions] Final filtered count:', sessions.length);
    return sessions;
  };

  const clearFilters = () => {
    setFilterIntent('all');
    setFilterProvider('all');
    setFilterDateFrom('');
    setFilterDateTo('');
    setSearchQuery('');
  };

  const hasActiveFilters = () => {
    return filterIntent() !== 'all'
      || filterProvider() !== 'all'
      || filterDateFrom() !== ''
      || filterDateTo() !== ''
      || searchQuery() !== '';
  };

  const startSession = async () => {
    if (!newTitle().trim()) {
      props.onError?.('Please enter a session title');
      return;
    }

    try {
      setLoading(true);

      const session = await invoke<Session>('start_session', {
        title: newTitle(),
        intentType: newIntentType().toLowerCase(),
        intentValue: newIntentTopic(),
        description: newDescription() || undefined
      });

      setCurrentSession(session);
      setViewMode('list');

      // Clear form
      setNewTitle('');
      setNewDescription('');
      setNewIntentTopic('');
    } catch (error) {
      console.error('Failed to start session:', error);
      props.onError?.(String(error));
    } finally {
      setLoading(false);
    }
  };

  const createBackfillSession = async () => {
    if (!backfillTitle().trim()) {
      props.onError?.('Please enter a session title');
      return;
    }

    if (!backfillStartTime()) {
      props.onError?.('Please enter a start time');
      return;
    }

    try {
      setLoading(true);

      // Parse the datetime-local strings to ISO format
      const startTime = new Date(backfillStartTime()).toISOString();
      const endTime = backfillEndTime() ? new Date(backfillEndTime()).toISOString() : undefined;

      // For now, we'll create a note-taking activity with the activities summary
      const activities = backfillActivities().trim() ? [{
        activityType: 'thinking',
        details: {
          notes: backfillActivities(),
          insights: [],
        }
      }] : [];

      // Create the session with custom timestamps via a special command
      // For now, we'll create it as a regular session and update the history
      await invoke('create_backfill_session', {
        title: backfillTitle(),
        intentType: backfillIntentType().toLowerCase(),
        intentValue: backfillIntentTopic(),
        description: backfillDescription() || undefined,
        startTime,
        endTime,
        activities,
      });

      setViewMode('list');
      await loadSessions();

      // Clear form
      setBackfillTitle('');
      setBackfillDescription('');
      setBackfillIntentTopic('');
      setBackfillStartTime('');
      setBackfillEndTime('');
      setBackfillActivities('');
    } catch (error) {
      console.error('Failed to create backfill session:', error);
      props.onError?.(String(error));
    } finally {
      setLoading(false);
    }
  };

  const endSession = async () => {
    if (!currentSession()) return;

    try {
      setLoading(true);
      const steps = nextSteps()
        .split('\n')
        .map(s => s.trim())
        .filter(s => s.length > 0);

      await invoke('end_session', { nextSteps: steps });
      setCurrentSession(null);
      setNextSteps('');
      setViewMode('list');
      await loadSessions();
    } catch (error) {
      console.error('Failed to end session:', error);
      props.onError?.(String(error));
    } finally {
      setLoading(false);
    }
  };

  const resumeSession = async (sessionId: string) => {
    try {
      setLoading(true);
      const session = await invoke<Session>('resume_session', { sessionId });
      setCurrentSession(session);
      setViewMode('list');
      await loadSessions();
    } catch (error) {
      console.error('Failed to resume session:', error);
      props.onError?.(String(error));
    } finally {
      setLoading(false);
    }
  };

  const searchSessions = async () => {
    if (!searchQuery().trim()) {
      await loadSessions();
      return;
    }

    try {
      setLoading(true);
      const results = await invoke<Session[]>('search_sessions', { query: searchQuery() });
      setSessionHistory(results);
    } catch (error) {
      console.error('Failed to search sessions:', error);
      props.onError?.(String(error));
    } finally {
      setLoading(false);
    }
  };

  const formatTimestamp = (timestamp: string) => {
    const date = new Date(timestamp);
    return date.toLocaleString();
  };

  const getIntentEmoji = (intent: SessionIntent) => {
    const emojiMap: Record<SessionIntent['type'], string> = {
      Research: '🔬',
      Writing: '✍️',
      Coding: '💻',
      Learning: '📚',
      Planning: '📋',
      Debugging: '🐛',
      Brainstorming: '💡',
      Other: '📌',
    };
    return emojiMap[intent.type] || '📌';
  };

  const getActivityEmoji = (type: SessionActivity['activity_type']) => {
    const emojiMap: Record<SessionActivity['activity_type'], string> = {
      DocumentView: '📄',
      NoteTaking: '📝',
      AiChat: '💬',
      WebResearch: '🌐',
      Coding: '💻',
      Writing: '✍️',
      Thinking: '🧠',
      Other: '📌',
    };
    return emojiMap[type] || '📌';
  };

  return (
    <div class="sessions-container">
      <header class="sessions-header">
        <div>
          <h1>Sessions</h1>
          <p class="subtitle">Your work organized by time and thinking</p>
        </div>
        <button class="btn-close" onClick={props.onClose} title="Close (Escape)">
          ✕
        </button>
      </header>

      {/* View Mode Toggle */}
      <div class="view-toggle">
        <button
          class={viewMode() === 'list' ? 'active' : ''}
          onClick={() => setViewMode('list')}
        >
          📋 All Sessions
        </button>
        <Show when={currentSession()}>
          <button
            class={viewMode() === 'detail' && selectedSession()?.id === currentSession()?.id ? 'active' : ''}
            onClick={() => {
              setSelectedSession(currentSession()!);
              setViewMode('detail');
            }}
          >
            🎯 Current Session
          </button>
        </Show>
        <button
          class={viewMode() === 'new' ? 'active' : ''}
          onClick={() => setViewMode('new')}
        >
          ➕ New Session
        </button>
        <button
          class={viewMode() === 'backfill' ? 'active' : ''}
          onClick={() => setViewMode('backfill')}
        >
          ⏪ Backfill Session
        </button>
      </div>

      {/* List View */}
      <Show when={viewMode() === 'list'}>
        <div class="sessions-list-view">
          {/* Current Session Card */}
          <Show when={currentSession()}>
            <div class="session-card current">
              <div class="session-header">
                <span class="session-status live">● LIVE</span>
                <h2>{currentSession()!.title}</h2>
                <button
                  class="btn-end-session"
                  onClick={() => {
                    setSelectedSession(currentSession()!);
                    setViewMode('detail');
                  }}
                >
                  View Details
                </button>
              </div>
              <div class="session-meta">
                <span>{getIntentEmoji(currentSession()!.intent)} {currentSession()!.intent.type}</span>
                <span>Started {formatTimestamp(currentSession()!.started_at)}</span>
                <span>{currentSession()!.activities.length} activities</span>
              </div>
            </div>
          </Show>

          {/* Import Button */}
          <Show when={sessionHistory().length === 0}>
            <div class="import-prompt">
              <div class="empty-state">
                <h3>No sessions yet</h3>
                <p>Import your existing Claude Code, Cursor, and ChatGPT conversations as sessions.</p>
                <div style="display: flex; flex-direction: column; gap: 12px;">
                  <div style="display: flex; gap: 8px;">
                    <button
                      class="btn-primary"
                      onClick={importClaudeCodeSessions}
                      disabled={loading()}
                      style="flex: 2;"
                    >
                      {loading() ? 'Importing...' : '🚀 Import Claude Code Sessions (600)'}
                    </button>
                  </div>
                  <div style="display: flex; gap: 8px;">
                    <button
                      class="btn-primary"
                      onClick={syncFromAndor}
                      disabled={loading()}
                      style="flex: 1;"
                    >
                      {loading() ? 'Syncing...' : '☁️ Sync from Andor'}
                    </button>
                    <button
                      class="btn-primary"
                      onClick={importChatGPT}
                      disabled={loading()}
                      style="flex: 1;"
                    >
                      {loading() ? 'Importing...' : '💬 Import ChatGPT'}
                    </button>
                  </div>
                  <div style="display: flex; gap: 8px;">
                    <button
                      class="btn-secondary"
                      onClick={importClaudeCode}
                      disabled={loading()}
                      style="flex: 1;"
                    >
                      {loading() ? 'Scanning...' : '📂 Scan Repos'}
                    </button>
                    <button
                      class="btn-secondary"
                      onClick={importConversations}
                      disabled={loading()}
                      style="flex: 1;"
                    >
                      {loading() ? 'Importing...' : '📥 Import from DB'}
                    </button>
                  </div>
                  <div style="display: flex; gap: 8px;">
                    <button
                      class="btn-secondary"
                      onClick={debugListObjects}
                      style="font-size: 12px; padding: 6px 12px;"
                    >
                      🔍 Debug Objects
                    </button>
                  </div>
                </div>
                <p class="hint">
                  <strong>Import Sources:</strong>
                  <br />• Claude Code: ~/.claude/projects/ (~600 sessions)
                  <br />• Andor: Cloud backup (preserves before 2-month deletion)
                  <br />• ChatGPT: Already imported conversations
                  <br />• Scan Repos: For .claude folders in projects
                </p>
              </div>
            </div>
          </Show>

          {/* Filters */}
          <div class="filters-section">
            <Show when={hasActiveFilters()}>
              <div class="active-filters">
                <span class="filters-label">Active Filters:</span>
                <Show when={filterIntent() !== 'all'}>
                  <span class="filter-chip">
                    Intent: {filterIntent()}
                    <button onClick={() => setFilterIntent('all')} title="Clear">✕</button>
                  </span>
                </Show>
                <Show when={filterProvider() !== 'all'}>
                  <span class="filter-chip">
                    Provider: {filterProvider()}
                    <button onClick={() => setFilterProvider('all')} title="Clear">✕</button>
                  </span>
                </Show>
                <Show when={filterDateFrom() || filterDateTo()}>
                  <span class="filter-chip">
                    {filterDateFrom() ? 'From ' + filterDateFrom() : ''}
                    {filterDateFrom() && filterDateTo() ? ' - ' : ''}
                    {filterDateTo() ? 'To ' + filterDateTo() : ''}
                    <button onClick={() => { setFilterDateFrom(''); setFilterDateTo(''); }} title="Clear">✕</button>
                  </span>
                </Show>
                <Show when={searchQuery()}>
                  <span class="filter-chip">
                    Search: "{searchQuery()}"
                    <button onClick={() => setSearchQuery('')} title="Clear">✕</button>
                  </span>
                </Show>
                <button class="clear-filters-btn" onClick={clearFilters}>
                  Clear All
                </button>
              </div>
            </Show>

            <div class="filters-row">
              <div class="filter-group">
                <label>Intent:</label>
                <select value={filterIntent()} onInput={(e) => setFilterIntent(e.currentTarget.value as any)}>
                  <option value="all">All Types</option>
                  <option value="Coding">💻 Coding</option>
                  <option value="Research">🔬 Research</option>
                  <option value="Writing">✍️ Writing</option>
                  <option value="Learning">📚 Learning</option>
                  <option value="Planning">📋 Planning</option>
                  <option value="Debugging">🐛 Debugging</option>
                  <option value="Brainstorming">💡 Brainstorming</option>
                </select>
              </div>

              <div class="filter-group">
                <label>AI Provider:</label>
                <select value={filterProvider()} onInput={(e) => setFilterProvider(e.currentTarget.value)}>
                  <option value="all">All Providers</option>
                  <option value="Claude">Claude</option>
                  <option value="ChatGPT">ChatGPT</option>
                  <option value="Codex">Codex</option>
                  <option value="Cursor">Cursor</option>
                  <option value="Local">Local</option>
                </select>
              </div>

              <div class="filter-group">
                <label>From:</label>
                <input
                  type="date"
                  value={filterDateFrom()}
                  onInput={(e) => setFilterDateFrom(e.currentTarget.value)}
                />
              </div>

              <div class="filter-group">
                <label>To:</label>
                <input
                  type="date"
                  value={filterDateTo()}
                  onInput={(e) => setFilterDateTo(e.currentTarget.value)}
                />
              </div>
            </div>

            <div class="search-bar">
              <input
                type="text"
                placeholder="Search sessions by meaning..."
                value={searchQuery()}
                onInput={(e) => setSearchQuery(e.currentTarget.value)}
              />
              <button onClick={() => {}}>🔍</button>
            </div>
          </div>

          {/* Session History */}
          <div class="session-history">
            <div class="session-history-header">
              <h3>Past Sessions {hasActiveFilters() ? `(${filteredSessions().length} filtered)` : ''}</h3>
              <Show when={hasActiveFilters()}>
                <button class="btn-secondary btn-small" onClick={clearFilters}>
                  Show All ({sessionHistory().length} total)
                </button>
              </Show>
            </div>
            <Show
              when={filteredSessions().length > 0}
              fallback={
                <div class="empty-state">
                  {hasActiveFilters()
                    ? "No sessions match your filters. Try adjusting your search criteria."
                    : "No sessions yet. Start your first session!"}
                </div>
              }
            >
              <div class="sessions-list">
                <For each={filteredSessions()}>
                  {(session) => (
                    <div
                      class="session-card"
                      onClick={() => {
                        setSelectedSession(session);
                        setViewMode('detail');
                      }}
                    >
                      <div class="session-header">
                        <h3>{getIntentEmoji(session.intent)} {session.title}</h3>
                        <Show when={session.ended_at}>
                          <span class="session-duration">
                            {new Date(session.started_at).toLocaleDateString()}
                          </span>
                        </Show>
                      </div>
                      <Show when={session.description}>
                        <p class="session-description">{session.description}</p>
                      </Show>
                      <div class="session-meta">
                        <span>{session.activities.length} activities</span>
                        <Show when={session.next_steps.length > 0}>
                          <span>{session.next_steps.length} next steps</span>
                        </Show>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </div>
      </Show>

      {/* New Session Form */}
      <Show when={viewMode() === 'new'}>
        <div class="new-session-form">
          <h2>Start New Session</h2>
          <p class="form-hint">What are you trying to accomplish?</p>

          <div class="form-group">
            <label>Title</label>
            <input
              type="text"
              placeholder="e.g., Research paper on climate change"
              value={newTitle()}
              onInput={(e) => setNewTitle(e.currentTarget.value)}
            />
          </div>

          <div class="form-group">
            <label>Intent</label>
            <select value={newIntentType()} onInput={(e) => setNewIntentType(e.currentTarget.value as SessionIntent['type'])}>
              <option value="Research">🔬 Research</option>
              <option value="Writing">✍️ Writing</option>
              <option value="Coding">💻 Coding</option>
              <option value="Learning">📚 Learning</option>
              <option value="Planning">📋 Planning</option>
              <option value="Debugging">🐛 Debugging</option>
              <option value="Brainstorming">💡 Brainstorming</option>
              <option value="Other">📌 Other</option>
            </select>
          </div>

          <div class="form-group">
            <label>
              {newIntentType() === 'Research' && 'Research Topic'}
              {newIntentType() === 'Writing' && 'Project'}
              {newIntentType() === 'Coding' && 'Project'}
              {newIntentType() === 'Learning' && 'Subject'}
              {newIntentType() === 'Planning' && 'Goal'}
              {newIntentType() === 'Debugging' && 'Issue'}
              {newIntentType() === 'Brainstorming' && 'Theme'}
              {newIntentType() === 'Other' && 'Description'}
            </label>
            <input
              type="text"
              placeholder={
                newIntentType() === 'Research' ? 'e.g., Climate change effects on agriculture' :
                newIntentType() === 'Writing' ? 'e.g., Novel chapter 5' :
                newIntentType() === 'Coding' ? 'e.g., marlos-rust' :
                newIntentType() === 'Learning' ? 'e.g., Rust programming' :
                newIntentType() === 'Planning' ? 'e.g., Plan Q1 roadmap' :
                newIntentType() === 'Debugging' ? 'e.g., Memory leak in renderer' :
                newIntentType() === 'Brainstorming' ? 'e.g., AI education ideas' :
                'Describe what you\'re working on'
              }
              value={newIntentTopic()}
              onInput={(e) => setNewIntentTopic(e.currentTarget.value)}
            />
          </div>

          <div class="form-group">
            <label>Description (optional)</label>
            <textarea
              placeholder="Additional context about this session..."
              value={newDescription()}
              onInput={(e) => setNewDescription(e.currentTarget.value)}
              rows={3}
            />
          </div>

          <div class="form-actions">
            <button class="btn-secondary" onClick={() => setViewMode('list')}>
              Cancel
            </button>
            <button
              class="btn-primary"
              onClick={startSession}
              disabled={loading() || !newTitle().trim()}
            >
              {loading() ? 'Starting...' : '▶ Start Session'}
            </button>
          </div>
        </div>
      </Show>

      {/* Backfill Session Form */}
      <Show when={viewMode() === 'backfill'}>
        <div class="new-session-form">
          <h2>⏪ Backfill Session</h2>
          <p class="form-hint">Create a session for work you did in the past</p>

          <div class="form-group">
            <label>Title</label>
            <input
              type="text"
              placeholder="e.g., Fixed authentication bug"
              value={backfillTitle()}
              onInput={(e) => setBackfillTitle(e.currentTarget.value)}
            />
          </div>

          <div class="form-group">
            <label>Intent</label>
            <select value={backfillIntentType()} onInput={(e) => setBackfillIntentType(e.currentTarget.value as SessionIntent['type'])}>
              <option value="Research">🔬 Research</option>
              <option value="Writing">✍️ Writing</option>
              <option value="Coding">💻 Coding</option>
              <option value="Learning">📚 Learning</option>
              <option value="Planning">📋 Planning</option>
              <option value="Debugging">🐛 Debugging</option>
              <option value="Brainstorming">💡 Brainstorming</option>
              <option value="Other">📌 Other</option>
            </select>
          </div>

          <div class="form-group">
            <label>
              {backfillIntentType() === 'Research' && 'Research Topic'}
              {backfillIntentType() === 'Writing' && 'Project'}
              {backfillIntentType() === 'Coding' && 'Project'}
              {backfillIntentType() === 'Learning' && 'Subject'}
              {backfillIntentType() === 'Planning' && 'Goal'}
              {backfillIntentType() === 'Debugging' && 'Issue'}
              {backfillIntentType() === 'Brainstorming' && 'Theme'}
              {backfillIntentType() === 'Other' && 'Description'}
            </label>
            <input
              type="text"
              placeholder="What did you work on?"
              value={backfillIntentTopic()}
              onInput={(e) => setBackfillIntentTopic(e.currentTarget.value)}
            />
          </div>

          <div class="form-row">
            <div class="form-group">
              <label>Start Time</label>
              <input
                type="datetime-local"
                value={backfillStartTime()}
                onInput={(e) => setBackfillStartTime(e.currentTarget.value)}
              />
            </div>
            <div class="form-group">
              <label>End Time</label>
              <input
                type="datetime-local"
                value={backfillEndTime()}
                onInput={(e) => setBackfillEndTime(e.currentTarget.value)}
              />
            </div>
          </div>

          <div class="form-group">
            <label>Description (optional)</label>
            <textarea
              placeholder="What did you accomplish?"
              value={backfillDescription()}
              onInput={(e) => setBackfillDescription(e.currentTarget.value)}
              rows={3}
            />
          </div>

          <div class="form-group">
            <label>Activities Summary (optional)</label>
            <textarea
              placeholder="Describe what you did (e.g., 'Fixed 3 bugs, wrote documentation, had 2 AI chats about the architecture')"
              value={backfillActivities()}
              onInput={(e) => setBackfillActivities(e.currentTarget.value)}
              rows={3}
            />
          </div>

          <div class="form-actions">
            <button class="btn-secondary" onClick={() => setViewMode('list')}>
              Cancel
            </button>
            <button
              class="btn-primary"
              onClick={createBackfillSession}
              disabled={loading() || !backfillTitle().trim()}
            >
              {loading() ? 'Creating...' : '✓ Create Backfilled Session'}
            </button>
          </div>
        </div>
      </Show>

      {/* Session Detail View */}
      <Show when={viewMode() === 'detail' && selectedSession()}>
        <div class="session-detail">
          <div class="detail-header">
            <button class="btn-back" onClick={() => setViewMode('list')}>
              ← Back
            </button>
            <h2>{selectedSession()!.title}</h2>
            <Show when={selectedSession()!.id === currentSession()?.id}>
              <span class="session-status live">● LIVE</span>
            </Show>
          </div>

          <div class="detail-meta">
            <span>{getIntentEmoji(selectedSession()!.intent)} {selectedSession()!.intent.type}</span>
            <span>Started {formatTimestamp(selectedSession()!.started_at)}</span>
            <Show when={selectedSession()!.ended_at}>
              <span>Ended {formatTimestamp(selectedSession()!.ended_at!)}</span>
            </Show>
          </div>

          <Show when={selectedSession()!.description}>
            <div class="detail-section">
              <h3>Description</h3>
              <p>{selectedSession()!.description}</p>
            </div>
          </Show>

          {/* Activities */}
          <div class="detail-section">
            <h3>Activities ({selectedSession()!.activities.length})</h3>
            <Show
              when={selectedSession()!.activities.length > 0}
              fallback={<div class="empty-state">No activities recorded yet</div>}
            >
              <div class="activities-list">
                <For each={selectedSession()!.activities}>
                  {(activity) => (
                    <div class="activity-item">
                      <div class="activity-header">
                        <span class="activity-icon">{getActivityEmoji(activity.activity_type)}</span>
                        <span class="activity-type">{activity.activity_type}</span>
                        <span class="activity-time">{formatTimestamp(activity.timestamp)}</span>
                      </div>
                      <Show when={activity.details.ai_chat}>
                        <div class="activity-details">
                          <p><strong>Provider:</strong> {activity.details.ai_chat!.provider}</p>
                          <p><strong>Topic:</strong> {activity.details.ai_chat!.topic}</p>
                          <p><strong>Messages:</strong> {activity.details.ai_chat!.message_count}</p>
                        </div>
                      </Show>
                      <Show when={activity.details.took_notes}>
                        <div class="activity-details">
                          <p class="notes-content">{activity.details.took_notes!.content}</p>
                        </div>
                      </Show>
                      <Show when={activity.details.coding}>
                        <div class="activity-details">
                          <p><strong>Language:</strong> {activity.details.coding!.language}</p>
                          <p><strong>Files:</strong> {activity.details.coding!.files_modified.join(', ')}</p>
                        </div>
                      </Show>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>

          {/* Snapshots */}
          <Show when={selectedSession()!.snapshots.length > 0}>
            <div class="detail-section">
              <h3>Snapshots ({selectedSession()!.snapshots.length})</h3>
              <div class="snapshots-list">
                <For each={selectedSession()!.snapshots}>
                  {(snapshot) => (
                    <div class="snapshot-item">
                      <span class="snapshot-time">{formatTimestamp(snapshot.timestamp)}</span>
                      <p><strong>Focus:</strong> {snapshot.current_focus}</p>
                      <Show when={snapshot.mental_state}>
                        <div class="mental-state">
                          <span>Energy: {snapshot.mental_state!.energy}/10</span>
                          <span>Focus: {snapshot.mental_state!.focus}/10</span>
                          <span>Mood: {snapshot.mental_state!.mood}</span>
                        </div>
                      </Show>
                    </div>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Next Steps */}
          <Show when={selectedSession()!.next_steps.length > 0}>
            <div class="detail-section">
              <h3>Next Steps</h3>
              <ul class="next-steps-list">
                <For each={selectedSession()!.next_steps}>
                  {(step) => <li>{step}</li>}
                </For>
              </ul>
            </div>
          </Show>

          {/* End Session Button */}
          <Show when={selectedSession()!.id === currentSession()?.id}>
            <div class="detail-section">
              <h3>End Session</h3>
              <div class="end-session-form">
                <textarea
                  placeholder="What are your next steps? (one per line)"
                  value={nextSteps()}
                  onInput={(e) => setNextSteps(e.currentTarget.value)}
                  rows={4}
                />
                <button
                  class="btn-end-session"
                  onClick={endSession}
                  disabled={loading()}
                >
                  {loading() ? 'Ending...' : '⏹ End Session'}
                </button>
              </div>
            </div>
          </Show>

          {/* Resume Button */}
          <Show when={selectedSession()!.id !== currentSession()?.id && selectedSession()!.ended_at}>
            <div class="detail-section">
              <button
                class="btn-resume"
                onClick={() => resumeSession(selectedSession()!.id)}
                disabled={loading()}
              >
                {loading() ? 'Resuming...' : '▶ Resume This Session'}
              </button>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

export default Sessions;
