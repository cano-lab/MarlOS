import { Component, createSignal, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { aiProviderManager, type AIProvider } from "../services/ai-config";
import { useLocalLLM } from "../services/ai-service";
import "./AISettings.css";

const AISettings: Component = () => {
  const [config, setConfig] = createSignal(aiProviderManager.getConfig());
  const [editingProvider, setEditingProvider] = createSignal<string | null>(null);
  const [testStatus, setTestStatus] = createSignal<{ provider: string; success: boolean; message: string } | null>(null);
  
  // Local LLM hooks for browser provider
  const localLLM = useLocalLLM();

  // Push a menu provider into the backend AiManager so it actually drives
  // server-side generation (the resume/custom Style panel via ai_generate,
  // code ops, etc.). Browser-local models aren't backend HTTP providers, so
  // they're skipped. Field shapes match the Rust ProviderConfig.
  const pushActiveToBackend = async (provider: AIProvider) => {
    if (provider.type === "browser") return;
    const params = config().defaultParams;
    try {
      await invoke("ai_set_config", {
        config: {
          name: provider.name,
          base_url: provider.baseUrl,
          api_key: provider.apiKey || null,
          model: provider.defaultModel,
          temperature: params.temperature,
          max_tokens: params.maxTokens,
          timeout_secs: 300,
        },
      });
    } catch (e) {
      console.debug("ai_set_config bridge failed:", e);
    }
  };

  const handleSetActive = (id: string) => {
    aiProviderManager.setActiveProvider(id);
    const cfg = aiProviderManager.getConfig();
    setConfig(cfg);
    const provider = cfg.providers.find((p) => p.id === id);
    if (provider) void pushActiveToBackend(provider);
  };

  const handleUpdateProvider = (id: string, updates: Partial<AIProvider>) => {
    aiProviderManager.updateProvider(id, updates);
    const cfg = aiProviderManager.getConfig();
    setConfig(cfg);
    // Re-sync to the backend when the *active* provider is edited, so a
    // freshly pasted API key or model change takes effect immediately.
    if (cfg.activeProvider === id) {
      const provider = cfg.providers.find((p) => p.id === id);
      if (provider) void pushActiveToBackend(provider);
    }
  };

  const handleTestConnection = async (provider: AIProvider) => {
    setTestStatus({ provider: provider.id, success: false, message: "Testing..." });
    
    try {
      const isHealthy = await aiProviderManager.checkHealth();
      setTestStatus({
        provider: provider.id,
        success: isHealthy,
        message: isHealthy ? "Connected!" : "Failed to connect",
      });
    } catch (e) {
      setTestStatus({
        provider: provider.id,
        success: false,
        message: `Error: ${(e as Error).message}`,
      });
    }
  };

  const handleReset = () => {
    if (confirm("Reset to default configuration?")) {
      localStorage.removeItem('ai-config');
      window.location.reload();
    }
  };

  return (
    <div class="ai-settings">
      <h2>AI Providers</h2>
      
      <div class="providers-list">
        <For each={config().providers}>
          {(provider) => (
            <div 
              class={`provider-card ${config().activeProvider === provider.id ? 'active' : ''}`}
              onClick={() => handleSetActive(provider.id)}
            >
              <div class="provider-header">
                <input
                  type="radio"
                  name="active-provider"
                  checked={config().activeProvider === provider.id}
                  onChange={() => handleSetActive(provider.id)}
                />
                <span class="provider-name">{provider.name}</span>
                <span class={`provider-type ${provider.type}`}>{provider.type}</span>
              </div>

              <Show when={editingProvider() === provider.id}>
                <div class="provider-edit">
                  {/* Browser provider specific UI */}
                  <Show when={provider.type === 'browser'}>
                    <div class="browser-provider-ui">
                      <div class="webgpu-status">
                        WebGPU Support: {localLLM.isWebGPUSupported() === null ? 'Checking...' : 
                          localLLM.isWebGPUSupported() ? '✅ Available' : '❌ Not available (will use CPU)'}
                      </div>
                      
                      <div class="model-status">
                        Model Status: {localLLM.status().status === 'idle' ? '📦 Not loaded' :
                          localLLM.status().status === 'loading' ? `⏳ Loading (${Math.round(localLLM.status().progress || 0)}%)` :
                          localLLM.status().status === 'ready' ? '✅ Ready' :
                          localLLM.status().status === 'error' ? `❌ Error: ${localLLM.status().message}` : 'Unknown'}
                      </div>

                      <Show when={localLLM.status().status === 'idle' || localLLM.status().status === 'error'}>
                        <button 
                          class="btn-primary"
                          onClick={() => localLLM.loadModel(localLLM.isWebGPUSupported() ? 'webgpu' : 'cpu')}
                          disabled={localLLM.status().status === 'loading'}
                        >
                          Load Model
                        </button>
                      </Show>

                      <Show when={localLLM.status().status === 'ready'}>
                        <button 
                          class="btn-secondary"
                          onClick={() => localLLM.unloadModel()}
                        >
                          Unload Model
                        </button>
                      </Show>

                      <Show when={localLLM.status().message}>
                        <div class="status-message">{localLLM.status().message}</div>
                      </Show>
                    </div>
                  </Show>

                  <Show when={provider.type !== 'browser'}>
                    <label>
                      Base URL:
                      <input
                        type="text"
                        value={provider.baseUrl}
                        onChange={(e) => handleUpdateProvider(provider.id, { baseUrl: e.target.value })}
                        placeholder="http://localhost:4321/v1"
                      />
                    </label>

                    <Show when={provider.type === 'cloud'}>
                      <label>
                        API Key:
                        <input
                          type="password"
                          value={provider.apiKey || ''}
                          onChange={(e) => handleUpdateProvider(provider.id, { apiKey: e.target.value })}
                          placeholder="sk-..."
                        />
                      </label>
                    </Show>

                    <label>
                      Default Model:
                      <select
                        value={provider.defaultModel}
                        onChange={(e) => handleUpdateProvider(provider.id, { defaultModel: e.target.value })}
                      >
                        <For each={provider.availableModels}>
                          {(model) => <option value={model}>{model}</option>}
                        </For>
                      </select>
                    </label>

                    <div class="provider-actions">
                      <button 
                        class="btn-secondary"
                        onClick={() => handleTestConnection(provider)}
                      >
                        Test Connection
                      </button>
                      <Show when={testStatus()?.provider === provider.id}>
                        <div class={`test-result ${testStatus()?.success ? 'success' : 'error'}`}>
                          {testStatus()?.message}
                        </div>
                      </Show>
                    </div>
                  </Show>

                  <button 
                    class="btn-secondary"
                    onClick={() => setEditingProvider(null)}
                  >
                    Done
                  </button>
                </div>
              </Show>

              <Show when={editingProvider() !== provider.id}>
                <div class="provider-info">
                  <Show when={provider.type === 'browser'}>
                    <span class="provider-url">
                      {localLLM.status().status === 'ready' ? '✅ Model loaded' : 
                       localLLM.status().status === 'loading' ? `⏳ Loading ${Math.round(localLLM.status().progress || 0)}%` :
                       '📦 Click Edit to load model'}
                    </span>
                  </Show>
                  <Show when={provider.type !== 'browser'}>
                    <span class="provider-url">{provider.baseUrl}</span>
                  </Show>
                  
                  <button 
                    class="btn-link"
                    onClick={(e) => {
                      e.stopPropagation();
                      setEditingProvider(provider.id);
                    }}
                  >
                    Edit
                  </button>
                </div>
              </Show>
            </div>
          )}
        </For>
      </div>

      <div class="default-params">
        <h3>Default Parameters</h3>
        <label>
          Temperature: {config().defaultParams.temperature}
          <input
            type="range"
            min="0"
            max="2"
            step="0.1"
            value={config().defaultParams.temperature}
            onChange={(e) => {
              aiProviderManager.updateDefaultParams({ temperature: parseFloat(e.target.value) });
              setConfig(aiProviderManager.getConfig());
            }}
          />
        </label>

        <label>
          Max Tokens: {config().defaultParams.maxTokens}
          <input
            type="number"
            min="100"
            max="8000"
            step="100"
            value={config().defaultParams.maxTokens}
            onChange={(e) => {
              aiProviderManager.updateDefaultParams({ maxTokens: parseInt(e.target.value) });
              setConfig(aiProviderManager.getConfig());
            }}
          />
        </label>
      </div>

      <div class="settings-actions">
        <button class="btn-secondary" onClick={handleReset}>
          Reset to Defaults
        </button>
      </div>
    </div>
  );
};

export default AISettings;
