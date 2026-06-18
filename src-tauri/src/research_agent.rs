//! Autonomous Research Agent
//!
//! Continuously discovers, processes, and organizes research content
//! based on user-defined interests. Uses waveform similarity to group
//! related articles and uncover hidden connections.
//!
//! # Workflow
//!
//! ```text
//! Interests → Discovery → Fetch → Chunk → Cluster → Notify
//!    ↓          ↓          ↓       ↓        ↓         ↓
//!  Topics    Search    Articles  Chunks   Groups   Digest
//! ```
//!
//! # Usage
//!
//! ```rust
//! use research_agent::{AutonomousResearchAgent, Interest};
//!
//! let agent = AutonomousResearchAgent::new(ai_manager, search_engine);
//!
//! // Add interests to follow
//! agent.add_interest(Interest {
//!     topic: "machine learning interpretability".to_string(),
//!     queries: vec![
//!         "explainable AI",
//!         "model interpretability",
//!         "XAI techniques",
//!     ],
//!     sources: vec!["arxiv".to_string(), "scholar".to_string()],
//!     priority: 8,
//! }).await?;
//!
//! // Run a research sweep
//! let digest = agent.run_sweep().await?;
//!
//! // Get grouped articles
//! let groups = agent.get_article_groups().await?;
//! ```

use std::sync::Arc;
use std::collections::HashMap;
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::semantic_search::{SemanticSearch, SearchOptions};
use crate::semantic_object::{SemanticObject, Suid};
use crate::ai::AiManager;

/// A research interest to track
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interest {
    /// Unique ID
    pub id: String,
    /// Main topic name
    pub topic: String,
    /// Search queries for this interest
    pub queries: Vec<String>,
    /// Sources to search (arxiv, scholar, web, etc.)
    pub sources: Vec<String>,
    /// Priority (1-10) for ordering results
    pub priority: u8,
    /// How many articles to fetch per sweep
    pub max_articles: usize,
    /// When this interest was added
    pub created_at: DateTime<Utc>,
    /// Last time we searched for this interest
    pub last_sweep: Option<DateTime<Utc>>,
    /// Whether this interest is active
    pub active: bool,
}

impl Interest {
    pub fn new(topic: &str, queries: Vec<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            topic: topic.to_string(),
            queries,
            sources: vec!["arxiv".to_string(), "scholar".to_string()],
            priority: 5,
            max_articles: 10,
            created_at: Utc::now(),
            last_sweep: None,
            active: true,
        }
    }

    pub fn with_sources(mut self, sources: Vec<String>) -> Self {
        self.sources = sources;
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// A discovered and processed article
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    /// Unique ID
    pub id: String,
    /// Article title
    pub title: String,
    /// URL or source
    pub url: String,
    /// Main content (may be chunked)
    pub content: String,
    /// Extracted key findings
    pub findings: Vec<String>,
    /// Interest that led to this article
    pub interest_id: String,
    /// Relevance score to the interest
    pub relevance: f32,
    /// When discovered
    pub discovered_at: DateTime<Utc>,
    /// Source type (arxiv, scholar, web, etc.)
    pub source_type: String,
    /// Semantic object SUID if stored
    pub suid: Option<Suid>,
    /// Which cluster this article belongs to
    pub cluster_id: Option<String>,
}

/// A group of related articles discovered by waveform similarity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleCluster {
    /// Cluster ID
    pub id: String,
    /// Suggested theme/topic for this cluster
    pub theme: String,
    /// Articles in this cluster
    pub article_ids: Vec<String>,
    /// Average similarity within cluster
    pub cohesion: f32,
    /// Key findings across all articles
    pub key_findings: Vec<String>,
    /// When created
    pub created_at: DateTime<Utc>,
}

