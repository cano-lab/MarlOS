import { Component, createSignal, createEffect, createMemo, Show, For, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./EmbeddingExplorer.css";

interface SearchResult {
  object: {
    id: string;
    kind: string;
    content: string;
    tags: string[];
  };
  score: number;
}

// Color mapping for embedding values
const valToColor = (val: number, min: number, max: number): string => {
  const t = (val - min) / (max - min || 1);
  if (t < 0.5) {
    const s = t * 2;
    return `rgb(${Math.round(15 + s * 20)}, ${Math.round(20 + s * 60)}, ${Math.round(80 + s * 100)})`;
  } else {
    const s = (t - 0.5) * 2;
    return `rgb(${Math.round(35 + s * 220)}, ${Math.round(80 + s * 100)}, ${Math.round(180 - s * 140)})`;
  }
};

// Generate test vectors
const generateVector = (type: string, length: number = 1024): number[] => {
  const v = new Array(length);
  if (type === "random") {
    for (let i = 0; i < length; i++) v[i] = (Math.random() - 0.5) * 2;
  } else if (type === "sparse") {
    for (let i = 0; i < length; i++) v[i] = Math.random() < 0.1 ? (Math.random() - 0.5) * 4 : 0;
  } else if (type === "clustered") {
    for (let i = 0; i < length; i++) {
      const cluster = Math.floor(i / 128);
      v[i] = (Math.random() - 0.5) * 2 + Math.sin(cluster * 1.2) * 1.5;
    }
  } else if (type === "smooth") {
    for (let i = 0; i < length; i++) {
      v[i] = Math.sin(i * 0.05) * Math.cos(i * 0.02) + (Math.random() - 0.5) * 0.3;
    }
  } else if (type === "harmonic") {
    for (let i = 0; i < length; i++) {
      v[i] = Math.sin(i * 0.03) * 0.5 + Math.sin(i * 0.07) * 0.3 + Math.sin(i * 0.13) * 0.2;
    }
  }
  return v;
};

const VECTOR_TYPES = ["random", "sparse", "clustered", "smooth", "harmonic"] as const;

// Vector math utilities
const magnitude = (v: number[]): number => Math.sqrt(v.reduce((s, x) => s + x * x, 0));
const dotProduct = (a: number[], b: number[]): number => a.reduce((s, x, i) => s + x * b[i], 0);
const cosineSim = (a: number[], b: number[]): number => {
  const dot = dotProduct(a, b);
  const m = magnitude(a) * magnitude(b);
  return m > 0 ? dot / m : 0;
};
const lerp = (a: number[], b: number[], t: number): number[] => a.map((v, i) => v * (1 - t) + b[i] * t);

// Smoothing functions
const smoothMovingAverage = (v: number[], windowSize: number): number[] => {
  if (windowSize <= 1) return v;
  const half = Math.floor(windowSize / 2);
  return v.map((_, i) => {
    let sum = 0, count = 0;
    for (let j = Math.max(0, i - half); j <= Math.min(v.length - 1, i + half); j++) {
      sum += v[j];
      count++;
    }
    return sum / count;
  });
};

const smoothGaussian = (v: number[], sigma: number): number[] => {
  if (sigma <= 0) return v;
  const kernelSize = Math.ceil(sigma * 3) * 2 + 1;
  const kernel: number[] = [];
  let sum = 0;
  for (let i = 0; i < kernelSize; i++) {
    const x = i - Math.floor(kernelSize / 2);
    const g = Math.exp(-(x * x) / (2 * sigma * sigma));
    kernel.push(g);
    sum += g;
  }
  kernel.forEach((_, i) => kernel[i] /= sum);

  const half = Math.floor(kernelSize / 2);
  return v.map((_, i) => {
    let result = 0;
    for (let k = 0; k < kernelSize; k++) {
      const j = Math.min(Math.max(0, i - half + k), v.length - 1);
      result += v[j] * kernel[k];
    }
    return result;
  });
};

const smoothSavitzkyGolay = (v: number[], windowSize: number): number[] => {
  if (windowSize <= 2) return v;
  const half = Math.floor(windowSize / 2);
  return v.map((_, i) => {
    const start = Math.max(0, i - half);
    const end = Math.min(v.length - 1, i + half);
    const n = end - start + 1;
    if (n < 3) return v[i];
    let sumX = 0, sumX2 = 0, sumX3 = 0, sumX4 = 0;
    let sumY = 0, sumXY = 0, sumX2Y = 0;
    for (let j = start; j <= end; j++) {
      const x = j - i;
      sumX += x; sumX2 += x*x; sumX3 += x*x*x; sumX4 += x*x*x*x;
      sumY += v[j]; sumXY += x * v[j]; sumX2Y += x*x * v[j];
    }
    const det = n * (sumX2 * sumX4 - sumX3 * sumX3) - sumX * (sumX * sumX4 - sumX2 * sumX3) + sumX2 * (sumX * sumX3 - sumX2 * sumX2);
    if (Math.abs(det) < 1e-10) return v[i];
    return (sumY * (sumX2 * sumX4 - sumX3 * sumX3) - sumXY * (sumX * sumX4 - sumX2 * sumX3) + sumX2Y * (sumX * sumX3 - sumX2 * sumX2)) / det;
  });
};

type SmoothingType = "none" | "moving" | "gaussian" | "savgol";

const applySmoothing = (v: number[], type: SmoothingType, strength: number): number[] => {
  switch (type) {
    case "moving": return smoothMovingAverage(v, Math.round(strength * 20) + 1);
    case "gaussian": return smoothGaussian(v, strength * 8);
    case "savgol": return smoothSavitzkyGolay(v, Math.round(strength * 15) + 3);
    default: return v;
  }
};

// =====================
// ANALYSIS FUNCTIONS
// =====================

// FFT (Cooley-Tukey radix-2 decimation-in-time)
interface ComplexNum { re: number; im: number; }

const fft = (signal: number[]): ComplexNum[] => {
  const n = Math.pow(2, Math.ceil(Math.log2(signal.length)));
  const padded = [...signal, ...new Array(n - signal.length).fill(0)];

  const output: ComplexNum[] = padded.map(x => ({ re: x, im: 0 }));
  const bits = Math.log2(n);
  for (let i = 0; i < n; i++) {
    let rev = 0;
    for (let j = 0; j < bits; j++) {
      rev = (rev << 1) | ((i >> j) & 1);
    }
    if (rev > i) [output[i], output[rev]] = [output[rev], output[i]];
  }

  for (let size = 2; size <= n; size *= 2) {
    const halfSize = size / 2;
    const angleStep = -2 * Math.PI / size;
    for (let i = 0; i < n; i += size) {
      for (let j = 0; j < halfSize; j++) {
        const angle = angleStep * j;
        const twiddle = { re: Math.cos(angle), im: Math.sin(angle) };
        const even = output[i + j];
        const odd = output[i + j + halfSize];
        const t = {
          re: odd.re * twiddle.re - odd.im * twiddle.im,
          im: odd.re * twiddle.im + odd.im * twiddle.re
        };
        output[i + j] = { re: even.re + t.re, im: even.im + t.im };
        output[i + j + halfSize] = { re: even.re - t.re, im: even.im - t.im };
      }
    }
  }
  return output;
};

const fftMagnitude = (signal: number[]): number[] => {
  const spectrum = fft(signal);
  return spectrum.slice(0, spectrum.length / 2).map(c =>
    Math.sqrt(c.re * c.re + c.im * c.im) / signal.length
  );
};

// Fourier series fitting (sine + cosine waves)
interface FourierTerm {
  frequency: number;  // Which harmonic (1, 2, 3, ...)
  cosCoeff: number;   // aₙ coefficient
  sinCoeff: number;   // bₙ coefficient
  amplitude: number;  // √(aₙ² + bₙ²)
  phase: number;      // atan2(bₙ, aₙ)
}

interface FourierFit {
  dc: number;              // a₀ (constant/average term)
  terms: FourierTerm[];    // Harmonic terms sorted by amplitude
  fitted: number[];        // Reconstructed signal
  numTerms: number;        // How many terms used
  r2: number;              // Goodness of fit
}

const fitFourier = (y: number[], maxTerms: number = 20): FourierFit => {
  const n = y.length;

  // Calculate DC component (average)
  const dc = y.reduce((a, b) => a + b, 0) / n;

  // Calculate Fourier coefficients for each frequency
  const allTerms: FourierTerm[] = [];
  const maxFreq = Math.floor(n / 2); // Nyquist limit

  for (let k = 1; k <= Math.min(maxFreq, 100); k++) {
    let cosSum = 0, sinSum = 0;
    for (let i = 0; i < n; i++) {
      const angle = (2 * Math.PI * k * i) / n;
      cosSum += y[i] * Math.cos(angle);
      sinSum += y[i] * Math.sin(angle);
    }
    const cosCoeff = (2 * cosSum) / n;
    const sinCoeff = (2 * sinSum) / n;
    const amplitude = Math.sqrt(cosCoeff * cosCoeff + sinCoeff * sinCoeff);
    const phase = Math.atan2(sinCoeff, cosCoeff);

    allTerms.push({ frequency: k, cosCoeff, sinCoeff, amplitude, phase });
  }

  // Sort by amplitude and take top terms
  allTerms.sort((a, b) => b.amplitude - a.amplitude);
  const terms = allTerms.slice(0, maxTerms);

  // Reconstruct signal using selected terms
  const fitted = new Array(n).fill(dc);
  for (const term of terms) {
    for (let i = 0; i < n; i++) {
      const angle = (2 * Math.PI * term.frequency * i) / n;
      fitted[i] += term.cosCoeff * Math.cos(angle) + term.sinCoeff * Math.sin(angle);
    }
  }

  // Calculate R²
  const yMean = y.reduce((a, b) => a + b, 0) / n;
  const ssTot = y.reduce((s, yi) => s + (yi - yMean) ** 2, 0);
  const ssRes = y.reduce((s, yi, i) => s + (yi - fitted[i]) ** 2, 0);
  const r2 = 1 - ssRes / (ssTot || 1);

  return { dc, terms, fitted, numTerms: terms.length, r2 };
};

// Format Fourier series as formula string
const formatFourierFormula = (fit: FourierFit, maxDisplay: number = 5): string => {
  const parts: string[] = [];

  // DC term
  if (Math.abs(fit.dc) > 0.001) {
    parts.push(fit.dc.toFixed(3));
  }

  // Harmonic terms (show top ones by amplitude)
  const displayTerms = fit.terms.slice(0, maxDisplay);
  for (const term of displayTerms) {
    if (Math.abs(term.cosCoeff) > 0.001) {
      const sign = term.cosCoeff >= 0 && parts.length > 0 ? "+" : "";
      parts.push(`${sign}${term.cosCoeff.toFixed(3)}cos(${term.frequency}ω)`);
    }
    if (Math.abs(term.sinCoeff) > 0.001) {
      const sign = term.sinCoeff >= 0 ? "+" : "";
      parts.push(`${sign}${term.sinCoeff.toFixed(3)}sin(${term.frequency}ω)`);
    }
  }

  if (fit.terms.length > maxDisplay) {
    parts.push(`... +${fit.terms.length - maxDisplay} more`);
  }

  return parts.join(" ") || "0";
};

// Haar wavelet transform
interface WaveletResult {
  coefficients: number[][];
  approximation: number[];
  levels: number;
}

const haarWavelet = (signal: number[], maxLevels: number = 6): WaveletResult => {
  const n = Math.pow(2, Math.ceil(Math.log2(signal.length)));
  let current = [...signal, ...new Array(n - signal.length).fill(0)];

  const coefficients: number[][] = [];
  const levels = Math.min(maxLevels, Math.log2(n));

  for (let level = 0; level < levels; level++) {
    const len = current.length;
    const approx: number[] = [];
    const detail: number[] = [];

    for (let i = 0; i < len; i += 2) {
      approx.push((current[i] + current[i + 1]) / Math.sqrt(2));
      detail.push((current[i] - current[i + 1]) / Math.sqrt(2));
    }

    coefficients.push(detail);
    current = approx;
  }

  return { coefficients, approximation: current, levels };
};

// Peak detection
interface Peak {
  index: number;
  value: number;
  prominence: number;
}

const findPeaks = (signal: number[], minProminence: number = 0.1): Peak[] => {
  const peaks: Peak[] = [];
  const absMax = Math.max(...signal.map(Math.abs));
  const threshold = absMax * minProminence;

  for (let i = 1; i < signal.length - 1; i++) {
    if (signal[i] > signal[i - 1] && signal[i] > signal[i + 1]) {
      let leftMin = signal[i], rightMin = signal[i];
      for (let j = i - 1; j >= 0 && signal[j] < signal[i]; j--) leftMin = Math.min(leftMin, signal[j]);
      for (let j = i + 1; j < signal.length && signal[j] < signal[i]; j++) rightMin = Math.min(rightMin, signal[j]);
      const prominence = signal[i] - Math.max(leftMin, rightMin);
      if (prominence > threshold) peaks.push({ index: i, value: signal[i], prominence });
    }
    if (signal[i] < signal[i - 1] && signal[i] < signal[i + 1]) {
      let leftMax = signal[i], rightMax = signal[i];
      for (let j = i - 1; j >= 0 && signal[j] > signal[i]; j--) leftMax = Math.max(leftMax, signal[j]);
      for (let j = i + 1; j < signal.length && signal[j] > signal[i]; j++) rightMax = Math.max(rightMax, signal[j]);
      const prominence = Math.min(leftMax, rightMax) - signal[i];
      if (prominence > threshold) peaks.push({ index: i, value: signal[i], prominence: -prominence });
    }
  }

  return peaks.sort((a, b) => Math.abs(b.prominence) - Math.abs(a.prominence)).slice(0, 20);
};

// =====================
// AUDIO SYNTHESIS
// =====================

type PlaybackMode = "waveform" | "frequency" | "melody" | "drone";

interface AudioEngine {
  context: AudioContext | null;
  isPlaying: boolean;
  stop: () => void;
}

const createAudioEngine = (): AudioEngine => {
  return {
    context: null,
    isPlaying: false,
    stop: () => {}
  };
};

// Play embedding as raw waveform (looped)
const playWaveform = (
  engine: AudioEngine,
  vector: number[],
  duration: number = 2,
  sampleRate: number = 22050
): void => {
  if (engine.isPlaying) engine.stop();

  const ctx = new AudioContext({ sampleRate });
  engine.context = ctx;
  engine.isPlaying = true;

  // Normalize vector to [-1, 1]
  const absMax = Math.max(...vector.map(Math.abs)) || 1;
  const normalized = vector.map(v => v / absMax * 0.8);

  // Stretch/compress to fill duration
  const totalSamples = Math.floor(sampleRate * duration);
  const buffer = ctx.createBuffer(1, totalSamples, sampleRate);
  const data = buffer.getChannelData(0);

  for (let i = 0; i < totalSamples; i++) {
    const srcIdx = (i / totalSamples) * normalized.length;
    const idx0 = Math.floor(srcIdx);
    const idx1 = Math.min(idx0 + 1, normalized.length - 1);
    const t = srcIdx - idx0;
    data[i] = normalized[idx0] * (1 - t) + normalized[idx1] * t;
  }

  // Apply fade in/out to avoid clicks
  const fadeLen = Math.floor(sampleRate * 0.02);
  for (let i = 0; i < fadeLen; i++) {
    const t = i / fadeLen;
    data[i] *= t;
    data[totalSamples - 1 - i] *= t;
  }

  const source = ctx.createBufferSource();
  source.buffer = buffer;
  source.loop = true;

  const gain = ctx.createGain();
  gain.gain.value = 0.5;

  source.connect(gain);
  gain.connect(ctx.destination);
  source.start();

  engine.stop = () => {
    source.stop();
    ctx.close();
    engine.isPlaying = false;
  };
};

// Play embedding using FFT frequencies as oscillators
const playFrequencies = (
  engine: AudioEngine,
  vector: number[],
  baseFreq: number = 110,
  numVoices: number = 8
): void => {
  if (engine.isPlaying) engine.stop();

  const ctx = new AudioContext();
  engine.context = ctx;
  engine.isPlaying = true;

  const spectrum = fftMagnitude(vector);
  const indexed = spectrum.map((m, i) => ({ m, i })).sort((a, b) => b.m - a.m);
  const topFreqs = indexed.slice(0, numVoices);

  const masterGain = ctx.createGain();
  masterGain.gain.value = 0.3 / numVoices;
  masterGain.connect(ctx.destination);

  const oscillators: OscillatorNode[] = [];

  topFreqs.forEach(({ m, i }, idx) => {
    const osc = ctx.createOscillator();
    const freq = baseFreq * (1 + i * 0.1); // Map index to frequency
    osc.frequency.value = freq;
    osc.type = idx % 2 === 0 ? "sine" : "triangle";

    const oscGain = ctx.createGain();
    oscGain.gain.value = m / (topFreqs[0].m || 1); // Relative amplitude

    osc.connect(oscGain);
    oscGain.connect(masterGain);
    osc.start();
    oscillators.push(osc);
  });

  engine.stop = () => {
    oscillators.forEach(o => o.stop());
    ctx.close();
    engine.isPlaying = false;
  };
};

// Play embedding as melody (dimensions become notes)
const playMelody = (
  engine: AudioEngine,
  vector: number[],
  tempo: number = 120,
  scale: number[] = [0, 2, 4, 5, 7, 9, 11] // Major scale
): void => {
  if (engine.isPlaying) engine.stop();

  const ctx = new AudioContext();
  engine.context = ctx;
  engine.isPlaying = true;

  const beatDuration = 60 / tempo;
  const noteDuration = beatDuration * 0.8;

  // Sample every Nth dimension for melody
  const step = Math.floor(vector.length / 32);
  const notes: number[] = [];
  for (let i = 0; i < vector.length; i += step) {
    notes.push(vector[i]);
  }

  // Map values to MIDI notes
  const absMax = Math.max(...notes.map(Math.abs)) || 1;
  const baseNote = 60; // Middle C
  // range: 2 octaves (24 semitones)

  const masterGain = ctx.createGain();
  masterGain.gain.value = 0.4;
  masterGain.connect(ctx.destination);

  let currentTime = ctx.currentTime + 0.1;
  const allOscs: OscillatorNode[] = [];

  notes.forEach((val, _idx) => {
    const normalized = (val / absMax + 1) / 2; // 0 to 1
    const scaleIdx = Math.floor(normalized * scale.length * 2);
    const octave = Math.floor(scaleIdx / scale.length);
    const noteInScale = scale[scaleIdx % scale.length];
    const midiNote = baseNote + octave * 12 + noteInScale;
    const freq = 440 * Math.pow(2, (midiNote - 69) / 12);

    const osc = ctx.createOscillator();
    osc.frequency.value = freq;
    osc.type = "sine";

    const env = ctx.createGain();
    env.gain.setValueAtTime(0, currentTime);
    env.gain.linearRampToValueAtTime(0.3, currentTime + 0.02);
    env.gain.exponentialRampToValueAtTime(0.01, currentTime + noteDuration);

    osc.connect(env);
    env.connect(masterGain);
    osc.start(currentTime);
    osc.stop(currentTime + noteDuration + 0.1);
    allOscs.push(osc);

    currentTime += beatDuration * 0.25; // 16th notes
  });

  const totalDuration = notes.length * beatDuration * 0.25 + 1;
  const stopTimeout = setTimeout(() => {
    if (engine.isPlaying) {
      engine.isPlaying = false;
    }
  }, totalDuration * 1000);

  engine.stop = () => {
    clearTimeout(stopTimeout);
    allOscs.forEach(o => { try { o.stop(); } catch {} });
    ctx.close();
    engine.isPlaying = false;
  };
};

// Play embedding as evolving drone
const playDrone = (
  engine: AudioEngine,
  vector: number[],
  baseFreq: number = 55
): void => {
  if (engine.isPlaying) engine.stop();

  const ctx = new AudioContext();
  engine.context = ctx;
  engine.isPlaying = true;

  // Divide vector into frequency bands
  const bands = 6;
  const bandSize = Math.floor(vector.length / bands);
  const bandAvgs: number[] = [];
  for (let b = 0; b < bands; b++) {
    let sum = 0;
    for (let i = 0; i < bandSize; i++) {
      sum += Math.abs(vector[b * bandSize + i]);
    }
    bandAvgs.push(sum / bandSize);
  }
  const maxAvg = Math.max(...bandAvgs) || 1;

  const masterGain = ctx.createGain();
  masterGain.gain.value = 0.25;
  masterGain.connect(ctx.destination);

  const oscillators: OscillatorNode[] = [];
  const harmonics = [1, 2, 3, 4, 5, 6];

  harmonics.forEach((h, idx) => {
    const osc = ctx.createOscillator();
    osc.frequency.value = baseFreq * h;
    osc.type = "sine";

    const oscGain = ctx.createGain();
    const amplitude = bandAvgs[idx] / maxAvg;
    oscGain.gain.value = amplitude * (1 / h); // Higher harmonics quieter

    // Add slow modulation
    const lfo = ctx.createOscillator();
    lfo.frequency.value = 0.1 + idx * 0.05;
    const lfoGain = ctx.createGain();
    lfoGain.gain.value = amplitude * 0.1;
    lfo.connect(lfoGain);
    lfoGain.connect(oscGain.gain);
    lfo.start();

    osc.connect(oscGain);
    oscGain.connect(masterGain);
    osc.start();
    oscillators.push(osc, lfo);
  });

  engine.stop = () => {
    oscillators.forEach(o => o.stop());
    ctx.close();
    engine.isPlaying = false;
  };
};

// =====================
// EMBEDDING FINGERPRINT
// =====================

interface EmbeddingFingerprint {
  // Basic stats
  dimensions: number;
  magnitude: number;
  mean: number;
  std: number;
  min: number;
  max: number;
  sparsity: number;      // % of values near zero
  kurtosis: number;      // Peakedness (high = spiky, low = flat)

  // FFT analysis
  dominantFreqs: { bin: number; magnitude: number }[];
  spectralCentroid: number;   // Center of mass of spectrum
  lowFreqEnergy: number;      // % energy in low frequencies
  highFreqEnergy: number;     // % energy in high frequencies

  // Peak analysis
  peakCount: number;
  valleyCount: number;
  avgPeakProminence: number;
  peakLocations: number[];    // Top peak dimension indices

  // Wavelet analysis
  waveletEnergy: number[];    // Energy at each decomposition level
  dominantScale: number;      // Which level has most energy

  // Fourier fit
  topHarmonics: { freq: number; cos: number; sin: number; amp: number }[];
}

const computeFingerprint = (vector: number[]): EmbeddingFingerprint => {
  const n = vector.length;

  // Basic stats
  const mag = magnitude(vector);
  const mean = vector.reduce((a, b) => a + b, 0) / n;
  const variance = vector.reduce((s, v) => s + (v - mean) ** 2, 0) / n;
  const std = Math.sqrt(variance);
  const min = Math.min(...vector);
  const max = Math.max(...vector);

  // Sparsity (% within 0.1 of zero)
  const nearZero = vector.filter(v => Math.abs(v) < 0.1).length;
  const sparsity = nearZero / n;

  // Kurtosis (normalized 4th moment)
  const m4 = vector.reduce((s, v) => s + ((v - mean) / (std || 1)) ** 4, 0) / n;
  const kurtosis = m4 - 3; // Excess kurtosis (0 = normal distribution)

  // FFT analysis
  const spectrum = fftMagnitude(vector);
  const totalEnergy = spectrum.reduce((s, m) => s + m * m, 0);

  // Spectral centroid
  let weightedSum = 0;
  spectrum.forEach((m, i) => weightedSum += i * m * m);
  const spectralCentroid = totalEnergy > 0 ? weightedSum / totalEnergy : 0;

  // Energy distribution
  const lowCutoff = Math.floor(spectrum.length * 0.25);
  const highCutoff = Math.floor(spectrum.length * 0.75);
  const lowEnergy = spectrum.slice(0, lowCutoff).reduce((s, m) => s + m * m, 0);
  const highEnergy = spectrum.slice(highCutoff).reduce((s, m) => s + m * m, 0);
  const lowFreqEnergy = totalEnergy > 0 ? lowEnergy / totalEnergy : 0;
  const highFreqEnergy = totalEnergy > 0 ? highEnergy / totalEnergy : 0;

  // Dominant frequencies
  const indexed = spectrum.map((m, i) => ({ bin: i, magnitude: m }));
  indexed.sort((a, b) => b.magnitude - a.magnitude);
  const dominantFreqs = indexed.slice(0, 5);

  // Peak analysis
  const peaks = findPeaks(vector, 0.1);
  const posPeaks = peaks.filter(p => p.prominence > 0);
  const negPeaks = peaks.filter(p => p.prominence < 0);
  const avgProm = peaks.length > 0
    ? peaks.reduce((s, p) => s + Math.abs(p.prominence), 0) / peaks.length
    : 0;

  // Wavelet analysis
  const wavelet = haarWavelet(vector, 6);
  const waveletEnergy = wavelet.coefficients.map(level =>
    level.reduce((s, c) => s + c * c, 0)
  );
  const maxEnergy = Math.max(...waveletEnergy);
  const dominantScale = waveletEnergy.indexOf(maxEnergy) + 1;

  // Fourier fit (top harmonics)
  const fourier = fitFourier(vector, 10);
  const topHarmonics = fourier.terms.slice(0, 5).map(t => ({
    freq: t.frequency,
    cos: t.cosCoeff,
    sin: t.sinCoeff,
    amp: t.amplitude
  }));

  return {
    dimensions: n,
    magnitude: mag,
    mean,
    std,
    min,
    max,
    sparsity,
    kurtosis,
    dominantFreqs,
    spectralCentroid,
    lowFreqEnergy,
    highFreqEnergy,
    peakCount: posPeaks.length,
    valleyCount: negPeaks.length,
    avgPeakProminence: avgProm,
    peakLocations: posPeaks.slice(0, 5).map(p => p.index),
    waveletEnergy,
    dominantScale,
    topHarmonics
  };
};

const formatFingerprintForLLM = (fp: EmbeddingFingerprint, name?: string): string => {
  const lines: string[] = [];

  if (name) {
    lines.push(`Embedding: "${name}"`);
  }
  lines.push(`Dimensions: ${fp.dimensions}`);
  lines.push('');
  lines.push('## Basic Statistics');
  lines.push(`- Magnitude: ${fp.magnitude.toFixed(3)}`);
  lines.push(`- Mean: ${fp.mean.toFixed(4)}, Std: ${fp.std.toFixed(4)}`);
  lines.push(`- Range: [${fp.min.toFixed(3)}, ${fp.max.toFixed(3)}]`);
  lines.push(`- Sparsity: ${(fp.sparsity * 100).toFixed(1)}% near zero`);
  lines.push(`- Kurtosis: ${fp.kurtosis.toFixed(2)} (${fp.kurtosis > 1 ? 'spiky/peaked' : fp.kurtosis < -1 ? 'flat/uniform' : 'normal-like'})`);

  lines.push('');
  lines.push('## Frequency Analysis (FFT)');
  lines.push(`- Spectral centroid: ${fp.spectralCentroid.toFixed(1)} (${fp.spectralCentroid < 100 ? 'low-frequency dominated' : fp.spectralCentroid > 200 ? 'high-frequency dominated' : 'balanced'})`);
  lines.push(`- Low-freq energy: ${(fp.lowFreqEnergy * 100).toFixed(1)}%`);
  lines.push(`- High-freq energy: ${(fp.highFreqEnergy * 100).toFixed(1)}%`);
  lines.push(`- Dominant frequency bins: [${fp.dominantFreqs.map(f => f.bin).join(', ')}]`);

  lines.push('');
  lines.push('## Peak Structure');
  lines.push(`- ${fp.peakCount} peaks, ${fp.valleyCount} valleys`);
  lines.push(`- Average prominence: ${fp.avgPeakProminence.toFixed(3)}`);
  if (fp.peakLocations.length > 0) {
    lines.push(`- Notable peak dimensions: [${fp.peakLocations.join(', ')}]`);
  }

  lines.push('');
  lines.push('## Multi-scale Structure (Wavelet)');
  lines.push(`- Energy by level: [${fp.waveletEnergy.map(e => e.toFixed(2)).join(', ')}]`);
  lines.push(`- Dominant scale: Level ${fp.dominantScale} (${fp.dominantScale <= 2 ? 'fine detail' : fp.dominantScale >= 5 ? 'coarse structure' : 'medium scale'})`);

  lines.push('');
  lines.push('## Fourier Decomposition');
  const harmonicStr = fp.topHarmonics.map(h =>
    `${h.amp.toFixed(3)}·sin(${h.freq}ω + φ)`
  ).join(' + ');
  lines.push(`- Top harmonics: ${harmonicStr}`);

  return lines.join('\n');
};

// Hilbert curve for 2D mapping
const hilbertD2xy = (n: number, d: number): [number, number] => {
  let x = 0, y = 0, rx: number, ry: number, s: number, t = d;
  for (s = 1; s < n; s *= 2) {
    rx = 1 & Math.floor(t / 2);
    ry = 1 & (t ^ rx);
    if (ry === 0) {
      if (rx === 1) { x = s - 1 - x; y = s - 1 - y; }
      [x, y] = [y, x];
    }
    x += s * rx;
    y += s * ry;
    t = Math.floor(t / 4);
  }
  return [x, y];
};

// Heatmap visualization
const HeatmapViz: Component<{ vector: number[]; compareVector?: number[] | null }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const vector = props.vector;
    const min = Math.min(...vector), max = Math.max(...vector);
    const cols = 64, rows = Math.ceil(vector.length / cols);
    const cellW = canvasRef.width / cols;
    const cellH = canvasRef.height / (props.compareVector ? rows * 2 + 1 : rows);

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, canvasRef.width, canvasRef.height);

    vector.forEach((val, i) => {
      ctx.fillStyle = valToColor(val, min, max);
      ctx.fillRect((i % cols) * cellW, Math.floor(i / cols) * cellH, cellW + 0.5, cellH + 0.5);
    });

    if (props.compareVector) {
      const cMin = Math.min(...props.compareVector), cMax = Math.max(...props.compareVector);
      const offset = (rows + 1) * cellH;
      props.compareVector.forEach((val, i) => {
        ctx.fillStyle = valToColor(val, cMin, cMax);
        ctx.fillRect((i % cols) * cellW, offset + Math.floor(i / cols) * cellH, cellW + 0.5, cellH + 0.5);
      });
    }
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={640} height={props.compareVector ? 400 : 200} />
      <div class="viz-label">dim 0 → 64×16 = {props.vector?.length || 0} → dim {(props.vector?.length || 1) - 1}</div>
    </div>
  );
};

