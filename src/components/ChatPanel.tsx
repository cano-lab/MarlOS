import { Component, createSignal, createEffect, onMount, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./ChatPanel.css";

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

// Predefined AI tasks
const AI_TASKS = [
  { id: "intent_map", label: "Intent Map", icon: "🎯", description: "Extract themes and structure" },
  { id: "summarize", label: "Summarize", icon: "📝", description: "Concise summary" },
  { id: "explain", label: "Explain", icon: "💡", description: "Clear explanation" },
  { id: "brainstorm", label: "Brainstorm", icon: "🧠", description: "Generate ideas" },
  { id: "critique", label: "Critique", icon: "🔍", description: "Find gaps and issues" },
  { id: "action_items", label: "Actions", icon: "✅", description: "Extract TODOs" },
];

interface ChatPanelProps {
  /** Optional content to include as context (e.g., selected text) */
  contextContent?: string;
  /** Callback when panel requests to be closed */
  onClose?: () => void;
}

const ChatPanel: Component<ChatPanelProps> = (props) => {
  const [messages, setMessages] = createSignal<Message[]>([]);
  const [inputText, setInputText] = createSignal("");
  const [isLoading, setIsLoading] = createSignal(false);
  const [isAvailable, setIsAvailable] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [showSettings, setShowSettings] = createSignal(false);
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
    // Check if AI provider is available
    await checkStatus();
    // Load config
    await loadConfig();
    // Add welcome message
    addSystemMessage("Chat ready. Type a message or use the quick actions above.");
  });

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

  const sendMessage = async () => {
    const text = inputText().trim();
    if (!text || isLoading()) return;

    // Build message with optional context
    let fullMessage = text;
    if (props.contextContent) {
      fullMessage = `[Context]\n${props.contextContent}\n[/Context]\n\n${text}`;
    }

    addUserMessage(text);
    setInputText("");
    setIsLoading(true);
    setError(null);

    try {
      // Build chat history for context
      const chatMessages = messages()
        .filter(m => m.role !== "system")
        .map(m => ({ role: m.role, content: m.content }));

      chatMessages.push({ role: "user", content: fullMessage });

      const response = await invoke<AiResponse>("ai_chat", {
        messages: chatMessages,
        systemPrompt: null,
      });

      addAssistantMessage(response.content);
    } catch (e) {
      setError(`${e}`);
      addSystemMessage(`Error: ${e}`);
    } finally {
      setIsLoading(false);
      inputRef?.focus();
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

  const formatMessage = (content: string) => {
    // Simple markdown-like formatting
    return content
      .replace(/\*\*(.*?)\*\*/g, "<strong>$1</strong>")
      .replace(/\*(.*?)\*/g, "<em>$1</em>")
      .replace(/`(.*?)`/g, "<code>$1</code>")
      .replace(/\n/g, "<br>");
  };

  return (
    <div class="chat-panel">
      {/* Header */}
      <div class="chat-header">
        <div class="chat-title">
          <span class="chat-icon">🤖</span>
          <span>Unstuck</span>
          <span class={`status-dot ${isAvailable() ? "connected" : "disconnected"}`}
                title={isAvailable() ? "Connected" : "Not connected"} />
        </div>
        <div class="chat-actions">
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

      {/* Quick actions */}
      <div class="quick-actions">
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
            <div class="typing-indicator">
              <span></span><span></span><span></span>
            </div>
          </div>
        </Show>
        <div ref={messagesEndRef} />
      </div>

      {/* Error display */}
      <Show when={error()}>
        <div class="error-banner">
          {error()}
          <button onClick={() => setError(null)}>✕</button>
        </div>
      </Show>

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
          placeholder={isAvailable() ? "Ask anything... (Ctrl+Enter to send)" : "AI not connected. Check settings."}
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
    </div>
  );
};

export default ChatPanel;
