//! Semantic Search - Find objects by meaning
//!
//! Provides semantic search capabilities by combining:
//! - ObjectStore for persistent storage
//! - Embeddings for vector representations
//! - Cosine similarity for semantic matching
//!
//! Supports both pure semantic search and hybrid (semantic + keyword) search.
//! Enhanced with recency boosting and field-weighted keyword scoring.

use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

use crate::embeddings::{cosine_similarity, EmbeddingManager};
use crate::embedding_store::{FieldWeights, SearchConfig};
use crate::memory::SecurityTier;
use crate::object_store::{ObjectStore, ObjectStoreError};
use crate::semantic_object::{GuardedObjectView, SemanticObject, Suid};

/// Errors specific to semantic search
#[derive(Debug)]
pub enum SearchError {
    Store(ObjectStoreError),
    Embedding(crate::embeddings::EmbeddingError),
    NoEmbedding,
}

impl From<ObjectStoreError> for SearchError {
    fn from(e: ObjectStoreError) -> Self {
        SearchError::Store(e)
    }
}

impl From<crate::embeddings::EmbeddingError> for SearchError {
    fn from(e: crate::embeddings::EmbeddingError) -> Self {
        SearchError::Embedding(e)
    }
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::Store(e) => write!(f, "Store error: {}", e),
            SearchError::Embedding(e) => write!(f, "Embedding error: {}", e),
            SearchError::NoEmbedding => write!(f, "Object has no embedding"),
        }
    }
}

impl std::error::Error for SearchError {}

pub type Result<T> = std::result::Result<T, SearchError>;

// ============================================================================
// Enhanced Scoring Functions
// ============================================================================

/// Calculate recency boost based on document age
/// Returns a boost value between 0.0 and 0.15 (max 15% boost for recent items)
pub fn calculate_recency_boost(modified_at: &DateTime<Utc>, decay_days: f32) -> f32 {
    if decay_days <= 0.0 {
        return 0.0;
    }

    let days_old = (Utc::now() - *modified_at).num_days() as f32;
    if days_old < 0.0 {
        return 0.15; // Future dates get max boost
    }

    // Exponential decay: e^(-days / decay_days) * 0.15
    (-days_old / decay_days).exp() * 0.15
}

/// Calculate field-weighted keyword score
/// Returns a normalized score based on which fields contain the query
pub fn calculate_field_weighted_score(
    query: &str,
    object: &SemanticObject,
    weights: &FieldWeights,
) -> f32 {
    let mut score = 0.0;
    let query_lower = query.to_lowercase();
    let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

    if query_terms.is_empty() {
        return 0.0;
    }

    // Check name field
    if let Some(ref name) = object.name {
        let name_lower = name.to_lowercase();
        let matches = query_terms.iter().filter(|t| name_lower.contains(*t)).count();
        if matches > 0 {
            score += weights.name * (matches as f32 / query_terms.len() as f32);
        }
    }

    // Check summary field
    if let Some(ref summary) = object.summary {
        let summary_lower = summary.to_lowercase();
        let matches = query_terms.iter().filter(|t| summary_lower.contains(*t)).count();
        if matches > 0 {
            score += weights.summary * (matches as f32 / query_terms.len() as f32);
        }
    }

    // Check tags
    let tag_matches = object.tags.iter().filter(|tag| {
        let tag_lower = tag.to_lowercase();
        query_terms.iter().any(|t| tag_lower.contains(*t))
    }).count();
    if tag_matches > 0 {
        score += weights.tags * (tag_matches as f32 / object.tags.len().max(1) as f32).min(1.0);
    }

    // Check content (with lower weight)
    if let Some(content) = object.content_as_str() {
        let content_lower = content.to_lowercase();
        let matches = query_terms.iter().filter(|t| content_lower.contains(*t)).count();
        if matches > 0 {
            score += weights.content * (matches as f32 / query_terms.len() as f32);
        }
    }

    // Normalize to 0-1 range based on max possible score
    let max_score = weights.name + weights.summary + weights.tags + weights.content;
    if max_score > 0.0 {
        (score / max_score).min(1.0)
    } else {
        0.0
    }
}

/// A search result with relevance score
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// The matching object
    pub object: SemanticObject,
    /// Similarity score (0.0 to 1.0)
    pub score: f32,
    /// How the match was found
    pub match_type: MatchType,
}

