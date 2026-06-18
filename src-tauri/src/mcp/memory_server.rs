//! MCP Memory Server - Exposes MarlOS semantic memory to external AI tools
//!
//! This module implements a standalone MCP server that allows AI tools like
//! Claude Code, Cursor, and others to query the user's semantic memory.
//!
//! # Protocol
//!
//! The server uses the Model Context Protocol (MCP) over stdio or HTTP.
//! Tools exposed:
//! - search_memory: Semantic search across all stored content
//! - get_context: Get relevant context for a file or project
//! - log_decision: Record a decision with reasoning
//! - get_decisions: Query past decisions
//! - get_related: Find content related to current work

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};

use crate::semantic_search::{SemanticSearch, SearchOptions};
use crate::semantic_object::{SemanticObject, Suid, ContentType};
use crate::memory::SecurityTier;

use super::{Tool, ToolParameters, ParameterProperty, ToolCall, ToolResult};

// ============================================================================
// Memory-Specific Types
// ============================================================================

/// A decision recorded in the memory system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub topic: String,
    pub choice: String,
    pub reasoning: String,
    pub alternatives: Vec<String>,
    pub project: Option<String>,
    pub file_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

/// Context retrieved for current work
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkContext {
    pub query: String,
    pub relevant_sessions: Vec<MemoryHit>,
    pub relevant_decisions: Vec<Decision>,
    pub related_files: Vec<String>,
    pub summary: Option<String>,
}

/// A hit from memory search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHit {
    pub id: String,
    pub name: String,
    pub content_preview: String,
    pub score: f32,
    pub source_type: String, // "session", "document", "decision", "research"
    pub created_at: DateTime<Utc>,
    pub project: Option<String>,
    pub tags: Vec<String>,
}

// ============================================================================
// Memory Tools Registration
// ============================================================================

