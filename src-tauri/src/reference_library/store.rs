//! SQLite-backed reference library storage.

use std::path::PathBuf;
use std::sync::Mutex;
use chrono::Utc;
use rusqlite::{Connection, params};

use super::reference::{Reference, ReadingStatus, ReferenceType};

pub struct ReferenceStore {
    conn: Mutex<Connection>,
}

impl ReferenceStore {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        let db_path = data_dir.join("references.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open references database: {}", e))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS refs (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                authors TEXT NOT NULL DEFAULT '[]',
                year INTEGER,
                doi TEXT,
                isbn TEXT,
                issn TEXT,
                url TEXT,
                journal TEXT,
                publisher TEXT,
                volume TEXT,
                issue TEXT,
                pages TEXT,
                edition TEXT,
                abstract_text TEXT,
                keywords TEXT NOT NULL DEFAULT '[]',
                notes TEXT,
                pdf_path TEXT,
                collections TEXT NOT NULL DEFAULT '[]',
                tags TEXT NOT NULL DEFAULT '[]',
                reading_status TEXT NOT NULL DEFAULT 'unread',
                rating INTEGER,
                cite_key TEXT NOT NULL,
                ref_type TEXT NOT NULL DEFAULT 'article',
                added_at TEXT NOT NULL,
                modified_at TEXT NOT NULL,
                suid TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_refs_doi ON refs(doi);
            CREATE INDEX IF NOT EXISTS idx_refs_title ON refs(title);
            CREATE INDEX IF NOT EXISTS idx_refs_cite_key ON refs(cite_key);
            CREATE INDEX IF NOT EXISTS idx_refs_reading_status ON refs(reading_status);",
        ).map_err(|e| format!("Failed to create references table: {}", e))?;

        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn add(&self, reference: &Reference) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let authors_json = serde_json::to_string(&reference.authors).unwrap_or_default();
        let keywords_json = serde_json::to_string(&reference.keywords).unwrap_or_default();
        let collections_json = serde_json::to_string(&reference.collections).unwrap_or_default();
        let tags_json = serde_json::to_string(&reference.tags).unwrap_or_default();

        conn.execute(
            "INSERT OR REPLACE INTO refs (id, title, authors, year, doi, isbn, issn, url,
             journal, publisher, volume, issue, pages, edition, abstract_text, keywords,
             notes, pdf_path, collections, tags, reading_status, rating, cite_key,
             ref_type, added_at, modified_at, suid)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                     ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)",
            params![
                reference.id,
                reference.title,
                authors_json,
                reference.year,
                reference.doi,
                reference.isbn,
                reference.issn,
                reference.url,
                reference.journal,
                reference.publisher,
                reference.volume,
                reference.issue,
                reference.pages,
                reference.edition,
                reference.abstract_text,
                keywords_json,
                reference.notes,
                reference.pdf_path,
                collections_json,
                tags_json,
                reference.reading_status.as_str(),
                reference.rating,
                reference.cite_key,
                reference.ref_type.as_str(),
                reference.added_at.to_rfc3339(),
                reference.modified_at.to_rfc3339(),
                reference.suid,
            ],
        ).map_err(|e| format!("Failed to add reference: {}", e))?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Reference, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        Self::query_one(&conn, "SELECT * FROM refs WHERE id = ?1", params![id])
    }

    pub fn get_by_doi(&self, doi: &str) -> Result<Option<Reference>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        match Self::query_one(&conn, "SELECT * FROM refs WHERE doi = ?1", params![doi]) {
            Ok(r) => Ok(Some(r)),
            Err(_) => Ok(None),
        }
    }

    pub fn get_by_cite_key(&self, key: &str) -> Result<Option<Reference>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        match Self::query_one(&conn, "SELECT * FROM refs WHERE cite_key = ?1", params![key]) {
            Ok(r) => Ok(Some(r)),
            Err(_) => Ok(None),
        }
    }

    pub fn list(&self, collection: Option<&str>, status: Option<&str>) -> Result<Vec<Reference>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        let query = match (collection, status) {
            (Some(c), Some(s)) => {
                let pattern = format!("%\"{}\"%" , c);
                let mut stmt = conn.prepare(
                    "SELECT * FROM refs WHERE collections LIKE ?1 AND reading_status = ?2 ORDER BY added_at DESC"
                ).map_err(|e| e.to_string())?;
                return Self::collect_rows(&mut stmt, params![pattern, s]);
            }
            (Some(c), None) => {
                let pattern = format!("%\"{}\"%" , c);
                let mut stmt = conn.prepare(
                    "SELECT * FROM refs WHERE collections LIKE ?1 ORDER BY added_at DESC"
                ).map_err(|e| e.to_string())?;
                return Self::collect_rows(&mut stmt, params![pattern]);
            }
            (None, Some(s)) => {
                let mut stmt = conn.prepare(
                    "SELECT * FROM refs WHERE reading_status = ?1 ORDER BY added_at DESC"
                ).map_err(|e| e.to_string())?;
                return Self::collect_rows(&mut stmt, params![s]);
            }
            (None, None) => {
                let mut stmt = conn.prepare(
                    "SELECT * FROM refs ORDER BY added_at DESC"
                ).map_err(|e| e.to_string())?;
                return Self::collect_rows(&mut stmt, params![]);
            }
        };
    }

    pub fn search(&self, query: &str) -> Result<Vec<Reference>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT * FROM refs WHERE title LIKE ?1 OR authors LIKE ?1 OR keywords LIKE ?1 OR cite_key LIKE ?1 OR doi LIKE ?1 ORDER BY added_at DESC LIMIT 50"
        ).map_err(|e| e.to_string())?;
        Self::collect_rows(&mut stmt, params![pattern])
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM refs WHERE id = ?1", params![id])
            .map_err(|e| format!("Failed to delete reference: {}", e))?;
        Ok(())
    }

    pub fn update(&self, reference: &Reference) -> Result<(), String> {
        let mut updated = reference.clone();
        updated.modified_at = Utc::now();
        self.add(&updated)
    }

    pub fn set_reading_status(&self, id: &str, status: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE refs SET reading_status = ?1, modified_at = ?2 WHERE id = ?3",
            params![status, Utc::now().to_rfc3339(), id],
        ).map_err(|e| format!("Failed to update reading status: {}", e))?;
        Ok(())
    }

    pub fn add_to_collection(&self, id: &str, collection: &str) -> Result<(), String> {
        let mut reference = self.get(id)?;
        if !reference.collections.contains(&collection.to_string()) {
            reference.collections.push(collection.to_string());
            self.update(&reference)
        } else {
            Ok(())
        }
    }

    pub fn list_collections(&self) -> Result<Vec<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT DISTINCT collections FROM refs")
            .map_err(|e| e.to_string())?;

        let mut collections = std::collections::HashSet::new();
        let rows = stmt.query_map(params![], |row| {
            let json: String = row.get(0)?;
            Ok(json)
        }).map_err(|e| e.to_string())?;

        for row in rows {
            let json = row.map_err(|e| e.to_string())?;
            if let Ok(colls) = serde_json::from_str::<Vec<String>>(&json) {
                for c in colls {
                    collections.insert(c);
                }
            }
        }

        let mut result: Vec<String> = collections.into_iter().collect();
        result.sort();
        Ok(result)
    }

    pub fn count(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM refs", params![], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        Ok(count as usize)
    }

    // --- Internal helpers ---

    fn query_one(conn: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<Reference, String> {
        conn.query_row(sql, params, |row| Self::row_to_reference(row))
            .map_err(|e| format!("Reference not found: {}", e))
    }

    fn collect_rows(stmt: &mut rusqlite::Statement, params: impl rusqlite::Params) -> Result<Vec<Reference>, String> {
        let rows = stmt.query_map(params, |row| Self::row_to_reference(row))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    fn row_to_reference(row: &rusqlite::Row) -> rusqlite::Result<Reference> {
        let authors_json: String = row.get(2)?;
        let keywords_json: String = row.get(15)?;
        let collections_json: String = row.get(18)?;
        let tags_json: String = row.get(19)?;
        let added_str: String = row.get(24)?;
        let modified_str: String = row.get(25)?;
        let status_str: String = row.get(20)?;
        let type_str: String = row.get(23)?;

        Ok(Reference {
            id: row.get(0)?,
            title: row.get(1)?,
            authors: serde_json::from_str(&authors_json).unwrap_or_default(),
            year: row.get(3)?,
            doi: row.get(4)?,
            isbn: row.get(5)?,
            issn: row.get(6)?,
            url: row.get(7)?,
            journal: row.get(8)?,
            publisher: row.get(9)?,
            volume: row.get(10)?,
            issue: row.get(11)?,
            pages: row.get(12)?,
            edition: row.get(13)?,
            abstract_text: row.get(14)?,
            keywords: serde_json::from_str(&keywords_json).unwrap_or_default(),
            notes: row.get(16)?,
            pdf_path: row.get(17)?,
            collections: serde_json::from_str(&collections_json).unwrap_or_default(),
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            reading_status: ReadingStatus::from_str(&status_str),
            rating: row.get(21)?,
            cite_key: row.get(22)?,
            ref_type: ReferenceType::from_str(&type_str),
            added_at: chrono::DateTime::parse_from_rfc3339(&added_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            modified_at: chrono::DateTime::parse_from_rfc3339(&modified_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            suid: row.get(26)?,
        })
    }
}
