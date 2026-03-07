/**
 * Waveform Similarity for Vector Embeddings
 * 
 * Traditional cosine similarity fails at scale (50k+ vectors) because:
 * - Vector space becomes dense
 * - All embeddings converge to similar directions
 * - Cosine loses discriminative power
 * 
 * Waveform approach treats embeddings as signals and compares their
 * structural patterns using signal processing techniques.
 */

export interface EmbeddingVector {
  id: string;
  values: number[];
  metadata?: Record<string, any>;
}

export interface SimilarityResult {
  id: string;
  score: number;
  method: 'cosine' | 'waveform' | 'hybrid';
  metadata?: Record<string, any>;
}

/**
 * Traditional cosine similarity (for comparison)
 */
export function cosineSimilarity(a: number[], b: number[]): number {
  let dot = 0;
  let normA = 0;
  let normB = 0;
  
  for (let i = 0; i < a.length; i++) {
    dot += a[i] * b[i];
    normA += a[i] * a[i];
    normB += b[i] * b[i];
  }
  
  return dot / (Math.sqrt(normA) * Math.sqrt(normB));
}

/**
 * Waveform Cross-Correlation Similarity
 * Treats embeddings as signals and finds best alignment
 */
export function waveformCrossCorrelation(a: number[], b: number[]): number {
  const n = a.length;
  let maxCorrelation = -Infinity;
  
  // Try different phase shifts
  for (let shift = -Math.floor(n / 4); shift <= Math.floor(n / 4); shift += 4) {
    let correlation = 0;
    let count = 0;
    
    for (let i = 0; i < n; i++) {
      const j = i + shift;
      if (j >= 0 && j < n) {
        correlation += a[i] * b[j];
        count++;
      }
    }
    
    if (count > 0) {
      correlation /= count;
      maxCorrelation = Math.max(maxCorrelation, correlation);
    }
  }
  
  // Normalize to [-1, 1]
  return Math.max(-1, Math.min(1, maxCorrelation * 4)); // Scale factor
}

/**
 * Spectral Similarity using FFT approach
 * Compares frequency domain representations
 */
export function spectralSimilarity(a: number[], b: number[]): number {
  const aSpectrum = computeSpectrum(a);
  const bSpectrum = computeSpectrum(b);
  
  // Compare spectral energy distribution
  let similarity = 0;
  let totalEnergy = 0;
  
  for (let i = 0; i < aSpectrum.length; i++) {
    const minEnergy = Math.min(aSpectrum[i], bSpectrum[i]);
    const maxEnergy = Math.max(aSpectrum[i], bSpectrum[i]);
    
    if (maxEnergy > 0) {
      similarity += minEnergy / maxEnergy;
      totalEnergy++;
    }
  }
  
  return totalEnergy > 0 ? similarity / totalEnergy : 0;
}

/**
 * Compute simplified spectrum (without full FFT)
 * Uses band-pass energy analysis
 */
function computeSpectrum(vector: number[]): number[] {
  const bands = 8;
  const bandSize = Math.floor(vector.length / bands);
  const spectrum: number[] = [];
  
  for (let b = 0; b < bands; b++) {
    let energy = 0;
    const start = b * bandSize;
    const end = Math.min(start + bandSize, vector.length);
    
    for (let i = start; i < end; i++) {
      energy += vector[i] * vector[i];
    }
    
    spectrum.push(Math.sqrt(energy));
  }
  
  return spectrum;
}

/**
 * Wavelet-inspired Multi-Scale Similarity
 * Compares embeddings at different resolutions
 */
export function multiscaleSimilarity(a: number[], b: number[]): number {
  const scales = [1, 2, 4, 8];
  let totalScore = 0;
  let totalWeight = 0;
  
  for (const scale of scales) {
    // Downsample vectors
    const aDown = downsample(a, scale);
    const bDown = downsample(b, scale);
    
    // Compute similarity at this scale
    const scaleSimilarity = cosineSimilarity(aDown, bDown);
    
    // Weight by scale (finer scales get higher weight)
    const weight = 1 / scale;
    totalScore += scaleSimilarity * weight;
    totalWeight += weight;
  }
  
  return totalWeight > 0 ? totalScore / totalWeight : 0;
}

function downsample(vector: number[], factor: number): number[] {
  const result: number[] = [];
  for (let i = 0; i < vector.length; i += factor) {
    // Average over window
    let sum = 0;
    let count = 0;
    for (let j = i; j < Math.min(i + factor, vector.length); j++) {
      sum += vector[j];
      count++;
    }
    result.push(sum / count);
  }
  return result;
}

/**
 * Hybrid Similarity that combines multiple methods
 * Uses ensemble approach for robustness
 */
export function hybridSimilarity(a: number[], b: number[]): number {
  const cosine = cosineSimilarity(a, b);
  const waveform = waveformCrossCorrelation(a, b);
  const spectral = spectralSimilarity(a, b);
  const multiscale = multiscaleSimilarity(a, b);
  
  // Weighted combination
  // When cosine is saturated (close to 1), rely more on waveform
  const saturation = Math.max(0, cosine - 0.85) / 0.15; // 0 to 1
  
  const weights = {
    cosine: 0.4 * (1 - saturation),
    waveform: 0.3 + 0.2 * saturation,
    spectral: 0.2,
    multiscale: 0.1 + 0.1 * saturation,
  };
  
  return (
    cosine * weights.cosine +
    waveform * weights.waveform +
    spectral * weights.spectral +
    multiscale * weights.multiscale
  );
}

/**
 * Batch similarity computation with filtering
 * Efficient for large-scale comparison
 */
export function findSimilarWaveform(
  query: number[],
  candidates: EmbeddingVector[],
  topK: number = 10,
  threshold: number = 0.5
): SimilarityResult[] {
  const results: SimilarityResult[] = [];
  
  for (const candidate of candidates) {
    // Quick filter with cosine
    const cosine = cosineSimilarity(query, candidate.values);
    
    // Skip obviously dissimilar
    if (cosine < threshold - 0.2) continue;
    
    // Deep comparison with hybrid method
    const hybrid = hybridSimilarity(query, candidate.values);
    
    if (hybrid >= threshold) {
      results.push({
        id: candidate.id,
        score: hybrid,
        method: 'hybrid',
        metadata: candidate.metadata,
      });
    }
  }
  
  // Sort by score descending
  results.sort((a, b) => b.score - a.score);
  
  return results.slice(0, topK);
}

/**
 * Detect if vector space is becoming saturated
 * Helps decide when to switch to waveform methods
 */
export function detectSaturation(vectors: number[][]): {
  isSaturated: boolean;
  averageCosine: number;
  variance: number;
} {
  if (vectors.length < 2) {
    return { isSaturated: false, averageCosine: 0, variance: 0 };
  }
  
  const sampleSize = Math.min(100, vectors.length);
  const samples: number[] = [];
  
  // Sample random pairs
  for (let i = 0; i < sampleSize; i++) {
    const a = vectors[Math.floor(Math.random() * vectors.length)];
    const b = vectors[Math.floor(Math.random() * vectors.length)];
    if (a !== b) {
      samples.push(cosineSimilarity(a, b));
    }
  }
  
  const avg = samples.reduce((a, b) => a + b, 0) / samples.length;
  const variance = samples.reduce((sum, s) => sum + (s - avg) ** 2, 0) / samples.length;
  
  // Saturation: high average similarity with low variance
  return {
    isSaturated: avg > 0.8 && variance < 0.01,
    averageCosine: avg,
    variance,
  };
}
