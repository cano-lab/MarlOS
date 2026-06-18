/**
 * OCR Service using Tesseract.js
 * Runs entirely in the browser — no native deps, no API keys.
 * First use downloads a worker + the chosen language traineddata (~10–15MB per lang).
 */

import { createWorker, type Worker, type RecognizeResult, PSM } from "tesseract.js";
import { createSignal, onCleanup, createEffect } from "solid-js";

export type OcrStatus = "idle" | "loading" | "ready" | "recognizing" | "error";

export interface OcrProgress {
  status: OcrStatus;
  progress?: number;
  message?: string;
}

export interface OcrResult {
  text: string;
  confidence: number;
  words: Array<{ text: string; confidence: number; bbox: { x0: number; y0: number; x1: number; y1: number } }>;
}

export type OcrInput = HTMLCanvasElement | HTMLImageElement | ImageBitmap | Blob | File | string;

class OCRService {
  private worker: Worker | null = null;
  private currentLang: string = "eng";
  private progressCallbacks: Array<(p: OcrProgress) => void> = [];
  private _status: OcrStatus = "idle";

  get status() {
    return this._status;
  }

  onProgress(cb: (p: OcrProgress) => void) {
    this.progressCallbacks.push(cb);
    return () => {
      this.progressCallbacks = this.progressCallbacks.filter((c) => c !== cb);
    };
  }

  private notify(p: OcrProgress) {
    this._status = p.status;
    this.progressCallbacks.forEach((cb) => cb(p));
  }

  /**
   * Initialize the Tesseract worker for a given language (default English).
   * Pass multiple langs joined with `+` (e.g. "eng+fra") to enable multi-language OCR.
   */
  async loadWorker(lang: string = "eng"): Promise<void> {
    if (this.worker && this.currentLang === lang) {
      this.notify({ status: "ready", message: "OCR already loaded" });
      return;
    }

    if (this.worker) {
      await this.worker.terminate();
      this.worker = null;
    }

    try {
      this.notify({ status: "loading", progress: 0, message: `Loading OCR (${lang})...` });

      this.worker = await createWorker(lang, 1, {
        logger: (m: any) => {
          if (typeof m?.progress === "number") {
            this.notify({
              status: m.status === "recognizing text" ? "recognizing" : "loading",
              progress: m.progress,
              message: `${m.status}: ${Math.round(m.progress * 100)}%`,
            });
          }
        },
      });

      this.currentLang = lang;
      this.notify({ status: "ready", progress: 1, message: `OCR ready (${lang})` });
    } catch (err) {
      const message = err instanceof Error ? err.message : "Unknown OCR error";
      this.notify({ status: "error", message });
      throw err;
    }
  }

  /**
   * Run OCR on an image source. Auto-loads the worker if needed.
   */
  async recognize(input: OcrInput, opts?: { psm?: PSM; lang?: string }): Promise<OcrResult> {
    const lang = opts?.lang ?? this.currentLang;
    if (!this.worker || this.currentLang !== lang) {
      await this.loadWorker(lang);
    }
    if (!this.worker) throw new Error("OCR worker failed to initialize");

    if (opts?.psm !== undefined) {
      await this.worker.setParameters({ tessedit_pageseg_mode: opts.psm });
    }

    this.notify({ status: "recognizing", progress: 0, message: "Recognizing text..." });

    const result: RecognizeResult = await this.worker.recognize(input as any);
    this.notify({ status: "ready", progress: 1, message: "Done" });

    return {
      text: result.data.text,
      confidence: result.data.confidence,
      words: (result.data.words || []).map((w) => ({
        text: w.text,
        confidence: w.confidence,
        bbox: w.bbox,
      })),
    };
  }

  /**
   * Convenience: OCR a base64 PNG/JPEG data string (no data URL prefix).
   */
  async recognizeBase64(base64: string, mime: string = "image/png"): Promise<OcrResult> {
    return this.recognize(`data:${mime};base64,${base64}`);
  }

  async terminate() {
    if (this.worker) {
      await this.worker.terminate();
      this.worker = null;
    }
    this._status = "idle";
    this.notify({ status: "idle", message: "OCR terminated" });
  }
}

export const ocrService = new OCRService();

export function useOCR() {
  const [status, setStatus] = createSignal<OcrProgress>({ status: "idle" });

  createEffect(() => {
    const unsubscribe = ocrService.onProgress(setStatus);
    onCleanup(unsubscribe);
  });

  return {
    status,
    loadWorker: (lang?: string) => ocrService.loadWorker(lang),
    recognize: (input: OcrInput, opts?: { psm?: PSM; lang?: string }) => ocrService.recognize(input, opts),
    recognizeBase64: (b64: string, mime?: string) => ocrService.recognizeBase64(b64, mime),
    terminate: () => ocrService.terminate(),
  };
}

export { PSM };
