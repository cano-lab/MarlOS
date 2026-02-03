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

/// List all sources
#[tauri::command]
pub async fn research_list_sources(
    tag_filter: Option<String>,
    limit: Option<usize>,
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

    Ok(format!("Cleared {} session chunks from vector database", deleted))
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
