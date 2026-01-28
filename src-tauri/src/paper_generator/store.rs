//! Paper Storage
//!
//! Persistence for papers using the SemanticObject system.
//! Papers are stored with tags for filtering and retrieval.

use std::sync::Arc;

use crate::semantic_search::{SemanticSearch, SearchOptions};
use crate::semantic_object::{SemanticObject, ContentType};
use crate::memory::SecurityTier;

use super::{Paper, PaperView};

// ============================================================================
// Paper Store
// ============================================================================

/// Storage layer for papers using SemanticObject backend
pub struct PaperStore {
    search: Arc<SemanticSearch>,
}

impl PaperStore {
    pub fn new(search: Arc<SemanticSearch>) -> Self {
        Self { search }
    }

    /// Save a paper to storage
    pub async fn save(&self, paper: &Paper) -> Result<(), String> {
        // Serialize paper to JSON
        let json = serde_json::to_string_pretty(paper)
            .map_err(|e| format!("Failed to serialize paper: {}", e))?;

        // Create semantic object
        let obj = SemanticObject::new(json.as_bytes().to_vec(), ContentType::Json)
            .with_name(&paper.title)
            .with_tier(SecurityTier::Guarded)
            .with_tags(&[
                "kind:paper",
                &format!("paper_id:{}", paper.id),
                &format!("paper_status:{:?}", paper.status).to_lowercase(),
                &format!("paper_type:{:?}", paper.paper_type).to_lowercase(),
            ])
            .with_summary(&format!(
                "Research Paper: {} | Question: {} | Status: {:?} | Sources: {} | Sections: {}",
                paper.title,
                paper.research_question,
                paper.status,
                paper.source_ids.len(),
                paper.sections.len()
            ));

        // Check if paper already exists
        let existing = self.get_object_by_paper_id(&paper.id).await?;

        if let Some(existing_obj) = existing {
            // Update existing object
            let mut updated_obj = existing_obj;
            updated_obj.update_content(json.as_bytes().to_vec());
            updated_obj.name = Some(paper.title.clone());
            updated_obj.tags = vec![
                "kind:paper".to_string(),
                format!("paper_id:{}", paper.id),
                format!("paper_status:{:?}", paper.status).to_lowercase(),
                format!("paper_type:{:?}", paper.paper_type).to_lowercase(),
            ];
            updated_obj.tags.extend(paper.tags.iter().cloned());
            updated_obj.summary = Some(format!(
                "Research Paper: {} | Question: {} | Status: {:?}",
                paper.title, paper.research_question, paper.status
            ));

            self.search.update(&updated_obj).await
                .map_err(|e| format!("Failed to update paper: {}", e))?;
        } else {
            // Create new object
            let mut obj = obj;
            obj.tags.extend(paper.tags.iter().cloned());

            self.search.store(&obj).await
                .map_err(|e| format!("Failed to save paper: {}", e))?;
        }

        Ok(())
    }

    /// Get a paper by ID
    pub async fn get(&self, paper_id: &str) -> Result<Option<Paper>, String> {
        let obj = self.get_object_by_paper_id(paper_id).await?;

        match obj {
            Some(obj) => {
                let content_str = obj.content_as_str()
                    .ok_or("Paper content is not valid UTF-8")?;
                let paper: Paper = serde_json::from_str(content_str)
                    .map_err(|e| format!("Failed to deserialize paper: {}", e))?;
                Ok(Some(paper))
            }
            None => Ok(None),
        }
    }

    /// List all papers
    pub async fn list(&self, limit: usize) -> Result<Vec<PaperView>, String> {
        let store = self.search.store.read().await;
        let all_objects = store.list(1000, 0)
            .map_err(|e| format!("Failed to list papers: {}", e))?;

        let mut papers = Vec::new();
        for obj in all_objects.iter()
            .filter(|obj| obj.tags.contains(&"kind:paper".to_string()))
        {
            if let Some(content_str) = obj.content_as_str() {
                if let Ok(paper) = serde_json::from_str::<Paper>(content_str) {
                    papers.push(PaperView::from(&paper));
                }
            }
        }

        // Sort by modified date (most recent first)
        papers.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

        // Apply limit
        papers.truncate(limit);

        Ok(papers)
    }

    /// Delete a paper
    pub async fn delete(&self, paper_id: &str) -> Result<bool, String> {
        let obj = self.get_object_by_paper_id(paper_id).await?;

        match obj {
            Some(obj) => {
                self.search.delete(&obj.suid).await
                    .map_err(|e| format!("Failed to delete paper: {}", e))?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Search papers by text
    pub async fn search(&self, query_text: &str, limit: usize) -> Result<Vec<PaperView>, String> {
        let options = SearchOptions {
            limit,
            ..Default::default()
        };

        let results = self.search.search(query_text, options).await
            .map_err(|e| format!("Failed to search papers: {}", e))?;

        let mut papers = Vec::new();
        for hit in results.into_iter()
            .filter(|hit| hit.object.tags.contains(&"kind:paper".to_string()))
        {
            if let Some(content_str) = hit.object.content_as_str() {
                if let Ok(paper) = serde_json::from_str::<Paper>(content_str) {
                    papers.push(PaperView::from(&paper));
                }
            }
        }

        Ok(papers)
    }

    /// Get papers by status
    pub async fn get_by_status(&self, status: super::PaperStatus, limit: usize) -> Result<Vec<PaperView>, String> {
        let status_tag = format!("paper_status:{:?}", status).to_lowercase();

        let store = self.search.store.read().await;
        let all_objects = store.list(1000, 0)
            .map_err(|e| format!("Failed to get papers by status: {}", e))?;

        let mut papers = Vec::new();
        for obj in all_objects.iter()
            .filter(|obj| obj.tags.contains(&"kind:paper".to_string()))
            .filter(|obj| obj.tags.contains(&status_tag))
        {
            if let Some(content_str) = obj.content_as_str() {
                if let Ok(paper) = serde_json::from_str::<Paper>(content_str) {
                    papers.push(PaperView::from(&paper));
                }
            }
        }

        papers.truncate(limit);
        Ok(papers)
    }

    /// Helper to get the semantic object for a paper by its paper_id
    async fn get_object_by_paper_id(&self, paper_id: &str) -> Result<Option<SemanticObject>, String> {
        let tag = format!("paper_id:{}", paper_id);

        let store = self.search.store.read().await;
        let all_objects = store.list(1000, 0)
            .map_err(|e| format!("Failed to find paper: {}", e))?;

        let result = all_objects.into_iter()
            .find(|obj| obj.tags.contains(&"kind:paper".to_string()) && obj.tags.contains(&tag));

        Ok(result)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    // Integration tests would go here, but require mocking SemanticSearch
}
