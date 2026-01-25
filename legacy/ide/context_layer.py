"""
Context Layer for AI Assistant Context Layer

Provides context aggregation, workflow inference, focus detection,
and context export capabilities for AI assistants.
"""

import time
import json
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from typing import List, Dict, Any, Optional, Tuple
from collections import defaultdict
import re


@dataclass
class DocumentFocus:
    """Information about a focused document."""
    path: str
    focus_duration: float  # Minutes focused
    edit_count: int
    last_edited: float  # Timestamp
    sections_edited: List[str] = field(default_factory=list)
    semantic_tags: List[str] = field(default_factory=list)
    relations: List[str] = field(default_factory=list)

    def __post_init__(self):
        # Generate display name from path
        if self.path:
            self.display_name = self.path.split('/')[-1].split('\\')[-1]
        else:
            self.display_name = "Unknown"


@dataclass
class WorkItem:
    """A work item in the recent timeline."""
    timestamp: float
    work_type: str  # 'coding', 'writing', 'research', etc.
    files_count: int
    edit_count: int
    duration_minutes: float
    description: str


@dataclass
class Session:
    """A work session (grouped work with gaps < 30min)."""
    session_id: str
    start_time: float
    end_time: float
    documents_worked_on: List[str]
    dominant_project: str
    work_type: str
    document_count: int
    edit_count: int
    focus_duration: float  # Minutes


@dataclass
class Project:
    """An inferred project from file co-access patterns."""
    name: str
    core_files: List[str]  # Most frequently accessed
    peripheral_files: List[str]  # Less frequently accessed
    file_types: List[str]
    last_active: float
    inferred_tags: List[str] = field(default_factory=list)


@dataclass
class TemporalContext:
    """Time-based context information."""
    current_time: float
    time_of_day: str  # 'morning', 'afternoon', 'evening', 'night'
    day_of_week: str
    recent_time_window: str  # 'today', 'this_week', 'this_month'


@dataclass
class WorkflowContext:
    """Comprehensive workflow context for AI consumption."""
    current_focus: List[DocumentFocus]
    recent_work: List[WorkItem]
    workflow_patterns: List[str] = field(default_factory=list)
    related_documents: List[str] = field(default_factory=list)
    active_projects: List[Project] = field(default_factory=list)
    temporal_context: Optional[TemporalContext] = None
    generated_at: float = field(default_factory=time.time)


class FocusDetection:
    """Detects what the user is currently focused on."""

    def __init__(self, kernel):
        self.kernel = kernel
        self._focus_history: Dict[str, float] = defaultdict(float)  # path -> minutes
        self._edit_counts: Dict[str, int] = defaultdict(int)
        self._last_edit_time: Dict[str, float] = {}
        self._current_document: Optional[str] = None
        self._current_focus_start: float = 0

    def record_focus_start(self, doc_path: str):
        """Record when user starts focusing on a document."""
        now = time.time()

        # Save previous document's focus time
        if self._current_document and self._current_focus_start:
            duration = now - self._current_focus_start
            self._focus_history[self._current_document] += duration / 60  # Convert to minutes

        self._current_document = doc_path
        self._current_focus_start = now

    def record_edit(self, doc_path: str, section: Optional[str] = None):
        """Record an edit to a document."""
        self._edit_counts[doc_path] += 1
        self._last_edit_time[doc_path] = time.time()

    def get_current_focus(self, limit: int = 10) -> List[DocumentFocus]:
        """Get currently focused documents ranked by attention."""
        # Update current document's focus time
        if self._current_document and self._current_focus_start:
            duration = time.time() - self._current_focus_start
            self._focus_history[self._current_document] += duration / 60
            self._current_focus_start = time.time()  # Reset for next call

        # Score documents: (focus_time * 2) + edit_count + recency_bonus
        scores = []
        now = time.time()

        for path, focus_time in self._focus_history.items():
            edit_count = self._edit_counts.get(path, 0)
            last_edit = self._last_edit_time.get(path, 0)

            # Recency bonus: more recent edits get higher scores
            recency_hours = (now - last_edit) / 3600 if last_edit else 24
            recency_bonus = max(0, 10 - recency_hours)  # Decays over 10 hours

            score = (focus_time * 2) + edit_count + recency_bonus

            # Get relations from kernel
            relations = []
            if self.kernel:
                try:
                    # Get outgoing relations
                    rels = self.kernel.relations.get_outgoing(path, limit=3)
                    relations = [r.target for r in rels]
                except Exception:
                    pass

            scores.append(DocumentFocus(
                path=path,
                focus_duration=round(focus_time, 1),
                edit_count=edit_count,
                last_edited=last_edit,
                relations=relations
            ))

        # Sort by score and limit
        scores.sort(key=lambda x: (
            x.focus_duration * 2 + x.edit_count +
            (10 - (time.time() - x.last_edited) / 3600) if x.last_edited else 0
        ), reverse=True)

        return scores[:limit]


