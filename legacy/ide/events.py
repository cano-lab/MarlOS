from PyQt6.QtCore import QObject, pyqtSignal
from datetime import datetime
from dataclasses import dataclass, field
from typing import Any, List, Optional
import uuid


@dataclass
class SpineEvent:
    """A recorded event on the spine."""
    event_id: str
    event_type: str
    timestamp: str
    payload: Any = None
    context_id: Optional[str] = None

    def to_dict(self):
        return {
            "event_id": self.event_id,
            "event_type": self.event_type,
            "timestamp": self.timestamp,
            "payload": str(self.payload) if self.payload else None,
            "context_id": self.context_id,
        }


class EventsSpine(QObject):
    """Local spine for a single document context.

    Records all events for episodic memory. Every state change
    flows through the spine and is logged for later querying.
    """

    # Signals (backwards compatible)
    document_changed = pyqtSignal(object)
    document_saved = pyqtSignal(object)
    cursor_moved = pyqtSignal(int)
    document_opened = pyqtSignal(object)
    document_closed = pyqtSignal(object)

    # New: generic event signal for custom events
    event_emitted = pyqtSignal(object)  # Emits SpineEvent

    def __init__(self, context_id: str = None):
        super().__init__()
        self.context_id = context_id or str(uuid.uuid4())[:8]
        self._history: List[SpineEvent] = []
        self._max_history = 1000  # Configurable limit

        # Auto-log when signals emit
        self.document_changed.connect(lambda d: self._log("document_changed", d))
        self.document_saved.connect(lambda d: self._log("document_saved", d))
        self.cursor_moved.connect(lambda pos: self._log("cursor_moved", pos))
        self.document_opened.connect(lambda d: self._log("document_opened", d))
        self.document_closed.connect(lambda d: self._log("document_closed", d))

    def _log(self, event_type: str, payload: Any = None):
        """Record an event to the history."""
        event = SpineEvent(
            event_id=str(uuid.uuid4())[:8],
            event_type=event_type,
            timestamp=datetime.now().isoformat(),
            payload=payload,
            context_id=self.context_id,
        )
        self._history.append(event)

        # Trim if over limit
        if len(self._history) > self._max_history:
            self._history = self._history[-self._max_history:]

        # Emit generic event signal
        self.event_emitted.emit(event)

    def emit_event(self, event_type: str, payload: Any = None):
        """Emit a custom event (ctx.emit equivalent)."""
        self._log(event_type, payload)

    @property
    def history(self) -> List[SpineEvent]:
        """Get the event history."""
        return self._history.copy()

    def query_history(
        self,
        event_type: str = None,
        since: str = None,
        limit: int = 100
    ) -> List[SpineEvent]:
        """Query the event history.

        Args:
            event_type: Filter by event type
            since: ISO timestamp to filter events after
            limit: Maximum events to return
        """
        result = self._history

        if event_type:
            result = [e for e in result if e.event_type == event_type]

        if since:
            result = [e for e in result if e.timestamp >= since]

        return result[-limit:]

    def get_narrative(self, limit: int = 20) -> str:
        """Get a human-readable narrative of recent events.

        Useful for AI context - answers "what happened recently?"
        """
        events = self._history[-limit:]
        if not events:
            return "No events recorded yet."

        lines = []
        for e in events:
            time = e.timestamp.split("T")[1].split(".")[0]  # HH:MM:SS
            lines.append(f"[{time}] {e.event_type}")

        return "\n".join(lines)

    def clear_history(self):
        """Clear the event history."""
        self._history = []
