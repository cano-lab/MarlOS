"""
Linux Syscall Tracer
====================

Uses ptrace to intercept ALL syscalls from traced processes.
This is the core of Semantic OS on Linux - we see everything.

Intercepted syscalls:
    - open/openat: File access
    - read/write: I/O operations
    - execve: Process execution
    - fork/clone: Process creation
    - connect/sendto: Network operations
    - unlink/rename: File modifications

Usage:
    tracer = SyscallTracer(kernel)
    tracer.trace_command(["ls", "-la"])

    # Or trace an existing process
    tracer.attach(pid)
"""

import os
import sys
import ctypes
import struct
import signal
import time
from dataclasses import dataclass, field
from typing import Optional, Dict, List, Callable, Any, Tuple
from enum import IntEnum

# Only works on Linux
if sys.platform != 'linux':
    raise ImportError("linux_tracer only works on Linux")

# Load libc
libc = ctypes.CDLL("libc.so.6", use_errno=True)

# ptrace constants
PTRACE_TRACEME = 0
PTRACE_PEEKTEXT = 1
PTRACE_PEEKDATA = 2
PTRACE_PEEKUSER = 3
PTRACE_POKETEXT = 4
PTRACE_POKEDATA = 5
PTRACE_POKEUSER = 6
PTRACE_CONT = 7
PTRACE_KILL = 8
PTRACE_SINGLESTEP = 9
PTRACE_GETREGS = 12
PTRACE_SETREGS = 13
PTRACE_ATTACH = 16
PTRACE_DETACH = 17
PTRACE_SYSCALL = 24
PTRACE_SETOPTIONS = 0x4200
PTRACE_GETEVENTMSG = 0x4201
PTRACE_GETSIGINFO = 0x4202
PTRACE_O_TRACESYSGOOD = 0x00000001
PTRACE_O_TRACEFORK = 0x00000002
PTRACE_O_TRACEVFORK = 0x00000004
PTRACE_O_TRACECLONE = 0x00000008
PTRACE_O_TRACEEXEC = 0x00000010
PTRACE_O_TRACEEXIT = 0x00000040

# Wait constants
WALL = 0x40000000  # __WALL from sys/wait.h

# x86_64 syscall numbers (most common ones)
class Syscall(IntEnum):
    READ = 0
    WRITE = 1
    OPEN = 2
    CLOSE = 3
    STAT = 4
    FSTAT = 5
    LSTAT = 6
    POLL = 7
    LSEEK = 8
    MMAP = 9
    MPROTECT = 10
    MUNMAP = 11
    BRK = 12
    IOCTL = 16
    ACCESS = 21
    PIPE = 22
    SELECT = 23
    DUP = 32
    DUP2 = 33
    SOCKET = 41
    CONNECT = 42
    ACCEPT = 43
    SENDTO = 44
    RECVFROM = 45
    SENDMSG = 46
    RECVMSG = 47
    SHUTDOWN = 48
    BIND = 49
    LISTEN = 50
    GETSOCKNAME = 51
    CLONE = 56
    FORK = 57
    VFORK = 58
    EXECVE = 59
    EXIT = 60
    WAIT4 = 61
    KILL = 62
    UNAME = 63
    FCNTL = 72
    FLOCK = 73
    FSYNC = 74
    TRUNCATE = 76
    FTRUNCATE = 77
    GETDENTS = 78
    GETCWD = 79
    CHDIR = 80
    FCHDIR = 81
    RENAME = 82
    MKDIR = 83
    RMDIR = 84
    CREAT = 85
    LINK = 86
    UNLINK = 87
    SYMLINK = 88
    READLINK = 89
    CHMOD = 90
    FCHMOD = 91
    CHOWN = 92
    FCHOWN = 93
    UMASK = 95
    GETUID = 102
    GETGID = 104
    GETEUID = 107
    GETEGID = 108
    GETPPID = 110
    GETPGRP = 111
    SETSID = 112
    SETREUID = 113
    SETREGID = 114
    GETGROUPS = 115
    SETGROUPS = 116
    OPENAT = 257
    MKDIRAT = 258
    MKNODAT = 259
    FCHOWNAT = 260
    UNLINKAT = 263
    RENAMEAT = 264
    LINKAT = 265
    SYMLINKAT = 266
    READLINKAT = 267
    FCHMODAT = 268
    FACCESSAT = 269
    DUP3 = 292
    PIPE2 = 293
    RENAMEAT2 = 316


