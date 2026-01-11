"""
Syscall Interception Layer
==========================

Intercepts system calls from legacy applications and routes them
through the semantic API translator.

Platform Support:
- Linux: ptrace, eBPF, LD_PRELOAD
- Windows: API hooking, ETW (Event Tracing)
- macOS: DTrace, dyld interposition

This module provides a unified interface across platforms.
"""

import os
import sys
import subprocess
import threading
import queue
from dataclasses import dataclass, field
from typing import Optional, Dict, Any, Callable, List
from enum import Enum
from abc import ABC, abstractmethod

from .semantic_ops import APICallContext, SemanticOp
from .api_translator import SemanticAPITranslator, TranslationResult, get_translator


class HookMethod(str, Enum):
    """Method for intercepting syscalls."""
    PTRACE = "ptrace"           # Linux ptrace
    EBPF = "ebpf"               # Linux eBPF (requires root)
    LD_PRELOAD = "ld_preload"   # Library preloading
    API_HOOK = "api_hook"       # Windows API hooking
    ETW = "etw"                 # Windows Event Tracing
    DTRACE = "dtrace"           # macOS DTrace
    DYLD = "dyld"               # macOS dyld interposition


@dataclass
class InterceptedCall:
    """A syscall/API call intercepted from a process."""
    api_name: str
    args: Dict[str, Any]
    pid: int
    tid: int = 0
    timestamp: float = 0
    return_value: Any = None

    # Process info
    app_name: str = ""
    app_path: str = ""

    # Translation result
    translation: Optional[TranslationResult] = None


@dataclass
class HookConfig:
    """Configuration for syscall hooking."""
    method: HookMethod = HookMethod.LD_PRELOAD
    target_pid: Optional[int] = None
    target_app: Optional[str] = None

    # Filter which calls to intercept
    filter_categories: List[str] = field(default_factory=lambda: ["file", "clipboard"])

    # Passthrough mode (also execute original syscall)
    passthrough: bool = True

    # Callback for intercepted calls
    on_intercept: Optional[Callable[[InterceptedCall], None]] = None

    # Callback for translated operations
    on_semantic: Optional[Callable[[SemanticOp], None]] = None


class SyscallHook(ABC):
    """Base class for syscall interception."""

    def __init__(self, config: HookConfig, translator: SemanticAPITranslator = None):
        self.config = config
        self.translator = translator or get_translator()
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._call_queue: queue.Queue = queue.Queue()

    @abstractmethod
    def start(self) -> bool:
        """Start intercepting syscalls."""
        pass

    @abstractmethod
    def stop(self):
        """Stop intercepting syscalls."""
        pass

    def process_call(self, call: InterceptedCall):
        """Process an intercepted call through the translator."""
        # Build context
        context = APICallContext(
            api_name=call.api_name,
            args=call.args,
            pid=call.pid,
            app_name=call.app_name,
            app_path=call.app_path,
        )

        # Translate
        result = self.translator.translate(context)
        call.translation = result

        # Notify callbacks
        if self.config.on_intercept:
            self.config.on_intercept(call)

        if result.success and self.config.on_semantic:
            for op in result.operations:
                self.config.on_semantic(op)

        return result