// Hilbert curve visualization
const HilbertViz: Component<{ vector: number[] }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const size = canvasRef.width;
    const n = 32, cellSize = size / n;
    const min = Math.min(...props.vector), max = Math.max(...props.vector);

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, size, size);

    for (let d = 0; d < Math.min(1024, props.vector.length); d++) {
      const [x, y] = hilbertD2xy(n, d);
      ctx.fillStyle = valToColor(props.vector[d], min, max);
      ctx.fillRect(x * cellSize, y * cellSize, cellSize + 0.5, cellSize + 0.5);
    }
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={512} height={512} />
      <div class="viz-label">Hilbert curve — adjacent dims stay spatially close</div>
    </div>
  );
};

// Radial visualization
const RadialViz: Component<{ vector: number[]; compareVector?: number[] | null }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const size = canvasRef.width;
    const cx = size / 2, cy = size / 2, maxR = size * 0.42;

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, size, size);

    // Draw reference circles
    ctx.strokeStyle = "rgba(100,120,160,0.12)";
    ctx.lineWidth = 0.5;
    [0.25, 0.5, 0.75, 1].forEach(r => {
      ctx.beginPath();
      ctx.arc(cx, cy, maxR * r, 0, Math.PI * 2);
      ctx.stroke();
    });

    const drawShape = (v: number[], color: string, alpha: number) => {
      const absMax = Math.max(...v.map(Math.abs));
      ctx.beginPath();
      v.forEach((val, i) => {
        const angle = (i / v.length) * Math.PI * 2 - Math.PI / 2;
        const r = (Math.abs(val) / absMax) * maxR;
        const x = cx + Math.cos(angle) * r, y = cy + Math.sin(angle) * r;
        i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
      });
      ctx.closePath();
      ctx.fillStyle = color.replace(")", `,${alpha * 0.15})`).replace("rgb", "rgba");
      ctx.fill();
      ctx.strokeStyle = color.replace(")", `,${alpha})`).replace("rgb", "rgba");
      ctx.lineWidth = 0.8;
      ctx.stroke();
    };

    if (props.compareVector) drawShape(props.compareVector, "rgb(255,120,80)", 0.5);
    drawShape(props.vector, "rgb(80,160,255)", 0.7);
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={512} height={512} />
      <div class="viz-label">Each spoke = 1 dim, distance = absolute value</div>
    </div>
  );
};