# x86_64 user_regs_struct
class UserRegsStruct(ctypes.Structure):
    _fields_ = [
        ("r15", ctypes.c_ulonglong),
        ("r14", ctypes.c_ulonglong),
        ("r13", ctypes.c_ulonglong),
        ("r12", ctypes.c_ulonglong),
        ("rbp", ctypes.c_ulonglong),
        ("rbx", ctypes.c_ulonglong),
        ("r11", ctypes.c_ulonglong),
        ("r10", ctypes.c_ulonglong),
        ("r9", ctypes.c_ulonglong),
        ("r8", ctypes.c_ulonglong),
        ("rax", ctypes.c_ulonglong),
        ("rcx", ctypes.c_ulonglong),
        ("rdx", ctypes.c_ulonglong),
        ("rsi", ctypes.c_ulonglong),
        ("rdi", ctypes.c_ulonglong),
        ("orig_rax", ctypes.c_ulonglong),
        ("rip", ctypes.c_ulonglong),
        ("cs", ctypes.c_ulonglong),
        ("eflags", ctypes.c_ulonglong),
        ("rsp", ctypes.c_ulonglong),
        ("ss", ctypes.c_ulonglong),
        ("fs_base", ctypes.c_ulonglong),
        ("gs_base", ctypes.c_ulonglong),
        ("ds", ctypes.c_ulonglong),
        ("es", ctypes.c_ulonglong),
        ("fs", ctypes.c_ulonglong),
        ("gs", ctypes.c_ulonglong),
    ]


def ptrace(request, pid, addr=0, data=0):
    """Call ptrace syscall."""
    result = libc.ptrace(request, pid, addr, data)
    if result == -1:
        errno = ctypes.get_errno()
        if errno != 0:
            raise OSError(errno, os.strerror(errno))
    return result


def read_string(pid: int, addr: int, max_len: int = 4096, debug: bool = False) -> str:
    """Read a null-terminated string from process memory."""
    if addr == 0:
        return ""

    # Try using /proc/<pid>/mem first (more reliable)
    try:
        with open(f"/proc/{pid}/mem", "rb") as f:
            f.seek(addr)
            data = f.read(min(max_len, 256))  # Start with smaller read
            null_pos = data.find(b'\x00')
            if null_pos >= 0:
                data = data[:null_pos]
            result = data.decode('utf-8', errors='replace')
            if debug:
                print(f"  [read_string] pid={pid} addr=0x{addr:x} result={result!r}")
            return result
    except (OSError, IOError) as e:
        if debug:
            print(f"  [read_string] /proc/{pid}/mem error: {e}")
        pass

    # Fallback to ptrace PEEKDATA
    result = []
    for i in range(0, max_len, 8):
        try:
            word = ptrace(PTRACE_PEEKDATA, pid, addr + i)
            word_bytes = struct.pack("Q", word & 0xFFFFFFFFFFFFFFFF)
            for b in word_bytes:
                if b == 0:
                    return bytes(result).decode('utf-8', errors='replace')
                result.append(b)
        except OSError:
            break

    return bytes(result).decode('utf-8', errors='replace')


@dataclass
class SyscallEvent:
    """A captured syscall event."""
    pid: int
    syscall_num: int
    syscall_name: str
    args: Tuple
    result: Optional[int] = None
    timestamp: float = field(default_factory=time.time)

    # Decoded information
    path: Optional[str] = None
    fd: Optional[int] = None
    flags: Optional[int] = None
    mode: Optional[int] = None
    buffer_preview: Optional[str] = None

    def __repr__(self):
        if self.path:
            return f"<Syscall {self.syscall_name}({self.path!r}) = {self.result}>"
        return f"<Syscall {self.syscall_name}(...) = {self.result}>"


@dataclass
class TracedProcess:
    """A process being traced."""
    pid: int
    command: str
    parent_pid: Optional[int] = None
    started_at: float = field(default_factory=time.time)

    # State
    in_syscall: bool = False
    current_syscall: Optional[int] = None
    current_args: Tuple = ()

    # Statistics
    syscall_count: int = 0
    files_opened: List[str] = field(default_factory=list)
    files_read: List[str] = field(default_factory=list)
    files_written: List[str] = field(default_factory=list)
    children: List[int] = field(default_factory=list)

    # File descriptor tracking
    fd_table: Dict[int, str] = field(default_factory=dict)


