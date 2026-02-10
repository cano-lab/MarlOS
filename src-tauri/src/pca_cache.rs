//! PCA Cache - Caches PCA results for fast 3D space loading
//!
//! Smart caching that reuses PCA when only new items are added.
//! Only triggers full recomputation when items deleted or >20% new items.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;
use chrono::{DateTime, Utc};

use crate::embedding_analysis::PCAResult;

/// Cached PCA data with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PCACache {
    /// The cached PCA result
    pub pca: PCAResult,
    /// Number of objects when PCA was ORIGINALLY computed (baseline for 20% threshold)
    pub original_count: usize,
    /// Current known object count (updated as items are added)
    pub current_count: usize,
    /// Embedding dimensions
    pub dimensions: usize,
    /// When the cache was created
    pub computed_at: DateTime<Utc>,
    /// Version for cache invalidation
    pub version: u32,
}

/// Result of checking cache validity
#[derive(Debug)]
pub enum CacheStatus {
    /// Cache is exact match
    Valid(PCAResult),
    /// Cache is usable (only new items added, < threshold)
    UsableWithNewItems { pca: PCAResult, cached_count: usize, new_count: usize },
    /// Cache is stale and needs recomputation
    Stale { reason: String },
    /// No cache exists
    None,
}

const CACHE_VERSION: u32 = 3;  // Bumped for original_count tracking
/// Maximum percentage of new items before forcing recomputation
const MAX_NEW_ITEMS_PERCENT: f32 = 0.20;  // 20%

/// Manages PCA caching
pub struct PCACacheManager {
    cache: RwLock<Option<PCACache>>,
    cache_path: PathBuf,
}

impl PCACacheManager {
    /// Create a new cache manager
    pub fn new(app_data_dir: PathBuf) -> Self {
        let cache_path = app_data_dir.join("pca_cache.json");

        // Try to load existing cache
        let cache = Self::load_from_disk(&cache_path);

        Self {
            cache: RwLock::new(cache),
            cache_path,
        }
    }

