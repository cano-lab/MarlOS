# Session Import Call Graph

## Overview
This documents the complete function call chain when importing Claude Code sessions into MarlOS via the CLI.

## Entry Point
- **CLI**: `marlos-cli sessions import all --embed`
- **Command**: `bin/marlos.rs` (Commands::Sessions::Import)

## Call Graph

```
CLI: marlos-cli sessions import all --embed
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ bin/marlos.rs (Commands::Sessions::Import)                               │
│     │                                                                    │
│     ├──► SessionDiscovery::new() ──────────────────────────────────┐    │
│     │                                                               │    │
│     │    ┌──────────────────────────────────────────────────────────┘    │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ providers/session.rs:771                                    │  │
│     │  │ SessionDiscovery::new()                                     │  │
│     │  │     │                                                       │  │
│     │  │     ▼                                                       │  │
│     │  │ Finds ~/.claude/projects path                               │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     ├──► discovery.find_all_sessions() ────────────────────────────┐    │
│     │                                                               │    │
│     │    ┌──────────────────────────────────────────────────────────┘    │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ providers/session.rs:782                                    │  │
│     │  │ find_all_sessions() -> Vec<PathBuf>                         │  │
│     │  │     │                                                       │  │
│     │  │     ▼                                                       │  │
│     │  │ Walk ~/.claude/projects/**/                                 │  │
│     │  │ Filter: *.jsonl files                                       │  │
│     │  │ Returns: [path1.jsonl, path2.jsonl, ...]                    │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     ├──► for each session_path:                                          │
│     │        ChunkedSession::parse_file(path) ─────────────────────┐    │
│     │                                                               │    │
│     │    ┌──────────────────────────────────────────────────────────┘    │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ providers/session.rs:246                                    │  │
│     │  │ ChunkedSession::parse_file(path) -> ChunkedSession          │  │
│     │  │     │                                                       │  │
│     │  │     ├──► Read file line by line                             │  │
│     │  │     │                                                       │  │
│     │  │     ├──► for each line:                                     │  │
│     │  │     │        serde_json::from_str::<SessionEntry>           │  │
│     │  │     │        Extract: role, content, timestamp              │  │
│     │  │     │                                                       │  │
│     │  │     ├──► Group into ConversationChunks                      │  │
│     │  │     │        target_size: ~2000 chars                       │  │
│     │  │     │        maintain context (user Q + assistant A)        │  │
│     │  │     │                                                       │  │
│     │  │     ├──► Extract file operations                            │  │
│     │  │     │        Parse tool_use for Read/Write/Edit             │  │
│     │  │     │                                                       │  │
│     │  │     └──► Returns: ChunkedSession {                          │  │
│     │  │              id, project, path,                             │  │
│     │  │              chunks: Vec<ConversationChunk>,                │  │
│     │  │              file_operations: Vec<FileOperation>,           │  │
│     │  │              start_time, end_time                           │  │
│     │  │          }                                                  │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     ├──► chunked_session_to_objects(session) ──────────────────────┐    │
│     │                                                               │    │
│     │    ┌──────────────────────────────────────────────────────────┘    │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ providers/session.rs:843                                    │  │
│     │  │ chunked_session_to_objects(session) -> Vec<SemanticObject>  │  │
│     │  │     │                                                       │  │
│     │  │     ├──► Create session-level object:                       │  │
│     │  │     │        SemanticObject::from_text(summary)             │  │
│     │  │     │            .with_name("Session: project - topic")     │  │
│     │  │     │            .with_tags(["session", "claude-code"])     │  │
│     │  │     │            .with_tier(Guarded)                        │  │
│     │  │     │                                                       │  │
│     │  │     └──► for each chunk:                                    │  │
│     │  │              SemanticObject::from_text(chunk.text)          │  │
│     │  │                  .with_name("Chunk N: topic")               │  │
│     │  │                  .with_tags(["session", "chunk"])           │  │
│     │  │                  .with_relation(PartOf -> session_obj)      │  │
│     │  │                                                             │  │
│     │  │     Returns: Vec<SemanticObject>                            │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     ├──► for each object:                                                │
│     │        store.create(&object) ────────────────────────────────┐    │
│     │                                                               │    │
│     │    ┌──────────────────────────────────────────────────────────┘    │
│     │    ▼                                                               │
│     │  ┌─────────────────────────────────────────────────────────────┐  │
│     │  │ object_store.rs:166                                         │  │
│     │  │ create(&self, object) -> Result<()>                         │  │
│     │  │     │                                                       │  │
│     │  │     ▼                                                       │  │
│     │  │ SQL: INSERT INTO objects (suid, name, content, ...)         │  │
│     │  │      VALUES (?, ?, ?, ...)                                  │  │
│     │  └─────────────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     └──► if --embed:                                                     │
│              embeddings.embed(object.content) ─────────────────────┐    │
│                                                                     │    │
│              store.store_embedding(suid, vec, model) ──────────────┼──┐ │
│                                                                     │  │ │
│          ┌──────────────────────────────────────────────────────────┘  │ │
│          │  ┌──────────────────────────────────────────────────────────┘ │
│          ▼  ▼                                                            │
│        ┌─────────────────────────────────────────────────────────────┐  │
│        │ object_store.rs:633                                         │  │
│        │ store_embedding(suid, embedding, model)                     │  │
│        │     │                                                       │  │
│        │     ├──► Convert Vec<f32> to bytes:                         │  │
│        │     │        embedding.iter()                               │  │
│        │     │            .flat_map(|f| f.to_le_bytes())             │  │
│        │     │            .collect()                                 │  │
│        │     │                                                       │  │
│        │     └──► SQL: INSERT INTO embeddings (suid, embedding, model)│  │
│        │              VALUES (?, ?, ?)                               │  │
│        │              -- embedding is BLOB (4096 bytes for 1024 dims) │  │
│        └─────────────────────────────────────────────────────────────┘  │
│                                                                          │
│     Output: "Imported 21,144 objects from 473 sessions"                  │
└──────────────────────────────────────────────────────────────────────────┘
```

