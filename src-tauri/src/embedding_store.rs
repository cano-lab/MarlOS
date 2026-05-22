//! Embedding Store - Persistence for embedding provider and search configurations
//!
//! Stores user-configured embedding settings in ~/.local/share/marlos/embeddings.json
//! Supports configurable embedding providers and search parameters.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

/// Supported embedding provider types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingProviderType {
    LmStudio,
    Ollama,
    OpenAI,
    Mock,
}

impl Default for EmbeddingProviderType {
    fn default() -> Self {
        Self::LmStudio
    }
}

/// Embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider_type: EmbeddingProviderType,
    pub base_url: String,
    pub model: String,
    pub dimensions: usize,
    pub max_tokens: usize,
    #[serde(default)]
    pub api_key: Option<String>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider_type: EmbeddingProviderType::LmStudio,
            base_url: "http://localhost:4321/v1".to_string(),
            model: "text-embedding-qwen3-embedding-0.6b".to_string(),
            dimensions: 1024,
            max_tokens: 8192,
            api_key: None,
        }
    }
}

/// Field weights for keyword scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldWeights {
    #[serde(default = "default_name_weight")]
    pub name: f32,
    #[serde(default = "default_summary_weight")]
    pub summary: f32,
    #[serde(default = "default_tags_weight")]
    pub tags: f32,
    #[serde(default = "default_content_weight")]
    pub content: f32,
}

fn default_name_weight() -> f32 { 1.5 }
fn default_summary_weight() -> f32 { 1.2 }
fn default_tags_weight() -> f32 { 1.3 }
fn default_content_weight() -> f32 { 1.0 }

impl Default for FieldWeights {
    fn default() -> Self {
        Self {
            name: 1.5,
            summary: 1.2,
            tags: 1.3,
            content: 1.0,
        }
    }
}

/// Search configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Minimum similarity score threshold (0.0 to 1.0)
    #[serde(default = "default_min_score")]
    pub min_score: f32,
    /// Boost factor for keyword matches (0.0 to 0.5)
    #[serde(default = "default_keyword_boost")]
    pub keyword_boost: f32,
    /// Whether to include keyword search results
    #[serde(default = "default_include_keyword")]
    pub include_keyword: bool,
    /// Recency decay half-life in days (0 = disabled)
    #[serde(default = "default_recency_decay")]
    pub recency_decay: f32,
    /// Field-specific weights for keyword matching
    #[serde(default)]
    pub field_weights: FieldWeights,
}

fn default_min_score() -> f32 { 0.3 }
fn default_keyword_boost() -> f32 { 0.2 }
fn default_include_keyword() -> bool { true }
fn default_recency_decay() -> f32 { 7.0 }

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            min_score: 0.3,
            keyword_boost: 0.2,
            include_keyword: true,
            recency_decay: 7.0,
            field_weights: FieldWeights::default(),
        }
    }
}

/// Embedding model preset for quick configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelPreset {
    pub name: String,
    pub model_id: String,
    pub dimensions: usize,
    pub max_tokens: usize,
    pub provider: EmbeddingProviderType,
    pub description: String,
}

