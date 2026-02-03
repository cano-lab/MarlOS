//! Object Store - Persistent storage for SemanticObjects
//!
//! Provides SQLite-backed storage for:
//! - SemanticObjects with full CRUD operations
//! - Relations between objects
//! - Vector embeddings for semantic search
//!
//! Key design: Objects are the primary storage unit, files only exist at boundaries.

use std::path::PathBuf;
use std::sync::Mutex;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params, OptionalExtension};
use thiserror::Error;

use crate::memory::SecurityTier;
use crate::semantic_object::{
    ContentType, RelationType, Relation, RelationEdge, SemanticObject, Suid,
};

#[allow(unused_imports)]
use crate::tier_classifier::TierClassifier;

#[derive(Error, Debug)]
pub enum ObjectStoreError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Lock error")]
    Lock,
    #[error("Object not found: {0}")]
    NotFound(String),
    #[error("Invalid SUID: {0}")]
    InvalidSuid(String),
}

pub type Result<T> = std::result::Result<T, ObjectStoreError>;

/// Persistent store for SemanticObjects
pub struct ObjectStore {
    conn: Mutex<Connection>,
    #[allow(dead_code)]
    classifier: TierClassifier,
}

impl ObjectStore {
    /// Create a new object store at the given path
    pub fn new(db_path: PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&db_path)?;
        let store = Self {
            conn: Mutex::new(conn),
            classifier: TierClassifier::new(),
        };