// Waveform visualization with zoom and pan
const WaveformViz: Component<{ vector: number[]; compareVector?: number[] | null }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;
  const [zoom, setZoom] = createSignal(1);
  const [panX, setPanX] = createSignal(0);
  const [isDragging, setIsDragging] = createSignal(false);
  const [dragStartX, setDragStartX] = createSignal(0);
  const [dragStartPan, setDragStartPan] = createSignal(0);

  const draw = () => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const w = canvasRef.width, h = canvasRef.height;
    const z = zoom();
    const pan = panX();

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, w, h);

    // Center line
    ctx.strokeStyle = "rgba(100,120,160,0.2)";
    ctx.lineWidth = 0.5;
    ctx.beginPath();
    ctx.moveTo(0, h / 2);
    ctx.lineTo(w, h / 2);
    ctx.stroke();

    // Grid lines based on zoom
    ctx.strokeStyle = "rgba(100,120,160,0.1)";
    const gridStep = z > 4 ? 16 : z > 2 ? 32 : z > 1 ? 64 : 128;
    for (let i = 0; i < props.vector.length; i += gridStep) {
      const x = ((i / (props.vector.length - 1)) * w * z) - pan;
      if (x >= 0 && x <= w) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
        ctx.stroke();
        ctx.fillStyle = "rgba(100,120,160,0.5)";
        ctx.font = "9px monospace";
        ctx.fillText(String(i), x + 2, h - 4);
      }
    }

    const drawWave = (v: number[], color: string, lineWidth: number = 1.2) => {
      const absMax = Math.max(...v.map(Math.abs));
      ctx.beginPath();
      ctx.strokeStyle = color;
      ctx.lineWidth = lineWidth;

      let started = false;
      v.forEach((val, i) => {
        const x = ((i / (v.length - 1)) * w * z) - pan;
        const y = h / 2 - (val / absMax) * (h * 0.4);
        if (x >= -10 && x <= w + 10) {
          if (!started) { ctx.moveTo(x, y); started = true; }
          else { ctx.lineTo(x, y); }
        }
      });
      ctx.stroke();

      if (z >= 4) {
        v.forEach((val, i) => {
          const x = ((i / (v.length - 1)) * w * z) - pan;
          const y = h / 2 - (val / absMax) * (h * 0.4);
          if (x >= 0 && x <= w) {
            ctx.beginPath();
            ctx.arc(x, y, 3, 0, Math.PI * 2);
            ctx.fillStyle = color;
            ctx.fill();
          }
        });
      }
    };

    if (props.compareVector) drawWave(props.compareVector, "rgba(255,120,80,0.4)", 1);
    drawWave(props.vector, "rgba(80,160,255,0.85)", z >= 2 ? 1.5 : 1.2);

    ctx.fillStyle = "#667";
    ctx.font = "11px monospace";
    ctx.fillText(`Zoom: ${z.toFixed(1)}x`, 10, 16);
    const startDim = Math.floor((pan / (w * z)) * props.vector.length);
    const endDim = Math.ceil(((pan + w) / (w * z)) * props.vector.length);
    ctx.fillText(`Dims: ${Math.max(0, startDim)}-${Math.min(props.vector.length, endDim)}`, 10, 30);
  };

  createEffect(draw);

  const handleWheel = (e: WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    const newZoom = Math.max(1, Math.min(32, zoom() * delta));
    if (canvasRef) {
      const rect = canvasRef.getBoundingClientRect();
      const mouseX = e.clientX - rect.left;
      const newPan = mouseX * (newZoom / zoom() - 1) + panX() * (newZoom / zoom());
      setPanX(Math.max(0, Math.min(newPan, canvasRef.width * newZoom - canvasRef.width)));
    }
    setZoom(newZoom);
  };

  const handleMouseDown = (e: MouseEvent) => {
    setIsDragging(true);
    setDragStartX(e.clientX);
    setDragStartPan(panX());
  };

  const handleMouseMove = (e: MouseEvent) => {
    if (!isDragging()) return;
    const dx = dragStartX() - e.clientX;
    if (canvasRef) {
      setPanX(Math.max(0, Math.min(dragStartPan() + dx, canvasRef.width * zoom() - canvasRef.width)));
    }
  };

  const handleMouseUp = () => setIsDragging(false);
  const resetView = () => { setZoom(1); setPanX(0); };

  return (
    <div class="viz-container waveform-viz">
      <div class="zoom-controls">
        <button onClick={() => setZoom(z => Math.min(32, z * 1.5))}>+</button>
        <button onClick={() => setZoom(z => Math.max(1, z / 1.5))}>−</button>
        <button onClick={resetView}>Reset</button>
      </div>
      <canvas
        ref={canvasRef}
        width={1024}
        height={300}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        style={{ cursor: isDragging() ? "grabbing" : zoom() > 1 ? "grab" : "default" }}
      />
      <div class="viz-label">Scroll to zoom, drag to pan (all {props.vector?.length || 0} dimensions)</div>
    </div>
  );
};

