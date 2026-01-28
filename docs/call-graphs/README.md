# MarlOS Call Graphs

Visual documentation of function call chains and data flow in MarlOS.

## Contents

| Document | Description |
|----------|-------------|
| [01-semantic-search.md](01-semantic-search.md) | How search queries flow through the system |
| [02-session-import.md](02-session-import.md) | How Claude Code sessions are imported |
| [03-paper-generation.md](03-paper-generation.md) | Research paper generation pipeline |
| [04-application-startup.md](04-application-startup.md) | Application initialization sequence |
| [05-module-dependencies.md](05-module-dependencies.md) | Module relationships and dependencies |

## How to Read These Diagrams

### Function Calls
```
function_a()
    │
    └──► function_b()  ← function_a calls function_b
```

### Data Flow
```
Input Data
    │
    ▼
[Transformation]
    │
    ▼
Output Data
```

### File References
```
┌─────────────────────────────────────┐
│ file.rs:123                         │  ← file name and line number
│ function_name(params) -> ReturnType │  ← function signature
│     │                               │
│     └──► what it does               │
└─────────────────────────────────────┘
```

## Quick Reference

### Main Entry Points

| Entry | File | Purpose |
|-------|------|---------|
| GUI App | `main.rs` → `lib.rs:31` | Start Tauri application |
| CLI | `bin/marlos.rs` | Command-line interface |

### Key Data Structures

| Type | File | Purpose |
|------|------|---------|
| `SemanticObject` | semantic_object.rs | Universal content container |
| `Suid` | semantic_object.rs | Unique identifier (UUID-based) |
| `SearchHit` | semantic_search.rs | Search result with score |
| `Paper` | paper_generator/paper.rs | Research paper model |
| `Source` | providers/research.rs | Research source |

### Key Functions

| Function | File | Purpose |
|----------|------|---------|
| `SemanticSearch::search` | semantic_search.rs:251 | Main search entry |
| `cosine_similarity` | embeddings.rs:470 | Vector comparison |
| `ObjectStore::create` | object_store.rs:166 | Store object |
| `EmbeddingManager::embed` | embeddings.rs:450 | Generate vector |
| `PaperPipeline::write_section` | pipeline.rs:504 | Generate paper section |

## Updating These Docs

When adding new features:
1. Add relevant call graph to appropriate file
2. Update module dependencies if new modules added
3. Add to README index

When modifying existing flows:
1. Update line numbers if they change
2. Add/remove functions from the chain
3. Update data flow diagrams
