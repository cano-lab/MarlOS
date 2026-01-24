//! Tauri commands - IPC interface between frontend and backend
//!
//! These commands are callable from JavaScript/TypeScript via Tauri's invoke API.

use std::sync::Mutex;
use tauri::State;
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::kernel::SemanticKernel;
use crate::memory::MemoryType;
use crate::pdf::{PdfManager, PdfInfo, RenderedPage, Measurement, PdfPoint};

/// Response for version info
#[derive(Serialize)]
pub struct VersionInfo {
    pub version: String,
    pub name: String,
    pub build: String,
}

/// Get application version
#[tauri::command]
pub fn get_version() -> VersionInfo {
    VersionInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        name: "MarlOS".to_string(),
        build: if cfg!(debug_assertions) { "debug" } else { "release" }.to_string(),
    }
}

/// Document state managed by Tauri
pub struct DocumentState {
    pub documents: Mutex<Vec<Document>>,
    pub active_index: Mutex<Option<usize>>,
}

impl Default for DocumentState {
    fn default() -> Self {
        Self {
            documents: Mutex::new(Vec::new()),
            active_index: Mutex::new(None),
        }
    }
}

/// Create a new document
#[tauri::command]
pub fn create_document(
    kernel: State<'_, SemanticKernel>,
) -> Result<Document, String> {
    let doc = Document::new();

    // Register with kernel
    let handle = crate::kernel::ContextHandle::new("document");
    kernel.register_context(handle);

    // Store in memory
    kernel.memory.store(
        &format!("Created new document: {}", doc.title),
        MemoryType::Event,
        serde_json::json!({ "document_id": doc.id }),
    ).map_err(|e| e.to_string())?;

    log::info!("Created document: {}", doc.id);
    Ok(doc)
}

/// Open a document from file
#[tauri::command]
pub fn open_document(
    path: String,
    kernel: State<'_, SemanticKernel>,
) -> Result<Document, String> {
    let doc = Document::open(&path).map_err(|e| e.to_string())?;

    // Register with kernel
    let handle = crate::kernel::ContextHandle::new("document")
        .with_path(&path);
    kernel.register_context(handle);

    // Store in memory
    kernel.memory.store(
        &format!("Opened document: {}", doc.title),
        MemoryType::Event,
        serde_json::json!({
            "document_id": doc.id,
            "path": path,
            "word_count": doc.word_count
        }),
    ).map_err(|e| e.to_string())?;

    log::info!("Opened document: {} from {}", doc.id, path);
    Ok(doc)
}

/// Save document request
#[derive(Deserialize)]
pub struct SaveRequest {
    pub id: String,
    pub content: String,
    pub path: Option<String>,
}

/// Save a document
#[tauri::command]
pub fn save_document(
    request: SaveRequest,
    kernel: State<'_, SemanticKernel>,
) -> Result<Document, String> {
    let mut doc = Document::new();
    doc.id = request.id;
    doc.set_content(request.content);

    if let Some(path) = request.path {
        doc.save_as(&path).map_err(|e| e.to_string())?;
    } else {
        return Err("No path provided for save".to_string());
    }

    // Store in memory
    kernel.memory.store(
        &format!("Saved document: {}", doc.title),
        MemoryType::Event,
        serde_json::json!({
            "document_id": doc.id,
            "path": doc.path,
            "word_count": doc.word_count
        }),
    ).map_err(|e| e.to_string())?;

    log::info!("Saved document: {}", doc.id);
    Ok(doc)
}

/// Search memory
#[tauri::command]
pub fn search_memory(
    query: String,
    limit: Option<usize>,
    kernel: State<'_, SemanticKernel>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(20);

    let results = kernel.memory.search(&query, limit)
        .map_err(|e| e.to_string())?;

    let json_results: Vec<serde_json::Value> = results
        .into_iter()
        .map(|entry| serde_json::json!({
            "id": entry.id,
            "content": entry.content,
            "type": entry.memory_type.as_str(),
            "created_at": entry.created_at.to_rfc3339(),
            "metadata": entry.metadata,
        }))
        .collect();

    Ok(json_results)
}

// ============================================================================
// PDF Commands
// ============================================================================

/// Open a PDF file
#[tauri::command]
pub fn pdf_open(
    path: String,
    pdf_manager: State<'_, PdfManager>,
) -> Result<PdfInfo, String> {
    pdf_manager.open(&path).map_err(|e| e.to_string())
}

/// Get current PDF info
#[tauri::command]
pub fn pdf_get_info(
    pdf_manager: State<'_, PdfManager>,
) -> Result<PdfInfo, String> {
    pdf_manager.get_info().map_err(|e| e.to_string())
}

/// Render a PDF page at specified DPI
#[tauri::command]
pub fn pdf_render_page(
    page_index: u32,
    dpi: f32,
    pdf_manager: State<'_, PdfManager>,
) -> Result<RenderedPage, String> {
    pdf_manager.render_page(page_index, dpi).map_err(|e| e.to_string())
}

/// Measure distance between two points (in pixel coordinates)
#[tauri::command]
pub fn pdf_measure_distance(
    page_index: u32,
    x1_px: f64,
    y1_px: f64,
    x2_px: f64,
    y2_px: f64,
    render_scale: f32,
    pdf_manager: State<'_, PdfManager>,
) -> Result<Measurement, String> {
    // Convert pixel coordinates to PDF points
    let p1 = pdf_manager.pixel_to_points(page_index, x1_px, y1_px, render_scale)
        .map_err(|e| e.to_string())?;
    let p2 = pdf_manager.pixel_to_points(page_index, x2_px, y2_px, render_scale)
        .map_err(|e| e.to_string())?;

    pdf_manager.measure_distance(page_index, p1, p2).map_err(|e| e.to_string())
}

/// Measure area of a polygon (points in pixel coordinates)
#[tauri::command]
pub fn pdf_measure_area(
    page_index: u32,
    points_px: Vec<(f64, f64)>,
    render_scale: f32,
    pdf_manager: State<'_, PdfManager>,
) -> Result<Measurement, String> {
    // Convert all pixel coordinates to PDF points
    let points: Result<Vec<PdfPoint>, String> = points_px
        .into_iter()
        .map(|(x, y)| {
            pdf_manager.pixel_to_points(page_index, x, y, render_scale)
                .map_err(|e| e.to_string())
        })
        .collect();

    pdf_manager.measure_area(page_index, points?).map_err(|e| e.to_string())
}

/// Convert pixel coordinates to real-world coordinates
#[tauri::command]
pub fn pdf_pixel_to_real(
    page_index: u32,
    x_px: f64,
    y_px: f64,
    render_scale: f32,
    pdf_manager: State<'_, PdfManager>,
) -> Result<serde_json::Value, String> {
    let point = pdf_manager.pixel_to_points(page_index, x_px, y_px, render_scale)
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "points": { "x": point.x, "y": point.y },
        "inches": { "x": point.x / 72.0, "y": point.y / 72.0 },
        "mm": { "x": point.x / 72.0 * 25.4, "y": point.y / 72.0 * 25.4 },
    }))
}
