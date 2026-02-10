//! MCP (Model Context Protocol) Server for MarlOS
//!
//! This module implements an MCP server that exposes tools for LLMs to use.
//! The LLM can call these tools to search the web, fetch pages, and interact
//! with the user's research context.
//!
//! # Architecture
//!
//! ```text
//! LLM Request
//!     │
//!     ▼
//! ┌─────────────────┐
//! │   MCP Server    │
//! │  ┌───────────┐  │
//! │  │  Tools:   │  │
//! │  │ - search  │  │
//! │  │ - fetch   │  │
//! │  │ - context │  │
//! │  └───────────┘  │
//! └─────────────────┘
//!     │
//!     ▼
//! Tool Results
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod tools;
pub mod web_search;
pub mod memory_server;

pub use tools::*;
pub use web_search::*;
pub use memory_server::*;

// ============================================================================
// MCP Protocol Types
// ============================================================================

/// A tool that can be called by an LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: ToolParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameters {
    #[serde(rename = "type")]
    pub param_type: String,
    pub properties: HashMap<String, ParameterProperty>,
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterProperty {
    #[serde(rename = "type")]
    pub prop_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

/// A request to call a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

/// Result of a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(content: String) -> Self {
        Self {
            success: true,
            content,
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            content: String::new(),
            error: Some(message),
        }
    }
}

// ============================================================================
// MCP Server
// ============================================================================

/// The MCP server that manages tools and executes them
pub struct McpServer {
    tools: HashMap<String, Tool>,
}

impl McpServer {
    pub fn new() -> Self {
        let mut server = Self {
            tools: HashMap::new(),
        };
        server.register_default_tools();
        server
    }