/// How a search result was matched
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatchType {
    /// Matched via semantic similarity
    Semantic,
    /// Matched via keyword/text
    Keyword,
    /// Matched via both semantic and keyword
    Hybrid,
}

/// Options for semantic search
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Maximum number of results
    pub limit: usize,
    /// Minimum similarity score (0.0 to 1.0)
    pub min_score: f32,
    /// Maximum security tier to include
    pub max_tier: SecurityTier,
    /// Whether to include keyword results
    pub include_keyword: bool,
    /// Boost factor for keyword matches in hybrid search
    pub keyword_boost: f32,
    /// Recency decay half-life in days (0 = disabled)
    pub recency_decay: f32,
    /// Field weights for keyword scoring
    pub field_weights: FieldWeights,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 10,
            min_score: 0.3,
            max_tier: SecurityTier::Guarded,
            include_keyword: true,
            keyword_boost: 0.2,
            recency_decay: 7.0,
            field_weights: FieldWeights::default(),
        }
    }
}

impl SearchOptions {
    /// Create SearchOptions from a SearchConfig
    pub fn from_config(config: &SearchConfig) -> Self {
        Self {
            min_score: config.min_score,
            keyword_boost: config.keyword_boost,
            include_keyword: config.include_keyword,
            recency_decay: config.recency_decay,
            field_weights: config.field_weights.clone(),
            ..Default::default()
        }
    }

    /// Apply a SearchConfig to these options
    pub fn with_config(mut self, config: &SearchConfig) -> Self {
        self.min_score = config.min_score;
        self.keyword_boost = config.keyword_boost;
        self.include_keyword = config.include_keyword;
        self.recency_decay = config.recency_decay;
        self.field_weights = config.field_weights.clone();
        self
    }
}

/// Semantic search engine
pub struct SemanticSearch {
    /// Object store (public for direct access from commands)
    pub store: Arc<RwLock<ObjectStore>>,
    embeddings: Arc<EmbeddingManager>,
}

impl SemanticSearch {
    /// Create a new semantic search engine with shared Arc<ObjectStore>
    pub fn with_arc(store: Arc<ObjectStore>, embeddings: EmbeddingManager) -> Self {
        Self {
            store: Arc::new(RwLock::new(Arc::try_unwrap(store).unwrap_or_else(|_| {
                panic!("Cannot unwrap Arc - multiple references exist")
            }))),
            embeddings: Arc::new(embeddings),
        }
    }

    /// Create a new semantic search engine
    pub fn new(store: ObjectStore, embeddings: EmbeddingManager) -> Self {
        Self {
            store: Arc::new(RwLock::new(store)),
            embeddings: Arc::new(embeddings),
        }
    }

    /// Create with default paths and mock embeddings (for testing)
    pub fn in_memory_mock() -> Result<Self> {
        let store = ObjectStore::in_memory()?;
        let embeddings = EmbeddingManager::mock();
        Ok(Self::new(store, embeddings))
    }

    /// Get the embedding manager
    pub fn embeddings(&self) -> &EmbeddingManager {
        &self.embeddings
    }

    // ========================================================================
    // Object Management
    // ========================================================================

    /// Store a new object and generate its embedding
    pub async fn store(&self, object: &SemanticObject) -> Result<()> {
        // Store the object
        {
            let store = self.store.write().await;
            store.create(object)?;
        }

        // Generate and store embedding if text content
        if object.content_type.is_text() {
            if let Some(text) = object.content_as_str() {
                self.index_object(&object.suid, text).await?;
            }
        }

        Ok(())
    }

    /// Update an object and regenerate its embedding
    pub async fn update(&self, object: &SemanticObject) -> Result<()> {
        // Update the object
        {
            let store = self.store.write().await;
            store.update(object)?;
        }

        // Regenerate embedding if text content
        if object.content_type.is_text() {
            if let Some(text) = object.content_as_str() {
                self.index_object(&object.suid, text).await?;
            }
        }

        Ok(())
    }

    /// Get an object by SUID
    pub async fn get(&self, suid: &Suid) -> Result<Option<SemanticObject>> {
        let store = self.store.read().await;
        Ok(store.get(suid)?)
    }

    /// Delete an object
    pub async fn delete(&self, suid: &Suid) -> Result<bool> {
        let store = self.store.write().await;
        Ok(store.delete(suid)?)
    }

