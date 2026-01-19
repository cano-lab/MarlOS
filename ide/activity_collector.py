"""
System Activity Collector
=========================

Collects raw activity data from the OS and feeds it into semantic memory.
Captures data from proprietary software through system-level observation.

Data Streams:
- Active window tracking (title, app, focus time)
- File system events (create, modify, delete, rename)
- Clipboard changes (text, images, files)
- Process lifecycle (launch, exit)
- Audio capture for voice notes (optional)

All events are stored as semantic memory entries for later querying.
"""

import time
import threading
import hashlib
from datetime import datetime
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Callable, Any
from pathlib import Path
from enum import Enum
import json

# Windows APIs
import ctypes
from ctypes import wintypes
import ctypes.wintypes

# Third-party (with fallbacks)
try:
    import psutil
    HAS_PSUTIL = True
except ImportError:
    HAS_PSUTIL = False
    print("[ActivityCollector] psutil not available - process monitoring disabled")

try:
    from watchdog.observers import Observer
    from watchdog.events import FileSystemEventHandler, FileSystemEvent
    HAS_WATCHDOG = True
except ImportError:
    HAS_WATCHDOG = False
    print("[ActivityCollector] watchdog not available - file monitoring disabled")


class ActivityType(Enum):
    """Types of system activities we track."""
    WINDOW_FOCUS = "window_focus"
    WINDOW_TITLE_CHANGE = "window_title_change"
    FILE_CREATED = "file_created"
    FILE_MODIFIED = "file_modified"
    FILE_DELETED = "file_deleted"
    FILE_RENAMED = "file_renamed"
    CLIPBOARD_TEXT = "clipboard_text"
    CLIPBOARD_IMAGE = "clipboard_image"
    CLIPBOARD_FILES = "clipboard_files"
    PROCESS_STARTED = "process_started"
    PROCESS_ENDED = "process_ended"
    AUDIO_NOTE = "audio_note"
    USER_IDLE = "user_idle"
    USER_ACTIVE = "user_active"


@dataclass
class ActivityEvent:
    """A single activity event."""
    type: ActivityType
    timestamp: float
    data: Dict[str, Any]
    source: str = ""  # App or source that generated this

    def to_dict(self) -> Dict:
        return {
            "type": self.type.value,
            "timestamp": self.timestamp,
            "datetime": datetime.fromtimestamp(self.timestamp).isoformat(),
            "data": self.data,
            "source": self.source,
        }

    def to_content(self) -> str:
        """Convert to searchable content string."""
        parts = [f"[{self.type.value}]"]

        if self.source:
            parts.append(f"App: {self.source}")

        for key, value in self.data.items():
            if isinstance(value, str) and len(value) < 500:
                parts.append(f"{key}: {value}")
            elif isinstance(value, (int, float)):
                parts.append(f"{key}: {value}")

        return "\n".join(parts)


# ============== Windows API Definitions ==============

user32 = ctypes.windll.user32
kernel32 = ctypes.windll.kernel32

# Window tracking
GetForegroundWindow = user32.GetForegroundWindow
GetWindowTextW = user32.GetWindowTextW
GetWindowTextLengthW = user32.GetWindowTextLengthW
GetWindowThreadProcessId = user32.GetWindowThreadProcessId

# Clipboard
OpenClipboard = user32.OpenClipboard
CloseClipboard = user32.CloseClipboard
GetClipboardData = user32.GetClipboardData
IsClipboardFormatAvailable = user32.IsClipboardFormatAvailable
GetClipboardSequenceNumber = user32.GetClipboardSequenceNumber
EnumClipboardFormats = user32.EnumClipboardFormats

# Clipboard formats
CF_TEXT = 1
CF_UNICODETEXT = 13
CF_HDROP = 15  # File list
CF_DIB = 8  # Bitmap

# For getting process name from PID
OpenProcess = kernel32.OpenProcess
CloseHandle = kernel32.CloseHandle
PROCESS_QUERY_INFORMATION = 0x0400
PROCESS_VM_READ = 0x0010

# Idle time detection
class LASTINPUTINFO(ctypes.Structure):
    _fields_ = [
        ('cbSize', wintypes.UINT),
        ('dwTime', wintypes.DWORD),
    ]

GetLastInputInfo = user32.GetLastInputInfo
GetTickCount = kernel32.GetTickCount


