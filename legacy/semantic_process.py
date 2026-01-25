"""
Semantic Process Manager
========================

Runs processes "inside" Semantic OS with syscall interception.

For CLI apps: Full stdin/stdout capture, command logging
For GUI apps: File dialog monitoring, window tracking

This is the bridge between real Windows processes and Semantic OS.

Usage:
    # Run a command inside Semantic OS
    python semantic_process.py cmd
    python semantic_process.py notepad.exe
    python semantic_process.py python script.py

    # Interactive terminal
    python semantic_process.py --terminal
"""

import sys
import os
import time
import subprocess
import threading
import queue
import signal
import ctypes
from pathlib import Path
from typing import Optional, Dict, List, Callable, Any
from dataclasses import dataclass, field
from datetime import datetime

# Windows-specific imports for process monitoring
if sys.platform == 'win32':
    import ctypes.wintypes
    from ctypes import windll, wintypes, byref, sizeof, create_unicode_buffer

from kernel import init_kernel, SemanticKernel, MemoryType, Process as KernelProcess


@dataclass
class SemanticProcess:
    """A process running inside Semantic OS."""
    pid: str                          # Semantic OS process ID
    real_pid: int                     # Real Windows PID
    name: str                         # Process name
    command: str                      # Full command line
    cwd: str                          # Working directory
    started_at: float = field(default_factory=time.time)

    # State
    status: str = "running"           # running, stopped, terminated
    exit_code: Optional[int] = None

    # I/O capture
    stdin_log: List[str] = field(default_factory=list)
    stdout_log: List[str] = field(default_factory=list)
    stderr_log: List[str] = field(default_factory=list)

    # File operations detected
    files_opened: List[str] = field(default_factory=list)
    files_written: List[str] = field(default_factory=list)

    # Subprocess handle
    _proc: Optional[subprocess.Popen] = field(default=None, repr=False)


class ProcessMonitor:
    """Monitors a running process for file operations."""

    def __init__(self, proc: SemanticProcess, kernel: SemanticKernel):
        self.proc = proc
        self.kernel = kernel
        self._stop_event = threading.Event()
        self._monitor_thread = None

    def start(self):
        """Start monitoring the process."""
        self._monitor_thread = threading.Thread(target=self._monitor_loop, daemon=True)
        self._monitor_thread.start()

    def stop(self):
        """Stop monitoring."""
        self._stop_event.set()
        if self._monitor_thread:
            self._monitor_thread.join(timeout=1)

    def _monitor_loop(self):
        """Monitor loop - checks for file operations."""
        # On Windows, we can use NtQuerySystemInformation or
        # walk /proc on Linux. For now, we'll use a simpler approach
        # of monitoring common locations and window titles.

        last_title = ""

        while not self._stop_event.is_set():
            try:
                # Check if process still running
                if self.proc._proc and self.proc._proc.poll() is not None:
                    self.proc.status = "terminated"
                    self.proc.exit_code = self.proc._proc.returncode
                    break

                # Try to detect file operations via window title
                if sys.platform == 'win32':
                    title = self._get_window_title(self.proc.real_pid)
                    if title and title != last_title:
                        last_title = title
                        self._parse_window_title(title)

            except Exception as e:
                pass  # Ignore monitoring errors

            time.sleep(0.5)

    def _get_window_title(self, pid: int) -> Optional[str]:
        """Get the window title for a process."""
        if sys.platform != 'win32':
            return None

        try:
            EnumWindows = windll.user32.EnumWindows
            GetWindowThreadProcessId = windll.user32.GetWindowThreadProcessId
            GetWindowTextW = windll.user32.GetWindowTextW
            GetWindowTextLengthW = windll.user32.GetWindowTextLengthW
            IsWindowVisible = windll.user32.IsWindowVisible

            titles = []

            def callback(hwnd, lparam):
                if IsWindowVisible(hwnd):
                    proc_id = wintypes.DWORD()
                    GetWindowThreadProcessId(hwnd, byref(proc_id))
                    if proc_id.value == pid:
                        length = GetWindowTextLengthW(hwnd)
                        if length > 0:
                            buff = create_unicode_buffer(length + 1)
                            GetWindowTextW(hwnd, buff, length + 1)
                            titles.append(buff.value)
                return True

            WNDENUMPROC = ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
            EnumWindows(WNDENUMPROC(callback), 0)

            return titles[0] if titles else None

        except Exception:
            return None

    def _parse_window_title(self, title: str):
        """Parse window title for file information."""
        # Many apps show filename in title: "filename - App Name" or "App Name - filename"

        # Common patterns
        parts = title.split(" - ")

        for part in parts:
            part = part.strip()
            # Check if it looks like a file path
            if os.path.exists(part):
                if part not in self.proc.files_opened:
                    self.proc.files_opened.append(part)
                    self.kernel.emit("process.file_opened", {
                        "pid": self.proc.pid,
                        "path": part,
                        "process": self.proc.name,
                    })
            # Check for just filename
            elif '.' in part and len(part) < 260:
                # Could be a filename, log it
                self.kernel.emit("process.file_detected", {
                    "pid": self.proc.pid,
                    "filename": part,
                    "title": title,
                })