    // ========================================================================
    // Indexing
    // ========================================================================

    /// Generate and store embedding for an object
    pub async fn index_object(&self, suid: &Suid, text: &str) -> Result<()> {
        let embedding = self.embeddings.embed(text).await?;
        let model = self.embeddings.model_info().id.clone();

        let store = self.store.write().await;
        store.store_embedding(suid, &embedding, &model)?;

        Ok(())
    }

    /// Index all objects that don't have embeddings
    pub async fn index_missing(&self) -> Result<usize> {
        // First, collect objects that need indexing
        let objects_to_index: Vec<(Suid, String)> = {
            let store = self.store.read().await;
            let objects = store.list(1000, 0)?;

            let mut to_index = Vec::new();
            for obj in objects {
                // Check if embedding exists in database
                let has_embedding = store.get_embedding(&obj.suid)?.is_some();
                if !has_embedding && obj.content_type.is_text() {
                    if let Some(text) = obj.content_as_str() {
                        to_index.push((obj.suid, text.to_string()));
                    }
                }
            }
            to_index
        }; // Lock released here

        // Now index each one
        let mut indexed = 0;
        for (suid, text) in objects_to_index {
            self.index_object(&suid, &text).await?;
            indexed += 1;
        }

        Ok(indexed)
    }

    /// Reindex all objects (regenerate all embeddings)
    pub async fn reindex_all(&self) -> Result<usize> {
        let objects = {
            let store = self.store.read().await;
            store.list(10000, 0)?
        };

        let mut indexed = 0;
        for obj in objects {
            if obj.content_type.is_text() {
                if let Some(text) = obj.content_as_str() {
                    self.index_object(&obj.suid, text).await?;
                    indexed += 1;
                }
            }
        }

        Ok(indexed)
    }

    // ========================================================================
    // Search
    // ========================================================================

    /// Search objects by semantic similarity with enhanced scoring
    pub async fn search(&self, query: &str, options: SearchOptions) -> Result<Vec<SearchHit>> {
        // Generate query embedding
        let query_embedding = self.embeddings.embed(query).await?;

        // Get all objects with embeddings
        let objects_with_embeddings = {
            let store = self.store.read().await;
            store.get_objects_with_embeddings(options.max_tier)?
        };

        // Calculate similarity scores with recency boost
        let mut hits: Vec<SearchHit> = objects_with_embeddings
            .into_iter()
            .map(|(obj, embedding)| {
                // Base semantic score
                let semantic_score = cosine_similarity(&query_embedding, &embedding);

                // Calculate recency boost
                let recency_boost = calculate_recency_boost(&obj.modified_at, options.recency_decay);

                // Combined score (semantic + recency)
                let score = (semantic_score + recency_boost).min(1.0);

                SearchHit {
                    object: obj,
                    score,
                    match_type: MatchType::Semantic,
                }
            })
            .filter(|hit| hit.score >= options.min_score)
            .collect();

        // Add keyword matches if requested
        if options.include_keyword {
            let keyword_hits = self.search_keyword_weighted(query, &options).await?;

            // Merge results, boosting keyword matches
            for kw_hit in keyword_hits {
                if let Some(existing) = hits.iter_mut().find(|h| h.object.suid == kw_hit.object.suid) {
                    // Object found by both - calculate combined boost
                    let keyword_weight = calculate_field_weighted_score(
                        query,
                        &existing.object,
                        &options.field_weights,
                    );
                    let boost = keyword_weight * options.keyword_boost;
                    existing.score = (existing.score + boost).min(1.0);
                    existing.match_type = MatchType::Hybrid;
                } else {
                    // Only found by keyword
                    hits.push(kw_hit);
                }
            }
        }

        // Sort by score descending
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Limit results
        hits.truncate(options.limit);

        Ok(hits)
    }