/// Create all memory-related MCP tools
pub fn create_memory_tools() -> Vec<Tool> {
    vec![
        // search_memory tool
        Tool {
            name: "search_memory".to_string(),
            description: "Search the user's semantic memory for relevant past work, sessions, decisions, and research. Returns content semantically similar to the query.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("query".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The search query - can be a question, topic, or description of what you're looking for".to_string(),
                        enum_values: None,
                    }),
                    ("limit".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Maximum number of results to return (default: 10, max: 50)".to_string(),
                        enum_values: None,
                    }),
                    ("source_type".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Filter by source type: 'session', 'document', 'decision', 'research', or 'all' (default)".to_string(),
                        enum_values: Some(vec!["all".to_string(), "session".to_string(), "document".to_string(), "decision".to_string(), "research".to_string()]),
                    }),
                    ("project".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Filter by project name (optional)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["query".to_string()],
            },
        },

        // get_context tool
        Tool {
            name: "get_context".to_string(),
            description: "Get relevant context for the current work. Provide a file path, project name, or description of what you're working on. Returns related sessions, decisions, and knowledge.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("file_path".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Current file being worked on (optional)".to_string(),
                        enum_values: None,
                    }),
                    ("project".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Project name or path (optional)".to_string(),
                        enum_values: None,
                    }),
                    ("description".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Description of current task or what you need context for".to_string(),
                        enum_values: None,
                    }),
                    ("include_decisions".to_string(), ParameterProperty {
                        prop_type: "boolean".to_string(),
                        description: "Include past decisions related to this context (default: true)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec![],
            },
        },

        // log_decision tool
        Tool {
            name: "log_decision".to_string(),
            description: "Record a decision with its reasoning. This helps the AI remember why choices were made.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("topic".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "What the decision is about (e.g., 'authentication method', 'database schema')".to_string(),
                        enum_values: None,
                    }),
                    ("choice".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The choice that was made".to_string(),
                        enum_values: None,
                    }),
                    ("reasoning".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Why this choice was made".to_string(),
                        enum_values: None,
                    }),
                    ("alternatives".to_string(), ParameterProperty {
                        prop_type: "array".to_string(),
                        description: "Other options that were considered".to_string(),
                        enum_values: None,
                    }),
                    ("project".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Project this decision relates to".to_string(),
                        enum_values: None,
                    }),
                    ("file_path".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "File this decision relates to".to_string(),
                        enum_values: None,
                    }),
                    ("tags".to_string(), ParameterProperty {
                        prop_type: "array".to_string(),
                        description: "Tags for categorizing the decision".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["topic".to_string(), "choice".to_string(), "reasoning".to_string()],
            },
        },

        // get_decisions tool
        Tool {
            name: "get_decisions".to_string(),
            description: "Query past decisions. Filter by topic, project, or time range.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("topic".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Filter by decision topic (semantic search)".to_string(),
                        enum_values: None,
                    }),
                    ("project".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Filter by project name".to_string(),
                        enum_values: None,
                    }),
                    ("limit".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Maximum number of decisions to return (default: 10)".to_string(),
                        enum_values: None,
                    }),
                    ("days_back".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Only return decisions from the last N days".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec![],
            },
        },

        // get_related tool
        Tool {
            name: "get_related".to_string(),
            description: "Find content related to a specific piece of work. Provide an ID or content snippet to find semantically similar items.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("content".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Content to find related items for".to_string(),
                        enum_values: None,
                    }),
                    ("object_id".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "ID of an existing object to find related items for".to_string(),
                        enum_values: None,
                    }),
                    ("limit".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Maximum number of related items to return (default: 10)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec![],
            },
        },

        // get_stats tool
        Tool {
            name: "get_memory_stats".to_string(),
            description: "Get statistics about the memory system - total objects, sessions, decisions, etc.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::new(),
                required: vec![],
            },
        },

        // === Proactive Intelligence Tools ===

        // get_suggestions tool
        Tool {
            name: "get_suggestions".to_string(),
            description: "Get proactive suggestions based on current context. The system will surface relevant past work, decisions, and patterns.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("file_path".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Current file being worked on".to_string(),
                        enum_values: None,
                    }),
                    ("query".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Current query or task description".to_string(),
                        enum_values: None,
                    }),
                    ("project_path".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Path to current project".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec![],
            },
        },

        // check_decision_conflicts tool
        Tool {
            name: "check_decision_conflicts".to_string(),
            description: "Check if a proposed decision conflicts with past decisions. Use this before making important choices.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("topic".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The topic of the decision".to_string(),
                        enum_values: None,
                    }),
                    ("proposed_choice".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The choice you're considering".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["topic".to_string(), "proposed_choice".to_string()],
            },
        },

        // get_work_context tool
        Tool {
            name: "get_work_context".to_string(),
            description: "Get continuity context for resuming work. Returns what was being worked on, recent files, and pending notes.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("project_path".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Path to the project".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["project_path".to_string()],
            },
        },

        // === Research & Paper Writing Tools ===

        // search_sources tool
        Tool {
            name: "search_sources".to_string(),
            description: "Search research sources - PDFs, papers, documents, and notes in the knowledge base. Use this to find relevant sources for citations and research.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("query".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Search query - describe what you're looking for".to_string(),
                        enum_values: None,
                    }),
                    ("source_type".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Filter by type: 'pdf', 'paper', 'notes', 'web', or 'all'".to_string(),
                        enum_values: Some(vec!["all".to_string(), "pdf".to_string(), "paper".to_string(), "notes".to_string(), "web".to_string()]),
                    }),
                    ("limit".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Maximum results (default: 10)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["query".to_string()],
            },
        },

        // get_coding_sessions tool
        Tool {
            name: "get_coding_sessions".to_string(),
            description: "Get coding sessions from Claude Code, Cursor, or other AI coding tools. Search by project, topic, or time range. Useful for understanding past implementation decisions and code context.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("query".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Search query - describe what coding work you're looking for".to_string(),
                        enum_values: None,
                    }),
                    ("project".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Filter by project name".to_string(),
                        enum_values: None,
                    }),
                    ("provider".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Filter by AI provider: 'claude-code', 'cursor', 'chatgpt', or 'all'".to_string(),
                        enum_values: Some(vec!["all".to_string(), "claude-code".to_string(), "cursor".to_string(), "chatgpt".to_string()]),
                    }),
                    ("days_back".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Only return sessions from last N days".to_string(),
                        enum_values: None,
                    }),
                    ("limit".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Maximum results (default: 10)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec![],
            },
        },

        // get_citation tool
        Tool {
            name: "get_citation".to_string(),
            description: "Get a formatted citation for a source. Provide a source ID or search for it by title/author.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("source_id".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "ID of the source to cite".to_string(),
                        enum_values: None,
                    }),
                    ("title".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Title of the source to find and cite".to_string(),
                        enum_values: None,
                    }),
                    ("format".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Citation format: 'apa', 'mla', 'chicago', 'bibtex'".to_string(),
                        enum_values: Some(vec!["apa".to_string(), "mla".to_string(), "chicago".to_string(), "bibtex".to_string()]),
                    }),
                ]),
                required: vec![],
            },
        },

        // summarize_source tool
        Tool {
            name: "summarize_source".to_string(),
            description: "Get a summary of a source document. Provide a source ID or search query.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("source_id".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "ID of the source to summarize".to_string(),
                        enum_values: None,
                    }),
                    ("query".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Search for source by title/content".to_string(),
                        enum_values: None,
                    }),
                    ("focus".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "What aspect to focus the summary on".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec![],
            },
        },

        // save_draft tool
        Tool {
            name: "save_draft".to_string(),
            description: "Save paper content to a file. Creates or updates a markdown file with the paper content.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("path".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "File path to save to (will create directories if needed)".to_string(),
                        enum_values: None,
                    }),
                    ("content".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The paper content in markdown format".to_string(),
                        enum_values: None,
                    }),
                    ("section".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "If provided, only update this section of the paper".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["path".to_string(), "content".to_string()],
            },
        },

        // read_file tool
        Tool {
            name: "read_file".to_string(),
            description: "Read content from a file. Use this to read existing drafts or source documents.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("path".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "File path to read".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["path".to_string()],
            },
        },

        // list_sources tool
        Tool {
            name: "list_sources".to_string(),
            description: "List all sources in the research collection. Returns titles, types, and IDs for citation.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("source_type".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Filter by type: 'pdf', 'paper', 'notes', 'web', or 'all'".to_string(),
                        enum_values: Some(vec!["all".to_string(), "pdf".to_string(), "paper".to_string(), "notes".to_string(), "web".to_string()]),
                    }),
                    ("limit".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Maximum results (default: 50)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec![],
            },
        },

        // === Learning Tools ===
        // These tools implement the predict-test-compare-integrate learning algorithm

        // learning_start tool
        Tool {
            name: "learning_start".to_string(),
            description: "Start a learning session. Use this to guide a user through the scientific method of learning: predict, test, compare, integrate. First, get them to make a prediction before revealing the answer.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("topic".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The topic or question to learn about".to_string(),
                        enum_values: None,
                    }),
                    ("user_prediction".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The user's prediction/guess about the answer (required before revealing truth)".to_string(),
                        enum_values: None,
                    }),
                    ("confidence".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "User's confidence level in their prediction".to_string(),
                        enum_values: Some(vec!["confident".to_string(), "partial".to_string(), "guess".to_string(), "no_idea".to_string()]),
                    }),
                    ("use_web_search".to_string(), ParameterProperty {
                        prop_type: "boolean".to_string(),
                        description: "Whether to search the web for current information (default: false)".to_string(),
                        enum_values: None,
                    }),
                    ("use_academic_search".to_string(), ParameterProperty {
                        prop_type: "boolean".to_string(),
                        description: "Whether to search academic papers (default: false)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["topic".to_string()],
            },
        },

        // learning_compare tool
        Tool {
            name: "learning_compare".to_string(),
            description: "After revealing the answer, use this to compare the user's prediction against the truth. This analysis helps identify gaps in their mental model.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("topic".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The topic that was being learned".to_string(),
                        enum_values: None,
                    }),
                    ("prediction".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "What the user predicted".to_string(),
                        enum_values: None,
                    }),
                    ("actual_answer".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The actual correct answer".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["topic".to_string(), "prediction".to_string(), "actual_answer".to_string()],
            },
        },

        // learning_save tool
        Tool {
            name: "learning_save".to_string(),
            description: "Save a completed learning cycle to memory. This records what was learned for future reference.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("topic".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The topic that was learned".to_string(),
                        enum_values: None,
                    }),
                    ("prediction".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "What the user predicted".to_string(),
                        enum_values: None,
                    }),
                    ("actual_answer".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "The correct answer".to_string(),
                        enum_values: None,
                    }),
                    ("comparison".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Analysis of prediction vs reality".to_string(),
                        enum_values: None,
                    }),
                    ("user_integration".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "What the user said they learned in their own words".to_string(),
                        enum_values: None,
                    }),
                    ("followup_questions".to_string(), ParameterProperty {
                        prop_type: "array".to_string(),
                        description: "Follow-up questions for continued learning".to_string(),
                        enum_values: None,
                    }),
                    ("tags".to_string(), ParameterProperty {
                        prop_type: "array".to_string(),
                        description: "Tags to categorize this learning".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["topic".to_string(), "actual_answer".to_string()],
            },
        },

        // research_papers tool
        Tool {
            name: "research_papers".to_string(),
            description: "Search for academic papers on Semantic Scholar and arXiv. Returns titles, authors, abstracts, citation counts, and URLs. Use this to find papers on a topic, then use fetch_page to read full content.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("query".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Search query for academic papers".to_string(),
                        enum_values: None,
                    }),
                    ("num_results".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Number of results to return (default: 8, max: 20)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["query".to_string()],
            },
        },

        // web_search tool
        Tool {
            name: "web_search".to_string(),
            description: "Search the web using DuckDuckGo. Returns titles, URLs, and snippets. Use this for general web searches, blog posts, documentation, and non-academic content.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("query".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Search query".to_string(),
                        enum_values: None,
                    }),
                    ("num_results".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Number of results to return (default: 5, max: 15)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["query".to_string()],
            },
        },

        // fetch_page tool
        Tool {
            name: "fetch_page".to_string(),
            description: "Fetch and extract the main text content from a URL. Use this to read full articles, papers, blog posts, or documentation. Returns the extracted text, title, and metadata.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("url".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "URL to fetch and extract content from".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["url".to_string()],
            },
        },

        // learning_history tool
        Tool {
            name: "learning_history".to_string(),
            description: "Get the user's learning history. Use this to see what topics they've studied and how their understanding has evolved.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("topic".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Filter by topic (semantic search)".to_string(),
                        enum_values: None,
                    }),
                    ("days_back".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Only return learning from the last N days".to_string(),
                        enum_values: None,
                    }),
                    ("limit".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Maximum number of results (default: 10)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec![],
            },
        },

        // ====================================================================
        // Daily Research Automation tools
        // ====================================================================

        Tool {
            name: "list_research_topics".to_string(),
            description: "List all registered research topics for the daily research pipeline. Each topic has a name and one or more search queries.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::new(),
                required: vec![],
            },
        },

        Tool {
            name: "add_research_topic".to_string(),
            description: "Register a new research topic for the daily research pipeline. If queries are not provided, the topic name is used as the query.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("name".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Topic name (e.g., 'AI', 'Operating Systems'). Case-sensitive, used as the identifier.".to_string(),
                        enum_values: None,
                    }),
                    ("queries".to_string(), ParameterProperty {
                        prop_type: "array".to_string(),
                        description: "Optional list of search queries for this topic. Defaults to [name].".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["name".to_string()],
            },
        },

        Tool {
            name: "remove_research_topic".to_string(),
            description: "Remove a research topic from the daily research pipeline. Does not delete already-stored summaries.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("name".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Topic name to remove".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["name".to_string()],
            },
        },

        Tool {
            name: "daily_research_fetch".to_string(),
            description: "Fetch candidate academic papers for a topic, deduplicated against already-summarized papers. Returns candidates for the LLM to summarize and then store via store_research_summary.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("topic".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Topic name (must be a registered topic from list_research_topics)".to_string(),
                        enum_values: None,
                    }),
                    ("limit".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Maximum number of fresh candidates to return (default: 5)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["topic".to_string()],
            },
        },

        Tool {
            name: "store_research_summary".to_string(),
            description: "Store a summary of an academic paper under a topic. Dedup keys (doi, arxiv_id, url) are recorded as tags so future daily_research_fetch calls skip the same paper.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("topic".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Topic this summary belongs to".to_string(),
                        enum_values: None,
                    }),
                    ("title".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Paper title".to_string(),
                        enum_values: None,
                    }),
                    ("url".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Source URL".to_string(),
                        enum_values: None,
                    }),
                    ("summary".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "3-5 sentence summary of the paper's key findings and contributions".to_string(),
                        enum_values: None,
                    }),
                    ("doi".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "DOI if available (preferred dedup key)".to_string(),
                        enum_values: None,
                    }),
                    ("arxiv_id".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "arXiv ID if available (e.g. '2401.12345')".to_string(),
                        enum_values: None,
                    }),
                    ("authors".to_string(), ParameterProperty {
                        prop_type: "array".to_string(),
                        description: "List of author names".to_string(),
                        enum_values: None,
                    }),
                    ("year".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Publication year".to_string(),
                        enum_values: None,
                    }),
                    ("venue".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Journal or conference name".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec!["topic".to_string(), "title".to_string(), "url".to_string(), "summary".to_string()],
            },
        },

        Tool {
            name: "list_research_summaries".to_string(),
            description: "List stored research summaries, optionally filtered by topic and date range. Use this to pull accumulated research context when synthesizing papers.".to_string(),
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: HashMap::from([
                    ("topic".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "Filter by topic name (optional)".to_string(),
                        enum_values: None,
                    }),
                    ("since".to_string(), ParameterProperty {
                        prop_type: "string".to_string(),
                        description: "ISO date (YYYY-MM-DD) - only return summaries stored on or after this date".to_string(),
                        enum_values: None,
                    }),
                    ("limit".to_string(), ParameterProperty {
                        prop_type: "integer".to_string(),
                        description: "Maximum number of summaries to return (default: 50)".to_string(),
                        enum_values: None,
                    }),
                ]),
                required: vec![],
            },
        },
    ]
}

// ============================================================================
// Memory Tool Executor
// ============================================================================

/// Executor for memory tools
pub struct MemoryToolExecutor {
    search: Arc<SemanticSearch>,
}

impl MemoryToolExecutor {
    pub fn new(search: Arc<SemanticSearch>) -> Self {
        Self { search }
    }

