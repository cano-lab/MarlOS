# Document IDE - Quick Reference Cheatsheet

> One-page reference for common questions about the codebase.

---

## How Do I...?

### Open a file programmatically?
```python
# In MarkdownEditor
self.open_file("/path/to/file.md")
```

### Get the current document content?
```python
# From context (in a provider)
content = context.document.content

# From tab (in viewer.py)
tab = self.tab_widget.currentWidget()
content = tab.document.content
```

### Insert text at cursor?
```python
context.insert_text("your text here")
```

### Check if text is selected?
```python
if context.has_selection():
    selected = context.get_selection()
```

### Register a new command?
```python
from ide.commands import Command

context.commands.register(
    Command(
        "namespace.action.name",   # Stable ID
        "Display Title",            # User-visible name
        lambda ctx: self.handler(ctx),
        modifies_document=True,     # Optional
        confirm="Are you sure?",    # Optional
    )
)
```

### Listen for document changes?
```python
def activate(self, context):
    context.events.document_changed.connect(self.on_change)

def on_change(self, document):
    # document.content has new content
    pass
```

### Show a dialog with text results?
```python
context.present_text("Title", "Content here", allow_insert=True)
```

### Ask user to choose an option?
```python
choice = context.choose_option(
    "Title",
    "What do you want to do?",
    ["Option 1", "Option 2", "Cancel"]
)
```

### Ask for a save file path?
```python
path = context.request_save_path(
    "Save File",
    "Text Files (*.txt);;All Files (*)"
)
```

---

## Where Is...?

| What | File | Line |
|------|------|------|
| Main entry point | `viewer.py` | `main()` at ~2808 |
| Main window class | `viewer.py` | `MarkdownEditor` at ~1550 |
| Document tab | `viewer.py` | `MarkdownTab` at ~700 |
| PDF/EPUB reader | `viewer.py` | `ReaderTab` at ~195 |
| Syntax highlighter | `viewer.py` | `MarkdownHighlighter` at ~67 |
| Command palette | `viewer.py` | `CommandPalette` at ~530 |
| Document model | `ide/document.py` | `Document` |
| Event system | `ide/events.py` | `EventsSpine` |
| Context object | `ide/context.py` | `IDEContext` |
| Command system | `ide/commands.py` | `CommandRegistry`, `Command` |
| All providers | `ide/providers.py` | Various classes |
| Task extraction | `ide/tasks.py` | `TaskExtractor` at ~9 |
| Task storage | `ide/tasks.py` | `TaskIndexStore` at ~64 |
| AI functions | `ide/ai.py` | `LocalAIClient` |
| Lexicon lookup | `ide/lexicon.py` | `LocalLexicon` |
| Markdown renderer | `ide/providers.py` | `MarkdownRenderer` at ~24 |

---

## Provider Categories

| Category | When Active | Can Mutate? | Has UI? |
|----------|-------------|-------------|---------|
| **Always-On** | Automatically | No | No |
| **Implied** | Automatically | No | Hidden by default |
| **On-Demand** | By command | Yes (with confirm) | As needed |

---

## All Commands (Quick List)

### Document
- `document.save` - Save
- `document.save_as` - Save As
- `document.print` - Print
- `markdown.export.pdf` - Export PDF

### Formatting
- `markdown.format.heading1/2/3` - Headings
- `markdown.format.bold` - **Bold**
- `markdown.format.italic` - *Italic*
- `markdown.format.code` - `Code`
- `markdown.format.link` - [Link]()
- `markdown.format.image` - ![Image]()
- `markdown.format.list.bulleted` - Bullet list
- `markdown.format.list.numbered` - Numbered list
- `markdown.format.quote` - Blockquote
- `markdown.format.table` - Table
- `markdown.format.codeblock` - Code block
- `markdown.format.diagram` - Mermaid diagram

### Analysis
- `document.intent.map` - Intent Map
- `document.citation.helper` - Citation Helper
- `document.diff.narrator` - Diff Narrator
- `document.outline.enhancer` - Outline Enhancer
- `document.action.extractor` - Action Extractor
- `document.typo.fix` - Fix Typos
- `document.text.suggestions` - Suggestions
- `document.lookup.term` - Lookup Definition
- `markdown.generate.image` - Generate SVG

### Tasks
- `tasks.showPanel` - Show Tasks Panel
- `tasks.refreshIndex` - Refresh Index
- `tasks.openSource` - Jump to Task
- `tasks.toggleDone` - Toggle Done

### Reminders
- `reminders.createFromSelection` - Create from Selection
- `reminders.createFromTask` - Create from Task
- `reminders.list` - List All
- `reminders.dismiss` - Dismiss Reminder

### Agent
- `agent.summarizeOpenTasks` - Summarize Tasks
- `agent.suggestDueDates` - Suggest Due Dates
- `agent.proposeReminderSchedule` - Propose Schedule

### Timer
- `timer.start` - Start Timer
- `timer.stop` - Stop Timer
- `timer.status` - Timer Status

---

## Task Badge Indicator

The status bar has a **Tasks** button with a subtle red dot indicator:

- **Red dot appears** when there are actionable items:
  - Open tasks with due dates today or overdue
  - Scheduled reminders that are due
