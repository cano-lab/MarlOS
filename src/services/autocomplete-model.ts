/**
 * Autocomplete Model Service
 *
 * Uses local LLM service for personalized autocomplete suggestions.
 * Supports both base models and fine-tuned models.
 */

import { localLLMService, type ModelConfig } from "./local-llm-service";

export interface AutocompleteModelProgress {
  status: "idle" | "loading" | "ready" | "error";
  progress?: number;
  message?: string;
}

class AutocompleteModelService {
  private progressCallbacks: Array<(progress: AutocompleteModelProgress) => void> = [];
  private _status: AutocompleteModelProgress["status"] = "idle";
  private initPromise: Promise<void> | null = null;
  private useFineTunedModel: boolean = false;

  get status() {
    return this._status;
  }

  /**
   * Set whether to use fine-tuned model
   */
  setUseFineTunedModel(use: boolean) {
    this.useFineTunedModel = use;
  }

  /**
   * Get available fine-tuned models
   */
  getFineTunedModels(): ModelConfig[] {
    const allModels = localLLMService.getAvailableModels();
    return allModels.filter(m => m.provider === "local");
  }

  onProgress(callback: (progress: AutocompleteModelProgress) => void) {
    this.progressCallbacks.push(callback);
    return () => {
      this.progressCallbacks = this.progressCallbacks.filter((cb) => cb !== callback);
    };
  }

  private notifyProgress(progress: AutocompleteModelProgress) {
    this._status = progress.status;
    this.progressCallbacks.forEach((cb) => cb(progress));
  }

  /**
   * Initialize the autocomplete model
   */
  async initialize(): Promise<void> {
    if (this.initPromise) return this.initPromise;

    this.initPromise = (async () => {
      try {
        this.notifyProgress({ status: "loading", progress: 0, message: "Loading autocomplete model..." });

        // Check if we should use fine-tuned model
        const fineTunedModels = this.getFineTunedModels();
        const modelToLoad = this.useFineTunedModel && fineTunedModels.length > 0
          ? fineTunedModels[0]
          : "qwen2"; // Use smaller Qwen2 model for faster autocomplete

        await localLLMService.loadModel(modelToLoad, "webgpu");

        this.notifyProgress({ status: "ready", progress: 100, message: "Autocomplete ready" });
      } catch (error) {
        const message = error instanceof Error ? error.message : "Unknown error";
        console.error(`[AutocompleteModel] Failed to load:`, error);
        this.notifyProgress({ status: "error", message });
        this.initPromise = null;
        throw error;
      }
    })();

    return this.initPromise;
  }

  /**
   * Get autocomplete completion
   */
  async complete(
    textBefore: string,
    maxWords: number = 8
  ): Promise<string | null> {
    // Initialize if needed
    if (!localLLMService.getCurrentConfig()) {
      await this.initialize();
    }

    try {
      // Use last ~100 chars as context
      const context = textBefore.slice(-100);

      // Get the last word to avoid repeating it
      const lastWord = textBefore.split(/\s+/).pop() || "";

      // Build a simple prompt for completion
      const prompt = `Continue this text naturally. Only provide 1-8 words that would naturally come next.\n\nText: ${context}\n\nContinuation:`;

      const completion = await localLLMService.complete(prompt, {
        max_new_tokens: 20,
        temperature: 0.5,
        top_k: 30,
      });

      if (!completion) return null;

      // Clean up the output
      let cleaned = completion
        .replace(/[\n\r]/g, " ")
        .replace(/\s+/g, " ")
        .trim();

      // Remove if it repeats the last word
      const firstCompletionWord = cleaned.split(/\s+/)[0]?.toLowerCase();
      if (firstCompletionWord === lastWord.toLowerCase()) {
        cleaned = cleaned.split(/\s+/).slice(1).join(" ");
      }

      // Stop at common sentence boundaries
      const boundaries = ['. ', '! ', '? ', '\n', '.'];
      for (const boundary of boundaries) {
        const idx = cleaned.indexOf(boundary);
        if (idx > 0) {
          cleaned = cleaned.slice(0, idx + 1);
          break;
        }
      }

      // Only take first maxWords words
      const words = cleaned.split(/\s+/);
      if (words.length > maxWords) {
        cleaned = words.slice(0, maxWords).join(" ");
      }

      // Remove trailing punctuation
      cleaned = cleaned.replace(/[.!?,;:]+$/, "");

      // Don't return if too short
      if (cleaned.length < 2) return null;

      // Remove leading special characters
      cleaned = cleaned.replace(/^[^\w\s]+/, "");

      return cleaned || null;
    } catch (error) {
      console.error("Autocomplete generation failed:", error);
      return null;
    }
  }

  /**
   * Switch to a specific fine-tuned model
   */
  async switchModel(modelId: string): Promise<boolean> {
    try {
      const models = localLLMService.getAvailableModels();
      const model = models.find(m => m.id === modelId);

      if (!model) {
        console.error(`Model ${modelId} not found`);
        return false;
      }

      // Unload current model and load new one
      localLLMService.unloadModel();

      await localLLMService.loadModel(model, "webgpu");
      return true;
    } catch (error) {
      console.error("Failed to switch model:", error);
      return false;
    }
  }

  /**
   * Unload model to free memory
   */
  unload() {
    localLLMService.unloadModel();
    this.initPromise = null;
    this._status = "idle";
    this.notifyProgress({ status: "idle", message: "Model unloaded" });
  }
}

// Singleton instance
export const autocompleteModel = new AutocompleteModelService();

// Hook for SolidJS
import { createSignal, onCleanup, createEffect } from "solid-js";

export function useAutocompleteModel() {
  const [status, setStatus] = createSignal<AutocompleteModelProgress>({ status: "idle" });
  const [useFineTuned, setUseFineTuned] = createSignal(false);
  const [fineTunedModels, setFineTunedModels] = createSignal<ModelConfig[]>([]);

  createEffect(() => {
    // Update fine-tuned models list
    setFineTunedModels(autocompleteModel.getFineTunedModels());

    // Subscribe to progress
    const unsubscribe = autocompleteModel.onProgress(setStatus);
    onCleanup(unsubscribe);
  });

  return {
    status,
    useFineTuned,
    setUseFineTuned: (use: boolean) => {
      setUseFineTuned(use);
      autocompleteModel.setUseFineTunedModel(use);
    },
    fineTunedModels,
    initialize: () => autocompleteModel.initialize(),
    complete: (text: string, maxWords?: number) => autocompleteModel.complete(text, maxWords),
    switchModel: (modelId: string) => autocompleteModel.switchModel(modelId),
    unload: () => autocompleteModel.unload(),
  };
}
