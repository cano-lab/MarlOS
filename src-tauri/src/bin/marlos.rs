//! MarlOS CLI - Command-line interface for semantic object management
//!
//! This CLI allows Claude Code and other tools to interact with MarlOS's
//! semantic object store without needing the GUI.
//!
//! Features:
//! - Auto-starts LM Studio for embeddings if available
//! - Semantic search across all objects
//! - CRUD operations on semantic objects
//! - Security tier management
//! - JSON output for easy parsing

use chrono::Utc;
use clap::{Parser, Subcommand};
use marlos_lib::andor_client::{AndorClient, MemoryQueryRequest, ContextSearchRequest, SemanticSearchRequest};
use marlos_lib::embeddings::EmbeddingManager;
use marlos_lib::memory::SecurityTier;
use marlos_lib::object_store::ObjectStore;
use marlos_lib::providers::{
    SessionScanner, SessionParser, session_to_objects,
    ChunkedSessionParser, chunked_session_to_objects,
};
use marlos_lib::semantic_object::{ContentType, SemanticObject, Suid, FileBoundary, RelationType};
use marlos_lib::semantic_search::{SemanticSearch, SearchOptions};
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use std::thread;

#[derive(Parser)]
#[command(name = "marlos")]
#[command(about = "MarlOS CLI - Semantic object management for AI assistants")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format (json or text)
    #[arg(long, default_value = "json")]
    format: String,

    /// Skip LM Studio auto-start
    #[arg(long)]
    no_lm_studio: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Search objects semantically
    Search {
        /// Search query
        query: String,
        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,
        /// Filter by tier (open, guarded, sealed)
        #[arg(short, long)]
        tier: Option<String>,
    },

    /// Get an object by SUID
    Get {
        /// Object SUID
        suid: String,
    },

    /// List all objects
    List {
        /// Filter by tier
        #[arg(short, long)]
        tier: Option<String>,
        /// Filter by content type
        #[arg(short = 'c', long)]
        content_type: Option<String>,
        /// Maximum results
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },

    /// Create a new object
    Create {
        /// Object content
        content: String,
        /// Object name
        #[arg(short, long)]
        name: Option<String>,
        /// Content type (text, markdown, json, code)
        #[arg(short = 'c', long, default_value = "text")]
        content_type: String,
        /// Security tier (open, guarded, sealed)
        #[arg(long, default_value = "open")]
        tier: String,
    },

    /// Import a file as an object
    Import {
        /// Path to file
        path: PathBuf,
    },

    /// Export an object to a file
    Export {
        /// Object SUID
        suid: String,
        /// Output path
        path: PathBuf,
    },

    /// Delete an object
    Delete {
        /// Object SUID
        suid: String,
    },

    /// Change an object's security tier
    Tier {
        /// Object SUID
        suid: String,
        /// New tier (open, guarded, sealed)
        tier: String,
        /// Reason for change
        #[arg(short, long, default_value = "Changed via CLI")]
        reason: String,
    },

    /// Find objects similar to a given object
    Similar {
        /// Object SUID
        suid: String,
        /// Maximum results
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },

    /// Show status (database stats, LM Studio status)
    Status,

    /// Reindex all objects with current embedding model
    Reindex {
        /// Only show what would be reindexed (dry run)
        #[arg(long)]
        dry_run: bool,
    },

    /// Add a relation between objects
    Relate {
        /// Source object SUID
        from: String,
        /// Target object SUID
        to: String,
        /// Relation type (references, extends, implements, etc.)
        #[arg(short, long, default_value = "references")]
        relation: String,
    },

    /// Query Andor Hub (coding sessions, memories, context)
    Andor {
        #[command(subcommand)]
        command: AndorCommands,
    },

    /// Manage Claude Code sessions (native MarlOS tracking)
    Sessions {
        #[command(subcommand)]
        command: SessionCommands,
    },
}

#[derive(Subcommand)]
enum AndorCommands {
    /// Check if Andor Hub is running
    Status,

    /// List coding sessions
    Sessions {
        /// Filter by status (active, completed)
        #[arg(short, long)]
        status: Option<String>,
        /// Filter by session type
        #[arg(short = 't', long)]
        session_type: Option<String>,
        /// Filter by repo path
        #[arg(short, long)]
        repo: Option<String>,
        /// Maximum results
        #[arg(short, long, default_value = "20")]
        limit: i32,
    },

    /// Get a specific session by ID
    Session {
        /// Session ID
        id: String,
    },

    /// Get session statistics
    SessionStats,

    /// Query memories (semantic search)
    Memory {
        /// Search query
        query: String,
        /// Filter by namespace
        #[arg(short, long)]
        namespace: Option<String>,
        /// Filter by type (fact, decision, pattern, etc.)
        #[arg(short = 't', long)]
        memory_type: Option<String>,
        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: i32,
        /// Search mode (exact, semantic, hybrid)
        #[arg(short, long, default_value = "hybrid")]
        mode: String,
    },

    /// Get memory statistics
    MemoryStats,

    /// Get memories for a specific session
    SessionMemories {
        /// Session ID
        session_id: String,
    },

