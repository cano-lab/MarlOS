//! MarlOS MCP Memory Server
//!
//! A standalone MCP server that exposes MarlOS semantic memory to AI tools.
//!
//! # Usage
//!
//! Run as an MCP server for Claude Code or other MCP clients:
//!
//! ```bash
//! marlos-mcp
//! ```
//!
//! Or with a custom database path:
//!
//! ```bash
//! marlos-mcp --db-path /path/to/objects.db
//! ```
//!
//! # Configuration for Claude Code
//!
//! Add to your Claude Code MCP settings (~/.claude/claude_desktop_config.json):
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "marlos-memory": {
//!       "command": "marlos-mcp",
//!       "args": []
//!     }
//!   }
//! }
//! ```

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use clap::Parser;
use tokio::runtime::Runtime;

use marlos_lib::object_store::ObjectStore;
use marlos_lib::embeddings::EmbeddingManager;
use marlos_lib::semantic_search::SemanticSearch;
use marlos_lib::mcp::memory_server::{McpMemoryServer, JsonRpcRequest, JsonRpcResponse};

#[derive(Parser, Debug)]
#[command(name = "marlos-mcp")]
#[command(about = "MarlOS MCP Memory Server - Exposes semantic memory to AI tools")]
#[command(version)]
struct Args {
    /// Path to the database file
    #[arg(long, short = 'd')]
    db_path: Option<PathBuf>,

    /// Run in HTTP mode instead of stdio (for debugging)
    #[arg(long)]
    http: bool,

    /// HTTP port (only used with --http)
    #[arg(long, default_value = "3100")]
    port: u16,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, default_value = "warn")]
    log_level: String,
}

fn main() {
    let args = Args::parse();

    // Initialize logging
    std::env::set_var("RUST_LOG", &args.log_level);
    env_logger::init();

    // Create runtime
    let rt = Runtime::new().expect("Failed to create Tokio runtime");

    rt.block_on(async {
        // Determine database path
        let db_path = args.db_path.unwrap_or_else(|| {
            // Default to the standard MarlOS data directory
            // Tauri uses app_data_dir which maps to AppData\Roaming on Windows
            // Try Roaming first (where Tauri stores data), then Local as fallback
            let roaming_path = dirs::data_dir()
                .map(|d| d.join("com.marlos.app").join("objects.db"));
            let local_path = dirs::data_local_dir()
                .map(|d| d.join("com.marlos.app").join("objects.db"));

            if let Some(path) = roaming_path {
                if path.exists() {
                    return path;
                }
            }
            if let Some(path) = local_path {
                if path.exists() {
                    return path;
                }
            }

            // Fallback to roaming path (will error if not found)
            dirs::data_dir()
                .map(|d| d.join("com.marlos.app"))
                .unwrap_or_else(|| PathBuf::from("."))
                .join("objects.db")
        });

        log::info!("Using database: {:?}", db_path);

        // Check if database exists
        if !db_path.exists() {
            eprintln!("Error: Database not found at {:?}", db_path);
            eprintln!("Run the MarlOS app first to create the database, or specify a path with --db-path");
            std::process::exit(1);
        }

        // Initialize components
        let object_store = match ObjectStore::new(db_path) {
            Ok(store) => store,
            Err(e) => {
                eprintln!("Failed to open database: {}", e);
                std::process::exit(1);
            }
        };

        // Auto-detect embedding provider
        let embedding_manager = EmbeddingManager::auto_detect().await;
        log::info!("Using embedding model: {}", embedding_manager.model_info().name);

        // Create semantic search
        let semantic_search = Arc::new(SemanticSearch::new(object_store, embedding_manager));

        // Create MCP server
        let server = McpMemoryServer::new(semantic_search);

        if args.http {
            run_http_server(server, args.port).await;
        } else {
            run_stdio_server(server).await;
        }
    });
}

/// Run the MCP server over stdio (standard mode for MCP)
async fn run_stdio_server(server: McpMemoryServer) {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    log::info!("MarlOS MCP Memory Server starting (stdio mode)");

    for line in stdin.lock().lines() {
        match line {
            Ok(input) => {
                if input.trim().is_empty() {
                    continue;
                }

                log::debug!("Received: {}", input);

                // Parse JSON-RPC request
                let response = match serde_json::from_str::<JsonRpcRequest>(&input) {
                    Ok(request) => {
                        log::debug!("Handling method: {}", request.method);
                        server.handle_request(request).await
                    }
                    Err(e) => {
                        log::warn!("Failed to parse request: {}", e);
                        JsonRpcResponse::error(
                            serde_json::Value::Null,
                            -32700,
                            format!("Parse error: {}", e),
                        )
                    }
                };

                // Write response
                let response_json = serde_json::to_string(&response).unwrap();
                log::debug!("Sending: {}", response_json);

                if let Err(e) = writeln!(stdout, "{}", response_json) {
                    log::error!("Failed to write response: {}", e);
                    break;
                }
                if let Err(e) = stdout.flush() {
                    log::error!("Failed to flush stdout: {}", e);
                    break;
                }
            }
            Err(e) => {
                log::error!("Failed to read input: {}", e);
                break;
            }
        }
    }

    log::info!("MarlOS MCP Memory Server shutting down");
}

/// Run the MCP server over HTTP (for debugging/testing)
async fn run_http_server(server: McpMemoryServer, port: u16) {
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await.expect("Failed to bind");

    eprintln!("MarlOS MCP Memory Server listening on http://{}", addr);
    eprintln!("Tools available:");
    eprintln!("  - search_memory: Search semantic memory");
    eprintln!("  - get_context: Get context for current work");
    eprintln!("  - log_decision: Record a decision");
    eprintln!("  - get_decisions: Query past decisions");
    eprintln!("  - get_related: Find related content");
    eprintln!("  - get_memory_stats: Get memory statistics");

    let server = Arc::new(server);

    loop {
        let (mut socket, _) = listener.accept().await.expect("Failed to accept");
        let server = Arc::clone(&server);

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();

            // Read HTTP request (simplified - just read until empty line, then body)
            let mut content_length = 0usize;
            loop {
                line.clear();
                if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                    return;
                }
                if line.starts_with("Content-Length:") {
                    content_length = line.trim_start_matches("Content-Length:")
                        .trim()
                        .parse()
                        .unwrap_or(0);
                }
                if line == "\r\n" || line == "\n" {
                    break;
                }
            }

            // Read body
            let mut body = vec![0u8; content_length];
            if content_length > 0 {
                use tokio::io::AsyncReadExt;
                if reader.read_exact(&mut body).await.is_err() {
                    return;
                }
            }

            let body_str = String::from_utf8_lossy(&body);

            // Parse and handle request
            let response = match serde_json::from_str::<JsonRpcRequest>(&body_str) {
                Ok(request) => server.handle_request(request).await,
                Err(e) => JsonRpcResponse::error(
                    serde_json::Value::Null,
                    -32700,
                    format!("Parse error: {}", e),
                ),
            };

            let response_body = serde_json::to_string(&response).unwrap();
            let http_response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
                response_body.len(),
                response_body
            );

            let _ = writer.write_all(http_response.as_bytes()).await;
        });
    }
}
