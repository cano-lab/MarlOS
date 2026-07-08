/**
 * Centralized AI Provider Configuration
 * Simple, unified interface for all AI backends
 */

export interface AIProvider {
  id: string;
  name: string;
  type: 'local' | 'cloud' | 'browser';
  baseUrl: string;
  apiKey?: string;
  defaultModel: string;
  availableModels: string[];
}

export interface AIConfig {
  activeProvider: string;
  providers: AIProvider[];
  defaultParams: {
    temperature: number;
    maxTokens: number;
    topP: number;
  };
}

// Default configuration
export const defaultAIConfig: AIConfig = {
  activeProvider: 'local',
  providers: [
    {
      id: 'browser',
      name: 'Local Browser (Qwen3.5-0.8B)',
      type: 'browser',
      baseUrl: '', // Not used for browser provider
      defaultModel: 'Qwen3.5-0.8B-ONNX',
      availableModels: ['Qwen3.5-0.8B-ONNX'],
    },
    {
      id: 'local',
      name: 'Local API (LM Studio/Ollama)',
      type: 'local',
      baseUrl: 'http://localhost:4321/v1', // LM Studio default
      defaultModel: 'local-model',
      availableModels: ['local-model'],
    },
    {
      id: 'openai',
      name: 'OpenAI',
      type: 'cloud',
      baseUrl: 'https://api.openai.com/v1',
      apiKey: '',
      defaultModel: 'gpt-4',
      availableModels: ['gpt-4', 'gpt-4-turbo', 'gpt-3.5-turbo'],
    },
    {
      id: 'anthropic',
      name: 'Anthropic',
      type: 'cloud',
      baseUrl: 'https://api.anthropic.com/v1',
      apiKey: '',
      defaultModel: 'claude-3-sonnet',
      availableModels: ['claude-3-opus', 'claude-3-sonnet', 'claude-3-haiku'],
    },
    {
      // Kimi K2 coding endpoint. OpenAI-compatible: the client appends
      // /chat/completions, so the effective URL is
      // https://api.kimi.com/coding/chat/completions. Powers the
      // resume/custom typeset Style panel when set active.
      id: 'kimi',
      name: 'Kimi (Moonshot K2)',
      type: 'cloud',
      baseUrl: 'https://api.kimi.com/coding',
      apiKey: '',
      defaultModel: 'kimi-k2.7',
      availableModels: ['kimi-k2.7'],
    },
  ],
  defaultParams: {
    temperature: 0.7,
    maxTokens: 2000,
    topP: 1.0,
  },
};

class AIProviderManager {
  private config: AIConfig;

  constructor() {
    this.config = this.loadConfig();
  }

  private loadConfig(): AIConfig {
    try {
      const saved = localStorage.getItem('ai-config');
      if (saved) {
        const merged = { ...defaultAIConfig, ...JSON.parse(saved) };
        return this.ensureBuiltinProviders(merged);
      }
    } catch (e) {
      console.error('Failed to load AI config:', e);
    }
    return defaultAIConfig;
  }

  /** Append any built-in provider missing from a (possibly older) saved
   *  config — so newer defaults like Kimi appear without the user having
   *  to reset. Existing entries and edits are left untouched. */
  private ensureBuiltinProviders(cfg: AIConfig): AIConfig {
    for (const builtin of defaultAIConfig.providers) {
      if (!cfg.providers.find((p) => p.id === builtin.id)) {
        cfg.providers.push(builtin);
      }
    }
    return cfg;
  }

  saveConfig() {
    localStorage.setItem('ai-config', JSON.stringify(this.config));
  }

  getActiveProvider(): AIProvider {
    const provider = this.config.providers.find(p => p.id === this.config.activeProvider);
    return provider || this.config.providers[0];
  }

  setActiveProvider(id: string) {
    if (this.config.providers.find(p => p.id === id)) {
      this.config.activeProvider = id;
      this.saveConfig();
    }
  }

  updateProvider(id: string, updates: Partial<AIProvider>) {
    const index = this.config.providers.findIndex(p => p.id === id);
    if (index >= 0) {
      this.config.providers[index] = { ...this.config.providers[index], ...updates };
      this.saveConfig();
    }
  }

  addProvider(provider: AIProvider) {
    this.config.providers.push(provider);
    this.saveConfig();
  }

  removeProvider(id: string) {
    this.config.providers = this.config.providers.filter(p => p.id !== id);
    if (this.config.activeProvider === id) {
      this.config.activeProvider = this.config.providers[0]?.id || '';
    }
    this.saveConfig();
  }

  getConfig(): AIConfig {
    return { ...this.config };
  }

  updateDefaultParams(params: Partial<AIConfig['defaultParams']>) {
    this.config.defaultParams = { ...this.config.defaultParams, ...params };
    this.saveConfig();
  }

  // Generate headers for API calls
  getHeaders(provider?: AIProvider): Record<string, string> {
    const p = provider || this.getActiveProvider();
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };
    if (p.apiKey) {
      headers['Authorization'] = `Bearer ${p.apiKey}`;
    }
    return headers;
  }

  // Get complete request URL
  getUrl(endpoint: string, provider?: AIProvider): string {
    const p = provider || this.getActiveProvider();
    const base = p.baseUrl.replace(/\/$/, '');
    const path = endpoint.startsWith('/') ? endpoint : `/${endpoint}`;
    return `${base}${path}`;
  }

  // Check health of a provider (for API-based providers)
  async checkHealth(): Promise<boolean> {
    const provider = this.getActiveProvider();
    
    // Browser provider doesn't use HTTP
    if (provider.type === 'browser') {
      return true;
    }

    try {
      const response = await fetch(this.getUrl('/models', provider), {
        headers: this.getHeaders(provider),
      });
      return response.ok;
    } catch {
      return false;
    }
  }
}

// Singleton instance
export const aiProviderManager = new AIProviderManager();

// React hook for components
import { createSignal, createEffect } from 'solid-js';

export function useAIConfig() {
  const [config, setConfig] = createSignal(aiProviderManager.getConfig());

  createEffect(() => {
    // Subscribe to changes
    const originalSave = aiProviderManager.saveConfig.bind(aiProviderManager);
    aiProviderManager.saveConfig = () => {
      originalSave();
      setConfig(aiProviderManager.getConfig());
    };
  });

  return {
    config,
    setActiveProvider: (id: string) => {
      aiProviderManager.setActiveProvider(id);
      setConfig(aiProviderManager.getConfig());
    },
    updateProvider: (id: string, updates: Partial<AIProvider>) => {
      aiProviderManager.updateProvider(id, updates);
      setConfig(aiProviderManager.getConfig());
    },
    getActiveProvider: () => aiProviderManager.getActiveProvider(),
  };
}