- **No counts, no popups** - just a quiet invitation to look
- **Click** to open the Tasks Panel

The badge updates:
- Every 60 seconds automatically
- When documents are saved (tasks re-indexed)
- When reminders are created/dismissed

---

## Task Syntax

```markdown
# Checkbox tasks
- [ ] Open task
- [x] Completed task

# TODO tasks
TODO: Task description
TODO(label): With optional label

# Tags
- [ ] Email client @clientA @urgent

# Due dates
- [ ] Invoice due:2026-01-15
- [ ] Passport @due(2026-02-01)
```

---

## LM Studio Integration

The IDE integrates with LM Studio for AI-powered features:

### Setup
1. Download and install [LM Studio](https://lmstudio.ai/)
2. Load a model in LM Studio
3. Start the local server (default: `localhost:1234`)
4. Use **View > Settings** to configure the endpoint if needed

### AI Commands (via Command Palette)
| Command | Description |
|---------|-------------|
| `code.complete` | Complete code at cursor position |
| `code.explain` | Explain selected code |
| `code.edit` | Edit code with natural language instruction |
| `code.generate` | Generate code from description |
| `code.status` | Check if LM Studio is connected |

### Hybrid Mode
- When LM Studio is running: Uses real AI for all features
- When LM Studio is offline: Falls back to heuristic-based responses
- Code commands require LM Studio (no fallback)

---

## Key Shortcuts

| Shortcut | Action |
|----------|--------|
| `F2` | Toggle Edit/View |
| `Ctrl+S` | Save |
| `Ctrl+Shift+S` | Save As |
| `Ctrl+P` | New Tab |
| `Ctrl+O` | Open File |
| `Ctrl+W` | Close Tab |
| `Ctrl+F` | Find in Preview |
| `Ctrl+Shift+P` | Command Palette |
| `Ctrl+,` | Settings |

---

## Data Flow Summary

```
User Action
    │
    ▼
Document.set_content()
    │
    ▼
EventsSpine.document_changed.emit()
    │
    ├──▶ LanguageService (re-parse)
    ├──▶ WordCount (update stats)
    ├──▶ Preview (invalidate cache)
    └──▶ TaskExtractor (if enabled)
```

---

## Event Signals

| Signal | Emitted When |
|--------|--------------|
| `document_changed` | Content modified |
| `document_saved` | File written to disk |
| `cursor_moved` | Cursor position changed |
| `document_opened` | New file loaded |
| `document_closed` | Tab closed |

---

## Database Schema (SQLite)

### tasks table
```sql
task_id TEXT PRIMARY KEY,
normalized_text TEXT,
text TEXT,
status TEXT,           -- "open" or "done"
source_path TEXT,
source_line INTEGER,
heading_path TEXT,     -- JSON array
tags TEXT,             -- JSON array
due TEXT,              -- ISO date or NULL
created_at TEXT,
updated_at TEXT,
first_seen_iso TEXT
```

### reminders table
```sql
reminder_id TEXT PRIMARY KEY,
task_id TEXT,          -- NULL if from selection
title TEXT,
notes TEXT,
scheduled_for TEXT,    -- ISO datetime
status TEXT,           -- "scheduled", "dismissed", "fired"
created_at TEXT,
updated_at TEXT
```

---

## File Types Supported

| Extension | Handler |
|-----------|---------|
| `.md`, `.markdown` | MarkdownTab (edit + preview) |
| `.txt` | MarkdownTab (plain text) |
| `.pdf` | ReaderTab (pdf.js viewer) |
| `.epub` | ReaderTab (ebooklib) |

---

## Adding a Provider (Minimal)

```python
from ide.providers import Provider, ProviderCategory
from ide.commands import Command

class MyProvider(Provider):
    def __init__(self):
        super().__init__("my_name", ProviderCategory.ON_DEMAND)

    def activate(self, context):
        context.commands.register(Command(
            "my.command.id",
            "My Command",
            self.handler
        ))

    def handler(self, context):
        # Do something
        return True
```

Then in `MarkdownTab.__init__`:
```python
self.my_provider = MyProvider()
self.my_provider.activate(self.context)
```

---

## Common Patterns

### Non-greedy matching
```python
r"```(.+?)```"   # Stops at first ```
r"```(.+)```"    # Goes to last ```
```

### Word boundary
```python
r"\bword\b"      # Matches "word" not "swordfish"
```

### Match newlines
```python
r"[\s\S]*"       # Any char including newline
# or use re.DOTALL flag
```

### Negative lookahead
```python
r"(?<!\*)\*"     # * not preceded by *
r"\*(?!\*)"      # * not followed by *
```

---

## Troubleshooting

**Command not showing up?**
- Check it's registered in `activate()`
- Verify command ID is unique
- Check if `requires_selection=True` but nothing is selected

**Provider not running?**
- Ensure `activate()` is called in `MarkdownTab.__init__`
- Check event connections are correct

**Task not appearing?**
- Verify syntax: `- [ ] text` or `TODO: text`
- Check if file is saved (triggers index update)
- Run `tasks.refreshIndex` command

**Preview not updating?**
- Cache is invalidated on `document_changed`
- Force refresh by toggling mode (F2)
