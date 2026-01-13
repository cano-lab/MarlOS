"""
Semantic OS Syscall Emulator
============================

This is the REAL Semantic OS kernel. Instead of passing syscalls
to Linux, we HANDLE them ourselves. Apps run against pure semantic
memory - they never touch the real filesystem.

How it works:
1. ptrace intercepts syscall on ENTRY
2. We change the syscall to a dummy (getpid)
3. On EXIT, we set the return value to OUR result

The app thinks it's talking to a real OS, but everything
goes through semantic memory.

Usage:
    emulator = SyscallEmulator()
    emulator.run(["cat", "/etc/passwd"])
    # cat runs, but reads from semantic memory, not real filesystem
"""

import os
import sys
import ctypes
import struct
import signal
import time
from pathlib import Path
from typing import Optional, Dict, List, Any, Tuple, Callable
from dataclasses import dataclass, field
from enum import IntEnum

if sys.platform != 'linux':
    raise ImportError("syscall_emulator only works on Linux")

from .linux_tracer import (
    ptrace, read_string, UserRegsStruct, TracedProcess,
    PTRACE_TRACEME, PTRACE_SYSCALL, PTRACE_GETREGS, PTRACE_SETREGS,
    PTRACE_SETOPTIONS, PTRACE_O_TRACESYSGOOD, PTRACE_O_TRACEFORK,
    PTRACE_O_TRACEVFORK, PTRACE_O_TRACECLONE, PTRACE_O_TRACEEXEC,
    WALL, Syscall,
)


# Syscall we replace intercepted calls with (harmless, just returns pid)
DUMMY_SYSCALL = 39  # getpid


@dataclass
class FileAccess:
    """Record of a file access."""
    timestamp: float
    operation: str  # 'read', 'write', 'open', 'stat'
    process: str  # Command that accessed the file
    bytes_transferred: int = 0


@dataclass
class SemanticFile:
    """A file in semantic memory with semantic features."""
    path: str
    content: bytes = b""
    mode: int = 0o644
    is_dir: bool = False
    created_at: float = field(default_factory=time.time)
    modified_at: float = field(default_factory=time.time)

    # Semantic metadata
    metadata: Dict[str, Any] = field(default_factory=dict)
    tags: List[str] = field(default_factory=list)
    embedding: Optional[List[float]] = None  # Vector embedding
    access_history: List[FileAccess] = field(default_factory=list)
    related_files: List[str] = field(default_factory=list)  # Paths of related files

    def record_access(self, operation: str, process: str = "", bytes_count: int = 0):
        """Record an access to this file."""
        self.access_history.append(FileAccess(
            timestamp=time.time(),
            operation=operation,
            process=process,
            bytes_transferred=bytes_count
        ))
        # Keep only last 100 accesses
        if len(self.access_history) > 100:
            self.access_history = self.access_history[-100:]

    def get_text_content(self) -> str:
        """Get content as text for embedding."""
        try:
            return self.content.decode('utf-8', errors='ignore')
        except:
            return ""


@dataclass
class FileHandle:
    """An open file handle."""
    fd: int
    file: SemanticFile
    flags: int
    position: int = 0


@dataclass
class EmulatedProcess:
    """A process running in the emulated environment."""
    pid: int
    real_pid: int

    # File descriptor table (fd -> FileHandle)
    fd_table: Dict[int, FileHandle] = field(default_factory=dict)
    next_fd: int = 3  # 0,1,2 reserved for stdin/stdout/stderr

    # Current working directory
    cwd: str = "/"

    # Pending syscall info (set on entry, used on exit)
    pending_syscall: Optional[int] = None
    pending_result: Optional[int] = None
    pending_args: Tuple = ()


