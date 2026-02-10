//! Provider Store - Persistence for custom AI provider configurations
//!
//! Stores user-configured AI providers in ~/.local/share/marlos/providers.json
//! Supports multiple providers with switching between them.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use uuid::Uuid;

/// Custom provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_timeout() -> u64 {
    60
}

impl CustomProviderConfig {
    /// Create a new provider config with generated ID
    pub fn new(name: String, base_url: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            base_url,
            api_key: None,
            model: None,
            temperature: 0.7,
            max_tokens: 4096,
            timeout_secs: 60,
        }
    }
}

/// Provider preset - quick-add templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPreset {
    pub name: String,
    pub base_url: String,
    pub requires_api_key: bool,
    pub default_model: Option<String>,
    pub description: String,
}

/// Get built-in provider presets
pub fn get_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            name: "LM Studio".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            requires_api_key: false,
            default_model: None,
            description: "Local LLM server with OpenAI-compatible API".to_string(),
        },
        ProviderPreset {
            name: "Ollama".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            requires_api_key: false,
            default_model: Some("llama3.2".to_string()),
            description: "Run local models with Ollama".to_string(),
        },
        ProviderPreset {
            name: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            requires_api_key: true,
            default_model: Some("gpt-4".to_string()),
            description: "OpenAI GPT models (requires API key)".to_string(),
        },
        ProviderPreset {
            name: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            requires_api_key: true,
            default_model: Some("claude-3-5-sonnet-20241022".to_string()),
            description: "Anthropic Claude models (requires API key)".to_string(),
        },
        ProviderPreset {
            name: "Groq".to_string(),
            base_url: "https://api.groq.com/openai/v1".to_string(),
            requires_api_key: true,
            default_model: Some("llama-3.3-70b-versatile".to_string()),
            description: "Fast inference with Groq (requires API key)".to_string(),
        },
        ProviderPreset {
            name: "Together".to_string(),
            base_url: "https://api.together.xyz/v1".to_string(),
            requires_api_key: true,
            default_model: Some("meta-llama/Llama-3.3-70B-Instruct-Turbo".to_string()),
            description: "Together AI platform (requires API key)".to_string(),
        },
        ProviderPreset {
            name: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            requires_api_key: true,
            default_model: Some("anthropic/claude-3.5-sonnet".to_string()),
            description: "Access multiple providers via OpenRouter".to_string(),
        },
    ]
}

/// Stored provider data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStoreData {
    pub custom_providers: Vec<CustomProviderConfig>,
    pub active_provider_id: Option<String>,
}

impl Default for ProviderStoreData {
    fn default() -> Self {
        // Create default LM Studio provider
        let default_provider = CustomProviderConfig::new(
            "LM Studio".to_string(),
            "http://localhost:1234/v1".to_string(),
        );
        let default_id = default_provider.id.clone();

        Self {
            custom_providers: vec![default_provider],
            active_provider_id: Some(default_id),
        }
    }
}

/// Provider store manager
pub struct ProviderStore {
    data: RwLock<ProviderStoreData>,
    storage_path: PathBuf,
}

impl ProviderStore {
    /// Create a new provider store, loading from disk if available
    pub fn new() -> Self {
        let storage_path = Self::get_storage_path();
        let data = Self::load_from_disk(&storage_path).unwrap_or_default();

        Self {
            data: RwLock::new(data),
            storage_path,
        }
    }

    /// Get the storage path for providers.json
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

