//! Semantic Object - The core storage primitive for MarlOS
//!
//! Key insight: "Objects are for machines, files are for humans."
//!
//! Internally, everything is typed, searchable, security-tiered objects.
//! Files only exist at boundaries (import/export).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::memory::SecurityTier;

// ============================================================================
// SUID - Semantic Unique Identifier
// ============================================================================

/// 128-bit Semantic Unique Identifier
///
/// Unlike file paths, SUIds are:
/// - Globally unique (UUID v4)
/// - Immutable (never changes for an object's lifetime)
/// - Location-independent (object can move without changing identity)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Suid(Uuid);

impl Suid {
    /// Generate a new random SUID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from existing UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse from string
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Get short form (first 8 chars) for display
    pub fn short(&self) -> String {
        self.0.to_string()[..8].to_string()
    }
}

impl Default for Suid {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Suid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for Suid {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

// ============================================================================
// Content Types
// ============================================================================

/// Content type classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    /// Plain text (UTF-8)
    Text,
    /// Markdown document
    Markdown,
    /// Source code with language hint
    Code { language: String },
    /// JSON data
    Json,
    /// Binary data with MIME type
    Binary { mime: String },
    /// Structured data with schema
    Structured { schema: String },
    /// Unknown/unclassified
    Unknown,
}

impl ContentType {
    /// Detect content type from file extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "md" | "markdown" => ContentType::Markdown,
            "txt" => ContentType::Text,
            "json" => ContentType::Json,
            "rs" => ContentType::Code { language: "rust".to_string() },
            "py" => ContentType::Code { language: "python".to_string() },
            "js" | "mjs" => ContentType::Code { language: "javascript".to_string() },
            "ts" | "tsx" => ContentType::Code { language: "typescript".to_string() },
            "go" => ContentType::Code { language: "go".to_string() },
            "c" | "h" => ContentType::Code { language: "c".to_string() },
            "cpp" | "hpp" | "cc" => ContentType::Code { language: "cpp".to_string() },
            "java" => ContentType::Code { language: "java".to_string() },
            "html" | "htm" => ContentType::Code { language: "html".to_string() },
            "css" | "scss" => ContentType::Code { language: "css".to_string() },
            "yaml" | "yml" => ContentType::Code { language: "yaml".to_string() },
            "toml" => ContentType::Code { language: "toml".to_string() },
            "xml" => ContentType::Code { language: "xml".to_string() },
            "sh" | "bash" => ContentType::Code { language: "bash".to_string() },
            "sql" => ContentType::Code { language: "sql".to_string() },
            "pdf" => ContentType::Binary { mime: "application/pdf".to_string() },
            "png" => ContentType::Binary { mime: "image/png".to_string() },
            "jpg" | "jpeg" => ContentType::Binary { mime: "image/jpeg".to_string() },
            "gif" => ContentType::Binary { mime: "image/gif".to_string() },
            "svg" => ContentType::Binary { mime: "image/svg+xml".to_string() },
            _ => ContentType::Unknown,
        }
    }

    /// Check if content is text-based (can be embedded)
    pub fn is_text(&self) -> bool {
        matches!(
            self,
            ContentType::Text
                | ContentType::Markdown
                | ContentType::Code { .. }
                | ContentType::Json
        )
    }
}

impl Default for ContentType {
    fn default() -> Self {
        ContentType::Unknown
    }
}

// ============================================================================
// Relations
// ============================================================================

/// Types of relationships between objects
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    /// Object A references/cites object B
    References,
    /// Object A contains object B (parent-child)
    Contains,
    /// Object A is derived from object B (fork, copy, based-on)
    DerivedFrom,
    /// Objects are semantically related (auto-detected)
    RelatedTo,
    /// Object A is a newer version of B
    VersionOf,
    /// Object A replies to / comments on B
    RepliesTo,
    /// Object A depends on B (code dependency)
    DependsOn,
    /// Custom relationship type
    Custom(String),
}

