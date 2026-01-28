//! Tool Definitions for MCP Server
//!
//! This module contains helper functions for creating and managing tools.

use super::{Tool, ToolParameters, ParameterProperty};
use std::collections::HashMap;

/// Builder for creating tools easily
pub struct ToolBuilder {
    name: String,
    description: String,
    properties: HashMap<String, ParameterProperty>,
    required: Vec<String>,
}

impl ToolBuilder {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            properties: HashMap::new(),
            required: Vec::new(),
        }
    }

    pub fn param(mut self, name: &str, prop_type: &str, description: &str, required: bool) -> Self {
        self.properties.insert(name.to_string(), ParameterProperty {
            prop_type: prop_type.to_string(),
            description: description.to_string(),
            enum_values: None,
        });
        if required {
            self.required.push(name.to_string());
        }
        self
    }

    pub fn param_enum(mut self, name: &str, description: &str, values: Vec<&str>, required: bool) -> Self {
        self.properties.insert(name.to_string(), ParameterProperty {
            prop_type: "string".to_string(),
            description: description.to_string(),
            enum_values: Some(values.into_iter().map(String::from).collect()),
        });
        if required {
            self.required.push(name.to_string());
        }
        self
    }

    pub fn build(self) -> Tool {
        Tool {
            name: self.name,
            description: self.description,
            parameters: ToolParameters {
                param_type: "object".to_string(),
                properties: self.properties,
                required: self.required,
            },
        }
    }
}

/// Create standard research tools
pub fn create_research_tools() -> Vec<Tool> {
    vec![
        ToolBuilder::new(
            "web_search",
            "Search the web for information on a topic"
        )
        .param("query", "string", "The search query", true)
        .param("num_results", "integer", "Number of results (max 20)", false)
        .build(),

        ToolBuilder::new(
            "fetch_page",
            "Fetch and extract content from a web page"
        )
        .param("url", "string", "The URL to fetch", true)
        .param("extract_links", "boolean", "Whether to extract links", false)
        .build(),

        ToolBuilder::new(
            "get_research_context",
            "Get the user's current research context"
        )
        .build(),

        ToolBuilder::new(
            "add_source",
            "Add a source to the user's collection"
        )
        .param("url", "string", "The source URL", true)
        .param("title", "string", "The source title", true)
        .param("summary", "string", "Why this source is relevant", false)
        .param("tags", "array", "Tags for categorization", false)
        .build(),

        ToolBuilder::new(
            "evaluate_source",
            "Evaluate a source's reliability and relevance"
        )
        .param("url", "string", "The source URL to evaluate", true)
        .param("criteria", "string", "Specific criteria to evaluate", false)
        .build(),
    ]
}

/// Format tools for display in a prompt
pub fn format_tools_for_prompt(tools: &[Tool]) -> String {
    tools.iter()
        .map(|tool| {
            let params: Vec<String> = tool.parameters.properties.iter()
                .map(|(name, prop)| {
                    let required = if tool.parameters.required.contains(name) {
                        " (required)"
                    } else {
                        ""
                    };
                    format!("    - {}: {}{} - {}", name, prop.prop_type, required, prop.description)
                })
                .collect();

            format!(
                "**{}**\n  {}\n  Parameters:\n{}",
                tool.name,
                tool.description,
                params.join("\n")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
