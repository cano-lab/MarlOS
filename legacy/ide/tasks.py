"""Task extraction and indexing system.

This module provides:
- TaskExtractor: Finds tasks in markdown content using regex patterns
- TaskIndexStore: Persists tasks and reminders to SQLite

See TASK_REMINDERS_AGENT_SPEC.md for the full specification.
"""

import hashlib
import json
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
import re


class TaskExtractor:
    """Extract tasks from markdown content.

    Recognizes two task formats:
    1. Checkbox: - [ ] Task text  or  - [x] Completed
    2. TODO: TODO: Task text  or  TODO(label): Task text

    Also extracts metadata:
    - Tags: @tagname tokens
    - Due dates: due:YYYY-MM-DD or @due(YYYY-MM-DD)
    - Heading context: breadcrumb path to the task
    """

    def __init__(self):
        # Pattern: ^\s*[-*+]\s+\[( |x|X)\]\s+(.*)
        # Matches: - [ ] Task text  or  * [x] Done
        # Group 1: checkbox state (space = open, x/X = done)
        # Group 2: task text (rest of line)
        self.checkbox_pattern = re.compile(r"^\s*[-*+]\s+\[( |x|X)\]\s+(.*)")

        # Pattern: ^\s*TODO(?:\([^)]+\))?:\s+(.*)
        # Matches: TODO: text  or  TODO(label): text
        # Non-capturing group (?:...) matches optional (label) without capturing
        # Group 1: task text after colon
        self.todo_pattern = re.compile(r"^\s*TODO(?:\([^)]+\))?:\s+(.*)", re.I)

        # Pattern: @([A-Za-z0-9_-]+)
        # Matches: @tag @my-tag @tag_2
        # Group 1: tag name without @
        self.tag_pattern = re.compile(r"@([A-Za-z0-9_-]+)")

        # Pattern: due:(\d{4}-\d{2}-\d{2})
        # Matches: due:2026-01-15
        # Group 1: ISO date string
        self.due_pattern = re.compile(r"due:(\d{4}-\d{2}-\d{2})", re.I)

        # Pattern: @due\((\d{4}-\d{2}-\d{2})\)
        # Matches: @due(2026-01-15)
        # Group 1: ISO date string
        self.due_alt_pattern = re.compile(r"@due\((\d{4}-\d{2}-\d{2})\)", re.I)

        # Pattern: ^(#{1,6})\s+(.*)
        # Matches: # Heading  or  ## Subheading (up to 6 levels)
        # Group 1: hash characters (determines level)
        # Group 2: heading text
        self.heading_pattern = re.compile(r"^(#{1,6})\s+(.*)")

    def extract(self, content, source_path):
        """Extract all tasks from markdown content.

        Args:
            content: Raw markdown text to parse
            source_path: File path (used for task indexing)

        Returns:
            List of task dicts with keys:
            - text: Task description
            - status: "open" or "done"
            - source_path: File containing the task
            - source_line: 0-indexed line number
            - source_heading_path: List of heading breadcrumbs
            - tags: List of @tag values
            - due: ISO date string or None
        """
        tasks = []
        # heading_stack maintains the current heading breadcrumb
        # When we see ## Foo under # Bar, stack becomes ["Bar", "Foo"]
        heading_stack = []

        lines = content.splitlines()
        for line_index, line in enumerate(lines):
            heading_match = self.heading_pattern.match(line)
            if heading_match:
                level = len(heading_match.group(1))
                heading_text = heading_match.group(2).strip()
                heading_stack = heading_stack[: level - 1]
                heading_stack.append(heading_text)

            checkbox_match = self.checkbox_pattern.match(line)
            todo_match = self.todo_pattern.match(line)

            if checkbox_match:
                status = "done" if checkbox_match.group(1).lower() == "x" else "open"
                text = checkbox_match.group(2).strip()
            elif todo_match:
                status = "open"
                text = todo_match.group(1).strip()
            else:
                continue

            tags = self.tag_pattern.findall(text)
            due = None
            due_match = self.due_pattern.search(text) or self.due_alt_pattern.search(text)
            if due_match:
                due = due_match.group(1)

            tasks.append(
                {
                    "text": text,
                    "status": status,
                    "source_path": source_path,
                    "source_line": line_index,
                    "source_heading_path": list(heading_stack),
                    "tags": tags,
                    "due": due,
                }
            )

        return tasks