class ContextExporter:
    """Export context in various formats for AI consumption."""

    def to_markdown(self, context: WorkflowContext) -> str:
        """Format context as markdown (for Claude, ChatGPT)."""
        lines = []
        now = datetime.fromtimestamp(context.generated_at)

        lines.append(f"# User's Current Context ({now.strftime('%Y-%m-%d %H:%M')})")
        lines.append("")

        # Current Focus
        if context.current_focus:
            lines.append("## Current Focus")
            lines.append("The user is actively working on:")
            for doc in context.current_focus[:5]:
                focus_time = int(doc.focus_duration)
                lines.append(f"- **{doc.display_name}** ({focus_time}min focus, {doc.edit_count} edits)")
                if doc.relations:
                    lines.append(f"  - Related: {', '.join(doc.relations[:3])}")
            lines.append("")

        # Recent Work
        if context.recent_work:
            lines.append("## Recent Work (Last 24h)")
            for item in context.recent_work:
                time_str = datetime.fromtimestamp(item.timestamp).strftime("%H:%M")
                emoji = self._work_type_emoji(item.work_type)
                lines.append(f"{time_str} - {time_str}: {emoji} {item.work_type.title()} ({item.files_count} files, {item.edit_count} edits)")
            lines.append("")

        # Active Projects
        if context.active_projects:
            lines.append("## Active Projects")
            for i, proj in enumerate(context.active_projects, 1):
                status = "PRIMARY" if i == 1 else "Active"
                lines.append(f"{i}. **{proj.name}** ({status})")
                if proj.core_files:
                    core_names = [f.split('/')[-1].split('\\')[-1] for f in proj.core_files[:3]]
                    lines.append(f"   - Core: {', '.join(core_names)}")
                if proj.inferred_tags:
                    lines.append(f"   - Tags: {', '.join(proj.inferred_tags)}")
            lines.append("")

        # Related Documents
        if context.related_documents:
            lines.append("## Related Documents")
            for doc in context.related_documents[:5]:
                lines.append(f"- {doc}")
            lines.append("")

        # Workflow Patterns
        if context.workflow_patterns:
            lines.append("## Workflow Patterns")
            for pattern in context.workflow_patterns:
                lines.append(f"- {pattern}")
            lines.append("")

        # Temporal Context
        if context.temporal_context:
            tc = context.temporal_context
            lines.append("## Temporal Context")
            lines.append(f"- Time: {tc.time_of_day.title()}, {tc.day_of_week}")
            lines.append(f"- Window: {tc.recent_time_window.replace('_', ' ').title()}")
            lines.append("")

        return "\n".join(lines)

    def to_json(self, context: WorkflowContext) -> str:
        """Format context as JSON (for programmatic access)."""
        def convert_docfocus(doc):
            return {
                "path": doc.path,
                "display_name": doc.display_name,
                "focus_duration": doc.focus_duration,
                "edit_count": doc.edit_count,
                "last_edited": doc.last_edited,
                "sections_edited": doc.sections_edited,
                "semantic_tags": doc.semantic_tags,
                "relations": doc.relations
            }

        def convert_project(proj):
            return {
                "name": proj.name,
                "core_files": proj.core_files,
                "peripheral_files": proj.peripheral_files,
                "file_types": proj.file_types,
                "last_active": proj.last_active,
                "inferred_tags": proj.inferred_tags
            }

        data = {
            "current_focus": [convert_docfocus(d) for d in context.current_focus],
            "recent_work": [
                {
                    "timestamp": w.timestamp,
                    "work_type": w.work_type,
                    "files_count": w.files_count,
                    "edit_count": w.edit_count,
                    "duration_minutes": w.duration_minutes,
                    "description": w.description
                }
                for w in context.recent_work
            ],
            "workflow_patterns": context.workflow_patterns,
            "related_documents": context.related_documents,
            "active_projects": [convert_project(p) for p in context.active_projects],
            "generated_at": context.generated_at
        }

        if context.temporal_context:
            data["temporal_context"] = {
                "current_time": context.temporal_context.current_time,
                "time_of_day": context.temporal_context.time_of_day,
                "day_of_week": context.temporal_context.day_of_week,
                "recent_time_window": context.temporal_context.recent_time_window
            }

        return json.dumps(data, indent=2)

    def to_openai_context(self, context: WorkflowContext) -> List[Dict[str, str]]:
        """Format as OpenAI chat messages."""
        return [
            {
                "role": "system",
                "content": f"You are an AI assistant with context about the user's work.\n\n{self.to_compact_markdown(context)}"
            }
        ]

    def to_compact_markdown(self, context: WorkflowContext) -> str:
        """Format as compact markdown for inline context."""
        lines = []
        lines.append("## User Context")

        if context.current_focus:
            focus_items = []
            for doc in context.current_focus[:3]:
                focus_items.append(f"{doc.display_name} ({int(doc.focus_duration)}min)")
            lines.append(f"**Working on**: {', '.join(focus_items)}")

        if context.active_projects:
            lines.append(f"**Project**: {context.active_projects[0].name}")

        if context.recent_work:
            recent = context.recent_work[0]
            lines.append(f"**Last activity**: {recent.work_type} ({recent.files_count} files)")

        return "\n".join(lines)

    def _work_type_emoji(self, work_type: str) -> str:
        """Get emoji for work type."""
        emojis = {
            "coding": "",
            "writing": "",
            "research": "",
            "review": "",
            "debugging": "",
            "documentation": "",
            "testing": "",
            "meeting": "",
        }
        return emojis.get(work_type.lower(), "")


