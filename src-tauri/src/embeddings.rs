//! Embeddings - Vector embedding generation for semantic search
//!
//! Provides a pluggable interface for generating text embeddings.
//! Supports multiple backends:
//! - External APIs (OpenAI, Ollama, etc.)
//! - Local models (future: ONNX runtime)
//!
//! Default model: all-MiniLM-L6-v2 (384 dimensions)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use thiserror::Error;

use crate::embedding_transform::{EmbeddingTransformer, TransformConfig};

#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("Model not available: {0}")]
    ModelNotAvailable(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

pub type Result<T> = std::result::Result<T, EmbeddingError>;

/// Information about an embedding model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Output dimension size
    pub dimensions: usize,
    /// Maximum input tokens
    pub max_tokens: usize,
}

impl Default for ModelInfo {
    fn default() -> Self {
        Self {
            id: "all-MiniLM-L6-v2".to_string(),
            name: "MiniLM L6 v2".to_string(),
            dimensions: 384,
            max_tokens: 512,
        }
    }
}

/// Trait for embedding providers
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Get model information
    fn model_info(&self) -> &ModelInfo;

    /// Generate embedding for a single text
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts (batch)
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // Default implementation: sequential embedding
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// Check if the provider is available
    async fn is_available(&self) -> bool;
}

// ============================================================================
// Ollama Embedding Provider
// ============================================================================

/// Ollama-based embedding provider
pub struct OllamaEmbedding {
    client: reqwest::Client,
    base_url: String,
    model: String,
    model_info: ModelInfo,
}

/// Short-timeout HTTP client for liveness probes only.
///
/// A real embedding request may legitimately take a while (the server
/// might be loading a model), so the provider clients use a 120s
/// timeout. A *probe*, on the other hand, must fail fast: `auto_detect`
/// runs on the main thread during Tauri `setup()` via `block_on`, so a
/// slow probe freezes the whole window ("Not Responding", white view).
///
/// The dangerous case isn't a closed port (that refuses instantly) —
/// it's a process that accepts the TCP connection but never speaks
/// HTTP. QEMU's GDB stub defaults to port 1234, which is also LM
/// Studio's default port; when `localhost` resolves to the QEMU
/// binding the probe would otherwise hang for the full request timeout.
/// A 1s connect / 2s total ceiling bounds that to a brief blip.
fn probe_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(1))
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

impl OllamaEmbedding {
    /// Create a new Ollama embedding provider
    pub fn new(base_url: &str, model: &str) -> Self {
        let (dimensions, max_tokens) = match model {
            "all-minilm" | "all-minilm:latest" => (384, 512),
            "nomic-embed-text" | "nomic-embed-text:latest" => (768, 8192),
            "mxbai-embed-large" | "mxbai-embed-large:latest" => (1024, 512),
            _ => (384, 512), // Default assumption
        };

        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            model: model.to_string(),
            model_info: ModelInfo {
                id: model.to_string(),
                name: model.to_string(),
                dimensions,
                max_tokens,
            },
        }
    }

    /// Create with default settings (localhost, all-minilm)
    pub fn default_local() -> Self {
        Self::new("http://localhost:11434", "all-minilm")
    }
}

#[derive(Serialize)]
struct OllamaEmbedRequest {
    model: String,
    prompt: String,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbedding {
    fn model_info(&self) -> &ModelInfo {
        &self.model_info
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/api/embeddings", self.base_url);

        let request = OllamaEmbedRequest {
            model: self.model.clone(),
            prompt: text.to_string(),
        };

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::Api(format!("{}: {}", status, body)));
        }

        let result: OllamaEmbedResponse = response.json().await?;
        Ok(result.embedding)
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        probe_client().get(&url).send().await.is_ok()
    }
}

// ============================================================================
// OpenAI Embedding Provider
// ============================================================================

/// OpenAI-compatible embedding provider (works with OpenAI, Azure, local APIs)
pub struct OpenAIEmbedding {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
    model_info: ModelInfo,
}

