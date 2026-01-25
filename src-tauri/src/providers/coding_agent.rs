//! Coding Agent Provider
//!
//! An ON_DEMAND provider for AI-powered code assistance:
//! - Code completion at cursor
//! - Code explanation for selected code
//! - Code editing with natural language instructions
//! - Code generation from descriptions
//!
//! Requires an AI provider (LM Studio, Ollama, OpenAI, etc.) to be configured.

use serde::{Deserialize, Serialize};

use super::{Command, Provider, ProviderCategory, ProviderContext};

/// Code operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeOperation {
    /// Complete code at cursor position
    Complete,
    /// Explain selected code
    Explain,
    /// Edit code with instruction
    Edit,
    /// Generate code from description
    Generate,
    /// Refactor selected code
    Refactor,
    /// Add documentation/comments
    Document,
    /// Find potential bugs
    FindBugs,
    /// Suggest tests
    SuggestTests,
}

impl CodeOperation {
    pub fn as_str(&self) -> &'static str {
        match self {
            CodeOperation::Complete => "complete",
            CodeOperation::Explain => "explain",
            CodeOperation::Edit => "edit",
            CodeOperation::Generate => "generate",
            CodeOperation::Refactor => "refactor",
            CodeOperation::Document => "document",
            CodeOperation::FindBugs => "find_bugs",
            CodeOperation::SuggestTests => "suggest_tests",
        }
    }

    /// Get the system prompt for this operation
    pub fn system_prompt(&self) -> &'static str {
        match self {
            CodeOperation::Complete => {
                "You are a code completion assistant. Complete the code at the cursor position. \
                 Only output the completion, no explanations. Match the existing code style."
            }
            CodeOperation::Explain => {
                "You are a code explanation assistant. Explain the provided code clearly and concisely. \
                 Cover what it does, how it works, and any important details."
            }
            CodeOperation::Edit => {
                "You are a code editing assistant. Modify the code according to the instruction. \
                 Only output the modified code, no explanations."
            }
            CodeOperation::Generate => {
                "You are a code generation assistant. Generate code based on the description. \
                 Write clean, idiomatic code with appropriate comments."
            }
            CodeOperation::Refactor => {
                "You are a code refactoring assistant. Improve the code structure, readability, \
                 and maintainability while preserving functionality. Only output the refactored code."
            }
            CodeOperation::Document => {
                "You are a documentation assistant. Add clear, helpful documentation and comments \
                 to the code. Follow the language's documentation conventions."
            }
            CodeOperation::FindBugs => {
                "You are a code review assistant. Analyze the code for potential bugs, security issues, \
                 and problems. List each issue with its location and suggested fix."
            }
            CodeOperation::SuggestTests => {
                "You are a testing assistant. Suggest test cases for the provided code. \
                 Include edge cases and important scenarios to test."
            }
        }
    }
}

/// Request for a code operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRequest {
    /// The operation to perform
    pub operation: CodeOperation,
    /// The code to operate on
    pub code: String,
    /// Additional instruction (for edit operations)
    pub instruction: Option<String>,
    /// Language hint (if known)
    pub language: Option<String>,
    /// Cursor position (for completion)
    pub cursor_position: Option<usize>,
    /// Context before cursor (for completion)
    pub context_before: Option<String>,
    /// Context after cursor (for completion)
    pub context_after: Option<String>,
}

/// Response from a code operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeResponse {
    /// The operation that was performed
    pub operation: CodeOperation,
    /// The result (completion, explanation, edited code, etc.)
    pub result: String,
    /// Whether the operation was successful
    pub success: bool,
    /// Error message if not successful
    pub error: Option<String>,
}

/// Provider state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodingAgentState {
    /// Whether the AI provider is available
    pub ai_available: bool,
    /// Last operation performed
    pub last_operation: Option<String>,
    /// Number of operations performed this session
    pub operation_count: usize,
}

/// Coding Agent Provider
pub struct CodingAgentProvider {
    state: CodingAgentState,
}

impl CodingAgentProvider {
    pub fn new() -> Self {
        Self {
            state: CodingAgentState::default(),
        }
    }

    /// Build a prompt for the given operation
    pub fn build_prompt(request: &CodeRequest) -> String {
        match request.operation {
            CodeOperation::Complete => {
                let before = request.context_before.as_deref().unwrap_or(&request.code);
                let after = request.context_after.as_deref().unwrap_or("");
                let lang = request.language.as_deref().unwrap_or("code");
                format!(
                    "Complete the following {} code at the cursor position marked with <CURSOR>:\n\n\
                     ```{}\n{}<CURSOR>{}\n```\n\n\
                     Only output the completion text that should be inserted at the cursor.",
                    lang, lang, before, after
                )
            }
            CodeOperation::Explain => {
                let lang = request.language.as_deref().unwrap_or("code");
                format!(
                    "Explain the following {} code:\n\n```{}\n{}\n```",
                    lang, lang, request.code
                )
            }
            CodeOperation::Edit => {
                let lang = request.language.as_deref().unwrap_or("code");
                let instruction = request.instruction.as_deref().unwrap_or("improve this code");
                format!(
                    "Edit the following {} code according to this instruction: {}\n\n\
                     ```{}\n{}\n```\n\n\
                     Output only the edited code.",
                    lang, instruction, lang, request.code
                )
            }
            CodeOperation::Generate => {
                let lang = request.language.as_deref().unwrap_or("code");
                let description = request.instruction.as_deref().unwrap_or(&request.code);
                format!(
                    "Generate {} code for the following:\n\n{}\n\n\
                     Output only the code.",
                    lang, description
                )
            }
            CodeOperation::Refactor => {
                let lang = request.language.as_deref().unwrap_or("code");
                format!(
                    "Refactor the following {} code to improve its structure and readability:\n\n\
                     ```{}\n{}\n```\n\n\
                     Output only the refactored code.",
                    lang, lang, request.code
                )
            }
            CodeOperation::Document => {
                let lang = request.language.as_deref().unwrap_or("code");
                format!(
                    "Add documentation and comments to the following {} code:\n\n\
                     ```{}\n{}\n```\n\n\
                     Output the code with added documentation.",
                    lang, lang, request.code
                )
            }
            CodeOperation::FindBugs => {
                let lang = request.language.as_deref().unwrap_or("code");
                format!(
                    "Analyze the following {} code for bugs and issues:\n\n\
                     ```{}\n{}\n```",
                    lang, lang, request.code
                )
            }
            CodeOperation::SuggestTests => {
                let lang = request.language.as_deref().unwrap_or("code");
                format!(
                    "Suggest test cases for the following {} code:\n\n\
                     ```{}\n{}\n```",
                    lang, lang, request.code
                )
            }
        }
    }

