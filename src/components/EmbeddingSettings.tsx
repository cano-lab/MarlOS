import { Component, createSignal, createEffect, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./EmbeddingSettings.css";

// Types matching Rust structs
interface EmbeddingConfig {
  provider_type: "lm_studio" | "ollama" | "open_ai" | "mock";
  base_url: string;
  model: string;
  dimensions: number;
  max_tokens: number;
  api_key: string | null;
}

interface FieldWeights {
  name: number;
  summary: number;
  tags: number;
  content: number;
}

interface SearchConfig {
  min_score: number;
  keyword_boost: number;
  include_keyword: boolean;
  recency_decay: number;
  field_weights: FieldWeights;
}

interface EmbeddingModelPreset {
  name: string;
  model_id: string;
  dimensions: number;
  max_tokens: number;
  provider: "lm_studio" | "ollama" | "open_ai" | "mock";
  description: string;
}

interface EmbeddingSettingsProps {
  isOpen: boolean;
  onClose: () => void;
  onReindex?: () => void;
}

const PROVIDER_URLS: Record<string, string> = {
  lm_studio: "http://localhost:4321/v1",
  ollama: "http://localhost:11434",
  open_ai: "https://api.openai.com/v1",
  mock: "mock://",
};

const PROVIDER_LABELS: Record<string, string> = {
  lm_studio: "LM Studio",
  ollama: "Ollama",
  open_ai: "OpenAI",
  mock: "Mock (Testing)",
};

const EmbeddingSettings: Component<EmbeddingSettingsProps> = (props) => {
  // Embedding config state
  const [embeddingConfig, setEmbeddingConfig] = createSignal<EmbeddingConfig>({
    provider_type: "lm_studio",
    base_url: "http://localhost:4321/v1",
    model: "text-embedding-qwen3-embedding-0.6b",
    dimensions: 1024,
    max_tokens: 8192,
    api_key: null,
  });

  // Search config state
  const [searchConfig, setSearchConfig] = createSignal<SearchConfig>({
    min_score: 0.3,
    keyword_boost: 0.2,
    include_keyword: true,
    recency_decay: 7.0,
    field_weights: {
      name: 1.5,
      summary: 1.2,
      tags: 1.3,
      content: 1.0,
    },
  });

  const [presets, setPresets] = createSignal<EmbeddingModelPreset[]>([]);
  const [testStatus, setTestStatus] = createSignal<"idle" | "testing" | "success" | "error">("idle");
  const [error, setError] = createSignal<string | null>(null);
  const [needsReindex, setNeedsReindex] = createSignal(false);
  const [originalDimensions, setOriginalDimensions] = createSignal(1024);

  // Load config when modal opens
  createEffect(() => {
    if (props.isOpen) {
      loadConfig();
    }
  });

  const loadConfig = async () => {
    try {
      const [embedding, search, presetList] = await Promise.all([
        invoke<EmbeddingConfig>("embedding_get_config"),
        invoke<SearchConfig>("search_get_config"),
        invoke<EmbeddingModelPreset[]>("embedding_get_presets"),
      ]);
      setEmbeddingConfig(embedding);
      setSearchConfig(search);
      setPresets(presetList);
      setOriginalDimensions(embedding.dimensions);
      setNeedsReindex(false);
      setError(null);
    } catch (e) {
      console.error("Failed to load embedding config:", e);
      setError(`Failed to load configuration: ${e}`);
    }
  };

  const handleProviderChange = (provider: string) => {
    setEmbeddingConfig((prev) => ({
      ...prev,
      provider_type: provider as EmbeddingConfig["provider_type"],
      base_url: PROVIDER_URLS[provider] || prev.base_url,
    }));
  };

  const handlePresetSelect = (presetName: string) => {
    const preset = presets().find((p) => p.name === presetName);
    if (preset) {
      const newDimensions = preset.dimensions;
      const dimChanged = newDimensions !== originalDimensions();
      setNeedsReindex(dimChanged);

      setEmbeddingConfig((prev) => ({
        ...prev,
        provider_type: preset.provider,
        base_url: PROVIDER_URLS[preset.provider] || prev.base_url,
        model: preset.model_id,
        dimensions: newDimensions,
        max_tokens: preset.max_tokens,
      }));
    }
  };

  const handleDimensionsChange = (dims: number) => {
    setNeedsReindex(dims !== originalDimensions());
    setEmbeddingConfig((prev) => ({
      ...prev,
      dimensions: dims,
    }));
  };

  const handleTestConnection = async () => {
    setTestStatus("testing");
    try {
      const success = await invoke<boolean>("embedding_test_provider", {
        config: embeddingConfig(),
      });
      setTestStatus(success ? "success" : "error");
      setTimeout(() => setTestStatus("idle"), 3000);
    } catch (e) {
      console.error("Test failed:", e);
      setTestStatus("error");
      setTimeout(() => setTestStatus("idle"), 3000);
    }
  };

  const handleSave = async () => {
    try {
      await Promise.all([
        invoke("embedding_set_config", { config: embeddingConfig() }),
        invoke("search_set_config", { config: searchConfig() }),
      ]);

      if (needsReindex() && props.onReindex) {
        props.onReindex();
      }

      props.onClose();
    } catch (e) {
      setError(`Failed to save: ${e}`);
    }
  };

  const handleReset = async () => {
    if (!confirm("Reset all embedding and search settings to defaults?")) return;

    try {
      await invoke("embedding_reset_to_defaults");
      await loadConfig();
    } catch (e) {
      setError(`Failed to reset: ${e}`);
    }
  };

  const getTestButtonText = () => {
    switch (testStatus()) {
      case "testing": return "Testing...";
      case "success": return "Connected!";
      case "error": return "Failed";
      default: return "Test Connection";
    }
  };

  const filteredPresets = () => {
    const provider = embeddingConfig().provider_type;
    return presets().filter((p) => p.provider === provider);
  };

  return (
    <Show when={props.isOpen}>
      <div class="embedding-settings-overlay" onClick={props.onClose}>
        <div class="embedding-settings-modal" onClick={(e) => e.stopPropagation()}>
          <div class="modal-header">
            <h2>Embedding Settings</h2>
            <button class="close-btn" onClick={props.onClose}>&times;</button>
          </div>

          <Show when={error()}>
            <div class="error-banner">
              {error()}
              <button onClick={() => setError(null)}>&times;</button>
            </div>
          </Show>

          <Show when={needsReindex()}>
            <div class="warning-banner">
              Dimensions changed. Reindexing required after save.
            </div>
          </Show>

          <div class="modal-content">
            {/* Embedding Provider Section */}
            <section class="settings-section">
              <h3>EMBEDDING PROVIDER</h3>

              <div class="form-group">
                <label>Provider</label>
                <select
                  value={embeddingConfig().provider_type}
                  onChange={(e) => handleProviderChange(e.currentTarget.value)}
                >
                  <For each={Object.entries(PROVIDER_LABELS)}>
                    {([value, label]) => <option value={value}>{label}</option>}
                  </For>
                </select>
              </div>

              <div class="form-group">
                <label>URL</label>
                <input
                  type="text"
                  value={embeddingConfig().base_url}
                  onInput={(e) =>
                    setEmbeddingConfig((prev) => ({
                      ...prev,
                      base_url: e.currentTarget.value,
                    }))
                  }
                />
              </div>

              <div class="form-group">
                <label>Model</label>
                <select
                  value={embeddingConfig().model}
                  onChange={(e) => handlePresetSelect(
                    presets().find((p) => p.model_id === e.currentTarget.value)?.name || ""
                  )}
                >
                  <For each={filteredPresets()}>
                    {(preset) => (
                      <option value={preset.model_id}>
                        {preset.name} ({preset.dimensions}d)
                      </option>
                    )}
                  </For>
                  <option value={embeddingConfig().model}>
                    Custom: {embeddingConfig().model}
                  </option>
                </select>
              </div>

              <Show when={embeddingConfig().provider_type === "open_ai"}>
                <div class="form-group">
                  <label>API Key</label>
                  <input
                    type="password"
                    value={embeddingConfig().api_key || ""}
                    placeholder="sk-..."
                    onInput={(e) =>
                      setEmbeddingConfig((prev) => ({
                        ...prev,
                        api_key: e.currentTarget.value || null,
                      }))
                    }
                  />
                </div>
              </Show>

              <div class="form-row">
                <div class="form-group">
                  <label>Dimensions</label>
                  <input
                    type="number"
                    value={embeddingConfig().dimensions}
                    min={128}
                    max={4096}
                    onInput={(e) => handleDimensionsChange(parseInt(e.currentTarget.value) || 1024)}
                  />
                </div>
                <div class="form-group">
                  <label>Max Tokens</label>
                  <input
                    type="number"
                    value={embeddingConfig().max_tokens}
                    min={128}
                    max={32768}
                    onInput={(e) =>
                      setEmbeddingConfig((prev) => ({
                        ...prev,
                        max_tokens: parseInt(e.currentTarget.value) || 8192,
                      }))
                    }
                  />
                </div>
              </div>

              <button
                class={`test-btn ${testStatus()}`}
                onClick={handleTestConnection}
                disabled={testStatus() === "testing"}
              >
                {getTestButtonText()}
              </button>
            </section>

            {/* Search Parameters Section */}
            <section class="settings-section">
              <h3>SEARCH PARAMETERS</h3>

              <div class="slider-group">
                <label>
                  Min Score
                  <span class="slider-value">{searchConfig().min_score.toFixed(2)}</span>
                </label>
                <input
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  value={searchConfig().min_score}
                  onInput={(e) =>
                    setSearchConfig((prev) => ({
                      ...prev,
                      min_score: parseFloat(e.currentTarget.value),
                    }))
                  }
                />
              </div>

              <div class="slider-group">
                <label>
                  Keyword Boost
                  <span class="slider-value">{searchConfig().keyword_boost.toFixed(2)}</span>
                </label>
                <input
                  type="range"
                  min={0}
                  max={0.5}
                  step={0.05}
                  value={searchConfig().keyword_boost}
                  onInput={(e) =>
                    setSearchConfig((prev) => ({
                      ...prev,
                      keyword_boost: parseFloat(e.currentTarget.value),
                    }))
                  }
                />
              </div>

              <div class="slider-group">
                <label>
                  Recency Decay
                  <span class="slider-value">{searchConfig().recency_decay.toFixed(0)} days</span>
                </label>
                <input
                  type="range"
                  min={0}
                  max={30}
                  step={1}
                  value={searchConfig().recency_decay}
                  onInput={(e) =>
                    setSearchConfig((prev) => ({
                      ...prev,
                      recency_decay: parseFloat(e.currentTarget.value),
                    }))
                  }
                />
              </div>

              <div class="checkbox-group">
                <label>
                  <input
                    type="checkbox"
                    checked={searchConfig().include_keyword}
                    onChange={(e) =>
                      setSearchConfig((prev) => ({
                        ...prev,
                        include_keyword: e.currentTarget.checked,
                      }))
                    }
                  />
                  Include keyword search
                </label>
              </div>
            </section>

            {/* Field Weights Section */}
            <section class="settings-section">
              <h3>FIELD WEIGHTS</h3>

              <div class="slider-group">
                <label>
                  Name
                  <span class="slider-value">{searchConfig().field_weights.name.toFixed(1)}</span>
                </label>
                <input
                  type="range"
                  min={0}
                  max={3}
                  step={0.1}
                  value={searchConfig().field_weights.name}
                  onInput={(e) =>
                    setSearchConfig((prev) => ({
                      ...prev,
                      field_weights: {
                        ...prev.field_weights,
                        name: parseFloat(e.currentTarget.value),
                      },
                    }))
                  }
                />
              </div>

              <div class="slider-group">
                <label>
                  Summary
                  <span class="slider-value">{searchConfig().field_weights.summary.toFixed(1)}</span>
                </label>
                <input
                  type="range"
                  min={0}
                  max={3}
                  step={0.1}
                  value={searchConfig().field_weights.summary}
                  onInput={(e) =>
                    setSearchConfig((prev) => ({
                      ...prev,
                      field_weights: {
                        ...prev.field_weights,
                        summary: parseFloat(e.currentTarget.value),
                      },
                    }))
                  }
                />
              </div>

              <div class="slider-group">
                <label>
                  Tags
                  <span class="slider-value">{searchConfig().field_weights.tags.toFixed(1)}</span>
                </label>
                <input
                  type="range"
                  min={0}
                  max={3}
                  step={0.1}
                  value={searchConfig().field_weights.tags}
                  onInput={(e) =>
                    setSearchConfig((prev) => ({
                      ...prev,
                      field_weights: {
                        ...prev.field_weights,
                        tags: parseFloat(e.currentTarget.value),
                      },
                    }))
                  }
                />
              </div>

              <div class="slider-group">
                <label>
                  Content
                  <span class="slider-value">{searchConfig().field_weights.content.toFixed(1)}</span>
                </label>
                <input
                  type="range"
                  min={0}
                  max={3}
                  step={0.1}
                  value={searchConfig().field_weights.content}
                  onInput={(e) =>
                    setSearchConfig((prev) => ({
                      ...prev,
                      field_weights: {
                        ...prev.field_weights,
                        content: parseFloat(e.currentTarget.value),
                      },
                    }))
                  }
                />
              </div>

              <button class="reset-btn" onClick={handleReset}>
                Reset to Defaults
              </button>
            </section>
          </div>

          <div class="modal-footer">
            <button class="cancel-btn" onClick={props.onClose}>
              Cancel
            </button>
            <button class="save-btn" onClick={handleSave}>
              {needsReindex() ? "Save & Reindex" : "Save"}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default EmbeddingSettings;