class TaskIndexStore:
    """SQLite-based storage for tasks and reminders.

    Provides persistent storage for the derived task index and user-created
    reminders. The task index is rebuildable from source documents; reminders
    represent explicit user intent and must be preserved.

    Database: tasks_index.db in the workspace root

    Tables:
    - tasks: Extracted tasks with stable IDs, status, metadata
    - reminders: User-created reminders linked to tasks or selections

    Stable ID Strategy:
    Task IDs are computed as SHA1(source_path + normalized_text + first_seen_iso)
    This ensures tasks maintain identity even when lines move.
    """

    def __init__(self, base_dir):
        """Initialize the task store.

        Args:
            base_dir: Directory for the database file
        """
        self.db_path = Path(base_dir) / "tasks_index.db"
        self._init_db()

    def _init_db(self):
        with sqlite3.connect(self.db_path) as conn:
            conn.execute(
                """
                CREATE TABLE IF NOT EXISTS tasks (
                    task_id TEXT PRIMARY KEY,
                    normalized_text TEXT NOT NULL,
                    text TEXT NOT NULL,
                    status TEXT NOT NULL,
                    source_path TEXT NOT NULL,
                    source_line INTEGER NOT NULL,
                    heading_path TEXT NOT NULL,
                    tags TEXT NOT NULL,
                    due TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    first_seen_iso TEXT NOT NULL
                )
                """
            )
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_tasks_source ON tasks(source_path)"
            )
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_tasks_norm ON tasks(source_path, normalized_text)"
            )
            conn.execute(
                """
                CREATE TABLE IF NOT EXISTS reminders (
                    reminder_id TEXT PRIMARY KEY,
                    task_id TEXT,
                    title TEXT NOT NULL,
                    notes TEXT,
                    scheduled_for TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                )
                """
            )

    def upsert_document_tasks(self, source_path, tasks):
        """Update the task index for a single document.

        This is the core indexing method. It:
        1. Matches new tasks against existing ones by normalized text
        2. Preserves stable IDs for matched tasks (even if line moved)
        3. Creates new IDs for genuinely new tasks
        4. Removes tasks that no longer exist in the document

        Args:
            source_path: File path of the document
            tasks: List of task dicts from TaskExtractor.extract()
        """
        now = _now_iso()
        normalized = [self._normalize(task["text"]) for task in tasks]

        with sqlite3.connect(self.db_path) as conn:
            conn.row_factory = sqlite3.Row
            existing = conn.execute(
                "SELECT * FROM tasks WHERE source_path = ?",
                (source_path,),
            ).fetchall()

            existing_by_norm = {}
            for row in existing:
                existing_by_norm.setdefault(row["normalized_text"], []).append(row)

            task_ids = []
            for task, norm_text in zip(tasks, normalized):
                match = self._match_existing(existing_by_norm.get(norm_text, []), task["source_line"])
                if match:
                    task_id = match["task_id"]
                    first_seen_iso = match["first_seen_iso"]
                    created_at = match["created_at"]
                else:
                    first_seen_iso = now
                    task_id = self._compute_task_id(source_path, norm_text, first_seen_iso)
                    created_at = now

                task_ids.append(task_id)
                conn.execute(
                    """
                    INSERT INTO tasks (
                        task_id, normalized_text, text, status, source_path, source_line,
                        heading_path, tags, due, created_at, updated_at, first_seen_iso
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(task_id) DO UPDATE SET
                        text = excluded.text,
                        status = excluded.status,
                        source_line = excluded.source_line,
                        heading_path = excluded.heading_path,
                        tags = excluded.tags,
                        due = excluded.due,
                        updated_at = excluded.updated_at
                    """,
                    (
                        task_id,
                        norm_text,
                        task["text"],
                        task["status"],
                        source_path,
                        task["source_line"],
                        json.dumps(task["source_heading_path"]),
                        json.dumps(task["tags"]),
                        task["due"],
                        created_at,
                        now,
                        first_seen_iso,
                    ),
                )

            if task_ids:
                placeholders = ",".join(["?"] * len(task_ids))
                conn.execute(
                    f"DELETE FROM tasks WHERE source_path = ? AND task_id NOT IN ({placeholders})",
                    (source_path, *task_ids),
                )
            else:
                conn.execute("DELETE FROM tasks WHERE source_path = ?", (source_path,))

    def list_tasks(self, status=None):
        query = "SELECT * FROM tasks"
        params = []
        if status in ("open", "done"):
            query += " WHERE status = ?"
            params.append(status)
        query += " ORDER BY source_path, source_line"
        with sqlite3.connect(self.db_path) as conn:
            conn.row_factory = sqlite3.Row
            rows = conn.execute(query, params).fetchall()
        return [self._row_to_task(row) for row in rows]

    def get_task(self, task_id):
        with sqlite3.connect(self.db_path) as conn:
            conn.row_factory = sqlite3.Row
            row = conn.execute(
                "SELECT * FROM tasks WHERE task_id = ?",
                (task_id,),
            ).fetchone()
            return self._row_to_task(row) if row else None

    def list_open_tasks(self):
        return self.list_tasks(status="open")

    def create_reminder(self, task_id, title, notes, scheduled_for):
        reminder_id = self._new_id("reminder")
        now = _now_iso()
        with sqlite3.connect(self.db_path) as conn:
            conn.execute(
                """
                INSERT INTO reminders (
                    reminder_id, task_id, title, notes, scheduled_for,
                    status, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    reminder_id,
                    task_id,
                    title,
                    notes,
                    scheduled_for,
                    "scheduled",
                    now,
                    now,
                ),
            )
        return reminder_id

    def list_reminders(self, status=None):
        query = "SELECT * FROM reminders"
        params = []
        if status:
            query += " WHERE status = ?"
            params.append(status)
        query += " ORDER BY scheduled_for"
        with sqlite3.connect(self.db_path) as conn:
            conn.row_factory = sqlite3.Row
            rows = conn.execute(query, params).fetchall()
        return [dict(row) for row in rows]

    def dismiss_reminder(self, reminder_id):
        now = _now_iso()
        with sqlite3.connect(self.db_path) as conn:
            conn.execute(
                """
                UPDATE reminders SET status = ?, updated_at = ?
                WHERE reminder_id = ?
                """,
                ("dismissed", now, reminder_id),
            )

    def _row_to_task(self, row):
        if not row:
            return None
        return {
            "task_id": row["task_id"],
            "normalized_text": row["normalized_text"],
            "text": row["text"],
            "status": row["status"],
            "source_path": row["source_path"],
            "source_line": row["source_line"],
            "source_heading_path": json.loads(row["heading_path"]),
            "tags": json.loads(row["tags"]),
            "due": row["due"],
            "created_at": row["created_at"],
            "updated_at": row["updated_at"],
            "first_seen_iso": row["first_seen_iso"],
        }

    def _normalize(self, text):
        return " ".join(text.lower().split())

    def _compute_task_id(self, source_path, normalized_text, first_seen_iso):
        data = f"{source_path}\n{normalized_text}\n{first_seen_iso}".encode("utf-8")
        return hashlib.sha1(data).hexdigest()

    def _match_existing(self, candidates, source_line):
        if not candidates:
            return None
        return min(
            candidates,
            key=lambda row: abs(row["source_line"] - source_line),
        )

    def _new_id(self, prefix):
        raw = f"{prefix}:{_now_iso()}".encode("utf-8")
        return hashlib.sha1(raw).hexdigest()


def _now_iso():
    return datetime.now(timezone.utc).isoformat()
