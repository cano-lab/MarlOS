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
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Callable, Tuple, TYPE_CHECKING
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

        # Initialize built-in actions
        self._register_builtin_actions()

        # Boot
        self._boot_time = time.time()
        self.emit("kernel.boot", {"time": self._boot_time})

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
                       metadata: Dict = None) -> Handle:
        """IDE compatibility: create_context -> attach."""
        return self.attach(
            path=path or f"untitled_{uuid.uuid4().hex[:8]}",
            intent=Intent.CREATE,
            context_type=context_type,
            metadata=metadata,
        )

    def get_context(self, handle_id: str) -> Optional[Handle]:
        """IDE compatibility alias."""
        return self.get_handle(handle_id)

    def get_context_by_path(self, path: str) -> Optional[Handle]:
        """IDE compatibility alias."""
        return self.get_handle_by_path(path)

    def destroy_context(self, handle_id: str):
        """IDE compatibility: destroy_context -> detach."""
        handle = self.handles.get(handle_id)
        if handle:
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

        def __init__(self, db_path: str = None, enable_qt: bool = True):
            QObject.__init__(self)
            self._enable_qt = enable_qt  # Set before base init
            SemanticKernelBase.__init__(self, db_path)
            if enable_qt:
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
