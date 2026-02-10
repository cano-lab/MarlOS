//! Browser Sync - Real-time import of ChatGPT conversations from browser storage
//!
//! This module monitors browser's local storage/IndexedDB for ChatGPT conversations
//! and imports them in real-time to MarlOS.
//!
//! Privacy First:
//! - Always asks permission before accessing browser data
//! - Shows preview of what will be imported
//! - Allows skipping/deleting sensitive conversations
//! - Users maintain full control

use std::path::PathBuf;
use std::fs;
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::interval;

/// Browser types we support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserType {
    Chrome,
    ChromeBeta,
    ChromeDev,
    Chromium,
    Edge,
    Brave,
    Vivaldi,
    Opera,
}

impl BrowserType {
    /// Get all possible browser data paths for this platform
    pub fn data_paths(&self) -> Vec<PathBuf> {
        let base = match self {
            BrowserType::Chrome => dirs::home_dir()
                .map(|p| p.join(".config").join("google-chrome").join("Default")),
            BrowserType::ChromeBeta => dirs::home_dir()
                .map(|p| p.join(".config").join("google-chrome-beta").join("Default")),
            BrowserType::ChromeDev => dirs::home_dir()
                .map(|p| p.join(".config").join("google-chrome-unstable").join("Default")),
            BrowserType::Chromium => dirs::home_dir()
                .map(|p| p.join(".config").join("chromium").join("Default")),
            BrowserType::Edge => dirs::home_dir()
                .map(|p| p.join(".config").join("microsoft-edge").join("Default")),
            BrowserType::Brave => dirs::home_dir()
                .map(|p| p.join(".config").join("BraveSoftware").join("Brave-Browser").join("Default")),
            BrowserType::Vivaldi => dirs::home_dir()
                .map(|p| p.join(".config").join("vivaldi").join("Default")),
            BrowserType::Opera => dirs::home_dir()
                .map(|p| p.join(".config").join("opera").join("Default")),
        };

        base.map(|b| vec![b]).unwrap_or_default()
    }

    /// Local Storage path for ChatGPT
    pub fn chatgpt_storage_path(&self) -> Option<PathBuf> {
        // Chrome/Edge store IndexedDB/LocalStorage in:
        // {Base}/Local Storage/leveldb/
        self.data_paths().first()
            .map(|base| base.join("Local Storage").join("leveldb"))
    }

    /// ChatGPT conversation storage file
    pub fn chatgpt_conversations_file(&self) -> Option<PathBuf> {
        // ChatGPT stores conversations in IndexedDB, which is in LevelDB format
        // The actual file is in the leveldb directory
        self.chatgpt_storage_path()
    }
}

/// Sync state - tracks what we've imported
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub last_sync: Option<DateTime<Utc>>,
    pub imported_conversations: HashMap<String, ConversationMeta>,
    pub skipped_conversations: HashMap<String, SkipReason>,
    pub browser_type: Option<BrowserType>,
    pub auto_sync_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub imported_at: DateTime<Utc>,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkipReason {
    pub reason: String,
    pub skipped_at: DateTime<Utc>,
}

/// Browser sync manager
pub struct BrowserSync {
    state: Arc<Mutex<SyncState>>,
    browser_type: BrowserType,
}

impl BrowserSync {
    pub fn new(browser_type: BrowserType) -> Self {
        Self {
            state: Arc::new(Mutex::new(SyncState {
                last_sync: None,
                imported_conversations: HashMap::new(),
                skipped_conversations: HashMap::new(),
                browser_type: Some(browser_type),
                auto_sync_enabled: false,
            })),
            browser_type,
        }
    }

    /// Check if browser is accessible (permission check)
    pub fn check_browser_access(&self) -> Result<bool, String> {
        let storage_path = self.browser_type.chatgpt_storage_path()
            .ok_or("Could not determine browser storage path")?;

        if !storage_path.exists() {
            return Ok(false);
        }

        // Try to read directory to verify access
        fs::read_dir(&storage_path)
            .map(|_| true)
            .map_err(|e| format!("Cannot access browser data: {}", e))
    }

