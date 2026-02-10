//! Embedding Transform - Security and Watermarking for Vector Embeddings
//!
//! Applies a secret rotation matrix and watermark to embeddings, making them:
//! - Useless if extracted without the secret key
//! - Identifiable as coming from this tool
//! - Still fully functional for semantic search (rotation preserves cosine similarity)

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Configuration for embedding transformation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformConfig {
    /// Secret key for generating rotation matrix
    pub secret_key: String,
    /// Whether to apply rotation (security)
    pub enable_rotation: bool,
    /// Whether to add watermark (identification)
    pub enable_watermark: bool,
    /// Watermark strength (0.001 - 0.01 recommended, higher = more detectable but affects similarity more)
    pub watermark_strength: f32,
    /// Watermark pattern identifier
    pub watermark_id: String,
}

impl Default for TransformConfig {
    fn default() -> Self {
        Self {
            secret_key: "marlos-default-key".to_string(),
            enable_rotation: true,
            enable_watermark: true,
            // 0.02 is strong enough for reliable detection but small enough
            // to not significantly affect cosine similarity
            watermark_strength: 0.02,
            watermark_id: "MARLOS".to_string(),
        }
    }
}

/// Embedding transformer that applies security and watermarking
pub struct EmbeddingTransformer {
    config: TransformConfig,
    /// Cached rotation matrix for the current dimension
    rotation_cache: Option<(usize, Vec<Vec<f32>>)>,
}

impl EmbeddingTransformer {
    pub fn new(config: TransformConfig) -> Self {
        Self {
            config,
            rotation_cache: None,
        }
    }

    /// Generate a deterministic pseudo-random number from seed
    fn prng(seed: u64, index: usize) -> f32 {
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        index.hash(&mut hasher);
        let hash = hasher.finish();
        // Convert to float in range [-1, 1]
        ((hash % 10000) as f32 / 5000.0) - 1.0
    }

    /// Generate a random orthogonal rotation matrix using Gram-Schmidt
    /// This preserves vector lengths and angles (cosine similarity unchanged)
    fn ensure_rotation_matrix(&mut self, dim: usize) {
        // Check if cache is valid
        if let Some((cached_dim, _)) = &self.rotation_cache {
            if *cached_dim == dim {
                return;
            }
        }

        // Generate seed from secret key
        let mut hasher = DefaultHasher::new();
        self.config.secret_key.hash(&mut hasher);
        let seed = hasher.finish();

        // Generate random vectors
        let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(dim);
        for i in 0..dim {
            let v: Vec<f32> = (0..dim)
                .map(|j| Self::prng(seed, i * dim + j))
                .collect();
            vectors.push(v);
        }

        // Gram-Schmidt orthogonalization to make it a valid rotation matrix
        let mut orthogonal: Vec<Vec<f32>> = Vec::with_capacity(dim);

        for v in vectors {
            let mut u = v.clone();

            // Subtract projections onto previous orthogonal vectors
            for prev in &orthogonal {
                let dot: f32 = u.iter().zip(prev.iter()).map(|(a, b)| a * b).sum();
                let prev_mag_sq: f32 = prev.iter().map(|x| x * x).sum();

                if prev_mag_sq > 1e-10 {
                    let scale = dot / prev_mag_sq;
                    for (idx, p) in prev.iter().enumerate() {
                        u[idx] -= scale * p;
                    }
                }
            }

            // Normalize
            let mag: f32 = u.iter().map(|x| x * x).sum::<f32>().sqrt();
            if mag > 1e-10 {
                for x in &mut u {
                    *x /= mag;
                }
                orthogonal.push(u);
            }
        }

        // Pad if we lost some vectors due to linear dependence
        while orthogonal.len() < dim {
            let mut v = vec![0.0; dim];
            v[orthogonal.len()] = 1.0;
            orthogonal.push(v);
        }

        self.rotation_cache = Some((dim, orthogonal));
    }

    /// Get the rotation matrix (generates if needed)
    fn get_rotation_matrix(&mut self, dim: usize) -> &Vec<Vec<f32>> {
        self.ensure_rotation_matrix(dim);
        &self.rotation_cache.as_ref().unwrap().1
    }