    fn register_default_tools(&mut self) {
        // Web Search tool
        self.register_tool(Tool {
            name: "web_search".to_string(),
            description: "Search the web for information. Returns a list of relevant results with titles, URLs, and snippets.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("query".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The search query".to_string(),
                        enum_values: None,
                    }),
                    ("num_results".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Number of results to return (default: 10, max: 20)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["query".to_string()],
            },
        });

        // Fetch Page tool
        self.register_tool(Tool {
            name: "fetch_page".to_string(),
            description: "Fetch and extract the main content from a web page. Returns the page title, text content, and metadata.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("url".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The URL to fetch".to_string(),
                        enum_values: None,
                    }),
                    ("extract_links".to_string(), ParameterProperty {
                        prop_type: "boolean".to_string(),
                        description: "Whether to extract links from the page (default: false)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["url".to_string()],
            },
        });

        // Get Research Context tool
        self.register_tool(Tool {
            name: "get_research_context".to_string(),
            description: "Get the user's current research context including collected sources, notes, and research topic.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::new(),
                required: vec![],
            },
        });

        // Add Source tool
        self.register_tool(Tool {
            name: "add_source".to_string(),
            description: "Add a discovered source to the user's research collection.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("url".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The URL of the source".to_string(),
                        enum_values: None,
                    }),
                    ("title".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The title of the source".to_string(),
                        enum_values: None,
                    }),
                    ("summary".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "A brief summary of why this source is relevant".to_string(),
                        enum_values: None,
                    }),
                    ("tags".to_string(), ParameterProperty {
                        prop_type: "array".to_string(),
                        description: "Tags to categorize the source".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["url".to_string(), "title".to_string()],
            },
        });
    }

    pub fn register_tool(&mut self, tool: Tool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    pub fn list_tools(&self) -> Vec<&Tool> {
        self.tools.values().collect()
    }

    pub fn get_tool(&self, name: &str) -> Option<&Tool> {
        self.tools.get(name)
    }

    /// Execute a tool call
    pub async fn execute(&self, call: ToolCall, context: &McpContext) -> ToolResult {
        log::info!("MCP executing tool: {} with args: {:?}", call.name, call.arguments);

        match call.name.as_str() {
            "web_search" => self.execute_web_search(call.arguments).await,
            "fetch_page" => self.execute_fetch_page(call.arguments).await,
            "get_research_context" => self.execute_get_context(context).await,
            "add_source" => self.execute_add_source(call.arguments, context).await,
            _ => ToolResult::error(format!("Unknown tool: {}", call.name)),
        }
    }

    async fn execute_web_search(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("Missing required parameter: query".to_string()),
        };

        let num_results = args.get("num_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        match web_search::search(query, num_results.min(20)).await {
            Ok(results) => {
                let content = serde_json::to_string_pretty(&results)
                    .unwrap_or_else(|_| "Failed to serialize results".to_string());
                ToolResult::success(content)
            }
            Err(e) => ToolResult::error(format!("Search failed: {}", e)),
        }
    }

    async fn execute_fetch_page(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let url = match args.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return ToolResult::error("Missing required parameter: url".to_string()),
        };

        let extract_links = args.get("extract_links")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        match web_search::fetch_page(url, extract_links).await {
            Ok(page) => {
                let content = serde_json::to_string_pretty(&page)
                    .unwrap_or_else(|_| "Failed to serialize page".to_string());
                ToolResult::success(content)
            }
            Err(e) => ToolResult::error(format!("Fetch failed: {}", e)),
        }
    }

    async fn execute_get_context(&self, context: &McpContext) -> ToolResult {
        let content = serde_json::to_string_pretty(context)
            .unwrap_or_else(|_| "Failed to serialize context".to_string());
        ToolResult::success(content)
    }

    async fn execute_add_source(&self, args: HashMap<String, serde_json::Value>, _context: &McpContext) -> ToolResult {
        let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let summary = args.get("summary").and_then(|v| v.as_str());
        let tags: Vec<String> = args.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        if url.is_empty() || title.is_empty() {
            return ToolResult::error("URL and title are required".to_string());
        }

        // Return the source info - the actual storage will be handled by the command layer
        let source_info = serde_json::json!({
            "url": url,
            "title": title,
            "summary": summary,
            "tags": tags,
            "status": "pending_add"
        });

        ToolResult::success(serde_json::to_string_pretty(&source_info).unwrap())
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// MCP Context
// ============================================================================

/// Context passed to tools during execution
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpContext {
    /// Current research topic/description
    pub research_topic: Option<String>,
    /// Existing sources in the collection
    pub sources: Vec<ContextSource>,
    /// User notes
    pub notes: Option<String>,
    /// Current document content (if any)
    pub document_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSource {
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
}

// ============================================================================
// Agent Loop
// ============================================================================

/// An agent that uses the MCP server to accomplish research tasks
pub struct ResearchAgent<'a> {
    mcp: &'a McpServer,
    ai_manager: &'a crate::ai::AiManager,
    max_iterations: usize,
}

impl<'a> ResearchAgent<'a> {
    pub fn new(mcp: &'a McpServer, ai_manager: &'a crate::ai::AiManager) -> Self {
        Self {
            mcp,
            ai_manager,
            max_iterations: 10,
        }
    }

    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Run the agent to accomplish a research task
    /// This uses a simplified single-shot approach that works better with local LLMs
    pub async fn run(&self, task: &str, context: &McpContext) -> Result<AgentResult, String> {
        log::info!("Starting research for: {}", task);

        // Step 1: Generate search queries using LLM
        let query_prompt = format!(
            r#"Generate 2-3 search queries to research this topic: "{}"

Respond with just the search queries, one per line. Be specific and focused."#,
            task
        );

        let queries_response = self.ai_manager
            .generate(&query_prompt, Some("You are a research assistant. Generate effective search queries."))
            .await
            .map_err(|e| format!("Failed to generate queries: {}", e))?;

        // Parse queries (one per line)
        let queries: Vec<&str> = queries_response.content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && l.len() > 5)
            .take(3)
            .collect();

        log::info!("Generated {} search queries", queries.len());

        // Step 2: Perform web searches
        let mut all_results = Vec::new();
        for query in &queries {
            log::info!("Searching: {}", query);
            match web_search::search(query, 5).await {
                Ok(results) => {
                    for r in results.results {
                        all_results.push(r);
                    }
                }
                Err(e) => {
                    log::warn!("Search failed for '{}': {}", query, e);
                }
            }
        }

        // Deduplicate by URL
        let mut seen_urls = std::collections::HashSet::new();
        all_results.retain(|r| seen_urls.insert(r.url.clone()));

        log::info!("Found {} unique results", all_results.len());

        if all_results.is_empty() {
            return Ok(AgentResult {
                summary: format!("No results found for: {}", task),
                sources: Vec::new(),
                iterations: 1,
            });
        }

        // Step 3: Have LLM analyze and summarize results
        let results_text = all_results.iter()
            .take(10)
            .enumerate()
            .map(|(i, r)| format!("{}. {} - {}\n   URL: {}", i+1, r.title, r.snippet, r.url))
            .collect::<Vec<_>>()
            .join("\n\n");

        let analysis_prompt = format!(
            r#"Research topic: "{}"

Search results:
{}

Analyze these results and provide:
1. A brief summary of what you found (2-3 sentences)
2. Which sources are most relevant and why

Keep your response concise."#,
            task, results_text
        );

        let analysis = self.ai_manager
            .generate(&analysis_prompt, Some("You are a research assistant. Analyze search results objectively."))
            .await
            .map_err(|e| format!("Failed to analyze results: {}", e))?;

        // Build the result
        let sources: Vec<DiscoveredSource> = all_results.into_iter()
            .take(10)
            .map(|r| DiscoveredSource {
                url: r.url,
                title: r.title,
                relevance: r.snippet,
                authors: None,
                year: None,
                citation_count: None,
                pdf_url: None,
                doi: None,
                venue: None,
                source_type: Some("web".to_string()),
            })
            .collect();

        Ok(AgentResult {
            summary: analysis.content,
            sources,
            iterations: 1,
        })
    }

    /// Run academic research using scholarly sources (Semantic Scholar, arXiv)
    pub async fn run_academic(&self, task: &str, _context: &McpContext) -> Result<AgentResult, String> {
        log::info!("Starting academic research for: {}", task);

        // Step 1: Generate academic search queries using LLM
        let query_prompt = format!(
            r#"Generate 2-3 academic search queries for researching: "{}"

Focus on finding scholarly papers, research articles, and academic publications.
Respond with just the search queries, one per line. Be specific and use academic terminology."#,
            task
        );

        let queries_response = self.ai_manager
            .generate(&query_prompt, Some("You are an academic research assistant. Generate scholarly search queries."))
            .await
            .map_err(|e| format!("Failed to generate queries: {}", e))?;

        // Parse queries (one per line)
        let queries: Vec<&str> = queries_response.content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && l.len() > 5)
            .take(3)
            .collect();

        log::info!("Generated {} academic search queries", queries.len());

        // Step 2: Perform academic searches
        let mut all_papers = Vec::new();
        for query in &queries {
            log::info!("Academic search: {}", query);
            match web_search::search_academic(query, 5).await {
                Ok(results) => {
                    for paper in results.papers {
                        all_papers.push(paper);
                    }
                }
                Err(e) => {
                    log::warn!("Academic search failed for '{}': {}", query, e);
                }
            }
        }

        // Deduplicate by title
        let mut seen_titles = std::collections::HashSet::new();
        all_papers.retain(|p| {
            let normalized = p.title.to_lowercase().replace(|c: char| !c.is_alphanumeric(), "");
            seen_titles.insert(normalized)
        });

        // Sort by citation count
        all_papers.sort_by(|a, b| {
            b.citation_count.unwrap_or(0).cmp(&a.citation_count.unwrap_or(0))
        });

        log::info!("Found {} unique academic papers", all_papers.len());

        if all_papers.is_empty() {
            return Ok(AgentResult {
                summary: format!("No academic papers found for: {}", task),
                sources: Vec::new(),
                iterations: 1,
            });
        }

        // Step 3: Have LLM analyze and summarize papers
        let papers_text = all_papers.iter()
            .take(10)
            .enumerate()
            .map(|(i, p)| {
                let authors = p.authors.join(", ");
                let year = p.year.map(|y| y.to_string()).unwrap_or_else(|| "n.d.".to_string());
                let citations = p.citation_count.map(|c| format!("{} citations", c)).unwrap_or_default();
                let venue = p.venue.as_deref().unwrap_or("");
                let abstract_preview = p.abstract_text.as_deref()
                    .map(|a| if a.len() > 200 { format!("{}...", &a[..200]) } else { a.to_string() })
                    .unwrap_or_default();
                format!("{}. {} ({})\n   Authors: {}\n   Venue: {} {}\n   Abstract: {}\n   URL: {}",
                    i+1, p.title, year, authors, venue, citations, abstract_preview, p.url)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let analysis_prompt = format!(
            r#"Research topic: "{}"

Academic papers found:
{}

Provide a brief synthesis of the research:
1. Main findings and themes across these papers (2-3 sentences)
2. Which papers are most relevant and why
3. Any notable gaps or areas needing more research

Keep your response concise and academic in tone."#,
            task, papers_text
        );

        let analysis = self.ai_manager
            .generate(&analysis_prompt, Some("You are an academic research assistant. Synthesize scholarly literature."))
            .await
            .map_err(|e| format!("Failed to analyze papers: {}", e))?;

        // Build the result
        let sources: Vec<DiscoveredSource> = all_papers.into_iter()
            .take(10)
            .map(|p| DiscoveredSource {
                url: p.url,
                title: p.title,
                relevance: p.abstract_text.unwrap_or_default(),
                authors: Some(p.authors),
                year: p.year,
                citation_count: p.citation_count,
                pdf_url: p.pdf_url,
                doi: p.doi,
                venue: p.venue,
                source_type: Some(p.source),
            })
            .collect();

        Ok(AgentResult {
            summary: analysis.content,
            sources,
            iterations: 1,
        })
    }

    /// Advanced agent loop (for capable models like GPT-4/Claude)
    /// This version uses tool calling which requires the LLM to follow a specific format
    pub async fn run_with_tools(&self, task: &str, context: &McpContext) -> Result<AgentResult, String> {
        let tools_description = self.build_tools_description();

        let system_prompt = format!(
            r#"You are a research assistant with access to web search tools. Your job is to help users find relevant sources for their research.

Available tools:
{}

When you need to use a tool, respond with a JSON block like this:
```tool
{{"name": "tool_name", "arguments": {{"arg1": "value1"}}}}
```

After using tools and gathering information, provide your final response with:
```result
{{"summary": "What you found", "sources": [{{"url": "...", "title": "...", "relevance": "..."}}]}}
```

Be thorough but efficient. Search for multiple perspectives. Evaluate source quality."#,
            tools_description
        );

        let mut messages = vec![
            format!("Research task: {}\n\nCurrent context:\n{}",
                task,
                serde_json::to_string_pretty(context).unwrap_or_default()
            )
        ];

        let mut iterations = 0;
        let mut all_sources = Vec::new();

        while iterations < self.max_iterations {
            iterations += 1;
            log::info!("Research agent iteration {}", iterations);

            let user_message = messages.join("\n\n---\n\n");

            let response = self.ai_manager
                .generate(&user_message, Some(&system_prompt))
                .await
                .map_err(|e| format!("AI error: {}", e))?;

            // Check for tool calls
            if let Some(tool_call) = self.extract_tool_call(&response.content) {
                log::info!("Agent calling tool: {}", tool_call.name);

                let result = self.mcp.execute(tool_call.clone(), context).await;

                messages.push(format!(
                    "Tool '{}' result:\n{}",
                    tool_call.name,
                    if result.success { &result.content } else { result.error.as_deref().unwrap_or("Unknown error") }
                ));

                continue;
            }

            // Check for final result
            if let Some(result) = self.extract_result(&response.content) {
                return Ok(result);
            }

            // No tool call or result - agent is done
            return Ok(AgentResult {
                summary: response.content,
                sources: all_sources,
                iterations,
            });
        }

        Ok(AgentResult {
            summary: "Reached maximum iterations".to_string(),
            sources: all_sources,
            iterations,
        })
    }

    fn build_tools_description(&self) -> String {
        self.mcp.list_tools()
            .iter()
            .map(|tool| {
                let params: Vec<String> = tool.parameters.properties.iter()
                    .map(|(name, prop)| format!("  - {}: {} ({})", name, prop.prop_type, prop.description))
                    .collect();
                format!(
                    "• {} - {}\n  Parameters:\n{}",
                    tool.name, tool.description, params.join("\n")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn extract_tool_call(&self, content: &str) -> Option<ToolCall> {
        // Look for ```tool blocks
        let start = content.find("```tool")?;
        let json_start = content[start..].find('{')? + start;
        let json_end = content[json_start..].find("```")
            .map(|i| json_start + i)
            .or_else(|| {
                // Find matching brace
                let mut depth = 0;
                for (i, c) in content[json_start..].char_indices() {
                    match c {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(json_start + i + 1);
                            }
                        }
                        _ => {}
                    }
                }
                None
            })?;

        let json_str = &content[json_start..json_end];
        serde_json::from_str(json_str).ok()
    }

    fn extract_result(&self, content: &str) -> Option<AgentResult> {
        let start = content.find("```result")?;
        let json_start = content[start..].find('{')? + start;
        let json_end = content[json_start..].find("```")
            .map(|i| json_start + i)?;

        let json_str = &content[json_start..json_end];
        serde_json::from_str(json_str).ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub summary: String,
    pub sources: Vec<DiscoveredSource>,
    #[serde(default)]
    pub iterations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSource {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub relevance: String,
    // Academic paper fields (optional)
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
    pub source_type: Option<String>, // "web", "semantic_scholar", "arxiv"
}
