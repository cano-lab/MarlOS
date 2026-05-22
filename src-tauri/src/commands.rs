//! Tauri commands - IPC interface between frontend and backend
//!
//! These commands are callable from JavaScript/TypeScript via Tauri's invoke API.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::State;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::document::Document;
use crate::kernel::SemanticKernel;
use crate::memory::{MemoryType, SecurityTier};
use crate::pdf::{PdfManager, PdfInfo, RenderedPage, Measurement, PdfPoint};
use crate::epub::{EpubManager, EpubInfo, ChapterContent, SearchResult as EpubSearchResult};
use crate::ai::{AiManager, ProviderConfig, Message, Role, AiResponse};
use crate::providers::{
    ProviderRegistry, ProviderInfo, Command as ProviderCommand,
    WordCountProvider, MarkdownLanguageService, CodingAgentProvider,
    CodeRequest, CodeResponse, CodeOperation,
    Source, SourceType, CitationStyle, CitationGenerator,
    FactCheckResult, WebFetcher,
};
use crate::semantic_search::SemanticSearch;
use crate::semantic_object::{
    SemanticObject, Suid, ContentType,
    FileBoundary, RelationType,
};
use crate::llm_tasks::{
    TaskRunner,
    SummarizeContentTask, AnalyzeCodeTask,
    AnswerQuestionTask, ClassifyTextTask,
};
use crate::mcp::{McpServer, McpContext, ContextSource, ResearchAgent};
use crate::images::{ImageManager, ImageInfo, ToneMapOptions};
use crate::document_versions::{VersionStore, VersionSummary, DocumentVersion, VersionDiff};
use crate::reference_library::{Reference, ReferenceStore, dedup::DuplicateMatch};
use crate::research_project::{ProjectStore, ProjectView, ProjectStatus};

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

/// Get debug info about EPUB structure
#[tauri::command]
pub fn epub_debug_info(
    epub_manager: State<'_, EpubManager>,
) -> Result<crate::epub::EpubDebugInfo, String> {
    epub_manager.get_debug_info().map_err(|e| e.to_string())
}

// ============================================================================
// EPUB Notes Commands
// ============================================================================

use crate::epub_notes::{EpubNotesManager, EpubHighlight};

/// Save an EPUB highlight (create or update)
#[tauri::command]
pub fn epub_notes_save(
    highlight: EpubHighlight,
    notes_manager: State<'_, EpubNotesManager>,
) -> Result<EpubHighlight, String> {
    notes_manager.save(highlight)
}

/// Load an EPUB highlight by ID
#[tauri::command]
pub fn epub_notes_load(
    id: String,
    notes_manager: State<'_, EpubNotesManager>,
) -> Result<Option<EpubHighlight>, String> {
    notes_manager.load(&id)
}

/// Delete an EPUB highlight by ID
#[tauri::command]
pub fn epub_notes_delete(
    id: String,
    notes_manager: State<'_, EpubNotesManager>,
) -> Result<bool, String> {
    notes_manager.delete(&id)
}

/// Get all highlights for a specific book
#[tauri::command]
pub fn epub_notes_get_for_book(
    book_path: String,
    notes_manager: State<'_, EpubNotesManager>,
) -> Result<Vec<EpubHighlight>, String> {
    notes_manager.get_for_book(&book_path)
}

/// Get highlights for a specific book and chapter
#[tauri::command]
pub fn epub_notes_get_for_chapter(
    book_path: String,
    chapter_index: usize,
    notes_manager: State<'_, EpubNotesManager>,
) -> Result<Vec<EpubHighlight>, String> {
    notes_manager.get_for_chapter(&book_path, chapter_index)
}