    /// Execute a memory tool
    pub async fn execute(&self, call: ToolCall) -> ToolResult {
        match call.name.as_str() {
            "search_memory" => self.execute_search_memory(call.arguments).await,
            "get_context" => self.execute_get_context(call.arguments).await,
            "log_decision" => self.execute_log_decision(call.arguments).await,
            "get_decisions" => self.execute_get_decisions(call.arguments).await,
            "get_related" => self.execute_get_related(call.arguments).await,
            "get_memory_stats" => self.execute_get_stats().await,
            // Proactive intelligence tools
            "get_suggestions" => self.execute_get_suggestions(call.arguments).await,
            "check_decision_conflicts" => self.execute_check_conflicts(call.arguments).await,
            "get_work_context" => self.execute_get_work_context(call.arguments).await,
            // Research & paper writing tools
            "search_sources" => self.execute_search_sources(call.arguments).await,
            "get_coding_sessions" => self.execute_get_coding_sessions(call.arguments).await,
            "get_citation" => self.execute_get_citation(call.arguments).await,
            "summarize_source" => self.execute_summarize_source(call.arguments).await,
            "save_draft" => self.execute_save_draft(call.arguments).await,
            "read_file" => self.execute_read_file(call.arguments).await,
            "list_sources" => self.execute_list_sources(call.arguments).await,
            // Web & research tools
            "research_papers" => self.execute_research_papers(call.arguments).await,
            "web_search" => self.execute_web_search(call.arguments).await,
            "fetch_page" => self.execute_fetch_page(call.arguments).await,
            // Learning tools
            "learning_start" => self.execute_learning_start(call.arguments).await,
            "learning_compare" => self.execute_learning_compare(call.arguments).await,
            "learning_save" => self.execute_learning_save(call.arguments).await,
            "learning_history" => self.execute_learning_history(call.arguments).await,
            // Daily research automation tools
            "list_research_topics" => self.execute_list_research_topics().await,
            "add_research_topic" => self.execute_add_research_topic(call.arguments).await,
            "remove_research_topic" => self.execute_remove_research_topic(call.arguments).await,
            "daily_research_fetch" => self.execute_daily_research_fetch(call.arguments).await,
            "store_research_summary" => self.execute_store_research_summary(call.arguments).await,
            "list_research_summaries" => self.execute_list_research_summaries(call.arguments).await,
            _ => ToolResult::error(format!("Unknown memory tool: {}", call.name)),
        }
    }

    async fn execute_search_memory(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("Missing required parameter: query".to_string()),
        };

        let limit = args.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let source_type = args.get("source_type")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let project = args.get("project")
            .and_then(|v| v.as_str());

        // Build search options
        let options = SearchOptions {
            limit: limit.min(50),
            min_score: 0.3,
            max_tier: SecurityTier::Guarded, // Include Guarded tier (sessions, chunks)
            include_keyword: true,
            keyword_boost: 0.2,
            ..Default::default()
        };

        // Execute search
        match self.search.search(query, options).await {
            Ok(hits) => {
                // Filter by source type and project if specified
                let filtered: Vec<MemoryHit> = hits.into_iter()
                    .filter(|h| {
                        // Filter by source type
                        if source_type != "all" {
                            let obj_type = Self::get_source_type(&h.object);
                            if obj_type != source_type {
                                return false;
                            }
                        }
                        // Filter by project
                        if let Some(proj) = project {
                            if !h.object.tags.iter().any(|t| t.contains(proj)) {
                                return false;
                            }
                        }
                        true
                    })
                    .map(|h| MemoryHit {
                        id: h.object.suid.to_string(),
                        name: h.object.name.clone().unwrap_or_default(),
                        content_preview: h.object.content_as_str()
                            .map(|s| if s.len() > 500 { format!("{}...", &s[..500]) } else { s.to_string() })
                            .unwrap_or_default(),
                        score: h.score,
                        source_type: Self::get_source_type(&h.object),
                        created_at: h.object.created_at,
                        project: h.object.tags.iter()
                            .find(|t| !["session", "chunk", "decision", "research", "document"].contains(&t.as_str()))
                            .cloned(),
                        tags: h.object.tags.clone(),
                    })
                    .collect();

                let result = serde_json::json!({
                    "query": query,
                    "count": filtered.len(),
                    "results": filtered
                });

                ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => ToolResult::error(format!("Search failed: {}", e)),
        }
    }

