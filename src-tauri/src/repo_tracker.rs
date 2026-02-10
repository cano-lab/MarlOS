//! Repository Tracker - Manages watched repositories for auto-import
//!
//! Tracks which repositories have been imported to the vector database
//! and supports detecting changes for re-import.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use chrono::{DateTime, Utc};

/// Configuration for a tracked repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedRepo {
    /// Unique ID for this repo
    pub id: String,
    /// Path to the repository root
    pub path: String,
    /// Display name (defaults to folder name)
    pub name: String,
    /// When this repo was first imported
    pub first_imported: DateTime<Utc>,
    /// When this repo was last synced
    pub last_synced: DateTime<Utc>,
    /// Git commit hash at last sync (if git repo)
    pub last_commit: Option<String>,
    /// Number of files imported
    pub file_count: usize,
    /// Number of objects created
    pub object_count: usize,
    /// File extensions to include (empty = all supported)
    pub include_extensions: Vec<String>,
    /// Directories to exclude
    pub exclude_dirs: Vec<String>,
    /// Auto-sync mode
    pub auto_sync: AutoSyncMode,
    /// Is this repo currently being synced?
    #[serde(skip)]
    pub syncing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AutoSyncMode {
    /// Never auto-sync
    Manual,
    /// Sync on app startup
    OnStartup,
    /// Sync after detecting changes
    OnChange,
    /// Sync on a schedule
    Scheduled { interval_hours: u32 },
}

impl Default for AutoSyncMode {
    fn default() -> Self {
        Self::Manual
    }
}

/// Status of a repository's sync state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStatus {
    pub id: String,
    pub name: String,
    pub path: String,
    pub last_synced: DateTime<Utc>,
    pub is_git_repo: bool,
    pub current_commit: Option<String>,
    pub last_synced_commit: Option<String>,
    pub has_changes: bool,
    pub files_changed: usize,
    pub auto_sync: AutoSyncMode,
}

/// Persistent storage for tracked repositories
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RepoTrackerData {
    repos: HashMap<String, TrackedRepo>,
}

pub struct RepoTracker {
    data: std::sync::RwLock<RepoTrackerData>,
    storage_path: PathBuf,
}

impl RepoTracker {
    pub fn new() -> Self {
        let storage_path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("marlos")
            .join("repos.json");

        let data = if storage_path.exists() {
            match fs::read_to_string(&storage_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => RepoTrackerData::default(),
            }
        } else {
            RepoTrackerData::default()
        };

        Self {
            data: std::sync::RwLock::new(data),
            storage_path,
        }
    }

    fn save(&self) -> Result<(), String> {
        let data = self.data.read().map_err(|e| e.to_string())?;

        // Ensure directory exists
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let json = serde_json::to_string_pretty(&*data)
            .map_err(|e| e.to_string())?;
        fs::write(&self.storage_path, json)
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Register a new repository for tracking
    pub fn add_repo(&self, path: &str, name: Option<String>) -> Result<TrackedRepo, String> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(format!("Path does not exist: {}", path.display()));
        }

        let canonical = path.canonicalize()
            .map_err(|e| format!("Failed to resolve path: {}", e))?;
        let path_str = canonical.to_string_lossy().to_string();

        // Generate ID from path hash (simple hash function)
        let hash: u64 = path_str.bytes()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        let id = format!("repo_{:x}", hash);

        // Check if already tracked
        {
            let data = self.data.read().map_err(|e| e.to_string())?;
            if data.repos.contains_key(&id) {
                return Err(format!("Repository already tracked: {}", path_str));
            }
        }

        // Determine name
        let display_name = name.unwrap_or_else(|| {
            canonical.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        });

        // Check if it's a git repo and get current commit
        let last_commit = get_git_head(&canonical);

        let repo = TrackedRepo {
            id: id.clone(),
            path: path_str,
            name: display_name,
            first_imported: Utc::now(),
            last_synced: Utc::now(),
            last_commit,
            file_count: 0,
            object_count: 0,
            include_extensions: vec![],
            exclude_dirs: vec![
                "node_modules".to_string(),
                ".git".to_string(),
                "target".to_string(),
                "dist".to_string(),
                "build".to_string(),
                "__pycache__".to_string(),
                ".venv".to_string(),
                "venv".to_string(),
            ],
            auto_sync: AutoSyncMode::Manual,
            syncing: false,
        };

        {
            let mut data = self.data.write().map_err(|e| e.to_string())?;
            data.repos.insert(id, repo.clone());
        }

