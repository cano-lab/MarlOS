/**
 * Voice cloning service — talks to the Rust-side F5-TTS sidecar via Tauri.
 *
 * The Rust sidecar lazily spawns a Python child process and routes JSON-RPC.
 * First synthesis call after a fresh app start can take ~30s while F5-TTS loads.
 */

import { invoke } from "@tauri-apps/api/core";
import { createSignal, onCleanup, createEffect } from "solid-js";

export interface VoiceTtsStatus {
  running: boolean;
  model_loaded: boolean;
  has_reference: boolean;
  reference_audio_path: string | null;
  reference_transcript: string | null;
  python_path: string | null;
}

export interface SynthesizeResult {
  wav_b64: string;
  sample_rate: number;
  samples: number;
}

export interface SynthesizeOptions {
  speed?: number;
  /** Number of function evaluations — higher = better quality, slower. F5-TTS default is 32. */
  nfe?: number;
}

export interface VoiceCloneSettings {
  speed: number;
  nfe: number;
}

const SETTINGS_KEY = "marlos_voice_clone_settings";
const DEFAULT_SETTINGS: VoiceCloneSettings = { speed: 1.0, nfe: 32 };

function loadSettings(): VoiceCloneSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    const parsed = JSON.parse(raw);
    return {
      speed: typeof parsed.speed === "number" ? parsed.speed : DEFAULT_SETTINGS.speed,
      nfe: typeof parsed.nfe === "number" ? parsed.nfe : DEFAULT_SETTINGS.nfe,
    };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

function saveSettings(s: VoiceCloneSettings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    // ignore quota errors
  }
}

class VoiceCloneService {
  private currentAudio: HTMLAudioElement | null = null;
  private currentObjectUrl: string | null = null;
  private settings: VoiceCloneSettings = loadSettings();
  private settingsListeners: Array<(s: VoiceCloneSettings) => void> = [];

  getSettings(): VoiceCloneSettings {
    return { ...this.settings };
  }

  setSettings(patch: Partial<VoiceCloneSettings>): VoiceCloneSettings {
    this.settings = { ...this.settings, ...patch };
    saveSettings(this.settings);
    this.settingsListeners.forEach((cb) => cb({ ...this.settings }));
    return { ...this.settings };
  }

  onSettingsChange(cb: (s: VoiceCloneSettings) => void): () => void {
    this.settingsListeners.push(cb);
    return () => {
      this.settingsListeners = this.settingsListeners.filter((c) => c !== cb);
    };
  }

  async status(): Promise<VoiceTtsStatus> {
    return invoke<VoiceTtsStatus>("voice_tts_status");
  }

  /**
   * Encode a Float32Array (mono) to a WAV file (PCM 16-bit) and return as base64.
   * F5-TTS prefers a clean 24kHz mono reference, but it resamples internally.
   */
  encodeWav(samples: Float32Array, sampleRate: number): Uint8Array {
    const numChannels = 1;
    const bitsPerSample = 16;
    const byteRate = (sampleRate * numChannels * bitsPerSample) / 8;
    const blockAlign = (numChannels * bitsPerSample) / 8;
    const dataSize = samples.length * 2;
    const buffer = new ArrayBuffer(44 + dataSize);
    const view = new DataView(buffer);

    const writeString = (off: number, s: string) => {
      for (let i = 0; i < s.length; i++) view.setUint8(off + i, s.charCodeAt(i));
    };

    writeString(0, "RIFF");
    view.setUint32(4, 36 + dataSize, true);
    writeString(8, "WAVE");
    writeString(12, "fmt ");
    view.setUint32(16, 16, true); // PCM chunk size
    view.setUint16(20, 1, true); // PCM format
    view.setUint16(22, numChannels, true);
    view.setUint32(24, sampleRate, true);
    view.setUint32(28, byteRate, true);
    view.setUint16(32, blockAlign, true);
    view.setUint16(34, bitsPerSample, true);
    writeString(36, "data");
    view.setUint32(40, dataSize, true);

    let off = 44;
    for (let i = 0; i < samples.length; i++) {
      const s = Math.max(-1, Math.min(1, samples[i]));
      view.setInt16(off, s < 0 ? s * 0x8000 : s * 0x7fff, true);
      off += 2;
    }
    return new Uint8Array(buffer);
  }

  toBase64(bytes: Uint8Array): string {
    let binary = "";
    const chunk = 0x8000;
    for (let i = 0; i < bytes.length; i += chunk) {
      binary += String.fromCharCode.apply(
        null,
        bytes.subarray(i, Math.min(i + chunk, bytes.length)) as any
      );
    }
    return btoa(binary);
  }

