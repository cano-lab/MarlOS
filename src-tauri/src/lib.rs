//! MarlOS - Semantic Document Environment
//!
//! A local-first, AI-enhanced document environment that remembers your work context.

pub mod kernel;
pub mod memory;
pub mod document;
pub mod commands;
pub mod pdf;
pub mod epub;
pub mod ai;
pub mod healing_test;
pub mod llm_client;
pub mod healing_engine;
pub mod providers;
pub mod tier_classifier;
pub mod semantic_object;
pub mod object_store;
pub mod embeddings;
pub mod semantic_search;
pub mod andor_client;
pub mod llm_tasks;
pub mod mcp;
pub mod paper_generator;

use std::sync::Arc;
use tauri::Manager;

/// Initialize and run the Tauri application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();
    log::info!("Starting MarlOS...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            log::info!("MarlOS initialized");

            // Initialize the semantic kernel
            let kernel = kernel::SemanticKernel::new();
            app.manage(kernel);

            // Initialize PDF manager (may fail if PDFium not available)
            match pdf::PdfManager::new() {
                Ok(pdf_manager) => {
                    log::info!("PDF support enabled");
                    app.manage(pdf_manager);
                }
                Err(e) => {
                    log::warn!("PDF support disabled: {}", e);
                }
            }

            // Initialize EPUB manager
            let epub_manager = epub::EpubManager::new();
            log::info!("EPUB support enabled");
            app.manage(epub_manager);

            // Initialize AI manager (wrapped in Arc for shared ownership)
            let ai_manager = std::sync::Arc::new(ai::AiManager::new());
            log::info!("AI support enabled");
            app.manage(ai_manager);

            // Initialize Provider registry
            let provider_registry = providers::ProviderRegistry::new();
            log::info!("Provider system initialized");
            app.manage(provider_registry);

            // Initialize ObjectStore and SemanticSearch
            let app_data_dir = app.path().app_data_dir()
                .expect("Failed to get app data directory");
            std::fs::create_dir_all(&app_data_dir)
                .expect("Failed to create app data directory");

            let db_path = app_data_dir.join("objects.db");
            log::info!("Object store path: {:?}", db_path);

            let object_store = object_store::ObjectStore::new(db_path)
                .expect("Failed to create object store");

            // Auto-detect embedding provider (LM Studio → Ollama → Mock)
            let embedding_manager = tauri::async_runtime::block_on(
                embeddings::EmbeddingManager::auto_detect()
            );
            log::info!("Embedding manager initialized: {}", embedding_manager.model_info().name);

            let semantic_search = semantic_search::SemanticSearch::new(object_store, embedding_manager);
            log::info!("Semantic search initialized");

            // Wrap in Arc for shared access across commands
            app.manage(Arc::new(semantic_search));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_version,
            commands::create_document,
            commands::open_document,
            commands::save_document,
            commands::search_memory,
            // PDF commands
            commands::pdf_open,
            commands::pdf_get_info,
            commands::pdf_render_page,
            commands::pdf_measure_distance,
            commands::pdf_measure_area,
            commands::pdf_pixel_to_real,
            // EPUB commands
            commands::epub_open,
            commands::epub_get_info,
            commands::epub_get_chapter,
            commands::epub_get_chapter_by_path,
            commands::epub_search,
            commands::epub_get_cover,
            // AI commands
            commands::ai_check_status,
            commands::ai_get_config,
            commands::ai_set_config,
            commands::ai_chat,
            commands::ai_run_task,
            commands::ai_generate,
            // Provider commands
            commands::provider_list,
            commands::provider_list_commands,
            commands::provider_get_state,
            commands::provider_init_for_document,
            commands::provider_content_changed,
            commands::provider_word_count,
            commands::provider_markdown_structure,
            // Coding agent commands
            commands::provider_code_operation,
            commands::provider_code_explain,
            commands::provider_code_complete,
            commands::provider_code_edit,
            // Semantic object commands
            commands::object_create,
            commands::object_get,
            commands::object_list,
            commands::object_delete,
            commands::object_search,
            commands::object_find_similar,
            commands::object_import_file,
            commands::object_export_file,
            // Tier management commands (LLM can use these)
            commands::object_get_tier,
            commands::object_set_tier,
            commands::object_tier_history,
            commands::object_add_relation,
            // Research commands
            commands::research_add_from_url,
            commands::research_add_manual,
            commands::research_get_source,
            commands::research_list_sources,
            commands::research_search_sources,
            commands::research_delete_source,
            commands::research_update_tags,
            commands::research_add_notes,
            commands::research_generate_citation,
            commands::research_generate_bibliography,
            commands::research_summarize_source,
            commands::research_find_connections,
            commands::research_fact_check,
            commands::research_discover_sources,
            // LLM Task-based commands
            commands::llm_summarize,
            commands::llm_analyze_code,
            commands::llm_answer_question,
            commands::llm_classify_text,
            // MCP Research commands
            commands::mcp_research,
            commands::mcp_research_academic,
            commands::mcp_web_search,
            commands::mcp_fetch_page,
            commands::mcp_academic_search,
            // Paper Generator commands
            commands::paper_create,
            commands::paper_get,
            commands::paper_list,
            commands::paper_delete,
            commands::paper_add_sources,
            commands::paper_chunk_sources,
            commands::paper_extract_findings,
            commands::paper_generate_outline,
            commands::paper_update_outline,
            commands::paper_write_section,
            commands::paper_write_all,
            commands::paper_review_section,
            commands::paper_update_section,
            commands::paper_export,
            commands::paper_get_sections,
            commands::paper_get_findings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
