//! Embedding Analysis - Tools for understanding vector space structure
//!
//! Analyzes Q&A pairs, clustering patterns, and vector arithmetic properties.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::embeddings::cosine_similarity;

/// Statistics about a single Q&A pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QAPairStats {
    pub question_id: String,
    pub answer_id: String,
    pub question_preview: String,
    pub answer_preview: String,
    pub similarity: f32,
    pub distance: f32,
    pub diff_magnitude: f32,
}

/// Cluster analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterAnalysis {
    /// Average similarity between Q&A pairs (should be high)
    pub avg_qa_similarity: f32,
    /// Standard deviation of Q&A similarity
    pub std_qa_similarity: f32,
    /// Average similarity between random pairs (baseline)
    pub avg_random_similarity: f32,
    /// How much better are Q&A pairs than random? (ratio)
    pub qa_vs_random_ratio: f32,
    /// Individual pair statistics
    pub pairs: Vec<QAPairStats>,
}

/// Vector arithmetic test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorArithmeticTest {
    pub description: String,
    pub expected_concept: String,
    pub nearest_actual: String,
    pub similarity_to_expected: f32,
    pub success: bool,
}

/// Full embedding space analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingSpaceAnalysis {
    pub total_objects: usize,
    pub objects_with_embeddings: usize,
    pub embedding_dimensions: usize,
    pub cluster_analysis: Option<ClusterAnalysis>,
    pub diff_vector_analysis: Option<DiffVectorAnalysis>,
    pub recommendations: Vec<String>,
}

/// Analysis of difference vectors between Q&A pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffVectorAnalysis {
    /// Are diff vectors consistent? (high = yes)
    pub diff_consistency: f32,
    /// Average magnitude of diff vectors
    pub avg_diff_magnitude: f32,
    /// Can we predict answers by adding avg_diff to questions?
    pub prediction_accuracy: f32,
    /// Sample predictions
    pub sample_predictions: Vec<PredictionResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResult {
    pub question: String,
    pub actual_answer: String,
    pub predicted_nearest: String,
    pub similarity: f32,
}

/// Compute Euclidean distance between two vectors
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Compute the difference vector (b - a)
pub fn vector_diff(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b.iter()).map(|(x, y)| y - x).collect()
}

/// Compute the sum of two vectors (a + b)
pub fn vector_add(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b.iter()).map(|(x, y)| x + y).collect()
}

/// Compute vector magnitude
pub fn vector_magnitude(v: &[f32]) -> f32 {
    v.iter().map(|x| x.powi(2)).sum::<f32>().sqrt()
}

/// Normalize a vector to unit length
pub fn normalize(v: &[f32]) -> Vec<f32> {
    let mag = vector_magnitude(v);
    if mag > 0.0 {
        v.iter().map(|x| x / mag).collect()
    } else {
        v.to_vec()
    }
}

/// Average multiple vectors
pub fn average_vectors(vectors: &[Vec<f32>]) -> Option<Vec<f32>> {
    if vectors.is_empty() {
        return None;
    }

    let dim = vectors[0].len();
    let n = vectors.len() as f32;

    let mut avg = vec![0.0; dim];
    for v in vectors {
        for (i, val) in v.iter().enumerate() {
            avg[i] += val / n;
        }
    }

    Some(avg)
}

