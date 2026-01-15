# Semantic OS - Architecture Documentation

## Overview

**Semantic OS** is an experimental operating system environment where **everything is semantic memory**. The project has two main layers:

1. **System Layer** (Linux/WSL): Syscall interception and semantic filesystem
2. **Application Layer** (Cross-platform): Context-aware IDE with AI integration

Instead of traditional file systems and process isolation, all data exists as vectors in a unified semantic pool. This enables revolutionary features like natural language queries for system state, automatic relationship discovery between files, semantic search across all system operations, and AI assistants that understand your complete workflow context.

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

---

# Application Layer - Context-Aware IDE

## Overview

The Application Layer is a cross-platform, context-aware development environment that tracks your work, understands your projects, and provides AI assistants with complete context. It runs on Windows, macOS, and Linux.

## What Problem Does This Solve?

Traditional development tools treat files as isolated entities. They don't know that:
- The README.md you're editing is related to the 3 Python files you just modified
- You've been working on a "feature-X" project for 2 hours across 12 files
- The research you did yesterday is relevant to the code you're writing today
- You have a workflow pattern of: edit code → test → document → repeat

**The Solution**: Semantic OS Application Layer tracks, understands, and aggregates context.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    Application Layer                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────────┐      ┌──────────────────┐                 │
│  │  Markdown Viewer │      │   Browser Tabs   │                 │
│  │  - Syntax High   │      │  - Web Browsing  │                 │
│  │  - Live Preview  │      │  - Link Nav      │                 │
│  │  - Task Extract  │      │  - Context Track │                 │
│  └────────┬─────────┘      └────────┬─────────┘                 │
│           │                          │                            │
│           └──────────┬───────────────┘                            │
│                      ▼                                            │
│           ┌──────────────────────┐                               │
│           │   Context Layer      │                               │
│           │  - Focus Detection   │                               │
│           │  - Workflow Infer    │                               │
│           │  - Project Detection │                               │
│           │  - Context Export    │                               │
│           └──────────┬───────────┘                               │
│                      │                                            │
│                      ▼                                            │
│           ┌──────────────────────┐                               │
│           │   Semantic Kernel    │                               │
│           │  - Memory Store      │                               │
│           │  - Relation Graph    │                               │
│           │  - Event Stream      │                               │
│           │  - Actions System    │                               │
│           └──────────┬───────────┘                               │
│                      │                                            │
│         ┌────────────┼────────────┐                               │
│         ▼            ▼            ▼                                │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐                      │
│  │ Vectors  │ │ Graph DB │ │  File System │                      │
│  │ (Chroma) │ │ (SQLite) │ │  (Watching)  │                      │
│  └──────────┘ └──────────┘ └──────────────┘                      │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Provider System (Extensible)                │   │
│  │  - TaskManager - LinkProvider - ContextInsights          │   │
│  │  - AIProvider - ScreenMemory - PythonInterpreter         │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Semantic Kernel (`kernel/core.py`)

The brain of the system. Coordinates all semantic operations.

**Features:**
- **Memory Storage**: Vector database (ChromaDB) + Graph database (SQLite)
- **Event Tracking**: All user actions logged as events
- **Action System**: Extensible command execution
- **Provider Management**: Plugin-like architecture
- **Signal Emission**: PyQt events for real-time updates

```python
kernel = SemanticKernel()

# Store semantic memory
kernel.memory.store(
    content="Function that authenticates users",
    metadata={"type": "function", "file": "auth.py"}
)

# Query by semantic similarity
results = kernel.memory.query("user login")

# Track events
kernel.emit("file.edit", {"path": "auth.py", "lines": 10})

# Execute actions
kernel.execute_action("refactor", {"target": "auth.py"})
```

### 2. Context Layer (`ide/context_layer.py`)

Understands what you're working on.

#### FocusDetection
Tracks document attention:
- How long you've focused on each file
- Edit counts and recency
- Related documents

```python
# Get current focus
focus_docs = kernel.get_current_focus(limit=10)
# [
#   DocumentFocus(path="auth.py", focus_duration=45.2, edit_count=23),
#   DocumentFocus(path="login.html", focus_duration=12.0, edit_count=5)
# ]
```

