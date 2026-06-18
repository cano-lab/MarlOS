//! Waveform Similarity - Signal processing approach to embedding comparison
//!
//! Treats embeddings as waveforms/signals and uses signal processing techniques
//! to find similarities that cosine similarity misses, especially at scale.
//!
//! Key insight: At scale (50k+ vectors), cosine similarity saturates because
//! high-dimensional vectors tend to have similar angular relationships. By treating
//! embeddings as signals, we can capture:
//! - Phase relationships (cross-correlation)
//! - Frequency domain patterns (spectral similarity)
//! - Multi-scale structural patterns
//!
//! This is particularly effective for:
//! - Detecting semantic relationships that cosine misses
//! - Large-scale vector databases where cosine saturates
//! - Finding "hidden" similarities in embedding space

use std::f32::consts::PI;

/// Result of a waveform similarity computation
#[derive(Debug, Clone)]
pub struct WaveformSimilarityResult {
    /// Overall similarity score (0.0 to 1.0)
    pub score: f32,
    /// Cross-correlation component
    pub cross_correlation: f32,
    /// Spectral similarity component
    pub spectral: f32,
    /// Multi-scale similarity component
    pub multiscale: f32,
    /// Whether saturation was detected
    pub saturation_detected: bool,
}

/// Compute cross-correlation between two embeddings treated as signals
///
/// Cross-correlation measures similarity as a function of displacement.
/// This captures phase relationships that pure cosine similarity misses.
///
/// # Arguments
/// * `a` - First embedding vector
/// * `b` - Second embedding vector
/// * `max_shift` - Maximum phase shift to check (default: 8)
///
/// # Returns
/// Maximum cross-correlation value across all phase shifts
pub fn waveform_cross_correlation(a: &[f32], b: &[f32], max_shift: usize) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let n = a.len();
    let max_shift = max_shift.min(n / 4).min(32); // Cap at 1/4 of vector or 32

    let mut max_corr = 0.0f32;

    // Try different phase shifts
    for shift in 0..=max_shift {
        let corr = cross_correlation_at_shift(a, b, shift);
        max_corr = max_corr.max(corr);
    }

    max_corr
}

/// Cross-correlation at a specific phase shift
fn cross_correlation_at_shift(a: &[f32], b: &[f32], shift: usize) -> f32 {
    let n = a.len();

    // Normalize both vectors to zero mean
    let mean_a: f32 = a.iter().sum::<f32>() / n as f32;
    let mean_b: f32 = b.iter().sum::<f32>() / n as f32;

    let mut numerator = 0.0f32;
    let mut var_a = 0.0f32;
    let mut var_b = 0.0f32;

    for i in 0..n {
        let a_shifted = if i >= shift { a[i - shift] } else { 0.0 };
        let a_centered = a_shifted - mean_a;
        let b_centered = b[i] - mean_b;

        numerator += a_centered * b_centered;
        var_a += a_centered * a_centered;
        var_b += b_centered * b_centered;
    }

    let denom = (var_a * var_b).sqrt();
    if denom < 1e-6 {
        return 0.0;
    }

    (numerator / denom).abs()
}

/// Compute spectral similarity using frequency domain analysis
///
/// Uses a simplified DCT (Discrete Cosine Transform) approach to analyze
/// the frequency spectrum of embeddings. Similar embeddings should have
/// similar energy distribution across frequency bands.
///
/// # Arguments
/// * `a` - First embedding vector
/// * `b` - Second embedding vector
/// * `bands` - Number of frequency bands to analyze (default: 8)
///
/// # Returns
/// Spectral similarity score (0.0 to 1.0)
pub fn spectral_similarity(a: &[f32], b: &[f32], bands: usize) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let bands = bands.min(16).max(4); // Reasonable range

    // Compute energy in each frequency band using DCT approximation
    let spectrum_a = compute_spectrum(a, bands);
    let spectrum_b = compute_spectrum(b, bands);

    // Compare energy distributions
    let mut similarity = 0.0f32;
    for i in 0..bands {
        let diff = (spectrum_a[i] - spectrum_b[i]).abs();
        let sum = spectrum_a[i] + spectrum_b[i] + 1e-6;
        similarity += 1.0 - (diff / sum);
    }

    similarity / bands as f32
}

/// Compute frequency spectrum using simplified DCT
fn compute_spectrum(signal: &[f32], bands: usize) -> Vec<f32> {
    let n = signal.len();
    let mut spectrum = vec![0.0f32; bands];

    // Approximate DCT: compute cosine basis coefficients
    for band in 0..bands {
        let mut energy = 0.0f32;
        let freq = (band + 1) as f32 * PI / (n as f32);

        for (i, &sample) in signal.iter().enumerate() {
            let basis = (freq * i as f32).cos();
            energy += sample * basis;
        }

        spectrum[band] = energy.abs();
    }

    // Normalize
    let total: f32 = spectrum.iter().sum();
    if total > 1e-6 {
        for e in spectrum.iter_mut() {
            *e /= total;
        }
    }

    spectrum
}

