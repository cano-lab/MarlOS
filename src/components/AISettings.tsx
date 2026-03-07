import { Component, createSignal, For, Show } from "solid-js";
import { aiProviderManager, defaultAIConfig, type AIProvider } from "../../services/ai-config";
import "./AISettings.css";

const AISettings: Component = () => {
  const [config, setConfig] = createSignal(aiProviderManager.getConfig());
  const [editingProvider, setEditingProvider] = createSignal<string | null>(null);
  const [testStatus, setTestStatus] = createSignal<{ provider: string; success: boolean; message: string } | null>(null);

  const handleSetActive = (id: string) => {
    aiProviderManager.setActiveProvider(id);
    setConfig(aiProviderManager.getConfig());
  };

  const handleUpdateProvider = (id: string, updates: Partial<AIProvider>) => {
    aiProviderManager.updateProvider(id, updates);
    setConfig(aiProviderManager.getConfig());
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
        message: `Error: ${e.message}`,
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
                  <label>
                    Base URL:
                    <input
                      type="text"
                      value={provider.baseUrl}
                      onChange={(e) => handleUpdateProvider(provider.id, { baseUrl: e.target.value })}
                      placeholder="http://localhost:1234/v1"
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
                    <button 
                      class="btn-secondary"
                      onClick={() => setEditingProvider(null)}
                    >
                      Done
                    </button>
                  </div>

                  <Show when={testStatus()?.provider === provider.id}>
                    <div class={`test-result ${testStatus()?.success ? 'success' : 'error'}`}>
                      {testStatus()?.message}
                    </div>
                  </Show>
                </div>
              </Show>

              <Show when={editingProvider() !== provider.id}>
                <div class="provider-info">
                  <span class="provider-url">{provider.baseUrl}</span>
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