#### WorkflowInference
Detects work patterns:
- Groups events into sessions (>30min gap = new session)
- Infers work type (coding, writing, research, debugging)
- Identifies projects from file co-access patterns

```python
workflow = kernel.get_workflow_context(time_window="today")
# WorkflowContext(
#     current_focus=[...],
#     recent_work=[
#         WorkItem(work_type="coding", files_count=5, edit_count=32)
#     ],
#     active_projects=[
#         Project(name="Authentication", core_files=["auth.py", ...])
#     ]
# )
```

#### ContextExporter
Exports context for AI assistants in multiple formats:

**Markdown Format** (for Claude, ChatGPT):
```markdown
## User's Current Context (2026-01-14 14:30)

### Current Focus
The user is actively working on:
- **auth.py** (45min focus, 23 edits)
- **login.html** (12min focus, 5 edits)

### Recent Work (Last 24h)
14:15 - Coding (5 files, 32 edits)
13:30 - Writing (2 files, 8 edits)

### Active Projects
1. **Authentication System** (PRIMARY)
   - Core: auth.py, login.html, user_model.py
```

**JSON Format** (programmatic access):
```json
{
  "current_focus": [
    {"path": "auth.py", "focus_duration": 45.2, "edit_count": 23}
  ],
  "active_projects": [
    {"name": "Authentication", "core_files": ["auth.py"]}
  ]
}
```

### 3. Screen Memory (`ide/screen_memory.py`)

Episodic visual memory of your work.

Periodically captures screenshots with:
- **Change Detection**: Only captures when screen changes significantly (>5% pixels)
- **OCR Text Extraction**: Extracts text from screenshots
- **AI Summarization**: Vision models summarize content
- **Privacy Protection**: Blacklists passwords, secrets

```python
visual_memory = VisualMemoryCapture(kernel)
visual_memory.start()

# Query visual memory
screenshots = visual_memory.query("authentication error")
```

**Use Cases:**
- "What was that error I saw 2 hours ago?"
- "Find the screenshot where I was debugging the login form"
- "Show me what I was working on yesterday morning"

### 4. Provider System (`ide/providers.py`)

Extensible command and feature system.

Providers add functionality through:
- Console commands (`/focus`, `/export_context`)
- UI panels (semantic panel, task panel)
- Event handlers
- Background services

**Built-in Providers:**

| Provider | Description |
|----------|-------------|
| `TaskManagerProvider` | Task tracking and management |
| `LinkProvider` | Semantic link navigation |
| `ContextInsightsProvider` | Context visualization and export |
| `AIProvider` | AI-powered code assistance |
| `ScreenMemoryProvider` | Visual memory management |
| `PythonInterpreterProvider` | Python REPL integration |

### 5. Markdown Viewer (`viewer.py`)

The main UI application.

**Features:**
- Markdown editing with syntax highlighting
- Live HTML preview
- Task extraction and tracking
- Semantic panel with related documents, tags, current focus
- Browser tabs with semantic tracking
- Dark mode support
- Find/replace with regex

## Browser Implementation

### Web Engine: QtWebEngine (Chromium-based)

The browser uses `QWebEngineView` from `PyQt6-WebEngine`, which embeds the **Chromium** browser engine.

**This is NOT:**
- ❌ Firefox (Gecko)
- ❌ Chrome/Edge installation
- ❌ System web browser

**This IS:**
- ✅ Chromium engine embedded directly in the app
- ✅ Same rendering as Chrome
- ✅ No external dependencies
- ✅ Cross-platform
- ✅ Full web standards support

**Installation:**
```bash
pip install PyQt6-WebEngine
```

### Semantic Web Tracking

The browser tracks:
- URLs visited
- Page titles
- Navigation history
- Time spent on each page
- Links between web pages and files

This web activity is integrated with the context layer, so research done in the browser is correlated with code written in files.

## Key Features

### 1. Automatic Project Detection

**Problem**: Manually organizing files into projects is tedious.

**Solution**: The system automatically detects projects from your work patterns:
```python
projects = kernel.get_workflow_context().active_projects
# [Project(name="Authentication System", core_files=[...])]
```

