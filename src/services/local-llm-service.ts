/**
 * Local Browser LLM Service using Transformers.js
 * Runs Qwen3.5-0.8B-ONNX or custom fine-tuned models directly in the browser
 * Supports WebGPU acceleration and CPU fallback
 */

import {
  AutoProcessor,
  AutoModelForCausalLM,
  Qwen3_5ForConditionalGeneration,
  TextStreamer,
  env,
} from "@huggingface/transformers";

export type ModelType = "qwen3.5" | "qwen2" | "custom";
export type DeviceType = "webgpu" | "cpu";

export interface ModelConfig {
  id: string;
  name: string;
  type: ModelType;
  modelPath?: string; // For custom ONNX models
  provider: "huggingface" | "local";
}

export interface LocalModelProgress {
  status: "idle" | "loading" | "ready" | "error";
  progress?: number;
  message?: string;
}

export interface LocalGenerationOptions {
  max_new_tokens?: number;
  temperature?: number;
  top_p?: number;
  top_k?: number;
  do_sample?: boolean;
  enable_thinking?: boolean;
  stream?: boolean;
  onToken?: (token: string) => void;
}

// Type declarations for WebGPU
declare global {
  interface Navigator {
    gpu?: {
      requestAdapter(): Promise<GPUAdapter | null>;
    };
  }
}

interface GPUAdapter {}

// Configure transformers.js
env.allowLocalModels = false;
env.useBrowserCache = true;

class LocalLLMService {
  private model: any = null;
  private processor: any = null;
  private currentConfig: ModelConfig | null = null;
  private progressCallbacks: Array<(progress: LocalModelProgress) => void> = [];
  private _status: LocalModelProgress["status"] = "idle";

  // Default model configurations
  private readonly DEFAULT_MODELS: Record<string, ModelConfig> = {
    qwen35: {
      id: "qwen35",
      name: "Qwen 3.5 0.8B (Base)",
      type: "qwen3.5",
      provider: "huggingface",
    },
    qwen2: {
      id: "qwen2",
      name: "Qwen 2 0.5B (Base)",
      type: "qwen2",
      provider: "huggingface",
    },
  };

  /**
   * Get available model configurations
   */
  getAvailableModels(): ModelConfig[] {
    const models = Object.values(this.DEFAULT_MODELS);

    // Add custom models from localStorage if they exist
    try {
      const customModels = JSON.parse(localStorage.getItem("finetuned_models") || "[]");
      for (const cm of customModels) {
        models.push({
          id: cm.id,
          name: cm.name,
          type: "custom",
          modelPath: cm.modelPath,
          provider: "local",
        } as ModelConfig);
      }
    } catch (e) {
      console.warn("Failed to load custom models from localStorage:", e);
    }

    return models;
  }

  /**
   * Save a custom model configuration
   */
  saveCustomModel(model: Omit<ModelConfig, "provider">): void {
    try {
      const customModels = JSON.parse(localStorage.getItem("finetuned_models") || "[]");
      customModels.push({ ...model, provider: "local" as const });
      localStorage.setItem("finetuned_models", JSON.stringify(customModels));
    } catch (e) {
      console.error("Failed to save custom model:", e);
    }
  }

  /**
   * Remove a custom model
   */
  removeCustomModel(modelId: string): void {
    try {
      let customModels = JSON.parse(localStorage.getItem("finetuned_models") || "[]");
      customModels = customModels.filter((m: any) => m.id !== modelId);
      localStorage.setItem("finetuned_models", JSON.stringify(customModels));
    } catch (e) {
      console.error("Failed to remove custom model:", e);
    }
  }

  get status() {
    return this._status;
  }

  onProgress(callback: (progress: LocalModelProgress) => void) {
    this.progressCallbacks.push(callback);
    return () => {
      this.progressCallbacks = this.progressCallbacks.filter((cb) => cb !== callback);
    };
  }

  private notifyProgress(progress: LocalModelProgress) {
    this._status = progress.status;
    this.progressCallbacks.forEach((cb) => cb(progress));
  }