/// Get built-in embedding model presets
pub fn get_embedding_presets() -> Vec<EmbeddingModelPreset> {
    vec![
        // LM Studio models
        EmbeddingModelPreset {
            name: "Qwen3 0.6B".to_string(),
            model_id: "text-embedding-qwen3-embedding-0.6b".to_string(),
            dimensions: 1024,
            max_tokens: 8192,
            provider: EmbeddingProviderType::LmStudio,
            description: "Fast, efficient embedding model".to_string(),
        },
        EmbeddingModelPreset {
            name: "Qwen3 1.5B".to_string(),
            model_id: "text-embedding-qwen3-embedding-1.5b".to_string(),
            dimensions: 1024,
            max_tokens: 8192,
            provider: EmbeddingProviderType::LmStudio,
            description: "Higher quality, more compute".to_string(),
        },
        EmbeddingModelPreset {
            name: "Nomic Embed Text".to_string(),
            model_id: "text-embedding-nomic-embed-text-v1.5".to_string(),
            dimensions: 768,
            max_tokens: 8192,
            provider: EmbeddingProviderType::LmStudio,
            description: "Open-source alternative".to_string(),
        },
        // Ollama models
        EmbeddingModelPreset {
            name: "all-minilm".to_string(),
            model_id: "all-minilm".to_string(),
            dimensions: 384,
            max_tokens: 512,
            provider: EmbeddingProviderType::Ollama,
            description: "Lightweight, fast".to_string(),
        },
        EmbeddingModelPreset {
            name: "nomic-embed-text".to_string(),
            model_id: "nomic-embed-text".to_string(),
            dimensions: 768,
            max_tokens: 8192,
            provider: EmbeddingProviderType::Ollama,
            description: "Good quality, moderate size".to_string(),
        },
        EmbeddingModelPreset {
            name: "mxbai-embed-large".to_string(),
            model_id: "mxbai-embed-large".to_string(),
            dimensions: 1024,
            max_tokens: 512,
            provider: EmbeddingProviderType::Ollama,
            description: "High quality, larger model".to_string(),
        },
        // OpenAI models
        EmbeddingModelPreset {
            name: "text-embedding-3-small".to_string(),
            model_id: "text-embedding-3-small".to_string(),
            dimensions: 1536,
            max_tokens: 8191,
            provider: EmbeddingProviderType::OpenAI,
            description: "OpenAI small model".to_string(),
        },
        EmbeddingModelPreset {
            name: "text-embedding-3-large".to_string(),
            model_id: "text-embedding-3-large".to_string(),
            dimensions: 3072,
            max_tokens: 8191,
            provider: EmbeddingProviderType::OpenAI,
            description: "OpenAI large model".to_string(),
        },
    ]
}

/// Stored embedding configuration data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingStoreData {
    pub embedding_config: EmbeddingConfig,
    pub search_config: SearchConfig,
}

impl Default for EmbeddingStoreData {
    fn default() -> Self {
        Self {
            embedding_config: EmbeddingConfig::default(),
            search_config: SearchConfig::default(),
        }
    }
}

/// Embedding store manager
pub struct EmbeddingStore {
    data: RwLock<EmbeddingStoreData>,
    storage_path: PathBuf,
}

impl EmbeddingStore {
    /// Create a new embedding store, loading from disk if available
    pub fn new() -> Self {
        let storage_path = Self::get_storage_path();
        let data = Self::load_from_disk(&storage_path).unwrap_or_default();

        Self {
            data: RwLock::new(data),
            storage_path,
        }
    }

    /// Get the storage path for embeddings.json
    fn get_storage_path() -> PathBuf {
        let data_dir = if cfg!(target_os = "windows") {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("marlos")
        } else {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from(".local/share"))
                .join("marlos")
        };

        // Ensure directory exists
        let _ = fs::create_dir_all(&data_dir);

