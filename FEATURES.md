# MarlOS - Semantic Markdown Editor

A comprehensive markdown editor with integrated AI, semantic memory, relationship tracking, and 3D visualization capabilities.

---

## Core Features

### Markdown Editing & Viewing
- **Syntax Highlighting** - Full markdown syntax highlighting with Python and JavaScript code block support
- **Live Preview** - Toggle between Edit and View modes (F2)
- **Split View** - Side-by-side editing and preview
- **Dark Mode** - Toggle dark/light theme (Ctrl+D)
- **Search** - Find in document with previous/next navigation (Ctrl+F)
- **Multi-Tab Editing** - Work on multiple documents simultaneously
- **Reader Mode** - Distraction-free reading (Ctrl+R)

### File Management
- New file (Ctrl+N), Open (Ctrl+O), Save (Ctrl+S), Save As (Ctrl+Shift+S)
- Close tab (Ctrl+W)
- Print/PDF export (Ctrl+Shift+E)
- Recent files tracking
- File watching for external changes with auto-reload
- Reload document (F5)

---

## IDE Features

### Smart Import System
Intelligent file import with repository-aware parsing:

- **Repository Detection** - Automatically detects Python, Node, Rust, Java, Go, and generic repositories
- **Structure Analysis** - Identifies source, test, docs, and config directories
- **Dependency Parsing** - Extracts dependencies from package managers
- **Entry Point Detection** - Finds main files and scripts
- **Relationship Linking** - Links tests to source, docs to code

### Bulk Import
- Recursive directory scanning with depth control
- File type filtering by categories:
  - **Code** - .py, .js, .ts, .java, .c, .cpp, .go, .rs, .swift, etc.
  - **Web** - .html, .css, .vue, .svelte, etc.
  - **Data** - .json, .yaml, .xml, .csv, .toml, .ini
  - **Documentation** - .md, .rst, .txt, .pdf, .docx
  - **Scripts** - .sh, .bash, .bat, .ps1
  - **Images** - .png, .jpg, .gif, .webp, .bmp, .tiff
- File size guards and exclude patterns
- Progress tracking with statistics
- Vision model integration for image descriptions

### Command Palette
- Quick command access (Ctrl+Shift+P)
- Command registry for IDE operations
- Document mutation tracking

---

## Semantic Memory System

### Memory Pool
All application state stored as semantic vectors in a unified memory pool:

- **Entry Types** - Document, Chunk, Event, Handle, Process, Link, Snapshot, Query
- **Versioning** - Full version history for entries
- **Metadata** - Rich metadata attached to all entries
- **Time-Stamped** - Temporal queries and time-based filtering
- **Relationships** - Link entries with semantic relationship types
- **SQLite Storage** - Persistent, portable database

### Vector Embeddings
384-dimensional semantic vectors for similarity search:

- **MiniLM-L6-v2** - Fast, 80MB model (default)
- **mPNet** - Higher quality, 420MB
- **E5-Small** - Retrieval-optimized, 130MB
- **BGE-Small** - Quality-focused, 130MB
- **Hash-TF-IDF** - Zero-dependency fallback

### Semantic Search
- Search by meaning, not just keywords
- Cosine similarity matching
- Batch embedding operations

---

## 3D Memory Visualization

Interactive OpenGL-based visualization of the semantic memory space:

### Dimensionality Reduction
- **PCA** - Fast principal component analysis (384D to 3D)
- **t-SNE** - Slower but better clustering visualization

### Visualization Features
- Color-coded by entry type (documents, code files, images, etc.)
- Interactive 3D rotation, zoom, and pan
- Click to select and view entry details
- Relationship edges between linked entries
- Cluster detection with KMeans
- Search and highlight matching points
- Find similar points by cosine similarity
- Type filtering with checkboxes

### Time Dimension
- **Z-axis = Time** - Spread points vertically by creation date
- **Color by Time** - Gradient from blue (old) to red (new)
- **Time Range Filter** - Show only points within a time range

---

## AI Integration

