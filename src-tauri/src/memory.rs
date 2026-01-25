//! Memory Store - Semantic memory with SQLite backend
//!
//! Provides persistent storage for:
//! - Document content and metadata
//! - Events and activity history
//! - Embeddings for semantic search (future)

use std::path::PathBuf;
use std::sync::Mutex;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Lock error")]
    Lock,
}

pub type Result<T> = std::result::Result<T, MemoryError>;

/// Security tiers for data classification (3-tier model)
///
/// - Open: Full content visible to LLM (logs, metrics, config)
/// - Guarded: Summary + metadata only (user documents, activity, PII)
/// - Sealed: Completely hidden from LLM (credentials, API keys)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord)]
#[repr(u8)]
pub enum SecurityTier {
    /// Full content visible to LLM
    Open = 0,
    /// Summary + metadata only - content hidden
    Guarded = 1,
    /// Completely hidden from LLM
    Sealed = 2,
}

impl SecurityTier {
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => SecurityTier::Open,
            1 => SecurityTier::Guarded,
            _ => SecurityTier::Sealed,
        }
    }

    /// Check if agent with max_tier can access this tier at all
    pub fn can_access(&self, agent_max_tier: SecurityTier) -> bool {
        *self as u8 <= agent_max_tier as u8
    }

    /// Check if agent can see full content (only Open tier)
    pub fn can_see_content(&self, agent_max_tier: SecurityTier) -> bool {
        *self == SecurityTier::Open && self.can_access(agent_max_tier)
    }

    /// Check if agent can see summary (Open or Guarded)
    pub fn can_see_summary(&self, agent_max_tier: SecurityTier) -> bool {
        *self <= SecurityTier::Guarded && self.can_access(agent_max_tier)
    }
}

/// Types of memory entries
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Document,
    Event,
    Note,
    Command,
    Search,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryType::Document => "document",
            MemoryType::Event => "event",
            MemoryType::Note => "note",
            MemoryType::Command => "command",
            MemoryType::Search => "search",
        }
    }
}

/// A memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: i64,
    pub content: String,
    pub memory_type: MemoryType,
    pub security_tier: SecurityTier,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
    pub embedding: Option<Vec<f32>>,
    /// Summary for Guarded tier (shown instead of content)
    pub summary: Option<String>,
}

/// A redacted view of a memory entry (for Guarded tier)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardedMemoryView {
    pub id: i64,
    pub memory_type: MemoryType,
    pub security_tier: SecurityTier,
    pub created_at: DateTime<Utc>,
    pub size_bytes: usize,
    pub summary: Option<String>,
    pub metadata: serde_json::Value,
}

impl MemoryEntry {
    /// Convert to guarded view (hides content, shows summary)
    pub fn to_guarded_view(&self) -> GuardedMemoryView {
        GuardedMemoryView {
            id: self.id,
            memory_type: self.memory_type,
            security_tier: self.security_tier,
            created_at: self.created_at,
            size_bytes: self.content.len(),
            summary: self.summary.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

/// The memory store backed by SQLite (thread-safe)
pub struct MemoryStore {
    conn: Mutex<Connection>,
}

impl MemoryStore {
    /// Create a new memory store
    pub fn new() -> Result<Self> {
        let db_path = Self::get_db_path()?;
        log::info!("Opening memory store at: {:?}", db_path);

        let conn = Connection::open(&db_path)?;
        let store = Self { conn: Mutex::new(conn) };
        store.init_schema()?;
        store.migrate_schema()?;

        Ok(store)
    }

    /// Create an in-memory store (for testing)
    #[cfg(test)]
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn: Mutex::new(conn) };
        store.init_schema()?;
        Ok(store)
    }

    /// Get the database path
    fn get_db_path() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("MarlOS");

        std::fs::create_dir_all(&data_dir)?;
        Ok(data_dir.join("memory.db"))
    }

    /// Migrate existing schema to add new columns
    fn migrate_schema(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;

        // Check existing columns
        let mut stmt = conn.prepare("PRAGMA table_info(memories)")?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();

        // Add security_tier if missing (ignore error if column already exists due to race)
        if !columns.iter().any(|c| c == "security_tier") {
            log::info!("Migrating database: adding security_tier column");
            match conn.execute("ALTER TABLE memories ADD COLUMN security_tier INTEGER NOT NULL DEFAULT 0", []) {
                Ok(_) => {}
                Err(rusqlite::Error::SqliteFailure(_, Some(ref msg))) if msg.contains("duplicate column") => {
                    log::debug!("Security tier column already exists");
                }
                Err(e) => return Err(e.into()),
            }
        }

        // Add summary if missing (ignore error if column already exists due to race)
        if !columns.iter().any(|c| c == "summary") {
            log::info!("Migrating database: adding summary column");
            match conn.execute("ALTER TABLE memories ADD COLUMN summary TEXT", []) {
                Ok(_) => {}
                Err(rusqlite::Error::SqliteFailure(_, Some(ref msg))) if msg.contains("duplicate column") => {
                    log::debug!("Summary column already exists");
                }
                Err(e) => return Err(e.into()),
            }
        }

        // Create index if it doesn't exist
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_tier ON memories(security_tier)",
            [],
        )?;

