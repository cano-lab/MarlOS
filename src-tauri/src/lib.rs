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

            // Initialize AI manager
            let ai_manager = ai::AiManager::new();
            log::info!("AI support enabled");
            app.manage(ai_manager);

            // Initialize Provider registry
            let provider_registry = providers::ProviderRegistry::new();
            log::info!("Provider system initialized");
            app.manage(provider_registry);

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