/// Result of a research sweep
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchDigest {
    /// When this sweep ran
    pub sweep_time: DateTime<Utc>,
    /// How many interests were searched
    pub interests_searched: usize,
    /// New articles discovered
    pub new_articles: usize,
    /// Articles clustered
    pub articles_clustered: usize,
    /// Number of clusters created
    pub clusters_created: usize,
    /// Summary of findings
    pub summary: String,
    /// Articles by interest
    pub articles_by_interest: HashMap<String, Vec<String>>,
}

/// Configuration for the research agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchAgentConfig {
    /// Minimum time between sweeps for the same interest (hours)
    pub sweep_interval_hours: i64,
    /// Minimum relevance to keep an article
    pub min_relevance: f32,
    /// Whether to auto-chunk long articles
    pub auto_chunk: bool,
    /// Max chunk size (tokens)
    pub max_chunk_size: usize,
    /// Minimum similarity for clustering
    pub cluster_min_similarity: f32,
    /// Whether to use waveform similarity for clustering
    pub use_waveform_clustering: bool,
}

impl Default for ResearchAgentConfig {
    fn default() -> Self {
        Self {
            sweep_interval_hours: 24,
            min_relevance: 0.6,
            auto_chunk: true,
            max_chunk_size: 1000,
            cluster_min_similarity: 0.75,
            use_waveform_clustering: true,
        }
    }
}

/// The autonomous research agent
pub struct AutonomousResearchAgent {
    ai: Arc<AiManager>,
    search: Arc<SemanticSearch>,
    interests: Arc<RwLock<Vec<Interest>>>,
    articles: Arc<RwLock<Vec<Article>>>,
    clusters: Arc<RwLock<Vec<ArticleCluster>>>,
    config: ResearchAgentConfig,
}

impl AutonomousResearchAgent {
    pub fn new(ai: Arc<AiManager>, search: Arc<SemanticSearch>) -> Self {
        Self {
            ai,
            search,
            interests: Arc::new(RwLock::new(Vec::new())),
            articles: Arc::new(RwLock::new(Vec::new())),
            clusters: Arc::new(RwLock::new(Vec::new())),
            config: ResearchAgentConfig::default(),
        }
    }

    pub fn with_config(mut self, config: ResearchAgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Add a research interest to track
    pub async fn add_interest(&self, interest: Interest) -> Result<(), String> {
        let mut interests = self.interests.write().await;
        interests.push(interest);
        Ok(())
    }

    /// Remove an interest
    pub async fn remove_interest(&self, interest_id: &str) -> Result<(), String> {
        let mut interests = self.interests.write().await;
        interests.retain(|i| i.id != interest_id);
        Ok(())
    }

    /// Get all active interests
    pub async fn get_interests(&self) -> Vec<Interest> {
        let interests = self.interests.read().await;
        interests.iter().filter(|i| i.active).cloned().collect()
    }

    /// Run a research sweep across all active interests
    pub async fn run_sweep(&self) -> Result<ResearchDigest, String> {
        let interests = self.get_interests().await;
        let sweep_time = Utc::now();
        let min_interval = Duration::hours(self.config.sweep_interval_hours);

        let mut interests_searched = 0;
        let mut new_articles = 0;
        let mut articles_by_interest = HashMap::new();

        // Find interests that need a sweep
        let interests_to_sweep: Vec<_> = interests.into_iter()
            .filter(|i| {
                i.last_sweep.map_or(true, |last| sweep_time - last > min_interval)
            })
            .collect();

        for interest in interests_to_sweep {
            log::info!("ResearchAgent: Sweeping interest: {}", interest.topic);

            // Search for articles using each query
            let mut found_for_interest = Vec::new();

            for query in &interest.queries {
                match self.discover_articles(query, &interest, 5).await {
                    Ok(mut articles) => {
                        found_for_interest.append(&mut articles);
                    }
                    Err(e) => {
                        log::warn!("ResearchAgent: Failed to discover articles for '{}': {}", query, e);
                    }
                }
            }

            // Deduplicate by URL
            found_for_interest.sort_by(|a, b| a.url.cmp(&b.url));
            found_for_interest.dedup_by(|a, b| a.url == b.url);

            // Take top articles by relevance
            found_for_interest.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());
            found_for_interest.truncate(interest.max_articles);

            // Store articles
            for article in found_for_interest {
                let article_id = self.store_article(article.clone()).await?;
                let article_ids = articles_by_interest
                    .entry(interest.id.clone())
                    .or_insert_with(Vec::new);
                article_ids.push(article_id.clone());
                new_articles += 1;
            }

            // Update last sweep time
            let mut interests = self.interests.write().await;
            if let Some(i) = interests.iter_mut().find(|i| i.id == interest.id) {
                i.last_sweep = Some(sweep_time);
            }
            interests_searched += 1;
        }

        // Cluster articles
        let clusters_created = self.cluster_articles().await?;
        let articles_clustered = self.get_article_count().await;

        // Generate summary
        let summary = self.generate_digest_summary(&articles_by_interest).await?;

        Ok(ResearchDigest {
            sweep_time,
            interests_searched,
            new_articles,
            articles_clustered,
            clusters_created,
            summary,
            articles_by_interest,
        })
    }

