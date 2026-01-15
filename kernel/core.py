"""
Semantic Kernel Core
====================

The heart of Semantic OS. All operations are semantic.
All state is vectors in memory pool.

Now with PyQt integration for IDE use.

Implements the semantic primitives:
    ctx.attach(path, intent)  - Open document context
    ctx.detach(path)          - Close document context
    ctx.emit(event, data)     - Emit semantic event
    ctx.invoke(action, args)  - Request action
    ctx.link(src, dst, rel)   - Create relationship
    ctx.query(q)              - Query semantic memory

Usage (CLI):
    kernel = SemanticKernel()
    handle = kernel.attach("doc.txt", Intent.EDIT)

Usage (PyQt IDE):
    kernel = SemanticKernel(enable_qt=True)
    kernel.context_attached.connect(on_attach)
"""

import time
import uuid
import threading
from collections import defaultdict
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Callable, Tuple, Set, TYPE_CHECKING
from enum import Enum

from .memory import SemanticMemory, MemoryEntry, MemoryType

# Optional PyQt support
try:
    from PyQt6.QtCore import QObject, pyqtSignal
    HAS_PYQT = True
except ImportError:
    HAS_PYQT = False
    QObject = object
    pyqtSignal = lambda *args: None


# ==================== IDE COMPATIBILITY: RELATION GRAPH ====================

