"""
Semantic Embeddings Module
==========================

Provides text-to-vector embeddings for semantic search.
Supports multiple embedding models with lazy loading and auto-installation.

Features:
- Auto-installs dependencies if missing
- Lazy loads models (only when first used)
- Configurable model selection
- Graceful fallback if ML unavailable
- Fine-tuning capability (future)

Usage:
    embedder = SemanticEmbedder()
    vector = embedder.embed("some text")
    vectors = embedder.embed_batch(["text1", "text2"])
    similarity = embedder.similarity(vec1, vec2)
"""

import subprocess
import sys
import math
import hashlib
from typing import List, Optional, Dict, Any
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path


class EmbeddingModel(str, Enum):
    """Available embedding models."""
    # Local models (sentence-transformers)
    MINILM = "all-MiniLM-L6-v2"           # 80MB, fast, good quality
    MPNET = "all-mpnet-base-v2"            # 420MB, slower, better quality
    E5_SMALL = "intfloat/e5-small-v2"      # 130MB, good for retrieval
    BGE_SMALL = "BAAI/bge-small-en-v1.5"   # 130MB, good quality

    # Fallback (no ML dependencies)
    HASH_TFIDF = "hash-tfidf"              # Zero dependencies, keyword-based


@dataclass
class EmbeddingConfig:
    """Configuration for the embedding system."""
    model: EmbeddingModel = EmbeddingModel.MINILM
    auto_install: bool = True
    cache_dir: Optional[str] = None  # Model cache directory
    device: str = "cpu"  # "cpu" or "cuda"
    normalize: bool = True  # L2 normalize embeddings

    # Dimension override (for models that support it)
    dimensions: Optional[int] = None

    # Future: fine-tuning config
    fine_tune_data: Optional[str] = None


class DependencyInstaller:
    """Handles auto-installation of ML dependencies."""

    PACKAGES = {
        "sentence-transformers": "sentence-transformers",
        "torch": "torch --index-url https://download.pytorch.org/whl/cpu",
    }

    @staticmethod
    def is_installed(package: str) -> bool:
        """Check if a package is installed."""
        try:
            __import__(package.replace("-", "_"))
            return True
        except ImportError:
            return False

    @staticmethod
    def install(package: str, pip_name: Optional[str] = None) -> bool:
        """Install a package using pip."""
        pip_package = pip_name or DependencyInstaller.PACKAGES.get(package, package)

        print(f"[Embeddings] Installing {package}...")
        try:
            # Use subprocess to install
            cmd = [sys.executable, "-m", "pip", "install"] + pip_package.split()
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=300  # 5 min timeout for large packages
            )

            if result.returncode == 0:
                print(f"[Embeddings] Successfully installed {package}")
                return True
            else:
                print(f"[Embeddings] Failed to install {package}: {result.stderr}")
                return False

        except subprocess.TimeoutExpired:
            print(f"[Embeddings] Installation timed out for {package}")
            return False
        except Exception as e:
            print(f"[Embeddings] Installation error: {e}")
            return False

    @classmethod
    def ensure_dependencies(cls, auto_install: bool = True) -> bool:
        """Ensure all ML dependencies are available."""
        # Check torch first
        if not cls.is_installed("torch"):
            if auto_install:
                if not cls.install("torch"):
                    return False
            else:
                return False

        # Check sentence-transformers
        if not cls.is_installed("sentence_transformers"):
            if auto_install:
                if not cls.install("sentence-transformers"):
                    return False
            else:
                return False

        return True


class FallbackEmbedder:
    """Zero-dependency fallback using feature hashing.

    Not truly semantic but provides basic similarity for keyword matching.
    Used when ML dependencies aren't available.
    """

    DIM = 384  # Match MiniLM dimensions for compatibility

    def embed(self, text: str) -> List[float]:
        """Create hash-based embedding."""
        words = text.lower().split()

        # Feature hashing with TF weighting
        vec = [0.0] * self.DIM
        word_counts: Dict[str, int] = {}

        for word in words:
            word_counts[word] = word_counts.get(word, 0) + 1

        for word, count in word_counts.items():
            # Hash to get bucket and sign
            h = int(hashlib.sha256(word.encode()).hexdigest(), 16)
            idx = h % self.DIM
            sign = 1 if (h >> 8) % 2 == 0 else -1

            # TF weighting: log(1 + count)
            weight = math.log1p(count)
            vec[idx] += sign * weight

        # L2 normalize
        norm = math.sqrt(sum(x * x for x in vec)) or 1.0
        return [x / norm for x in vec]

    def embed_batch(self, texts: List[str]) -> List[List[float]]:
        """Batch embedding."""
        return [self.embed(t) for t in texts]