class SemanticTerminal:
    """An interactive terminal running inside Semantic OS."""

    def __init__(self, kernel: SemanticKernel, shell: str = None):
        self.kernel = kernel
        self.shell = shell or self._get_default_shell()
        self.process: Optional[SemanticProcess] = None
        self._output_queue = queue.Queue()
        self._running = False

    def _get_default_shell(self) -> str:
        """Get the default shell for this platform."""
        if sys.platform == 'win32':
            return os.environ.get('COMSPEC', 'cmd.exe')
        else:
            return os.environ.get('SHELL', '/bin/bash')

    def start(self):
        """Start the terminal session."""
        # Create kernel process
        kernel_proc = self.kernel.create_process(self.shell)

        # Start the real shell process
        try:
            if sys.platform == 'win32':
                # Windows: use cmd with specific settings
                proc = subprocess.Popen(
                    self.shell,
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    shell=False,
                    cwd=str(Path.home()),
                    creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
                    bufsize=0,
                )
            else:
                proc = subprocess.Popen(
                    [self.shell],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    cwd=str(Path.home()),
                    bufsize=0,
                )

        except Exception as e:
            print(f"[Semantic OS] Failed to start shell: {e}")
            return False

        self.process = SemanticProcess(
            pid=kernel_proc.id,
            real_pid=proc.pid,
            name=Path(self.shell).name,
            command=self.shell,
            cwd=str(Path.home()),
            _proc=proc,
        )

        # Log process start
        self.kernel.memory.store(
            content=f"Process started: {self.shell}",
            type=MemoryType.EVENT,
            metadata={
                "event": "process.started",
                "pid": self.process.pid,
                "real_pid": self.process.real_pid,
                "command": self.shell,
                "cwd": self.process.cwd,
            }
        )

        self.kernel.emit("process.started", {
            "pid": self.process.pid,
            "name": self.process.name,
            "command": self.shell,
        })

        # Start output reader thread
        self._running = True
        self._reader_thread = threading.Thread(target=self._read_output, daemon=True)
        self._reader_thread.start()

        return True

    def _read_output(self):
        """Read output from the process."""
        try:
            while self._running and self.process._proc:
                try:
                    # Read one byte at a time for real-time output
                    char = self.process._proc.stdout.read(1)
                    if char:
                        self._output_queue.put(char)
                    elif self.process._proc.poll() is not None:
                        break
                except Exception:
                    break
        except Exception:
            pass
        finally:
            self._running = False

    def send_command(self, command: str):
        """Send a command to the terminal."""
        if not self.process or not self.process._proc:
            return

        # Log the command
        self.process.stdin_log.append(command)

        self.kernel.memory.store(
            content=f"Command: {command}",
            type=MemoryType.EVENT,
            metadata={
                "event": "command.executed",
                "pid": self.process.pid,
                "command": command,
                "cwd": self.process.cwd,
                "timestamp": time.time(),
            }
        )

        self.kernel.emit("command.executed", {
            "pid": self.process.pid,
            "command": command,
        })

        # Detect file operations in command
        self._detect_file_ops(command)

        # Send to process
        try:
            self.process._proc.stdin.write(f"{command}\n".encode())
            self.process._proc.stdin.flush()
        except Exception as e:
            print(f"[Semantic OS] Error sending command: {e}")

    def _detect_file_ops(self, command: str):
        """Detect file operations in a command."""
        # Simple detection of common file operations
        parts = command.split()
        if not parts:
            return

        cmd = parts[0].lower()

        # File reading commands
        if cmd in ('cat', 'type', 'more', 'less', 'head', 'tail', 'code', 'notepad', 'vim', 'nano'):
            for arg in parts[1:]:
                if not arg.startswith('-'):
                    path = self._resolve_path(arg)
                    if path:
                        self.process.files_opened.append(path)
                        self.kernel.emit("process.file_opened", {
                            "pid": self.process.pid,
                            "path": path,
                            "command": command,
                        })

        # File writing commands
        elif cmd in ('echo', 'printf') and '>' in command:
            # Find the redirect target
            idx = command.find('>')
            target = command[idx+1:].strip().split()[0] if idx > 0 else None
            if target:
                path = self._resolve_path(target)
                if path:
                    self.process.files_written.append(path)
                    self.kernel.emit("process.file_written", {
                        "pid": self.process.pid,
                        "path": path,
                        "command": command,
                    })

        # Directory changes
        elif cmd == 'cd' and len(parts) > 1:
            new_dir = self._resolve_path(parts[1])
            if new_dir and os.path.isdir(new_dir):
                self.process.cwd = new_dir
                self.kernel.emit("process.cwd_changed", {
                    "pid": self.process.pid,
                    "cwd": new_dir,
                })

    def _resolve_path(self, path: str) -> Optional[str]:
        """Resolve a path relative to current working directory."""
        if os.path.isabs(path):
            return path if os.path.exists(path) else None
        full_path = os.path.join(self.process.cwd, path)
        return full_path if os.path.exists(full_path) else path

    def get_output(self, timeout: float = 0.1) -> str:
        """Get available output from the terminal."""
        output = []
        deadline = time.time() + timeout

        while time.time() < deadline:
            try:
                char = self._output_queue.get(timeout=0.05)
                output.append(char.decode('utf-8', errors='replace'))
            except queue.Empty:
                if output:
                    break
                continue

        result = ''.join(output)

        if result:
            self.process.stdout_log.append(result)

        return result

    def is_running(self) -> bool:
        """Check if terminal is still running."""
        if not self.process or not self.process._proc:
            return False
        return self.process._proc.poll() is None

    def terminate(self):
        """Terminate the terminal session."""
        self._running = False

        if self.process and self.process._proc:
            try:
                self.process._proc.terminate()
                self.process._proc.wait(timeout=2)
            except Exception:
                try:
                    self.process._proc.kill()
                except Exception:
                    pass

            self.process.status = "terminated"
            self.process.exit_code = self.process._proc.returncode

            self.kernel.emit("process.terminated", {
                "pid": self.process.pid,
                "exit_code": self.process.exit_code,
            })

            self.kernel.memory.store(
                content=f"Process terminated: {self.process.name}",
                type=MemoryType.EVENT,
                metadata={
                    "event": "process.terminated",
                    "pid": self.process.pid,
                    "exit_code": self.process.exit_code,
                    "duration": time.time() - self.process.started_at,
                }
            )


