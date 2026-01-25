"""
Vector Store Module
===================

MySQL-based vector storage for semantic search.
Stores embeddings and performs similarity search.

Features:
- MySQL storage with JSON embeddings
- Cosine similarity search
- Automatic table creation
- Batch operations
- Configurable for different embedding dimensions

Usage:
    store = VectorStore(mysql_config)
    store.add("doc1", "some text", {"type": "history"})
    results = store.search("query text", k=5)
"""

import json
import math
import time
from typing import List, Optional, Dict, Any, Tuple
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path

from .embeddings import SemanticEmbedder, EmbeddingConfig, get_embedder


@dataclass
class MySQLConfig:
    """MySQL connection configuration."""
    host: str = "localhost"
    port: int = 3306
    user: str = "root"
    password: str = ""
    database: str = "semantic_os"

    # Connection pool settings
    pool_size: int = 5
    pool_name: str = "semantic_pool"


@dataclass
class VectorRecord:
    """A record in the vector store."""
    id: str
    text: str
    embedding: List[float]
    metadata: Dict[str, Any]
    created_at: float
    collection: str = "default"


class VectorStore:
    """MySQL-based vector store for semantic search.

    Stores text with embeddings for semantic similarity search.
    Uses MySQL JSON columns for vector storage and computes
    similarity in Python for flexibility.

    For large-scale production, consider:
    - MySQL 9.0+ native vector support
    - pgvector (PostgreSQL)
    - Dedicated vector DBs

    Usage:
        config = MySQLConfig(database="my_app")
        store = VectorStore(config)

        # Add documents
        store.add("id1", "Hello world", {"source": "user"})

        # Search
        results = store.search("greeting", k=5)
        for doc_id, text, score, metadata in results:
            print(f"{doc_id}: {score:.3f} - {text}")
    """

    # SQL for table creation
    CREATE_TABLE_SQL = """
    CREATE TABLE IF NOT EXISTS {table} (
        id VARCHAR(255) PRIMARY KEY,
        collection VARCHAR(128) NOT NULL DEFAULT 'default',
        text TEXT NOT NULL,
        embedding JSON NOT NULL,
        metadata JSON,
        created_at DOUBLE NOT NULL,
        updated_at DOUBLE,

        INDEX idx_collection (collection),
        INDEX idx_created (created_at)
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci
    """

    def __init__(
        self,
        mysql_config: Optional[MySQLConfig] = None,
        embedder: Optional[SemanticEmbedder] = None,
        table_name: str = "vector_store",
        auto_create: bool = True
    ):
        self.mysql_config = mysql_config or MySQLConfig()
        self.embedder = embedder or get_embedder()
        self.table_name = table_name
        self._connection = None
        self._pool = None

        # Auto-create table
        if auto_create:
            self._ensure_table()

    def _get_connection(self):
        """Get MySQL connection (lazy initialization)."""
        if self._connection is not None:
            try:
                self._connection.ping(reconnect=True)
                return self._connection
            except Exception:
                self._connection = None

        # Try to import and connect
        try:
            import mysql.connector
            from mysql.connector import pooling
        except ImportError:
            self._install_mysql_connector()
            import mysql.connector
            from mysql.connector import pooling

        try:
            # Create connection pool
            self._pool = pooling.MySQLConnectionPool(
                pool_name=self.mysql_config.pool_name,
                pool_size=self.mysql_config.pool_size,
                host=self.mysql_config.host,
                port=self.mysql_config.port,
                user=self.mysql_config.user,
                password=self.mysql_config.password,
                database=self.mysql_config.database,
            )
            self._connection = self._pool.get_connection()
            return self._connection

        except mysql.connector.Error as e:
            # Try to create database if it doesn't exist
            if e.errno == 1049:  # Unknown database
                self._create_database()
                return self._get_connection()
            raise

    def _install_mysql_connector(self):
        """Auto-install mysql-connector-python."""
        import subprocess
        import sys

        print("[VectorStore] Installing mysql-connector-python...")
        result = subprocess.run(
            [sys.executable, "-m", "pip", "install", "mysql-connector-python"],
            capture_output=True,
            text=True,
            timeout=120
        )
        if result.returncode != 0:
            raise RuntimeError(f"Failed to install mysql-connector-python: {result.stderr}")
        print("[VectorStore] mysql-connector-python installed successfully")

    def _create_database(self):
        """Create the database if it doesn't exist."""
        import mysql.connector

        conn = mysql.connector.connect(
            host=self.mysql_config.host,
            port=self.mysql_config.port,
            user=self.mysql_config.user,
            password=self.mysql_config.password,
        )
        cursor = conn.cursor()
        cursor.execute(f"CREATE DATABASE IF NOT EXISTS {self.mysql_config.database}")
        cursor.close()
        conn.close()
        print(f"[VectorStore] Created database: {self.mysql_config.database}")

    def _ensure_table(self):
        """Ensure the vector store table exists."""
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            cursor.execute(self.CREATE_TABLE_SQL.format(table=self.table_name))
            conn.commit()
            cursor.close()
        except Exception as e:
            print(f"[VectorStore] Warning: Could not create table: {e}")

    def add(
        self,
        id: str,
        text: str,
        metadata: Optional[Dict[str, Any]] = None,
        collection: str = "default"
    ) -> bool:
        """Add a document to the vector store.

        Args:
            id: Unique document ID
            text: Text content to embed and store
            metadata: Optional metadata dictionary
            collection: Collection name for grouping

        Returns:
            True if successful
        """
        # Generate embedding
        embedding = self.embedder.embed(text)

        return self.add_with_embedding(id, text, embedding, metadata, collection)

    def add_with_embedding(
        self,
        id: str,
        text: str,
        embedding: List[float],
        metadata: Optional[Dict[str, Any]] = None,
        collection: str = "default"
    ) -> bool:
        """Add a document with pre-computed embedding."""
        conn = self._get_connection()
        cursor = conn.cursor()

        now = time.time()

        try:
            cursor.execute(
                f"""
                INSERT INTO {self.table_name}
                (id, collection, text, embedding, metadata, created_at, updated_at)
                VALUES (%s, %s, %s, %s, %s, %s, %s)
                ON DUPLICATE KEY UPDATE
                    text = VALUES(text),
                    embedding = VALUES(embedding),
                    metadata = VALUES(metadata),
                    updated_at = VALUES(updated_at)
                """,
                (
                    id,
                    collection,
                    text,
                    json.dumps(embedding),
                    json.dumps(metadata) if metadata else None,
                    now,
                    now
                )
            )
            conn.commit()
            return True

        except Exception as e:
            print(f"[VectorStore] Error adding document: {e}")
            conn.rollback()
            return False

        finally:
            cursor.close()

    def add_batch(
        self,
        documents: List[Tuple[str, str, Optional[Dict]]],
        collection: str = "default"
    ) -> int:
        """Add multiple documents efficiently.

        Args:
            documents: List of (id, text, metadata) tuples
            collection: Collection name

        Returns:
            Number of documents added
        """
        if not documents:
            return 0

        # Batch embed all texts
        texts = [doc[1] for doc in documents]
        embeddings = self.embedder.embed_batch(texts)

        conn = self._get_connection()
        cursor = conn.cursor()
        now = time.time()
        added = 0

        try:
            for (doc_id, text, metadata), embedding in zip(documents, embeddings):
                cursor.execute(
                    f"""
                    INSERT INTO {self.table_name}
                    (id, collection, text, embedding, metadata, created_at, updated_at)
                    VALUES (%s, %s, %s, %s, %s, %s, %s)
                    ON DUPLICATE KEY UPDATE
                        text = VALUES(text),
                        embedding = VALUES(embedding),
                        metadata = VALUES(metadata),
                        updated_at = VALUES(updated_at)
                    """,
                    (
                        doc_id,
                        collection,
                        text,
                        json.dumps(embedding),
                        json.dumps(metadata) if metadata else None,
                        now,
                        now
                    )
                )
                added += 1

            conn.commit()
            return added

        except Exception as e:
            print(f"[VectorStore] Error in batch add: {e}")
            conn.rollback()
            return added

        finally:
            cursor.close()

    def search(
        self,
        query: str,
        k: int = 5,
        collection: Optional[str] = None,
        min_score: float = 0.0,
        metadata_filter: Optional[Dict[str, Any]] = None
    ) -> List[Tuple[str, str, float, Dict]]:
        """Search for similar documents.

        Args:
            query: Query text
            k: Number of results to return
            collection: Filter by collection (None = all)
            min_score: Minimum similarity score (0-1)
            metadata_filter: Filter by metadata fields

        Returns:
            List of (id, text, score, metadata) tuples
        """
        # Embed query
        query_embedding = self.embedder.embed(query)

        return self.search_by_vector(
            query_embedding, k, collection, min_score, metadata_filter
        )

    def search_by_vector(
        self,
        query_embedding: List[float],
        k: int = 5,
        collection: Optional[str] = None,
        min_score: float = 0.0,
        metadata_filter: Optional[Dict[str, Any]] = None
    ) -> List[Tuple[str, str, float, Dict]]:
        """Search using a pre-computed embedding vector."""
        conn = self._get_connection()
        cursor = conn.cursor(dictionary=True)

        # Build query
        sql = f"SELECT id, text, embedding, metadata FROM {self.table_name}"
        conditions = []
        params = []

        if collection:
            conditions.append("collection = %s")
            params.append(collection)

        if conditions:
            sql += " WHERE " + " AND ".join(conditions)

        try:
            cursor.execute(sql, params)
            rows = cursor.fetchall()

            # Compute similarities in Python
            results = []
            for row in rows:
                embedding = json.loads(row['embedding'])
                score = self._cosine_similarity(query_embedding, embedding)

                if score >= min_score:
                    metadata = json.loads(row['metadata']) if row['metadata'] else {}

                    # Apply metadata filter
                    if metadata_filter:
                        if not self._matches_filter(metadata, metadata_filter):
                            continue

                    results.append((row['id'], row['text'], score, metadata))

            # Sort by score and return top k
            results.sort(key=lambda x: x[2], reverse=True)
            return results[:k]

        finally:
            cursor.close()

    def get(self, id: str) -> Optional[VectorRecord]:
        """Get a document by ID."""
        conn = self._get_connection()
        cursor = conn.cursor(dictionary=True)

        try:
            cursor.execute(
                f"SELECT * FROM {self.table_name} WHERE id = %s",
                (id,)
            )
            row = cursor.fetchone()

            if row:
                return VectorRecord(
                    id=row['id'],
                    text=row['text'],
                    embedding=json.loads(row['embedding']),
                    metadata=json.loads(row['metadata']) if row['metadata'] else {},
                    created_at=row['created_at'],
                    collection=row['collection']
                )
            return None

        finally:
            cursor.close()

    def delete(self, id: str) -> bool:
        """Delete a document by ID."""
        conn = self._get_connection()
        cursor = conn.cursor()

        try:
            cursor.execute(
                f"DELETE FROM {self.table_name} WHERE id = %s",
                (id,)
            )
            conn.commit()
            return cursor.rowcount > 0

        finally:
            cursor.close()

    def delete_collection(self, collection: str) -> int:
        """Delete all documents in a collection."""
        conn = self._get_connection()
        cursor = conn.cursor()

        try:
            cursor.execute(
                f"DELETE FROM {self.table_name} WHERE collection = %s",
                (collection,)
            )
            conn.commit()
            return cursor.rowcount

        finally:
            cursor.close()

    def count(self, collection: Optional[str] = None) -> int:
        """Count documents in store."""
        conn = self._get_connection()
        cursor = conn.cursor()

        try:
            if collection:
                cursor.execute(
                    f"SELECT COUNT(*) FROM {self.table_name} WHERE collection = %s",
                    (collection,)
                )
            else:
                cursor.execute(f"SELECT COUNT(*) FROM {self.table_name}")

            return cursor.fetchone()[0]

        finally:
            cursor.close()

    def list_collections(self) -> List[str]:
        """List all collections."""
        conn = self._get_connection()
        cursor = conn.cursor()

        try:
            cursor.execute(
                f"SELECT DISTINCT collection FROM {self.table_name}"
            )
            return [row[0] for row in cursor.fetchall()]

        finally:
            cursor.close()

    @staticmethod
    def _cosine_similarity(vec1: List[float], vec2: List[float]) -> float:
        """Compute cosine similarity between two vectors."""
        if len(vec1) != len(vec2):
            return 0.0

        dot = sum(a * b for a, b in zip(vec1, vec2))
        norm1 = math.sqrt(sum(a * a for a in vec1))
        norm2 = math.sqrt(sum(b * b for b in vec2))

        if norm1 == 0 or norm2 == 0:
            return 0.0

        return dot / (norm1 * norm2)

    @staticmethod
    def _matches_filter(metadata: Dict, filter_dict: Dict) -> bool:
        """Check if metadata matches filter criteria."""
        for key, value in filter_dict.items():
            if key not in metadata:
                return False
            if metadata[key] != value:
                return False
        return True

    def close(self):
        """Close database connections."""
        if self._connection:
            try:
                self._connection.close()
            except Exception:
                pass
            self._connection = None