    /// Discover articles for a query
    async fn discover_articles(
        &self,
        query: &str,
        interest: &Interest,
        limit: usize,
    ) -> Result<Vec<Article>, String> {
        let mut articles = Vec::new();

        // Use academic search if available
        if interest.sources.contains(&"arxiv".to_string()) {
            match self.search_arxiv(query, limit).await {
                Ok(mut arxiv_articles) => {
                    for article in &mut arxiv_articles {
                        article.interest_id = interest.id.clone();
                        article.source_type = "arxiv".to_string();
                    }
                    articles.append(&mut arxiv_articles);
                }
                Err(e) => {
                    log::warn!("ResearchAgent: arXiv search failed: {}", e);
                }
            }
        }

        // Use web search for general sources
        if interest.sources.contains(&"web".to_string()) || articles.is_empty() {
            match self.search_web(query, limit).await {
                Ok(mut web_articles) => {
                    for article in &mut web_articles {
                        article.interest_id = interest.id.clone();
                        article.source_type = "web".to_string();
                    }
                    articles.append(&mut web_articles);
                }
                Err(e) => {
                    log::warn!("ResearchAgent: web search failed: {}", e);
                }
            }
        }

        // Filter by relevance
        articles.retain(|a| a.relevance >= self.config.min_relevance);

        Ok(articles)
    }

    /// Search academic sources (Semantic Scholar + arXiv) for papers
    async fn search_arxiv(&self, query: &str, limit: usize) -> Result<Vec<Article>, String> {
        log::info!("ResearchAgent: Searching academic sources for: {}", query);

        let results = crate::mcp::web_search::search_academic(query, limit).await?;

        let articles: Vec<Article> = results.papers.into_iter().map(|paper| {
            let content = paper.abstract_text.clone().unwrap_or_default();
            Article {
                id: uuid::Uuid::new_v4().to_string(),
                title: paper.title,
                url: paper.url,
                content,
                findings: Vec::new(),
                interest_id: String::new(), // filled by caller
                relevance: 0.5, // default, refined later
                discovered_at: Utc::now(),
                source_type: paper.source,
                suid: None,
                cluster_id: None,
            }
        }).collect();

        log::info!("ResearchAgent: Found {} academic articles", articles.len());
        Ok(articles)
    }

    /// Search the web for articles using DuckDuckGo
    async fn search_web(&self, query: &str, limit: usize) -> Result<Vec<Article>, String> {
        log::info!("ResearchAgent: Searching web for: {}", query);

        let results = crate::mcp::web_search::search(query, limit).await?;

        let articles: Vec<Article> = results.results.into_iter().map(|result| {
            Article {
                id: uuid::Uuid::new_v4().to_string(),
                title: result.title,
                url: result.url,
                content: result.snippet,
                findings: Vec::new(),
                interest_id: String::new(), // filled by caller
                relevance: 0.3, // web results get lower default relevance
                discovered_at: Utc::now(),
                source_type: format!("web:{}", result.source_domain),
                suid: None,
                cluster_id: None,
            }
        }).collect();

        log::info!("ResearchAgent: Found {} web articles", articles.len());
        Ok(articles)
    }

