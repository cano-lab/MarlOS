//! MarlOS - Semantic Document Environment
//!
//! A local-first, AI-enhanced document environment that remembers your work context.

pub mod kernel;
pub mod memory;
pub mod document;
pub mod commands;
pub mod pdf;

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