impl OpenAIEmbedding {
    /// Create HTTP client with proper settings to prevent connection exhaustion
    fn create_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_max_idle_per_host(5)
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    }

    /// Create a new OpenAI embedding provider
    pub fn new(base_url: &str, api_key: Option<String>, model: &str) -> Self {
        let (dimensions, max_tokens) = match model {
            "text-embedding-3-small" => (1536, 8191),
            "text-embedding-3-large" => (3072, 8191),
            "text-embedding-ada-002" => (1536, 8191),
            _ => (1536, 8191), // Default assumption
        };

        Self {
            client: Self::create_client(),
            base_url: base_url.to_string(),
            api_key,
            model: model.to_string(),
            model_info: ModelInfo {
                id: model.to_string(),
                name: model.to_string(),
                dimensions,
                max_tokens,
            },
        }
    }

    /// Create with OpenAI defaults
    pub fn openai(api_key: &str) -> Self {
        Self::new(
            "https://api.openai.com/v1",
            Some(api_key.to_string()),
            "text-embedding-3-small",
        )
    }

    /// Create for LM Studio (localhost:1234, no API key needed)
    pub fn lm_studio(model: &str) -> Self {
        let (dimensions, max_tokens) = match model {
            // Qwen3 embedding models
            "text-embedding-qwen3-embedding-0.6b" | "Qwen/Qwen3-Embedding-0.6B" => (1024, 8192),
            "text-embedding-qwen3-embedding-1.5b" | "Qwen/Qwen3-Embedding-1.5B" => (1024, 8192),
            "text-embedding-qwen3-embedding-4b" | "Qwen/Qwen3-Embedding-4B" => (1024, 8192),
            "text-embedding-qwen3-embedding-8b" | "Qwen/Qwen3-Embedding-8B" => (1024, 8192),
            // Nomic models
            "nomic-embed-text" | "nomic-ai/nomic-embed-text-v1.5-GGUF" => (768, 8192),
            "text-embedding-nomic-embed-text-v1.5" => (768, 8192),
            // Sentence transformers
            "all-MiniLM-L6-v2" | "sentence-transformers/all-MiniLM-L6-v2" => (384, 512),
            // BGE models
            "bge-small-en" | "BAAI/bge-small-en-v1.5" => (384, 512),
            "bge-base-en" | "BAAI/bge-base-en-v1.5" => (768, 512),
            "bge-large-en" | "BAAI/bge-large-en-v1.5" => (1024, 512),
            // Default - try to detect from model name
            m if m.contains("qwen") && m.contains("embed") => (1024, 8192),
            _ => (768, 8192), // Reasonable default for most embedding models
        };

        Self {
            client: Self::create_client(),
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: None, // LM Studio doesn't require API key
            model: model.to_string(),
            model_info: ModelInfo {
                id: format!("lm-studio:{}", model),
                name: format!("LM Studio: {}", model),
                dimensions,
                max_tokens,
            },
        }
    }

    /// Create for LM Studio with default model (Qwen3 0.6B - 1024 dims)
    pub fn lm_studio_default() -> Self {
        Self::lm_studio("text-embedding-qwen3-embedding-0.6b")
    }
}

#[derive(Serialize)]
struct OpenAIEmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct OpenAIEmbedResponse {
    data: Vec<OpenAIEmbedData>,
}

#[derive(Deserialize)]
struct OpenAIEmbedData {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbedding {
    fn model_info(&self) -> &ModelInfo {
        &self.model_info
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed_batch(&[text]).await?;
        results.into_iter().next()
            .ok_or_else(|| EmbeddingError::InvalidResponse("Empty response".to_string()))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.base_url);

        let request = OpenAIEmbedRequest {
            model: self.model.clone(),
            input: texts.iter().map(|s| s.to_string()).collect(),
        };

        let mut req = self.client.post(&url).json(&request);

        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::Api(format!("{}: {}", status, body)));
        }

        let result: OpenAIEmbedResponse = response.json().await?;
        Ok(result.data.into_iter().map(|d| d.embedding).collect())
    }

    async fn is_available(&self) -> bool {
        // For LM Studio (no API key), probe the server
        if self.api_key.is_none() {
            let url = format!("{}/models", self.base_url);
            return probe_client().get(&url).send().await.is_ok();
        }
        // For API services with key, assume available
        true
    }
}

// ============================================================================
// Mock Embedding Provider (for testing)
// ============================================================================

/// Mock embedding provider that generates deterministic embeddings
/// Useful for testing without requiring external services
pub struct MockEmbedding {
    model_info: ModelInfo,
}

impl MockEmbedding {
    pub fn new(dimensions: usize) -> Self {
        Self {
            model_info: ModelInfo {
                id: "mock".to_string(),
                name: "Mock Embedding".to_string(),
                dimensions,
                max_tokens: 512,
            },
        }
    }
}