class SemanticEmbedder:
    """Main embedding interface with lazy loading and auto-install.

    Usage:
        embedder = SemanticEmbedder()

        # Single text
        vec = embedder.embed("Hello world")

        # Batch (more efficient)
        vecs = embedder.embed_batch(["text1", "text2", "text3"])

        # Similarity
        sim = embedder.similarity(vec1, vec2)
    """

    def __init__(self, config: Optional[EmbeddingConfig] = None):
        self.config = config or EmbeddingConfig()
        self._model = None
        self._model_loaded = False
        self._using_fallback = False
        self._dimensions: Optional[int] = None

    @property
    def dimensions(self) -> int:
        """Get embedding dimensions (loads model if needed)."""
        if self._dimensions is None:
            # Trigger model load to get dimensions
            self._ensure_model()
        return self._dimensions or 384

    @property
    def is_semantic(self) -> bool:
        """Check if using true semantic model (not fallback)."""
        self._ensure_model()
        return not self._using_fallback

    @property
    def model_name(self) -> str:
        """Get current model name."""
        return self.config.model.value

    def _ensure_model(self):
        """Lazy-load the embedding model."""
        if self._model_loaded:
            return

        # Try hash-based fallback first if explicitly selected
        if self.config.model == EmbeddingModel.HASH_TFIDF:
            self._model = FallbackEmbedder()
            self._using_fallback = True
            self._dimensions = FallbackEmbedder.DIM
            self._model_loaded = True
            print("[Embeddings] Using hash-based fallback (no ML)")
            return

        # Try to load ML model
        if DependencyInstaller.ensure_dependencies(self.config.auto_install):
            try:
                from sentence_transformers import SentenceTransformer

                print(f"[Embeddings] Loading model: {self.config.model.value}")

                # Load model with optional cache directory
                model_kwargs = {}
                if self.config.cache_dir:
                    model_kwargs['cache_folder'] = self.config.cache_dir

                self._model = SentenceTransformer(
                    self.config.model.value,
                    device=self.config.device,
                    **model_kwargs
                )

                # Get dimensions from model
                self._dimensions = self._model.get_sentence_embedding_dimension()
                self._using_fallback = False
                self._model_loaded = True

                print(f"[Embeddings] Model loaded: {self._dimensions}D vectors")
                return

            except Exception as e:
                print(f"[Embeddings] Failed to load model: {e}")

        # Fall back to hash-based
        print("[Embeddings] Falling back to hash-based embeddings")
        self._model = FallbackEmbedder()
        self._using_fallback = True
        self._dimensions = FallbackEmbedder.DIM
        self._model_loaded = True

    def embed(self, text: str) -> List[float]:
        """Embed a single text string.

        Args:
            text: The text to embed

        Returns:
            List of floats representing the embedding vector
        """
        self._ensure_model()

        if self._using_fallback:
            return self._model.embed(text)

        # Use sentence-transformers
        embedding = self._model.encode(
            text,
            normalize_embeddings=self.config.normalize,
            convert_to_numpy=True
        )
        return embedding.tolist()

    def embed_batch(self, texts: List[str]) -> List[List[float]]:
        """Embed multiple texts efficiently.

        Args:
            texts: List of texts to embed

        Returns:
            List of embedding vectors
        """
        self._ensure_model()

        if not texts:
            return []

        if self._using_fallback:
            return self._model.embed_batch(texts)

        # Batch encode with sentence-transformers
        embeddings = self._model.encode(
            texts,
            normalize_embeddings=self.config.normalize,
            convert_to_numpy=True,
            show_progress_bar=len(texts) > 100
        )
        return embeddings.tolist()

    @staticmethod
    def similarity(vec1: List[float], vec2: List[float]) -> float:
        """Compute cosine similarity between two vectors.

        Args:
            vec1: First embedding vector
            vec2: Second embedding vector

        Returns:
            Similarity score between -1 and 1 (1 = identical)
        """
        if len(vec1) != len(vec2):
            raise ValueError(f"Vector dimensions don't match: {len(vec1)} vs {len(vec2)}")

        dot = sum(a * b for a, b in zip(vec1, vec2))
        norm1 = math.sqrt(sum(a * a for a in vec1))
        norm2 = math.sqrt(sum(b * b for b in vec2))

        if norm1 == 0 or norm2 == 0:
            return 0.0

        return dot / (norm1 * norm2)

    @staticmethod
    def top_k_similar(
        query_vec: List[float],
        candidates: List[List[float]],
        k: int = 5
    ) -> List[tuple]:
        """Find top-k most similar vectors.

        Args:
            query_vec: Query embedding
            candidates: List of candidate embeddings
            k: Number of results to return

        Returns:
            List of (index, similarity) tuples, sorted by similarity descending
        """
        similarities = [
            (i, SemanticEmbedder.similarity(query_vec, cand))
            for i, cand in enumerate(candidates)
        ]
        similarities.sort(key=lambda x: x[1], reverse=True)
        return similarities[:k]

    def change_model(self, model: EmbeddingModel):
        """Change to a different embedding model.

        Note: This resets the loaded model. Existing embeddings from
        the old model won't be compatible with the new one.
        """
        if model != self.config.model:
            self.config.model = model
            self._model = None
            self._model_loaded = False
            self._using_fallback = False
            self._dimensions = None

    def unload(self):
        """Unload the model to free memory."""
        self._model = None
        self._model_loaded = False
        # Keep config so it can be reloaded


# Global embedder instance (lazy initialized)
_embedder: Optional[SemanticEmbedder] = None


def get_embedder(config: Optional[EmbeddingConfig] = None) -> SemanticEmbedder:
    """Get the global embedder instance."""
    global _embedder
    if _embedder is None:
        _embedder = SemanticEmbedder(config)
    return _embedder


def configure_embedder(config: EmbeddingConfig):
    """Configure the global embedder."""
    global _embedder
    _embedder = SemanticEmbedder(config)


def embed(text: str) -> List[float]:
    """Convenience function to embed text using global embedder."""
    return get_embedder().embed(text)


def embed_batch(texts: List[str]) -> List[List[float]]:
    """Convenience function to batch embed using global embedder."""
    return get_embedder().embed_batch(texts)


def similarity(vec1: List[float], vec2: List[float]) -> float:
    """Convenience function for similarity using global embedder."""
    return SemanticEmbedder.similarity(vec1, vec2)
