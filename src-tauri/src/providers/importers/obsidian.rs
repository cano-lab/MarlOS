//! Obsidian Importer - Import notes from Obsidian vaults
//!
//! Obsidian vaults are directories containing:
//! - Markdown files with [[wikilinks]]
//! - YAML frontmatter for metadata
//! - .obsidian/ folder with configuration
//!
//! This importer:
//! - Parses markdown files with frontmatter
//! - Extracts wikilinks to build a knowledge graph
//! - Preserves tags, aliases, and other metadata
//! - Optionally chunks large notes

use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Deserialize;

use crate::memory::SecurityTier;
use crate::semantic_object::{ContentType, SemanticObject, Relation, RelationType};
use super::{Importer, ImportResult};

/// Parsed Obsidian note
#[derive(Debug, Clone)]
pub struct ObsidianNote {
    /// Relative path within vault
    pub path: String,
    /// Note title (from filename or frontmatter)
    pub title: String,
    /// Raw markdown content (without frontmatter)
    pub content: String,
    /// Parsed frontmatter
    pub frontmatter: NoteFrontmatter,
    /// Outgoing wikilinks
    pub outgoing_links: Vec<WikiLink>,
    /// Tags from frontmatter and inline
    pub tags: Vec<String>,
    /// File modification time
    pub modified_at: Option<DateTime<Utc>>,
    /// File creation time (if available)
    pub created_at: Option<DateTime<Utc>>,
}

/// Parsed frontmatter from YAML
#[derive(Debug, Clone, Default)]
pub struct NoteFrontmatter {
    /// Title override
    pub title: Option<String>,
    /// Aliases for this note
    pub aliases: Vec<String>,
    /// Tags defined in frontmatter
    pub tags: Vec<String>,
    /// Created date
    pub created: Option<String>,
    /// Modified date
    pub modified: Option<String>,
    /// All other frontmatter as JSON
    pub extra: HashMap<String, serde_json::Value>,
}

/// A wikilink reference
#[derive(Debug, Clone)]
pub struct WikiLink {
    /// Target note name or path
    pub target: String,
    /// Display text (if different from target)
    pub display: Option<String>,
    /// Whether this links to a heading
    pub heading: Option<String>,
    /// Whether this is an embed (![[...]])
    pub is_embed: bool,
}

impl ObsidianNote {
    /// Get summary for this note
    pub fn generate_summary(&self) -> String {
        let mut summary = format!("# {}\n\n", self.title);

        if !self.frontmatter.aliases.is_empty() {
            summary.push_str(&format!("Aliases: {}\n", self.frontmatter.aliases.join(", ")));
        }

        if !self.tags.is_empty() {
            summary.push_str(&format!("Tags: {}\n", self.tags.join(", ")));
        }

        if !self.outgoing_links.is_empty() {
            let link_targets: Vec<&str> = self.outgoing_links.iter()
                .take(10)
                .map(|l| l.target.as_str())
                .collect();
            summary.push_str(&format!("Links to: {}\n", link_targets.join(", ")));
        }

        // First paragraph as preview
        if let Some(first_para) = self.content.split("\n\n").next() {
            let preview: String = first_para.chars().take(200).collect();
            summary.push_str(&format!("\nPreview: {}...\n", preview));
        }

        summary
    }

    /// Get text optimized for embedding
    pub fn get_embedding_text(&self) -> String {
        let mut text = format!("Note: {}\n\n", self.title);

        if !self.tags.is_empty() {
            text.push_str(&format!("Topics: {}\n\n", self.tags.join(", ")));
        }

        // Content, truncated for embedding
        let content_truncated: String = self.content.chars().take(4000).collect();
        text.push_str(&content_truncated);

        text
    }
}

pub struct ObsidianImporter {
    /// Whether to chunk large notes
    pub chunk_large_notes: bool,
    /// Maximum characters per chunk
    pub max_note_size: usize,
    /// Include daily notes
    pub include_daily_notes: bool,
    /// Exclude patterns (regex)
    pub exclude_patterns: Vec<String>,
}

impl ObsidianImporter {
    pub fn new() -> Self {
        Self {
            chunk_large_notes: false,  // Keep notes atomic by default
            max_note_size: 10000,
            include_daily_notes: true,
            exclude_patterns: vec![
                r"^\.".to_string(),       // Hidden files
                r"/\.".to_string(),       // Hidden directories
                r"templates/".to_string(), // Template folder
            ],
        }
    }