class RelationGraph:
    """Semantic relationship graph between documents.

    Tracks typed relationships (implements, references, supersedes, etc.)
    between documents. Enables queries like:
    - "What documents implement this spec?"
    - "What are all the references to this document?"

    Supports optional persistence via SemanticMemory.
    """

    # Relations that should propagate changes
    PROPAGATING_RELATIONS = {
        "implements", "extends", "requires", "references",
        "derived_from", "part_of",
    }

    def __init__(self, emit_callback: Callable = None, memory: 'SemanticMemory' = None):
        self._emit = emit_callback or (lambda *args: None)
        self._memory = memory  # For persistence
        # Forward edges: source_path -> [(target_path, relation, label, metadata)]
        self._edges: Dict[str, List[tuple]] = defaultdict(list)
        # Reverse edges: target_path -> [(source_path, relation, label, metadata)]
        self._reverse_edges: Dict[str, List[tuple]] = defaultdict(list)
        # Change handlers
        self._change_handlers: Dict[str, List[Callable]] = defaultdict(list)

        # Load persisted relations
        if memory:
            self._load_from_storage()

    def _load_from_storage(self):
        """Load relations from persistent storage."""
        if not self._memory:
            return
        try:
            relations = self._memory.load_relations()
            for rel in relations:
                edge = (rel["target"], rel["relation"], rel.get("label"), rel.get("metadata", {}))
                self._edges[rel["source"]].append(edge)
                # Add reverse edge
                inverse_rel = self._get_inverse(rel["relation"])
                reverse_edge = (rel["source"], inverse_rel, rel.get("label"), rel.get("metadata", {}))
                self._reverse_edges[rel["target"]].append(reverse_edge)
        except Exception:
            pass  # Gracefully handle missing table on first run

    def add_link(self, source_path: str, target_path: str, relation: str,
                 label: str = None, metadata: Dict = None) -> bool:
        """Add a semantic link to the graph."""
        edge = (target_path, relation, label, metadata or {})
        if edge in self._edges[source_path]:
            return False

        self._edges[source_path].append(edge)

        # Add reverse edge
        inverse_rel = self._get_inverse(relation)
        reverse_edge = (source_path, inverse_rel, label, metadata or {})
        self._reverse_edges[target_path].append(reverse_edge)

        # Persist to storage
        if self._memory:
            self._memory.save_relation(source_path, target_path, relation, label, metadata)

        self._emit("relation.added", {
            "source": source_path,
            "target": target_path,
            "relation": relation,
        })
        return True

    def _get_inverse(self, relation: str) -> str:
        """Get inverse relation type."""
        inverses = {
            "implements": "implemented_by",
            "extends": "extended_by",
            "requires": "required_by",
            "references": "referenced_by",
            "derived_from": "source_of",
            "part_of": "contains",
            "supersedes": "superseded_by",
        }
        return inverses.get(relation, "related_to")

    def remove_link(self, source_path: str, target_path: str,
                    relation: str = None) -> int:
        """Remove link(s). If relation is None, removes all links to target."""
        original = self._edges[source_path]
        if relation:
            self._edges[source_path] = [
                e for e in original if not (e[0] == target_path and e[1] == relation)
            ]
        else:
            self._edges[source_path] = [e for e in original if e[0] != target_path]
        removed = len(original) - len(self._edges[source_path])

        # Remove reverse edges
        if target_path in self._reverse_edges:
            self._reverse_edges[target_path] = [
                e for e in self._reverse_edges[target_path] if e[0] != source_path
            ]

        # Persist deletion
        if removed > 0 and self._memory:
            self._memory.delete_relation(source_path, target_path, relation)

        if removed > 0:
            self._emit("relation.removed", {
                "source": source_path, "target": target_path, "count": removed,
            })
        return removed

    def get_outgoing(self, path: str, relation: str = None) -> List[Dict]:
        """Get all outgoing links from a document."""
        result = []
        for target, rel, label, meta in self._edges.get(path, []):
            if relation is None or rel == relation:
                result.append({
                    "target": target, "relation": rel, "label": label, "metadata": meta,
                })
        return result

    def get_incoming(self, path: str, relation: str = None) -> List[Dict]:
        """Get all incoming links to a document."""
        result = []
        for source, rel, label, meta in self._reverse_edges.get(path, []):
            if relation is None or rel == relation:
                result.append({
                    "source": source, "relation": rel, "label": label, "metadata": meta,
                })
        return result

    def find_by_relation(self, relation: str) -> List[Dict]:
        """Find all links with a specific relation type."""
        result = []
        for source, edges in self._edges.items():
            for target, rel, label, meta in edges:
                if rel == relation:
                    result.append({
                        "source": source, "target": target, "label": label, "metadata": meta,
                    })
        return result

    def get_related(self, path: str, max_depth: int = 1) -> Set[str]:
        """Get all documents related to this one, up to max_depth."""
        visited = set()
        frontier = {path}

        for _ in range(max_depth):
            next_frontier = set()
            for p in frontier:
                if p in visited:
                    continue
                visited.add(p)
                for edge in self._edges.get(p, []):
                    next_frontier.add(edge[0])
                for edge in self._reverse_edges.get(p, []):
                    next_frontier.add(edge[0])
            frontier = next_frontier - visited

        visited.discard(path)
        return visited

    def get_graph_summary(self) -> Dict:
        """Get a summary of the relation graph."""
        relation_counts = defaultdict(int)
        for edges in self._edges.values():
            for _, rel, _, _ in edges:
                relation_counts[rel] += 1
        return {
            "nodes": len(self._edges) + len(self._reverse_edges),
            "edges": sum(len(e) for e in self._edges.values()),
            "relation_types": dict(relation_counts),
        }

    def get_affected_by_change(self, path: str) -> List[Dict]:
        """Get documents affected if this document changes."""
        affected = []
        for source_path, edges in self._edges.items():
            for target, rel, label, meta in edges:
                if target == path and rel in self.PROPAGATING_RELATIONS:
                    affected.append({
                        "path": source_path, "relation": rel, "label": label,
                    })
        return affected

    def notify_change(self, changed_path: str, change_type: str = "modified") -> List[str]:
        """Notify related documents that this document has changed."""
        notified = []
        for source_path, edges in self._edges.items():
            for target, rel, label, meta in edges:
                if target == changed_path and rel in self.PROPAGATING_RELATIONS:
                    if source_path in self._change_handlers:
                        for handler in self._change_handlers[source_path]:
                            try:
                                handler(changed_path, rel, {"change_type": change_type, **meta})
                            except Exception:
                                pass
                        notified.append(source_path)
        return notified

    def on_change(self, path: str, handler: Callable):
        """Register to be notified when related documents change."""
        self._change_handlers[path].append(handler)

    def off_change(self, path: str, handler: Callable = None):
        """Remove change handler(s)."""
        if handler:
            self._change_handlers[path] = [
                h for h in self._change_handlers[path] if h != handler
            ]
        else:
            self._change_handlers[path] = []

    def clear_path(self, path: str):
        """Remove all links from/to a path."""
        if path in self._edges:
            for target, _, _, _ in self._edges[path]:
                if target in self._reverse_edges:
                    self._reverse_edges[target] = [
                        e for e in self._reverse_edges[target] if e[0] != path
                    ]
            del self._edges[path]

            # Persist deletion
            if self._memory:
                self._memory.clear_relations_for_path(path)

        if path in self._change_handlers:
            del self._change_handlers[path]


# ==================== IDE COMPATIBILITY: GLOBAL SPINE ====================