/// Find the nearest vector to a target from a set of candidates
pub fn find_nearest<'a>(
    target: &[f32],
    candidates: &'a [(String, Vec<f32>)],
    exclude_ids: &[&str],
) -> Option<(&'a str, f32)> {
    candidates
        .iter()
        .filter(|(id, _)| !exclude_ids.contains(&id.as_str()))
        .map(|(id, vec)| (id.as_str(), cosine_similarity(target, vec)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

/// Analyze consistency of difference vectors
/// Returns a score 0-1 where 1 means all diff vectors point the same direction
pub fn analyze_diff_consistency(diff_vectors: &[Vec<f32>]) -> f32 {
    if diff_vectors.len() < 2 {
        return 1.0;
    }

    let mut total_sim = 0.0;
    let mut count = 0;

    // Compare each pair of diff vectors
    for i in 0..diff_vectors.len() {
        for j in (i + 1)..diff_vectors.len() {
            total_sim += cosine_similarity(&diff_vectors[i], &diff_vectors[j]);
            count += 1;
        }
    }

    if count > 0 {
        // Convert from [-1, 1] to [0, 1]
        (total_sim / count as f32 + 1.0) / 2.0
    } else {
        1.0
    }
}

/// Statistics helper
pub fn compute_stats(values: &[f32]) -> (f32, f32) {
    if values.is_empty() {
        return (0.0, 0.0);
    }

    let n = values.len() as f32;
    let mean = values.iter().sum::<f32>() / n;
    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n;
    let std_dev = variance.sqrt();

    (mean, std_dev)
}

/// Result of attempting to generate text that matches a target vector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorTargetingResult {
    /// The target concept or description
    pub target_description: String,
    /// Generated candidates with their similarities to target
    pub candidates: Vec<GeneratedCandidate>,
    /// Best match found
    pub best_match: Option<GeneratedCandidate>,
    /// How many iterations were run
    pub iterations: usize,
    /// Did we converge (similarity > threshold)?
    pub converged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedCandidate {
    pub text: String,
    pub similarity_to_target: f32,
    pub iteration: usize,
}

/// Result of vector interpolation experiment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpolationResult {
    pub start_text: String,
    pub end_text: String,
    /// Points along the interpolation path
    pub path: Vec<InterpolationPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpolationPoint {
    /// 0.0 = start, 1.0 = end
    pub t: f32,
    /// The interpolated vector (not included, too large)
    /// Instead, we find the nearest existing content
    pub nearest_content: String,
    pub similarity: f32,
}

/// Interpolate between two vectors: (1-t)*a + t*b
pub fn interpolate_vectors(a: &[f32], b: &[f32], t: f32) -> Vec<f32> {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (1.0 - t) * x + t * y)
        .collect()
}

/// Project a vector onto the direction defined by (end - start)
/// Returns how far along the axis the vector is (0 = at start, 1 = at end)
pub fn project_onto_axis(point: &[f32], start: &[f32], end: &[f32]) -> f32 {
    let axis = vector_diff(start, end);
    let axis_mag_sq: f32 = axis.iter().map(|x| x * x).sum();

    if axis_mag_sq < 1e-10 {
        return 0.5; // Degenerate case
    }

    let to_point = vector_diff(start, point);
    let dot: f32 = axis.iter().zip(to_point.iter()).map(|(a, b)| a * b).sum();

    dot / axis_mag_sq
}

// ============================================================================
// 3D IDEA SPACE - Dimensionality Reduction for VR Visualization
// ============================================================================

/// Projection mode for 3D visualization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProjectionMode {
    /// PCA - Principal Component Analysis (default, finds most important directions)
    PCA,
    /// Folded - Chunk vector into 3 parts, sum each for X/Y/Z
    Folded,
    /// Random - Random orthogonal projection (baseline comparison)
    Random,
}

impl Default for ProjectionMode {
    fn default() -> Self {
        ProjectionMode::PCA
    }
}

/// A point in 3D idea space with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeaSpacePoint {
    pub id: String,
    pub name: String,
    pub object_type: String,
    /// 3D coordinates (x, y, z) normalized to [-1, 1] range
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Original high-dimensional distance from center
    pub distance_from_center: f32,
    /// Vector magnitude (for coloring by "distinctiveness")
    pub magnitude: f32,
    /// Tags for filtering/coloring
    pub tags: Vec<String>,
    /// Preview of content
    pub preview: String,
}

/// Semantic axis defined by two concept endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticAxis {
    pub name: String,
    pub negative_label: String,  // e.g., "simple"
    pub positive_label: String,  // e.g., "complex"
    /// The direction vector (normalized)
    pub direction: Vec<f32>,
}

/// Result of 3D projection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeaSpace3D {
    pub points: Vec<IdeaSpacePoint>,
    /// The three orthogonal axes used for projection
    pub axes: Vec<SemanticAxis>,
    /// How much variance is captured by these 3 dimensions (0-1)
    pub variance_captured: f32,
    /// Bounds of the space
    pub bounds: SpaceBounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
    pub min_z: f32,
    pub max_z: f32,
}

