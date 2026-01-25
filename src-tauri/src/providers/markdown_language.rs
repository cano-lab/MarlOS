//! Markdown Language Service Provider
//!
//! An ALWAYS_ON provider that parses markdown document structure:
//! - Headings (level, text, line number)
//! - Links (text, url)
//! - Code blocks (language, line number)
//! - Tasks (checkbox items)
//!
//! This is the core "understanding" service that other providers can query.

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::{Provider, ProviderCategory, ProviderContext};

/// A heading in the document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    /// Heading level (1-6)
    pub level: u8,
    /// Heading text content
    pub text: String,
    /// Line number (1-based)
    pub line: usize,
}

/// A link in the document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// Link text
    pub text: String,
    /// Link URL
    pub url: String,
    /// Line number (1-based)
    pub line: usize,
    /// Whether this is an external link (http/https)
    pub is_external: bool,
}

/// A code block in the document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    /// Programming language (if specified)
    pub language: Option<String>,
    /// Starting line number (1-based)
    pub start_line: usize,
    /// Ending line number (1-based)
    pub end_line: usize,
    /// The code content
    pub content: String,
}

/// A task item (checkbox) in the document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Whether the task is completed
    pub done: bool,
    /// Task text
    pub text: String,
    /// Line number (1-based)
    pub line: usize,
}

/// Parsed document structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentStructure {
    /// All headings in document order
    pub headings: Vec<Heading>,
    /// All links in document order
    pub links: Vec<Link>,
    /// All code blocks in document order
    pub code_blocks: Vec<CodeBlock>,
    /// All task items in document order
    pub tasks: Vec<Task>,
}

/// Markdown Language Service Provider
pub struct MarkdownLanguageService {
    structure: DocumentStructure,
    heading_regex: Regex,
    link_regex: Regex,
    task_regex: Regex,
    fence_regex: Regex,
}

impl MarkdownLanguageService {
    pub fn new() -> Self {
        Self {
            structure: DocumentStructure::default(),
            // Match ATX headings: # Heading, ## Heading, etc.
            heading_regex: Regex::new(r"^(#{1,6})\s+(.+)$").unwrap(),
            // Match markdown links: [text](url)
            link_regex: Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap(),
            // Match task items: - [ ] task or - [x] task
            task_regex: Regex::new(r"^\s*[-*+]\s+\[([xX ])\]\s+(.+)$").unwrap(),
            // Match code fence: ```language or ```
            fence_regex: Regex::new(r"^```(\w*)").unwrap(),
        }
    }

    /// Get the current document structure
    pub fn get_structure(&self) -> &DocumentStructure {
        &self.structure
    }

    /// Get headings only
    pub fn get_headings(&self) -> &[Heading] {
        &self.structure.headings
    }

    /// Get links only
    pub fn get_links(&self) -> &[Link] {
        &self.structure.links
    }

    /// Get code blocks only
    pub fn get_code_blocks(&self) -> &[CodeBlock] {
        &self.structure.code_blocks
    }

    /// Get tasks only
    pub fn get_tasks(&self) -> &[Task] {
        &self.structure.tasks
    }

    /// Parse the document content
    fn parse(&mut self, content: &str) {
        let mut headings = Vec::new();
        let mut links = Vec::new();
        let mut code_blocks = Vec::new();
        let mut tasks = Vec::new();

        let mut in_code_block = false;
        let mut code_block_start = 0;
        let mut code_block_lang: Option<String> = None;
        let mut code_block_content = String::new();

        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1; // 1-based line numbers

            // Check for code fence
            if let Some(caps) = self.fence_regex.captures(line) {
                if in_code_block {
                    // End of code block
                    code_blocks.push(CodeBlock {
                        language: code_block_lang.take(),
                        start_line: code_block_start,
                        end_line: line_num,
                        content: code_block_content.trim_end().to_string(),
                    });
                    code_block_content.clear();
                    in_code_block = false;
                } else {
                    // Start of code block
                    in_code_block = true;
                    code_block_start = line_num;
                    let lang = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    code_block_lang = if lang.is_empty() {
                        None
                    } else {
                        Some(lang.to_string())
                    };
                }
                continue;
            }

            // If inside code block, accumulate content and skip other parsing
            if in_code_block {
                if !code_block_content.is_empty() {
                    code_block_content.push('\n');
                }
                code_block_content.push_str(line);
                continue;
            }

            // Parse headings
            if let Some(caps) = self.heading_regex.captures(line) {
                let level = caps.get(1).map(|m| m.as_str().len()).unwrap_or(1) as u8;
                let text = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
                headings.push(Heading {
                    level,
                    text,
                    line: line_num,
                });
            }

            // Parse links (can have multiple per line)
            for caps in self.link_regex.captures_iter(line) {
                let text = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                let url = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
                let is_external = url.starts_with("http://") || url.starts_with("https://");
                links.push(Link {
                    text,
                    url,
                    line: line_num,
                    is_external,
                });
            }

            // Parse tasks
            if let Some(caps) = self.task_regex.captures(line) {
                let checkbox = caps.get(1).map(|m| m.as_str()).unwrap_or(" ");
                let done = checkbox.eq_ignore_ascii_case("x");
                let text = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
                tasks.push(Task {
                    done,
                    text,
                    line: line_num,
                });
            }
        }