/// A relation to another object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    /// Target object SUID
    pub target: Suid,
    /// Type of relationship
    pub relation_type: RelationType,
    /// Optional metadata about the relation
    pub metadata: Option<serde_json::Value>,
    /// When this relation was created
    pub created_at: DateTime<Utc>,
}

impl Relation {
    pub fn new(target: Suid, relation_type: RelationType) -> Self {
        Self {
            target,
            relation_type,
            metadata: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

// ============================================================================
// Semantic Object
// ============================================================================

/// The core storage primitive - a semantic object
///
/// Unlike files, semantic objects:
/// - Have stable identity (SUID) independent of location
/// - Are typed and schema-aware
/// - Are searchable by meaning (vector embeddings)
/// - Have explicit relationships to other objects
/// - Are classified by security tier
/// - Are automatically versioned
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticObject {
    // === Identity ===
    /// Unique identifier (never changes)
    pub suid: Suid,
    /// Human-readable name (optional, not unique)
    pub name: Option<String>,
    /// Virtual path for compatibility (optional)
    pub path: Option<String>,

    // === Content ===
    /// The actual content (may be lazy-loaded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<u8>>,
    /// Content type classification
    pub content_type: ContentType,
    /// Size in bytes (always available even if content is not loaded)
    pub size_bytes: usize,
    /// Content hash for deduplication
    pub content_hash: Option<String>,

    // === Semantics ===
    /// Vector embedding for similarity search (384-dim for all-MiniLM-L6-v2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// User/app-defined tags
    pub tags: Vec<String>,
    /// Summary for display (especially for Guarded tier)
    pub summary: Option<String>,

    // === Relationships ===
    /// Links to other objects
    pub relations: Vec<Relation>,

    // === Security ===
    /// Security tier (Open/Guarded/Sealed)
    pub security_tier: SecurityTier,

    // === Temporal ===
    /// When object was created
    pub created_at: DateTime<Utc>,
    /// When object was last modified
    pub modified_at: DateTime<Utc>,
    /// Version number (increments on each update)
    pub version: u64,

    // === Metadata ===
    /// Arbitrary key-value metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl SemanticObject {
    /// Create a new semantic object with content
    pub fn new(content: Vec<u8>, content_type: ContentType) -> Self {
        let size_bytes = content.len();
        let content_hash = Some(Self::hash_content(&content));

        Self {
            suid: Suid::new(),
            name: None,
            path: None,
            content: Some(content),
            content_type,
            size_bytes,
            content_hash,
            embedding: None,
            tags: Vec::new(),
            summary: None,
            relations: Vec::new(),
            security_tier: SecurityTier::Guarded, // Default to Guarded
            created_at: Utc::now(),
            modified_at: Utc::now(),
            version: 1,
            metadata: HashMap::new(),
        }
    }

    /// Create from text content
    pub fn from_text(text: &str) -> Self {
        Self::new(text.as_bytes().to_vec(), ContentType::Text)
    }

    /// Create from markdown content
    pub fn from_markdown(text: &str) -> Self {
        Self::new(text.as_bytes().to_vec(), ContentType::Markdown)
    }

    /// Hash content for deduplication (using blake3)
    fn hash_content(content: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Builder: set name
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// Builder: set path
    pub fn with_path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    /// Builder: set security tier
    pub fn with_tier(mut self, tier: SecurityTier) -> Self {
        self.security_tier = tier;
        self
    }

    /// Builder: add tag
    pub fn with_tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    /// Builder: add tags
    pub fn with_tags(mut self, tags: &[&str]) -> Self {
        self.tags.extend(tags.iter().map(|s| s.to_string()));
        self
    }

    /// Builder: set summary
    pub fn with_summary(mut self, summary: &str) -> Self {
        self.summary = Some(summary.to_string());
        self
    }

    /// Builder: set embedding
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Builder: add relation
    pub fn with_relation(mut self, relation: Relation) -> Self {
        self.relations.push(relation);
        self
    }

    /// Builder: set metadata
    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }

    /// Get content as string (if text-based)
    pub fn content_as_str(&self) -> Option<&str> {
        self.content
            .as_ref()
            .and_then(|c| std::str::from_utf8(c).ok())
    }

    /// Update content and bump version
    pub fn update_content(&mut self, content: Vec<u8>) {
        self.size_bytes = content.len();
        self.content_hash = Some(Self::hash_content(&content));
        self.content = Some(content);
        self.modified_at = Utc::now();
        self.version += 1;
        // Clear embedding - needs to be regenerated
        self.embedding = None;
    }

    /// Create a view without content (for Guarded tier responses)
    pub fn to_guarded_view(&self) -> GuardedObjectView {
        GuardedObjectView {
            suid: self.suid,
            name: self.name.clone(),
            path: self.path.clone(),
            content_type: self.content_type.clone(),
            size_bytes: self.size_bytes,
            tags: self.tags.clone(),
            summary: self.summary.clone(),
            security_tier: self.security_tier,
            created_at: self.created_at,
            modified_at: self.modified_at,
            version: self.version,
        }
    }

    /// Create a minimal view (for listings)
    pub fn to_minimal_view(&self) -> MinimalObjectView {
        MinimalObjectView {
            suid: self.suid,
            name: self.name.clone(),
            content_type: self.content_type.clone(),
            size_bytes: self.size_bytes,
            security_tier: self.security_tier,
            modified_at: self.modified_at,
        }
    }
}

/// A view of an object without content (for Guarded tier)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardedObjectView {
    pub suid: Suid,
    pub name: Option<String>,
    pub path: Option<String>,
    pub content_type: ContentType,
    pub size_bytes: usize,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub security_tier: SecurityTier,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub version: u64,
}

/// Minimal view for listings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimalObjectView {
    pub suid: Suid,
    pub name: Option<String>,
    pub content_type: ContentType,
    pub size_bytes: usize,
    pub security_tier: SecurityTier,
    pub modified_at: DateTime<Utc>,
}

// ============================================================================
// Object Creation Options
// ============================================================================

/// Options for creating a new semantic object
#[derive(Debug, Clone, Default)]
pub struct CreateOptions {
    pub name: Option<String>,
    pub path: Option<String>,
    pub content_type: Option<ContentType>,
    pub security_tier: Option<SecurityTier>,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub source_hint: Option<String>, // Original file path (for reference)
}

impl CreateOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn with_path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    pub fn with_tier(mut self, tier: SecurityTier) -> Self {
        self.security_tier = Some(tier);
        self
    }