/// PCA result containing principal components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PCAResult {
    /// Principal component vectors (each is a direction in original space)
    pub components: Vec<Vec<f32>>,
    /// Variance explained by each component
    pub explained_variance: Vec<f32>,
    /// Mean vector (center of the data)
    pub mean: Vec<f32>,
}

/// Compute mean vector of a set of vectors
/// Only uses vectors that match the dimension of the first vector
pub fn compute_mean(vectors: &[&[f32]]) -> Vec<f32> {
    if vectors.is_empty() {
        return vec![];
    }

    let dim = vectors[0].len();
    if dim == 0 {
        return vec![];
    }

    // Filter to only vectors matching the expected dimension
    let valid_vectors: Vec<_> = vectors.iter()
        .filter(|v| v.len() == dim)
        .collect();

    if valid_vectors.is_empty() {
        return vec![0.0; dim];
    }

    let n = valid_vectors.len() as f32;
    let mut mean = vec![0.0; dim];

    for v in valid_vectors {
        for (i, val) in v.iter().enumerate() {
            mean[i] += val / n;
        }
    }
    mean
}

/// Center vectors by subtracting mean
pub fn center_vectors(vectors: &[&[f32]], mean: &[f32]) -> Vec<Vec<f32>> {
    vectors.iter()
        .map(|v| v.iter().zip(mean.iter()).map(|(x, m)| x - m).collect())
        .collect()
}

/// Simple power iteration to find top eigenvector
fn power_iteration(matrix: &[Vec<f32>], num_iterations: usize) -> Vec<f32> {
    let dim = matrix.len();
    if dim == 0 {
        return vec![];
    }

    // Start with random-ish vector
    let mut v: Vec<f32> = (0..dim).map(|i| ((i * 7 + 3) % 11) as f32 / 11.0).collect();
    v = normalize(&v);

    for _ in 0..num_iterations {
        // Multiply matrix by vector
        let mut new_v = vec![0.0; dim];
        for i in 0..dim {
            for j in 0..dim {
                new_v[i] += matrix[i][j] * v[j];
            }
        }
        v = normalize(&new_v);
    }

    v
}

/// Compute eigenvalue for eigenvector
fn compute_eigenvalue(matrix: &[Vec<f32>], eigenvector: &[f32]) -> f32 {
    let dim = matrix.len();
    let mut result = vec![0.0; dim];

    for i in 0..dim {
        for j in 0..dim {
            result[i] += matrix[i][j] * eigenvector[j];
        }
    }

    // Rayleigh quotient: v^T * A * v
    eigenvector.iter().zip(result.iter()).map(|(a, b)| a * b).sum()
}

/// Deflate matrix by removing component in direction of eigenvector
fn deflate_matrix(matrix: &mut [Vec<f32>], eigenvector: &[f32], eigenvalue: f32) {
    let dim = matrix.len();
    for i in 0..dim {
        for j in 0..dim {
            matrix[i][j] -= eigenvalue * eigenvector[i] * eigenvector[j];
        }
    }
}

/// Perform simple PCA to find top k principal components
pub fn simple_pca(vectors: &[&[f32]], k: usize) -> Option<PCAResult> {
    simple_pca_with_progress(vectors, k, |_, _| {})
}