// Distribution visualization
const DistributionViz: Component<{ vector: number[]; compareVector?: number[] | null }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const w = canvasRef.width, h = canvasRef.height;

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, w, h);

    const drawHist = (v: number[], color: string) => {
      const bins = 60;
      const min = Math.min(...v), max = Math.max(...v), range = max - min || 1;
      const counts = new Array(bins).fill(0);
      v.forEach(val => {
        counts[Math.min(Math.floor(((val - min) / range) * bins), bins - 1)]++;
      });
      const maxCount = Math.max(...counts);
      ctx.fillStyle = color;
      counts.forEach((count, i) => {
        ctx.fillRect((i / bins) * w, h - (count / maxCount) * h * 0.85, w / bins - 1, (count / maxCount) * h * 0.85);
      });
    };

    if (props.compareVector) drawHist(props.compareVector, "rgba(255,120,80,0.3)");
    drawHist(props.vector, "rgba(80,160,255,0.6)");

    // Stats
    const mean = props.vector.reduce((a, b) => a + b, 0) / props.vector.length;
    const std = Math.sqrt(props.vector.reduce((s, x) => s + (x - mean) ** 2, 0) / props.vector.length);
    ctx.fillStyle = "#8899aa";
    ctx.font = "11px monospace";
    ctx.fillText(`μ=${mean.toFixed(3)}  σ=${std.toFixed(3)}  mag=${magnitude(props.vector).toFixed(2)}`, 10, 16);
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={600} height={200} />
      <div class="viz-label">Value distribution across {props.vector?.length || 0} dimensions</div>
    </div>
  );
};

