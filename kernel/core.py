"""
Semantic Kernel Core
====================

The heart of Semantic OS. All operations are semantic.
All state is vectors in memory pool.

Implements the semantic primitives:
    ctx.attach(path, intent)  - Open document context
    ctx.detach(path)          - Close document context
    ctx.emit(event, data)     - Emit semantic event
    ctx.invoke(action, args)  - Request action
    ctx.link(src, dst, rel)   - Create relationship
    ctx.query(q)              - Query semantic memory
"""

import time
import uuid
import threading
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Callable, Tuple
from enum import Enum

from .memory import SemanticMemory, MemoryEntry, MemoryType


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


@dataclass
class Process:
    """A process context."""
    id: str                         # Process ID
    name: str                       # Process name (e.g., "notepad.exe")
    handles: Dict[str, Handle] = field(default_factory=dict)  # Open handles
    created_at: float = field(default_factory=time.time)

    # Process memory (for simulated address space)
    memory: Dict[int, bytes] = field(default_factory=dict)


class SemanticKernel:
    """The Semantic OS kernel.

    Everything is semantic memory. Syscalls become semantic operations.

    Usage:
        kernel = SemanticKernel()

        # Create a process context
        proc = kernel.create_process("notepad.exe")

        # Open a document
        handle = kernel.attach("document.txt", Intent.EDIT, proc.id)

        # Write content
        kernel.write(handle, "Hello, semantic world!")

        # Emit event
        kernel.emit("content_changed", {"handle": handle.id})

        # Query memory
        results = kernel.query("documents edited recently")

        # Close
        kernel.detach(handle)
    """

    def __init__(self, db_path: str = None):
        """Initialize the kernel.

        Args:
            db_path: Path to memory database (None for in-memory)
        """
        self.memory = SemanticMemory(db_path)
        self.processes: Dict[str, Process] = {}
        self.handles: Dict[str, Handle] = {}
        self._lock = threading.RLock()

        # Event subscribers
        self._subscribers: Dict[str, List[Callable]] = {}

        # Action handlers
        self._action_handlers: Dict[str, Callable] = {}

        # Initialize built-in actions
        self._register_builtin_actions()

        # Boot message
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

        # Store in semantic memory
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

        # Close all handles
        for handle_id in list(proc.handles.keys()):
            handle = proc.handles[handle_id]
            self.detach(handle)

        # Update memory
        self.memory.update(f"proc_{pid}", metadata={"state": "terminated"})
        del self.processes[pid]

        self.emit("process.terminated", {"pid": pid})

    # ==================== SEMANTIC PRIMITIVES ====================

    def attach(self, path: str, intent: Intent, process_id: str = None) -> Handle:
        """ctx.attach - Open/access a document context.

        Args:
            path: Document path/identifier
            intent: Access intent (read, edit, create)
            process_id: Owning process (optional)

        Returns:
            Handle for the document
        """
        with self._lock:
            # Find or create document in memory
            doc_id = f"doc_{path.replace('/', '_').replace('.', '_')}"

            existing = self.memory.get(doc_id)

            if existing:
                # Document exists
                document_id = existing.id
                content = existing.content
            elif intent == Intent.CREATE:
                # Create new document
                document_id = self.memory.store(
                    content="",
                    type=MemoryType.DOCUMENT,
                    metadata={"path": path, "created_by": process_id},
                    id=doc_id,
                )
                content = ""
            else:
                # Document doesn't exist and not creating
                # Create placeholder for now (in real OS, would fail)
                document_id = self.memory.store(
                    content="",
                    type=MemoryType.DOCUMENT,
                    metadata={"path": path, "virtual": True},
                    id=doc_id,
                )
                content = ""

            # Create handle
            handle_id = str(uuid.uuid4())[:8]
            handle = Handle(
                id=handle_id,
                document_id=document_id,
                path=path,
                intent=intent,
                process_id=process_id or "kernel",
                content_buffer=content,
            )

            self.handles[handle_id] = handle

            # Add to process
            if process_id and process_id in self.processes:
                self.processes[process_id].handles[handle_id] = handle

            # Store handle in memory
            self.memory.store(
                content=f"Handle opened for {path} with intent {intent.value}",
                type=MemoryType.HANDLE,
                metadata={
                    "handle_id": handle_id,
                    "document_id": document_id,
                    "path": path,
                    "intent": intent.value,
                    "process_id": process_id,
                },
                id=f"handle_{handle_id}",
                links=[document_id],
            )

            self.emit("document.attached", {
                "handle_id": handle_id,
                "path": path,
                "intent": intent.value,
            })

            return handle

    def detach(self, handle: Handle):
        """ctx.detach - Close/release a document context.

        Args:
            handle: The handle to close
        """
        with self._lock:
            # Flush any buffered changes
            if handle.modified:
                self._flush_handle(handle)

            # Remove from tracking
            if handle.id in self.handles:
                del self.handles[handle.id]

            # Remove from process
            if handle.process_id in self.processes:
                proc = self.processes[handle.process_id]
                if handle.id in proc.handles:
                    del proc.handles[handle.id]

            # Update memory
            self.memory.update(f"handle_{handle.id}", metadata={"state": "closed"})

            self.emit("document.detached", {
                "handle_id": handle.id,
                "path": handle.path,
            })

    def emit(self, event: str, data: Any = None):
        """ctx.emit - Emit a semantic event.

        Args:
            event: Event name
            data: Event data
        """
        # Store event in memory
        event_content = f"{event}: {data}" if data else event
        self.memory.store(
            content=event_content,
            type=MemoryType.EVENT,
            metadata={
                "event": event,
                "data": data,
                "timestamp": time.time(),
            },
        )

        # Notify subscribers
        if event in self._subscribers:
            for callback in self._subscribers[event]:
                try:
                    callback(event, data)
                except Exception as e:
                    print(f"[Kernel] Event handler error: {e}")

        # Wildcard subscribers
        if "*" in self._subscribers:
            for callback in self._subscribers["*"]:
                try:
                    callback(event, data)
                except Exception:
                    pass

    def invoke(self, action: str, **params) -> Any:
        """ctx.invoke - Request an action.

        Args:
            action: Action name (e.g., "document.save")
            **params: Action parameters

        Returns:
            Action result
        """
        # Store invocation
        self.memory.store(
            content=f"Invoke {action} with {params}",
            type=MemoryType.EVENT,
            metadata={
                "event": "action.invoked",
                "action": action,
                "params": params,
            },
        )

        # Execute handler
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
        """ctx.link - Create relationship between documents.

        Args:
            source: Source document path/id
            target: Target document path/id
            relation: Relationship type

        Returns:
            Success
        """
        # Find documents
        source_id = f"doc_{source.replace('/', '_').replace('.', '_')}"
        target_id = f"doc_{target.replace('/', '_').replace('.', '_')}"

        # Create link
        success = self.memory.link(source_id, target_id, relation)

        if success:
            self.emit("documents.linked", {
                "source": source,
                "target": target,
                "relation": relation,
            })

        return success

    def query(self, q: str, **filters) -> List[Tuple[MemoryEntry, float]]:
        """ctx.query - Query semantic memory.

        Args:
            q: Natural language query
            **filters: Optional filters (type, metadata, etc.)

        Returns:
            List of (entry, score) tuples
        """
        type_filter = filters.pop("type", None)
        if isinstance(type_filter, str):
            type_filter = MemoryType(type_filter)

        return self.memory.query(
            query=q,
            type=type_filter,
            **filters,
        )

    # ==================== READ/WRITE OPERATIONS ====================

    def read(self, handle: Handle, size: int = -1) -> str:
        """Read from a document handle.

        Args:
            handle: Document handle
            size: Bytes to read (-1 for all)

        Returns:
            Content string
        """
        if size < 0:
            return handle.content_buffer[handle.position:]
        else:
            content = handle.content_buffer[handle.position:handle.position + size]
            handle.position += len(content)
            return content

    def write(self, handle: Handle, content: str) -> int:
        """Write to a document handle.

        Args:
            handle: Document handle
            content: Content to write

        Returns:
            Bytes written
        """
        if handle.intent not in (Intent.EDIT, Intent.CREATE):
            raise PermissionError("Handle not opened for writing")

        # Insert at position
        buffer = handle.content_buffer
        handle.content_buffer = (
            buffer[:handle.position] +
            content +
            buffer[handle.position:]
        )
        handle.position += len(content)
        handle.modified = True

        self.emit("content.changed", {
            "handle_id": handle.id,
            "path": handle.path,
            "bytes": len(content),
        })

        return len(content)

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

        self.emit("content.saved", {
            "handle_id": handle.id,
            "path": handle.path,
        })

    # ==================== SUBSCRIPTIONS ====================

    def subscribe(self, event: str, callback: Callable):
        """Subscribe to semantic events.

        Args:
            event: Event name (or "*" for all)
            callback: Function to call (receives event, data)
        """
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
        """Built-in save action."""
        if handle_id:
            handle = self.handles.get(handle_id)
            if handle:
                self._flush_handle(handle)
                return True
        return False

    def _action_delete(self, path: str) -> bool:
        """Built-in delete action."""
        doc_id = f"doc_{path.replace('/', '_').replace('.', '_')}"
        return self.memory.delete(doc_id)

    def _action_copy(self, source: str, destination: str) -> bool:
        """Built-in copy action."""
        source_id = f"doc_{source.replace('/', '_').replace('.', '_')}"
        source_doc = self.memory.get(source_id)

        if not source_doc:
            return False

        dest_id = f"doc_{destination.replace('/', '_').replace('.', '_')}"
        self.memory.store(
            content=source_doc.content,
            type=MemoryType.DOCUMENT,
            metadata={**source_doc.metadata, "path": destination, "copied_from": source},
            id=dest_id,
        )

        self.link(destination, source, "copied_from")
        return True

    def _action_move(self, source: str, destination: str) -> bool:
        """Built-in move action."""
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

        # Terminate all processes
        for pid in list(self.processes.keys()):
            self.terminate_process(pid)

        self.memory.close()