class SyscallTracer:
    """Traces syscalls using ptrace.

    This is the core of Semantic OS on Linux - we intercept every
    syscall and log it semantically.
    """

    # Syscalls we care about for semantic logging
    INTERESTING_SYSCALLS = {
        Syscall.OPEN, Syscall.OPENAT, Syscall.CREAT,
        Syscall.READ, Syscall.WRITE,
        Syscall.CLOSE,
        Syscall.EXECVE,
        Syscall.FORK, Syscall.VFORK, Syscall.CLONE,
        Syscall.UNLINK, Syscall.UNLINKAT,
        Syscall.RENAME, Syscall.RENAMEAT, Syscall.RENAMEAT2,
        Syscall.MKDIR, Syscall.MKDIRAT, Syscall.RMDIR,
        Syscall.CONNECT, Syscall.BIND,
        Syscall.CHDIR, Syscall.FCHDIR,
        Syscall.DUP, Syscall.DUP2, Syscall.DUP3,
    }

    def __init__(self, callback: Callable[[SyscallEvent], None] = None):
        """Initialize the tracer.

        Args:
            callback: Function called for each syscall event
        """
        self.callback = callback or self._default_callback
        self.processes: Dict[int, TracedProcess] = {}
        self._running = False

    def _default_callback(self, event: SyscallEvent):
        """Default callback - just print interesting syscalls."""
        if event.syscall_num in self.INTERESTING_SYSCALLS:
            print(f"[{event.pid}] {event}")

    def trace_command(self, args: List[str], cwd: str = None) -> int:
        """Trace a command from start.

        Args:
            args: Command and arguments (e.g., ["ls", "-la"])
            cwd: Working directory

        Returns:
            Exit code of the traced process
        """
        pid = os.fork()

        if pid == 0:
            # Child process
            try:
                # Request to be traced
                ptrace(PTRACE_TRACEME, 0)

                # Stop ourselves so parent can set options
                os.kill(os.getpid(), signal.SIGSTOP)

                # Change directory if specified
                if cwd:
                    os.chdir(cwd)

                # Execute the command
                os.execvp(args[0], args)
            except Exception as e:
                print(f"Child exec failed: {e}", file=sys.stderr)
                os._exit(1)
        else:
            # Parent process - trace the child
            return self._trace_pid(pid, ' '.join(args))

    def attach(self, pid: int) -> int:
        """Attach to an existing process.

        Args:
            pid: Process ID to attach to

        Returns:
            Exit code when process exits
        """
        ptrace(PTRACE_ATTACH, pid)
        return self._trace_pid(pid, f"attached:{pid}")

    def _trace_pid(self, pid: int, command: str) -> int:
        """Main tracing loop for a process."""
        # Wait for initial stop
        _, status = os.waitpid(pid, 0)

        # Set tracing options
        options = (
            PTRACE_O_TRACESYSGOOD |  # Set bit 7 in signal for syscall stops
            PTRACE_O_TRACEFORK |      # Trace fork
            PTRACE_O_TRACEVFORK |     # Trace vfork
            PTRACE_O_TRACECLONE |     # Trace clone
            PTRACE_O_TRACEEXEC        # Trace exec
        )
        ptrace(PTRACE_SETOPTIONS, pid, 0, options)

        # Create tracked process
        self.processes[pid] = TracedProcess(pid=pid, command=command)

        # Continue execution, stopping at syscalls
        ptrace(PTRACE_SYSCALL, pid)

        self._running = True
        exit_code = 0

        while self._running and self.processes:
            try:
                # Wait for any traced process
                wpid, status = os.waitpid(-1, WALL)
            except ChildProcessError:
                break

            if wpid not in self.processes:
                # New child process from fork/clone
                self.processes[wpid] = TracedProcess(
                    pid=wpid,
                    command=f"child of {pid}",
                    parent_pid=pid
                )
                if pid in self.processes:
                    self.processes[pid].children.append(wpid)

            proc = self.processes[wpid]

            if os.WIFEXITED(status):
                # Process exited normally
                exit_code = os.WEXITSTATUS(status)
                del self.processes[wpid]
                if wpid == pid:
                    self._running = False
                continue

            elif os.WIFSIGNALED(status):
                # Process killed by signal
                del self.processes[wpid]
                if wpid == pid:
                    self._running = False
                continue

            elif os.WIFSTOPPED(status):
                sig = os.WSTOPSIG(status)

                # Check if this is a syscall stop (bit 7 set due to TRACESYSGOOD)
                if sig == (signal.SIGTRAP | 0x80):
                    self._handle_syscall(proc)

                # Check for ptrace events
                elif sig == signal.SIGTRAP:
                    event = (status >> 16) & 0xFFFF
                    if event in (1, 2, 3):  # FORK, VFORK, CLONE
                        # Get the new child PID
                        new_pid = ctypes.c_ulong()
                        ptrace(PTRACE_GETEVENTMSG, wpid, 0, ctypes.byref(new_pid))
                        # Will be handled when we wait for it

                # Other signal - deliver it
                else:
                    ptrace(PTRACE_SYSCALL, wpid, 0, sig)
                    continue

            # Continue tracing
            try:
                ptrace(PTRACE_SYSCALL, wpid)
            except OSError:
                pass

        return exit_code

    def _handle_syscall(self, proc: TracedProcess):
        """Handle a syscall entry or exit."""
        # Get registers
        regs = UserRegsStruct()
        ptrace(PTRACE_GETREGS, proc.pid, 0, ctypes.byref(regs))

        syscall_num = regs.orig_rax

        if not proc.in_syscall:
            # Syscall entry
            proc.in_syscall = True
            proc.current_syscall = syscall_num
            proc.current_args = (regs.rdi, regs.rsi, regs.rdx, regs.r10, regs.r8, regs.r9)
        else:
            # Syscall exit
            proc.in_syscall = False
            proc.syscall_count += 1

            result = regs.rax
            # Convert to signed
            if result > 0x7FFFFFFFFFFFFFFF:
                result = result - 0x10000000000000000

            event = self._decode_syscall(
                proc,
                proc.current_syscall,
                proc.current_args,
                result
            )

            if event:
                self.callback(event)

    def _decode_syscall(self, proc: TracedProcess, syscall_num: int,
                        args: Tuple, result: int) -> Optional[SyscallEvent]:
        """Decode a syscall into a semantic event."""
        try:
            syscall_name = Syscall(syscall_num).name
        except ValueError:
            syscall_name = f"syscall_{syscall_num}"

        event = SyscallEvent(
            pid=proc.pid,
            syscall_num=syscall_num,
            syscall_name=syscall_name,
            args=args,
            result=result,
        )

        # Decode specific syscalls
        try:
            if syscall_num == Syscall.OPEN:
                event.path = read_string(proc.pid, args[0])
                event.flags = args[1]
                event.mode = args[2]
                if result >= 0:
                    proc.fd_table[result] = event.path
                    proc.files_opened.append(event.path)

            elif syscall_num == Syscall.OPENAT:
                event.fd = args[0]
                event.path = read_string(proc.pid, args[1])
                event.flags = args[2]
                event.mode = args[3]
                if result >= 0:
                    proc.fd_table[result] = event.path
                    proc.files_opened.append(event.path)

            elif syscall_num == Syscall.CLOSE:
                event.fd = args[0]
                if args[0] in proc.fd_table:
                    event.path = proc.fd_table.pop(args[0])

            elif syscall_num == Syscall.READ:
                event.fd = args[0]
                if args[0] in proc.fd_table:
                    event.path = proc.fd_table[args[0]]
                    if event.path not in proc.files_read:
                        proc.files_read.append(event.path)

            elif syscall_num == Syscall.WRITE:
                event.fd = args[0]
                if args[0] in proc.fd_table:
                    event.path = proc.fd_table[args[0]]
                    if event.path not in proc.files_written:
                        proc.files_written.append(event.path)

            elif syscall_num == Syscall.EXECVE:
                event.path = read_string(proc.pid, args[0])

            elif syscall_num in (Syscall.UNLINK, Syscall.RMDIR):
                event.path = read_string(proc.pid, args[0])

            elif syscall_num == Syscall.UNLINKAT:
                event.fd = args[0]
                event.path = read_string(proc.pid, args[1])

            elif syscall_num == Syscall.RENAME:
                old_path = read_string(proc.pid, args[0])
                new_path = read_string(proc.pid, args[1])
                event.path = f"{old_path} -> {new_path}"

            elif syscall_num in (Syscall.MKDIR, Syscall.MKDIRAT):
                if syscall_num == Syscall.MKDIR:
                    event.path = read_string(proc.pid, args[0])
                else:
                    event.path = read_string(proc.pid, args[1])

            elif syscall_num == Syscall.CHDIR:
                event.path = read_string(proc.pid, args[0])

            elif syscall_num in (Syscall.DUP, Syscall.DUP2, Syscall.DUP3):
                old_fd = args[0]
                if old_fd in proc.fd_table and result >= 0:
                    proc.fd_table[result] = proc.fd_table[old_fd]
                event.fd = old_fd

        except Exception:
            pass  # Ignore decoding errors

        return event

    def stop(self):
        """Stop tracing."""
        self._running = False
        for pid in list(self.processes.keys()):
            try:
                ptrace(PTRACE_DETACH, pid)
            except OSError:
                pass


