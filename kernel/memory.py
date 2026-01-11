"""
Semantic Memory Pool
====================

All OS state is stored as vectors in a unified memory pool.
No filesystem, no process tables, no separate stores - just vectors.

Every "thing" in the system is:
    vector = embed(content)
    metadata = {type, timestamp, relations, ...}

Query anything by meaning.
"""

import time
import uuid
import json
import sqlite3
import threading
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Tuple, Union
from enum import Enum
from pathlib import Path


class MemoryType(str, Enum):
    """Types of entries in semantic memory."""
    DOCUMENT = "document"       # File/document content
    CHUNK = "chunk"             # Document chunk (for large docs)
    EVENT = "event"             # Semantic event
    HANDLE = "handle"           # Open file handle
    PROCESS = "process"         # Process context
    LINK = "link"               # Relationship between entries
    SNAPSHOT = "snapshot"       # Point-in-time state
    QUERY = "query"             # Cached query


@dataclass
class MemoryEntry:
    """An entry in semantic memory."""
    id: str                                 # Unique ID
    type: MemoryType                        # Entry type
    content: str                            # Text content (for embedding)
    vector: Optional[List[float]] = None    # Embedding vector

    # Metadata
    metadata: Dict[str, Any] = field(default_factory=dict)
    created_at: float = field(default_factory=time.time)
    updated_at: float = field(default_factory=time.time)

    # For versioning
    version: int = 1
    parent_id: Optional[str] = None         # Previous version

    # For relationships
    links: List[str] = field(default_factory=list)  # Related entry IDs

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "type": self.type.value,
            "content": self.content,
            "vector": self.vector,
            "metadata": self.metadata,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "version": self.version,
            "parent_id": self.parent_id,
            "links": self.links,
        }

    @classmethod
    def from_dict(cls, data: dict) -> 'MemoryEntry':
        return cls(
            id=data["id"],
            type=MemoryType(data["type"]),
            content=data["content"],
            vector=data.get("vector"),
            metadata=data.get("metadata", {}),
            created_at=data.get("created_at", time.time()),
            updated_at=data.get("updated_at", time.time()),
            version=data.get("version", 1),
            parent_id=data.get("parent_id"),
            links=data.get("links", []),
        )