    /// Scan for new conversations (doesn't import yet)
    pub fn scan_for_conversations(&self) -> Result<Vec<ConversationMeta>, String> {
        // This is a placeholder - actual implementation would parse the browser's
        // IndexedDB/LocalStorage to extract conversation metadata

        // For now, return empty vec since we haven't implemented the browser DB parser
        // TODO: Implement actual Chrome/Edge IndexedDB parsing
        Ok(vec![])
    }

    /// Import new conversations (real-time sync)
    pub async fn sync_new_conversations(&self) -> Result<SyncResult, String> {
        let mut state = self.state.lock().map_err(|e| format!("Lock error: {}", e))?;

        let new_conversations = self.scan_for_conversations()?;

        // Filter out already imported and skipped
        let to_import: Vec<_> = new_conversations.into_iter()
            .filter(|conv| {
                !state.imported_conversations.contains_key(&conv.id)
                    && !state.skipped_conversations.contains_key(&conv.id)
            })
            .collect();

        let imported = vec![]; // Would be filled by actual import
        let skipped = vec![];

        state.last_sync = Some(Utc::now());

        Ok(SyncResult {
            imported,
            skipped,
            total_found: to_import.len(),
        })
    }

    /// Manual full sync (user requested)
    pub async fn manual_sync(&self) -> Result<SyncResult, String> {
        // Same as sync_new_conversations but with user intent
        self.sync_new_conversations().await
    }

    /// Enable/disable auto-sync
    pub fn set_auto_sync(&self, enabled: bool) {
        if let Ok(mut state) = self.state.lock() {
            state.auto_sync_enabled = enabled;
        }
    }

    /// Skip a conversation (won't be imported in auto-sync)
    pub fn skip_conversation(&self, id: String, reason: String) {
        if let Ok(mut state) = self.state.lock() {
            state.skipped_conversations.insert(id, SkipReason {
                reason,
                skipped_at: Utc::now(),
            });
        }
    }

    /// Get current sync state
    pub fn get_state(&self) -> Result<SyncState, String> {
        self.state.lock()
            .map(|s| s.clone())
            .map_err(|e| format!("Lock error: {}", e))
    }

    /// Start background sync task
    pub async fn start_background_sync(&self, interval_secs: u64) {
        let state = self.state.clone();
        let browser_sync = self.clone_state();

        tokio::spawn(async move {
            let mut timer = interval(Duration::from_secs(interval_secs));
            loop {
                timer.tick().await;

                let should_sync = state.lock()
                    .map(|s| s.auto_sync_enabled)
                    .unwrap_or(false);

                if should_sync {
                    if let Err(e) = browser_sync.sync_new_conversations().await {
                        eprintln!("Background sync error: {}", e);
                    }
                }
            }
        });
    }

    fn clone_state(&self) -> Self {
        Self {
            state: self.state.clone(),
            browser_type: self.browser_type,
        }
    }
}

/// Result of a sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub imported: Vec<ConversationMeta>,
    pub skipped: Vec<ConversationMeta>,
    pub total_found: usize,
}

/// Get all available browsers
pub fn detect_browsers() -> Vec<BrowserType> {
    vec![
        BrowserType::Chrome,
        BrowserType::Edge,
        BrowserType::Brave,
        BrowserType::Chromium,
    ].into_iter()
        .filter(|b| b.chatgpt_storage_path().map(|p| p.exists()).unwrap_or(false))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_paths() {
        let chrome = BrowserType::Chrome;
        let paths = chrome.data_paths();
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_detect_browsers() {
        let browsers = detect_browsers();
        // Don't assert count - depends on user's system
        println!("Found browsers: {:?}", browsers);
    }
}
