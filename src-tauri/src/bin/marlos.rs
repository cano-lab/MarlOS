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
use marlos_lib::embeddings::EmbeddingManager;
use marlos_lib::memory::SecurityTier;
use marlos_lib::object_store::ObjectStore;
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