def get_window_title(hwnd: int) -> str:
    """Get window title from handle."""
    length = GetWindowTextLengthW(hwnd)
    if length == 0:
        return ""

    buffer = ctypes.create_unicode_buffer(length + 1)
    GetWindowTextW(hwnd, buffer, length + 1)
    return buffer.value


def get_process_name(pid: int) -> str:
    """Get process name from PID."""
    if not HAS_PSUTIL:
        return f"PID:{pid}"

    try:
        proc = psutil.Process(pid)
        return proc.name()
    except (psutil.NoSuchProcess, psutil.AccessDenied):
        return f"PID:{pid}"


def get_process_path(pid: int) -> str:
    """Get process executable path from PID."""
    if not HAS_PSUTIL:
        return ""

    try:
        proc = psutil.Process(pid)
        return proc.exe()
    except (psutil.NoSuchProcess, psutil.AccessDenied):
        return ""


def get_idle_time() -> float:
    """Get user idle time in seconds."""
    lii = LASTINPUTINFO()
    lii.cbSize = ctypes.sizeof(LASTINPUTINFO)

    if GetLastInputInfo(ctypes.byref(lii)):
        millis = GetTickCount() - lii.dwTime
        return millis / 1000.0

    return 0.0


# ============== File System Monitor ==============

if HAS_WATCHDOG:
    class FileActivityHandler(FileSystemEventHandler):
        """Handles file system events."""

        def __init__(self, callback: Callable[[ActivityEvent], None]):
            super().__init__()
            self.callback = callback

        def _create_event(self, event_type: ActivityType, event: FileSystemEvent) -> ActivityEvent:
            return ActivityEvent(
                type=event_type,
                timestamp=time.time(),
                data={
                    "path": event.src_path,
                    "is_directory": event.is_directory,
                    "filename": Path(event.src_path).name,
                    "extension": Path(event.src_path).suffix.lower(),
                },
                source="filesystem",
            )

        def on_created(self, event: FileSystemEvent):
            if not event.is_directory:
                self.callback(self._create_event(ActivityType.FILE_CREATED, event))

        def on_modified(self, event: FileSystemEvent):
            if not event.is_directory:
                self.callback(self._create_event(ActivityType.FILE_MODIFIED, event))

        def on_deleted(self, event: FileSystemEvent):
            self.callback(self._create_event(ActivityType.FILE_DELETED, event))

        def on_moved(self, event: FileSystemEvent):
            activity = ActivityEvent(
                type=ActivityType.FILE_RENAMED,
                timestamp=time.time(),
                data={
                    "old_path": event.src_path,
                    "new_path": event.dest_path,
                    "is_directory": event.is_directory,
                    "filename": Path(event.dest_path).name,
                },
                source="filesystem",
            )
            self.callback(activity)


# ============== Main Activity Collector ==============