    /// Apply rotation matrix to a vector
    fn rotate(&mut self, v: &[f32]) -> Vec<f32> {
        let dim = v.len();
        self.ensure_rotation_matrix(dim);
        let matrix = &self.rotation_cache.as_ref().unwrap().1;

        // Matrix-vector multiplication: result[i] = sum_j(matrix[i][j] * v[j])
        let mut result = vec![0.0; dim];
        for i in 0..dim {
            for j in 0..dim {
                result[i] += matrix[i][j] * v[j];
            }
        }
        result
    }

    /// Apply inverse rotation (transpose of orthogonal matrix)
    fn rotate_inverse(&mut self, v: &[f32]) -> Vec<f32> {
        let dim = v.len();
        self.ensure_rotation_matrix(dim);
        let matrix = &self.rotation_cache.as_ref().unwrap().1;

        // Transpose multiplication: result[i] = sum_j(matrix[j][i] * v[j])
        let mut result = vec![0.0; dim];
        for i in 0..dim {
            for j in 0..dim {
                result[i] += matrix[j][i] * v[j];
            }
        }
        result
    }

    /// Generate watermark pattern for given dimensions
    fn generate_watermark(&self, dim: usize) -> Vec<f32> {
        let mut hasher = DefaultHasher::new();
        self.config.watermark_id.hash(&mut hasher);
        "watermark-pattern".hash(&mut hasher);
        let seed = hasher.finish();

        // Create a subtle pattern that's spread across all dimensions
        // but concentrates on specific "signature" dimensions
        let mut pattern = vec![0.0; dim];

        // Select ~5% of dimensions for watermark
        let num_watermark_dims = (dim as f32 * 0.05).max(10.0) as usize;

        for i in 0..num_watermark_dims {
            // Deterministic dimension selection
            let mut dim_hasher = DefaultHasher::new();
            seed.hash(&mut dim_hasher);
            i.hash(&mut dim_hasher);
            let dim_index = (dim_hasher.finish() as usize) % dim;

            // Deterministic sign (+1 or -1)
            let sign = if (seed.wrapping_add(i as u64)) % 2 == 0 { 1.0 } else { -1.0 };

            pattern[dim_index] = sign * self.config.watermark_strength;
        }

        pattern
    }

    /// Add watermark to vector
    fn add_watermark(&self, v: &mut [f32]) {
        let watermark = self.generate_watermark(v.len());
        for (i, w) in watermark.iter().enumerate() {
            v[i] += w;
        }
    }

    /// Check if vector contains watermark (returns confidence 0-1)
    /// Uses correlation-based detection that's more robust than sign matching
    pub fn detect_watermark(&self, v: &[f32]) -> f32 {
        let watermark = self.generate_watermark(v.len());

        // Compute correlation between vector values at watermark positions and watermark pattern
        let mut dot_product = 0.0f32;
        let mut watermark_norm_sq = 0.0f32;
        let mut vector_at_watermark_norm_sq = 0.0f32;

        for (i, &w) in watermark.iter().enumerate() {
            if w.abs() > 1e-10 {
                dot_product += v[i] * w;
                watermark_norm_sq += w * w;
                vector_at_watermark_norm_sq += v[i] * v[i];
            }
        }

        if watermark_norm_sq < 1e-10 || vector_at_watermark_norm_sq < 1e-10 {
            return 0.0;
        }

        // Compute cosine similarity between watermark and vector at watermark positions
        let correlation = dot_product / (watermark_norm_sq.sqrt() * vector_at_watermark_norm_sq.sqrt());

        // Map from [-1, 1] to [0, 1] range
        // -1 = anti-correlated, 0 = random, 1 = perfect match
        (correlation + 1.0) / 2.0
    }

    /// Transform an embedding (apply rotation + watermark)
    pub fn transform(&mut self, embedding: &[f32]) -> Vec<f32> {
        let mut result = embedding.to_vec();

        // Apply rotation first (security)
        if self.config.enable_rotation {
            result = self.rotate(&result);
        }

        // Then add watermark (identification)
        if self.config.enable_watermark {
            self.add_watermark(&mut result);
        }

        result
    }

