//! MarlOS - Semantic Document Environment
//!
//! A local-first, AI-enhanced document environment that remembers your work context.

pub mod kernel;
pub mod memory;
pub mod document;
#[cfg(feature = "tauri-app")]
pub mod commands;
pub mod pdf;
pub mod epub;
pub mod images;
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
pub mod waveform_similarity;
pub mod andor_client;
pub mod llm_tasks;
pub mod mcp;
pub mod paper_generator;
pub mod platform;
pub mod proactive;
pub mod sessions;
pub mod provider_store;
pub mod embedding_store;
pub mod embedding_analysis;
pub mod embedding_transform;
pub mod pca_cache;
pub mod repo_tracker;
pub mod chunking;
pub mod epub_notes;
pub mod finetuning;
pub mod research_agent;
pub mod document_versions;
pub mod reference_library;
pub mod research_project;
pub mod typesetter;
pub mod voice_tts;

#[cfg(feature = "tauri-app")]
use std::sync::Arc;
#[cfg(feature = "tauri-app")]
use tauri::Manager;

/// Initialize and run the Tauri application
#[cfg(feature = "tauri-app")]
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
            let kernel = kernel::SemanticKernel::new()
                .map_err(|e| format!("Failed to initialize semantic kernel: {}", e))?;
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

            // Initialize EPUB Notes manager
            let epub_notes_manager = epub_notes::EpubNotesManager::new();
            log::info!("EPUB Notes support enabled");
            app.manage(epub_notes_manager);

            // Initialize AI manager (wrapped in Arc for shared ownership)
            let ai_manager = std::sync::Arc::new(ai::AiManager::new());
            log::info!("AI support enabled");
            app.manage(ai_manager.clone());

            // Initialize Provider registry
            let provider_registry = providers::ProviderRegistry::new();
            log::info!("Provider system initialized");
            app.manage(provider_registry);

            // Initialize ObjectStore and SemanticSearch
            let app_data_dir = app.path().app_data_dir()
                .map_err(|e| format!("Failed to get app data directory: {}", e))?;
            std::fs::create_dir_all(&app_data_dir)
                .map_err(|e| format!("Failed to create app data directory: {}", e))?;

            let db_path = app_data_dir.join("objects.db");
            log::info!("Object store path: {:?}", db_path);

            let object_store = object_store::ObjectStore::new(db_path)
                .map_err(|e| format!("Failed to create object store: {}", e))?;

            // Auto-detect embedding provider (LM Studio → Ollama → Mock)
            let embedding_manager = tauri::async_runtime::block_on(
                embeddings::EmbeddingManager::auto_detect()
            );
            log::info!("Embedding manager initialized: {}", embedding_manager.model_info().name);

            let semantic_search = semantic_search::SemanticSearch::new(object_store, embedding_manager);
            log::info!("Semantic search initialized");

            // Initialize SessionManager
            let session_manager = sessions::SessionManager::new()
                .map_err(|e| format!("Failed to create session manager: {}", e))?;

            // Load existing sessions from disk
            if let Err(e) = session_manager.load() {
                log::warn!("Failed to load sessions: {}", e);
            } else {
                log::info!("Loaded {} sessions from disk", session_manager.get_session_history().len());
            }

            app.manage(session_manager);

            // Initialize ProviderStore for custom AI providers
            let provider_store = provider_store::ProviderStore::new();
            log::info!("Provider store initialized");
            app.manage(provider_store);

            // Initialize EmbeddingStore for embedding configuration
            let embedding_store = embedding_store::EmbeddingStore::new();
            log::info!("Embedding store initialized");
            app.manage(embedding_store);

            // Initialize PCA cache for fast 3D space loading
            let pca_cache = pca_cache::PCACacheManager::new(app_data_dir.clone());
            log::info!("PCA cache initialized");
            app.manage(pca_cache);

            // Initialize Document Version Store
            let version_store = document_versions::VersionStore::new(app_data_dir.clone())
                .map_err(|e| format!("Failed to create version store: {}", e))?;
            log::info!("Document version history initialized");
            app.manage(version_store);

            // Initialize Reference Library
            let reference_store = reference_library::ReferenceStore::new(app_data_dir.clone())
                .map_err(|e| format!("Failed to create reference store: {}", e))?;
            log::info!("Reference library initialized");
            app.manage(reference_store);

            // Initialize Project Store
            let project_store = research_project::ProjectStore::new(app_data_dir.clone());
            log::info!("Project store initialized");
            app.manage(project_store);

            // Initialize RepoTracker for watched repositories
            let repo_tracker = repo_tracker::RepoTracker::new();
            log::info!("Repository tracker initialized with {} repos", repo_tracker.list_repos().len());
            app.manage(repo_tracker);

            // Wrap semantic_search in Arc for shared access across commands
            let semantic_search = Arc::new(semantic_search);
            app.manage(semantic_search.clone());

            // Initialize Autonomous Research Agent for autonomous research
            let research_agent = Arc::new(tokio::sync::RwLock::new(
                research_agent::AutonomousResearchAgent::new(ai_manager.clone(), semantic_search.clone())
            ));
            log::info!("Autonomous Research Agent initialized");
            app.manage(research_agent);

            // Initialize Voice TTS sidecar manager (lazy — sidecar starts on first call)
            let script_candidates = [
                std::path::PathBuf::from("../python/tts_sidecar.py"),
                std::path::PathBuf::from("python/tts_sidecar.py"),
            ];
            let script_path = script_candidates
                .iter()
                .find(|p| p.exists())
                .cloned()
                .unwrap_or_else(|| std::path::PathBuf::from("../python/tts_sidecar.py"));
            let voice_tts_manager = voice_tts::VoiceTtsManager::new(app_data_dir.clone(), script_path);
            log::info!("Voice TTS sidecar manager initialized");
            app.manage(voice_tts_manager);

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
            commands::epub_debug_info,
            // EPUB Notes commands
            commands::epub_notes_save,
            commands::epub_notes_load,
            commands::epub_notes_delete,
            commands::epub_notes_get_for_book,
            commands::epub_notes_get_for_chapter,
            commands::epub_notes_update_note,
            // AI commands
            commands::ai_check_status,
            commands::ai_get_config,
            commands::ai_set_config,
            commands::ai_list_models,
            commands::ai_chat,
            commands::ai_run_task,
            commands::ai_generate,
            // Custom Provider commands
            commands::custom_provider_list,
            commands::custom_provider_get_active,
            commands::custom_provider_get_active_id,
            commands::custom_provider_get,
            commands::custom_provider_add,
            commands::custom_provider_update,
            commands::custom_provider_delete,
            commands::custom_provider_set_active,
            commands::custom_provider_test,
            commands::custom_provider_get_presets,
            commands::custom_provider_create_from_preset,
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
            commands::object_search_waveform,
            commands::object_find_similar_waveform,
            commands::object_import_file,
            commands::import_repository,
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
            commands::paper_export_latex,
            commands::paper_get_sections,
            commands::paper_get_findings,
            // Autonomous Research Agent commands
            commands::research_add_interest,
            commands::research_remove_interest,
            commands::research_list_interests,
            commands::research_run_sweep,
            commands::research_discover_from_library,
            commands::research_get_clusters,
            commands::research_get_cluster_articles,
            commands::research_get_articles_by_interest,
            commands::research_find_similar_articles,
            // Proactive Intelligence commands
            commands::proactive_on_file_opened,
            commands::proactive_on_query,
            commands::proactive_on_session_start,
            commands::proactive_check_decision,
            commands::proactive_log_decision,
            commands::proactive_decision_stats,
            // Mobile capture commands
            commands::quick_capture,
            commands::get_recent_objects,
            commands::semantic_search,
            commands::save_research_source,
            commands::get_saved_sources,
            commands::open_external_url,
            // Thinking Debugger commands
            commands::analyze_thinking,
            commands::fact_check_claim,
            commands::suggest_better_questions,
            commands::list_sessions,
            commands::get_session_conversation,
            // Browser Sync commands
            commands::detect_browsers,
            commands::request_browser_access,
            commands::scan_chatgpt_conversations,
            commands::sync_chatgpt_conversations,
            commands::skip_conversation,
            commands::get_sync_state,
            commands::set_auto_sync,
            // Session commands
            commands::start_session,
            commands::end_session,
            commands::get_current_session,
            commands::get_session_history,
            commands::resume_session,
            commands::search_sessions,
            commands::record_session_activity,
            commands::take_session_snapshot,
            commands::get_session_context,
            commands::create_backfill_session,
            commands::create_sample_sessions,
            commands::import_conversations_as_sessions,
            commands::debug_list_objects,
            commands::import_claude_code_conversations,
            commands::import_claude_code_sessions,
            commands::sync_from_andor,
            commands::import_chatgpt_export,
            // LLM-accessible session queries
            commands::get_session_details,
            commands::get_sessions_by_provider,
            commands::get_recent_files,
            commands::get_active_ai_sessions,
            commands::open_terminal,
            commands::launch_claude_agent,
            commands::get_recent_context,
            commands::get_session_vector_analysis,
            // Vector Database Query UI commands
            commands::index_sessions_to_vector_db,
            commands::vector_search,
            commands::find_similar,
            commands::get_vector_clusters,
            commands::get_all_vector_objects,
            commands::clear_vector_database,
            commands::force_reindex_all,
            // Embedding configuration commands
            commands::embedding_get_config,
            commands::embedding_set_config,
            commands::embedding_test_provider,
            commands::embedding_get_presets,
            commands::search_get_config,
            commands::search_set_config,
            commands::embedding_reset_to_defaults,
            // Embedding Analysis commands
            commands::analyze_embedding_space,
            commands::test_vector_arithmetic,
            commands::generate_toward_vector,
            commands::interpolate_concepts,
            commands::find_semantic_midpoint,
            commands::analyze_vector_difference,
            // 3D Idea Space commands
            commands::get_idea_space_3d,
            commands::get_idea_space_custom_axes,
            commands::find_near_point_3d,
            commands::get_idea_clusters,
            commands::export_idea_space_vr,
            commands::analyze_knowledge_center,
            // Image Viewer commands
            commands::get_image_info,
            commands::render_image,
            commands::get_supported_image_formats,
            commands::is_supported_image,
            // Repository Tracker commands
            commands::repo_add,
            commands::repo_remove,
            commands::repo_list,
            commands::repo_get_status,
            commands::repo_sync,
            commands::repo_set_auto_sync,
            commands::repo_check_path,
            // Semantic Calculator commands
            commands::vector_calculate,
            commands::vector_info,
            // File operation commands (for chat slash commands)
            commands::read_file_content,
            commands::list_directory,
            commands::ingest_file_to_memory,
            commands::get_object_embedding,
            // Plan Space commands
            commands::plan_create_goal,
            commands::plan_update_goal,
            commands::plan_delete_goal,
            commands::plan_list_goals,
            commands::plan_save_canvas,
            commands::plan_generate_milestones,
            // Widget commands
            commands::plan_create_widget,
            commands::plan_list_widgets,
            commands::plan_update_widget,
            commands::plan_delete_widget,
            commands::plan_save_canvas_widgets,
            commands::open_file_path,
            commands::pick_file,
            // Fine-tuning commands
            finetuning::export_conversations_for_finetuning,
            finetuning::get_export_stats,
            // Document version history commands
            commands::version_save,
            commands::version_list,
            commands::version_get,
            commands::version_diff,
            commands::version_diff_current,
            commands::version_label,
            // Reference library commands
            commands::ref_add,
            commands::ref_add_from_doi,
            commands::ref_add_from_isbn,
            commands::ref_get,
            commands::ref_list,
            commands::ref_search,
            commands::ref_update,
            commands::ref_delete,
            commands::ref_set_reading_status,
            commands::ref_add_to_collection,
            commands::ref_list_collections,
            commands::ref_import_bibtex,
            commands::ref_export_bibtex,
            commands::ref_check_duplicates,
            commands::ref_attach_pdf,
            commands::ref_count,
            // CSL citation style commands
            commands::csl_list_styles,
            commands::csl_render_bibliography,
            commands::csl_render_inline,
            commands::ref_generate_bibliography,
            // Research Project commands
            commands::project_create,
            commands::project_list,
            commands::project_get,
            commands::project_delete,
            commands::project_set_status,
            commands::project_add_paper,
            commands::project_add_reference,
            commands::project_add_document,
            // Paper template & cross-ref commands
            commands::template_list,
            commands::template_get,
            commands::resolve_cross_refs,
            // Research intelligence commands
            commands::ref_find_related,
            commands::ref_extract_keywords,
            commands::ref_gap_analysis,
            // Voice TTS (F5-TTS sidecar) commands
            commands::voice_tts_status,
            commands::voice_tts_set_reference,
            commands::voice_tts_clear_reference,
            commands::voice_tts_synthesize,
            commands::voice_tts_shutdown,
            // Book typesetter (Phase A — pandoc bridge) commands
            commands::typesetter_pandoc_probe,
            commands::typesetter_pandoc_convert_file,
            commands::typesetter_pandoc_convert_str,
            // Book typesetter (Phase B — book.toml + structure) commands
            commands::typesetter_book_load,
            commands::typesetter_book_init,
            commands::typesetter_book_save,
            commands::typesetter_analyze_html,
            // Book typesetter (Phase E — headless Chromium PDF export)
            commands::typesetter_export_pdf,
            // Book typesetter (Phase F — EPUB export)
            commands::typesetter_export_epub,
            // Book typesetter — Source-view live edits
            commands::typesetter_read_book_file,
            commands::typesetter_write_book_file,
            commands::typesetter_read_custom_css,
            commands::typesetter_write_custom_css,
            // Research feed
            commands::research_feed_list,
            commands::research_run_daily_fetch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