  /**
   * Check if WebGPU is available
   */
  async checkWebGPU(): Promise<boolean> {
    try {
      if (!navigator.gpu) return false;
      const adapter = await navigator.gpu.requestAdapter();
      return !!adapter;
    } catch {
      return false;
    }
  }

  /**
   * Load the model (call this before generating)
   */
  async loadModel(config: ModelConfig | string, device: DeviceType = "webgpu"): Promise<void> {
    // Support passing just the model ID for convenience
    const modelConfig = typeof config === "string"
      ? this.DEFAULT_MODELS[config] || this.DEFAULT_MODELS.qwen35
      : config;

    this.currentConfig = modelConfig;

    if (this.model && this.processor) {
      this.notifyProgress({ status: "ready", message: "Model already loaded" });
      return;
    }

    try {
      this.notifyProgress({ status: "loading", progress: 0, message: `Loading ${modelConfig.name}...` });

      const modelId = modelConfig.type === "custom" && modelConfig.modelPath
        ? modelConfig.modelPath
        : this.getModelIdForType(modelConfig.type);

      // Load processor/tokenizer
      this.notifyProgress({ status: "loading", progress: 0, message: "Loading tokenizer..." });

      this.processor = await AutoProcessor.from_pretrained(modelId, {
        progress_callback: (progress: any) => {
          const pct = progress.progress || 0;
          this.notifyProgress({
            status: "loading",
            progress: pct * 30,
            message: `Loading tokenizer: ${Math.round(pct * 100)}%`,
          });
        },
      });

      this.notifyProgress({ status: "loading", progress: 30, message: "Loading model..." });

      // Determine best dtype based on device
      const dtype = device === "webgpu"
        ? { embed_tokens: "q4" as const, vision_encoder: "fp16" as const, decoder_model_merged: "q4" as const }
        : { embed_tokens: "q4" as const, vision_encoder: "fp32" as const, decoder_model_merged: "q4" as const };

      // Load the appropriate model class based on type
      if (modelConfig.type === "qwen3.5") {
        this.model = await Qwen3_5ForConditionalGeneration.from_pretrained(modelId, {
          dtype,
          device,
          progress_callback: (progress: any) => {
            const pct = progress.progress || 0;
            this.notifyProgress({
              status: "loading",
              progress: 30 + pct * 70,
              message: `Loading model: ${Math.round(pct * 100)}%`,
            });
          },
        });
      } else {
        // For Qwen2 and custom models, use AutoModelForCausalLM
        this.model = await AutoModelForCausalLM.from_pretrained(modelId, {
          dtype,
          device,
          progress_callback: (progress: any) => {
            const pct = progress.progress || 0;
            this.notifyProgress({
              status: "loading",
              progress: 30 + pct * 70,
              message: `Loading model: ${Math.round(pct * 100)}%`,
            });
          },
        });
      }

      this.notifyProgress({ status: "ready", progress: 100, message: `${modelConfig.name} ready` });
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unknown error";
      this.notifyProgress({ status: "error", message });
      throw error;
    }
  }

  /**
   * Get the HuggingFace model ID for a given model type
   */
  private getModelIdForType(type: ModelType): string {
    switch (type) {
      case "qwen3.5":
        return "onnx-community/Qwen3.5-0.8B-ONNX";
      case "qwen2":
        return "Xenova/Qwen2-0.5B-Instruct"; // Or ONNX equivalent
      case "custom":
        throw new Error("Custom models must specify modelPath");
      default:
        return "onnx-community/Qwen3.5-0.8B-ONNX";
    }
  }

  /**
   * Get the current model configuration
   */
  getCurrentConfig(): ModelConfig | null {
    return this.currentConfig;
  }

