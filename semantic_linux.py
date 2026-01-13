#!/usr/bin/env python3
"""
Semantic OS for Linux
=====================

A true semantic operating system layer. Every syscall is intercepted,
logged, and queryable by meaning.

This is the real thing - we see EVERYTHING a process does:
- Every file it opens, reads, writes
- Every process it spawns
- Every network connection
- Every directory change

Usage:
    # Interactive shell (every command traced)
    ./semantic_linux.py

    # Run a single command
    ./semantic_linux.py ls -la
    ./semantic_linux.py python script.py

    # Query history
    ./semantic_linux.py --query "files opened today"
    ./semantic_linux.py --history

Requirements:
    - Linux (or WSL)
    - Python 3.8+
    - Root not required (traces own children)
"""

import sys
import os
import time
import readline  # For command history in the shell
from pathlib import Path
from datetime import datetime
from typing import Optional, List, Dict

# Add parent to path for imports
sys.path.insert(0, str(Path(__file__).parent))

# Check we're on Linux
if sys.platform != 'linux':
    print("Error: Semantic OS Linux requires Linux (or WSL)")
    print("Run this inside WSL: wsl python3 semantic_linux.py")
    sys.exit(1)

from kernel.linux_tracer import SyscallTracer, SemanticSyscallHandler, SyscallEvent


class SemanticLinuxKernel:
    """The Semantic OS kernel for Linux.

    Manages the semantic memory and process tracing.
    """

    def __init__(self, db_path: str = None):
        """Initialize the kernel.

        Args:
            db_path: Path to SQLite database for persistence
        """
        # Determine database path
        if db_path is None:
            data_dir = Path.home() / '.semantic_os'
            data_dir.mkdir(exist_ok=True)
            db_path = str(data_dir / 'kernel.db')

        self.db_path = db_path

        # Import kernel components
        from kernel.memory import SemanticMemory, MemoryType
        self.MemoryType = MemoryType

        # Initialize semantic memory
        self.memory = SemanticMemory(db_path)

        # Track processes
        self.processes: Dict[int, dict] = {}

        # Boot event
        self._boot_time = time.time()
        self.memory.store(
            content="Semantic OS Linux kernel booted",
            type=MemoryType.EVENT,
            metadata={
                "event": "kernel.boot",
                "platform": "linux",
                "timestamp": self._boot_time,
            }
        )

        print(f"[Kernel] Booted. Database: {db_path}")

    def log_syscall(self, event: SyscallEvent):
        """Log a syscall event to semantic memory."""
        content = f"Syscall: {event.syscall_name}"
        if event.path:
            content += f" path={event.path}"

        metadata = {
            "event": f"syscall.{event.syscall_name.lower()}",
            "pid": event.pid,
            "syscall_num": event.syscall_num,
            "result": event.result,
            "timestamp": event.timestamp,
        }

        if event.path:
            metadata["path"] = event.path
        if event.fd is not None:
            metadata["fd"] = event.fd

        self.memory.store(
            content=content,
            type=self.MemoryType.EVENT,
            metadata=metadata
        )

    def log_command(self, command: str, cwd: str):
        """Log a command execution."""
        self.memory.store(
            content=f"Command executed: {command}",
            type=self.MemoryType.EVENT,
            metadata={
                "event": "command.executed",
                "command": command,
                "cwd": cwd,
                "timestamp": time.time(),
            }
        )

    def log_process_start(self, pid: int, command: str):
        """Log process start."""
        self.processes[pid] = {
            "command": command,
            "started_at": time.time(),
            "files_opened": [],
            "files_read": [],
            "files_written": [],
        }

        self.memory.store(
            content=f"Process started: {command}",
            type=self.MemoryType.PROCESS,
            metadata={
                "event": "process.started",
                "pid": pid,
                "command": command,
                "timestamp": time.time(),
            }
        )

    def log_process_end(self, pid: int, exit_code: int, summary: dict):
        """Log process end with summary."""
        proc = self.processes.pop(pid, {})

        self.memory.store(
            content=f"Process ended: {proc.get('command', 'unknown')} (exit={exit_code})",
            type=self.MemoryType.EVENT,
            metadata={
                "event": "process.ended",
                "pid": pid,
                "exit_code": exit_code,
                "duration": time.time() - proc.get("started_at", time.time()),
                "syscalls": summary.get("total_syscalls", 0),
                "files_opened": summary.get("files_opened", []),
                "files_read": summary.get("files_read", []),
                "files_written": summary.get("files_written", []),
                "timestamp": time.time(),
            }
        )

    def query(self, query: str, limit: int = 20) -> List[tuple]:
        """Query semantic memory."""
        return self.memory.query(query, limit=limit)

    def get_recent_commands(self, limit: int = 20) -> List[dict]:
        """Get recent commands."""
        events = self.memory.get_by_type(self.MemoryType.EVENT, limit=200)

        commands = []
        for event in events:
            if event.metadata.get("event") == "command.executed":
                commands.append({
                    "command": event.metadata.get("command"),
                    "cwd": event.metadata.get("cwd"),
                    "timestamp": event.metadata.get("timestamp"),
                })
                if len(commands) >= limit:
                    break

        return commands

    def get_file_history(self, path: str) -> List[dict]:
        """Get access history for a file."""
        events = self.memory.get_by_type(self.MemoryType.EVENT, limit=500)

        history = []
        for event in events:
            if event.metadata.get("path") == path:
                history.append({
                    "event": event.metadata.get("event"),
                    "pid": event.metadata.get("pid"),
                    "timestamp": event.metadata.get("timestamp"),
                })

        return history[:50]

    def stats(self) -> dict:
        """Get kernel statistics."""
        mem_stats = self.memory.stats()
        return {
            "uptime": time.time() - self._boot_time,
            "memory": mem_stats,
            "active_processes": len(self.processes),
        }

    def shutdown(self):
        """Shutdown the kernel."""
        self.memory.store(
            content="Semantic OS Linux kernel shutdown",
            type=self.MemoryType.EVENT,
            metadata={
                "event": "kernel.shutdown",
                "uptime": time.time() - self._boot_time,
                "timestamp": time.time(),
            }
        )
        self.memory.close()