        store.init_schema()?;
        Ok(store)
    }

    /// Create an in-memory object store (for testing)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
            classifier: TierClassifier::new(),
        };

        store.init_schema()?;
        Ok(store)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        // Main objects table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS objects (
                suid TEXT PRIMARY KEY,
                name TEXT,
                path TEXT,
                content BLOB,
                content_type TEXT NOT NULL,
                content_hash TEXT,
                size_bytes INTEGER NOT NULL,
                tags TEXT,
                summary TEXT,
                security_tier INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                modified_at TEXT NOT NULL,
                version INTEGER NOT NULL,
                metadata TEXT
            )",
            [],
        )?;

        // Relations table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS relations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_suid TEXT NOT NULL,
                target_suid TEXT NOT NULL,
                relation_type TEXT NOT NULL,
                metadata TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (source_suid) REFERENCES objects(suid) ON DELETE CASCADE,
                FOREIGN KEY (target_suid) REFERENCES objects(suid) ON DELETE CASCADE,
                UNIQUE(source_suid, target_suid, relation_type)
            )",
            [],
        )?;

        // Embeddings table (separate for performance - embeddings are large)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS embeddings (
                suid TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (suid) REFERENCES objects(suid) ON DELETE CASCADE
            )",
            [],
        )?;

        // Indices for common queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_objects_name ON objects(name)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_objects_path ON objects(path)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_objects_tier ON objects(security_tier)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_objects_modified ON objects(modified_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_suid)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_suid)",
            [],
        )?;

        Ok(())
    }

    // ========================================================================
    // CRUD Operations
    // ========================================================================

    /// Store a new object
    pub fn create(&self, object: &SemanticObject) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let content_type_json = serde_json::to_string(&object.content_type)?;
        let tags_json = serde_json::to_string(&object.tags)?;
        let metadata_json = serde_json::to_string(&object.metadata)?;

        conn.execute(
            "INSERT INTO objects (
                suid, name, path, content, content_type, content_hash, size_bytes,
                tags, summary, security_tier, created_at, modified_at, version, metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                object.suid.to_string(),
                object.name,
                object.path,
                object.content,
                content_type_json,
                object.content_hash,
                object.size_bytes as i64,
                tags_json,
                object.summary,
                object.security_tier.as_u8(),
                object.created_at.to_rfc3339(),
                object.modified_at.to_rfc3339(),
                object.version as i64,
                metadata_json,
            ],
        )?;

        // Store relations
        for relation in &object.relations {
            self.add_relation_internal(&conn, &object.suid, relation)?;
        }

        Ok(())
    }

    /// Get an object by SUID
    pub fn get(&self, suid: &Suid) -> Result<Option<SemanticObject>> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;
        self.get_internal(&conn, suid)
    }

    fn get_internal(&self, conn: &Connection, suid: &Suid) -> Result<Option<SemanticObject>> {
        let mut stmt = conn.prepare(
            "SELECT suid, name, path, content, content_type, content_hash, size_bytes,
                    tags, summary, security_tier, created_at, modified_at, version, metadata
             FROM objects WHERE suid = ?1"
        )?;

        let result = stmt.query_row(params![suid.to_string()], |row| {
            Ok(ObjectRow {
                suid: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                content: row.get(3)?,
                content_type: row.get(4)?,
                content_hash: row.get(5)?,
                size_bytes: row.get(6)?,
                tags: row.get(7)?,
                summary: row.get(8)?,
                security_tier: row.get(9)?,
                created_at: row.get(10)?,
                modified_at: row.get(11)?,
                version: row.get(12)?,
                metadata: row.get(13)?,
            })
        }).optional()?;

        match result {
            Some(row) => {
                let mut obj = self.row_to_object(row)?;
                // Load relations
                obj.relations = self.get_relations_internal(conn, suid)?;
                // Load embedding if exists
                obj.embedding = self.get_embedding_internal(conn, suid)?;
                Ok(Some(obj))
            }
            None => Ok(None),
        }
    }

    /// Update an existing object
    pub fn update(&self, object: &SemanticObject) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let content_type_json = serde_json::to_string(&object.content_type)?;
        let tags_json = serde_json::to_string(&object.tags)?;
        let metadata_json = serde_json::to_string(&object.metadata)?;

        let rows = conn.execute(
            "UPDATE objects SET
                name = ?2, path = ?3, content = ?4, content_type = ?5, content_hash = ?6,
                size_bytes = ?7, tags = ?8, summary = ?9, security_tier = ?10,
                modified_at = ?11, version = ?12, metadata = ?13
             WHERE suid = ?1",
            params![
                object.suid.to_string(),
                object.name,
                object.path,
                object.content,
                content_type_json,
                object.content_hash,
                object.size_bytes as i64,
                tags_json,
                object.summary,
                object.security_tier.as_u8(),
                object.modified_at.to_rfc3339(),
                object.version as i64,
                metadata_json,
            ],
        )?;

        if rows == 0 {
            return Err(ObjectStoreError::NotFound(object.suid.to_string()));
        }

        // Update relations - delete old and insert new
        conn.execute(
            "DELETE FROM relations WHERE source_suid = ?1",
            params![object.suid.to_string()],
        )?;

        for relation in &object.relations {
            self.add_relation_internal(&conn, &object.suid, relation)?;
        }

        Ok(())
    }

    /// Delete an object by SUID
    pub fn delete(&self, suid: &Suid) -> Result<bool> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        // Relations and embeddings are deleted via CASCADE
        let rows = conn.execute(
            "DELETE FROM objects WHERE suid = ?1",
            params![suid.to_string()],
        )?;

        Ok(rows > 0)
    }

    /// Check if an object exists
    pub fn exists(&self, suid: &Suid) -> Result<bool> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM objects WHERE suid = ?1",
            params![suid.to_string()],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    // ========================================================================
    // Query Operations
    // ========================================================================

    /// List all objects (with pagination)
    pub fn list(&self, limit: usize, offset: usize) -> Result<Vec<SemanticObject>> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let mut stmt = conn.prepare(
            "SELECT suid, name, path, content, content_type, content_hash, size_bytes,
                    tags, summary, security_tier, created_at, modified_at, version, metadata
             FROM objects
             ORDER BY modified_at DESC
             LIMIT ?1 OFFSET ?2"
        )?;

        let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
            Ok(ObjectRow {
                suid: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                content: row.get(3)?,
                content_type: row.get(4)?,
                content_hash: row.get(5)?,
                size_bytes: row.get(6)?,
                tags: row.get(7)?,
                summary: row.get(8)?,
                security_tier: row.get(9)?,
                created_at: row.get(10)?,
                modified_at: row.get(11)?,
                version: row.get(12)?,
                metadata: row.get(13)?,
            })
        })?;

        let mut objects = Vec::new();
        for row in rows {
            let row = row?;
            let suid = Suid::parse(&row.suid)
                .map_err(|_| ObjectStoreError::InvalidSuid(row.suid.clone()))?;
            let mut obj = self.row_to_object(row)?;
            obj.relations = self.get_relations_internal(&conn, &suid)?;
            objects.push(obj);
        }

        Ok(objects)
    }

    /// List objects containing a specific tag (more efficient than list + filter)
    pub fn list_by_tag(&self, tag: &str, limit: usize) -> Result<Vec<SemanticObject>> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        // Use SQL LIKE to filter by tag in the JSON array
        // Tags are stored as JSON array, e.g., '["kind:source", "user_tag:foo"]'
        let pattern = format!("%\"{}%", tag);

        let mut stmt = conn.prepare(
            "SELECT suid, name, path, content, content_type, content_hash, size_bytes,
                    tags, summary, security_tier, created_at, modified_at, version, metadata
             FROM objects
             WHERE tags LIKE ?1
             ORDER BY created_at DESC
             LIMIT ?2"
        )?;

        let rows = stmt.query_map(params![pattern, limit as i64], |row| {
            Ok(ObjectRow {
                suid: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                content: row.get(3)?,
                content_type: row.get(4)?,
                content_hash: row.get(5)?,
                size_bytes: row.get(6)?,
                tags: row.get(7)?,
                summary: row.get(8)?,
                security_tier: row.get(9)?,
                created_at: row.get(10)?,
                modified_at: row.get(11)?,
                version: row.get(12)?,
                metadata: row.get(13)?,
            })
        })?;

        let mut objects = Vec::new();
        for row in rows {
            let row = row?;
            let suid = Suid::parse(&row.suid)
                .map_err(|_| ObjectStoreError::InvalidSuid(row.suid.clone()))?;
            let mut obj = self.row_to_object(row)?;
            obj.relations = self.get_relations_internal(&conn, &suid)?;
            objects.push(obj);
        }

        Ok(objects)
    }

    /// Search objects by text (keyword search)
    pub fn search_text(&self, query: &str, limit: usize) -> Result<Vec<SemanticObject>> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let pattern = format!("%{}%", query);

        let mut stmt = conn.prepare(
            "SELECT suid, name, path, content, content_type, content_hash, size_bytes,
                    tags, summary, security_tier, created_at, modified_at, version, metadata
             FROM objects
             WHERE name LIKE ?1 OR summary LIKE ?1 OR tags LIKE ?1
             ORDER BY modified_at DESC
             LIMIT ?2"
        )?;

        let rows = stmt.query_map(params![pattern, limit as i64], |row| {
            Ok(ObjectRow {
                suid: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                content: row.get(3)?,
                content_type: row.get(4)?,
                content_hash: row.get(5)?,
                size_bytes: row.get(6)?,
                tags: row.get(7)?,
                summary: row.get(8)?,
                security_tier: row.get(9)?,
                created_at: row.get(10)?,
                modified_at: row.get(11)?,
                version: row.get(12)?,
                metadata: row.get(13)?,
            })
        })?;

        let mut objects = Vec::new();
        for row in rows {
            objects.push(self.row_to_object(row?)?);
        }

        Ok(objects)
    }

    /// Search by tags
    pub fn search_by_tags(&self, tags: &[String], limit: usize) -> Result<Vec<SemanticObject>> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        // Build query to match all tags
        let mut conditions = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        for tag in tags {
            conditions.push("tags LIKE ?");
            params.push(Box::new(format!("%\"{}\",%", tag)));
        }

        let query = format!(
            "SELECT suid, name, path, content, content_type, content_hash, size_bytes,
                    tags, summary, security_tier, created_at, modified_at, version, metadata
             FROM objects
             WHERE {}
             ORDER BY modified_at DESC
             LIMIT ?",
            conditions.join(" AND ")
        );

        params.push(Box::new(limit as i64));

        let mut stmt = conn.prepare(&query)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(ObjectRow {
                suid: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                content: row.get(3)?,
                content_type: row.get(4)?,
                content_hash: row.get(5)?,
                size_bytes: row.get(6)?,
                tags: row.get(7)?,
                summary: row.get(8)?,
                security_tier: row.get(9)?,
                created_at: row.get(10)?,
                modified_at: row.get(11)?,
                version: row.get(12)?,
                metadata: row.get(13)?,
            })
        })?;

        let mut objects = Vec::new();
        for row in rows {
            objects.push(self.row_to_object(row?)?);
        }

        Ok(objects)
    }

    /// Get objects by security tier
    pub fn get_by_tier(&self, max_tier: SecurityTier, limit: usize) -> Result<Vec<SemanticObject>> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let mut stmt = conn.prepare(
            "SELECT suid, name, path, content, content_type, content_hash, size_bytes,
                    tags, summary, security_tier, created_at, modified_at, version, metadata
             FROM objects
             WHERE security_tier <= ?1
             ORDER BY modified_at DESC
             LIMIT ?2"
        )?;

        let rows = stmt.query_map(params![max_tier.as_u8(), limit as i64], |row| {
            Ok(ObjectRow {
                suid: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                content: row.get(3)?,
                content_type: row.get(4)?,
                content_hash: row.get(5)?,
                size_bytes: row.get(6)?,
                tags: row.get(7)?,
                summary: row.get(8)?,
                security_tier: row.get(9)?,
                created_at: row.get(10)?,
                modified_at: row.get(11)?,
                version: row.get(12)?,
                metadata: row.get(13)?,
            })
        })?;

        let mut objects = Vec::new();
        for row in rows {
            objects.push(self.row_to_object(row?)?);
        }

        Ok(objects)
    }

    /// Count total objects
    pub fn count(&self) -> Result<usize> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM objects",
            [],
            |row| row.get(0),
        )?;

        Ok(count as usize)
    }

    // ========================================================================
    // Relations
    // ========================================================================

    /// Add a relation between objects
    pub fn add_relation(&self, source: &Suid, target: &Suid, relation_type: RelationType) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let relation = Relation::new(*target, relation_type);
        self.add_relation_internal(&conn, source, &relation)
    }

    fn add_relation_internal(&self, conn: &Connection, source: &Suid, relation: &Relation) -> Result<()> {
        let type_json = serde_json::to_string(&relation.relation_type)?;
        let metadata_json = relation.metadata.as_ref().map(|m| serde_json::to_string(m)).transpose()?;

        conn.execute(
            "INSERT OR REPLACE INTO relations (source_suid, target_suid, relation_type, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                source.to_string(),
                relation.target.to_string(),
                type_json,
                metadata_json,
                relation.created_at.to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    /// Remove a relation
    pub fn remove_relation(&self, source: &Suid, target: &Suid, relation_type: &RelationType) -> Result<bool> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let type_json = serde_json::to_string(relation_type)?;

        let rows = conn.execute(
            "DELETE FROM relations WHERE source_suid = ?1 AND target_suid = ?2 AND relation_type = ?3",
            params![source.to_string(), target.to_string(), type_json],
        )?;

        Ok(rows > 0)
    }

    /// Get outgoing relations from an object
    pub fn get_outgoing_relations(&self, source: &Suid) -> Result<Vec<RelationEdge>> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;
        self.get_outgoing_internal(&conn, source)
    }

    fn get_outgoing_internal(&self, conn: &Connection, source: &Suid) -> Result<Vec<RelationEdge>> {
        let mut stmt = conn.prepare(
            "SELECT source_suid, target_suid, relation_type, metadata, created_at
             FROM relations WHERE source_suid = ?1"
        )?;

        let rows = stmt.query_map(params![source.to_string()], |row| {
            Ok(RelationRow {
                source_suid: row.get(0)?,
                target_suid: row.get(1)?,
                relation_type: row.get(2)?,
                metadata: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut edges = Vec::new();
        for row in rows {
            edges.push(self.row_to_edge(row?)?);
        }

        Ok(edges)
    }

    /// Get incoming relations to an object
    pub fn get_incoming_relations(&self, target: &Suid) -> Result<Vec<RelationEdge>> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let mut stmt = conn.prepare(
            "SELECT source_suid, target_suid, relation_type, metadata, created_at
             FROM relations WHERE target_suid = ?1"
        )?;

        let rows = stmt.query_map(params![target.to_string()], |row| {
            Ok(RelationRow {
                source_suid: row.get(0)?,
                target_suid: row.get(1)?,
                relation_type: row.get(2)?,
                metadata: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut edges = Vec::new();
        for row in rows {
            edges.push(self.row_to_edge(row?)?);
        }

        Ok(edges)
    }

    fn get_relations_internal(&self, conn: &Connection, suid: &Suid) -> Result<Vec<Relation>> {
        let edges = self.get_outgoing_internal(conn, suid)?;
        Ok(edges.into_iter().map(|e| e.to_relation()).collect())
    }

    // ========================================================================
    // Embeddings
    // ========================================================================

    /// Store an embedding for an object
    pub fn store_embedding(&self, suid: &Suid, embedding: &[f32], model: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        // Convert f32 slice to bytes
        let bytes: Vec<u8> = embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        conn.execute(
            "INSERT OR REPLACE INTO embeddings (suid, embedding, model, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                suid.to_string(),
                bytes,
                model,
                Utc::now().to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    /// Get embedding for an object
    pub fn get_embedding(&self, suid: &Suid) -> Result<Option<Vec<f32>>> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;
        self.get_embedding_internal(&conn, suid)
    }

    fn get_embedding_internal(&self, conn: &Connection, suid: &Suid) -> Result<Option<Vec<f32>>> {
        let result: Option<Vec<u8>> = conn.query_row(
            "SELECT embedding FROM embeddings WHERE suid = ?1",
            params![suid.to_string()],
            |row| row.get(0),
        ).optional()?;

        match result {
            Some(bytes) => {
                // Convert bytes back to f32 slice
                let floats: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                Ok(Some(floats))
            }
            None => Ok(None),
        }
    }

    /// Get all objects with embeddings (for semantic search)
    pub fn get_objects_with_embeddings(&self, max_tier: SecurityTier) -> Result<Vec<(SemanticObject, Vec<f32>)>> {
        let conn = self.conn.lock().map_err(|_| ObjectStoreError::Lock)?;

        let mut stmt = conn.prepare(
            "SELECT o.suid, o.name, o.path, o.content, o.content_type, o.content_hash, o.size_bytes,
                    o.tags, o.summary, o.security_tier, o.created_at, o.modified_at, o.version, o.metadata,
                    e.embedding
             FROM objects o
             INNER JOIN embeddings e ON o.suid = e.suid
             WHERE o.security_tier <= ?1"
        )?;

        let rows = stmt.query_map(params![max_tier.as_u8()], |row| {
            Ok((
                ObjectRow {
                    suid: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    content: row.get(3)?,
                    content_type: row.get(4)?,
                    content_hash: row.get(5)?,
                    size_bytes: row.get(6)?,
                    tags: row.get(7)?,
                    summary: row.get(8)?,
                    security_tier: row.get(9)?,
                    created_at: row.get(10)?,
                    modified_at: row.get(11)?,
                    version: row.get(12)?,
                    metadata: row.get(13)?,
                },
                row.get::<_, Vec<u8>>(14)?,
            ))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (obj_row, embedding_bytes) = row?;
            let obj = self.row_to_object(obj_row)?;
            let embedding: Vec<f32> = embedding_bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            results.push((obj, embedding));
        }

        Ok(results)
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    fn row_to_object(&self, row: ObjectRow) -> Result<SemanticObject> {
        let suid = Suid::parse(&row.suid)
            .map_err(|_| ObjectStoreError::InvalidSuid(row.suid))?;

        let content_type: ContentType = serde_json::from_str(&row.content_type)?;
        let tags: Vec<String> = serde_json::from_str(&row.tags)?;
        let metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(&row.metadata)?;

        let created_at = DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let modified_at = DateTime::parse_from_rfc3339(&row.modified_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(SemanticObject {
            suid,
            name: row.name,
            path: row.path,
            content: row.content,
            content_type,
            content_hash: row.content_hash,
            size_bytes: row.size_bytes as usize,
            embedding: None, // Loaded separately
            tags,
            summary: row.summary,
            relations: Vec::new(), // Loaded separately
            security_tier: SecurityTier::from_u8(row.security_tier as u8),
            created_at,
            modified_at,
            version: row.version as u64,
            metadata,
        })
    }

    fn row_to_edge(&self, row: RelationRow) -> Result<RelationEdge> {
        let source = Suid::parse(&row.source_suid)
            .map_err(|_| ObjectStoreError::InvalidSuid(row.source_suid))?;
        let target = Suid::parse(&row.target_suid)
            .map_err(|_| ObjectStoreError::InvalidSuid(row.target_suid))?;

        let relation_type: RelationType = serde_json::from_str(&row.relation_type)?;
        let metadata: Option<serde_json::Value> = row.metadata
            .map(|s| serde_json::from_str(&s))
            .transpose()?;

        let created_at = DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(RelationEdge {
            source,
            target,
            relation_type,
            metadata,
            created_at,
        })
    }
}

// Row types for query results
struct ObjectRow {
    suid: String,
    name: Option<String>,
    path: Option<String>,
    content: Option<Vec<u8>>,
    content_type: String,
    content_hash: Option<String>,
    size_bytes: i64,
    tags: String,
    summary: Option<String>,
    security_tier: i64,
    created_at: String,
    modified_at: String,
    version: i64,
    metadata: String,
}

struct RelationRow {
    source_suid: String,
    target_suid: String,
    relation_type: String,
    metadata: Option<String>,
    created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get() {
        let store = ObjectStore::in_memory().unwrap();

        let obj = SemanticObject::from_text("Hello, world!")
            .with_name("greeting")
            .with_tier(SecurityTier::Open);

        store.create(&obj).unwrap();

        let retrieved = store.get(&obj.suid).unwrap().unwrap();
        assert_eq!(retrieved.name, Some("greeting".to_string()));
        assert_eq!(retrieved.content_as_str(), Some("Hello, world!"));
    }

    #[test]
    fn test_update() {
        let store = ObjectStore::in_memory().unwrap();

        let mut obj = SemanticObject::from_text("v1")
            .with_name("doc");

        store.create(&obj).unwrap();

        obj.update_content(b"v2".to_vec());
        store.update(&obj).unwrap();

        let retrieved = store.get(&obj.suid).unwrap().unwrap();
        assert_eq!(retrieved.content_as_str(), Some("v2"));
        assert_eq!(retrieved.version, 2);
    }

    #[test]
    fn test_delete() {
        let store = ObjectStore::in_memory().unwrap();

        let obj = SemanticObject::from_text("temp");
        store.create(&obj).unwrap();

        assert!(store.exists(&obj.suid).unwrap());
        assert!(store.delete(&obj.suid).unwrap());
        assert!(!store.exists(&obj.suid).unwrap());
    }

    #[test]
    fn test_list_and_count() {
        let store = ObjectStore::in_memory().unwrap();

        for i in 0..5 {
            let obj = SemanticObject::from_text(&format!("doc {}", i))
                .with_name(&format!("doc{}", i));
            store.create(&obj).unwrap();
        }

        assert_eq!(store.count().unwrap(), 5);

        let list = store.list(3, 0).unwrap();
        assert_eq!(list.len(), 3);

        let list2 = store.list(10, 3).unwrap();
        assert_eq!(list2.len(), 2);
    }

    #[test]
    fn test_search_text() {
        let store = ObjectStore::in_memory().unwrap();

        let obj1 = SemanticObject::from_text("hello")
            .with_name("greeting")
            .with_summary("A friendly greeting");
        let obj2 = SemanticObject::from_text("goodbye")
            .with_name("farewell");

        store.create(&obj1).unwrap();
        store.create(&obj2).unwrap();

        let results = store.search_text("greet", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, Some("greeting".to_string()));
    }

    #[test]
    fn test_relations() {
        let store = ObjectStore::in_memory().unwrap();

        let obj1 = SemanticObject::from_text("parent");
        let obj2 = SemanticObject::from_text("child");

        store.create(&obj1).unwrap();
        store.create(&obj2).unwrap();

        store.add_relation(&obj1.suid, &obj2.suid, RelationType::Contains).unwrap();

        let outgoing = store.get_outgoing_relations(&obj1.suid).unwrap();
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].target, obj2.suid);

        let incoming = store.get_incoming_relations(&obj2.suid).unwrap();
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].source, obj1.suid);
    }

    #[test]
    fn test_embeddings() {
        let store = ObjectStore::in_memory().unwrap();

        let obj = SemanticObject::from_text("test");
        store.create(&obj).unwrap();

        let embedding = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        store.store_embedding(&obj.suid, &embedding, "test-model").unwrap();

        let retrieved = store.get_embedding(&obj.suid).unwrap().unwrap();
        assert_eq!(retrieved.len(), 5);
        assert!((retrieved[0] - 0.1).abs() < 0.0001);
    }

    #[test]
    fn test_tier_filtering() {
        let store = ObjectStore::in_memory().unwrap();

        let open = SemanticObject::from_text("open").with_tier(SecurityTier::Open);
        let guarded = SemanticObject::from_text("guarded").with_tier(SecurityTier::Guarded);
        let sealed = SemanticObject::from_text("sealed").with_tier(SecurityTier::Sealed);

        store.create(&open).unwrap();
        store.create(&guarded).unwrap();
        store.create(&sealed).unwrap();

        let open_only = store.get_by_tier(SecurityTier::Open, 10).unwrap();
        assert_eq!(open_only.len(), 1);

        let up_to_guarded = store.get_by_tier(SecurityTier::Guarded, 10).unwrap();
        assert_eq!(up_to_guarded.len(), 2);

        let all = store.get_by_tier(SecurityTier::Sealed, 10).unwrap();
        assert_eq!(all.len(), 3);
    }
}
