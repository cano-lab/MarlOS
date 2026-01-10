# Provider Authoring Guide (Document IDE v0)
*A consistency guide for building providers, commands, and services without breaking the manifesto.*

---

## 0. North Star

> **Providers may observe by default. Providers may act only through commands.**

If a provider surprises the user, it is a bug.

---

## 1. Vocabulary (use these words consistently)

- **Document**: Raw Markdown text + file metadata (path, dirty state). Source of truth.
- **Events**: Signals emitted when document/editor state changes.
- **Language Service**: Read-only structural understanding of the document (symbols, blocks, links).
- **Provider**: A bounded capability that observes/derives/requests actions.
- **Command**: A named, user-invoked action executed by the IDE.
- **Context**: The only interface a provider uses to interact with the IDE.

---

## 2. Provider Categories (must pick exactly one)

### 2.1 Always-On (Silent Infrastructure)
**Purpose:** shared understanding; caching; indexing  
**Rules:**
- No UI
- No prompts
- No mutation
- Must be fast

Examples: Markdown language service, task extraction/indexing

---

### 2.2 Implied (Contextual, Passive)
**Purpose:** quiet affordances; optional UI surfaces  
**Rules:**
- UI must be hidden by default
- No popups while typing
- No mutation

Examples: outline panel, word count, preview view

---

### 2.3 On-Demand (Explicit Tools)
**Purpose:** transformations, exports, generators  
**Rules:**
- Must be activated by a command
- Must ask for confirmation if it modifies the document or overwrites assets
- Must be undoable if it edits the document

Examples: image generator, PDF export, formatters, diagnostics checks

---

## 3. The Provider Contract (v0)

### 3.1 Minimal interface
A provider **must** implement:

```python
class Provider:
    def activate(self, context):
        """Called once after the provider is registered."""
        pass
```

### 3.2 Golden rules
- Providers **MUST NOT** hold references to the main window.
- Providers **MUST NOT** directly manipulate widgets except those explicitly handed in via `context`.
- Providers **MUST NOT** modify document text unless invoked via a **command**.
- Providers **MUST** be removable without breaking the app.

---

## 4. Context Usage Rules

Providers interact with the IDE only via `context`.

### 4.1 Context is capability, not ownership
Assume some fields may be `None` depending on mode or UI:

- `context.editor` may be `None` in view-only contexts
- `context.preview` may be `None` in edit-only contexts

Providers must degrade gracefully.

### 4.2 Allowed read operations
- `context.document.content`
- `context.document.file_path`
- `context.get_selection()` (if available)
- `context.language_service` outputs (symbols, links, etc.)

### 4.3 Allowed write operations (only via command handlers)
- `context.insert_text(text)`
- `context.replace_selection(text)` *(optional helper)*
- `context.apply_text_edit(edit)` *(future; structured edits)*

**Never** do writes on event callbacks unless the event was triggered by a command execution.

---

## 5. Events: How to listen safely

### 5.1 Common events
- `document_changed(document)`
- `document_saved(document)`
- `cursor_moved(position)`
- `document_opened(path)`
- `document_closed(path)`

### 5.2 Debounce expensive work
If a provider performs non-trivial computation, it must:
- debounce on `document_changed`
- or compute incrementally
- or run only on save / on-demand

**Rule:** typing latency must not degrade.

---

## 6. Commands: How to act safely

### 6.1 Command naming
Commands must be stable IDs:

- `tasks.showPanel`
- `markdown.export.pdf`
- `markdown.generate.image`

Format:
- lower-case
- dot-separated namespaces
- verb-first for actions when possible

### 6.2 Command flags (use consistently)
- `requires_selection=True` if the command reads selection
- `modifies_document=True` if it edits text
- `confirm=True` if it:
  - modifies the document
  - overwrites assets
  - triggers exports
  - schedules reminders

### 6.3 Command handlers
Handlers should be:

- pure-ish (depend on context only)
- easy to test
- return success/failure (`True/False`) or a small result object

Example:

```python
def handler(context):
    sel = context.get_selection()
    if not sel:
        return False
    context.insert_text(f"**{sel}**")
    return True
```

---

## 7. UI Surfaces (Implied Providers)

### 7.1 “Hidden by default” requirement
If your provider has a panel/widget:
- it must not appear automatically
- it must open only via command (or explicit user gesture)

### 7.2 No unsolicited popups
No message boxes on document changes. Ever.

If the provider needs to report something:
- use status bar
- use a quiet panel
- or wait until user runs a command

---

## 8. Document Mutation & Undo (Transformative Providers)

### 8.1 Don’t mutate without undo
Any command that modifies the document must be undoable.

v0 approach (PyQt):
- rely on `QPlainTextEdit` undo stack if edits are performed through its cursor
- group edits using `beginEditBlock` / `endEditBlock` when using `QTextCursor`

### 8.2 Prefer minimal edits
For task toggling:
- change only `[ ]` ↔ `[x]`
- do not rewrite the whole line

---

## 9. Storage Providers (Index/Reminders)

### 9.1 Use an interface
Do not bind providers directly to SQLite/MySQL.

Define:

```python
class IndexStore:
    def upsert_tasks(self, tasks): ...
    def query_tasks(self, filters): ...
```

Then implement:
- `SQLiteIndexStore` (v0)
- future: `MySQLIndexStore`

### 9.2 Rebuildable cache + persisted user intent
Separate:
- **Derived index** (rebuildable)
- **Reminders** (persisted user decisions)

---

## 10. Task/Reminder Safety Rules (Journaling consistency)

- Extracting tasks is okay by default (silent).
- Creating reminders is **never** automatic.
- Agents can suggest; only commands can apply.

---

## 11. File/Folder Layout (recommended)

```
src/
  ide_context.py
  document_events.py
  commands.py
  services/
    markdown_language_service.py
    task_extraction_service.py
  providers/
    outline_provider.py
    preview_provider.py
    word_count_provider.py
    reminders_provider.py
    image_generator_provider.py
  stores/
    index_store.py
    sqlite_index_store.py
```

---

## 12. Definition of “Done” for a Provider PR

A provider is acceptable when:

- It has a clear category (Always-on / Implied / On-demand)
- It registers any actions as commands
- It introduces no typing lag (or it debounces)
- It does not pop UI unexpectedly
- It can be removed without breaking core editing
- It includes minimal tests (for parsing/logic providers)

---

## 13. Quick Templates

### 13.1 Always-on provider template

```python
class MyServiceProvider(Provider):
    def activate(self, context):
        context.events.document_changed.connect(self.on_change)

    def on_change(self, document):
        # compute + cache, no UI, no writes
        pass
```

### 13.2 On-demand provider template

```python
class MyToolProvider(Provider):
    def activate(self, context):
        context.commands.register(Command(
            id="mytool.run",
            title="Run My Tool",
            handler=self.run,
            modifies_document=True,
            confirm=True
        ))

    def run(self, context):
        # user-invoked, safe to write via context methods
        return True
```

---

## 14. Philosophy Check (before merging)

Ask these three questions:

1. **Could this ever surprise the user?**
2. **Could this ever slow typing?**
3. **Could this be removed cleanly?**

If any answer is “yes”, revise.