class GlobalSpineBase:
    """Global event spine for cross-context communication (non-Qt version)."""

    def __init__(self, kernel_emit: Callable = None):
        self._kernel_emit = kernel_emit or (lambda *a, **kw: None)
        self._history: List[Dict] = []
        self._max_history = 5000

    def emit(self, event_type: str, payload: Any = None,
             source: str = None, target: str = None) -> Dict:
        """Emit a global event."""
        event = {
            "event_id": str(uuid.uuid4())[:8],
            "event_type": event_type,
            "timestamp": time.time(),
            "source": source,
            "target": target,
            "payload": payload,
        }
        self._history.append(event)
        if len(self._history) > self._max_history:
            self._history = self._history[-self._max_history:]

        # Forward to kernel
        self._kernel_emit(event_type, payload, source, target)
        return event

    def query(self, event_type: str = None, context_id: str = None,
              since: float = None, limit: int = 100) -> List[Dict]:
        """Query the global event history."""
        result = self._history
        if event_type:
            result = [e for e in result if e["event_type"] == event_type]
        if context_id:
            result = [e for e in result
                     if e["source"] == context_id or e["target"] == context_id]
        if since:
            result = [e for e in result if e["timestamp"] >= since]
        return result[-limit:]


# ==================== IDE COMPATIBILITY: CONTEXT PRIMITIVES ====================

class ContextPrimitives:
    """Context primitives API (ctx.attach, ctx.emit, ctx.invoke, ctx.link).

    This wraps a Handle and provides the IDE-compatible API.
    """

    def __init__(self, handle: 'Handle', kernel: 'SemanticKernelBase'):
        self._handle = handle
        self._kernel = kernel
        self._attachments: Dict[str, Any] = {}
        self._handlers: Dict[str, List[Callable]] = {}

    @property
    def id(self) -> str:
        return self._handle.id

    @property
    def path(self) -> Optional[str]:
        return self._handle.path

    @property
    def context_id(self) -> str:
        """Alias for id (IDE compatibility)."""
        return self._handle.id

    @property
    def context_type(self) -> str:
        return self._handle.context_type

    @property
    def ai_access(self) -> str:
        return self._handle.ai_access

    @property
    def manifest(self):
        """Get the manifest for this context."""
        return self._handle.metadata.get("manifest")

    @property
    def can_write(self) -> bool:
        return self._handle.can_write

    @property
    def can_emit(self) -> bool:
        return self._handle.can_emit

    @property
    def can_invoke(self) -> bool:
        return self._handle.can_invoke

    @property
    def can_link(self) -> bool:
        return self._handle.can_link

    def attach(self, key: str, value: Any, provider_id: str = None):
        """Attach data to this context."""
        if not self._handle.can_write:
            raise PermissionError(f"Context {self.id} cannot write (attach)")
        self._attachments[key] = value
        self._kernel.emit("context.attach", {"key": key}, source=self.id)

    def get(self, key: str, default: Any = None) -> Any:
        """Get attached data."""
        return self._attachments.get(key, default)

    def emit(self, event_type: str, payload: Any = None, target: str = None):
        """Emit an event from this context."""
        if not self._handle.can_emit:
            raise PermissionError(f"Context {self.id} cannot emit")
        return self._kernel.emit(event_type, payload, source=self.id, target=target)

    def on(self, event_type: str, handler: Callable):
        """Subscribe to events."""
        if event_type not in self._handlers:
            self._handlers[event_type] = []
        self._handlers[event_type].append(handler)
        # Also subscribe at kernel level
        self._kernel.subscribe(event_type, handler)

    def invoke(self, command: str, **kwargs):
        """Invoke a command in this context."""
        if not self._handle.can_invoke:
            raise PermissionError(f"Context {self.id} cannot invoke commands")
        return self._kernel.invoke(command, **kwargs)

    def link(self, target_id: str):
        """Link this context to another."""
        if not self._handle.can_link:
            raise PermissionError(f"Context {self.id} cannot link")
        target_handle = self._kernel.get_handle(target_id)
        if target_handle:
            self._kernel.link(self._handle.path, target_handle.path, "related_to")

    def unlink(self, target_id: str):
        """Unlink from another context."""
        target_handle = self._kernel.get_handle(target_id)
        if target_handle and self._handle.path and target_handle.path:
            self._kernel.relations.remove_link(self._handle.path, target_handle.path)

    def get_links(self) -> List[str]:
        """Get all linked context IDs."""
        if not self._handle.path:
            return []
        related = self._kernel.relations.get_outgoing(self._handle.path)
        result = []
        for r in related:
            # Find context by path
            h = self._kernel.get_handle_by_path(r["target"])
            if h:
                result.append(h.id)
        return result

    # Semantic relation methods
    def add_relation(self, target_path: str, relation: str,
                     label: str = None, metadata: Dict = None):
        """Add a semantic relation to another document."""
        if not self._handle.path:
            raise ValueError("Context has no path")
        if not self._handle.can_link:
            raise PermissionError(f"Context {self.id} cannot create relations")
        self._kernel.relations.add_link(
            self._handle.path, target_path, relation, label, metadata
        )

    def get_outgoing_relations(self, relation: str = None) -> List[Dict]:
        """Get all outgoing semantic relations."""
        if not self._handle.path:
            return []
        return self._kernel.relations.get_outgoing(self._handle.path, relation)

    def get_incoming_relations(self, relation: str = None) -> List[Dict]:
        """Get all incoming semantic relations."""
        if not self._handle.path:
            return []
        return self._kernel.relations.get_incoming(self._handle.path, relation)

    def notify_change(self, change_type: str = "modified") -> List[str]:
        """Notify related documents that this document has changed."""
        if not self._handle.path:
            return []
        return self._kernel.relations.notify_change(self._handle.path, change_type)