        // Handle unclosed code block at end of document
        if in_code_block {
            code_blocks.push(CodeBlock {
                language: code_block_lang,
                start_line: code_block_start,
                end_line: content.lines().count(),
                content: code_block_content.trim_end().to_string(),
            });
        }

        self.structure = DocumentStructure {
            headings,
            links,
            code_blocks,
            tasks,
        };

        log::trace!(
            "Parsed markdown: {} headings, {} links, {} code blocks, {} tasks",
            self.structure.headings.len(),
            self.structure.links.len(),
            self.structure.code_blocks.len(),
            self.structure.tasks.len()
        );
    }
}

impl Default for MarkdownLanguageService {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for MarkdownLanguageService {
    fn name(&self) -> &str {
        "markdown_language_service"
    }

    fn category(&self) -> ProviderCategory {
        ProviderCategory::AlwaysOn
    }

    fn activate(&mut self, context: &ProviderContext) -> Result<(), String> {
        self.parse(context.content);
        Ok(())
    }

    fn on_content_changed(&mut self, content: &str) {
        self.parse(content);
    }

    fn get_state(&self) -> serde_json::Value {
        serde_json::to_value(&self.structure).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_headings() {
        let mut provider = MarkdownLanguageService::new();
        let content = "# Title\n\nSome text\n\n## Section 1\n\n### Subsection\n\n## Section 2";

        let context = ProviderContext {
            content,
            file_path: None,
            commands: &super::super::CommandRegistry::new(),
        };
        provider.activate(&context).unwrap();

        let headings = provider.get_headings();
        assert_eq!(headings.len(), 4);
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[0].text, "Title");
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[1].text, "Section 1");
        assert_eq!(headings[2].level, 3);
        assert_eq!(headings[3].level, 2);
    }

    #[test]
    fn test_parse_links() {
        let mut provider = MarkdownLanguageService::new();
        let content = "Check out [Google](https://google.com) and [local](/docs/readme.md)";

        let context = ProviderContext {
            content,
            file_path: None,
            commands: &super::super::CommandRegistry::new(),
        };
        provider.activate(&context).unwrap();

        let links = provider.get_links();
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].text, "Google");
        assert!(links[0].is_external);
        assert_eq!(links[1].text, "local");
        assert!(!links[1].is_external);
    }

    #[test]
    fn test_parse_code_blocks() {
        let mut provider = MarkdownLanguageService::new();
        let content = "Text before\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n\nText after";

        let context = ProviderContext {
            content,
            file_path: None,
            commands: &super::super::CommandRegistry::new(),
        };
        provider.activate(&context).unwrap();

        let blocks = provider.get_code_blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, Some("rust".to_string()));
        assert!(blocks[0].content.contains("fn main()"));
    }

    #[test]
    fn test_parse_tasks() {
        let mut provider = MarkdownLanguageService::new();
        let content = "Tasks:\n- [ ] Todo item\n- [x] Done item\n- [X] Also done";

        let context = ProviderContext {
            content,
            file_path: None,
            commands: &super::super::CommandRegistry::new(),
        };
        provider.activate(&context).unwrap();

        let tasks = provider.get_tasks();
        assert_eq!(tasks.len(), 3);
        assert!(!tasks[0].done);
        assert_eq!(tasks[0].text, "Todo item");
        assert!(tasks[1].done);
        assert!(tasks[2].done);
    }

    #[test]
    fn test_no_parsing_inside_code_blocks() {
        let mut provider = MarkdownLanguageService::new();
        // Headings inside code blocks should NOT be parsed
        let content = "# Real Heading\n\n```markdown\n# Not a heading\n[not a link](url)\n```";

        let context = ProviderContext {
            content,
            file_path: None,
            commands: &super::super::CommandRegistry::new(),
        };
        provider.activate(&context).unwrap();

        assert_eq!(provider.get_headings().len(), 1);
        assert_eq!(provider.get_links().len(), 0);
    }
}
