//! Provider System for MarlOS
//!
//! Providers are bounded units of capability that observe documents/events,
//! compute derived information, and request actions via commands.
//!
//! Provider Categories:
//! - ALWAYS_ON: Silent, read-only, no UI (e.g., LanguageService, WordCount)
//! - IMPLIED: Hidden UI by default, no mutation (e.g., Preview, Outline)
//! - ON_DEMAND: Command-activated, may mutate with permission (e.g., Formatters)

use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard, PoisonError};
use serde::{Deserialize, Serialize};

// ============================================================================
// Lock Recovery Helpers
// ============================================================================

/// Recover from a poisoned read lock, logging a warning
fn recover_read_lock<'a, T>(result: Result<RwLockReadGuard<'a, T>, PoisonError<RwLockReadGuard<'a, T>>>) -> RwLockReadGuard<'a, T> {
    match result {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("Recovered from poisoned read lock");
            poisoned.into_inner()
        }
    }
}

/// Recover from a poisoned write lock, logging a warning
fn recover_write_lock<'a, T>(result: Result<RwLockWriteGuard<'a, T>, PoisonError<RwLockWriteGuard<'a, T>>>) -> RwLockWriteGuard<'a, T> {
    match result {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("Recovered from poisoned write lock");
            poisoned.into_inner()
        }
    }
}

// ============================================================================
// Provider Category
// ============================================================================

/// Provider activation categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCategory {
    /// Activated automatically, silent, read-only, no UI
    AlwaysOn,
    /// Activated automatically, UI hidden by default, no mutation
    Implied,
    /// Activated by command only, may mutate with confirmation
    OnDemand,
}

// ============================================================================
// Command System
// ============================================================================

/// A command that can be executed by a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Unique command identifier (e.g., "markdown.format.bold")
    pub id: String,
    /// Human-readable title (e.g., "Bold")
    pub title: String,
    /// Whether this command requires a text selection
    pub requires_selection: bool,
    /// Whether this command modifies the document
    pub modifies_document: bool,
    /// Optional confirmation message before execution
    pub confirm: Option<String>,
    /// The provider that registered this command
    pub provider: String,
}

impl Command {
    pub fn new(id: &str, title: &str, provider: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            requires_selection: false,
            modifies_document: false,
            confirm: None,
            provider: provider.to_string(),
        }
    }

    pub fn requires_selection(mut self) -> Self {
        self.requires_selection = true;
        self
    }

    pub fn modifies_document(mut self) -> Self {
        self.modifies_document = true;
        self
    }

    pub fn with_confirm(mut self, message: &str) -> Self {
        self.confirm = Some(message.to_string());
        self
    }
}

/// Registry of available commands
pub struct CommandRegistry {
    commands: RwLock<HashMap<String, Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: RwLock::new(HashMap::new()),
        }
    }

    /// Register a command
    pub fn register(&self, command: Command) -> Result<(), String> {
        let mut commands = recover_write_lock(self.commands.write());
        if commands.contains_key(&command.id) {
            return Err(format!("Command already registered: {}", command.id));
        }
        log::debug!("Registered command: {} ({})", command.id, command.title);
        commands.insert(command.id.clone(), command);
        Ok(())
    }

    /// Unregister a command
    pub fn unregister(&self, command_id: &str) {
        let mut commands = recover_write_lock(self.commands.write());
        commands.remove(command_id);
    }

    /// Get a command by ID
    pub fn get(&self, command_id: &str) -> Option<Command> {
        let commands = recover_read_lock(self.commands.read());
        commands.get(command_id).cloned()
    }

    /// List all commands
    pub fn list(&self) -> Vec<Command> {
        let commands = recover_read_lock(self.commands.read());
        commands.values().cloned().collect()
    }

    /// List commands by provider
    pub fn list_by_provider(&self, provider: &str) -> Vec<Command> {
        let commands = recover_read_lock(self.commands.read());
        commands
            .values()
            .filter(|c| c.provider == provider)
            .cloned()
            .collect()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Provider Context
// ============================================================================

/// Context passed to providers during activation
///
/// Provides access to document content, events, and command registration.
pub struct ProviderContext<'a> {
    /// Current document content
    pub content: &'a str,
    /// Document file path (if saved)
    pub file_path: Option<&'a str>,
    /// Command registry for registering provider commands
    pub commands: &'a CommandRegistry,
}