    async fn execute_get_context(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let file_path = args.get("file_path").and_then(|v| v.as_str());
        let project = args.get("project").and_then(|v| v.as_str());
        let description = args.get("description").and_then(|v| v.as_str());
        let include_decisions = args.get("include_decisions")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Build a query from the context
        let query = match (file_path, project, description) {
            (Some(fp), Some(proj), _) => format!("{} {}", proj, fp),
            (Some(fp), None, _) => fp.to_string(),
            (None, Some(proj), _) => proj.to_string(),
            (None, None, Some(desc)) => desc.to_string(),
            (None, None, None) => {
                return ToolResult::error("Provide at least one of: file_path, project, or description".to_string());
            }
        };

        // Search for relevant sessions
        let options = SearchOptions {
            limit: 10,
            min_score: 0.3,
            max_tier: SecurityTier::Open,
            include_keyword: true,
            keyword_boost: 0.2,
            ..Default::default()
        };

        let sessions = match self.search.search(&query, options).await {
            Ok(hits) => hits.into_iter()
                .filter(|h| h.object.tags.contains(&"session".to_string()))
                .take(5)
                .map(|h| MemoryHit {
                    id: h.object.suid.to_string(),
                    name: h.object.name.clone().unwrap_or_default(),
                    content_preview: h.object.content_as_str()
                        .map(|s| if s.len() > 300 { format!("{}...", &s[..300]) } else { s.to_string() })
                        .unwrap_or_default(),
                    score: h.score,
                    source_type: "session".to_string(),
                    created_at: h.object.created_at,
                    project: project.map(String::from),
                    tags: h.object.tags.clone(),
                })
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };

        // Search for decisions if requested
        let decisions = if include_decisions {
            let decision_query = format!("decision {}", query);
            let options = SearchOptions {
                limit: 5,
                min_score: 0.3,
                max_tier: SecurityTier::Open,
                include_keyword: true,
                keyword_boost: 0.3,
                ..Default::default()
            };

            match self.search.search(&decision_query, options).await {
                Ok(hits) => hits.into_iter()
                    .filter(|h| h.object.tags.contains(&"decision".to_string()))
                    .filter_map(|h| {
                        // Try to parse decision from content
                        h.object.content_as_str()
                            .and_then(|s| serde_json::from_str::<Decision>(s).ok())
                    })
                    .take(5)
                    .collect::<Vec<_>>(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        let context = WorkContext {
            query: query.clone(),
            relevant_sessions: sessions,
            relevant_decisions: decisions,
            related_files: Vec::new(), // TODO: extract from sessions
            summary: None,
        };

        ToolResult::success(serde_json::to_string_pretty(&context).unwrap())
    }

    async fn execute_log_decision(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let topic = match args.get("topic").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("Missing required parameter: topic".to_string()),
        };

        let choice = match args.get("choice").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("Missing required parameter: choice".to_string()),
        };

        let reasoning = match args.get("reasoning").and_then(|v| v.as_str()) {
            Some(r) => r,
            None => return ToolResult::error("Missing required parameter: reasoning".to_string()),
        };

        let alternatives: Vec<String> = args.get("alternatives")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let project = args.get("project").and_then(|v| v.as_str()).map(String::from);
        let file_path = args.get("file_path").and_then(|v| v.as_str()).map(String::from);

        let tags: Vec<String> = args.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        // Create decision object
        let decision = Decision {
            id: Suid::new().to_string(),
            topic: topic.to_string(),
            choice: choice.to_string(),
            reasoning: reasoning.to_string(),
            alternatives,
            project: project.clone(),
            file_path,
            created_at: Utc::now(),
            tags: tags.clone(),
        };

        // Store as semantic object
        let content = serde_json::to_string(&decision).unwrap();
        let mut obj = SemanticObject::from_text(&content)
            .with_name(&format!("Decision: {}", topic))
            .with_tag("decision")
            .with_tier(SecurityTier::Open);

        // Add project tag if provided
        if let Some(proj) = &project {
            obj = obj.with_tag(proj);
        }

        // Add custom tags
        for tag in &tags {
            obj = obj.with_tag(tag);
        }

        match self.search.store(&obj).await {
            Ok(_) => {
                let result = serde_json::json!({
                    "success": true,
                    "decision_id": decision.id,
                    "message": format!("Decision logged: {} -> {}", topic, choice)
                });
                ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to store decision: {}", e)),
        }
    }

    async fn execute_get_decisions(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let topic = args.get("topic").and_then(|v| v.as_str());
        let project = args.get("project").and_then(|v| v.as_str());
        let limit = args.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;
        let days_back = args.get("days_back")
            .and_then(|v| v.as_u64())
            .map(|d| d as i64);

        // Build query
        let query = match topic {
            Some(t) => format!("decision {}", t),
            None => "decision".to_string(),
        };

        let options = SearchOptions {
            limit: limit.min(50),
            min_score: 0.2,
            max_tier: SecurityTier::Open,
            include_keyword: true,
            keyword_boost: 0.3,
            ..Default::default()
        };

        match self.search.search(&query, options).await {
            Ok(hits) => {
                let cutoff = days_back.map(|d| Utc::now() - chrono::Duration::days(d));

                let decisions: Vec<Decision> = hits.into_iter()
                    .filter(|h| h.object.tags.contains(&"decision".to_string()))
                    .filter(|h| {
                        // Filter by project
                        if let Some(proj) = project {
                            if !h.object.tags.contains(&proj.to_string()) {
                                return false;
                            }
                        }
                        // Filter by time
                        if let Some(cutoff) = cutoff {
                            if h.object.created_at < cutoff {
                                return false;
                            }
                        }
                        true
                    })
                    .filter_map(|h| {
                        h.object.content_as_str()
                            .and_then(|s| serde_json::from_str::<Decision>(s).ok())
                    })
                    .take(limit)
                    .collect();

                let result = serde_json::json!({
                    "count": decisions.len(),
                    "decisions": decisions
                });

                ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => ToolResult::error(format!("Search failed: {}", e)),
        }
    }

    async fn execute_get_related(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let content = args.get("content").and_then(|v| v.as_str());
        let object_id = args.get("object_id").and_then(|v| v.as_str());
        let limit = args.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let query = match (content, object_id) {
            (Some(c), _) => c.to_string(),
            (None, Some(id)) => {
                // Fetch the object and use its content
                match Suid::parse(id) {
                    Ok(suid) => {
                        match self.search.get(&suid).await {
                            Ok(Some(obj)) => obj.content_as_str()
                                .map(|s| s.to_string())
                                .unwrap_or_default(),
                            _ => return ToolResult::error(format!("Object not found: {}", id)),
                        }
                    }
                    Err(_) => return ToolResult::error(format!("Invalid object ID: {}", id)),
                }
            }
            (None, None) => {
                return ToolResult::error("Provide either content or object_id".to_string());
            }
        };

        let options = SearchOptions {
            limit: limit.min(50),
            min_score: 0.4,
            max_tier: SecurityTier::Open,
            include_keyword: true,
            keyword_boost: 0.1,
            ..Default::default()
        };

        match self.search.search(&query, options).await {
            Ok(hits) => {
                let related: Vec<MemoryHit> = hits.into_iter()
                    .map(|h| MemoryHit {
                        id: h.object.suid.to_string(),
                        name: h.object.name.clone().unwrap_or_default(),
                        content_preview: h.object.content_as_str()
                            .map(|s| if s.len() > 300 { format!("{}...", &s[..300]) } else { s.to_string() })
                            .unwrap_or_default(),
                        score: h.score,
                        source_type: Self::get_source_type(&h.object),
                        created_at: h.object.created_at,
                        project: None,
                        tags: h.object.tags.clone(),
                    })
                    .collect();

                let result = serde_json::json!({
                    "count": related.len(),
                    "related": related
                });

                ToolResult::success(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => ToolResult::error(format!("Search failed: {}", e)),
        }
    }

    async fn execute_get_stats(&self) -> ToolResult {
        // Get basic stats from the store
        let store = &self.search;

        // Count objects by type
        let options = SearchOptions {
            limit: 10000,
            min_score: 0.0,
            max_tier: SecurityTier::Open,
            include_keyword: false,
            keyword_boost: 0.0,
            ..Default::default()
        };

        // This is a rough approximation - in production we'd have dedicated count methods
        let stats = serde_json::json!({
            "status": "operational",
            "embedding_model": store.embeddings().model_info().name,
            "embedding_dimensions": store.embeddings().model_info().dimensions,
            "message": "Memory system is operational. Use search_memory to query content."
        });

        ToolResult::success(serde_json::to_string_pretty(&stats).unwrap())
    }

    // ========================================================================
    // Research & Paper Writing Tools
    // ========================================================================

    async fn execute_search_sources(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("Missing required parameter: query".to_string()),
        };

        let source_type = args.get("source_type")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let limit = args.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        // Search for research sources
        // Use higher limit since we filter to kind:source afterwards
        let options = SearchOptions {
            limit: 200, // Higher limit to ensure sources appear after filtering
            min_score: 0.2,
            max_tier: SecurityTier::Guarded, // Include Guarded tier like search_memory
            include_keyword: true,
            keyword_boost: 0.3,
            ..Default::default()
        };

        match self.search.search(query, options).await {
            Ok(hits) => {
                let sources: Vec<serde_json::Value> = hits.into_iter()
                    // First filter to only research sources (tagged with kind:source)
                    .filter(|h| h.object.tags.contains(&"kind:source".to_string()))
                    .filter(|h| {
                        // Then apply source_type filter
                        let tags = &h.object.tags;
                        match source_type {
                            "pdf" => tags.iter().any(|t| t.contains("pdf")),
                            "paper" => tags.iter().any(|t| t.contains("source_type:Paper")),
                            "notes" => tags.iter().any(|t| t.contains("note")),
                            "web" => tags.iter().any(|t| t.contains("source_type:WebPage")),
                            _ => true,
                        }
                    })
                    .map(|h| {
                        // Extract source_type from tags
                        let src_type = h.object.tags.iter()
                            .find(|t| t.starts_with("source_type:"))
                            .map(|t| t.replace("source_type:", ""))
                            .unwrap_or_else(|| "unknown".to_string());

                        // Extract user tags
                        let user_tags: Vec<&str> = h.object.tags.iter()
                            .filter(|t| t.starts_with("user_tag:"))
                            .map(|t| t.trim_start_matches("user_tag:"))
                            .collect();

                        serde_json::json!({
                            "id": h.object.suid.to_string(),
                            "title": h.object.name.clone().unwrap_or_else(|| "Untitled".to_string()),
                            "type": src_type,
                            "score": h.score,
                            "preview": h.object.content_as_str()
                                .map(|s| if s.len() > 500 { format!("{}...", &s[..500]) } else { s.to_string() })
                                .unwrap_or_default(),
                            "created_at": h.object.created_at.to_rfc3339(),
                            "tags": user_tags,
                        })
                    })
                    .collect();

                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "query": query,
                    "count": sources.len(),
                    "sources": sources,
                })).unwrap())
            }
            Err(e) => ToolResult::error(format!("Search failed: {}", e)),
        }
    }