    /// Store an article as a semantic object
    async fn store_article(&self, mut article: Article) -> Result<String, String> {
        // Create semantic object
        let mut object = SemanticObject::from_text(&article.content)
            .with_name(&article.title)
            .with_tags(&[
                &format!("source:{}", article.source_type),
                "research",
                &format!("interest:{}", article.interest_id),
            ]);

        // Add findings as metadata
        object.metadata.insert("findings".to_string(), serde_json::to_value(&article.findings).unwrap());
        object.metadata.insert("url".to_string(), serde_json::json!(article.url));
        object.metadata.insert("relevance".to_string(), serde_json::json!(article.relevance));

        // Store it
        self.search.store(&object).await.map_err(|e| e.to_string())?;

        article.suid = Some(object.suid.clone());
        let id = object.suid.to_string();

        // Store in our articles list
        let mut articles = self.articles.write().await;
        articles.push(article);

        Ok(id)
    }

    /// Cluster related articles using waveform similarity
    async fn cluster_articles(&self) -> Result<usize, String> {
        // Collect unclustered article IDs first
        let unclustered_ids: Vec<String> = {
            let articles = self.articles.read().await;
            articles.iter()
                .filter(|a| a.cluster_id.is_none() && a.suid.is_some())
                .map(|a| a.id.clone())
                .collect()
        };

        if unclustered_ids.len() < 2 {
            return Ok(0);
        }

        // Get embeddings for all unclustered articles
        let store = self.search.store.read().await;
        let mut article_embeddings: Vec<(String, Vec<f32>)> = Vec::new();

        // Get the actual article SUIDs
        let article_suids: std::collections::HashMap<String, Suid> = {
            let articles = self.articles.read().await;
            articles.iter()
                .filter_map(|a| a.suid.as_ref().map(|s| (a.id.clone(), s.clone())))
                .collect()
        };

        drop(store);

        // Re-acquire store for embeddings
        let store = self.search.store.read().await;
        for id in &unclustered_ids {
            if let Some(suid) = article_suids.get(id) {
                if let Ok(Some(embedding)) = store.get_embedding(suid) {
                    article_embeddings.push((id.clone(), embedding));
                }
            }
        }

        drop(store);

        // Use waveform similarity to find clusters
        let mut clusters_created = 0;
        let mut assigned = std::collections::HashSet::new();

        for (i, (id_i, emb_i)) in article_embeddings.iter().enumerate() {
            if assigned.contains(id_i) {
                continue;
            }

            // Find similar articles using waveform
            let mut cluster_members = vec![id_i.clone()];
            assigned.insert(id_i.clone());

            for (id_j, emb_j) in article_embeddings.iter().skip(i + 1) {
                if assigned.contains(id_j) {
                    continue;
                }

                // Calculate similarity
                let similarity = if self.config.use_waveform_clustering {
                    use crate::waveform_similarity::hybrid_similarity;
                    let result = hybrid_similarity(emb_i, emb_j, None);
                    result.score
                } else {
                    use crate::embeddings::cosine_similarity;
                    cosine_similarity(emb_i, emb_j)
                };

                if similarity >= self.config.cluster_min_similarity {
                    cluster_members.push(id_j.clone());
                    assigned.insert(id_j.clone());
                }
            }

            // Only create cluster if we have multiple members
            if cluster_members.len() > 1 {
                let cluster_id = uuid::Uuid::new_v4().to_string();
                let theme = self.generate_cluster_theme(&cluster_members).await?;

                let mut clusters = self.clusters.write().await;
                clusters.push(ArticleCluster {
                    id: cluster_id.clone(),
                    theme,
                    article_ids: cluster_members.clone(),
                    cohesion: 0.0, // Would calculate
                    key_findings: vec![],
                    created_at: Utc::now(),
                });
                clusters_created += 1;

                // Update articles with cluster ID
                let mut articles = self.articles.write().await;
                for article in articles.iter_mut() {
                    if cluster_members.contains(&article.id) {
                        article.cluster_id = Some(cluster_id.clone());
                    }
                }
            }
        }

        Ok(clusters_created)
    }