class SemanticFilesystem:
    """The semantic filesystem - all storage is here with semantic features."""

    def __init__(self):
        self.files: Dict[str, SemanticFile] = {}
        self._embedder = None  # Lazy load
        self._current_process: str = ""  # Track current command for access logging
        self._session_accesses: List[Tuple[str, float]] = []  # (path, time) for co-access
        self._init_default_files()

    def _get_embedder(self):
        """Lazy load sentence transformer for embeddings."""
        if self._embedder is None:
            try:
                from sentence_transformers import SentenceTransformer
                self._embedder = SentenceTransformer('all-MiniLM-L6-v2')
            except ImportError:
                pass  # Not available
        return self._embedder

    def set_current_process(self, process: str):
        """Set the current process name for access logging."""
        self._current_process = process

    def _init_default_files(self):
        """Create some default files for testing."""
        # /etc/passwd - classic test file
        self.create("/etc/passwd", b"root:x:0:0:root:/root:/bin/bash\n"
                                   b"semantic:x:1000:1000:Semantic OS:/home/semantic:/bin/bash\n")

        # /etc/hostname
        self.create("/etc/hostname", b"semantic-os\n")

        # /etc/os-release
        self.create("/etc/os-release",
                    b'NAME="Semantic OS"\n'
                    b'VERSION="1.0"\n'
                    b'ID=semantic\n'
                    b'PRETTY_NAME="Semantic OS 1.0"\n')

        # Some directories
        self.mkdir("/tmp")
        self.mkdir("/home")
        self.mkdir("/home/user")

        # A test file
        self.create("/home/user/hello.txt", b"Hello from Semantic OS!\n")

        # Root directory listing
        self.mkdir("/")
        self.mkdir("/etc")
        self.mkdir("/bin")
        self.mkdir("/usr")

    def normalize_path(self, path: str, cwd: str = "/") -> str:
        """Normalize a path, resolving . and .."""
        if not path.startswith("/"):
            path = cwd + "/" + path

        parts = []
        for part in path.split("/"):
            if part == "" or part == ".":
                continue
            elif part == "..":
                if parts:
                    parts.pop()
            else:
                parts.append(part)

        return "/" + "/".join(parts)

    def exists(self, path: str) -> bool:
        return path in self.files

    def is_dir(self, path: str) -> bool:
        f = self.files.get(path)
        return f is not None and f.is_dir

    def create(self, path: str, content: bytes = b"", mode: int = 0o644) -> SemanticFile:
        """Create a file."""
        f = SemanticFile(path=path, content=content, mode=mode)
        self.files[path] = f
        return f

    def mkdir(self, path: str, mode: int = 0o755) -> SemanticFile:
        """Create a directory."""
        f = SemanticFile(path=path, is_dir=True, mode=mode)
        self.files[path] = f
        return f

    def get(self, path: str) -> Optional[SemanticFile]:
        return self.files.get(path)

    def read(self, path: str) -> Optional[bytes]:
        f = self.files.get(path)
        return f.content if f else None

    def write(self, path: str, content: bytes):
        if path not in self.files:
            self.create(path, content)
        else:
            self.files[path].content = content
            self.files[path].modified_at = time.time()

    def append(self, path: str, content: bytes):
        if path not in self.files:
            self.create(path, content)
        else:
            self.files[path].content += content
            self.files[path].modified_at = time.time()

    def list_dir(self, path: str) -> List[str]:
        """List directory contents."""
        path = path.rstrip("/")
        entries = []
        for fpath in self.files:
            if fpath == path:
                continue
            parent = str(Path(fpath).parent)
            if parent == path or (path == "/" and parent == "/"):
                entries.append(Path(fpath).name)
        return sorted(set(entries))

    def unlink(self, path: str) -> bool:
        if path in self.files:
            del self.files[path]
            return True
        return False

    # ==================== SEMANTIC FEATURES ====================

    def record_file_access(self, path: str, operation: str, bytes_count: int = 0):
        """Record a file access for semantic analysis."""
        if path in self.files:
            f = self.files[path]
            f.record_access(operation, self._current_process, bytes_count)

            # Track co-access within 5 second window
            now = time.time()
            self._session_accesses.append((path, now))

            # Find co-accessed files and update relationships
            for other_path, access_time in self._session_accesses:
                if other_path != path and now - access_time < 5.0:
                    # These files were accessed together
                    if other_path not in f.related_files:
                        f.related_files.append(other_path)
                    if path in self.files and other_path in self.files:
                        other = self.files[other_path]
                        if path not in other.related_files:
                            other.related_files.append(path)

            # Clean old accesses
            self._session_accesses = [(p, t) for p, t in self._session_accesses
                                       if now - t < 10.0]

    def generate_embedding(self, path: str) -> bool:
        """Generate embedding for a file's content."""
        embedder = self._get_embedder()
        if not embedder or path not in self.files:
            return False

        f = self.files[path]
        text = f.get_text_content()
        if text:
            f.embedding = embedder.encode(text).tolist()
            return True
        return False

    def generate_all_embeddings(self):
        """Generate embeddings for all text files."""
        for path in self.files:
            if not self.files[path].is_dir:
                self.generate_embedding(path)

    def semantic_search(self, query: str, top_k: int = 5) -> List[Tuple[str, float]]:
        """Search files by semantic similarity to query."""
        embedder = self._get_embedder()
        if not embedder:
            return []

        # Generate query embedding
        query_embedding = embedder.encode(query)

        # Calculate similarities
        results = []
        for path, f in self.files.items():
            if f.is_dir or f.embedding is None:
                continue

            # Cosine similarity
            import numpy as np
            file_emb = np.array(f.embedding)
            query_emb = np.array(query_embedding)
            similarity = np.dot(file_emb, query_emb) / (
                np.linalg.norm(file_emb) * np.linalg.norm(query_emb) + 1e-8
            )
            results.append((path, float(similarity)))

        # Sort by similarity
        results.sort(key=lambda x: x[1], reverse=True)
        return results[:top_k]

    def search_by_tag(self, tag: str) -> List[str]:
        """Find files with a specific tag."""
        return [path for path, f in self.files.items()
                if tag.lower() in [t.lower() for t in f.tags]]

    def add_tag(self, path: str, tag: str):
        """Add a tag to a file."""
        if path in self.files:
            f = self.files[path]
            if tag not in f.tags:
                f.tags.append(tag)

    def get_recent_files(self, limit: int = 10) -> List[Tuple[str, float]]:
        """Get recently accessed files."""
        results = []
        for path, f in self.files.items():
            if f.is_dir or not f.access_history:
                continue
            last_access = f.access_history[-1].timestamp
            results.append((path, last_access))

        results.sort(key=lambda x: x[1], reverse=True)
        return results[:limit]

    def get_related_files(self, path: str) -> List[str]:
        """Get files related to the given file."""
        if path in self.files:
            return self.files[path].related_files.copy()
        return []

    def get_file_stats(self, path: str) -> Dict[str, Any]:
        """Get semantic statistics for a file."""
        if path not in self.files:
            return {}

        f = self.files[path]
        stats = {
            "path": path,
            "size": len(f.content),
            "created": f.created_at,
            "modified": f.modified_at,
            "tags": f.tags,
            "has_embedding": f.embedding is not None,
            "access_count": len(f.access_history),
            "related_files": f.related_files,
        }

        if f.access_history:
            stats["last_accessed"] = f.access_history[-1].timestamp
            stats["last_operation"] = f.access_history[-1].operation

        return stats

    def auto_tag_file(self, path: str) -> List[str]:
        """Automatically generate tags for a file based on content."""
        if path not in self.files:
            return []

        f = self.files[path]
        text = f.get_text_content().lower()
        tags = []

        # Simple keyword-based tagging
        tag_keywords = {
            "config": ["config", "settings", "conf", "ini", "yaml", "json"],
            "log": ["log", "error", "warning", "info", "debug"],
            "script": ["#!/", "bash", "python", "node", "perl"],
            "documentation": ["readme", "doc", "manual", "help", "usage"],
            "data": ["csv", "data", "record", "entry"],
            "system": ["/etc/", "/usr/", "/var/", "passwd", "hostname"],
        }

        for tag, keywords in tag_keywords.items():
            if any(kw in text or kw in path.lower() for kw in keywords):
                if tag not in f.tags:
                    f.tags.append(tag)
                tags.append(tag)

        return tags