class LDPreloadHook(SyscallHook):
    """Intercept syscalls using LD_PRELOAD on Linux.

    This injects a shared library that intercepts libc functions
    and reports them back to us via a Unix socket.
    """

    # Preload library source (would be compiled separately)
    PRELOAD_LIB_PATH = "/usr/lib/semantic_os/libsemantic_hook.so"

    def __init__(self, config: HookConfig, translator: SemanticAPITranslator = None):
        super().__init__(config, translator)
        self._socket_path = f"/tmp/semantic_hook_{os.getpid()}.sock"
        self._process: Optional[subprocess.Popen] = None

    def start(self) -> bool:
        """Start the hooked process."""
        if not self.config.target_app:
            print("[Hook] No target app specified")
            return False

        # Check if preload library exists
        if not os.path.exists(self.PRELOAD_LIB_PATH):
            print(f"[Hook] Preload library not found: {self.PRELOAD_LIB_PATH}")
            print("[Hook] Using stub mode (no actual interception)")
            return self._start_stub_mode()

        # Set up environment for preloading
        env = os.environ.copy()
        env["LD_PRELOAD"] = self.PRELOAD_LIB_PATH
        env["SEMANTIC_HOOK_SOCKET"] = self._socket_path

        # Start the target process
        try:
            self._process = subprocess.Popen(
                self.config.target_app.split(),
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            # Start listener thread
            self._running = True
            self._thread = threading.Thread(target=self._listen_loop)
            self._thread.daemon = True
            self._thread.start()

            return True

        except Exception as e:
            print(f"[Hook] Failed to start process: {e}")
            return False

    def _start_stub_mode(self) -> bool:
        """Start in stub mode without actual hooking."""
        # Just run the process normally
        try:
            self._process = subprocess.Popen(
                self.config.target_app.split(),
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            self._running = True
            print(f"[Hook] Running in stub mode (PID: {self._process.pid})")
            return True
        except Exception as e:
            print(f"[Hook] Failed to start: {e}")
            return False

    def _listen_loop(self):
        """Listen for intercepted calls from the preload library."""
        import socket
        import json

        # Create Unix socket
        if os.path.exists(self._socket_path):
            os.unlink(self._socket_path)

        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.bind(self._socket_path)
        sock.listen(1)
        sock.settimeout(1.0)

        while self._running:
            try:
                conn, _ = sock.accept()
                data = conn.recv(4096).decode()
                conn.close()

                if data:
                    call_data = json.loads(data)
                    call = InterceptedCall(
                        api_name=call_data["api"],
                        args=call_data.get("args", {}),
                        pid=call_data.get("pid", 0),
                        app_name=self.config.target_app,
                    )
                    self.process_call(call)

            except socket.timeout:
                continue
            except Exception as e:
                if self._running:
                    print(f"[Hook] Listener error: {e}")

        sock.close()
        if os.path.exists(self._socket_path):
            os.unlink(self._socket_path)

    def stop(self):
        """Stop interception."""
        self._running = False
        if self._process:
            self._process.terminate()
            self._process.wait(timeout=5)
        if self._thread:
            self._thread.join(timeout=2)


class PtraceHook(SyscallHook):
    """Intercept syscalls using ptrace on Linux.

    More powerful than LD_PRELOAD but requires attaching to process.
    """

    def __init__(self, config: HookConfig, translator: SemanticAPITranslator = None):
        super().__init__(config, translator)

    def start(self) -> bool:
        """Start ptrace monitoring."""
        if sys.platform != "linux":
            print("[Hook] ptrace only available on Linux")
            return False

        # Would use python-ptrace or ctypes to implement
        print("[Hook] ptrace hook not yet implemented")
        print("[Hook] Requires: pip install python-ptrace")
        return False

    def stop(self):
        """Stop ptrace monitoring."""
        self._running = False


class WindowsAPIHook(SyscallHook):
    """Intercept Windows API calls using hooking techniques.

    Methods:
    - IAT (Import Address Table) hooking
    - Inline hooking (detours)
    - ETW (Event Tracing for Windows)
    """

    def __init__(self, config: HookConfig, translator: SemanticAPITranslator = None):
        super().__init__(config, translator)

    def start(self) -> bool:
        """Start Windows API hooking."""
        if sys.platform != "win32":
            print("[Hook] Windows API hooking only available on Windows")
            return False

        # Would use libraries like:
        # - pydetour
        # - frida
        # - minhook (via ctypes)
        print("[Hook] Windows API hook - using simulation mode")
        return self._start_simulation()

    def _start_simulation(self) -> bool:
        """Simulate API hooking for development."""
        self._running = True
        self._thread = threading.Thread(target=self._simulation_loop)
        self._thread.daemon = True
        self._thread.start()
        return True

    def _simulation_loop(self):
        """Simulate intercepted calls for testing."""
        import time
        import random

        # Simulated API calls that might come from an app
        simulated_calls = [
            ("CreateFileW", {"path": "document.txt", "access": "GENERIC_WRITE"}),
            ("WriteFile", {"bytes": 1024}),
            ("SetWindowTextW", {"text": "document.txt - Notepad"}),
            ("GetSaveFileNameW", {}),
            ("WriteFile", {"bytes": 2048}),
            ("CloseHandle", {"handle": 0x1234}),
        ]

        idx = 0
        while self._running and idx < len(simulated_calls):
            time.sleep(random.uniform(0.5, 2.0))

            api_name, args = simulated_calls[idx]
            call = InterceptedCall(
                api_name=api_name,
                args=args,
                pid=os.getpid(),
                app_name="notepad.exe",
            )

            result = self.process_call(call)
            if result.success:
                print(f"[Hook] {api_name} → {result.operations[0].primitive.value}")

            idx += 1

    def stop(self):
        """Stop API hooking."""
        self._running = False
        if self._thread:
            self._thread.join(timeout=2)


class SemanticHookManager:
    """Manages syscall hooks for multiple processes.

    This is the main interface for the semantic interception layer.

    Usage:
        manager = SemanticHookManager(kernel)

        # Hook a new process
        hook_id = manager.hook_process(
            app="photoshop",
            on_semantic=lambda op: kernel.execute(op)
        )

        # Later, unhook
        manager.unhook(hook_id)
    """

    def __init__(self, kernel=None, translator: SemanticAPITranslator = None):
        self.kernel = kernel
        self.translator = translator or get_translator()
        self._hooks: Dict[str, SyscallHook] = {}
        self._next_id = 0

    def hook_process(
        self,
        app: str = None,
        pid: int = None,
        method: HookMethod = None,
        on_intercept: Callable = None,
        on_semantic: Callable = None,
    ) -> Optional[str]:
        """Hook a process for semantic interception.

        Args:
            app: Application to launch and hook
            pid: Existing process ID to attach to
            method: Hook method (auto-detected if None)
            on_intercept: Callback for raw intercepted calls
            on_semantic: Callback for translated semantic operations

        Returns:
            Hook ID or None if failed
        """
        # Auto-detect method based on platform
        if method is None:
            if sys.platform == "linux":
                method = HookMethod.LD_PRELOAD
            elif sys.platform == "win32":
                method = HookMethod.API_HOOK
            elif sys.platform == "darwin":
                method = HookMethod.DYLD
            else:
                print(f"[Hook] Unsupported platform: {sys.platform}")
                return None

        config = HookConfig(
            method=method,
            target_pid=pid,
            target_app=app,
            on_intercept=on_intercept,
            on_semantic=on_semantic or self._default_semantic_handler,
        )

        # Create appropriate hook
        hook: SyscallHook
        if method == HookMethod.LD_PRELOAD:
            hook = LDPreloadHook(config, self.translator)
        elif method == HookMethod.PTRACE:
            hook = PtraceHook(config, self.translator)
        elif method == HookMethod.API_HOOK:
            hook = WindowsAPIHook(config, self.translator)
        else:
            print(f"[Hook] Method not implemented: {method}")
            return None

        # Start hooking
        if not hook.start():
            return None

        # Track hook
        hook_id = f"hook_{self._next_id}"
        self._next_id += 1
        self._hooks[hook_id] = hook

        print(f"[Hook] Started: {hook_id} ({method.value})")
        return hook_id

    def unhook(self, hook_id: str) -> bool:
        """Stop hooking a process."""
        if hook_id not in self._hooks:
            return False

        self._hooks[hook_id].stop()
        del self._hooks[hook_id]

        print(f"[Hook] Stopped: {hook_id}")
        return True

    def unhook_all(self):
        """Stop all hooks."""
        for hook_id in list(self._hooks.keys()):
            self.unhook(hook_id)

    def _default_semantic_handler(self, op: SemanticOp):
        """Default handler for semantic operations."""
        if self.kernel:
            # Execute through kernel
            self.kernel.execute_semantic(op)
        else:
            # Just log
            print(f"[Semantic] {op.primitive.value}({op.target})")

    def get_stats(self) -> Dict[str, Any]:
        """Get hooking statistics."""
        return {
            "active_hooks": len(self._hooks),
            "translator_stats": self.translator.get_stats(),
        }


# Convenience function
def hook_app(
    app: str,
    on_semantic: Callable[[SemanticOp], None] = None,
    kernel=None
) -> Optional[str]:
    """Hook an application for semantic translation.

    Args:
        app: Application command to launch
        on_semantic: Callback for semantic operations
        kernel: Semantic OS kernel

    Returns:
        Hook ID or None
    """
    manager = SemanticHookManager(kernel)
    return manager.hook_process(app=app, on_semantic=on_semantic)
