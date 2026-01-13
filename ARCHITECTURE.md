# Semantic OS Architecture Guide

## Overview

Semantic OS is an experimental operating system where **everything is semantic memory**. Instead of traditional file systems and process isolation, all data exists as vectors in a unified semantic pool. This enables revolutionary features like natural language queries for system state, automatic relationship discovery between files, and semantic search across all system operations.

## Core Architecture

```
+------------------+     +------------------+     +------------------+
|   Applications   |     |   Applications   |     |   Applications   |
|   (cat, echo,    |     |   (Python,       |     |   (Custom        |
|    shell, etc.)  |     |    Node, etc.)   |     |    apps)         |
+--------+---------+     +--------+---------+     +--------+---------+
         |                        |                        |
         v                        v                        v
+------------------------------------------------------------------------+
|                     SYSCALL EMULATION LAYER                            |
|  - Intercepts syscalls via ptrace                                      |
|  - Routes file ops to SemanticFilesystem                               |
|  - Handles process management (fork, exec)                             |
+------------------------------------------------------------------------+
         |
         v
+------------------------------------------------------------------------+
|                     SEMANTIC FILESYSTEM                                |
|  - Virtual files stored as SemanticFile objects                        |
|  - File content + metadata + embeddings                                |
|  - Path normalization and directory structure                          |
+------------------------------------------------------------------------+
         |
         v
+------------------------------------------------------------------------+
|                     SEMANTIC MEMORY POOL                               |
|  - SQLite + vector embeddings                                          |
|  - Documents, relations, history                                       |
|  - Natural language queryable                                          |
+------------------------------------------------------------------------+
```

## Key Components

### 1. Syscall Emulator (`kernel/syscall_emulator.py`)

The heart of Semantic OS. Uses Linux ptrace to intercept syscalls and handle them in userspace.

**How it works:**
1. Fork a child process
2. Child calls `PTRACE_TRACEME` and execs the target program
3. Parent intercepts syscalls at entry via `PTRACE_SYSCALL`
4. For emulated syscalls: replace with dummy (getpid), handle ourselves
5. On exit: set our return value in the registers

**Emulated syscalls:**
- `open/openat` - Route to SemanticFilesystem
- `read/write` - Read/write semantic file content
- `close` - Close our virtual file descriptors
- `stat/fstat/lstat` - Return stat info for semantic files
- `dup/dup2/dup3` - Duplicate file descriptors (critical for shell redirects)
- `fcntl` - File descriptor control
- `mkdir/unlink` - Create/delete in semantic filesystem

**Key technique - Syscall replacement:**
```python
# On syscall entry, if we want to emulate:
regs.orig_rax = DUMMY_SYSCALL  # Replace with getpid
ptrace(PTRACE_SETREGS, pid, regs)
proc.pending_result = our_result

# On syscall exit:
regs.rax = proc.pending_result  # Set our return value
ptrace(PTRACE_SETREGS, pid, regs)
```

### 2. Semantic Filesystem (`SemanticFilesystem` class)

Virtual filesystem where files are Python objects in memory.

```python
@dataclass
class SemanticFile:
    path: str
    content: bytes = b""
    mode: int = 0o644
    is_dir: bool = False
    created_at: float
    modified_at: float
    metadata: Dict[str, Any]  # Future: embeddings, relations
```

**Features:**
- Path normalization (resolves `.`, `..`, relative paths)
- Directory structure with `list_dir()`
- File CRUD operations
- Prepared for semantic extensions (metadata field)

### 3. Semantic Memory (`kernel/memory.py`)

Persistent storage with vector embeddings for semantic search.

```python
class SemanticMemory:
    def store_document(self, doc: Document)  # Store with embedding
    def query_similar(self, query: str, k: int)  # Semantic search
    def save_relation(self, subject, predicate, object)  # Knowledge graph
```

### 4. Linux Tracer (`kernel/linux_tracer.py`)

Low-level ptrace interface for syscall interception.

**Key structures:**
- `UserRegsStruct` - x86_64 register layout
- `Syscall` enum - Syscall numbers
- `read_string()` - Read strings from tracee memory via `/proc/pid/mem`

## Data Flow Example

When `cat /etc/hostname` runs:

1. **Program starts**: Fork, ptrace setup, exec `cat`
2. **openat("/etc/hostname")** intercepted:
   - Read path from process memory
   - Check SemanticFilesystem: file exists
   - Create FileHandle, assign fd=3
   - Return fd=3 to program
3. **fstat(3)** intercepted:
   - Look up fd=3 in our fd_table
   - Write stat structure to process memory
   - Return 0
4. **read(3, buf, size)** intercepted:
   - Read from SemanticFile.content
   - Write to process memory at buf
   - Update file position
   - Return bytes read