    /// Scan an Obsidian vault and return all notes
    pub fn scan_vault(&self, vault_path: &Path) -> Result<Vec<ObsidianNote>, String> {
        if !vault_path.is_dir() {
            return Err(format!("Not a directory: {:?}", vault_path));
        }

        let obsidian_dir = vault_path.join(".obsidian");
        if !obsidian_dir.exists() {
            return Err(format!("Not an Obsidian vault (no .obsidian folder): {:?}", vault_path));
        }

        let mut notes = Vec::new();
        self.scan_directory(vault_path, vault_path, &mut notes)?;

        Ok(notes)
    }

    fn scan_directory(
        &self,
        vault_root: &Path,
        dir: &Path,
        notes: &mut Vec<ObsidianNote>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(dir)
            .map_err(|e| format!("Failed to read directory {:?}: {}", dir, e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            let relative = path.strip_prefix(vault_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            // Check exclusions
            if self.should_exclude(&relative) {
                continue;
            }

            if path.is_dir() {
                // Skip .obsidian folder
                if path.file_name().map_or(false, |n| n == ".obsidian") {
                    continue;
                }
                self.scan_directory(vault_root, &path, notes)?;
            } else if path.extension().map_or(false, |e| e == "md") {
                // Parse markdown file
                match self.parse_note(&path, &relative) {
                    Ok(note) => notes.push(note),
                    Err(e) => {
                        log::warn!("Failed to parse note {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(())
    }

    fn should_exclude(&self, relative_path: &str) -> bool {
        for pattern in &self.exclude_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(relative_path) {
                    return true;
                }
            }
        }
        false
    }

    /// Parse a single markdown note
    pub fn parse_note(&self, path: &Path, relative_path: &str) -> Result<ObsidianNote, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let (frontmatter, body) = self.parse_frontmatter(&content);

        // Get title from frontmatter or filename
        let title = frontmatter.title.clone()
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string()
            });

        // Extract wikilinks
        let outgoing_links = self.extract_wikilinks(&body);

        // Extract inline tags
        let inline_tags = self.extract_inline_tags(&body);
        let mut all_tags = frontmatter.tags.clone();
        all_tags.extend(inline_tags);

        // Get file times
        let metadata = std::fs::metadata(path).ok();
        let modified_at = metadata.as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .and_then(|d| DateTime::from_timestamp(d.as_secs() as i64, 0))
            });
        let created_at = metadata.as_ref()
            .and_then(|m| m.created().ok())
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .and_then(|d| DateTime::from_timestamp(d.as_secs() as i64, 0))
            });

        Ok(ObsidianNote {
            path: relative_path.to_string(),
            title,
            content: body,
            frontmatter,
            outgoing_links,
            tags: all_tags,
            modified_at,
            created_at,
        })
    }

    fn parse_frontmatter(&self, content: &str) -> (NoteFrontmatter, String) {
        let mut frontmatter = NoteFrontmatter::default();

        // Check for YAML frontmatter (--- ... ---)
        if !content.starts_with("---") {
            return (frontmatter, content.to_string());
        }

        // Find the closing ---
        let rest = &content[3..];
        if let Some(end_idx) = rest.find("\n---") {
            let yaml_str = &rest[..end_idx];
            let body = rest[end_idx + 4..].trim_start().to_string();

            // Parse YAML
            if let Ok(yaml) = serde_yaml::from_str::<serde_json::Value>(yaml_str) {
                if let Some(obj) = yaml.as_object() {
                    // Extract known fields
                    frontmatter.title = obj.get("title")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    frontmatter.aliases = obj.get("aliases")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    frontmatter.tags = self.extract_frontmatter_tags(obj.get("tags"));

                    frontmatter.created = obj.get("created")
                        .or_else(|| obj.get("date"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    frontmatter.modified = obj.get("modified")
                        .or_else(|| obj.get("updated"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    // Store extra fields
                    for (key, value) in obj {
                        if !["title", "aliases", "tags", "created", "date", "modified", "updated"].contains(&key.as_str()) {
                            frontmatter.extra.insert(key.clone(), value.clone());
                        }
                    }
                }
            }

            return (frontmatter, body);
        }

        (frontmatter, content.to_string())
    }

    fn extract_frontmatter_tags(&self, value: Option<&serde_json::Value>) -> Vec<String> {
        let Some(val) = value else { return Vec::new() };

        // Tags can be array or space-separated string
        if let Some(arr) = val.as_array() {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim_start_matches('#').to_string()))
                .collect()
        } else if let Some(s) = val.as_str() {
            s.split_whitespace()
                .map(|t| t.trim_start_matches('#').to_string())
                .collect()
        } else {
            Vec::new()
        }
    }

    fn extract_wikilinks(&self, content: &str) -> Vec<WikiLink> {
        let mut links = Vec::new();

        // Match [[target]] or [[target|display]] or [[target#heading]]
        // Also match embeds ![[...]]
        let re = Regex::new(r"(!?)\[\[([^\]|#]+)(?:#([^\]|]+))?(?:\|([^\]]+))?\]\]")
            .unwrap();

        for cap in re.captures_iter(content) {
            let is_embed = cap.get(1).map_or(false, |m| m.as_str() == "!");
            let target = cap.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            let heading = cap.get(3).map(|m| m.as_str().to_string());
            let display = cap.get(4).map(|m| m.as_str().to_string());

            if !target.is_empty() {
                links.push(WikiLink {
                    target,
                    display,
                    heading,
                    is_embed,
                });
            }
        }

        links
    }

    fn extract_inline_tags(&self, content: &str) -> Vec<String> {
        let mut tags = HashSet::new();

        // Match #tag (but not ## headings)
        let re = Regex::new(r"(?:^|\s)#([a-zA-Z][a-zA-Z0-9_/-]*)").unwrap();

        for cap in re.captures_iter(content) {
            if let Some(tag) = cap.get(1) {
                tags.insert(tag.as_str().to_string());
            }
        }

        tags.into_iter().collect()
    }

    /// Convert notes to SemanticObjects, preserving links as relations
    pub fn notes_to_objects(&self, notes: &[ObsidianNote]) -> Vec<SemanticObject> {
        // First pass: create objects and build name -> SUID map
        let mut objects = Vec::new();
        let mut name_to_suid: HashMap<String, crate::semantic_object::Suid> = HashMap::new();

        for note in notes {
            let obj = self.note_to_object(note);
            name_to_suid.insert(note.title.to_lowercase(), obj.suid);

            // Also map aliases
            for alias in &note.frontmatter.aliases {
                name_to_suid.insert(alias.to_lowercase(), obj.suid);
            }

            objects.push(obj);
        }

        // Second pass: add relations based on wikilinks
        for (i, note) in notes.iter().enumerate() {
            for link in &note.outgoing_links {
                let target_lower = link.target.to_lowercase();
                if let Some(&target_suid) = name_to_suid.get(&target_lower) {
                    let rel_type = if link.is_embed {
                        RelationType::Contains
                    } else {
                        RelationType::References
                    };
                    objects[i].relations.push(Relation::new(target_suid, rel_type));
                }
            }
        }

        objects
    }

    fn note_to_object(&self, note: &ObsidianNote) -> SemanticObject {
        let embedding_text = note.get_embedding_text();

        let mut obj = SemanticObject::new(
            note.content.as_bytes().to_vec(),
            ContentType::Markdown,
        );

        obj.name = Some(note.title.clone());
        obj.summary = Some(note.generate_summary());
        obj.security_tier = SecurityTier::Open;

        // Metadata
        obj.metadata.insert("source".to_string(), serde_json::json!("obsidian"));
        obj.metadata.insert("path".to_string(), serde_json::json!(note.path));
        obj.metadata.insert("embedding_text".to_string(), serde_json::json!(embedding_text));

        if !note.frontmatter.aliases.is_empty() {
            obj.metadata.insert("aliases".to_string(), serde_json::json!(note.frontmatter.aliases));
        }

        // Add extra frontmatter to metadata
        for (key, value) in &note.frontmatter.extra {
            obj.metadata.insert(key.clone(), value.clone());
        }

        // Outgoing links for graph building
        let link_targets: Vec<&str> = note.outgoing_links.iter()
            .map(|l| l.target.as_str())
            .collect();
        if !link_targets.is_empty() {
            obj.metadata.insert("outgoing_links".to_string(), serde_json::json!(link_targets));
        }

        // Tags
        obj.tags.push("obsidian".to_string());
        obj.tags.push("note".to_string());
        for tag in &note.tags {
            obj.tags.push(tag.clone());
        }

        // Timestamps
        if let Some(created) = note.created_at {
            obj.created_at = created;
        }
        if let Some(modified) = note.modified_at {
            obj.modified_at = modified;
        }

        obj
    }
}

impl Default for ObsidianImporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Importer for ObsidianImporter {
    fn source_name(&self) -> &'static str {
        "Obsidian"
    }

    fn can_import(&self, path: &Path) -> bool {
        path.is_dir() && path.join(".obsidian").exists()
    }

    fn import(&self, path: &Path, _embed: bool) -> Result<ImportResult, String> {
        let mut result = ImportResult::new("Obsidian");

        let notes = self.scan_vault(path)?;
        result.items_imported = notes.len();

        let objects = self.notes_to_objects(&notes);
        result.objects_created = objects.len();
        result.objects = objects;

        Ok(result)
    }
}