class ActivityCollector:
    """
    Collects system activity and feeds it to semantic memory.

    Usage:
        collector = ActivityCollector(kernel)
        collector.start()
        # ... app runs ...
        collector.stop()
    """

    def __init__(self, kernel=None, db_path: str = None):
        """
        Initialize the activity collector.

        Args:
            kernel: SemanticKernel instance (optional)
            db_path: Path to store activity database (optional)
        """
        self.kernel = kernel
        self.db_path = db_path

        self._running = False
        self._threads: List[threading.Thread] = []

        # Event callbacks
        self._callbacks: List[Callable[[ActivityEvent], None]] = []

        # State tracking
        self._last_window_hwnd = 0
        self._last_window_title = ""
        self._last_window_time = 0.0
        self._last_clipboard_seq = 0
        self._known_processes: Dict[int, str] = {}
        self._last_idle_state = False

        # File system observer
        self._fs_observer: Optional[Observer] = None
        self._watched_paths: List[str] = []

        # Activity log (in-memory buffer)
        self._event_buffer: List[ActivityEvent] = []
        self._buffer_lock = threading.Lock()
        self._max_buffer_size = 1000

        # Polling intervals (seconds)
        self.window_poll_interval = 0.5
        self.clipboard_poll_interval = 1.0
        self.process_poll_interval = 5.0
        self.idle_poll_interval = 10.0
        self.idle_threshold = 300.0  # 5 minutes

    def add_callback(self, callback: Callable[[ActivityEvent], None]):
        """Add a callback for activity events."""
        self._callbacks.append(callback)

    def remove_callback(self, callback: Callable[[ActivityEvent], None]):
        """Remove a callback."""
        if callback in self._callbacks:
            self._callbacks.remove(callback)

    def watch_directory(self, path: str, recursive: bool = True):
        """Add a directory to watch for file changes."""
        self._watched_paths.append((path, recursive))

    def start(self):
        """Start collecting activity."""
        if self._running:
            return

        self._running = True
        print("[ActivityCollector] Starting activity collection...")

        # Initialize process list
        if HAS_PSUTIL:
            for proc in psutil.process_iter(['pid', 'name']):
                try:
                    self._known_processes[proc.info['pid']] = proc.info['name']
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    pass

        # Start window tracking thread
        t = threading.Thread(target=self._window_monitor_loop, daemon=True)
        t.start()
        self._threads.append(t)

        # Start clipboard tracking thread
        t = threading.Thread(target=self._clipboard_monitor_loop, daemon=True)
        t.start()
        self._threads.append(t)

        # Start process tracking thread
        if HAS_PSUTIL:
            t = threading.Thread(target=self._process_monitor_loop, daemon=True)
            t.start()
            self._threads.append(t)

        # Start idle detection thread
        t = threading.Thread(target=self._idle_monitor_loop, daemon=True)
        t.start()
        self._threads.append(t)

        # Start file system observer
        if HAS_WATCHDOG and self._watched_paths:
            self._fs_observer = Observer()
            handler = FileActivityHandler(self._emit_event)

            for path, recursive in self._watched_paths:
                if Path(path).exists():
                    self._fs_observer.schedule(handler, path, recursive=recursive)
                    print(f"[ActivityCollector] Watching: {path}")

            self._fs_observer.start()

        print("[ActivityCollector] Activity collection started")

    def stop(self):
        """Stop collecting activity."""
        if not self._running:
            return

        print("[ActivityCollector] Stopping activity collection...")
        self._running = False

        # Stop file system observer
        if self._fs_observer:
            self._fs_observer.stop()
            self._fs_observer.join(timeout=2.0)
            self._fs_observer = None

        # Wait for threads to finish
        for t in self._threads:
            t.join(timeout=1.0)

        self._threads.clear()
        print("[ActivityCollector] Activity collection stopped")

    def _emit_event(self, event: ActivityEvent):
        """Emit an activity event."""
        # Add to buffer
        with self._buffer_lock:
            self._event_buffer.append(event)

            # Trim buffer if too large
            if len(self._event_buffer) > self._max_buffer_size:
                self._event_buffer = self._event_buffer[-self._max_buffer_size:]

        # Store in semantic memory
        if self.kernel:
            try:
                from kernel.memory import MemoryType
                self.kernel.memory.store(
                    content=event.to_content(),
                    type=MemoryType.EVENT,
                    metadata={
                        "activity_type": event.type.value,
                        "source": event.source,
                        "timestamp": event.timestamp,
                        **event.data,
                    },
                )
            except Exception as e:
                print(f"[ActivityCollector] Failed to store event: {e}")

        # Notify callbacks
        for callback in self._callbacks:
            try:
                callback(event)
            except Exception as e:
                print(f"[ActivityCollector] Callback error: {e}")

    def _window_monitor_loop(self):
        """Monitor active window changes."""
        while self._running:
            try:
                hwnd = GetForegroundWindow()

                if hwnd and hwnd != self._last_window_hwnd:
                    # Window changed
                    title = get_window_title(hwnd)

                    # Get process info
                    pid = wintypes.DWORD()
                    GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                    process_name = get_process_name(pid.value)
                    process_path = get_process_path(pid.value)

                    # Calculate time spent on previous window
                    now = time.time()
                    duration = now - self._last_window_time if self._last_window_time > 0 else 0

                    # Emit focus change event
                    event = ActivityEvent(
                        type=ActivityType.WINDOW_FOCUS,
                        timestamp=now,
                        data={
                            "window_title": title,
                            "process_name": process_name,
                            "process_path": process_path,
                            "pid": pid.value,
                            "hwnd": hwnd,
                            "previous_title": self._last_window_title,
                            "previous_duration": round(duration, 1),
                        },
                        source=process_name,
                    )
                    self._emit_event(event)

                    self._last_window_hwnd = hwnd
                    self._last_window_title = title
                    self._last_window_time = now

                elif hwnd == self._last_window_hwnd:
                    # Check if title changed (same window)
                    title = get_window_title(hwnd)

                    if title != self._last_window_title and title:
                        pid = wintypes.DWORD()
                        GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                        process_name = get_process_name(pid.value)

                        event = ActivityEvent(
                            type=ActivityType.WINDOW_TITLE_CHANGE,
                            timestamp=time.time(),
                            data={
                                "old_title": self._last_window_title,
                                "new_title": title,
                                "process_name": process_name,
                                "pid": pid.value,
                            },
                            source=process_name,
                        )
                        self._emit_event(event)

                        self._last_window_title = title

            except Exception as e:
                print(f"[ActivityCollector] Window monitor error: {e}")

            time.sleep(self.window_poll_interval)

    def _clipboard_monitor_loop(self):
        """Monitor clipboard changes."""
        # Initialize sequence number
        self._last_clipboard_seq = GetClipboardSequenceNumber()

        while self._running:
            try:
                seq = GetClipboardSequenceNumber()

                if seq != self._last_clipboard_seq:
                    self._last_clipboard_seq = seq

                    # Try to get clipboard content
                    if OpenClipboard(0):
                        try:
                            # Check for text
                            if IsClipboardFormatAvailable(CF_UNICODETEXT):
                                handle = GetClipboardData(CF_UNICODETEXT)
                                if handle:
                                    # Get text from handle
                                    text_ptr = kernel32.GlobalLock(handle)
                                    if text_ptr:
                                        text = ctypes.wstring_at(text_ptr)
                                        kernel32.GlobalUnlock(handle)

                                        # Emit clipboard event
                                        event = ActivityEvent(
                                            type=ActivityType.CLIPBOARD_TEXT,
                                            timestamp=time.time(),
                                            data={
                                                "text": text[:1000],  # Limit size
                                                "length": len(text),
                                                "hash": hashlib.md5(text.encode()).hexdigest()[:8],
                                            },
                                            source="clipboard",
                                        )
                                        self._emit_event(event)

                            # Check for files
                            elif IsClipboardFormatAvailable(CF_HDROP):
                                event = ActivityEvent(
                                    type=ActivityType.CLIPBOARD_FILES,
                                    timestamp=time.time(),
                                    data={"note": "Files copied to clipboard"},
                                    source="clipboard",
                                )
                                self._emit_event(event)

                            # Check for image
                            elif IsClipboardFormatAvailable(CF_DIB):
                                event = ActivityEvent(
                                    type=ActivityType.CLIPBOARD_IMAGE,
                                    timestamp=time.time(),
                                    data={"note": "Image copied to clipboard"},
                                    source="clipboard",
                                )
                                self._emit_event(event)

                        finally:
                            CloseClipboard()

            except Exception as e:
                print(f"[ActivityCollector] Clipboard monitor error: {e}")

            time.sleep(self.clipboard_poll_interval)

    def _process_monitor_loop(self):
        """Monitor process starts and exits."""
        while self._running:
            try:
                current_processes = {}

                for proc in psutil.process_iter(['pid', 'name', 'create_time']):
                    try:
                        info = proc.info
                        current_processes[info['pid']] = info['name']

                        # Check for new process
                        if info['pid'] not in self._known_processes:
                            try:
                                exe_path = proc.exe()
                                cmdline = ' '.join(proc.cmdline()[:5])  # First 5 args
                            except (psutil.NoSuchProcess, psutil.AccessDenied):
                                exe_path = ""
                                cmdline = ""

                            event = ActivityEvent(
                                type=ActivityType.PROCESS_STARTED,
                                timestamp=time.time(),
                                data={
                                    "pid": info['pid'],
                                    "name": info['name'],
                                    "path": exe_path,
                                    "cmdline": cmdline[:200],
                                },
                                source=info['name'],
                            )
                            self._emit_event(event)

                    except (psutil.NoSuchProcess, psutil.AccessDenied):
                        pass

                # Check for ended processes
                for pid, name in list(self._known_processes.items()):
                    if pid not in current_processes:
                        event = ActivityEvent(
                            type=ActivityType.PROCESS_ENDED,
                            timestamp=time.time(),
                            data={
                                "pid": pid,
                                "name": name,
                            },
                            source=name,
                        )
                        self._emit_event(event)

                self._known_processes = current_processes

            except Exception as e:
                print(f"[ActivityCollector] Process monitor error: {e}")

            time.sleep(self.process_poll_interval)

    def _idle_monitor_loop(self):
        """Monitor user idle/active state."""
        while self._running:
            try:
                idle_time = get_idle_time()
                is_idle = idle_time > self.idle_threshold

                if is_idle != self._last_idle_state:
                    if is_idle:
                        event = ActivityEvent(
                            type=ActivityType.USER_IDLE,
                            timestamp=time.time(),
                            data={
                                "idle_seconds": round(idle_time, 1),
                            },
                            source="system",
                        )
                    else:
                        event = ActivityEvent(
                            type=ActivityType.USER_ACTIVE,
                            timestamp=time.time(),
                            data={
                                "was_idle_seconds": round(idle_time, 1),
                            },
                            source="system",
                        )

                    self._emit_event(event)
                    self._last_idle_state = is_idle

            except Exception as e:
                print(f"[ActivityCollector] Idle monitor error: {e}")

            time.sleep(self.idle_poll_interval)

    def get_recent_events(self, limit: int = 100, event_type: ActivityType = None) -> List[ActivityEvent]:
        """Get recent events from buffer."""
        with self._buffer_lock:
            events = self._event_buffer[-limit:]

            if event_type:
                events = [e for e in events if e.type == event_type]

            return events

    def get_window_history(self, hours: float = 1.0) -> List[Dict]:
        """Get window focus history."""
        cutoff = time.time() - (hours * 3600)

        with self._buffer_lock:
            return [
                e.to_dict() for e in self._event_buffer
                if e.type == ActivityType.WINDOW_FOCUS and e.timestamp > cutoff
            ]

    def get_app_usage(self, hours: float = 1.0) -> Dict[str, float]:
        """Get app usage time in seconds."""
        window_events = self.get_window_history(hours)

        usage = {}
        for i, event in enumerate(window_events):
            app = event['data'].get('process_name', 'Unknown')
            duration = event['data'].get('previous_duration', 0)

            if app not in usage:
                usage[app] = 0
            usage[app] += duration

        return dict(sorted(usage.items(), key=lambda x: -x[1]))

    def get_clipboard_history(self, limit: int = 50) -> List[Dict]:
        """Get clipboard history."""
        with self._buffer_lock:
            clipboard_events = [
                e.to_dict() for e in self._event_buffer
                if e.type in (ActivityType.CLIPBOARD_TEXT, ActivityType.CLIPBOARD_IMAGE, ActivityType.CLIPBOARD_FILES)
            ]
            return clipboard_events[-limit:]