class SemanticMemory:
    """Unified semantic memory pool.

    All OS state stored as vectors. Query by meaning.

    Usage:
        memory = SemanticMemory()

        # Store something
        entry_id = memory.store(
            content="Project design document for new feature",
            type=MemoryType.DOCUMENT,
            metadata={"path": "design.md", "app": "vscode"}
        )

        # Query by meaning
        results = memory.query("design documents")

        # Get related entries
        related = memory.get_related(entry_id)
    """

    def __init__(self, db_path: str = None, embedder=None):
        """Initialize semantic memory.

        Args:
            db_path: Path to SQLite database (None for in-memory)
            embedder: Embedding model (lazy-loaded if None)
        """
        self.db_path = db_path or ":memory:"
        self._embedder = embedder
        self._lock = threading.RLock()

        # Initialize database
        self._init_db()

        # In-memory cache for hot entries
        self._cache: Dict[str, MemoryEntry] = {}
        self._cache_limit = 1000

    def _init_db(self):
        """Initialize SQLite database."""
        self.conn = sqlite3.connect(self.db_path, check_same_thread=False)
        self.conn.row_factory = sqlite3.Row

        self.conn.executescript("""
            CREATE TABLE IF NOT EXISTS memory (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                content TEXT NOT NULL,
                vector BLOB,
                metadata TEXT,
                created_at REAL,
                updated_at REAL,
                version INTEGER DEFAULT 1,
                parent_id TEXT,
                links TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_memory_type ON memory(type);
            CREATE INDEX IF NOT EXISTS idx_memory_created ON memory(created_at);
            CREATE INDEX IF NOT EXISTS idx_memory_parent ON memory(parent_id);

            -- Full-text search for fast keyword matching
            CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                id, content, metadata,
                content='memory',
                content_rowid='rowid'
            );

            -- Triggers to keep FTS in sync
            CREATE TRIGGER IF NOT EXISTS memory_ai AFTER INSERT ON memory BEGIN
                INSERT INTO memory_fts(id, content, metadata)
                VALUES (new.id, new.content, new.metadata);
            END;

            CREATE TRIGGER IF NOT EXISTS memory_ad AFTER DELETE ON memory BEGIN
                DELETE FROM memory_fts WHERE id = old.id;
            END;

            CREATE TRIGGER IF NOT EXISTS memory_au AFTER UPDATE ON memory BEGIN
                DELETE FROM memory_fts WHERE id = old.id;
                INSERT INTO memory_fts(id, content, metadata)
                VALUES (new.id, new.content, new.metadata);
            END;
        """)
        self.conn.commit()

    @property
    def embedder(self):
        """Lazy-load embedder."""
        if self._embedder is None:
            try:
                from ide.embeddings import SemanticEmbedder
                self._embedder = SemanticEmbedder()
            except ImportError:
                # Fallback: no embeddings, use FTS only
                self._embedder = None
        return self._embedder

    def _embed(self, text: str) -> Optional[List[float]]:
        """Generate embedding for text."""
        if self.embedder is None:
            return None
        try:
            return self.embedder.embed(text)
        except Exception:
            return None

    def _serialize_vector(self, vector: List[float]) -> bytes:
        """Serialize vector to bytes."""
        import struct
        return struct.pack(f'{len(vector)}f', *vector)

    def _deserialize_vector(self, data: bytes) -> List[float]:
        """Deserialize vector from bytes."""
        import struct
        count = len(data) // 4
        return list(struct.unpack(f'{count}f', data))

    def store(
        self,
        content: str,
        type: MemoryType,
        metadata: Dict[str, Any] = None,
        id: str = None,
        parent_id: str = None,
        links: List[str] = None,
    ) -> str:
        """Store content in semantic memory.

        Args:
            content: Text content to store
            type: Type of entry
            metadata: Additional metadata
            id: Optional ID (generated if not provided)
            parent_id: ID of parent/previous version
            links: Related entry IDs

        Returns:
            Entry ID
        """
        entry_id = id or str(uuid.uuid4())
        now = time.time()

        # Get version number
        version = 1
        if parent_id:
            parent = self.get(parent_id)
            if parent:
                version = parent.version + 1

        # Generate embedding
        vector = self._embed(content)

        entry = MemoryEntry(
            id=entry_id,
            type=type,
            content=content,
            vector=vector,
            metadata=metadata or {},
            created_at=now,
            updated_at=now,
            version=version,
            parent_id=parent_id,
            links=links or [],
        )

        # Store in database
        with self._lock:
            vector_blob = self._serialize_vector(vector) if vector else None

            self.conn.execute("""
                INSERT OR REPLACE INTO memory
                (id, type, content, vector, metadata, created_at, updated_at, version, parent_id, links)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """, (
                entry.id,
                entry.type.value,
                entry.content,
                vector_blob,
                json.dumps(entry.metadata),
                entry.created_at,
                entry.updated_at,
                entry.version,
                entry.parent_id,
                json.dumps(entry.links),
            ))
            self.conn.commit()

            # Update cache
            self._cache[entry_id] = entry
            if len(self._cache) > self._cache_limit:
                # Remove oldest entries
                oldest = sorted(self._cache.items(), key=lambda x: x[1].updated_at)
                for k, _ in oldest[:100]:
                    del self._cache[k]

        return entry_id

    def get(self, id: str) -> Optional[MemoryEntry]:
        """Get entry by ID."""
        # Check cache first
        if id in self._cache:
            return self._cache[id]

        with self._lock:
            row = self.conn.execute(
                "SELECT * FROM memory WHERE id = ?", (id,)
            ).fetchone()

            if not row:
                return None

            entry = self._row_to_entry(row)
            self._cache[id] = entry
            return entry

    def _row_to_entry(self, row: sqlite3.Row) -> MemoryEntry:
        """Convert database row to MemoryEntry."""
        vector = None
        if row["vector"]:
            vector = self._deserialize_vector(row["vector"])

        return MemoryEntry(
            id=row["id"],
            type=MemoryType(row["type"]),
            content=row["content"],
            vector=vector,
            metadata=json.loads(row["metadata"]) if row["metadata"] else {},
            created_at=row["created_at"],
            updated_at=row["updated_at"],
            version=row["version"],
            parent_id=row["parent_id"],
            links=json.loads(row["links"]) if row["links"] else [],
        )

    def update(self, id: str, content: str = None, metadata: Dict = None) -> bool:
        """Update an entry (creates new version).

        Args:
            id: Entry ID
            content: New content (optional)
            metadata: New metadata (optional, merged with existing)

        Returns:
            Success
        """
        existing = self.get(id)
        if not existing:
            return False

        new_content = content if content is not None else existing.content
        new_metadata = {**existing.metadata, **(metadata or {})}

        # Store as new version
        self.store(
            content=new_content,
            type=existing.type,
            metadata=new_metadata,
            id=id,
            parent_id=existing.parent_id or existing.id,
            links=existing.links,
        )
        return True

    def delete(self, id: str) -> bool:
        """Delete entry (soft delete - marks as deleted)."""
        return self.update(id, metadata={"deleted": True, "deleted_at": time.time()})

    def query(
        self,
        query: str,
        type: MemoryType = None,
        limit: int = 10,
        threshold: float = 0.5,
        metadata_filter: Dict[str, Any] = None,
    ) -> List[Tuple[MemoryEntry, float]]:
        """Query memory by semantic similarity.

        Args:
            query: Search query (natural language)
            type: Filter by entry type
            limit: Maximum results
            threshold: Minimum similarity score
            metadata_filter: Filter by metadata fields

        Returns:
            List of (entry, score) tuples
        """
        results = []

        # Try vector similarity first
        query_vector = self._embed(query)

        if query_vector:
            results = self._vector_search(query_vector, type, limit * 2, metadata_filter)
        else:
            # Fall back to FTS
            results = self._fts_search(query, type, limit * 2, metadata_filter)

        # Filter by threshold and limit
        results = [(e, s) for e, s in results if s >= threshold]
        results = results[:limit]

        return results

    def _vector_search(
        self,
        query_vector: List[float],
        type: MemoryType = None,
        limit: int = 20,
        metadata_filter: Dict = None,
    ) -> List[Tuple[MemoryEntry, float]]:
        """Search by vector similarity."""
        results = []

        with self._lock:
            # Build query
            sql = "SELECT * FROM memory WHERE vector IS NOT NULL"
            params = []

            if type:
                sql += " AND type = ?"
                params.append(type.value)

            # Exclude deleted
            sql += " AND (metadata NOT LIKE '%\"deleted\": true%' OR metadata IS NULL)"

            rows = self.conn.execute(sql, params).fetchall()

        # Calculate similarities
        for row in rows:
            entry = self._row_to_entry(row)

            # Check metadata filter
            if metadata_filter:
                match = all(
                    entry.metadata.get(k) == v
                    for k, v in metadata_filter.items()
                )
                if not match:
                    continue

            # Calculate cosine similarity
            if entry.vector:
                score = self._cosine_similarity(query_vector, entry.vector)
                results.append((entry, score))

        # Sort by score
        results.sort(key=lambda x: x[1], reverse=True)
        return results[:limit]

    def _fts_search(
        self,
        query: str,
        type: MemoryType = None,
        limit: int = 20,
        metadata_filter: Dict = None,
    ) -> List[Tuple[MemoryEntry, float]]:
        """Search by full-text search."""
        results = []

        with self._lock:
            # FTS query
            rows = self.conn.execute("""
                SELECT m.*, bm25(memory_fts) as score
                FROM memory_fts fts
                JOIN memory m ON fts.id = m.id
                WHERE memory_fts MATCH ?
                ORDER BY score
                LIMIT ?
            """, (query, limit * 2)).fetchall()

        for row in rows:
            entry = self._row_to_entry(row)

            # Check type filter
            if type and entry.type != type:
                continue

            # Check metadata filter
            if metadata_filter:
                match = all(
                    entry.metadata.get(k) == v
                    for k, v in metadata_filter.items()
                )
                if not match:
                    continue

            # Check not deleted
            if entry.metadata.get("deleted"):
                continue

            # BM25 scores are negative, normalize to 0-1
            score = 1.0 / (1.0 + abs(row["score"]))
            results.append((entry, score))

        return results[:limit]

    def _cosine_similarity(self, a: List[float], b: List[float]) -> float:
        """Calculate cosine similarity between vectors."""
        dot = sum(x * y for x, y in zip(a, b))
        norm_a = sum(x * x for x in a) ** 0.5
        norm_b = sum(x * x for x in b) ** 0.5
        if norm_a == 0 or norm_b == 0:
            return 0.0
        return dot / (norm_a * norm_b)

    def get_related(self, id: str, depth: int = 1) -> List[MemoryEntry]:
        """Get entries related to the given entry."""
        entry = self.get(id)
        if not entry:
            return []

        related = []
        seen = {id}

        def collect(entry_id: str, current_depth: int):
            if current_depth > depth:
                return
            e = self.get(entry_id)
            if not e:
                return
            for link_id in e.links:
                if link_id not in seen:
                    seen.add(link_id)
                    linked = self.get(link_id)
                    if linked:
                        related.append(linked)
                        collect(link_id, current_depth + 1)

        collect(id, 1)
        return related

    def get_history(self, id: str) -> List[MemoryEntry]:
        """Get version history of an entry."""
        history = []
        current = self.get(id)

        while current:
            history.append(current)
            if current.parent_id:
                current = self.get(current.parent_id)
            else:
                break

        return history

    def link(self, source_id: str, target_id: str, relation: str = None) -> bool:
        """Create a link between entries."""
        source = self.get(source_id)
        target = self.get(target_id)

        if not source or not target:
            return False

        # Add link to source
        if target_id not in source.links:
            source.links.append(target_id)
            self.update(source_id, metadata={"links": source.links})

        # Optionally create explicit link entry
        if relation:
            self.store(
                content=f"{source_id} {relation} {target_id}",
                type=MemoryType.LINK,
                metadata={
                    "source": source_id,
                    "target": target_id,
                    "relation": relation,
                },
                links=[source_id, target_id],
            )

        return True

    def get_by_type(self, type: MemoryType, limit: int = 100) -> List[MemoryEntry]:
        """Get all entries of a type."""
        with self._lock:
            rows = self.conn.execute("""
                SELECT * FROM memory
                WHERE type = ?
                AND (metadata NOT LIKE '%"deleted": true%' OR metadata IS NULL)
                ORDER BY updated_at DESC
                LIMIT ?
            """, (type.value, limit)).fetchall()

        return [self._row_to_entry(row) for row in rows]

    def get_by_metadata(self, key: str, value: Any) -> List[MemoryEntry]:
        """Get entries by metadata field."""
        # This is a simple implementation; could be optimized with JSON indexing
        with self._lock:
            rows = self.conn.execute("""
                SELECT * FROM memory
                WHERE metadata LIKE ?
                AND (metadata NOT LIKE '%"deleted": true%' OR metadata IS NULL)
            """, (f'%"{key}": {json.dumps(value)}%',)).fetchall()

        return [self._row_to_entry(row) for row in rows]

    def stats(self) -> Dict[str, Any]:
        """Get memory statistics."""
        with self._lock:
            total = self.conn.execute("SELECT COUNT(*) FROM memory").fetchone()[0]
            by_type = {}
            for row in self.conn.execute(
                "SELECT type, COUNT(*) FROM memory GROUP BY type"
            ).fetchall():
                by_type[row[0]] = row[1]

        return {
            "total_entries": total,
            "by_type": by_type,
            "cache_size": len(self._cache),
            "has_embedder": self.embedder is not None,
        }

    def close(self):
        """Close database connection."""
        self.conn.close()