class SemanticSyscallHandler:
    """Handles syscalls and logs them to the semantic kernel."""

    def __init__(self, kernel=None):
        """Initialize with optional kernel connection."""
        self.kernel = kernel
        self._events: List[SyscallEvent] = []

    def handle_syscall(self, event: SyscallEvent):
        """Handle a syscall event."""
        self._events.append(event)

        # Only log interesting syscalls
        if event.syscall_num not in SyscallTracer.INTERESTING_SYSCALLS:
            return

        # Log to kernel if available
        if self.kernel:
            self._log_to_kernel(event)

        # Print to console
        self._print_event(event)

    def _log_to_kernel(self, event: SyscallEvent):
        """Log syscall to semantic kernel."""
        try:
            # Import here to avoid circular imports
            from kernel import MemoryType

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

            self.kernel.memory.store(
                content=content,
                type=MemoryType.EVENT,
                metadata=metadata
            )

            # Emit kernel event for real-time listeners
            self.kernel.emit(f"syscall.{event.syscall_name.lower()}", {
                "pid": event.pid,
                "path": event.path,
                "result": event.result,
            })

        except Exception as e:
            pass  # Don't crash on logging errors

    def _print_event(self, event: SyscallEvent):
        """Print event to console."""
        result_str = f"= {event.result}" if event.result is not None else ""

        if event.path:
            print(f"[{event.pid}] {event.syscall_name}({event.path!r}) {result_str}")
        elif event.fd is not None:
            # Show fd with mapped path if known
            fd_display = f"fd={event.fd}"
            print(f"[{event.pid}] {event.syscall_name}({fd_display}) {result_str}")

    def get_summary(self) -> Dict:
        """Get summary of traced syscalls."""
        by_name = {}
        files_opened = set()
        files_read = set()
        files_written = set()

        for event in self._events:
            name = event.syscall_name
            by_name[name] = by_name.get(name, 0) + 1

            if event.path:
                if event.syscall_num in (Syscall.OPEN, Syscall.OPENAT, Syscall.CREAT):
                    files_opened.add(event.path)
                elif event.syscall_num == Syscall.READ:
                    files_read.add(event.path)
                elif event.syscall_num == Syscall.WRITE:
                    files_written.add(event.path)

        return {
            "total_syscalls": len(self._events),
            "by_name": by_name,
            "files_opened": list(files_opened),
            "files_read": list(files_read),
            "files_written": list(files_written),
        }