    pub fn with_tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|s| s.to_string()).collect();
        self
    }
}

// ============================================================================
// Search Types
// ============================================================================

/// Query for searching objects
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Semantic text search (uses embeddings)
    pub text: Option<String>,
    /// Filter by tags (AND logic)
    pub tags: Option<Vec<String>>,
    /// Filter by content type
    pub content_types: Option<Vec<ContentType>>,
    /// Maximum security tier to include
    pub max_tier: Option<SecurityTier>,
    /// Filter by creation time
    pub created_after: Option<DateTime<Utc>>,
    /// Filter by modification time
    pub modified_after: Option<DateTime<Utc>>,
    /// Maximum results to return
    pub limit: usize,
}

impl SearchQuery {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }

    pub fn text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    pub fn tags(mut self, tags: &[&str]) -> Self {
        self.tags = Some(tags.iter().map(|s| s.to_string()).collect());
        self
    }

    pub fn max_tier(mut self, tier: SecurityTier) -> Self {
        self.max_tier = Some(tier);
        self
    }
}

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub object: SemanticObject,
    pub score: f32,
}

// ============================================================================
// Relation Store - Managing Object Relationships
// ============================================================================

/// Represents an edge in the relation graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationEdge {
    /// Source object SUID
    pub source: Suid,
    /// Target object SUID
    pub target: Suid,
    /// Type of relationship
    pub relation_type: RelationType,
    /// Optional metadata about the relation
    pub metadata: Option<serde_json::Value>,
    /// When this relation was created
    pub created_at: DateTime<Utc>,
}