// Need serde_yaml for frontmatter parsing
mod serde_yaml {
    pub fn from_str<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, String> {
        // Simple YAML parser for frontmatter
        // This is a minimal implementation - a real one would use the serde_yaml crate
        let json_str = yaml_to_json(s)?;
        serde_json::from_str(&json_str).map_err(|e| e.to_string())
    }

    fn yaml_to_json(yaml: &str) -> Result<String, String> {
        // Very simple YAML to JSON converter for frontmatter
        // Handles: key: value, key: [array], key: multi-line
        let mut result = String::from("{");
        let mut first = true;
        let mut current_key: Option<String> = None;
        let mut array_values: Vec<String> = Vec::new();
        let mut in_array = false;

        for line in yaml.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() {
                continue;
            }

            // Check for array item
            if trimmed.starts_with("- ") {
                if let Some(ref _key) = current_key {
                    let value = trimmed.strip_prefix("- ").unwrap().trim();
                    array_values.push(format!("\"{}\"", escape_json(value)));
                    in_array = true;
                    continue;
                }
            }

            // Close previous array if we're starting a new key
            if in_array {
                if !first { result.push(','); }
                first = false;
                if let Some(key) = current_key.take() {
                    result.push_str(&format!("\"{}\":[{}]", key, array_values.join(",")));
                }
                array_values.clear();
                in_array = false;
            }

            // Parse key: value
            if let Some(colon_idx) = trimmed.find(':') {
                let key = trimmed[..colon_idx].trim();
                let value = trimmed[colon_idx + 1..].trim();

                if value.is_empty() {
                    // Start of array or nested object
                    current_key = Some(key.to_string());
                } else if value.starts_with('[') && value.ends_with(']') {
                    // Inline array
                    if !first { result.push(','); }
                    first = false;
                    let inner = &value[1..value.len()-1];
                    let items: Vec<String> = inner.split(',')
                        .map(|s| format!("\"{}\"", escape_json(s.trim())))
                        .collect();
                    result.push_str(&format!("\"{}\":[{}]", key, items.join(",")));
                } else {
                    // Simple value
                    if !first { result.push(','); }
                    first = false;
                    result.push_str(&format!("\"{}\":\"{}\"", key, escape_json(value)));
                }
            }
        }