// FFT Spectrum visualization
const FFTViz: Component<{ vector: number[] }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const w = canvasRef.width, h = canvasRef.height;

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, w, h);

    const spectrum = fftMagnitude(props.vector);
    const maxMag = Math.max(...spectrum);

    // Draw frequency bars
    const barWidth = w / spectrum.length;
    spectrum.forEach((mag, i) => {
      const barH = (mag / maxMag) * h * 0.9;
      const hue = 220 + (i / spectrum.length) * 60; // Blue to purple
      ctx.fillStyle = `hsla(${hue}, 70%, 55%, 0.8)`;
      ctx.fillRect(i * barWidth, h - barH, Math.max(1, barWidth - 0.5), barH);
    });

    // Frequency axis labels
    ctx.fillStyle = "#667";
    ctx.font = "10px monospace";
    const nyquist = spectrum.length;
    [0, 0.25, 0.5, 0.75, 1].forEach(f => {
      const x = f * w;
      ctx.fillText(`${Math.round(f * nyquist)}`, x + 2, h - 4);
    });

    // Find dominant frequencies
    const indexed = spectrum.map((m, i) => ({ m, i })).sort((a, b) => b.m - a.m);
    const top3 = indexed.slice(0, 3);
    ctx.fillStyle = "#9cf";
    ctx.fillText(`Top: ${top3.map(t => `f${t.i}`).join(", ")}`, 10, 16);
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={800} height={250} />
      <div class="viz-label">Frequency spectrum (FFT magnitude) — X = frequency bin, Y = magnitude</div>
    </div>
  );
};

// Fourier series visualization
const FourierViz: Component<{ vector: number[] }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;
  const [numTerms, setNumTerms] = createSignal(10);
  const [showFormula, setShowFormula] = createSignal(true);

  const fourierFit = createMemo(() => {
    if (!props.vector) return null;
    return fitFourier(props.vector, numTerms());
  });

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const w = canvasRef.width, h = canvasRef.height;
    const fit = fourierFit();
    if (!fit) return;

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, w, h);

    const absMax = Math.max(...props.vector.map(Math.abs), ...fit.fitted.map(Math.abs));

    // Center line
    ctx.strokeStyle = "rgba(100,120,160,0.2)";
    ctx.lineWidth = 0.5;
    ctx.beginPath();
    ctx.moveTo(0, h / 2);
    ctx.lineTo(w, h / 2);
    ctx.stroke();

    // Draw original signal (faint blue)
    ctx.beginPath();
    ctx.strokeStyle = "rgba(80,160,255,0.35)";
    ctx.lineWidth = 1;
    props.vector.forEach((val, i) => {
      const x = (i / (props.vector.length - 1)) * w;
      const y = h / 2 - (val / absMax) * (h * 0.4);
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    });
    ctx.stroke();

    // Draw fitted Fourier series (bright orange)
    ctx.beginPath();
    ctx.strokeStyle = "rgba(255,180,80,0.95)";
    ctx.lineWidth = 2;
    fit.fitted.forEach((val, i) => {
      const x = (i / (fit.fitted.length - 1)) * w;
      const y = h / 2 - (val / absMax) * (h * 0.4);
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    });
    ctx.stroke();

    // Stats
    ctx.fillStyle = "#8899aa";
    ctx.font = "11px monospace";
    ctx.fillText(`${fit.numTerms} harmonics  R²: ${fit.r2.toFixed(4)}`, 10, 16);

    // Top frequencies
    ctx.fillStyle = "#9cf";
    ctx.font = "10px monospace";
    const topFreqs = fit.terms.slice(0, 5).map(t => `f${t.frequency}:${t.amplitude.toFixed(2)}`).join("  ");
    ctx.fillText(`Dominant: ${topFreqs}`, 10, 32);
  });

  return (
    <div class="viz-container">
      <div class="fourier-controls">
        <span>Harmonics:</span>
        <input
          type="range"
          min="1"
          max="50"
          value={numTerms()}
          onInput={(e) => setNumTerms(parseInt(e.currentTarget.value))}
        />
        <span class="fourier-terms-val">{numTerms()}</span>
        <label class="formula-toggle">
          <input
            type="checkbox"
            checked={showFormula()}
            onChange={(e) => setShowFormula(e.currentTarget.checked)}
          />
          Formula
        </label>
      </div>
      <canvas ref={canvasRef} width={800} height={220} />
      <Show when={showFormula() && fourierFit()}>
        <div class="fourier-formula">
          <span class="formula-label">f(x) ≈</span>
          <span class="formula-text">{formatFourierFormula(fourierFit()!, 6)}</span>
        </div>
      </Show>
      <div class="viz-label">Blue = original, Orange = Fourier reconstruction ({numTerms()} terms)</div>
    </div>
  );
};