### Local AI Providers
- **LM Studio** - OpenAI-compatible local inference
- **Ollama** - Local model serving

### Cloud Providers
- **OpenAI** - GPT models
- **Anthropic** - Claude models
- **Generic OpenAI-compatible** - Any compatible endpoint

### AI-Powered Features
- Intent mapping and document analysis
- Citation helper for academic writing
- Diff narrator for change descriptions
- Outline enhancement
- Action/task extraction
- Typo and grammar fixing
- Writing suggestions
- Code completion, explanation, editing, and generation

---

## Provider System

### Always-On Providers (Silent, Read-Only)
- **MarkdownLanguageService** - Syntax analysis and completions
- **MarkdownRenderer** - HTML rendering with extensions
- **OutlineProvider** - Document structure/TOC
- **WordCountProvider** - Real-time statistics
- **PreviewProvider** - Live preview rendering

### On-Demand Providers (Command-Activated)
- **FormattingProvider** - Code and text formatting
- **IntentMapProvider** - Document intent analysis
- **CitationHelperProvider** - Reference checking
- **DiffNarratorProvider** - Change description
- **OutlineEnhancerProvider** - Structure suggestions
- **ActionExtractorProvider** - Task extraction
- **ImageGeneratorProvider** - Image generation
- **TypoFixerProvider** - Spelling correction
- **SuggestionProvider** - Writing suggestions
- **LexiconProvider** - Definitions and synonyms
- **TimerProvider** - Pomodoro functionality
- **CodingAgentProvider** - Code generation/debugging
- **CodeRunnerProvider** - Sandboxed execution
- **BuildProvider** - Build system integration
- **ShellProvider** - Shell commands
- **PythonInterpreterProvider** - Python execution
- **HistoryProvider** - Document versioning
- **FileReadProvider** / **FileWriteProvider** / **FileListProvider** - File system access

---

## Relationship Graph (Pro)

Interactive visualization of file relationships:

### Relationship Types
- **Structural** - contains, part_of
- **Reference** - references, cites, quotes
- **Dependency** - implements, extends, requires, used_by
- **Versioning** - supersedes, version_of, derived_from
- **Semantic** - related_to, contradicts, supports, explains
- **Workflow** - reviewed_by, approved_by, blocks, blocked_by

### Graph Features
- Force-directed layout physics
- Draggable nodes
- Color-coded by file type
- Relationship type filtering
- Zoom and pan controls

---

## Visual Memory (Pro)

Automatic screenshot capture and AI-powered summarization:

- Periodic screenshot capture
- Screen change detection (5% threshold)
- Image deduplication via hashing
- OCR text extraction
- Active window and app tracking
- Multi-monitor support
- Semantic tagging and querying
- Related file linking

---

## Activity Collector

System-wide activity monitoring for context awareness:

### Data Streams
| Source | What It Captures |
|--------|-----------------|
| **Window Tracking** | Active window, title changes, focus duration, app name |
| **Clipboard** | Text copies, images, file lists |
| **File System** | Create, modify, delete, rename events |
| **Processes** | App launches and exits |
| **Idle Detection** | User active/idle state |

### Features
- Real-time activity feed
- App usage statistics (time per application)
- Clipboard history
- All events stored in semantic memory
- Queryable activity history
- Background collection with minimal overhead

### UI Panel
- Recent Activity tab with live event feed
- App Usage tab showing time spent per application
- Clipboard History tab with copy history
- Start/Stop collection controls

---

## Window Border Tracking

Visual borders around windows launched from MarlOS:

