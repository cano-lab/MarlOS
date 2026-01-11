"""
Semantic OS Kernel
==================

The kernel is the trusted, stable core of the Semantic OS.
It provides:
- Global spine for cross-context events
- Context registry for managing active contexts
- Context primitives (attach, emit, invoke, link)
- Secure API that providers use

Key principle: AI observes the kernel but never controls it.
The kernel is the "quiet and capable" foundation.
"""

from PyQt6.QtCore import QObject, pyqtSignal
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Callable, Set, TYPE_CHECKING
from datetime import datetime
from collections import defaultdict
import uuid

if TYPE_CHECKING:
    from ide.manifest import DocumentManifest, SemanticLink, LinkRelation


@dataclass
class ContextHandle:
    """A handle to a registered context."""
    context_id: str
    path: Optional[str] = None
    context_type: str = "document"  # document, pdf, image, etc.
    created_at: str = field(default_factory=lambda: datetime.now().isoformat())
    metadata: Dict[str, Any] = field(default_factory=dict)

    # Permissions (derived from manifest)
    can_write: bool = True
    can_emit: bool = True
    can_invoke: bool = True
    can_link: bool = True

    # AI access level from manifest
    ai_access: str = "observe"  # "none", "observe", "suggest", "edit"

    # Reference to the full manifest
    manifest: Optional['DocumentManifest'] = None


@dataclass
class KernelEvent:
    """A kernel-level event (global spine)."""
    event_id: str
    event_type: str
    timestamp: str
    source_context: Optional[str] = None
    target_context: Optional[str] = None
    payload: Any = None

    def to_dict(self) -> Dict:
        return {
            "event_id": self.event_id,
            "event_type": self.event_type,
            "timestamp": self.timestamp,
            "source": self.source_context,
            "target": self.target_context,
            "payload": str(self.payload) if self.payload else None,
        }


class GlobalSpine(QObject):
    """Global event spine for cross-context communication.

    This is the kernel's event bus. All inter-context events flow here.
    Local spines (EventsSpine) handle document-level events.
    """

    # Global signals
    context_created = pyqtSignal(object)  # ContextHandle
    context_destroyed = pyqtSignal(str)   # context_id
    context_focused = pyqtSignal(str)     # context_id

    # Cross-context events
    event_broadcast = pyqtSignal(object)  # KernelEvent

    def __init__(self):
        super().__init__()
        self._history: List[KernelEvent] = []
        self._max_history = 5000

    def emit(self, event_type: str, payload: Any = None,
             source: str = None, target: str = None):
        """Emit a global event."""
        event = KernelEvent(
            event_id=str(uuid.uuid4())[:8],
            event_type=event_type,
            timestamp=datetime.now().isoformat(),
            source_context=source,
            target_context=target,
            payload=payload,
        )
        self._history.append(event)

        if len(self._history) > self._max_history:
            self._history = self._history[-self._max_history:]

        self.event_broadcast.emit(event)
        return event

    def query(self, event_type: str = None, context_id: str = None,
              since: str = None, limit: int = 100) -> List[KernelEvent]:
        """Query the global event history."""
        result = self._history

        if event_type:
            result = [e for e in result if e.event_type == event_type]
        if context_id:
            result = [e for e in result
                     if e.source_context == context_id or e.target_context == context_id]
        if since:
            result = [e for e in result if e.timestamp >= since]

        return result[-limit:]

    def get_narrative(self, limit: int = 30) -> str:
        """Human-readable narrative of recent global events."""
        events = self._history[-limit:]
        if not events:
            return "No global events yet."

        lines = []
        for e in events:
            time = e.timestamp.split("T")[1].split(".")[0]
            ctx = f"[{e.source_context}]" if e.source_context else "[kernel]"
            lines.append(f"{time} {ctx} {e.event_type}")

        return "\n".join(lines)