  /**
   * Decode a recorded Blob (any browser-decodable format) into mono Float32 PCM
   * at the given target sample rate, then encode to WAV bytes.
   */
  async blobToWavBytes(blob: Blob, targetSampleRate: number = 24000): Promise<Uint8Array> {
    const arrayBuffer = await blob.arrayBuffer();
    const ctx = new (window.AudioContext || (window as any).webkitAudioContext)();
    const decoded = await ctx.decodeAudioData(arrayBuffer.slice(0));
    ctx.close();

    // Downmix to mono
    const channels = decoded.numberOfChannels;
    const length = decoded.length;
    const mono = new Float32Array(length);
    for (let c = 0; c < channels; c++) {
      const data = decoded.getChannelData(c);
      for (let i = 0; i < length; i++) mono[i] += data[i] / channels;
    }

    let resampled = mono;
    if (decoded.sampleRate !== targetSampleRate) {
      const ratio = targetSampleRate / decoded.sampleRate;
      const newLen = Math.round(mono.length * ratio);
      resampled = new Float32Array(newLen);
      for (let i = 0; i < newLen; i++) {
        const srcIdx = i / ratio;
        const i0 = Math.floor(srcIdx);
        const i1 = Math.min(i0 + 1, mono.length - 1);
        const frac = srcIdx - i0;
        resampled[i] = mono[i0] * (1 - frac) + mono[i1] * frac;
      }
    }

    return this.encodeWav(resampled, targetSampleRate);
  }

  async setReferenceFromBlob(blob: Blob, transcript: string): Promise<void> {
    const wav = await this.blobToWavBytes(blob, 24000);
    const audio_b64 = this.toBase64(wav);
    await invoke("voice_tts_set_reference", { audioB64: audio_b64, transcript });
  }

  async clearReference(): Promise<void> {
    return invoke("voice_tts_clear_reference");
  }

  async synthesize(text: string, opts: SynthesizeOptions = {}): Promise<SynthesizeResult> {
    return invoke<SynthesizeResult>("voice_tts_synthesize", {
      text,
      speed: opts.speed ?? this.settings.speed,
      nfe: opts.nfe ?? this.settings.nfe,
    });
  }

  /**
   * Synthesize and play. Returns the result so callers can also save/download.
   */
  async speak(text: string, opts: SynthesizeOptions = {}): Promise<SynthesizeResult> {
    const result = await this.synthesize(text, opts);
    this.stop();

    const bin = atob(result.wav_b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    const blob = new Blob([bytes], { type: "audio/wav" });
    const url = URL.createObjectURL(blob);
    this.currentObjectUrl = url;
    this.currentAudio = new Audio(url);
    await this.currentAudio.play().catch((e) => {
      console.warn("Voice clone audio play() failed:", e);
    });
    return result;
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
}

export const voiceCloneService = new VoiceCloneService();

export function useVoiceClone() {
  const [status, setStatus] = createSignal<VoiceTtsStatus | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [settings, setSettingsSignal] = createSignal<VoiceCloneSettings>(
    voiceCloneService.getSettings()
  );

  const refresh = async () => {
    try {
      setStatus(await voiceCloneService.status());
    } catch (e) {
      console.error("voice-clone status failed:", e);
    }
  };

  createEffect(() => {
    void refresh();
    const unsub = voiceCloneService.onSettingsChange(setSettingsSignal);
    onCleanup(() => {
      unsub();
      voiceCloneService.stop();
    });
  });

  return {
    status,
    busy,
    settings,
    updateSettings: (patch: Partial<VoiceCloneSettings>) => {
      setSettingsSignal(voiceCloneService.setSettings(patch));
    },
    refresh,
    setReferenceFromBlob: async (blob: Blob, transcript: string) => {
      setBusy(true);
      try {
        await voiceCloneService.setReferenceFromBlob(blob, transcript);
        await refresh();
      } finally {
        setBusy(false);
      }
    },
    clearReference: async () => {
      setBusy(true);
      try {
        await voiceCloneService.clearReference();
        await refresh();
      } finally {
        setBusy(false);
      }
    },
    synthesize: (text: string, opts?: SynthesizeOptions) =>
      voiceCloneService.synthesize(text, opts),
    speak: async (text: string, opts?: SynthesizeOptions) => {
      setBusy(true);
      try {
        return await voiceCloneService.speak(text, opts);
      } finally {
        setBusy(false);
      }
    },
    stop: () => voiceCloneService.stop(),
  };
}
