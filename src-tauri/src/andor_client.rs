//! Andor Hub Client - HTTP interface to query Andor Hub's API
//!
//! Provides read-only access to Andor Hub's:
//! - Sessions (AI coding sessions)
//! - Memories (extracted facts, decisions, patterns)
//! - Context (repo semantic search)
//!
//! MarlOS can read from Andor, but Andor cannot access MarlOS data.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Andor Hub API client
pub struct AndorClient {
    client: Client,
    base_url: String,
}

/// Error types for Andor client
#[derive(Debug)]
pub enum AndorError {
    Http(reqwest::Error),
    Api(String),
    NotRunning,
}

impl std::fmt::Display for AndorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AndorError::Http(e) => write!(f, "HTTP error: {}", e),
            AndorError::Api(msg) => write!(f, "API error: {}", msg),
            AndorError::NotRunning => write!(f, "Andor Hub is not running"),
        }
    }
}

impl std::error::Error for AndorError {}

impl From<reqwest::Error> for AndorError {
    fn from(e: reqwest::Error) -> Self {
        AndorError::Http(e)
    }
}

pub type Result<T> = std::result::Result<T, AndorError>;

// ============================================================================
// Session Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "type")]
    pub session_type: Option<String>,
    pub status: Option<String>,
    pub project: Option<String>,
    #[serde(rename = "repoPath")]
    pub repo_path: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    #[serde(rename = "messageCount")]
    pub message_count: Option<i32>,
    pub summary: Option<String>,
    pub themes: Option<Vec<String>>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListResponse {
    pub sessions: Vec<Session>,
    pub total: i32,
    pub page: i32,
    #[serde(rename = "pageSize")]
    pub page_size: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub total: i32,
    pub active: i32,
    pub completed: i32,
    #[serde(rename = "byType")]
    pub by_type: Option<serde_json::Value>,
    #[serde(rename = "byRepo")]
    pub by_repo: Option<serde_json::Value>,
}

// ============================================================================
// Memory Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String,
    pub namespace: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub relevance: Option<f32>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQueryRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub memory_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>, // "exact", "semantic", "hybrid"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQueryResponse {
    pub memories: Vec<Memory>,
    pub total: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total: i32,
    #[serde(rename = "byType")]
    pub by_type: Option<serde_json::Value>,
    #[serde(rename = "byNamespace")]
    pub by_namespace: Option<serde_json::Value>,
}

// ============================================================================
// Context Types (Repo Semantic Search)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSearchRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "repoName")]
    pub repo_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    pub path: String,
    pub summary: Option<String>,
    pub language: Option<String>,
    pub themes: Option<Vec<String>>,
    pub score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSearchResponse {
    pub files: Vec<ContextFile>,
    pub total: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub name: String,
    #[serde(rename = "fileCount")]
    pub file_count: Option<i32>,
    #[serde(rename = "lastUpdated")]
    pub last_updated: Option<String>,
}

// ============================================================================
// Semantic Search Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "minScore")]
    pub min_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub score: f32,
    pub summary: Option<String>,
    pub themes: Option<Vec<String>>,
    #[serde(rename = "repoPath")]
    pub repo_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResponse {
    pub results: Vec<SemanticSearchResult>,
}

// ============================================================================
// Client Implementation
// ============================================================================

impl AndorClient {
    /// Create a new Andor client with default localhost URL
    pub fn new() -> Self {
        Self::with_url("http://localhost:8080")
    }

    /// Create with custom URL
    pub fn with_url(base_url: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Check if Andor Hub is running
    pub fn is_running(&self) -> bool {
        self.client
            .get(&format!("{}/api/sessions/stats", self.base_url))
            .send()
            .is_ok()
    }

    // ========================================================================
    // Session APIs
    // ========================================================================

    /// List sessions with optional filters
    pub fn list_sessions(
        &self,
        status: Option<&str>,
        session_type: Option<&str>,
        repo_path: Option<&str>,
        limit: Option<i32>,
    ) -> Result<SessionListResponse> {
        let mut url = format!("{}/api/sessions/list", self.base_url);
        let mut params = vec![];

        if let Some(s) = status {
            params.push(format!("status={}", s));
        }
        if let Some(t) = session_type {
            params.push(format!("type={}", t));
        }
        if let Some(r) = repo_path {
            params.push(format!("repoPath={}", urlencoding::encode(r)));
        }
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }

        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        let response = self.client.get(&url).send()?;

        if !response.status().is_success() {
            return Err(AndorError::Api(format!(
                "Status {}: {}",
                response.status(),
                response.text().unwrap_or_default()
            )));
        }

        Ok(response.json()?)
    }