/// Compute multi-scale similarity by comparing embeddings at different resolutions
///
/// Analyzes similarity at different scales by downsampling the embeddings.
/// This captures both local (fine-grained) and global (coarse) patterns.
///
/// # Arguments
/// * `a` - First embedding vector
/// * `b` - Second embedding vector
/// * `scales` - Scale factors to use (default: &[1, 2, 4, 8])
///
/// # Returns
/// Multi-scale similarity score (0.0 to 1.0)
pub fn multiscale_similarity(a: &[f32], b: &[f32], scales: &[usize]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let scales = if scales.is_empty() {
        &[1, 2, 4, 8]
    } else {
        scales
    };

    let mut total_similarity = 0.0f32;

    for &scale in scales {
        let downsampled_a = downsample(a, scale);
        let downsampled_b = downsample(b, scale);

        // Use cosine similarity on downsampled vectors
        let sim = cosine_similarity(&downsampled_a, &downsampled_b);
        total_similarity += sim;
    }

    total_similarity / scales.len() as f32
}

/// Downsample a signal by averaging every n samples
fn downsample(signal: &[f32], factor: usize) -> Vec<f32> {
    if factor == 0 || factor >= signal.len() {
        return vec![signal.iter().sum::<f32>() / signal.len() as f32];
    }

    let mut result = Vec::with_capacity((signal.len() + factor - 1) / factor);

    for chunk in signal.chunks(factor) {
        let avg: f32 = chunk.iter().sum::<f32>() / chunk.len() as f32;
        result.push(avg);
    }

    result
}

/// Standard cosine similarity
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    dot / (mag_a * mag_b)
}

/// Detect if cosine similarity is saturated (many vectors with similar scores)
///
/// Saturation occurs when the distribution of cosine similarities becomes
/// too clustered, indicating reduced discriminative power.
///
/// # Arguments
/// * `scores` - Vector of cosine similarity scores
///
/// # Returns
/// True if saturation is detected
pub fn detect_saturation(scores: &[f32]) -> bool {
    if scores.len() < 10 {
        return false;
    }

    // Calculate standard deviation
    let mean: f32 = scores.iter().sum::<f32>() / scores.len() as f32;
    let variance: f32 = scores.iter()
        .map(|s| (s - mean).powi(2))
        .sum::<f32>() / scores.len() as f32;
    let std_dev = variance.sqrt();

    // Saturation detected if std_dev is too low (< 0.1)
    std_dev < 0.1 && mean > 0.7
}

/// Hybrid similarity combining all waveform methods with adaptive weighting
///
/// This is the main entry point for waveform-based similarity search.
/// It adaptively weights different methods based on their effectiveness.
///
/// # Arguments
/// * `a` - First embedding vector
/// * `b` - Second embedding vector
/// * `cosine_score` - Pre-computed cosine similarity (optional)
///
/// # Returns
/// WaveformSimilarityResult with detailed breakdown
pub fn hybrid_similarity(a: &[f32], b: &[f32], cosine_score: Option<f32>) -> WaveformSimilarityResult {
    if a.len() != b.len() || a.is_empty() {
        return WaveformSimilarityResult {
            score: 0.0,
            cross_correlation: 0.0,
            spectral: 0.0,
            multiscale: 0.0,
            saturation_detected: false,
        };
    }

    // Compute individual components
    let cross_corr = waveform_cross_correlation(a, b, 8);
    let spectral = spectral_similarity(a, b, 8);
    let multiscale = multiscale_similarity(a, b, &[1, 2, 4, 8]);

    // Detect saturation using cosine score if provided
    let saturation_detected = if let Some(cos) = cosine_score {
        // Simulate saturation detection
        cos > 0.85 && (cross_corr - cos).abs() < 0.1
    } else {
        false
    };

    // Adaptive weighting based on saturation
    let (w_cross, w_spectral, w_multiscale) = if saturation_detected {
        // When saturated, trust waveform methods more
        (0.4, 0.3, 0.3)
    } else {
        // Normal case: balanced weighting
        (0.3, 0.3, 0.4)
    };

    // Combine scores
    let score = w_cross * cross_corr + w_spectral * spectral + w_multiscale * multiscale;

    WaveformSimilarityResult {
        score: score.min(1.0).max(0.0),
        cross_correlation: cross_corr,
        spectral,
        multiscale,
        saturation_detected,
    }
}