class SemanticShell:
    """Interactive shell for Semantic OS Linux.

    Every command you run is traced at the syscall level.
    """

    BANNER = """
+===========================================================+
|                  SEMANTIC OS for LINUX                    |
|                                                           |
|  Every syscall is intercepted and logged semantically.    |
|  Query your activity by meaning, not just keywords.       |
|                                                           |
|  Commands:                                                |
|    /help     - Show help                                  |
|    /history  - Show command history                       |
|    /files    - Show recent file activity                  |
|    /query    - Semantic search                            |
|    /stats    - Show statistics                            |
|    exit      - Exit shell                                 |
+===========================================================+
"""

    def __init__(self, kernel: SemanticLinuxKernel):
        self.kernel = kernel
        self.cwd = os.getcwd()

    def run(self):
        """Run the interactive shell."""
        print(self.BANNER)
        print(f"[Shell] Working directory: {self.cwd}")
        print()

        while True:
            try:
                # Get prompt
                prompt = f"\033[1;32msemantic\033[0m:\033[1;34m{self.cwd}\033[0m$ "
                command = input(prompt).strip()

                if not command:
                    continue

                # Handle built-in commands
                if command.startswith('/'):
                    self._handle_builtin(command)
                    continue

                if command in ('exit', 'quit'):
                    break

                # Handle cd specially (affects shell state)
                if command.startswith('cd '):
                    self._handle_cd(command[3:].strip())
                    continue

                # Execute command with syscall tracing
                self._execute_traced(command)

            except EOFError:
                print()
                break
            except KeyboardInterrupt:
                print()
                continue

        print("[Shell] Goodbye!")
        self.kernel.shutdown()

    def _handle_builtin(self, command: str):
        """Handle built-in commands."""
        parts = command.split(maxsplit=1)
        cmd = parts[0].lower()
        args = parts[1] if len(parts) > 1 else ""

        if cmd == '/help':
            self._show_help()
        elif cmd == '/history':
            self._show_history()
        elif cmd == '/files':
            self._show_files()
        elif cmd == '/query':
            self._do_query(args)
        elif cmd == '/stats':
            self._show_stats()
        elif cmd == '/filehistory':
            self._show_file_history(args)
        else:
            print(f"Unknown command: {cmd}")

    def _show_help(self):
        """Show help."""
        print("""
Semantic OS Shell Commands:

  /help              - Show this help
  /history           - Show command history
  /files             - Show recent file activity
  /query <text>      - Semantic search across all activity
  /filehistory <path> - Show access history for a file
  /stats             - Show kernel statistics

Regular shell commands are traced at the syscall level.
Every file open, read, write is logged semantically.
""")

    def _show_history(self):
        """Show command history."""
        commands = self.kernel.get_recent_commands(20)

        if not commands:
            print("No command history yet.")
            return

        print("\nRecent Commands:")
        print("-" * 50)
        for cmd in commands:
            ts = datetime.fromtimestamp(cmd["timestamp"])
            print(f"  {ts.strftime('%H:%M:%S')}  {cmd['command']}")

    def _show_files(self):
        """Show recent file activity."""
        # Query for file-related events
        events = self.kernel.memory.get_by_type(self.kernel.MemoryType.EVENT, limit=100)

        opened = []
        read = []
        written = []

        for event in events:
            path = event.metadata.get("path")
            if not path:
                continue

            evt_type = event.metadata.get("event", "")
            if "open" in evt_type.lower():
                if path not in opened:
                    opened.append(path)
            elif evt_type == "syscall.read":
                if path not in read:
                    read.append(path)
            elif evt_type == "syscall.write":
                if path not in written:
                    written.append(path)

        print("\nRecent File Activity:")
        print("-" * 50)

        if opened:
            print("\nOpened:")
            for f in opened[:10]:
                print(f"    {f}")

        if read:
            print("\nRead:")
            for f in read[:10]:
                print(f"    {f}")

        if written:
            print("\nWritten:")
            for f in written[:10]:
                print(f"    {f}")

        if not (opened or read or written):
            print("  No file activity yet.")

    def _do_query(self, query: str):
        """Perform semantic search."""
        if not query:
            query = input("Search query: ").strip()
            if not query:
                return

        print(f"\nSearching for: {query}")
        print("-" * 50)

        results = self.kernel.query(query, limit=15)

        if not results:
            print("No results found.")
            return

        for entry, score in results:
            ts = datetime.fromtimestamp(entry.created_at)
            preview = entry.content[:60] + "..." if len(entry.content) > 60 else entry.content
            print(f"  [{score:.2f}] {ts.strftime('%m/%d %H:%M')} - {preview}")

    def _show_file_history(self, path: str):
        """Show access history for a file."""
        if not path:
            path = input("File path: ").strip()
            if not path:
                return

        # Resolve path
        if not os.path.isabs(path):
            path = os.path.join(self.cwd, path)

        history = self.kernel.get_file_history(path)

        if not history:
            print(f"No history for: {path}")
            return

        print(f"\nAccess History: {path}")
        print("-" * 50)

        for item in history:
            ts = datetime.fromtimestamp(item["timestamp"])
            event = item["event"].replace("syscall.", "")
            print(f"  {ts.strftime('%m/%d %H:%M:%S')}  {event}  (pid={item['pid']})")

    def _show_stats(self):
        """Show kernel statistics."""
        stats = self.kernel.stats()

        print("\nSemantic OS Statistics:")
        print("-" * 50)
        print(f"  Uptime: {stats['uptime']:.1f}s")
        print(f"  Active processes: {stats['active_processes']}")
        print(f"  Memory entries: {stats['memory']['total_entries']}")
        print(f"  Relations: {stats['memory'].get('relations', 0)}")
        print(f"  Database: {stats['memory']['db_path']}")

    def _handle_cd(self, path: str):
        """Handle cd command."""
        if not path:
            path = str(Path.home())

        # Expand ~ and resolve
        path = os.path.expanduser(path)
        if not os.path.isabs(path):
            path = os.path.join(self.cwd, path)
        path = os.path.normpath(path)

        if os.path.isdir(path):
            self.cwd = path
            os.chdir(path)
            print(f"[Shell] Changed to: {path}")
        else:
            print(f"cd: no such directory: {path}")

    def _execute_traced(self, command: str):
        """Execute a command with syscall tracing."""
        # Log the command
        self.kernel.log_command(command, self.cwd)

        # Create handler that logs to kernel
        handler = SemanticSyscallHandler()

        def callback(event: SyscallEvent):
            handler.handle_syscall(event)
            self.kernel.log_syscall(event)

        tracer = SyscallTracer(callback=callback)

        # Parse command
        # Use /bin/sh -c for proper shell parsing
        args = ["/bin/sh", "-c", command]

        print(f"[Tracing] {command}")
        print("-" * 40)

        self.kernel.log_process_start(0, command)

        try:
            exit_code = tracer.trace_command(args, cwd=self.cwd)
        except Exception as e:
            print(f"[Error] {e}")
            exit_code = 1

        print("-" * 40)

        # Get summary
        summary = handler.get_summary()
        self.kernel.log_process_end(0, exit_code, summary)

        # Print summary
        if summary["files_opened"]:
            print(f"[Summary] Files opened: {len(summary['files_opened'])}")
        if summary["files_written"]:
            print(f"[Summary] Files written: {len(summary['files_written'])}")
        print(f"[Summary] Total syscalls: {summary['total_syscalls']}")