class SemanticProcessManager:
    """Manages processes running inside Semantic OS."""

    def __init__(self, kernel: SemanticKernel = None):
        self.kernel = kernel or init_kernel(enable_qt=False)
        self.processes: Dict[str, SemanticProcess] = {}
        self.monitors: Dict[str, ProcessMonitor] = {}

    def launch(self, command: str, cwd: str = None) -> SemanticProcess:
        """Launch a process inside Semantic OS."""
        cwd = cwd or os.getcwd()

        # Parse command
        if isinstance(command, str):
            parts = command.split()
            exe = parts[0]
        else:
            parts = command
            exe = parts[0]

        name = Path(exe).name

        # Create kernel process
        kernel_proc = self.kernel.create_process(name)

        # Start the real process
        try:
            proc = subprocess.Popen(
                command,
                shell=True,
                cwd=cwd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
        except Exception as e:
            self.kernel.emit("process.failed", {"command": command, "error": str(e)})
            raise

        sem_proc = SemanticProcess(
            pid=kernel_proc.id,
            real_pid=proc.pid,
            name=name,
            command=command if isinstance(command, str) else ' '.join(command),
            cwd=cwd,
            _proc=proc,
        )

        self.processes[sem_proc.pid] = sem_proc

        # Start monitor
        monitor = ProcessMonitor(sem_proc, self.kernel)
        monitor.start()
        self.monitors[sem_proc.pid] = monitor

        # Log
        self.kernel.memory.store(
            content=f"Launched: {command}",
            type=MemoryType.EVENT,
            metadata={
                "event": "process.launched",
                "pid": sem_proc.pid,
                "real_pid": sem_proc.real_pid,
                "command": sem_proc.command,
                "cwd": cwd,
            }
        )

        self.kernel.emit("process.launched", {
            "pid": sem_proc.pid,
            "name": name,
            "command": sem_proc.command,
        })

        return sem_proc

    def get_process(self, pid: str) -> Optional[SemanticProcess]:
        """Get a process by ID."""
        return self.processes.get(pid)

    def list_processes(self) -> List[SemanticProcess]:
        """List all processes."""
        return list(self.processes.values())

    def terminate(self, pid: str) -> bool:
        """Terminate a process."""
        proc = self.processes.get(pid)
        if not proc:
            return False

        # Stop monitor
        if pid in self.monitors:
            self.monitors[pid].stop()
            del self.monitors[pid]

        # Terminate process
        if proc._proc:
            try:
                proc._proc.terminate()
                proc._proc.wait(timeout=2)
            except Exception:
                try:
                    proc._proc.kill()
                except Exception:
                    pass

        proc.status = "terminated"
        proc.exit_code = proc._proc.returncode if proc._proc else -1

        self.kernel.emit("process.terminated", {
            "pid": pid,
            "exit_code": proc.exit_code,
        })

        return True

    def get_process_history(self) -> List[Dict]:
        """Get history of all processes."""
        events = self.kernel.memory.get_by_type(MemoryType.EVENT, limit=200)

        history = []
        for event in events:
            if event.metadata.get("event", "").startswith("process."):
                history.append({
                    "event": event.metadata.get("event"),
                    "pid": event.metadata.get("pid"),
                    "command": event.metadata.get("command"),
                    "timestamp": event.metadata.get("timestamp") or event.created_at,
                })

        return history


def run_interactive_terminal():
    """Run an interactive terminal inside Semantic OS."""
    print("""
+-----------------------------------------------------------+
|               SEMANTIC OS TERMINAL                        |
|                                                           |
|  Every command is logged semantically.                    |
|  Type 'exit' to quit, '/history' for command history      |
+-----------------------------------------------------------+
""")

    kernel = init_kernel(enable_qt=False)
    terminal = SemanticTerminal(kernel)

    if not terminal.start():
        print("Failed to start terminal")
        return

    print(f"[Semantic OS] Started {terminal.shell} (PID: {terminal.process.pid})")
    print(f"[Semantic OS] Working directory: {terminal.process.cwd}")
    print()

    try:
        while terminal.is_running():
            # Print any available output
            output = terminal.get_output(timeout=0.1)
            if output:
                print(output, end='', flush=True)

            # Check for input (non-blocking)
            try:
                if sys.platform == 'win32':
                    import msvcrt
                    if msvcrt.kbhit():
                        # Read a line
                        line = input()

                        # Handle special commands
                        if line.strip() == '/history':
                            print("\n[Semantic OS] Command History:")
                            for i, cmd in enumerate(terminal.process.stdin_log[-20:], 1):
                                print(f"  {i}. {cmd}")
                            continue
                        elif line.strip() == '/files':
                            print("\n[Semantic OS] Files accessed:")
                            for f in terminal.process.files_opened:
                                print(f"  [R] {f}")
                            for f in terminal.process.files_written:
                                print(f"  [W] {f}")
                            continue
                        elif line.strip() == '/stats':
                            stats = kernel.memory.stats()
                            print(f"\n[Semantic OS] Stats: {stats}")
                            continue

                        terminal.send_command(line)
                else:
                    import select
                    if select.select([sys.stdin], [], [], 0.1)[0]:
                        line = sys.stdin.readline().strip()
                        terminal.send_command(line)
            except EOFError:
                break

    except KeyboardInterrupt:
        print("\n[Semantic OS] Interrupted")
    finally:
        terminal.terminate()
        print(f"\n[Semantic OS] Session ended. Commands logged: {len(terminal.process.stdin_log)}")

        # Show summary
        if terminal.process.files_opened:
            print(f"[Semantic OS] Files opened: {len(terminal.process.files_opened)}")
        if terminal.process.files_written:
            print(f"[Semantic OS] Files written: {len(terminal.process.files_written)}")

        kernel.shutdown()


def run_command(command: str):
    """Run a single command inside Semantic OS."""
    kernel = init_kernel(enable_qt=False)
    manager = SemanticProcessManager(kernel)

    print(f"[Semantic OS] Launching: {command}")

    proc = manager.launch(command)
    print(f"[Semantic OS] Process started (PID: {proc.pid}, Real PID: {proc.real_pid})")

    # Wait for completion
    try:
        while proc._proc.poll() is None:
            # Read output
            try:
                output = proc._proc.stdout.read1(1024)
                if output:
                    print(output.decode('utf-8', errors='replace'), end='')
            except Exception:
                pass
            time.sleep(0.1)
    except KeyboardInterrupt:
        print("\n[Semantic OS] Terminating...")
        manager.terminate(proc.pid)

    print(f"\n[Semantic OS] Process exited with code: {proc._proc.returncode}")

    # Show file operations
    if proc.files_opened:
        print(f"[Semantic OS] Files opened: {proc.files_opened}")
    if proc.files_written:
        print(f"[Semantic OS] Files written: {proc.files_written}")

    kernel.shutdown()


def main():
    if len(sys.argv) < 2 or sys.argv[1] == '--terminal':
        run_interactive_terminal()
    else:
        # Run the specified command
        command = ' '.join(sys.argv[1:])
        run_command(command)


if __name__ == "__main__":
    main()