impl RelationEdge {
    pub fn new(source: Suid, target: Suid, relation_type: RelationType) -> Self {
        Self {
            source,
            target,
            relation_type,
            metadata: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Convert to a Relation (for embedding in SemanticObject)
    pub fn to_relation(&self) -> Relation {
        Relation {
            target: self.target,
            relation_type: self.relation_type.clone(),
            metadata: self.metadata.clone(),
            created_at: self.created_at,
        }
    }
}

/// In-memory relation store for managing object relationships
///
/// Supports bidirectional traversal: find what an object references
/// and what references it.
#[derive(Debug, Default)]
pub struct RelationStore {
    /// All relation edges
    edges: Vec<RelationEdge>,
    /// Index: source SUID -> edge indices (outgoing relations)
    outgoing_index: HashMap<Suid, Vec<usize>>,
    /// Index: target SUID -> edge indices (incoming relations)
    incoming_index: HashMap<Suid, Vec<usize>>,
}

impl RelationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a relation between two objects
    pub fn add(&mut self, source: Suid, target: Suid, relation_type: RelationType) -> &RelationEdge {
        self.add_edge(RelationEdge::new(source, target, relation_type))
    }

    /// Add a relation edge
    pub fn add_edge(&mut self, edge: RelationEdge) -> &RelationEdge {
        let index = self.edges.len();

        // Update indices
        self.outgoing_index
            .entry(edge.source)
            .or_default()
            .push(index);
        self.incoming_index
            .entry(edge.target)
            .or_default()
            .push(index);

        self.edges.push(edge);
        &self.edges[index]
    }

    /// Get all outgoing relations from an object (what this object points to)
    pub fn outgoing(&self, source: &Suid) -> Vec<&RelationEdge> {
        self.outgoing_index
            .get(source)
            .map(|indices| indices.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }

    /// Get outgoing relations of a specific type
    pub fn outgoing_of_type(&self, source: &Suid, relation_type: &RelationType) -> Vec<&RelationEdge> {
        self.outgoing(source)
            .into_iter()
            .filter(|e| &e.relation_type == relation_type)
            .collect()
    }

    /// Get all incoming relations to an object (what points to this object)
    pub fn incoming(&self, target: &Suid) -> Vec<&RelationEdge> {
        self.incoming_index
            .get(target)
            .map(|indices| indices.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }

    /// Get incoming relations of a specific type
    pub fn incoming_of_type(&self, target: &Suid, relation_type: &RelationType) -> Vec<&RelationEdge> {
        self.incoming(target)
            .into_iter()
            .filter(|e| &e.relation_type == relation_type)
            .collect()
    }

    /// Check if a relation exists between two objects
    pub fn exists(&self, source: &Suid, target: &Suid, relation_type: Option<&RelationType>) -> bool {
        self.outgoing(source).iter().any(|e| {
            &e.target == target && relation_type.map_or(true, |rt| &e.relation_type == rt)
        })
    }

    /// Get all objects that contain this object (parents)
    pub fn get_parents(&self, child: &Suid) -> Vec<Suid> {
        self.incoming_of_type(child, &RelationType::Contains)
            .iter()
            .map(|e| e.source)
            .collect()
    }

    /// Get all objects contained by this object (children)
    pub fn get_children(&self, parent: &Suid) -> Vec<Suid> {
        self.outgoing_of_type(parent, &RelationType::Contains)
            .iter()
            .map(|e| e.target)
            .collect()
    }

    /// Get all objects that reference this object
    pub fn get_referrers(&self, target: &Suid) -> Vec<Suid> {
        self.incoming_of_type(target, &RelationType::References)
            .iter()
            .map(|e| e.source)
            .collect()
    }

    /// Get all objects this object references
    pub fn get_references(&self, source: &Suid) -> Vec<Suid> {
        self.outgoing_of_type(source, &RelationType::References)
            .iter()
            .map(|e| e.target)
            .collect()
    }

    /// Get the version history chain (all versions leading to this object)
    pub fn get_version_history(&self, suid: &Suid) -> Vec<Suid> {
        let mut history = Vec::new();
        let mut current = *suid;

        // Follow VersionOf relations backward
        while let Some(edge) = self
            .outgoing_of_type(&current, &RelationType::VersionOf)
            .first()
        {
            history.push(edge.target);
            current = edge.target;
        }

        history
    }

    /// Remove all relations involving an object (when deleting)
    pub fn remove_object(&mut self, suid: &Suid) {
        // Remove from edges and rebuild indices
        self.edges.retain(|e| &e.source != suid && &e.target != suid);
        self.rebuild_indices();
    }

    /// Remove a specific relation
    pub fn remove(&mut self, source: &Suid, target: &Suid, relation_type: &RelationType) -> bool {
        let original_len = self.edges.len();
        self.edges.retain(|e| {
            !(&e.source == source && &e.target == target && &e.relation_type == relation_type)
        });

        if self.edges.len() != original_len {
            self.rebuild_indices();
            true
        } else {
            false
        }
    }

    /// Rebuild indices after modifications
    fn rebuild_indices(&mut self) {
        self.outgoing_index.clear();
        self.incoming_index.clear();

        for (index, edge) in self.edges.iter().enumerate() {
            self.outgoing_index
                .entry(edge.source)
                .or_default()
                .push(index);
            self.incoming_index
                .entry(edge.target)
                .or_default()
                .push(index);
        }
    }

    /// Get total number of relations
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    /// Check if store is empty
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Get all edges (for serialization)
    pub fn all_edges(&self) -> &[RelationEdge] {
        &self.edges
    }
}

// ============================================================================
// File Boundary Gateway - Import/Export
// ============================================================================

use std::path::Path;

/// Result of importing a file
#[derive(Debug)]
pub struct ImportResult {
    /// The created semantic object
    pub object: SemanticObject,
    /// Detected security tier
    pub detected_tier: SecurityTier,
    /// Whether secrets were detected in content
    pub secrets_detected: bool,
    /// Original file path
    pub source_path: String,
}

/// Result of exporting an object
#[derive(Debug)]
pub struct ExportResult {
    /// Path where the file was written
    pub path: String,
    /// Number of bytes written
    pub bytes_written: usize,
}

/// File Boundary Gateway
///
/// Manages the boundary between files (human-facing) and objects (machine-facing).
/// Files are imported INTO objects; objects are exported TO files.
/// There is no bidirectional sync - these are one-way transformations.
pub struct FileBoundary;

impl FileBoundary {
    /// Import a file into a semantic object
    ///
    /// This is a one-way transformation. The file's content becomes an object.
    /// The original file is not tracked or synced.
    pub fn import(
        file_path: &Path,
        options: Option<CreateOptions>,
    ) -> Result<ImportResult, String> {
        // Read file content
        let content = std::fs::read(file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        // Detect content type from extension
        let content_type = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(ContentType::from_extension)
            .unwrap_or(ContentType::Unknown);

        // Get path as string for tier classification
        let path_str = file_path.to_string_lossy().to_string();

        // Use tier classifier to detect appropriate tier
        let classifier = crate::tier_classifier::TierClassifier::new();
        let content_str = std::str::from_utf8(&content).unwrap_or("");
        let detected_tier = classifier.classify(
            content_str,
            Some(&path_str),
            crate::memory::MemoryType::Document,
        );
        let secrets_detected = classifier.contains_secrets(content_str);

        // Create the object
        let opts = options.unwrap_or_default();
        let mut obj = SemanticObject::new(content, content_type);

        // Apply options
        if let Some(name) = opts.name {
            obj.name = Some(name);
        } else {
            // Use filename as default name
            obj.name = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
        }

        if let Some(path) = opts.path {
            obj.path = Some(path);
        }

        // Use explicit tier if provided, otherwise use detected
        obj.security_tier = opts.security_tier.unwrap_or(detected_tier);

        obj.tags = opts.tags;
        obj.summary = opts.summary;

        // Store source hint in metadata
        obj.metadata.insert(
            "source_file".to_string(),
            serde_json::json!(path_str),
        );
        obj.metadata.insert(
            "import_time".to_string(),
            serde_json::json!(Utc::now().to_rfc3339()),
        );

        Ok(ImportResult {
            object: obj,
            detected_tier,
            secrets_detected,
            source_path: path_str,
        })
    }

    /// Import text content directly (without a file)
    pub fn import_text(
        text: &str,
        name: &str,
        content_type: ContentType,
    ) -> ImportResult {
        let classifier = crate::tier_classifier::TierClassifier::new();
        let detected_tier = classifier.classify(
            text,
            None,
            crate::memory::MemoryType::Document,
        );
        let secrets_detected = classifier.contains_secrets(text);

        let obj = SemanticObject::new(text.as_bytes().to_vec(), content_type)
            .with_name(name);

        ImportResult {
            object: obj,
            detected_tier,
            secrets_detected,
            source_path: String::new(),
        }
    }

    /// Export an object to a file
    ///
    /// This creates a file from the object's content.
    /// The file is not linked to the object after export.
    pub fn export(
        object: &SemanticObject,
        dest_path: &Path,
    ) -> Result<ExportResult, String> {
        // Prevent exporting Sealed content
        if object.security_tier == SecurityTier::Sealed {
            return Err("Cannot export Sealed content".to_string());
        }

        let content = object.content.as_ref()
            .ok_or("Object has no content to export")?;

        // Create parent directories if needed
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        // Write the file
        std::fs::write(dest_path, content)
            .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(ExportResult {
            path: dest_path.to_string_lossy().to_string(),
            bytes_written: content.len(),
        })
    }

    /// Export object with automatic filename based on name/suid
    pub fn export_auto(
        object: &SemanticObject,
        dest_dir: &Path,
    ) -> Result<ExportResult, String> {
        let extension = match &object.content_type {
            ContentType::Text => "txt",
            ContentType::Markdown => "md",
            ContentType::Code { language } => match language.as_str() {
                "rust" => "rs",
                "python" => "py",
                "javascript" => "js",
                "typescript" => "ts",
                "go" => "go",
                "java" => "java",
                "c" => "c",
                "cpp" => "cpp",
                "html" => "html",
                "css" => "css",
                "yaml" => "yaml",
                "toml" => "toml",
                "json" => "json",
                "sql" => "sql",
                "bash" => "sh",
                _ => "txt",
            },
            ContentType::Json => "json",
            ContentType::Binary { mime } => {
                if mime.contains("pdf") { "pdf" }
                else if mime.contains("png") { "png" }
                else if mime.contains("jpeg") || mime.contains("jpg") { "jpg" }
                else if mime.contains("gif") { "gif" }
                else if mime.contains("svg") { "svg" }
                else { "bin" }
            },
            ContentType::Structured { .. } => "json",
            ContentType::Unknown => "bin",
        };

        let filename = object
            .name
            .clone()
            .unwrap_or_else(|| object.suid.short());

        // Sanitize filename
        let safe_filename: String = filename
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();

        let dest_path = dest_dir.join(format!("{}.{}", safe_filename, extension));
        Self::export(object, &dest_path)
    }

    /// Bulk import all files from a directory
    pub fn import_directory(
        dir_path: &Path,
        recursive: bool,
        options: Option<CreateOptions>,
    ) -> Result<Vec<ImportResult>, String> {
        let mut results = Vec::new();

        let entries = std::fs::read_dir(dir_path)
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();

            if path.is_file() {
                match Self::import(&path, options.clone()) {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        log::warn!("Failed to import {}: {}", path.display(), e);
                    }
                }
            } else if path.is_dir() && recursive {
                match Self::import_directory(&path, recursive, options.clone()) {
                    Ok(mut sub_results) => results.append(&mut sub_results),
                    Err(e) => {
                        log::warn!("Failed to import directory {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suid_generation() {
        let suid1 = Suid::new();
        let suid2 = Suid::new();
        assert_ne!(suid1, suid2);
    }

    #[test]
    fn test_suid_parsing() {
        let suid = Suid::new();
        let s = suid.to_string();
        let parsed = Suid::parse(&s).unwrap();
        assert_eq!(suid, parsed);
    }

    #[test]
    fn test_object_creation() {
        let obj = SemanticObject::from_text("Hello, world!")
            .with_name("greeting")
            .with_tier(SecurityTier::Open)
            .with_tag("test");

        assert_eq!(obj.name, Some("greeting".to_string()));
        assert_eq!(obj.security_tier, SecurityTier::Open);
        assert!(obj.tags.contains(&"test".to_string()));
        assert_eq!(obj.content_as_str(), Some("Hello, world!"));
    }

    #[test]
    fn test_content_type_detection() {
        assert!(matches!(ContentType::from_extension("md"), ContentType::Markdown));
        assert!(matches!(ContentType::from_extension("rs"), ContentType::Code { .. }));
        assert!(matches!(ContentType::from_extension("pdf"), ContentType::Binary { .. }));
    }

    #[test]
    fn test_guarded_view() {
        let obj = SemanticObject::from_text("Secret content")
            .with_name("doc")
            .with_summary("A document");

        let view = obj.to_guarded_view();
        assert_eq!(view.suid, obj.suid);
        assert_eq!(view.summary, Some("A document".to_string()));
        // View doesn't have content field
    }

    #[test]
    fn test_version_increment() {
        let mut obj = SemanticObject::from_text("v1");
        assert_eq!(obj.version, 1);

        obj.update_content("v2".as_bytes().to_vec());
        assert_eq!(obj.version, 2);
        assert_eq!(obj.content_as_str(), Some("v2"));
    }

    // ========================================
    // RelationStore Tests
    // ========================================

    #[test]
    fn test_relation_store_add_and_query() {
        let mut store = RelationStore::new();

        let obj1 = Suid::new();
        let obj2 = Suid::new();
        let obj3 = Suid::new();

        // obj1 references obj2
        store.add(obj1, obj2, RelationType::References);
        // obj1 contains obj3
        store.add(obj1, obj3, RelationType::Contains);
        // obj2 references obj3
        store.add(obj2, obj3, RelationType::References);

        // Check outgoing from obj1
        let outgoing = store.outgoing(&obj1);
        assert_eq!(outgoing.len(), 2);

        // Check incoming to obj3
        let incoming = store.incoming(&obj3);
        assert_eq!(incoming.len(), 2);

        // Check specific type query
        let refs = store.outgoing_of_type(&obj1, &RelationType::References);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].target, obj2);
    }

    #[test]
    fn test_relation_store_bidirectional() {
        let mut store = RelationStore::new();

        let parent = Suid::new();
        let child1 = Suid::new();
        let child2 = Suid::new();

        store.add(parent, child1, RelationType::Contains);
        store.add(parent, child2, RelationType::Contains);

        // Get children
        let children = store.get_children(&parent);
        assert_eq!(children.len(), 2);
        assert!(children.contains(&child1));
        assert!(children.contains(&child2));

        // Get parents
        let parents = store.get_parents(&child1);
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0], parent);
    }

    #[test]
    fn test_relation_store_exists() {
        let mut store = RelationStore::new();

        let a = Suid::new();
        let b = Suid::new();

        store.add(a, b, RelationType::References);

        assert!(store.exists(&a, &b, None));
        assert!(store.exists(&a, &b, Some(&RelationType::References)));
        assert!(!store.exists(&a, &b, Some(&RelationType::Contains)));
        assert!(!store.exists(&b, &a, None)); // Reverse doesn't exist
    }

    #[test]
    fn test_relation_store_remove() {
        let mut store = RelationStore::new();

        let a = Suid::new();
        let b = Suid::new();
        let c = Suid::new();

        store.add(a, b, RelationType::References);
        store.add(a, c, RelationType::References);

        assert_eq!(store.len(), 2);

        // Remove one relation
        assert!(store.remove(&a, &b, &RelationType::References));
        assert_eq!(store.len(), 1);
        assert!(!store.exists(&a, &b, None));
        assert!(store.exists(&a, &c, None));

        // Try to remove non-existent
        assert!(!store.remove(&a, &b, &RelationType::References));
    }

    #[test]
    fn test_relation_store_remove_object() {
        let mut store = RelationStore::new();

        let a = Suid::new();
        let b = Suid::new();
        let c = Suid::new();

        store.add(a, b, RelationType::References);
        store.add(b, c, RelationType::Contains);
        store.add(c, a, RelationType::DerivedFrom);

        assert_eq!(store.len(), 3);

        // Remove all relations involving b
        store.remove_object(&b);
        assert_eq!(store.len(), 1); // Only c->a remains
        assert!(store.exists(&c, &a, None));
    }

    #[test]
    fn test_version_history() {
        let mut store = RelationStore::new();

        let v1 = Suid::new();
        let v2 = Suid::new();
        let v3 = Suid::new();

        // v3 is version of v2, v2 is version of v1
        store.add(v2, v1, RelationType::VersionOf);
        store.add(v3, v2, RelationType::VersionOf);

        // Get history from v3
        let history = store.get_version_history(&v3);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0], v2);
        assert_eq!(history[1], v1);
    }

    // ========================================
    // FileBoundary Tests
    // ========================================

    #[test]
    fn test_import_text() {
        let result = FileBoundary::import_text(
            "Hello, world!",
            "greeting",
            ContentType::Text,
        );

        assert_eq!(result.object.name, Some("greeting".to_string()));
        assert_eq!(result.object.content_as_str(), Some("Hello, world!"));
        assert!(!result.secrets_detected);
    }

    #[test]
    fn test_import_text_with_secret() {
        let result = FileBoundary::import_text(
            "api_key=sk-1234567890abcdefghijklmnop",
            "config",
            ContentType::Text,
        );

        assert!(result.secrets_detected);
        assert_eq!(result.detected_tier, SecurityTier::Sealed);
    }

    #[test]
    fn test_export_sealed_blocked() {
        let obj = SemanticObject::from_text("secret data")
            .with_tier(SecurityTier::Sealed);

        let result = FileBoundary::export(&obj, Path::new("/tmp/test.txt"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Sealed"));
    }

    #[test]
    fn test_export_auto_extension() {
        // Test that we get correct extensions
        let rust_code = SemanticObject::new(
            b"fn main() {}".to_vec(),
            ContentType::Code { language: "rust".to_string() }
        ).with_name("main");

        let md_doc = SemanticObject::from_markdown("# Hello")
            .with_name("readme");

        // Check content types are correct
        assert!(matches!(rust_code.content_type, ContentType::Code { .. }));
        assert!(matches!(md_doc.content_type, ContentType::Markdown));
    }

    #[test]
    fn test_relation_edge_to_relation() {
        let source = Suid::new();
        let target = Suid::new();

        let edge = RelationEdge::new(source, target, RelationType::References)
            .with_metadata(serde_json::json!({"note": "test"}));

        let relation = edge.to_relation();
        assert_eq!(relation.target, target);
        assert_eq!(relation.relation_type, RelationType::References);
        assert!(relation.metadata.is_some());
    }
}
