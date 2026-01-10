# Derived Tasks & Reminders System (Document IDE v0)
## Implementation Specification (Agent + Providers)

> This spec defines a **derived** (out-of-band) tasks & reminders system that can index multiple Markdown documents and present a master task list, with reminders created **only by explicit user intent**.

---

## 1. Goals

- Extract tasks/reminder candidates from **many documents** (journals, specs, notes) into a **master list**.
- Keep documents **clean and portable** (no required embedded metadata).
- Support **quiet-by-default** behavior:
  - No popups.
  - No automatic scheduling.
  - No automatic rewriting.
- Provide an **agent-assisted** workflow (optional) that can:
  - Summarize open tasks
  - Suggest due dates/tags
  - Draft reminder plans
  - Never schedule or edit without an explicit command.

---

## 2. Non-goals

- Real-time “nagging” notifications while typing.
- Automatic task creation from arbitrary prose without user opt-in.
- Calendar sync in v0 (can be added later via exporters, e.g., `.ics`).
- Multi-user shared database.

---

## 3. Core Laws

1. **Providers may observe by default.**
2. **Providers may act only via commands.**
3. **Scheduling reminders requires explicit user confirmation.**
4. **Documents remain the source of truth; the system is derived and rebuildable.**

---

## 4. Architecture Overview

### 4.1 Components

**Always-on (silent)**
- **Document Events Spine**
  - Emits: `document_changed`, `document_saved`, `cursor_moved`, `document_opened`, `document_closed`
- **Markdown Language Service (structure)**
  - Parses headings/blocks/links
- **Task Extraction Service**
  - Extracts tasks + metadata from text/AST
- **Task Index Store**
  - Persists derived index; rebuildable

**Implied (quiet UI)**
- **Master Tasks Panel**
  - Hidden by default; opens on command
  - Filter/group/sort, jump-to-source

**On-demand (explicit)**
- **Reminder Provider**
  - Creates reminders from selected text or a task item
- **Agent Provider**
  - Suggests tags/due dates; generates summaries; proposes reminder schedules (drafts)
- **Export Providers (optional)**
  - Export reminders/tasks to `.ics`, CSV, JSON, etc.

---

## 5. Provider Activation Policy

- **Always-on:** Task extraction + indexing (silent, read-only)
- **Implied:** Master tasks panel (visible only when opened)
- **On-demand:** Reminder creation; agent suggestions; exports; toggle-done (edits document)

---

## 6. Data Model (v0)

### 6.1 TaskItem (derived)
Minimum fields:

- `task_id: str` — stable ID (see §7)
- `text: str` — task content (without checkbox marker)
- `status: "open" | "done"`
- `source_path: str`
- `source_line: int` — 0-based line index (best-effort)
- `source_heading_path: list[str]` — e.g., `["2026-01-06", "Morning"]`
- `tags: list[str]` — e.g., `["@home", "@clientA"]`
- `due: str | None` — ISO date `YYYY-MM-DD` (v0)
- `created_at: str` — ISO datetime (when first seen by indexer)
- `updated_at: str` — ISO datetime (last indexed)

Optional fields (v0+):
- `priority: "low"|"med"|"high"|None`
- `estimate_minutes: int|None`

### 6.2 Reminder (explicit)
- `reminder_id: str`
- `task_id: str | None` — link to TaskItem (if created from a task)
- `title: str`
- `notes: str | None`
- `scheduled_for: str` — ISO datetime
- `status: "scheduled" | "dismissed" | "fired"`
- `created_at: str`
- `updated_at: str`

---

## 7. Stable Identity Strategy

Line numbers change. v0 must not rely on them.

### 7.1 v0 Task ID (good-enough)
Compute:

- Normalize task text: trim, collapse whitespace, lowercase
- Include `source_path`
- Include a **first-seen timestamp** (from index store) to prevent collisions

Example:
- `task_id = sha1(source_path + "\n" + normalized_text + "\n" + first_seen_iso)`

