# Module Dependencies

## Overview
This documents the dependency relationships between MarlOS Rust modules.

## Dependency Graph

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         MODULE DEPENDENCIES                                 │
│                                                                             │
│                           ┌──────────┐                                      │
│                           │  lib.rs  │  (Entry point)                       │
│                           └────┬─────┘                                      │
│                                │                                            │
│        ┌───────────┬──────────┼──────────┬───────────┬──────────┐          │
│        ▼           ▼          ▼          ▼           ▼          ▼          │
│   ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ │
│   │commands │ │ kernel  │ │semantic │ │   ai    │ │providers│ │  mcp    │ │
│   │         │ │         │ │_search  │ │         │ │         │ │         │ │
│   └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ │
│        │           │           │           │           │           │       │
│        │           │      ┌────┴────┐      │           │           │       │
│        │           │      ▼         ▼      │           │           │       │
│        │           │ ┌─────────┐┌───────┐  │           │           │       │
│        │           │ │object_  ││embed- │  │           │           │       │
│        │           │ │store    ││dings  │  │           │           │       │
│        │           │ └────┬────┘└───┬───┘  │           │           │       │
│        │           │      │         │      │           │           │       │
│        │           ▼      ▼         ▼      ▼           │           │       │
│        │      ┌─────────────────────────────────┐      │           │       │
│        │      │         memory.rs               │      │           │       │
│        │      │  (SecurityTier, MemoryType)     │      │           │       │
│        │      └─────────────────────────────────┘      │           │       │
│        │                      │                        │           │       │
│        └──────────────────────┼────────────────────────┘           │       │
│                               ▼                                    │       │
│                    ┌───────────────────────┐                       │       │
│                    │   semantic_object.rs  │ ◄─────────────────────┘       │
│                    │  (SemanticObject,     │                               │
│                    │   Suid, ContentType)  │                               │
│                    └───────────────────────┘                               │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Module List

### Core Modules

| Module | File | Purpose | Key Types |
|--------|------|---------|-----------|
| `lib` | lib.rs | Application entry, Tauri setup | `run()` |
| `commands` | commands.rs | IPC handlers (95+ commands) | Tauri commands |
| `kernel` | kernel.rs | Event system, context management | `SemanticKernel`, `KernelEvent` |
| `memory` | memory.rs | Security tiers, legacy memory | `SecurityTier`, `MemoryStore` |
| `semantic_object` | semantic_object.rs | Core data model | `SemanticObject`, `Suid` |
| `object_store` | object_store.rs | SQLite persistence | `ObjectStore` |
| `embeddings` | embeddings.rs | Vector generation | `EmbeddingManager`, `cosine_similarity` |
| `semantic_search` | semantic_search.rs | Search engine | `SemanticSearch`, `SearchHit` |

### AI & LLM Modules

| Module | File | Purpose | Key Types |
|--------|------|---------|-----------|
| `ai` | ai.rs | AI manager, prompts | `AiManager`, `SystemPrompts` |
| `llm_client` | llm_client.rs | LLM HTTP client | `LlmClient`, `LlmProvider` |
| `llm_tasks` | llm_tasks/mod.rs | Structured LLM tasks | `TaskRunner`, `LlmTask` |
| `llm_tasks::tasks` | llm_tasks/tasks.rs | Task implementations | `SummarizeContentTask`, etc. |

### Document Modules

| Module | File | Purpose | Key Types |
|--------|------|---------|-----------|
| `document` | document.rs | Basic document handling | `Document` |
| `pdf` | pdf.rs | PDF rendering, measurement | `PdfManager`, `PdfInfo` |
| `epub` | epub.rs | EPUB parsing | `EpubManager`, `ChapterContent` |

### Provider Modules