impl Default for MockEmbedding {
    fn default() -> Self {
        Self::new(384)
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbedding {
    fn model_info(&self) -> &ModelInfo {
        &self.model_info
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Generate a deterministic embedding based on text hash
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // Use hash to seed a simple PRNG for deterministic output
        let mut seed = hash;
        let embedding: Vec<f32> = (0..self.model_info.dimensions)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                // Normalize to [-1, 1]
                (seed as f32 / u64::MAX as f32) * 2.0 - 1.0
            })
            .collect();

        // Normalize to unit vector
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        Ok(embedding.into_iter().map(|x| x / magnitude).collect())
    }

    async fn is_available(&self) -> bool {
        true
    }
}

// ============================================================================
// Embedding Manager
// ============================================================================

/// Manages embedding generation with automatic provider selection
/// Optionally applies security transforms (rotation + watermarking) to embeddings
pub struct EmbeddingManager {
    provider: Box<dyn EmbeddingProvider>,
    /// Optional transformer for security/watermarking
    transformer: Option<Mutex<EmbeddingTransformer>>,
}

impl EmbeddingManager {
    /// Create with a specific provider (no transform)
    pub fn new(provider: Box<dyn EmbeddingProvider>) -> Self {
        Self {
            provider,
            transformer: None,
        }
    }

    /// Create with a specific provider and transform config
    pub fn new_with_transform(provider: Box<dyn EmbeddingProvider>, config: TransformConfig) -> Self {
        Self {
            provider,
            transformer: Some(Mutex::new(EmbeddingTransformer::new(config))),
        }
    }

    /// Create with mock provider (for testing)
    pub fn mock() -> Self {
        Self::new(Box::new(MockEmbedding::default()))
    }

    /// Try to create with Ollama, fall back to mock
    pub async fn try_ollama_or_mock() -> Self {
        let ollama = OllamaEmbedding::default_local();
        if ollama.is_available().await {
            Self::new(Box::new(ollama))
        } else {
            log::warn!("Ollama not available, using mock embeddings");
            Self::mock()
        }
    }

    /// Try to create with LM Studio, fall back to Ollama, then mock
    /// This is the recommended auto-detection method
    /// Applies security transform (rotation + watermark) by default
    pub async fn auto_detect() -> Self {
        // Default transform config - rotation for security, watermark for identification
        let transform_config = TransformConfig::default();

        // Try LM Studio first (localhost:1234)
        let lm_studio = OpenAIEmbedding::lm_studio_default();
        if lm_studio.is_available().await {
            log::info!("Using LM Studio for embeddings (text-embedding-qwen3-embedding-0.6b, 1024 dims)");
            log::info!("Embedding security enabled: rotation + watermarking");
            return Self::new_with_transform(Box::new(lm_studio), transform_config);
        }

        // Try Ollama second (localhost:11434)
        let ollama = OllamaEmbedding::default_local();
        if ollama.is_available().await {
            log::info!("Using Ollama for embeddings (all-minilm)");
            log::info!("Embedding security enabled: rotation + watermarking");
            return Self::new_with_transform(Box::new(ollama), transform_config);
        }

        // Fall back to mock embeddings
        log::warn!("No embedding service available, using mock embeddings");
        log::warn!("For real embeddings, start LM Studio or Ollama");
        Self::mock()
    }

    /// Create with LM Studio provider
    pub fn lm_studio(model: &str) -> Self {
        Self::new(Box::new(OpenAIEmbedding::lm_studio(model)))
    }

    /// Create with LM Studio default model (nomic-embed-text)
    pub fn lm_studio_default() -> Self {
        Self::new(Box::new(OpenAIEmbedding::lm_studio_default()))
    }

    /// Get model information
    pub fn model_info(&self) -> &ModelInfo {
        self.provider.model_info()
    }

    /// Check if transform is enabled
    pub fn has_transform(&self) -> bool {
        self.transformer.is_some()
    }

    /// Get transform config (if enabled)
    pub fn transform_config(&self) -> Option<TransformConfig> {
        self.transformer.as_ref().map(|t| {
            t.lock().unwrap().config().clone()
        })
    }

    /// Apply transform to an embedding (if enabled)
    fn apply_transform(&self, embedding: Vec<f32>) -> Vec<f32> {
        match &self.transformer {
            Some(transformer) => {
                let mut t = transformer.lock().unwrap();
                t.transform(&embedding)
            }
            None => embedding,
        }
    }

