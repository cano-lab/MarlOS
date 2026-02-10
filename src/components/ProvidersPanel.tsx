import { Component, createSignal, For, Show, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./ProvidersPanel.css";

// AI Service (LLM provider) configuration
interface AIService {
  id: string;
  name: string;
  base_url: string;
  api_key: string | null;
  model: string | null;
  temperature: number;
  max_tokens: number;
  timeout_secs: number;
}

// System provider from ProviderRegistry
// Note: category is snake_case from Rust serde serialization
interface SystemProvider {
  name: string;
  category: "always_on" | "implied" | "on_demand";
}

interface ProvidersPanelProps {
  onOpenSettings?: () => void;
}

// Known system providers (these are always available)
const SYSTEM_PROVIDERS: SystemProvider[] = [
  { name: "word_count", category: "always_on" },
  { name: "markdown_language_service", category: "always_on" },
  { name: "coding_agent", category: "on_demand" },
];

// Importers available
const IMPORTERS = [
  { id: "chatgpt", name: "ChatGPT", description: "Import conversations from ChatGPT export" },
  { id: "cursor", name: "Cursor", description: "Import from Cursor AI sessions" },
  { id: "obsidian", name: "Obsidian", description: "Import notes from Obsidian vault" },
  { id: "claude-code", name: "Claude Code", description: "Import Claude Code conversations" },
];

const ProvidersPanel: Component<ProvidersPanelProps> = (props) => {
  // AI Services state
  const [aiServices, setAIServices] = createSignal<AIService[]>([]);
  const [activeServiceId, setActiveServiceId] = createSignal<string | null>(null);
  const [aiLoading, setAILoading] = createSignal(true);
  const [testingId, setTestingId] = createSignal<string | null>(null);
  const [serviceStatus, setServiceStatus] = createSignal<Record<string, boolean>>({});

  const loadAIServices = async () => {
    try {
      const list = await invoke<AIService[]>("custom_provider_list");
      setAIServices(list);

      const activeId = await invoke<string | null>("custom_provider_get_active_id");
      setActiveServiceId(activeId);
    } catch (e) {
      console.error("Failed to load AI services:", e);
    } finally {
      setAILoading(false);
    }
  };

  onMount(() => {
    loadAIServices();
  });

  const handleSetActiveService = async (id: string) => {
    try {
      await invoke("custom_provider_set_active", { id });
      setActiveServiceId(id);
    } catch (e) {
      console.error("Failed to set active AI service:", e);
    }
  };

  const handleTestService = async (id: string) => {
    setTestingId(id);
    try {
      const isAvailable = await invoke<boolean>("custom_provider_test", { id });
      setServiceStatus((prev) => ({ ...prev, [id]: isAvailable }));
    } catch {
      setServiceStatus((prev) => ({ ...prev, [id]: false }));
    } finally {
      setTestingId(null);
    }
  };

  const handleImport = async (importerId: string) => {
    switch (importerId) {
      case "chatgpt":
        try {
          await invoke("import_chatgpt_export", {});
        } catch (e) {
          console.error("ChatGPT import failed:", e);
        }
        break;
      case "claude-code":
        try {
          await invoke("import_claude_code_conversations", {});
        } catch (e) {
          console.error("Claude Code import failed:", e);
        }
        break;
      default:
        console.log("Import not implemented for:", importerId);
    }
  };

  const formatProviderName = (name: string): string => {
    return name
      .split("_")
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join(" ");
  };

  const getCategoryLabel = (category: string): string => {
    switch (category) {
      case "always_on": return "Active";
      case "implied": return "Background";
      case "on_demand": return "On Demand";
      default: return category;
    }
  };

  const getCategoryClass = (category: string): string => {
    switch (category) {
      case "always_on": return "always-on";
      case "implied": return "implied";
      case "on_demand": return "on-demand";
      default: return "";
    }
  };

  const getStatusClass = (id: string): string => {
    if (testingId() === id) return "testing";
    return serviceStatus()[id] ? "online" : "offline";
  };

  return (
    <div class="providers-panel">
      {/* System Providers Section */}
      <div class="providers-section">
        <h3 class="section-title">SYSTEM PROVIDERS</h3>
        <div class="system-provider-list">
          <For each={SYSTEM_PROVIDERS}>
            {(provider) => (
              <div class="system-provider-item">
                <span class="provider-name">{formatProviderName(provider.name)}</span>
                <span class={`category-badge ${getCategoryClass(provider.category)}`}>
                  {getCategoryLabel(provider.category)}
                </span>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* AI Services Section */}
      <div class="providers-section">
        <div class="section-header">
          <h3 class="section-title">AI SERVICES</h3>
          <button
            class="add-provider-btn"
            onClick={() => props.onOpenSettings?.()}
            title="Configure AI services"
          >
            +
          </button>
        </div>

        <Show when={!aiLoading()} fallback={<div class="loading">Loading...</div>}>
          <div class="provider-list">
            <For each={aiServices()}>
              {(service) => (
                <button
                  class={`provider-item ${activeServiceId() === service.id ? "active" : ""}`}
                  onClick={() => handleSetActiveService(service.id)}
                >
                  <span class={`status-indicator ${getStatusClass(service.id)}`} />
                  <div class="provider-info">
                    <span class="provider-name">{service.name}</span>
                    <span class="provider-url">
                      {service.base_url.replace(/^https?:\/\//, "").replace(/\/v1$/, "")}
                    </span>
                  </div>
                  <button
                    class="test-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleTestService(service.id);
                    }}
                    disabled={testingId() === service.id}
                    title="Test connection"
                  >
                    {testingId() === service.id ? "..." : "Test"}
                  </button>
                </button>
              )}
            </For>
          </div>

          <Show when={aiServices().length === 0}>
            <div class="empty-state">No AI services configured</div>
          </Show>

          <button class="configure-btn" onClick={() => props.onOpenSettings?.()}>
            Configure AI Services
          </button>
        </Show>
      </div>

      {/* Importers Section */}
      <div class="providers-section">
        <h3 class="section-title">IMPORTERS</h3>
        <div class="importer-list">
          <For each={IMPORTERS}>
            {(importer) => (
              <div class="importer-item">
                <div class="importer-info">
                  <span class="importer-name">{importer.name}</span>
                </div>
                <button
                  class="import-btn"
                  onClick={() => handleImport(importer.id)}
                  title={importer.description}
                >
                  Import
                </button>
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  );
};

export default ProvidersPanel;