5. **write(1, buf, size)** passes through:
   - fd=1 is real stdout, not ours
   - Let real kernel handle it
   - Content appears on terminal
6. **close(3)** intercepted:
   - Remove from fd_table
   - Return 0

## Semantic Features (Implemented)

### 1. File Access Tracking
Every file operation is recorded with timestamp, operation type, and process name.

```python
# Get file statistics
stats = emulator.fs.get_file_stats("/etc/hostname")
# Returns: access_count, last_operation, last_accessed, related_files, tags, etc.

# Get recently accessed files
recent = emulator.fs.get_recent_files(limit=10)
```

### 2. Automatic Relationship Discovery
Files accessed together within a 5-second window are automatically linked as related.

```python
# After reading /etc/hostname and /etc/os-release together:
related = emulator.fs.get_related_files("/etc/hostname")
# Returns: ['/etc/os-release']
```

### 3. Auto-Tagging
Files are automatically tagged based on content and path patterns.

```python
tags = emulator.fs.auto_tag_file("/etc/passwd")
# Returns: ['script', 'system']

# Search by tag
system_files = emulator.fs.search_by_tag("system")
```

Built-in tag categories:
- `config`: Configuration files
- `log`: Log files with errors/warnings
- `script`: Shell scripts
- `documentation`: README, docs
- `data`: CSV, data files
- `system`: /etc/, /usr/, /var/ paths

### 4. Semantic Search (requires sentence-transformers)
Natural language search across file contents using vector embeddings.

```python
# Generate embeddings for all files
emulator.fs.generate_all_embeddings()

# Search semantically
results = emulator.fs.semantic_search("server configuration", top_k=5)
# Returns: [(path, similarity_score), ...]
```

### 5. Manual Tagging
```python
emulator.fs.add_tag("/path/to/file", "important")
```

## Future Directions

### Path to Bootable OS

The syscall handlers we've written ARE the kernel logic. To create a bootable OS:

1. **Current**: Userspace emulator on Linux
2. **Next**: Minimal Linux kernel module that routes syscalls to our handlers
3. **Future**: Custom kernel with semantic memory as the only storage

### Semantic Features to Add

1. **File embeddings**: Store vector embeddings for each file
2. **Automatic tagging**: AI analyzes file content, adds semantic tags
3. **Relationship discovery**: Track which files are accessed together
4. **Natural language queries**: "Find all config files modified yesterday"
5. **Semantic deduplication**: Detect similar files across the system

### Process Semantics

Currently we only track file operations. Future enhancements:

1. **Process memory as semantic data**: What's in memory becomes queryable
2. **Inter-process communication logging**: Track all IPC semantically
3. **Syscall patterns as signatures**: Learn what "normal" looks like

## Code Organization

```
kernel/
  syscall_emulator.py  - Main emulation layer (THIS IS THE OS)
  linux_tracer.py      - ptrace interface, syscall definitions
  memory.py            - Semantic memory pool with SQLite
  core.py              - Kernel state, relation graph

semantic_linux.py      - Interactive shell for Semantic OS
semantic_explorer.py   - PyQt file explorer (Windows)
semantic_process.py    - Windows process wrapper

wsl_setup.sh          - Setup script for WSL
```

## Dependencies

**Required:**
- Python 3.8+
- Linux/WSL (for ptrace syscall interception)

**Optional:**
- `sentence-transformers`: For semantic search with embeddings
  ```bash
  pip install sentence-transformers
  ```
- `numpy`: For embedding similarity calculations (installed with sentence-transformers)

## Running Semantic OS

```bash
# In WSL:
cd /mnt/c/path/to/markdown_viewer
python3 -m kernel.syscall_emulator

# Or run specific commands:
python3 -c "
from kernel.syscall_emulator import SyscallEmulator
emu = SyscallEmulator()
emu.run(['cat', '/etc/hostname'])
"
```

## Testing

The emulator passes these tests:
- Read files from semantic filesystem
- Write files via shell redirection (`echo foo > file`)
- File append (`echo bar >> file`)
- Complex shell commands with dup2 for redirects
- Multiple processes (fork handling)

## Known Limitations

1. **Performance**: ptrace has overhead; every syscall traps to parent
2. **Compatibility**: Not all syscalls emulated; complex apps may fail
3. **Networking**: Not yet emulated (passes through to real kernel)
4. **Threads**: Basic support via PTRACE_O_TRACECLONE, needs more work

## Contributing

Key areas needing work:
1. More syscall implementations (especially for network, threads)
2. Semantic features (embeddings, AI analysis)
3. Performance optimization
4. Testing with more real-world applications
