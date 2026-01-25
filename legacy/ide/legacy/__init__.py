"""
Legacy App Virtualization Module
================================

Provides infrastructure for running legacy applications within Semantic OS
while translating their operations into semantic events.

Components:
- app_manager: Main entry point for launching legacy apps
- wine_runtime: Wine/Proton for Windows apps (full GPU!)
- docker_runtime: Docker container management for Linux apps
- microvm_runtime: Micro-VM support (Firecracker, QEMU) [planned]
- wasm_runtime: WebAssembly sandbox for web apps [planned]
- gui_monitor: Platform-specific GUI monitoring [planned]
- ai_interpreter: AI-based intent interpretation [planned]

Usage:
    from ide.legacy import LegacyAppManager

    manager = LegacyAppManager(kernel)

    # Run native Linux app
    app = manager.launch("gimp", document="image.png")

    # Run Windows app via Wine (full GPU performance!)
    app = manager.launch("photoshop", document="design.psd")

    # All actions translated to semantic events
"""

from .app_manager import LegacyAppManager, AppConfig, RuntimeType
from .wine_runtime import WineRuntime, WineAppConfig, ProtonRuntime, WineVariant
from .semantic_ops import (
    SemanticOp, SemanticPrimitive, Intent, EventType,
    APICallContext, TranslationResult
)
from .api_translator import (
    SemanticAPITranslator, get_translator, translate_api_call
)
from .syscall_hook import (
    SemanticHookManager, SyscallHook, HookMethod, HookConfig,
    hook_app
)

__all__ = [
    # App Manager
    "LegacyAppManager",
    "AppConfig",
    "RuntimeType",
    # Wine Runtime
    "WineRuntime",
    "WineAppConfig",
    "ProtonRuntime",
    "WineVariant",
    # Semantic Operations
    "SemanticOp",
    "SemanticPrimitive",
    "Intent",
    "EventType",
    "APICallContext",
    "TranslationResult",
    # API Translator
    "SemanticAPITranslator",
    "get_translator",
    "translate_api_call",
    # Syscall Hooking
    "SemanticHookManager",
    "SyscallHook",
    "HookMethod",
    "HookConfig",
    "hook_app",
]
