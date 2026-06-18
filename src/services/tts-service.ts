/**
 * Text-to-Speech Service using kokoro-js (Kokoro 82M ONNX).
 * Runs in the browser via transformers.js / ONNX Runtime Web.
 * First use downloads ~80–160MB depending on dtype. WebGPU when available, CPU otherwise.
 */

import { KokoroTTS } from "kokoro-js";
import { createSignal, onCleanup, createEffect } from "solid-js";

export type TtsStatus = "idle" | "loading" | "ready" | "synthesizing" | "playing" | "error";
export type TtsDevice = "webgpu" | "cpu";

export interface TtsProgress {
  status: TtsStatus;
  progress?: number;
  message?: string;
}

// Kokoro voice IDs — see https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX
export type KokoroVoice =
  | "af_heart" | "af_bella" | "af_nicole" | "af_sarah" | "af_sky"
  | "am_adam" | "am_michael"
  | "bf_emma" | "bf_isabella"
  | "bm_george" | "bm_lewis";

export interface TtsSpeakOptions {
  voice?: KokoroVoice;
  /** 0.5 = slow, 1.0 = normal, 1.5 = fast. Affects synthesis pitch/duration. */
  speed?: number;
  /** If true, do not auto-play; just return the audio. */
  noAutoPlay?: boolean;
}

const KOKORO_MODEL_ID = "onnx-community/Kokoro-82M-v1.0-ONNX";

class TTSService {
  private tts: any = null; // KokoroTTS instance
  private device: TtsDevice = "webgpu";
  private currentAudio: HTMLAudioElement | null = null;
  private currentObjectUrl: string | null = null;
  private progressCallbacks: Array<(p: TtsProgress) => void> = [];
  private _status: TtsStatus = "idle";

  get status() {
    return this._status;
  }

  onProgress(cb: (p: TtsProgress) => void) {
    this.progressCallbacks.push(cb);
    return () => {
      this.progressCallbacks = this.progressCallbacks.filter((c) => c !== cb);
    };
  }

  private notify(p: TtsProgress) {
    this._status = p.status;
    this.progressCallbacks.forEach((cb) => cb(p));
  }

  async checkWebGPU(): Promise<boolean> {
    try {
      if (!(navigator as any).gpu) return false;
      const adapter = await (navigator as any).gpu.requestAdapter();
      return !!adapter;
    } catch {
      return false;
    }
  }

  async loadModel(device: TtsDevice = "webgpu"): Promise<void> {
    if (this.tts && this.device === device) {
      this.notify({ status: "ready", message: "TTS already loaded" });
      return;
    }

    try {
      this.notify({ status: "loading", progress: 0, message: "Loading Kokoro TTS..." });

      // q8 is a good balance for browser; q4 for very small footprint, fp32 for highest quality
      const dtype = device === "webgpu" ? "fp32" : "q8";

      this.tts = await KokoroTTS.from_pretrained(KOKORO_MODEL_ID, {
        dtype,
        device,
        progress_callback: (p: any) => {
          const pct = typeof p?.progress === "number" ? p.progress : 0;
          this.notify({
            status: "loading",
            progress: pct,
            message: `Loading TTS: ${Math.round(pct * 100)}%`,
          });
        },
      } as any);

      this.device = device;
      this.notify({ status: "ready", progress: 1, message: "TTS ready" });
    } catch (err) {
      const message = err instanceof Error ? err.message : "Unknown TTS error";
      this.notify({ status: "error", message });
      throw err;
    }
  }

  /**
   * List available voices known to Kokoro.
   */
  listVoices(): KokoroVoice[] {
    return [
      "af_heart", "af_bella", "af_nicole", "af_sarah", "af_sky",
      "am_adam", "am_michael",
      "bf_emma", "bf_isabella",
      "bm_george", "bm_lewis",
    ];
  }

  /**
   * Synthesize speech for `text`. Auto-plays via HTMLAudioElement unless `noAutoPlay` is set.
   * Returns the WAV Blob and a usable object URL.
   */
  async speak(text: string, opts: TtsSpeakOptions = {}): Promise<{ blob: Blob; url: string }> {
    if (!this.tts) await this.loadModel(this.device);
    if (!this.tts) throw new Error("TTS model failed to initialize");

    const trimmed = text.trim();
    if (!trimmed) throw new Error("TTS: empty text");

    this.notify({ status: "synthesizing", message: "Synthesizing audio..." });

    const audio = await this.tts.generate(trimmed, {
      voice: opts.voice ?? "af_heart",
      speed: opts.speed ?? 1.0,
    });

    // kokoro-js returns a RawAudio with .toBlob() / .toWav() helpers
    const blob: Blob =
      typeof audio?.toBlob === "function"
        ? audio.toBlob()
        : new Blob([audio?.toWav?.() ?? audio], { type: "audio/wav" });

    // Clean up any previous URL/audio
    this.stop();
    const url = URL.createObjectURL(blob);
    this.currentObjectUrl = url;

    if (!opts.noAutoPlay) {
      this.currentAudio = new Audio(url);
      this.currentAudio.addEventListener("ended", () => {
        this.notify({ status: "ready", message: "Done" });
      });
      this.currentAudio.addEventListener("error", () => {
        this.notify({ status: "error", message: "Audio playback failed" });
      });
      this.notify({ status: "playing", message: "Playing..." });
      await this.currentAudio.play().catch(() => {
        this.notify({ status: "error", message: "Audio play() blocked — user gesture required" });
      });
    } else {
      this.notify({ status: "ready", message: "Synthesis done" });
    }

    return { blob, url };
  }

  pause() {
    if (this.currentAudio && !this.currentAudio.paused) {
      this.currentAudio.pause();
      this.notify({ status: "ready", message: "Paused" });
    }
  }

  resume() {
    if (this.currentAudio && this.currentAudio.paused) {
      this.notify({ status: "playing", message: "Playing..." });
      void this.currentAudio.play();
    }
  }

  stop() {
    if (this.currentAudio) {
      this.currentAudio.pause();
      this.currentAudio.currentTime = 0;
      this.currentAudio = null;
    }
    if (this.currentObjectUrl) {
      URL.revokeObjectURL(this.currentObjectUrl);
      this.currentObjectUrl = null;
    }
  }

  unload() {
    this.stop();
    this.tts = null;
    this._status = "idle";
    this.notify({ status: "idle", message: "TTS unloaded" });
  }
}

export const ttsService = new TTSService();

export function useTTS() {
  const [status, setStatus] = createSignal<TtsProgress>({ status: "idle" });
  const [isWebGPUSupported, setIsWebGPUSupported] = createSignal<boolean | null>(null);

  createEffect(() => {
    ttsService.checkWebGPU().then(setIsWebGPUSupported);
    const unsubscribe = ttsService.onProgress(setStatus);
    onCleanup(unsubscribe);
  });

  return {
    status,
    isWebGPUSupported,
    voices: ttsService.listVoices(),
    loadModel: (device?: TtsDevice) => ttsService.loadModel(device),
    speak: (text: string, opts?: TtsSpeakOptions) => ttsService.speak(text, opts),
    pause: () => ttsService.pause(),
    resume: () => ttsService.resume(),
    stop: () => ttsService.stop(),
    unload: () => ttsService.unload(),
  };
}