        self.save()?;
        Ok(repo)
    }

    /// Remove a repository from tracking
    pub fn remove_repo(&self, id: &str) -> Result<(), String> {
        let mut data = self.data.write().map_err(|e| e.to_string())?;
        data.repos.remove(id)
            .ok_or_else(|| format!("Repository not found: {}", id))?;
        drop(data);
        self.save()
    }

    /// Get all tracked repositories
    pub fn list_repos(&self) -> Vec<TrackedRepo> {
        self.data.read()
            .map(|data| data.repos.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get a specific repository
    pub fn get_repo(&self, id: &str) -> Option<TrackedRepo> {
        self.data.read().ok()
            .and_then(|data| data.repos.get(id).cloned())
    }

    /// Check if a path is already tracked
    pub fn is_tracked(&self, path: &str) -> Option<TrackedRepo> {
        let path = Path::new(path);
        let canonical = path.canonicalize().ok()?;
        let path_str = canonical.to_string_lossy().to_string();

        self.data.read().ok()
            .and_then(|data| {
                data.repos.values()
                    .find(|r| r.path == path_str)
                    .cloned()
            })
    }

    /// Get the status of a repository (check for changes)
    pub fn get_repo_status(&self, id: &str) -> Result<RepoStatus, String> {
        let repo = self.get_repo(id)
            .ok_or_else(|| format!("Repository not found: {}", id))?;

        let path = Path::new(&repo.path);
        let is_git_repo = path.join(".git").exists();
        let current_commit = get_git_head(path);

        let has_changes = if is_git_repo {
            // Compare commits
            current_commit.as_ref() != repo.last_commit.as_ref()
        } else {
            // For non-git repos, check if any files modified since last sync
            check_files_modified(path, repo.last_synced)
        };

        let files_changed = if has_changes && is_git_repo {
            count_changed_files(path, repo.last_commit.as_deref())
        } else {
            0
        };

        Ok(RepoStatus {
            id: repo.id,
            name: repo.name,
            path: repo.path,
            last_synced: repo.last_synced,
            is_git_repo,
            current_commit,
            last_synced_commit: repo.last_commit,
            has_changes,
            files_changed,
            auto_sync: repo.auto_sync,
        })
    }

    /// Update repository after a successful sync
    pub fn update_after_sync(
        &self,
        id: &str,
        file_count: usize,
        object_count: usize,
    ) -> Result<(), String> {
        let mut data = self.data.write().map_err(|e| e.to_string())?;

        let repo = data.repos.get_mut(id)
            .ok_or_else(|| format!("Repository not found: {}", id))?;

        repo.last_synced = Utc::now();
        repo.file_count = file_count;
        repo.object_count = object_count;
        repo.last_commit = get_git_head(Path::new(&repo.path));
        repo.syncing = false;

        drop(data);
        self.save()
    }

    /// Set auto-sync mode for a repository
    pub fn set_auto_sync(&self, id: &str, mode: AutoSyncMode) -> Result<(), String> {
        let mut data = self.data.write().map_err(|e| e.to_string())?;

        let repo = data.repos.get_mut(id)
            .ok_or_else(|| format!("Repository not found: {}", id))?;

        repo.auto_sync = mode;
        drop(data);
        self.save()
    }

    /// Get all repositories that need syncing
    pub fn get_repos_needing_sync(&self) -> Vec<TrackedRepo> {
        let data = match self.data.read() {
            Ok(d) => d,
            Err(_) => return vec![],
        };

        data.repos.values()
            .filter(|repo| {
                match &repo.auto_sync {
                    AutoSyncMode::Manual => false,
                    AutoSyncMode::OnStartup => true,
                    AutoSyncMode::OnChange => {
                        // Check if there are changes
                        let path = Path::new(&repo.path);
                        if path.join(".git").exists() {
                            get_git_head(path) != repo.last_commit
                        } else {
                            check_files_modified(path, repo.last_synced)
                        }
                    }
                    AutoSyncMode::Scheduled { interval_hours } => {
                        let hours_since_sync = (Utc::now() - repo.last_synced).num_hours();
                        hours_since_sync >= *interval_hours as i64
                    }
                }
            })
            .cloned()
            .collect()
    }
}

/// Get the current git HEAD commit hash
fn get_git_head(repo_path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Check if any files in the directory were modified since the given time
fn check_files_modified(path: &Path, since: DateTime<Utc>) -> bool {
    let since_systime = std::time::SystemTime::UNIX_EPOCH
        + std::time::Duration::from_secs(since.timestamp() as u64);

    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .any(|e| {
            e.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|mtime| mtime > since_systime)
                .unwrap_or(false)
        })
}

/// Count files changed since a given commit
fn count_changed_files(repo_path: &Path, since_commit: Option<&str>) -> usize {
    let args = match since_commit {
        Some(commit) => vec!["diff", "--name-only", commit, "HEAD"],
        None => vec!["diff", "--name-only", "HEAD"],
    };

    Command::new("git")
        .args(&args)
        .current_dir(repo_path)
        .output()
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .count()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_sync_mode_default() {
        assert_eq!(AutoSyncMode::default(), AutoSyncMode::Manual);
    }
}
