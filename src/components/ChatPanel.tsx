import { Component, createSignal, onMount, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./ChatPanel.css";
import ThinkingDebugger from "./ThinkingDebugger";
import { useLearningSession, LearningStep, webSearch, academicSearch, formatSearchResultsForContext, formatAcademicResultsForContext, SearchResults, AcademicSearchResults } from "../hooks/useLearningSession";
import { useTTS } from "../services/tts-service";
import { useVoiceClone } from "../services/voice-clone-service";
import VoiceCloneSettings from "./VoiceCloneSettings";

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
  { id: "research", label: "Research", icon: "🔬", description: "Search web & papers, then summarize" },
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
  "/research": { description: "Search web & papers, summarize findings", handler: "research" },
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
  // showHelp signal reserved for future use
  const [availableModels, setAvailableModels] = createSignal<string[]>([]);

  // Learning mode state
  const learning = useLearningSession();
  const [topicInput, setTopicInput] = createSignal("");
  const [predictionInput, setPredictionInput] = createSignal("");
  const [predictionType, setPredictionType] = createSignal<typeof PREDICTION_TYPES[number]["id"]>("guess");
  const [integrationInput, setIntegrationInput] = createSignal("");
  const [useWebSearch, setUseWebSearch] = createSignal(true);
  const [useAcademicSearch, setUseAcademicSearch] = createSignal(true);
  const [isSearching, setIsSearching] = createSignal(false);
  const [, setSearchResults] = createSignal<string>("");
  const [webSearchData, setWebSearchData] = createSignal<SearchResults | null>(null);
  const [academicSearchData, setAcademicSearchData] = createSignal<AcademicSearchResults | null>(null);
  const [searchError] = createSignal<string | null>(null);

  const [config, setConfig] = createSignal<ProviderConfig>({
    name: "LM Studio",
    base_url: "http://localhost:4321/v1",
    api_key: null,
    model: null,
    temperature: 0.7,
    max_tokens: 2048,
    timeout_secs: 300,
  });

  let messagesEndRef: HTMLDivElement | undefined;
  let inputRef: HTMLTextAreaElement | undefined;

  // TTS
  const tts = useTTS();
  const voiceClone = useVoiceClone();
  const [speakingIndex, setSpeakingIndex] = createSignal<number | null>(null);
  const [showVoiceSettings, setShowVoiceSettings] = createSignal(false);
  // Voice clone synthesis progress
  const [voicePhase, setVoicePhase] = createSignal<"idle" | "loading_model" | "synthesizing">(
    "idle"
  );
  const [voiceElapsed, setVoiceElapsed] = createSignal(0);
  let voiceTimerId: number | null = null;

  const startVoiceTimer = (phase: "loading_model" | "synthesizing") => {
    setVoicePhase(phase);
    setVoiceElapsed(0);
    if (voiceTimerId !== null) clearInterval(voiceTimerId);
    voiceTimerId = window.setInterval(() => setVoiceElapsed((s) => s + 1), 1000);
  };

  const stopVoiceTimer = () => {
    if (voiceTimerId !== null) {
      clearInterval(voiceTimerId);
      voiceTimerId = null;
    }
    setVoicePhase("idle");
    setVoiceElapsed(0);
  };

  const stripMarkdown = (md: string): string =>
    md
      .replace(/```[\s\S]*?```/g, " ")
      .replace(/`([^`]*)`/g, "$1")
      .replace(/!\[[^\]]*\]\([^)]*\)/g, " ")
      .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
      .replace(/[#>*_~]/g, " ")
      .replace(/\s+/g, " ")
      .trim();

  const handleSpeak = async (idx: number, content: string) => {
    const cloneReady = voiceClone.status()?.has_reference === true;

    // Toggle off if already speaking this message
    if (speakingIndex() === idx) {
      if (cloneReady) voiceClone.stop();
      else if (tts.status().status === "playing") tts.stop();
      setSpeakingIndex(null);
      return;
    }

    setSpeakingIndex(idx);
    try {
      const text = stripMarkdown(content);
      if (cloneReady) {
        // Pick the right phase based on whether the model is already loaded.
        // status() reflects last refresh; if model_loaded is false treat as first call.
        const modelLoaded = voiceClone.status()?.model_loaded === true;
        startVoiceTimer(modelLoaded ? "synthesizing" : "loading_model");
        try {
          await voiceClone.speak(text);
          // Refresh status so subsequent calls show "synthesizing" instead of "loading"
          await voiceClone.refresh();
        } finally {
          stopVoiceTimer();
        }
      } else {
        await tts.speak(text);
      }
    } catch (e) {
      console.error("Speak failed:", e);
    } finally {
      setSpeakingIndex(null);
    }
  };

  onMount(async () => {
    // Load custom providers (may set config if active provider exists)
    await loadCustomProviders();

    // Check if AI provider is available
    await checkStatus();

    // Only load generic config if no custom provider is active
    if (!activeProviderId()) {
      await loadConfig();
    }

    // Add welcome message
    addSystemMessage("Chat ready. Type a message or use the quick actions above.");
  });

  const loadCustomProviders = async () => {
    try {
      const providers = await invoke<CustomProvider[]>("custom_provider_list");
      setCustomProviders(providers);
      const activeId = await invoke<string | null>("custom_provider_get_active_id");
      setActiveProviderId(activeId);

      // Load active provider config if available
      if (activeId) {
        const activeProvider = await invoke<CustomProvider | null>("custom_provider_get", { id: activeId });
        if (activeProvider) {
          const newConfig: ProviderConfig = {
            name: activeProvider.name,
            base_url: activeProvider.base_url,
            api_key: activeProvider.api_key,
            model: activeProvider.model,
            temperature: activeProvider.temperature,
            max_tokens: activeProvider.max_tokens,
            timeout_secs: activeProvider.timeout_secs,
          };
          setConfig(newConfig);
          await invoke("ai_set_config", { config: newConfig });
        }
      }
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

      // Fetch available models for the dropdown
      if (available) {
        try {
          const models = await invoke<string[]>("ai_list_models");
          setAvailableModels(models);
        } catch {
          setAvailableModels([]);
        }
      }
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

    if (cmd === "/write" && args) {
      addUserMessage(text);
      setIsLoading(true);
      try {
        // Parse path and content from args
        const writeParts = args.split(" ", 2);
        const writePath = writeParts[0];
        const writeContent = writeParts[1] || "";

        if (!writePath) {
          addSystemMessage("Usage: /write <path> [content]");
          setIsLoading(false);
          return true;
        }

        // Write to file using Tauri command
        const result = await invoke<string>("write_file_content", {
          path: writePath,
          contents: writeContent
        });

        addFileOperation("write", writePath, `Wrote ${writeContent.length} characters`);
        addSystemMessage(result);
      } catch (e) {
        addSystemMessage(`Could not write file: ${e}`);
      } finally {
        setIsLoading(false);
      }
      return true;
    }

    if (cmd === "/research") {
      if (!args) {
        addSystemMessage("Usage: /research <topic> — searches web + academic papers and synthesizes findings");
        return true;
      }
      await runResearch(args);
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
      // Build message — keep it lean for local models
      let fullMessage = text;
      if (props.contextContent) {
        // Truncate document context to avoid overwhelming the model
        const truncated = props.contextContent.length > 2000
          ? props.contextContent.slice(0, 2000) + "\n...(truncated)"
          : props.contextContent;
        fullMessage = `[Document Context]\n${truncated}\n[/Document Context]\n\n${text}`;
      }

      // Build chat history — limit to last 10 exchanges to keep context manageable
      const allMessages = messages().filter(m => m.role !== "system");
      const recentMessages = allMessages.slice(-20); // last 10 user+assistant pairs
      const chatMessages = recentMessages.map(m => ({ role: m.role, content: m.content }));
      chatMessages.push({ role: "user", content: fullMessage });

      // Compact system prompt — avoid bloating context for small models
      let systemPrompt = `You are an UNSTUCK assistant. Help the user clarify thinking, get unstuck on problems, and suggest next steps. Be direct and actionable.`;

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

  const runResearch = async (query?: string) => {
    const topic = query || inputText().trim() || props.contextContent?.slice(0, 200);
    if (!topic) {
      addSystemMessage("Enter a topic to research, or select text first.");
      return;
    }

    setInputText("");
    addUserMessage(`[Research] ${topic}`);
    setIsLoading(true);
    setError(null);

    try {
      // Run web + academic search in parallel
      addSystemMessage("Searching web and academic databases...");
      const [webResults, academicResults] = await Promise.all([
        webSearch(topic, 5),
        academicSearch(topic, 8),
      ]);

      // Build source summary for display
      let sourceSummary = "";
      if (webResults.results.length > 0) {
        sourceSummary += `**Web Sources (${webResults.results.length}):**\n`;
        for (const r of webResults.results) {
          sourceSummary += `- [${r.title}](${r.url}) — ${r.snippet.slice(0, 120)}\n`;
        }
        sourceSummary += "\n";
      }
      if (academicResults.papers.length > 0) {
        sourceSummary += `**Academic Papers (${academicResults.papers.length}):**\n`;
        for (const p of academicResults.papers) {
          const authors = p.authors.slice(0, 2).join(", ") + (p.authors.length > 2 ? " et al." : "");
          const year = p.year ? ` (${p.year})` : "";
          const cites = p.citation_count ? ` [${p.citation_count} citations]` : "";
          sourceSummary += `- **${p.title}**${year} — ${authors}${cites}\n`;
          if (p.abstract_text) {
            sourceSummary += `  ${p.abstract_text.slice(0, 150)}...\n`;
          }
        }
      }

      if (!sourceSummary) {
        addAssistantMessage("No results found. Try rephrasing your search topic.");
        return;
      }

      // Now ask AI to synthesize
      addSystemMessage("Synthesizing findings...");
      const searchContext = formatSearchResultsForContext(webResults) + formatAcademicResultsForContext(academicResults);

      const response = await invoke<AiResponse>("ai_chat", {
        messages: [{ role: "user", content: `Research topic: ${topic}\n\n${searchContext}\n\nSynthesize these sources into a clear summary. Cite specific papers/sources. Highlight key findings, areas of consensus, and open questions.` }],
        systemPrompt: `You are a research assistant. The user wants to learn about a topic. You have been given real search results from the web and academic databases. Your job is to:
