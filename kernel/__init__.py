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

Shell:
    python -m kernel.shell

    semantic> show recent documents
    semantic> what did I work on today
"""

from .core import SemanticKernel, Handle, Process, Intent, get_kernel, init_kernel
from .memory import SemanticMemory, MemoryEntry, MemoryType
from .syscall import SyscallSimulator
from .ai_providers import (
    get_provider,
    AIProvider,
    LMStudioProvider,
    OllamaProvider,
    OpenAIProvider,
    AnthropicProvider,
    ProviderType,
)
from .shell import SemanticShell

__all__ = [
    # Kernel
    "SemanticKernel",
    "Handle",
    "Process",
    "Intent",
    "get_kernel",
    "init_kernel",
    "SemanticMemory",
    "MemoryEntry",
    "MemoryType",
    "SyscallSimulator",
    # AI Providers
    "get_provider",
    "AIProvider",
    "LMStudioProvider",
    "OllamaProvider",
    "OpenAIProvider",
    "AnthropicProvider",
    "ProviderType",
    # Shell
    "SemanticShell",
]