    /// Generate a theme name for a cluster of articles
    async fn generate_cluster_theme(&self, article_ids: &[String]) -> Result<String, String> {
        let articles = self.articles.read().await;

        // Collect titles
        let titles: Vec<&str> = article_ids.iter()
            .filter_map(|id| articles.iter().find(|a| &a.id == id))
            .map(|a| a.title.as_str())
            .collect();

        if titles.is_empty() {
            return Ok("Unknown Theme".to_string());
        }

        // Use AI to generate theme (fallback to simple approach)
        let prompt = format!(
            "Given these article titles, suggest a concise 2-4 word theme:\n{}",
            titles.join("\n")
        );

        match self.ai.generate(&prompt, None).await {
            Ok(response) => Ok(response.content.trim().to_string()),
            Err(_) => {
                // Fallback: use most common words
                let mut word_counts = HashMap::new();
                for title in &titles {
                    for word in title.split_whitespace() {
                        if word.len() > 3 {
                            *word_counts.entry(word.to_lowercase()).or_insert(0) += 1;
                        }
                    }
                }
                word_counts.into_iter()
                    .max_by_key(|(_, c)| *c)
                    .map(|(word, _)| {
                        let mut s = word.to_string();
                        s.get(0..1).map(|first| first.to_uppercase() + &s[1..]).unwrap_or(s)
                    })
                    .ok_or_else(|| "No theme".to_string())
            }
        }
    }

    /// Generate a summary of the research digest
    async fn generate_digest_summary(&self, articles_by_interest: &HashMap<String, Vec<String>>) -> Result<String, String> {
        let interests = self.interests.read().await;

        if articles_by_interest.is_empty() {
            return Ok("No new articles discovered in this sweep.".to_string());
        }

        let mut parts = vec!["📚 Research Digest Summary".to_string()];

        for (interest_id, article_ids) in articles_by_interest {
            let interest = interests.iter().find(|i| i.id == *interest_id);
            let topic = interest.map(|i| i.topic.as_str()).unwrap_or("Unknown");
            parts.push(format!("\n🔬 {} - {} new articles", topic, article_ids.len()));
        }

        parts.push("\n💡 Articles have been clustered by similarity using waveform analysis.".to_string());

        Ok(parts.join("\n"))
    }

    /// Get all article clusters
    pub async fn get_clusters(&self) -> Vec<ArticleCluster> {
        let clusters = self.clusters.read().await;
        clusters.clone()
    }

    /// Get articles in a specific cluster
    pub async fn get_cluster_articles(&self, cluster_id: &str) -> Vec<Article> {
        let articles = self.articles.read().await;
        articles.iter()
            .filter(|a| a.cluster_id.as_deref() == Some(cluster_id))
            .cloned()
            .collect()
    }

    /// Get total article count
    async fn get_article_count(&self) -> usize {
        let articles = self.articles.read().await;
        articles.len()
    }

    /// Get articles by interest
    pub async fn get_articles_by_interest(&self, interest_id: &str) -> Vec<Article> {
        let articles = self.articles.read().await;
        articles.iter()
            .filter(|a| a.interest_id == interest_id)
            .cloned()
            .collect()
    }

