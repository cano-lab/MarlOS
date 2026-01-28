# Application Startup Call Graph

## Overview
This documents the complete initialization sequence when MarlOS starts.

## Entry Point
- **Binary**: `main.rs` → `lib.rs:31 run()`

## Call Graph

```
main.rs → lib.rs:31 run()
     │
     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ tauri::Builder::default()                                                │
│     │                                                                    │
│     ├──► .plugin(tauri_plugin_shell::init())                             │
│     ├──► .plugin(tauri_plugin_dialog::init())                            │
│     ├──► .plugin(tauri_plugin_fs::init())                                │
│     │                                                                    │
│     ├──► .setup(|app| { ... })                                           │
│     │        │                                                           │
│     │        ├──► kernel::SemanticKernel::new() ───────────────────┐    │
│     │        │                                                      │    │
│     │        │    ┌─────────────────────────────────────────────────┘    │
│     │        │    ▼                                                      │
│     │        │  ┌────────────────────────────────────────────────────┐  │
│     │        │  │ kernel.rs:90                                       │  │
│     │        │  │ SemanticKernel::new()                              │  │
│     │        │  │     │                                              │  │
│     │        │  │     ├──► MemoryStore::new() ──────────────────┐    │  │
│     │        │  │     │                                          │    │  │
│     │        │  │     │    ┌─────────────────────────────────────┘    │  │
│     │        │  │     │    ▼                                          │  │
│     │        │  │     │  memory.rs:146                                │  │
│     │        │  │     │  Creates SQLite DB at ~/.marlos/memory.db    │  │
│     │        │  │     │                                              │  │
│     │        │  │     └──► Initialize context registry               │  │
│     │        │  └────────────────────────────────────────────────────┘  │
│     │        │                                                           │
│     │        ├──► pdf::PdfManager::new() ──────────────────────────┐    │
│     │        │                                                      │    │
│     │        │    ┌─────────────────────────────────────────────────┘    │
│     │        │    ▼                                                      │
│     │        │  ┌────────────────────────────────────────────────────┐  │
│     │        │  │ pdf.rs:131                                         │  │
│     │        │  │ PdfManager::new() -> Result<Self>                  │  │
│     │        │  │     │                                              │  │
│     │        │  │     └──► Pdfium::default()                         │  │
│     │        │  │              Load PDFium library (may fail!)       │  │
│     │        │  └────────────────────────────────────────────────────┘  │
│     │        │                                                           │
│     │        ├──► epub::EpubManager::new()                               │
│     │        │        │                                                  │
│     │        │        └──► epub.rs:97 - Initialize EPUB parser           │
│     │        │                                                           │
│     │        ├──► Arc::new(ai::AiManager::new()) ──────────────────┐    │
│     │        │                                                      │    │
│     │        │    ┌─────────────────────────────────────────────────┘    │
│     │        │    ▼                                                      │
│     │        │  ┌────────────────────────────────────────────────────┐  │
│     │        │  │ ai.rs:172                                          │  │
│     │        │  │ AiManager::new()                                   │  │
│     │        │  │     │                                              │  │
│     │        │  │     └──► Default config:                           │  │
│     │        │  │              provider: "lm-studio"                 │  │
│     │        │  │              api_base: localhost:1234/v1           │  │
│     │        │  │              model: (auto-detect)                  │  │
│     │        │  └────────────────────────────────────────────────────┘  │
│     │        │                                                           │
│     │        ├──► providers::ProviderRegistry::new() ──────────────┐    │
│     │        │                                                      │    │
│     │        │    ┌─────────────────────────────────────────────────┘    │
│     │        │    ▼                                                      │
│     │        │  ┌────────────────────────────────────────────────────┐  │
│     │        │  │ providers/mod.rs:196                               │  │
│     │        │  │ ProviderRegistry::new()                            │  │
│     │        │  │     │                                              │  │
│     │        │  │     └──► Initialize provider + command registries  │  │
│     │        │  └────────────────────────────────────────────────────┘  │
│     │        │                                                           │
│     │        ├──► ObjectStore::new(app_data/objects.db) ───────────┐    │
│     │        │                                                      │    │
│     │        │    ┌─────────────────────────────────────────────────┘    │
│     │        │    ▼                                                      │
│     │        │  ┌────────────────────────────────────────────────────┐  │
│     │        │  │ object_store.rs:51                                 │  │
│     │        │  │ ObjectStore::new(path)                             │  │
│     │        │  │     │                                              │  │
│     │        │  │     ├──► Create directory if needed                │  │
│     │        │  │     ├──► rusqlite::Connection::open(path)          │  │
│     │        │  │     └──► init_schema()                             │  │
│     │        │  │              CREATE TABLE objects (...)            │  │
│     │        │  │              CREATE TABLE embeddings (...)         │  │
│     │        │  │              CREATE TABLE relations (...)          │  │
│     │        │  │              CREATE INDEX idx_objects_name ...     │  │
│     │        │  │              CREATE INDEX idx_objects_tier ...     │  │
│     │        │  └────────────────────────────────────────────────────┘  │
│     │        │                                                           │
│     │        ├──► EmbeddingManager::auto_detect() ─────────────────┐    │
│     │        │                                                      │    │
│     │        │    ┌─────────────────────────────────────────────────┘    │
│     │        │    ▼                                                      │
│     │        │  ┌────────────────────────────────────────────────────┐  │
│     │        │  │ embeddings.rs:413                                  │  │
│     │        │  │ auto_detect() -> EmbeddingManager                  │  │
│     │        │  │     │                                              │  │
│     │        │  │     ├──► Try LM Studio (localhost:1234)            │  │
│     │        │  │     │        OpenAIEmbedding::lm_studio_default()  │  │
│     │        │  │     │        is_available()? → Use it              │  │
│     │        │  │     │                                              │  │
│     │        │  │     ├──► Try Ollama (localhost:11434)              │  │
│     │        │  │     │        OllamaEmbedding::default_local()      │  │
│     │        │  │     │        is_available()? → Use it              │  │
│     │        │  │     │                                              │  │
│     │        │  │     └──► Fall back to MockEmbedding                │  │
│     │        │  │              (deterministic hash-based vectors)    │  │
│     │        │  └────────────────────────────────────────────────────┘  │
│     │        │                                                           │
│     │        └──► SemanticSearch::new(store, embeddings) ──────────┐    │
│     │                                                               │    │
│     │             ┌─────────────────────────────────────────────────┘    │
│     │             ▼                                                      │
│     │           ┌────────────────────────────────────────────────────┐  │
│     │           │ semantic_search.rs:110                             │  │
│     │           │ SemanticSearch::new(store, embeddings)             │  │
│     │           │     │                                              │  │
│     │           │     └──► Wrap in Arc<RwLock<>> for thread safety   │  │
│     │           └────────────────────────────────────────────────────┘  │
│     │                                                                    │
│     │             app.manage(Arc::new(semantic_search))                  │
│     │                                                                    │
│     ├──► .invoke_handler(generate_handler![...])                         │
│     │        │                                                           │
│     │        └──► Register 95+ commands:                                 │
│     │                 get_version, create_document, open_document,       │
│     │                 pdf_open, pdf_render_page, epub_open,              │
│     │                 ai_chat, ai_generate, object_search,               │
│     │                 research_add_from_url, paper_create, ...           │
│     │                                                                    │
│     └──► .run(generate_context!())                                       │
│              │                                                           │
│              └──► Start WebView window with frontend                     │
└──────────────────────────────────────────────────────────────────────────┘
```

