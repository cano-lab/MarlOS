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

    /// Initialize the database schema
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                memory_type TEXT NOT NULL,
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
        metadata: serde_json::Value,
    ) -> Result<i64> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO memories (content, memory_type, created_at, updated_at, metadata)
             VALUES (?1, ?2, ?3, ?3, ?4)",
            params![content, memory_type.as_str(), now, metadata.to_string()],
        )?;

        let id = conn.last_insert_rowid();
        log::trace!("Stored memory {} (type: {:?})", id, memory_type);

        Ok(id)
    }

    /// Search memories by text (full-text search)
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT m.id, m.content, m.memory_type, m.created_at, m.updated_at, m.metadata
            FROM memories m
            JOIN memories_fts fts ON m.id = fts.rowid
            WHERE memories_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
            "#,
        )?;

        let entries = stmt
            .query_map(params![query, limit as i64], |row| {
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
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    metadata: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                    embedding: None,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    /// Get recent memories
    pub fn recent(&self, memory_type: Option<MemoryType>, limit: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|_| MemoryError::Lock)?;
        let query = match memory_type {
            Some(t) => format!(
                "SELECT id, content, memory_type, created_at, updated_at, metadata
                 FROM memories WHERE memory_type = '{}' ORDER BY created_at DESC LIMIT {}",
                t.as_str(),
                limit
            ),
            None => format!(
                "SELECT id, content, memory_type, created_at, updated_at, metadata
                 FROM memories ORDER BY created_at DESC LIMIT {}",
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
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    metadata: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                    embedding: None,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_and_search() {
        let store = MemoryStore::new().unwrap();

        store
            .store("Hello world test content", MemoryType::Note, serde_json::json!({}))
            .unwrap();

        let results = store.search("hello", 10).unwrap();
        assert!(!results.is_empty());
    }
}