    /// Get session statistics
    pub fn session_stats(&self) -> Result<SessionStats> {
        let response = self
            .client
            .get(&format!("{}/api/sessions/stats", self.base_url))
            .send()?;

        if !response.status().is_success() {
            return Err(AndorError::Api(format!(
                "Status {}: {}",
                response.status(),
                response.text().unwrap_or_default()
            )));
        }

        Ok(response.json()?)
    }

    /// Get a specific session by ID
    pub fn get_session(&self, session_id: &str) -> Result<Session> {
        let response = self
            .client
            .get(&format!("{}/api/sessions/{}", self.base_url, session_id))
            .send()?;

        if !response.status().is_success() {
            return Err(AndorError::Api(format!(
                "Status {}: {}",
                response.status(),
                response.text().unwrap_or_default()
            )));
        }

        Ok(response.json()?)
    }

    // ========================================================================
    // Memory APIs
    // ========================================================================

    /// Query memories (semantic search)
    pub fn query_memories(&self, request: MemoryQueryRequest) -> Result<MemoryQueryResponse> {
        let response = self
            .client
            .post(&format!("{}/api/memory/query", self.base_url))
            .json(&request)
            .send()?;

        if !response.status().is_success() {
            return Err(AndorError::Api(format!(
                "Status {}: {}",
                response.status(),
                response.text().unwrap_or_default()
            )));
        }

        Ok(response.json()?)
    }

    /// Get memory statistics
    pub fn memory_stats(&self) -> Result<MemoryStats> {
        let response = self
            .client
            .get(&format!("{}/api/memory/stats", self.base_url))
            .send()?;

        if !response.status().is_success() {
            return Err(AndorError::Api(format!(
                "Status {}: {}",
                response.status(),
                response.text().unwrap_or_default()
            )));
        }

        Ok(response.json()?)
    }

    /// Get memories for a specific session
    pub fn session_memories(&self, session_id: &str) -> Result<Vec<Memory>> {
        let response = self
            .client
            .get(&format!("{}/api/memory/session/{}", self.base_url, session_id))
            .send()?;

        if !response.status().is_success() {
            return Err(AndorError::Api(format!(
                "Status {}: {}",
                response.status(),
                response.text().unwrap_or_default()
            )));
        }

        #[derive(Deserialize)]
        struct Wrapper {
            memories: Vec<Memory>,
        }

        let wrapper: Wrapper = response.json()?;
        Ok(wrapper.memories)
    }

    // ========================================================================
    // Context APIs (Repo Semantic Search)
    // ========================================================================

    /// List repos with context maps
    pub fn list_repos(&self) -> Result<Vec<RepoInfo>> {
        let response = self
            .client
            .get(&format!("{}/api/context/repos", self.base_url))
            .send()?;

        if !response.status().is_success() {
            return Err(AndorError::Api(format!(
                "Status {}: {}",
                response.status(),
                response.text().unwrap_or_default()
            )));
        }

        #[derive(Deserialize)]
        struct Wrapper {
            repos: Vec<RepoInfo>,
        }

        let wrapper: Wrapper = response.json()?;
        Ok(wrapper.repos)
    }

    /// Search context (repo files by semantic meaning)
    pub fn search_context(&self, request: ContextSearchRequest) -> Result<ContextSearchResponse> {
        let response = self
            .client
            .post(&format!("{}/api/context/search", self.base_url))
            .json(&request)
            .send()?;

        if !response.status().is_success() {
            return Err(AndorError::Api(format!(
                "Status {}: {}",
                response.status(),
                response.text().unwrap_or_default()
            )));
        }

        Ok(response.json()?)
    }

    // ========================================================================
    // Semantic Search APIs
    // ========================================================================

    /// Search sessions by semantic meaning
    pub fn semantic_search(&self, request: SemanticSearchRequest) -> Result<SemanticSearchResponse> {
        let response = self
            .client
            .post(&format!("{}/api/semantic/search", self.base_url))
            .json(&request)
            .send()?;

        if !response.status().is_success() {
            return Err(AndorError::Api(format!(
                "Status {}: {}",
                response.status(),
                response.text().unwrap_or_default()
            )));
        }

        Ok(response.json()?)
    }
}

impl Default for AndorClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = AndorClient::new();
        assert_eq!(client.base_url, "http://localhost:8080");
    }

    #[test]
    fn test_custom_url() {
        let client = AndorClient::with_url("http://localhost:9000/");
        assert_eq!(client.base_url, "http://localhost:9000");
    }
}