    async fn execute_get_coding_sessions(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let query = args.get("query").and_then(|v| v.as_str());
        let project = args.get("project").and_then(|v| v.as_str());
        let provider = args.get("provider").and_then(|v| v.as_str()).unwrap_or("all");
        let days_back = args.get("days_back").and_then(|v| v.as_i64());
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        // Build search query
        let search_query = match (query, project) {
            (Some(q), Some(p)) => format!("{} {} coding session", q, p),
            (Some(q), None) => format!("{} coding session", q),
            (None, Some(p)) => format!("{} coding session", p),
            (None, None) => "coding session ai chat".to_string(),
        };

        let options = SearchOptions {
            limit: limit.min(50),
            min_score: 0.2,
            max_tier: SecurityTier::Open,
            include_keyword: true,
            keyword_boost: 0.3,
            ..Default::default()
        };

        match self.search.search(&search_query, options).await {
            Ok(hits) => {
                let cutoff = days_back.map(|d| Utc::now() - chrono::Duration::days(d));

                let sessions: Vec<serde_json::Value> = hits.into_iter()
                    .filter(|h| {
                        // Filter by session tag
                        if !h.object.tags.contains(&"session".to_string()) {
                            return false;
                        }
                        // Filter by provider
                        if provider != "all" {
                            let has_provider = h.object.tags.iter()
                                .any(|t| t.to_lowercase().contains(&provider.to_lowercase()));
                            if !has_provider {
                                return false;
                            }
                        }
                        // Filter by time
                        if let Some(cutoff) = cutoff {
                            if h.object.created_at < cutoff {
                                return false;
                            }
                        }
                        true
                    })
                    .map(|h| {
                        serde_json::json!({
                            "id": h.object.suid.to_string(),
                            "title": h.object.name.clone().unwrap_or_else(|| "Coding Session".to_string()),
                            "score": h.score,
                            "content": h.object.content_as_str()
                                .map(|s| if s.len() > 1000 { format!("{}...", &s[..1000]) } else { s.to_string() })
                                .unwrap_or_default(),
                            "created_at": h.object.created_at.to_rfc3339(),
                            "tags": h.object.tags,
                            "project": h.object.tags.iter()
                                .find(|t| !["session", "chunk", "decision", "research", "document", "claude-code", "cursor", "chatgpt"].contains(&t.as_str()))
                                .cloned(),
                        })
                    })
                    .collect();

                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "query": search_query,
                    "count": sessions.len(),
                    "sessions": sessions,
                })).unwrap())
            }
            Err(e) => ToolResult::error(format!("Search failed: {}", e)),
        }
    }

    async fn execute_get_citation(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let source_id = args.get("source_id").and_then(|v| v.as_str());
        let title = args.get("title").and_then(|v| v.as_str());
        let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("apa");

        // Find the source
        let source = if let Some(id) = source_id {
            match Suid::parse(id) {
                Ok(suid) => self.search.get(&suid).await.ok().flatten(),
                Err(_) => None,
            }
        } else if let Some(title_query) = title {
            let options = SearchOptions {
                limit: 1,
                min_score: 0.3,
                max_tier: SecurityTier::Open,
                include_keyword: true,
                keyword_boost: 0.5,
                ..Default::default()
            };
            self.search.search(title_query, options).await
                .ok()
                .and_then(|hits| hits.into_iter().next())
                .map(|h| h.object)
        } else {
            return ToolResult::error("Provide either source_id or title".to_string());
        };

        match source {
            Some(obj) => {
                let name = obj.name.clone().unwrap_or_else(|| "Untitled".to_string());
                let year = obj.created_at.format("%Y").to_string();

                // Generate citation based on format
                let citation = match format {
                    "apa" => format!("{}. ({}). {}.", "Author", year, name),
                    "mla" => format!("Author. \"{}.\" {}", name, year),
                    "chicago" => format!("Author. \"{}.\" {}", name, year),
                    "bibtex" => format!(
                        "@article{{{},\n  title = {{{}}},\n  year = {{{}}},\n  author = {{{}}}\n}}",
                        name.chars().filter(|c| c.is_alphanumeric()).take(10).collect::<String>().to_lowercase(),
                        name,
                        year,
                        "Author"
                    ),
                    _ => format!("{}. ({}). {}", "Author", year, name),
                };

                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "source_id": obj.suid.to_string(),
                    "title": name,
                    "format": format,
                    "citation": citation,
                    "note": "Author information may need to be manually updated",
                })).unwrap())
            }
            None => ToolResult::error("Source not found".to_string()),
        }
    }

    async fn execute_summarize_source(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let source_id = args.get("source_id").and_then(|v| v.as_str());
        let query = args.get("query").and_then(|v| v.as_str());
        let focus = args.get("focus").and_then(|v| v.as_str());

        // Find the source
        let source = if let Some(id) = source_id {
            match Suid::parse(id) {
                Ok(suid) => self.search.get(&suid).await.ok().flatten(),
                Err(_) => None,
            }
        } else if let Some(search_query) = query {
            let options = SearchOptions {
                limit: 1,
                min_score: 0.3,
                max_tier: SecurityTier::Open,
                include_keyword: true,
                keyword_boost: 0.5,
                ..Default::default()
            };
            self.search.search(search_query, options).await
                .ok()
                .and_then(|hits| hits.into_iter().next())
                .map(|h| h.object)
        } else {
            return ToolResult::error("Provide either source_id or query".to_string());
        };

        match source {
            Some(obj) => {
                let content = obj.content_as_str().unwrap_or("");
                let name = obj.name.clone().unwrap_or_else(|| "Untitled".to_string());

                // For now, return the content with metadata
                // In a full implementation, this would use an LLM to summarize
                let preview = if content.len() > 2000 {
                    format!("{}...\n\n[Content truncated - {} total characters]", &content[..2000], content.len())
                } else {
                    content.to_string()
                };

                let summary_note = if let Some(f) = focus {
                    format!("Focus requested: {}. Full summarization requires LLM processing.", f)
                } else {
                    "Full summarization requires LLM processing. Showing content preview.".to_string()
                };

                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "source_id": obj.suid.to_string(),
                    "title": name,
                    "content_length": content.len(),
                    "preview": preview,
                    "tags": obj.tags,
                    "note": summary_note,
                })).unwrap())
            }
            None => ToolResult::error("Source not found".to_string()),
        }
    }

    async fn execute_save_draft(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: path".to_string()),
        };

        let content = match args.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("Missing required parameter: content".to_string()),
        };

        let section = args.get("section").and_then(|v| v.as_str());

        use std::fs;
        use std::path::Path;

        let file_path = Path::new(path);

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return ToolResult::error(format!("Failed to create directories: {}", e));
                }
            }
        }

        // If section is specified, try to update just that section
        let final_content = if let Some(section_name) = section {
            if file_path.exists() {
                match fs::read_to_string(file_path) {
                    Ok(existing) => {
                        // Try to find and replace the section
                        let section_header = format!("## {}", section_name);
                        if let Some(start) = existing.find(&section_header) {
                            // Find the next section or end of file
                            let after_header = start + section_header.len();
                            let end = existing[after_header..]
                                .find("\n## ")
                                .map(|i| after_header + i)
                                .unwrap_or(existing.len());

                            format!("{}{}\n\n{}\n{}",
                                &existing[..start],
                                section_header,
                                content,
                                &existing[end..])
                        } else {
                            // Section not found, append it
                            format!("{}\n\n{}\n\n{}", existing, section_header, content)
                        }
                    }
                    Err(_) => content.to_string(),
                }
            } else {
                format!("## {}\n\n{}", section_name, content)
            }
        } else {
            content.to_string()
        };

        match fs::write(file_path, &final_content) {
            Ok(_) => {
                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "success": true,
                    "path": path,
                    "bytes_written": final_content.len(),
                    "section": section,
                })).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to write file: {}", e)),
        }
    }

    async fn execute_read_file(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: path".to_string()),
        };

        use std::fs;

        match fs::read_to_string(path) {
            Ok(content) => {
                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "path": path,
                    "content": content,
                    "length": content.len(),
                })).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to read file: {}", e)),
        }
    }

    async fn execute_list_sources(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let source_type = args.get("source_type")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let limit = args.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        // Use tag-filtered query to efficiently get all research sources
        let store = self.search.store.read().await;
        let source_objects = match store.list_by_tag("kind:source", 500) {
            Ok(objs) => objs,
            Err(e) => return ToolResult::error(format!("Failed to list sources: {}", e)),
        };

        // Apply source_type filter
        let mut sources: Vec<serde_json::Value> = source_objects.iter()
            .filter(|obj| {
                // Apply source_type filter
                let tags = &obj.tags;
                match source_type {
                    "pdf" => tags.iter().any(|t| t.contains("pdf")),
                    "paper" => tags.iter().any(|t| t.contains("source_type:Paper")),
                    "notes" => tags.iter().any(|t| t.contains("note")),
                    "web" => tags.iter().any(|t| t.contains("source_type:WebPage")),
                    _ => true,
                }
            })
            .map(|obj| {
                // Extract source_type from tags
                let src_type = obj.tags.iter()
                    .find(|t| t.starts_with("source_type:"))
                    .map(|t| t.replace("source_type:", ""))
                    .unwrap_or_else(|| "unknown".to_string());

                // Extract user tags
                let user_tags: Vec<&str> = obj.tags.iter()
                    .filter(|t| t.starts_with("user_tag:"))
                    .map(|t| t.trim_start_matches("user_tag:"))
                    .collect();

                serde_json::json!({
                    "id": obj.suid.to_string(),
                    "title": obj.name.clone().unwrap_or_else(|| "Untitled".to_string()),
                    "type": src_type,
                    "created_at": obj.created_at.to_rfc3339(),
                    "tags": user_tags,
                })
            })
            .collect();

        // Sort by created_at descending (newest first)
        sources.sort_by(|a, b| {
            let a_date = a.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            let b_date = b.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            b_date.cmp(a_date)
        });

        // Apply limit
        sources.truncate(limit);

        ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
            "count": sources.len(),
            "sources": sources,
        })).unwrap())
    }

    fn get_source_type(obj: &SemanticObject) -> String {
        if obj.tags.contains(&"session".to_string()) {
            "session".to_string()
        } else if obj.tags.contains(&"decision".to_string()) {
            "decision".to_string()
        } else if obj.tags.contains(&"research".to_string()) || obj.tags.contains(&"source".to_string()) {
            "research".to_string()
        } else if obj.tags.contains(&"document".to_string()) {
            "document".to_string()
        } else {
            "unknown".to_string()
        }
    }

    // ========================================================================
    // Proactive Intelligence Tools
    // ========================================================================

    async fn execute_get_suggestions(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        use crate::proactive::ProactiveEngine;

        let mut engine = ProactiveEngine::new(self.search.clone());
        let mut all_suggestions = Vec::new();

        // Get suggestions based on file if provided
        if let Some(file_path) = args.get("file_path").and_then(|v| v.as_str()) {
            let suggestions = engine.on_file_opened(file_path).await;
            all_suggestions.extend(suggestions);
        }

        // Get suggestions based on query if provided
        if let Some(query) = args.get("query").and_then(|v| v.as_str()) {
            let suggestions = engine.on_query(query).await;
            all_suggestions.extend(suggestions);
        }

        // Get session start suggestions if project path provided
        if let Some(project) = args.get("project_path").and_then(|v| v.as_str()) {
            let suggestions = engine.on_session_start(Some(project)).await;
            all_suggestions.extend(suggestions);
        }

        // Format for output
        let result: Vec<serde_json::Value> = all_suggestions.iter()
            .map(|s| serde_json::json!({
                "type": format!("{:?}", s.suggestion_type),
                "title": s.title,
                "content": s.content,
                "reason": s.reason,
                "relevance": s.relevance,
                "sources": s.source_ids,
            }))
            .collect();

        if result.is_empty() {
            ToolResult::success("No relevant suggestions for the current context.".to_string())
        } else {
            ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                "suggestions": result,
                "count": result.len(),
            })).unwrap())
        }
    }

    async fn execute_check_conflicts(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        use crate::proactive::{ProactiveEngine, SuggestionType};

        let topic = match args.get("topic").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("Missing required parameter: topic".to_string()),
        };

        let proposed_choice = match args.get("proposed_choice").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("Missing required parameter: proposed_choice".to_string()),
        };

        let mut engine = ProactiveEngine::new(self.search.clone());
        let suggestions = engine.on_decision_context(topic, proposed_choice).await;

        // Filter for conflicts
        let conflicts: Vec<_> = suggestions.iter()
            .filter(|s| s.suggestion_type == SuggestionType::DecisionConflict)
            .collect();

        if conflicts.is_empty() {
            ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                "has_conflicts": false,
                "message": format!("No conflicts found with the proposed decision on '{}'.", topic),
            })).unwrap())
        } else {
            let conflict_details: Vec<_> = conflicts.iter()
                .map(|s| serde_json::json!({
                    "title": s.title,
                    "content": s.content,
                    "reason": s.reason,
                }))
                .collect();

            ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                "has_conflicts": true,
                "conflict_count": conflicts.len(),
                "message": "Potential conflicts found with past decisions.",
                "conflicts": conflict_details,
            })).unwrap())
        }
    }

    async fn execute_get_work_context(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        use crate::proactive::{ProactiveEngine, SuggestionType};

        let project_path = match args.get("project_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: project_path".to_string()),
        };

        let mut engine = ProactiveEngine::new(self.search.clone());
        let suggestions = engine.on_session_start(Some(project_path)).await;

        // Format the context
        let continuity: Vec<_> = suggestions.iter()
            .filter(|s| s.suggestion_type == SuggestionType::WorkContinuity)
            .map(|s| serde_json::json!({
                "title": s.title,
                "content": s.content,
                "reason": s.reason,
            }))
            .collect();

        let patterns: Vec<_> = suggestions.iter()
            .filter(|s| s.suggestion_type == SuggestionType::DetectedPattern)
            .map(|s| serde_json::json!({
                "title": s.title,
                "content": s.content,
            }))
            .collect();

        ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
            "project": project_path,
            "has_context": !continuity.is_empty(),
            "previous_work": continuity,
            "detected_patterns": patterns,
        })).unwrap())
    }

    // ========================================================================
    // Learning Tools - Implement predict-test-compare-integrate algorithm
    // ========================================================================

    async fn execute_learning_start(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let topic = match args.get("topic").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("Missing required parameter: topic".to_string()),
        };

        let user_prediction = args.get("user_prediction").and_then(|v| v.as_str());
        let confidence = args.get("confidence").and_then(|v| v.as_str()).unwrap_or("guess");
        let use_web_search = args.get("use_web_search").and_then(|v| v.as_bool()).unwrap_or(false);
        let use_academic = args.get("use_academic_search").and_then(|v| v.as_bool()).unwrap_or(false);

        // Build response with guidance for the learning process
        let mut response = serde_json::json!({
            "topic": topic,
            "step": if user_prediction.is_some() { "ready_for_answer" } else { "needs_prediction" },
            "confidence": confidence,
        });

        // If no prediction yet, prompt for one
        if user_prediction.is_none() && confidence != "no_idea" {
            response["message"] = serde_json::json!(
                "Before revealing the answer, ask the user to make a prediction. \
                This is the key to learning - predictions create stakes and reveal mental models. \
                Ask: 'What do you think the answer is? Even a guess is valuable.'"
            );
            response["prompts"] = serde_json::json!([
                "What do you think?",
                "Take a guess - it's okay to be wrong",
                "What's your intuition telling you?"
            ]);
        } else {
            // User has made a prediction or says they don't know
            response["prediction"] = serde_json::json!(user_prediction.unwrap_or("I don't know"));
            response["message"] = serde_json::json!(
                "Good! Now provide the accurate answer. Be thorough but accessible. \
                Include concrete examples. If the user said 'no_idea', build them a minimal mental model first."
            );

            // Add search context if requested
            if use_web_search || use_academic {
                response["search_enabled"] = serde_json::json!(true);
                response["search_instructions"] = serde_json::json!(
                    "Use web_search and/or search academic papers to get current information on this topic."
                );
            }

            // Special handling for "no idea" case
            if confidence == "no_idea" {
                response["approach"] = serde_json::json!("build_mental_model");
                response["guidance"] = serde_json::json!(
                    "The user doesn't have a mental model yet. Your job is to: \
                    1. Normalize not knowing - it's the honest starting point. \
                    2. Build a MINIMAL mental model - just enough to make predictions next time. \
                    3. Use analogies to connect to things they might already know. \
                    4. End with a simple question they could now predict on."
                );
            }
        }

        ToolResult::success(serde_json::to_string_pretty(&response).unwrap())
    }

    async fn execute_learning_compare(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let topic = match args.get("topic").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("Missing required parameter: topic".to_string()),
        };

        let prediction = match args.get("prediction").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("Missing required parameter: prediction".to_string()),
        };

        let actual = match args.get("actual_answer").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::error("Missing required parameter: actual_answer".to_string()),
        };

        let response = serde_json::json!({
            "topic": topic,
            "step": "compare",
            "prediction": prediction,
            "actual_answer": actual,
            "analysis_instructions": {
                "goal": "Analyze the gap between prediction and reality",
                "steps": [
                    "1. Acknowledge what they got RIGHT (even if partial)",
                    "2. Identify specific gaps or misconceptions",
                    "3. Explain WHY the gap exists (what assumption led them astray?)",
                    "4. Frame wrongness as valuable data, not failure"
                ],
                "tone": "Be encouraging but honest. Wrong predictions are the best teachers.",
                "example_phrases": [
                    "You were right that...",
                    "The gap in your model was...",
                    "This reveals an interesting assumption...",
                    "This is exactly why predictions are so valuable for learning"
                ]
            },
            "next_step": "After analysis, ask the user to summarize what they learned in their own words. This integration step is crucial for retention."
        });

        ToolResult::success(serde_json::to_string_pretty(&response).unwrap())
    }

    async fn execute_learning_save(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let topic = match args.get("topic").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("Missing required parameter: topic".to_string()),
        };

        let actual = match args.get("actual_answer").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::error("Missing required parameter: actual_answer".to_string()),
        };

        let prediction = args.get("prediction").and_then(|v| v.as_str()).unwrap_or("");
        let comparison = args.get("comparison").and_then(|v| v.as_str()).unwrap_or("");
        let integration = args.get("user_integration").and_then(|v| v.as_str()).unwrap_or("");

        let followups: Vec<String> = args.get("followup_questions")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let tags: Vec<String> = args.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        // Create a learning cycle object to store
        let learning_cycle = serde_json::json!({
            "type": "learning_cycle",
            "topic": topic,
            "prediction": prediction,
            "actual_answer": actual,
            "comparison": comparison,
            "user_integration": integration,
            "followup_questions": followups,
            "timestamp": Utc::now().to_rfc3339(),
        });

        // Create a semantic object for this learning cycle
        let content = serde_json::to_string_pretty(&learning_cycle).unwrap();

        // Use the same pattern as log_decision
        let mut obj = SemanticObject::from_text(&content)
            .with_name(&format!("Learning: {}", topic))
            .with_tag("learning")
            .with_tag("learning_cycle")
            .with_tier(SecurityTier::Open);

        // Add custom tags
        for tag in &tags {
            obj = obj.with_tag(tag);
        }

        match self.search.store(&obj).await {
            Ok(_) => {
                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "success": true,
                    "id": obj.suid.to_string(),
                    "topic": topic,
                    "message": "Learning cycle saved to memory. Use learning_history to review past learning.",
                    "followup_questions": followups,
                })).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to save learning cycle: {}", e)),
        }
    }

    async fn execute_learning_history(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let topic = args.get("topic").and_then(|v| v.as_str());
        let days_back = args.get("days_back").and_then(|v| v.as_i64());
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        // Build search query
        let query = match topic {
            Some(t) => format!("learning {} learning_cycle", t),
            None => "learning learning_cycle".to_string(),
        };

        let options = SearchOptions {
            limit: limit.min(50),
            min_score: 0.2,
            max_tier: SecurityTier::Open,
            include_keyword: true,
            keyword_boost: 0.3,
            ..Default::default()
        };

        match self.search.search(&query, options).await {
            Ok(hits) => {
                let cutoff = days_back.map(|d| Utc::now() - chrono::Duration::days(d));

                let cycles: Vec<serde_json::Value> = hits.into_iter()
                    .filter(|h| {
                        // Filter to learning cycles
                        if !h.object.tags.contains(&"learning_cycle".to_string()) {
                            return false;
                        }
                        // Filter by date if specified
                        if let Some(cutoff) = cutoff {
                            if h.object.created_at < cutoff {
                                return false;
                            }
                        }
                        true
                    })
                    .filter_map(|h| {
                        // Try to parse the learning cycle content
                        h.object.content_as_str()
                            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                            .map(|mut v| {
                                v["id"] = serde_json::json!(h.object.suid.to_string());
                                v["score"] = serde_json::json!(h.score);
                                v
                            })
                    })
                    .collect();

                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "query": topic.unwrap_or("all"),
                    "count": cycles.len(),
                    "learning_cycles": cycles,
                    "message": if cycles.is_empty() {
                        "No learning history found. Start learning with learning_start!"
                    } else {
                        "Here are past learning cycles. You can revisit topics or build on previous understanding."
                    }
                })).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to search learning history: {}", e)),
        }
    }

    // ========================================================================
    // Web & Research Tools
    // ========================================================================

    async fn execute_research_papers(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("Missing required parameter: query".to_string()),
        };

        let num_results = args.get("num_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(8)
            .min(20) as usize;

        match super::web_search::search_academic(query, num_results).await {
            Ok(results) => {
                let papers: Vec<serde_json::Value> = results.papers.iter().map(|p| {
                    serde_json::json!({
                        "title": p.title,
                        "authors": p.authors,
                        "year": p.year,
                        "abstract": p.abstract_text,
                        "url": p.url,
                        "pdf_url": p.pdf_url,
                        "citation_count": p.citation_count,
                        "source": p.source,
                        "doi": p.doi,
                        "venue": p.venue,
                    })
                }).collect();

                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "query": results.query,
                    "total_found": results.total_found,
                    "papers": papers,
                })).unwrap())
            }
            Err(e) => ToolResult::error(format!("Academic search failed: {}", e)),
        }
    }

    async fn execute_web_search(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("Missing required parameter: query".to_string()),
        };

        let num_results = args.get("num_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(15) as usize;

        match super::web_search::search(query, num_results).await {
            Ok(results) => {
                let items: Vec<serde_json::Value> = results.results.iter().map(|r| {
                    serde_json::json!({
                        "title": r.title,
                        "url": r.url,
                        "snippet": r.snippet,
                        "source_domain": r.source_domain,
                    })
                }).collect();

                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "query": results.query,
                    "total_found": results.total_found,
                    "results": items,
                })).unwrap())
            }
            Err(e) => ToolResult::error(format!("Web search failed: {}", e)),
        }
    }

    async fn execute_fetch_page(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let url = match args.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return ToolResult::error("Missing required parameter: url".to_string()),
        };

        match super::web_search::fetch_page(url, false).await {
            Ok(page) => {
                // Truncate very long content to avoid overwhelming the LLM
                let content = if page.content.len() > 15000 {
                    format!("{}...\n\n[Content truncated at 15000 chars. Total: {} chars]",
                        &page.content[..15000], page.content.len())
                } else {
                    page.content
                };

                ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "url": page.url,
                    "title": page.title,
                    "content": content,
                    "word_count": page.word_count,
                    "metadata": page.metadata,
                })).unwrap())
            }
            Err(e) => ToolResult::error(format!("Failed to fetch page: {}", e)),
        }
    }

    // ========================================================================
    // Daily Research Automation Executors
    // ========================================================================

    async fn execute_list_research_topics(&self) -> ToolResult {
        let store = self.search.store.read().await;
        let topic_objs = match store.list_by_tag("kind:research-topic", 200) {
            Ok(objs) => objs,
            Err(e) => return ToolResult::error(format!("Failed to list topics: {}", e)),
        };

        // Exact-match filter — list_by_tag uses substring LIKE so prefix collisions are possible.
        let topics: Vec<serde_json::Value> = topic_objs.iter()
            .filter(|o| o.tags.iter().any(|t| t == "kind:research-topic"))
            .map(|o| {
                let parsed: serde_json::Value = o.content_as_str()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(serde_json::json!({}));
                serde_json::json!({
                    "name": o.name.clone().unwrap_or_default(),
                    "queries": parsed.get("queries").cloned().unwrap_or(serde_json::json!([])),
                    "last_run": parsed.get("last_run").cloned().unwrap_or(serde_json::Value::Null),
                    "enabled": parsed.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                    "created_at": o.created_at.to_rfc3339(),
                })
            })
            .collect();

        ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
            "count": topics.len(),
            "topics": topics,
        })).unwrap())
    }

    async fn execute_add_research_topic(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => return ToolResult::error("Missing required parameter: name".to_string()),
        };

        let queries: Vec<String> = args.get("queries")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_else(|| vec![name.clone()]);

        // Check for existing topic with same name (upsert)
        let name_tag = format!("research-topic-name:{}", name);
        let store = self.search.store.read().await;
        let existing = store.list_by_tag(&name_tag, 10)
            .unwrap_or_default()
            .into_iter()
            .find(|o| o.tags.iter().any(|t| t == &name_tag)
                && o.tags.iter().any(|t| t == "kind:research-topic"));
        drop(store);

        let body = serde_json::json!({
            "queries": queries,
            "last_run": serde_json::Value::Null,
            "enabled": true,
        });
        let content = serde_json::to_string(&body).unwrap();

        if let Some(mut obj) = existing {
            obj.update_content(content.into_bytes());
            match self.search.update(&obj).await {
                Ok(_) => ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "status": "updated",
                    "name": name,
                    "queries": queries,
                })).unwrap()),
                Err(e) => ToolResult::error(format!("Failed to update topic: {}", e)),
            }
        } else {
            let obj = SemanticObject::new(content.into_bytes(), ContentType::Json)
                .with_name(&name)
                .with_tag("kind:research-topic")
                .with_tag(&name_tag)
                .with_tier(SecurityTier::Open);

            match self.search.store(&obj).await {
                Ok(_) => ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                    "status": "created",
                    "name": name,
                    "queries": queries,
                })).unwrap()),
                Err(e) => ToolResult::error(format!("Failed to create topic: {}", e)),
            }
        }
    }

    async fn execute_remove_research_topic(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => return ToolResult::error("Missing required parameter: name".to_string()),
        };

        let name_tag = format!("research-topic-name:{}", name);
        let store = self.search.store.write().await;

        let matches = match store.list_by_tag(&name_tag, 10) {
            Ok(objs) => objs,
            Err(e) => return ToolResult::error(format!("Failed to look up topic: {}", e)),
        };

        let mut removed = 0;
        for obj in matches {
            if obj.tags.iter().any(|t| t == &name_tag)
                && obj.tags.iter().any(|t| t == "kind:research-topic")
            {
                if let Ok(true) = store.delete(&obj.suid) {
                    removed += 1;
                }
            }
        }

        if removed == 0 {
            return ToolResult::error(format!("No topic found with name: {}", name));
        }

        ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
            "status": "removed",
            "name": name,
            "removed_count": removed,
        })).unwrap())
    }

    async fn execute_daily_research_fetch(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let topic_name = match args.get("topic").and_then(|v| v.as_str()) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => return ToolResult::error("Missing required parameter: topic".to_string()),
        };

        let limit = args.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(20) as usize;

        // Load topic to get queries
        let name_tag = format!("research-topic-name:{}", topic_name);
        let (queries, topic_obj) = {
            let store = self.search.store.read().await;
            let topic = store.list_by_tag(&name_tag, 10)
                .unwrap_or_default()
                .into_iter()
                .find(|o| o.tags.iter().any(|t| t == &name_tag)
                    && o.tags.iter().any(|t| t == "kind:research-topic"));
            match topic {
                Some(o) => {
                    let parsed: serde_json::Value = o.content_as_str()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(serde_json::json!({}));
                    let qs: Vec<String> = parsed.get("queries")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_else(|| vec![topic_name.clone()]);
                    (qs, Some(o))
                }
                None => return ToolResult::error(format!("Unknown topic: {}. Register it with add_research_topic first.", topic_name)),
            }
        };

        // Fetch candidates from academic search across all queries
        let per_query = (limit * 2).max(8);
        let mut all_candidates: Vec<super::web_search::AcademicPaper> = Vec::new();
        let mut seen_titles = std::collections::HashSet::new();
        let mut search_errors: Vec<String> = Vec::new();

        for query in &queries {
            match super::web_search::search_academic(query, per_query).await {
                Ok(results) => {
                    for paper in results.papers {
                        let normalized = paper.title.to_lowercase()
                            .chars().filter(|c| c.is_alphanumeric()).collect::<String>();
                        if seen_titles.insert(normalized) {
                            all_candidates.push(paper);
                        }
                    }
                }
                Err(e) => search_errors.push(format!("{}: {}", query, e)),
            }
        }

        // Dedup against already-stored summaries using dedup tags
        let store = self.search.store.read().await;
        let mut fresh: Vec<serde_json::Value> = Vec::new();

        for paper in all_candidates {
            if fresh.len() >= limit {
                break;
            }

            let mut dedup_hits = 0;

            if let Some(doi) = &paper.doi {
                let tag = if doi.starts_with("arXiv:") {
                    format!("dedup-arxiv:{}", doi.trim_start_matches("arXiv:"))
                } else {
                    format!("dedup-doi:{}", doi)
                };
                if let Ok(hits) = store.list_by_tag(&tag, 1) {
                    if hits.iter().any(|o| o.tags.iter().any(|t| t == &tag)) {
                        dedup_hits += 1;
                    }
                }
            }

            let url_tag = format!("dedup-url:{}", Self::normalize_url(&paper.url));
            if dedup_hits == 0 {
                if let Ok(hits) = store.list_by_tag(&url_tag, 1) {
                    if hits.iter().any(|o| o.tags.iter().any(|t| t == &url_tag)) {
                        dedup_hits += 1;
                    }
                }
            }

            if dedup_hits == 0 {
                fresh.push(serde_json::json!({
                    "title": paper.title,
                    "authors": paper.authors,
                    "year": paper.year,
                    "abstract": paper.abstract_text,
                    "url": paper.url,
                    "pdf_url": paper.pdf_url,
                    "doi": paper.doi,
                    "venue": paper.venue,
                    "source": paper.source,
                    "citation_count": paper.citation_count,
                }));
            }
        }
        drop(store);

        // Update last_run on the topic
        if let Some(mut obj) = topic_obj {
            let mut parsed: serde_json::Value = obj.content_as_str()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::json!({}));
            if let Some(map) = parsed.as_object_mut() {
                map.insert("last_run".to_string(), serde_json::json!(Utc::now().to_rfc3339()));
            }
            let new_content = serde_json::to_string(&parsed).unwrap();
            obj.update_content(new_content.into_bytes());
            let _ = self.search.update(&obj).await;
        }

        ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
            "topic": topic_name,
            "queries": queries,
            "candidates": fresh,
            "count": fresh.len(),
            "search_errors": search_errors,
            "instructions": "For each candidate, write a 3-5 sentence summary capturing key findings, methodology, and novelty. Then call store_research_summary with topic, title, url, summary, and any of (doi, arxiv_id, authors, year, venue).",
        })).unwrap())
    }

    async fn execute_store_research_summary(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let topic = match args.get("topic").and_then(|v| v.as_str()) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => return ToolResult::error("Missing required parameter: topic".to_string()),
        };
        let title = match args.get("title").and_then(|v| v.as_str()) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => return ToolResult::error("Missing required parameter: title".to_string()),
        };
        let url = match args.get("url").and_then(|v| v.as_str()) {
            Some(u) if !u.is_empty() => u.to_string(),
            _ => return ToolResult::error("Missing required parameter: url".to_string()),
        };
        let summary = match args.get("summary").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => return ToolResult::error("Missing required parameter: summary".to_string()),
        };

        let doi = args.get("doi").and_then(|v| v.as_str()).map(String::from);
        let arxiv_id = args.get("arxiv_id").and_then(|v| v.as_str()).map(String::from);
        let authors: Vec<String> = args.get("authors")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let year = args.get("year").and_then(|v| v.as_i64());
        let venue = args.get("venue").and_then(|v| v.as_str()).map(String::from);

        // Build markdown content
        let authors_line = if authors.is_empty() {
            String::new()
        } else {
            format!("\n**Authors:** {}", authors.join(", "))
        };
        let year_line = year.map(|y| format!("\n**Year:** {}", y)).unwrap_or_default();
        let venue_line = venue.as_ref().map(|v| format!("\n**Venue:** {}", v)).unwrap_or_default();
        let doi_line = doi.as_ref().map(|d| format!("\n**DOI:** {}", d)).unwrap_or_default();
        let arxiv_line = arxiv_id.as_ref().map(|a| format!("\n**arXiv:** {}", a)).unwrap_or_default();

        let markdown = format!(
            "# {}\n\n**URL:** {}{}{}{}{}{}\n\n## Summary\n\n{}\n",
            title, url, authors_line, year_line, venue_line, doi_line, arxiv_line, summary
        );

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut obj = SemanticObject::from_markdown(&markdown)
            .with_name(&title)
            .with_tag("kind:research-summary")
            .with_tag(&format!("research-topic-name:{}", topic))
            .with_tag(&format!("research-date:{}", today))
            .with_tag(&format!("dedup-url:{}", Self::normalize_url(&url)))
            .with_tier(SecurityTier::Open);

        if let Some(d) = &doi {
            obj = obj.with_tag(&format!("dedup-doi:{}", d));
        }
        if let Some(a) = &arxiv_id {
            obj = obj.with_tag(&format!("dedup-arxiv:{}", a));
        }

        // Stash structured metadata for later retrieval
        obj = obj
            .with_metadata("topic", serde_json::json!(topic))
            .with_metadata("source_url", serde_json::json!(url))
            .with_metadata("authors", serde_json::json!(authors))
            .with_metadata("year", serde_json::json!(year))
            .with_metadata("venue", serde_json::json!(venue))
            .with_metadata("doi", serde_json::json!(doi))
            .with_metadata("arxiv_id", serde_json::json!(arxiv_id));

        match self.search.store(&obj).await {
            Ok(_) => ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
                "status": "stored",
                "id": obj.suid.to_string(),
                "topic": topic,
                "title": title,
            })).unwrap()),
            Err(e) => ToolResult::error(format!("Failed to store summary: {}", e)),
        }
    }

    async fn execute_list_research_summaries(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let topic = args.get("topic").and_then(|v| v.as_str()).map(String::from);
        let since = args.get("since").and_then(|v| v.as_str()).map(String::from);
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

        let store = self.search.store.read().await;

        // Efficient path: filter by topic tag if provided, otherwise all summaries
        let tag = match &topic {
            Some(t) => format!("research-topic-name:{}", t),
            None => "kind:research-summary".to_string(),
        };
        let exact_tag = tag.clone();
        let topic_filter = topic.clone();

        let objs = match store.list_by_tag(&tag, limit.max(200)) {
            Ok(o) => o,
            Err(e) => return ToolResult::error(format!("Failed to list summaries: {}", e)),
        };

        let mut summaries: Vec<serde_json::Value> = objs.iter()
            .filter(|o| o.tags.iter().any(|t| t == "kind:research-summary"))
            .filter(|o| o.tags.iter().any(|t| t == &exact_tag)
                || (topic_filter.is_none() && exact_tag == "kind:research-summary"))
            .filter(|o| {
                if let Some(s) = &since {
                    o.tags.iter().any(|t| {
                        t.strip_prefix("research-date:")
                            .map(|d| d.as_ref() as &str >= s.as_str())
                            .unwrap_or(false)
                    })
                } else {
                    true
                }
            })
            .map(|o| {
                let topic_tag = o.tags.iter()
                    .find(|t| t.starts_with("research-topic-name:"))
                    .map(|t| t.trim_start_matches("research-topic-name:").to_string())
                    .unwrap_or_default();
                let date = o.tags.iter()
                    .find(|t| t.starts_with("research-date:"))
                    .map(|t| t.trim_start_matches("research-date:").to_string())
                    .unwrap_or_default();
                serde_json::json!({
                    "id": o.suid.to_string(),
                    "title": o.name.clone().unwrap_or_default(),
                    "topic": topic_tag,
                    "date": date,
                    "doi": o.metadata.get("doi").cloned().unwrap_or(serde_json::Value::Null),
                    "arxiv_id": o.metadata.get("arxiv_id").cloned().unwrap_or(serde_json::Value::Null),
                    "url": o.metadata.get("source_url").cloned().unwrap_or(serde_json::Value::Null),
                    "authors": o.metadata.get("authors").cloned().unwrap_or(serde_json::Value::Null),
                    "year": o.metadata.get("year").cloned().unwrap_or(serde_json::Value::Null),
                    "venue": o.metadata.get("venue").cloned().unwrap_or(serde_json::Value::Null),
                    "summary": o.content_as_str().unwrap_or("").to_string(),
                })
            })
            .collect();

        summaries.sort_by(|a, b| {
            let ad = a.get("date").and_then(|v| v.as_str()).unwrap_or("");
            let bd = b.get("date").and_then(|v| v.as_str()).unwrap_or("");
            bd.cmp(ad)
        });
        summaries.truncate(limit);

        ToolResult::success(serde_json::to_string_pretty(&serde_json::json!({
            "count": summaries.len(),
            "topic_filter": topic,
            "since_filter": since,
            "summaries": summaries,
        })).unwrap())
    }

    fn normalize_url(url: &str) -> String {
        let lower = url.trim().to_lowercase();
        let without_scheme = lower.trim_start_matches("https://")
            .trim_start_matches("http://");
        let without_query = without_scheme.split('?').next().unwrap_or(without_scheme);
        let without_frag = without_query.split('#').next().unwrap_or(without_query);
        without_frag.trim_end_matches('/').to_string()
    }
}