        data_dir.join("providers.json")
    }

    /// Load provider data from disk
    fn load_from_disk(path: &PathBuf) -> Option<ProviderStoreData> {
        let content = fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save provider data to disk
    fn save_to_disk(&self) -> Result<(), String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        let content = serde_json::to_string_pretty(&*data)
            .map_err(|e| e.to_string())?;
        fs::write(&self.storage_path, content)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// List all custom providers
    pub fn list(&self) -> Result<Vec<CustomProviderConfig>, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.custom_providers.clone())
    }

    /// Get the active provider
    pub fn get_active(&self) -> Result<Option<CustomProviderConfig>, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        if let Some(active_id) = &data.active_provider_id {
            Ok(data.custom_providers.iter().find(|p| &p.id == active_id).cloned())
        } else {
            Ok(data.custom_providers.first().cloned())
        }
    }

    /// Get the active provider ID
    pub fn get_active_id(&self) -> Result<Option<String>, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.active_provider_id.clone())
    }

    /// Get a provider by ID
    pub fn get(&self, id: &str) -> Result<Option<CustomProviderConfig>, String> {
        let data = self.data.read().map_err(|e| e.to_string())?;
        Ok(data.custom_providers.iter().find(|p| p.id == id).cloned())
    }

    /// Add a new provider
    pub fn add(&self, mut config: CustomProviderConfig) -> Result<String, String> {
        // Generate ID if not provided
        if config.id.is_empty() {
            config.id = Uuid::new_v4().to_string();
        }

        let id = config.id.clone();

        {
            let mut data = self.data.write().map_err(|e| e.to_string())?;
            data.custom_providers.push(config);

            // If this is the first provider, make it active
            if data.active_provider_id.is_none() {
                data.active_provider_id = Some(id.clone());
            }
        }

        self.save_to_disk()?;
        log::info!("Added provider: {}", id);
        Ok(id)
    }

    /// Update an existing provider
    pub fn update(&self, id: &str, config: CustomProviderConfig) -> Result<(), String> {
        {
            let mut data = self.data.write().map_err(|e| e.to_string())?;
            if let Some(provider) = data.custom_providers.iter_mut().find(|p| p.id == id) {
                // Keep the original ID
                let original_id = provider.id.clone();
                *provider = config;
                provider.id = original_id;
            } else {
                return Err(format!("Provider not found: {}", id));
            }
        }

        self.save_to_disk()?;
        log::info!("Updated provider: {}", id);
        Ok(())
    }

    /// Delete a provider
    pub fn delete(&self, id: &str) -> Result<(), String> {
        {
            let mut data = self.data.write().map_err(|e| e.to_string())?;
            let initial_len = data.custom_providers.len();
            data.custom_providers.retain(|p| p.id != id);

            if data.custom_providers.len() == initial_len {
                return Err(format!("Provider not found: {}", id));
            }

            // If we deleted the active provider, select a new one
            if data.active_provider_id.as_deref() == Some(id) {
                data.active_provider_id = data.custom_providers.first().map(|p| p.id.clone());
            }
        }

        self.save_to_disk()?;
        log::info!("Deleted provider: {}", id);
        Ok(())
    }

    /// Set the active provider
    pub fn set_active(&self, id: &str) -> Result<(), String> {
        {
            let mut data = self.data.write().map_err(|e| e.to_string())?;

            // Verify provider exists
            if !data.custom_providers.iter().any(|p| p.id == id) {
                return Err(format!("Provider not found: {}", id));
            }

            data.active_provider_id = Some(id.to_string());
        }

        self.save_to_disk()?;
        log::info!("Set active provider: {}", id);
        Ok(())
    }

    /// Test a provider connection
    pub async fn test_provider(&self, id: &str) -> Result<bool, String> {
        let provider = self.get(id)?
            .ok_or_else(|| format!("Provider not found: {}", id))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;

        let url = format!("{}/models", provider.base_url);
        let mut request = client.get(&url);

        if let Some(api_key) = &provider.api_key {
            if !api_key.is_empty() {
                request = request.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                log::info!("Provider test for {}: {} -> {}", provider.name, url, status);
                // Accept 200 OK or some other success-ish responses
                Ok(status.is_success() || status.as_u16() == 404)
            }
            Err(e) => {
                log::warn!("Provider test failed for {}: {}", provider.name, e);
                Ok(false)
            }
        }
    }

    /// Create a provider from a preset
    pub fn create_from_preset(preset_name: &str, api_key: Option<String>) -> Option<CustomProviderConfig> {
        let presets = get_presets();
        let preset = presets.iter().find(|p| p.name == preset_name)?;

        Some(CustomProviderConfig {
            id: Uuid::new_v4().to_string(),
            name: preset.name.clone(),
            base_url: preset.base_url.clone(),
            api_key,
            model: preset.default_model.clone(),
            temperature: 0.7,
            max_tokens: 4096,
            timeout_secs: 60,
        })
    }
}

impl Default for ProviderStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presets() {
        let presets = get_presets();
        assert!(!presets.is_empty());
        assert!(presets.iter().any(|p| p.name == "LM Studio"));
        assert!(presets.iter().any(|p| p.name == "OpenAI"));
    }

    #[test]
    fn test_create_from_preset() {
        let config = ProviderStore::create_from_preset("OpenAI", Some("sk-test".to_string()));
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.name, "OpenAI");
        assert_eq!(config.api_key, Some("sk-test".to_string()));
    }
}