### 2. Semantic Search

**Problem**: Finding files by keyword doesn't capture intent.

**Solution**: Search by meaning:
```python
results = kernel.memory.query("where is the user login code?")
# Returns: auth.py:45-89 (92% similar)
```

### 3. Workflow Review

**Problem**: "What did I work on yesterday?"

**Solution**:
```python
workflow = kernel.get_workflow_context(time_window="this_week")
# Shows sessions, patterns, projects, timeline
```

### 4. Context-Aware AI Assistance

**Problem**: AI assistants don't know what you're working on.

**Solution**:
```python
# Export current context
context = kernel.export_context(format="markdown")

# Send to Claude
send_to_claude(f"{context}\n\n{my_question}")
```

Now Claude knows:
- What files you're actively editing
- What project you're working on
- What you did in the last 24 hours
- Related documents

### 5. Visual Memory

**Problem**: "I saw an error message earlier, what was it?"

**Solution**:
```python
screenshots = visual_memory.query("error", limit=10)
# Returns screenshots with OCR text, summaries
```

## File Organization

```
markdown_viewer/
├── kernel/
│   ├── core.py              # Semantic kernel (main)
│   ├── memory.py            # Memory storage
│   └── syscall_emulator.py  # System layer (Linux)
│
├── ide/
│   ├── context_layer.py     # Context aggregation
│   ├── screen_memory.py     # Visual memory capture
│   ├── providers.py         # Provider system
│   ├── manifest.py          # Document relations
│   └── tasks.py             # Task extraction
│
├── viewer.py                # Main application
├── semantic_explorer.py     # File explorer
└── ARCHITECTURE.md          # This file
```

## Technology Stack

| Component | Technology |
|-----------|-----------|
| UI Framework | PyQt6 |
| Web Browser | QtWebEngine (Chromium) |
| Vector Database | ChromaDB |
| Graph Database | SQLite (custom schema) |
| Embeddings | OpenAI / HuggingFace / Local |
| Markdown | Python Markdown |
| OCR | Tesseract (optional) |
| Vision AI | OpenAI Vision / Local (optional) |

## Use Cases

### 1. Developer Workflow Tracking
- Track what files you edit in a session
- Automatically group related files into projects
- Detect when you switch between projects
- Export context for AI code review

### 2. Research Integration
- Browser tabs track web research
- Correlate research with code changes
- Find web pages related to current file
- Visual memory of research sessions

### 3. Task Management
- Extract tasks from markdown files
- Track task completion
- Link tasks to implementation files
- Generate progress reports

### 4. Context-Aware AI
- Give AI assistants full context
- Include related files, recent work, focus
- Export in multiple formats (Markdown, JSON)
- Improve AI responses significantly

## Future Roadmap

### Phase 1: Core ✅ (Current)
- Semantic kernel with memory and events
- Context layer (focus, workflow, projects)
- Provider system
- Markdown viewer with semantic features
- Browser tabs (Chromium-based)
- Context export (Markdown/JSON)
- Screen memory capture (basic)

### Phase 2: Intelligence (Next)
- AI-powered code understanding
- Automatic refactoring suggestions
- Work pattern optimization
- Smart task prioritization
- Cross-document refactoring

### Phase 3: Autonomy (Future)
- Autonomous task execution
- Self-improving workflows
- Predictive file loading
- Automatic documentation generation
- Code review assistant

### Phase 4: Integration
- VS Code extension
- IntelliJ plugin
- Browser extension (Chrome/Firefox)
- CLI tools
- API server

## Running the Application

```bash
# Install dependencies
pip install PyQt6 PyQt6-WebEngine chromadb markdown

# Run the markdown viewer
python viewer.py

# Run semantic explorer
python semantic_explorer.py
```

## Philosophy

1. **Semantics Over Syntax**: Traditional tools care about file names. We care about meaning.
2. **Context is King**: Your AI assistant is only as good as its context.
3. **Privacy-First**: All data stored locally. You control what gets exported.
4. **Extensibility**: Everything is a provider. Add functionality without modifying core.
5. **Multi-Modal Intelligence**: Text + Code + Web + Visuals = Complete understanding