Store `first_seen_iso` in the index and reuse it when the same task is re-identified.

### 7.2 Re-identification heuristic (v0)
On re-index:
- Match by `(source_path, normalized_text)` first
- If multiple matches, prefer the nearest prior `source_line`

---

## 8. Task Syntax (v0)

### 8.1 Task markers
Recognize as tasks:

- GitHub-style checkboxes:
  - `- [ ] Task text`
  - `- [x] Done text`
- TODO prefix:
  - `TODO: Task text`
  - `TODO(Task): Task text`

### 8.2 Tags
Recognize tags in task text:
- `@tag` tokens (letters, numbers, `_`, `-`)

Examples:
- `- [ ] Email client @clientA`
- `TODO: buy milk @home`

### 8.3 Due dates (minimal)
Recognize either:
- `due:YYYY-MM-DD`
- `@due(YYYY-MM-DD)`

Examples:
- `- [ ] Submit invoice @clientA due:2026-01-10`
- `- [ ] Renew passport @due(2026-02-01)`

No natural-language date parsing in v0 unless user invokes agent explicitly.

---

## 9. Indexing & Refresh Rules

### 9.1 When to index
- On document open → index once
- On save → index
- Optional (config): debounced index on edit idle (e.g., 750ms) **only** if it does not affect typing performance

### 9.2 Scope
v0 supports two scopes:
- **Opened documents only** (default)
- **Workspace folder** (optional; recursive scan; opt-in)

### 9.3 Performance
- Parse incrementally where possible
- Cache per-document results keyed by `(path, last_modified_time, content_hash)`

---

## 10. Command Surface (v0)

Commands are the only way to act.

### 10.1 Master list
- `tasks.showPanel`
- `tasks.refreshIndex`
- `tasks.openSource(task_id)`

### 10.2 Reminders (explicit)
- `reminders.createFromSelection`
- `reminders.createFromTask(task_id)`
  - MUST show confirmation dialog (title, date/time)
- `reminders.list`
- `reminders.dismiss(reminder_id)`

### 10.3 Document mutation (explicit)
- `tasks.toggleDone(task_id)`
  - MUST be undoable
  - MUST modify only the checkbox marker if possible

### 10.4 Agent (explicit)
- `agent.summarizeOpenTasks`
- `agent.suggestDueDates(task_id|selection)`
- `agent.proposeReminderSchedule(task_id|selection)`
  - Outputs a draft; user must confirm to create reminders.

---

## 11. Agent Provider Behavior (v0)

### 11.1 Inputs
- Selection text OR TaskItem(s)
- Optional user constraints:
  - “this week”, “after work”, “no weekends”, etc.

### 11.2 Outputs (must be non-destructive)
- Suggested tags
- Suggested due date
- Suggested reminder time(s)
- A short rationale

### 11.3 Prohibitions
- Agent MUST NOT:
  - modify the document
  - create reminders
  - write files
  …unless invoked by a command and confirmed by the user.

---

## 12. Storage Requirements

- Must be offline-friendly
- Must be easy to back up
- Must be fast for local queries (filter by tag/due/status)
- Must support rebuildable cache + persisted reminders

Recommended v0:
- SQLite for index + reminders (single file per user/workspace)

---

## 13. UX Requirements (v0)

- Master tasks panel:
  - Filter: status, tag, due
  - Group by: file, due date, tag
  - Jump-to-source on click
- No interruptions while typing
- No auto-reminders

---

## 14. Testing Requirements

- Unit tests:
  - Task extraction rules (checkbox, TODO, tags, due)
  - ID stability across line moves
  - Index update on save
- Integration tests:
  - Two docs → one master list
  - Create reminder from task → appears in reminders list
  - Toggle done → updates derived index

---

## 15. Versioning

This document defines **Derived Tasks & Reminders System v0**.
v0 prioritizes trust, simplicity, and portability over “smartness”.