        // Close any remaining array
        if in_array {
            if !first { result.push(','); }
            if let Some(key) = current_key {
                result.push_str(&format!("\"{}\":[{}]", key, array_values.join(",")));
            }
        }

        result.push('}');
        Ok(result)
    }

    fn escape_json(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wikilink_extraction() {
        let importer = ObsidianImporter::new();

        let content = "Check out [[Another Note]] and [[Link|Display Text]] and [[Note#Heading]]";
        let links = importer.extract_wikilinks(content);

        assert_eq!(links.len(), 3);
        assert_eq!(links[0].target, "Another Note");
        assert_eq!(links[1].target, "Link");
        assert_eq!(links[1].display, Some("Display Text".to_string()));
        assert_eq!(links[2].heading, Some("Heading".to_string()));
    }

    #[test]
    fn test_inline_tags() {
        let importer = ObsidianImporter::new();

        let content = "This has #tag1 and #tag2/nested but ## not-a-heading";
        let tags = importer.extract_inline_tags(content);

        assert!(tags.contains(&"tag1".to_string()));
        assert!(tags.contains(&"tag2/nested".to_string()));
        assert!(!tags.iter().any(|t| t.contains("not-a-heading")));
    }

    #[test]
    fn test_frontmatter_parsing() {
        let importer = ObsidianImporter::new();

        let content = r#"---
title: My Note
tags:
  - tag1
  - tag2
aliases: [alias1, alias2]
---

# Content here"#;

        let (fm, body) = importer.parse_frontmatter(content);

        assert_eq!(fm.title, Some("My Note".to_string()));
        assert!(body.contains("# Content here"));
    }

    #[test]
    fn test_importer_detection() {
        let importer = ObsidianImporter::new();
        assert_eq!(importer.source_name(), "Obsidian");
    }
}