/// PCA with progress callback - reports progress during covariance matrix computation
/// callback receives (current_vector_index, total_vectors)
pub fn simple_pca_with_progress<F>(vectors: &[&[f32]], k: usize, mut progress: F) -> Option<PCAResult>
where
    F: FnMut(usize, usize),
{
    if vectors.is_empty() || k == 0 {
        return None;
    }

    let dim = vectors[0].len();
    if dim == 0 {
        return None;
    }

    // Filter to only vectors matching the expected dimension
    let valid_vectors: Vec<&[f32]> = vectors.iter()
        .filter(|v| v.len() == dim)
        .copied()
        .collect();

    if valid_vectors.len() < 3 {
        return None; // Need at least 3 vectors for meaningful PCA
    }

    let total = valid_vectors.len();

    // Compute mean and center data
    let mean = compute_mean(&valid_vectors);
    let centered = center_vectors(&valid_vectors, &mean);

    // Compute covariance matrix (dim x dim)
    // For large dims, we compute X^T * X / n instead
    let n = centered.len() as f32;
    let mut cov = vec![vec![0.0; dim]; dim];

    for (idx, v) in centered.iter().enumerate() {
        // Report progress every 100 vectors
        if idx % 100 == 0 {
            progress(idx, total);
        }

        // Skip vectors that don't match dimension (shouldn't happen after filtering, but be safe)
        if v.len() != dim {
            continue;
        }
        for i in 0..dim {
            for j in 0..dim {
                cov[i][j] += v[i] * v[j] / n;
            }
        }
    }

    // Report 100% for covariance
    progress(total, total);

    // Find top k eigenvectors using power iteration + deflation
    let mut components = Vec::with_capacity(k);
    let mut explained_variance = Vec::with_capacity(k);

    for _ in 0..k {
        let eigenvector = power_iteration(&cov, 100);
        let eigenvalue = compute_eigenvalue(&cov, &eigenvector);

        if eigenvalue.abs() < 1e-10 {
            break;
        }

        components.push(eigenvector.clone());
        explained_variance.push(eigenvalue);

        // Remove this component from the matrix
        deflate_matrix(&mut cov, &eigenvector, eigenvalue);
    }

    Some(PCAResult {
        components,
        explained_variance,
        mean,
    })
}

/// Project a vector onto principal components to get 3D coordinates
pub fn project_to_3d(vector: &[f32], pca: &PCAResult) -> (f32, f32, f32) {
    // Center the vector
    let centered: Vec<f32> = vector.iter()
        .zip(pca.mean.iter())
        .map(|(x, m)| x - m)
        .collect();

    // Project onto each component
    let coords: Vec<f32> = pca.components.iter()
        .take(3)
        .map(|component| {
            centered.iter().zip(component.iter()).map(|(a, b)| a * b).sum()
        })
        .collect();

    (
        coords.get(0).copied().unwrap_or(0.0),
        coords.get(1).copied().unwrap_or(0.0),
        coords.get(2).copied().unwrap_or(0.0),
    )
}

/// Folded projection - chunk vector into 3 parts and aggregate each
/// This includes ALL dimensions in the projection (no information loss from selection)
pub fn project_folded(vector: &[f32]) -> (f32, f32, f32) {
    if vector.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    let dim = vector.len();
    let chunk_size = dim / 3;
    let remainder = dim % 3;

    // Split into 3 chunks (handle uneven division)
    let chunk1_end = chunk_size + if remainder > 0 { 1 } else { 0 };
    let chunk2_end = chunk1_end + chunk_size + if remainder > 1 { 1 } else { 0 };

    // Sum each chunk
    let x: f32 = vector[..chunk1_end].iter().sum();
    let y: f32 = vector[chunk1_end..chunk2_end].iter().sum();
    let z: f32 = vector[chunk2_end..].iter().sum();

    (x, y, z)
}

/// Folded projection with different aggregation methods
pub fn project_folded_method(vector: &[f32], method: &str) -> (f32, f32, f32) {
    if vector.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    let dim = vector.len();
    let chunk_size = dim / 3;
    let remainder = dim % 3;

    let chunk1_end = chunk_size + if remainder > 0 { 1 } else { 0 };
    let chunk2_end = chunk1_end + chunk_size + if remainder > 1 { 1 } else { 0 };

    let chunks = [
        &vector[..chunk1_end],
        &vector[chunk1_end..chunk2_end],
        &vector[chunk2_end..],
    ];

    let aggregate = |chunk: &[f32]| -> f32 {
        match method {
            "sum" => chunk.iter().sum(),
            "mean" => chunk.iter().sum::<f32>() / chunk.len() as f32,
            "max" => chunk.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
            "min" => chunk.iter().cloned().fold(f32::INFINITY, f32::min),
            "variance" => {
                let mean = chunk.iter().sum::<f32>() / chunk.len() as f32;
                chunk.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / chunk.len() as f32
            }
            "l2" => chunk.iter().map(|x| x * x).sum::<f32>().sqrt(),
            _ => chunk.iter().sum(), // default to sum
        }
    };

    (
        aggregate(chunks[0]),
        aggregate(chunks[1]),
        aggregate(chunks[2]),
    )
}

