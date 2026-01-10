# Document IDE - Architecture Reference

> A quick reference guide to understanding how the Document IDE works.

---

## Table of Contents

1. [Overview](#overview)
2. [Core Philosophy](#core-philosophy)
3. [Architecture Diagram](#architecture-diagram)
4. [Core Components](#core-components)
5. [Data Flow](#data-flow)
6. [Provider System](#provider-system)
7. [Command System](#command-system)
8. [Task System](#task-system)
9. [File Structure](#file-structure)

---

## Overview

The Document IDE is a PyQt6-based application that evolved from a markdown viewer into a full development environment for writing, coding, notes, and journals. It follows a **quiet-by-default** philosophy where nothing happens without explicit user intent.

### Key Technologies

| Technology | Purpose |
|------------|---------|
| PyQt6 | Desktop GUI framework |
| SQLite3 | Task/reminder persistence |
| markdown | Markdown to HTML conversion |
| Pygments | Syntax highlighting (via codehilite) |

---

## Core Philosophy

The system is built on five non-negotiable laws:

1. **Providers may observe by default** - They can watch documents/events passively
2. **Providers may act only via commands** - All mutations require explicit invocation
3. **Commands exist only by user intent** - No auto-execution
4. **Editing is sacred** - Default behavior matches a plain text editor
5. **Understanding precedes action** - Quiet-by-default, no surprises

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                         MarkdownEditor (QMainWindow)                │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    Tab Widget (QTabWidget)                   │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │   │
│  │  │ MarkdownTab │  │ MarkdownTab │  │  ReaderTab  │  ...    │   │
│  │  └──────┬──────┘  └─────────────┘  └─────────────┘         │   │
│  └─────────┼───────────────────────────────────────────────────┘   │
│            │                                                        │
│            ▼                                                        │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                      IDEContext                              │   │
│  │  ┌──────────┐  ┌──────────┐  ┌────────────┐  ┌──────────┐  │   │
│  │  │ Document │  │  Events  │  │  Commands  │  │  Editor  │  │   │
│  │  │          │  │  Spine   │  │  Registry  │  │ (widget) │  │   │
│  │  └────┬─────┘  └────┬─────┘  └─────┬──────┘  └──────────┘  │   │
│  └───────┼─────────────┼──────────────┼───────────────────────┘   │
│          │             │              │                            │
│          ▼             ▼              ▼                            │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                     Provider Layer                           │   │
│  │  ┌────────────┐ ┌────────────┐ ┌────────────┐               │   │
│  │  │ Always-On  │ │  Implied   │ │ On-Demand  │               │   │
│  │  │ Providers  │ │ Providers  │ │ Providers  │               │   │
│  │  │            │ │            │ │            │               │   │
│  │  │ • Language │ │ • Preview  │ │ • Formatng │               │   │
│  │  │   Service  │ │ • Outline  │ │ • IntentMap│               │   │
│  │  │ • WordCount│ │            │ │ • ImageGen │               │   │
│  │  └────────────┘ └────────────┘ └────────────┘               │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Core Components

### 1. Document (`ide/document.py`)

The **single source of truth** for textual content.

```python
class Document:
    _content: str          # Raw markdown text
    _last_saved_content: str  # Content at last save (for diff)
    file_path: str         # File location (or None for unsaved)
    modified: bool         # Has unsaved changes
    events: EventsSpine    # Event broadcaster
```

**Key Methods:**
- `load(file_path)` - Load file, emit `document_changed`
- `save()` - Write to disk, emit `document_saved`
- `set_content(content)` - Update content, emit `document_changed`

**Rules:**
- MUST NOT know about UI
- MUST NOT render itself
- MUST NOT call providers directly

---

### 2. Events Spine (`ide/events.py`)

The **pub-sub system** for state change propagation.

```python
class EventsSpine(QObject):
    document_changed = pyqtSignal(object)  # Content modified
    document_saved = pyqtSignal(object)    # File written to disk
    cursor_moved = pyqtSignal(int)         # Cursor position changed
    document_opened = pyqtSignal(object)   # New document loaded
    document_closed = pyqtSignal(object)   # Document tab closed
```

**Usage:**
```python
# Connecting (in a provider)
context.events.document_changed.connect(self.on_change)

# Emitting (in Document)
self.events.document_changed.emit(self)
```

---

### 3. IDEContext (`ide/context.py`)

The **capability surface** - the only interface providers use to interact with the IDE.

```python
class IDEContext:
    document: Document
    events: EventsSpine
    commands: CommandRegistry
    editor: QPlainTextEdit | None  # May be None in view-only mode
    preview: QWidget | None        # May be None in edit mode
```

**Key Methods:**

| Method | Purpose |
|--------|---------|
| `has_selection()` | Check if text is selected |
| `get_selection()` | Get selected text |
| `insert_text(text)` | Insert at cursor |
| `set_content(content)` | Replace entire document |
| `confirm(title)` | Show confirmation dialog |
| `present_text(title, content)` | Show result dialog |
| `choose_option(title, msg, opts)` | Show choice dialog |

**Rules:**
- Providers MUST use IDEContext for all interactions
- Providers MUST NOT hold direct widget references

---

### 4. Command Registry (`ide/commands.py`)

The **action system** for user-invoked operations.

```python
@dataclass(frozen=True)
class Command:
    command_id: str           # Stable ID like "markdown.format.bold"
    title: str                # Display name like "Bold"
    handler: Callable         # Function to execute
    requires_selection: bool  # Needs selected text?
    modifies_document: bool   # Will change content?
    confirm: str | None       # Confirmation message (if any)

class CommandRegistry:
    def register(command)     # Add a command
    def execute(id, context)  # Run a command
    def get(id)               # Retrieve by ID
```

**Command Naming Convention:**
```
namespace.action.subaction

Examples:
  document.save
  markdown.format.bold
  markdown.export.pdf
  tasks.showPanel
  reminders.createFromTask
```

---

## Data Flow

### Example: User Types in Edit Mode

```
1. User types "Hello"
       │
       ▼
2. QPlainTextEdit.textChanged signal
       │
       ▼
3. MarkdownTab.on_text_changed()
       │
       ▼
4. Document.set_content("Hello")
       │
       ▼
5. EventsSpine.document_changed.emit(document)
       │
       ├──▶ MarkdownLanguageService._parse()  [re-analyze structure]
       ├──▶ WordCountProvider._update()        [update stats]
       ├──▶ PreviewProvider._on_document_changed()  [invalidate cache]
       └──▶ TaskExtractor (if enabled)         [re-extract tasks]
```

### Example: User Runs a Command

```
1. User presses Ctrl+Shift+P, types "Bold", hits Enter
       │
       ▼
2. CommandPalette selects command_id="markdown.format.bold"
       │
       ▼
3. CommandRegistry.execute("markdown.format.bold", context)
       │
       ▼
4. Registry validates:
   - Command exists? ✓
   - requires_selection met? ✓
   - Confirmation needed? (show dialog if so)
       │
       ▼
5. FormattingProvider._wrap_selection(context, "**", "**")
       │
       ▼
6. context.insert_text("**selected**")
       │
       ▼
7. Document.set_content() → document_changed event
       │
       ▼
8. Preview updates on next view
```

---

## Provider System

Providers are **bounded capabilities** that observe, derive, and request actions.

### Provider Categories

| Category | Activation | UI | Mutation |
|----------|------------|-----|----------|
| **Always-On** | Automatic at load | None | Never |
| **Implied** | Automatic | Hidden by default | Never |
| **On-Demand** | Command only | As needed | With permission |

### Always-On Providers

**Purpose:** Shared understanding, caching, indexing

| Provider | File | Purpose |
|----------|------|---------|
| `MarkdownLanguageService` | providers.py:233 | Parse headings, links, code blocks, tasks |
| `WordCountProvider` | providers.py:299 | Track word/char counts |

### Implied Providers

**Purpose:** Quiet affordances, optional UI

| Provider | File | Purpose |
|----------|------|---------|
| `OutlineProvider` | providers.py:287 | Extract heading hierarchy |
| `PreviewProvider` | providers.py:315 | Cache rendered HTML |

### On-Demand Providers

**Purpose:** Transformations, exports, generators

| Provider | File | Purpose |
|----------|------|---------|
| `FormattingProvider` | providers.py:339 | Bold, italic, headings, lists, etc. |
| `IntentMapProvider` | providers.py:527 | Summarize document structure |
| `CitationHelperProvider` | providers.py:551 | Find claims without citations |
| `DiffNarratorProvider` | providers.py:575 | Describe changes since save |
| `OutlineEnhancerProvider` | providers.py:597 | Suggest structure improvements |
| `ActionExtractorProvider` | providers.py:621 | Extract action items |
| `ImageGeneratorProvider` | providers.py:645 | Generate placeholder SVG |
| `TypoFixerProvider` | providers.py:702 | Fix common typos |
| `SuggestionProvider` | providers.py:762 | Generate text suggestions |
| `LexiconProvider` | providers.py:794 | Definition/synonym lookup |
| `TimerProvider` | providers.py:843 | Pomodoro-style timer |
| `DocumentCommandProvider` | providers.py:500 | Save, print, PDF export |

---

## Command System

### All Registered Commands

#### Document Commands
| Command ID | Title | Provider |
|------------|-------|----------|
| `document.save` | Save | DocumentCommandProvider |
| `document.save_as` | Save As | DocumentCommandProvider |
| `document.print` | Print | DocumentCommandProvider |
| `markdown.export.pdf` | Export PDF | DocumentCommandProvider |

#### Formatting Commands
| Command ID | Title |
|------------|-------|
| `markdown.format.heading1` | Heading 1 |
| `markdown.format.heading2` | Heading 2 |
| `markdown.format.heading3` | Heading 3 |
| `markdown.format.bold` | Bold |
| `markdown.format.italic` | Italic |
| `markdown.format.code` | Inline Code |
| `markdown.format.link` | Insert Link |
| `markdown.format.image` | Insert Image |
| `markdown.format.list.bulleted` | Bulleted List |
| `markdown.format.list.numbered` | Numbered List |
| `markdown.format.quote` | Blockquote |
| `markdown.format.table` | Insert Table |
| `markdown.format.codeblock` | Insert Code Block |
| `markdown.format.diagram` | Insert Mermaid Diagram |

#### AI/Analysis Commands
| Command ID | Title |
|------------|-------|
| `document.intent.map` | Generate Intent Map |
| `document.citation.helper` | Citation Helper |
| `document.diff.narrator` | Diff Narrator |
| `document.outline.enhancer` | Outline Enhancer |
| `document.action.extractor` | Action Extractor |
| `document.typo.fix` | Fix Typos |
| `document.text.suggestions` | Text Suggestions |
| `document.lookup.term` | Lookup Definition |
| `markdown.generate.image` | Generate Image (SVG) |

#### Task Commands
| Command ID | Title |
|------------|-------|
| `tasks.showPanel` | Show Tasks Panel |
| `tasks.refreshIndex` | Refresh Task Index |
| `tasks.openSource` | Open Task Source |
| `tasks.toggleDone` | Toggle Task Done |

#### Reminder Commands
| Command ID | Title |
|------------|-------|
| `reminders.createFromSelection` | Create Reminder From Selection |
| `reminders.createFromTask` | Create Reminder From Task |
| `reminders.list` | List Reminders |
| `reminders.dismiss` | Dismiss Reminder |

#### Agent Commands
| Command ID | Title |
|------------|-------|
| `agent.summarizeOpenTasks` | Summarize Open Tasks |
| `agent.suggestDueDates` | Suggest Due Dates |
| `agent.proposeReminderSchedule` | Propose Reminder Schedule |

#### Timer Commands
| Command ID | Title |
|------------|-------|
| `timer.start` | Start Timer |
| `timer.stop` | Stop Timer |
| `timer.status` | Timer Status |

#### Code Agent Commands (LM Studio)
| Command ID | Title |
|------------|-------|
| `code.complete` | Complete Code at Cursor |
| `code.explain` | Explain Selected Code |
| `code.edit` | Edit Code with Instruction |
| `code.generate` | Generate Code from Description |
| `code.status` | Check AI Status |

---

## Task System

### Task Extraction (`ide/tasks.py`)

The `TaskExtractor` finds tasks in markdown documents.

**Recognized Syntax:**
```markdown
- [ ] Open task
- [x] Completed task
TODO: Another task
TODO(optional label): Yet another task
```

**Metadata Extraction:**
```markdown
- [ ] Email client @clientA due:2026-01-10
- [ ] Renew passport @due(2026-02-01) @personal
```

Extracts:
- `@tags` - Identifiers starting with @
- `due:YYYY-MM-DD` or `@due(YYYY-MM-DD)` - Due dates
- Heading context (breadcrumb path to task)

### Task Index Store

**Database:** `tasks_index.db` (SQLite)

**Task Table Schema:**
```sql
CREATE TABLE tasks (
    task_id TEXT PRIMARY KEY,      -- SHA1 hash
    normalized_text TEXT NOT NULL,  -- Lowercase, collapsed whitespace
    text TEXT NOT NULL,             -- Original task text
    status TEXT NOT NULL,           -- "open" or "done"
    source_path TEXT NOT NULL,      -- File containing the task
    source_line INTEGER NOT NULL,   -- Line number (0-indexed)
    heading_path TEXT NOT NULL,     -- JSON array of heading breadcrumbs
    tags TEXT NOT NULL,             -- JSON array of tags
    due TEXT,                       -- ISO date string or NULL
    created_at TEXT NOT NULL,       -- First seen timestamp
    updated_at TEXT NOT NULL,       -- Last indexed timestamp
    first_seen_iso TEXT NOT NULL    -- For stable ID generation
)
```

**Reminder Table Schema:**
```sql
CREATE TABLE reminders (
    reminder_id TEXT PRIMARY KEY,
    task_id TEXT,                   -- Link to task (optional)
    title TEXT NOT NULL,
    notes TEXT,
    scheduled_for TEXT NOT NULL,    -- ISO datetime
    status TEXT NOT NULL,           -- "scheduled", "dismissed", "fired"
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
)
```

**Stable ID Strategy:**
```python
task_id = sha1(source_path + "\n" + normalized_text + "\n" + first_seen_iso)
```

This ensures tasks maintain identity even when lines move.

---

## File Structure

```
markdown_viewer/
├── viewer.py                 # Main application (2800+ lines)
├── document_events.py        # (Legacy, use ide/events.py)
├── pyproject.toml           # Project metadata
│
├── ide/                     # Core IDE framework
│   ├── __init__.py
│   ├── document.py          # Document model (56 lines)
│   ├── events.py            # Events spine (10 lines)
│   ├── context.py           # IDEContext (90 lines)
│   ├── commands.py          # Command registry (40 lines)
│   ├── providers.py         # All providers (860+ lines)
│   ├── ai.py                # LocalAIClient (270 lines)
│   ├── tasks.py             # Task extraction & indexing (290 lines)
│   ├── lexicon.py           # Local lexicon (52 lines)
│   └── data/
│       ├── lexicon.json     # Built-in definitions
│       └── qbd_questions.json
│
├── lexicon_packs/           # Custom lexicon packs
│   └── README.md
│
├── reader/                  # PDF/EPUB reader assets
│   └── pdfjs/              # pdf.js distribution
│
├── scripts/
│   └── build_wordnet_pack.py
│
└── Documentation/
    ├── Document IDE Architecture v0.txt
    ├── TASK_REMINDERS_AGENT_SPEC.md
    ├── PROVIDER_AUTHORING_GUIDE.md
    └── Documentation guide.md
```

---

## Key Classes Quick Reference

| Class | File | Line | Purpose |
|-------|------|------|---------|
| `MarkdownEditor` | viewer.py | ~1550 | Main window |
| `MarkdownTab` | viewer.py | ~700 | Single document tab |
| `ReaderTab` | viewer.py | ~270 | PDF/EPUB viewer tab |
| `MarkdownHighlighter` | viewer.py | ~67 | Syntax highlighting |
| `SearchBar` | viewer.py | ~148 | Find in preview |
| `TaskBadgeButton` | viewer.py | ~195 | Status bar task indicator with red dot badge |
| `CommandPalette` | viewer.py | ~600 | Ctrl+Shift+P dialog |
| `Document` | ide/document.py | 1 | Document model |
| `EventsSpine` | ide/events.py | 4 | Event system |
| `IDEContext` | ide/context.py | 1 | Provider API |
| `CommandRegistry` | ide/commands.py | 15 | Command storage |
| `TaskExtractor` | ide/tasks.py | 9 | Find tasks in markdown |
| `TaskIndexStore` | ide/tasks.py | 64 | SQLite persistence |
| `LMStudioClient` | ide/ai.py | 43 | LM Studio API client |
| `HybridAIClient` | ide/ai.py | 193 | LM Studio + local fallback |
| `LocalAIClient` | ide/ai.py | 250 | Heuristic AI (fallback) |
| `CodingAgentProvider` | ide/providers.py | 949 | AI code completion/editing |
| `LocalLexicon` | ide/lexicon.py | 5 | Dictionary lookup |

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `F2` | Toggle Edit/View mode |
| `Ctrl+S` | Save |
| `Ctrl+Shift+S` | Save As |
| `Ctrl+P` | New Tab |
| `Ctrl+O` | Open File |
| `Ctrl+W` | Close Tab |
| `Ctrl+F` | Find in Preview |
| `Ctrl+Shift+P` | Command Palette |
| `Ctrl+,` | Settings |

---

## Adding a New Provider

See `PROVIDER_AUTHORING_GUIDE.md` for the full guide. Quick template:

```python
from ide.providers import Provider, ProviderCategory
from ide.commands import Command

class MyProvider(Provider):
    def __init__(self):
        super().__init__("my_provider", ProviderCategory.ON_DEMAND)

    def activate(self, context):
        context.commands.register(
            Command(
                "myprovider.doThing",
                "Do My Thing",
                lambda ctx: self._do_thing(ctx),
                modifies_document=True,
            )
        )

    def _do_thing(self, context):
        # Your logic here
        return True
```

Then register in `MarkdownTab.__init__`:
```python
self.my_provider = MyProvider()
self.my_provider.activate(self.context)
```
