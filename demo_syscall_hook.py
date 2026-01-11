#!/usr/bin/env python3
"""
Demo: Syscall Hook Simulation
=============================

Demonstrates the full flow:
1. Legacy app makes syscalls
2. Hook layer intercepts them
3. API translator converts to semantic ops
4. Semantic kernel executes operations

This runs in simulation mode since actual hooking requires
platform-specific setup (LD_PRELOAD on Linux, etc.)

Run: python demo_syscall_hook.py
"""

import sys
import time
import threading
from dataclasses import dataclass
from typing import List

sys.path.insert(0, ".")

from ide.legacy import (
    SemanticHookManager,
    SemanticAPITranslator,
    SemanticOp,
    APICallContext,
    HookConfig,
    HookMethod,
)
from ide.legacy.syscall_hook import InterceptedCall


@dataclass
class SimulatedApp:
    """Simulates a legacy application making syscalls."""
    name: str
    pid: int
    syscalls: List[dict]


# Simulated Notepad session
NOTEPAD_SESSION = SimulatedApp(
    name="notepad.exe",
    pid=12345,
    syscalls=[
        # User opens a file
        {"api": "GetOpenFileNameW", "args": {"filter": "*.txt"}},
        {"api": "CreateFileW", "args": {"path": "notes.txt", "access": "GENERIC_READ"}},

        # User edits the file
        {"api": "CreateFileW", "args": {"path": "notes.txt", "access": "GENERIC_WRITE", "creation": "OPEN_EXISTING"}},
        {"api": "SetWindowTextW", "args": {"text": "notes.txt - Notepad"}},
        {"api": "WriteFile", "args": {"bytes": 256}},
        {"api": "WriteFile", "args": {"bytes": 512}},

        # User copies some text
        {"api": "SetClipboardData", "args": {"format": "CF_UNICODETEXT", "data": "Hello World"}},

        # User saves
        {"api": "GetSaveFileNameW", "args": {"defaultName": "notes.txt"}},
        {"api": "WriteFile", "args": {"bytes": 1024}},

        # User closes
        {"api": "CloseHandle", "args": {"handle": 0x1234}},
    ]
)

# Simulated vim session (POSIX)
VIM_SESSION = SimulatedApp(
    name="vim",
    pid=54321,
    syscalls=[
        # Open file
        {"api": "open", "args": {"path": "/home/user/code.py", "flags": 2}},  # O_RDWR

        # Read file
        {"api": "read", "args": {"fd": 3, "bytes": 4096, "path": "/home/user/code.py"}},

        # Edit and write
        {"api": "write", "args": {"fd": 3, "bytes": 128, "path": "/home/user/code.py"}},
        {"api": "write", "args": {"fd": 3, "bytes": 256, "path": "/home/user/code.py"}},

        # Save and close
        {"api": "write", "args": {"fd": 3, "bytes": 4200, "path": "/home/user/code.py"}},
        {"api": "close", "args": {"fd": 3, "path": "/home/user/code.py"}},
    ]
)

# Simulated file manager operations
EXPLORER_SESSION = SimulatedApp(
    name="explorer.exe",
    pid=99999,
    syscalls=[
        # User copies a file
        {"api": "CopyFileW", "args": {"lpExistingFileName": "document.docx", "lpNewFileName": "document_backup.docx"}},

        # User renames a file
        {"api": "MoveFileW", "args": {"lpExistingFileName": "old_name.txt", "lpNewFileName": "new_name.txt"}},

        # User deletes a file
        {"api": "DeleteFileW", "args": {"lpFileName": "temp.txt"}},
    ]
)


class SemanticKernelStub:
    """Stub kernel that logs semantic operations."""

    def __init__(self):
        self.operations = []
        self.documents = {}

    def execute(self, op: SemanticOp):
        """Execute a semantic operation."""
        self.operations.append(op)

        # Simple simulation of operation effects
        if op.primitive.value == "ctx.attach":
            self.documents[op.target] = {"intent": op.intent, "active": True}
        elif op.primitive.value == "ctx.detach":
            if op.target in self.documents:
                self.documents[op.target]["active"] = False

    def get_state(self):
        """Get current kernel state."""
        return {
            "operations_count": len(self.operations),
            "open_documents": [k for k, v in self.documents.items() if v.get("active")],
        }