class Intent(str, Enum):
    """Document access intents."""
    READ = "read"
    EDIT = "edit"
    CREATE = "create"
    DELETE = "delete"


@dataclass
class Handle:
    """An open document handle."""
    id: str                         # Handle ID
    document_id: str                # Memory entry ID for document
    path: str                       # Original path
    intent: Intent                  # Access intent
    process_id: str                 # Owning process
    created_at: float = field(default_factory=time.time)

    # Current state
    position: int = 0               # Read/write position
    content_buffer: str = ""        # Buffered content
    modified: bool = False          # Has unsaved changes

    # IDE compatibility
    context_type: str = "document"
    ai_access: str = "observe"
    metadata: Dict[str, Any] = field(default_factory=dict)

    # Permissions (for IDE compatibility)
    can_write: bool = True
    can_emit: bool = True
    can_invoke: bool = True
    can_link: bool = True


@dataclass
class Process:
    """A process context."""
    id: str                         # Process ID
    name: str                       # Process name (e.g., "notepad.exe")
    handles: Dict[str, Handle] = field(default_factory=dict)  # Open handles
    created_at: float = field(default_factory=time.time)

    # Process memory (for simulated address space)
    memory: Dict[int, bytes] = field(default_factory=dict)


class SemanticKernelBase:
    """Base kernel without PyQt (for CLI use)."""

    def __init__(self, db_path: str = None):
        self.memory = SemanticMemory(db_path)
        self.processes: Dict[str, Process] = {}
        self.handles: Dict[str, Handle] = {}
        self._lock = threading.RLock()

        # Event subscribers (callback-based)
        self._subscribers: Dict[str, List[Callable]] = {}

        # Action handlers
        self._action_handlers: Dict[str, Callable] = {}

        # Path to handle mapping for IDE compatibility
        self._path_index: Dict[str, str] = {}

        # Context primitives (IDE compatibility)
        self._primitives: Dict[str, ContextPrimitives] = {}

        # Relation graph (IDE compatibility) - with persistence
        self.relations = RelationGraph(
            emit_callback=self._emit_relation_event,
            memory=self.memory  # Pass memory for persistent storage
        )

        # Global spine (non-Qt version, overridden in subclass)
        self.spine = GlobalSpineBase(kernel_emit=self.emit)

        # Initialize built-in actions
        self._register_builtin_actions()

        # Context layer (lazy loaded)
        self._context_aggregator = None

        # Boot
        self._boot_time = time.time()
        self.emit("kernel.boot", {"time": self._boot_time})

    def _emit_relation_event(self, event: str, data: Any):
        """Emit a relation event."""
        self.emit(event, data)

    def _register_builtin_actions(self):
        """Register built-in action handlers."""
        self.register_action("document.save", self._action_save)
        self.register_action("document.delete", self._action_delete)
        self.register_action("document.copy", self._action_copy)
        self.register_action("document.move", self._action_move)

    # ==================== PROCESS MANAGEMENT ====================

    def create_process(self, name: str, pid: str = None) -> Process:
        """Create a new process context."""
        proc_id = pid or str(uuid.uuid4())[:8]

        proc = Process(id=proc_id, name=name)
        self.processes[proc_id] = proc

        self.memory.store(
            content=f"Process {name} started",
            type=MemoryType.PROCESS,
            metadata={"pid": proc_id, "name": name, "state": "running"},
            id=f"proc_{proc_id}",
        )

        self.emit("process.created", {"pid": proc_id, "name": name})
        return proc

    def get_process(self, pid: str) -> Optional[Process]:
        """Get process by ID."""
        return self.processes.get(pid)

    def terminate_process(self, pid: str):
        """Terminate a process."""
        proc = self.processes.get(pid)
        if not proc:
            return

        for handle_id in list(proc.handles.keys()):
            handle = proc.handles[handle_id]
            self.detach(handle)

        self.memory.update(f"proc_{pid}", metadata={"state": "terminated"})
        del self.processes[pid]

        self.emit("process.terminated", {"pid": pid})

    # ==================== SEMANTIC PRIMITIVES ====================

    def attach(self, path: str, intent: Intent = Intent.READ,
               process_id: str = None, context_type: str = "document",
               metadata: Dict = None) -> Handle:
        """ctx.attach - Open/access a document context."""
        with self._lock:
            doc_id = f"doc_{path.replace('/', '_').replace('.', '_').replace(':', '_')}"

            existing = self.memory.get(doc_id)

            if existing:
                document_id = existing.id
                content = existing.content
            elif intent == Intent.CREATE:
                document_id = self.memory.store(
                    content="",
                    type=MemoryType.DOCUMENT,
                    metadata={"path": path, "created_by": process_id},
                    id=doc_id,
                )
                content = ""
            else:
                document_id = self.memory.store(
                    content="",
                    type=MemoryType.DOCUMENT,
                    metadata={"path": path, "virtual": True},
                    id=doc_id,
                )
                content = ""

            handle_id = str(uuid.uuid4())[:8]
            handle = Handle(
                id=handle_id,
                document_id=document_id,
                path=path,
                intent=intent,
                process_id=process_id or "kernel",
                content_buffer=content,
                context_type=context_type,
                metadata=metadata or {},
            )

            self.handles[handle_id] = handle
            self._path_index[path] = handle_id

            if process_id and process_id in self.processes:
                self.processes[process_id].handles[handle_id] = handle

            self.memory.store(
                content=f"Handle opened for {path} with intent {intent.value}",
                type=MemoryType.HANDLE,
                metadata={
                    "handle_id": handle_id,
                    "document_id": document_id,
                    "path": path,
                    "intent": intent.value,
                    "process_id": process_id,
                    "context_type": context_type,
                },
                id=f"handle_{handle_id}",
                links=[document_id],
            )

            self.emit("context.attached", {
                "handle_id": handle_id,
                "path": path,
                "intent": intent.value,
                "context_type": context_type,
            })

            # Emit PyQt signal if available (overridden in subclass)
            self._emit_qt_attached(handle)

            return handle

    def detach(self, handle: Handle):
        """ctx.detach - Close/release a document context."""
        with self._lock:
            if handle.modified:
                self._flush_handle(handle)

            if handle.id in self.handles:
                del self.handles[handle.id]

            if handle.path in self._path_index:
                del self._path_index[handle.path]

            if handle.process_id in self.processes:
                proc = self.processes[handle.process_id]
                if handle.id in proc.handles:
                    del proc.handles[handle.id]

            self.memory.update(f"handle_{handle.id}", metadata={"state": "closed"})

            self.emit("context.detached", {
                "handle_id": handle.id,
                "path": handle.path,
            })

            self._emit_qt_detached(handle.id)

    def emit(self, event: str, data: Any = None, source: str = None, target: str = None):
        """ctx.emit - Emit a semantic event."""
        event_content = f"{event}: {data}" if data else event
        self.memory.store(
            content=event_content,
            type=MemoryType.EVENT,
            metadata={
                "event": event,
                "data": data,
                "source": source,
                "target": target,
                "timestamp": time.time(),
            },
        )

        # Callback subscribers
        if event in self._subscribers:
            for callback in self._subscribers[event]:
                try:
                    callback(event, data)
                except Exception as e:
                    print(f"[Kernel] Event handler error: {e}")

        if "*" in self._subscribers:
            for callback in self._subscribers["*"]:
                try:
                    callback(event, data)
                except Exception:
                    pass

        # PyQt signal (overridden in subclass)
        self._emit_qt_event(event, data, source, target)

    def invoke(self, action: str, **params) -> Any:
        """ctx.invoke - Request an action."""
        self.memory.store(
            content=f"Invoke {action} with {params}",
            type=MemoryType.EVENT,
            metadata={"event": "action.invoked", "action": action, "params": params},
        )

        if action in self._action_handlers:
            try:
                result = self._action_handlers[action](**params)
                self.emit("action.completed", {"action": action, "result": result})
                return result
            except Exception as e:
                self.emit("action.failed", {"action": action, "error": str(e)})
                raise
        else:
            self.emit("action.unknown", {"action": action})
            return None

    def link(self, source: str, target: str, relation: str) -> bool:
        """ctx.link - Create relationship between documents."""
        source_id = f"doc_{source.replace('/', '_').replace('.', '_').replace(':', '_')}"
        target_id = f"doc_{target.replace('/', '_').replace('.', '_').replace(':', '_')}"

        success = self.memory.link(source_id, target_id, relation)

        if success:
            self.emit("documents.linked", {
                "source": source,
                "target": target,
                "relation": relation,
            })

        return success

    def query(self, q: str, **filters) -> List[Tuple[MemoryEntry, float]]:
        """ctx.query - Query semantic memory."""
        type_filter = filters.pop("type", None)
        if isinstance(type_filter, str):
            type_filter = MemoryType(type_filter)

        return self.memory.query(query=q, type=type_filter, **filters)

    # ==================== IDE COMPATIBILITY ====================

    def get_handle(self, handle_id: str) -> Optional[Handle]:
        """Get handle by ID (IDE: get_context)."""
        return self.handles.get(handle_id)

    def get_handle_by_path(self, path: str) -> Optional[Handle]:
        """Get handle by path (IDE: get_context_by_path)."""
        handle_id = self._path_index.get(path)
        return self.handles.get(handle_id) if handle_id else None

    def list_handles(self) -> List[Handle]:
        """List all handles (IDE: list_all)."""
        return list(self.handles.values())

    # Aliases for IDE compatibility
    def create_context(self, path: str = None, context_type: str = "document",
                       metadata: Dict = None) -> ContextPrimitives:
        """IDE compatibility: create_context -> attach, returns ContextPrimitives."""
        handle = self.attach(
            path=path or f"untitled_{uuid.uuid4().hex[:8]}",
            intent=Intent.CREATE,
            context_type=context_type,
            metadata=metadata,
        )
        # Create and store ContextPrimitives wrapper
        primitives = ContextPrimitives(handle, self)
        self._primitives[handle.id] = primitives
        return primitives

    def get_context(self, context_id: str) -> Optional[ContextPrimitives]:
        """IDE compatibility: get ContextPrimitives by ID."""
        return self._primitives.get(context_id)

    def get_context_by_path(self, path: str) -> Optional[ContextPrimitives]:
        """IDE compatibility: get ContextPrimitives by path."""
        handle = self.get_handle_by_path(path)
        return self._primitives.get(handle.id) if handle else None

    def destroy_context(self, context_id: str):
        """IDE compatibility: destroy_context -> detach."""
        handle = self.handles.get(context_id)
        if handle:
            # Clean up relation graph
            if handle.path:
                self.relations.clear_path(handle.path)
            # Remove primitives
            if context_id in self._primitives:
                del self._primitives[context_id]
            self.detach(handle)

    # ==================== READ/WRITE OPERATIONS ====================

    def read(self, handle: Handle, size: int = -1) -> str:
        """Read from a document handle."""
        if size < 0:
            return handle.content_buffer[handle.position:]
        else:
            content = handle.content_buffer[handle.position:handle.position + size]
            handle.position += len(content)
            return content

    def write(self, handle: Handle, content: str) -> int:
        """Write to a document handle."""
        if handle.intent not in (Intent.EDIT, Intent.CREATE):
            raise PermissionError("Handle not opened for writing")

        buffer = handle.content_buffer
        handle.content_buffer = (
            buffer[:handle.position] + content + buffer[handle.position:]
        )
        handle.position += len(content)
        handle.modified = True

        self.emit("content.changed", {
            "handle_id": handle.id,
            "path": handle.path,
            "bytes": len(content),
        })

        return len(content)

    def set_content(self, handle: Handle, content: str):
        """Set full content of handle (IDE compatibility)."""
        handle.content_buffer = content
        handle.modified = True
        self.emit("content.changed", {
            "handle_id": handle.id,
            "path": handle.path,
            "bytes": len(content),
        })

    def get_content(self, handle: Handle) -> str:
        """Get full content of handle."""
        return handle.content_buffer

    def seek(self, handle: Handle, position: int):
        """Seek to position in document."""
        handle.position = max(0, min(position, len(handle.content_buffer)))

    def _flush_handle(self, handle: Handle):
        """Flush handle buffer to memory."""
        if not handle.modified:
            return

        self.memory.update(
            handle.document_id,
            content=handle.content_buffer,
            metadata={"last_modified": time.time()},
        )
        handle.modified = False

        self.emit("content.saved", {"handle_id": handle.id, "path": handle.path})

    # ==================== SUBSCRIPTIONS ====================

    def subscribe(self, event: str, callback: Callable):
        """Subscribe to semantic events."""
        if event not in self._subscribers:
            self._subscribers[event] = []
        self._subscribers[event].append(callback)

    def unsubscribe(self, event: str, callback: Callable):
        """Unsubscribe from events."""
        if event in self._subscribers:
            self._subscribers[event] = [
                cb for cb in self._subscribers[event] if cb != callback
            ]

    # ==================== ACTION HANDLERS ====================

    def register_action(self, action: str, handler: Callable):
        """Register an action handler."""
        self._action_handlers[action] = handler

    def _action_save(self, handle_id: str = None, path: str = None) -> bool:
        if handle_id:
            handle = self.handles.get(handle_id)
            if handle:
                self._flush_handle(handle)
                return True
        return False

    def _action_delete(self, path: str) -> bool:
        doc_id = f"doc_{path.replace('/', '_').replace('.', '_').replace(':', '_')}"
        return self.memory.delete(doc_id)

    def _action_copy(self, source: str, destination: str) -> bool:
        source_id = f"doc_{source.replace('/', '_').replace('.', '_').replace(':', '_')}"
        source_doc = self.memory.get(source_id)

        if not source_doc:
            return False

        dest_id = f"doc_{destination.replace('/', '_').replace('.', '_').replace(':', '_')}"
        self.memory.store(
            content=source_doc.content,
            type=MemoryType.DOCUMENT,
            metadata={**source_doc.metadata, "path": destination, "copied_from": source},
            id=dest_id,
        )

        self.link(destination, source, "copied_from")
        return True

    def _action_move(self, source: str, destination: str) -> bool:
        if self._action_copy(source, destination):
            return self._action_delete(source)
        return False

    # ==================== UTILITIES ====================

    def get_documents(self, limit: int = 100) -> List[MemoryEntry]:
        """Get all documents."""
        return self.memory.get_by_type(MemoryType.DOCUMENT, limit)

    def get_events(self, limit: int = 100) -> List[MemoryEntry]:
        """Get recent events."""
        return self.memory.get_by_type(MemoryType.EVENT, limit)

    def get_open_handles(self) -> List[Handle]:
        """Get all open handles."""
        return list(self.handles.values())

    def get_system_state(self) -> Dict:
        """Get current kernel state (IDE compatibility)."""
        handle_list = []
        for h in self.handles.values():
            handle_list.append({
                "id": h.id,
                "path": h.path,
                "type": h.context_type,
                "intent": h.intent.value,
                "modified": h.modified,
            })

        return {
            "contexts": len(self.handles),
            "context_list": handle_list,
            "recent_events": [
                {"event": e.metadata.get("event"), "data": e.metadata.get("data")}
                for e in self.get_events(10)
            ],
            "memory": self.memory.stats(),
        }

    def stats(self) -> Dict[str, Any]:
        """Get kernel statistics."""
        return {
            "uptime": time.time() - self._boot_time,
            "processes": len(self.processes),
            "open_handles": len(self.handles),
            "memory": self.memory.stats(),
            "subscribers": {k: len(v) for k, v in self._subscribers.items()},
            "actions": list(self._action_handlers.keys()),
        }

    # ==================== CONTEXT LAYER METHODS ====================

    def _get_context_aggregator(self):
        """Lazy-load the context aggregator."""
        if self._context_aggregator is None:
            from ide.context_layer import ContextAggregator
            self._context_aggregator = ContextAggregator(self)
        return self._context_aggregator

    def get_workflow_context(self, time_window: str = "today"):
        """Get comprehensive workflow context for time period.

        Args:
            time_window: 'today', 'this_week', 'this_month', or 'all'

        Returns:
            WorkflowContext object with current focus, recent work, projects, etc.
        """
        aggregator = self._get_context_aggregator()
        return aggregator.get_workflow_context(time_window)

    def get_current_focus(self, limit: int = 10):
        """Get currently focused documents ranked by attention.

        Args:
            limit: Maximum number of documents to return

        Returns:
            List of DocumentFocus objects
        """
        aggregator = self._get_context_aggregator()
        return aggregator.get_current_focus(limit)

    def export_context(self, format: str = "markdown", time_window: str = "today") -> str:
        """Export context in specified format.

        Args:
            format: 'markdown', 'json', or 'compact'
            time_window: 'today', 'this_week', 'this_month', or 'all'

        Returns:
            Formatted context string
        """
        aggregator = self._get_context_aggregator()
        return aggregator.export_context(format, time_window)

    def record_focus(self, doc_path: str):
        """Record that user is focusing on a document."""
        aggregator = self._get_context_aggregator()
        aggregator.record_focus(doc_path)

    def record_edit(self, doc_path: str, section: str = None):
        """Record an edit to a document."""
        aggregator = self._get_context_aggregator()
        aggregator.record_edit(doc_path, section)

    def shutdown(self):
        """Shutdown the kernel."""
        self.emit("kernel.shutdown", {"uptime": time.time() - self._boot_time})

        for pid in list(self.processes.keys()):
            self.terminate_process(pid)

        self.memory.close()

    # ==================== PyQt STUBS (overridden in subclass) ====================

    def _emit_qt_attached(self, handle: Handle):
        pass

    def _emit_qt_detached(self, handle_id: str):
        pass

    def _emit_qt_event(self, event: str, data: Any, source: str, target: str):
        pass


