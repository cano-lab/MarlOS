//! Semantic Kernel - The core of MarlOS
//!
//! Provides:
//! - Global event spine for cross-context communication
//! - Context registry for managing active documents
//! - Memory integration for semantic storage

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::memory::MemoryStore;

/// A kernel event that can be broadcast across contexts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelEvent {
    pub id: String,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub source_context: Option<String>,
    pub target_context: Option<String>,
    pub payload: serde_json::Value,
}

impl KernelEvent {
    pub fn new(event_type: &str, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            event_type: event_type.to_string(),
            timestamp: Utc::now(),
            source_context: None,
            target_context: None,
            payload,
        }
    }

    pub fn with_source(mut self, source: &str) -> Self {
        self.source_context = Some(source.to_string());
        self
    }

    pub fn with_target(mut self, target: &str) -> Self {
        self.target_context = Some(target.to_string());
        self
    }
}

/// A handle to a registered context (document, window, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextHandle {
    pub id: String,
    pub path: Option<String>,
    pub context_type: String,
    pub created_at: DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ContextHandle {
    pub fn new(context_type: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            path: None,
            context_type: context_type.to_string(),
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }
}

/// The Semantic Kernel - manages contexts, events, and memory
pub struct SemanticKernel {
    /// Registry of active contexts
    contexts: RwLock<HashMap<String, ContextHandle>>,
    /// Event history (circular buffer)
    events: RwLock<Vec<KernelEvent>>,
    /// Maximum events to keep
    max_events: usize,
    /// Memory store for semantic search
    pub memory: Arc<MemoryStore>,
}

impl SemanticKernel {
    pub fn new() -> Self {
        log::info!("Initializing Semantic Kernel");

        Self {
            contexts: RwLock::new(HashMap::new()),
            events: RwLock::new(Vec::with_capacity(5000)),
            max_events: 5000,
            memory: Arc::new(MemoryStore::new().expect("Failed to initialize memory store")),
        }
    }

    /// Register a new context
    pub fn register_context(&self, handle: ContextHandle) -> String {
        let id = handle.id.clone();
        let mut contexts = self.contexts.write().unwrap();
        contexts.insert(id.clone(), handle);

        self.emit("context.registered", serde_json::json!({ "context_id": id }));

        log::debug!("Registered context: {}", id);
        id
    }

    /// Unregister a context
    pub fn unregister_context(&self, context_id: &str) -> Option<ContextHandle> {
        let mut contexts = self.contexts.write().unwrap();
        let handle = contexts.remove(context_id);

        if handle.is_some() {
            self.emit("context.unregistered", serde_json::json!({ "context_id": context_id }));
            log::debug!("Unregistered context: {}", context_id);
        }

        handle
    }

    /// Get a context by ID
    pub fn get_context(&self, context_id: &str) -> Option<ContextHandle> {
        let contexts = self.contexts.read().unwrap();
        contexts.get(context_id).cloned()
    }

    /// List all active contexts
    pub fn list_contexts(&self) -> Vec<ContextHandle> {
        let contexts = self.contexts.read().unwrap();
        contexts.values().cloned().collect()
    }

    /// Emit a kernel event
    pub fn emit(&self, event_type: &str, payload: serde_json::Value) -> KernelEvent {
        let event = KernelEvent::new(event_type, payload);

        let mut events = self.events.write().unwrap();
        events.push(event.clone());

        // Trim if over max
        if events.len() > self.max_events {
            let drain_count = events.len() - self.max_events;
            events.drain(0..drain_count);
        }

        log::trace!("Event: {} - {:?}", event_type, event.payload);
        event
    }

    /// Query event history
    pub fn query_events(
        &self,
        event_type: Option<&str>,
        context_id: Option<&str>,
        limit: usize,
    ) -> Vec<KernelEvent> {
        let events = self.events.read().unwrap();

        events
            .iter()
            .rev()
            .filter(|e| {
                event_type.map_or(true, |t| e.event_type == t)
                    && context_id.map_or(true, |c| {
                        e.source_context.as_deref() == Some(c)
                            || e.target_context.as_deref() == Some(c)
                    })
            })
            .take(limit)
            .cloned()
            .collect()
    }
}

impl Default for SemanticKernel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_lifecycle() {
        let kernel = SemanticKernel::new();

        let handle = ContextHandle::new("document").with_path("/test/file.md");
        let id = kernel.register_context(handle);

        assert!(kernel.get_context(&id).is_some());

        kernel.unregister_context(&id);
        assert!(kernel.get_context(&id).is_none());
    }

    #[test]
    fn test_event_emission() {
        let kernel = SemanticKernel::new();

        kernel.emit("test.event", serde_json::json!({ "data": "test" }));

        let events = kernel.query_events(Some("test.event"), None, 10);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "test.event");
    }
}
