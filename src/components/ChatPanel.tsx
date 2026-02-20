import { Component, createSignal, createEffect, onMount, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./ChatPanel.css";
import ThinkingDebugger from "./ThinkingDebugger";
import { useLearningSession, LearningStep, webSearch, academicSearch, formatSearchResultsForContext, formatAcademicResultsForContext } from "../hooks/useLearningSession";

type ChatMode = "chat" | "learn";

const PREDICTION_TYPES = [
  { id: "confident", label: "I'm fairly confident", emoji: "💪" },
  { id: "partial", label: "I have a partial idea", emoji: "🤔" },
  { id: "guess", label: "This is a guess", emoji: "🎲" },
  { id: "no_idea", label: "I don't know", emoji: "🌱" },
] as const;

const STEP_INFO: Record<LearningStep, { title: string; description: string }> = {
  topic: {
    title: "What do you want to learn?",
    description: "Ask a question or pick a topic to explore",
  },
  predict: {
    title: "Make a Prediction",
    description: "Before seeing the answer, what do you think? Any guess counts.",
  },
  answer: {
    title: "The Answer",
    description: "Here's what's actually true. Compare with your prediction.",
  },
  compare: {
    title: "Prediction Analysis",
    description: "Let's see what your prediction reveals about your mental model.",
  },
  integrate: {
    title: "What Did You Learn?",
    description: "Summarize the key insight in your own words.",
  },
  next: {
    title: "Keep Going",
    description: "Your updated model reveals new questions. Pick one or ask your own.",
  },
};

interface Message {
  role: "user" | "assistant" | "system";
  content: string;
  timestamp?: Date;
}

interface AiResponse {
  content: string;
  model: string;
  tokens_used: number | null;
  finish_reason: string | null;
}

interface ProviderConfig {
  name: string;
  base_url: string;
  api_key: string | null;
  model: string | null;
  temperature: number;
  max_tokens: number;
  timeout_secs: number;
}

interface CustomProvider {
  id: string;
  name: string;
  base_url: string;
  api_key: string | null;
  model: string | null;
  temperature: number;
  max_tokens: number;
  timeout_secs: number;
}

// Predefined AI tasks
const AI_TASKS = [
  { id: "intent_map", label: "Intent Map", icon: "🎯", description: "Extract themes and structure" },
  { id: "summarize", label: "Summarize", icon: "📝", description: "Concise summary" },
  { id: "explain", label: "Explain", icon: "💡", description: "Clear explanation" },
  { id: "brainstorm", label: "Brainstorm", icon: "🧠", description: "Generate ideas" },
  { id: "critique", label: "Critique", icon: "🔍", description: "Find gaps and issues" },
  { id: "action_items", label: "Actions", icon: "✅", description: "Extract TODOs" },
];

// Slash commands for power users
const SLASH_COMMANDS: Record<string, { description: string; handler: string }> = {
  "/read": { description: "Read a file", handler: "file_read" },
  "/write": { description: "Write to a file", handler: "file_write" },
  "/ingest": { description: "Ingest a file into context", handler: "file_ingest" },
  "/ls": { description: "List files", handler: "file_list" },
  "/search": { description: "Search semantic memory", handler: "semantic_search" },
  "/help": { description: "Show available commands", handler: "help" },
};

interface FileOperation {
  operation: string;
  filePath: string;
  details?: string;
  timestamp: Date;
}

interface ChatPanelProps {
  /** Optional content to include as context (e.g., selected text) */
  contextContent?: string;
  /** Callback when panel requests to be closed */
  onClose?: () => void;
  /** Whether to render in full view mode (takes up main content area) */
  fullView?: boolean;
  /** Discovery/search state and handlers */
  showDiscovery?: boolean;
  onToggleDiscovery?: () => void;
}

const ChatPanel: Component<ChatPanelProps> = (props) => {
  // Mode toggle: chat or learn
  const [mode, setMode] = createSignal<ChatMode>("chat");

  // Discovery/search handlers and state
  const [showDiscovery, setShowDiscovery] = createSignal(false);
  const [discoveryQuery, setDiscoveryQuery] = createSignal("");
  const [discoveryResults, setDiscoveryResults] = createSignal<Array<{
    name: string;
    summary: string;
    score: number;
    source_type?: string;
    url: string;
    authors?: string[];
    year?: number;
    pdf_url?: string;
    doi?: string;
    venue?: string;
    citations?: number;
  }>>([]);
  const [isDiscovering, setIsDiscovering] = createSignal(false);

  const toggleDiscovery = () => setShowDiscovery(!showDiscovery());
  const discoverSources = async () => {
    const query = discoveryQuery().trim();
    if (!query) return;

    setIsDiscovering(true);
    setDiscoveryResults(null);
    setError(null);

    try {
      // Use web search by default
      const results = await invoke<Array<{ name: string; summary: string; score: number }>>(
        "semantic_search",
        {
          query,
          limit: 10
        }
      );

      setDiscoveryResults({
        sources: results.map(r => ({
          name: r.name,
          summary: r.summary,
          score: r.score,
          url: r.url,
          source_type: "web"
        }))
      });
    } catch (e) {
      setError(`Search failed: ${e}`);
    } finally {
      setIsDiscovering(false);
    }
  };

  // Chat mode state
  const [messages, setMessages] = createSignal<Message[]>([]);
  const [inputText, setInputText] = createSignal("");
  const [isLoading, setIsLoading] = createSignal(false);
  const [isAvailable, setIsAvailable] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [showSettings, setShowSettings] = createSignal(false);
  const [showDebugger, setShowDebugger] = createSignal(false);
  const [showProviderPicker, setShowProviderPicker] = createSignal(false);
  const [customProviders, setCustomProviders] = createSignal<CustomProvider[]>([]);
  const [activeProviderId, setActiveProviderId] = createSignal<string | null>(null);
  const [fileOperations, setFileOperations] = createSignal<FileOperation[]>([]);
  const [abortController, setAbortController] = createSignal<AbortController | null>(null);
  const [showHelp, setShowHelp] = createSignal(false);

  // Learning mode state
  const learning = useLearningSession();
  const [topicInput, setTopicInput] = createSignal("");
  const [predictionInput, setPredictionInput] = createSignal("");
  const [predictionType, setPredictionType] = createSignal<typeof PREDICTION_TYPES[number]["id"]>("guess");
  const [integrationInput, setIntegrationInput] = createSignal("");
  const [useWebSearch, setUseWebSearch] = createSignal(false);
  const [useAcademicSearch, setUseAcademicSearch] = createSignal(false);
  const [searchResults, setSearchResults] = createSignal<string>("");

  const [config, setConfig] = createSignal<ProviderConfig>({
    name: "LM Studio",
    base_url: "http://localhost:1234/v1",
    api_key: null,
    model: null,
    temperature: 0.7,
    max_tokens: 2048,
    timeout_secs: 60,
  });

  let messagesEndRef: HTMLDivElement | undefined;
  let inputRef: HTMLTextAreaElement | undefined;

  onMount(async () => {
    // Load custom providers
    await loadCustomProviders();
    // Check if AI provider is available
    await checkStatus();
    // Load config
    await loadConfig();
    // Add welcome message
    addSystemMessage("Chat ready. Type a message or use the quick actions above.");
  });

  const loadCustomProviders = async () => {
    try {
      const providers = await invoke<CustomProvider[]>("custom_provider_list");
      setCustomProviders(providers);
      const activeId = await invoke<string | null>("custom_provider_get_active_id");
      setActiveProviderId(activeId);
    } catch (e) {
      console.debug("Custom providers not available:", e);
    }
  };

  const handleSwitchProvider = async (providerId: string) => {
    try {
      // Set active provider
      await invoke("custom_provider_set_active", { id: providerId });
      setActiveProviderId(providerId);

      // Get provider config and apply it
      const provider = customProviders().find(p => p.id === providerId);
      if (provider) {
        const newConfig: ProviderConfig = {
          name: provider.name,
          base_url: provider.base_url,
          api_key: provider.api_key,
          model: provider.model,
          temperature: provider.temperature,
          max_tokens: provider.max_tokens,
          timeout_secs: provider.timeout_secs,
        };
        await invoke("ai_set_config", { config: newConfig });
        setConfig(newConfig);
        await checkStatus();
        addSystemMessage(`Switched to ${provider.name}`);
      }
      setShowProviderPicker(false);
    } catch (e) {
      setError(`Failed to switch provider: ${e}`);
    }
  };

  const checkStatus = async () => {
    try {
      const available = await invoke<boolean>("ai_check_status");
      setIsAvailable(available);
    } catch (e) {
      setIsAvailable(false);
    }
  };

  const loadConfig = async () => {
    try {
      const cfg = await invoke<ProviderConfig>("ai_get_config");
      setConfig(cfg);
    } catch (e) {
      console.error("Failed to load config:", e);
    }
  };

  const saveConfig = async () => {
    try {
      await invoke("ai_set_config", { config: config() });
      setShowSettings(false);
      await checkStatus();
      addSystemMessage("Settings saved. Provider status: " + (isAvailable() ? "Connected" : "Not available"));
    } catch (e) {
      setError(`Failed to save config: ${e}`);
    }
  };

  const addSystemMessage = (content: string) => {
    setMessages([...messages(), { role: "system", content, timestamp: new Date() }]);
    scrollToBottom();
  };

  const addUserMessage = (content: string) => {
    setMessages([...messages(), { role: "user", content, timestamp: new Date() }]);
    scrollToBottom();
  };

  const addAssistantMessage = (content: string) => {
    setMessages([...messages(), { role: "assistant", content, timestamp: new Date() }]);
    scrollToBottom();
  };

  const scrollToBottom = () => {
    setTimeout(() => {
      messagesEndRef?.scrollIntoView({ behavior: "smooth" });
    }, 100);
  };

  const handleSlashCommand = async (text: string): Promise<boolean> => {
    const parts = text.split(" ", 2);
    const cmd = parts[0].toLowerCase();
    const args = parts.length > 1 ? text.substring(parts[0].length + 1) : "";

    if (!(cmd in SLASH_COMMANDS)) return false;

    const command = SLASH_COMMANDS[cmd];

    if (cmd === "/help") {
      let helpText = "**Available Commands:**\n\n";
      for (const [c, info] of Object.entries(SLASH_COMMANDS)) {
        helpText += `\`${c}\` - ${info.description}\n`;
      }
      helpText += "\n**Quick Buttons:** Use the buttons above for common actions.";
      helpText += "\n**Context:** Selected text is automatically included.";
      addSystemMessage(helpText);
      return true;
    }

    if (cmd === "/search" && args) {
      addUserMessage(text);
      setIsLoading(true);
      try {
        const results = await invoke<Array<{ name: string; summary: string; score: number }>>(
          "semantic_search",
          { query: args, limit: 5 }
        );
        if (results.length > 0) {
          let resultText = `**Search Results for "${args}":**\n\n`;
          for (const r of results) {
            resultText += `**${r.name}** (${(r.score * 100).toFixed(0)}%)\n${r.summary}\n\n`;
          }
          addAssistantMessage(resultText);
        } else {
          addSystemMessage("No results found.");
        }
      } catch (e) {
        addSystemMessage(`Search error: ${e}`);
      } finally {
        setIsLoading(false);
      }
      return true;
    }

    if (cmd === "/read" && args) {
      addUserMessage(text);
      setIsLoading(true);
      try {
        const content = await invoke<string>("read_file_content", { path: args });
        addFileOperation("read", args);
        addAssistantMessage(`**File: ${args}**\n\n\`\`\`\n${content}\n\`\`\``);
      } catch (e) {
        addSystemMessage(`Could not read file: ${e}`);
      } finally {
        setIsLoading(false);
      }
      return true;
    }

    if (cmd === "/ls") {
      addUserMessage(text);
      setIsLoading(true);
      try {
        const dir = args || ".";
        const files = await invoke<string[]>("list_directory", { path: dir });
        addAssistantMessage(`**Files in ${dir}:**\n\n${files.join("\n")}`);
      } catch (e) {
        addSystemMessage(`Could not list directory: ${e}`);
      } finally {
        setIsLoading(false);
      }
      return true;
    }

    if (cmd === "/ingest" && args) {
      addUserMessage(text);
      setIsLoading(true);
      try {
        await invoke("ingest_file_to_memory", { path: args });
        addFileOperation("ingest", args, "File stored in semantic memory");
        addSystemMessage(`File "${args}" ingested into semantic memory.`);
      } catch (e) {
        addSystemMessage(`Could not ingest file: ${e}`);
      } finally {
        setIsLoading(false);
      }
      return true;
    }

    addSystemMessage(`Command "${cmd}" requires arguments. Type /help for usage.`);
    return true;
  };

  const addFileOperation = (operation: string, filePath: string, details?: string) => {
    setFileOperations([
      ...fileOperations(),
      { operation, filePath, details, timestamp: new Date() },
    ]);
  };

  const cancelRequest = () => {
    const controller = abortController();
    if (controller) {
      controller.abort();
      setAbortController(null);
      setIsLoading(false);
      addSystemMessage("Request cancelled.");
    }
  };

  const sendMessage = async () => {
    const text = inputText().trim();
    if (!text || isLoading()) return;

    // Check for slash commands first
    if (text.startsWith("/")) {
      setInputText("");
      const handled = await handleSlashCommand(text);
      if (handled) return;
    }

    addUserMessage(text);
    setInputText("");
    setIsLoading(true);
    setError(null);

    // Create abort controller for cancellation
    const controller = new AbortController();
    setAbortController(controller);

    try {
      // Fetch session context if available
      let sessionContext = "";
      try {
        sessionContext = await invoke<string>("get_session_context");
      } catch (e) {
        // No active session or error fetching context - that's fine
        console.debug("No session context available:", e);
      }

      // Fetch relevant semantic memory automatically
      let semanticContext = "";
      try {
        const results = await invoke<Array<{ name: string; summary: string; score: number }>>(
          "semantic_search",
          { query: text, limit: 3 }
        );
        if (results.length > 0) {
          semanticContext = results
            .filter(r => r.score > 0.3)
            .map(r => `[${r.name}]: ${r.summary}`)
            .join("\n");
        }
      } catch (e) {
        console.debug("Semantic search not available:", e);
      }

      // Build message with optional context
      let fullMessage = text;
      if (props.contextContent) {
        fullMessage = `[Document Context]\n${props.contextContent}\n[/Document Context]\n\n${text}`;
      }
      if (semanticContext) {
        fullMessage = `[Relevant Memory]\n${semanticContext}\n[/Relevant Memory]\n\n${fullMessage}`;
      }
      if (sessionContext && !sessionContext.includes("No active session")) {
        fullMessage = `[Session Context]\n${sessionContext}\n[/Session Context]\n\n${fullMessage}`;
      }

      // Build chat history for context
      const chatMessages = messages()
        .filter(m => m.role !== "system")
        .map(m => ({ role: m.role, content: m.content }));

      chatMessages.push({ role: "user", content: fullMessage });

      // System prompt with session query capabilities
      let systemPrompt = `You are an UNSTUCK assistant with access to the user's session history.

The user has work sessions stored in MarlOS. You can query these sessions to provide better context and help.

Available Session Commands:
- get_session_context: Get current active session info
- get_session_history: List all past sessions
- get_session_details(session_id): Get full details of a specific session
- get_sessions_by_provider(provider, limit): Get sessions from a specific AI (e.g., "Claude", "ChatGPT", "Codex")
- get_recent_context(days): Get summary of recent work (default 7 days)
- search_sessions(query): Search sessions by content

When helpful, ask the user if they want to check their past sessions for relevant context.`;

      // Fetch recent context automatically for better AI responses
      // Use vector-based analysis (fast, no LLM needed)
      try {
        const recentContext = await invoke<string>("get_session_vector_analysis", { days: 7 });
        if (recentContext && !recentContext.includes("No sessions in")) {
          systemPrompt += `\n\nRecent Work Context:\n${recentContext}`;
        }
      } catch (e) {
        console.debug("Could not fetch recent context:", e);
      }

      const response = await invoke<AiResponse>("ai_chat", {
        messages: chatMessages,
        systemPrompt: systemPrompt,
      });

      addAssistantMessage(response.content);

      // Record this AI chat activity in the session
      try {
        await invoke("record_session_activity", {
          activityType: "ai_chat",
          details: {
            provider: config().name,
            topic: text.slice(0, 100), // First 100 chars as topic summary
            messageCount: messages().filter(m => m.role !== "system").length / 2 + 1,
            summary: response.content.slice(0, 200), // First 200 chars as summary
          }
        });
      } catch (e) {
        console.debug("Failed to record activity:", e);
      }
    } catch (e) {
      setError(`${e}`);
      addSystemMessage(`Error: ${e}`);
    } finally {
      setIsLoading(false);
      setAbortController(null);
      inputRef?.focus();
    }
  };

  const injectSessionContext = async () => {
    try {
      addSystemMessage("Analyzing session patterns...");
      // Use vector-based analysis (fast, scalable) instead of LLM
      const analysis = await invoke<string>("get_session_vector_analysis", { days: 7 });
      addUserMessage(`Show me my recent work patterns (last 7 days)`);
      addAssistantMessage(analysis);
    } catch (e) {
      addSystemMessage(`Could not fetch session analysis: ${e}`);
    }
  };

  const runTask = async (taskId: string) => {
    const content = props.contextContent || inputText().trim();
    if (!content) {
      addSystemMessage("Please select some text or enter content to analyze.");
      return;
    }

    const task = AI_TASKS.find(t => t.id === taskId);
    addUserMessage(`[${task?.label}] Analyzing content...`);
    setIsLoading(true);
    setError(null);

    try {
      const response = await invoke<AiResponse>("ai_run_task", {
        task: taskId,
        content,
      });

      addAssistantMessage(response.content);
    } catch (e) {
      setError(`${e}`);
      addSystemMessage(`Error: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const clearChat = () => {
    setMessages([]);
    addSystemMessage("Chat cleared. Ready for new conversation.");
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      sendMessage();
    }
  };

  // Learning mode handlers
  const handleTopicSubmit = async (e: Event) => {
    e.preventDefault();
    const topic = topicInput().trim();
    if (topic) {
      let searchContext = "";

      if (useWebSearch()) {
        const results = await webSearch(topic, 5);
        searchContext += formatSearchResultsForContext(results);
      }

      if (useAcademicSearch()) {
        const results = await academicSearch(topic, 5);
        searchContext += formatAcademicResultsForContext(results);
      }

      setSearchResults(searchContext);
      learning.setTopic(topic, searchContext);
      setTopicInput("");
    }
  };

  const handlePredictionSubmit = (e: Event) => {
    e.preventDefault();
    const prediction = predictionInput().trim();
    if (prediction || predictionType() === "no_idea") {
      learning.submitPrediction(
        prediction || "I don't know - please help me build a mental model",
        predictionType()
      );
      setPredictionInput("");
    }
  };

  const handleIntegrationSubmit = (e: Event) => {
    e.preventDefault();
    const integration = integrationInput().trim();
    if (integration) {
      learning.submitIntegration(integration);
      setIntegrationInput("");
    }
  };

  const currentStep = () => learning.state().step;
  const currentCycle = () => learning.state().currentCycle;
  const learningLoading = () => learning.state().isLoading;
  const learningError = () => learning.state().error;
  const history = () => learning.state().history;

  // Escape HTML to prevent XSS attacks
  const escapeHtml = (text: string): string => {
    const htmlEscapes: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    };
    return text.replace(/[&<>"']/g, (char) => htmlEscapes[char]);
  };

  const formatMessage = (content: string) => {
    // SECURITY: Escape HTML first to prevent XSS, then apply markdown formatting
    const escaped = escapeHtml(content);
    return escaped
      .replace(/\*\*(.*?)\*\*/g, "<strong>$1</strong>")
      .replace(/\*(.*?)\*/g, "<em>$1</em>")
      .replace(/`(.*?)`/g, "<code>$1</code>")
      .replace(/\n/g, "<br>");
  };

  return (
    <div class={`chat-panel ${props.fullView ? "full-view" : ""}`}>
      {/* Header */}
      <div class="chat-header">
        <div class="chat-title">
          <span class="chat-icon">{mode() === "chat" ? "🤖" : "🧠"}</span>
          <span>{mode() === "chat" ? "Unstuck" : "Learn"}</span>
          <span class={`status-dot ${isAvailable() ? "connected" : "disconnected"}`}
                title={isAvailable() ? "Connected" : "Not connected"} />
        </div>
        <div class="mode-toggle">
          <button
            class={`mode-btn ${mode() === "chat" ? "active" : ""}`}
            onClick={() => setMode("chat")}
            title="Chat mode - ask anything"
          >
            Chat
          </button>
          <button
            class={`mode-btn ${mode() === "learn" ? "active" : ""}`}
            onClick={() => setMode("learn")}
            title="Learn mode - predict, test, integrate"
          >
            Learn
          </button>
        </div>
        <div class="chat-actions">
          <button
            class={`icon-btn ${showDebugger() ? "active" : ""}`}
            onClick={() => setShowDebugger(!showDebugger())}
            title="Debug My Thinking"
          >
            🔍
          </button>
          <button
            class={`icon-btn provider-picker-btn ${showProviderPicker() ? "active" : ""}`}
            onClick={() => setShowProviderPicker(!showProviderPicker())}
            title="Switch AI Service"
          >
            🔌
          </button>
          <button class="icon-btn" onClick={() => setShowSettings(!showSettings())} title="Settings">
            ⚙️
          </button>
          <button class="icon-btn" onClick={clearChat} title="Clear chat">
            🗑️
          </button>
          <Show when={props.onClose}>
            <button class="icon-btn" onClick={props.onClose} title="Close">
              ✕
            </button>
          </Show>
        </div>
      </div>

      {/* Provider Picker */}
      <Show when={showProviderPicker()}>
        <div class="provider-picker">
          <div class="provider-picker-header">
            <span>Select AI Service</span>
            <button class="close-picker" onClick={() => setShowProviderPicker(false)}>&times;</button>
          </div>
          <div class="provider-picker-list">
            <For each={customProviders()}>
              {(provider) => (
                <button
                  class={`provider-option ${activeProviderId() === provider.id ? "active" : ""}`}
                  onClick={() => handleSwitchProvider(provider.id)}
                >
                  <span class="provider-option-name">{provider.name}</span>
                  <span class="provider-option-url">
                    {provider.base_url.replace(/^https?:\/\//, "").replace(/\/v1$/, "")}
                  </span>
                  <Show when={activeProviderId() === provider.id}>
                    <span class="provider-active-badge">Active</span>
                  </Show>
                </button>
              )}
            </For>
            <Show when={customProviders().length === 0}>
              <div class="no-providers">No AI services configured</div>
            </Show>
          </div>
        </div>
      </Show>

      {/* Settings panel */}
      <Show when={showSettings()}>
        <div class="settings-panel">
          <div class="setting-row">
            <label>Provider URL</label>
            <input
              type="text"
              value={config().base_url}
              onInput={(e) => setConfig({ ...config(), base_url: e.currentTarget.value })}
              placeholder="http://localhost:1234/v1"
            />
          </div>
          <div class="setting-row">
            <label>API Key (optional)</label>
            <input
              type="password"
              value={config().api_key || ""}
              onInput={(e) => setConfig({ ...config(), api_key: e.currentTarget.value || null })}
              placeholder="sk-..."
            />
          </div>
          <div class="setting-row">
            <label>Model (optional)</label>
            <input
              type="text"
              value={config().model || ""}
              onInput={(e) => setConfig({ ...config(), model: e.currentTarget.value || null })}
              placeholder="Auto-detect"
            />
          </div>
          <div class="setting-row half">
            <div>
              <label>Temperature</label>
              <input
                type="number"
                min="0"
                max="2"
                step="0.1"
                value={config().temperature}
                onInput={(e) => setConfig({ ...config(), temperature: parseFloat(e.currentTarget.value) })}
              />
            </div>
            <div>
              <label>Max Tokens</label>
              <input
                type="number"
                min="100"
                max="8192"
                step="100"
                value={config().max_tokens}
                onInput={(e) => setConfig({ ...config(), max_tokens: parseInt(e.currentTarget.value) })}
              />
            </div>
          </div>
          <div class="setting-actions">
            <button class="btn-secondary" onClick={() => setShowSettings(false)}>Cancel</button>
            <button class="btn-primary" onClick={saveConfig}>Save</button>
          </div>
        </div>
      </Show>

      {/* Thinking Debugger */}
      <Show when={showDebugger()}>
        <div class="debugger-overlay">
          <ThinkingDebugger
            conversation={messages()
              .filter(m => m.role !== "system")
              .map(m => ({ role: m.role, content: m.content }))}
            onClose={() => setShowDebugger(false)}
          />
        </div>
      </Show>

      {/* CHAT MODE */}
      <Show when={mode() === "chat"}>
        {/* Quick actions */}
        <div class="quick-actions">
          <button
            class="quick-action-btn"
            onClick={injectSessionContext}
            disabled={isLoading()}
            title="Show your recent work context from sessions"
          >
            <span class="action-icon">📚</span>
            <span class="action-label">Sessions</span>
          </button>
          <For each={AI_TASKS}>
            {(task) => (
              <button
                class="quick-action-btn"
                onClick={() => runTask(task.id)}
                disabled={isLoading()}
                title={task.description}
              >
                <span class="action-icon">{task.icon}</span>
                <span class="action-label">{task.label}</span>
              </button>
            )}
          </For>
        </div>

        {/* Messages */}
        <div class="chat-messages">
          <For each={messages()}>
            {(msg) => (
              <div class={`message ${msg.role}`}>
                <div class="message-header">
                  <span class="message-role">
                    {msg.role === "user" ? "You" : msg.role === "assistant" ? "Assistant" : "System"}
                  </span>
                  <Show when={msg.timestamp}>
                    <span class="message-time">
                      {msg.timestamp?.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                    </span>
                  </Show>
                </div>
                <div class="message-content" innerHTML={formatMessage(msg.content)} />
              </div>
            )}
          </For>
          <Show when={isLoading()}>
            <div class="message assistant loading">
              <div class="loading-row">
                <div class="typing-indicator">
                  <span></span><span></span><span></span>
                </div>
                <button class="cancel-btn" onClick={cancelRequest} title="Cancel request">
                  Cancel
                </button>
              </div>
            </div>
          </Show>
          {/* File operations display */}
          <For each={fileOperations().slice(-3)}>
            {(op) => (
              <div class="file-operation">
                <span class="op-icon">
                  {op.operation === "read" ? "📖" : op.operation === "write" ? "✏️" : "📥"}
                </span>
                <span class="op-type">{op.operation.toUpperCase()}</span>
                <span class="op-path">{op.filePath}</span>
                <Show when={op.details}>
                  <span class="op-details">{op.details}</span>
                </Show>
              </div>
            )}
          </For>
          <div ref={messagesEndRef} />
        </div>

        {/* Error display */}
        <Show when={error()}>
          <div class="error-banner">
            {error()}
            <button onClick={() => setError(null)}>✕</button>
          </div>
        </Show>
      </Show>

      {/* LEARN MODE */}
      <Show when={mode() === "learn"}>
        <div class="learn-mode">
          {/* Progress indicator */}
          <div class="step-progress">
            <For each={Object.keys(STEP_INFO) as LearningStep[]}>
              {(step, index) => (
                <div
                  class={`step-dot ${currentStep() === step ? "active" : ""} ${
                    Object.keys(STEP_INFO).indexOf(currentStep()) > index() ? "complete" : ""
                  }`}
                  title={STEP_INFO[step].title}
                />
              )}
            </For>
          </div>

          {/* Current step info */}
          <div class="step-info">
            <h3>{STEP_INFO[currentStep()].title}</h3>
            <p>{STEP_INFO[currentStep()].description}</p>
          </div>

          {/* Learning history indicator */}
          <Show when={history().length > 0}>
            <div class="cycle-count">{history().length} cycles completed</div>
          </Show>

          {/* Error display */}
          <Show when={learningError()}>
            <div class="learning-error">{learningError()}</div>
          </Show>

          {/* Step content */}
          <div class="step-content">
            {/* TOPIC STEP */}
            <Show when={currentStep() === "topic"}>
              <form class="topic-form" onSubmit={handleTopicSubmit}>
                <textarea
                  class="learn-input"
                  placeholder="What do you want to understand? e.g., 'How does TCP handle packet loss?' or 'Explain quantum entanglement'"
                  value={topicInput()}
                  onInput={(e) => setTopicInput(e.currentTarget.value)}
                  rows={3}
                />
                <div class="search-options">
                  <label class="search-toggle">
                    <input
                      type="checkbox"
                      checked={useWebSearch()}
                      onChange={(e) => setUseWebSearch(e.currentTarget.checked)}
                    />
                    <span class="toggle-icon">🌐</span>
                    <span>Web Search</span>
                  </label>
                  <label class="search-toggle">
                    <input
                      type="checkbox"
                      checked={useAcademicSearch()}
                      onChange={(e) => setUseAcademicSearch(e.currentTarget.checked)}
                    />
                    <span class="toggle-icon">📚</span>
                    <span>Academic</span>
                  </label>
                </div>
                <button type="submit" class="learn-btn" disabled={!topicInput().trim()}>
                  Start Learning
                </button>
              </form>
            </Show>

            {/* PREDICT STEP */}
            <Show when={currentStep() === "predict"}>
              <div class="predict-section">
                <div class="topic-display">
                  <span class="label">Topic:</span>
                  <span class="value">{currentCycle().topic}</span>
                </div>

                <Show when={searchResults()}>
                  <div class="search-results-indicator">
                    🔍 Web/Academic search results will enhance the answer
                  </div>
                </Show>

                <form class="predict-form" onSubmit={handlePredictionSubmit}>
                  <div class="prediction-type-selector">
                    <For each={PREDICTION_TYPES}>
                      {(type) => (
                        <button
                          type="button"
                          class={`type-btn ${predictionType() === type.id ? "selected" : ""}`}
                          onClick={() => setPredictionType(type.id)}
                        >
                          <span class="emoji">{type.emoji}</span>
                          <span class="label">{type.label}</span>
                        </button>
                      )}
                    </For>
                  </div>

                  <Show when={predictionType() !== "no_idea"}>
                    <textarea
                      class="learn-input"
                      placeholder="What do you think the answer is? It's okay to be wrong - that's where learning happens."
                      value={predictionInput()}
                      onInput={(e) => setPredictionInput(e.currentTarget.value)}
                      rows={4}
                    />
                  </Show>

                  <Show when={predictionType() === "no_idea"}>
                    <div class="no-idea-message">
                      <p>That's completely fine! Not knowing is the honest starting point.</p>
                      <p>I'll build you a minimal mental model so you can make predictions next time.</p>
                    </div>
                  </Show>

                  <button
                    type="submit"
                    class="learn-btn"
                    disabled={predictionType() !== "no_idea" && !predictionInput().trim()}
                  >
                    {learningLoading() ? "Thinking..." : "Submit Prediction"}
                  </button>
                </form>
              </div>
            </Show>

            {/* ANSWER STEP */}
            <Show when={currentStep() === "answer"}>
              <div class="answer-section">
                <div class="your-prediction">
                  <span class="label">Your prediction:</span>
                  <p class="prediction-text">{currentCycle().prediction}</p>
                </div>

                <div class="answer-display">
                  <span class="label">The answer:</span>
                  <div class="answer-content">{currentCycle().answer}</div>
                </div>

                <button
                  class="learn-btn"
                  onClick={learning.requestComparison}
                  disabled={learningLoading()}
                >
                  {learningLoading() ? "Analyzing..." : "Analyze My Prediction"}
                </button>
              </div>
            </Show>

            {/* COMPARE STEP */}
            <Show when={currentStep() === "compare"}>
              <div class="compare-section">
                <div class="comparison-display">
                  <div class="comparison-content">{currentCycle().comparison}</div>
                </div>

                <form class="integration-form" onSubmit={handleIntegrationSubmit}>
                  <label class="integration-label">
                    In your own words, what's the key thing you learned?
                  </label>
                  <textarea
                    class="learn-input"
                    placeholder="The main insight I'm taking away is..."
                    value={integrationInput()}
                    onInput={(e) => setIntegrationInput(e.currentTarget.value)}
                    rows={3}
                  />
                  <button
                    type="submit"
                    class="learn-btn"
                    disabled={!integrationInput().trim() || learningLoading()}
                  >
                    {learningLoading() ? "Generating questions..." : "Continue"}
                  </button>
                </form>
              </div>
            </Show>

            {/* INTEGRATE/NEXT STEP */}
            <Show when={currentStep() === "integrate" || currentStep() === "next"}>
              <div class="next-section">
                <div class="integration-summary">
                  <span class="label">What you learned:</span>
                  <p>{currentCycle().integration}</p>
                </div>

                <Show when={currentCycle().followUpQuestions?.length}>
                  <div class="followup-questions">
                    <span class="label">Questions to explore next:</span>
                    <div class="question-list">
                      <For each={currentCycle().followUpQuestions}>
                        {(question) => (
                          <button
                            class="question-btn"
                            onClick={() => learning.startNewCycle(question)}
                          >
                            {question}
                          </button>
                        )}
                      </For>
                    </div>
                  </div>
                </Show>

                <div class="next-actions">
                  <button class="secondary-btn" onClick={() => learning.startNewCycle()}>
                    Ask Something New
                  </button>
                </div>
              </div>
            </Show>
          </div>

          {/* Reset button */}
          <Show when={currentStep() !== "topic"}>
            <button class="reset-btn" onClick={learning.reset}>
              Start Over
            </button>
          </Show>
        </div>
      </Show>

      {/* Chat mode: Context indicator and input area */}
      <Show when={mode() === "chat"}>
        {/* Context indicator */}
        <Show when={props.contextContent}>
          <div class="context-indicator">
            <span>📎 Context attached ({props.contextContent?.length} chars)</span>
          </div>
        </Show>

        {/* Input area */}
        <div class="chat-input-area">
          <textarea
            ref={inputRef}
            class="chat-input"
            value={inputText()}
            onInput={(e) => setInputText(e.currentTarget.value)}
            onKeyDown={handleKeyDown}
            placeholder={isAvailable() ? "Ask anything or /help for commands (Ctrl+Enter)" : "AI not connected. Check settings."}
            disabled={isLoading()}
            rows={3}
          />
          <button
            class="send-btn"
            onClick={sendMessage}
            disabled={isLoading() || !inputText().trim()}
          >
            {isLoading() ? "..." : "Send"}
          </button>
        </div>
      </Show>
    </div>
  );
};

export default ChatPanel;