// ============================================================================
// Provider Trait
// ============================================================================

/// Trait that all providers must implement
pub trait Provider: Send + Sync {
    /// Unique provider name
    fn name(&self) -> &str;

    /// Provider category
    fn category(&self) -> ProviderCategory;

    /// Called once when provider is registered
    fn activate(&mut self, context: &ProviderContext) -> Result<(), String>;

    /// Called when document content changes
    fn on_content_changed(&mut self, _content: &str) {
        // Default: do nothing
    }

    /// Called when provider is deactivated
    fn deactivate(&mut self) {
        // Default: do nothing
    }

    /// Get provider state as JSON (for serialization to frontend)
    fn get_state(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

// ============================================================================
// Provider Registry
// ============================================================================

/// Registry of active providers
pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, Box<dyn Provider>>>,
    pub commands: Arc<CommandRegistry>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            commands: Arc::new(CommandRegistry::new()),
        }
    }

    /// Register and activate a provider
    pub fn register(
        &self,
        mut provider: Box<dyn Provider>,
        content: &str,
        file_path: Option<&str>,
    ) -> Result<(), String> {
        let name = provider.name().to_string();
        let category = provider.category();

        // Create context for activation
        let context = ProviderContext {
            content,
            file_path,
            commands: &self.commands,
        };

        // Activate the provider
        provider.activate(&context)?;

        // Store in registry
        let mut providers = recover_write_lock(self.providers.write());
        providers.insert(name.clone(), provider);

        log::info!("Registered provider: {} ({:?})", name, category);
        Ok(())
    }

    /// Unregister a provider
    pub fn unregister(&self, name: &str) -> Option<Box<dyn Provider>> {
        let mut providers = recover_write_lock(self.providers.write());
        if let Some(mut provider) = providers.remove(name) {
            provider.deactivate();
            log::info!("Unregistered provider: {}", name);
            Some(provider)
        } else {
            None
        }
    }

    /// Notify all providers of content change
    pub fn notify_content_changed(&self, content: &str) {
        let mut providers = recover_write_lock(self.providers.write());
        for provider in providers.values_mut() {
            provider.on_content_changed(content);
        }
    }

    /// Get provider state
    pub fn get_provider_state(&self, name: &str) -> Option<serde_json::Value> {
        let providers = recover_read_lock(self.providers.read());
        providers.get(name).map(|p| p.get_state())
    }

    /// List all registered providers
    pub fn list(&self) -> Vec<ProviderInfo> {
        let providers = recover_read_lock(self.providers.read());
        providers
            .values()
            .map(|p| ProviderInfo {
                name: p.name().to_string(),
                category: p.category(),
            })
            .collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary info about a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub category: ProviderCategory,
}

// ============================================================================
// Built-in Providers
// ============================================================================

pub mod word_count;
pub mod markdown_language;
pub mod coding_agent;
pub mod session;
pub mod research;
pub mod importers;

// Re-export built-in providers
pub use word_count::WordCountProvider;
pub use markdown_language::MarkdownLanguageService;
pub use coding_agent::{CodingAgentProvider, CodeOperation, CodeRequest, CodeResponse};
pub use session::{
    SessionParser, SessionScanner, ParsedSession, session_to_objects,
    ChunkedSessionParser, ChunkedSession, ConversationChunk,
    FileRef, FileOperation, chunked_session_to_objects,
};
pub use research::{
    Source, SourceType, CitationStyle,
    CitationGenerator, WebFetcher, FetchedContent, ContentMetadata,
    SourceConnection, FactCheckResult, WebSearchResult,
};

// Re-export universal importers
pub use importers::{
    Importer, ImportResult, detect_source,
    ChatGptImporter, CursorImporter, ObsidianImporter,
};
