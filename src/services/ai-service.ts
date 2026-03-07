/**
 * Unified AI Service
 * Makes calls to any configured provider with the same interface
 */

import { aiProviderManager, type AIProvider } from './ai-config';

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

export interface ChatCompletionRequest {
  messages: ChatMessage[];
  model?: string;
  temperature?: number;
  maxTokens?: number;
  stream?: boolean;
}

export interface ChatCompletionResponse {
  content: string;
  model: string;
  usage?: {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
  };
}

export interface CompletionRequest {
  prompt: string;
  model?: string;
  temperature?: number;
  maxTokens?: number;
}

class AIService {
  /**
   * Send a chat completion request to the active provider
   */
  async chat(request: ChatCompletionRequest): Promise<ChatCompletionResponse> {
    const provider = aiProviderManager.getActiveProvider();
    const config = aiProviderManager.getConfig();
    
    const body = this.formatRequest(provider, request, config);
    
    const response = await fetch(
      aiProviderManager.getUrl('/chat/completions', provider),
      {
        method: 'POST',
        headers: aiProviderManager.getHeaders(provider),
        body: JSON.stringify(body),
      }
    );

    if (!response.ok) {
      throw new Error(`AI request failed: ${response.status} ${response.statusText}`);
    }

    const data = await response.json();
    return this.parseResponse(provider, data);
  }

  /**
   * Get a simple text completion
   */
  async complete(request: CompletionRequest): Promise<string> {
    const messages: ChatMessage[] = [
      { role: 'user', content: request.prompt }
    ];
    
    const response = await this.chat({
      messages,
      model: request.model,
      temperature: request.temperature,
      maxTokens: request.maxTokens,
    });

    return response.content;
  }

  /**
   * Get inline completion for ghost text
   */
  async getInlineCompletion(
    textBefore: string,
    textAfter: string,
    context?: string
  ): Promise<string | null> {
    const prompt = `Complete the following text naturally. Only provide the completion, no explanation.

Context: ${context || 'General writing'}

Text before cursor:
${textBefore}

Text after cursor:
${textAfter}

Continue the text:`;

    try {
      const completion = await this.complete({
        prompt,
        temperature: 0.3, // Lower temp for predictable completions
        maxTokens: 150,
      });
      
      // Clean up the response
      return completion.trim() || null;
    } catch (e) {
      console.error('Inline completion failed:', e);
      return null;
    }
  }

  /**
   * Check if the AI service is available
   */
  async checkHealth(): Promise<boolean> {
    try {
      const provider = aiProviderManager.getActiveProvider();
      const response = await fetch(
        aiProviderManager.getUrl('/models', provider),
        {
          headers: aiProviderManager.getHeaders(provider),
        }
      );
      return response.ok;
    } catch {
      return false;
    }
  }

  /**
   * Format request for specific provider
   */
  private formatRequest(
    provider: AIProvider,
    request: ChatCompletionRequest,
    config: { defaultParams: { temperature: number; maxTokens: number } }
  ): unknown {
    const model = request.model || provider.defaultModel;
    const temperature = request.temperature ?? config.defaultParams.temperature;
    const maxTokens = request.maxTokens ?? config.defaultParams.maxTokens;

    // Standard OpenAI-compatible format
    return {
      model,
      messages: request.messages,
      temperature,
      max_tokens: maxTokens,
      stream: request.stream || false,
    };
  }

  /**
   * Parse response from specific provider
   */
  private parseResponse(provider: AIProvider, data: any): ChatCompletionResponse {
    // Handle Anthropic format differences
    if (provider.id === 'anthropic' && data.content) {
      return {
        content: data.content[0]?.text || '',
        model: data.model,
        usage: data.usage ? {
          promptTokens: data.usage.input_tokens,
          completionTokens: data.usage.output_tokens,
          totalTokens: data.usage.input_tokens + data.usage.output_tokens,
        } : undefined,
      };
    }

    // Standard OpenAI format
    return {
      content: data.choices?.[0]?.message?.content || '',
      model: data.model,
      usage: data.usage ? {
        promptTokens: data.usage.prompt_tokens,
        completionTokens: data.usage.completion_tokens,
        totalTokens: data.usage.total_tokens,
      } : undefined,
    };
  }
}

// Singleton instance
export const aiService = new AIService();

// Simple hook for components
import { createSignal, createResource } from 'solid-js';

export function useAIHealth() {
  const [health] = createResource(() => aiService.checkHealth());
  return health;
}