    /// Search objects by keyword only (legacy method)
    async fn search_keyword(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchHit>> {
        self.search_keyword_weighted(query, options).await
    }

    /// Search objects by keyword with field-weighted scoring
    async fn search_keyword_weighted(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchHit>> {
        let store = self.store.read().await;
        let objects = store.search_text(query, options.limit * 2)?; // Get more for merging

        Ok(objects
            .into_iter()
            .filter(|obj| obj.security_tier.can_access(options.max_tier))
            .map(|obj| {
                // Calculate field-weighted score
                let field_score = calculate_field_weighted_score(query, &obj, &options.field_weights);

                // Calculate recency boost
                let recency_boost = calculate_recency_boost(&obj.modified_at, options.recency_decay);

                // Base keyword score (0.5) + field weighting + recency
                let score = (0.5 + field_score * 0.3 + recency_boost).min(1.0);

                SearchHit {
                    object: obj,
                    score,
                    match_type: MatchType::Keyword,
                }
            })
            .collect())
    }

    /// Find similar objects to a given object
    pub async fn find_similar(&self, suid: &Suid, limit: usize) -> Result<Vec<SearchHit>> {
        // Get the object's embedding
        let embedding = {
            let store = self.store.read().await;
            store.get_embedding(suid)?
                .ok_or(SearchError::NoEmbedding)?
        };

        // Get all objects with embeddings
        let objects_with_embeddings = {
            let store = self.store.read().await;
            store.get_objects_with_embeddings(SecurityTier::Sealed)?
        };

        // Calculate similarity scores (excluding the query object)
        let mut hits: Vec<SearchHit> = objects_with_embeddings
            .into_iter()
            .filter(|(obj, _)| &obj.suid != suid)
            .map(|(obj, obj_embedding)| {
                let score = cosine_similarity(&embedding, &obj_embedding);
                SearchHit {
                    object: obj,
                    score,
                    match_type: MatchType::Semantic,
                }
            })
            .collect();

        // Sort by score descending
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Limit results
        hits.truncate(limit);

        Ok(hits)
    }

    // ========================================================================
    // LLM-Safe Access
    // ========================================================================

    /// Search with tier-aware response (for LLM consumption)
    pub async fn search_for_llm(
        &self,
        query: &str,
        max_tier: SecurityTier,
        limit: usize,
    ) -> Result<Vec<LlmSearchResult>> {
        let options = SearchOptions {
            limit,
            max_tier,
            ..Default::default()
        };

        let hits = self.search(query, options).await?;

        Ok(hits
            .into_iter()
            .map(|hit| {
                let tier = hit.object.security_tier;
                if tier == SecurityTier::Open {
                    LlmSearchResult::Full {
                        object: hit.object,
                        score: hit.score,
                    }
                } else {
                    LlmSearchResult::Guarded {
                        view: hit.object.to_guarded_view(),
                        score: hit.score,
                    }
                }
            })
            .collect())
    }
}

/// Search result formatted for LLM consumption
#[derive(Debug)]
pub enum LlmSearchResult {
    /// Full object (Open tier)
    Full {
        object: SemanticObject,
        score: f32,
    },
    /// Guarded view (Guarded tier)
    Guarded {
        view: GuardedObjectView,
        score: f32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_search() -> SemanticSearch {
        SemanticSearch::in_memory_mock().unwrap()
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let search = create_test_search().await;

        let obj = SemanticObject::from_text("Hello, world!")
            .with_name("greeting")
            .with_tier(SecurityTier::Open);

        search.store(&obj).await.unwrap();

        let retrieved = search.get(&obj.suid).await.unwrap().unwrap();
        assert_eq!(retrieved.name, Some("greeting".to_string()));
    }

    #[tokio::test]
    async fn test_semantic_search() {
        let search = create_test_search().await;

        // Store documents directly (same approach as tier_filtering which works)
        {
            let store = search.store.write().await;

            let doc1 = SemanticObject::from_text("The quick brown fox jumps over the lazy dog")
                .with_name("fox");
            store.create(&doc1).unwrap();
            store.store_embedding(&doc1.suid, &[0.1; 384], "mock").unwrap();

            let doc2 = SemanticObject::from_text("Machine learning and artificial intelligence")
                .with_name("ml");
            store.create(&doc2).unwrap();
            store.store_embedding(&doc2.suid, &[0.2; 384], "mock").unwrap();

            let doc3 = SemanticObject::from_text("The cat sat on the mat")
                .with_name("cat");
            store.create(&doc3).unwrap();
            store.store_embedding(&doc3.suid, &[0.3; 384], "mock").unwrap();
        }

        // Search - use min_score 0.0 for mock embeddings, max_tier Sealed to get all
        let results = search.search("animals like foxes and dogs", SearchOptions {
            include_keyword: false,
            min_score: 0.0,
            max_tier: SecurityTier::Sealed,
            ..Default::default()
        }).await.unwrap();

        // With 3 documents stored and indexed, we should get 3 results
        assert_eq!(results.len(), 3);

        // Verify all results have semantic match type
        for result in &results {
            assert_eq!(result.match_type, MatchType::Semantic);
        }
    }

    #[tokio::test]
    async fn test_find_similar() {
        let search = create_test_search().await;

        let doc1 = SemanticObject::from_text("Rust programming language")
            .with_name("rust");
        let doc2 = SemanticObject::from_text("Python programming language")
            .with_name("python");
        let doc3 = SemanticObject::from_text("Cooking recipes")
            .with_name("cooking");

        search.store(&doc1).await.unwrap();
        search.store(&doc2).await.unwrap();
        search.store(&doc3).await.unwrap();

        let similar = search.find_similar(&doc1.suid, 10).await.unwrap();
        assert_eq!(similar.len(), 2); // Should find doc2 and doc3
    }

    #[tokio::test]
    async fn test_tier_filtering() {
        let search = create_test_search().await;

        // Create objects with different tiers
        // Note: We're creating new objects directly in the store to ensure tiers are set
        let open = SemanticObject::from_text("public info");
        let guarded = SemanticObject::from_text("private info");
        let sealed = SemanticObject::from_text("secret info");

        // Store objects first
        {
            let store = search.store.write().await;

            // Create with explicit tiers by modifying before insert
            let mut open_obj = open.clone();
            open_obj.security_tier = SecurityTier::Open;
            store.create(&open_obj).unwrap();
            store.store_embedding(&open_obj.suid, &[0.1; 384], "mock").unwrap();

            let mut guarded_obj = guarded.clone();
            guarded_obj.security_tier = SecurityTier::Guarded;
            store.create(&guarded_obj).unwrap();
            store.store_embedding(&guarded_obj.suid, &[0.2; 384], "mock").unwrap();

            let mut sealed_obj = sealed.clone();
            sealed_obj.security_tier = SecurityTier::Sealed;
            store.create(&sealed_obj).unwrap();
            store.store_embedding(&sealed_obj.suid, &[0.3; 384], "mock").unwrap();
        }

        // Search with Open tier should only find Open
        let results = search.search("info", SearchOptions {
            max_tier: SecurityTier::Open,
            include_keyword: false,
            min_score: 0.0,
            ..Default::default()
        }).await.unwrap();
        assert_eq!(results.len(), 1);

        // Search with Guarded tier should find Open and Guarded
        let results = search.search("info", SearchOptions {
            max_tier: SecurityTier::Guarded,
            include_keyword: false,
            min_score: 0.0,
            ..Default::default()
        }).await.unwrap();
        assert_eq!(results.len(), 2);

        // Search with Sealed tier should find all
        let results = search.search("info", SearchOptions {
            max_tier: SecurityTier::Sealed,
            include_keyword: false,
            min_score: 0.0,
            ..Default::default()
        }).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_search_for_llm() {
        let search = create_test_search().await;

        let open = SemanticObject::from_text("public document content")
            .with_name("public")
            .with_summary("A public document")
            .with_tier(SecurityTier::Open);
        let guarded = SemanticObject::from_text("private document content")
            .with_name("private")
            .with_summary("A private document")
            .with_tier(SecurityTier::Guarded);

        search.store(&open).await.unwrap();
        search.store(&guarded).await.unwrap();

        let results = search.search_for_llm("document", SecurityTier::Guarded, 10).await.unwrap();

        for result in results {
            match result {
                LlmSearchResult::Full { object, .. } => {
                    assert_eq!(object.security_tier, SecurityTier::Open);
                    assert!(object.content.is_some());
                }
                LlmSearchResult::Guarded { view, .. } => {
                    assert!(view.summary.is_some());
                    // GuardedView doesn't have content field
                }
            }
        }
    }

    #[tokio::test]
    async fn test_index_missing() {
        let search = create_test_search().await;

        // Directly create objects in store without indexing
        {
            let store = search.store.write().await;
            let obj = SemanticObject::from_text("unindexed content")
                .with_name("unindexed");
            store.create(&obj).unwrap();
        }

        // Index missing
        let indexed = search.index_missing().await.unwrap();
        assert_eq!(indexed, 1);

        // Second call should find nothing to index
        let indexed = search.index_missing().await.unwrap();
        assert_eq!(indexed, 0);
    }
}