def print_separator(title):
    """Print a section separator."""
    print(f"\n{'='*60}")
    print(f"  {title}")
    print(f"{'='*60}")


def print_operation(op: SemanticOp, indent="  "):
    """Pretty print a semantic operation."""
    print(f"{indent}Semantic: {op.primitive.value}")
    print(f"{indent}  Target: {op.target}")
    if op.intent:
        print(f"{indent}  Intent: {op.intent}")
    if op.params:
        params_str = str(op.params)
        if len(params_str) > 60:
            params_str = params_str[:57] + "..."
        print(f"{indent}  Params: {params_str}")
    print(f"{indent}  Confidence: {op.confidence:.0%}")


def simulate_app_session(app: SimulatedApp, translator: SemanticAPITranslator, kernel: SemanticKernelStub):
    """Simulate an application session with syscall interception."""

    print_separator(f"Simulating: {app.name} (PID: {app.pid})")

    for syscall in app.syscalls:
        # Create intercepted call
        call = InterceptedCall(
            api_name=syscall["api"],
            args=syscall["args"],
            pid=app.pid,
            app_name=app.name,
            timestamp=time.time(),
        )

        # Create context for translation
        context = APICallContext(
            api_name=call.api_name,
            args=call.args,
            pid=call.pid,
            app_name=call.app_name,
        )

        # Translate
        result = translator.translate(context)

        # Print the translation
        print(f"\n[{app.name}] {call.api_name}")
        args_str = str(call.args)
        if len(args_str) > 50:
            args_str = args_str[:47] + "..."
        print(f"  Args: {args_str}")

        if result.success:
            for op in result.operations:
                print_operation(op, indent="  -> ")
                kernel.execute(op)
        else:
            print(f"  -> [SKIP] {result.reasoning}")

        # Small delay for visual effect
        time.sleep(0.1)


def main():
    print("="*60)
    print("  SYSCALL HOOK SIMULATION")
    print("  Legacy Apps -> Semantic Operations")
    print("="*60)

    # Create translator and kernel
    translator = SemanticAPITranslator()
    kernel = SemanticKernelStub()

    # Simulate different app sessions
    simulate_app_session(NOTEPAD_SESSION, translator, kernel)
    simulate_app_session(VIM_SESSION, translator, kernel)
    simulate_app_session(EXPLORER_SESSION, translator, kernel)

    # Print final statistics
    print_separator("SIMULATION RESULTS")

    stats = translator.get_stats()
    print(f"\nTranslation Statistics:")
    print(f"  Total API calls:    {stats['total']}")
    print(f"  Pattern matches:    {stats['pattern_matches']}")
    print(f"  AI translations:    {stats['ai_translations']}")
    print(f"  Failures:           {stats['failures']}")
    if stats['total'] > 0:
        success_rate = (stats['pattern_matches'] + stats['ai_translations']) / stats['total'] * 100
        print(f"  Success rate:       {success_rate:.1f}%")

    print(f"\nKernel State:")
    state = kernel.get_state()
    print(f"  Total operations:   {state['operations_count']}")
    print(f"  Open documents:     {state['open_documents']}")

    # Show semantic event flow summary
    print_separator("SEMANTIC EVENT FLOW")

    print("\nEvent types generated:")
    event_counts = {}
    for op in kernel.operations:
        event_type = f"{op.primitive.value}"
        event_counts[event_type] = event_counts.get(event_type, 0) + 1

    for event_type, count in sorted(event_counts.items()):
        print(f"  {event_type}: {count}")

    print("\nDocument lifecycle:")
    doc_events = {}
    for op in kernel.operations:
        if op.primitive.value in ("ctx.attach", "ctx.detach"):
            if op.target not in doc_events:
                doc_events[op.target] = []
            doc_events[op.target].append(op.primitive.value.split(".")[-1])

    for doc, events in doc_events.items():
        print(f"  {doc}: {' -> '.join(events)}")


if __name__ == "__main__":
    main()
