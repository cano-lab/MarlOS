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
use crate::epub::{EpubManager, EpubInfo, ChapterContent, SearchResult as EpubSearchResult};
use crate::ai::{AiManager, ProviderConfig, Message, Role, AiResponse};
use crate::providers::{
    ProviderRegistry, ProviderInfo, Command as ProviderCommand,
    WordCountProvider, MarkdownLanguageService, CodingAgentProvider,
    CodeRequest, CodeResponse, CodeOperation,
};

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

    // Store in memory (public tier - these are events)
    kernel.memory.store_open(
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

    // Store in memory (public tier - these are events)
    kernel.memory.store_open(
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

    // Store in memory (public tier - these are events)
    kernel.memory.store_open(
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

// ============================================================================
// EPUB Commands
// ============================================================================

/// Open an EPUB file
#[tauri::command]
pub fn epub_open(
    path: String,
    epub_manager: State<'_, EpubManager>,
) -> Result<EpubInfo, String> {
    epub_manager.open(&path).map_err(|e| e.to_string())
}

/// Get current EPUB info
#[tauri::command]
pub fn epub_get_info(
    epub_manager: State<'_, EpubManager>,
) -> Result<EpubInfo, String> {
    epub_manager.get_info().map_err(|e| e.to_string())
}

/// Get chapter content by index
#[tauri::command]
pub fn epub_get_chapter(
    index: usize,
    epub_manager: State<'_, EpubManager>,
) -> Result<ChapterContent, String> {
    epub_manager.get_chapter(index).map_err(|e| e.to_string())
}

/// Get chapter by content path
#[tauri::command]
pub fn epub_get_chapter_by_path(
    content_path: String,
    epub_manager: State<'_, EpubManager>,
) -> Result<ChapterContent, String> {
    epub_manager.get_chapter_by_path(&content_path).map_err(|e| e.to_string())
}

/// Search EPUB content
#[tauri::command]
pub fn epub_search(
    query: String,
    max_results: Option<usize>,
    epub_manager: State<'_, EpubManager>,
) -> Result<Vec<EpubSearchResult>, String> {
    let max = max_results.unwrap_or(50);
    epub_manager.search(&query, max).map_err(|e| e.to_string())
}

/// Get EPUB cover image as base64
#[tauri::command]
pub fn epub_get_cover(
    epub_manager: State<'_, EpubManager>,
) -> Result<Option<String>, String> {
    epub_manager.get_cover().map_err(|e| e.to_string())
}

// ============================================================================
// AI Commands
// ============================================================================

/// Chat message for the API
#[derive(Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl From<ChatMessage> for Message {
    fn from(msg: ChatMessage) -> Self {
        Message {
            role: match msg.role.as_str() {
                "system" => Role::System,
                "assistant" => Role::Assistant,
                _ => Role::User,
            },
            content: msg.content,
        }
    }
}

/// Check if AI provider is available
#[tauri::command]
pub async fn ai_check_status(
    ai_manager: State<'_, AiManager>,
) -> Result<bool, String> {
    Ok(ai_manager.is_available().await)
}

/// Get AI provider configuration
#[tauri::command]
pub fn ai_get_config(
    ai_manager: State<'_, AiManager>,
) -> Result<ProviderConfig, String> {
    ai_manager.get_config().map_err(|e| e.to_string())
}

/// Set AI provider configuration
#[tauri::command]
pub fn ai_set_config(
    config: ProviderConfig,
    ai_manager: State<'_, AiManager>,
) -> Result<(), String> {
    ai_manager.set_config(config).map_err(|e| e.to_string())
}

/// Send a chat message and get response
#[tauri::command]
pub async fn ai_chat(
    messages: Vec<ChatMessage>,
    system_prompt: Option<String>,
    ai_manager: State<'_, AiManager>,
) -> Result<AiResponse, String> {
    let msgs: Vec<Message> = messages.into_iter().map(|m| m.into()).collect();
    ai_manager.chat(msgs, system_prompt.as_deref()).await.map_err(|e| e.to_string())
}

/// Run a predefined AI task on content
#[tauri::command]
pub async fn ai_run_task(
    task: String,
    content: String,
    ai_manager: State<'_, AiManager>,
) -> Result<AiResponse, String> {
    ai_manager.run_task(&task, &content).await.map_err(|e| e.to_string())
}

/// Simple generate with just a prompt
#[tauri::command]
pub async fn ai_generate(
    prompt: String,
    system_prompt: Option<String>,
    ai_manager: State<'_, AiManager>,
) -> Result<AiResponse, String> {
    ai_manager.generate(&prompt, system_prompt.as_deref()).await.map_err(|e| e.to_string())
}

// ============================================================================
// Provider Commands
// ============================================================================

/// List all registered providers
#[tauri::command]
pub fn provider_list(
    registry: State<'_, ProviderRegistry>,
) -> Vec<ProviderInfo> {
    registry.list()
}

/// List all registered commands
#[tauri::command]
pub fn provider_list_commands(
    registry: State<'_, ProviderRegistry>,
) -> Vec<ProviderCommand> {
    registry.commands.list()
}

/// Get provider state
#[tauri::command]
pub fn provider_get_state(
    name: String,
    registry: State<'_, ProviderRegistry>,
) -> Result<serde_json::Value, String> {
    registry
        .get_provider_state(&name)
        .ok_or_else(|| format!("Provider not found: {}", name))
}

/// Initialize providers for a document
#[tauri::command]
pub fn provider_init_for_document(
    content: String,
    file_path: Option<String>,
    registry: State<'_, ProviderRegistry>,
) -> Result<Vec<ProviderInfo>, String> {
    // Register ALWAYS_ON providers
    let word_count = Box::new(WordCountProvider::new());
    registry.register(word_count, &content, file_path.as_deref())?;

    let markdown_service = Box::new(MarkdownLanguageService::new());
    registry.register(markdown_service, &content, file_path.as_deref())?;

    // Register ON_DEMAND providers
    let coding_agent = Box::new(CodingAgentProvider::new());
    registry.register(coding_agent, &content, file_path.as_deref())?;

    Ok(registry.list())
}

/// Notify providers of content change
#[tauri::command]
pub fn provider_content_changed(
    content: String,
    registry: State<'_, ProviderRegistry>,
) -> Result<(), String> {
    registry.notify_content_changed(&content);
    Ok(())
}

/// Get word count stats
#[tauri::command]
pub fn provider_word_count(
    registry: State<'_, ProviderRegistry>,
) -> Result<serde_json::Value, String> {
    registry
        .get_provider_state("word_count")
        .ok_or_else(|| "Word count provider not initialized".to_string())
}

/// Get markdown document structure (headings, links, code blocks, tasks)
#[tauri::command]
pub fn provider_markdown_structure(
    registry: State<'_, ProviderRegistry>,
) -> Result<serde_json::Value, String> {
    registry
        .get_provider_state("markdown_language_service")
        .ok_or_else(|| "Markdown language service not initialized".to_string())
}

/// Execute a coding agent operation
#[tauri::command]
pub async fn provider_code_operation(
    request: CodeRequest,
    _registry: State<'_, ProviderRegistry>,
    ai_manager: State<'_, AiManager>,
) -> Result<CodeResponse, String> {
    // Check if AI is available
    if !ai_manager.is_available().await {
        return Ok(CodeResponse {
            operation: request.operation,
            result: String::new(),
            success: false,
            error: Some("AI provider not available. Please configure an AI provider in settings.".to_string()),
        });
    }

    // Build the prompt
    let prompt = CodingAgentProvider::build_prompt(&request);
    let system_prompt = request.operation.system_prompt();

    // Call the AI
    match ai_manager.generate(&prompt, Some(system_prompt)).await {
        Ok(response) => Ok(CodeResponse {
            operation: request.operation,
            result: response.content,
            success: true,
            error: None,
        }),
        Err(e) => Ok(CodeResponse {
            operation: request.operation,
            result: String::new(),
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

/// Quick code explanation (convenience wrapper)
#[tauri::command]
pub async fn provider_code_explain(
    code: String,
    language: Option<String>,
    ai_manager: State<'_, AiManager>,
) -> Result<CodeResponse, String> {
    let request = CodeRequest {
        operation: CodeOperation::Explain,
        code,
        instruction: None,
        language,
        cursor_position: None,
        context_before: None,
        context_after: None,
    };

    let prompt = CodingAgentProvider::build_prompt(&request);
    let system_prompt = CodeOperation::Explain.system_prompt();

    match ai_manager.generate(&prompt, Some(system_prompt)).await {
        Ok(response) => Ok(CodeResponse {
            operation: CodeOperation::Explain,
            result: response.content,
            success: true,
            error: None,
        }),
        Err(e) => Ok(CodeResponse {
            operation: CodeOperation::Explain,
            result: String::new(),
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

/// Quick code completion (convenience wrapper)
#[tauri::command]
pub async fn provider_code_complete(
    context_before: String,
    context_after: String,
    language: Option<String>,
    ai_manager: State<'_, AiManager>,
) -> Result<CodeResponse, String> {
    let request = CodeRequest {
        operation: CodeOperation::Complete,
        code: String::new(),
        instruction: None,
        language,
        cursor_position: None,
        context_before: Some(context_before),
        context_after: Some(context_after),
    };

    let prompt = CodingAgentProvider::build_prompt(&request);
    let system_prompt = CodeOperation::Complete.system_prompt();

    match ai_manager.generate(&prompt, Some(system_prompt)).await {
        Ok(response) => Ok(CodeResponse {
            operation: CodeOperation::Complete,
            result: response.content,
            success: true,
            error: None,
        }),
        Err(e) => Ok(CodeResponse {
            operation: CodeOperation::Complete,
            result: String::new(),
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

/// Edit code with instruction (convenience wrapper)
#[tauri::command]
pub async fn provider_code_edit(
    code: String,
    instruction: String,
    language: Option<String>,
    ai_manager: State<'_, AiManager>,
) -> Result<CodeResponse, String> {
    let request = CodeRequest {
        operation: CodeOperation::Edit,
        code,
        instruction: Some(instruction),
        language,
        cursor_position: None,
        context_before: None,
        context_after: None,
    };

    let prompt = CodingAgentProvider::build_prompt(&request);
    let system_prompt = CodeOperation::Edit.system_prompt();

    match ai_manager.generate(&prompt, Some(system_prompt)).await {
        Ok(response) => Ok(CodeResponse {
            operation: CodeOperation::Edit,
            result: response.content,
            success: true,
            error: None,
        }),
        Err(e) => Ok(CodeResponse {
            operation: CodeOperation::Edit,
            result: String::new(),
            success: false,
            error: Some(e.to_string()),
        }),
    }
}
