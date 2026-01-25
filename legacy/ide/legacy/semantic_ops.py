"""
Semantic Operations
===================

Defines the semantic primitives that legacy API calls translate to.
These are the "system calls" of Semantic OS.

Core Primitives:
- ctx.attach(path, intent) - Open/access a document context
- ctx.detach(path) - Close/release a document context
- ctx.emit(event, data) - Emit a semantic event
- ctx.invoke(action, params) - Request an action
- ctx.link(source, target, relation) - Create semantic link
- ctx.query(query) - Query the semantic graph

All legacy app operations ultimately map to these primitives.
"""

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Union
from enum import Enum
import time
import json


class SemanticPrimitive(str, Enum):
    """Core semantic primitives."""
    ATTACH = "ctx.attach"       # Open/access document
    DETACH = "ctx.detach"       # Close/release document
    EMIT = "ctx.emit"           # Emit event
    INVOKE = "ctx.invoke"       # Request action
    LINK = "ctx.link"           # Create relationship
    QUERY = "ctx.query"         # Query semantic graph


class Intent(str, Enum):
    """Common intents for document operations."""
    READ = "read"               # Read-only access
    EDIT = "edit"               # Read-write access
    CREATE = "create"           # Create new document
    DELETE = "delete"           # Delete document
    COPY = "copy"               # Copy/duplicate
    MOVE = "move"               # Move/rename
    EXPORT = "export"           # Export to different format
    PRINT = "print"             # Print document
    SHARE = "share"             # Share with others


class EventType(str, Enum):
    """Semantic event types."""
    # Document events
    CONTENT_CHANGED = "content_changed"
    CONTENT_SAVED = "content_saved"
    SELECTION_CHANGED = "selection_changed"
    CURSOR_MOVED = "cursor_moved"

    # Clipboard events
    CONTENT_COPIED = "content_copied"
    CONTENT_CUT = "content_cut"
    CONTENT_PASTED = "content_pasted"

    # UI events
    USER_ACTION = "user_action"
    DIALOG_OPENED = "dialog_opened"
    DIALOG_CLOSED = "dialog_closed"
    MENU_SELECTED = "menu_selected"

    # Application events
    APP_FOCUSED = "app_focused"
    APP_BLURRED = "app_blurred"
    TOOL_CHANGED = "tool_changed"
    VIEW_CHANGED = "view_changed"


@dataclass
class SemanticOp:
    """A semantic operation translated from an API call.

    This is the universal format that all legacy API calls
    translate to before being executed by the semantic kernel.
    """
    primitive: SemanticPrimitive

    # Target (document path, event type, action name)
    target: str

    # Intent or event type
    intent: Optional[str] = None

    # Additional parameters
    params: Dict[str, Any] = field(default_factory=dict)

    # Source information
    source_api: Optional[str] = None      # Original API call
    source_app: Optional[str] = None      # App that made the call
    source_pid: Optional[int] = None      # Process ID

    # Metadata
    timestamp: float = field(default_factory=time.time)
    confidence: float = 1.0               # AI confidence (1.0 for pattern match)

    def to_dict(self) -> dict:
        """Convert to dictionary."""
        return {
            "primitive": self.primitive.value,
            "target": self.target,
            "intent": self.intent,
            "params": self.params,
            "source_api": self.source_api,
            "source_app": self.source_app,
            "timestamp": self.timestamp,
            "confidence": self.confidence,
        }

    def to_json(self) -> str:
        """Convert to JSON string."""
        return json.dumps(self.to_dict())

    @classmethod
    def from_dict(cls, data: dict) -> 'SemanticOp':
        """Create from dictionary."""
        return cls(
            primitive=SemanticPrimitive(data["primitive"]),
            target=data["target"],
            intent=data.get("intent"),
            params=data.get("params", {}),
            source_api=data.get("source_api"),
            source_app=data.get("source_app"),
            source_pid=data.get("source_pid"),
            timestamp=data.get("timestamp", time.time()),
            confidence=data.get("confidence", 1.0),
        )

    @classmethod
    def from_json(cls, json_str: str) -> 'SemanticOp':
        """Create from JSON string."""
        return cls.from_dict(json.loads(json_str))

    # Factory methods for common operations

    @classmethod
    def attach(cls, path: str, intent: Intent = Intent.READ, **params) -> 'SemanticOp':
        """Create an attach (open document) operation."""
        return cls(
            primitive=SemanticPrimitive.ATTACH,
            target=path,
            intent=intent.value if isinstance(intent, Intent) else intent,
            params=params,
        )

    @classmethod
    def detach(cls, path: str, **params) -> 'SemanticOp':
        """Create a detach (close document) operation."""
        return cls(
            primitive=SemanticPrimitive.DETACH,
            target=path,
            params=params,
        )

    @classmethod
    def emit(cls, event: EventType, data: Any = None, **params) -> 'SemanticOp':
        """Create an emit (event) operation."""
        event_name = event.value if isinstance(event, EventType) else event
        return cls(
            primitive=SemanticPrimitive.EMIT,
            target=event_name,
            params={"data": data, **params},
        )

    @classmethod
    def invoke(cls, action: str, **params) -> 'SemanticOp':
        """Create an invoke (action request) operation."""
        return cls(
            primitive=SemanticPrimitive.INVOKE,
            target=action,
            params=params,
        )

    @classmethod
    def link(cls, source: str, target: str, relation: str, **params) -> 'SemanticOp':
        """Create a link (relationship) operation."""
        return cls(
            primitive=SemanticPrimitive.LINK,
            target=target,
            params={"source": source, "relation": relation, **params},
        )

    @classmethod
    def query(cls, query: str, **params) -> 'SemanticOp':
        """Create a query operation."""
        return cls(
            primitive=SemanticPrimitive.QUERY,
            target=query,
            params=params,
        )


