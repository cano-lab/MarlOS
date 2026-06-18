//! Document version history for MarlOS
//!
//! Tracks content snapshots on save, allowing users to browse
//! and restore previous versions of their documents.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentVersion {
    pub id: String,
    pub document_path: String,
    pub content: String,
    pub word_count: usize,
    pub saved_at: DateTime<Utc>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionSummary {
    pub id: String,
    pub document_path: String,
    pub word_count: usize,
    pub saved_at: DateTime<Utc>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDiff {
    pub added_lines: usize,
    pub removed_lines: usize,
    pub changes: Vec<DiffChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffChunk {
    pub kind: String, // "add", "remove", "same"
    pub lines: Vec<String>,
}

pub struct VersionStore {
    conn: Mutex<Connection>,
    max_versions: usize,
}

impl VersionStore {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        let db_path = data_dir.join("versions.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open versions database: {}", e))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS versions (
                id TEXT PRIMARY KEY,
                document_path TEXT NOT NULL,
                content TEXT NOT NULL,
                word_count INTEGER NOT NULL DEFAULT 0,
                saved_at TEXT NOT NULL,
                label TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_versions_doc_path ON versions(document_path);
            CREATE INDEX IF NOT EXISTS idx_versions_saved_at ON versions(document_path, saved_at);"
        ).map_err(|e| format!("Failed to create versions table: {}", e))?;

        Ok(Self {
            conn: Mutex::new(conn),
            max_versions: 50,
        })
    }

    pub fn save_version(
        &self,
        document_path: &str,
        content: &str,
        label: Option<&str>,
    ) -> Result<DocumentVersion, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let word_count = content.split_whitespace().count();

        conn.execute(
            "INSERT INTO versions (id, document_path, content, word_count, saved_at, label) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, document_path, content, word_count, now.to_rfc3339(), label],
        ).map_err(|e| format!("Failed to save version: {}", e))?;

        // Prune old versions beyond max
        self.prune_versions(&conn, document_path)?;

        Ok(DocumentVersion {
            id,
            document_path: document_path.to_string(),
            content: content.to_string(),
            word_count,
            saved_at: now,
            label: label.map(|s| s.to_string()),
        })
    }

    pub fn list_versions(&self, document_path: &str) -> Result<Vec<VersionSummary>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT id, document_path, word_count, saved_at, label FROM versions WHERE document_path = ?1 ORDER BY saved_at DESC"
        ).map_err(|e| format!("Failed to prepare query: {}", e))?;

        let versions = stmt.query_map(params![document_path], |row| {
            let saved_at_str: String = row.get(3)?;
            Ok(VersionSummary {
                id: row.get(0)?,
                document_path: row.get(1)?,
                word_count: row.get::<_, i64>(2)? as usize,
                saved_at: DateTime::parse_from_rfc3339(&saved_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                label: row.get(4)?,
            })
        }).map_err(|e| format!("Failed to query versions: {}", e))?;

        versions.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect versions: {}", e))
    }

    pub fn get_version(&self, version_id: &str) -> Result<DocumentVersion, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id, document_path, content, word_count, saved_at, label FROM versions WHERE id = ?1",
            params![version_id],
            |row| {
                let saved_at_str: String = row.get(4)?;
                Ok(DocumentVersion {
                    id: row.get(0)?,
                    document_path: row.get(1)?,
                    content: row.get(2)?,
                    word_count: row.get::<_, i64>(3)? as usize,
                    saved_at: DateTime::parse_from_rfc3339(&saved_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    label: row.get(5)?,
                })
            },
        ).map_err(|e| format!("Failed to get version: {}", e))
    }

    pub fn diff_versions(&self, old_id: &str, new_id: &str) -> Result<VersionDiff, String> {
        let old = self.get_version(old_id)?;
        let new_ver = self.get_version(new_id)?;
        Ok(compute_diff(&old.content, &new_ver.content))
    }

    pub fn diff_with_current(&self, version_id: &str, current_content: &str) -> Result<VersionDiff, String> {
        let ver = self.get_version(version_id)?;
        Ok(compute_diff(&ver.content, current_content))
    }

    pub fn label_version(&self, version_id: &str, label: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE versions SET label = ?1 WHERE id = ?2",
            params![label, version_id],
        ).map_err(|e| format!("Failed to label version: {}", e))?;
        Ok(())
    }

    fn prune_versions(&self, conn: &Connection, document_path: &str) -> Result<(), String> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM versions WHERE document_path = ?1",
            params![document_path],
            |row| row.get(0),
        ).map_err(|e| format!("Failed to count versions: {}", e))?;

        if count as usize > self.max_versions {
            let excess = count as usize - self.max_versions;
            conn.execute(
                "DELETE FROM versions WHERE id IN (
                    SELECT id FROM versions WHERE document_path = ?1
                    ORDER BY saved_at ASC LIMIT ?2
                )",
                params![document_path, excess],
            ).map_err(|e| format!("Failed to prune versions: {}", e))?;
        }
        Ok(())
    }
}

fn compute_diff(old: &str, new: &str) -> VersionDiff {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut chunks = Vec::new();
    let mut added = 0usize;
    let mut removed = 0usize;

    // Simple LCS-based diff
    let (old_len, new_len) = (old_lines.len(), new_lines.len());

    // Build LCS table
    let mut dp = vec![vec![0u32; new_len + 1]; old_len + 1];
    for i in 1..=old_len {
        for j in 1..=new_len {
            dp[i][j] = if old_lines[i - 1] == new_lines[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }

    // Backtrack to produce diff
    let mut i = old_len;
    let mut j = new_len;
    let mut ops: Vec<(char, String)> = Vec::new();

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old_lines[i - 1] == new_lines[j - 1] {
            ops.push(('=', old_lines[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            ops.push(('+', new_lines[j - 1].to_string()));
            added += 1;
            j -= 1;
        } else if i > 0 {
            ops.push(('-', old_lines[i - 1].to_string()));
            removed += 1;
            i -= 1;
        }
    }

    ops.reverse();

    // Group consecutive operations into chunks
    let mut current_kind = None;
    let mut current_lines: Vec<String> = Vec::new();

    for (op, line) in ops {
        let kind = match op {
            '=' => "same",
            '+' => "add",
            '-' => "remove",
            _ => "same",
        };

        if Some(kind) != current_kind {
            if !current_lines.is_empty() {
                chunks.push(DiffChunk {
                    kind: current_kind.unwrap_or("same").to_string(),
                    lines: std::mem::take(&mut current_lines),
                });
            }
            current_kind = Some(kind);
        }
        current_lines.push(line);
    }

    if !current_lines.is_empty() {
        chunks.push(DiffChunk {
            kind: current_kind.unwrap_or("same").to_string(),
            lines: current_lines,
        });
    }

    VersionDiff {
        added_lines: added,
        removed_lines: removed,
        changes: chunks,
    }
}