class SQLiteVectorStore:
    """SQLite-based vector store for local/embedded use.

    No MySQL server required - stores everything in a local file.
    Good for development, single-user apps, or when MySQL isn't available.

    Usage:
        store = SQLiteVectorStore("./vectors.db")
        store.add("id1", "Hello world")
        results = store.search("greeting")
    """

    CREATE_TABLE_SQL = """
    CREATE TABLE IF NOT EXISTS vectors (
        id TEXT PRIMARY KEY,
        collection TEXT NOT NULL DEFAULT 'default',
        text TEXT NOT NULL,
        embedding TEXT NOT NULL,
        metadata TEXT,
        created_at REAL NOT NULL
    )
    """

    CREATE_INDEX_SQL = """
    CREATE INDEX IF NOT EXISTS idx_collection ON vectors(collection);
    CREATE INDEX IF NOT EXISTS idx_created ON vectors(created_at);
    """

    def __init__(
        self,
        db_path: str = "vectors.db",
        embedder: Optional[SemanticEmbedder] = None
    ):
        self.db_path = db_path
        self.embedder = embedder or get_embedder()
        self._connection = None
        self._ensure_table()

    def _get_connection(self):
        """Get SQLite connection."""
        if self._connection is None:
            import sqlite3
            self._connection = sqlite3.connect(self.db_path)
            self._connection.row_factory = sqlite3.Row
        return self._connection

    def _ensure_table(self):
        """Create table if not exists."""
        conn = self._get_connection()
        conn.execute(self.CREATE_TABLE_SQL)
        conn.executescript(self.CREATE_INDEX_SQL)
        conn.commit()

    def add(
        self,
        id: str,
        text: str,
        metadata: Optional[Dict] = None,
        collection: str = "default"
    ) -> bool:
        """Add a document."""
        embedding = self.embedder.embed(text)
        conn = self._get_connection()

        try:
            conn.execute(
                """
                INSERT OR REPLACE INTO vectors
                (id, collection, text, embedding, metadata, created_at)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                (
                    id,
                    collection,
                    text,
                    json.dumps(embedding),
                    json.dumps(metadata) if metadata else None,
                    time.time()
                )
            )
            conn.commit()
            return True
        except Exception as e:
            print(f"[SQLiteVectorStore] Error: {e}")
            return False

    def search(
        self,
        query: str,
        k: int = 5,
        collection: Optional[str] = None,
        min_score: float = 0.0
    ) -> List[Tuple[str, str, float, Dict]]:
        """Search for similar documents."""
        query_embedding = self.embedder.embed(query)
        conn = self._get_connection()

        sql = "SELECT id, text, embedding, metadata FROM vectors"
        params = []

        if collection:
            sql += " WHERE collection = ?"
            params.append(collection)

        cursor = conn.execute(sql, params)
        rows = cursor.fetchall()

        results = []
        for row in rows:
            embedding = json.loads(row['embedding'])
            score = VectorStore._cosine_similarity(query_embedding, embedding)

            if score >= min_score:
                metadata = json.loads(row['metadata']) if row['metadata'] else {}
                results.append((row['id'], row['text'], score, metadata))

        results.sort(key=lambda x: x[2], reverse=True)
        return results[:k]

    def delete(self, id: str) -> bool:
        """Delete a document."""
        conn = self._get_connection()
        cursor = conn.execute("DELETE FROM vectors WHERE id = ?", (id,))
        conn.commit()
        return cursor.rowcount > 0

    def count(self, collection: Optional[str] = None) -> int:
        """Count documents."""
        conn = self._get_connection()
        if collection:
            cursor = conn.execute(
                "SELECT COUNT(*) FROM vectors WHERE collection = ?",
                (collection,)
            )
        else:
            cursor = conn.execute("SELECT COUNT(*) FROM vectors")
        return cursor.fetchone()[0]

    def close(self):
        """Close connection."""
        if self._connection:
            self._connection.close()
            self._connection = None


# Factory function to get appropriate store
def get_vector_store(
    use_mysql: bool = False,
    mysql_config: Optional[MySQLConfig] = None,
    sqlite_path: str = "vectors.db",
    embedder: Optional[SemanticEmbedder] = None
):
    """Get a vector store instance.

    Args:
        use_mysql: If True, use MySQL; otherwise use SQLite
        mysql_config: MySQL configuration (if using MySQL)
        sqlite_path: SQLite database path (if using SQLite)
        embedder: Optional custom embedder

    Returns:
        VectorStore or SQLiteVectorStore instance
    """
    if use_mysql:
        return VectorStore(mysql_config, embedder)
    else:
        return SQLiteVectorStore(sqlite_path, embedder)
