//! Research Project Organization
//!
//! Groups papers, references, notes, and documents into research projects.
//! Each project tracks its own sources, papers, and status.

use std::collections::HashMap;
use std::sync::Mutex;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A research project that groups related work
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchProject {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: ProjectStatus,
    /// Paper IDs associated with this project
    pub paper_ids: Vec<String>,
    /// Reference IDs (from reference library)
    pub reference_ids: Vec<String>,
    /// Document paths associated with this project
    pub document_paths: Vec<String>,
    /// Semantic object SUIDs linked to this project
    pub object_ids: Vec<String>,
    /// Tags for filtering
    pub tags: Vec<String>,
    /// Deadline (if any)
    pub deadline: Option<DateTime<Utc>>,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Paused,
    Completed,
    Archived,
}

impl Default for ProjectStatus {
    fn default() -> Self {
        ProjectStatus::Active
    }
}

impl ResearchProject {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            status: ProjectStatus::Active,
            paper_ids: Vec::new(),
            reference_ids: Vec::new(),
            document_paths: Vec::new(),
            object_ids: Vec::new(),
            tags: Vec::new(),
            deadline: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
        }
    }
}

/// Summary view for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: ProjectStatus,
    pub paper_count: usize,
    pub reference_count: usize,
    pub document_count: usize,
    pub tags: Vec<String>,
    pub deadline: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

impl From<&ResearchProject> for ProjectView {
    fn from(p: &ResearchProject) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            status: p.status,
            paper_count: p.paper_ids.len(),
            reference_count: p.reference_ids.len(),
            document_count: p.document_paths.len(),
            tags: p.tags.clone(),
            deadline: p.deadline,
            created_at: p.created_at,
            modified_at: p.modified_at,
        }
    }
}

/// In-memory project store (persisted via JSON file)
pub struct ProjectStore {
    projects: Mutex<Vec<ResearchProject>>,
    storage_path: std::path::PathBuf,
}

impl ProjectStore {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        let storage_path = data_dir.join("projects.json");
        let projects = if storage_path.exists() {
            match std::fs::read_to_string(&storage_path) {
                Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        Self {
            projects: Mutex::new(projects),
            storage_path,
        }
    }

    fn save(&self, projects: &[ResearchProject]) -> Result<(), String> {
        let json = serde_json::to_string_pretty(projects)
            .map_err(|e| format!("Failed to serialize projects: {}", e))?;
        std::fs::write(&self.storage_path, json)
            .map_err(|e| format!("Failed to write projects: {}", e))?;
        Ok(())
    }

    pub fn create(&self, name: &str, description: &str) -> Result<ResearchProject, String> {
        let project = ResearchProject::new(name, description);
        let mut projects = self.projects.lock().map_err(|e| e.to_string())?;
        projects.push(project.clone());
        self.save(&projects)?;
        Ok(project)
    }

    pub fn get(&self, id: &str) -> Result<Option<ResearchProject>, String> {
        let projects = self.projects.lock().map_err(|e| e.to_string())?;
        Ok(projects.iter().find(|p| p.id == id).cloned())
    }

    pub fn list(&self) -> Result<Vec<ProjectView>, String> {
        let projects = self.projects.lock().map_err(|e| e.to_string())?;
        Ok(projects.iter().map(ProjectView::from).collect())
    }

    pub fn update(&self, project: ResearchProject) -> Result<(), String> {
        let mut projects = self.projects.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = projects.iter_mut().find(|p| p.id == project.id) {
            *existing = project;
            self.save(&projects)?;
            Ok(())
        } else {
            Err(format!("Project not found: {}", project.id))
        }
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut projects = self.projects.lock().map_err(|e| e.to_string())?;
        let len_before = projects.len();
        projects.retain(|p| p.id != id);
        if projects.len() == len_before {
            return Err(format!("Project not found: {}", id));
        }
        self.save(&projects)?;
        Ok(())
    }

    pub fn add_paper(&self, project_id: &str, paper_id: &str) -> Result<(), String> {
        let mut projects = self.projects.lock().map_err(|e| e.to_string())?;
        let project = projects.iter_mut().find(|p| p.id == project_id)
            .ok_or_else(|| format!("Project not found: {}", project_id))?;
        if !project.paper_ids.contains(&paper_id.to_string()) {
            project.paper_ids.push(paper_id.to_string());
            project.modified_at = Utc::now();
        }
        self.save(&projects)?;
        Ok(())
    }

    pub fn add_reference(&self, project_id: &str, reference_id: &str) -> Result<(), String> {
        let mut projects = self.projects.lock().map_err(|e| e.to_string())?;
        let project = projects.iter_mut().find(|p| p.id == project_id)
            .ok_or_else(|| format!("Project not found: {}", project_id))?;
        if !project.reference_ids.contains(&reference_id.to_string()) {
            project.reference_ids.push(reference_id.to_string());
            project.modified_at = Utc::now();
        }
        self.save(&projects)?;
        Ok(())
    }

    pub fn add_document(&self, project_id: &str, path: &str) -> Result<(), String> {
        let mut projects = self.projects.lock().map_err(|e| e.to_string())?;
        let project = projects.iter_mut().find(|p| p.id == project_id)
            .ok_or_else(|| format!("Project not found: {}", project_id))?;
        if !project.document_paths.contains(&path.to_string()) {
            project.document_paths.push(path.to_string());
            project.modified_at = Utc::now();
        }
        self.save(&projects)?;
        Ok(())
    }

    pub fn set_status(&self, project_id: &str, status: ProjectStatus) -> Result<(), String> {
        let mut projects = self.projects.lock().map_err(|e| e.to_string())?;
        let project = projects.iter_mut().find(|p| p.id == project_id)
            .ok_or_else(|| format!("Project not found: {}", project_id))?;
        project.status = status;
        project.modified_at = Utc::now();
        self.save(&projects)?;
        Ok(())
    }
}
