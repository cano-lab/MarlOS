# Semantic Search Call Graph

## Overview
This documents the complete function call chain when performing a semantic search in MarlOS.

## Entry Point
- **Frontend**: `invoke("object_search", {...})`
- **Command**: `commands.rs:816`

## Call Graph

```
Frontend: invoke("object_search", {...})
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ commands.rs:816                                                          │
│ object_search(query, limit, min_score, tier)                            │
│     │                                                                    │
│     │ Creates SearchOptions from params                                  │
│     ▼                                                                    │
│ semantic_search.search(query, options) ─────────────────────────────┐   │
└─────────────────────────────────────────────────────────────────────│───┘
                                                                      │
     ┌────────────────────────────────────────────────────────────────┘
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ semantic_search.rs:251                                                   │
│ search(&self, query, options) -> Vec<SearchHit>                         │
│     │                                                                    │
│     ├──► embeddings.embed(query) ──────────────────────────────────┐    │
│     │                                                               │    │
│     │    ┌──────────────────────────────────────────────────────────┘    │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ embeddings.rs:450                                           │  │
│     │  │ EmbeddingManager.embed(text) -> Vec<f32>                    │  │
│     │  │     │                                                       │  │
│     │  │     ▼                                                       │  │
│     │  │ provider.embed(text) ───────────────────────────────────┐   │  │
│     │  │                                                          │   │  │
│     │  │    ┌─────────────────────────────────────────────────────┘   │  │
│     │  │    ▼                                                         │  │
│     │  │  ┌───────────────────────────────────────────────────────┐   │  │
│     │  │  │ embeddings.rs:273 (OpenAIEmbedding)                   │   │  │
│     │  │  │ embed_batch([text]) -> HTTP POST                      │   │  │
│     │  │  │     │                                                 │   │  │
│     │  │  │     ▼                                                 │   │  │
│     │  │  │ LM Studio localhost:1234/v1/embeddings                │   │  │
│     │  │  │ Model: qwen3-embedding-0.6b                           │   │  │
│     │  │  │     │                                                 │   │  │
│     │  │  │     ▼                                                 │   │  │
│     │  │  │ Returns: [f32; 1024]                                  │   │  │
│     │  │  └───────────────────────────────────────────────────────┘   │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     ├──► store.get_objects_with_embeddings(tier) ──────────────────┐    │
│     │                                                               │    │
│     │    ┌──────────────────────────────────────────────────────────┘    │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ object_store.rs:683                                         │  │
│     │  │ get_objects_with_embeddings(max_tier)                       │  │
│     │  │     │                                                       │  │
│     │  │     ▼                                                       │  │
│     │  │ SQL: SELECT o.*, e.embedding                                │  │
│     │  │      FROM objects o                                         │  │
│     │  │      INNER JOIN embeddings e ON o.suid = e.suid             │  │
│     │  │      WHERE o.security_tier <= max_tier                      │  │
│     │  │     │                                                       │  │
│     │  │     ▼                                                       │  │
│     │  │ Returns: Vec<(SemanticObject, Vec<f32>)>                    │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     ├──► for each (obj, embedding):                                      │
│     │        cosine_similarity(query_vec, obj_vec) ────────────────┐    │
│     │                                                               │    │
│     │    ┌──────────────────────────────────────────────────────────┘    │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ embeddings.rs:470                                           │  │
│     │  │ cosine_similarity(a, b) -> f32                              │  │
│     │  │     │                                                       │  │
│     │  │     ▼                                                       │  │
│     │  │ dot = Σ(a[i] * b[i])                                        │  │
│     │  │ mag_a = √(Σ(a[i]²))                                         │  │
│     │  │ mag_b = √(Σ(b[i]²))                                         │  │
│     │  │ return dot / (mag_a * mag_b)                                │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     ├──► if options.include_keyword:                                     │
│     │        search_keyword(query, options) ───────────────────────┐    │
│     │                                                               │    │
│     │    ┌──────────────────────────────────────────────────────────┘    │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ semantic_search.rs:302                                      │  │
│     │  │ search_keyword(query, options) -> Vec<SearchHit>            │  │
│     │  │     │                                                       │  │
│     │  │     ▼                                                       │  │
│     │  │ store.search_text(query, limit) ──────────────────────┐     │  │
│     │  │                                                        │     │  │
│     │  │    ┌───────────────────────────────────────────────────┘     │  │
│     │  │    ▼                                                         │  │
│     │  │  ┌───────────────────────────────────────────────────────┐   │  │
│     │  │  │ object_store.rs:372                                   │   │  │
│     │  │  │ search_text(query, limit)                             │   │  │
│     │  │  │     │                                                 │   │  │
│     │  │  │     ▼                                                 │   │  │
│     │  │  │ SQL: SELECT * FROM objects                            │   │  │
│     │  │  │      WHERE name LIKE '%query%'                        │   │  │
│     │  │  │         OR summary LIKE '%query%'                     │   │  │
│     │  │  │         OR tags LIKE '%query%'                        │   │  │
│     │  │  └───────────────────────────────────────────────────────┘   │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     ├──► Merge: if found by both semantic + keyword:                     │
│     │        score += keyword_boost (0.2)                                │
│     │        match_type = Hybrid                                         │
│     │                                                                    │
│     ├──► hits.sort_by(|a,b| b.score.cmp(&a.score))                       │
│     │                                                                    │
│     ├──► hits.filter(|h| h.score >= min_score)                           │
│     │                                                                    │
│     └──► hits.truncate(limit)                                            │
│                                                                          │
│     Returns: Vec<SearchHit { object, score, match_type }>                │
└──────────────────────────────────────────────────────────────────────────┘
```

## Key Functions

| Function | File:Line | Purpose |
|----------|-----------|---------|
| `object_search` | commands.rs:816 | Tauri command entry point |
| `SemanticSearch::search` | semantic_search.rs:251 | Orchestrates search |
| `EmbeddingManager::embed` | embeddings.rs:450 | Converts text to vector |
| `OpenAIEmbedding::embed_batch` | embeddings.rs:273 | HTTP call to LM Studio |
| `get_objects_with_embeddings` | object_store.rs:683 | Loads all vectors from SQLite |
| `cosine_similarity` | embeddings.rs:470 | Computes similarity score |
| `search_keyword` | semantic_search.rs:302 | Text-based fallback search |
| `search_text` | object_store.rs:372 | SQL LIKE query |

## Data Flow

```
Query String
    │
    ▼
[f32; 1024] (query embedding via LM Studio)
    │
    ├──────────────────────────────┐
    ▼                              ▼
cosine_similarity()         keyword search
    │                              │
    ▼                              ▼
Vec<(obj, score)>           Vec<(obj, 0.5)>
    │                              │
    └──────────┬───────────────────┘
               ▼
         Merge + Boost
               │
               ▼
         Sort by score
               │
               ▼
         Filter min_score
               │
               ▼
         Truncate to limit
               │
               ▼
    Vec<SearchHit>
```

## Configuration

- **Embedding Model**: Qwen3-Embedding-0.6B (1024 dimensions)
- **Default min_score**: 0.3
- **Default limit**: 10
- **Keyword boost**: 0.2
- **LM Studio endpoint**: localhost:1234/v1/embeddings