### Features
- Colored border overlay around tracked windows
- Pulsing "TRACKED" indicator badge
- App label with eye icon
- Per-monitor DPI awareness (works on mixed 1080p/4K setups)
- 60 FPS smooth tracking
- Click-through (doesn't interfere with window interaction)

### Visual Elements
- Corner accents for visibility
- Brighter border when window is active
- Customizable colors per application type

---

## Task Management

### Task Extraction
- Checkbox detection (- [ ] and - [x])
- TODO comment extraction
- Tag extraction (@tagname)
- Due date parsing (due:YYYY-MM-DD)
- Heading context (breadcrumb path)

### Task Tracking
- SQLite persistence
- Query by date, tag, status
- Reminder system
- Statistics

---

## Workspace & Window Management

- Multi-tab document editing
- Undock tab to new window (Ctrl+Shift+U)
- Grid workspace creation (Ctrl+Shift+G)
- 3x3 grid layout
- Side-by-side document editing

---

## Browser & Terminal

### Web Browser
- New browser tab (Ctrl+Shift+B)
- URL opening dialog (Ctrl+U)
- Full QWebEngine rendering

### Terminal
- New terminal tab (Ctrl+Shift+T)
- Terminal grid layout (Ctrl+Shift+G)
- Quad terminal setup

---

## Panels & Sidebars

| Panel | Shortcut | Description |
|-------|----------|-------------|
| Tasks | Ctrl+T | Task list viewer |
| Reader | Ctrl+R | Reading-focused view |
| Outline | View menu | Document structure |
| Word Count | View menu | Statistics |
| Chat | Ctrl+Shift+C | AI assistant |
| Console | Ctrl+` | Output and logging |
| Links | Ctrl+L | Document references |
| Semantic | Ctrl+M | Semantic analysis |
| Relations | Ctrl+G | Relationship graph (Pro) |

---

## Markdown Rendering

### Extensions
- Tables and footnotes
- Abbreviations
- Syntax highlighting (Pygments)
- Table of contents generation
- Task list checkboxes
- Strikethrough (~~text~~)
- Emoji support (:emoji:)
- Code block language labels
- External link indicators
- Mermaid diagram support

---

## Code Execution

### Sandbox Modes
- **Direct** - Development mode execution
- **Docker** - Container isolation
- **WebAssembly** - Future support

### Resource Limits
- Memory: 256MB default
- CPU: 0.5 cores
- Timeout: 30 seconds
- Filesystem isolation
- Network isolation (optional)

---

## Export & Import

- Export semantic memory (encrypted)
- Import semantic memory
- Export context for cloud AI models
- Fernet encryption

---

## License Tiers

### Free
- Basic editor and preview
- Local AI integration
- Workspace management

### Pro
- Full semantic memory
- Screen memory/visual capture
- Analytics and focus tracking
- Relation graph
- Context export
- Advanced search

### Team (Future)
- Collaboration features

---

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| New File | Ctrl+N |
| Open File | Ctrl+O |
| Save | Ctrl+S |
| Save As | Ctrl+Shift+S |
| Close Tab | Ctrl+W |
| Find | Ctrl+F |
| Toggle Edit/View | F2 |
| Reload | F5 |
| Dark Mode | Ctrl+D |
| Reader Mode | Ctrl+R |
| Command Palette | Ctrl+Shift+P |
| Tasks Panel | Ctrl+T |
| Links Panel | Ctrl+L |
| Semantic Panel | Ctrl+M |
| Relation Graph | Ctrl+G |
| Chat Panel | Ctrl+Shift+C |
| Console | Ctrl+` |
| New Browser Tab | Ctrl+Shift+B |
| New Terminal | Ctrl+Shift+T |
| Grid Workspace | Ctrl+Shift+G |
| Undock Tab | Ctrl+Shift+U |
| Preferences | Ctrl+, |
| Print/Export PDF | Ctrl+Shift+E |
| Open URL | Ctrl+U |
| App Launcher | Ctrl+Shift+A |

---

## Technical Details

- **Modules**: 28+ Python modules
- **Providers**: 26+ specialized providers
- **Memory Types**: 8 distinct entry types
- **Relationship Types**: 20+ semantic types
- **File Extensions**: 50+ supported
- **Embedding Models**: 5 available (4 local + 1 fallback)
- **Vector Dimensions**: 384D semantic vectors
- **Visualization**: OpenGL 3D with PCA/t-SNE reduction