    /// Detect programming language from file extension
    pub fn detect_language(file_path: Option<&str>) -> Option<String> {
        let path = file_path?;
        let ext = path.rsplit('.').next()?;

        let lang = match ext.to_lowercase().as_str() {
            "rs" => "rust",
            "py" | "pyw" => "python",
            "js" | "mjs" | "cjs" => "javascript",
            "ts" | "tsx" => "typescript",
            "jsx" => "jsx",
            "go" => "go",
            "java" => "java",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" => "cpp",
            "cs" => "csharp",
            "rb" => "ruby",
            "php" => "php",
            "swift" => "swift",
            "kt" | "kts" => "kotlin",
            "scala" => "scala",
            "sh" | "bash" => "bash",
            "sql" => "sql",
            "html" | "htm" => "html",
            "css" | "scss" | "sass" => "css",
            "json" => "json",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "xml" => "xml",
            "md" | "markdown" => "markdown",
            _ => return None,
        };

        Some(lang.to_string())
    }
}

impl Default for CodingAgentProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for CodingAgentProvider {
    fn name(&self) -> &str {
        "coding_agent"
    }

    fn category(&self) -> ProviderCategory {
        ProviderCategory::OnDemand
    }

    fn activate(&mut self, context: &ProviderContext) -> Result<(), String> {
        // Register commands
        context.commands.register(
            Command::new("code.complete", "Complete Code at Cursor", self.name())
                .modifies_document()
        )?;

        context.commands.register(
            Command::new("code.explain", "Explain Selected Code", self.name())
                .requires_selection()
        )?;

        context.commands.register(
            Command::new("code.edit", "Edit Code with Instruction", self.name())
                .requires_selection()
                .modifies_document()
        )?;

        context.commands.register(
            Command::new("code.generate", "Generate Code from Description", self.name())
                .modifies_document()
        )?;

        context.commands.register(
            Command::new("code.refactor", "Refactor Selected Code", self.name())
                .requires_selection()
                .modifies_document()
        )?;

        context.commands.register(
            Command::new("code.document", "Add Documentation", self.name())
                .requires_selection()
                .modifies_document()
        )?;

        context.commands.register(
            Command::new("code.find_bugs", "Find Potential Bugs", self.name())
                .requires_selection()
        )?;

        context.commands.register(
            Command::new("code.suggest_tests", "Suggest Test Cases", self.name())
                .requires_selection()
        )?;

        context.commands.register(
            Command::new("code.status", "Check AI Status", self.name())
        )?;

        log::info!("Coding agent provider activated with {} commands", 9);
        Ok(())
    }

    fn get_state(&self) -> serde_json::Value {
        serde_json::to_value(&self.state).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(
            CodingAgentProvider::detect_language(Some("main.rs")),
            Some("rust".to_string())
        );
        assert_eq!(
            CodingAgentProvider::detect_language(Some("script.py")),
            Some("python".to_string())
        );
        assert_eq!(
            CodingAgentProvider::detect_language(Some("app.tsx")),
            Some("typescript".to_string())
        );
        assert_eq!(
            CodingAgentProvider::detect_language(Some("unknown.xyz")),
            None
        );
    }

    #[test]
    fn test_build_explain_prompt() {
        let request = CodeRequest {
            operation: CodeOperation::Explain,
            code: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            instruction: None,
            language: Some("rust".to_string()),
            cursor_position: None,
            context_before: None,
            context_after: None,
        };

        let prompt = CodingAgentProvider::build_prompt(&request);
        assert!(prompt.contains("Explain the following rust code"));
        assert!(prompt.contains("fn add"));
    }

    #[test]
    fn test_build_edit_prompt() {
        let request = CodeRequest {
            operation: CodeOperation::Edit,
            code: "let x = 5".to_string(),
            instruction: Some("add type annotation".to_string()),
            language: Some("rust".to_string()),
            cursor_position: None,
            context_before: None,
            context_after: None,
        };

        let prompt = CodingAgentProvider::build_prompt(&request);
        assert!(prompt.contains("add type annotation"));
        assert!(prompt.contains("let x = 5"));
    }

    #[test]
    fn test_system_prompts() {
        assert!(CodeOperation::Complete.system_prompt().contains("completion"));
        assert!(CodeOperation::Explain.system_prompt().contains("explanation"));
        assert!(CodeOperation::Generate.system_prompt().contains("generation"));
    }
}