        data_dir.join("embeddings.json")
    }

    /// Load embedding data from disk
    fn load_from_disk(path: &PathBuf) -> Option<EmbeddingStoreData> {
        let content = fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save embedding data to disk
    fn save_to_disk(&self) -> Result<(), String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        let content = serde_json::to_string_pretty(&*data)
            .map_err(|e| e.to_string())?;
        fs::write(&self.storage_path, content)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get the current embedding configuration
    pub fn get_embedding_config(&self) -> Result<EmbeddingConfig, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.embedding_config.clone())
    }

    /// Set the embedding configuration
    pub fn set_embedding_config(&self, config: EmbeddingConfig) -> Result<(), String> {
        {
            let mut data = self.data.write().map_err(|e| e.to_string())?;
            data.embedding_config = config;
        }
        self.save_to_disk()?;
        log::info!("Updated embedding configuration");
        Ok(())
    }

    /// Get the current search configuration
    pub fn get_search_config(&self) -> Result<SearchConfig, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.search_config.clone())
    }

    /// Set the search configuration
    pub fn set_search_config(&self, config: SearchConfig) -> Result<(), String> {
        {
            let mut data = self.data.write().map_err(|e| e.to_string())?;
            data.search_config = config;
        }
        self.save_to_disk()?;
        log::info!("Updated search configuration");
        Ok(())
    }

    /// Reset to default configuration
    pub fn reset_to_defaults(&self) -> Result<(), String> {
        {
            let mut data = self.data.write().map_err(|e| e.to_string())?;
            *data = EmbeddingStoreData::default();
        }
        self.save_to_disk()?;
        log::info!("Reset embedding configuration to defaults");
        Ok(())
    }

    /// Check if dimensions have changed (requires reindex)
    pub fn dimensions_changed(&self, new_dimensions: usize) -> Result<bool, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.embedding_config.dimensions != new_dimensions)
    }

    /// Get the base URL for the current provider
    pub fn get_base_url(&self) -> Result<String, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.embedding_config.base_url.clone())
    }

    /// Create embedding config from a preset
    pub fn config_from_preset(preset_name: &str) -> Option<EmbeddingConfig> {
        let presets = get_embedding_presets();
        let preset = presets.iter().find(|p| p.name == preset_name)?;

        let base_url = match preset.provider {
            EmbeddingProviderType::LmStudio => "http://localhost:4321/v1".to_string(),
            EmbeddingProviderType::Ollama => "http://localhost:11434".to_string(),
            EmbeddingProviderType::OpenAI => "https://api.openai.com/v1".to_string(),
            EmbeddingProviderType::Mock => "mock://".to_string(),
        };

        Some(EmbeddingConfig {
            provider_type: preset.provider.clone(),
            base_url,
            model: preset.model_id.clone(),
            dimensions: preset.dimensions,
            max_tokens: preset.max_tokens,
            api_key: None,
        })
    }
}

impl Default for EmbeddingStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Test an embedding provider connection
pub async fn test_embedding_provider(config: &EmbeddingConfig) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = match config.provider_type {
        EmbeddingProviderType::LmStudio | EmbeddingProviderType::OpenAI => {
            format!("{}/models", config.base_url)
        }
        EmbeddingProviderType::Ollama => {
            format!("{}/api/tags", config.base_url.trim_end_matches("/v1"))
        }
        EmbeddingProviderType::Mock => {
            return Ok(true); // Mock is always available
        }
    };

    let mut request = client.get(&url);

    if let Some(api_key) = &config.api_key {
        if !api_key.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }
    }

    match request.send().await {
        Ok(response) => {
            let status = response.status();
            log::info!("Embedding provider test: {} -> {}", url, status);
            Ok(status.is_success())
        }
        Err(e) => {
            log::warn!("Embedding provider test failed: {}", e);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.provider_type, EmbeddingProviderType::LmStudio);
        assert_eq!(config.dimensions, 1024);
        assert!(config.model.contains("qwen3"));
    }

    #[test]
    fn test_default_search_config() {
        let config = SearchConfig::default();
        assert_eq!(config.min_score, 0.3);
        assert_eq!(config.keyword_boost, 0.2);
        assert!(config.include_keyword);
        assert_eq!(config.recency_decay, 7.0);
    }

    #[test]
    fn test_field_weights_default() {
        let weights = FieldWeights::default();
        assert_eq!(weights.name, 1.5);
        assert_eq!(weights.summary, 1.2);
        assert_eq!(weights.tags, 1.3);
        assert_eq!(weights.content, 1.0);
    }

    #[test]
    fn test_presets() {
        let presets = get_embedding_presets();
        assert!(!presets.is_empty());
        assert!(presets.iter().any(|p| p.name == "Qwen3 0.6B"));
        assert!(presets.iter().any(|p| p.name == "all-minilm"));
    }

    #[test]
    fn test_config_from_preset() {
        let config = EmbeddingStore::config_from_preset("Qwen3 0.6B");
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.provider_type, EmbeddingProviderType::LmStudio);
        assert_eq!(config.dimensions, 1024);
    }

    #[test]
    fn test_serialization() {
        let data = EmbeddingStoreData::default();
        let json = serde_json::to_string(&data).unwrap();
        let parsed: EmbeddingStoreData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.embedding_config.dimensions, data.embedding_config.dimensions);
    }
}