@dataclass
class APICallContext:
    """Context for an intercepted API call."""
    api_name: str                         # e.g., "CreateFileW"
    args: Dict[str, Any]                  # Call arguments
    return_value: Any = None              # Return value (if captured)

    # Process context
    pid: int = 0
    app_name: str = ""
    app_path: str = ""

    # Recent history (for AI context)
    recent_calls: List[str] = field(default_factory=list)

    # Current document context
    open_documents: List[str] = field(default_factory=list)
    active_document: Optional[str] = None

    # UI state
    window_title: str = ""
    focused_control: str = ""


@dataclass
class TranslationResult:
    """Result of translating an API call."""
    success: bool
    operation: Optional[SemanticOp] = None

    # If multiple operations result from one call
    operations: List[SemanticOp] = field(default_factory=list)

    # Translation metadata
    method: str = "pattern"               # "pattern", "ai", "hybrid"
    confidence: float = 1.0
    reasoning: str = ""

    # Pass-through flag (execute original syscall too)
    passthrough: bool = True

    def __post_init__(self):
        if self.operation and not self.operations:
            self.operations = [self.operation]


# Common API → Semantic mappings (training data format)
API_SEMANTIC_EXAMPLES = [
    # File operations
    {
        "api": "CreateFileW",
        "pattern": {"access": "GENERIC_READ"},
        "semantic": {"primitive": "ctx.attach", "intent": "read"}
    },
    {
        "api": "CreateFileW",
        "pattern": {"access": "GENERIC_WRITE"},
        "semantic": {"primitive": "ctx.attach", "intent": "edit"}
    },
    {
        "api": "CreateFileW",
        "pattern": {"creation": "CREATE_ALWAYS"},
        "semantic": {"primitive": "ctx.attach", "intent": "create"}
    },
    {
        "api": "CloseHandle",
        "pattern": {"handle_type": "file"},
        "semantic": {"primitive": "ctx.detach"}
    },
    {
        "api": "WriteFile",
        "pattern": {},
        "semantic": {"primitive": "ctx.emit", "event": "content_changed"}
    },
    {
        "api": "DeleteFileW",
        "pattern": {},
        "semantic": {"primitive": "ctx.invoke", "action": "document.delete"}
    },

    # Clipboard
    {
        "api": "SetClipboardData",
        "pattern": {},
        "semantic": {"primitive": "ctx.emit", "event": "content_copied"}
    },
    {
        "api": "GetClipboardData",
        "pattern": {},
        "semantic": {"primitive": "ctx.emit", "event": "content_pasted"}
    },

    # Dialogs
    {
        "api": "GetOpenFileNameW",
        "pattern": {},
        "semantic": {"primitive": "ctx.invoke", "action": "document.open_dialog"}
    },
    {
        "api": "GetSaveFileNameW",
        "pattern": {},
        "semantic": {"primitive": "ctx.invoke", "action": "document.save_dialog"}
    },
    {
        "api": "PrintDlgW",
        "pattern": {},
        "semantic": {"primitive": "ctx.invoke", "action": "document.print"}
    },

    # Window management
    {
        "api": "SetForegroundWindow",
        "pattern": {},
        "semantic": {"primitive": "ctx.emit", "event": "app_focused"}
    },
    {
        "api": "SetWindowTextW",
        "pattern": {},
        "semantic": {"primitive": "ctx.emit", "event": "view_changed"}
    },
]