  /**
   * Generate text from a conversation
   */
  async generate(
    conversation: Array<{ role: "user" | "assistant" | "system"; content: string }>,
    options: LocalGenerationOptions = {}
  ): Promise<string> {
    if (!this.model || !this.processor) {
      throw new Error("Model not loaded. Call loadModel() first.");
    }

    const {
      max_new_tokens = 512,
      temperature = 0.7,
      top_p = 0.8,
      top_k = 20,
      do_sample = true,
      enable_thinking = false,
      stream = false,
      onToken,
    } = options;

    // Format conversation
    const text = this.processor.apply_chat_template(conversation, {
      add_generation_prompt: true,
      enable_thinking,
    });

    const inputs = await this.processor(text);

    let fullResponse = "";

    if (stream && onToken) {
      // Streaming generation
      const streamer = new TextStreamer(this.processor.tokenizer, {
        skip_prompt: true,
        skip_special_tokens: true,
        callback_function: (token: string) => {
          fullResponse += token;
          onToken(token);
        },
      });

      await this.model.generate({
        ...inputs,
        max_new_tokens,
        temperature,
        top_p,
        top_k,
        do_sample,
        streamer,
      });
    } else {
      // Non-streaming generation
      const outputs = await this.model.generate({
        ...inputs,
        max_new_tokens,
        temperature,
        top_p,
        top_k,
        do_sample,
      });

      const decoded = this.processor.batch_decode(outputs, {
        skip_special_tokens: true,
      });

      // Extract only the new generated text (after input)
      const inputLength = inputs.input_ids.dims.at(-1);
      const fullText = decoded[0];
      fullResponse = fullText.slice(inputLength);
    }

    return fullResponse.trim();
  }

  /**
   * Simple completion (single prompt)
   */
  async complete(prompt: string, options: Omit<LocalGenerationOptions, "stream" | "onToken"> = {}): Promise<string> {
    const conversation = [{ role: "user" as const, content: prompt }];
    return this.generate(conversation, options);
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

Context: ${context || "General writing"}

Text before cursor:
${textBefore}

Text after cursor:
${textAfter}

Continue the text:`;

    try {
      const completion = await this.complete(prompt, {
        max_new_tokens: 150,
        temperature: 0.3,
      });
      return completion.trim() || null;
    } catch (e) {
      console.error("Local inline completion failed:", e);
      return null;
    }
  }

  /**
   * Unload model to free memory
   */
  unloadModel() {
    this.model = null;
    this.processor = null;
    this._status = "idle";
    this.notifyProgress({ status: "idle", message: "Model unloaded" });
  }
}

// Singleton instance
export const localLLMService = new LocalLLMService();

// Hook for SolidJS
import { createSignal, onCleanup, createEffect } from "solid-js";

export function useLocalLLM() {
  const [status, setStatus] = createSignal<LocalModelProgress>({ status: "idle" });
  const [isWebGPUSupported, setIsWebGPUSupported] = createSignal<boolean | null>(null);
  const [availableModels, setAvailableModels] = createSignal<ModelConfig[]>([]);
  const [currentConfig, setCurrentConfig] = createSignal<ModelConfig | null>(null);

  createEffect(() => {
    // Check WebGPU support
    localLLMService.checkWebGPU().then(setIsWebGPUSupported);

    // Load available models
    setAvailableModels(localLLMService.getAvailableModels());

    // Subscribe to progress
    const unsubscribe = localLLMService.onProgress((progress) => {
      setStatus(progress);
      if (progress.status === "ready") {
        setCurrentConfig(localLLMService.getCurrentConfig());
      }
    });
    onCleanup(unsubscribe);
  });

  return {
    status,
    isWebGPUSupported,
    availableModels,
    currentConfig,
    loadModel: (config: ModelConfig | string, device?: DeviceType) =>
      localLLMService.loadModel(config, device),
    unloadModel: () => localLLMService.unloadModel(),
    generate: (conversation: any[], options?: LocalGenerationOptions) =>
      localLLMService.generate(conversation, options),
    complete: (prompt: string, options?: any) => localLLMService.complete(prompt, options),
    getInlineCompletion: (before: string, after: string, context?: string) =>
      localLLMService.getInlineCompletion(before, after, context),
    saveCustomModel: (model: Omit<ModelConfig, "provider">) =>
      localLLMService.saveCustomModel(model),
    removeCustomModel: (modelId: string) =>
      localLLMService.removeCustomModel(modelId),
    refreshModels: () => setAvailableModels(localLLMService.getAvailableModels()),
  };
}