    /// Load cache from disk
    fn load_from_disk(path: &PathBuf) -> Option<PCACache> {
        if !path.exists() {
            return None;
        }

        match std::fs::read_to_string(path) {
            Ok(contents) => {
                match serde_json::from_str::<PCACache>(&contents) {
                    Ok(cache) if cache.version == CACHE_VERSION => {
                        log::info!("Loaded PCA cache: {} original, {} current objects, computed at {}",
                            cache.original_count, cache.current_count, cache.computed_at);
                        Some(cache)
                    }
                    Ok(_) => {
                        log::info!("PCA cache version mismatch, will recompute");
                        None
                    }
                    Err(e) => {
                        log::warn!("Failed to parse PCA cache: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to read PCA cache: {}", e);
                None
            }
        }
    }

    /// Save cache to disk
    fn save_to_disk(&self, cache: &PCACache) -> Result<(), String> {
        let contents = serde_json::to_string_pretty(cache)
            .map_err(|e| format!("Failed to serialize PCA cache: {}", e))?;

        std::fs::write(&self.cache_path, contents)
            .map_err(|e| format!("Failed to write PCA cache: {}", e))?;

        log::info!("Saved PCA cache: {} original, {} current objects", cache.original_count, cache.current_count);
        Ok(())
    }

    /// Check cache status for current object count
    pub fn check(&self, current_object_count: usize) -> CacheStatus {
        let cache = match self.cache.read().ok() {
            Some(guard) => guard,
            None => return CacheStatus::None,
        };

        match cache.as_ref() {
            Some(c) if c.current_count == current_object_count => {
                // Exact match with current count
                log::info!("PCA cache exact match ({} objects)", c.current_count);
                CacheStatus::Valid(c.pca.clone())
            }
            Some(c) if current_object_count > c.current_count => {
                // Items were added - check against ORIGINAL count for threshold
                let total_new_since_pca = current_object_count - c.original_count;
                let percent_new = total_new_since_pca as f32 / c.original_count as f32;

                if percent_new <= MAX_NEW_ITEMS_PERCENT {
                    let new_since_last = current_object_count - c.current_count;
                    log::info!("PCA cache usable: {} new items since last check, {} total since PCA ({:.1}% of original {}, threshold {}%)",
                        new_since_last, total_new_since_pca, percent_new * 100.0, c.original_count, MAX_NEW_ITEMS_PERCENT * 100.0);
                    CacheStatus::UsableWithNewItems {
                        pca: c.pca.clone(),
                        cached_count: c.original_count,
                        new_count: total_new_since_pca,
                    }
                } else {
                    log::info!("PCA cache stale: {} new items since PCA ({:.1}% > {}% threshold)",
                        total_new_since_pca, percent_new * 100.0, MAX_NEW_ITEMS_PERCENT * 100.0);
                    CacheStatus::Stale {
                        reason: format!("{} new items since PCA ({:.1}% change exceeds {}% threshold)",
                            total_new_since_pca, percent_new * 100.0, MAX_NEW_ITEMS_PERCENT * 100.0)
                    }
                }
            }
            Some(c) => {
                // Items were deleted (current < cached current)
                let removed = c.current_count - current_object_count;
                log::info!("PCA cache stale: {} items removed", removed);
                CacheStatus::Stale {
                    reason: format!("{} items were removed, must recompute", removed)
                }
            }
            None => CacheStatus::None
        }
    }

    /// Get cached PCA if valid for current object count (legacy compatibility)
    pub fn get(&self, current_object_count: usize) -> Option<PCAResult> {
        match self.check(current_object_count) {
            CacheStatus::Valid(pca) => Some(pca),
            CacheStatus::UsableWithNewItems { pca, .. } => Some(pca),
            _ => None,
        }
    }

    /// Store new PCA result
    pub fn set(&self, pca: PCAResult, object_count: usize, dimensions: usize) {
        let cache = PCACache {
            pca,
            original_count: object_count,  // This is the baseline for 20% threshold
            current_count: object_count,   // Starts same as original
            dimensions,
            computed_at: Utc::now(),
            version: CACHE_VERSION,
        };

        // Save to memory
        if let Ok(mut guard) = self.cache.write() {
            *guard = Some(cache.clone());
        }

        // Save to disk (async-safe, non-blocking)
        if let Err(e) = self.save_to_disk(&cache) {
            log::error!("Failed to save PCA cache: {}", e);
        }
    }

    /// Update current count without recomputing PCA (for incremental additions)
    /// Note: original_count stays the same - it's the baseline for the 20% threshold
    pub fn update_count(&self, new_object_count: usize) {
        if let Ok(mut guard) = self.cache.write() {
            if let Some(ref mut cache) = *guard {
                log::info!("Updating PCA cache current count: {} -> {} (original: {})",
                    cache.current_count, new_object_count, cache.original_count);
                cache.current_count = new_object_count;
                // Save updated cache
                if let Err(e) = self.save_to_disk(cache) {
                    log::error!("Failed to save updated PCA cache: {}", e);
                }
            }
        }
    }

    /// Invalidate cache (call when objects change significantly)
    pub fn invalidate(&self) {
        if let Ok(mut guard) = self.cache.write() {
            *guard = None;
        }

        // Remove cache file
        if self.cache_path.exists() {
            let _ = std::fs::remove_file(&self.cache_path);
            log::info!("Invalidated PCA cache");
        }
    }

    /// Check if cache exists and is valid
    pub fn is_valid(&self, current_object_count: usize) -> bool {
        matches!(self.check(current_object_count), CacheStatus::Valid(_) | CacheStatus::UsableWithNewItems { .. })
    }

    /// Get cache info for display (original_count, current_count, computed_at)
    pub fn info(&self) -> Option<(usize, usize, DateTime<Utc>)> {
        self.cache.read()
            .ok()
            .and_then(|c| c.as_ref().map(|c| (c.original_count, c.current_count, c.computed_at)))
    }
}