        Ok(())
    }

    /// Initialize the database schema
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                memory_type TEXT NOT NULL,
                security_tier INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata TEXT DEFAULT '{}',
                embedding BLOB,
                summary TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(memory_type);
            CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);

            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                content,
                content='memories',
                content_rowid='id'
            );

            CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, content) VALUES('delete', old.id, old.content);
            END;

            CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, content) VALUES('delete', old.id, old.content);
                INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content);
            END;
            "#,
        )?;

        log::debug!("Memory schema initialized");
        Ok(())
    }

    /// Store a new memory entry
    pub fn store(
        &self,
        content: &str,
        memory_type: MemoryType,
        security_tier: SecurityTier,
        metadata: serde_json::Value,
    ) -> Result<i64> {
        self.store_with_summary(content, memory_type, security_tier, metadata, None)
    }

    /// Store a new memory entry with optional summary (for Guarded tier)
    pub fn store_with_summary(
        &self,
        content: &str,
        memory_type: MemoryType,
        security_tier: SecurityTier,
        metadata: serde_json::Value,
        summary: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO memories (content, memory_type, security_tier, created_at, updated_at, metadata, summary)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6)",
            params![content, memory_type.as_str(), security_tier.as_u8(), now, metadata.to_string(), summary],
        )?;

        let id = conn.last_insert_rowid();
        log::trace!("Stored memory {} (type: {:?}, tier: {:?})", id, memory_type, security_tier);

        Ok(id)
    }

    /// Store with Open tier (full LLM access)
    pub fn store_open(
        &self,
        content: &str,
        memory_type: MemoryType,
        metadata: serde_json::Value,
    ) -> Result<i64> {
        self.store(content, memory_type, SecurityTier::Open, metadata)
    }

    /// Store with Guarded tier (summary only for LLM)
    pub fn store_guarded(
        &self,
        content: &str,
        memory_type: MemoryType,
        metadata: serde_json::Value,
        summary: &str,
    ) -> Result<i64> {
        self.store_with_summary(content, memory_type, SecurityTier::Guarded, metadata, Some(summary))
    }

    /// Store with Sealed tier (hidden from LLM)
    pub fn store_sealed(
        &self,
        content: &str,
        memory_type: MemoryType,
        metadata: serde_json::Value,
    ) -> Result<i64> {
        self.store(content, memory_type, SecurityTier::Sealed, metadata)
    }

    /// Search memories by text (full-text search) with tier filtering
    /// Only returns entries at or below the agent's max tier
    pub fn search_with_tier(&self, query: &str, max_tier: SecurityTier, limit: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT m.id, m.content, m.memory_type, m.security_tier, m.created_at, m.updated_at, m.metadata, m.summary
            FROM memories m
            JOIN memories_fts fts ON m.id = fts.rowid
            WHERE memories_fts MATCH ?1 AND m.security_tier <= ?2
            ORDER BY rank
            LIMIT ?3
            "#,
        )?;

        let entries = stmt
            .query_map(params![query, max_tier.as_u8(), limit as i64], |row| {
                Self::row_to_entry(row)
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    /// Search memories (backward compatible - returns all tiers)
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        self.search_with_tier(query, SecurityTier::Sealed, limit)
    }

    /// Helper to convert a database row to MemoryEntry
    fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<MemoryEntry> {
        Ok(MemoryEntry {
            id: row.get(0)?,
            content: row.get(1)?,
            memory_type: match row.get::<_, String>(2)?.as_str() {
                "document" => MemoryType::Document,
                "event" => MemoryType::Event,
                "note" => MemoryType::Note,
                "command" => MemoryType::Command,
                "search" => MemoryType::Search,
                _ => MemoryType::Note,
            },
            security_tier: SecurityTier::from_u8(row.get(3)?),
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                .unwrap()
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                .unwrap()
                .with_timezone(&Utc),
            metadata: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
            summary: row.get::<_, Option<String>>(7)?,
            embedding: None,
        })
    }

    /// Get recent memories with tier filtering
    pub fn recent_with_tier(&self, memory_type: Option<MemoryType>, max_tier: SecurityTier, limit: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        let query = match memory_type {
            Some(t) => format!(
                "SELECT id, content, memory_type, security_tier, created_at, updated_at, metadata, summary
                 FROM memories WHERE memory_type = '{}' AND security_tier <= {} ORDER BY created_at DESC LIMIT {}",
                t.as_str(),
                max_tier.as_u8(),
                limit
            ),
            None => format!(
                "SELECT id, content, memory_type, security_tier, created_at, updated_at, metadata, summary
                 FROM memories WHERE security_tier <= {} ORDER BY created_at DESC LIMIT {}",
                max_tier.as_u8(),
                limit
            ),
        };

        let mut stmt = conn.prepare(&query)?;

        let entries = stmt
            .query_map([], |row| Self::row_to_entry(row))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    /// Get recent memories (backward compatible - returns all tiers)
    pub fn recent(&self, memory_type: Option<MemoryType>, limit: usize) -> Result<Vec<MemoryEntry>> {
        self.recent_with_tier(memory_type, SecurityTier::Sealed, limit)
    }

    /// Search for LLM context - returns full entries for Open, guarded views for Guarded, nothing for Sealed
    pub fn search_for_llm(&self, query: &str, max_tier: SecurityTier, limit: usize) -> Result<Vec<serde_json::Value>> {
        let entries = self.search_with_tier(query, max_tier, limit)?;

        let results: Vec<serde_json::Value> = entries
            .into_iter()
            .filter_map(|entry| {
                match entry.security_tier {
                    SecurityTier::Open => Some(serde_json::to_value(&entry).ok()?),
                    SecurityTier::Guarded => Some(serde_json::to_value(&entry.to_guarded_view()).ok()?),
                    SecurityTier::Sealed => None, // Should not happen due to filter, but be safe
                }
            })
            .collect();

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_and_search() {
        let store = MemoryStore::new_in_memory().unwrap();

        store
            .store_open("Hello world test content", MemoryType::Note, serde_json::json!({}))
            .unwrap();

        let results = store.search("hello", 10).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_tier_filtering() {
        let store = MemoryStore::new_in_memory().unwrap();

        // Store entries at different tiers
        store.store("open log entry", MemoryType::Event, SecurityTier::Open, serde_json::json!({})).unwrap();
        store.store_guarded("user document content", MemoryType::Document, serde_json::json!({}), "A user document").unwrap();
        store.store_sealed("secret API key: sk-12345", MemoryType::Note, serde_json::json!({})).unwrap();

        // Agent with Guarded tier should see Open and Guarded, not Sealed
        let results = store.search_with_tier("log OR document OR key", SecurityTier::Guarded, 10).unwrap();
        assert_eq!(results.len(), 2, "Should see Open and Guarded entries");
        for entry in &results {
            assert!(entry.security_tier <= SecurityTier::Guarded, "Tier filtering failed");
        }

        // Agent with Open tier should only see Open
        let results = store.search_with_tier("log OR document OR key", SecurityTier::Open, 10).unwrap();
        assert_eq!(results.len(), 1, "Should only see Open entries");
        assert_eq!(results[0].security_tier, SecurityTier::Open);
    }

    #[test]
    fn test_guarded_view() {
        let store = MemoryStore::new_in_memory().unwrap();

        // Store a guarded entry with summary
        store.store_guarded(
            "This is my private journal entry about my day...",
            MemoryType::Document,
            serde_json::json!({"path": "/notes/journal.md"}),
            "Personal journal entry"
        ).unwrap();

        let results = store.search("journal", 10).unwrap();
        assert_eq!(results.len(), 1);

        let entry = &results[0];
        assert_eq!(entry.security_tier, SecurityTier::Guarded);
        assert_eq!(entry.summary, Some("Personal journal entry".to_string()));

        // Convert to guarded view
        let view = entry.to_guarded_view();
        assert_eq!(view.summary, Some("Personal journal entry".to_string()));
        assert!(view.size_bytes > 0);
        // Note: view does not have content field
    }

    #[test]
    fn test_search_for_llm() {
        let store = MemoryStore::new_in_memory().unwrap();

        store.store_open("open metrics data", MemoryType::Event, serde_json::json!({})).unwrap();
        store.store_guarded("private user content", MemoryType::Document, serde_json::json!({}), "User document").unwrap();
        store.store_sealed("api_key=secret123", MemoryType::Note, serde_json::json!({})).unwrap();

        // Search for LLM with Guarded max tier
        let results = store.search_for_llm("metrics OR user OR api", SecurityTier::Guarded, 10).unwrap();

        // Should have 2 results (Open and Guarded)
        assert_eq!(results.len(), 2);

        // Open entry should have full content
        let open_result = results.iter().find(|r| r.get("content").is_some()).unwrap();
        assert!(open_result.get("content").unwrap().as_str().unwrap().contains("metrics"));

        // Guarded entry should have summary but no content
        let guarded_result = results.iter().find(|r| r.get("size_bytes").is_some()).unwrap();
        assert!(guarded_result.get("content").is_none());
        assert_eq!(guarded_result.get("summary").unwrap().as_str().unwrap(), "User document");
    }

    #[test]
    fn test_tier_ordering() {
        assert!(SecurityTier::Open < SecurityTier::Guarded);
        assert!(SecurityTier::Guarded < SecurityTier::Sealed);
        assert!(SecurityTier::Open.can_access(SecurityTier::Guarded));
        assert!(!SecurityTier::Sealed.can_access(SecurityTier::Guarded));
    }
}