/// Update the note text for a highlight
#[tauri::command]
pub fn epub_notes_update_note(
    id: String,
    note: String,
    notes_manager: State<'_, EpubNotesManager>,
) -> Result<Option<EpubHighlight>, String> {
    notes_manager.update_note(&id, note)
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
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<bool, String> {
    Ok(ai_manager.is_available().await)
}

/// Get AI provider configuration
#[tauri::command]
pub fn ai_get_config(
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<ProviderConfig, String> {
    ai_manager.get_config().map_err(|e| e.to_string())
}

/// Set AI provider configuration
#[tauri::command]
pub fn ai_set_config(
    config: ProviderConfig,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<(), String> {
    ai_manager.set_config(config).map_err(|e| e.to_string())
}

/// List available models from the AI provider
#[tauri::command]
pub async fn ai_list_models(
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<Vec<String>, String> {
    ai_manager.list_models().await.map_err(|e| e.to_string())
}

/// Send a chat message and get response
#[tauri::command]
pub async fn ai_chat(
    messages: Vec<ChatMessage>,
    system_prompt: Option<String>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<AiResponse, String> {
    let msgs: Vec<Message> = messages.into_iter().map(|m| m.into()).collect();
    ai_manager.chat(msgs, system_prompt.as_deref()).await.map_err(|e| e.to_string())
}

/// Run a predefined AI task on content
#[tauri::command]
pub async fn ai_run_task(
    task: String,
    content: String,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<AiResponse, String> {
    ai_manager.run_task(&task, &content).await.map_err(|e| e.to_string())
}

/// Simple generate with just a prompt
#[tauri::command]
pub async fn ai_generate(
    prompt: String,
    system_prompt: Option<String>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<AiResponse, String> {
    ai_manager.generate(&prompt, system_prompt.as_deref()).await.map_err(|e| e.to_string())
}

// ============================================================================
// Custom Provider Commands
// ============================================================================

use crate::provider_store::{ProviderStore, CustomProviderConfig, ProviderPreset, get_presets};

/// List all custom providers
#[tauri::command]
pub fn custom_provider_list(
    store: State<'_, ProviderStore>,
) -> Result<Vec<CustomProviderConfig>, String> {
    store.list()
}

/// Get the active provider
#[tauri::command]
pub fn custom_provider_get_active(
    store: State<'_, ProviderStore>,
) -> Result<Option<CustomProviderConfig>, String> {
    store.get_active()
}

/// Get the active provider ID
#[tauri::command]
pub fn custom_provider_get_active_id(
    store: State<'_, ProviderStore>,
) -> Result<Option<String>, String> {
    store.get_active_id()
}

/// Get a provider by ID
#[tauri::command]
pub fn custom_provider_get(
    id: String,
    store: State<'_, ProviderStore>,
) -> Result<Option<CustomProviderConfig>, String> {
    store.get(&id)
}

/// Add a new custom provider
#[tauri::command]
pub fn custom_provider_add(
    config: CustomProviderConfig,
    store: State<'_, ProviderStore>,
) -> Result<String, String> {
    store.add(config)
}

/// Update an existing provider
#[tauri::command]
pub fn custom_provider_update(
    id: String,
    config: CustomProviderConfig,
    store: State<'_, ProviderStore>,
) -> Result<(), String> {
    store.update(&id, config)
}

/// Delete a provider
#[tauri::command]
pub fn custom_provider_delete(
    id: String,
    store: State<'_, ProviderStore>,
) -> Result<(), String> {
    store.delete(&id)
}

/// Set the active provider
#[tauri::command]
pub fn custom_provider_set_active(
    id: String,
    store: State<'_, ProviderStore>,
) -> Result<(), String> {
    store.set_active(&id)
}

/// Test a provider connection
#[tauri::command]
pub async fn custom_provider_test(
    id: String,
    store: State<'_, ProviderStore>,
) -> Result<bool, String> {
    store.test_provider(&id).await
}

/// Get provider presets
#[tauri::command]
pub fn custom_provider_get_presets() -> Vec<ProviderPreset> {
    get_presets()
}

/// Create a provider from a preset
#[tauri::command]
pub fn custom_provider_create_from_preset(
    preset_name: String,
    api_key: Option<String>,
) -> Result<CustomProviderConfig, String> {
    ProviderStore::create_from_preset(&preset_name, api_key)
        .ok_or_else(|| format!("Preset not found: {}", preset_name))
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
    ai_manager: State<'_, Arc<AiManager>>,
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
    ai_manager: State<'_, Arc<AiManager>>,
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
    ai_manager: State<'_, Arc<AiManager>>,
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
    ai_manager: State<'_, Arc<AiManager>>,
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

// ============================================================================
// Semantic Object Commands
// ============================================================================

/// Object view for API responses (serializable)
#[derive(Serialize)]
pub struct ObjectView {
    pub suid: String,
    pub name: Option<String>,
    pub path: Option<String>,
    pub content_type: String,
    pub size_bytes: usize,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub security_tier: String,
    pub created_at: String,
    pub modified_at: String,
    pub version: u64,
}

impl From<&SemanticObject> for ObjectView {
    fn from(obj: &SemanticObject) -> Self {
        Self {
            suid: obj.suid.to_string(),
            name: obj.name.clone(),
            path: obj.path.clone(),
            content_type: format!("{:?}", obj.content_type),
            size_bytes: obj.size_bytes,
            tags: obj.tags.clone(),
            summary: obj.summary.clone(),
            security_tier: format!("{:?}", obj.security_tier),
            created_at: obj.created_at.to_rfc3339(),
            modified_at: obj.modified_at.to_rfc3339(),
            version: obj.version,
        }
    }
}

/// Search result for API
#[derive(Serialize)]
pub struct ObjectSearchResult {
    pub object: ObjectView,
    pub score: f32,
    pub match_type: String,
}

/// Waveform similarity components for API
#[derive(Serialize)]
pub struct WaveformSimilarityComponents {
    pub cross_correlation: f32,
    pub spectral: f32,
    pub multiscale: f32,
}

/// Waveform search result for API
#[derive(Serialize)]
pub struct WaveformSearchResult {
    pub object: ObjectView,
    pub score: f32,
    pub waveform: WaveformSimilarityComponents,
    pub saturation_detected: bool,
}

/// Tier change request
#[derive(Deserialize)]
pub struct TierChangeRequest {
    pub suid: String,
    pub new_tier: String,
    pub reason: String,
}

/// Tier change record for audit
#[derive(Serialize)]
pub struct TierChangeRecord {
    pub suid: String,
    pub old_tier: String,
    pub new_tier: String,
    pub reason: String,
    pub changed_at: String,
    pub success: bool,
}

/// Create a new semantic object from text
#[tauri::command]
pub async fn object_create(
    content: String,
    name: Option<String>,
    content_type: Option<String>,
    tags: Option<Vec<String>>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<ObjectView, String> {
    let ct = content_type
        .map(|s| match s.as_str() {
            "text" => ContentType::Text,
            "markdown" => ContentType::Markdown,
            "json" => ContentType::Json,
            _ => ContentType::Text,
        })
        .unwrap_or(ContentType::Text);

    let mut obj = SemanticObject::new(content.as_bytes().to_vec(), ct);

    if let Some(n) = name {
        obj.name = Some(n);
    }
    if let Some(t) = tags {
        obj.tags = t;
    }

    // Store directly in the object store with embedding
    {
        let store = search.store.write().await;
        store.create(&obj).map_err(|e| e.to_string())?;

        // Generate and store embedding
        let embedding = search.embeddings().embed(&content).await
            .map_err(|e| e.to_string())?;
        let model = search.embeddings().model_info().id.clone();
        store.store_embedding(&obj.suid, &embedding, &model)
            .map_err(|e| e.to_string())?;
    }

    log::info!("Created object: {}", obj.suid.short());
    Ok(ObjectView::from(&obj))
}

/// Get an object by SUID
#[tauri::command]
pub async fn object_get(
    suid: String,
    include_content: Option<bool>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<serde_json::Value, String> {
    let suid = Suid::parse(&suid).map_err(|e| format!("Invalid SUID: {}", e))?;

    let obj = search.get(&suid).await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Object not found".to_string())?;

    let include = include_content.unwrap_or(false);

    if include && obj.security_tier == SecurityTier::Open {
        // Return full object with content
        Ok(serde_json::json!({
            "suid": obj.suid.to_string(),
            "name": obj.name,
            "path": obj.path,
            "content": obj.content_as_str(),
            "content_type": format!("{:?}", obj.content_type),
            "size_bytes": obj.size_bytes,
            "tags": obj.tags,
            "summary": obj.summary,
            "security_tier": format!("{:?}", obj.security_tier),
            "created_at": obj.created_at.to_rfc3339(),
            "modified_at": obj.modified_at.to_rfc3339(),
            "version": obj.version,
        }))
    } else {
        // Return view without content
        Ok(serde_json::to_value(ObjectView::from(&obj))
            .map_err(|e| e.to_string())?)
    }
}

/// List objects with pagination
#[tauri::command]
pub async fn object_list(
    limit: Option<usize>,
    offset: Option<usize>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<ObjectView>, String> {
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);

    let store = search.store.read().await;
    let objects = store.list(limit, offset).map_err(|e| e.to_string())?;

    Ok(objects.iter().map(ObjectView::from).collect())
}

/// Delete an object
#[tauri::command]
pub async fn object_delete(
    suid: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<bool, String> {
    let suid = Suid::parse(&suid).map_err(|e| format!("Invalid SUID: {}", e))?;

    search.delete(&suid).await.map_err(|e| e.to_string())
}

/// Search objects semantically
#[tauri::command]
pub async fn object_search(
    query: String,
    limit: Option<usize>,
    max_tier: Option<String>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<ObjectSearchResult>, String> {
    use crate::semantic_search::SearchOptions;

    let tier = max_tier
        .map(|s| match s.as_str() {
            "Open" | "open" => SecurityTier::Open,
            "Guarded" | "guarded" => SecurityTier::Guarded,
            _ => SecurityTier::Sealed,
        })
        .unwrap_or(SecurityTier::Guarded);

    let options = SearchOptions {
        limit: limit.unwrap_or(20),
        max_tier: tier,
        ..Default::default()
    };

    let results = search.search(&query, options).await
        .map_err(|e| format!("{}", e))?;

    Ok(results
        .into_iter()
        .map(|hit| ObjectSearchResult {
            object: ObjectView::from(&hit.object),
            score: hit.score,
            match_type: format!("{:?}", hit.match_type),
        })
        .collect())
}

/// Find objects similar to a given object
#[tauri::command]
pub async fn object_find_similar(
    suid: String,
    limit: Option<usize>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<ObjectSearchResult>, String> {
    let suid = Suid::parse(&suid).map_err(|e| format!("Invalid SUID: {}", e))?;

    let results = search.find_similar(&suid, limit.unwrap_or(10)).await
        .map_err(|e| format!("{}", e))?;

    Ok(results
        .into_iter()
        .map(|hit| ObjectSearchResult {
            object: ObjectView::from(&hit.object),
            score: hit.score,
            match_type: format!("{:?}", hit.match_type),
        })
        .collect())
}

/// Search objects using waveform similarity
///
/// Treats embeddings as signals and uses cross-correlation, spectral analysis,
/// and multi-scale comparison to find similarities that cosine similarity misses.
/// Particularly effective at scale where cosine similarity saturates.
#[tauri::command]
pub async fn object_search_waveform(
    query: String,
    limit: Option<usize>,
    max_tier: Option<String>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<WaveformSearchResult>, String> {
    use crate::semantic_search::SearchOptions;

    let tier = max_tier
        .map(|s| match s.as_str() {
            "Open" | "open" => SecurityTier::Open,
            "Guarded" | "guarded" => SecurityTier::Guarded,
            _ => SecurityTier::Sealed,
        })
        .unwrap_or(SecurityTier::Guarded);

    let options = SearchOptions {
        limit: limit.unwrap_or(20),
        max_tier: tier,
        ..Default::default()
    };

    let results = search.search_waveform(&query, options).await
        .map_err(|e| format!("{}", e))?;

    Ok(results
        .into_iter()
        .map(|hit| WaveformSearchResult {
            object: ObjectView::from(&hit.object),
            score: hit.score,
            waveform: WaveformSimilarityComponents {
                cross_correlation: hit.waveform.cross_correlation,
                spectral: hit.waveform.spectral,
                multiscale: hit.waveform.multiscale,
            },
            saturation_detected: hit.saturation_detected,
        })
        .collect())
}

/// Find objects similar to a given object using waveform similarity
#[tauri::command]
pub async fn object_find_similar_waveform(
    suid: String,
    limit: Option<usize>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<WaveformSearchResult>, String> {
    let suid = Suid::parse(&suid).map_err(|e| format!("Invalid SUID: {}", e))?;

    let results = search.find_similar_waveform(&suid, limit.unwrap_or(10)).await
        .map_err(|e| format!("{}", e))?;

    Ok(results
        .into_iter()
        .map(|hit| WaveformSearchResult {
            object: ObjectView::from(&hit.object),
            score: hit.score,
            waveform: WaveformSimilarityComponents {
                cross_correlation: hit.waveform.cross_correlation,
                spectral: hit.waveform.spectral,
                multiscale: hit.waveform.multiscale,
            },
            saturation_detected: hit.saturation_detected,
        })
        .collect())
}

/// Import a file into the object store
#[tauri::command]
pub async fn object_import_file(
    path: String,
    tags: Option<Vec<String>>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<ObjectView, String> {
    use std::path::Path;
    use crate::semantic_object::CreateOptions;

    let file_path = Path::new(&path);

    let options = tags.map(|t| CreateOptions {
        tags: t,
        ..Default::default()
    });

    let result = FileBoundary::import(file_path, options)
        .map_err(|e| e.to_string())?;

    // Warn if secrets detected
    if result.secrets_detected {
        log::warn!(
            "Secrets detected in imported file: {}. Auto-classified as Sealed.",
            path
        );
    }

    // Store the object
    {
        let store = search.store.write().await;
        store.create(&result.object).map_err(|e| e.to_string())?;

        // Index if text content
        if result.object.content_type.is_text() {
            if let Some(text) = result.object.content_as_str() {
                let embedding = search.embeddings().embed(text).await
                    .map_err(|e| e.to_string())?;
                let model = search.embeddings().model_info().id.clone();
                store.store_embedding(&result.object.suid, &embedding, &model)
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    log::info!(
        "Imported file {} as {} (tier: {:?})",
        path,
        result.object.suid.short(),
        result.detected_tier
    );

    Ok(ObjectView::from(&result.object))
}

/// Import a repository/directory recursively
#[tauri::command]
pub async fn import_repository(
    app: tauri::AppHandle,
    path: String,
    search: State<'_, Arc<SemanticSearch>>,
    pca_cache: State<'_, crate::pca_cache::PCACacheManager>,
) -> Result<String, String> {
    use std::path::Path;
    use tauri::Emitter;
    use walkdir::WalkDir;
    use crate::semantic_object::CreateOptions;

    let repo_path = Path::new(&path);
    if !repo_path.is_dir() {
        return Err(format!("{} is not a directory", path));
    }

    // Get repo name for tagging
    let repo_name = repo_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let _ = app.emit("repo-import-progress", serde_json::json!({
        "stage": "scanning",
        "message": format!("Scanning {}...", repo_name),
        "imported": 0,
        "total": 0
    }));

    // Code file extensions to include
    let code_extensions: std::collections::HashSet<&str> = [
        "rs", "py", "js", "ts", "tsx", "jsx", "go", "java", "c", "cpp", "h", "hpp",
        "cs", "rb", "php", "swift", "kt", "scala", "r", "sql", "sh", "bash", "zsh",
        "yaml", "yml", "toml", "json", "md", "txt", "html", "css", "scss", "less",
        "vue", "svelte", "astro", "ex", "exs", "zig", "nim", "lua", "pl", "pm"
    ].iter().cloned().collect();

    // Directories to skip
    let skip_dirs: std::collections::HashSet<&str> = [
        "node_modules", "target", "dist", "build", ".git", "__pycache__",
        ".next", ".nuxt", "vendor", "venv", ".venv", "env", ".env",
        "coverage", ".cache", ".idea", ".vscode"
    ].iter().cloned().collect();

    // Collect files to import
    let mut files_to_import: Vec<std::path::PathBuf> = Vec::new();

    for entry in WalkDir::new(repo_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            !skip_dirs.contains(name)
        })
    {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                    if code_extensions.contains(ext) {
                        // Skip very large files (>500KB)
                        if let Ok(meta) = entry.metadata() {
                            if meta.len() < 500_000 {
                                files_to_import.push(entry.path().to_path_buf());
                            }
                        }
                    }
                }
            }
        }
    }

    let total = files_to_import.len();
    log::info!("Found {} code files in {}", total, repo_name);

    let _ = app.emit("repo-import-progress", serde_json::json!({
        "stage": "importing",
        "message": format!("Found {} files to import", total),
        "imported": 0,
        "total": total
    }));

    let mut imported = 0;
    let mut chunks_created = 0;
    let mut errors = 0;

    // Chunking config for code files
    use crate::chunking::{chunk_text, ChunkConfig};
    use crate::semantic_object::{Relation, RelationType};
    let chunk_config = ChunkConfig {
        target_size: 2000,
        min_size: 200,
        max_size: 4000,
        overlap: 100,
    };

    for (i, file_path) in files_to_import.iter().enumerate() {
        // Create tags for the file
        let relative_path = file_path.strip_prefix(repo_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Report progress for every file since embedding is the slow part
        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let _ = app.emit("repo-import-progress", serde_json::json!({
            "stage": "importing",
            "message": format!("Processing {}/{}: {}", i + 1, total, file_name),
            "imported": imported,
            "total": total,
            "current_file": relative_path.clone(),
            "chunks_created": chunks_created
        }));

        let tags = vec![
            format!("repo:{}", repo_name),
            format!("path:{}", relative_path),
            "kind:code".to_string(),
        ];

        let options = Some(CreateOptions {
            tags: tags.clone(),
            source_hint: Some(file_path.to_string_lossy().to_string()),
            ..Default::default()
        });

        match FileBoundary::import(file_path, options) {
            Ok(result) => {
                // Store the parent object
                let store = search.store.write().await;
                if let Err(e) = store.create(&result.object) {
                    log::warn!("Failed to store {}: {}", relative_path, e);
                    errors += 1;
                    continue;
                }

                // Index if text content - with proper chunking
                if result.object.content_type.is_text() {
                    if let Some(text) = result.object.content_as_str() {
                        let file_ext = file_path.extension()
                            .and_then(|e| e.to_str());

                        let chunks = chunk_text(text, file_ext, &chunk_config);
                        let num_chunks = chunks.len();

                        if num_chunks == 1 {
                            // Small file - just embed directly
                            let text_to_embed = if text.len() > 8000 {
                                &text[..8000]
                            } else {
                                text
                            };

                            match search.embeddings().embed(text_to_embed).await {
                                Ok(embedding) => {
                                    let model = search.embeddings().model_info().id.clone();
                                    let _ = store.store_embedding(&result.object.suid, &embedding, &model);
                                }
                                Err(e) => {
                                    log::warn!("Failed to embed {}: {}", relative_path, e);
                                }
                            }
                        } else {
                            // Large file - create chunk objects
                            log::info!("Chunking {} into {} parts", relative_path, num_chunks);

                            for (chunk_idx, chunk) in chunks.iter().enumerate() {
                                // Progress update for each chunk
                                let _ = app.emit("repo-import-progress", serde_json::json!({
                                    "stage": "chunking",
                                    "message": format!("Embedding chunk {}/{} of {}", chunk_idx + 1, num_chunks, file_name),
                                    "imported": imported,
                                    "total": total,
                                    "current_file": relative_path.clone(),
                                    "chunk": chunk_idx + 1,
                                    "chunk_total": num_chunks
                                }));

                                // Create chunk object
                                let chunk_name = format!("{}#chunk{}",
                                    result.object.name.as_deref().unwrap_or(file_name),
                                    chunk_idx);

                                let context_suffix = chunk.context.as_ref()
                                    .map(|c| format!(" ({})", c))
                                    .unwrap_or_default();

                                let mut chunk_tags = tags.clone();
                                chunk_tags.push("kind:chunk".to_string());
                                chunk_tags.push(format!("chunk:{}", chunk_idx));
                                chunk_tags.push(format!("parent:{}", result.object.suid));

                                let mut chunk_obj = SemanticObject::from_text(&chunk.text)
                                    .with_name(&format!("{}{}", chunk_name, context_suffix));
                                chunk_obj.tags = chunk_tags;

                                // Link to parent
                                chunk_obj.relations.push(Relation::new(
                                    result.object.suid.clone(),
                                    RelationType::DerivedFrom,
                                ));

                                // Store chunk
                                if let Err(e) = store.create(&chunk_obj) {
                                    log::warn!("Failed to store chunk {} of {}: {}", chunk_idx, relative_path, e);
                                    continue;
                                }

                                // Embed chunk
                                match search.embeddings().embed(&chunk.text).await {
                                    Ok(embedding) => {
                                        let model = search.embeddings().model_info().id.clone();
                                        let _ = store.store_embedding(&chunk_obj.suid, &embedding, &model);
                                        chunks_created += 1;
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to embed chunk {} of {}: {}", chunk_idx, relative_path, e);
                                    }
                                }
                            }

                            // Also embed the parent with first chunk for discoverability
                            if let Some(first_chunk) = chunks.first() {
                                let text_to_embed = if first_chunk.text.len() > 4000 {
                                    &first_chunk.text[..4000]
                                } else {
                                    &first_chunk.text
                                };

                                if let Ok(embedding) = search.embeddings().embed(text_to_embed).await {
                                    let model = search.embeddings().model_info().id.clone();
                                    let _ = store.store_embedding(&result.object.suid, &embedding, &model);
                                }
                            }
                        }
                    }
                }

                imported += 1;
            }
            Err(e) => {
                log::warn!("Failed to import {}: {}", relative_path, e);
                errors += 1;
            }
        }
    }

    // Invalidate PCA cache
    if imported > 0 || chunks_created > 0 {
        pca_cache.invalidate();
    }

    let chunk_msg = if chunks_created > 0 {
        format!(", {} chunks", chunks_created)
    } else {
        String::new()
    };

    let _ = app.emit("repo-import-progress", serde_json::json!({
        "stage": "done",
        "message": format!("Done! Imported {} files{}", imported, chunk_msg),
        "imported": imported,
        "total": total,
        "chunks_created": chunks_created
    }));

    log::info!("Repository import complete: {} files, {} chunks, {} errors", imported, chunks_created, errors);
    Ok(format!("Imported {} files, created {} objects from {} ({} errors)",
        imported, imported + chunks_created, repo_name, errors))
}

/// Export an object to a file
#[tauri::command]
pub async fn object_export_file(
    suid: String,
    dest_path: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<String, String> {
    use std::path::Path;

    let suid = Suid::parse(&suid).map_err(|e| format!("Invalid SUID: {}", e))?;

    let obj = search.get(&suid).await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Object not found".to_string())?;

    let result = FileBoundary::export(&obj, Path::new(&dest_path))
        .map_err(|e| e.to_string())?;

    log::info!("Exported {} to {}", suid.to_string()[..8].to_string(), result.path);

    Ok(result.path)
}

// ============================================================================
// Tier Management Commands (LLM can use these)
// ============================================================================

/// Get the security tier of an object
#[tauri::command]
pub async fn object_get_tier(
    suid: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<serde_json::Value, String> {
    let suid = Suid::parse(&suid).map_err(|e| format!("Invalid SUID: {}", e))?;

    let obj = search.get(&suid).await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Object not found".to_string())?;

    Ok(serde_json::json!({
        "suid": suid.to_string(),
        "tier": format!("{:?}", obj.security_tier),
        "tier_value": obj.security_tier.as_u8(),
        "name": obj.name,
    }))
}

/// Change the security tier of an object (with audit trail)
///
/// LLM can use this to reclassify objects, but CANNOT modify content.
/// Requires a reason for audit purposes.
#[tauri::command]
pub async fn object_set_tier(
    request: TierChangeRequest,
    search: State<'_, Arc<SemanticSearch>>,
    kernel: State<'_, SemanticKernel>,
) -> Result<TierChangeRecord, String> {
    let suid = Suid::parse(&request.suid)
        .map_err(|e| format!("Invalid SUID: {}", e))?;

    let new_tier = match request.new_tier.to_lowercase().as_str() {
        "open" => SecurityTier::Open,
        "guarded" => SecurityTier::Guarded,
        "sealed" => SecurityTier::Sealed,
        _ => return Err(format!("Invalid tier: {}. Use Open, Guarded, or Sealed.", request.new_tier)),
    };

    // Get current object
    let mut obj = search.get(&suid).await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Object not found".to_string())?;

    let old_tier = obj.security_tier;
    let old_tier_str = format!("{:?}", old_tier);
    let new_tier_str = format!("{:?}", new_tier);

    // Don't allow changing if already at the target tier
    if old_tier == new_tier {
        return Ok(TierChangeRecord {
            suid: request.suid,
            old_tier: old_tier_str.clone(),
            new_tier: new_tier_str,
            reason: request.reason,
            changed_at: chrono::Utc::now().to_rfc3339(),
            success: false,
        });
    }

    // Update the tier
    obj.security_tier = new_tier;
    obj.modified_at = chrono::Utc::now();

    // Store the update
    {
        let store = search.store.write().await;
        store.update(&obj).map_err(|e| e.to_string())?;
    }

    // Log the tier change for audit
    let audit_entry = serde_json::json!({
        "event": "tier_change",
        "suid": suid.to_string(),
        "object_name": obj.name,
        "old_tier": old_tier_str,
        "new_tier": new_tier_str,
        "reason": request.reason,
    });

    kernel.memory.store_open(
        &format!("Security tier changed: {} -> {} for {}",
            old_tier_str, new_tier_str,
            obj.name.as_deref().unwrap_or(&suid.short())),
        MemoryType::Event,
        audit_entry,
    ).map_err(|e| e.to_string())?;

    log::info!(
        "Tier changed for {}: {:?} -> {:?} (reason: {})",
        suid.short(),
        old_tier,
        new_tier,
        request.reason
    );

    Ok(TierChangeRecord {
        suid: request.suid,
        old_tier: old_tier_str,
        new_tier: new_tier_str,
        reason: request.reason,
        changed_at: chrono::Utc::now().to_rfc3339(),
        success: true,
    })
}

/// Get tier change history for an object (from audit log)
#[tauri::command]
pub fn object_tier_history(
    suid: String,
    kernel: State<'_, SemanticKernel>,
) -> Result<Vec<serde_json::Value>, String> {
    // Search memory for tier change events for this object
    let results = kernel.memory.search(&format!("tier_change {}", suid), 100)
        .map_err(|e| e.to_string())?;

    let history: Vec<serde_json::Value> = results
        .into_iter()
        .filter(|entry| {
            entry.metadata.get("event")
                .map(|v| v == "tier_change")
                .unwrap_or(false)
                && entry.metadata.get("suid")
                    .map(|v| v.as_str() == Some(&suid))
                    .unwrap_or(false)
        })
        .map(|entry| serde_json::json!({
            "old_tier": entry.metadata.get("old_tier"),
            "new_tier": entry.metadata.get("new_tier"),
            "reason": entry.metadata.get("reason"),
            "changed_at": entry.created_at.to_rfc3339(),
        }))
        .collect();

    Ok(history)
}

/// Add a relation between two objects
#[tauri::command]
pub async fn object_add_relation(
    source_suid: String,
    target_suid: String,
    relation_type: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<bool, String> {
    let source = Suid::parse(&source_suid)
        .map_err(|e| format!("Invalid source SUID: {}", e))?;
    let target = Suid::parse(&target_suid)
        .map_err(|e| format!("Invalid target SUID: {}", e))?;

    let rel_type = match relation_type.to_lowercase().as_str() {
        "references" => RelationType::References,
        "contains" => RelationType::Contains,
        "derivedfrom" | "derived_from" => RelationType::DerivedFrom,
        "relatedto" | "related_to" => RelationType::RelatedTo,
        "versionof" | "version_of" => RelationType::VersionOf,
        "repliesto" | "replies_to" => RelationType::RepliesTo,
        "dependson" | "depends_on" => RelationType::DependsOn,
        other => RelationType::Custom(other.to_string()),
    };

    let store = search.store.write().await;
    store.add_relation(&source, &target, rel_type)
        .map_err(|e| e.to_string())?;

    log::info!("Added relation: {} -> {} ({})", source.short(), target.short(), relation_type);
    Ok(true)
}

// ============================================================================
// Research Provider Commands
// ============================================================================

/// Source view for API responses
#[derive(Serialize)]
pub struct SourceView {
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub source_type: String,
    pub authors: Vec<String>,
    pub published_date: Option<String>,
    pub accessed_date: String,
    pub summary: Option<String>,
    pub key_points: Vec<String>,
    pub tags: Vec<String>,
    pub reliability_score: Option<f32>,
    pub notes: Option<String>,
    pub publisher: Option<String>,
    pub doi: Option<String>,
    pub citations: std::collections::HashMap<String, String>,
}

impl From<&Source> for SourceView {
    fn from(source: &Source) -> Self {
        Self {
            id: source.id.clone(),
            title: source.title.clone(),
            url: source.url.clone(),
            source_type: format!("{:?}", source.source_type),
            authors: source.authors.clone(),
            published_date: source.published_date.clone(),
            accessed_date: source.accessed_date.to_rfc3339(),
            summary: source.summary.clone(),
            key_points: source.key_points.clone(),
            tags: source.tags.clone(),
            reliability_score: source.reliability_score,
            notes: source.notes.clone(),
            publisher: source.publisher.clone(),
            doi: source.doi.clone(),
            citations: source.citations.clone(),
        }
    }
}

/// Add source request
#[derive(Deserialize)]
pub struct AddSourceRequest {
    pub url: Option<String>,
    pub title: Option<String>,
    pub authors: Option<Vec<String>>,
    pub published_date: Option<String>,
    pub source_type: Option<String>,
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
    pub content: Option<String>,
}

/// Add a source from URL (fetches and extracts metadata)
#[tauri::command]
pub async fn research_add_from_url(
    url: String,
    tags: Option<Vec<String>>,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<SourceView, String> {
    log::info!("Adding source from URL: {}", url);

    // Fetch content
    let fetched = WebFetcher::fetch(&url).await?;

    // Create source
    let mut source = Source::new(
        fetched.metadata.title.as_deref().unwrap_or("Untitled"),
        Some(&url),
    );

    source.authors = fetched.metadata.authors;
    source.published_date = fetched.metadata.published_date;
    source.publisher = fetched.metadata.site_name;
    source.content = Some(fetched.text.clone());
    source.tags = tags.unwrap_or_default();

    // Detect source type from URL
    source.source_type = detect_source_type(&url);

    // Generate citations
    source.citations = CitationGenerator::generate_all(&source);

    // Generate summary if AI is available
    if ai_manager.is_available().await {
        if let Ok(summary) = generate_source_summary(&fetched.text, &ai_manager).await {
            source.summary = Some(summary.summary);
            source.key_points = summary.key_points;
        }
    }

    // Store as semantic object
    store_source(&source, &search).await?;

    log::info!("Added source: {} ({})", source.title, source.id);
    Ok(SourceView::from(&source))
}

/// Add a source manually
#[tauri::command]
pub async fn research_add_manual(
    request: AddSourceRequest,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<SourceView, String> {
    let title = request.title.as_deref().unwrap_or("Untitled");
    let mut source = Source::new(title, request.url.as_deref());

    source.authors = request.authors.unwrap_or_default();
    source.published_date = request.published_date;
    source.tags = request.tags.unwrap_or_default();
    source.notes = request.notes;
    source.content = request.content;

    // Parse source type
    if let Some(st) = request.source_type {
        source.source_type = match st.to_lowercase().as_str() {
            "article" => SourceType::Article,
            "paper" => SourceType::Paper,
            "book" => SourceType::Book,
            "webpage" | "web_page" => SourceType::WebPage,
            "video" => SourceType::Video,
            "podcast" => SourceType::Podcast,
            "documentation" | "docs" => SourceType::Documentation,
            "code" | "repository" | "code_repository" => SourceType::CodeRepository,
            other => SourceType::Other(other.to_string()),
        };
    }

    // Generate citations
    source.citations = CitationGenerator::generate_all(&source);

    // Store
    store_source(&source, &search).await?;

    log::info!("Added manual source: {} ({})", source.title, source.id);
    Ok(SourceView::from(&source))
}

/// Get a source by ID
#[tauri::command]
pub async fn research_get_source(
    source_id: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<SourceView, String> {
    let source = get_source_by_id(&source_id, &search).await?;
    Ok(SourceView::from(&source))
}

/// List all sources with pagination support
#[tauri::command]
pub async fn research_list_sources(
    tag_filter: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<SourceView>, String> {
    let store = search.store.read().await;

    // Use tag-filtered query to efficiently get all research sources
    let source_objects = store.list_by_tag("kind:source", 500)
        .map_err(|e| e.to_string())?;

    let mut sources: Vec<Source> = source_objects.iter()
        .filter_map(|obj| source_from_object(obj))
        .collect();

    // Apply tag filter
    if let Some(tag) = tag_filter {
        let tag_lower = tag.to_lowercase();
        sources.retain(|s| s.tags.iter().any(|t| t.to_lowercase().contains(&tag_lower)));
    }

    // Sort by accessed date (newest first)
    sources.sort_by(|a, b| b.accessed_date.cmp(&a.accessed_date));

    // Apply offset for pagination
    let offset = offset.unwrap_or(0);
    if offset > 0 {
        if offset >= sources.len() {
            return Ok(vec![]);
        }
        sources = sources.split_off(offset);
    }

    // Apply limit
    let limit = limit.unwrap_or(100);
    sources.truncate(limit);

    Ok(sources.iter().map(SourceView::from).collect())
}

/// Search sources semantically
#[tauri::command]
pub async fn research_search_sources(
    query: String,
    limit: Option<usize>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<(SourceView, f32)>, String> {
    use crate::semantic_search::SearchOptions;

    let limit = limit.unwrap_or(10);

    // Search with embeddings
    let options = SearchOptions {
        limit: limit * 2,
        ..Default::default()
    };
    let results = search.search(&query, options).await
        .map_err(|e| format!("{}", e))?;

    // Filter to only sources
    let source_results: Vec<(SourceView, f32)> = results.into_iter()
        .filter(|hit| hit.object.tags.contains(&"kind:source".to_string()))
        .filter_map(|hit| {
            source_from_object(&hit.object).map(|s| (SourceView::from(&s), hit.score))
        })
        .take(limit)
        .collect();

    Ok(source_results)
}

/// Delete a source
#[tauri::command]
pub async fn research_delete_source(
    source_id: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<bool, String> {
    // Find the object by looking for its suid in tags
    let store = search.store.write().await;

    let objects = store.list(1000, 0).map_err(|e| e.to_string())?;
    let obj = objects.iter()
        .find(|o| o.tags.contains(&format!("source_id:{}", source_id)))
        .ok_or_else(|| format!("Source not found: {}", source_id))?;

    let suid = obj.suid.clone();
    store.delete(&suid).map_err(|e| e.to_string())?;

    log::info!("Deleted source: {}", source_id);
    Ok(true)
}

/// Update source tags
#[tauri::command]
pub async fn research_update_tags(
    source_id: String,
    tags: Vec<String>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<SourceView, String> {
    let mut source = get_source_by_id(&source_id, &search).await?;
    source.tags = tags;

    // Re-store
    store_source(&source, &search).await?;

    Ok(SourceView::from(&source))
}

/// Add notes to a source
#[tauri::command]
pub async fn research_add_notes(
    source_id: String,
    notes: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<SourceView, String> {
    let mut source = get_source_by_id(&source_id, &search).await?;
    source.notes = Some(notes);

    // Re-store
    store_source(&source, &search).await?;

    Ok(SourceView::from(&source))
}

/// Generate citation for a source
#[tauri::command]
pub async fn research_generate_citation(
    source_id: String,
    style: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<String, String> {
    let source = get_source_by_id(&source_id, &search).await?;

    let citation_style = match style.to_lowercase().as_str() {
        "apa" => CitationStyle::APA,
        "mla" => CitationStyle::MLA,
        "chicago" => CitationStyle::Chicago,
        "harvard" => CitationStyle::Harvard,
        "ieee" => CitationStyle::IEEE,
        "bibtex" => CitationStyle::BibTeX,
        _ => return Err(format!("Unknown citation style: {}", style)),
    };

    Ok(CitationGenerator::generate(&source, citation_style))
}

/// Generate bibliography from multiple sources
#[tauri::command]
pub async fn research_generate_bibliography(
    source_ids: Vec<String>,
    style: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<String, String> {
    let mut sources = Vec::new();
    for id in &source_ids {
        if let Ok(source) = get_source_by_id(id, &search).await {
            sources.push(source);
        }
    }

    let citation_style = match style.to_lowercase().as_str() {
        "apa" => CitationStyle::APA,
        "mla" => CitationStyle::MLA,
        "chicago" => CitationStyle::Chicago,
        "harvard" => CitationStyle::Harvard,
        "ieee" => CitationStyle::IEEE,
        "bibtex" => CitationStyle::BibTeX,
        _ => return Err(format!("Unknown citation style: {}", style)),
    };

    let mut citations: Vec<String> = sources.iter()
        .map(|s| CitationGenerator::generate(s, citation_style))
        .collect();

    citations.sort();
    Ok(citations.join("\n\n"))
}

/// Summarize a source using AI
#[tauri::command]
pub async fn research_summarize_source(
    source_id: String,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<SourceView, String> {
    if !ai_manager.is_available().await {
        return Err("AI not available for summarization".to_string());
    }

    let mut source = get_source_by_id(&source_id, &search).await?;

    let content = source.content.as_ref()
        .ok_or_else(|| "Source has no content to summarize".to_string())?;

    let summary = generate_source_summary(content, &ai_manager).await?;
    source.summary = Some(summary.summary);
    source.key_points = summary.key_points;

    // Re-store
    store_source(&source, &search).await?;

    Ok(SourceView::from(&source))
}

/// Find connections between sources using AI
#[tauri::command]
pub async fn research_find_connections(
    source_ids: Vec<String>,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<serde_json::Value, String> {
    if !ai_manager.is_available().await {
        return Err("AI not available for analysis".to_string());
    }

    let mut sources = Vec::new();
    for id in &source_ids {
        if let Ok(source) = get_source_by_id(id, &search).await {
            sources.push(source);
        }
    }

    if sources.len() < 2 {
        return Ok(serde_json::json!({
            "connections": [],
            "common_themes": [],
            "synthesis": "Need at least 2 sources to find connections."
        }));
    }

    // Build prompt
    let sources_text = sources.iter()
        .map(|s| format!(
            "Source [{}]: {}\nSummary: {}\n",
            s.id,
            s.title,
            s.summary.as_deref().unwrap_or("No summary")
        ))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        r#"Analyze these sources and identify connections, themes, and relationships between them:

{}

Respond in JSON format:
{{
  "connections": [
    {{
      "source_a": "source_id",
      "source_b": "source_id",
      "relationship": "agrees|contradicts|extends|references|similar_topic",
      "description": "brief description of the connection"
    }}
  ],
  "common_themes": ["theme1", "theme2"],
  "synthesis": "A brief synthesis of how these sources relate"
}}"#,
        sources_text
    );

    let system_prompt = "You are a research assistant helping to analyze and connect sources. Always respond with valid JSON.";

    let response = ai_manager.generate(&prompt, Some(system_prompt)).await
        .map_err(|e| format!("AI error: {}", e))?;

    // Parse response
    let json_start = response.content.find('{').unwrap_or(0);
    let json_end = response.content.rfind('}').map(|i| i + 1).unwrap_or(response.content.len());
    let json_str = &response.content[json_start..json_end];

    serde_json::from_str::<serde_json::Value>(json_str)
        .map_err(|e| format!("Failed to parse AI response: {}", e))
}

/// Fact-check a claim against sources
#[tauri::command]
pub async fn research_fact_check(
    claim: String,
    source_ids: Option<Vec<String>>,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<FactCheckResult, String> {
    if !ai_manager.is_available().await {
        return Err("AI not available for fact-checking".to_string());
    }

    // Get sources - either specified or search for relevant ones
    let sources = if let Some(ids) = source_ids {
        let mut sources = Vec::new();
        for id in ids {
            if let Ok(source) = get_source_by_id(&id, &search).await {
                sources.push(source);
            }
        }
        sources
    } else {
        // Search for relevant sources
        use crate::semantic_search::SearchOptions;
        let options = SearchOptions {
            limit: 5,
            ..Default::default()
        };
        let results = search.search(&claim, options).await
            .map_err(|e| format!("{}", e))?;

        results.into_iter()
            .filter(|hit| hit.object.tags.contains(&"kind:source".to_string()))
            .filter_map(|hit| source_from_object(&hit.object))
            .collect()
    };

    if sources.is_empty() {
        return Ok(FactCheckResult {
            claim: claim.clone(),
            verdict: "unverifiable".to_string(),
            confidence: 0.0,
            supporting_sources: Vec::new(),
            contradicting_sources: Vec::new(),
            explanation: "No relevant sources found to verify this claim.".to_string(),
        });
    }

    // Build prompt
    let sources_text = sources.iter()
        .map(|s| format!(
            "Source [{}]: {}\nContent: {}\n",
            s.id,
            s.title,
            s.content.as_deref().unwrap_or(s.summary.as_deref().unwrap_or("No content"))
                .chars().take(1000).collect::<String>()
        ))
        .collect::<Vec<_>>()
        .join("\n---\n");

    let prompt = format!(
        r#"Fact-check the following claim against the provided sources:

CLAIM: {}

SOURCES:
{}

Analyze whether the claim is supported, contradicted, or unverifiable based on these sources.

Respond in JSON format:
{{
  "verdict": "supported|contradicted|partially_supported|unverifiable",
  "confidence": 0.0 to 1.0,
  "supporting_sources": ["source_id1", "source_id2"],
  "contradicting_sources": ["source_id3"],
  "explanation": "Detailed explanation of the verdict"
}}"#,
        claim, sources_text
    );

    let system_prompt = "You are a fact-checker. Analyze claims against provided sources objectively. Always respond with valid JSON.";

    let response = ai_manager.generate(&prompt, Some(system_prompt)).await
        .map_err(|e| format!("AI error: {}", e))?;

    // Parse response
    let json_start = response.content.find('{').unwrap_or(0);
    let json_end = response.content.rfind('}').map(|i| i + 1).unwrap_or(response.content.len());
    let json_str = &response.content[json_start..json_end];

    let mut result: FactCheckResult = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse fact-check result: {}", e))?;

    result.claim = claim;
    Ok(result)
}

// ============================================================================
// Research Helper Functions
// ============================================================================

fn detect_source_type(url: &str) -> SourceType {
    let url_lower = url.to_lowercase();

    if url_lower.contains("arxiv.org") || url_lower.contains("doi.org") ||
       url_lower.contains("scholar.google") || url_lower.contains("researchgate") ||
       url_lower.contains("pubmed") || url_lower.contains("ieee.org") {
        SourceType::Paper
    } else if url_lower.contains("youtube.com") || url_lower.contains("vimeo.com") {
        SourceType::Video
    } else if url_lower.contains("github.com") || url_lower.contains("gitlab.com") {
        SourceType::CodeRepository
    } else if url_lower.contains("docs.") || url_lower.contains("/documentation") ||
              url_lower.contains("/docs/") || url_lower.contains("readme") {
        SourceType::Documentation
    } else if url_lower.ends_with(".pdf") {
        SourceType::Paper
    } else {
        SourceType::WebPage
    }
}

#[derive(Deserialize)]
struct SummaryResult {
    summary: String,
    key_points: Vec<String>,
}

async fn generate_source_summary(content: &str, ai_manager: &AiManager) -> Result<SummaryResult, String> {
    let prompt = format!(
        r#"Analyze the following content and provide:
1. A concise summary (2-3 sentences)
2. 3-5 key points or takeaways

Content:
{}

Respond in this exact JSON format:
{{
  "summary": "...",
  "key_points": ["point 1", "point 2", "point 3"]
}}"#,
        content.chars().take(4000).collect::<String>()
    );

    let system_prompt = "You are a research assistant. Summarize content accurately and extract key points. Always respond with valid JSON.";

    let response = ai_manager.generate(&prompt, Some(system_prompt)).await
        .map_err(|e| format!("AI error: {}", e))?;

    // Parse JSON response
    let json_start = response.content.find('{').unwrap_or(0);
    let json_end = response.content.rfind('}').map(|i| i + 1).unwrap_or(response.content.len());
    let json_str = &response.content[json_start..json_end];

    serde_json::from_str::<SummaryResult>(json_str)
        .map_err(|e| format!("Failed to parse summary: {}", e))
}

async fn store_source(source: &Source, search: &SemanticSearch) -> Result<(), String> {
    // Build content for embedding
    let mut content_parts = vec![source.title.clone()];
    if let Some(summary) = &source.summary {
        content_parts.push(summary.clone());
    }
    if let Some(content) = &source.content {
        content_parts.push(content.chars().take(2000).collect());
    }
    content_parts.extend(source.key_points.clone());

    let content = content_parts.join("\n\n");

    // Create semantic object
    let mut obj = SemanticObject::new(content.as_bytes().to_vec(), ContentType::Text);

    obj.name = Some(source.title.clone());
    obj.path = source.url.clone();

    // Store metadata in tags for filtering
    obj.tags.push("kind:source".to_string());
    obj.tags.push(format!("source_id:{}", source.id));
    obj.tags.push(format!("source_type:{:?}", source.source_type));
    obj.tags.extend(source.tags.iter().map(|t| format!("user_tag:{}", t)));

    // Store full metadata as summary (JSON)
    obj.summary = Some(serde_json::to_string(&source).unwrap_or_default());

    // Store with embedding
    {
        let store = search.store.write().await;
        store.create(&obj).map_err(|e| e.to_string())?;

        let embedding = search.embeddings().embed(&content).await
            .map_err(|e| e.to_string())?;
        let model = search.embeddings().model_info().id.clone();
        store.store_embedding(&obj.suid, &embedding, &model)
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn get_source_by_id(source_id: &str, search: &SemanticSearch) -> Result<Source, String> {
    let store = search.store.read().await;

    let objects = store.list(1000, 0).map_err(|e| e.to_string())?;
    let obj = objects.iter()
        .find(|o| o.tags.contains(&format!("source_id:{}", source_id)))
        .ok_or_else(|| format!("Source not found: {}", source_id))?;

    source_from_object(obj)
        .ok_or_else(|| "Failed to parse source from object".to_string())
}

fn source_from_object(obj: &SemanticObject) -> Option<Source> {
    // First try parsing from summary JSON (the preferred method)
    if let Some(summary) = &obj.summary {
        if let Ok(source) = serde_json::from_str::<Source>(summary) {
            return Some(source);
        }
        // Log parsing failures for debugging
        log::debug!("Failed to parse source from summary JSON for object: {:?}", obj.suid);
    }

    // Fallback: construct Source from object metadata and tags
    // This handles sources that were stored without proper JSON serialization
    let source_id = obj.tags.iter()
        .find(|t| t.starts_with("source_id:"))
        .map(|t| t.trim_start_matches("source_id:").to_string())
        .unwrap_or_else(|| obj.suid.to_string());

    let source_type = obj.tags.iter()
        .find(|t| t.starts_with("source_type:"))
        .map(|t| t.trim_start_matches("source_type:"))
        .and_then(|t| match t {
            "WebPage" => Some(SourceType::WebPage),
            "Article" => Some(SourceType::Article),
            "Paper" => Some(SourceType::Paper),
            "Book" => Some(SourceType::Book),
            "Video" => Some(SourceType::Video),
            "Podcast" => Some(SourceType::Podcast),
            "Documentation" => Some(SourceType::Documentation),
            "CodeRepository" => Some(SourceType::CodeRepository),
            _ => None,
        })
        .unwrap_or(SourceType::WebPage);

    let user_tags: Vec<String> = obj.tags.iter()
        .filter(|t| t.starts_with("user_tag:"))
        .map(|t| t.trim_start_matches("user_tag:").to_string())
        .collect();

    // Only create fallback if we have basic required info
    let title = obj.name.clone().or_else(|| {
        // Try to extract title from content if available
        None
    })?;

    Some(Source {
        id: source_id,
        title,
        url: obj.path.clone(),
        source_type,
        authors: vec![],
        published_date: None,
        accessed_date: obj.created_at,
        content: None,
        summary: obj.summary.clone(), // Use raw summary as source summary
        key_points: vec![],
        tags: user_tags,
        citations: std::collections::HashMap::new(),
        reliability_score: None,
        notes: None,
        publisher: None,
        doi: None,
        isbn: None,
    })
}

// ============================================================================
// Source Discovery (Web Search)
// ============================================================================

/// A discovered source from web search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSource {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub relevance_reason: Option<String>,
}

/// Result of source discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResult {
    pub query_used: String,
    pub sources: Vec<DiscoveredSource>,
}

/// Discover sources based on a research description
#[tauri::command]
pub async fn research_discover_sources(
    description: String,
    max_results: Option<usize>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<DiscoveryResult, String> {
    let max_results = max_results.unwrap_or(10);
    log::info!("Discovering sources for: {}", description);

    // Step 1: Generate search query using AI (if available) or use description directly
    let search_query = if ai_manager.is_available().await {
        let prompt = format!(
            r#"Convert this research description into an effective web search query.
Keep it concise (3-8 words) and focused on finding academic or authoritative sources.

Research description: "{}"

Respond with ONLY the search query, nothing else."#,
            description
        );

        let system = "You are a research assistant that creates effective search queries.";
        match ai_manager.generate(&prompt, Some(system)).await {
            Ok(response) => response.content.trim().to_string(),
            Err(_) => description.clone(),
        }
    } else {
        description.clone()
    };

    log::info!("Using search query: {}", search_query);

    // Step 2: Perform web search using DuckDuckGo HTML API (no API key needed)
    let sources = perform_web_search(&search_query, max_results).await?;

    // Step 3: If AI is available, analyze and rank results
    let sources = if ai_manager.is_available().await && !sources.is_empty() {
        analyze_discovered_sources(&sources, &description, &ai_manager).await
            .unwrap_or(sources)
    } else {
        sources
    };

    Ok(DiscoveryResult {
        query_used: search_query,
        sources,
    })
}

async fn perform_web_search(query: &str, max_results: usize) -> Result<Vec<DiscoveredSource>, String> {
    use reqwest::Client;
    use scraper::{Html, Selector};

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // Use DuckDuckGo HTML search
    let encoded_query = urlencoding::encode(query);
    let url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);

    let response = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("Search request failed: {}", e))?;

    let html = response.text().await
        .map_err(|e| format!("Failed to read search response: {}", e))?;

    // Parse HTML results
    let document = Html::parse_document(&html);

    // DuckDuckGo result selectors
    let result_selector = Selector::parse(".result").unwrap();
    let title_selector = Selector::parse(".result__title a").unwrap();
    let snippet_selector = Selector::parse(".result__snippet").unwrap();

    let mut sources = Vec::new();

    for result in document.select(&result_selector).take(max_results) {
        // Get title and URL
        if let Some(title_elem) = result.select(&title_selector).next() {
            let title = title_elem.text().collect::<String>().trim().to_string();
            let url = title_elem.value().attr("href")
                .map(|h| extract_duckduckgo_url(h))
                .unwrap_or_default();

            // Get snippet
            let snippet = result.select(&snippet_selector)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            if !url.is_empty() && !title.is_empty() {
                sources.push(DiscoveredSource {
                    title,
                    url,
                    snippet,
                    relevance_reason: None,
                });
            }
        }
    }

    log::info!("Found {} search results", sources.len());
    Ok(sources)
}

fn extract_duckduckgo_url(href: &str) -> String {
    // DuckDuckGo wraps URLs in a redirect, extract the actual URL
    if href.contains("uddg=") {
        if let Some(start) = href.find("uddg=") {
            let encoded = &href[start + 5..];
            if let Some(end) = encoded.find('&') {
                return urlencoding::decode(&encoded[..end])
                    .map(|s| s.to_string())
                    .unwrap_or_default();
            }
            return urlencoding::decode(encoded)
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
    }
    href.to_string()
}

async fn analyze_discovered_sources(
    sources: &[DiscoveredSource],
    description: &str,
    ai_manager: &AiManager,
) -> Result<Vec<DiscoveredSource>, String> {
    let sources_list = sources.iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {} - {}", i + 1, s.title, s.snippet))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        r#"Analyze these search results for relevance to the research topic.
For each result, provide a brief reason why it might be useful (or mark as "low relevance").

Research topic: "{}"

Search results:
{}

Respond in JSON format:
{{
  "results": [
    {{"index": 1, "reason": "Contains relevant data on..."}},
    {{"index": 2, "reason": "low relevance"}}
  ]
}}"#,
        description, sources_list
    );

    let system = "You are a research assistant evaluating source relevance. Be concise. Always respond with valid JSON.";

    let response = ai_manager.generate(&prompt, Some(system)).await
        .map_err(|e| format!("AI analysis failed: {}", e))?;

    // Parse response and update sources
    #[derive(Deserialize)]
    struct AnalysisResult {
        results: Vec<SourceAnalysis>,
    }

    #[derive(Deserialize)]
    struct SourceAnalysis {
        index: usize,
        reason: String,
    }

    let json_start = response.content.find('{').unwrap_or(0);
    let json_end = response.content.rfind('}').map(|i| i + 1).unwrap_or(response.content.len());
    let json_str = &response.content[json_start..json_end];

    if let Ok(analysis) = serde_json::from_str::<AnalysisResult>(json_str) {
        let mut updated_sources: Vec<DiscoveredSource> = sources.to_vec();

        for item in analysis.results {
            if item.index > 0 && item.index <= updated_sources.len() {
                let idx = item.index - 1;
                if item.reason.to_lowercase() != "low relevance" {
                    updated_sources[idx].relevance_reason = Some(item.reason);
                }
            }
        }

        // Sort: sources with relevance reasons first
        updated_sources.sort_by(|a, b| {
            b.relevance_reason.is_some().cmp(&a.relevance_reason.is_some())
        });

        Ok(updated_sources)
    } else {
        Ok(sources.to_vec())
    }
}

// ============================================================================
// LLM Task-Based Commands
// ============================================================================
// These commands demonstrate the new unified LLM task system.
// Use TaskRunner to execute any LlmTask with consistent error handling.

/// Summarize content using the LLM task system
#[tauri::command]
pub async fn llm_summarize(
    content: String,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<crate::llm_tasks::SummaryResponse, String> {
    let task = SummarizeContentTask::new(content);
    let runner = TaskRunner::new(&ai_manager);
    runner.execute(task).await
}

/// Analyze code using the LLM task system
#[tauri::command]
pub async fn llm_analyze_code(
    code: String,
    language: String,
    focus: Option<String>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<crate::llm_tasks::tasks::CodeAnalysisResult, String> {
    let mut task = AnalyzeCodeTask::new(code, language);
    if let Some(f) = focus {
        task = task.with_focus(&f);
    }
    let runner = TaskRunner::new(&ai_manager);
    runner.execute(task).await
}

/// Answer a question with optional context
#[tauri::command]
pub async fn llm_answer_question(
    question: String,
    context: Option<String>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<crate::llm_tasks::tasks::AnswerResult, String> {
    let mut task = AnswerQuestionTask::new(question);
    if let Some(ctx) = context {
        task = task.with_context(ctx);
    }
    let runner = TaskRunner::new(&ai_manager);
    runner.execute(task).await
}

/// Classify text into categories
#[tauri::command]
pub async fn llm_classify_text(
    text: String,
    categories: Vec<String>,
    allow_multiple: Option<bool>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<crate::llm_tasks::tasks::ClassificationResult, String> {
    let mut task = ClassifyTextTask::new(text, categories);
    if allow_multiple.unwrap_or(false) {
        task = task.allow_multiple();
    }
    let runner = TaskRunner::new(&ai_manager);
    runner.execute(task).await
}

// ============================================================================
// MCP Research Agent Commands
// ============================================================================

/// Run the MCP-powered research agent to find sources for a topic
#[tauri::command]
pub async fn mcp_research(
    topic: String,
    existing_sources: Option<Vec<ContextSourceInput>>,
    notes: Option<String>,
    ai_manager: State<'_, Arc<AiManager>>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<McpResearchResult, String> {
    log::info!("Starting MCP research for topic: {}", topic);

    // Build context from existing sources
    let mut context = McpContext::default();
    context.research_topic = Some(topic.clone());
    context.notes = notes;

    // Add existing sources to context
    if let Some(sources) = existing_sources {
        context.sources = sources.into_iter()
            .map(|s| ContextSource {
                id: s.id,
                title: s.title,
                url: s.url,
                summary: s.summary,
                tags: s.tags.unwrap_or_default(),
            })
            .collect();
    } else {
        // Load sources from storage
        let store = search.store.read().await;
        if let Ok(objects) = store.list(100, 0) {
            context.sources = objects.iter()
                .filter(|obj| obj.tags.contains(&"kind:source".to_string()))
                .filter_map(|obj| {
                    let source: Option<Source> = obj.summary.as_ref()
                        .and_then(|s| serde_json::from_str(s).ok());
                    source.map(|s| ContextSource {
                        id: s.id,
                        title: s.title,
                        url: s.url,
                        summary: s.summary,
                        tags: s.tags,
                    })
                })
                .collect();
        }
    }

    // Create MCP server and research agent
    let mcp_server = McpServer::new();
    let agent = ResearchAgent::new(&mcp_server, &ai_manager)
        .with_max_iterations(8);

    // Run the agent
    let result = agent.run(&topic, &context).await?;

    Ok(McpResearchResult {
        summary: result.summary,
        sources: result.sources.into_iter()
            .map(|s| McpDiscoveredSource {
                url: s.url,
                title: s.title,
                relevance: s.relevance,
                authors: s.authors,
                year: s.year,
                citation_count: s.citation_count,
                pdf_url: s.pdf_url,
                doi: s.doi,
                venue: s.venue,
                source_type: s.source_type,
            })
            .collect(),
        iterations: result.iterations,
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextSourceInput {
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub summary: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpResearchResult {
    pub summary: String,
    pub sources: Vec<McpDiscoveredSource>,
    pub iterations: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpDiscoveredSource {
    pub url: String,
    pub title: String,
    pub relevance: String,
    // Academic fields (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub venue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
}

/// Run MCP-powered academic research agent (searches scholarly sources)
#[tauri::command]
pub async fn mcp_research_academic(
    topic: String,
    existing_sources: Option<Vec<ContextSourceInput>>,
    notes: Option<String>,
    ai_manager: State<'_, Arc<AiManager>>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<McpResearchResult, String> {
    log::info!("Starting MCP academic research for topic: {}", topic);

    // Build context from existing sources
    let mut context = McpContext::default();
    context.research_topic = Some(topic.clone());
    context.notes = notes;

    // Add existing sources to context
    if let Some(sources) = existing_sources {
        context.sources = sources.into_iter()
            .map(|s| ContextSource {
                id: s.id,
                title: s.title,
                url: s.url,
                summary: s.summary,
                tags: s.tags.unwrap_or_default(),
            })
            .collect();
    } else {
        // Load sources from storage
        let store = search.store.read().await;
        if let Ok(objects) = store.list(100, 0) {
            context.sources = objects.iter()
                .filter(|obj| obj.tags.contains(&"kind:source".to_string()))
                .filter_map(|obj| {
                    let source: Option<Source> = obj.summary.as_ref()
                        .and_then(|s| serde_json::from_str(s).ok());
                    source.map(|s| ContextSource {
                        id: s.id,
                        title: s.title,
                        url: s.url,
                        summary: s.summary,
                        tags: s.tags,
                    })
                })
                .collect();
        }
    }

    // Create MCP server and research agent
    let mcp_server = McpServer::new();
    let agent = ResearchAgent::new(&mcp_server, &ai_manager)
        .with_max_iterations(8);

    // Run the academic research agent
    let result = agent.run_academic(&topic, &context).await?;

    Ok(McpResearchResult {
        summary: result.summary,
        sources: result.sources.into_iter()
            .map(|s| McpDiscoveredSource {
                url: s.url,
                title: s.title,
                relevance: s.relevance,
                authors: s.authors,
                year: s.year,
                citation_count: s.citation_count,
                pdf_url: s.pdf_url,
                doi: s.doi,
                venue: s.venue,
                source_type: s.source_type,
            })
            .collect(),
        iterations: result.iterations,
    })
}

/// Perform a simple web search (non-agent, just returns results)
#[tauri::command]
pub async fn mcp_web_search(
    query: String,
    num_results: Option<usize>,
) -> Result<crate::mcp::web_search::SearchResults, String> {
    let num = num_results.unwrap_or(10).min(20);
    crate::mcp::web_search::search(&query, num).await
}

/// Fetch and extract content from a web page
#[tauri::command]
pub async fn mcp_fetch_page(
    url: String,
    extract_links: Option<bool>,
) -> Result<crate::mcp::web_search::FetchedPage, String> {
    crate::mcp::web_search::fetch_page(&url, extract_links.unwrap_or(false)).await
}

/// Search for academic papers (Semantic Scholar + arXiv)
#[tauri::command]
pub async fn mcp_academic_search(
    query: String,
    num_results: Option<usize>,
) -> Result<crate::mcp::web_search::AcademicSearchResults, String> {
    let num = num_results.unwrap_or(10).min(20);
    crate::mcp::web_search::search_academic(&query, num).await
}

// ============================================================================
// Paper Generator Commands
// ============================================================================

use crate::paper_generator::{
    Paper, PaperView, PaperSection, SectionView, PaperStatus, PaperType,
    PaperPipeline, PaperStore, ChunkingProgress, ExtractionProgress, WritingProgress, ReviewResult,
    ExportFormat, ExportOptions, PaperExporter,
};

/// Request to create a new paper
#[derive(Deserialize)]
pub struct CreatePaperRequest {
    pub title: String,
    pub research_question: String,
    pub paper_type: Option<String>,
    pub citation_style: Option<String>,
    pub thesis: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Create a new research paper
#[tauri::command]
pub async fn paper_create(
    request: CreatePaperRequest,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<PaperView, String> {
    let mut paper = Paper::new(&request.title, &request.research_question);

    // Set paper type
    if let Some(pt) = request.paper_type {
        paper.paper_type = match pt.to_lowercase().as_str() {
            "research_paper" | "research" => PaperType::ResearchPaper,
            "literature_review" | "literature" => PaperType::LiteratureReview,
            "argumentative" | "argumentative_essay" => PaperType::ArgumentativeEssay,
            "expository" | "expository_essay" => PaperType::ExpositoryEssay,
            "case_study" | "case" => PaperType::CaseStudy,
            "technical_report" | "technical" => PaperType::TechnicalReport,
            "thesis" | "dissertation" => PaperType::Thesis,
            other => PaperType::Custom(other.to_string()),
        };
    }

    // Set citation style
    if let Some(cs) = request.citation_style {
        paper.citation_style = match cs.to_lowercase().as_str() {
            "apa" => CitationStyle::APA,
            "mla" => CitationStyle::MLA,
            "chicago" => CitationStyle::Chicago,
            "harvard" => CitationStyle::Harvard,
            "ieee" => CitationStyle::IEEE,
            "bibtex" => CitationStyle::BibTeX,
            _ => CitationStyle::APA,
        };
    }

    if let Some(thesis) = request.thesis {
        paper.thesis = Some(thesis);
    }

    if let Some(tags) = request.tags {
        paper.tags = tags;
    }

    // Initialize standard sections based on paper type
    paper.initialize_standard_sections();

    // Store the paper
    let store = PaperStore::new(search.inner().clone());
    store.save(&paper).await?;

    log::info!("Created paper: {} ({})", paper.title, paper.id);
    Ok(PaperView::from(&paper))
}

/// Get a paper by ID
#[tauri::command]
pub async fn paper_get(
    paper_id: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Paper, String> {
    let store = PaperStore::new(search.inner().clone());
    store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))
}

/// List all papers
#[tauri::command]
pub async fn paper_list(
    limit: Option<usize>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<PaperView>, String> {
    let store = PaperStore::new(search.inner().clone());
    store.list(limit.unwrap_or(50)).await
}

/// Delete a paper
#[tauri::command]
pub async fn paper_delete(
    paper_id: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<bool, String> {
    let store = PaperStore::new(search.inner().clone());
    store.delete(&paper_id).await
}

/// Add sources to a paper
#[tauri::command]
pub async fn paper_add_sources(
    paper_id: String,
    source_ids: Vec<String>,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<usize, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    let mut pipeline = PaperPipeline::new(
        Arc::clone(ai_manager.inner()),
        search.inner().clone(),
        paper,
    );

    let added = pipeline.add_sources(source_ids).await?;
    log::info!("Added {} sources to paper {}", added, paper_id);
    Ok(added)
}

/// Chunk sources for a paper
#[tauri::command]
pub async fn paper_chunk_sources(
    paper_id: String,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<ChunkingProgress, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    let mut pipeline = PaperPipeline::new(
        Arc::clone(ai_manager.inner()),
        search.inner().clone(),
        paper,
    );

    pipeline.chunk_sources().await
}

/// Extract findings from sources
#[tauri::command]
pub async fn paper_extract_findings(
    paper_id: String,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<ExtractionProgress, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    let mut pipeline = PaperPipeline::new(
        Arc::clone(ai_manager.inner()),
        search.inner().clone(),
        paper,
    );

    // First chunk the sources
    pipeline.chunk_sources().await?;

    // Then extract findings
    pipeline.extract_findings().await
}

/// Generate paper outline
#[tauri::command]
pub async fn paper_generate_outline(
    paper_id: String,
    thesis_hint: Option<String>,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<Vec<SectionView>, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    let mut pipeline = PaperPipeline::new(
        Arc::clone(ai_manager.inner()),
        search.inner().clone(),
        paper,
    );

    let sections = pipeline.generate_outline(thesis_hint.as_deref()).await?;
    Ok(sections.iter().map(SectionView::from).collect())
}

/// Update paper outline (reorder/modify sections)
#[tauri::command]
pub async fn paper_update_outline(
    paper_id: String,
    sections: Vec<SectionUpdateRequest>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<SectionView>, String> {
    let store = PaperStore::new(search.inner().clone());
    let mut paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    // Update sections based on request
    for update in sections {
        if let Some(section) = paper.get_section_mut(&update.section_id) {
            if let Some(title) = update.title {
                section.title = title;
            }
            if let Some(order) = update.order {
                section.order = order;
            }
            if let Some(target_words) = update.target_word_count {
                section.target_word_count = Some(target_words);
            }
        }
    }

    // Re-sort sections by order
    paper.sections.sort_by_key(|s| s.order);

    // Save
    store.save(&paper).await?;

    Ok(paper.sections.iter().map(SectionView::from).collect())
}

#[derive(Deserialize)]
pub struct SectionUpdateRequest {
    pub section_id: String,
    pub title: Option<String>,
    pub order: Option<usize>,
    pub target_word_count: Option<usize>,
}

/// Write a single section
#[tauri::command]
pub async fn paper_write_section(
    paper_id: String,
    section_id: String,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<PaperSection, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    let mut pipeline = PaperPipeline::new(
        Arc::clone(ai_manager.inner()),
        search.inner().clone(),
        paper,
    );

    pipeline.write_section(&section_id).await
}

/// Write all pending sections
#[tauri::command]
pub async fn paper_write_all(
    paper_id: String,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<WritingProgress, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    let mut pipeline = PaperPipeline::new(
        Arc::clone(ai_manager.inner()),
        search.inner().clone(),
        paper,
    );

    pipeline.write_all_sections().await
}

/// Review a section
#[tauri::command]
pub async fn paper_review_section(
    paper_id: String,
    section_id: String,
    focus_areas: Option<Vec<String>>,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<ReviewResult, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    let mut pipeline = PaperPipeline::new(
        Arc::clone(ai_manager.inner()),
        search.inner().clone(),
        paper,
    );

    pipeline.review_section(&section_id, focus_areas).await
}

/// Update section content manually
#[tauri::command]
pub async fn paper_update_section(
    paper_id: String,
    section_id: String,
    content: String,
    search: State<'_, Arc<SemanticSearch>>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<bool, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    let mut pipeline = PaperPipeline::new(
        Arc::clone(ai_manager.inner()),
        search.inner().clone(),
        paper,
    );

    pipeline.update_section(&section_id, &content).await
}

/// Export request
#[derive(Deserialize)]
pub struct ExportRequest {
    pub format: String,
    pub include_title_page: Option<bool>,
    pub include_toc: Option<bool>,
    pub include_bibliography: Option<bool>,
    pub number_sections: Option<bool>,
}

/// Export paper to a format
#[tauri::command]
pub async fn paper_export(
    paper_id: String,
    request: ExportRequest,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<String, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    // Load sources for citation processing
    let mut sources = Vec::new();
    for source_id in &paper.source_ids {
        if let Ok(source) = get_source_by_id(source_id, &search).await {
            sources.push(source);
        }
    }

    let format = match request.format.to_lowercase().as_str() {
        "markdown" | "md" => ExportFormat::Markdown,
        "html" => ExportFormat::Html,
        "text" | "plain" | "txt" => ExportFormat::PlainText,
        "latex" | "tex" => ExportFormat::LaTeX,
        _ => ExportFormat::Markdown,
    };

    let options = ExportOptions {
        format,
        include_title_page: request.include_title_page.unwrap_or(true),
        include_toc: request.include_toc.unwrap_or(true),
        include_abstract: true,
        include_bibliography: request.include_bibliography.unwrap_or(true),
        include_page_numbers: true,
        number_sections: request.number_sections.unwrap_or(true),
        use_rendered_content: true,
    };

    PaperExporter::export(&paper, &sources, &options)
}

/// Export paper to LaTeX with companion .bib file
#[tauri::command]
pub async fn paper_export_latex(
    paper_id: String,
    preset: Option<String>,
    search: State<'_, Arc<SemanticSearch>>,
    ref_store: State<'_, ReferenceStore>,
) -> Result<crate::paper_generator::latex_export::LaTeXOutput, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    // Load sources for citation processing
    let mut sources = Vec::new();
    for source_id in &paper.source_ids {
        if let Ok(source) = get_source_by_id(source_id, &search).await {
            sources.push(source);
        }
    }

    // Load references from the reference library
    let references = ref_store.list(None, None).unwrap_or_default();

    let options = match preset.as_deref() {
        Some("ieee") => crate::paper_generator::latex_export::LaTeXExportOptions::ieee(),
        Some("apa") => crate::paper_generator::latex_export::LaTeXExportOptions::apa(),
        Some("thesis") => crate::paper_generator::latex_export::LaTeXExportOptions::thesis(),
        _ => crate::paper_generator::latex_export::LaTeXExportOptions::default(),
    };

    crate::paper_generator::latex_export::export_latex(&paper, &sources, &references, &options)
}

/// Get paper sections
#[tauri::command]
pub async fn paper_get_sections(
    paper_id: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<SectionView>, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    Ok(paper.sections.iter().map(SectionView::from).collect())
}

/// Get paper findings
#[tauri::command]
pub async fn paper_get_findings(
    paper_id: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<crate::paper_generator::KeyFinding>, String> {
    let store = PaperStore::new(search.inner().clone());
    let paper = store.get(&paper_id).await?
        .ok_or_else(|| format!("Paper not found: {}", paper_id))?;

    Ok(paper.findings)
}

// ============================================================================
// Autonomous Research Agent Commands
// ============================================================================

use crate::research_agent::{AutonomousResearchAgent, Interest, Article, ArticleCluster};

/// Interest view for frontend
#[derive(Serialize)]
pub struct InterestView {
    pub id: String,
    pub topic: String,
    pub queries: Vec<String>,
    pub sources: Vec<String>,
    pub priority: u8,
    pub max_articles: usize,
    pub created_at: String,
    pub last_sweep: Option<String>,
    pub active: bool,
}

impl From<&Interest> for InterestView {
    fn from(i: &Interest) -> Self {
        Self {
            id: i.id.clone(),
            topic: i.topic.clone(),
            queries: i.queries.clone(),
            sources: i.sources.clone(),
            priority: i.priority,
            max_articles: i.max_articles,
            created_at: i.created_at.to_rfc3339(),
            last_sweep: i.last_sweep.map(|d| d.to_rfc3339()),
            active: i.active,
        }
    }
}

/// Article view for frontend
#[derive(Serialize)]
pub struct ArticleView {
    pub id: String,
    pub title: String,
    pub url: String,
    pub findings: Vec<String>,
    pub interest_id: String,
    pub interest_topic: Option<String>,
    pub relevance: f32,
    pub discovered_at: String,
    pub source_type: String,
    pub suid: Option<String>,
    pub cluster_id: Option<String>,
    pub cluster_theme: Option<String>,
}

/// Article cluster view for frontend
#[derive(Serialize)]
pub struct ArticleClusterView {
    pub id: String,
    pub theme: String,
    pub article_ids: Vec<String>,
    pub article_count: usize,
    pub cohesion: f32,
    pub key_findings: Vec<String>,
    pub created_at: String,
}

/// Research digest view for frontend
#[derive(Serialize)]
pub struct ResearchDigestView {
    pub sweep_time: String,
    pub interests_searched: usize,
    pub new_articles: usize,
    pub articles_clustered: usize,
    pub clusters_created: usize,
    pub summary: String,
    pub articles_by_interest: Vec<(String, String)>, // (topic, count)
}

/// Add a research interest to track
#[tauri::command]
pub async fn research_add_interest(
    topic: String,
    queries: Vec<String>,
    sources: Option<Vec<String>>,
    priority: Option<u8>,
    max_articles: Option<usize>,
    agent: State<'_, std::sync::Arc<tokio::sync::RwLock<AutonomousResearchAgent>>>,
) -> Result<String, String> {
    let mut interest = Interest::new(&topic, queries);

    if let Some(srcs) = sources {
        interest = interest.with_sources(srcs);
    }
    if let Some(p) = priority {
        interest = interest.with_priority(p);
    }
    if let Some(m) = max_articles {
        interest.max_articles = m;
    }

    let agent_ref = agent.read().await;
    agent_ref.add_interest(interest.clone()).await?;

    Ok(interest.id)
}

/// Remove a research interest
#[tauri::command]
pub async fn research_remove_interest(
    interest_id: String,
    agent: State<'_, std::sync::Arc<tokio::sync::RwLock<AutonomousResearchAgent>>>,
) -> Result<(), String> {
    let agent_ref = agent.read().await;
    agent_ref.remove_interest(&interest_id).await
}

/// List all research interests
#[tauri::command]
pub async fn research_list_interests(
    agent: State<'_, std::sync::Arc<tokio::sync::RwLock<AutonomousResearchAgent>>>,
) -> Result<Vec<InterestView>, String> {
    let agent_ref = agent.read().await;
    let interests = agent_ref.get_interests().await;
    Ok(interests.iter().map(InterestView::from).collect())
}

/// Run a research sweep
#[tauri::command]
pub async fn research_run_sweep(
    agent: State<'_, std::sync::Arc<tokio::sync::RwLock<AutonomousResearchAgent>>>,
) -> Result<ResearchDigestView, String> {
    let agent_ref = agent.read().await;
    let digest = agent_ref.run_sweep().await?;

    // Convert articles_by_interest to a simpler format
    let articles_by_interest: Vec<(String, String)> = digest.articles_by_interest
        .into_iter()
        .map(|(interest_id, article_ids)| (interest_id, article_ids.len().to_string()))
        .collect();

    Ok(ResearchDigestView {
        sweep_time: digest.sweep_time.to_rfc3339(),
        interests_searched: digest.interests_searched,
        new_articles: digest.new_articles,
        articles_clustered: digest.articles_clustered,
        clusters_created: digest.clusters_created,
        summary: digest.summary,
        articles_by_interest,
    })
}

/// Discover papers based on user's reading preferences from the reference library.
/// Mines keywords, authors, tags, and reading history to auto-generate search interests,
/// then runs a sweep to find new relevant papers.
#[tauri::command]
pub async fn research_discover_from_library(
    agent: State<'_, std::sync::Arc<tokio::sync::RwLock<AutonomousResearchAgent>>>,
    ref_store: State<'_, ReferenceStore>,
) -> Result<ResearchDigestView, String> {
    let references = ref_store.list(None, None).map_err(|e| e.to_string())?;

    let agent_ref = agent.read().await;
    let digest = agent_ref.discover_from_preferences(&references).await?;

    let articles_by_interest: Vec<(String, String)> = digest.articles_by_interest
        .into_iter()
        .map(|(interest_id, article_ids)| (interest_id, article_ids.len().to_string()))
        .collect();

    Ok(ResearchDigestView {
        sweep_time: digest.sweep_time.to_rfc3339(),
        interests_searched: digest.interests_searched,
        new_articles: digest.new_articles,
        articles_clustered: digest.articles_clustered,
        clusters_created: digest.clusters_created,
        summary: digest.summary,
        articles_by_interest,
    })
}

/// Get all article clusters
#[tauri::command]
pub async fn research_get_clusters(
    agent: State<'_, std::sync::Arc<tokio::sync::RwLock<AutonomousResearchAgent>>>,
) -> Result<Vec<ArticleClusterView>, String> {
    let agent_ref = agent.read().await;
    let clusters = agent_ref.get_clusters().await;

    Ok(clusters.into_iter().map(|c| {
        let article_count = c.article_ids.len();
        ArticleClusterView {
            id: c.id,
            theme: c.theme,
            article_ids: c.article_ids,
            article_count,
            cohesion: c.cohesion,
            key_findings: c.key_findings,
            created_at: c.created_at.to_rfc3339(),
        }
    }).collect())
}

/// Get articles in a cluster
#[tauri::command]
pub async fn research_get_cluster_articles(
    cluster_id: String,
    agent: State<'_, std::sync::Arc<tokio::sync::RwLock<AutonomousResearchAgent>>>,
) -> Result<Vec<ArticleView>, String> {
    let agent_ref = agent.read().await;
    let articles = agent_ref.get_cluster_articles(&cluster_id).await;

    // Get interests for topic lookup
    let interests = agent_ref.get_interests().await;
    let interest_topics: std::collections::HashMap<String, String> = interests
        .into_iter()
        .map(|i| (i.id, i.topic))
        .collect();

    Ok(articles.into_iter().map(|a| ArticleView {
        id: a.id,
        title: a.title,
        url: a.url,
        findings: a.findings,
        interest_id: a.interest_id.clone(),
        interest_topic: interest_topics.get(&a.interest_id).cloned(),
        relevance: a.relevance,
        discovered_at: a.discovered_at.to_rfc3339(),
        source_type: a.source_type,
        suid: a.suid.map(|s| s.to_string()),
        cluster_id: a.cluster_id,
        cluster_theme: None, // Would populate from cluster lookup
    }).collect())
}

/// Get articles by interest
#[tauri::command]
pub async fn research_get_articles_by_interest(
    interest_id: String,
    agent: State<'_, std::sync::Arc<tokio::sync::RwLock<AutonomousResearchAgent>>>,
) -> Result<Vec<ArticleView>, String> {
    let agent_ref = agent.read().await;
    let articles = agent_ref.get_articles_by_interest(&interest_id).await;

    // Get interest for topic lookup
    let interests = agent_ref.get_interests().await;
    let interest_topic = interests.iter()
        .find(|i| i.id == interest_id)
        .map(|i| i.topic.clone());

    Ok(articles.into_iter().map(|a| ArticleView {
        id: a.id,
        title: a.title,
        url: a.url,
        findings: a.findings,
        interest_id: a.interest_id,
        interest_topic: interest_topic.clone(),
        relevance: a.relevance,
        discovered_at: a.discovered_at.to_rfc3339(),
        source_type: a.source_type,
        suid: a.suid.map(|s| s.to_string()),
        cluster_id: a.cluster_id,
        cluster_theme: None,
    }).collect())
}

/// Find similar articles using waveform similarity
#[tauri::command]
pub async fn research_find_similar_articles(
    article_id: String,
    limit: Option<usize>,
    agent: State<'_, std::sync::Arc<tokio::sync::RwLock<AutonomousResearchAgent>>>,
) -> Result<Vec<ArticleView>, String> {
    let agent_ref = agent.read().await;
    let articles = agent_ref.find_similar_articles(&article_id, limit.unwrap_or(10)).await?;

    Ok(articles.into_iter().map(|a| ArticleView {
        id: a.id,
        title: a.title,
        url: a.url,
        findings: a.findings,
        interest_id: a.interest_id,
        interest_topic: None,
        relevance: 0.0, // Would calculate
        discovered_at: a.discovered_at.to_rfc3339(),
        source_type: a.source_type,
        suid: a.suid.map(|s| s.to_string()),
        cluster_id: a.cluster_id,
        cluster_theme: None,
    }).collect())
}

// ============================================================================
// Proactive Intelligence Commands
// ============================================================================

use crate::proactive::{ProactiveSuggestion, ProactiveConfig};

/// Suggestion view for frontend
#[derive(Serialize)]
pub struct SuggestionView {
    pub id: String,
    pub suggestion_type: String,
    pub relevance: f32,
    pub title: String,
    pub content: String,
    pub reason: String,
    pub source_ids: Vec<String>,
    pub generated_at: String,
    pub seen: bool,
}

impl From<&ProactiveSuggestion> for SuggestionView {
    fn from(s: &ProactiveSuggestion) -> Self {
        Self {
            id: s.id.clone(),
            suggestion_type: format!("{:?}", s.suggestion_type),
            relevance: s.relevance,
            title: s.title.clone(),
            content: s.content.clone(),
            reason: s.reason.clone(),
            source_ids: s.source_ids.clone(),
            generated_at: s.generated_at.to_rfc3339(),
            seen: s.seen,
        }
    }
}

/// Get proactive suggestions for a file being opened
#[tauri::command]
pub async fn proactive_on_file_opened(
    file_path: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<SuggestionView>, String> {
    let mut engine = crate::proactive::ProactiveEngine::new(search.inner().clone());
    let suggestions = engine.on_file_opened(&file_path).await;
    Ok(suggestions.iter().map(SuggestionView::from).collect())
}

/// Get proactive suggestions for a query
#[tauri::command]
pub async fn proactive_on_query(
    query: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<SuggestionView>, String> {
    let mut engine = crate::proactive::ProactiveEngine::new(search.inner().clone());
    let suggestions = engine.on_query(&query).await;
    Ok(suggestions.iter().map(SuggestionView::from).collect())
}

/// Get proactive suggestions when starting a session
#[tauri::command]
pub async fn proactive_on_session_start(
    project_path: Option<String>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<SuggestionView>, String> {
    let mut engine = crate::proactive::ProactiveEngine::new(search.inner().clone());
    let suggestions = engine.on_session_start(project_path.as_deref()).await;
    Ok(suggestions.iter().map(SuggestionView::from).collect())
}

/// Check for decision conflicts
#[tauri::command]
pub async fn proactive_check_decision(
    topic: String,
    proposed_choice: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<SuggestionView>, String> {
    let mut engine = crate::proactive::ProactiveEngine::new(search.inner().clone());
    let suggestions = engine.on_decision_context(&topic, &proposed_choice).await;
    Ok(suggestions.iter().map(SuggestionView::from).collect())
}

/// Log a decision for future reference
#[tauri::command]
pub async fn proactive_log_decision(
    topic: String,
    choice: String,
    reasoning: String,
    project: Option<String>,
    file_path: Option<String>,
    tags: Option<Vec<String>>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<String, String> {
    use crate::semantic_object::{ContentType, SemanticObject};
    use crate::memory::SecurityTier;

    // Create decision content
    let content = format!(
        "# Decision: {}\n\n## Choice\n{}\n\n## Reasoning\n{}",
        topic, choice, reasoning
    );

    let mut obj = SemanticObject::new(
        content.as_bytes().to_vec(),
        ContentType::Markdown,
    );

    obj.name = Some(format!("Decision: {}", topic));
    obj.summary = Some(format!("{}: {}", topic, choice));
    obj.security_tier = SecurityTier::Guarded;

    // Add metadata
    obj.metadata.insert("topic".to_string(), serde_json::json!(topic));
    obj.metadata.insert("choice".to_string(), serde_json::json!(choice));
    obj.metadata.insert("reasoning".to_string(), serde_json::json!(reasoning));
    if let Some(proj) = project {
        obj.metadata.insert("project".to_string(), serde_json::json!(proj));
    }
    if let Some(path) = file_path {
        obj.metadata.insert("file_path".to_string(), serde_json::json!(path));
    }

    // Add tags
    obj.tags.push("decision".to_string());
    if let Some(custom_tags) = tags {
        obj.tags.extend(custom_tags);
    }

    // Store the decision
    let suid = obj.suid.to_string();
    let store = search.store.blocking_write();
    store.create(&obj).map_err(|e| e.to_string())?;

    // Generate embedding
    drop(store);
    let embedding_text = format!("{} {} {}", topic, choice, reasoning);
    if let Ok(embedding) = search.embeddings().embed(&embedding_text).await {
        let store = search.store.blocking_write();
        let model = search.embeddings().model_info().id.clone();
        let _ = store.store_embedding(&obj.suid, &embedding, &model);
    }

    Ok(suid)
}

/// Get decision statistics
#[tauri::command]
pub async fn proactive_decision_stats(
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<serde_json::Value, String> {
    let tracker = crate::proactive::DecisionTracker::new(search.inner().clone());
    let stats = tracker.get_stats().await?;

    Ok(serde_json::json!({
        "total_decisions": stats.total_decisions,
        "recent_decisions": stats.recent_decisions,
        "top_topics": stats.top_topics.into_iter()
            .map(|(topic, count)| serde_json::json!({"topic": topic, "count": count}))
            .collect::<Vec<_>>(),
    }))
}

// ============================================================================
// Mobile Capture Commands
// ============================================================================

/// Quick capture from mobile - saves text, photo references, or voice notes
#[tauri::command]
pub async fn quick_capture(
    capture_type: String,
    content: String,
    tags: Option<Vec<String>>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<String, String> {
    use crate::semantic_object::{ContentType, SemanticObject};
    use crate::memory::SecurityTier;

    // Parse the JSON content
    let capture_data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid capture data: {}", e))?;

    let title = capture_data.get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Quick Capture")
        .to_string();

    let text_content = capture_data.get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Create markdown content based on type
    let markdown_content = match capture_type.as_str() {
        "photo" => {
            let photo_ref = capture_data.get("photo")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("# {}\n\n![Photo]({})\n\n{}", title, photo_ref, text_content)
        }
        "voice" => {
            format!("# {} (Voice Note)\n\n{}", title, text_content)
        }
        _ => {
            format!("# {}\n\n{}", title, text_content)
        }
    };

    let mut obj = SemanticObject::new(
        markdown_content.as_bytes().to_vec(),
        ContentType::Markdown,
    );

    obj.name = Some(title);
    obj.security_tier = SecurityTier::Guarded;

    // Add capture metadata
    obj.metadata.insert("capture_type".to_string(), serde_json::json!(capture_type));
    obj.metadata.insert("source".to_string(), serde_json::json!("mobile_capture"));
    if let Some(captured_at) = capture_data.get("captured_at") {
        obj.metadata.insert("captured_at".to_string(), captured_at.clone());
    }

    // Add tags
    obj.tags.push("capture".to_string());
    obj.tags.push(format!("capture:{}", capture_type));
    if let Some(custom_tags) = tags {
        obj.tags.extend(custom_tags);
    }

    // Store
    let suid = obj.suid.to_string();
    {
        let store = search.store.write().await;
        store.create(&obj).map_err(|e| e.to_string())?;
    }

    // Generate embedding from the text content
    if !text_content.is_empty() {
        if let Ok(embedding) = search.embeddings().embed(&text_content).await {
            let store = search.store.write().await;
            let model = search.embeddings().model_info().id.clone();
            let _ = store.store_embedding(&obj.suid, &embedding, &model);
        }
    }

    log::info!("Quick capture saved: {} ({})", obj.name.unwrap_or_default(), suid);
    Ok(suid)
}

/// Get recent objects for mobile memory browser
#[tauri::command]
pub async fn get_recent_objects(
    limit: Option<usize>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(20);
    let store = search.store.read().await;

    let objects = store.list(limit, 0).map_err(|e| e.to_string())?;

    // Sort by modified_at descending and convert to JSON
    let mut sorted: Vec<_> = objects.iter().collect();
    sorted.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

    Ok(sorted.into_iter()
        .take(limit)
        .map(|obj| {
            serde_json::json!({
                "id": obj.suid.to_string(),
                "kind": obj.tags.iter()
                    .find(|t| t.starts_with("kind:"))
                    .map(|t| t.strip_prefix("kind:").unwrap_or("unknown"))
                    .unwrap_or("note"),
                "content": obj.content_as_str().unwrap_or_default().chars().take(500).collect::<String>(),
                "tags": obj.tags.iter()
                    .filter(|t| !t.starts_with("kind:"))
                    .collect::<Vec<_>>(),
                "created_at": obj.created_at.to_rfc3339(),
                "updated_at": obj.modified_at.to_rfc3339(),
                "metadata": obj.metadata,
            })
        })
        .collect())
}

/// Semantic search for mobile
#[tauri::command]
pub async fn semantic_search(
    query: String,
    limit: Option<usize>,
    kind: Option<String>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<serde_json::Value>, String> {
    use crate::semantic_search::SearchOptions;
    use crate::memory::SecurityTier;

    let options = SearchOptions {
        limit: limit.unwrap_or(20),
        max_tier: SecurityTier::Guarded,
        ..Default::default()
    };

    let results = search.search(&query, options).await
        .map_err(|e| format!("{}", e))?;

    // Filter by kind if specified
    let filtered = results.into_iter()
        .filter(|hit| {
            if let Some(ref k) = kind {
                hit.object.tags.iter().any(|t| t == &format!("kind:{}", k))
            } else {
                true
            }
        });

    Ok(filtered
        .map(|hit| {
            serde_json::json!({
                "object": {
                    "id": hit.object.suid.to_string(),
                    "kind": hit.object.tags.iter()
                        .find(|t| t.starts_with("kind:"))
                        .map(|t| t.strip_prefix("kind:").unwrap_or("unknown"))
                        .unwrap_or("note"),
                    "content": hit.object.content_as_str().unwrap_or_default().chars().take(500).collect::<String>(),
                    "tags": hit.object.tags.iter()
                        .filter(|t| !t.starts_with("kind:"))
                        .collect::<Vec<_>>(),
                    "created_at": hit.object.created_at.to_rfc3339(),
                    "updated_at": hit.object.modified_at.to_rfc3339(),
                    "metadata": hit.object.metadata,
                },
                "score": hit.score,
            })
        })
        .collect())
}

/// Save a research source from mobile
#[tauri::command]
pub async fn save_research_source(
    title: String,
    url: String,
    snippet: String,
    notes: Option<String>,
    tags: Option<Vec<String>>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<String, String> {
    use crate::semantic_object::{ContentType, SemanticObject};
    use crate::memory::SecurityTier;

    // Create content with title, URL, snippet, and notes
    let content = format!(
        "# {}\n\nURL: {}\n\n## Summary\n{}\n\n{}",
        title,
        url,
        snippet,
        notes.as_ref().map(|n| format!("## Notes\n{}", n)).unwrap_or_default()
    );

    let mut obj = SemanticObject::new(
        content.as_bytes().to_vec(),
        ContentType::Markdown,
    );

    obj.name = Some(title.clone());
    obj.security_tier = SecurityTier::Open;

    // Add metadata
    obj.metadata.insert("url".to_string(), serde_json::json!(url));
    obj.metadata.insert("snippet".to_string(), serde_json::json!(snippet));
    if let Some(ref n) = notes {
        obj.metadata.insert("notes".to_string(), serde_json::json!(n));
    }
    obj.metadata.insert("saved_at".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));

    // Add tags
    obj.tags.push("kind:research".to_string());
    obj.tags.push("source:mobile".to_string());
    if let Some(custom_tags) = tags {
        obj.tags.extend(custom_tags);
    }

    // Store
    let suid = obj.suid.to_string();
    {
        let store = search.store.write().await;
        store.create(&obj).map_err(|e| e.to_string())?;
    }

    // Generate embedding
    let embed_text = format!("{} {}", title, snippet);
    if let Ok(embedding) = search.embeddings().embed(&embed_text).await {
        let store = search.store.write().await;
        let model = search.embeddings().model_info().id.clone();
        let _ = store.store_embedding(&obj.suid, &embedding, &model);
    }

    log::info!("Saved research source: {} ({})", title, suid);
    Ok(suid)
}

/// Get saved research sources
#[tauri::command]
pub async fn get_saved_sources(
    limit: Option<usize>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(50);
    let store = search.store.read().await;

    let objects = store.list(500, 0).map_err(|e| e.to_string())?;

    // Filter to research sources and sort by date
    let mut sources: Vec<_> = objects.iter()
        .filter(|obj| obj.tags.contains(&"kind:research".to_string()))
        .collect();

    sources.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

    Ok(sources.into_iter()
        .take(limit)
        .map(|obj| {
            serde_json::json!({
                "id": obj.suid.to_string(),
                "title": obj.name.clone().unwrap_or_else(|| "Untitled".to_string()),
                "url": obj.metadata.get("url").and_then(|v| v.as_str()).unwrap_or(""),
                "content": obj.content_as_str().map(|s| s.chars().take(300).collect::<String>()),
                "notes": obj.metadata.get("notes").and_then(|v| v.as_str()),
                "tags": obj.tags.iter()
                    .filter(|t| !t.starts_with("kind:") && !t.starts_with("source:"))
                    .collect::<Vec<_>>(),
                "saved_at": obj.metadata.get("saved_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&obj.created_at.to_rfc3339()),
            })
        })
        .collect())
}

/// Open external URL (uses system browser)
#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("Failed to open URL: {}", e))
}

// ============================================================================
// Thinking Debugger Commands
// ============================================================================

/// Message in a conversation for thinking analysis
#[derive(Deserialize)]
pub struct ConversationMessageInput {
    pub role: String,
    pub content: String,
}

/// Analyze a conversation for thinking errors (with automatic chunking for long conversations)
#[tauri::command]
pub async fn analyze_thinking(
    conversation: Vec<ConversationMessageInput>,
    topic: Option<String>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<serde_json::Value, String> {
    use crate::llm_tasks::{
        TaskRunner, AnalyzeChunkTask, ConversationMessage,
        chunk_conversation, merge_analyses, ThinkingAnalysis,
    };

    let messages: Vec<ConversationMessage> = conversation.into_iter()
        .map(|m| ConversationMessage {
            role: m.role,
            content: m.content,
        })
        .collect();

    // Split into chunks that fit within context window
    let chunks = chunk_conversation(&messages);

    if chunks.is_empty() {
        return Err("No conversation to analyze".to_string());
    }

    log::info!("Analyzing conversation in {} chunk(s)", chunks.len());

    let runner = TaskRunner::new(&ai_manager);
    let mut chunk_results: Vec<ThinkingAnalysis> = Vec::new();

    // Analyze each chunk
    for chunk in chunks {
        log::info!("Analyzing chunk {} of {}", chunk.chunk_index + 1, chunk.total_chunks);

        let task = AnalyzeChunkTask {
            chunk,
            topic: topic.clone(),
        };

        match runner.execute(task).await {
            Ok(result) => chunk_results.push(result),
            Err(e) => {
                log::warn!("Failed to analyze chunk: {}", e);
                // Continue with other chunks instead of failing entirely
            }
        }
    }

    if chunk_results.is_empty() {
        return Err("Failed to analyze any chunks of the conversation".to_string());
    }

    // Merge all chunk results into final analysis
    let final_result = merge_analyses(chunk_results);

    serde_json::to_value(final_result).map_err(|e| e.to_string())
}

/// Fact-check a specific claim
#[tauri::command]
pub async fn fact_check_claim(
    claim: String,
    context: Option<String>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<serde_json::Value, String> {
    use crate::llm_tasks::{TaskRunner, ClaimFactCheckTask};

    let task = ClaimFactCheckTask { claim, context };

    let runner = TaskRunner::new(&ai_manager);
    let result = runner.execute(task).await?;

    serde_json::to_value(result).map_err(|e| e.to_string())
}

/// Suggest better questions
#[tauri::command]
pub async fn suggest_better_questions(
    question: String,
    topic: Option<String>,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<serde_json::Value, String> {
    use crate::llm_tasks::{TaskRunner, BetterQuestionsTask};

    let task = BetterQuestionsTask {
        original_question: question,
        topic,
    };

    let runner = TaskRunner::new(&ai_manager);
    let result = runner.execute(task).await?;

    serde_json::to_value(result).map_err(|e| e.to_string())
}

// ============================================================================
// Session Browser Commands (for Thinking Debugger)
// ============================================================================

/// List all available Claude Code sessions
#[tauri::command]
pub async fn list_sessions(
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    use crate::providers::session::{SessionScanner, ChunkedSessionParser};

    let scanner = SessionScanner::new();
    let session_paths = scanner.find_all_sessions()?;

    let limit = limit.unwrap_or(20);
    let mut sessions = Vec::new();

    // Parse sessions and collect metadata (most recent first)
    let mut session_paths: Vec<_> = session_paths.into_iter().collect();
    session_paths.sort_by(|a, b| {
        let a_time = a.metadata().and_then(|m| m.modified()).ok();
        let b_time = b.metadata().and_then(|m| m.modified()).ok();
        b_time.cmp(&a_time)
    });

    for path in session_paths.into_iter().take(limit) {
        if let Ok(session) = ChunkedSessionParser::parse_file(&path) {
            let preview: String = session.chunks.first()
                .map(|c| c.user_content.chars().take(100).collect())
                .unwrap_or_default();

            sessions.push(serde_json::json!({
                "id": session.id,
                "project_path": session.project_path,
                "git_branch": session.git_branch,
                "chunk_count": session.chunks.len(),
                "message_count": session.message_count,
                "started_at": session.started_at,
                "ended_at": session.ended_at,
                "preview": preview,
                "source_file": session.source_file,
            }));
        }
    }

    Ok(sessions)
}

/// Get a session's conversation for the thinking debugger
#[tauri::command]
pub async fn get_session_conversation(
    session_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    use crate::providers::session::{SessionScanner, ChunkedSessionParser};

    let scanner = SessionScanner::new();
    let session_paths = scanner.find_all_sessions()?;

    // Find the session by ID
    for path in session_paths {
        let file_id = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        if file_id == session_id || path.to_string_lossy().contains(&session_id) {
            let session = ChunkedSessionParser::parse_file(&path)?;

            // Convert chunks to conversation messages
            let mut messages = Vec::new();
            for chunk in session.chunks {
                if !chunk.user_content.is_empty() {
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": chunk.user_content,
                    }));
                }
                if !chunk.assistant_content.is_empty() {
                    messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": chunk.assistant_content,
                    }));
                }
            }

            return Ok(messages);
        }
    }

    Err(format!("Session not found: {}", session_id))
}

// ============================================================================
// Browser Sync Commands (Real-time ChatGPT import)
// ============================================================================

/// Detect available browsers that have ChatGPT data
#[tauri::command]
pub async fn detect_browsers() -> Result<Vec<serde_json::Value>, String> {
    use crate::providers::importers::browser_sync::{detect_browsers, BrowserType};

    let browsers = detect_browsers();

    let result: Vec<serde_json::Value> = browsers.into_iter()
        .map(|b| {
            let name = match b {
                BrowserType::Chrome => "Chrome",
                BrowserType::ChromeBeta => "Chrome Beta",
                BrowserType::ChromeDev => "Chrome Dev",
                BrowserType::Chromium => "Chromium",
                BrowserType::Edge => "Microsoft Edge",
                BrowserType::Brave => "Brave",
                BrowserType::Vivaldi => "Vivaldi",
                BrowserType::Opera => "Opera",
            };

            serde_json::json!({
                "browser_type": format!("{:?}", b),
                "name": name,
                "storage_path": b.chatgpt_storage_path().map(|p| p.to_string_lossy().to_string()),
            })
        })
        .collect();

    Ok(result)
}

/// Request permission to access browser data
#[tauri::command]
pub async fn request_browser_access(browser_type: String) -> Result<bool, String> {
    use crate::providers::importers::browser_sync::BrowserType;

    let browser = match browser_type.as_str() {
        "Chrome" => BrowserType::Chrome,
        "Edge" => BrowserType::Edge,
        "Brave" => BrowserType::Brave,
        "Chromium" => BrowserType::Chromium,
        _ => return Err("Unknown browser type".to_string()),
    };

    // In a real implementation, this would:
    // 1. Show a permission dialog to the user
    // 2. Store the permission decision
    // 3. Return whether access was granted

    // For now, just check if we can access the path
    let path = browser.chatgpt_storage_path()
        .ok_or("Cannot determine browser path")?;

    if !path.exists() {
        return Ok(false);
    }

    // Try to access the directory
    std::fs::read_dir(&path)
        .map(|_| true)
        .map_err(|e| format!("Cannot access browser data: {}", e))
}

/// Scan for new ChatGPT conversations (doesn't import yet)
#[tauri::command]
pub async fn scan_chatgpt_conversations(
    browser_type: String,
) -> Result<Vec<serde_json::Value>, String> {
    use crate::providers::importers::browser_sync::BrowserType;

    let browser = match browser_type.as_str() {
        "Chrome" => BrowserType::Chrome,
        "Edge" => BrowserType::Edge,
        "Brave" => BrowserType::Brave,
        "Chromium" => BrowserType::Chromium,
        _ => return Err("Unknown browser type".to_string()),
    };

    // TODO: Implement actual browser DB scanning
    // For now, return empty with note that this needs implementation

    Ok(vec![])
}

/// Sync ChatGPT conversations (manual or auto)
#[tauri::command]
pub async fn sync_chatgpt_conversations(
    browser_type: String,
    state: State<'_, Arc<AiManager>>,
) -> Result<serde_json::Value, String> {
    // This would use the BrowserSync manager to import conversations
    // For now, return placeholder result

    Ok(serde_json::json!({
        "imported": [],
        "skipped": [],
        "total_found": 0,
        "message": "Browser sync implementation in progress - need to parse Chrome/Edge IndexedDB"
    }))
}

/// Skip a specific conversation from auto-import
#[tauri::command]
pub async fn skip_conversation(
    conversation_id: String,
    reason: String,
) -> Result<(), String> {
    // Store the skip reason
    // TODO: Implement persistent storage for skipped conversations
    Ok(())
}

/// Get current sync state
#[tauri::command]
pub async fn get_sync_state() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "last_sync": null,
        "auto_sync_enabled": false,
        "imported_count": 0,
        "skipped_count": 0,
        "message": "Sync state tracking to be implemented"
    }))
}

/// Set auto-sync enabled/disabled
#[tauri::command]
pub async fn set_auto_sync(enabled: bool) -> Result<(), String> {
    // TODO: Implement persistent settings storage
    Ok(())
}

// ============================================================================
// Session Commands (Session-Based Continuity)
// ============================================================================

/// Start a new work session
#[tauri::command]
pub async fn start_session(
    title: String,
    intent_type: String,
    intent_value: String,
    description: Option<String>,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<serde_json::Value, String> {
    use crate::sessions::SessionIntent;

    let intent = match intent_type.as_str() {
        "research" => SessionIntent::Research { topic: intent_value },
        "writing" => SessionIntent::Writing { project: intent_value },
        "coding" => SessionIntent::Coding { project: intent_value },
        "learning" => SessionIntent::Learning { subject: intent_value },
        "planning" => SessionIntent::Planning { goal: intent_value },
        "debugging" => SessionIntent::Debugging { issue: intent_value },
        "brainstorming" => SessionIntent::Brainstorming { theme: intent_value },
        _ => SessionIntent::Other { description: intent_value },
    };

    let session = manager.start_session(title, intent, description)
        .map_err(|e| format!("Failed to start session: {}", e))?;

    Ok(serde_json::to_value(session).map_err(|e| e.to_string())?)
}

/// End the current session
#[tauri::command]
pub async fn end_session(
    next_steps: Vec<String>,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<Option<serde_json::Value>, String> {
    let session = manager.end_session(next_steps)
        .map_err(|e| format!("Failed to end session: {}", e))?;

    Ok(session.map(|s| serde_json::to_value(s).map_err(|e| e.to_string())).transpose()?)
}

/// Get the current active session
#[tauri::command]
pub async fn get_current_session(
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<Option<serde_json::Value>, String> {
    Ok(manager.get_current_session()
        .map(|s| serde_json::to_value(s).map_err(|e| e.to_string()))
        .transpose()?)
}

/// Get session history
#[tauri::command]
pub async fn get_session_history(
    limit: Option<usize>,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut sessions = manager.get_session_history();

    // Sort by start time (most recent first)
    sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    // Apply limit
    if let Some(limit) = limit {
        sessions.truncate(limit);
    }

    sessions.into_iter()
        .map(|s| serde_json::to_value(s).map_err(|e| e.to_string()))
        .collect()
}

/// Resume a previous session
#[tauri::command]
pub async fn resume_session(
    session_id: String,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<serde_json::Value, String> {
    let session = manager.resume_session(&session_id)
        .map_err(|e| format!("Failed to resume session: {}", e))?;

    Ok(serde_json::to_value(session).map_err(|e| e.to_string())?)
}

/// Search sessions by meaning
#[tauri::command]
pub async fn search_sessions(
    query: String,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<Vec<serde_json::Value>, String> {
    let sessions = manager.search_sessions(&query)
        .map_err(|e| format!("Failed to search sessions: {}", e))?;

    sessions.into_iter()
        .map(|s| serde_json::to_value(s).map_err(|e| e.to_string()))
        .collect()
}

/// Record an activity during the current session
#[tauri::command]
pub async fn record_session_activity(
    activity_type: String,
    details: serde_json::Value,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<(), String> {
    use crate::sessions::{SessionActivity, ActivityDetails, ActivityType};
    use std::collections::HashMap;

    // Parse activity type
    let activity_type = match activity_type.as_str() {
        "document_view" => ActivityType::DocumentView,
        "note_taking" => ActivityType::NoteTaking,
        "ai_chat" => ActivityType::AiChat,
        "web_research" => ActivityType::WebResearch,
        "coding" => ActivityType::Coding,
        "writing" => ActivityType::Writing,
        "thinking" => ActivityType::Thinking,
        _ => ActivityType::Other,
    };

    // Parse details based on activity type
    let details = match activity_type {
        ActivityType::DocumentView => {
            ActivityDetails::ViewedDocument {
                title: details.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                path: details.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                page_number: details.get("page_number").and_then(|v| v.as_u64()).map(|v| v as usize),
                duration_secs: details.get("duration_secs").and_then(|v| v.as_u64()).unwrap_or(0),
            }
        },
        ActivityType::NoteTaking => {
            ActivityDetails::TookNotes {
                content: details.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                related_to: details.get("related_to")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default(),
            }
        },
        ActivityType::AiChat => {
            ActivityDetails::AiChat {
                provider: details.get("provider").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                topic: details.get("topic").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                message_count: details.get("message_count").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(0),
                summary: details.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            }
        },
        _ => ActivityDetails::Other {
            description: details.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            metadata: HashMap::new(),
        },
    };

    let activity = SessionActivity {
        id: uuid::Uuid::new_v4().to_string(),
        activity_type,
        timestamp: chrono::Utc::now(),
        duration: None,
        details,
    };

    manager.record_activity(activity)
        .map_err(|e| format!("Failed to record activity: {}", e))?;

    Ok(())
}

/// Take a snapshot of the current session state
#[tauri::command]
pub async fn take_session_snapshot(
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<(), String> {
    manager.take_snapshot()
        .map_err(|e| format!("Failed to take snapshot: {}", e))?;

    Ok(())
}

/// Get current session context for AI injection
/// Returns a formatted string with session information
#[tauri::command]
pub async fn get_session_context(
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<String, String> {
    let current = manager.get_current_session();

    match current {
        Some(session) => {
            let mut context = format!("=== Current Session ===\n");
            context.push_str(&format!("Title: {}\n", session.title));
            context.push_str(&format!("Intent: {:?}\n", session.intent));

            if let Some(desc) = &session.description {
                context.push_str(&format!("Description: {}\n", desc));
            }

            context.push_str(&format!("Started: {}\n", session.started_at));
            context.push_str(&format!("Activities: {}\n", session.activities.len()));

            // Add recent activities
            if !session.activities.is_empty() {
                context.push_str("\nRecent Activities:\n");
                for activity in session.activities.iter().take(5) {
                    context.push_str(&format!("- {:?}: {:?} at {}\n",
                        activity.activity_type,
                        activity.details,
                        activity.timestamp));
                }
            }

            // Add next steps if available
            if !session.next_steps.is_empty() {
                context.push_str("\nNext Steps:\n");
                for step in &session.next_steps {
                    context.push_str(&format!("- {}\n", step));
                }
            }

            Ok(context)
        },
        None => Ok("No active session".to_string()),
    }
}

/// Create a backfilled session (for work done in the past)
#[tauri::command]
pub async fn create_backfill_session(
    title: String,
    intent_type: String,
    intent_value: String,
    description: Option<String>,
    start_time: String,
    end_time: Option<String>,
    activities: Vec<serde_json::Value>,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<serde_json::Value, String> {
    use crate::sessions::{Session, SessionIntent, SessionActivity, ActivityDetails, ActivityType, SessionContext, SessionSnapshot};
    use chrono::DateTime;

    let intent = match intent_type.as_str() {
        "research" => SessionIntent::Research { topic: intent_value },
        "writing" => SessionIntent::Writing { project: intent_value },
        "coding" => SessionIntent::Coding { project: intent_value },
        "learning" => SessionIntent::Learning { subject: intent_value },
        "planning" => SessionIntent::Planning { goal: intent_value },
        "debugging" => SessionIntent::Debugging { issue: intent_value },
        "brainstorming" => SessionIntent::Brainstorming { theme: intent_value },
        _ => SessionIntent::Other { description: intent_value },
    };

    // Parse timestamps
    let started_at = start_time.parse::<DateTime<Utc>>()
        .map_err(|e| format!("Invalid start time: {}", e))?;
    let ended_at = end_time.and_then(|t| t.parse::<DateTime<Utc>>().ok());

    // Parse activities
    let parsed_activities: Vec<SessionActivity> = activities.into_iter()
        .filter_map(|act| {
            let activity_type_str = act.get("activityType")
                .and_then(|v| v.as_str())
                .unwrap_or("other");

            let details = act.get("details").unwrap_or(&serde_json::Value::Null);

            let activity_type = match activity_type_str {
                "thinking" => ActivityType::Thinking,
                _ => ActivityType::Other,
            };

            let activity_details = match activity_type {
                ActivityType::Thinking => {
                    ActivityDetails::Thinking {
                        notes: details.get("notes").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        insights: details.get("insights")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                            .unwrap_or_default(),
                    }
                },
                _ => ActivityDetails::Other {
                    description: format!("{:?}", details),
                    metadata: std::collections::HashMap::new(),
                }
            };

            Some(SessionActivity {
                id: uuid::Uuid::new_v4().to_string(),
                activity_type,
                timestamp: started_at, // Use session start time for all activities
                duration: None,
                details: activity_details,
            })
        })
        .collect();

    let session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        title,
        description,
        intent,
        started_at,
        ended_at,
        activities: parsed_activities,
        snapshots: vec![],
        context: SessionContext {
            project: None,
            related_sessions: vec![],
            prerequisites: vec![],
            environment: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        },
        next_steps: vec![],
        tags: vec![],
    };

    // Serialize before moving
    let session_json = serde_json::to_value(&session).map_err(|e| e.to_string())?;

    // Add to history directly
    manager.add_session_to_history(session)
        .map_err(|e| format!("Failed to add backfill session: {}", e))?;

    // Save to disk
    manager.save()
        .map_err(|e| format!("Failed to save sessions: {}", e))?;

    Ok(session_json)
}

/// Create sample sessions for testing
#[tauri::command]
pub async fn create_sample_sessions(
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<String, String> {
    use crate::sessions::{Session, SessionIntent, SessionActivity, ActivityDetails, ActivityType, SessionContext};
    use chrono::Utc;

    let now = Utc::now();
    let yesterday = now - chrono::Duration::days(1);

    // Sample coding session with Claude
    let claude_session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Fixed authentication bug with Claude".to_string(),
        description: Some("Debugged JWT token validation issue".to_string()),
        intent: SessionIntent::Coding { project: "marlos-rust".to_string() },
        started_at: yesterday,
        ended_at: Some(yesterday + chrono::Duration::hours(2)),
        activities: vec![
            SessionActivity {
                id: uuid::Uuid::new_v4().to_string(),
                activity_type: ActivityType::AiChat,
                timestamp: yesterday,
                duration: Some(std::time::Duration::from_secs(1800)),
                details: ActivityDetails::AiChat {
                    provider: "Claude".to_string(),
                    topic: "Authentication error debugging".to_string(),
                    message_count: 15,
                    summary: "Identified issue with JWT token validation in middleware".to_string(),
                },
            },
            SessionActivity {
                id: uuid::Uuid::new_v4().to_string(),
                activity_type: ActivityType::Coding,
                timestamp: yesterday + chrono::Duration::minutes(30),
                duration: Some(std::time::Duration::from_secs(3600)),
                details: ActivityDetails::Coding {
                    files_modified: vec!["src/auth.rs".to_string(), "src/middleware.rs".to_string()],
                    language: "Rust".to_string(),
                    commit_message: Some("Fix JWT validation".to_string()),
                },
            },
        ],
        snapshots: vec![],
        context: SessionContext {
            project: Some("marlos-rust".to_string()),
            related_sessions: vec![],
            prerequisites: vec![],
            environment: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        },
        next_steps: vec!["Add more tests for auth".to_string(), "Document the fix".to_string()],
        tags: vec!["bug".to_string(), "authentication".to_string()],
    };

    // Sample coding session with Codex
    let codex_session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Refactored database schema".to_string(),
        description: Some("Used Codex to refactor SQL queries".to_string()),
        intent: SessionIntent::Coding { project: "marlos-rust".to_string() },
        started_at: now - chrono::Duration::hours(4),
        ended_at: Some(now - chrono::Duration::hours(2)),
        activities: vec![
            SessionActivity {
                id: uuid::Uuid::new_v4().to_string(),
                activity_type: ActivityType::AiChat,
                timestamp: now - chrono::Duration::hours(4),
                duration: Some(std::time::Duration::from_secs(2400)),
                details: ActivityDetails::AiChat {
                    provider: "Codex".to_string(),
                    topic: "Database schema refactoring".to_string(),
                    message_count: 20,
                    summary: "Generated optimized SQL queries for new schema".to_string(),
                },
            },
            SessionActivity {
                id: uuid::Uuid::new_v4().to_string(),
                activity_type: ActivityType::Coding,
                timestamp: now - chrono::Duration::hours(3),
                duration: Some(std::time::Duration::from_secs(5400)),
                details: ActivityDetails::Coding {
                    files_modified: vec!["src/db/schema.rs".to_string(), "src/db/migrations/*.rs".to_string()],
                    language: "Rust".to_string(),
                    commit_message: Some("Refactor database schema".to_string()),
                },
            },
        ],
        snapshots: vec![],
        context: SessionContext {
            project: Some("marlos-rust".to_string()),
            related_sessions: vec![],
            prerequisites: vec![],
            environment: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        },
        next_steps: vec!["Update database documentation".to_string()],
        tags: vec!["refactor".to_string(), "database".to_string()],
    };

    // Add both sessions to history
    manager.add_session_to_history(claude_session)
        .map_err(|e| format!("Failed to add session: {}", e))?;
    manager.add_session_to_history(codex_session)
        .map_err(|e| format!("Failed to add session: {}", e))?;
    manager.save()
        .map_err(|e| format!("Failed to save sessions: {}", e))?;

    Ok("Created 2 sample coding sessions".to_string())
}

/// Debug: List all objects in ObjectStore
#[tauri::command]
pub async fn debug_list_objects(
    semantic_search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
) -> Result<String, String> {
    let store = semantic_search.store.read().await;
    let all_objects = store.list(1000, 0)
        .map_err(|e| format!("Failed to list objects: {}", e))?;

    // Count by content type
    let mut type_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut tag_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut conversation_count = 0;

    for obj in &all_objects {
        let type_name = format!("{:?}", obj.content_type);
        *type_counts.entry(type_name).or_insert(0) += 1;

        for tag in &obj.tags {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
        }

        // Check if it looks like a conversation
        if let Some(content) = obj.content_as_str() {
            if content.to_lowercase().contains("user:") && content.to_lowercase().contains("assistant:") {
                conversation_count += 1;
            }
        }
    }

    drop(store);

    let mut info = format!("Total objects: {}\n\n", all_objects.len());
    info.push_str("By content type:\n");
    for (type_name, count) in type_counts.iter() {
        info.push_str(&format!("  {}: {}\n", type_name, count));
    }

    info.push_str("\nTop tags:\n");
    let mut sorted_tags: Vec<_> = tag_counts.iter().collect();
    sorted_tags.sort_by(|a, b| b.1.cmp(a.1));
    for (tag, count) in sorted_tags.iter().take(10) {
        info.push_str(&format!("  {}: {}\n", tag, count));
    }

    info.push_str(&format!("\nConversations (detected): {}\n", conversation_count));

    info.push_str("\nNote: Full details logged to console (F12)");
    log::info!("=== DEBUG: All Objects ===");
    for obj in all_objects.iter().take(100) {
        log::info!("Object: name={:?}, type={:?}, tags={:?}",
            obj.name, obj.content_type, obj.tags);
    }

    Ok(info)
}

/// Import Claude Code conversations from repos
#[tauri::command]
pub async fn import_claude_code_conversations(
    semantic_search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
) -> Result<String, String> {
    use crate::providers::importers::claude_code::ClaudeCodeImporter;

    // Create importer with the user's specified path
    let mut importer = ClaudeCodeImporter::new();
    importer.add_scan_path(std::path::PathBuf::from(r"X:\ARCH\Software"));

    // Import conversations
    let result = importer.import_from_repos();

    // Store all imported objects
    let store = semantic_search.store.write().await;
    let mut stored = 0;
    for obj in result.imported {
        store.create(&obj)
            .map_err(|e| format!("Failed to store object: {}", e))?;
        stored += 1;
    }
    drop(store);

    let mut response = result.summary;
    response.push_str(&format!("\nStored {} objects in database", stored));

    if !result.errors.is_empty() {
        response.push_str(&format!("\nErrors: {}", result.errors.join(", ")));
    }

    Ok(response)
}

/// Import Claude Code session files directly into Session system
#[tauri::command]
pub async fn import_claude_code_sessions(
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<String, String> {
    use crate::providers::{SessionScanner, SessionParser};

    // Scan for all session files
    let scanner = SessionScanner::new();
    let session_files = scanner.find_all_sessions()
        .map_err(|e| format!("Failed to scan for sessions: {}", e))?;

    log::info!("Found {} session files", session_files.len());

    let mut imported = 0;
    let mut errors = Vec::new();

    for session_path in session_files {
        // Parse the session file
        let session = match SessionParser::parse_file(&session_path) {
            Ok(s) => s,
            Err(e) => {
                errors.push(format!("Failed to parse {:?}: {}", session_path, e));
                continue;
            }
        };

        // Convert to our Session format
        let marlos_session = crate::sessions::importer::claude_session_to_marlos_session(session, &session_path)?;

        // Add to history
        manager.add_session_to_history(marlos_session)
            .map_err(|e| format!("Failed to add session: {}", e))?;
        imported += 1;

        if imported % 50 == 0 {
            log::info!("Imported {} sessions...", imported);
        }
    }

    // Save all sessions
    manager.save()
        .map_err(|e| format!("Failed to save sessions: {}", e))?;

    let mut result = format!("Imported {} Claude Code sessions", imported);
    if !errors.is_empty() {
        result.push_str(&format!("\nErrors: {}", errors.join(", ")));
    }

    Ok(result)
}

/// Sync sessions from Andor Hub server
#[tauri::command]
pub async fn sync_from_andor(
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<String, String> {
    use crate::andor_client::AndorClient;

    let client = AndorClient::new();

    // Check if Andor is running
    let stats = match client.session_stats() {
        Ok(s) => s,
        Err(e) => {
            return Err(format!("Andor Hub not available: {}. Make sure it's running on http://localhost:8080", e));
        }
    };

    log::info!("Andor has {} sessions", stats.total);

    // Fetch all sessions (get completed ones, limit 1000)
    let response = client.list_sessions(Some("completed"), None, None, Some(1000))
        .map_err(|e| format!("Failed to list sessions: {}", e))?;

    log::info!("Fetched {} sessions from Andor", response.sessions.len());

    let mut imported = 0;
    let mut skipped = 0;
    let existing_sessions = manager.get_session_history();
    let existing_ids: std::collections::HashSet<String> = existing_sessions
        .iter()
        .map(|s| s.id.clone())
        .collect();

    for andor_session in response.sessions {
        // Skip if already imported
        if existing_ids.contains(&andor_session.session_id) {
            skipped += 1;
            continue;
        }

        // Convert Andor session to MarlOS session
        let marlos_session = crate::sessions::importer::andor_session_to_marlos_session(andor_session)?;

        // Add to history
        manager.add_session_to_history(marlos_session)
            .map_err(|e| format!("Failed to add session: {}", e))?;
        imported += 1;

        if imported % 50 == 0 {
            log::info!("Imported {} sessions from Andor...", imported);
        }
    }

    // Save all sessions
    manager.save()
        .map_err(|e| format!("Failed to save sessions: {}", e))?;

    let result = format!(
        "Synced from Andor Hub: {} new sessions, {} already exists ({} total)",
        imported,
        skipped,
        stats.total
    );

    Ok(result)
}

/// Import ChatGPT conversations from export file
#[tauri::command]
pub async fn import_chatgpt_export(
    manager: tauri::State<'_, crate::sessions::SessionManager>,
    semantic_search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
) -> Result<String, String> {
    use crate::providers::importers::chatgpt::ChatGptImporter;

    // For now, we'll scan the ObjectStore for ChatGPT conversations that were already imported
    // and convert them to sessions
    let store = semantic_search.store.read().await;

    // Get total count
    let total_count = store.count()
        .map_err(|e| format!("Failed to count objects: {}", e))?;

    // Fetch all objects that might be ChatGPT conversations
    let all_objects = store.list(total_count.min(10000), 0)
        .map_err(|e| format!("Failed to list objects: {}", e))?;
    drop(store);

    // Filter for ChatGPT-related objects
    let chatgpt_objects: Vec<_> = all_objects.into_iter()
        .filter(|obj| {
            let content = obj.content_as_str().unwrap_or("");
            content.to_lowercase().contains("chatgpt")
                || content.to_lowercase().contains("gpt-")
                || obj.tags.iter().any(|t| t.to_lowercase().contains("chatgpt"))
        })
        .collect();

    log::info!("Found {} ChatGPT objects", chatgpt_objects.len());

    let mut imported = 0;
    let existing_sessions = manager.get_session_history();
    let existing_ids: std::collections::HashSet<String> = existing_sessions
        .iter()
        .map(|s| s.id.clone())
        .collect();

    for obj in chatgpt_objects {
        // Skip if already imported
        if existing_ids.contains(&obj.suid.to_string()) {
            continue;
        }

        // Convert to session
        let session = crate::sessions::importer::chatgpt_object_to_session(obj)?;

        // Add to history
        manager.add_session_to_history(session)
            .map_err(|e| format!("Failed to add session: {}", e))?;
        imported += 1;
    }

    // Save all sessions
    manager.save()
        .map_err(|e| format!("Failed to save sessions: {}", e))?;

    Ok(format!("Imported {} ChatGPT conversations from database", imported))
}

/// Get detailed information about a specific session (for LLM access)
#[tauri::command]
pub async fn get_session_details(
    session_id: String,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<serde_json::Value, String> {
    let sessions = manager.get_session_history();

    let session = sessions.iter()
        .find(|s| s.id == session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Convert to a more detailed JSON format for LLM consumption
    Ok(serde_json::json!({
        "id": session.id,
        "title": session.title,
        "description": session.description,
        "intent": format!("{:?}", session.intent),
        "started_at": session.started_at.to_rfc3339(),
        "ended_at": session.ended_at.map(|dt| dt.to_rfc3339()),
        "activities": session.activities.iter().map(|a| {
            serde_json::json!({
                "type": format!("{:?}", a.activity_type),
                "timestamp": a.timestamp.to_rfc3339(),
                "duration_secs": a.duration.map(|d| d.as_secs()),
                "summary": match &a.details {
                    crate::sessions::ActivityDetails::AiChat { summary, .. } => summary.clone(),
                    crate::sessions::ActivityDetails::Coding { commit_message, .. } => {
                        commit_message.clone().unwrap_or_else(|| "Coding work".to_string())
                    },
                    _ => "Activity".to_string(),
                }
            })
        }).collect::<Vec<_>>(),
        "snapshots": session.snapshots.len(),
        "next_steps": session.next_steps,
        "tags": session.tags,
        "project": session.context.project,
    }))
}

/// Get sessions by provider (for LLM to query specific AI interactions)
#[tauri::command]
pub async fn get_sessions_by_provider(
    provider: String,
    limit: Option<usize>,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut sessions = manager.get_session_history();

    // Filter by provider (check tags and activities)
    let provider_lower = provider.to_lowercase();
    let filtered: Vec<_> = sessions.into_iter()
        .filter(|s| {
            // Check tags
            if s.tags.iter().any(|t| t.to_lowercase().contains(&provider_lower)) {
                return true;
            }
            // Check activities
            s.activities.iter().any(|a| {
                if let crate::sessions::ActivityDetails::AiChat { provider: p, .. } = &a.details {
                    p.to_lowercase().contains(&provider_lower)
                } else {
                    false
                }
            })
        })
        .collect();

    // Sort by date
    let mut sorted = filtered;
    sorted.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    // Apply limit
    let limited: Vec<_> = sorted.into_iter()
        .take(limit.unwrap_or(50))
        .map(|s| serde_json::json!({
            "id": s.id,
            "title": s.title,
            "date": s.started_at.to_rfc3339(),
            "activities": s.activities.len(),
            "tags": s.tags,
        }))
        .collect();

    Ok(limited)
}

/// Recent file entry for the frontend
#[derive(Serialize)]
pub struct RecentFile {
    pub path: String,
    pub name: String,
    pub file_type: String,
    pub last_opened: String, // ISO 8601 timestamp
}

/// Get recently opened files from session activity history
#[tauri::command]
pub async fn get_recent_files(
    limit: Option<usize>,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<Vec<RecentFile>, String> {
    use std::collections::HashMap;

    let sessions = manager.get_session_history();
    let limit = limit.unwrap_or(10);

    // Collect all document views with their timestamps
    let mut file_timestamps: HashMap<String, (String, DateTime<Utc>)> = HashMap::new();

    for session in sessions {
        for activity in &session.activities {
            if let crate::sessions::ActivityDetails::ViewedDocument { title, path, .. } = &activity.details {
                // Keep the most recent timestamp for each file
                let current = file_timestamps.get(path);
                if current.is_none() || current.unwrap().1 < activity.timestamp {
                    file_timestamps.insert(path.clone(), (title.clone(), activity.timestamp));
                }
            }
        }
    }

    // Sort by timestamp (most recent first)
    let mut files: Vec<_> = file_timestamps.into_iter().collect();
    files.sort_by(|a, b| b.1.1.cmp(&a.1.1));

    // Take the limit and convert to RecentFile
    let recent_files: Vec<RecentFile> = files
        .into_iter()
        .take(limit)
        .map(|(path, (title, timestamp))| {
            // Determine file type from extension
            let file_type = path
                .rsplit('.')
                .next()
                .map(|ext| ext.to_lowercase())
                .unwrap_or_else(|| "unknown".to_string());

            // Get the filename from path
            let name = path
                .rsplit(|c| c == '/' || c == '\\')
                .next()
                .unwrap_or(&title)
                .to_string();

            RecentFile {
                path,
                name,
                file_type,
                last_opened: timestamp.to_rfc3339(),
            }
        })
        .collect();

    Ok(recent_files)
}

/// Active AI coding session entry
#[derive(Serialize)]
pub struct ActiveAiSession {
    pub id: String,
    pub project_name: String,
    pub project_path: String,
    pub slug: Option<String>,
    pub git_branch: Option<String>,
    pub last_active: String,
    pub tool: String, // "claude-code" or "cursor"
}

/// Get active/recent AI coding sessions from Claude Code and Cursor
#[tauri::command]
pub async fn get_active_ai_sessions(
    limit: Option<usize>,
    hours_ago: Option<u64>,
) -> Result<Vec<ActiveAiSession>, String> {
    use std::fs;
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;

    let limit = limit.unwrap_or(10);
    let hours_ago = hours_ago.unwrap_or(48); // Default to last 48 hours
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours_ago as i64);

    let mut sessions: Vec<ActiveAiSession> = Vec::new();

    // Get Claude Code sessions from ~/.claude/projects/
    if let Some(home) = dirs::home_dir() {
        let claude_projects = home.join(".claude").join("projects");

        if claude_projects.exists() {
            if let Ok(entries) = fs::read_dir(&claude_projects) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }

                    // Find the most recent .jsonl file in this project
                    let mut latest_session: Option<(PathBuf, std::time::SystemTime, String)> = None;

                    if let Ok(files) = fs::read_dir(&path) {
                        for file in files.filter_map(|f| f.ok()) {
                            let file_path = file.path();
                            if file_path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                                if let Ok(metadata) = file_path.metadata() {
                                    if let Ok(modified) = metadata.modified() {
                                        // Check if this is newer than our current latest
                                        let dominated = latest_session.as_ref()
                                            .map(|(_, time, _)| modified > *time)
                                            .unwrap_or(true);

                                        if dominated {
                                            // Get session ID from filename
                                            let session_id = file_path
                                                .file_stem()
                                                .and_then(|s| s.to_str())
                                                .unwrap_or("")
                                                .to_string();
                                            latest_session = Some((file_path, modified, session_id));
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Process the latest session for this project
                    if let Some((session_path, modified, session_id)) = latest_session {
                        // Check if it's within our time window
                        let modified_datetime: chrono::DateTime<chrono::Utc> = modified.into();
                        if modified_datetime < cutoff {
                            continue;
                        }

                        // Parse first few lines to get session metadata
                        let mut slug = None;
                        let mut git_branch = None;
                        let mut cwd = None;

                        if let Ok(file) = fs::File::open(&session_path) {
                            let reader = BufReader::new(file);
                            for line in reader.lines().take(5).filter_map(|l| l.ok()) {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                                    if slug.is_none() {
                                        slug = json.get("slug")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());
                                    }
                                    if git_branch.is_none() {
                                        git_branch = json.get("gitBranch")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());
                                    }
                                    if cwd.is_none() {
                                        cwd = json.get("cwd")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());
                                    }
                                }
                            }
                        }

                        // Derive project name from folder name or cwd
                        let project_name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| {
                                // Convert folder name like "X--ARCH-Software-marlos-rust" to readable form
                                n.replace("--", "/").replace("-", " ")
                            })
                            .unwrap_or_else(|| "Unknown Project".to_string());

                        let project_path = cwd.unwrap_or_else(|| {
                            path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("")
                                .replace("--", ":\\")
                                .replace("-", "\\")
                        });

                        sessions.push(ActiveAiSession {
                            id: session_id,
                            project_name,
                            project_path,
                            slug,
                            git_branch,
                            last_active: modified_datetime.to_rfc3339(),
                            tool: "claude-code".to_string(),
                        });
                    }
                }
            }
        }
    }

    // Sort by last_active (most recent first)
    sessions.sort_by(|a, b| b.last_active.cmp(&a.last_active));

    // Apply limit
    sessions.truncate(limit);

    Ok(sessions)
}

/// Open a terminal at the given path, optionally running a command
#[tauri::command]
pub async fn open_terminal(
    path: String,
    command: Option<String>,
) -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    {
        // Try Windows Terminal first, fall back to cmd
        let cmd_to_run = command.unwrap_or_else(|| "claude".to_string());

        // Check if Windows Terminal is available
        let wt_result = Command::new("where")
            .arg("wt")
            .output();

        let result = if wt_result.map(|o| o.status.success()).unwrap_or(false) {
            // Use Windows Terminal
            Command::new("wt")
                .args(["-d", &path, "cmd", "/k", &cmd_to_run])
                .spawn()
        } else {
            // Fall back to cmd
            Command::new("cmd")
                .args(["/c", "start", "cmd", "/k", &format!("cd /d \"{}\" && {}", path, cmd_to_run)])
                .spawn()
        };

        result.map_err(|e| format!("Failed to open terminal: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        let cmd_to_run = command.unwrap_or_else(|| "claude".to_string());
        let script = format!(
            r#"tell application "Terminal"
                do script "cd '{}' && {}"
                activate
            end tell"#,
            path, cmd_to_run
        );

        Command::new("osascript")
            .args(["-e", &script])
            .spawn()
            .map_err(|e| format!("Failed to open terminal: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        let cmd_to_run = command.unwrap_or_else(|| "claude".to_string());
        // Try common terminal emulators
        let terminals = ["gnome-terminal", "konsole", "xterm", "alacritty"];
        let mut opened = false;

        for term in &terminals {
            let result = match *term {
                "gnome-terminal" => Command::new(term)
                    .args(["--working-directory", &path, "--", "bash", "-c", &format!("{}; exec bash", cmd_to_run)])
                    .spawn(),
                "konsole" => Command::new(term)
                    .args(["--workdir", &path, "-e", "bash", "-c", &format!("{}; exec bash", cmd_to_run)])
                    .spawn(),
                _ => Command::new(term)
                    .args(["-e", "bash", "-c", &format!("cd '{}' && {}; exec bash", path, cmd_to_run)])
                    .spawn(),
            };

            if result.is_ok() {
                opened = true;
                break;
            }
        }

        if !opened {
            return Err("No supported terminal emulator found".to_string());
        }
    }

    Ok(())
}

/// Launch Claude Code CLI with a context file for a Plan Space milestone
#[tauri::command]
pub async fn launch_claude_agent(
    context: String,
    working_dir: Option<String>,
) -> Result<String, String> {
    use std::process::Command;
    use std::io::Write;

    // Create a temp file with the context
    let temp_dir = std::env::temp_dir();
    let context_file = temp_dir.join("marlos-agent-context.md");

    let mut file = std::fs::File::create(&context_file)
        .map_err(|e| format!("Failed to create context file: {}", e))?;
    file.write_all(context.as_bytes())
        .map_err(|e| format!("Failed to write context: {}", e))?;

    let context_path = context_file.to_string_lossy().to_string();

    // Determine working directory
    let path = working_dir.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string())
    });

    // Build the claude command - read context file and start interactive session
    let claude_cmd = format!(
        "echo. && echo === MarlOS Agent Context === && type \"{}\" && echo. && echo === Starting Claude Code === && echo. && claude",
        context_path.replace("/", "\\")
    );

    #[cfg(target_os = "windows")]
    {
        // Try Windows Terminal first, fall back to cmd
        let wt_exists = Command::new("where")
            .arg("wt")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let result = if wt_exists {
            Command::new("wt")
                .args(["-d", &path, "cmd", "/k", &claude_cmd])
                .spawn()
        } else {
            Command::new("cmd")
                .args(["/c", "start", "cmd", "/k", &format!("cd /d \"{}\" && {}", path, claude_cmd)])
                .spawn()
        };

        result.map_err(|e| format!("Failed to open terminal: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            r#"tell application "Terminal"
                activate
                do script "cd '{}' && cat '{}' && echo '' && echo '=== Starting Claude Code ===' && claude"
            end tell"#,
            path, context_path
        );

        Command::new("osascript")
            .args(["-e", &script])
            .spawn()
            .map_err(|e| format!("Failed to open terminal: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        let terminals = ["gnome-terminal", "konsole", "xterm"];
        let mut opened = false;

        for term in terminals {
            let result = match term {
                "gnome-terminal" => Command::new(term)
                    .args(["--working-directory", &path, "--", "bash", "-c", &format!("cat '{}' && echo '' && echo '=== Starting Claude Code ===' && claude; exec bash", context_path)])
                    .spawn(),
                "konsole" => Command::new(term)
                    .args(["--workdir", &path, "-e", "bash", "-c", &format!("cat '{}' && echo '' && echo '=== Starting Claude Code ===' && claude; exec bash", context_path)])
                    .spawn(),
                _ => Command::new(term)
                    .args(["-e", "bash", "-c", &format!("cd '{}' && cat '{}' && echo '' && echo '=== Starting Claude Code ===' && claude; exec bash", path, context_path)])
                    .spawn(),
            };

            if result.is_ok() {
                opened = true;
                break;
            }
        }

        if !opened {
            return Err("No supported terminal emulator found".to_string());
        }
    }

    Ok(context_path)
}

/// Estimate the character size of a session when formatted
/// This helps with smart chunking for LLM processing
fn estimate_session_size(session: &crate::sessions::Session) -> usize {
    // Base size for metadata (date, title, project, intent)
    let mut size = 200;

    // Add size for activities
    for activity in &session.activities {
        size += match activity.activity_type {
            crate::sessions::ActivityType::AiChat => 100,
            crate::sessions::ActivityType::Coding => 80,
            crate::sessions::ActivityType::DocumentView => 60,
            crate::sessions::ActivityType::NoteTaking => 150,
            crate::sessions::ActivityType::WebResearch => 100,
            crate::sessions::ActivityType::Writing => 80,
            crate::sessions::ActivityType::Thinking => 120,
            crate::sessions::ActivityType::Other => 50,
        };
    }

    // Add description size if present
    if let Some(ref desc) = session.description {
        size += desc.len();
    }

    // Add next steps size
    for step in &session.next_steps {
        size += step.len() + 10;
    }

    size
}

/// Get recent session context for LLM awareness
/// Returns a summary of recent work across all sessions
/// Uses hierarchical LLM summarization to condense large amounts of session data
#[tauri::command]
pub async fn get_recent_context(
    days: Option<usize>,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
    ai_manager: tauri::State<'_, std::sync::Arc<crate::ai::AiManager>>,
) -> Result<String, String> {
    use crate::llm_tasks::{TaskRunner, SummarizeSessionsTask, SummarizeSummariesTask};

    let days = days.unwrap_or(7);
    let max_target_length = 2000; // Target ~500 tokens

    // Get recent sessions
    let cutoff_date = chrono::Utc::now() - chrono::Duration::days(days as i64);
    let sessions = manager.get_session_history();

    let recent: Vec<&crate::sessions::Session> = sessions.iter()
        .filter(|s| s.started_at > cutoff_date)
        .collect();

    if recent.is_empty() {
        return Ok(format!("No sessions in the last {} days", days));
    }

    // If we have a small number of sessions, use simple formatting
    if recent.len() <= 10 {
        let formatted = crate::sessions::SessionManager::format_sessions_for_llm(&recent, days);
        if formatted.chars().count() <= max_target_length {
            return Ok(formatted);
        }
    }

    // Hierarchical summarization:
    // 1. Chunk sessions into groups based on actual content size
    // 2. Summarize each chunk with LLM
    // 3. Combine summaries and summarize again if needed

    // Estimate session sizes and create smart chunks
    const MAX_CHARS_PER_CHUNK: usize = 4000; // Target ~1000 tokens per chunk
    let mut chunks: Vec<Vec<&crate::sessions::Session>> = Vec::new();
    let mut current_chunk: Vec<&crate::sessions::Session> = Vec::new();
    let mut current_chunk_size = 0;

    for session in &recent {
        // Estimate this session's formatted size
        let estimated_size = estimate_session_size(session);

        // If adding this session would exceed the limit and we already have some sessions
        if current_chunk_size + estimated_size > MAX_CHARS_PER_CHUNK && !current_chunk.is_empty() {
            chunks.push(current_chunk);
            current_chunk = Vec::new();
            current_chunk_size = 0;
        }

        current_chunk.push(session);
        current_chunk_size += estimated_size;
    }

    // Don't forget the last chunk
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    log::info!("Smart chunking: {} sessions -> {} chunks (avg {} sessions/chunk)",
        recent.len(), chunks.len(), recent.len() / chunks.len().max(1));

    let mut chunk_summaries = Vec::new();

    for chunk in chunks {
        // Format this chunk for the LLM
        let chunk_text = crate::sessions::SessionManager::format_sessions_for_llm(&chunk, days);

        log::debug!("Processing chunk: {} sessions, {} chars", chunk.len(), chunk_text.chars().count());

        // Ask LLM to summarize this chunk
        let task = SummarizeSessionsTask::new(chunk_text.clone(), days, recent.len());
        let ctx = crate::llm_tasks::TaskContext::new().with_max_tokens(500);
        match TaskRunner::new(&ai_manager)
            .with_context(ctx)
            .execute(task)
            .await
        {
            Ok(summary) => {
                chunk_summaries.push(format!(
                    "Summary:\n{}\nProjects: {}\nTopics: {}\nActivities: {}",
                    summary.summary,
                    summary.key_projects.join(", "),
                    summary.key_topics.join(", "),
                    summary.activity_summary
                ));
            }
            Err(e) => {
                log::warn!("LLM summarization failed for chunk: {}", e);
                // Fall back to raw text for this chunk
                chunk_summaries.push(chunk_text);
            }
        }
    }

    // Combine all summaries
    let combined = chunk_summaries.join("\n\n---\n\n");

    // If combined is still too long, summarize the summaries
    if combined.chars().count() > max_target_length && chunk_summaries.len() > 1 {
        let task = SummarizeSummariesTask::new(combined.clone());
        let ctx = crate::llm_tasks::TaskContext::new().with_max_tokens(300);
        match TaskRunner::new(&ai_manager)
            .with_context(ctx)
            .execute(task)
            .await
        {
            Ok(final_summary) => {
                return Ok(format!(
                    "Recent Work ({} days, {} sessions):\n\nOverview: {}\n\nProjects: {}\n\nMain Themes: {}",
                    days,
                    recent.len(),
                    final_summary.overview,
                    final_summary.projects_worked_on.join(", "),
                    final_summary.main_themes.join(", ")
                ));
            }
            Err(e) => {
                log::warn!("LLM summary-of-summaries failed: {}", e);
                // Fall back to combined summaries
            }
        }
    }

    Ok(combined)
}

/// Get vector-based analysis of sessions (no LLM needed)
/// Much faster than LLM analysis and scales to entire database
#[tauri::command]
pub async fn get_session_vector_analysis(
    days: Option<usize>,
    manager: tauri::State<'_, crate::sessions::SessionManager>,
) -> Result<String, String> {
    let days = days.unwrap_or(7);

    let analysis = manager.get_vector_analysis(days)
        .map_err(|e| format!("Failed to analyze sessions: {}", e))?;

    Ok(analysis.format())
}


/// Import existing AI conversations as Sessions
#[tauri::command]
pub async fn import_conversations_as_sessions(
    manager: tauri::State<'_, crate::sessions::SessionManager>,
    semantic_search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
) -> Result<String, String> {
    use crate::sessions::importer::import_conversations_as_sessions;

    // Get ALL objects, not just first 1000
    let store = semantic_search.store.read().await;

    // First get total count
    let total_count = store.count()
        .map_err(|e| format!("Failed to count objects: {}", e))?;

    log::info!("Total objects in database: {}", total_count);

    // Now fetch all objects in batches
    let mut all_objects = Vec::new();
    let mut offset = 0;
    let batch_size = 1000;

    loop {
        let batch = store.list(batch_size, offset)
            .map_err(|e| format!("Failed to list objects: {}", e))?;

        if batch.is_empty() {
            break;
        }

        all_objects.extend(batch);
        offset += batch_size;

        log::info!("Fetched {} objects so far...", all_objects.len());

        if all_objects.len() >= total_count {
            break;
        }
    }

    drop(store); // Release lock before async operation

    log::info!("Total objects loaded: {}", all_objects.len());

    let imported = import_conversations_as_sessions(&manager, all_objects).await
        .map_err(|e| format!("Failed to import: {}", e))?;

    Ok(format!("Imported {} conversations as sessions", imported))
}

// ============================================================================
// Vector Database Query UI Commands
// ============================================================================

/// Helper function to format ContentType as string
fn format_content_type(ct: &crate::semantic_object::ContentType) -> String {
    match ct {
        crate::semantic_object::ContentType::Text => "Text".to_string(),
        crate::semantic_object::ContentType::Markdown => "Markdown".to_string(),
        crate::semantic_object::ContentType::Code { language } => format!("Code({})", language),
        crate::semantic_object::ContentType::Json => "Json".to_string(),
        crate::semantic_object::ContentType::Binary { mime } => format!("Binary({})", mime),
        crate::semantic_object::ContentType::Structured { schema } => format!("Structured({})", schema),
        crate::semantic_object::ContentType::Unknown => "Unknown".to_string(),
    }
}

/// Helper function to format SecurityTier as string
fn format_security_tier(st: &crate::memory::SecurityTier) -> String {
    match st {
        crate::memory::SecurityTier::Open => "Open".to_string(),
        crate::memory::SecurityTier::Guarded => "Guarded".to_string(),
        crate::memory::SecurityTier::Sealed => "Sealed".to_string(),
    }
}

/// Vector search result with similarity score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub suid: String,
    pub title: Option<String>,
    pub content_type: String,
    pub similarity: f32,
    pub preview: String,
    pub tags: Vec<String>,
    pub security_tier: String,
    pub path: Option<String>,
    pub created_at: String,
}

/// A cluster of similar objects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorCluster {
    pub id: String,
    pub theme: String,
    pub objects: Vec<VectorSearchResult>,
    pub avg_similarity: f32,
}

/// Index all sessions into the vector database
/// Only indexes new or modified sessions, skips already-indexed ones
/// Uses semantic chunks directly for embedding (no AI summaries during indexing)
#[tauri::command]
pub async fn index_sessions_to_vector_db(
    manager: tauri::State<'_, crate::sessions::SessionManager>,
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    pca_cache: tauri::State<'_, crate::pca_cache::PCACacheManager>,
) -> Result<String, String> {
    use crate::sessions::vector_bridge;

    // Get all sessions from history
    let sessions = manager.get_session_history();
    log::info!("Checking {} sessions for vector indexing...", sessions.len());

    // Get existing session chunks to avoid re-indexing
    let existing_session_chunks = {
        let store = search.store.read().await;
        let all_objects = store.list(10000, 0)
            .map_err(|e| format!("Failed to list objects: {}", e))?;

        // Extract session IDs from existing chunks
        all_objects.into_iter()
            .filter(|obj| obj.tags.contains(&"kind:session-chunk".to_string()))
            .filter_map(|obj| {
                obj.tags.iter()
                    .find(|t| t.starts_with("session:"))
                    .map(|t| t.strip_prefix("session:").unwrap_or("").to_string())
            })
            .collect::<std::collections::HashSet<_>>()
    };

    let mut indexed_count = 0;
    let mut skipped_count = 0;
    let mut chunk_count = 0;

    for session in sessions {
        // Skip if already indexed
        if existing_session_chunks.contains(&session.id) {
            skipped_count += 1;
            continue;
        }

        // Chunk the session into Q&A pairs and topic sections
        let chunks = vector_bridge::chunk_session_for_vectors(&session);

        if chunks.is_empty() {
            log::debug!("Session {} has no indexable content", session.id);
            continue;
        }

        let num_chunks = chunks.len();

        for chunk in chunks {
            // Get the full content
            let full_content = chunk.to_text();

            // Create semantic object from chunk (using full content directly)
            let mut obj = crate::semantic_object::SemanticObject::new(
                full_content.as_bytes().to_vec(),
                crate::semantic_object::ContentType::Structured {
                    schema: "claude-qa-chunk".to_string()
                }
            );

            obj.name = Some(chunk.title());
            obj.tags = vec![
                "kind:session-chunk".to_string(),
                "claude-code".to_string(),
                chunk.chunk_type.to_string(),
                format!("session:{}", session.id),
            ];
            obj.tags.extend(session.tags.iter().cloned());
            obj.tags.extend(session.context.project.iter().map(|p| format!("project:{}", p)));

            // Store metadata including full content
            obj.summary = Some(serde_json::json!({
                "session_id": session.id,
                "session_title": session.title,
                "chunk_type": chunk.chunk_type.to_string(),
                "content": full_content,
                "metadata": chunk.metadata.clone(),
            }).to_string());

            // Store with embedding
            search.store(&obj).await
                .map_err(|e| format!("Failed to store chunk: {}", e))?;

            chunk_count += 1;
        }

        indexed_count += 1;
        log::debug!("Indexed session: {} ({} chunks)", session.title, num_chunks);
    }

    let message = if indexed_count == 0 && skipped_count > 0 {
        format!("All {} sessions already indexed", skipped_count)
    } else if skipped_count > 0 {
        format!("Indexed {} new sessions ({} chunks), skipped {} already indexed",
            indexed_count, chunk_count, skipped_count)
    } else {
        format!("Indexed {} sessions into {} chunks", indexed_count, chunk_count)
    };

    log::info!("Session indexing complete: {}", message);

    // Invalidate PCA cache since object count changed
    if indexed_count > 0 {
        pca_cache.invalidate();
        log::info!("PCA cache invalidated - will recompute on next 3D space load");
    }

    Ok(message)
}

/// Vector search across all objects with similarity scores
#[tauri::command]
pub async fn vector_search(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    query: String,
    limit: Option<usize>,
    min_score: Option<f32>,
    content_type: Option<String>,
    security_tier: Option<String>,
) -> Result<Vec<VectorSearchResult>, String> {
    use crate::semantic_search::SearchOptions;

    let options = SearchOptions {
        limit: limit.unwrap_or(50),
        min_score: min_score.unwrap_or(0.3),
        include_keyword: true,
        ..Default::default()
    };

    let hits = search.search(&query, options).await
        .map_err(|e| format!("Search failed: {}", e))?;

    // Convert to VectorSearchResult
    let mut results: Vec<VectorSearchResult> = hits.into_iter()
        .filter(|hit| {
            // Filter by content type if specified
            if let Some(ref ct) = content_type {
                if !format_content_type(&hit.object.content_type).contains(ct) {
                    return false;
                }
            }
            // Filter by security tier if specified
            if let Some(ref st) = security_tier {
                if format_security_tier(&hit.object.security_tier) != *st {
                    return false;
                }
            }
            true
        })
        .map(|hit| {
            let preview = hit.object.content_as_str()
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>();

            VectorSearchResult {
                suid: hit.object.suid.to_string(),
                title: hit.object.name.clone(),
                content_type: format_content_type(&hit.object.content_type),
                similarity: hit.score,
                preview,
                tags: hit.object.tags.clone(),
                security_tier: format_security_tier(&hit.object.security_tier),
                path: hit.object.path.clone(),
                created_at: hit.object.created_at.to_rfc3339(),
            }
        })
        .collect();

    // Sort by similarity descending
    results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));

    Ok(results)
}

/// Clear all session chunks from the vector database for a fresh start
#[tauri::command]
pub async fn clear_vector_database(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    pca_cache: tauri::State<'_, crate::pca_cache::PCACacheManager>,
) -> Result<String, String> {
    let store = search.store.write().await;

    // Get all objects with session-chunk tag
    let all_objects = store.list(10000, 0)
        .map_err(|e| format!("Failed to list objects: {}", e))?;

    let session_chunks: Vec<_> = all_objects.into_iter()
        .filter(|obj| obj.tags.contains(&"kind:session-chunk".to_string()))
        .collect();

    let count = session_chunks.len();

    if count == 0 {
        return Ok("Vector database is already clean (no session chunks found)".to_string());
    }

    // Delete each session chunk
    let mut deleted = 0;
    for obj in session_chunks {
        match store.delete(&obj.suid) {
            Ok(true) => deleted += 1,
            Ok(false) => {}, // Didn't exist
            Err(e) => {
                log::warn!("Failed to delete object {}: {}", obj.suid, e);
            }
        }
    }

    // Invalidate PCA cache
    if deleted > 0 {
        pca_cache.invalidate();
    }

    Ok(format!("Cleared {} session chunks from vector database", deleted))
}

/// Safely truncate a string at a character boundary
fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last character boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Force reindex all objects with current embedding model
/// This re-embeds ALL objects with proper chunking, fixing dimension mismatches
/// Uses streaming approach - process small groups at a time to minimize memory usage
#[tauri::command]
pub async fn force_reindex_all(
    app: tauri::AppHandle,
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    pca_cache: tauri::State<'_, crate::pca_cache::PCACacheManager>,
) -> Result<String, String> {
    use tauri::Emitter;
    use crate::chunking::{chunk_text, ChunkConfig};
    use crate::semantic_object::{Relation, RelationType};

    const GROUP_SIZE: usize = 10;      // Process 10 objects at a time
    const EMBED_BATCH: usize = 4;      // Embed 4 texts per API call
    const TIMEOUT_SECS: u64 = 60;      // Timeout per embedding call
    const MAX_CHUNKS_PER_FILE: usize = 5; // Limit chunks per file to avoid explosion

    // Log start
    log::info!("=== REINDEX STARTING (streaming mode) ===");

    // Conservative chunking - larger chunks, fewer of them
    let chunk_config = ChunkConfig {
        target_size: 6000,   // Larger chunks
        min_size: 1000,      // No tiny chunks
        max_size: 10000,     // Allow fairly large chunks
        overlap: 200,
    };

    // Step 1: Get only SUIds (minimal memory)
    log::info!("Step 1: Scanning for object IDs...");
    let _ = app.emit("reindex-progress", serde_json::json!({
        "stage": "cleanup",
        "message": "Scanning objects...",
        "current": 0,
        "total": 0
    }));

    let (chunk_ids, parent_suids): (Vec<String>, Vec<String>) = {
        let store = search.store.read().await;
        let all_objects = store.list(100000, 0)
            .map_err(|e| format!("Failed to list objects: {}", e))?;

        let mut chunks = Vec::new();
        let mut parents = Vec::new();

        // Only store SUID strings, not objects
        for obj in all_objects {
            let suid_str = obj.suid.to_string();
            if obj.tags.iter().any(|t| t == "kind:chunk") {
                chunks.push(suid_str);
            } else {
                parents.push(suid_str);
            }
        }
        log::info!("Found {} chunks to delete, {} parent objects", chunks.len(), parents.len());
        (chunks, parents)
    }; // Release read lock - all_objects dropped here

    // Delete chunks in small batches
    let mut chunks_deleted = 0;
    if !chunk_ids.is_empty() {
        log::info!("Deleting {} chunks in batches of 10...", chunk_ids.len());
        let _ = app.emit("reindex-progress", serde_json::json!({
            "stage": "cleanup",
            "message": format!("Deleting {} old chunks...", chunk_ids.len()),
            "current": 0,
            "total": chunk_ids.len()
        }));

        for (i, chunk_batch) in chunk_ids.chunks(10).enumerate() {
            // Parse SUIds and delete
            {
                let store = search.store.write().await;
                for suid_str in chunk_batch {
                    if let Ok(suid) = crate::semantic_object::Suid::parse(suid_str) {
                        if store.delete(&suid).is_ok() {
                            chunks_deleted += 1;
                        }
                    }
                }
            } // Release write lock

            if i % 5 == 0 {
                log::info!("Deleted {}/{} chunks", chunks_deleted, chunk_ids.len());
                let _ = app.emit("reindex-progress", serde_json::json!({
                    "stage": "cleanup",
                    "message": format!("Deleted {}/{}...", chunks_deleted, chunk_ids.len()),
                    "current": chunks_deleted,
                    "total": chunk_ids.len()
                }));
            }
            tokio::task::yield_now().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }
    }

    let total_parents = parent_suids.len();
    if total_parents == 0 {
        return Ok("No objects to reindex".to_string());
    }

    log::info!("Step 2: Streaming process {} objects in groups of {}...", total_parents, GROUP_SIZE);

    let model_name = search.embeddings().model_info().name.clone();
    let mut total_embedded = 0;
    let mut total_chunks = 0;
    let mut total_whole_files = 0;  // Files embedded without chunking
    let mut total_errors = 0;
    let mut total_skipped = 0;
    let mut total_too_small = 0;    // Files too small to chunk (< 10k chars)

    // STREAMING APPROACH: Process objects in small groups
    // Each group: fetch -> prepare -> embed -> store -> release memory
    let total_groups = (parent_suids.len() + GROUP_SIZE - 1) / GROUP_SIZE;

    for (group_idx, group) in parent_suids.chunks(GROUP_SIZE).enumerate() {
        let group_start = group_idx * GROUP_SIZE;

        log::info!("=== GROUP {}/{}: Processing items {}-{} of {} ===",
            group_idx + 1, total_groups, group_start + 1, group_start + group.len(), total_parents);

        let _ = app.emit("reindex-progress", serde_json::json!({
            "stage": "processing",
            "message": format!("Group {}/{}: {} whole, {} chunks ({} small)",
                group_idx + 1, total_groups, total_whole_files, total_chunks, total_too_small),
            "current": group_start,
            "total": total_parents,
            "chunks_created": total_chunks,
            "whole_files": total_whole_files,
            "too_small": total_too_small
        }));

        // Collect items for this group (small memory footprint)
        let mut group_items: Vec<(crate::semantic_object::Suid, String)> = Vec::new();

        for suid_str in group {
            // Fetch object
            let obj = {
                let store = search.store.read().await;
                let suid = match crate::semantic_object::Suid::parse(suid_str) {
                    Ok(s) => s,
                    Err(_) => { total_skipped += 1; continue; }
                };
                match store.get(&suid) {
                    Ok(Some(o)) => o,
                    _ => { total_skipped += 1; continue; }
                }
            };

            // Get text content
            let text = if let Some(content) = &obj.content {
                String::from_utf8_lossy(content).to_string()
            } else if let Some(summary) = &obj.summary {
                format!("{}\n{}", obj.name.clone().unwrap_or_default(), summary)
            } else {
                obj.name.clone().unwrap_or_default()
            };

            if text.trim().is_empty() {
                total_skipped += 1;
                continue;
            }

            // Check if this needs chunking - chunk ANY long content, not just code
            let is_code_file = obj.tags.iter().any(|t| t == "kind:code");
            let is_pdf = obj.tags.iter().any(|t| t == "kind:pdf");
            let is_session = obj.tags.iter().any(|t| t.starts_with("session:"));
            let file_ext = obj.name.as_ref()
                .and_then(|n| n.rsplit('.').next())
                .filter(|ext| ext.len() <= 5);

            // Track why we're not chunking
            let needs_chunking = text.len() > chunk_config.max_size;
            if !needs_chunking {
                total_too_small += 1;
            }

            if needs_chunking {
                let all_chunks = chunk_text(&text, file_ext, &chunk_config);
                // Limit chunks per file to avoid memory explosion
                let chunks: Vec<_> = all_chunks.into_iter().take(MAX_CHUNKS_PER_FILE).collect();

                if chunks.len() > 1 {
                    // Create chunk objects
                    {
                        let store = search.store.write().await;

                        for (chunk_idx, chunk) in chunks.iter().enumerate() {
                            let chunk_name = format!("{}#chunk{}",
                                obj.name.as_deref().unwrap_or("chunk"),
                                chunk_idx);

                            let context_suffix = chunk.context.as_ref()
                                .map(|c| format!(" ({})", c))
                                .unwrap_or_default();

                            let mut chunk_tags = obj.tags.clone();
                            chunk_tags.push("kind:chunk".to_string());
                            chunk_tags.push(format!("chunk:{}", chunk_idx));
                            chunk_tags.push(format!("parent:{}", obj.suid));

                            let mut chunk_obj = SemanticObject::from_text(&chunk.text)
                                .with_name(&format!("{}{}", chunk_name, context_suffix));
                            chunk_obj.tags = chunk_tags;

                            chunk_obj.relations.push(Relation::new(
                                obj.suid.clone(),
                                RelationType::DerivedFrom,
                            ));

                            if store.create(&chunk_obj).is_ok() {
                                let chunk_text = safe_truncate(&chunk.text, 4000).to_string();
                                group_items.push((chunk_obj.suid, chunk_text));
                                total_chunks += 1;
                            }
                        }
                    } // Release write lock

                    // Add parent with first chunk text
                    if let Some(first_chunk) = chunks.first() {
                        let text_to_embed = safe_truncate(&first_chunk.text, 4000).to_string();
                        group_items.push((obj.suid.clone(), text_to_embed));
                    }
                    continue;
                }
            }

            // Regular object (no chunking)
            let text_to_embed = safe_truncate(&text, 8000).to_string();
            group_items.push((obj.suid.clone(), text_to_embed));
            total_whole_files += 1;
        }

        // Embed this group's items in small batches
        log::info!("Group {} has {} items to embed", group_idx + 1, group_items.len());

        if group_items.is_empty() {
            log::info!("Group {} is empty, skipping to next group", group_idx + 1);
            continue;
        }

        for (batch_idx, batch) in group_items.chunks(EMBED_BATCH).enumerate() {
            log::info!("  Embedding batch {}: {} items", batch_idx + 1, batch.len());
            let texts: Vec<&str> = batch.iter().map(|(_, t)| t.as_str()).collect();

            // Embed with timeout
            let embed_result = tokio::time::timeout(
                tokio::time::Duration::from_secs(TIMEOUT_SECS),
                search.embeddings().embed_batch(&texts)
            ).await;

            match embed_result {
                Ok(Ok(embeddings)) => {
                    log::info!("  Batch {} embedded successfully, storing...", batch_idx + 1);
                    // Store embeddings (use write lock for safety)
                    let store = search.store.write().await;
                    for ((suid, _), embedding) in batch.iter().zip(embeddings.iter()) {
                        if store.store_embedding(suid, embedding, &model_name).is_ok() {
                            total_embedded += 1;
                        } else {
                            total_errors += 1;
                        }
                    }
                    drop(store); // Explicitly release lock
                    log::info!("  Batch {} stored. Total embedded: {}", batch_idx + 1, total_embedded);
                }
                Ok(Err(e)) => {
                    log::error!("  Batch {} embed FAILED: {}", batch_idx + 1, e);
                    total_errors += batch.len();
                }
                Err(_) => {
                    log::error!("  Batch {} TIMED OUT after {}s", batch_idx + 1, TIMEOUT_SECS);
                    total_errors += batch.len();
                }
            }

            // Small delay between batches
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }

        log::info!("=== GROUP {} COMPLETE: {} embedded so far, {} errors ===",
            group_idx + 1, total_embedded, total_errors);

        // Memory is released here as group_items goes out of scope
        tokio::task::yield_now().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    // Invalidate PCA cache
    if total_embedded > 0 || total_chunks > 0 {
        pca_cache.invalidate();
        log::info!("PCA cache invalidated after reindexing");
    }

    let _ = app.emit("reindex-progress", serde_json::json!({
        "stage": "done",
        "message": format!("Done! {} whole files, {} chunks, {} total", total_whole_files, total_chunks, total_embedded),
        "current": total_parents,
        "total": total_parents,
        "chunks_created": total_chunks,
        "whole_files": total_whole_files
    }));

    log::info!("=== REINDEX COMPLETE: {} whole files, {} chunks, {} total embedded, {} errors, {} skipped ===",
        total_whole_files, total_chunks, total_embedded, total_errors, total_skipped);

    Ok(format!("Embedded {} whole files, {} chunks, {} total ({} errors, {} skipped)",
        total_whole_files, total_chunks, total_embedded, total_errors, total_skipped))
}

/// Find objects similar to a specific object
#[tauri::command]
pub async fn find_similar(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    suid: String,
    limit: Option<usize>,
) -> Result<Vec<VectorSearchResult>, String> {
    let parsed_suid = crate::semantic_object::Suid::parse(&suid)
        .map_err(|e| format!("Invalid SUID: {}", e))?;

    let hits = search.find_similar(&parsed_suid, limit.unwrap_or(20)).await
        .map_err(|e| format!("Find similar failed: {}", e))?;

    let results: Vec<VectorSearchResult> = hits.into_iter()
        .map(|hit| {
            let preview = hit.object.content_as_str()
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>();

            VectorSearchResult {
                suid: hit.object.suid.to_string(),
                title: hit.object.name.clone(),
                content_type: format_content_type(&hit.object.content_type),
                similarity: hit.score,
                preview,
                tags: hit.object.tags.clone(),
                security_tier: format_security_tier(&hit.object.security_tier),
                path: hit.object.path.clone(),
                created_at: hit.object.created_at.to_rfc3339(),
            }
        })
        .collect();

    Ok(results)
}

/// Get vector clusters (grouping similar objects)
#[tauri::command]
pub async fn get_vector_clusters(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    min_similarity: Option<f32>,
    max_clusters: Option<usize>,
) -> Result<Vec<VectorCluster>, String> {
    use crate::semantic_search::SearchOptions;

    let min_sim = min_similarity.unwrap_or(0.7);
    let max_clusters = max_clusters.unwrap_or(10);

    // Get all objects with embeddings
    let all_objects = {
        let store = search.store.read().await;
        store.get_objects_with_embeddings(crate::memory::SecurityTier::Sealed)
            .map_err(|e| format!("Failed to get objects: {}", e))?
    };

    if all_objects.is_empty() {
        return Ok(Vec::new());
    }

    // Simple clustering: group by similarity using a greedy algorithm
    let mut clusters: Vec<VectorCluster> = Vec::new();
    let mut clustered_suids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (obj, embedding) in &all_objects {
        if clustered_suids.contains(&obj.suid.to_string()) {
            continue;
        }

        if clusters.len() >= max_clusters {
            break;
        }

        // Find similar objects to form a cluster
        let mut cluster_objects = Vec::new();

        for (other_obj, other_embedding) in &all_objects {
            if clustered_suids.contains(&other_obj.suid.to_string()) {
                continue;
            }

            let similarity = crate::embeddings::cosine_similarity(embedding, other_embedding);

            if similarity >= min_sim {
                let preview = other_obj.content_as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(200)
                    .collect::<String>();

                cluster_objects.push(VectorSearchResult {
                    suid: other_obj.suid.to_string(),
                    title: other_obj.name.clone(),
                    content_type: format_content_type(&other_obj.content_type),
                    similarity,
                    preview,
                    tags: other_obj.tags.clone(),
                    security_tier: format_security_tier(&other_obj.security_tier),
                    path: other_obj.path.clone(),
                    created_at: other_obj.created_at.to_rfc3339(),
                });

                clustered_suids.insert(other_obj.suid.to_string());
            }
        }

        if !cluster_objects.is_empty() {
            let avg_similarity: f32 = cluster_objects.iter()
                .map(|o| o.similarity)
                .sum::<f32>() / cluster_objects.len() as f32;

            // Derive theme from tags and titles
            let theme = derive_cluster_theme(&cluster_objects);

            clusters.push(VectorCluster {
                id: format!("cluster-{}", clusters.len()),
                theme,
                objects: cluster_objects,
                avg_similarity,
            });
        }
    }

    Ok(clusters)
}

/// Derive a theme name from cluster contents
fn derive_cluster_theme(objects: &[VectorSearchResult]) -> String {
    // Count tag prefixes (kind:*, project:*, etc.)
    let mut tag_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for obj in objects {
        for tag in &obj.tags {
            if let Some(prefix) = tag.split(':').next() {
                *tag_counts.entry(prefix.to_string()).or_insert(0) += 1;
            }
        }
    }

    // Get most common tag prefix
    let most_common = tag_counts.into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(tag, _)| tag)
        .unwrap_or_else(|| "mixed".to_string());

    match most_common.as_str() {
        "kind" => {
            // Look at the kind value
            for obj in objects {
                for tag in &obj.tags {
                    if tag.starts_with("kind:") {
                        return tag.replace("kind:", "").replace('-', " ");
                    }
                }
            }
            "Mixed Content".to_string()
        }
        "project" => {
            // Look at project tags
            for obj in objects {
                for tag in &obj.tags {
                    if tag.starts_with("project:") {
                        return tag.replace("project:", "");
                    }
                }
            }
            "Project Group".to_string()
        }
        _ => format!("{} Cluster", capitalize(&most_common)),
    }
}

fn capitalize(s: &str) -> String {
    s.chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + &s[c.len_utf8()..])
        .unwrap_or_else(|| s.to_string())
}

/// Get all objects with embeddings (bypasses semantic search)
#[tauri::command]
pub async fn get_all_vector_objects(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<VectorSearchResult>, String> {
    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);

    // Get all objects with embeddings
    let all_objects = {
        let store = search.store.read().await;
        store.get_objects_with_embeddings(crate::memory::SecurityTier::Sealed)
            .map_err(|e| format!("Failed to get objects: {}", e))?
    };

    // Paginate
    let paginated: Vec<_> = all_objects.into_iter()
        .skip(offset)
        .take(limit)
        .collect();

    // Convert to VectorSearchResult (similarity = 1.0 since it's exact match)
    let results: Vec<VectorSearchResult> = paginated.into_iter()
        .map(|(obj, _embedding)| {
            let preview = obj.content_as_str()
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>();

            VectorSearchResult {
                suid: obj.suid.to_string(),
                title: obj.name.clone(),
                content_type: format_content_type(&obj.content_type),
                similarity: 1.0, // Perfect match since it's the actual object
                preview,
                tags: obj.tags.clone(),
                security_tier: format_security_tier(&obj.security_tier),
                path: obj.path.clone(),
                created_at: obj.created_at.to_rfc3339(),
            }
        })
        .collect();

    Ok(results)
}

// ============================================================================
// Image Viewer Commands
// ============================================================================

/// Get image info (metadata without loading full image)
#[tauri::command]
pub async fn get_image_info(path: String) -> Result<ImageInfo, String> {
    let manager = ImageManager::new();
    let path = std::path::Path::new(&path);

    manager.get_info(path)
        .map_err(|e| format!("Failed to get image info: {}", e))
}

/// Render image to base64 PNG for display
#[tauri::command]
pub async fn render_image(
    path: String,
    max_dimension: Option<u32>,
    exposure: Option<f32>,
    gamma: Option<f32>,
) -> Result<String, String> {
    let mut manager = ImageManager::new();

    // Set custom tone mapping if provided
    if exposure.is_some() || gamma.is_some() {
        manager.set_tone_map_options(ToneMapOptions {
            exposure: exposure.unwrap_or(0.0),
            gamma: gamma.unwrap_or(2.2),
            reinhard: true,
            highlight_compression: 1.0,
        });
    }

    let path = std::path::Path::new(&path);
    let base64 = manager.render_to_base64(path, max_dimension)
        .map_err(|e| format!("Failed to render image: {}", e))?;

    Ok(format!("data:image/png;base64,{}", base64))
}

/// Get list of supported image formats
#[tauri::command]
pub fn get_supported_image_formats() -> Vec<String> {
    crate::images::supported_extensions()
        .into_iter()
        .map(|s| s.to_string())
        .collect()
}

/// Check if a file is a supported image
#[tauri::command]
pub fn is_supported_image(path: String) -> bool {
    crate::images::is_supported(std::path::Path::new(&path))
}

// ============================================================================
// Embedding Configuration Commands
// ============================================================================

use crate::embedding_store::{
    EmbeddingStore, EmbeddingConfig, SearchConfig,
    EmbeddingModelPreset, get_embedding_presets,
    test_embedding_provider,
};

/// Get the current embedding configuration
#[tauri::command]
pub fn embedding_get_config(
    store: State<'_, EmbeddingStore>,
) -> Result<EmbeddingConfig, String> {
    store.get_embedding_config()
}

/// Set the embedding configuration
#[tauri::command]
pub fn embedding_set_config(
    config: EmbeddingConfig,
    store: State<'_, EmbeddingStore>,
) -> Result<(), String> {
    store.set_embedding_config(config)
}

/// Test an embedding provider configuration
#[tauri::command]
pub async fn embedding_test_provider(
    config: EmbeddingConfig,
) -> Result<bool, String> {
    test_embedding_provider(&config).await
}

/// Get available embedding model presets
#[tauri::command]
pub fn embedding_get_presets() -> Vec<EmbeddingModelPreset> {
    get_embedding_presets()
}

/// Get the current search configuration
#[tauri::command]
pub fn search_get_config(
    store: State<'_, EmbeddingStore>,
) -> Result<SearchConfig, String> {
    store.get_search_config()
}

/// Set the search configuration
#[tauri::command]
pub fn search_set_config(
    config: SearchConfig,
    store: State<'_, EmbeddingStore>,
) -> Result<(), String> {
    store.set_search_config(config)
}

/// Reset embedding and search configuration to defaults
#[tauri::command]
pub fn embedding_reset_to_defaults(
    store: State<'_, EmbeddingStore>,
) -> Result<(), String> {
    store.reset_to_defaults()
}

// ============================================================================
// Embedding Analysis Commands
// ============================================================================

use crate::embedding_analysis::{
    EmbeddingSpaceAnalysis, ClusterAnalysis, QAPairStats, DiffVectorAnalysis,
    PredictionResult, VectorTargetingResult, GeneratedCandidate, InterpolationResult,
    InterpolationPoint, vector_diff, vector_add, average_vectors, find_nearest,
    analyze_diff_consistency, compute_stats, vector_magnitude, interpolate_vectors,
    normalize,
};
use crate::embeddings::cosine_similarity;

/// Analyze the embedding space structure
#[tauri::command]
pub async fn analyze_embedding_space(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
) -> Result<EmbeddingSpaceAnalysis, String> {
    let store = search.store.read().await;

    // Get all objects with embeddings
    let objects_with_embeddings = store
        .get_objects_with_embeddings(crate::memory::SecurityTier::Sealed)
        .map_err(|e| format!("Failed to get objects: {}", e))?;

    let total_objects = store.count().unwrap_or(0);
    let objects_with_emb = objects_with_embeddings.len();
    let dimensions = objects_with_embeddings
        .first()
        .map(|(_, emb)| emb.len())
        .unwrap_or(0);

    // Separate Q&A pairs (look for session chunks with user/assistant patterns)
    let mut questions: Vec<(String, String, Vec<f32>)> = Vec::new();
    let mut answers: Vec<(String, String, Vec<f32>)> = Vec::new();
    let mut all_embeddings: Vec<(String, Vec<f32>)> = Vec::new();

    for (obj, embedding) in &objects_with_embeddings {
        let content = obj.content_as_str().unwrap_or_default();
        let preview: String = content.chars().take(100).collect();

        all_embeddings.push((obj.suid.to_string(), embedding.clone()));

        // Heuristic: questions often start with interrogative words or end with ?
        let is_question = content.contains('?')
            || content.to_lowercase().starts_with("how ")
            || content.to_lowercase().starts_with("what ")
            || content.to_lowercase().starts_with("why ")
            || content.to_lowercase().starts_with("can ")
            || content.to_lowercase().starts_with("is ");

        // Check for user/assistant tags
        let has_user_tag = obj.tags.iter().any(|t| t.contains("user") || t.contains("human"));
        let has_assistant_tag = obj.tags.iter().any(|t| t.contains("assistant") || t.contains("ai"));

        if is_question || has_user_tag {
            questions.push((obj.suid.to_string(), preview, embedding.clone()));
        } else if has_assistant_tag || (!is_question && content.len() > 50) {
            answers.push((obj.suid.to_string(), preview, embedding.clone()));
        }
    }

    // Compute cluster analysis
    let cluster_analysis = if !questions.is_empty() && !answers.is_empty() {
        // Compute Q-A similarities (pair each Q with nearest A)
        let mut qa_similarities: Vec<f32> = Vec::new();
        let mut pair_stats: Vec<QAPairStats> = Vec::new();

        for (q_id, q_preview, q_emb) in &questions {
            if let Some((a_id, sim)) = find_nearest(q_emb, &answers.iter().map(|(id, _, emb)| (id.clone(), emb.clone())).collect::<Vec<_>>(), &[]) {
                qa_similarities.push(sim);

                let a_preview = answers.iter()
                    .find(|(id, _, _)| id == a_id)
                    .map(|(_, p, _)| p.clone())
                    .unwrap_or_default();

                let diff = vector_diff(q_emb, &answers.iter().find(|(id, _, _)| id == a_id).unwrap().2);

                pair_stats.push(QAPairStats {
                    question_id: q_id.clone(),
                    answer_id: a_id.to_string(),
                    question_preview: q_preview.clone(),
                    answer_preview: a_preview,
                    similarity: sim,
                    distance: 1.0 - sim,
                    diff_magnitude: vector_magnitude(&diff),
                });
            }
        }

        // Compute random pair similarities (baseline)
        let mut random_similarities: Vec<f32> = Vec::new();
        for i in 0..all_embeddings.len().min(100) {
            for j in (i + 1)..all_embeddings.len().min(100) {
                random_similarities.push(cosine_similarity(&all_embeddings[i].1, &all_embeddings[j].1));
            }
        }

        let (avg_qa, std_qa) = compute_stats(&qa_similarities);
        let (avg_random, _) = compute_stats(&random_similarities);

        Some(ClusterAnalysis {
            avg_qa_similarity: avg_qa,
            std_qa_similarity: std_qa,
            avg_random_similarity: avg_random,
            qa_vs_random_ratio: if avg_random > 0.0 { avg_qa / avg_random } else { 1.0 },
            pairs: pair_stats.into_iter().take(20).collect(), // Limit to 20 samples
        })
    } else {
        None
    };

    // Analyze diff vectors
    let diff_analysis = if let Some(ref cluster) = cluster_analysis {
        if cluster.pairs.len() >= 3 {
            // Compute diff vectors for each Q-A pair
            let mut diff_vectors: Vec<Vec<f32>> = Vec::new();

            for pair in &cluster.pairs {
                let q_emb = questions.iter().find(|(id, _, _)| id == &pair.question_id).map(|(_, _, e)| e);
                let a_emb = answers.iter().find(|(id, _, _)| id == &pair.answer_id).map(|(_, _, e)| e);

                if let (Some(q), Some(a)) = (q_emb, a_emb) {
                    diff_vectors.push(vector_diff(q, a));
                }
            }

            let consistency = analyze_diff_consistency(&diff_vectors);
            let avg_magnitude = diff_vectors.iter().map(|d| vector_magnitude(d)).sum::<f32>() / diff_vectors.len() as f32;

            // Test prediction: Q + avg_diff ≈ A?
            let avg_diff = average_vectors(&diff_vectors);
            let mut predictions: Vec<PredictionResult> = Vec::new();
            let mut prediction_scores: Vec<f32> = Vec::new();

            if let Some(ref avg_d) = avg_diff {
                for (i, (q_id, q_preview, q_emb)) in questions.iter().take(5).enumerate() {
                    let predicted = vector_add(q_emb, avg_d);

                    if let Some((nearest_id, sim)) = find_nearest(&predicted, &answers.iter().map(|(id, _, emb)| (id.clone(), emb.clone())).collect::<Vec<_>>(), &[]) {
                        let nearest_preview = answers.iter()
                            .find(|(id, _, _)| id == nearest_id)
                            .map(|(_, p, _)| p.clone())
                            .unwrap_or_default();

                        // Check if prediction matches actual answer
                        let actual_answer = cluster.pairs.iter()
                            .find(|p| p.question_id == *q_id)
                            .map(|p| p.answer_preview.clone())
                            .unwrap_or_default();

                        prediction_scores.push(sim);
                        predictions.push(PredictionResult {
                            question: q_preview.clone(),
                            actual_answer,
                            predicted_nearest: nearest_preview,
                            similarity: sim,
                        });
                    }
                }
            }

            let (avg_pred, _) = compute_stats(&prediction_scores);

            Some(DiffVectorAnalysis {
                diff_consistency: consistency,
                avg_diff_magnitude: avg_magnitude,
                prediction_accuracy: avg_pred,
                sample_predictions: predictions,
            })
        } else {
            None
        }
    } else {
        None
    };

    // Generate recommendations
    let mut recommendations: Vec<String> = Vec::new();

    if let Some(ref cluster) = cluster_analysis {
        if cluster.avg_qa_similarity < 0.5 {
            recommendations.push("Low Q&A similarity suggests questions and answers aren't well-connected semantically".to_string());
        }
        if cluster.qa_vs_random_ratio < 1.5 {
            recommendations.push("Q&A pairs aren't much more similar than random pairs - embeddings may not capture conversation context well".to_string());
        }
        if cluster.qa_vs_random_ratio > 2.0 {
            recommendations.push("Good! Q&A pairs are significantly more similar than random pairs".to_string());
        }
    }

    if let Some(ref diff) = diff_analysis {
        if diff.diff_consistency > 0.7 {
            recommendations.push(format!("Diff vectors are consistent ({:.0}%) - there's a learnable 'answer direction'", diff.diff_consistency * 100.0));
        } else {
            recommendations.push(format!("Diff vectors vary widely ({:.0}% consistency) - answers are topic-dependent", diff.diff_consistency * 100.0));
        }

        if diff.prediction_accuracy > 0.6 {
            recommendations.push("Vector arithmetic works! Q + avg_diff predicts answers reasonably well".to_string());
        }
    }

    if questions.is_empty() || answers.is_empty() {
        recommendations.push("Not enough Q&A pairs found for analysis. Try indexing more sessions.".to_string());
    }

    Ok(EmbeddingSpaceAnalysis {
        total_objects,
        objects_with_embeddings: objects_with_emb,
        embedding_dimensions: dimensions,
        cluster_analysis,
        diff_vector_analysis: diff_analysis,
        recommendations,
    })
}

/// Generate text that targets a specific vector region
/// Uses iterative generation: generate candidates, embed them, keep best, refine
#[tauri::command]
pub async fn generate_toward_vector(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    ai: tauri::State<'_, std::sync::Arc<crate::ai::AiManager>>,
    target_concept: String,
    iterations: Option<usize>,
) -> Result<VectorTargetingResult, String> {
    let max_iterations = iterations.unwrap_or(3);

    // First, embed the target concept
    let target_embedding = search.embeddings().embed(&target_concept).await
        .map_err(|e| format!("Failed to embed target: {}", e))?;

    let mut all_candidates: Vec<GeneratedCandidate> = Vec::new();
    let mut best_so_far: Option<GeneratedCandidate> = None;
    let mut current_prompt = target_concept.clone();

    for iteration in 0..max_iterations {
        // Generate variations using LLM
        let prompt = if iteration == 0 {
            format!(
                "Generate 5 different short phrases (one per line) that express the same concept as: \"{}\"\n\
                Be creative but stay semantically similar. Just output the phrases, nothing else.",
                current_prompt
            )
        } else {
            format!(
                "The phrase \"{}\" is close but not quite right for expressing: \"{}\"\n\
                Generate 5 alternative short phrases (one per line) that might be even closer semantically.\n\
                Just output the phrases, nothing else.",
                best_so_far.as_ref().map(|b| b.text.as_str()).unwrap_or(&current_prompt),
                target_concept
            )
        };

        let messages = vec![
            crate::ai::Message {
                role: crate::ai::Role::User,
                content: prompt,
            }
        ];

        let response = ai.chat(messages, None).await
            .map_err(|e| format!("LLM generation failed: {}", e))?;

        // Parse candidates from response
        let candidates: Vec<String> = response.content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && l.len() > 3 && l.len() < 200)
            .map(|l| l.trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == ')' || c == '-').trim().to_string())
            .take(5)
            .collect();

        // Embed each candidate and compute similarity to target
        for candidate_text in candidates {
            if candidate_text.is_empty() {
                continue;
            }

            match search.embeddings().embed(&candidate_text).await {
                Ok(candidate_emb) => {
                    let similarity = cosine_similarity(&target_embedding, &candidate_emb);

                    let candidate = GeneratedCandidate {
                        text: candidate_text,
                        similarity_to_target: similarity,
                        iteration,
                    };

                    // Update best if this is better
                    if best_so_far.as_ref().map(|b| similarity > b.similarity_to_target).unwrap_or(true) {
                        best_so_far = Some(candidate.clone());
                    }

                    all_candidates.push(candidate);
                }
                Err(e) => {
                    log::warn!("Failed to embed candidate: {}", e);
                }
            }
        }

        // Check convergence (similarity > 0.9)
        if best_so_far.as_ref().map(|b| b.similarity_to_target > 0.9).unwrap_or(false) {
            break;
        }
    }

    // Sort candidates by similarity
    all_candidates.sort_by(|a, b| b.similarity_to_target.partial_cmp(&a.similarity_to_target).unwrap_or(std::cmp::Ordering::Equal));

    let converged = best_so_far.as_ref().map(|b| b.similarity_to_target > 0.9).unwrap_or(false);

    Ok(VectorTargetingResult {
        target_description: target_concept,
        candidates: all_candidates.into_iter().take(10).collect(),
        best_match: best_so_far,
        iterations: max_iterations,
        converged,
    })
}

/// Interpolate between two concepts and find what exists along the path
#[tauri::command]
pub async fn interpolate_concepts(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    start_concept: String,
    end_concept: String,
    steps: Option<usize>,
) -> Result<InterpolationResult, String> {
    let num_steps = steps.unwrap_or(5);

    // Embed both concepts
    let start_emb = search.embeddings().embed(&start_concept).await
        .map_err(|e| format!("Failed to embed start: {}", e))?;
    let end_emb = search.embeddings().embed(&end_concept).await
        .map_err(|e| format!("Failed to embed end: {}", e))?;

    // Get all objects for finding nearest
    let store = search.store.read().await;
    let objects_with_embeddings = store
        .get_objects_with_embeddings(crate::memory::SecurityTier::Sealed)
        .map_err(|e| format!("Failed to get objects: {}", e))?;

    let candidates: Vec<(String, Vec<f32>)> = objects_with_embeddings
        .iter()
        .map(|(obj, emb)| {
            let preview: String = obj.content_as_str().unwrap_or_default().chars().take(150).collect();
            (preview, emb.clone())
        })
        .collect();

    drop(store);

    // Interpolate and find nearest at each step
    let mut path: Vec<InterpolationPoint> = Vec::new();

    for i in 0..=num_steps {
        let t = i as f32 / num_steps as f32;
        let interpolated = interpolate_vectors(&start_emb, &end_emb, t);

        // Find nearest existing content
        if let Some((nearest, similarity)) = find_nearest(&interpolated, &candidates, &[]) {
            path.push(InterpolationPoint {
                t,
                nearest_content: nearest.to_string(),
                similarity,
            });
        }
    }

    Ok(InterpolationResult {
        start_text: start_concept,
        end_text: end_concept,
        path,
    })
}

/// Find the "semantic midpoint" between two concepts
#[tauri::command]
pub async fn find_semantic_midpoint(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    concept_a: String,
    concept_b: String,
) -> Result<serde_json::Value, String> {
    // Embed both
    let emb_a = search.embeddings().embed(&concept_a).await
        .map_err(|e| format!("Failed to embed A: {}", e))?;
    let emb_b = search.embeddings().embed(&concept_b).await
        .map_err(|e| format!("Failed to embed B: {}", e))?;

    // Compute midpoint
    let midpoint = interpolate_vectors(&emb_a, &emb_b, 0.5);

    // Find nearest objects to midpoint
    let store = search.store.read().await;
    let objects_with_embeddings = store
        .get_objects_with_embeddings(crate::memory::SecurityTier::Sealed)
        .map_err(|e| format!("Failed to get objects: {}", e))?;

    let mut scored: Vec<(String, f32)> = objects_with_embeddings
        .iter()
        .map(|(obj, emb)| {
            let preview: String = obj.content_as_str().unwrap_or_default().chars().take(150).collect();
            (preview, cosine_similarity(&midpoint, emb))
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top5: Vec<_> = scored.into_iter().take(5).collect();

    // Also compute how similar A and B are to each other
    let ab_similarity = cosine_similarity(&emb_a, &emb_b);

    Ok(serde_json::json!({
        "concept_a": concept_a,
        "concept_b": concept_b,
        "similarity_between_concepts": ab_similarity,
        "semantic_midpoint_candidates": top5.iter().map(|(preview, score)| {
            serde_json::json!({
                "content": preview,
                "similarity_to_midpoint": score
            })
        }).collect::<Vec<_>>(),
        "interpretation": format!(
            "The semantic midpoint between '{}' and '{}' (similarity: {:.2}) is closest to the content shown above.",
            concept_a, concept_b, ab_similarity
        )
    }))
}

/// Analyze what dimensions differ most between two concepts
#[tauri::command]
pub async fn analyze_vector_difference(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    concept_a: String,
    concept_b: String,
) -> Result<serde_json::Value, String> {
    let emb_a = search.embeddings().embed(&concept_a).await
        .map_err(|e| format!("Failed to embed A: {}", e))?;
    let emb_b = search.embeddings().embed(&concept_b).await
        .map_err(|e| format!("Failed to embed B: {}", e))?;

    let diff = vector_diff(&emb_a, &emb_b);
    let magnitude = vector_magnitude(&diff);
    let similarity = cosine_similarity(&emb_a, &emb_b);

    // Find dimensions with largest absolute differences
    let mut dim_diffs: Vec<(usize, f32)> = diff.iter()
        .enumerate()
        .map(|(i, &d)| (i, d.abs()))
        .collect();
    dim_diffs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let top_dimensions: Vec<_> = dim_diffs.iter().take(10).collect();

    // Compute some statistics about the difference
    let (mean_diff, std_diff) = compute_stats(&diff.iter().map(|x| x.abs()).collect::<Vec<_>>());

    // Count how many dimensions changed significantly (> 2 std devs)
    let significant_threshold = mean_diff + 2.0 * std_diff;
    let significant_dims = diff.iter().filter(|&&d| d.abs() > significant_threshold).count();

    Ok(serde_json::json!({
        "concept_a": concept_a,
        "concept_b": concept_b,
        "cosine_similarity": similarity,
        "euclidean_distance": magnitude,
        "total_dimensions": diff.len(),
        "mean_absolute_difference": mean_diff,
        "std_absolute_difference": std_diff,
        "significant_dimensions": significant_dims,
        "top_differing_dimensions": top_dimensions.iter().map(|(dim, val)| {
            serde_json::json!({
                "dimension": dim,
                "absolute_difference": val,
                "direction": if diff[*dim] > 0.0 { "B > A" } else { "A > B" }
            })
        }).collect::<Vec<_>>(),
        "interpretation": format!(
            "These concepts have {:.1}% similarity. {} out of {} dimensions show significant differences.",
            similarity * 100.0,
            significant_dims,
            diff.len()
        )
    }))
}

/// Test vector arithmetic: A - B + C = ?
#[tauri::command]
pub async fn test_vector_arithmetic(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    concept_a: String,
    concept_b: String,
    concept_c: String,
) -> Result<serde_json::Value, String> {
    // Embed all concepts
    let emb_a = search.embeddings().embed(&concept_a).await
        .map_err(|e| format!("Failed to embed A: {}", e))?;
    let emb_b = search.embeddings().embed(&concept_b).await
        .map_err(|e| format!("Failed to embed B: {}", e))?;
    let emb_c = search.embeddings().embed(&concept_c).await
        .map_err(|e| format!("Failed to embed C: {}", e))?;

    // Compute A - B + C
    let diff = vector_diff(&emb_b, &emb_a); // A - B
    let result = vector_add(&emb_c, &diff);  // (A - B) + C

    // Find nearest objects to the result
    let store = search.store.read().await;
    let objects_with_embeddings = store
        .get_objects_with_embeddings(crate::memory::SecurityTier::Sealed)
        .map_err(|e| format!("Failed to get objects: {}", e))?;

    let candidates: Vec<(String, Vec<f32>)> = objects_with_embeddings
        .iter()
        .map(|(obj, emb)| {
            let preview: String = obj.content_as_str().unwrap_or_default().chars().take(100).collect();
            (preview, emb.clone())
        })
        .collect();

    // Find top 5 nearest
    let mut scored: Vec<(String, f32)> = candidates
        .iter()
        .map(|(preview, emb)| (preview.clone(), cosine_similarity(&result, emb)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top5: Vec<_> = scored.into_iter().take(5).collect();

    Ok(serde_json::json!({
        "operation": format!("({} - {}) + {}", concept_a, concept_b, concept_c),
        "interpretation": format!("'{}' transformed by the difference between '{}' and '{}'", concept_c, concept_a, concept_b),
        "nearest_results": top5.iter().map(|(preview, score)| {
            serde_json::json!({
                "content": preview,
                "similarity": score
            })
        }).collect::<Vec<_>>()
    }))
}

// ============================================================================
// 3D IDEA SPACE COMMANDS - Dimensionality Reduction for VR Visualization
// ============================================================================

use crate::embedding_analysis::{
    IdeaSpacePoint, IdeaSpace3D, SemanticAxis,
    simple_pca, project_to_3d, normalize_coordinates, gram_schmidt,
    euclidean_distance,
};

/// Get all objects projected into 3D idea space
/// projection_mode: "pca" (default), "folded", or "folded_mean"/"folded_max"/"folded_variance"
#[tauri::command]
pub async fn get_idea_space_3d(
    app: tauri::AppHandle,
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    pca_cache: tauri::State<'_, crate::pca_cache::PCACacheManager>,
    projection_mode: Option<String>,
) -> Result<IdeaSpace3D, String> {
    use tauri::Emitter;
    use crate::embedding_analysis::{project_folded, project_folded_method, vector_magnitude};

    let mode = projection_mode.unwrap_or_else(|| "pca".to_string());

    // Emit progress: loading
    let _ = app.emit("idea-space-progress", serde_json::json!({
        "stage": "loading",
        "message": format!("Loading vectors ({} mode)...", mode),
        "percent": 5
    }));

    let store = search.store.read().await;
    let objects_with_embeddings = store
        .get_objects_with_embeddings(crate::memory::SecurityTier::Sealed)
        .map_err(|e| format!("Failed to get objects: {}", e))?;

    if objects_with_embeddings.len() < 3 {
        return Err("Need at least 3 objects with embeddings for 3D projection".to_string());
    }

    let total_count = objects_with_embeddings.len();
    let dimensions = objects_with_embeddings.first().map(|(_, e)| e.len()).unwrap_or(0);

    // Check cache status with smart validation
    use crate::pca_cache::CacheStatus;
    let cache_status = pca_cache.check(total_count);

    let pca = match cache_status {
        CacheStatus::Valid(cached_pca) => {
            // Exact match - use cached PCA directly
            let _ = app.emit("idea-space-progress", serde_json::json!({
                "stage": "cached",
                "message": format!("Using cached PCA for {} vectors", total_count),
                "percent": 70
            }));
            cached_pca
        }
        CacheStatus::UsableWithNewItems { pca, cached_count, new_count } => {
            // Cache is still usable - just project new items with existing PCA
            let _ = app.emit("idea-space-progress", serde_json::json!({
                "stage": "cached",
                "message": format!("Using cached PCA ({} vectors) + {} new items", cached_count, new_count),
                "percent": 70
            }));
            // Update cache count for next time
            pca_cache.update_count(total_count);
            pca
        }
        CacheStatus::Stale { reason } => {
            let _ = app.emit("idea-space-progress", serde_json::json!({
                "stage": "recomputing",
                "message": format!("Recomputing PCA: {}", reason),
                "percent": 10
            }));

            // Emit progress: preparing
            let _ = app.emit("idea-space-progress", serde_json::json!({
                "stage": "preparing",
                "message": format!("Preparing {} vectors for PCA...", total_count),
                "percent": 15
            }));

            // Collect embeddings for PCA with progress
            let mut embeddings: Vec<&[f32]> = Vec::with_capacity(objects_with_embeddings.len());
            for (i, (_, emb)) in objects_with_embeddings.iter().enumerate() {
                embeddings.push(emb.as_slice());
                if i % 100 == 0 {
                    let percent = 15 + (i * 10 / total_count);
                    let _ = app.emit("idea-space-progress", serde_json::json!({
                        "stage": "collecting",
                        "message": format!("Collecting vectors: {}/{}", i, total_count),
                        "percent": percent
                    }));
                }
            }

            // Emit progress: computing PCA (the slow part)
            let _ = app.emit("idea-space-progress", serde_json::json!({
                "stage": "pca",
                "message": format!("Computing PCA on {} vectors ({} dimensions)...", total_count, dimensions),
                "percent": 25
            }));

            // Perform PCA to get 3 principal components with progress reporting
            let app_clone = app.clone();
            let computed_pca = crate::embedding_analysis::simple_pca_with_progress(&embeddings, 3, move |current, total| {
                if current % 100 == 0 {
                    let percent = 25 + (current * 40 / total.max(1));
                    let _ = app_clone.emit("idea-space-progress", serde_json::json!({
                        "stage": "pca",
                        "message": format!("Computing covariance matrix: {}/{}", current, total),
                        "percent": percent
                    }));
                }
            }).ok_or("PCA failed")?;

            // Cache the result for next time
            pca_cache.set(computed_pca.clone(), total_count, dimensions);

            let _ = app.emit("idea-space-progress", serde_json::json!({
                "stage": "cached",
                "message": "PCA computed and cached for future use",
                "percent": 70
            }));

            computed_pca
        }
        CacheStatus::None => {
            // Emit progress: preparing
            let _ = app.emit("idea-space-progress", serde_json::json!({
                "stage": "preparing",
                "message": format!("Preparing {} vectors for PCA...", total_count),
                "percent": 15
            }));

            // Collect embeddings for PCA with progress
            let mut embeddings: Vec<&[f32]> = Vec::with_capacity(objects_with_embeddings.len());
            for (i, (_, emb)) in objects_with_embeddings.iter().enumerate() {
                embeddings.push(emb.as_slice());
                if i % 100 == 0 {
                    let percent = 15 + (i * 10 / total_count);
                    let _ = app.emit("idea-space-progress", serde_json::json!({
                        "stage": "collecting",
                        "message": format!("Collecting vectors: {}/{}", i, total_count),
                        "percent": percent
                    }));
                }
            }

            // Emit progress: computing PCA (the slow part)
            let _ = app.emit("idea-space-progress", serde_json::json!({
                "stage": "pca",
                "message": format!("Computing PCA on {} vectors ({} dimensions)...", total_count, dimensions),
                "percent": 25
            }));

            // Perform PCA to get 3 principal components with progress reporting
            let app_clone = app.clone();
            let computed_pca = crate::embedding_analysis::simple_pca_with_progress(&embeddings, 3, move |current, total| {
                if current % 100 == 0 {
                    let percent = 25 + (current * 40 / total.max(1));
                    let _ = app_clone.emit("idea-space-progress", serde_json::json!({
                        "stage": "pca",
                        "message": format!("Computing covariance matrix: {}/{}", current, total),
                        "percent": percent
                    }));
                }
            }).ok_or("PCA failed")?;

            // Cache the result for next time
            pca_cache.set(computed_pca.clone(), total_count, dimensions);

            let _ = app.emit("idea-space-progress", serde_json::json!({
                "stage": "cached",
                "message": "PCA computed and cached for future use",
                "percent": 70
            }));

            computed_pca
        }
    };

    // Emit progress: projecting
    let _ = app.emit("idea-space-progress", serde_json::json!({
        "stage": "projecting",
        "message": format!("Projecting {} vectors to 3D...", total_count),
        "percent": 75
    }));

    // Calculate total variance for explained ratio
    let total_variance: f32 = pca.explained_variance.iter().sum();
    let variance_captured = if total_variance > 0.0 {
        pca.explained_variance.iter().take(3).sum::<f32>() / total_variance
    } else {
        0.0
    };

    // Compute center (mean) magnitude for distance calculations
    let center_magnitude = vector_magnitude(&pca.mean);

    // Project each object to 3D with progress reporting
    let mut points: Vec<IdeaSpacePoint> = Vec::with_capacity(objects_with_embeddings.len());
    for (i, (obj, embedding)) in objects_with_embeddings.iter().enumerate() {
        // Report progress every 100 vectors
        if i % 100 == 0 {
            let percent = 75 + (i * 20 / total_count);
            let _ = app.emit("idea-space-progress", serde_json::json!({
                "stage": "projecting",
                "message": format!("Projecting to 3D ({}): {}/{}", mode, i, total_count),
                "percent": percent
            }));
        }

        // Choose projection method based on mode
        let (x, y, z) = match mode.as_str() {
            "folded" | "folded_sum" => project_folded(embedding),
            "folded_mean" => project_folded_method(embedding, "mean"),
            "folded_max" => project_folded_method(embedding, "max"),
            "folded_variance" => project_folded_method(embedding, "variance"),
            "folded_l2" => project_folded_method(embedding, "l2"),
            _ => project_to_3d(embedding, &pca), // default to PCA
        };

        let distance = euclidean_distance(embedding, &pca.mean);
        let mag = vector_magnitude(embedding);

        // Convert ContentType to string
        let object_type = match &obj.content_type {
            crate::semantic_object::ContentType::Text => "text".to_string(),
            crate::semantic_object::ContentType::Markdown => "markdown".to_string(),
            crate::semantic_object::ContentType::Code { language } => format!("code:{}", language),
            crate::semantic_object::ContentType::Json => "json".to_string(),
            crate::semantic_object::ContentType::Binary { mime } => format!("binary:{}", mime),
            crate::semantic_object::ContentType::Structured { schema } => format!("structured:{}", schema),
            crate::semantic_object::ContentType::Unknown => "unknown".to_string(),
        };

        points.push(IdeaSpacePoint {
            id: obj.suid.to_string(),
            name: obj.name.clone().unwrap_or_else(|| "Untitled".to_string()),
            object_type,
            x,
            y,
            z,
            distance_from_center: distance,
            magnitude: mag,
            tags: obj.tags.clone(),
            preview: obj.summary.clone().unwrap_or_default().chars().take(100).collect(),
        });
    }

    // Emit progress: normalizing
    let _ = app.emit("idea-space-progress", serde_json::json!({
        "stage": "normalizing",
        "message": "Normalizing coordinates...",
        "percent": 90
    }));

    // Normalize coordinates to [-1, 1]
    let bounds = normalize_coordinates(&mut points);

    // Emit progress: done
    let _ = app.emit("idea-space-progress", serde_json::json!({
        "stage": "done",
        "message": format!("Ready! {} points in 3D space", points.len()),
        "percent": 100
    }));

    // Create axis descriptions based on mode
    let axes = if mode.starts_with("folded") {
        let method = if mode.contains("mean") { "mean" }
            else if mode.contains("max") { "max" }
            else if mode.contains("variance") { "variance" }
            else if mode.contains("l2") { "L2 norm" }
            else { "sum" };
        vec![
            SemanticAxis {
                name: "X (dims 0-340)".to_string(),
                negative_label: format!("← Low {} (first third)", method),
                positive_label: format!("High {} (first third) →", method),
                direction: vec![], // No direction vector for folded
            },
            SemanticAxis {
                name: "Y (dims 341-681)".to_string(),
                negative_label: format!("← Low {} (middle third)", method),
                positive_label: format!("High {} (middle third) →", method),
                direction: vec![],
            },
            SemanticAxis {
                name: "Z (dims 682-1023)".to_string(),
                negative_label: format!("← Low {} (last third)", method),
                positive_label: format!("High {} (last third) →", method),
                direction: vec![],
            },
        ]
    } else {
        // PCA mode
        vec![
            SemanticAxis {
                name: "PC1".to_string(),
                negative_label: "← Primary Axis -".to_string(),
                positive_label: "Primary Axis + →".to_string(),
                direction: pca.components.get(0).cloned().unwrap_or_default(),
            },
            SemanticAxis {
                name: "PC2".to_string(),
                negative_label: "← Secondary Axis -".to_string(),
                positive_label: "Secondary Axis + →".to_string(),
                direction: pca.components.get(1).cloned().unwrap_or_default(),
            },
            SemanticAxis {
                name: "PC3".to_string(),
                negative_label: "← Tertiary Axis -".to_string(),
                positive_label: "Tertiary Axis + →".to_string(),
                direction: pca.components.get(2).cloned().unwrap_or_default(),
            },
        ]
    };

    Ok(IdeaSpace3D {
        points,
        axes,
        variance_captured,
        bounds,
    })
}

/// Get 3D idea space with custom semantic axes
/// Example: axes could be ["simple", "complex"], ["past", "future"], ["technical", "creative"]
#[tauri::command]
pub async fn get_idea_space_custom_axes(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    axis_definitions: Vec<(String, String, String)>,  // (name, negative, positive)
) -> Result<IdeaSpace3D, String> {
    if axis_definitions.len() < 3 {
        return Err("Need at least 3 axis definitions for 3D space".to_string());
    }

    let store = search.store.read().await;
    let objects_with_embeddings = store
        .get_objects_with_embeddings(crate::memory::SecurityTier::Sealed)
        .map_err(|e| format!("Failed to get objects: {}", e))?;

    if objects_with_embeddings.is_empty() {
        return Err("No objects with embeddings found".to_string());
    }

    // Embed axis endpoints
    let mut axes: Vec<SemanticAxis> = Vec::new();
    for (name, neg, pos) in &axis_definitions {
        let neg_emb = search.embeddings().embed(neg).await
            .map_err(|e| format!("Failed to embed '{}': {}", neg, e))?;
        let pos_emb = search.embeddings().embed(pos).await
            .map_err(|e| format!("Failed to embed '{}': {}", pos, e))?;

        let direction = normalize(&vector_diff(&neg_emb, &pos_emb));

        axes.push(SemanticAxis {
            name: name.clone(),
            negative_label: neg.clone(),
            positive_label: pos.clone(),
            direction,
        });
    }

    // Orthogonalize the axes using Gram-Schmidt
    let directions: Vec<Vec<f32>> = axes.iter().map(|a| a.direction.clone()).collect();
    let orthogonal = gram_schmidt(&directions);

    // Update axes with orthogonalized directions
    for (i, axis) in axes.iter_mut().enumerate() {
        if i < orthogonal.len() {
            axis.direction = orthogonal[i].clone();
        }
    }

    // Project each object onto the custom axes
    let mut points: Vec<IdeaSpacePoint> = objects_with_embeddings
        .iter()
        .map(|(obj, embedding)| {
            // Project onto each axis
            let x = if !axes.is_empty() {
                embedding.iter().zip(axes[0].direction.iter()).map(|(a, b)| a * b).sum()
            } else { 0.0 };
            let y = if axes.len() > 1 {
                embedding.iter().zip(axes[1].direction.iter()).map(|(a, b)| a * b).sum()
            } else { 0.0 };
            let z = if axes.len() > 2 {
                embedding.iter().zip(axes[2].direction.iter()).map(|(a, b)| a * b).sum()
            } else { 0.0 };

            // Convert ContentType to string
            let object_type = match &obj.content_type {
                crate::semantic_object::ContentType::Text => "text".to_string(),
                crate::semantic_object::ContentType::Markdown => "markdown".to_string(),
                crate::semantic_object::ContentType::Code { language } => format!("code:{}", language),
                crate::semantic_object::ContentType::Json => "json".to_string(),
                crate::semantic_object::ContentType::Binary { mime } => format!("binary:{}", mime),
                crate::semantic_object::ContentType::Structured { schema } => format!("structured:{}", schema),
                crate::semantic_object::ContentType::Unknown => "unknown".to_string(),
            };

            let mag = vector_magnitude(embedding);
            IdeaSpacePoint {
                id: obj.suid.to_string(),
                name: obj.name.clone().unwrap_or_else(|| "Untitled".to_string()),
                object_type,
                x,
                y,
                z,
                distance_from_center: mag,
                magnitude: mag,
                tags: obj.tags.clone(),
                preview: obj.summary.clone().unwrap_or_default().chars().take(100).collect(),
            }
        })
        .collect();

    // Normalize coordinates to [-1, 1]
    let bounds = normalize_coordinates(&mut points);

    Ok(IdeaSpace3D {
        points,
        axes: axes.into_iter().take(3).collect(),
        variance_captured: 1.0, // Custom axes capture what we defined
        bounds,
    })
}

/// Find objects near a specific point in 3D idea space
#[tauri::command]
pub async fn find_near_point_3d(
    app: tauri::AppHandle,
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    pca_cache: tauri::State<'_, crate::pca_cache::PCACacheManager>,
    x: f32,
    y: f32,
    z: f32,
    limit: Option<usize>,
) -> Result<Vec<IdeaSpacePoint>, String> {
    // First get the full 3D space
    let space = get_idea_space_3d(app, search, pca_cache, None).await?;

    let limit = limit.unwrap_or(10);

    // Find objects closest to the target point
    let mut points_with_distance: Vec<(IdeaSpacePoint, f32)> = space.points
        .into_iter()
        .map(|p| {
            let dist = ((p.x - x).powi(2) + (p.y - y).powi(2) + (p.z - z).powi(2)).sqrt();
            (p, dist)
        })
        .collect();

    points_with_distance.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    Ok(points_with_distance.into_iter().take(limit).map(|(p, _)| p).collect())
}

/// Get cluster centers in 3D space (finds natural groupings)
#[tauri::command]
pub async fn get_idea_clusters(
    app: tauri::AppHandle,
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    pca_cache: tauri::State<'_, crate::pca_cache::PCACacheManager>,
    num_clusters: Option<usize>,
) -> Result<serde_json::Value, String> {
    let space = get_idea_space_3d(app, search, pca_cache, None).await?;

    let num_clusters = num_clusters.unwrap_or(5).min(space.points.len());
    if num_clusters == 0 {
        return Ok(serde_json::json!({ "clusters": [] }));
    }

    // Simple k-means clustering in 3D space
    let mut centroids: Vec<(f32, f32, f32)> = space.points.iter()
        .take(num_clusters)
        .map(|p| (p.x, p.y, p.z))
        .collect();

    // Run k-means iterations
    for _ in 0..20 {
        // Assign points to nearest centroid
        let mut assignments: Vec<Vec<&IdeaSpacePoint>> = vec![Vec::new(); num_clusters];

        for point in &space.points {
            let mut best_cluster = 0;
            let mut best_dist = f32::MAX;

            for (i, centroid) in centroids.iter().enumerate() {
                let dist = ((point.x - centroid.0).powi(2) +
                           (point.y - centroid.1).powi(2) +
                           (point.z - centroid.2).powi(2)).sqrt();
                if dist < best_dist {
                    best_dist = dist;
                    best_cluster = i;
                }
            }

            assignments[best_cluster].push(point);
        }

        // Update centroids
        for (i, cluster) in assignments.iter().enumerate() {
            if !cluster.is_empty() {
                let n = cluster.len() as f32;
                centroids[i] = (
                    cluster.iter().map(|p| p.x).sum::<f32>() / n,
                    cluster.iter().map(|p| p.y).sum::<f32>() / n,
                    cluster.iter().map(|p| p.z).sum::<f32>() / n,
                );
            }
        }
    }

    // Final assignment and cluster info
    let mut clusters: Vec<serde_json::Value> = Vec::new();

    for (i, centroid) in centroids.iter().enumerate() {
        let members: Vec<&IdeaSpacePoint> = space.points.iter()
            .filter(|p| {
                let mut best_cluster = 0;
                let mut best_dist = f32::MAX;
                for (j, c) in centroids.iter().enumerate() {
                    let dist = ((p.x - c.0).powi(2) + (p.y - c.1).powi(2) + (p.z - c.2).powi(2)).sqrt();
                    if dist < best_dist {
                        best_dist = dist;
                        best_cluster = j;
                    }
                }
                best_cluster == i
            })
            .collect();

        if !members.is_empty() {
            // Find common tags in cluster
            let mut tag_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for m in &members {
                for tag in &m.tags {
                    *tag_counts.entry(tag.clone()).or_insert(0) += 1;
                }
            }
            let mut common_tags: Vec<_> = tag_counts.into_iter().collect();
            common_tags.sort_by(|a, b| b.1.cmp(&a.1));

            clusters.push(serde_json::json!({
                "id": i,
                "centroid": { "x": centroid.0, "y": centroid.1, "z": centroid.2 },
                "member_count": members.len(),
                "common_tags": common_tags.iter().take(5).map(|(t, c)| {
                    serde_json::json!({ "tag": t, "count": c })
                }).collect::<Vec<_>>(),
                "sample_members": members.iter().take(5).map(|m| {
                    serde_json::json!({
                        "id": m.id,
                        "name": m.name,
                        "type": m.object_type
                    })
                }).collect::<Vec<_>>()
            }));
        }
    }

    Ok(serde_json::json!({
        "num_clusters": clusters.len(),
        "total_points": space.points.len(),
        "variance_captured": space.variance_captured,
        "clusters": clusters
    }))
}

/// Analyze vectors relative to the mean (center of knowledge)
#[derive(Debug, Clone, Serialize)]
pub struct MeanAnalysis {
    /// Objects closest to the mean (most typical)
    pub closest_to_mean: Vec<MeanDistanceResult>,
    /// Objects farthest from the mean (most unique)
    pub farthest_from_mean: Vec<MeanDistanceResult>,
    /// Summary of what the mean represents
    pub mean_summary: String,
    /// Total objects analyzed
    pub total_objects: usize,
    /// Average distance from mean
    pub avg_distance: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeanDistanceResult {
    pub id: String,
    pub name: String,
    pub object_type: String,
    pub distance: f32,
    pub preview: String,
    pub tags: Vec<String>,
}

#[tauri::command]
pub async fn analyze_knowledge_center(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    limit: Option<usize>,
) -> Result<MeanAnalysis, String> {
    use crate::embedding_analysis::{compute_mean, euclidean_distance};

    let limit = limit.unwrap_or(10);

    let store = search.store.read().await;
    let objects_with_embeddings = store
        .get_objects_with_embeddings(crate::memory::SecurityTier::Sealed)
        .map_err(|e| format!("Failed to get objects: {}", e))?;

    if objects_with_embeddings.len() < 3 {
        return Err("Need at least 3 objects for analysis".to_string());
    }

    // Compute mean
    let embeddings: Vec<&[f32]> = objects_with_embeddings
        .iter()
        .map(|(_, emb)| emb.as_slice())
        .collect();

    let mean = compute_mean(&embeddings);

    // Calculate distances from mean
    let mut distances: Vec<(usize, f32)> = objects_with_embeddings
        .iter()
        .enumerate()
        .map(|(i, (_, emb))| {
            let dist = euclidean_distance(emb, &mean);
            (i, dist)
        })
        .collect();

    // Sort by distance
    distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let avg_distance = distances.iter().map(|(_, d)| d).sum::<f32>() / distances.len() as f32;

    // Helper to convert object to result
    let to_result = |idx: usize, dist: f32| -> MeanDistanceResult {
        let (obj, _) = &objects_with_embeddings[idx];
        let object_type = match &obj.content_type {
            crate::semantic_object::ContentType::Text => "text".to_string(),
            crate::semantic_object::ContentType::Markdown => "markdown".to_string(),
            crate::semantic_object::ContentType::Code { language } => format!("code:{}", language),
            crate::semantic_object::ContentType::Json => "json".to_string(),
            crate::semantic_object::ContentType::Binary { mime } => format!("binary:{}", mime),
            crate::semantic_object::ContentType::Structured { schema } => format!("structured:{}", schema),
            crate::semantic_object::ContentType::Unknown => "unknown".to_string(),
        };

        MeanDistanceResult {
            id: obj.suid.to_string(),
            name: obj.name.clone().unwrap_or_else(|| "Untitled".to_string()),
            object_type,
            distance: dist,
            preview: obj.summary.clone().unwrap_or_default().chars().take(150).collect(),
            tags: obj.tags.clone(),
        }
    };

    // Get closest to mean
    let closest_to_mean: Vec<MeanDistanceResult> = distances
        .iter()
        .take(limit)
        .map(|(idx, dist)| to_result(*idx, *dist))
        .collect();

    // Get farthest from mean
    let farthest_from_mean: Vec<MeanDistanceResult> = distances
        .iter()
        .rev()
        .take(limit)
        .map(|(idx, dist)| to_result(*idx, *dist))
        .collect();

    // Generate summary of what the mean represents
    let mean_summary = if !closest_to_mean.is_empty() {
        let types: Vec<&str> = closest_to_mean.iter()
            .map(|r| r.object_type.as_str())
            .collect();
        let common_type = types.iter()
            .fold(std::collections::HashMap::new(), |mut acc, t| {
                *acc.entry(*t).or_insert(0) += 1;
                acc
            })
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(t, _)| t)
            .unwrap_or("mixed");

        format!(
            "Your knowledge center is dominated by {} content. Objects near the center represent your most common/typical work patterns.",
            common_type
        )
    } else {
        "Unable to determine center characteristics.".to_string()
    };

    Ok(MeanAnalysis {
        closest_to_mean,
        farthest_from_mean,
        mean_summary,
        total_objects: objects_with_embeddings.len(),
        avg_distance,
    })
}

/// Export idea space for VR visualization (optimized format)
#[tauri::command]
pub async fn export_idea_space_vr(
    app: tauri::AppHandle,
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    pca_cache: tauri::State<'_, crate::pca_cache::PCACacheManager>,
    format: Option<String>,  // "json" or "gltf_positions"
) -> Result<serde_json::Value, String> {
    let space = get_idea_space_3d(app, search, pca_cache, None).await?;
    let format = format.unwrap_or_else(|| "json".to_string());

    match format.as_str() {
        "gltf_positions" => {
            // Export just positions for easy 3D rendering
            let positions: Vec<f32> = space.points.iter()
                .flat_map(|p| vec![p.x, p.y, p.z])
                .collect();

            let colors: Vec<f32> = space.points.iter()
                .flat_map(|p| {
                    // Color by type
                    match p.object_type.as_str() {
                        "conversation" => vec![0.2, 0.6, 1.0],  // Blue
                        "note" => vec![0.2, 1.0, 0.4],          // Green
                        "research" => vec![1.0, 0.8, 0.2],      // Yellow
                        "code" => vec![1.0, 0.4, 0.8],          // Pink
                        _ => vec![0.7, 0.7, 0.7],               // Gray
                    }
                })
                .collect();

            Ok(serde_json::json!({
                "format": "gltf_positions",
                "point_count": space.points.len(),
                "positions": positions,
                "colors": colors,
                "metadata": space.points.iter().map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "name": p.name,
                        "type": p.object_type
                    })
                }).collect::<Vec<_>>()
            }))
        }
        _ => {
            // Full JSON export
            Ok(serde_json::json!({
                "format": "json",
                "space": space
            }))
        }
    }
}

// ============================================================================
// Repository Tracker Commands
// ============================================================================

use crate::repo_tracker::{RepoTracker, TrackedRepo, RepoStatus, AutoSyncMode};

/// Add a repository to be tracked
#[tauri::command]
pub fn repo_add(
    tracker: tauri::State<'_, RepoTracker>,
    path: String,
    name: Option<String>,
) -> Result<TrackedRepo, String> {
    tracker.add_repo(&path, name)
}

/// Remove a repository from tracking
#[tauri::command]
pub fn repo_remove(
    tracker: tauri::State<'_, RepoTracker>,
    id: String,
) -> Result<(), String> {
    tracker.remove_repo(&id)
}

/// List all tracked repositories
#[tauri::command]
pub fn repo_list(
    tracker: tauri::State<'_, RepoTracker>,
) -> Vec<TrackedRepo> {
    tracker.list_repos()
}

/// Get status of a specific repository (check for changes)
#[tauri::command]
pub fn repo_get_status(
    tracker: tauri::State<'_, RepoTracker>,
    id: String,
) -> Result<RepoStatus, String> {
    tracker.get_repo_status(&id)
}

/// Check if a path is already tracked
#[tauri::command]
pub fn repo_check_path(
    tracker: tauri::State<'_, RepoTracker>,
    path: String,
) -> Option<TrackedRepo> {
    tracker.is_tracked(&path)
}

/// Set auto-sync mode for a repository
#[tauri::command]
pub fn repo_set_auto_sync(
    tracker: tauri::State<'_, RepoTracker>,
    id: String,
    mode: String,
    interval_hours: Option<u32>,
) -> Result<(), String> {
    let auto_sync = match mode.as_str() {
        "manual" => AutoSyncMode::Manual,
        "on_startup" => AutoSyncMode::OnStartup,
        "on_change" => AutoSyncMode::OnChange,
        "scheduled" => AutoSyncMode::Scheduled {
            interval_hours: interval_hours.unwrap_or(24),
        },
        _ => return Err(format!("Unknown auto-sync mode: {}", mode)),
    };

    tracker.set_auto_sync(&id, auto_sync)
}

/// Sync a repository (re-import all files)
#[tauri::command]
pub async fn repo_sync(
    app: tauri::AppHandle,
    tracker: tauri::State<'_, RepoTracker>,
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    pca_cache: tauri::State<'_, crate::pca_cache::PCACacheManager>,
    id: String,
) -> Result<String, String> {
    let repo = tracker.get_repo(&id)
        .ok_or_else(|| format!("Repository not found: {}", id))?;

    // Use the existing import_repository logic
    let result = import_repository(
        app.clone(),
        repo.path.clone(),
        search.clone(),
        pca_cache.clone(),
    ).await?;

    // Parse the result to extract counts
    // Result format: "Imported X files, created Y objects"
    let (file_count, object_count) = parse_import_result(&result);

    // Update tracker
    tracker.update_after_sync(&id, file_count, object_count)?;

    Ok(result)
}

fn parse_import_result(result: &str) -> (usize, usize) {
    // Parse "Imported X files, created Y objects"
    let mut file_count = 0;
    let mut object_count = 0;

    if let Some(files_start) = result.find("Imported ") {
        let rest = &result[files_start + 9..];
        if let Some(files_end) = rest.find(" files") {
            if let Ok(n) = rest[..files_end].parse::<usize>() {
                file_count = n;
            }
        }
    }

    if let Some(objects_start) = result.find("created ") {
        let rest = &result[objects_start + 8..];
        if let Some(objects_end) = rest.find(" objects") {
            if let Ok(n) = rest[..objects_end].parse::<usize>() {
                object_count = n;
            }
        }
    }

    (file_count, object_count)
}

// ============================================================================
// SEMANTIC CALCULATOR - Vector Algebra Operations
// ============================================================================

/// Result of a vector operation
#[derive(Debug, Clone, serde::Serialize)]
pub struct VectorOperationResult {
    pub operation: String,
    pub result_vector: Vec<f32>,
    pub result_3d: (f32, f32, f32),
    pub nearest_neighbors: Vec<VectorNeighbor>,
    pub stats: VectorStats,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VectorNeighbor {
    pub suid: String,
    pub name: String,
    pub similarity: f32,
    pub distance_3d: f32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VectorStats {
    pub magnitude: f32,
    pub dimensions: usize,
    pub sparsity: f32,  // Percentage of near-zero values
    pub binary_hash: String,  // First 32 bits as hex
}

/// Perform vector algebra operations
#[tauri::command]
pub async fn vector_calculate(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    pca_cache: tauri::State<'_, crate::pca_cache::PCACacheManager>,
    operation: String,
    vector_suids: Vec<String>,
    interpolation_t: Option<f32>,  // For lerp operation
) -> Result<VectorOperationResult, String> {
    use crate::pca_cache::CacheStatus;

    // Get embeddings for the specified SUIds
    let store = search.store.read().await;
    let mut vectors: Vec<(String, String, Vec<f32>)> = Vec::new();

    for suid_str in &vector_suids {
        let suid = crate::semantic_object::Suid::parse(suid_str)
            .map_err(|e| format!("Invalid SUID {}: {}", suid_str, e))?;

        if let Ok(Some(obj)) = store.get(&suid) {
            if let Ok(Some(emb)) = store.get_embedding(&suid) {
                let name = obj.name.clone().unwrap_or_else(|| "Untitled".to_string());
                vectors.push((suid_str.clone(), name, emb));
            } else {
                return Err(format!("No embedding for: {}", suid_str));
            }
        } else {
            return Err(format!("Object not found: {}", suid_str));
        }
    }

    if vectors.is_empty() {
        return Err("No vectors provided".to_string());
    }

    let dim = vectors[0].2.len();

    // Perform the operation
    let (result, op_desc) = match operation.as_str() {
        "add" => {
            if vectors.len() < 2 {
                return Err("Add requires at least 2 vectors".to_string());
            }
            let mut sum = vec![0.0f32; dim];
            for (_, _, v) in &vectors {
                for (i, val) in v.iter().enumerate() {
                    sum[i] += val;
                }
            }
            (sum, format!("add({} vectors)", vectors.len()))
        }
        "subtract" => {
            if vectors.len() != 2 {
                return Err("Subtract requires exactly 2 vectors".to_string());
            }
            let result: Vec<f32> = vectors[0].2.iter()
                .zip(vectors[1].2.iter())
                .map(|(a, b)| a - b)
                .collect();
            (result, format!("{} - {}", vectors[0].1, vectors[1].1))
        }
        "average" => {
            let mut sum = vec![0.0f32; dim];
            for (_, _, v) in &vectors {
                for (i, val) in v.iter().enumerate() {
                    sum[i] += val;
                }
            }
            let n = vectors.len() as f32;
            let avg: Vec<f32> = sum.iter().map(|v| v / n).collect();
            (avg, format!("average({} vectors)", vectors.len()))
        }
        "hadamard" => {
            if vectors.len() != 2 {
                return Err("Hadamard requires exactly 2 vectors".to_string());
            }
            let result: Vec<f32> = vectors[0].2.iter()
                .zip(vectors[1].2.iter())
                .map(|(a, b)| a * b)
                .collect();
            (result, format!("{} ⊙ {}", vectors[0].1, vectors[1].1))
        }
        "interpolate" | "lerp" => {
            if vectors.len() != 2 {
                return Err("Interpolate requires exactly 2 vectors".to_string());
            }
            let t = interpolation_t.unwrap_or(0.5);
            let result: Vec<f32> = vectors[0].2.iter()
                .zip(vectors[1].2.iter())
                .map(|(a, b)| a + t * (b - a))
                .collect();
            (result, format!("lerp({}, {}, t={})", vectors[0].1, vectors[1].1, t))
        }
        "difference_apply" => {
            // A - B + C: apply the transformation from B to A onto C
            if vectors.len() != 3 {
                return Err("difference_apply requires exactly 3 vectors (A, B, C) → A - B + C".to_string());
            }
            let result: Vec<f32> = vectors[0].2.iter()
                .zip(vectors[1].2.iter())
                .zip(vectors[2].2.iter())
                .map(|((a, b), c)| a - b + c)
                .collect();
            (result, format!("({} - {}) + {}", vectors[0].1, vectors[1].1, vectors[2].1))
        }
        "negate" => {
            if vectors.len() != 1 {
                return Err("Negate requires exactly 1 vector".to_string());
            }
            let result: Vec<f32> = vectors[0].2.iter().map(|v| -v).collect();
            (result, format!("-{}", vectors[0].1))
        }
        "normalize" => {
            if vectors.len() != 1 {
                return Err("Normalize requires exactly 1 vector".to_string());
            }
            let mag: f32 = vectors[0].2.iter().map(|v| v * v).sum::<f32>().sqrt();
            let result: Vec<f32> = if mag > 0.0 {
                vectors[0].2.iter().map(|v| v / mag).collect()
            } else {
                vectors[0].2.clone()
            };
            (result, format!("normalize({})", vectors[0].1))
        }
        _ => return Err(format!("Unknown operation: {}", operation)),
    };

    // Compute stats
    let magnitude: f32 = result.iter().map(|v| v * v).sum::<f32>().sqrt();
    let near_zero_count = result.iter().filter(|v| v.abs() < 0.01).count();
    let sparsity = near_zero_count as f32 / dim as f32;

    // Create binary hash (first 32 dims as bits based on sign)
    let binary_hash: String = {
        let bits: u32 = result.iter()
            .take(32)
            .enumerate()
            .fold(0u32, |acc, (i, v)| {
                if *v > 0.0 { acc | (1 << i) } else { acc }
            });
        format!("{:08x}", bits)
    };

    let stats = VectorStats {
        magnitude,
        dimensions: dim,
        sparsity,
        binary_hash,
    };

    // Project to 3D using cached PCA
    let objects_with_embeddings = store
        .get_objects_with_embeddings(crate::memory::SecurityTier::Sealed)
        .map_err(|e| format!("Failed to get objects: {}", e))?;

    let total_count = objects_with_embeddings.len();
    let cache_status = pca_cache.check(total_count);

    let pca = match cache_status {
        CacheStatus::Valid(p) => Some(p),
        CacheStatus::UsableWithNewItems { pca, .. } => Some(pca),
        CacheStatus::Stale { .. } | CacheStatus::None => {
            // Need to compute PCA - collect embeddings
            let embeddings: Vec<&[f32]> = objects_with_embeddings.iter()
                .map(|(_, e)| e.as_slice())
                .collect();
            let new_pca = simple_pca(&embeddings, 3);
            if let Some(ref pca) = new_pca {
                pca_cache.set(pca.clone(), total_count, dim);
            }
            new_pca
        }
    };

    let pca = match pca {
        Some(p) => p,
        None => return Err("No objects to compute PCA".to_string()),
    };

    let result_3d = project_to_3d(&result, &pca);

    // Find nearest neighbors
    let mut neighbors: Vec<(String, String, f32, f32, f32, f32)> = Vec::new();

    for (obj, emb) in &objects_with_embeddings {
        let sim = cosine_similarity(&result, emb);
        let (ox, oy, oz) = project_to_3d(emb, &pca);
        let dist_3d = ((result_3d.0 - ox).powi(2) +
                       (result_3d.1 - oy).powi(2) +
                       (result_3d.2 - oz).powi(2)).sqrt();
        let name = obj.name.clone().unwrap_or_else(|| "Untitled".to_string());
        neighbors.push((obj.suid.to_string(), name, sim, dist_3d, ox, oy));
    }

    // Sort by similarity (descending)
    neighbors.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let nearest_neighbors: Vec<VectorNeighbor> = neighbors.into_iter()
        .take(10)
        .map(|(suid, name, sim, dist, _, _)| VectorNeighbor {
            suid,
            name,
            similarity: sim,
            distance_3d: dist,
        })
        .collect();

    Ok(VectorOperationResult {
        operation: op_desc,
        result_vector: result,
        result_3d,
        nearest_neighbors,
        stats,
    })
}

/// Get vector info for a single object (for display in UI)
#[tauri::command]
pub async fn vector_info(
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
    suid: String,
) -> Result<VectorStats, String> {
    let store = search.store.read().await;
    let parsed_suid = crate::semantic_object::Suid::parse(&suid)
        .map_err(|e| format!("Invalid SUID: {}", e))?;

    let emb = store.get_embedding(&parsed_suid)
        .map_err(|e| format!("Failed to get embedding: {}", e))?
        .ok_or_else(|| "No embedding for this object".to_string())?;

    let dim = emb.len();
    let magnitude: f32 = emb.iter().map(|v| v * v).sum::<f32>().sqrt();
    let near_zero_count = emb.iter().filter(|v| v.abs() < 0.01).count();
    let sparsity = near_zero_count as f32 / dim as f32;

    let binary_hash: String = {
        let bits: u32 = emb.iter()
            .take(32)
            .enumerate()
            .fold(0u32, |acc, (i, v)| {
                if *v > 0.0 { acc | (1 << i) } else { acc }
            });
        format!("{:08x}", bits)
    };

    Ok(VectorStats {
        magnitude,
        dimensions: dim,
        sparsity,
        binary_hash,
    })
}

// ============================================================================
// File operation commands for chat slash commands
// ============================================================================

/// Read file content
#[tauri::command]
pub async fn read_file_content(path: String) -> Result<String, String> {
    let path = std::path::Path::new(&path);

    // Security: Only allow reading from user-accessible locations
    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    // Read the file
    std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))
}

/// List directory contents
#[tauri::command]
pub async fn list_directory(path: String) -> Result<Vec<String>, String> {
    let path = if path == "." || path.is_empty() {
        std::env::current_dir().map_err(|e| format!("Failed to get current dir: {}", e))?
    } else {
        std::path::PathBuf::from(&path)
    };

    if !path.exists() {
        return Err(format!("Directory not found: {}", path.display()));
    }

    if !path.is_dir() {
        return Err(format!("Not a directory: {}", path.display()));
    }

    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&path).map_err(|e| format!("Failed to read directory: {}", e))? {
        if let Ok(entry) = entry {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let file_type = if entry.path().is_dir() { "/" } else { "" };
            entries.push(format!("{}{}", file_name, file_type));
        }
    }

    entries.sort();
    Ok(entries)
}

/// Get the embedding vector for an object
#[tauri::command]
pub async fn get_object_embedding(
    suid: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Option<Vec<f32>>, String> {
    let parsed_suid = Suid::parse(&suid).map_err(|e| format!("Invalid SUID: {}", e))?;

    let store = search.store.read().await;
    let embedding = store.get_embedding(&parsed_suid)
        .map_err(|e| format!("Failed to get embedding: {}", e))?;

    Ok(embedding)
}

/// Ingest file to semantic memory
#[tauri::command]
pub async fn ingest_file_to_memory(
    path: String,
    search: tauri::State<'_, std::sync::Arc<crate::semantic_search::SemanticSearch>>,
) -> Result<(), String> {
    let path_buf = std::path::PathBuf::from(&path);

    if !path_buf.exists() {
        return Err(format!("File not found: {}", path));
    }

    // Read file content
    let content = std::fs::read_to_string(&path_buf)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let file_name = path_buf.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Determine content type from extension
    let content_type = ContentType::from_extension(
        path_buf.extension().and_then(|e| e.to_str()).unwrap_or("")
    );

    // Create semantic object from file
    let mut obj = SemanticObject::new(content.as_bytes().to_vec(), content_type);
    obj.name = Some(file_name.clone());
    obj.path = Some(path.clone());
    obj.summary = Some(content.chars().take(500).collect::<String>());

    // Store in search index
    search.store(&obj).await
        .map_err(|e| format!("Failed to index file: {}", e))?;

    log::info!("Ingested file to memory: {}", file_name);
    Ok(())
}

/// Write content to file (for chat /write command)
#[tauri::command]
pub async fn write_file_content(
    path: String,
    contents: String,
) -> Result<String, String> {
    let path = std::path::Path::new(&path);

    // Create parent directories if they don't exist
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directory: {}", e))?;
    }

    // Calculate length before moving contents
    let len = contents.len();

    // Write to file
    std::fs::write(&path, contents)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(format!("Wrote {} bytes to {}", len, path.display()))
}

// ============================================================================
// Plan Space Commands - Living Canvas for Goals & Ideas
// ============================================================================

/// Goal object for Plan Space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanGoal {
    pub suid: Option<String>,
    pub title: String,
    pub description: String,
    #[serde(rename = "energyRequired")]
    pub energy_required: String,  // "low" | "medium" | "high" | "flow"
    #[serde(rename = "meaningScore")]
    pub meaning_score: i32,
    pub excitement: i32,
    #[serde(rename = "progressType")]
    pub progress_type: String,
    pub progress: i32,
    #[serde(default)]
    pub milestones: Vec<PlanMilestone>,
    pub position: PlanPosition,
    pub color: Option<String>,
    #[serde(rename = "relatedGoals")]
    pub related_goals: Vec<String>,
    #[serde(rename = "blockedBy")]
    pub blocked_by: Option<Vec<String>>,
    pub tags: Vec<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(rename = "lastTouched")]
    pub last_touched: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSubtask {
    pub id: String,
    pub title: String,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanMilestonePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanMilestone {
    pub id: String,
    pub title: String,
    pub completed: bool,
    pub notes: Option<String>,
    #[serde(default)]
    pub subtasks: Vec<PlanSubtask>,
    // Agent & complexity fields
    #[serde(rename = "agentType")]
    pub agent_type: Option<String>,
    pub complexity: Option<String>,
    pub branch: Option<String>,
    #[serde(rename = "dependsOn")]
    pub depends_on: Option<Vec<String>>,
    // Position (when dragged)
    pub position: Option<PlanMilestonePosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalPositionUpdate {
    pub suid: String,
    pub x: f64,
    pub y: f64,
}

/// Create a new goal in Plan Space
#[tauri::command]
pub async fn plan_create_goal(
    goal: PlanGoal,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<String, String> {
    // Create a unique identifier
    let suid = Suid::new();
    let suid_str = suid.to_string();

    // Build metadata for the goal
    let mut metadata = HashMap::new();
    metadata.insert("type".to_string(), serde_json::Value::String("plan_goal".to_string()));
    metadata.insert("energy_required".to_string(), serde_json::Value::String(goal.energy_required.clone()));
    metadata.insert("meaning_score".to_string(), serde_json::Value::Number(goal.meaning_score.into()));
    metadata.insert("excitement".to_string(), serde_json::Value::Number(goal.excitement.into()));
    metadata.insert("progress_type".to_string(), serde_json::Value::String(goal.progress_type.clone()));
    metadata.insert("progress".to_string(), serde_json::Value::Number(goal.progress.into()));
    metadata.insert("position_x".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(goal.position.x).unwrap_or(serde_json::Number::from(0))));
    metadata.insert("position_y".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(goal.position.y).unwrap_or(serde_json::Number::from(0))));

    // Store milestones
    let milestones_value = serde_json::to_value(&goal.milestones).unwrap_or(serde_json::Value::Array(vec![]));
    metadata.insert("milestones".to_string(), milestones_value);

    if let Some(color) = &goal.color {
        metadata.insert("color".to_string(), serde_json::Value::String(color.clone()));
    }
    if !goal.related_goals.is_empty() {
        metadata.insert("related_goals".to_string(), serde_json::to_value(&goal.related_goals).unwrap_or(serde_json::Value::Array(vec![])));
    }
    if let Some(blocked_by) = &goal.blocked_by {
        metadata.insert("blocked_by".to_string(), serde_json::to_value(blocked_by).unwrap_or(serde_json::Value::Array(vec![])));
    }

    // Create the semantic object
    let content = format!("{}\n\n{}", goal.title, goal.description);
    let mut obj = SemanticObject::new(content.as_bytes().to_vec(), ContentType::Text);
    obj.suid = suid;
    obj.name = Some(goal.title.clone());
    obj.summary = Some(goal.description.clone());
    obj.metadata = metadata;

    // Add tags
    obj.tags.push("kind:plan_goal".to_string());
    obj.tags.push("planspace".to_string());
    for tag in &goal.tags {
        obj.tags.push(format!("user_tag:{}", tag));
    }

    // Store the object
    search.store(&obj).await
        .map_err(|e| format!("Failed to create goal: {}", e))?;

    log::info!("Created plan goal: {} ({})", goal.title, suid_str);
    Ok(suid_str)
}

/// Update an existing goal
#[tauri::command]
pub async fn plan_update_goal(
    suid: String,
    updates: PlanGoal,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<(), String> {
    let parsed_suid = Suid::parse(&suid)
        .map_err(|e| format!("Invalid SUID: {}", e))?;

    // Get existing object
    let store = search.store.read().await;
    let existing = store.get(&parsed_suid)
        .map_err(|e| format!("Failed to get goal: {}", e))?
        .ok_or_else(|| "Goal not found".to_string())?;
    drop(store);

    // Build updated metadata
    let mut metadata = existing.metadata.clone();
    metadata.insert("energy_required".to_string(), serde_json::Value::String(updates.energy_required.clone()));
    metadata.insert("meaning_score".to_string(), serde_json::Value::Number(updates.meaning_score.into()));
    metadata.insert("excitement".to_string(), serde_json::Value::Number(updates.excitement.into()));
    metadata.insert("progress".to_string(), serde_json::Value::Number(updates.progress.into()));
    metadata.insert("position_x".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(updates.position.x).unwrap_or(serde_json::Number::from(0))));
    metadata.insert("position_y".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(updates.position.y).unwrap_or(serde_json::Number::from(0))));

    // Store milestones
    log::info!("plan_update_goal received {} milestones: {:?}", updates.milestones.len(), updates.milestones);
    let milestones_value = serde_json::to_value(&updates.milestones).unwrap_or(serde_json::Value::Array(vec![]));
    log::info!("Storing milestones value: {}", milestones_value);
    metadata.insert("milestones".to_string(), milestones_value);

    if let Some(color) = &updates.color {
        metadata.insert("color".to_string(), serde_json::Value::String(color.clone()));
    }
    if !updates.related_goals.is_empty() {
        metadata.insert("related_goals".to_string(), serde_json::to_value(&updates.related_goals).unwrap_or(serde_json::Value::Array(vec![])));
    }
    if let Some(blocked_by) = &updates.blocked_by {
        metadata.insert("blocked_by".to_string(), serde_json::to_value(blocked_by).unwrap_or(serde_json::Value::Array(vec![])));
    }

    // Create updated object
    let content = format!("{}\n\n{}", updates.title, updates.description);
    let mut obj = SemanticObject::new(content.as_bytes().to_vec(), ContentType::Text);
    obj.suid = parsed_suid;
    obj.name = Some(updates.title.clone());
    obj.summary = Some(updates.description.clone());
    obj.metadata = metadata;
    obj.created_at = existing.created_at;
    obj.modified_at = Utc::now();

    // Rebuild tags
    obj.tags.push("kind:plan_goal".to_string());
    obj.tags.push("planspace".to_string());
    for tag in &updates.tags {
        obj.tags.push(format!("user_tag:{}", tag));
    }

    // Update in store (use update, not store, to avoid UNIQUE constraint error)
    search.update(&obj).await
        .map_err(|e| format!("Failed to update goal: {}", e))?;

    log::info!("Updated plan goal: {}", suid);
    Ok(())
}

/// Delete a goal
#[tauri::command]
pub async fn plan_delete_goal(
    suid: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<(), String> {
    let parsed_suid = Suid::parse(&suid)
        .map_err(|e| format!("Invalid SUID: {}", e))?;

    let store = search.store.write().await;
    store.delete(&parsed_suid)
        .map_err(|e| format!("Failed to delete goal: {}", e))?;

    log::info!("Deleted plan goal: {}", suid);
    Ok(())
}

/// List all goals in Plan Space
#[tauri::command]
pub async fn plan_list_goals(
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<PlanGoal>, String> {
    let store = search.store.read().await;
    // Use list_by_tag to find all plan goals
    let objects = store.list_by_tag("kind:plan_goal", 1000)
        .map_err(|e| format!("Failed to list goals: {}", e))?;

    let goals: Vec<PlanGoal> = objects
        .into_iter()
        .map(|obj| {
            let metadata = &obj.metadata;

            // Extract user tags
            let tags: Vec<String> = obj.tags.iter()
                .filter_map(|t: &String| t.strip_prefix("user_tag:").map(|s| s.to_string()))
                .collect();

            // Parse milestones from metadata
            let milestones: Vec<PlanMilestone> = metadata.get("milestones")
                .and_then(|v: &serde_json::Value| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            // Parse related goals
            let related_goals: Vec<String> = metadata.get("related_goals")
                .and_then(|v: &serde_json::Value| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            // Parse blocked_by
            let blocked_by: Option<Vec<String>> = metadata.get("blocked_by")
                .and_then(|v: &serde_json::Value| serde_json::from_value(v.clone()).ok());

            PlanGoal {
                suid: Some(obj.suid.to_string()),
                title: obj.name.clone().unwrap_or_else(|| "Untitled".to_string()),
                description: obj.summary.clone().unwrap_or_default(),
                energy_required: metadata.get("energy_required")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("medium")
                    .to_string(),
                meaning_score: metadata.get("meaning_score")
                    .and_then(|v: &serde_json::Value| v.as_i64())
                    .unwrap_or(5) as i32,
                excitement: metadata.get("excitement")
                    .and_then(|v: &serde_json::Value| v.as_i64())
                    .unwrap_or(5) as i32,
                progress_type: metadata.get("progress_type")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("milestones")
                    .to_string(),
                progress: metadata.get("progress")
                    .and_then(|v: &serde_json::Value| v.as_i64())
                    .unwrap_or(0) as i32,
                milestones,
                position: PlanPosition {
                    x: metadata.get("position_x")
                        .and_then(|v: &serde_json::Value| v.as_f64())
                        .unwrap_or(100.0),
                    y: metadata.get("position_y")
                        .and_then(|v: &serde_json::Value| v.as_f64())
                        .unwrap_or(100.0),
                },
                color: metadata.get("color")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .map(|s: &str| s.to_string()),
                related_goals,
                blocked_by,
                tags,
                created_at: Some(obj.created_at),
                last_touched: Some(obj.modified_at),
            }
        })
        .collect();

    log::debug!("Listed {} plan goals", goals.len());
    Ok(goals)
}

/// Save canvas positions for all goals
#[tauri::command]
pub async fn plan_save_canvas(
    positions: Vec<GoalPositionUpdate>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<(), String> {
    for pos in positions {
        let parsed_suid = match Suid::parse(&pos.suid) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Get existing object
        let store = search.store.read().await;
        let existing = match store.get(&parsed_suid) {
            Ok(Some(obj)) => obj,
            _ => continue,
        };
        drop(store);

        // Update position in metadata
        let mut obj = existing.clone();
        obj.metadata.insert("position_x".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(pos.x).unwrap_or(serde_json::Number::from(0))));
        obj.metadata.insert("position_y".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(pos.y).unwrap_or(serde_json::Number::from(0))));
        obj.modified_at = Utc::now();

        // Save without regenerating embedding
        let store = search.store.write().await;
        store.update(&obj)
            .map_err(|e| format!("Failed to save position: {}", e))?;
    }

    log::debug!("Saved canvas positions");
    Ok(())
}

/// AI-generated milestone suggestions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneSuggestion {
    pub title: String,
    pub description: Option<String>,
}

/// Generate milestone suggestions for a goal using AI
#[tauri::command]
pub async fn plan_generate_milestones(
    goal_title: String,
    goal_description: String,
    energy_level: String,
    ai_manager: State<'_, Arc<AiManager>>,
) -> Result<Vec<MilestoneSuggestion>, String> {
    let system_prompt = r#"You are a helpful goal-planning assistant. When given a goal, break it down into 3-7 clear, actionable milestones. Each milestone should be:
- Specific and measurable
- Achievable as a single focused task
- Ordered logically (earlier steps before later ones)

Respond with a JSON array of milestones. Each milestone has a "title" (required, short action phrase) and "description" (optional, brief clarification).

Example response:
[
  {"title": "Research available options", "description": "Spend 30 mins exploring top 3 alternatives"},
  {"title": "Create initial outline", "description": null},
  {"title": "Draft first version"}
]

Only respond with the JSON array, no other text."#;

    let prompt = format!(
        "Break down this goal into milestones:\n\nGoal: {}\n\nDescription: {}\n\nEnergy level required: {} (consider this when sizing milestones - low energy goals should have simpler milestones)",
        goal_title,
        if goal_description.is_empty() { "No additional details provided" } else { &goal_description },
        energy_level
    );

    let response = ai_manager.generate(&prompt, Some(system_prompt)).await
        .map_err(|e| format!("AI generation failed: {}", e))?;

    // Parse the response as JSON
    let content = response.content.trim();

    // Try to extract JSON array from response (handle markdown code blocks)
    let json_str = if content.starts_with("```") {
        content
            .lines()
            .skip(1)
            .take_while(|line| !line.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        content.to_string()
    };

    let milestones: Vec<MilestoneSuggestion> = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse AI response as milestones: {}. Response was: {}", e, content))?;

    log::info!("Generated {} milestones for goal: {}", milestones.len(), goal_title);
    Ok(milestones)
}

// ============================================================================
// Widget System for Plan Space
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasWidget {
    pub id: String,
    #[serde(rename = "widgetType")]
    pub widget_type: String,
    pub title: String,
    pub position: PlanPosition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<WidgetSize>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(rename = "lastTouched")]
    pub last_touched: Option<DateTime<Utc>>,
    // Widget-specific data stored as JSON value
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetPositionUpdate {
    pub id: String,
    pub x: f64,
    pub y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// Create a new widget in Plan Space
#[tauri::command]
pub async fn plan_create_widget(
    widget: CanvasWidget,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<String, String> {
    // Create a unique identifier if not provided
    let suid = if widget.id.is_empty() {
        Suid::new()
    } else {
        Suid::parse(&widget.id).map_err(|e| format!("Invalid widget ID: {}", e))?
    };
    let suid_str = suid.to_string();

    // Build metadata for the widget
    let mut metadata = HashMap::new();
    metadata.insert("type".to_string(), serde_json::Value::String("plan_widget".to_string()));
    metadata.insert("widget_type".to_string(), serde_json::Value::String(widget.widget_type.clone()));
    metadata.insert("position_x".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(widget.position.x).unwrap_or(serde_json::Number::from(0))));
    metadata.insert("position_y".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(widget.position.y).unwrap_or(serde_json::Number::from(0))));

    // Store size if provided
    if let Some(size) = &widget.size {
        metadata.insert("width".to_string(), serde_json::Value::Number(size.width.into()));
        metadata.insert("height".to_string(), serde_json::Value::Number(size.height.into()));
    }

    // Store widget-specific data
    metadata.insert("widget_data".to_string(), widget.data);

    // Create the semantic object
    let content = format!("{} widget: {}", widget.widget_type, widget.title);
    let mut obj = SemanticObject::new(content.as_bytes().to_vec(), ContentType::Text);
    obj.suid = suid;
    obj.name = Some(widget.title.clone());
    obj.summary = Some(format!("{} widget", widget.widget_type));
    obj.metadata = metadata;

    // Add tags
    obj.tags.push("kind:plan_widget".to_string());
    obj.tags.push("planspace".to_string());
    obj.tags.push(format!("widget_type:{}", widget.widget_type));

    // Store the object
    search.store(&obj).await
        .map_err(|e| format!("Failed to create widget: {}", e))?;

    log::info!("Created plan widget: {} ({})", widget.title, suid_str);
    Ok(suid_str)
}

/// List all widgets in Plan Space
#[tauri::command]
pub async fn plan_list_widgets(
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<CanvasWidget>, String> {
    let store = search.store.read().await;

    // Use list_by_tag to find all plan widgets
    let objects = store.list_by_tag("kind:plan_widget", 1000)
        .map_err(|e| format!("Failed to query widgets: {}", e))?;

    let mut widgets = Vec::new();
    for obj in objects {
        // Extract widget type from metadata or tags
        let widget_type = obj.metadata.get("widget_type")
            .and_then(|v| v.as_str())
            .unwrap_or("note")
            .to_string();

        let position_x = obj.metadata.get("position_x")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let position_y = obj.metadata.get("position_y")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let size = if let (Some(width), Some(height)) = (
            obj.metadata.get("width").and_then(|v| v.as_u64()),
            obj.metadata.get("height").and_then(|v| v.as_u64())
        ) {
            Some(WidgetSize {
                width: width as u32,
                height: height as u32,
            })
        } else {
            None
        };

        let widget_data = obj.metadata.get("widget_data")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        widgets.push(CanvasWidget {
            id: obj.suid.to_string(),
            widget_type,
            title: obj.name.unwrap_or_else(|| "Untitled Widget".to_string()),
            position: PlanPosition { x: position_x, y: position_y },
            size,
            created_at: Some(obj.created_at),
            last_touched: Some(obj.modified_at),
            data: widget_data,
        });
    }

    log::debug!("Listed {} widgets", widgets.len());
    Ok(widgets)
}

/// Update an existing widget
#[tauri::command]
pub async fn plan_update_widget(
    id: String,
    updates: CanvasWidget,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<(), String> {
    let parsed_suid = Suid::parse(&id).map_err(|e| format!("Invalid widget ID: {}", e))?;

    // Get existing object
    let store = search.store.read().await;
    let existing = match store.get(&parsed_suid) {
        Ok(Some(obj)) => obj,
        Ok(None) => return Err("Widget not found".to_string()),
        Err(e) => return Err(format!("Failed to get widget: {}", e)),
    };
    drop(store);

    // Update metadata
    let mut obj = existing.clone();
    obj.metadata.insert("widget_type".to_string(), serde_json::Value::String(updates.widget_type.clone()));
    obj.metadata.insert("position_x".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(updates.position.x).unwrap_or(serde_json::Number::from(0))));
    obj.metadata.insert("position_y".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(updates.position.y).unwrap_or(serde_json::Number::from(0))));

    if let Some(size) = &updates.size {
        obj.metadata.insert("width".to_string(), serde_json::Value::Number(size.width.into()));
        obj.metadata.insert("height".to_string(), serde_json::Value::Number(size.height.into()));
    }

    obj.metadata.insert("widget_data".to_string(), updates.data);

    if !updates.title.is_empty() {
        obj.name = Some(updates.title);
    }

    obj.modified_at = Utc::now();

    // Save without regenerating embedding
    let store = search.store.write().await;
    store.update(&obj)
        .map_err(|e| format!("Failed to update widget: {}", e))?;

    log::info!("Updated plan widget: {}", id);
    Ok(())
}

/// Delete a widget
#[tauri::command]
pub async fn plan_delete_widget(
    id: String,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<(), String> {
    let parsed_suid = Suid::parse(&id).map_err(|e| format!("Invalid widget ID: {}", e))?;

    let store = search.store.write().await;
    store.delete(&parsed_suid)
        .map_err(|e| format!("Failed to delete widget: {}", e))?;

    log::info!("Deleted plan widget: {}", id);
    Ok(())
}

/// Save canvas positions for all widgets
#[tauri::command]
pub async fn plan_save_canvas_widgets(
    positions: Vec<WidgetPositionUpdate>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<(), String> {
    for pos in positions {
        let parsed_suid = match Suid::parse(&pos.id) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Get existing object
        let store = search.store.read().await;
        let existing = match store.get(&parsed_suid) {
            Ok(Some(obj)) => obj,
            _ => continue,
        };
        drop(store);

        // Update position in metadata
        let mut obj = existing.clone();
        obj.metadata.insert("position_x".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(pos.x).unwrap_or(serde_json::Number::from(0))));
        obj.metadata.insert("position_y".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(pos.y).unwrap_or(serde_json::Number::from(0))));

        if let Some(width) = pos.width {
            obj.metadata.insert("width".to_string(), serde_json::Value::Number(width.into()));
        }
        if let Some(height) = pos.height {
            obj.metadata.insert("height".to_string(), serde_json::Value::Number(height.into()));
        }

        obj.modified_at = Utc::now();

        // Save without regenerating embedding
        let store = search.store.write().await;
        store.update(&obj)
            .map_err(|e| format!("Failed to save widget position: {}", e))?;
    }

    log::debug!("Saved canvas widget positions");
    Ok(())
}

/// Open a file with the system's default application
#[tauri::command]
pub fn open_file_path(path: String) -> Result<(), String> {
    open::that(path)
        .map_err(|e| format!("Failed to open file: {}", e))
}

/// Open a file picker dialog and return the selected file path
#[tauri::command]
pub async fn pick_file() -> Result<Option<String>, String> {
    use rfd::AsyncFileDialog;

    let file: Option<rfd::FileHandle> = AsyncFileDialog::new()
        .pick_file()
        .await;

    match file {
        Some(f) => Ok(f.path().to_str().map(|s| s.to_string())),
        None => Ok(None),
    }
}

// ========== Document Version History commands ==========

#[tauri::command]
pub fn version_save(
    store: State<'_, VersionStore>,
    document_path: String,
    content: String,
    label: Option<String>,
) -> Result<DocumentVersion, String> {
    store.save_version(&document_path, &content, label.as_deref())
}

#[tauri::command]
pub fn version_list(
    store: State<'_, VersionStore>,
    document_path: String,
) -> Result<Vec<VersionSummary>, String> {
    store.list_versions(&document_path)
}

#[tauri::command]
pub fn version_get(
    store: State<'_, VersionStore>,
    version_id: String,
) -> Result<DocumentVersion, String> {
    store.get_version(&version_id)
}

#[tauri::command]
pub fn version_diff(
    store: State<'_, VersionStore>,
    old_id: String,
    new_id: String,
) -> Result<VersionDiff, String> {
    store.diff_versions(&old_id, &new_id)
}

#[tauri::command]
pub fn version_diff_current(
    store: State<'_, VersionStore>,
    version_id: String,
    current_content: String,
) -> Result<VersionDiff, String> {
    store.diff_with_current(&version_id, &current_content)
}

#[tauri::command]
pub fn version_label(
    store: State<'_, VersionStore>,
    version_id: String,
    label: String,
) -> Result<(), String> {
    store.label_version(&version_id, &label)
}

// ========== Reference Library commands ==========

#[tauri::command]
pub fn ref_add(
    store: State<'_, ReferenceStore>,
    reference: Reference,
) -> Result<(), String> {
    store.add(&reference)
}

#[tauri::command]
pub async fn ref_add_from_doi(
    store: State<'_, ReferenceStore>,
    doi: String,
) -> Result<Reference, String> {
    // Check for existing
    if let Ok(Some(_)) = store.get_by_doi(&doi) {
        return Err("Reference with this DOI already exists".to_string());
    }
    let reference = crate::reference_library::doi_lookup::resolve_doi(&doi).await?;
    store.add(&reference)?;
    Ok(reference)
}

#[tauri::command]
pub async fn ref_add_from_isbn(
    store: State<'_, ReferenceStore>,
    isbn: String,
) -> Result<Reference, String> {
    let reference = crate::reference_library::doi_lookup::resolve_isbn(&isbn).await?;
    store.add(&reference)?;
    Ok(reference)
}

#[tauri::command]
pub fn ref_get(
    store: State<'_, ReferenceStore>,
    id: String,
) -> Result<Reference, String> {
    store.get(&id)
}

#[tauri::command]
pub fn ref_list(
    store: State<'_, ReferenceStore>,
    collection: Option<String>,
    status: Option<String>,
) -> Result<Vec<Reference>, String> {
    store.list(collection.as_deref(), status.as_deref())
}

#[tauri::command]
pub fn ref_search(
    store: State<'_, ReferenceStore>,
    query: String,
) -> Result<Vec<Reference>, String> {
    store.search(&query)
}

#[tauri::command]
pub fn ref_update(
    store: State<'_, ReferenceStore>,
    reference: Reference,
) -> Result<(), String> {
    store.update(&reference)
}

#[tauri::command]
pub fn ref_delete(
    store: State<'_, ReferenceStore>,
    id: String,
) -> Result<(), String> {
    store.delete(&id)
}

#[tauri::command]
pub fn ref_set_reading_status(
    store: State<'_, ReferenceStore>,
    id: String,
    status: String,
) -> Result<(), String> {
    store.set_reading_status(&id, &status)
}

#[tauri::command]
pub fn ref_add_to_collection(
    store: State<'_, ReferenceStore>,
    id: String,
    collection: String,
) -> Result<(), String> {
    store.add_to_collection(&id, &collection)
}

#[tauri::command]
pub fn ref_list_collections(
    store: State<'_, ReferenceStore>,
) -> Result<Vec<String>, String> {
    store.list_collections()
}

#[tauri::command]
pub fn ref_import_bibtex(
    store: State<'_, ReferenceStore>,
    bibtex: String,
) -> Result<Vec<Reference>, String> {
    let references = crate::reference_library::bibtex::parse_bibtex(&bibtex)?;
    for r in &references {
        store.add(r)?;
    }
    Ok(references)
}

#[tauri::command]
pub fn ref_export_bibtex(
    store: State<'_, ReferenceStore>,
    ids: Option<Vec<String>>,
) -> Result<String, String> {
    let references = match ids {
        Some(ids) => {
            let mut refs = Vec::new();
            for id in ids {
                refs.push(store.get(&id)?);
            }
            refs
        }
        None => store.list(None, None)?,
    };
    Ok(crate::reference_library::bibtex::export_bibtex(&references))
}

#[tauri::command]
pub fn ref_check_duplicates(
    store: State<'_, ReferenceStore>,
    reference: Reference,
) -> Result<Vec<DuplicateMatch>, String> {
    let existing = store.list(None, None)?;
    Ok(crate::reference_library::dedup::find_duplicates(&reference, &existing))
}

#[tauri::command]
pub fn ref_attach_pdf(
    store: State<'_, ReferenceStore>,
    id: String,
    pdf_path: String,
) -> Result<(), String> {
    let mut reference = store.get(&id)?;
    reference.pdf_path = Some(pdf_path);
    store.update(&reference)
}

#[tauri::command]
pub fn ref_count(
    store: State<'_, ReferenceStore>,
) -> Result<usize, String> {
    store.count()
}

// ========== CSL Citation Style commands ==========

#[tauri::command]
pub fn csl_list_styles() -> Vec<crate::reference_library::csl::CslStyle> {
    crate::reference_library::csl::available_styles()
}

#[tauri::command]
pub fn csl_render_bibliography(
    store: State<'_, ReferenceStore>,
    style_id: String,
    cite_keys: Vec<String>,
) -> Result<Vec<String>, String> {
    let style = crate::reference_library::csl::get_style(&style_id)
        .ok_or_else(|| format!("Unknown style: {}", style_id))?;

    let mut entries = Vec::new();
    for (i, key) in cite_keys.iter().enumerate() {
        if let Ok(Some(reference)) = store.get_by_cite_key(key) {
            let number = if style.numbered { Some(i + 1) } else { None };
            entries.push(crate::reference_library::csl::render_bibliography(&style, &reference, number));
        }
    }
    Ok(entries)
}

#[tauri::command]
pub fn csl_render_inline(
    store: State<'_, ReferenceStore>,
    style_id: String,
    cite_key: String,
    number: Option<usize>,
    page: Option<String>,
) -> Result<String, String> {
    let style = crate::reference_library::csl::get_style(&style_id)
        .ok_or_else(|| format!("Unknown style: {}", style_id))?;

    let reference = store.get_by_cite_key(&cite_key)?
        .ok_or_else(|| format!("Reference not found: {}", cite_key))?;

    Ok(crate::reference_library::csl::render_inline(&style, &reference, number, page.as_deref()))
}

/// Generate a full bibliography from [[cite_key]] markers in content
#[tauri::command]
pub fn ref_generate_bibliography(
    store: State<'_, ReferenceStore>,
    content: String,
    style_id: String,
) -> Result<String, String> {
    let style = crate::reference_library::csl::get_style(&style_id)
        .ok_or_else(|| format!("Unknown style: {}", style_id))?;

    // Extract all [[cite_key]] markers in order of appearance
    let re = regex::Regex::new(r"\[\[([a-zA-Z][\w-]*)\]\]").unwrap();
    let mut seen = std::collections::HashSet::new();
    let mut ordered_keys = Vec::new();

    for cap in re.captures_iter(&content) {
        let key = cap.get(1).unwrap().as_str().to_string();
        if seen.insert(key.clone()) {
            ordered_keys.push(key);
        }
    }

    if ordered_keys.is_empty() {
        return Ok("No citations found.".to_string());
    }

    let mut bibliography = String::new();
    bibliography.push_str("## References\n\n");

    for (i, key) in ordered_keys.iter().enumerate() {
        if let Ok(Some(reference)) = store.get_by_cite_key(key) {
            let number = if style.numbered { Some(i + 1) } else { None };
            let entry = crate::reference_library::csl::render_bibliography(&style, &reference, number);
            bibliography.push_str(&format!("{}. {}\n\n", i + 1, entry));
        }
    }

    Ok(bibliography)
}

// ============================================================================
// Research Project Commands
// ============================================================================

/// Create a new research project
#[tauri::command]
pub fn project_create(
    name: String,
    description: String,
    store: State<'_, ProjectStore>,
) -> Result<ProjectView, String> {
    let project = store.create(&name, &description)?;
    Ok(ProjectView::from(&project))
}

/// List all projects
#[tauri::command]
pub fn project_list(
    store: State<'_, ProjectStore>,
) -> Result<Vec<ProjectView>, String> {
    store.list()
}

/// Get a project by ID
#[tauri::command]
pub fn project_get(
    project_id: String,
    store: State<'_, ProjectStore>,
) -> Result<crate::research_project::ResearchProject, String> {
    store.get(&project_id)?
        .ok_or_else(|| format!("Project not found: {}", project_id))
}

/// Delete a project
#[tauri::command]
pub fn project_delete(
    project_id: String,
    store: State<'_, ProjectStore>,
) -> Result<(), String> {
    store.delete(&project_id)
}

/// Update project status
#[tauri::command]
pub fn project_set_status(
    project_id: String,
    status: ProjectStatus,
    store: State<'_, ProjectStore>,
) -> Result<(), String> {
    store.set_status(&project_id, status)
}

/// Add a paper to a project
#[tauri::command]
pub fn project_add_paper(
    project_id: String,
    paper_id: String,
    store: State<'_, ProjectStore>,
) -> Result<(), String> {
    store.add_paper(&project_id, &paper_id)
}

/// Add a reference to a project
#[tauri::command]
pub fn project_add_reference(
    project_id: String,
    reference_id: String,
    store: State<'_, ProjectStore>,
) -> Result<(), String> {
    store.add_reference(&project_id, &reference_id)
}

/// Add a document to a project
#[tauri::command]
pub fn project_add_document(
    project_id: String,
    path: String,
    store: State<'_, ProjectStore>,
) -> Result<(), String> {
    store.add_document(&project_id, &path)
}

// ============================================================================
// Paper Template Commands
// ============================================================================

/// List available paper templates
#[tauri::command]
pub fn template_list() -> Vec<crate::paper_generator::templates::PaperTemplate> {
    crate::paper_generator::templates::available_templates()
}

/// Get a specific template by ID
#[tauri::command]
pub fn template_get(template_id: String) -> Result<crate::paper_generator::templates::PaperTemplate, String> {
    crate::paper_generator::templates::get_template(&template_id)
        .ok_or_else(|| format!("Template not found: {}", template_id))
}

/// Resolve cross-references in content
#[tauri::command]
pub fn resolve_cross_refs(content: String) -> String {
    let (resolved, _) = crate::paper_generator::cross_ref::auto_number_content(&content);
    resolved
}

// ============================================================================
// Research Intelligence Commands
// ============================================================================

/// Find related references based on keywords
#[tauri::command]
pub fn ref_find_related(
    keywords: Vec<String>,
    cited_ids: Vec<String>,
    max_results: Option<usize>,
    store: State<'_, ReferenceStore>,
) -> Result<Vec<crate::reference_library::suggestions::RelatedSuggestion>, String> {
    let references = store.list(None, None)?;
    Ok(crate::reference_library::suggestions::find_related_by_keywords(
        &keywords,
        &references,
        &cited_ids,
        max_results.unwrap_or(10),
    ))
}

/// Extract keywords from text for research matching
#[tauri::command]
pub fn ref_extract_keywords(text: String) -> Vec<String> {
    crate::reference_library::suggestions::extract_keywords(&text)
}

/// Perform gap analysis on the reference library
#[tauri::command]
pub fn ref_gap_analysis(
    topic_keywords: Vec<String>,
    store: State<'_, ReferenceStore>,
) -> Result<crate::reference_library::gap_analysis::GapAnalysisResult, String> {
    let references = store.list(None, None)?;
    Ok(crate::reference_library::gap_analysis::analyze_gaps(&references, &topic_keywords))
}

// ============================================================================
// Voice TTS (F5-TTS sidecar) commands
// ============================================================================

use crate::voice_tts::{VoiceTtsManager, VoiceTtsStatus, SynthesizeResult};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

#[tauri::command]
pub async fn voice_tts_status(
    manager: State<'_, Arc<VoiceTtsManager>>,
) -> Result<VoiceTtsStatus, String> {
    Ok(manager.status().await)
}

#[tauri::command]
pub async fn voice_tts_set_reference(
    audio_b64: String,
    transcript: String,
    manager: State<'_, Arc<VoiceTtsManager>>,
) -> Result<(), String> {
    let bytes = BASE64.decode(audio_b64.as_bytes()).map_err(|e| e.to_string())?;
    manager
        .set_reference(&bytes, transcript)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn voice_tts_clear_reference(
    manager: State<'_, Arc<VoiceTtsManager>>,
) -> Result<(), String> {
    manager.clear_reference().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn voice_tts_synthesize(
    text: String,
    speed: Option<f32>,
    nfe: Option<u32>,
    manager: State<'_, Arc<VoiceTtsManager>>,
) -> Result<SynthesizeResult, String> {
    manager
        .synthesize(text, speed, nfe)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn voice_tts_shutdown(
    manager: State<'_, Arc<VoiceTtsManager>>,
) -> Result<(), String> {
    manager.shutdown().await;
    Ok(())
}

// ============================================================================
// Book typesetter (Phase A — pandoc bridge) commands
// ============================================================================

use crate::typesetter::pandoc::{
    PandocConvertOptions, PandocConvertResult, PandocConverter, PandocProbe,
};

#[tauri::command]
pub async fn typesetter_pandoc_probe() -> PandocProbe {
    PandocConverter::probe().await
}

#[tauri::command]
pub async fn typesetter_pandoc_convert_file(
    path: String,
    options: Option<PandocConvertOptions>,
) -> Result<PandocConvertResult, String> {
    let opts = options.unwrap_or_default();
    PandocConverter::convert_file(std::path::Path::new(&path), &opts)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn typesetter_pandoc_convert_str(
    markdown: String,
    options: Option<PandocConvertOptions>,
) -> Result<PandocConvertResult, String> {
    let opts = options.unwrap_or_default();
    PandocConverter::convert_str(&markdown, &opts)
        .await
        .map_err(|e| e.to_string())
}

// ---- Phase B: book.toml + structured book ---------------------------------

use crate::typesetter::{
    analyze_structure, BookConfig, BookStructure, StructuredHtml,
};

#[derive(Serialize)]
pub struct LoadedBook {
    pub config: BookConfig,
    pub structure: BookStructure,
    pub enriched_html: String,
    pub stderr_warnings: String,
    pub pandoc_version: String,
    /// Absolute path to the front cover image, if set in config and the
    /// file exists. The frontend converts this via `convertFileSrc` for
    /// use in `<img src=...>`.
    pub front_cover_path: Option<String>,
    pub back_cover_path: Option<String>,
}

#[tauri::command]
pub async fn typesetter_book_load(book_path: String) -> Result<LoadedBook, String> {
    let path = std::path::PathBuf::from(&book_path);
    let mut config = BookConfig::load(&path).map_err(|e| e.to_string())?;

    // If the user opened a .md file directly and it's not in book.toml's
    // files array, rewrite the array to that single file. Intent: "the
    // file I open is the manuscript", regardless of what an older
    // book.toml says. Saves the writer from manually editing book.toml
    // every time they bump to v9 → v10 → v11.
    if path.is_file()
        && path
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown" | "txt"))
            .unwrap_or(false)
    {
        let opened_abs = std::fs::canonicalize(&path).unwrap_or(path.clone());
        let already_listed = config.resolved_files().iter().any(|p| {
            std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()) == opened_abs
        });
        if !already_listed {
            // Make path relative to book.toml's root_dir if possible —
            // strips the common prefix so book.toml stays portable.
            // Falls back to the absolute path if they're on different
            // volumes (Windows) or otherwise don't share a prefix.
            let root_abs = std::fs::canonicalize(&config.root_dir)
                .unwrap_or_else(|_| config.root_dir.clone());
            let rel_str = opened_abs
                .strip_prefix(&root_abs)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| opened_abs.to_string_lossy().to_string());
            config.files = vec![rel_str];
            config.save().map_err(|e| e.to_string())?;
        }
    }

    // Run the citation transformer over the concatenated markdown
    // before pandoc sees it. This both replaces [CITE:] markers with
    // numbered superscript references and appends the per-chapter
    // # Notes back-matter section.
    let (temp_md, citation_result) =
        crate::typesetter::prepare_book_markdown(&config).map_err(|e| e.to_string())?;

    let opts = crate::typesetter::pandoc::PandocConvertOptions::default();
    let pandoc_result = crate::typesetter::pandoc::PandocConverter::convert_file(&temp_md, &opts)
        .await
        .map_err(|e| format!("pandoc failed: {}", e));
    crate::typesetter::cleanup_temp_markdown(&temp_md);
    let pandoc_result = pandoc_result?;

    // Rewrite precomposed Unicode super/subscripts (10⁻³⁵, |ψ|², K₂⁰)
    // into <sup>/<sub> with ASCII glyphs so they render in the embedded
    // body font instead of falling back to a mismatched system font,
    // then tag "Math Anchor" blockquotes so they render as boxed asides.
    let combined_html = crate::typesetter::tag_math_anchors(
        &crate::typesetter::normalize_unicode_scripts(&pandoc_result.html),
    );
    let pandoc_version = pandoc_result.pandoc_version;

    // Surface citation warnings + pandoc stderr together so the writer
    // sees both in the same place.
    let mut combined_warnings = String::new();
    if !citation_result.warnings.is_empty() {
        combined_warnings.push_str("--- citation warnings ---\n");
        for w in &citation_result.warnings {
            combined_warnings.push_str(w);
            combined_warnings.push('\n');
        }
    }
    if !pandoc_result.stderr_warnings.trim().is_empty() {
        combined_warnings.push_str("\n--- pandoc warnings ---\n");
        combined_warnings.push_str(&pandoc_result.stderr_warnings);
    }
    if citation_result.note_count > 0 {
        combined_warnings.push_str(&format!(
            "\n--- citations: {} notes across {} chapter(s) ---\n",
            citation_result.note_count, citation_result.chapters_with_notes,
        ));
    }

    let StructuredHtml {
        structure,
        mut enriched_html,
    } = crate::typesetter::analyze_structure_with_options(
        &combined_html,
        config.typography.lead_in_word_count as usize,
    )
    .map_err(|e| e.to_string())?;

    // Prepend generated title / copyright / dedication pages drawn
    // from book.toml metadata. Empty pages are omitted automatically.
    let generated_front =
        crate::typesetter::build_generated_front_matter(&config.book);
    if !generated_front.is_empty() {
        enriched_html = format!("{}{}", generated_front, enriched_html);
    }
    // Append generated back-matter — acknowledgements page.
    let generated_back =
        crate::typesetter::build_generated_back_matter(&config.book);
    if !generated_back.is_empty() {
        enriched_html.push_str(&generated_back);
    }

    let resolve_cover = |rel: &Option<String>| -> Option<String> {
        let r = rel.as_ref()?;
        let p = std::path::Path::new(r);
        let abs = if p.is_absolute() { p.to_path_buf() } else { config.root_dir.join(p) };
        if abs.is_file() {
            Some(abs.to_string_lossy().to_string())
        } else {
            None
        }
    };
    let front_cover_path = resolve_cover(&config.book.cover_image);
    let back_cover_path = resolve_cover(&config.book.back_cover_image);

    Ok(LoadedBook {
        config,
        structure,
        enriched_html,
        stderr_warnings: combined_warnings,
        pandoc_version,
        front_cover_path,
        back_cover_path,
    })
}

#[tauri::command]
pub fn typesetter_book_init(markdown_path: String) -> Result<String, String> {
    let path = std::path::PathBuf::from(&markdown_path);
    let written = BookConfig::init_from_markdown(&path).map_err(|e| e.to_string())?;
    Ok(written.display().to_string())
}

#[tauri::command]
pub fn typesetter_analyze_html(html: String) -> Result<StructuredHtml, String> {
    analyze_structure(&html).map_err(|e| e.to_string())
}

/// Save an edited BookConfig back to disk. Resolves the target path the
/// same way `typesetter_book_load` does (the user passes the original
/// path they used to load — file or directory). The frontend BookConfig
/// payload omits the path fields (`#[serde(skip)]`), so we re-resolve.
#[tauri::command]
pub fn typesetter_book_save(book_path: String, config: BookConfig) -> Result<String, String> {
    let toml_path =
        BookConfig::locate(std::path::Path::new(&book_path)).map_err(|e| e.to_string())?;
    let mut to_write = config;
    to_write.config_path = toml_path.clone();
    to_write.root_dir = toml_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    to_write.save().map_err(|e| e.to_string())?;
    Ok(toml_path.display().to_string())
}

/// Read the contents of a manuscript file by index in book.toml's
/// files array. Used by Book Mode's Source view for in-place editing.
#[tauri::command]
pub fn typesetter_read_book_file(book_path: String, file_index: usize) -> Result<String, String> {
    let config = BookConfig::load(std::path::Path::new(&book_path)).map_err(|e| e.to_string())?;
    let paths = config.resolved_files();
    let target = paths
        .get(file_index)
        .ok_or_else(|| format!("file index {} out of range (have {})", file_index, paths.len()))?;
    std::fs::read_to_string(target).map_err(|e| e.to_string())
}

/// Write to a manuscript file by index. Used by Book Mode's Source
/// view's auto-save. Caller is expected to follow up with
/// typesetter_book_load to refresh structure + paginated preview.
#[tauri::command]
pub fn typesetter_write_book_file(
    book_path: String,
    file_index: usize,
    content: String,
) -> Result<(), String> {
    let config = BookConfig::load(std::path::Path::new(&book_path)).map_err(|e| e.to_string())?;
    let paths = config.resolved_files();
    let target = paths
        .get(file_index)
        .ok_or_else(|| format!("file index {} out of range (have {})", file_index, paths.len()))?;
    std::fs::write(target, content).map_err(|e| e.to_string())
}

/// Render the loaded book to an EPUB3 via pandoc.
#[tauri::command]
pub async fn typesetter_export_epub(
    book_path: String,
    output_path: String,
) -> Result<String, String> {
    use crate::typesetter::{export_epub, BookConfig};
    use std::path::Path;

    let config = BookConfig::load(Path::new(&book_path)).map_err(|e| e.to_string())?;
    let output = std::path::PathBuf::from(&output_path);
    export_epub(&config, &output)
        .await
        .map_err(|e| e.to_string())?;
    Ok(output.display().to_string())
}

/// Render the loaded book to a PDF via headless Chromium. Uses the
/// "export-grade" CSS (Phase D in full) which Chromium's native print
/// engine handles correctly, including string-set/string() running
/// headers, named-page roman/arabic page numbering, drop caps, and
/// forced-recto chapter starts — features Paged.js v0.4 chokes on but
/// Chromium native does not.
#[tauri::command]
pub async fn typesetter_export_pdf(
    book_path: String,
    output_path: String,
) -> Result<String, String> {
    use crate::typesetter::{
        build_export_html, html_to_pdf, paper_size_from_trim, BookConfig,
        PandocConverter,
    };
    use crate::typesetter::pandoc::PandocConvertOptions;
    use std::path::Path;

    let config = BookConfig::load(Path::new(&book_path)).map_err(|e| e.to_string())?;

    // Run the citation transform first, then a single pandoc pass over
    // the combined+transformed markdown. Same path the on-screen
    // preview uses, so the PDF and Pages preview always agree.
    let (temp_md, _citation_result) =
        crate::typesetter::prepare_book_markdown(&config).map_err(|e| e.to_string())?;

    // PDF-specific post-process: strip the <a> link wrapper from each
    // superscript reference. Print pages can't be clicked, and
    // Chromium's link annotations have been observed to change how
    // the underlying digit glyphs are embedded — KDP then flags those
    // specific digits as not printable while leaving non-link digits
    // alone. The EPUB pipeline keeps the links intact for tap-back
    // navigation in e-readers.
    {
        let original = std::fs::read_to_string(&temp_md).map_err(|e| e.to_string())?;
        let re = regex::Regex::new(
            r##"<sup class="note-ref"([^>]*)><a href="#[^"]*">(\d+)</a></sup>"##,
        )
        .map_err(|e| e.to_string())?;
        let stripped = re.replace_all(&original, r##"<sup class="note-ref"$1>$2</sup>"##);
        std::fs::write(&temp_md, stripped.as_bytes()).map_err(|e| e.to_string())?;
    }

    // PDF export: emit native MathML so Chromium renders math without
    // JS. The on-screen preview path keeps the default --katex (which
    // we render via katex.js after Paged.js paginates).
    let opts = PandocConvertOptions {
        math_format: Some("mathml".to_string()),
        ..Default::default()
    };
    let pandoc_outcome = PandocConverter::convert_file(&temp_md, &opts).await;
    crate::typesetter::cleanup_temp_markdown(&temp_md);
    let combined_html = crate::typesetter::tag_math_anchors(
        &crate::typesetter::normalize_unicode_scripts(
            &pandoc_outcome
                .map_err(|e| format!("pandoc failed: {}", e))?
                .html,
        ),
    );
    let mut structured = crate::typesetter::analyze_structure_with_options(
        &combined_html,
        config.typography.lead_in_word_count as usize,
    )
    .map_err(|e| e.to_string())?;

    // Prepend generated title / copyright / dedication pages.
    let generated_front =
        crate::typesetter::build_generated_front_matter(&config.book);
    if !generated_front.is_empty() {
        structured.enriched_html =
            format!("{}{}", generated_front, structured.enriched_html);
    }
    // Append generated back-matter — acknowledgements page.
    let generated_back =
        crate::typesetter::build_generated_back_matter(&config.book);
    if !generated_back.is_empty() {
        structured.enriched_html.push_str(&generated_back);
    }

    // Resolve cover paths
    let resolve_cover = |rel: &Option<String>| -> Option<std::path::PathBuf> {
        let r = rel.as_ref()?;
        let p = Path::new(r);
        let abs = if p.is_absolute() { p.to_path_buf() } else { config.root_dir.join(p) };
        if abs.is_file() { Some(abs) } else { None }
    };
    let front = resolve_cover(&config.book.cover_image);
    let back = resolve_cover(&config.book.back_cover_image);

    let html = build_export_html(
        &config,
        &structured.enriched_html,
        &structured.structure,
        front.as_deref(),
        back.as_deref(),
    );

    let paper = paper_size_from_trim(&config.trim.size);
    let output = std::path::PathBuf::from(&output_path);

    // Headless Chromium is blocking; spawn on a blocking task.
    let html_owned = html;
    let output_for_thread = output.clone();
    tokio::task::spawn_blocking(move || html_to_pdf(&html_owned, &output_for_thread, paper))
        .await
        .map_err(|e| format!("export task join error: {}", e))?
        .map_err(|e| e.to_string())?;

    Ok(output.display().to_string())
}


// =============================================================================
// Research feed — flat list of stored research summaries for the swipe-through
// UI in ResearchHub. Backed by the same MCP-side memory store that
// list_research_summaries reads.
// =============================================================================

#[derive(serde::Serialize)]
pub struct ResearchFeedItem {
    pub id: String,
    pub title: String,
    pub topic: String,
    pub date: String,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub url: Option<String>,
    pub authors: Option<String>,
    pub year: Option<String>,
    pub venue: Option<String>,
    pub summary: String,
}

#[tauri::command]
pub async fn research_feed_list(
    topic: Option<String>,
    limit: Option<usize>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<Vec<ResearchFeedItem>, String> {
    let limit = limit.unwrap_or(100).min(500);
    let tag = match &topic {
        Some(t) => format!("research-topic-name:{}", t),
        None => "kind:research-summary".to_string(),
    };
    let store = search.store.read().await;
    let objs = store
        .list_by_tag(&tag, limit.max(200))
        .map_err(|e| e.to_string())?;
    let topic_filter = topic.clone();
    let exact_tag = tag.clone();
    let mut items: Vec<ResearchFeedItem> = objs
        .iter()
        .filter(|o| o.tags.iter().any(|t| t == "kind:research-summary"))
        .filter(|o| {
            if topic_filter.is_some() {
                o.tags.iter().any(|t| t == &exact_tag)
            } else {
                true
            }
        })
        .map(|o| {
            let topic_tag = o
                .tags
                .iter()
                .find(|t| t.starts_with("research-topic-name:"))
                .map(|t| t.trim_start_matches("research-topic-name:").to_string())
                .unwrap_or_default();
            let date = o
                .tags
                .iter()
                .find(|t| t.starts_with("research-date:"))
                .map(|t| t.trim_start_matches("research-date:").to_string())
                .unwrap_or_default();
            ResearchFeedItem {
                id: o.suid.to_string(),
                title: o.name.clone().unwrap_or_default(),
                topic: topic_tag,
                date,
                doi: o.metadata.get("doi").and_then(|v| v.as_str()).map(String::from),
                arxiv_id: o.metadata.get("arxiv_id").and_then(|v| v.as_str()).map(String::from),
                url: o.metadata.get("source_url").and_then(|v| v.as_str()).map(String::from),
                authors: o.metadata.get("authors").and_then(|v| v.as_str()).map(String::from),
                year: o.metadata.get("year").and_then(|v| v.as_str()).map(String::from),
                venue: o.metadata.get("venue").and_then(|v| v.as_str()).map(String::from),
                summary: o.content_as_str().unwrap_or("").to_string(),
            }
        })
        .take(limit)
        .collect();
    items.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(items)
}

// =============================================================================
// Daily research fetch — triggers the same academic-search loop the cron job
// runs, but invocable from the Marlos UI. For each registered research topic,
// fetches papers via the academic-search APIs, dedups against already-stored
// summaries, and stores the abstract as the summary (no LLM call needed for
// browseable swipe-through cards; LLM-improved summaries can come later).
// =============================================================================

#[derive(serde::Serialize)]
pub struct ResearchFetchResult {
    pub total_stored: usize,
    pub per_topic: Vec<ResearchFetchTopicResult>,
}

#[derive(serde::Serialize)]
pub struct ResearchFetchTopicResult {
    pub topic: String,
    pub queries: Vec<String>,
    pub fetched: usize,
    pub stored: usize,
    pub error: Option<String>,
}

fn normalize_research_url(url: &str) -> String {
    let lower = url.trim().to_lowercase();
    let without_scheme = lower
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let without_query = without_scheme.split('?').next().unwrap_or(without_scheme);
    let without_frag = without_query.split('#').next().unwrap_or(without_query);
    without_frag.trim_end_matches('/').to_string()
}

#[tauri::command]
pub async fn research_run_daily_fetch(
    topic: Option<String>,
    limit_per_topic: Option<usize>,
    search: State<'_, Arc<SemanticSearch>>,
) -> Result<ResearchFetchResult, String> {
    use crate::mcp::web_search;
    use crate::semantic_object::SemanticObject;
    use crate::memory::SecurityTier;
    use chrono::Utc;

    let limit_per_topic = limit_per_topic.unwrap_or(5).clamp(1, 20);

    let topics: Vec<(String, Vec<String>)> = {
        let store = search.store.read().await;
        let objs = store
            .list_by_tag("kind:research-topic", 200)
            .map_err(|e| e.to_string())?;
        objs.into_iter()
            .filter_map(|o| {
                let name = o
                    .tags
                    .iter()
                    .find(|t| t.starts_with("research-topic-name:"))
                    .map(|t| t.trim_start_matches("research-topic-name:").to_string())?;
                if let Some(filter) = &topic {
                    if &name != filter {
                        return None;
                    }
                }
                let parsed: serde_json::Value = o
                    .content_as_str()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(serde_json::json!({}));
                let queries: Vec<String> = parsed
                    .get("queries")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_else(|| vec![name.clone()]);
                Some((name, queries))
            })
            .collect()
    };

    if topics.is_empty() {
        return Err(
            "No research topics registered. Use the MCP add_research_topic tool first.".to_string(),
        );
    }

    let mut per_topic_results = Vec::new();
    let mut total_stored = 0usize;

    for (topic_idx, (topic_name, queries)) in topics.into_iter().enumerate() {
        // 3-second gap between topics. The first topic runs without
        // wait so the user sees the spinner move quickly initially.
        if topic_idx > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
        let mut fetched_papers: Vec<web_search::AcademicPaper> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut fetch_error: Option<String> = None;

        // Call Semantic Scholar and arXiv directly per query so we can
        // surface per-source errors. Throttled — Semantic Scholar's
        // free tier is ~1 req/sec and arXiv asks for 3-sec spacing in
        // their robots.txt. Bursting 12 requests gets us 429s.
        let per_query = (limit_per_topic * 2).max(8);
        let mut source_errors: Vec<String> = Vec::new();
        for (qi, q) in queries.iter().enumerate() {
            let mut got_any = false;

            // Spacing between iterations — first query runs immediately.
            if qi > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
            }

            match web_search::search_semantic_scholar(q, per_query).await {
                Ok(papers) => {
                    for paper in papers {
                        let norm: String = paper
                            .title
                            .to_lowercase()
                            .chars()
                            .filter(|c| c.is_alphanumeric())
                            .collect();
                        if seen.insert(norm) {
                            fetched_papers.push(paper);
                            got_any = true;
                        }
                    }
                }
                Err(e) => {
                    source_errors.push(format!("S2 [{}]: {}", q, e));
                }
            }

            // Brief spacing between S2 and arXiv for the same query.
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;

            match web_search::search_arxiv(q, (per_query / 2).max(4)).await {
                Ok(papers) => {
                    for paper in papers {
                        let norm: String = paper
                            .title
                            .to_lowercase()
                            .chars()
                            .filter(|c| c.is_alphanumeric())
                            .collect();
                        if seen.insert(norm) {
                            fetched_papers.push(paper);
                            got_any = true;
                        }
                    }
                }
                Err(e) => {
                    source_errors.push(format!("arXiv [{}]: {}", q, e));
                }
            }

            if !got_any && source_errors.is_empty() {
                source_errors.push(format!("[{}]: both sources returned 0 results", q));
            }
        }

        if !source_errors.is_empty() {
            fetch_error = Some(source_errors.join(" | "));
        }

        let fetched_count = fetched_papers.len();
        let mut stored_count = 0usize;

        for paper in fetched_papers.into_iter().take(limit_per_topic * 2) {
            if stored_count >= limit_per_topic {
                break;
            }

            let url_tag = format!("dedup-url:{}", normalize_research_url(&paper.url));
            let doi_tag = paper.doi.as_ref().map(|d| {
                if d.starts_with("arXiv:") {
                    format!("dedup-arxiv:{}", d.trim_start_matches("arXiv:"))
                } else {
                    format!("dedup-doi:{}", d)
                }
            });

            let already_stored = {
                let store = search.store.read().await;
                let url_hit = store
                    .list_by_tag(&url_tag, 1)
                    .map(|hits| hits.iter().any(|o| o.tags.iter().any(|t| t == &url_tag)))
                    .unwrap_or(false);
                let doi_hit = if let Some(tag) = &doi_tag {
                    store
                        .list_by_tag(tag, 1)
                        .map(|hits| hits.iter().any(|o| o.tags.iter().any(|t| t == tag)))
                        .unwrap_or(false)
                } else {
                    false
                };
                url_hit || doi_hit
            };

            if already_stored {
                continue;
            }

            let summary = paper
                .abstract_text
                .as_deref()
                .unwrap_or("(no abstract available)")
                .to_string();

            let authors_line = if paper.authors.is_empty() {
                String::new()
            } else {
                format!("\n**Authors:** {}", paper.authors.join(", "))
            };
            let year_line = paper
                .year
                .map(|y| format!("\n**Year:** {}", y))
                .unwrap_or_default();
            let venue_line = paper
                .venue
                .as_ref()
                .map(|v| format!("\n**Venue:** {}", v))
                .unwrap_or_default();
            let doi_line = paper
                .doi
                .as_ref()
                .map(|d| format!("\n**DOI:** {}", d))
                .unwrap_or_default();

            let markdown = format!(
                "# {}\n\n**URL:** {}{}{}{}{}\n\n## Summary\n\n{}\n",
                paper.title, paper.url, authors_line, year_line, venue_line, doi_line, summary
            );

            let today = Utc::now().format("%Y-%m-%d").to_string();
            let mut obj = SemanticObject::from_markdown(&markdown)
                .with_name(&paper.title)
                .with_tag("kind:research-summary")
                .with_tag(&format!("research-topic-name:{}", topic_name))
                .with_tag(&format!("research-date:{}", today))
                .with_tag(&url_tag)
                .with_tier(SecurityTier::Open);

            if let Some(tag) = &doi_tag {
                obj = obj.with_tag(tag);
            }

            obj = obj
                .with_metadata("topic", serde_json::json!(topic_name))
                .with_metadata("source_url", serde_json::json!(paper.url))
                .with_metadata("authors", serde_json::json!(paper.authors))
                .with_metadata("year", serde_json::json!(paper.year))
                .with_metadata("venue", serde_json::json!(paper.venue))
                .with_metadata("doi", serde_json::json!(paper.doi))
                .with_metadata("arxiv_id", serde_json::Value::Null);

            match search.store(&obj).await {
                Ok(_) => {
                    stored_count += 1;
                    total_stored += 1;
                }
                Err(e) => {
                    log::warn!("Failed to store research summary: {}", e);
                }
            }
        }

        per_topic_results.push(ResearchFetchTopicResult {
            topic: topic_name,
            queries,
            fetched: fetched_count,
            stored: stored_count,
            error: fetch_error,
        });
    }

    Ok(ResearchFetchResult {
        total_stored,
        per_topic: per_topic_results,
    })
}