// Wavelet visualization
const WaveletViz: Component<{ vector: number[] }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const w = canvasRef.width, h = canvasRef.height;

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, w, h);

    const wavelet = haarWavelet(props.vector, 8);
    const allCoeffs = [...wavelet.coefficients.flat(), ...wavelet.approximation];
    const absMax = Math.max(...allCoeffs.map(Math.abs)) || 1;

    let y = 0;
    const levelHeight = h / (wavelet.levels + 1);

    // Draw each detail level
    wavelet.coefficients.forEach((level, lvl) => {
      const cellWidth = w / level.length;
      level.forEach((val, i) => {
        const normalized = val / absMax; // -1 to 1
        // Blue for positive, orange/red for negative, brightness by magnitude
        let r, g, b;
        if (normalized >= 0) {
          // Positive: dark blue to bright cyan
          const t = normalized;
          r = Math.round(20 + t * 60);
          g = Math.round(40 + t * 180);
          b = Math.round(80 + t * 175);
        } else {
          // Negative: dark to bright orange/red
          const t = -normalized;
          r = Math.round(80 + t * 175);
          g = Math.round(30 + t * 90);
          b = Math.round(20 + t * 40);
        }
        ctx.fillStyle = `rgb(${r},${g},${b})`;
        ctx.fillRect(i * cellWidth, y, cellWidth + 0.5, levelHeight - 1);
      });

      // Level label with background
      ctx.fillStyle = "rgba(0,0,0,0.6)";
      ctx.fillRect(2, y + 2, 70, 14);
      ctx.fillStyle = "#aab";
      ctx.font = "10px monospace";
      ctx.fillText(`L${lvl + 1} (${level.length})`, 6, y + 13);

      y += levelHeight;
    });

    // Draw approximation level
    const approxCellWidth = w / wavelet.approximation.length;
    wavelet.approximation.forEach((val, i) => {
      const normalized = val / absMax;
      let r, g, b;
      if (normalized >= 0) {
        // Positive: green tones
        const t = Math.abs(normalized);
        r = Math.round(20 + t * 80);
        g = Math.round(60 + t * 195);
        b = Math.round(40 + t * 80);
      } else {
        // Negative: purple tones
        const t = Math.abs(normalized);
        r = Math.round(60 + t * 150);
        g = Math.round(30 + t * 50);
        b = Math.round(80 + t * 175);
      }
      ctx.fillStyle = `rgb(${r},${g},${b})`;
      ctx.fillRect(i * approxCellWidth, y, approxCellWidth + 0.5, levelHeight - 1);
    });

    // Approximation label
    ctx.fillStyle = "rgba(0,0,0,0.6)";
    ctx.fillRect(2, y + 2, 85, 14);
    ctx.fillStyle = "#8f8";
    ctx.font = "10px monospace";
    ctx.fillText(`Approx (${wavelet.approximation.length})`, 6, y + 13);

    // Legend
    ctx.fillStyle = "rgba(0,0,0,0.7)";
    ctx.fillRect(w - 120, 4, 116, 28);
    ctx.fillStyle = "#6af";
    ctx.fillText("+ positive", w - 115, 16);
    ctx.fillStyle = "#fa6";
    ctx.fillText("− negative", w - 115, 28);
  });

  return (
    <div class="viz-container">
      <canvas ref={canvasRef} width={800} height={320} />
      <div class="viz-label">Haar wavelet decomposition — Top = fine detail (L1), Bottom = coarse structure (Approx)</div>
    </div>
  );
};

// Peaks visualization
const PeaksViz: Component<{ vector: number[] }> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;
  const [threshold, setThreshold] = createSignal(0.15);

  createEffect(() => {
    if (!canvasRef || !props.vector) return;
    const ctx = canvasRef.getContext("2d")!;
    const w = canvasRef.width, h = canvasRef.height;

    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, w, h);

    const absMax = Math.max(...props.vector.map(Math.abs));
    const peaks = findPeaks(props.vector, threshold());

    // Draw waveform
    ctx.beginPath();
    ctx.strokeStyle = "rgba(80,160,255,0.5)";
    ctx.lineWidth = 1;
    props.vector.forEach((val, i) => {
      const x = (i / (props.vector.length - 1)) * w;
      const y = h / 2 - (val / absMax) * (h * 0.4);
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    });
    ctx.stroke();

    // Draw peaks
    peaks.forEach((peak, idx) => {
      const x = (peak.index / (props.vector.length - 1)) * w;
      const y = h / 2 - (peak.value / absMax) * (h * 0.4);
      const isPositive = peak.prominence > 0;

      // Marker
      ctx.beginPath();
      ctx.arc(x, y, 5, 0, Math.PI * 2);
      ctx.fillStyle = isPositive ? "rgba(80,255,120,0.9)" : "rgba(255,80,120,0.9)";
      ctx.fill();
      ctx.strokeStyle = isPositive ? "#0f0" : "#f00";
      ctx.lineWidth = 1.5;
      ctx.stroke();

      // Label (top peaks only)
      if (idx < 8) {
        ctx.fillStyle = "#aab";
        ctx.font = "9px monospace";
        ctx.fillText(`${peak.index}`, x - 8, isPositive ? y - 10 : y + 16);
      }
    });

    // Stats
    ctx.fillStyle = "#8899aa";
    ctx.font = "11px monospace";
    const posPeaks = peaks.filter(p => p.prominence > 0).length;
    const negPeaks = peaks.filter(p => p.prominence < 0).length;
    ctx.fillText(`Peaks: ${posPeaks} maxima, ${negPeaks} minima (threshold: ${(threshold() * 100).toFixed(0)}%)`, 10, 16);

    // Peak list
    ctx.fillStyle = "#667";
    ctx.font = "9px monospace";
    const topPeaks = peaks.slice(0, 6);
    const peakStr = topPeaks.map(p => `dim${p.index}:${p.value.toFixed(2)}`).join("  ");
    ctx.fillText(peakStr, 10, h - 8);
  });

  return (
    <div class="viz-container">
      <div class="peak-controls">
        <span>Threshold:</span>
        <input
          type="range"
          min="0.05"
          max="0.5"
          step="0.01"
          value={threshold()}
          onInput={(e) => setThreshold(parseFloat(e.currentTarget.value))}
        />
        <span class="peak-threshold-val">{(threshold() * 100).toFixed(0)}%</span>
      </div>
      <canvas ref={canvasRef} width={800} height={250} />
      <div class="viz-label">Green = maxima, Red = minima — Numbers show dimension indices</div>
    </div>
  );
};

// Main component
interface EmbeddingExplorerProps {
  isOpen: boolean;
  onClose: () => void;
}

type ViewMode = "waveform" | "heatmap" | "hilbert" | "radial" | "distribution" | "fft" | "fourier" | "wavelet" | "peaks";