// ============================================================================
// MCP Server Protocol (stdio)
// ============================================================================

/// MCP protocol message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "jsonrpc")]
pub struct JsonRpcRequest {
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

/// MCP Server that handles stdio communication
pub struct McpMemoryServer {
    executor: MemoryToolExecutor,
    tools: Vec<Tool>,
}

impl McpMemoryServer {
    pub fn new(search: Arc<SemanticSearch>) -> Self {
        Self {
            executor: MemoryToolExecutor::new(search),
            tools: create_memory_tools(),
        }
    }

    /// Handle an incoming JSON-RPC request
    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id),
            "tools/list" => self.handle_list_tools(request.id),
            "tools/call" => self.handle_call_tool(request.id, request.params).await,
            "ping" => JsonRpcResponse::success(request.id, serde_json::json!({"status": "ok"})),
            _ => JsonRpcResponse::error(request.id, -32601, format!("Method not found: {}", request.method)),
        }
    }

    fn handle_initialize(&self, id: serde_json::Value) -> JsonRpcResponse {
        let result = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "marlos-memory",
                "version": env!("CARGO_PKG_VERSION")
            }
        });
        JsonRpcResponse::success(id, result)
    }

    fn handle_list_tools(&self, id: serde_json::Value) -> JsonRpcResponse {
        let tools: Vec<serde_json::Value> = self.tools.iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": {
                        "type": t.parameters.param_type,
                        "properties": t.parameters.properties.iter().map(|(k, v)| {
                            (k.clone(), serde_json::json!({
                                "type": v.prop_type,
                                "description": v.description
                            }))
                        }).collect::<HashMap<_, _>>(),
                        "required": t.parameters.required
                    }
                })
            })
            .collect();

        JsonRpcResponse::success(id, serde_json::json!({ "tools": tools }))
    }

    async fn handle_call_tool(&self, id: serde_json::Value, params: serde_json::Value) -> JsonRpcResponse {
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let arguments: HashMap<String, serde_json::Value> = params.get("arguments")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let call = ToolCall {
            name: name.to_string(),
            arguments,
        };

        let result = self.executor.execute(call).await;

        if result.success {
            // Parse the content as JSON if possible
            let content = serde_json::from_str::<serde_json::Value>(&result.content)
                .unwrap_or(serde_json::json!({"text": result.content}));

            JsonRpcResponse::success(id, serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&content).unwrap_or(result.content)
                }]
            }))
        } else {
            JsonRpcResponse::error(id, -32000, result.error.unwrap_or("Unknown error".to_string()))
        }
    }
}