1. Synthesize the findings into a clear, structured summary
2. Cite specific sources by name and author
3. Highlight key findings and areas of consensus
4. Note any contradictions or open questions
5. Suggest what to read first if the user wants to go deeper

Be thorough but accessible. Use the actual search results — don't make up information.`,
      });

      // Show sources first, then synthesis
      addAssistantMessage(`${sourceSummary}\n---\n\n**Synthesis:**\n${response.content}`);
    } catch (e) {
      setError(`Research failed: ${e}`);
      addSystemMessage(`Error: ${e}`);
    } finally {
      setIsLoading(false);
    }
  };

  const runTask = async (taskId: string) => {
    // Handle research task specially
    if (taskId === "research") {
      return runResearch();
    }

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

      // Clear previous search results
      setWebSearchData(null);
      setAcademicSearchData(null);

      const doSearch = useWebSearch() || useAcademicSearch();
      if (doSearch) {
        setIsSearching(true);
      }

      try {
        // Run searches in parallel for speed
        const searchPromises: Promise<void>[] = [];

        if (useWebSearch()) {
          searchPromises.push(
            webSearch(topic, 5).then(results => {
              setWebSearchData(results);
              searchContext += formatSearchResultsForContext(results);
            })
          );
        }

        if (useAcademicSearch()) {
          searchPromises.push(
            academicSearch(topic, 8).then(results => {
              setAcademicSearchData(results);
              searchContext += formatAcademicResultsForContext(results);
            })
          );
        }

        await Promise.all(searchPromises);
      } catch (err) {
        console.warn("Search failed:", err);
      } finally {
        setIsSearching(false);
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
          <button
            class="icon-btn"
            onClick={() => setShowVoiceSettings(!showVoiceSettings())}
            title={
              voiceClone.status()?.has_reference
                ? "Voice cloning ready — manage reference"
                : "Set up voice cloning"
            }
          >
            🎙️
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

      {/* Voice cloning settings panel */}
      <Show when={showVoiceSettings()}>
        <div class="voice-settings-overlay">
          <VoiceCloneSettings onClose={() => setShowVoiceSettings(false)} />
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
              placeholder="http://localhost:4321/v1"
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
            <label>Model {availableModels().length > 0 ? `(${availableModels().length} available)` : "(auto-detect)"}</label>
            <Show when={availableModels().length > 0} fallback={
              <input
                type="text"
                value={config().model || ""}
                onInput={(e) => setConfig({ ...config(), model: e.currentTarget.value || null })}
                placeholder="Auto-detect (connects to check models)"
              />
            }>
              <select
                value={config().model || ""}
                onChange={(e) => setConfig({ ...config(), model: e.currentTarget.value || null })}
              >
                <option value="">Auto-detect (pick best chat model)</option>
                <For each={availableModels()}>
                  {(model) => <option value={model}>{model}</option>}
                </For>
              </select>
            </Show>
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
            {(msg, idx) => (
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
                  <Show when={msg.role === "assistant" && msg.content}>
                    <button
                      class="message-speak-btn"
                      onClick={() => handleSpeak(idx(), msg.content)}
                      disabled={
                        tts.status().status === "loading" ||
                        tts.status().status === "synthesizing"
                      }
                      title={
                        speakingIndex() === idx() && tts.status().status === "playing"
                          ? "Stop speaking"
                          : tts.status().status === "loading"
                          ? `Loading TTS (${Math.round((tts.status().progress || 0) * 100)}%)`
                          : tts.status().status === "synthesizing"
                          ? "Synthesizing..."
                          : "Read aloud"
                      }
                    >
                      {speakingIndex() === idx() && tts.status().status === "playing"
                        ? "⏹"
                        : tts.status().status === "loading" && speakingIndex() === idx()
                        ? "⏳"
                        : tts.status().status === "synthesizing" && speakingIndex() === idx()
                        ? "…"
                        : "🔊"}
                    </button>
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

        {/* Voice synthesis progress */}
        <Show when={voicePhase() !== "idle"}>
          <div class={`voice-progress voice-progress-${voicePhase()}`}>
            <span class="voice-progress-spinner" />
            <span class="voice-progress-label">
              <Show
                when={voicePhase() === "loading_model"}
                fallback={<>Synthesizing voice...</>}
              >
                Loading voice model... <span class="voice-progress-hint">first call only, ~30s</span>
              </Show>
            </span>
            <span class="voice-progress-elapsed">{voiceElapsed()}s</span>
          </div>
        </Show>

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
                <button type="submit" class="learn-btn" disabled={!topicInput().trim() || isSearching()}>
                  {isSearching() ? "Searching..." : "Start Learning"}
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

                <Show when={webSearchData() || academicSearchData() || searchError()}>
                  <div class="search-results-panel">
                    <h4>🔍 Search Results</h4>

                    {/* Search error */}
                    <Show when={searchError()}>
                      <div class="search-results-section error-message">
                        <h5 class="error-title">Search Error</h5>
                        <div class="error-text">{searchError()}</div>
                      </div>
                    </Show>
                    <Show when={webSearchData()}>
                      <div class="search-results-section">
                        <h5>Web Search</h5>
                        <For each={webSearchData()!.results}>
                          {(result) => (
                            <div class="search-result-item">
                              <a
                                href={result.url}
                                target="_blank"
                                rel="noopener noreferrer"
                                class="search-result-title"
                              >
                                {result.title}
                              </a>
                              <div class="search-result-url">{result.source_domain}</div>
                              <div class="search-result-snippet">{result.snippet}</div>
                            </div>
                          )}
                        </For>
                      </div>
                    </Show>

                    {/* Academic search results */}
                    <Show when={academicSearchData()}>
                      <div class="search-results-section">
                        <h5>Academic Papers</h5>
                        <For each={academicSearchData()!.papers}>
                          {(paper) => (
                            <div class="search-result-item">
                              <Show when={paper.url}>
                                <a
                                  href={paper.url}
                                  target="_blank"
                                  rel="noopener noreferrer"
                                  class="search-result-title"
                                >
                                  {paper.title}
                                </a>
                              </Show>
                              <Show when={!paper.url}>
                                <div class="search-result-title">{paper.title}</div>
                              </Show>
                              <Show when={paper.authors && paper.authors.length > 0}>
                                <div class="search-result-authors">
                                  {paper.authors.slice(0, 3).join(", ")}
                                  {paper.authors.length > 3 ? " et al." : ""}
                                </div>
                              </Show>
                              <Show when={paper.year}>
                                <div class="search-result-meta">{paper.year}</div>
                              </Show>
                              <Show when={paper.venue}>
                                <div class="search-result-meta">{paper.venue}</div>
                              </Show>
                              <Show when={paper.abstract_text}>
                                <div class="search-result-snippet">{paper.abstract_text!.slice(0, 200)}...</div>
                              </Show>
                            </div>
                          )}
                        </For>
                      </div>
                    </Show>
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
