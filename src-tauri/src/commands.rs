//! Tauri commands - IPC interface between frontend and backend
//!
//! These commands are callable from JavaScript/TypeScript via Tauri's invoke API.

use std::sync::{Arc, Mutex};
use tauri::State;
use serde::{Deserialize, Serialize};

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
    ai_manager: State<'_, AiManager>,
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

    // Get all objects of kind "source"
    let all_objects = store.list(1000, 0).map_err(|e| e.to_string())?;

    let mut sources: Vec<Source> = all_objects.iter()
        .filter(|obj| obj.tags.contains(&"kind:source".to_string()))
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
    ai_manager: State<'_, AiManager>,
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
    ai_manager: State<'_, AiManager>,
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
    ai_manager: State<'_, AiManager>,
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
    // Parse source from summary JSON
    obj.summary.as_ref()
        .and_then(|s| serde_json::from_str::<Source>(s).ok())
}