/// Gram-Schmidt orthogonalization - make vectors orthogonal to each other
pub fn gram_schmidt(vectors: &[Vec<f32>]) -> Vec<Vec<f32>> {
    let mut orthogonal: Vec<Vec<f32>> = Vec::with_capacity(vectors.len());

    for v in vectors {
        let mut u = v.clone();

        // Subtract projections onto all previous orthogonal vectors
        for prev in &orthogonal {
            let dot: f32 = u.iter().zip(prev.iter()).map(|(a, b)| a * b).sum();
            let prev_mag_sq: f32 = prev.iter().map(|x| x * x).sum();

            if prev_mag_sq > 1e-10 {
                let scale = dot / prev_mag_sq;
                for (i, p) in prev.iter().enumerate() {
                    u[i] -= scale * p;
                }
            }
        }

        // Normalize
        let u = normalize(&u);
        if vector_magnitude(&u) > 0.5 {  // Only keep if not degenerate
            orthogonal.push(u);
        }
    }

    orthogonal
}

/// Create semantic axes from concept pairs
/// Each pair defines an axis: (negative_concept, positive_concept)
pub fn create_semantic_axes(
    concept_pairs: &[(String, String, String)],  // (name, negative, positive)
    get_embedding: impl Fn(&str) -> Option<Vec<f32>>,
) -> Vec<SemanticAxis> {
    let mut axes = Vec::new();

    for (name, neg, pos) in concept_pairs {
        if let (Some(neg_vec), Some(pos_vec)) = (get_embedding(neg), get_embedding(pos)) {
            let direction = normalize(&vector_diff(&neg_vec, &pos_vec));
            axes.push(SemanticAxis {
                name: name.clone(),
                negative_label: neg.clone(),
                positive_label: pos.clone(),
                direction,
            });
        }
    }

    // Orthogonalize the axes
    let directions: Vec<Vec<f32>> = axes.iter().map(|a| a.direction.clone()).collect();
    let orthogonal = gram_schmidt(&directions);

    // Update axes with orthogonalized directions
    for (i, axis) in axes.iter_mut().enumerate() {
        if i < orthogonal.len() {
            axis.direction = orthogonal[i].clone();
        }
    }

    axes
}

/// Project a vector onto custom semantic axes
pub fn project_to_semantic_axes(vector: &[f32], axes: &[SemanticAxis]) -> Vec<f32> {
    axes.iter()
        .map(|axis| {
            vector.iter().zip(axis.direction.iter()).map(|(a, b)| a * b).sum()
        })
        .collect()
}