def run_single_command(args: List[str], kernel: SemanticLinuxKernel):
    """Run a single command and exit."""
    command = ' '.join(args)
    cwd = os.getcwd()

    kernel.log_command(command, cwd)

    handler = SemanticSyscallHandler()

    def callback(event: SyscallEvent):
        handler.handle_syscall(event)
        kernel.log_syscall(event)

    tracer = SyscallTracer(callback=callback)

    print(f"[Semantic OS] Executing: {command}")
    print("=" * 50)

    kernel.log_process_start(0, command)

    # Use /bin/sh -c to handle shell features (redirects, pipes, etc.)
    shell_args = ["/bin/sh", "-c", command]

    try:
        exit_code = tracer.trace_command(shell_args, cwd=cwd)
    except KeyboardInterrupt:
        print("\n[Interrupted]")
        tracer.stop()
        exit_code = 130

    print("=" * 50)

    summary = handler.get_summary()
    kernel.log_process_end(0, exit_code, summary)

    print(f"\n[Semantic OS] Summary:")
    print(f"  Exit code: {exit_code}")
    print(f"  Syscalls: {summary['total_syscalls']}")
    print(f"  Files opened: {len(summary['files_opened'])}")
    print(f"  Files read: {len(summary['files_read'])}")
    print(f"  Files written: {len(summary['files_written'])}")

    if summary['files_opened']:
        print(f"\n  Opened:")
        for f in summary['files_opened'][:10]:
            print(f"    {f}")

    kernel.shutdown()
    return exit_code