## Key Functions

| Function | File:Line | Purpose |
|----------|-----------|---------|
| `SessionDiscovery::new` | session.rs:771 | Initialize session finder |
| `find_all_sessions` | session.rs:782 | Discover all .jsonl files |
| `ChunkedSession::parse_file` | session.rs:246 | Parse JSONL into chunks |
| `chunked_session_to_objects` | session.rs:843 | Convert to SemanticObjects |
| `ObjectStore::create` | object_store.rs:166 | Store object in SQLite |
| `store_embedding` | object_store.rs:633 | Store vector as BLOB |

## Data Flow

```
~/.claude/projects/**/*.jsonl
    │
    ▼
Parse JSONL lines
    │
    ▼
SessionEntry { role, content, timestamp }
    │
    ▼
Group into ConversationChunks (~2000 chars each)
    │
    ▼
ChunkedSession { id, project, chunks[], file_ops[] }
    │
    ▼
Vec<SemanticObject>
    │
    ├──► SQLite: objects table
    │
    └──► if --embed:
             │
             ▼
         LM Studio: embed(content)
             │
             ▼
         Vec<f32> (1024 dims)
             │
             ▼
         SQLite: embeddings table (BLOB)
```

## Session File Format

Input (Claude Code JSONL):
```json
{"type":"user","message":{"content":"fix the bug in parser.rs"},"timestamp":"2024-01-15T10:30:00Z"}
{"type":"assistant","message":{"content":"I'll examine parser.rs..."},"timestamp":"2024-01-15T10:30:05Z"}
```

Output (SemanticObject):
```rust
SemanticObject {
    suid: "01HQXYZ...",
    name: "Session: marlos-rust - fix parser bug",
    content: "user: fix the bug in parser.rs\nassistant: I'll examine...",
    tags: ["session", "claude-code", "marlos-rust"],
    content_type: Text(Plain),
    security_tier: Guarded,
    relations: [PartOf(session_parent_suid)],
}
```

## Storage Details

- **Objects table**: Full metadata + content
- **Embeddings table**: SUID → BLOB (1024 × 4 = 4096 bytes per vector)
- **Chunk target size**: ~2000 characters
- **Typical session**: 10-50 chunks