class SyscallEmulator:
    """
    Emulates Linux syscalls using semantic memory.

    Apps run normally but their syscalls are intercepted and handled
    by us - they never touch the real filesystem.
    """

    def __init__(self, fs: SemanticFilesystem = None, verbose: bool = True):
        self.fs = fs or SemanticFilesystem()
        self.verbose = verbose
        self.processes: Dict[int, EmulatedProcess] = {}

        # Syscalls we emulate (others pass through to real kernel)
        self.emulated_syscalls = {
            Syscall.OPEN, Syscall.OPENAT,
            Syscall.READ, Syscall.WRITE,
            Syscall.CLOSE,
            Syscall.STAT, Syscall.FSTAT, Syscall.LSTAT,
            Syscall.ACCESS, Syscall.FACCESSAT,
            Syscall.GETDENTS,  # Directory listing
            Syscall.GETCWD,
            Syscall.CHDIR,
            Syscall.UNLINK, Syscall.UNLINKAT,
            Syscall.MKDIR, Syscall.MKDIRAT,
            Syscall.DUP, Syscall.DUP2, Syscall.DUP3,  # File descriptor duplication
            Syscall.FCNTL,  # File descriptor control
            # Note: We let execve through so apps can actually run
        }

        # Stats
        self.syscalls_emulated = 0
        self.syscalls_passed = 0

    def log(self, msg: str):
        if self.verbose:
            print(f"[SemOS] {msg}")

    def run(self, args: List[str], cwd: str = None) -> int:
        """Run a command in the emulated environment."""
        cwd = cwd or os.getcwd()

        # Set current process for semantic tracking
        self.fs.set_current_process(' '.join(args))

        self.log(f"Starting: {' '.join(args)}")
        self.log(f"Filesystem has {len(self.fs.files)} files")

        pid = os.fork()

        if pid == 0:
            # Child process
            try:
                ptrace(PTRACE_TRACEME, 0)
                os.kill(os.getpid(), signal.SIGSTOP)
                os.chdir(cwd)
                os.execvp(args[0], args)
            except Exception as e:
                print(f"Exec failed: {e}", file=sys.stderr)
                os._exit(1)
        else:
            return self._trace_and_emulate(pid, cwd)

    def _trace_and_emulate(self, pid: int, cwd: str) -> int:
        """Main emulation loop."""
        # Wait for initial stop
        os.waitpid(pid, 0)

        # Set options
        options = (PTRACE_O_TRACESYSGOOD | PTRACE_O_TRACEFORK |
                   PTRACE_O_TRACEVFORK | PTRACE_O_TRACECLONE | PTRACE_O_TRACEEXEC)
        ptrace(PTRACE_SETOPTIONS, pid, 0, options)

        # Create process entry
        self.processes[pid] = EmulatedProcess(
            pid=len(self.processes),
            real_pid=pid,
            cwd=cwd,
        )

        # Set up stdin/stdout/stderr (these pass through to real)
        proc = self.processes[pid]
        proc.fd_table[0] = FileHandle(fd=0, file=SemanticFile("/dev/stdin"), flags=0)
        proc.fd_table[1] = FileHandle(fd=1, file=SemanticFile("/dev/stdout"), flags=1)
        proc.fd_table[2] = FileHandle(fd=2, file=SemanticFile("/dev/stderr"), flags=1)

        ptrace(PTRACE_SYSCALL, pid)

        exit_code = 0
        running = True

        while running and self.processes:
            try:
                wpid, status = os.waitpid(-1, WALL)
            except ChildProcessError:
                break

            if wpid not in self.processes:
                # New child from fork - inherit parent's fd_table
                parent_proc = self.processes.get(pid)
                self.processes[wpid] = EmulatedProcess(
                    pid=len(self.processes),
                    real_pid=wpid,
                    cwd=parent_proc.cwd if parent_proc else "/",
                    fd_table={fd: FileHandle(fd=fd, file=h.file, flags=h.flags, position=h.position)
                              for fd, h in (parent_proc.fd_table.items() if parent_proc else {})},
                    next_fd=parent_proc.next_fd if parent_proc else 3,
                )

            proc = self.processes[wpid]

            if os.WIFEXITED(status):
                exit_code = os.WEXITSTATUS(status)
                del self.processes[wpid]
                if wpid == pid:
                    running = False
                continue

            if os.WIFSIGNALED(status):
                del self.processes[wpid]
                if wpid == pid:
                    running = False
                continue

            if os.WIFSTOPPED(status):
                sig = os.WSTOPSIG(status)

                if sig == (signal.SIGTRAP | 0x80):
                    # Syscall stop
                    self._handle_syscall(proc)
                elif sig != signal.SIGTRAP:
                    # Other signal, deliver it
                    ptrace(PTRACE_SYSCALL, wpid, 0, sig)
                    continue

            try:
                ptrace(PTRACE_SYSCALL, wpid)
            except OSError:
                pass

        self.log(f"Emulated: {self.syscalls_emulated}, Passed: {self.syscalls_passed}")
        return exit_code

    def _handle_syscall(self, proc: EmulatedProcess):
        """Handle syscall entry or exit."""
        regs = UserRegsStruct()
        ptrace(PTRACE_GETREGS, proc.real_pid, 0, ctypes.byref(regs))

        syscall_num = regs.orig_rax

        if proc.pending_syscall is None:
            # ENTRY - decide whether to emulate
            args = (regs.rdi, regs.rsi, regs.rdx, regs.r10, regs.r8, regs.r9)
            self._handle_syscall_entry(proc, syscall_num, args, regs)
        else:
            # EXIT - set our return value if we emulated
            self._handle_syscall_exit(proc, regs)

    def _handle_syscall_entry(self, proc: EmulatedProcess, syscall_num: int,
                               args: Tuple, regs: UserRegsStruct):
        """Handle syscall entry - decide whether to emulate."""

        # Check if we should emulate this syscall
        should_emulate = syscall_num in self.emulated_syscalls

        # For fd-based syscalls, check if fd is one we manage
        if should_emulate and syscall_num in (Syscall.READ, Syscall.WRITE, Syscall.CLOSE, Syscall.FSTAT):
            fd = args[0]
            # Emulate if fd is in our table (even if it's 0,1,2 after dup2)
            # Otherwise don't emulate
            if fd not in proc.fd_table:
                should_emulate = False
            # Special case: stdin/stdout/stderr that haven't been redirected
            # These are in fd_table but point to real streams - check if they're still original
            elif fd < 3 and proc.fd_table[fd].file.path.startswith("/dev/"):
                should_emulate = False

        # For DUP/FCNTL syscalls, only emulate if source fd is one we manage
        if should_emulate and syscall_num in (Syscall.DUP, Syscall.DUP2, Syscall.DUP3, Syscall.FCNTL):
            fd = args[0]
            if fd not in proc.fd_table:
                should_emulate = False

        # Special case: only emulate file ops for paths in our filesystem
        if should_emulate and syscall_num in (Syscall.OPEN, Syscall.OPENAT):
            if syscall_num == Syscall.OPEN:
                path = read_string(proc.real_pid, args[0])
            else:
                path = read_string(proc.real_pid, args[1])

            norm_path = self.fs.normalize_path(path, proc.cwd)

            # System paths we should NEVER emulate (needed for library loading, etc.)
            system_passthrough = (
                norm_path.startswith("/lib") or
                norm_path.startswith("/usr/lib") or
                norm_path.startswith("/usr/share") or
                norm_path.startswith("/proc") or
                norm_path.startswith("/sys") or
                norm_path.startswith("/dev") or
                norm_path == "/etc/ld.so.cache" or
                norm_path == "/etc/ld.so.preload" or
                norm_path.startswith("/etc/ld.so") or
                ".so" in norm_path  # Any shared library
            )

            # Emulate ONLY if path is in our filesystem (not system passthrough)
            if system_passthrough:
                should_emulate = False
            else:
                # Emulate if file exists OR if it's in a directory we control
                # (allowing file creation in /tmp, /home, etc.)
                O_CREAT = 0o100
                open_flags = args[2] if syscall_num == Syscall.OPENAT else args[1]
                is_create = (open_flags & O_CREAT) != 0
                in_controlled_dir = (
                    norm_path.startswith("/tmp") or
                    norm_path.startswith("/home")
                )
                should_emulate = self.fs.exists(norm_path) or (is_create and in_controlled_dir)

        # Don't emulate CHDIR/GETCWD unless we're fully in semantic space
        if should_emulate and syscall_num in (Syscall.CHDIR, Syscall.GETCWD):
            # Let real kernel handle these for now
            should_emulate = False

        # Don't emulate stat for paths outside our filesystem
        if should_emulate and syscall_num in (Syscall.STAT, Syscall.LSTAT, Syscall.ACCESS):
            path = read_string(proc.real_pid, args[0])
            path = self.fs.normalize_path(path, proc.cwd)
            should_emulate = self.fs.exists(path)

        if should_emulate:
            # Emulate this syscall
            result = self._emulate_syscall(proc, syscall_num, args)

            # Replace syscall with dummy (getpid)
            regs.orig_rax = DUMMY_SYSCALL
            ptrace(PTRACE_SETREGS, proc.real_pid, 0, ctypes.byref(regs))

            proc.pending_syscall = syscall_num
            proc.pending_result = result
            proc.pending_args = args

            self.syscalls_emulated += 1
        else:
            # Let it pass through to real kernel
            proc.pending_syscall = -1  # Mark as not emulated
            self.syscalls_passed += 1

    def _handle_syscall_exit(self, proc: EmulatedProcess, regs: UserRegsStruct):
        """Handle syscall exit - set return value if we emulated."""

        if proc.pending_syscall is not None and proc.pending_syscall >= 0:
            # We emulated this, set our return value
            result = proc.pending_result if proc.pending_result is not None else 0

            # Handle negative results (errors)
            if result < 0:
                regs.rax = ctypes.c_ulonglong(result).value  # Convert to unsigned
            else:
                regs.rax = result

            ptrace(PTRACE_SETREGS, proc.real_pid, 0, ctypes.byref(regs))

            self._log_syscall(proc.pending_syscall, proc.pending_args, result)

        proc.pending_syscall = None
        proc.pending_result = None
        proc.pending_args = ()

    def _log_syscall(self, syscall_num: int, args: Tuple, result: int):
        """Log an emulated syscall."""
        try:
            name = Syscall(syscall_num).name
        except ValueError:
            name = f"syscall_{syscall_num}"

        if self.verbose:
            print(f"  [EMU] {name} = {result}")

    def _emulate_syscall(self, proc: EmulatedProcess, syscall_num: int,
                         args: Tuple) -> int:
        """Emulate a syscall. Returns the result."""

        try:
            if syscall_num == Syscall.OPEN:
                return self._sys_open(proc, args[0], args[1], args[2])

            elif syscall_num == Syscall.OPENAT:
                return self._sys_openat(proc, args[0], args[1], args[2], args[3])

            elif syscall_num == Syscall.READ:
                return self._sys_read(proc, args[0], args[1], args[2])

            elif syscall_num == Syscall.WRITE:
                return self._sys_write(proc, args[0], args[1], args[2])

            elif syscall_num == Syscall.CLOSE:
                return self._sys_close(proc, args[0])

            elif syscall_num in (Syscall.STAT, Syscall.LSTAT):
                return self._sys_stat(proc, args[0], args[1])

            elif syscall_num == Syscall.FSTAT:
                return self._sys_fstat(proc, args[0], args[1])

            elif syscall_num == Syscall.ACCESS:
                return self._sys_access(proc, args[0], args[1])

            elif syscall_num == Syscall.GETCWD:
                return self._sys_getcwd(proc, args[0], args[1])

            elif syscall_num == Syscall.CHDIR:
                return self._sys_chdir(proc, args[0])

            elif syscall_num == Syscall.MKDIR:
                return self._sys_mkdir(proc, args[0], args[1])

            elif syscall_num == Syscall.UNLINK:
                return self._sys_unlink(proc, args[0])

            elif syscall_num == Syscall.DUP:
                return self._sys_dup(proc, args[0])

            elif syscall_num == Syscall.DUP2:
                return self._sys_dup2(proc, args[0], args[1])

            elif syscall_num == Syscall.DUP3:
                return self._sys_dup2(proc, args[0], args[1])  # flags ignored for now

            elif syscall_num == Syscall.FCNTL:
                return self._sys_fcntl(proc, args[0], args[1], args[2])

            else:
                # Unknown syscall we said we'd emulate
                self.log(f"WARNING: Unhandled emulated syscall {syscall_num}")
                return -38  # ENOSYS

        except Exception as e:
            self.log(f"ERROR emulating syscall {syscall_num}: {e}")
            return -1  # EPERM

    # ==================== SYSCALL IMPLEMENTATIONS ====================

    def _sys_open(self, proc: EmulatedProcess, path_addr: int, flags: int, mode: int) -> int:
        """Emulate open()"""
        path = read_string(proc.real_pid, path_addr)
        path = self.fs.normalize_path(path, proc.cwd)

        return self._do_open(proc, path, flags, mode)

    def _sys_openat(self, proc: EmulatedProcess, dirfd: int, path_addr: int,
                    flags: int, mode: int) -> int:
        """Emulate openat()"""
        path = read_string(proc.real_pid, path_addr)

        # AT_FDCWD = -100
        if dirfd == 0xFFFFFF9C or dirfd == -100:
            path = self.fs.normalize_path(path, proc.cwd)
        elif dirfd in proc.fd_table:
            base = proc.fd_table[dirfd].file.path
            path = self.fs.normalize_path(path, base)
        else:
            path = self.fs.normalize_path(path, proc.cwd)

        return self._do_open(proc, path, flags, mode)

    def _do_open(self, proc: EmulatedProcess, path: str, flags: int, mode: int) -> int:
        """Actually open a file."""
        O_CREAT = 0o100
        O_TRUNC = 0o1000
        O_APPEND = 0o2000

        f = self.fs.get(path)

        if f is None:
            if flags & O_CREAT:
                f = self.fs.create(path, b"", mode)
                self.log(f"Created: {path}")
            else:
                return -2  # ENOENT

        if f.is_dir:
            return -21  # EISDIR (can't open directory for writing usually)

        if flags & O_TRUNC:
            f.content = b""

        # Allocate fd
        fd = proc.next_fd
        proc.next_fd += 1

        pos = len(f.content) if (flags & O_APPEND) else 0
        proc.fd_table[fd] = FileHandle(fd=fd, file=f, flags=flags, position=pos)

        # Record semantic access
        self.fs.record_file_access(path, 'open')

        self.log(f"Open: {path} -> fd={fd}")
        return fd

    def _sys_read(self, proc: EmulatedProcess, fd: int, buf_addr: int, count: int) -> int:
        """Emulate read()"""
        # stdin/stdout/stderr pass through
        if fd < 3:
            return -1  # Let it fail, real syscall will handle

        if fd not in proc.fd_table:
            return -9  # EBADF

        handle = proc.fd_table[fd]
        content = handle.file.content

        # Read from current position
        data = content[handle.position:handle.position + count]
        handle.position += len(data)

        if data:
            # Write data to process memory
            self._write_to_process(proc.real_pid, buf_addr, data)

        # Record semantic access
        self.fs.record_file_access(handle.file.path, 'read', len(data))

        self.log(f"Read: fd={fd} -> {len(data)} bytes")
        return len(data)

    def _sys_write(self, proc: EmulatedProcess, fd: int, buf_addr: int, count: int) -> int:
        """Emulate write()"""
        if fd not in proc.fd_table:
            return -9  # EBADF

        # If it's original stdin/stdout/stderr, don't emulate
        handle = proc.fd_table[fd]
        if handle.file.path.startswith("/dev/"):
            return -1  # Let real syscall handle it

        # Read data from process memory
        data = self._read_from_process(proc.real_pid, buf_addr, count)
        if not data and count > 0:
            self.log(f"WARNING: Could not read {count} bytes from process memory at 0x{buf_addr:x}")
            return -14  # EFAULT - bad address, so app knows to not retry

        # Write to file
        O_APPEND = 0o2000
        if handle.flags & O_APPEND:
            handle.file.content += data
        else:
            content = handle.file.content
            pos = handle.position
            handle.file.content = content[:pos] + data + content[pos+len(data):]
            handle.position += len(data)

        handle.file.modified_at = time.time()

        # Record semantic access
        self.fs.record_file_access(handle.file.path, 'write', len(data))

        self.log(f"Write: fd={fd} <- {len(data)} bytes")
        return len(data)

    def _sys_close(self, proc: EmulatedProcess, fd: int) -> int:
        """Emulate close()"""
        if fd < 3:
            return 0  # Pretend success for std streams

        if fd in proc.fd_table:
            del proc.fd_table[fd]
            self.log(f"Close: fd={fd}")
            return 0
        return -9  # EBADF

    def _sys_stat(self, proc: EmulatedProcess, path_addr: int, stat_buf: int) -> int:
        """Emulate stat()"""
        path = read_string(proc.real_pid, path_addr)
        path = self.fs.normalize_path(path, proc.cwd)

        f = self.fs.get(path)
        if f is None:
            return -2  # ENOENT

        # Write stat structure (simplified)
        self._write_stat(proc.real_pid, stat_buf, f)
        return 0

    def _sys_fstat(self, proc: EmulatedProcess, fd: int, stat_buf: int) -> int:
        """Emulate fstat()"""
        if fd < 3:
            return -1  # Let real syscall handle

        if fd not in proc.fd_table:
            return -9  # EBADF

        f = proc.fd_table[fd].file
        self._write_stat(proc.real_pid, stat_buf, f)
        return 0

    def _sys_access(self, proc: EmulatedProcess, path_addr: int, mode: int) -> int:
        """Emulate access()"""
        path = read_string(proc.real_pid, path_addr)
        path = self.fs.normalize_path(path, proc.cwd)

        if self.fs.exists(path):
            return 0
        return -2  # ENOENT

    def _sys_getcwd(self, proc: EmulatedProcess, buf_addr: int, size: int) -> int:
        """Emulate getcwd()"""
        cwd = proc.cwd.encode() + b'\x00'
        if len(cwd) > size:
            return -34  # ERANGE
        self._write_to_process(proc.real_pid, buf_addr, cwd)
        return buf_addr

    def _sys_chdir(self, proc: EmulatedProcess, path_addr: int) -> int:
        """Emulate chdir()"""
        path = read_string(proc.real_pid, path_addr)
        path = self.fs.normalize_path(path, proc.cwd)

        if self.fs.is_dir(path):
            proc.cwd = path
            self.log(f"Chdir: {path}")
            return 0
        return -2  # ENOENT

    def _sys_mkdir(self, proc: EmulatedProcess, path_addr: int, mode: int) -> int:
        """Emulate mkdir()"""
        path = read_string(proc.real_pid, path_addr)
        path = self.fs.normalize_path(path, proc.cwd)

        if self.fs.exists(path):
            return -17  # EEXIST

        self.fs.mkdir(path, mode)
        self.log(f"Mkdir: {path}")
        return 0

    def _sys_unlink(self, proc: EmulatedProcess, path_addr: int) -> int:
        """Emulate unlink()"""
        path = read_string(proc.real_pid, path_addr)
        path = self.fs.normalize_path(path, proc.cwd)

        if self.fs.unlink(path):
            self.log(f"Unlink: {path}")
            return 0
        return -2  # ENOENT

    def _sys_dup(self, proc: EmulatedProcess, old_fd: int) -> int:
        """Emulate dup()"""
        if old_fd not in proc.fd_table:
            return -9  # EBADF

        # Find lowest available fd
        new_fd = proc.next_fd
        proc.next_fd += 1

        # Copy the file handle
        old_handle = proc.fd_table[old_fd]
        proc.fd_table[new_fd] = FileHandle(
            fd=new_fd,
            file=old_handle.file,
            flags=old_handle.flags,
            position=old_handle.position
        )

        self.log(f"Dup: fd={old_fd} -> fd={new_fd}")
        return new_fd

    def _sys_dup2(self, proc: EmulatedProcess, old_fd: int, new_fd: int) -> int:
        """Emulate dup2()"""
        if old_fd not in proc.fd_table:
            return -9  # EBADF

        if old_fd == new_fd:
            return new_fd

        # Close new_fd if it exists
        if new_fd in proc.fd_table:
            del proc.fd_table[new_fd]

        # Copy the file handle
        old_handle = proc.fd_table[old_fd]
        proc.fd_table[new_fd] = FileHandle(
            fd=new_fd,
            file=old_handle.file,
            flags=old_handle.flags,
            position=old_handle.position
        )

        self.log(f"Dup2: fd={old_fd} -> fd={new_fd}")
        return new_fd

    def _sys_fcntl(self, proc: EmulatedProcess, fd: int, cmd: int, arg: int) -> int:
        """Emulate fcntl()"""
        # Common fcntl commands
        F_DUPFD = 0
        F_GETFD = 1
        F_SETFD = 2
        F_GETFL = 3
        F_SETFL = 4

        if fd not in proc.fd_table:
            return -9  # EBADF

        handle = proc.fd_table[fd]

        if cmd == F_DUPFD:
            # Duplicate fd to lowest available >= arg
            new_fd = max(arg, proc.next_fd)
            proc.next_fd = new_fd + 1
            proc.fd_table[new_fd] = FileHandle(
                fd=new_fd, file=handle.file, flags=handle.flags, position=handle.position
            )
            return new_fd
        elif cmd == F_GETFD:
            return 0  # FD_CLOEXEC not set
        elif cmd == F_SETFD:
            return 0  # Ignore cloexec flag
        elif cmd == F_GETFL:
            return handle.flags
        elif cmd == F_SETFL:
            handle.flags = arg
            return 0
        else:
            # Unknown command, return success
            return 0

    # ==================== MEMORY HELPERS ====================

    def _read_from_process(self, pid: int, addr: int, size: int) -> bytes:
        """Read bytes from process memory."""
        if size == 0:
            return b""

        # Try using /proc/<pid>/mem first (more reliable)
        try:
            with open(f"/proc/{pid}/mem", "rb") as f:
                f.seek(addr)
                return f.read(size)
        except (OSError, IOError):
            pass

        # Fallback to ptrace PEEKDATA
        result = []
        for i in range(0, size, 8):
            try:
                word = ptrace(3, pid, addr + i)  # PTRACE_PEEKDATA
                word_bytes = struct.pack("Q", word & 0xFFFFFFFFFFFFFFFF)
                result.extend(word_bytes[:min(8, size - i)])
            except OSError:
                break
        return bytes(result[:size])

    def _write_to_process(self, pid: int, addr: int, data: bytes):
        """Write bytes to process memory."""
        if not data:
            return

        # Try using /proc/<pid>/mem first (more reliable)
        try:
            with open(f"/proc/{pid}/mem", "r+b") as f:
                f.seek(addr)
                f.write(data)
                return
        except (OSError, IOError):
            pass

        # Fallback to ptrace POKEDATA
        padded = data + b'\x00' * (8 - len(data) % 8) if len(data) % 8 else data

        for i in range(0, len(padded), 8):
            word = struct.unpack("Q", padded[i:i+8])[0]
            try:
                ptrace(5, pid, addr + i, word)  # PTRACE_POKEDATA
            except OSError:
                break

    def _write_stat(self, pid: int, addr: int, f: SemanticFile):
        """Write a stat structure to process memory."""
        # struct stat on x86_64 Linux (144 bytes)
        mode = (0o40755 if f.is_dir else 0o100644)  # S_IFDIR or S_IFREG + perms
        size = len(f.content) if not f.is_dir else 4096
        now = int(f.modified_at)

        # Layout: dev(8) ino(8) nlink(8) mode(4) uid(4) gid(4) pad(4)
        #         rdev(8) size(8) blksize(8) blocks(8)
        #         atime_sec(8) atime_nsec(8) mtime_sec(8) mtime_nsec(8)
        #         ctime_sec(8) ctime_nsec(8) unused[3](24)
        stat_data = struct.pack(
            "QQQIIIiQqqqqqqqqqQQQ",
            1,              # st_dev
            1,              # st_ino
            1,              # st_nlink
            mode,           # st_mode
            1000,           # st_uid
            1000,           # st_gid
            0,              # __pad0
            0,              # st_rdev
            size,           # st_size
            4096,           # st_blksize
            (size + 511) // 512,  # st_blocks
            now, 0,         # st_atim (sec, nsec)
            now, 0,         # st_mtim
            now, 0,         # st_ctim
            0, 0, 0,        # __unused[3]
        )

        self._write_to_process(pid, addr, stat_data)


def main():
    """Test the emulator."""
    print("=" * 60)
    print("  SEMANTIC OS - Syscall Emulator")
    print("=" * 60)

    emulator = SyscallEmulator(verbose=True)

    # Test: cat a file from semantic filesystem
    print("\n--- Test 1: cat /etc/hostname ---")
    emulator.run(["cat", "/etc/hostname"])

    print("\n--- Test 2: cat /etc/os-release ---")
    emulator.run(["cat", "/etc/os-release"])

    print("\n--- Test 3: cat /home/user/hello.txt ---")
    emulator.run(["cat", "/home/user/hello.txt"])

    print("\n" + "=" * 60)
    print("  Files in semantic memory:")
    for path in sorted(emulator.fs.files.keys()):
        f = emulator.fs.files[path]
        if f.is_dir:
            print(f"    [DIR]  {path}")
        else:
            print(f"    [FILE] {path} ({len(f.content)} bytes)")


if __name__ == "__main__":
    main()