def main():
    import argparse

    parser = argparse.ArgumentParser(
        description="Semantic OS for Linux - Every syscall tracked semantically"
    )
    parser.add_argument(
        "--query", "-q",
        help="Perform semantic query and exit"
    )
    parser.add_argument(
        "--history",
        action="store_true",
        help="Show command history and exit"
    )
    parser.add_argument(
        "--db",
        help="Path to database file"
    )

    # Parse known args, rest is the command
    args, remaining = parser.parse_known_args()
    args.command = remaining

    # Initialize kernel
    kernel = SemanticLinuxKernel(db_path=args.db)

    if args.query:
        # Query mode
        results = kernel.query(args.query, limit=20)
        print(f"Results for: {args.query}")
        print("-" * 50)
        for entry, score in results:
            ts = datetime.fromtimestamp(entry.created_at)
            print(f"[{score:.2f}] {ts.strftime('%m/%d %H:%M')} - {entry.content[:60]}")
        kernel.shutdown()

    elif args.history:
        # History mode
        commands = kernel.get_recent_commands(30)
        print("Command History:")
        print("-" * 50)
        for cmd in commands:
            ts = datetime.fromtimestamp(cmd["timestamp"])
            print(f"  {ts.strftime('%Y-%m-%d %H:%M:%S')}  {cmd['command']}")
        kernel.shutdown()

    elif args.command:
        # Single command mode
        sys.exit(run_single_command(args.command, kernel))

    else:
        # Interactive shell mode
        shell = SemanticShell(kernel)
        shell.run()


if __name__ == "__main__":
    main()