/// Normalize coordinates to [-1, 1] range
pub fn normalize_coordinates(points: &mut [IdeaSpacePoint]) -> SpaceBounds {
    if points.is_empty() {
        return SpaceBounds {
            min_x: -1.0, max_x: 1.0,
            min_y: -1.0, max_y: 1.0,
            min_z: -1.0, max_z: 1.0,
        };
    }

    // Find bounds
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    let mut min_z = f32::MAX;
    let mut max_z = f32::MIN;

    for p in points.iter() {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
        min_z = min_z.min(p.z);
        max_z = max_z.max(p.z);
    }

    let bounds = SpaceBounds { min_x, max_x, min_y, max_y, min_z, max_z };

    // Normalize to [-1, 1]
    let range_x = (max_x - min_x).max(0.001);
    let range_y = (max_y - min_y).max(0.001);
    let range_z = (max_z - min_z).max(0.001);

    for p in points.iter_mut() {
        p.x = 2.0 * (p.x - min_x) / range_x - 1.0;
        p.y = 2.0 * (p.y - min_y) / range_y - 1.0;
        p.z = 2.0 * (p.z - min_z) / range_z - 1.0;
    }

    bounds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_operations() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];

        let diff = vector_diff(&a, &b);
        assert_eq!(diff, vec![3.0, 3.0, 3.0]);

        let sum = vector_add(&a, &b);
        assert_eq!(sum, vec![5.0, 7.0, 9.0]);

        let mag = vector_magnitude(&vec![3.0, 4.0]);
        assert!((mag - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_average_vectors() {
        let vectors = vec![
            vec![1.0, 2.0],
            vec![3.0, 4.0],
            vec![5.0, 6.0],
        ];

        let avg = average_vectors(&vectors).unwrap();
        assert!((avg[0] - 3.0).abs() < 0.001);
        assert!((avg[1] - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_diff_consistency() {
        // Identical diff vectors = perfect consistency
        let diffs = vec![
            vec![1.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0],
        ];
        let consistency = analyze_diff_consistency(&diffs);
        assert!((consistency - 1.0).abs() < 0.001);

        // Opposite diff vectors = low consistency
        let diffs = vec![
            vec![1.0, 0.0],
            vec![-1.0, 0.0],
        ];
        let consistency = analyze_diff_consistency(&diffs);
        assert!(consistency < 0.5);
    }

    #[test]
    fn test_gram_schmidt() {
        // Two non-orthogonal vectors
        let vectors = vec![
            vec![1.0, 1.0, 0.0],
            vec![1.0, 0.0, 0.0],
        ];

        let orthogonal = gram_schmidt(&vectors);
        assert_eq!(orthogonal.len(), 2);

        // Check orthogonality (dot product should be ~0)
        let dot: f32 = orthogonal[0].iter()
            .zip(orthogonal[1].iter())
            .map(|(a, b)| a * b)
            .sum();
        assert!(dot.abs() < 0.001);
    }

    #[test]
    fn test_simple_pca() {
        // Create 2D data with variance in both dimensions
        // Points form a rough ellipse with major axis along (1,1)
        let v1 = vec![1.0, 0.5];
        let v2 = vec![2.0, 1.5];
        let v3 = vec![3.0, 2.0];
        let v4 = vec![2.0, 2.5];
        let v5 = vec![1.0, 1.5];
        let v6 = vec![0.5, 1.0];

        let vectors: Vec<&[f32]> = vec![&v1, &v2, &v3, &v4, &v5, &v6];
        let pca = simple_pca(&vectors, 2).unwrap();

        // Should find at least one component
        assert!(!pca.components.is_empty());
        // First component should have positive variance
        assert!(pca.explained_variance[0] > 0.0);
    }

    #[test]
    fn test_project_to_3d() {
        let v1 = vec![1.0, 0.0, 0.0, 0.0];
        let v2 = vec![0.0, 1.0, 0.0, 0.0];
        let v3 = vec![0.0, 0.0, 1.0, 0.0];
        let v4 = vec![1.0, 1.0, 1.0, 0.0];

        let vectors: Vec<&[f32]> = vec![&v1, &v2, &v3, &v4];
        let pca = simple_pca(&vectors, 3).unwrap();

        let (x, y, z) = project_to_3d(&v1, &pca);
        // Should produce some non-zero coordinates
        assert!((x.abs() + y.abs() + z.abs()) > 0.0);
    }

    #[test]
    fn test_normalize_coordinates() {
        let mut points = vec![
            IdeaSpacePoint {
                id: "1".to_string(),
                name: "Test".to_string(),
                object_type: "note".to_string(),
                x: 0.0,
                y: 0.0,
                z: 0.0,
                distance_from_center: 0.0,
                tags: vec![],
                preview: "".to_string(),
            },
            IdeaSpacePoint {
                id: "2".to_string(),
                name: "Test2".to_string(),
                object_type: "note".to_string(),
                x: 10.0,
                y: 20.0,
                z: 30.0,
                distance_from_center: 0.0,
                tags: vec![],
                preview: "".to_string(),
            },
        ];

        let _bounds = normalize_coordinates(&mut points);

        // First point should be at -1
        assert!((points[0].x - (-1.0)).abs() < 0.001);
        // Second point should be at 1
        assert!((points[1].x - 1.0).abs() < 0.001);
    }
}