# ============== UI Widget for Activity Monitor ==============

def create_activity_panel(collector: ActivityCollector):
    """Create a Qt widget to display activity."""
    from PyQt6.QtWidgets import (
        QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
        QTableWidget, QTableWidgetItem, QGroupBox, QTabWidget,
        QHeaderView
    )
    from PyQt6.QtCore import Qt, QTimer

    class ActivityPanel(QWidget):
        def __init__(self, collector: ActivityCollector):
            super().__init__()
            self.collector = collector
            self.setup_ui()

            # Update timer
            self.timer = QTimer()
            self.timer.timeout.connect(self.refresh)
            self.timer.start(2000)  # Update every 2 seconds

        def setup_ui(self):
            layout = QVBoxLayout(self)

            # Controls
            controls = QHBoxLayout()

            self.status_label = QLabel("Status: Stopped")
            controls.addWidget(self.status_label)

            controls.addStretch()

            self.start_btn = QPushButton("Start")
            self.start_btn.clicked.connect(self.toggle_collection)
            controls.addWidget(self.start_btn)

            layout.addLayout(controls)

            # Tabs
            tabs = QTabWidget()

            # Recent Activity tab
            activity_widget = QWidget()
            activity_layout = QVBoxLayout(activity_widget)

            self.activity_table = QTableWidget()
            self.activity_table.setColumnCount(4)
            self.activity_table.setHorizontalHeaderLabels(["Time", "Type", "Source", "Details"])
            self.activity_table.horizontalHeader().setSectionResizeMode(3, QHeaderView.ResizeMode.Stretch)
            activity_layout.addWidget(self.activity_table)

            tabs.addTab(activity_widget, "Recent Activity")

            # App Usage tab
            usage_widget = QWidget()
            usage_layout = QVBoxLayout(usage_widget)

            self.usage_table = QTableWidget()
            self.usage_table.setColumnCount(2)
            self.usage_table.setHorizontalHeaderLabels(["Application", "Time"])
            self.usage_table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
            usage_layout.addWidget(self.usage_table)

            tabs.addTab(usage_widget, "App Usage (1h)")

            # Clipboard tab
            clipboard_widget = QWidget()
            clipboard_layout = QVBoxLayout(clipboard_widget)

            self.clipboard_table = QTableWidget()
            self.clipboard_table.setColumnCount(3)
            self.clipboard_table.setHorizontalHeaderLabels(["Time", "Type", "Content"])
            self.clipboard_table.horizontalHeader().setSectionResizeMode(2, QHeaderView.ResizeMode.Stretch)
            clipboard_layout.addWidget(self.clipboard_table)

            tabs.addTab(clipboard_widget, "Clipboard History")

            layout.addWidget(tabs)

        def toggle_collection(self):
            if self.collector._running:
                self.collector.stop()
                self.start_btn.setText("Start")
                self.status_label.setText("Status: Stopped")
            else:
                self.collector.start()
                self.start_btn.setText("Stop")
                self.status_label.setText("Status: Running")

        def refresh(self):
            if not self.collector._running:
                return

            # Update activity table
            events = self.collector.get_recent_events(50)
            self.activity_table.setRowCount(len(events))

            for i, event in enumerate(reversed(events)):
                dt = datetime.fromtimestamp(event.timestamp)

                self.activity_table.setItem(i, 0, QTableWidgetItem(dt.strftime("%H:%M:%S")))
                self.activity_table.setItem(i, 1, QTableWidgetItem(event.type.value))
                self.activity_table.setItem(i, 2, QTableWidgetItem(event.source[:20]))

                # Format details
                details = ""
                if event.type == ActivityType.WINDOW_FOCUS:
                    details = event.data.get('window_title', '')[:50]
                elif event.type == ActivityType.CLIPBOARD_TEXT:
                    details = event.data.get('text', '')[:50]
                elif event.type in (ActivityType.FILE_CREATED, ActivityType.FILE_MODIFIED):
                    details = event.data.get('filename', '')
                elif event.type == ActivityType.PROCESS_STARTED:
                    details = event.data.get('name', '')
                else:
                    details = str(event.data)[:50]

                self.activity_table.setItem(i, 3, QTableWidgetItem(details))

            # Update usage table
            usage = self.collector.get_app_usage(1.0)
            self.usage_table.setRowCount(len(usage))

            for i, (app, seconds) in enumerate(usage.items()):
                self.usage_table.setItem(i, 0, QTableWidgetItem(app))

                # Format time
                if seconds >= 3600:
                    time_str = f"{seconds/3600:.1f}h"
                elif seconds >= 60:
                    time_str = f"{seconds/60:.1f}m"
                else:
                    time_str = f"{seconds:.0f}s"

                self.usage_table.setItem(i, 1, QTableWidgetItem(time_str))

            # Update clipboard table
            clipboard = self.collector.get_clipboard_history(20)
            self.clipboard_table.setRowCount(len(clipboard))

            for i, entry in enumerate(reversed(clipboard)):
                dt = datetime.fromisoformat(entry['datetime'])

                self.clipboard_table.setItem(i, 0, QTableWidgetItem(dt.strftime("%H:%M:%S")))
                self.clipboard_table.setItem(i, 1, QTableWidgetItem(entry['type']))

                content = entry['data'].get('text', entry['data'].get('note', ''))[:100]
                self.clipboard_table.setItem(i, 2, QTableWidgetItem(content))

    return ActivityPanel(collector)


# ============== Convenience function ==============

def start_activity_collection(kernel=None, watch_paths: List[str] = None) -> ActivityCollector:
    """
    Start collecting system activity.

    Args:
        kernel: SemanticKernel instance (optional)
        watch_paths: List of directories to watch for file changes

    Returns:
        ActivityCollector instance
    """
    collector = ActivityCollector(kernel)

    # Add common paths to watch
    if watch_paths:
        for path in watch_paths:
            collector.watch_directory(path)
    else:
        # Default: watch user's Documents and Desktop
        home = Path.home()
        collector.watch_directory(str(home / "Documents"), recursive=True)
        collector.watch_directory(str(home / "Desktop"), recursive=True)

    collector.start()
    return collector
