"""
Semantic Kernel
===============

The core of Semantic OS. Everything is semantic memory.

Architecture:
    - All state is vectors in a single memory pool
    - Documents, events, processes, handles - all vectors
    - Query by meaning, not by path/id/key
    - Syscalls simulated by kernel, not passed to real OS

Usage:
    from kernel import SemanticKernel

    kernel = SemanticKernel()

    # Attach to a document
    handle = kernel.attach("design.psd", intent="edit")

    # Write content (stored semantically)
    kernel.write(handle, content)

    # Query by meaning
    results = kernel.query("documents I edited today")
"""

from .core import SemanticKernel
from .memory import SemanticMemory, MemoryEntry, MemoryType
from .syscall import SyscallSimulator

__all__ = [
    "SemanticKernel",
    "SemanticMemory",
    "MemoryEntry",
    "MemoryType",
    "SyscallSimulator",
]