| Module | File | Purpose | Key Types |
|--------|------|---------|-----------|
| `providers` | providers/mod.rs | Plugin system | `ProviderRegistry`, `Provider` |
| `providers::session` | providers/session.rs | Claude session import | `ChunkedSession`, `SessionDiscovery` |
| `providers::research` | providers/research.rs | Research sources | `Source`, `CitationGenerator` |
| `providers::markdown_language` | providers/markdown_language.rs | Markdown analysis | `MarkdownLanguageService` |
| `providers::coding_agent` | providers/coding_agent.rs | Code operations | `CodingAgentProvider` |
| `providers::word_count` | providers/word_count.rs | Word statistics | `WordCountProvider` |

### MCP (Model Context Protocol) Modules

| Module | File | Purpose | Key Types |
|--------|------|---------|-----------|
| `mcp` | mcp/mod.rs | MCP server, research agent | `McpServer`, `ResearchAgent` |
| `mcp::tools` | mcp/tools.rs | Tool definitions | `Tool`, `ToolBuilder` |
| `mcp::web_search` | mcp/web_search.rs | Web + academic search | `search()`, `search_academic()` |

### Paper Generator Modules

| Module | File | Purpose | Key Types |
|--------|------|---------|-----------|
| `paper_generator` | paper_generator/mod.rs | Module exports | - |
| `paper_generator::paper` | paper_generator/paper.rs | Paper data model | `Paper`, `PaperSection`, `KeyFinding` |
| `paper_generator::pipeline` | paper_generator/pipeline.rs | Generation orchestration | `PaperPipeline` |
| `paper_generator::chunk` | paper_generator/chunk.rs | Semantic chunking | `SemanticChunker`, `SemanticChunk` |
| `paper_generator::store` | paper_generator/store.rs | Paper persistence | `PaperStore` |
| `paper_generator::citation_tracker` | paper_generator/citation_tracker.rs | Citation handling | `CitationTracker` |
| `paper_generator::export` | paper_generator/export.rs | Export formats | `PaperExporter` |

### Utility Modules

| Module | File | Purpose | Key Types |
|--------|------|---------|-----------|
| `tier_classifier` | tier_classifier.rs | Security classification | `TierClassifier` |
| `healing_engine` | healing_engine.rs | Self-healing diagnostics | `HealingEngine` |
| `healing_test` | healing_test.rs | Fault scenarios | `FaultScenario`, `TestHarness` |
| `andor_client` | andor_client.rs | Andor Hub API client | `AndorClient` |

## Dependency Flow

```
commands.rs
    │
    ├──► semantic_search.rs ──► object_store.rs ──► rusqlite
    │                      └──► embeddings.rs ──► reqwest (HTTP)
    │
    ├──► ai.rs ──► llm_client.rs ──► reqwest (HTTP)
    │
    ├──► providers/*.rs
    │         │
    │         └──► semantic_object.rs
    │
    └──► paper_generator/*.rs
              │
              ├──► semantic_search.rs
              └──► ai.rs
```

## External Dependencies

| Crate | Purpose | Used By |
|-------|---------|---------|
| `tauri` | Desktop framework | lib.rs, commands.rs |
| `rusqlite` | SQLite database | object_store.rs, memory.rs |
| `reqwest` | HTTP client | embeddings.rs, llm_client.rs, mcp/*.rs |
| `serde` | Serialization | All modules |
| `chrono` | Date/time | semantic_object.rs, memory.rs |
| `uuid` | Unique IDs | semantic_object.rs (Suid) |
| `tokio` | Async runtime | All async modules |
| `pdfium-render` | PDF rendering | pdf.rs |
| `epub` | EPUB parsing | epub.rs |
| `scraper` | HTML parsing | mcp/web_search.rs |

## Security Tier Flow

```
Content/Path
     │
     ▼
TierClassifier.classify()
     │
     ├──► Open (0)    - Public, LLM sees full content
     ├──► Guarded (1) - LLM sees metadata only
     └──► Sealed (2)  - Hidden from LLM entirely
```

The tier is checked at:
- `object_store.get_objects_with_embeddings(max_tier)` - filters by tier
- `semantic_search.search_for_llm()` - returns Full or Guarded view
- `memory.search_with_tier()` - respects tier limits