const EmbeddingExplorer: Component<EmbeddingExplorerProps> = (props) => {
  const [searchQuery, setSearchQuery] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<SearchResult[]>([]);
  const [selectedSuid, setSelectedSuid] = createSignal<string | null>(null);
  const [selectedName, setSelectedName] = createSignal<string>("");
  const [embedding, setEmbedding] = createSignal<number[] | null>(null);
  const [compareEmbedding, setCompareEmbedding] = createSignal<number[] | null>(null);
  const [compareName, setCompareName] = createSignal<string>("");
  const [loading, setLoading] = createSignal(false);
  const [view, setView] = createSignal<ViewMode>("waveform");
  const [showCompare, setShowCompare] = createSignal(false);
  const [morphT, setMorphT] = createSignal(0);
  const [showMorph, setShowMorph] = createSignal(false);
  const [smoothingType, setSmoothingType] = createSignal<SmoothingType>("none");
  const [smoothingStrength, setSmoothingStrength] = createSignal(0.3);

  // Audio state
  const [audioEngine] = createSignal<AudioEngine>(createAudioEngine());
  const [isPlaying, setIsPlaying] = createSignal(false);
  const [playbackMode, setPlaybackMode] = createSignal<PlaybackMode>("waveform");
  const [playbackSpeed, setPlaybackSpeed] = createSignal(1.0);

  // Pattern analysis state
  const [analyzing, setAnalyzing] = createSignal(false);
  const [analysisResult, setAnalysisResult] = createSignal<string | null>(null);
  const [showAnalysis, setShowAnalysis] = createSignal(false);

  // Stop audio on unmount or close
  onCleanup(() => {
    const engine = audioEngine();
    if (engine.isPlaying) {
      engine.stop();
    }
  });

  const handleSearch = async () => {
    if (!searchQuery().trim()) return;
    setLoading(true);
    try {
      const results = await invoke<SearchResult[]>("semantic_search", {
        query: searchQuery(),
        limit: 10,
      });
      setSearchResults(results);
    } catch (e) {
      console.error("Search failed:", e);
    } finally {
      setLoading(false);
    }
  };

  const selectObject = async (suid: string, name: string, isCompare: boolean = false) => {
    setLoading(true);
    try {
      const emb = await invoke<number[] | null>("get_object_embedding", { suid });
      if (emb && emb.length > 0) {
        if (isCompare) {
          setCompareEmbedding(emb);
          setCompareName(name);
        } else {
          setSelectedSuid(suid);
          setSelectedName(name);
          setEmbedding(emb);
        }
      }
    } catch (e) {
      console.error("Failed to fetch embedding:", e);
    } finally {
      setLoading(false);
    }
  };

  const displayVector = createMemo(() => {
    const emb = embedding();
    const comp = compareEmbedding();
    if (!emb) return null;
    let result = emb;
    if (showMorph() && comp) {
      result = lerp(emb, comp, morphT());
    }
    return applySmoothing(result, smoothingType(), smoothingStrength());
  });

  const displayCompareVector = createMemo(() => {
    const comp = compareEmbedding();
    if (!comp || showMorph()) return null;
    return applySmoothing(comp, smoothingType(), smoothingStrength());
  });

  const stats = createMemo(() => {
    const v = displayVector();
    if (!v) return null;
    const min = Math.min(...v);
    const max = Math.max(...v);
    const mean = v.reduce((a, b) => a + b, 0) / v.length;
    const mag = magnitude(v);
    return { min, max, mean, mag, dims: v.length };
  });

  const similarity = createMemo(() => {
    const a = embedding();
    const b = compareEmbedding();
    if (a && b) return cosineSim(a, b);
    return null;
  });

  const VIEWS: ViewMode[] = ["waveform", "heatmap", "hilbert", "radial", "distribution", "fft", "fourier", "wavelet", "peaks"];

  return (
    <Show when={props.isOpen}>
      <div class="embedding-explorer-overlay">
        <div class="embedding-explorer-modal">
          <div class="explorer-header">
            <h2>
              <span class="accent">vec</span>[{embedding()?.length || "?"}]
              <span class="subtitle">embedding explorer</span>
            </h2>
            <button class="close-btn" onClick={props.onClose}>×</button>
          </div>

          <div class="explorer-content">
            {/* Sidebar */}
            <div class="explorer-sidebar">
              {/* Search */}
              <div class="sidebar-section">
                <h4>Select Object</h4>
                <div class="search-row">
                  <input
                    type="text"
                    placeholder="Search..."
                    value={searchQuery()}
                    onInput={(e) => setSearchQuery(e.currentTarget.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                  />
                  <button onClick={handleSearch} disabled={loading()}>
                    {loading() ? "..." : "Go"}
                  </button>
                </div>

                <Show when={searchResults().length > 0}>
                  <div class="search-results-list">
                    <For each={searchResults()}>
                      {(result) => (
                        <div class="result-item">
                          <span class="result-name">{result.object.content.slice(0, 40)}...</span>
                          <div class="result-actions">
                            <button
                              class={`slot-btn ${selectedSuid() === result.object.id ? "selected" : ""}`}
                              onClick={() => selectObject(result.object.id, result.object.content.slice(0, 30))}
                            >
                              A
                            </button>
                            <Show when={showCompare() || showMorph()}>
                              <button
                                class={`slot-btn compare ${compareEmbedding() && compareName() === result.object.content.slice(0, 30) ? "selected" : ""}`}
                                onClick={() => selectObject(result.object.id, result.object.content.slice(0, 30), true)}
                              >
                                B
                              </button>
                            </Show>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>

              {/* Mode */}
              <div class="sidebar-section">
                <h4>Mode</h4>
                <div class="mode-buttons">
                  <button
                    class={showCompare() ? "active compare" : ""}
                    onClick={() => { setShowCompare(!showCompare()); setShowMorph(false); }}
                  >
                    Compare
                  </button>
                  <button
                    class={showMorph() ? "active morph" : ""}
                    onClick={() => { setShowMorph(!showMorph()); setShowCompare(false); }}
                  >
                    Morph
                  </button>
                </div>
              </div>

              {/* Morph Slider */}
              <Show when={showMorph() && embedding() && compareEmbedding()}>
                <div class="sidebar-section morph-section">
                  <div class="morph-slider">
                    <span class="morph-label a">A</span>
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step="0.01"
                      value={morphT()}
                      onInput={(e) => setMorphT(parseFloat(e.currentTarget.value))}
                    />
                    <span class="morph-label b">B</span>
                  </div>
                  <div class="morph-value">t = {morphT().toFixed(2)}</div>
                </div>
              </Show>

              {/* Test Vectors */}
              <div class="sidebar-section">
                <h4>Test Vectors</h4>
                <div class="test-vector-grid">
                  <For each={VECTOR_TYPES}>
                    {(type) => (
                      <button
                        class="test-vector-btn"
                        onClick={() => {
                          const v = generateVector(type);
                          if ((showCompare() || showMorph()) && embedding()) {
                            setCompareEmbedding(v);
                            setCompareName(`${type} (gen)`);
                          } else {
                            setEmbedding(v);
                            setSelectedName(`${type} (gen)`);
                            setSelectedSuid(null);
                          }
                        }}
                      >
                        {type}
                      </button>
                    )}
                  </For>
                </div>
                <p class="test-hint">Generate 1024-dim test vector</p>
              </div>

              {/* Smoothing */}
              <div class="sidebar-section">
                <h4>Smoothing</h4>
                <div class="smoothing-buttons">
                  <button class={smoothingType() === "none" ? "active" : ""} onClick={() => setSmoothingType("none")}>None</button>
                  <button class={smoothingType() === "moving" ? "active" : ""} onClick={() => setSmoothingType("moving")} title="Moving Average">Moving</button>
                  <button class={smoothingType() === "gaussian" ? "active" : ""} onClick={() => setSmoothingType("gaussian")} title="Gaussian blur">Gaussian</button>
                  <button class={smoothingType() === "savgol" ? "active" : ""} onClick={() => setSmoothingType("savgol")} title="Savitzky-Golay (preserves peaks)">S-G</button>
                </div>
                <Show when={smoothingType() !== "none"}>
                  <div class="smoothing-slider">
                    <input
                      type="range"
                      min="0.05"
                      max="1"
                      step="0.05"
                      value={smoothingStrength()}
                      onInput={(e) => setSmoothingStrength(parseFloat(e.currentTarget.value))}
                    />
                    <span class="smoothing-value">{(smoothingStrength() * 100).toFixed(0)}%</span>
                  </div>
                </Show>
              </div>

              {/* Audio Playback */}
              <Show when={displayVector()}>
                <div class="sidebar-section audio-section">
                  <h4>Listen</h4>
                  <div class="audio-mode-buttons">
                    <button
                      class={playbackMode() === "waveform" ? "active" : ""}
                      onClick={() => setPlaybackMode("waveform")}
                      title="Play as audio waveform"
                    >
                      Wave
                    </button>
                    <button
                      class={playbackMode() === "frequency" ? "active" : ""}
                      onClick={() => setPlaybackMode("frequency")}
                      title="Play dominant frequencies"
                    >
                      Freq
                    </button>
                    <button
                      class={playbackMode() === "melody" ? "active" : ""}
                      onClick={() => setPlaybackMode("melody")}
                      title="Play as melody"
                    >
                      Melody
                    </button>
                    <button
                      class={playbackMode() === "drone" ? "active" : ""}
                      onClick={() => setPlaybackMode("drone")}
                      title="Play as ambient drone"
                    >
                      Drone
                    </button>
                  </div>
                  <div class="audio-controls">
                    <button
                      class={`play-btn ${isPlaying() ? "playing" : ""}`}
                      onClick={() => {
                        const engine = audioEngine();
                        const vec = displayVector();
                        if (!vec) return;

                        if (isPlaying()) {
                          engine.stop();
                          setIsPlaying(false);
                        } else {
                          const duration = 2 / playbackSpeed();
                          switch (playbackMode()) {
                            case "waveform":
                              playWaveform(engine, vec, duration);
                              break;
                            case "frequency":
                              playFrequencies(engine, vec, 110 * playbackSpeed());
                              break;
                            case "melody":
                              playMelody(engine, vec, 120 * playbackSpeed());
                              break;
                            case "drone":
                              playDrone(engine, vec, 55 * playbackSpeed());
                              break;
                          }
                          setIsPlaying(true);
                        }
                      }}
                    >
                      {isPlaying() ? "■ Stop" : "▶ Play"}
                    </button>
                    <div class="speed-control">
                      <span class="speed-label">Speed</span>
                      <input
                        type="range"
                        min="0.25"
                        max="2"
                        step="0.25"
                        value={playbackSpeed()}
                        onInput={(e) => setPlaybackSpeed(parseFloat(e.currentTarget.value))}
                      />
                      <span class="speed-value">{playbackSpeed()}x</span>
                    </div>
                  </div>
                  <p class="audio-hint">
                    {playbackMode() === "waveform" && "Embedding as raw audio"}
                    {playbackMode() === "frequency" && "FFT frequencies as tones"}
                    {playbackMode() === "melody" && "Dimensions as musical notes"}
                    {playbackMode() === "drone" && "Harmonic drone synthesis"}
                  </p>
                </div>
              </Show>

              {/* Pattern Analysis */}
              <Show when={displayVector()}>
                <div class="sidebar-section analysis-section">
                  <h4>AI Analysis</h4>
                  <button
                    class={`analyze-btn ${analyzing() ? "analyzing" : ""}`}
                    disabled={analyzing()}
                    onClick={async () => {
                      const vec = displayVector();
                      if (!vec) return;

                      setAnalyzing(true);
                      setAnalysisResult(null);

                      try {
                        const fingerprint = computeFingerprint(vec);
                        const formatted = formatFingerprintForLLM(fingerprint, selectedName() || "Unknown");

                        const systemPrompt = `You are an expert in analyzing embedding vectors and their mathematical properties.
You understand that embeddings encode semantic meaning in high-dimensional space, and different types of content produce characteristic patterns.

Analyze the embedding fingerprint provided and:
1. Describe the overall character of this embedding (smooth/noisy, sparse/dense, structured/chaotic)
2. Hypothesize what kind of content might produce this pattern (factual text, creative writing, code, conversational, technical, etc.)
3. Note any unusual or distinctive features
4. Compare to typical embedding patterns you'd expect

Be concise but insightful. Focus on what makes this embedding interesting or distinctive.`;

                        const prompt = `Analyze this embedding fingerprint:\n\n${formatted}`;

                        const result = await invoke<{ content: string }>("ai_generate", {
                          prompt,
                          systemPrompt
                        });

                        setAnalysisResult(result.content);
                        setShowAnalysis(true);
                      } catch (e) {
                        console.error("Analysis failed:", e);
                        setAnalysisResult(`Analysis failed: ${e}`);
                        setShowAnalysis(true);
                      } finally {
                        setAnalyzing(false);
                      }
                    }}
                  >
                    {analyzing() ? "Analyzing..." : "Analyze Pattern"}
                  </button>
                  <Show when={analysisResult()}>
                    <button
                      class="view-analysis-btn"
                      onClick={() => setShowAnalysis(true)}
                    >
                      View Analysis
                    </button>
                  </Show>
                  <p class="analysis-hint">
                    Send fingerprint to LLM for interpretation
                  </p>
                </div>
              </Show>

              {/* Stats */}
              <Show when={stats()}>
                <div class="sidebar-section stats-section">
                  <h4>Stats</h4>
                  <div class="stat-row"><span>Dims:</span><span>{stats()!.dims}</span></div>
                  <div class="stat-row"><span>Min:</span><span>{stats()!.min.toFixed(4)}</span></div>
                  <div class="stat-row"><span>Max:</span><span>{stats()!.max.toFixed(4)}</span></div>
                  <div class="stat-row"><span>Mean:</span><span>{stats()!.mean.toFixed(4)}</span></div>
                  <div class="stat-row"><span>Magnitude:</span><span>{stats()!.mag.toFixed(3)}</span></div>
                  <Show when={similarity() !== null}>
                    <div class="stat-row similarity">
                      <span>A↔B:</span>
                      <span>{similarity()!.toFixed(4)}</span>
                    </div>
                  </Show>
                </div>
              </Show>

              {/* Selected items */}
              <Show when={selectedName()}>
                <div class="sidebar-section selected-section">
                  <div class="selected-item a">
                    <span class="label">A:</span>
                    <span class="name">{selectedName()}</span>
                  </div>
                  <Show when={compareName()}>
                    <div class="selected-item b">
                      <span class="label">B:</span>
                      <span class="name">{compareName()}</span>
                    </div>
                  </Show>
                </div>
              </Show>
            </div>

            {/* Main content */}
            <div class="explorer-main">
              {/* View tabs */}
              <div class="view-tabs">
                <For each={VIEWS}>
                  {(v) => (
                    <button
                      class={view() === v ? "active" : ""}
                      onClick={() => setView(v)}
                    >
                      {v}
                    </button>
                  )}
                </For>
              </div>

              {/* Visualization */}
              <div class="viz-area">
                <Show when={displayVector()} fallback={
                  <div class="empty-state">
                    <p>Search and select an object to visualize its embedding vector</p>
                    <p class="hint">All {1024} dimensions will be displayed</p>
                  </div>
                }>
                  <Show when={view() === "waveform"}>
                    <WaveformViz
                      vector={displayVector()!}
                      compareVector={showCompare() ? displayCompareVector() : null}
                    />
                  </Show>
                  <Show when={view() === "heatmap"}>
                    <HeatmapViz
                      vector={displayVector()!}
                      compareVector={showCompare() ? displayCompareVector() : null}
                    />
                  </Show>
                  <Show when={view() === "hilbert"}>
                    <HilbertViz vector={displayVector()!} />
                  </Show>
                  <Show when={view() === "radial"}>
                    <RadialViz
                      vector={displayVector()!}
                      compareVector={showCompare() ? displayCompareVector() : null}
                    />
                  </Show>
                  <Show when={view() === "distribution"}>
                    <DistributionViz
                      vector={displayVector()!}
                      compareVector={showCompare() ? displayCompareVector() : null}
                    />
                  </Show>
                  <Show when={view() === "fft"}>
                    <FFTViz vector={displayVector()!} />
                  </Show>
                  <Show when={view() === "fourier"}>
                    <FourierViz vector={displayVector()!} />
                  </Show>
                  <Show when={view() === "wavelet"}>
                    <WaveletViz vector={displayVector()!} />
                  </Show>
                  <Show when={view() === "peaks"}>
                    <PeaksViz vector={displayVector()!} />
                  </Show>
                </Show>
              </div>

              {/* Info panel */}
              <div class="info-panel">
                <Show when={view() === "waveform"}>
                  The vector as a waveform. X = dimension index (0-{embedding()?.length || 1024}), Y = value.
                  Every single dimension is displayed. Patterns reveal the structure of the embedding.
                </Show>
                <Show when={view() === "heatmap"}>
                  Each pixel = one dimension. Color = value (blue=low, purple=high).
                  64×16 grid maps all {embedding()?.length || 1024} dimensions spatially.
                </Show>
                <Show when={view() === "hilbert"}>
                  Hilbert curve maps 1D→2D preserving locality. Adjacent dimensions stay near each other,
                  revealing clusters that a linear layout would hide.
                </Show>
                <Show when={view() === "radial"}>
                  {embedding()?.length || 1024} spokes from center. Distance = absolute value.
                  The shape is the vector's fingerprint. Similar vectors make similar shapes.
                </Show>
                <Show when={view() === "distribution"}>
                  Value distribution across all dimensions. Most embeddings are approximately Gaussian.
                  The distribution shape reveals the vector's statistical character.
                </Show>
                <Show when={view() === "fft"}>
                  FFT frequency spectrum. Shows which frequency components are present in the embedding.
                  High values at low frequencies = smooth patterns. High values at high frequencies = rapid oscillations.
                </Show>
                <Show when={view() === "fourier"}>
                  Fourier series approximation using sine and cosine waves.
                  Shows the formula: f(x) ≈ a₀ + Σ(aₙcos(nωx) + bₙsin(nωx)). Higher R² = better fit.
                </Show>
                <Show when={view() === "wavelet"}>
                  Haar wavelet decomposition. Top rows = fine detail, bottom = coarse structure.
                  Blue = positive coefficients, Red = negative. Bright = large magnitude.
                </Show>
                <Show when={view() === "peaks"}>
                  Peak detection finds local maxima (green) and minima (red).
                  Adjust threshold to filter by prominence. Peak locations may encode semantic features.
                </Show>
              </div>
            </div>
          </div>

          {/* Analysis Result Modal */}
          <Show when={showAnalysis() && analysisResult()}>
            <div class="analysis-overlay" onClick={() => setShowAnalysis(false)}>
              <div class="analysis-modal" onClick={(e) => e.stopPropagation()}>
                <div class="analysis-header">
                  <h3>Embedding Pattern Analysis</h3>
                  <button class="close-analysis-btn" onClick={() => setShowAnalysis(false)}>×</button>
                </div>
                <div class="analysis-content">
                  <div class="analysis-source">
                    <span class="source-label">Source:</span>
                    <span class="source-name">{selectedName() || "Generated vector"}</span>
                  </div>
                  <div class="analysis-text">
                    {analysisResult()}
                  </div>
                </div>
                <div class="analysis-footer">
                  <button class="copy-analysis-btn" onClick={() => {
                    navigator.clipboard.writeText(analysisResult() || "");
                  }}>
                    Copy to Clipboard
                  </button>
                </div>
              </div>
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};

export default EmbeddingExplorer;