    /// Generate interests from a reference library's reading patterns.
    /// Analyzes what the user has read, rated, and tagged to find topics they care about,
    /// then creates search queries and runs a sweep to find new related papers.
    pub async fn discover_from_preferences(
        &self,
        references: &[crate::reference_library::reference::Reference],
    ) -> Result<ResearchDigest, String> {
        if references.is_empty() {
            return Err("No references in library — add some papers first so I can learn your interests.".into());
        }

        log::info!("ResearchAgent: Mining {} references for preferences", references.len());

        // 1. Collect keyword frequencies (weighted by reading status + rating)
        let mut keyword_scores: HashMap<String, f32> = HashMap::new();
        let mut author_scores: HashMap<String, f32> = HashMap::new();
        let mut journal_scores: HashMap<String, f32> = HashMap::new();

        for reference in references {
            // Weight: read > reading > unread; higher rating = higher weight
            let status_weight = match reference.reading_status {
                crate::reference_library::reference::ReadingStatus::Read => 3.0,
                crate::reference_library::reference::ReadingStatus::Reading => 2.0,
                crate::reference_library::reference::ReadingStatus::Unread => 1.0,
                crate::reference_library::reference::ReadingStatus::Archived => 0.5,
            };
            let rating_weight = reference.rating.map(|r| r as f32 / 3.0).unwrap_or(1.0);
            let weight = status_weight * rating_weight;

            // Keywords
            for kw in &reference.keywords {
                let normalized = kw.to_lowercase();
                *keyword_scores.entry(normalized).or_insert(0.0) += weight;
            }

            // Tags (user-applied, strong signal)
            for tag in &reference.tags {
                let normalized = tag.to_lowercase();
                *keyword_scores.entry(normalized).or_insert(0.0) += weight * 1.5;
            }

            // Title words (extract meaningful terms)
            for word in reference.title.split_whitespace() {
                let w = word.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
                if w.len() > 3 && !is_stopword(&w) {
                    *keyword_scores.entry(w).or_insert(0.0) += weight * 0.3;
                }
            }

            // Authors (favorite authors)
            for author in &reference.authors {
                let last = author.split_whitespace().last().unwrap_or(author).to_lowercase();
                *author_scores.entry(last).or_insert(0.0) += weight;
            }

            // Journals (preferred venues)
            if let Some(journal) = &reference.journal {
                let j = journal.to_lowercase();
                *journal_scores.entry(j).or_insert(0.0) += weight;
            }
        }

        // 2. Rank and select top themes
        let mut top_keywords: Vec<(String, f32)> = keyword_scores.into_iter().collect();
        top_keywords.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_keywords.truncate(15);

        let mut top_authors: Vec<(String, f32)> = author_scores.into_iter().collect();
        top_authors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_authors.truncate(5);

        let mut top_journals: Vec<(String, f32)> = journal_scores.into_iter().collect();
        top_journals.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_journals.truncate(3);

        log::info!("ResearchAgent: Top keywords: {:?}", top_keywords.iter().map(|(k, _)| k).collect::<Vec<_>>());
        log::info!("ResearchAgent: Top authors: {:?}", top_authors.iter().map(|(a, _)| a).collect::<Vec<_>>());

        // 3. Group related keywords into interests (simple clustering by co-occurrence)
        let mut interests_to_add = Vec::new();

        // Group keywords into 3-5 interests by taking chunks of related terms
        let keyword_groups = group_keywords(&top_keywords, 3);

        for (i, group) in keyword_groups.iter().enumerate() {
            let topic = group.iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(k, _)| k.clone())
                .unwrap_or_else(|| format!("Research topic {}", i + 1));

            let queries: Vec<String> = group.iter().map(|(k, _)| k.clone()).collect();
            let mut interest = Interest::new(&topic, queries);
            interest.sources = vec!["arxiv".into(), "scholar".into(), "web".into()];
            interest.priority = (8 - i.min(4)) as u8; // Higher priority for top groups
            interest.max_articles = 5;
            interests_to_add.push(interest);
        }