    /// Inverse transform (remove watermark + inverse rotation)
    /// Used if you need to recover original embedding
    pub fn inverse_transform(&mut self, embedding: &[f32]) -> Vec<f32> {
        let mut result = embedding.to_vec();

        // Remove watermark first (reverse order)
        if self.config.enable_watermark {
            let watermark = self.generate_watermark(result.len());
            for (i, w) in watermark.iter().enumerate() {
                result[i] -= w;
            }
        }

        // Then inverse rotation
        if self.config.enable_rotation {
            result = self.rotate_inverse(&result);
        }

        result
    }

    /// Get config
    pub fn config(&self) -> &TransformConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotation_preserves_similarity() {
        let config = TransformConfig {
            enable_rotation: true,
            enable_watermark: false,
            ..Default::default()
        };
        let mut transformer = EmbeddingTransformer::new(config);

        // Two similar vectors
        let v1 = vec![1.0, 0.5, 0.3, 0.1];
        let v2 = vec![0.9, 0.6, 0.25, 0.15];

        // Calculate original cosine similarity
        let dot_orig: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        let mag1_orig: f32 = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag2_orig: f32 = v2.iter().map(|x| x * x).sum::<f32>().sqrt();
        let sim_orig = dot_orig / (mag1_orig * mag2_orig);

        // Transform both vectors
        let t1 = transformer.transform(&v1);
        let t2 = transformer.transform(&v2);

        // Calculate transformed cosine similarity
        let dot_trans: f32 = t1.iter().zip(t2.iter()).map(|(a, b)| a * b).sum();
        let mag1_trans: f32 = t1.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag2_trans: f32 = t2.iter().map(|x| x * x).sum::<f32>().sqrt();
        let sim_trans = dot_trans / (mag1_trans * mag2_trans);

        // Similarities should be very close (rotation preserves angles)
        assert!((sim_orig - sim_trans).abs() < 0.001,
            "Similarity changed: {} -> {}", sim_orig, sim_trans);
    }

    #[test]
    fn test_watermark_detection() {
        let config = TransformConfig {
            enable_rotation: false,
            enable_watermark: true,
            watermark_strength: 0.1, // Stronger watermark for reliable detection
            ..Default::default()
        };
        let mut transformer = EmbeddingTransformer::new(config);

        // Create a random vector with values around 0.1-0.2 magnitude
        let v: Vec<f32> = (0..100).map(|i| (i as f32 * 0.1).sin() * 0.15).collect();

        // Check watermark before (should be around 0.5, random correlation)
        let confidence_before = transformer.detect_watermark(&v);

        // Transform (adds watermark)
        let transformed = transformer.transform(&v);

        // Check watermark after (should be higher due to correlation with watermark pattern)
        let confidence_after = transformer.detect_watermark(&transformed);

        println!("Watermark detection: before={:.3}, after={:.3}", confidence_before, confidence_after);

        assert!(confidence_after > confidence_before,
            "Watermark not detected: before={:.3}, after={:.3}", confidence_before, confidence_after);
        assert!(confidence_after > 0.7,
            "Watermark confidence too low: {:.3}", confidence_after);
    }

    #[test]
    fn test_inverse_transform() {
        let config = TransformConfig::default();
        let mut transformer = EmbeddingTransformer::new(config);

        let original = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let transformed = transformer.transform(&original);
        let recovered = transformer.inverse_transform(&transformed);

        // Check recovery
        for (o, r) in original.iter().zip(recovered.iter()) {
            assert!((o - r).abs() < 0.0001,
                "Recovery failed: {} -> {}", o, r);
        }
    }

    #[test]
    fn test_different_keys_different_results() {
        let config1 = TransformConfig {
            secret_key: "key-one".to_string(),
            ..Default::default()
        };
        let config2 = TransformConfig {
            secret_key: "key-two".to_string(),
            ..Default::default()
        };

        let mut transformer1 = EmbeddingTransformer::new(config1);
        let mut transformer2 = EmbeddingTransformer::new(config2);

        let v = vec![1.0, 2.0, 3.0, 4.0];
        let t1 = transformer1.transform(&v);
        let t2 = transformer2.transform(&v);

        // Results should be different
        let diff: f32 = t1.iter().zip(t2.iter()).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff > 0.1, "Different keys should produce different results");
    }
}