# PyQt-enabled GlobalSpine
if HAS_PYQT:
    class GlobalSpine(QObject, GlobalSpineBase):
        """Global event spine with PyQt signals for IDE integration."""

        # Signal emitted for every event (IDE connects to this)
        event_broadcast = pyqtSignal(object)  # event dict
        context_created = pyqtSignal(object)  # handle/context
        context_destroyed = pyqtSignal(str)   # context_id
        context_focused = pyqtSignal(str)     # context_id

        def __init__(self, kernel_emit: Callable = None):
            QObject.__init__(self)
            GlobalSpineBase.__init__(self, kernel_emit)

        def emit(self, event_type: str, payload: Any = None,
                 source: str = None, target: str = None) -> Dict:
            """Emit a global event with PyQt signal."""
            event = GlobalSpineBase.emit(self, event_type, payload, source, target)
            self.event_broadcast.emit(event)
            return event

        def emit_context_created(self, handle):
            """Emit context created signal."""
            self.context_created.emit(handle)

        def emit_context_destroyed(self, context_id: str):
            """Emit context destroyed signal."""
            self.context_destroyed.emit(context_id)

        def emit_context_focused(self, context_id: str):
            """Emit context focused signal."""
            self.context_focused.emit(context_id)
else:
    GlobalSpine = GlobalSpineBase


