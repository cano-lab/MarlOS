/**
 * Local Browser LLM Service using Transformers.js
 * Runs Qwen3.5-0.8B-ONNX directly in the browser with WebGPU acceleration
 */

import {
  AutoProcessor,
  Qwen3_5ForConditionalGeneration,
  TextStreamer,
  env,
} from "@huggingface/transformers";

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
  private model_id = "onnx-community/Qwen3.5-0.8B-ONNX";
  private progressCallbacks: Array<(progress: LocalModelProgress) => void> = [];
  private _status: LocalModelProgress["status"] = "idle";

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
  async loadModel(device: "webgpu" | "cpu" = "webgpu"): Promise<void> {
    if (this.model && this.processor) {
      this.notifyProgress({ status: "ready", message: "Model already loaded" });
      return;
    }

    try {
      this.notifyProgress({ status: "loading", progress: 0, message: "Loading processor..." });

      this.processor = await AutoProcessor.from_pretrained(this.model_id, {
        progress_callback: (progress: any) => {
          const pct = progress.progress * 100;
          this.notifyProgress({
            status: "loading",
            progress: pct * 0.3,
            message: `Loading processor: ${pct.toFixed(1)}%`,
          });
        },
      });

      this.notifyProgress({ status: "loading", progress: 30, message: "Loading model..." });

      // Determine best dtype based on device
      const dtype = device === "webgpu" 
        ? { embed_tokens: "q4" as const, vision_encoder: "fp16" as const, decoder_model_merged: "q4" as const }
        : { embed_tokens: "q4" as const, vision_encoder: "fp32" as const, decoder_model_merged: "q4" as const };

      this.model = await Qwen3_5ForConditionalGeneration.from_pretrained(this.model_id, {
        dtype,
        device,
        progress_callback: (progress: any) => {
          const pct = progress.progress * 100;
          this.notifyProgress({
            status: "loading",
            progress: 30 + pct * 0.7,
            message: `Loading model: ${pct.toFixed(1)}%`,
          });
        },
      });

      this.notifyProgress({ status: "ready", progress: 100, message: "Model ready" });
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unknown error";
      this.notifyProgress({ status: "error", message });
      throw error;
    }
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

  createEffect(() => {
    // Check WebGPU support
    localLLMService.checkWebGPU().then(setIsWebGPUSupported);

    // Subscribe to progress
    const unsubscribe = localLLMService.onProgress(setStatus);
    onCleanup(unsubscribe);
  });

  return {
    status,
    isWebGPUSupported,
    loadModel: (device?: "webgpu" | "cpu") => localLLMService.loadModel(device),
    unloadModel: () => localLLMService.unloadModel(),
    generate: (conversation: any[], options?: LocalGenerationOptions) =>
      localLLMService.generate(conversation, options),
    complete: (prompt: string, options?: any) => localLLMService.complete(prompt, options),
    getInlineCompletion: (before: string, after: string, context?: string) =>
      localLLMService.getInlineCompletion(before, after, context),
  };
}