    /// List repos with context maps
    Repos,

    /// Search repo context (semantic search over files)
    Context {
        /// Search query
        query: String,
        /// Filter by repo name
        #[arg(short, long)]
        repo: Option<String>,
        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: i32,
    },

    /// Semantic search across sessions
    Search {
        /// Search query
        query: String,
        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: i32,
        /// Minimum score threshold
        #[arg(short = 's', long)]
        min_score: Option<f32>,
    },
}

#[derive(Subcommand)]
enum SessionCommands {
    /// Scan and list all Claude Code sessions
    List {
        /// Filter by project path (substring match)
        #[arg(short, long)]
        project: Option<String>,
        /// Maximum results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Parse and show details of a specific session
    Show {
        /// Session ID (UUID) or path to JSONL file
        session: String,
    },

    /// Show chunks from a session with file references
    Chunks {
        /// Session ID (UUID)
        session: String,
        /// Maximum chunks to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Search for sessions that touched specific files
    Files {
        /// File path pattern to search for (substring match)
        pattern: String,
        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Import sessions into MarlOS object store (chunked with file refs)
    Import {
        /// Session ID to import, or "all" to import all sessions
        session: String,
        /// Generate embeddings for imported chunks
        #[arg(long)]
        embed: bool,
        /// Use legacy mode (whole session, not chunked)
        #[arg(long)]
        legacy: bool,
    },

    /// Search across all sessions (without importing)
    Search {
        /// Search query
        query: String,
        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Show session statistics
    Stats,
}

#[derive(Serialize)]
struct CliResponse<T: Serialize> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct SearchResult {
    suid: String,
    name: Option<String>,
    content_type: String,
    tier: String,
    score: f32,
    preview: String,
}

#[derive(Serialize)]
struct ObjectInfo {
    suid: String,
    name: Option<String>,
    content_type: String,
    tier: String,
    content: String,
    size_bytes: usize,
    created_at: String,
    modified_at: String,
    version: u64,
    tags: Vec<String>,
    summary: Option<String>,
}

#[derive(Serialize)]
struct StatusInfo {
    database_path: String,
    object_count: usize,
    lm_studio_running: bool,
    lm_studio_model: Option<String>,
    embedding_provider: String,
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    let result = run_command(&cli);

    match cli.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        _ => {
            if result.success {
                if let Some(data) = &result.data {
                    println!("{}", serde_json::to_string_pretty(data).unwrap());
                } else {
                    println!("OK");
                }
            } else if let Some(err) = &result.error {
                eprintln!("Error: {}", err);
            }
        }
    }

    std::process::exit(if result.success { 0 } else { 1 });
}

fn run_command(cli: &Cli) -> CliResponse<serde_json::Value> {
    // Get database path
    let db_path = get_database_path();

    // Ensure LM Studio is running (unless disabled)
    if !cli.no_lm_studio {
        if let Err(e) = ensure_lm_studio_running() {
            log::warn!("LM Studio auto-start failed: {}", e);
            // Continue anyway, will fall back to mock embeddings
        }
    }

    // Initialize components
    let store = match ObjectStore::new(db_path.clone()) {
        Ok(s) => s,
        Err(e) => {
            return CliResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to open database: {}", e)),
            };
        }
    };

    // Create embedding manager (try LM Studio, fall back to mock)
    let rt = tokio::runtime::Runtime::new().unwrap();
    let embedding_manager = rt.block_on(EmbeddingManager::auto_detect());

    let search = SemanticSearch::new(store, embedding_manager);

    match &cli.command {
        Commands::Search { query, limit, tier } => {
            let max_tier = tier.as_ref()
                .and_then(|t| parse_tier(t))
                .unwrap_or(SecurityTier::Guarded);

            let options = SearchOptions {
                limit: *limit,
                max_tier,
                ..Default::default()
            };

            match rt.block_on(search.search(query, options)) {
                Ok(results) => {
                    let data: Vec<SearchResult> = results
                        .into_iter()
                        .map(|r| {
                            let content_str = r.object.content_as_str()
                                .unwrap_or("")
                                .chars()
                                .take(200)
                                .collect::<String>();
                            SearchResult {
                                suid: r.object.suid.to_string(),
                                name: r.object.name.clone(),
                                content_type: format!("{:?}", r.object.content_type),
                                tier: format!("{:?}", r.object.security_tier),
                                score: r.score,
                                preview: content_str,
                            }
                        })
                        .collect();
                    CliResponse {
                        success: true,
                        data: Some(serde_json::to_value(data).unwrap()),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        Commands::Get { suid } => {
            let store = search.store.blocking_read();
            let parsed_suid = match Suid::parse(suid) {
                Ok(s) => s,
                Err(e) => {
                    return CliResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid SUID: {}", e)),
                    };
                }
            };
            match store.get(&parsed_suid) {
                Ok(Some(obj)) => {
                    let info = object_to_info(&obj);
                    CliResponse {
                        success: true,
                        data: Some(serde_json::to_value(info).unwrap()),
                        error: None,
                    }
                }
                Ok(None) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Object not found: {}", suid)),
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        Commands::List { tier, content_type, limit } => {
            let store = search.store.blocking_read();
            let tier_filter = tier.as_ref().and_then(|t| parse_tier(t));
            let type_filter = content_type.as_ref().and_then(|t| parse_content_type(t));

            match store.list(*limit, 0) {
                Ok(objects) => {
                    // Apply filters in memory (store.list doesn't support them)
                    let filtered: Vec<&SemanticObject> = objects.iter()
                        .filter(|obj| {
                            tier_filter.map_or(true, |t| obj.security_tier == t) &&
                            type_filter.as_ref().map_or(true, |ct| {
                                std::mem::discriminant(&obj.content_type) == std::mem::discriminant(ct)
                            })
                        })
                        .collect();

                    let data: Vec<ObjectInfo> = filtered.iter().map(|o| object_to_info(o)).collect();
                    CliResponse {
                        success: true,
                        data: Some(serde_json::to_value(data).unwrap()),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        Commands::Create { content, name, content_type, tier } => {
            let ct = parse_content_type(content_type).unwrap_or(ContentType::Text);
            let t = parse_tier(tier).unwrap_or(SecurityTier::Open);

            let content_bytes = content.as_bytes().to_vec();
            let mut obj = SemanticObject::new(content_bytes, ct);
            obj.name = name.clone();
            obj.security_tier = t;

            let store = search.store.blocking_write();
            match store.create(&obj) {
                Ok(()) => {
                    // Generate and store embedding
                    drop(store);
                    if let Ok(embedding) = rt.block_on(search.embeddings().embed(content)) {
                        let store = search.store.blocking_write();
                        let model = search.embeddings().model_info().id.clone();
                        let _ = store.store_embedding(&obj.suid, &embedding, &model);
                    }

                    CliResponse {
                        success: true,
                        data: Some(serde_json::json!({
                            "suid": obj.suid.to_string(),
                            "message": "Object created"
                        })),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        Commands::Import { path } => {
            match FileBoundary::import(path, None) {
                Ok(result) => {
                    let obj = result.object;
                    let suid = obj.suid.to_string();
                    let content_for_embed: Option<String> = obj.content_as_str().map(|s| s.to_string());

                    let store = search.store.blocking_write();
                    match store.create(&obj) {
                        Ok(()) => {
                            // Generate embedding if text content
                            drop(store);
                            if let Some(content) = content_for_embed {
                                if let Ok(embedding) = rt.block_on(search.embeddings().embed(&content)) {
                                    let store = search.store.blocking_write();
                                    let model = search.embeddings().model_info().id.clone();
                                    let _ = store.store_embedding(&obj.suid, &embedding, &model);
                                }
                            }

                            CliResponse {
                                success: true,
                                data: Some(serde_json::json!({
                                    "suid": suid,
                                    "secrets_detected": result.secrets_detected,
                                    "detected_tier": format!("{:?}", result.detected_tier),
                                    "message": "File imported"
                                })),
                                error: None,
                            }
                        }
                        Err(e) => CliResponse {
                            success: false,
                            data: None,
                            error: Some(e.to_string()),
                        },
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e),
                },
            }
        }

        Commands::Export { suid, path } => {
            let parsed_suid = match Suid::parse(suid) {
                Ok(s) => s,
                Err(e) => {
                    return CliResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid SUID: {}", e)),
                    };
                }
            };

            let store = search.store.blocking_read();
            match store.get(&parsed_suid) {
                Ok(Some(obj)) => {
                    drop(store);
                    match FileBoundary::export(&obj, path) {
                        Ok(result) => CliResponse {
                            success: true,
                            data: Some(serde_json::json!({
                                "path": result.path,
                                "bytes_written": result.bytes_written,
                                "message": "Object exported"
                            })),
                            error: None,
                        },
                        Err(e) => CliResponse {
                            success: false,
                            data: None,
                            error: Some(e),
                        },
                    }
                }
                Ok(None) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Object not found: {}", suid)),
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        Commands::Delete { suid } => {
            let parsed_suid = match Suid::parse(suid) {
                Ok(s) => s,
                Err(e) => {
                    return CliResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid SUID: {}", e)),
                    };
                }
            };

            let store = search.store.blocking_write();
            match store.delete(&parsed_suid) {
                Ok(true) => CliResponse {
                    success: true,
                    data: Some(serde_json::json!({ "message": "Object deleted" })),
                    error: None,
                },
                Ok(false) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Object not found: {}", suid)),
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        Commands::Tier { suid, tier, reason: _ } => {
            let new_tier = match parse_tier(tier) {
                Some(t) => t,
                None => {
                    return CliResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid tier: {}. Use open, guarded, or sealed", tier)),
                    };
                }
            };

            let parsed_suid = match Suid::parse(suid) {
                Ok(s) => s,
                Err(e) => {
                    return CliResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid SUID: {}", e)),
                    };
                }
            };

            let store = search.store.blocking_write();
            match store.get(&parsed_suid) {
                Ok(Some(mut obj)) => {
                    let old_tier = obj.security_tier;
                    obj.security_tier = new_tier;
                    obj.version += 1;
                    obj.modified_at = Utc::now();

                    match store.update(&obj) {
                        Ok(()) => CliResponse {
                            success: true,
                            data: Some(serde_json::json!({
                                "suid": suid,
                                "old_tier": format!("{:?}", old_tier),
                                "new_tier": format!("{:?}", new_tier),
                                "message": "Tier updated"
                            })),
                            error: None,
                        },
                        Err(e) => CliResponse {
                            success: false,
                            data: None,
                            error: Some(e.to_string()),
                        },
                    }
                }
                Ok(None) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Object not found: {}", suid)),
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        Commands::Similar { suid, limit } => {
            let parsed_suid = match Suid::parse(suid) {
                Ok(s) => s,
                Err(e) => {
                    return CliResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid SUID: {}", e)),
                    };
                }
            };

            match rt.block_on(search.find_similar(&parsed_suid, *limit)) {
                Ok(results) => {
                    let data: Vec<SearchResult> = results
                        .into_iter()
                        .map(|r| {
                            let content_str = r.object.content_as_str()
                                .unwrap_or("")
                                .chars()
                                .take(200)
                                .collect::<String>();
                            SearchResult {
                                suid: r.object.suid.to_string(),
                                name: r.object.name.clone(),
                                content_type: format!("{:?}", r.object.content_type),
                                tier: format!("{:?}", r.object.security_tier),
                                score: r.score,
                                preview: content_str,
                            }
                        })
                        .collect();
                    CliResponse {
                        success: true,
                        data: Some(serde_json::to_value(data).unwrap()),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        Commands::Status => {
            let store = search.store.blocking_read();
            let count = store.count().unwrap_or(0);
            let lm_studio_running = check_lm_studio_running();
            let model_info = search.embeddings().model_info();

            let status = StatusInfo {
                database_path: db_path.to_string_lossy().to_string(),
                object_count: count,
                lm_studio_running,
                lm_studio_model: if lm_studio_running {
                    Some(model_info.id.clone())
                } else {
                    None
                },
                embedding_provider: model_info.name.clone(),
            };

            CliResponse {
                success: true,
                data: Some(serde_json::to_value(status).unwrap()),
                error: None,
            }
        }

        Commands::Reindex { dry_run } => {
            let model_info = search.embeddings().model_info();

            // Check if LM Studio is available
            if !check_lm_studio_running() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some("LM Studio is not running. Start it first to generate embeddings.".to_string()),
                };
            }

            let store = search.store.blocking_read();
            let objects = match store.list(10000, 0) {
                Ok(objs) => objs,
                Err(e) => {
                    return CliResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Failed to list objects: {}", e)),
                    };
                }
            };
            drop(store);

            let text_objects: Vec<_> = objects
                .iter()
                .filter(|obj| obj.content_type.is_text())
                .collect();

            if *dry_run {
                return CliResponse {
                    success: true,
                    data: Some(serde_json::json!({
                        "dry_run": true,
                        "total_objects": objects.len(),
                        "text_objects_to_reindex": text_objects.len(),
                        "embedding_model": model_info.id,
                        "dimensions": model_info.dimensions,
                    })),
                    error: None,
                };
            }

            eprintln!("Reindexing {} objects with {} ({} dimensions)...",
                text_objects.len(), model_info.id, model_info.dimensions);

            match rt.block_on(search.reindex_all()) {
                Ok(count) => {
                    eprintln!("Successfully reindexed {} objects", count);
                    CliResponse {
                        success: true,
                        data: Some(serde_json::json!({
                            "reindexed": count,
                            "embedding_model": model_info.id,
                            "dimensions": model_info.dimensions,
                        })),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to reindex: {}", e)),
                },
            }
        }

        Commands::Relate { from, to, relation } => {
            let from_suid = match Suid::parse(from) {
                Ok(s) => s,
                Err(e) => {
                    return CliResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid source SUID: {}", e)),
                    };
                }
            };
            let to_suid = match Suid::parse(to) {
                Ok(s) => s,
                Err(e) => {
                    return CliResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid target SUID: {}", e)),
                    };
                }
            };
            let rel_type = parse_relation_type(relation);

            let store = search.store.blocking_write();
            match store.add_relation(&from_suid, &to_suid, rel_type) {
                Ok(()) => CliResponse {
                    success: true,
                    data: Some(serde_json::json!({
                        "from": from,
                        "to": to,
                        "relation": relation,
                        "message": "Relation added"
                    })),
                    error: None,
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        Commands::Andor { command } => {
            run_andor_command(command)
        }

        Commands::Sessions { command } => {
            run_session_command(command, &rt, &search)
        }
    }
}

fn run_session_command(
    command: &SessionCommands,
    rt: &tokio::runtime::Runtime,
    search: &SemanticSearch,
) -> CliResponse<serde_json::Value> {
    let scanner = SessionScanner::new();

    match command {
        SessionCommands::List { project, limit } => {
            match scanner.find_all_sessions() {
                Ok(sessions) => {
                    let mut results: Vec<serde_json::Value> = Vec::new();

                    for path in sessions.iter() {
                        // Stop if we've reached the limit
                        if results.len() >= *limit {
                            break;
                        }

                        // Parse each session to get metadata
                        match SessionParser::parse_file(path) {
                            Ok(parsed) => {
                                // Apply project filter if provided
                                if let Some(filter) = project {
                                    if let Some(ref proj_path) = parsed.project_path {
                                        if !proj_path.to_lowercase().contains(&filter.to_lowercase()) {
                                            continue;
                                        }
                                    } else {
                                        continue;
                                    }
                                }

                                results.push(serde_json::json!({
                                    "id": parsed.id,
                                    "project": parsed.project_path,
                                    "branch": parsed.git_branch,
                                    "message_count": parsed.message_count,
                                    "user_messages": parsed.user_messages.len(),
                                    "assistant_messages": parsed.assistant_messages.len(),
                                    "started_at": parsed.started_at.map(|t| t.to_rfc3339()),
                                    "ended_at": parsed.ended_at.map(|t| t.to_rfc3339()),
                                    "source_file": parsed.source_file,
                                }));
                            }
                            Err(e) => {
                                log::warn!("Failed to parse session {}: {}", path.display(), e);
                            }
                        }
                    }

                    CliResponse {
                        success: true,
                        data: Some(serde_json::json!({
                            "sessions": results,
                            "total": results.len(),
                        })),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e),
                },
            }
        }

        SessionCommands::Show { session } => {
            // Try to find the session file
            let path = if session.ends_with(".jsonl") {
                PathBuf::from(session)
            } else {
                // Search for session by ID
                match scanner.find_all_sessions() {
                    Ok(sessions) => {
                        sessions.into_iter()
                            .find(|p| {
                                p.file_stem()
                                    .and_then(|s| s.to_str())
                                    .map_or(false, |s| s == session)
                            })
                            .unwrap_or_else(|| PathBuf::from(session))
                    }
                    Err(_) => PathBuf::from(session),
                }
            };

            match SessionParser::parse_file(&path) {
                Ok(parsed) => {
                    CliResponse {
                        success: true,
                        data: Some(serde_json::json!({
                            "id": parsed.id,
                            "project": parsed.project_path,
                            "branch": parsed.git_branch,
                            "message_count": parsed.message_count,
                            "user_messages": parsed.user_messages.len(),
                            "assistant_messages": parsed.assistant_messages.len(),
                            "tool_calls": parsed.tool_calls,
                            "started_at": parsed.started_at.map(|t| t.to_rfc3339()),
                            "ended_at": parsed.ended_at.map(|t| t.to_rfc3339()),
                            "summary": parsed.generate_summary(),
                            "source_file": parsed.source_file,
                        })),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e),
                },
            }
        }

        SessionCommands::Chunks { session, limit } => {
            // Find the session file
            let path = match scanner.find_all_sessions() {
                Ok(sessions) => {
                    sessions.into_iter()
                        .find(|p| {
                            p.file_stem()
                                .and_then(|s| s.to_str())
                                .map_or(false, |s| s == session)
                        })
                }
                Err(_) => None,
            };

            let Some(path) = path else {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Session not found: {}", session)),
                };
            };

            match ChunkedSessionParser::parse_file(&path) {
                Ok(chunked) => {
                    let chunks: Vec<serde_json::Value> = chunked.chunks.iter()
                        .take(*limit)
                        .map(|chunk| {
                            serde_json::json!({
                                "id": chunk.id,
                                "index": chunk.chunk_index,
                                "user_preview": chunk.user_content.chars().take(200).collect::<String>(),
                                "assistant_preview": chunk.assistant_content.chars().take(200).collect::<String>(),
                                "files_read": chunk.files_read,
                                "files_modified": chunk.files_modified,
                                "all_files": chunk.all_files,
                                "tools_used": chunk.tools_used,
                                "timestamp": chunk.timestamp.map(|t| t.to_rfc3339()),
                            })
                        })
                        .collect();

                    CliResponse {
                        success: true,
                        data: Some(serde_json::json!({
                            "session_id": chunked.id,
                            "total_chunks": chunked.chunks.len(),
                            "total_files": chunked.all_files.len(),
                            "chunks": chunks,
                        })),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e),
                },
            }
        }

        SessionCommands::Files { pattern, limit } => {
            let pattern_lower = pattern.to_lowercase();
            let mut results: Vec<serde_json::Value> = Vec::new();

            match scanner.find_all_sessions() {
                Ok(sessions) => {
                    for path in sessions {
                        if results.len() >= *limit {
                            break;
                        }

                        if let Ok(chunked) = ChunkedSessionParser::parse_file(&path) {
                            // Check if any file matches the pattern
                            let matching_files: Vec<&String> = chunked.all_files.iter()
                                .filter(|f| f.to_lowercase().contains(&pattern_lower))
                                .collect();

                            if !matching_files.is_empty() {
                                // Find chunks that touched these files
                                let matching_chunks: Vec<serde_json::Value> = chunked.chunks.iter()
                                    .filter(|chunk| {
                                        chunk.all_files.iter().any(|f| f.to_lowercase().contains(&pattern_lower))
                                    })
                                    .take(3)
                                    .map(|chunk| {
                                        serde_json::json!({
                                            "index": chunk.chunk_index,
                                            "files": chunk.all_files.iter()
                                                .filter(|f| f.to_lowercase().contains(&pattern_lower))
                                                .collect::<Vec<_>>(),
                                            "user_preview": chunk.user_content.chars().take(100).collect::<String>(),
                                        })
                                    })
                                    .collect();

                                results.push(serde_json::json!({
                                    "session_id": chunked.id,
                                    "project": chunked.project_path,
                                    "branch": chunked.git_branch,
                                    "matching_files": matching_files,
                                    "matching_chunks": matching_chunks,
                                }));
                            }
                        }
                    }

                    CliResponse {
                        success: true,
                        data: Some(serde_json::json!({
                            "pattern": pattern,
                            "results": results,
                            "total": results.len(),
                        })),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e),
                },
            }
        }

        SessionCommands::Import { session, embed, legacy } => {
            let sessions_to_import: Vec<PathBuf> = if session == "all" {
                scanner.find_all_sessions().unwrap_or_default()
            } else {
                match scanner.find_all_sessions() {
                    Ok(sessions) => {
                        sessions.into_iter()
                            .filter(|p| {
                                p.file_stem()
                                    .and_then(|s| s.to_str())
                                    .map_or(false, |s| s == session)
                            })
                            .collect()
                    }
                    Err(_) => vec![],
                }
            };

            if sessions_to_import.is_empty() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some(format!("No sessions found matching: {}", session)),
                };
            }

            let mut imported_objects = 0;
            let mut imported_sessions = 0;
            let mut failed = 0;

            for path in sessions_to_import {
                if *legacy {
                    // Legacy mode: one object per session
                    match SessionParser::parse_file(&path) {
                        Ok(parsed) => {
                            let objects = session_to_objects(&parsed);
                            for obj in objects {
                                let suid = obj.suid;
                                let content_for_embed: Option<String> = obj.content_as_str().map(|s| s.to_string());
                                let store = search.store.blocking_write();
                                match store.create(&obj) {
                                    Ok(()) => {
                                        drop(store);
                                        if *embed {
                                            if let Some(content) = content_for_embed {
                                                if let Ok(embedding) = rt.block_on(search.embeddings().embed(&content)) {
                                                    let store = search.store.blocking_write();
                                                    let model = search.embeddings().model_info().id.clone();
                                                    let _ = store.store_embedding(&suid, &embedding, &model);
                                                }
                                            }
                                        }
                                        imported_objects += 1;
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to import: {}", e);
                                        failed += 1;
                                    }
                                }
                            }
                            imported_sessions += 1;
                        }
                        Err(e) => {
                            log::warn!("Failed to parse session {}: {}", path.display(), e);
                            failed += 1;
                        }
                    }
                } else {
                    // Chunked mode: one object per chunk + session summary
                    match ChunkedSessionParser::parse_file(&path) {
                        Ok(chunked) => {
                            let objects = chunked_session_to_objects(&chunked);
                            for obj in objects {
                                let suid = obj.suid;
                                // Use embedding_text from metadata if available, otherwise content
                                let embed_text: Option<String> = obj.metadata
                                    .get("embedding_text")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .or_else(|| obj.content_as_str().map(|s| s.to_string()));

                                let store = search.store.blocking_write();
                                match store.create(&obj) {
                                    Ok(()) => {
                                        drop(store);
                                        if *embed {
                                            if let Some(content) = embed_text {
                                                if let Ok(embedding) = rt.block_on(search.embeddings().embed(&content)) {
                                                    let store = search.store.blocking_write();
                                                    let model = search.embeddings().model_info().id.clone();
                                                    let _ = store.store_embedding(&suid, &embedding, &model);
                                                }
                                            }
                                        }
                                        imported_objects += 1;
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to import chunk: {}", e);
                                        failed += 1;
                                    }
                                }
                            }
                            imported_sessions += 1;
                        }
                        Err(e) => {
                            log::warn!("Failed to parse session {}: {}", path.display(), e);
                            failed += 1;
                        }
                    }
                }
            }

            CliResponse {
                success: true,
                data: Some(serde_json::json!({
                    "imported_sessions": imported_sessions,
                    "imported_objects": imported_objects,
                    "failed": failed,
                    "mode": if *legacy { "legacy" } else { "chunked" },
                    "message": format!("Imported {} objects from {} sessions", imported_objects, imported_sessions),
                })),
                error: None,
            }
        }

        SessionCommands::Search { query, limit } => {
            // Simple text search across sessions (without embeddings)
            let mut results: Vec<serde_json::Value> = Vec::new();
            let query_lower = query.to_lowercase();

            match scanner.find_all_sessions() {
                Ok(sessions) => {
                    for path in sessions {
                        if results.len() >= *limit {
                            break;
                        }

                        match SessionParser::parse_file(&path) {
                            Ok(parsed) => {
                                // Search in messages
                                let full_text = parsed.get_full_text().to_lowercase();
                                if full_text.contains(&query_lower) {
                                    // Find matching excerpts
                                    let excerpts: Vec<String> = parsed.user_messages.iter()
                                        .chain(parsed.assistant_messages.iter())
                                        .filter(|m| m.content.to_lowercase().contains(&query_lower))
                                        .take(3)
                                        .map(|m| {
                                            let preview: String = m.content.chars().take(200).collect();
                                            format!("[{}]: {}...", m.role, preview)
                                        })
                                        .collect();

                                    results.push(serde_json::json!({
                                        "id": parsed.id,
                                        "project": parsed.project_path,
                                        "branch": parsed.git_branch,
                                        "message_count": parsed.message_count,
                                        "excerpts": excerpts,
                                    }));
                                }
                            }
                            Err(_) => continue,
                        }
                    }

                    CliResponse {
                        success: true,
                        data: Some(serde_json::json!({
                            "query": query,
                            "results": results,
                            "total": results.len(),
                        })),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e),
                },
            }
        }

        SessionCommands::Stats => {
            match scanner.find_all_sessions() {
                Ok(sessions) => {
                    let mut total_messages = 0;
                    let mut total_user = 0;
                    let mut total_assistant = 0;
                    let mut projects: std::collections::HashSet<String> = std::collections::HashSet::new();

                    for path in &sessions {
                        if let Ok(parsed) = SessionParser::parse_file(path) {
                            total_messages += parsed.message_count;
                            total_user += parsed.user_messages.len();
                            total_assistant += parsed.assistant_messages.len();
                            if let Some(proj) = parsed.project_path {
                                projects.insert(proj);
                            }
                        }
                    }

                    CliResponse {
                        success: true,
                        data: Some(serde_json::json!({
                            "total_sessions": sessions.len(),
                            "total_messages": total_messages,
                            "total_user_messages": total_user,
                            "total_assistant_messages": total_assistant,
                            "unique_projects": projects.len(),
                            "projects": projects.into_iter().collect::<Vec<_>>(),
                        })),
                        error: None,
                    }
                }
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e),
                },
            }
        }
    }
}

fn run_andor_command(command: &AndorCommands) -> CliResponse<serde_json::Value> {
    let client = AndorClient::new();

    match command {
        AndorCommands::Status => {
            let running = client.is_running();
            CliResponse {
                success: true,
                data: Some(serde_json::json!({
                    "running": running,
                    "url": "http://localhost:8080"
                })),
                error: None,
            }
        }

        AndorCommands::Sessions { status, session_type, repo, limit } => {
            if !client.is_running() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some("Andor Hub is not running".to_string()),
                };
            }

            match client.list_sessions(
                status.as_deref(),
                session_type.as_deref(),
                repo.as_deref(),
                Some(*limit),
            ) {
                Ok(response) => CliResponse {
                    success: true,
                    data: Some(serde_json::to_value(response).unwrap()),
                    error: None,
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        AndorCommands::Session { id } => {
            if !client.is_running() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some("Andor Hub is not running".to_string()),
                };
            }

            match client.get_session(id) {
                Ok(session) => CliResponse {
                    success: true,
                    data: Some(serde_json::to_value(session).unwrap()),
                    error: None,
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        AndorCommands::SessionStats => {
            if !client.is_running() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some("Andor Hub is not running".to_string()),
                };
            }

            match client.session_stats() {
                Ok(stats) => CliResponse {
                    success: true,
                    data: Some(serde_json::to_value(stats).unwrap()),
                    error: None,
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        AndorCommands::Memory { query, namespace, memory_type, limit, mode } => {
            if !client.is_running() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some("Andor Hub is not running".to_string()),
                };
            }

            let request = MemoryQueryRequest {
                query: query.clone(),
                namespace: namespace.clone(),
                memory_type: memory_type.clone(),
                limit: Some(*limit),
                mode: Some(mode.clone()),
            };

            match client.query_memories(request) {
                Ok(response) => CliResponse {
                    success: true,
                    data: Some(serde_json::to_value(response).unwrap()),
                    error: None,
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        AndorCommands::MemoryStats => {
            if !client.is_running() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some("Andor Hub is not running".to_string()),
                };
            }

            match client.memory_stats() {
                Ok(stats) => CliResponse {
                    success: true,
                    data: Some(serde_json::to_value(stats).unwrap()),
                    error: None,
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        AndorCommands::SessionMemories { session_id } => {
            if !client.is_running() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some("Andor Hub is not running".to_string()),
                };
            }

            match client.session_memories(session_id) {
                Ok(memories) => CliResponse {
                    success: true,
                    data: Some(serde_json::to_value(memories).unwrap()),
                    error: None,
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        AndorCommands::Repos => {
            if !client.is_running() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some("Andor Hub is not running".to_string()),
                };
            }

            match client.list_repos() {
                Ok(repos) => CliResponse {
                    success: true,
                    data: Some(serde_json::to_value(repos).unwrap()),
                    error: None,
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        AndorCommands::Context { query, repo, limit } => {
            if !client.is_running() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some("Andor Hub is not running".to_string()),
                };
            }

            let request = ContextSearchRequest {
                query: query.clone(),
                repo_name: repo.clone(),
                limit: Some(*limit),
            };

            match client.search_context(request) {
                Ok(response) => CliResponse {
                    success: true,
                    data: Some(serde_json::to_value(response).unwrap()),
                    error: None,
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }

        AndorCommands::Search { query, limit, min_score } => {
            if !client.is_running() {
                return CliResponse {
                    success: false,
                    data: None,
                    error: Some("Andor Hub is not running".to_string()),
                };
            }

            let request = SemanticSearchRequest {
                query: query.clone(),
                limit: Some(*limit),
                min_score: *min_score,
            };

            match client.semantic_search(request) {
                Ok(response) => CliResponse {
                    success: true,
                    data: Some(serde_json::to_value(response).unwrap()),
                    error: None,
                },
                Err(e) => CliResponse {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        }
    }
}

fn get_database_path() -> PathBuf {
    // Try standard Tauri app data location
    if let Some(data_dir) = dirs::data_dir() {
        let app_dir = data_dir.join("com.marlos.app");
        if app_dir.exists() {
            return app_dir.join("objects.db");
        }
        // Create if it doesn't exist
        if std::fs::create_dir_all(&app_dir).is_ok() {
            return app_dir.join("objects.db");
        }
    }

    // Fall back to current directory
    PathBuf::from("objects.db")
}

fn check_lm_studio_running() -> bool {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    client
        .get("http://localhost:1234/v1/models")
        .send()
        .is_ok()
}

fn ensure_lm_studio_running() -> Result<(), String> {
    // Check if already running
    if check_lm_studio_running() {
        log::info!("LM Studio already running");
        return Ok(());
    }

    log::info!("Starting LM Studio server...");

    // Try to start LM Studio server using lms CLI
    let result = Command::new("lms")
        .args(["server", "start"])
        .spawn();

    match result {
        Ok(_) => {
            // Wait for server to be ready
            for _ in 0..30 {
                thread::sleep(Duration::from_secs(1));
                if check_lm_studio_running() {
                    log::info!("LM Studio server started");

                    // Try to load the embedding model
                    let _ = load_embedding_model();
                    return Ok(());
                }
            }
            Err("LM Studio server did not start in time".to_string())
        }
        Err(e) => {
            Err(format!("Failed to start LM Studio: {}. Is 'lms' CLI installed?", e))
        }
    }
}

fn load_embedding_model() -> Result<(), String> {
    log::info!("Loading embedding model...");

    // Try to load nomic-embed-text (common embedding model)
    let models = ["nomic-embed-text", "text-embedding-nomic-embed-text-v1.5"];

    for model in models {
        let result = Command::new("lms")
            .args(["load", model])
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    log::info!("Loaded model: {}", model);
                    // Wait a moment for model to initialize
                    thread::sleep(Duration::from_secs(2));
                    return Ok(());
                }
            }
            Err(_) => continue,
        }
    }

    Err("Could not load any embedding model".to_string())
}

fn parse_tier(s: &str) -> Option<SecurityTier> {
    match s.to_lowercase().as_str() {
        "open" => Some(SecurityTier::Open),
        "guarded" => Some(SecurityTier::Guarded),
        "sealed" => Some(SecurityTier::Sealed),
        _ => None,
    }
}

fn parse_content_type(s: &str) -> Option<ContentType> {
    match s.to_lowercase().as_str() {
        "text" => Some(ContentType::Text),
        "markdown" | "md" => Some(ContentType::Markdown),
        "json" => Some(ContentType::Json),
        "code" => Some(ContentType::Code { language: "unknown".to_string() }),
        _ => None,
    }
}

fn parse_relation_type(s: &str) -> RelationType {
    match s.to_lowercase().as_str() {
        "references" => RelationType::References,
        "contains" => RelationType::Contains,
        "derived_from" | "derivedfrom" => RelationType::DerivedFrom,
        "related_to" | "relatedto" => RelationType::RelatedTo,
        "version_of" | "versionof" => RelationType::VersionOf,
        "replies_to" | "repliesto" => RelationType::RepliesTo,
        "depends_on" | "dependson" => RelationType::DependsOn,
        _ => RelationType::Custom(s.to_string()),
    }
}

fn object_to_info(obj: &SemanticObject) -> ObjectInfo {
    let content_str = obj.content_as_str()
        .unwrap_or("")
        .to_string();

    ObjectInfo {
        suid: obj.suid.to_string(),
        name: obj.name.clone(),
        content_type: format!("{:?}", obj.content_type),
        tier: format!("{:?}", obj.security_tier),
        content: content_str,
        size_bytes: obj.size_bytes,
        created_at: obj.created_at.to_rfc3339(),
        modified_at: obj.modified_at.to_rfc3339(),
        version: obj.version,
        tags: obj.tags.clone(),
        summary: obj.summary.clone(),
    }
}