## Initialization Order

| Step | Component | File:Line | What it does |
|------|-----------|-----------|--------------|
| 1 | Tauri Plugins | lib.rs:36-38 | Shell, Dialog, FS access |
| 2 | SemanticKernel | kernel.rs:90 | Memory store + context registry |
| 3 | PdfManager | pdf.rs:131 | PDFium library (optional) |
| 4 | EpubManager | epub.rs:97 | EPUB parser |
| 5 | AiManager | ai.rs:172 | LLM client config |
| 6 | ProviderRegistry | providers/mod.rs:196 | Plugin system |
| 7 | ObjectStore | object_store.rs:51 | SQLite database |
| 8 | EmbeddingManager | embeddings.rs:413 | Vector provider |
| 9 | SemanticSearch | semantic_search.rs:110 | Search engine |
| 10 | Commands | lib.rs:98 | Register IPC handlers |
| 11 | WebView | lib.rs:196 | Start UI |

## State Management

Each manager is wrapped and managed by Tauri:

```rust
app.manage(kernel);                      // SemanticKernel
app.manage(pdf_manager);                 // PdfManager (if available)
app.manage(epub_manager);                // EpubManager
app.manage(ai_manager);                  // Arc<AiManager>
app.manage(provider_registry);           // ProviderRegistry
app.manage(Arc::new(semantic_search));   // Arc<SemanticSearch>
```

Commands access state via:
```rust
#[tauri::command]
pub async fn object_search(
    search: State<'_, Arc<SemanticSearch>>,  // Injected by Tauri
    query: String,
    // ...
) -> Result<...> {
    search.search(&query, options).await
}
```

## File Paths

| Purpose | Path |
|---------|------|
| Memory DB | `~/.marlos/memory.db` |
| Objects DB | `{app_data}/objects.db` |
| Sessions | `~/.claude/projects/**/*.jsonl` |

## Error Handling

- **PdfManager**: May fail if PDFium library not found → logged as warning, PDF features disabled
- **EmbeddingManager**: Falls back to mock if LM Studio/Ollama unavailable
- **ObjectStore**: Panics if database cannot be created (fatal)

## Logging

```rust
env_logger::init();
log::info!("Starting MarlOS...");
log::info!("PDF support enabled");           // or "disabled: {error}"
log::info!("EPUB support enabled");
log::info!("AI support enabled");
log::info!("Provider system initialized");
log::info!("Object store path: {:?}", db_path);
log::info!("Embedding manager initialized: {}", model_name);
log::info!("Semantic search initialized");
```
