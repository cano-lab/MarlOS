"""
Legacy App Virtualization Module
================================

Provides infrastructure for running legacy applications within Semantic OS
while translating their operations into semantic events.

Components:
- docker_runtime: Docker container management for Linux apps
- microvm_runtime: Micro-VM support (Firecracker, QEMU)
- wasm_runtime: WebAssembly sandbox for web apps
- gui_monitor: Platform-specific GUI monitoring
- ai_interpreter: AI-based intent interpretation
- event_mapper: Map intents to semantic events

Usage:
    from ide.legacy import LegacyAppManager

    manager = LegacyAppManager(kernel)
    app = manager.launch("photoshop", document="design.psd")
    # App runs, all actions translated to semantic events
"""

from .app_manager import LegacyAppManager, AppConfig, RuntimeType

__all__ = [
    "LegacyAppManager",
    "AppConfig",
    "RuntimeType",
]