class ContextRegistry:
    """Registry of all active contexts in the system.

    The registry tracks what's open, their relationships,
    and provides lookup capabilities.
    """

    def __init__(self, spine: GlobalSpine):
        self._spine = spine
        self._contexts: Dict[str, ContextHandle] = {}
        self._path_index: Dict[str, str] = {}  # path -> context_id
        self._links: Dict[str, List[str]] = {}  # context_id -> linked context_ids

    def register(self, path: str = None, context_type: str = "document",
                 metadata: Dict = None, manifest: 'DocumentManifest' = None) -> ContextHandle:
        """Register a new context with optional manifest."""
        from ide.manifest import ManifestLoader

        context_id = str(uuid.uuid4())[:8]

        # Load manifest if not provided and path exists
        if manifest is None and path:
            manifest = ManifestLoader.load_for_path(path)

        # Create handle with manifest-derived permissions
        handle = ContextHandle(
            context_id=context_id,
            path=path,
            context_type=manifest.document_type if manifest else context_type,
            metadata=metadata or {},
            can_write=manifest.default_can_write if manifest else True,
            can_emit=manifest.default_can_emit if manifest else True,
            can_invoke=manifest.default_can_invoke if manifest else True,
            can_link=True,
            ai_access=manifest.ai_access if manifest else "observe",
            manifest=manifest,
        )

        self._contexts[context_id] = handle
        if path:
            self._path_index[path] = context_id
        self._links[context_id] = []

        # Auto-link to related contexts from manifest
        if manifest and manifest.links:
            for link_path in manifest.links:
                linked = self.get_by_path(link_path)
                if linked:
                    self.link(context_id, linked.context_id)

        self._spine.emit("context.registered", {
            "id": context_id,
            "path": path,
            "type": handle.context_type,
            "ai_access": handle.ai_access,
        })
        self._spine.context_created.emit(handle)

        return handle

    def unregister(self, context_id: str):
        """Unregister a context."""
        if context_id not in self._contexts:
            return

        handle = self._contexts[context_id]
        if handle.path and handle.path in self._path_index:
            del self._path_index[handle.path]

        # Clean up links
        for linked in self._links.get(context_id, []):
            if linked in self._links:
                self._links[linked] = [l for l in self._links[linked] if l != context_id]
        del self._links[context_id]

        del self._contexts[context_id]

        self._spine.emit("context.unregistered", context_id)
        self._spine.context_destroyed.emit(context_id)

    def get(self, context_id: str) -> Optional[ContextHandle]:
        """Get a context by ID."""
        return self._contexts.get(context_id)

    def get_by_path(self, path: str) -> Optional[ContextHandle]:
        """Get a context by file path."""
        context_id = self._path_index.get(path)
        return self._contexts.get(context_id) if context_id else None

    def list_all(self) -> List[ContextHandle]:
        """List all registered contexts."""
        return list(self._contexts.values())

    def link(self, source_id: str, target_id: str):
        """Create a link between two contexts."""
        if source_id in self._links and target_id not in self._links[source_id]:
            self._links[source_id].append(target_id)
        if target_id in self._links and source_id not in self._links[target_id]:
            self._links[target_id].append(source_id)

        self._spine.emit("context.linked", {"source": source_id, "target": target_id})

    def unlink(self, source_id: str, target_id: str):
        """Remove a link between two contexts."""
        if source_id in self._links:
            self._links[source_id] = [l for l in self._links[source_id] if l != target_id]
        if target_id in self._links:
            self._links[target_id] = [l for l in self._links[target_id] if l != source_id]

    def get_links(self, context_id: str) -> List[str]:
        """Get all contexts linked to this one."""
        return self._links.get(context_id, []).copy()