class WorkflowInference:
    """Infer workflow patterns from semantic data."""

    SESSION_GAP_MINUTES = 30  # Gap > 30min = new session

    def __init__(self, kernel):
        self.kernel = kernel

    def detect_sessions(self, events: List[Dict]) -> List[Session]:
        """Detect work sessions from event stream."""
        if not events:
            return []

        # Sort events by timestamp
        sorted_events = sorted(events, key=lambda e: e.get("timestamp", 0))

        sessions = []
        current_session_events = []
        session_id = 0

        for event in sorted_events:
            timestamp = event.get("timestamp", 0)

            # Check if gap > SESSION_GAP_MINUTES
            if current_session_events:
                last_time = current_session_events[-1].get("timestamp", 0)
                gap_minutes = (timestamp - last_time) / 60

                if gap_minutes > self.SESSION_GAP_MINUTES:
                    # Finalize current session
                    session = self._create_session(
                        f"session_{session_id}",
                        current_session_events
                    )
                    if session:
                        sessions.append(session)
                        session_id += 1

                    # Start new session
                    current_session_events = []

            current_session_events.append(event)

        # Don't forget the last session
        if current_session_events:
            session = self._create_session(
                f"session_{session_id}",
                current_session_events
            )
            if session:
                sessions.append(session)

        return sessions

    def _create_session(self, session_id: str, events: List[Dict]) -> Optional[Session]:
        """Create a Session from a list of events."""
        if not events:
            return None

        start_time = min(e.get("timestamp", 0) for e in events)
        end_time = max(e.get("timestamp", 0) for e in events)

        # Collect unique documents
        documents = set()
        edit_count = 0
        for event in events:
            if "path" in event:
                documents.add(event["path"])
            if event.get("type") == "edit":
                edit_count += 1

        duration = (end_time - start_time) / 60  # Minutes

        # Infer work type from events
        work_type = self._infer_work_type(events)

        # Infer dominant project
        dominant_project = self._infer_project_from_docs(list(documents))

        return Session(
            session_id=session_id,
            start_time=start_time,
            end_time=end_time,
            documents_worked_on=list(documents),
            dominant_project=dominant_project,
            work_type=work_type,
            document_count=len(documents),
            edit_count=edit_count,
            focus_duration=round(duration, 1)
        )

    def _infer_work_type(self, events: List[Dict]) -> str:
        """Infer work type from event patterns."""
        # Count event types
        edit_count = sum(1 for e in events if e.get("type") == "edit")
        file_count = len(set(e.get("path") for e in events if "path" in e))

        # Simple heuristic
        if file_count > 5:
            return "research"
        elif edit_count > 10:
            return "coding"
        elif edit_count > 0:
            return "writing"
        else:
            return "review"

    def _infer_project_from_docs(self, docs: List[str]) -> str:
        """Infer project name from document paths."""
        if not docs:
            return "Unknown"

        # Try to find common directory
        paths = [doc.replace("\\", "/").split("/") for doc in docs]

        # Find common prefix
        common = []
        for parts in zip(*paths):
            if len(set(parts)) == 1:
                common.append(parts[0])
            else:
                break

        if common:
            return common[-1] if common else "Unknown"

        # Fallback: use first doc's directory
        if paths:
            return paths[0][-2] if len(paths[0]) > 1 else "Unknown"

        return "Unknown"

    def infer_projects(self, file_access: Dict[str, int]) -> List[Project]:
        """Infer projects from file access patterns.

        Args:
            file_access: Dict mapping file path -> access count
        """
        if not file_access:
            return []

        # Group files by directory
        dir_groups = defaultdict(list)
        file_types = set()

        for path, count in file_access.items():
            # Get directory
            parts = path.replace("\\", "/").split("/")
            if len(parts) > 1:
                directory = parts[-2]
            else:
                directory = "root"

            dir_groups[directory].append((path, count))

            # Get file extension
            if "." in parts[-1]:
                ext = parts[-1].split(".")[-1]
                file_types.add(ext)

        # Create projects
        projects = []
        for dir_name, files in dir_groups.items():
            # Sort by access count
            files.sort(key=lambda x: x[1], reverse=True)

            # Split into core and peripheral
            total = len(files)
            core_threshold = max(1, total // 3)

            core = [f[0] for f in files[:core_threshold]]
            peripheral = [f[0] for f in files[core_threshold:]]

            # Infer project name from directory
            project_name = dir_name.replace("_", " ").replace("-", " ").title()

            projects.append(Project(
                name=project_name,
                core_files=core,
                peripheral_files=peripheral,
                file_types=list(file_types),
                last_active=time.time(),
                inferred_tags=["active"]
            ))

        # Sort by size (larger projects first)
        projects.sort(key=lambda p: len(p.core_files), reverse=True)

        return projects


class ContextAggregator:
    """Aggregates semantic data into AI-ready context packages."""

    def __init__(self, kernel):
        self.kernel = kernel
        self.focus_detection = FocusDetection(kernel)
        self.workflow_inference = WorkflowInference(kernel)
        self.exporter = ContextExporter()

        # Cache
        self._cache: Dict[str, Tuple[WorkflowContext, float]] = {}
        self._cache_ttl = 300  # 5 minutes

    def record_focus(self, doc_path: str):
        """Record that user is focusing on a document."""
        self.focus_detection.record_focus_start(doc_path)

    def record_edit(self, doc_path: str, section: Optional[str] = None):
        """Record an edit to a document."""
        self.focus_detection.record_edit(doc_path, section)

    def get_current_focus(self, limit: int = 10) -> List[DocumentFocus]:
        """Get currently focused documents."""
        return self.focus_detection.get_current_focus(limit)

    def get_workflow_context(self, time_window: str = "today") -> WorkflowContext:
        """Get comprehensive workflow context for time period.

        Args:
            time_window: 'today', 'this_week', 'this_month', or 'all'
        """
        # Check cache
        cache_key = f"{time_window}_{time.time() // 60}"  # Cache per minute
        if cache_key in self._cache:
            context, cached_time = self._cache[cache_key]
            if time.time() - cached_time < self._cache_ttl:
                return context

        # Get current focus
        current_focus = self.focus_detection.get_current_focus(limit=10)

        # Get recent work from kernel
        recent_work = self._get_recent_work(time_window)

        # Get related documents
        related_docs = self._get_related_documents(current_focus)

        # Infer projects
        projects = self._infer_projects_from_events()

        # Detect workflow patterns
        patterns = self._detect_patterns()

        # Temporal context
        temporal = self._create_temporal_context()

        # Create context
        context = WorkflowContext(
            current_focus=current_focus,
            recent_work=recent_work,
            workflow_patterns=patterns,
            related_documents=related_docs,
            active_projects=projects,
            temporal_context=temporal
        )

        # Cache it
        self._cache[cache_key] = (context, time.time())

        return context

    def export_context(self, format: str = "markdown", time_window: str = "today") -> str:
        """Export context in specified format.

        Args:
            format: 'markdown', 'json', 'compact'
            time_window: 'today', 'this_week', 'this_month', or 'all'
        """
        context = self.get_workflow_context(time_window)

        if format == "markdown":
            return self.exporter.to_markdown(context)
        elif format == "json":
            return self.exporter.to_json(context)
        elif format == "compact":
            return self.exporter.to_compact_markdown(context)
        else:
            raise ValueError(f"Unknown format: {format}")

    def _get_recent_work(self, time_window: str) -> List[WorkItem]:
        """Get recent work items from kernel."""
        # Calculate time range
        now = time.time()
        if time_window == "today":
            start_time = now - (24 * 3600)
        elif time_window == "this_week":
            start_time = now - (7 * 24 * 3600)
        elif time_window == "this_month":
            start_time = now - (30 * 24 * 3600)
        else:  # 'all'
            start_time = 0

        # Try to get events from kernel
        try:
            events = self.kernel.get_events(limit=1000)
            recent_events = [
                e for e in events
                if e.get("timestamp", 0) >= start_time
            ]

            # Group into sessions
            sessions = self.workflow_inference.detect_sessions(recent_events)

            # Convert to WorkItems
            work_items = []
            for session in sessions:
                work_items.append(WorkItem(
                    timestamp=session.start_time,
                    work_type=session.work_type,
                    files_count=session.document_count,
                    edit_count=session.edit_count,
                    duration_minutes=session.focus_duration,
                    description=f"{session.work_type.title()} on {session.dominant_project}"
                ))

            return work_items

        except Exception:
            # Fallback: return empty list
            return []

    def _get_related_documents(self, focus: List[DocumentFocus]) -> List[str]:
        """Get related documents from current focus."""
        related = set()
        for doc in focus[:5]:  # Top 5 focused docs
            related.update(doc.relations)
        return list(related)[:10]

    def _infer_projects_from_events(self) -> List[Project]:
        """Infer projects from recent events."""
        try:
            events = self.kernel.get_events(limit=500)

            # Count file accesses
            file_access = defaultdict(int)
            for event in events:
                if "path" in event:
                    file_access[event["path"]] += 1

            return self.workflow_inference.infer_projects(file_access)

        except Exception:
            return []

    def _detect_patterns(self) -> List[str]:
        """Detect workflow patterns."""
        patterns = []

        # Simple pattern detection for now
        try:
            events = self.kernel.get_events(limit=100)

            # Check for edit-then-test pattern
            edit_events = [e for e in events if e.get("type") == "edit"]
            if len(edit_events) > 5:
                patterns.append("Frequent editing sessions")

            # Check for multi-file work
            unique_files = len(set(e.get("path") for e in events if "path" in e))
            if unique_files > 3:
                patterns.append("Works across multiple files")

        except Exception:
            pass

        return patterns

    def _create_temporal_context(self) -> TemporalContext:
        """Create temporal context information."""
        now = datetime.now()
        current_time = time.time()

        # Time of day
        hour = now.hour
        if 5 <= hour < 12:
            time_of_day = "morning"
        elif 12 <= hour < 17:
            time_of_day = "afternoon"
        elif 17 <= hour < 21:
            time_of_day = "evening"
        else:
            time_of_day = "night"

        # Day of week
        day_of_week = now.strftime("%A")

        return TemporalContext(
            current_time=current_time,
            time_of_day=time_of_day,
            day_of_week=day_of_week,
            recent_time_window="today"
        )