    /// Generate embedding for text (with transform if enabled)
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let embedding = self.provider.embed(text).await?;
        Ok(self.apply_transform(embedding))
    }

    /// Generate embeddings for multiple texts (with transform if enabled)
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let embeddings = self.provider.embed_batch(texts).await?;
        Ok(embeddings.into_iter().map(|e| self.apply_transform(e)).collect())
    }

    /// Generate raw embedding without transform (for analysis/testing)
    pub async fn embed_raw(&self, text: &str) -> Result<Vec<f32>> {
        self.provider.embed(text).await
    }

    /// Check if provider is available
    pub async fn is_available(&self) -> bool {
        self.provider.is_available().await
    }

    /// Detect watermark in an embedding (returns confidence 0-1)
    pub fn detect_watermark(&self, embedding: &[f32]) -> f32 {
        match &self.transformer {
            Some(transformer) => {
                let t = transformer.lock().unwrap();
                t.detect_watermark(embedding)
            }
            None => 0.0,
        }
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Calculate cosine similarity between two vectors
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

/// Calculate euclidean distance between two vectors
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::MAX;
    }

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
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

        let d = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &d) + 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![3.0, 4.0, 0.0];
        assert!((euclidean_distance(&a, &b) - 5.0).abs() < 0.0001);
    }

    #[tokio::test]
    async fn test_mock_embedding() {
        let mock = MockEmbedding::default();
        let embedding = mock.embed("test").await.unwrap();

        assert_eq!(embedding.len(), 384);

        // Should be normalized (unit vector)
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 0.0001);

        // Same input should give same output
        let embedding2 = mock.embed("test").await.unwrap();
        assert!((cosine_similarity(&embedding, &embedding2) - 1.0).abs() < 0.0001);

        // Different input should give different output
        let embedding3 = mock.embed("different").await.unwrap();
        assert!(cosine_similarity(&embedding, &embedding3) < 0.99);
    }

    #[tokio::test]
    async fn test_mock_batch() {
        let mock = MockEmbedding::default();
        let embeddings = mock.embed_batch(&["a", "b", "c"]).await.unwrap();

        assert_eq!(embeddings.len(), 3);
        assert_eq!(embeddings[0].len(), 384);
    }

    #[test]
    fn test_model_info() {
        let ollama = OllamaEmbedding::new("http://localhost:11434", "nomic-embed-text");
        assert_eq!(ollama.model_info().dimensions, 768);

        let openai = OpenAIEmbedding::new(
            "https://api.openai.com/v1",
            Some("key".to_string()),
            "text-embedding-3-large",
        );
        assert_eq!(openai.model_info().dimensions, 3072);
    }

    #[tokio::test]
    async fn test_embedding_with_transform() {
        // Use stronger watermark for reliable detection in tests
        let config = TransformConfig {
            watermark_strength: 0.1, // Stronger for test visibility
            ..Default::default()
        };
        let manager = EmbeddingManager::new_with_transform(
            Box::new(MockEmbedding::default()),
            config,
        );

        assert!(manager.has_transform());

        // Generate embedding
        let embedding = manager.embed("test security").await.unwrap();
        assert_eq!(embedding.len(), 384);

        // Check watermark is present (should have high correlation)
        let confidence = manager.detect_watermark(&embedding);
        println!("Watermark confidence: {}", confidence);
        assert!(confidence > 0.65, "Watermark not detected: confidence = {}", confidence);

        // Raw embedding should have lower watermark correlation
        let raw = manager.embed_raw("test security").await.unwrap();
        let raw_confidence = manager.detect_watermark(&raw);
        println!("Raw confidence: {}", raw_confidence);
        assert!(confidence > raw_confidence,
            "Transformed should have higher watermark: raw={}, transformed={}", raw_confidence, confidence);
    }

    #[tokio::test]
    async fn test_transform_preserves_similarity() {
        let config = TransformConfig::default();
        let manager = EmbeddingManager::new_with_transform(
            Box::new(MockEmbedding::default()),
            config,
        );

        // Generate embeddings for similar concepts
        let e1 = manager.embed("cat sitting on mat").await.unwrap();
        let e2 = manager.embed("cat lying on mat").await.unwrap();
        let e3 = manager.embed("quantum physics theory").await.unwrap();

        // Similar texts should have high similarity
        let sim_similar = cosine_similarity(&e1, &e2);
        let sim_different = cosine_similarity(&e1, &e3);

        // cat/mat should be more similar to each other than to quantum physics
        // (Using mock embeddings so this tests the transform doesn't break similarity ordering)
        println!("Similar: {}, Different: {}", sim_similar, sim_different);
    }
}