class RelationGraph:
    """Semantic relationship graph between documents.

    This graph tracks typed relationships (implements, references, supersedes, etc.)
    between documents. It enables powerful queries like:
    - "What documents implement this spec?"
    - "What are all the references to this document?"
    - "What supersedes this document?"

    The graph is bidirectional: when A implements B, B is used_by A.
    """

    # Relations that should propagate changes (target should know when source changes)
    PROPAGATING_RELATIONS = {
        "implements",    # If spec changes, implementations should know
        "extends",       # If base changes, extensions should know
        "requires",      # If dependency changes, dependents should know
        "references",    # If referenced doc changes, referencer may care
        "derived_from",  # If original changes, derivation should know
        "part_of",       # If container changes, parts should know
    }

    def __init__(self, spine: GlobalSpine):
        self._spine = spine
        # Forward edges: source_path -> [(target_path, relation, metadata)]
        self._edges: Dict[str, List[tuple]] = defaultdict(list)
        # Reverse edges: target_path -> [(source_path, relation, metadata)]
        self._reverse_edges: Dict[str, List[tuple]] = defaultdict(list)
        # Path to context_id mapping (for active contexts)
        self._path_to_context: Dict[str, str] = {}
        # Change notification callbacks
        self._change_handlers: Dict[str, List[Callable]] = defaultdict(list)

    def add_link(self, source_path: str, target_path: str, relation: str,
                 label: str = None, metadata: Dict = None) -> bool:
        """Add a semantic link to the graph."""
        from ide.manifest import LinkRelation, INVERSE_RELATIONS

        edge = (target_path, relation, label, metadata or {})

        # Check for duplicate
        if edge in self._edges[source_path]:
            return False

        self._edges[source_path].append(edge)

        # Add reverse edge
        try:
            rel_enum = LinkRelation(relation)
            inverse = INVERSE_RELATIONS.get(rel_enum)
            inverse_rel = inverse.value if inverse else "related_to"
        except ValueError:
            inverse_rel = "related_to"

        reverse_edge = (source_path, inverse_rel, label, metadata or {})
        self._reverse_edges[target_path].append(reverse_edge)

        self._spine.emit("relation.added", {
            "source": source_path,
            "target": target_path,
            "relation": relation,
        })
        return True

    def remove_link(self, source_path: str, target_path: str,
                    relation: str = None) -> int:
        """Remove link(s). If relation is None, removes all links to target."""
        removed = 0

        # Remove forward edges
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
            original = self._reverse_edges[target_path]
            self._reverse_edges[target_path] = [
                e for e in original if e[0] != source_path
            ]

        if removed > 0:
            self._spine.emit("relation.removed", {
                "source": source_path,
                "target": target_path,
                "count": removed,
            })
        return removed

    def get_outgoing(self, path: str, relation: str = None) -> List[Dict]:
        """Get all outgoing links from a document."""
        edges = self._edges.get(path, [])
        result = []
        for target, rel, label, meta in edges:
            if relation is None or rel == relation:
                result.append({
                    "target": target,
                    "relation": rel,
                    "label": label,
                    "metadata": meta,
                })
        return result

    def get_incoming(self, path: str, relation: str = None) -> List[Dict]:
        """Get all incoming links to a document."""
        edges = self._reverse_edges.get(path, [])
        result = []
        for source, rel, label, meta in edges:
            if relation is None or rel == relation:
                result.append({
                    "source": source,
                    "relation": rel,
                    "label": label,
                    "metadata": meta,
                })
        return result

    def find_by_relation(self, relation: str) -> List[Dict]:
        """Find all links with a specific relation type."""
        result = []
        for source, edges in self._edges.items():
            for target, rel, label, meta in edges:
                if rel == relation:
                    result.append({
                        "source": source,
                        "target": target,
                        "label": label,
                        "metadata": meta,
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
                # Add outgoing
                for edge in self._edges.get(p, []):
                    next_frontier.add(edge[0])
                # Add incoming
                for edge in self._reverse_edges.get(p, []):
                    next_frontier.add(edge[0])
            frontier = next_frontier - visited

        visited.discard(path)  # Don't include self
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

    def load_from_manifest(self, path: str, manifest: 'DocumentManifest'):
        """Load all links from a document's manifest."""
        for link in manifest.links:
            self.add_link(
                source_path=path,
                target_path=link.target,
                relation=link.relation.value,
                label=link.label,
                metadata=link.metadata,
            )

    def clear_path(self, path: str):
        """Remove all links from/to a path (when document is closed)."""
        # Remove outgoing
        if path in self._edges:
            for target, rel, _, _ in self._edges[path]:
                # Clean up reverse edge
                if target in self._reverse_edges:
                    self._reverse_edges[target] = [
                        e for e in self._reverse_edges[target] if e[0] != path
                    ]
            del self._edges[path]

        # Remove incoming (keeping reverse edges intact for when doc reopens)
        # This is intentional - we want to remember relationships even when
        # the target document is not open

        # Clean up change handlers
        if path in self._change_handlers:
            del self._change_handlers[path]

    # ----- Change Propagation Methods -----

    def on_change(self, path: str, handler: Callable[[str, str, Dict], None]):
        """Register a handler to be called when related documents change.

        Handler receives: (changed_path, relation_type, metadata)
        """
        self._change_handlers[path].append(handler)

    def off_change(self, path: str, handler: Callable = None):
        """Remove change handler(s) for a path."""
        if handler:
            self._change_handlers[path] = [
                h for h in self._change_handlers[path] if h != handler
            ]
        else:
            self._change_handlers[path] = []

    def notify_change(self, changed_path: str, change_type: str = "modified"):
        """Notify related documents that this document has changed.

        This propagates change notifications based on relationship types.
        For example, if a spec changes, all implementing documents are notified.
        """
        notified = []

        # Find documents that link TO the changed document with propagating relations
        for source_path, edges in self._edges.items():
            for target, rel, label, meta in edges:
                if target == changed_path and rel in self.PROPAGATING_RELATIONS:
                    # This source implements/extends/requires the changed doc
                    if source_path in self._change_handlers:
                        for handler in self._change_handlers[source_path]:
                            try:
                                handler(changed_path, rel, {
                                    "change_type": change_type,
                                    "label": label,
                                    **meta,
                                })
                            except Exception:
                                pass
                        notified.append(source_path)

        # Emit global event for system awareness
        if notified:
            self._spine.emit("relation.change_propagated", {
                "source": changed_path,
                "change_type": change_type,
                "notified": notified,
            })

        return notified

    def get_affected_by_change(self, path: str) -> List[Dict]:
        """Get list of documents that would be affected if this document changes."""
        affected = []
        for source_path, edges in self._edges.items():
            for target, rel, label, meta in edges:
                if target == path and rel in self.PROPAGATING_RELATIONS:
                    affected.append({
                        "path": source_path,
                        "relation": rel,
                        "label": label,
                    })
        return affected


class ContextPrimitives:
    """Context primitives API (ctx.attach, ctx.emit, ctx.invoke, ctx.link).

    This is the API that providers use to interact with the kernel.
    Each primitive is permission-checked against the context's manifest.
    """

    def __init__(self, handle: ContextHandle, kernel: 'Kernel'):
        self._handle = handle
        self._kernel = kernel
        self._attachments: Dict[str, Any] = {}
        self._handlers: Dict[str, List[Callable]] = {}

    @property
    def id(self) -> str:
        """Get this context's ID."""
        return self._handle.context_id

    @property
    def path(self) -> Optional[str]:
        """Get this context's file path."""
        return self._handle.path

    @property
    def manifest(self):
        """Get the manifest for this context."""
        return self._handle.manifest

    @property
    def ai_access(self) -> str:
        """Get AI access level for this context."""
        return self._handle.ai_access

    def check_provider(self, provider_id: str) -> Dict[str, bool]:
        """Check what permissions a provider has for this context."""
        if not self._handle.manifest:
            return {
                "can_read": True,
                "can_write": self._handle.can_write,
                "can_emit": self._handle.can_emit,
                "can_invoke": self._handle.can_invoke,
            }

        perm = self._handle.manifest.get_provider_permission(provider_id)
        return {
            "can_read": perm.can_read,
            "can_write": perm.can_write,
            "can_emit": perm.can_emit,
            "can_invoke": perm.can_invoke,
        }

    def provider_can(self, provider_id: str, action: str) -> bool:
        """Check if a provider can perform a specific action."""
        perms = self.check_provider(provider_id)
        return perms.get(f"can_{action}", False)

    def attach(self, key: str, value: Any, provider_id: str = None):
        """Attach data to this context."""
        if provider_id and not self.provider_can(provider_id, "write"):
            raise PermissionError(f"Provider {provider_id} cannot write to context {self.id}")
        if not self._handle.can_write:
            raise PermissionError(f"Context {self.id} cannot write (attach)")
        self._attachments[key] = value
        self._kernel.spine.emit("context.attach", {"key": key}, source=self.id)

    def get(self, key: str, default: Any = None) -> Any:
        """Get attached data."""
        return self._attachments.get(key, default)

    def emit(self, event_type: str, payload: Any = None, target: str = None):
        """Emit an event from this context."""
        if not self._handle.can_emit:
            raise PermissionError(f"Context {self.id} cannot emit")
        return self._kernel.spine.emit(event_type, payload, source=self.id, target=target)

    def on(self, event_type: str, handler: Callable):
        """Subscribe to events."""
        if event_type not in self._handlers:
            self._handlers[event_type] = []
        self._handlers[event_type].append(handler)

    def invoke(self, command: str, **kwargs):
        """Invoke a command in this context."""
        if not self._handle.can_invoke:
            raise PermissionError(f"Context {self.id} cannot invoke commands")
        self._kernel.spine.emit("context.invoke", {"command": command, "args": kwargs}, source=self.id)
        # Actual command execution happens in the IDE layer

    def link(self, target_id: str):
        """Link this context to another."""
        if not self._handle.can_link:
            raise PermissionError(f"Context {self.id} cannot link")
        self._kernel.registry.link(self.id, target_id)

    def unlink(self, target_id: str):
        """Unlink from another context."""
        self._kernel.registry.unlink(self.id, target_id)

    def get_links(self) -> List[str]:
        """Get all linked context IDs."""
        return self._kernel.registry.get_links(self.id)

    # ----- Semantic Relation Methods -----

    def add_relation(self, target_path: str, relation: str,
                     label: str = None, metadata: Dict = None):
        """Add a semantic relation to another document."""
        if not self._handle.path:
            raise ValueError("Cannot add relation: context has no path")
        if not self._handle.can_link:
            raise PermissionError(f"Context {self.id} cannot create relations")
        self._kernel.relations.add_link(
            self._handle.path, target_path, relation, label, metadata
        )

    def remove_relation(self, target_path: str, relation: str = None):
        """Remove a semantic relation."""
        if not self._handle.path:
            return
        self._kernel.relations.remove_link(self._handle.path, target_path, relation)

    def get_outgoing_relations(self, relation: str = None) -> List[Dict]:
        """Get all outgoing semantic relations."""
        if not self._handle.path:
            return []
        return self._kernel.relations.get_outgoing(self._handle.path, relation)

    def get_incoming_relations(self, relation: str = None) -> List[Dict]:
        """Get all incoming semantic relations (documents that link to this)."""
        if not self._handle.path:
            return []
        return self._kernel.relations.get_incoming(self._handle.path, relation)

    def get_related_paths(self, max_depth: int = 1) -> Set[str]:
        """Get paths of all related documents."""
        if not self._handle.path:
            return set()
        return self._kernel.relations.get_related(self._handle.path, max_depth)

    def notify_change(self, change_type: str = "modified"):
        """Notify related documents that this document has changed."""
        if not self._handle.path:
            return []
        return self._kernel.relations.notify_change(self._handle.path, change_type)

    def on_related_change(self, handler: Callable):
        """Register to be notified when related documents change."""
        if not self._handle.path:
            return
        self._kernel.relations.on_change(self._handle.path, handler)

    def get_change_impact(self) -> List[Dict]:
        """Get list of documents that would be affected if this one changes."""
        if not self._handle.path:
            return []
        return self._kernel.relations.get_affected_by_change(self._handle.path)

    def _dispatch(self, event: KernelEvent):
        """Internal: dispatch event to handlers."""
        handlers = self._handlers.get(event.event_type, [])
        for handler in handlers:
            try:
                handler(event)
            except Exception:
                pass  # Handlers should not crash the kernel


class Kernel(QObject):
    """The Semantic OS Kernel.

    Central coordinator providing:
    - Global spine for events
    - Context registry
    - Context primitive factories

    The kernel is initialized once at startup and shared across the app.
    """

    # Kernel lifecycle signals
    kernel_ready = pyqtSignal()
    kernel_shutdown = pyqtSignal()

    def __init__(self):
        super().__init__()
        self.spine = GlobalSpine()
        self.registry = ContextRegistry(self.spine)
        self.relations = RelationGraph(self.spine)
        self._primitives: Dict[str, ContextPrimitives] = {}

        # Connect spine to dispatch events to primitives
        self.spine.event_broadcast.connect(self._dispatch_event)

    def create_context(self, path: str = None, context_type: str = "document",
                       metadata: Dict = None) -> ContextPrimitives:
        """Create a new context and return its primitives."""
        handle = self.registry.register(path, context_type, metadata)
        primitives = ContextPrimitives(handle, self)
        self._primitives[handle.context_id] = primitives

        # Load semantic links from manifest into relation graph
        if path and handle.manifest:
            self.relations.load_from_manifest(path, handle.manifest)

        return primitives

    def get_context(self, context_id: str) -> Optional[ContextPrimitives]:
        """Get primitives for an existing context."""
        return self._primitives.get(context_id)

    def get_context_by_path(self, path: str) -> Optional[ContextPrimitives]:
        """Get primitives for a context by its file path."""
        handle = self.registry.get_by_path(path)
        return self._primitives.get(handle.context_id) if handle else None

    def destroy_context(self, context_id: str):
        """Destroy a context and clean up."""
        # Clean up relation graph entries
        handle = self.registry.get(context_id)
        if handle and handle.path:
            self.relations.clear_path(handle.path)

        if context_id in self._primitives:
            del self._primitives[context_id]
        self.registry.unregister(context_id)

    def _dispatch_event(self, event: KernelEvent):
        """Dispatch global events to interested contexts."""
        # If targeted, dispatch only to target
        if event.target_context and event.target_context in self._primitives:
            self._primitives[event.target_context]._dispatch(event)
            return

        # Otherwise broadcast to all
        for primitives in self._primitives.values():
            primitives._dispatch(event)

    def get_system_state(self) -> Dict:
        """Get current kernel state (for debugging/AI context)."""
        context_list = []
        for p in self._primitives.values():
            handle = self.registry.get(p.id)
            ctx_info = {
                "id": p.id,
                "path": p.path,
                "type": handle.context_type if handle else "unknown",
                "ai_access": handle.ai_access if handle else "observe",
                "links": p.get_links(),
            }
            # Add manifest summary if present
            if handle and handle.manifest:
                ctx_info["manifest"] = {
                    "type": handle.manifest.document_type,
                    "ai_access": handle.manifest.ai_access,
                    "tags": handle.manifest.tags,
                    "providers": len(handle.manifest.providers),
                }
            context_list.append(ctx_info)

        return {
            "contexts": len(self._primitives),
            "context_list": context_list,
            "recent_events": [e.to_dict() for e in self.spine.query(limit=10)],
            "relations": self.relations.get_graph_summary(),
        }

    def shutdown(self):
        """Shutdown the kernel cleanly."""
        self.kernel_shutdown.emit()
        # Destroy all contexts
        for context_id in list(self._primitives.keys()):
            self.destroy_context(context_id)


# Singleton kernel instance
_kernel: Optional[Kernel] = None


def get_kernel() -> Kernel:
    """Get the global kernel instance."""
    global _kernel
    if _kernel is None:
        _kernel = Kernel()
    return _kernel


def init_kernel() -> Kernel:
    """Initialize the kernel (call once at startup)."""
    kernel = get_kernel()
    kernel.kernel_ready.emit()
    return kernel