/// Find similar embeddings using waveform similarity
///
/// # Arguments
/// * `query` - Query embedding
/// * `candidates` - Vector of (suid, embedding) tuples
/// * `cosine_scores` - Optional pre-computed cosine scores for each candidate
/// * `limit` - Maximum number of results
/// * `min_score` - Minimum similarity threshold
///
/// # Returns
/// Vector of (suid, WaveformSimilarityResult) sorted by score
pub fn find_similar_waveform(
    query: &[f32],
    candidates: &[(String, Vec<f32>)],
    cosine_scores: Option<&[f32]>,
    limit: usize,
    min_score: f32,
) -> Vec<(String, WaveformSimilarityResult)> {
    let mut results: Vec<(String, WaveformSimilarityResult)> = candidates
        .iter()
        .enumerate()
        .map(|(i, (suid, embedding))| {
            let cosine = cosine_scores.map(|scores| scores.get(i).copied().unwrap_or(0.0));
            let result = hybrid_similarity(query, embedding, cosine);
            (suid.clone(), result)
        })
        .filter(|(_, result)| result.score >= min_score)
        .collect();

    // Sort by score descending
    results.sort_by(|a, b| {
        b.1.score
            .partial_cmp(&a.1.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results.truncate(limit);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.0001);

        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 0.0001);

        let d = vec![2.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &d) - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_waveform_cross_correlation() {
        let a: Vec<f32> = (0..32).map(|i| (i as f32 / 32.0) * 2.0 - 1.0).collect();
        let b = a.clone();
        let c: Vec<f32> = (0..32).map(|i| ((i as f32 / 32.0) * 2.0 - 1.0) * -1.0).collect();

        let corr_ab = waveform_cross_correlation(&a, &b, 8);
        let corr_ac = waveform_cross_correlation(&a, &c, 8);

        // Same signal should have high correlation
        assert!(corr_ab > 0.9);
        // Inverted signal should have lower correlation (but not zero)
        assert!(corr_ac < corr_ab);
    }

    #[test]
    fn test_spectral_similarity() {
        // Create signals with similar spectral characteristics
        let n = 64;
        let a: Vec<f32> = (0..n).map(|i| (i as f32 / n as f32)).collect();
        let b: Vec<f32> = (0..n).map(|i| (i as f32 / n as f32) * 1.1).collect(); // Scaled version
        let c: Vec<f32> = (0..n).map(|i| ((i as f32 * 3.0) % n as f32) / n as f32).collect(); // Different pattern

        let sim_ab = spectral_similarity(&a, &b, 8);
        let sim_ac = spectral_similarity(&a, &c, 8);

        // Similar patterns should have higher spectral similarity
        assert!(sim_ab > sim_ac);
        assert!(sim_ab > 0.5);
    }

    #[test]
    fn test_multiscale_similarity() {
        let a: Vec<f32> = (0..64).map(|i| (i as f32 / 64.0) * 2.0 - 1.0).collect();
        let b = a.clone();
        let c: Vec<f32> = (0..64).map(|i| ((i as f32 / 64.0) * 2.0 - 1.0).sin()).collect();

        let sim_ab = multiscale_similarity(&a, &b, &[1, 2, 4, 8]);
        let sim_ac = multiscale_similarity(&a, &c, &[1, 2, 4, 8]);

        // Same signal should be very similar
        assert!(sim_ab > 0.95);
        // Different signal should be less similar
        assert!(sim_ac < sim_ab);
    }

    #[test]
    fn test_hybrid_similarity() {
        let a: Vec<f32> = (0..64).map(|i| (i as f32 / 64.0) * 2.0 - 1.0).collect();
        let b = a.clone();
        let c: Vec<f32> = (0..64).map(|_| rand::random()).collect();

        let result_ab = hybrid_similarity(&a, &b, Some(1.0));
        let result_ac = hybrid_similarity(&a, &c, Some(0.5));

        // Same signal should score high
        assert!(result_ab.score > 0.8);
        // Random signal should score lower
        assert!(result_ac.score < result_ab.score);

        // Check components are reasonable
        assert!(result_ab.cross_correlation > 0.8);
        assert!(result_ab.spectral > 0.5);
        assert!(result_ab.multiscale > 0.8);
    }

    #[test]
    fn test_detect_saturation() {
        // Non-saturated: varied scores
        let varied: Vec<f32> = vec![0.3, 0.5, 0.7, 0.4, 0.6, 0.8, 0.2, 0.9, 0.5, 0.6];
        assert!(!detect_saturation(&varied));

        // Saturated: clustered high scores
        let saturated: Vec<f32> = vec![0.85, 0.87, 0.86, 0.88, 0.85, 0.87, 0.86, 0.88, 0.85, 0.87];
        assert!(detect_saturation(&saturated));
    }

    #[test]
    fn test_downsample() {
        let signal = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let downsampled = downsample(&signal, 2);

        assert_eq!(downsampled, vec![1.5, 3.5, 5.5, 7.5]);

        let single = downsample(&signal, 100);
        assert_eq!(single.len(), 1);
        assert!((single[0] - 4.5).abs() < 0.1);
    }

    #[test]
    fn test_find_similar_waveform() {
        let query: Vec<f32> = (0..32).map(|i| (i as f32 / 32.0)).collect();

        let candidates = vec![
            ("similar1".to_string(), query.iter().map(|x| x * 1.05).collect()),
            ("similar2".to_string(), query.iter().map(|x| x * 0.95).collect()),
            ("different".to_string(), vec![0.5; 32]),
        ];

        let results = find_similar_waveform(&query, &candidates, None, 10, 0.0);

        assert!(results.len() >= 2);
        // Most similar should be first
        assert!(results[0].1.score > results[1].1.score);
    }
}