def main():
    """Command-line interface for the tracer."""
    import argparse

    parser = argparse.ArgumentParser(description="Trace syscalls of a command")
    parser.add_argument("command", nargs="+", help="Command to trace")
    parser.add_argument("--summary", action="store_true", help="Show summary at end")
    args = parser.parse_args()

    handler = SemanticSyscallHandler()
    tracer = SyscallTracer(callback=handler.handle_syscall)

    print(f"[Semantic OS] Tracing: {' '.join(args.command)}")
    print("-" * 50)

    try:
        exit_code = tracer.trace_command(args.command)
    except KeyboardInterrupt:
        print("\n[Interrupted]")
        tracer.stop()
        exit_code = 130

    print("-" * 50)
    print(f"[Semantic OS] Process exited with code: {exit_code}")

    if args.summary:
        summary = handler.get_summary()
        print(f"\nSyscall Summary:")
        print(f"  Total syscalls: {summary['total_syscalls']}")
        print(f"  Files opened: {len(summary['files_opened'])}")
        print(f"  Files read: {len(summary['files_read'])}")
        print(f"  Files written: {len(summary['files_written'])}")

        if summary['files_opened']:
            print(f"\n  Opened files:")
            for f in summary['files_opened'][:10]:
                print(f"    {f}")

    return exit_code


if __name__ == "__main__":
    sys.exit(main())
