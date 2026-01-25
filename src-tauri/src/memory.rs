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

/// Security tiers for data classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum SecurityTier {
    /// Public - LLM has full access (logs, metrics, public config)
    Public = 0,
    /// Internal - LLM can read and summarize (app state, non-secret config)
    Internal = 1,
    /// Sensitive - LLM sees metadata only (user data, PII)
    Sensitive = 2,
    /// Secret - No LLM access (credentials, keys)
    Secret = 3,
}

impl SecurityTier {
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => SecurityTier::Public,
            1 => SecurityTier::Internal,
            2 => SecurityTier::Sensitive,
            _ => SecurityTier::Secret,
        }
    }

    /// Check if agent with max_tier can read this tier
    pub fn can_read(&self, agent_max_tier: SecurityTier) -> bool {
        *self as u8 <= agent_max_tier as u8
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

        // Check if security_tier column exists using pragma_table_info
        let mut stmt = conn.prepare("PRAGMA table_info(memories)")?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();

        let has_tier = columns.iter().any(|c| c == "security_tier");

        if !has_tier {
            log::info!("Migrating database: adding security_tier column");
            conn.execute(
                "ALTER TABLE memories ADD COLUMN security_tier INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        // Create index if it doesn't exist (safe to call multiple times)
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
                embedding BLOB
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
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO memories (content, memory_type, security_tier, created_at, updated_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
            params![content, memory_type.as_str(), security_tier.as_u8(), now, metadata.to_string()],
        )?;

        let id = conn.last_insert_rowid();
        log::trace!("Stored memory {} (type: {:?}, tier: {:?})", id, memory_type, security_tier);

        Ok(id)
    }

    /// Store with default tier (Public)
    pub fn store_public(
        &self,
        content: &str,
        memory_type: MemoryType,
        metadata: serde_json::Value,
    ) -> Result<i64> {
        self.store(content, memory_type, SecurityTier::Public, metadata)
    }

    /// Search memories by text (full-text search) with tier filtering
    /// Only returns entries at or below the agent's max tier
    pub fn search_with_tier(&self, query: &str, max_tier: SecurityTier, limit: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT m.id, m.content, m.memory_type, m.security_tier, m.created_at, m.updated_at, m.metadata
            FROM memories m
            JOIN memories_fts fts ON m.id = fts.rowid
            WHERE memories_fts MATCH ?1 AND m.security_tier <= ?2
            ORDER BY rank
            LIMIT ?3
            "#,
        )?;

        let entries = stmt
            .query_map(params![query, max_tier.as_u8(), limit as i64], |row| {
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
                    embedding: None,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    /// Search memories (backward compatible - returns all tiers)
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        self.search_with_tier(query, SecurityTier::Secret, limit)
    }

    /// Get recent memories with tier filtering
    pub fn recent_with_tier(&self, memory_type: Option<MemoryType>, max_tier: SecurityTier, limit: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        let query = match memory_type {
            Some(t) => format!(
                "SELECT id, content, memory_type, security_tier, created_at, updated_at, metadata
                 FROM memories WHERE memory_type = '{}' AND security_tier <= {} ORDER BY created_at DESC LIMIT {}",
                t.as_str(),
                max_tier.as_u8(),
                limit
            ),
            None => format!(
                "SELECT id, content, memory_type, security_tier, created_at, updated_at, metadata
                 FROM memories WHERE security_tier <= {} ORDER BY created_at DESC LIMIT {}",
                max_tier.as_u8(),
                limit
            ),
        };

        let mut stmt = conn.prepare(&query)?;

        let entries = stmt
            .query_map([], |row| {
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
                    embedding: None,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    /// Get recent memories (backward compatible - returns all tiers)
    pub fn recent(&self, memory_type: Option<MemoryType>, limit: usize) -> Result<Vec<MemoryEntry>> {
        self.recent_with_tier(memory_type, SecurityTier::Secret, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_and_search() {
        let store = MemoryStore::new_in_memory().unwrap();

        store
            .store_public("Hello world test content", MemoryType::Note, serde_json::json!({}))
            .unwrap();

        let results = store.search("hello", 10).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_tier_filtering() {
        let store = MemoryStore::new_in_memory().unwrap();

        // Store entries at different tiers (SIMULATED test data)
        store.store("public log entry", MemoryType::Event, SecurityTier::Public, serde_json::json!({})).unwrap();
        store.store("internal config", MemoryType::Note, SecurityTier::Internal, serde_json::json!({})).unwrap();
        store.store("sensitive user data", MemoryType::Document, SecurityTier::Sensitive, serde_json::json!({})).unwrap();
        store.store("secret API key", MemoryType::Note, SecurityTier::Secret, serde_json::json!({})).unwrap();

        // Agent with tier 1 should only see public and internal
        let results = store.search_with_tier("entry OR config OR data OR key", SecurityTier::Internal, 10).unwrap();
        for entry in &results {
            assert!(entry.security_tier <= SecurityTier::Internal, "Tier filtering failed");
        }
    }
}