# PyQt-enabled kernel
if HAS_PYQT:
    class SemanticKernel(QObject, SemanticKernelBase):
        """Semantic Kernel with PyQt signals for IDE integration."""

        # PyQt signals
        context_attached = pyqtSignal(object)   # Handle
        context_detached = pyqtSignal(str)      # handle_id
        context_focused = pyqtSignal(str)       # handle_id
        event_emitted = pyqtSignal(str, object) # event, data
        kernel_ready = pyqtSignal()
        kernel_shutdown_signal = pyqtSignal()

        # Context layer signals
        focus_changed = pyqtSignal(str)         # doc_path
        session_started = pyqtSignal(object)    # Session object
        pattern_detected = pyqtSignal(str)      # pattern description

        def __init__(self, db_path: str = None, enable_qt: bool = True):
            QObject.__init__(self)
            self._enable_qt = enable_qt  # Set before base init
            SemanticKernelBase.__init__(self, db_path)
            # Replace spine with PyQt-enabled version
            if enable_qt:
                self.spine = GlobalSpine(kernel_emit=self.emit)
                self.kernel_ready.emit()

        def _emit_qt_attached(self, handle: Handle):
            if getattr(self, '_enable_qt', False):
                self.context_attached.emit(handle)

        def _emit_qt_detached(self, handle_id: str):
            if getattr(self, '_enable_qt', False):
                self.context_detached.emit(handle_id)

        def _emit_qt_event(self, event: str, data: Any, source: str, target: str):
            if getattr(self, '_enable_qt', False):
                self.event_emitted.emit(event, data)

        def focus_context(self, handle_id: str):
            """Signal that a context is focused."""
            if self._enable_qt:
                self.context_focused.emit(handle_id)
            self.emit("context.focused", {"handle_id": handle_id})

        def shutdown(self):
            """Shutdown with PyQt signal."""
            if self._enable_qt:
                self.kernel_shutdown_signal.emit()
            SemanticKernelBase.shutdown(self)

else:
    # Fallback: no PyQt
    class SemanticKernel(SemanticKernelBase):
        """Semantic Kernel (without PyQt)."""

        def __init__(self, db_path: str = None, enable_qt: bool = False):
            super().__init__(db_path)

        def focus_context(self, handle_id: str):
            self.emit("context.focused", {"handle_id": handle_id})


# ==================== GLOBAL INSTANCE ====================

_kernel: Optional[SemanticKernel] = None


def get_kernel() -> SemanticKernel:
    """Get the global kernel instance."""
    global _kernel
    if _kernel is None:
        _kernel = SemanticKernel()
    return _kernel


def init_kernel(db_path: str = None, enable_qt: bool = True) -> SemanticKernel:
    """Initialize the kernel (call once at startup)."""
    global _kernel
    _kernel = SemanticKernel(db_path, enable_qt)
    return _kernel