        // Add a favorite-author interest if we have strong author preferences
        if let Some((top_author, score)) = top_authors.first() {
            if *score > 3.0 {
                let author_queries: Vec<String> = top_authors.iter()
                    .take(3)
                    .map(|(a, _)| format!("author:{}", a))
                    .collect();
                let mut interest = Interest::new(
                    &format!("Papers by {}", top_author),
                    author_queries,
                );
                interest.sources = vec!["arxiv".into(), "scholar".into()];
                interest.priority = 6;
                interest.max_articles = 5;
                interests_to_add.push(interest);
            }
        }

        // 4. Add the generated interests (clear old auto-generated ones first)
        {
            let mut interests = self.interests.write().await;
            // Remove previously auto-generated interests
            interests.retain(|i| !i.topic.starts_with("[auto]"));
            // Add new ones with [auto] prefix
            for mut interest in interests_to_add {
                interest.topic = format!("[auto] {}", interest.topic);
                interests.push(interest);
            }
        }

        // 5. Run the sweep
        self.run_sweep().await
    }

    /// Search articles using waveform similarity
    pub async fn find_similar_articles(&self, article_id: &str, limit: usize) -> Result<Vec<Article>, String> {
        let articles = self.articles.read().await;

        let target = articles.iter()
            .find(|a| a.id == article_id)
            .ok_or_else(|| "Article not found".to_string())?;

        let target_suid = target.suid.as_ref()
            .ok_or_else(|| "Article not stored".to_string())?;

        // Use waveform search to find similar
        let store = self.search.store.read().await;
        let target_embedding = store.get_embedding(target_suid)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "No embedding found".to_string())?;

        drop(store);

        // Get all other articles with embeddings
        let mut similar_articles = Vec::new();

        for article in articles.iter() {
            if article.id == article_id {
                continue;
            }

            if let Some(suid) = &article.suid {
                let store = self.search.store.read().await;
                if let Ok(Some(embedding)) = store.get_embedding(suid) {
                    use crate::waveform_similarity::hybrid_similarity;
                    let result = hybrid_similarity(&target_embedding, &embedding, None);
                    drop(store);

                    if result.score >= self.config.min_relevance {
                        similar_articles.push((article.clone(), result.score));
                    }
                }
            }
        }

        // Sort by similarity
        similar_articles.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        similar_articles.truncate(limit);

        Ok(similar_articles.into_iter().map(|(a, _)| a).collect())
    }
}

/// Check if a word is a common English stopword
fn is_stopword(word: &str) -> bool {
    matches!(word,
        "the" | "and" | "for" | "are" | "but" | "not" | "you" | "all" |
        "can" | "had" | "her" | "was" | "one" | "our" | "out" | "has" |
        "have" | "been" | "from" | "that" | "this" | "with" | "they" |
        "will" | "each" | "make" | "like" | "long" | "look" | "many" |
        "some" | "them" | "than" | "been" | "call" | "first" | "who" |
        "more" | "into" | "over" | "such" | "what" | "when" | "which" |
        "their" | "about" | "would" | "there" | "these" | "other" |
        "using" | "based" | "approach" | "method" | "paper" | "study" |
        "results" | "analysis" | "data" | "model" | "system" | "also" |
        "between" | "through" | "during" | "before" | "after"
    )
}

/// Group keywords into clusters of related terms
fn group_keywords(keywords: &[(String, f32)], max_groups: usize) -> Vec<Vec<(String, f32)>> {
    if keywords.is_empty() {
        return Vec::new();
    }

    let per_group = (keywords.len() / max_groups).max(2);
    keywords.chunks(per_group)
        .map(|chunk| chunk.to_vec())
        .take(max_groups)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interest_new() {
        let interest = Interest::new("machine learning", vec!["ML", "AI"]);
        assert_eq!(interest.topic, "machine learning");
        assert_eq!(interest.queries.len(), 2);
        assert!(interest.active);
    }

    #[test]
    fn test_interest_with_priority() {
        let interest = Interest::new("test", vec!["query"])
            .with_priority(8);
        assert_eq!(interest.priority, 8);
    }
}
