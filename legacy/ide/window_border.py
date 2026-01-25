"""
Window Border Overlay
=====================

Creates a colored border around windows launched from MarlOS.
Uses transparent overlay windows that track target window positions.

This helps visually identify which windows were spawned by MarlOS
vs windows opened independently.
"""

import ctypes
from ctypes import wintypes
import threading
import time
from typing import Dict, Optional, Tuple, List
from dataclasses import dataclass

from PyQt6.QtWidgets import QWidget, QApplication
from PyQt6.QtCore import Qt, QTimer, QRect, pyqtSignal, QObject
from PyQt6.QtGui import QPainter, QColor, QPen

# Windows API
user32 = ctypes.windll.user32
dwmapi = ctypes.windll.dwmapi
shcore = ctypes.windll.shcore

# Enable per-monitor DPI awareness (critical for multi-monitor setups)
try:
    shcore.SetProcessDpiAwareness(2)  # PROCESS_PER_MONITOR_DPI_AWARE
except Exception:
    try:
        user32.SetProcessDPIAware()
    except Exception:
        pass

# Window functions
GetWindowRect = user32.GetWindowRect
IsWindow = user32.IsWindow
IsWindowVisible = user32.IsWindowVisible
GetWindowThreadProcessId = user32.GetWindowThreadProcessId
SetWindowPos = user32.SetWindowPos
GetForegroundWindow = user32.GetForegroundWindow
GetWindowTextW = user32.GetWindowTextW
GetWindowTextLengthW = user32.GetWindowTextLengthW
MonitorFromWindow = user32.MonitorFromWindow

# DPI functions
try:
    GetDpiForWindow = user32.GetDpiForWindow
    GetDpiForWindow.restype = ctypes.c_uint
except Exception:
    GetDpiForWindow = None

# Constants
HWND_TOPMOST = -1
HWND_NOTOPMOST = -2
SWP_NOACTIVATE = 0x0010
SWP_SHOWWINDOW = 0x0040
SWP_NOSIZE = 0x0001
SWP_NOMOVE = 0x0002

# For getting window info
class RECT(ctypes.Structure):
    _fields_ = [
        ('left', ctypes.c_long),
        ('top', ctypes.c_long),
        ('right', ctypes.c_long),
        ('bottom', ctypes.c_long),
    ]


def get_window_rect(hwnd: int) -> Optional[Tuple[int, int, int, int]]:
    """Get window rectangle (x, y, width, height) using DWM for accurate bounds."""
    rect = RECT()

    # Try DwmGetWindowAttribute first for accurate bounds (excludes shadow)
    # DWMWA_EXTENDED_FRAME_BOUNDS = 9
    DWMWA_EXTENDED_FRAME_BOUNDS = 9
    result = dwmapi.DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        ctypes.byref(rect),
        ctypes.sizeof(rect)
    )

    if result == 0:  # S_OK
        return (
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top
        )

    # Fallback to GetWindowRect
    if GetWindowRect(hwnd, ctypes.byref(rect)):
        return (
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top
        )

    return None


def is_window_valid(hwnd: int) -> bool:
    """Check if window handle is still valid and visible."""
    return bool(IsWindow(hwnd)) and bool(IsWindowVisible(hwnd))


@dataclass
class TrackedWindow:
    """A window being tracked with a border overlay."""
    hwnd: int
    pid: int
    overlay: 'BorderOverlay'
    color: Tuple[int, int, int]
    label: str = ""


class BorderOverlay(QWidget):
    """
    A transparent overlay window that draws a border.
    Follows a target window and draws a colored border around it.
    Includes a tracking warning badge.
    """

    def __init__(self, color: Tuple[int, int, int] = (0, 200, 255),
                 border_width: int = 4, label: str = "",
                 show_tracking_warning: bool = True):
        super().__init__()

        self.border_color = QColor(*color)
        self.border_width = border_width
        self.label = label
        self.show_tracking_warning = show_tracking_warning
        self.target_hwnd = 0
        self._is_active = False  # Whether target is the active window
        self._pulse_phase = 0  # For pulsing animation

        # Make window frameless and transparent
        # Note: WindowTransparentForInput allows clicks to pass through
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint |
            Qt.WindowType.WindowStaysOnTopHint |
            Qt.WindowType.Tool |  # Don't show in taskbar
            Qt.WindowType.WindowTransparentForInput |  # Click-through
            Qt.WindowType.X11BypassWindowManagerHint  # Helps on some systems
        )

        # Transparent background
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)

        # Don't take focus
        self.setFocusPolicy(Qt.FocusPolicy.NoFocus)

        # Pulse animation for the tracking indicator
        self._pulse_timer = QTimer()
        self._pulse_timer.timeout.connect(self._pulse_tick)
        self._pulse_timer.start(50)  # 20 FPS

    def _pulse_tick(self):
        """Update pulse animation."""
        self._pulse_phase = (self._pulse_phase + 1) % 60
        if self.isVisible():
            self.update()

    def set_target(self, hwnd: int):
        """Set the target window to track."""
        self.target_hwnd = hwnd

    def set_active(self, active: bool):
        """Set whether the target window is currently active."""
        if self._is_active != active:
            self._is_active = active
            self.update()

    def update_position(self) -> bool:
        """Update overlay position to match target. Returns False if window gone."""
        if not self.target_hwnd or not is_window_valid(self.target_hwnd):
            return False

        rect = get_window_rect(self.target_hwnd)
        if not rect:
            return False

        x, y, w, h = rect

        # Skip if window is minimized (zero size)
        if w <= 0 or h <= 0:
            self.hide()
            return True

        # Get DPI scale factor for the target window's monitor
        dpi_scale = 1.0
        if GetDpiForWindow:
            try:
                dpi = GetDpiForWindow(self.target_hwnd)
                if dpi > 0:
                    dpi_scale = dpi / 96.0
            except:
                pass

        # DWM gives physical pixels, Qt expects logical pixels
        # Divide by DPI scale to convert physical -> logical
        x = int(x / dpi_scale)
        y = int(y / dpi_scale)
        w = int(w / dpi_scale)
        h = int(h / dpi_scale)

        # Add a small margin for the border to sit outside the window
        margin = self.border_width + 2

        self.setGeometry(
            x - margin,
            y - margin,
            w + margin * 2,
            h + margin * 2
        )

        # Ensure overlay stays visible and on top
        if not self.isVisible():
            self.show()
        self.raise_()

        return True

    def paintEvent(self, event):
        """Draw the border and tracking indicator."""
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Use brighter color when active
        if self._is_active:
            color = self.border_color.lighter(120)
            width = self.border_width + 1
        else:
            color = self.border_color
            width = self.border_width

        pen = QPen(color)
        pen.setWidth(width)
        painter.setPen(pen)

        # Draw border rectangle
        margin = self.border_width
        painter.drawRect(
            margin,
            margin,
            self.width() - margin * 2,
            self.height() - margin * 2
        )

        # Draw corner accents for more visibility
        accent_len = 20
        pen.setWidth(width + 2)
        painter.setPen(pen)

        # Top-left corner
        painter.drawLine(margin, margin, margin + accent_len, margin)
        painter.drawLine(margin, margin, margin, margin + accent_len)

        # Top-right corner
        painter.drawLine(self.width() - margin, margin, self.width() - margin - accent_len, margin)
        painter.drawLine(self.width() - margin, margin, self.width() - margin, margin + accent_len)

        # Bottom-left corner
        painter.drawLine(margin, self.height() - margin, margin + accent_len, self.height() - margin)
        painter.drawLine(margin, self.height() - margin, margin, self.height() - margin - accent_len)

        # Bottom-right corner
        painter.drawLine(self.width() - margin, self.height() - margin,
                        self.width() - margin - accent_len, self.height() - margin)
        painter.drawLine(self.width() - margin, self.height() - margin,
                        self.width() - margin, self.height() - margin - accent_len)

        # Draw tracking warning badge at top-right
        if self.show_tracking_warning:
            self._draw_tracking_badge(painter, margin)

        # Draw label at top-left if provided
        if self.label:
            self._draw_label(painter, margin, color)

        painter.end()

    def _draw_tracking_badge(self, painter: QPainter, margin: int):
        """Draw the tracking warning badge at top center."""
        # Pulsing effect
        import math
        pulse = 0.7 + 0.3 * math.sin(self._pulse_phase * 0.2)

        badge_width = 95
        badge_height = 22

        # Center horizontally at top
        badge_x = (self.width() - badge_width) // 2
        badge_y = margin + 2

        # Badge background (dark with transparency, rounded)
        bg_color = QColor(30, 30, 30, int(230 * pulse))
        painter.setBrush(bg_color)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(badge_x, badge_y, badge_width, badge_height, 4, 4)

        # Recording dot (pulsing red)
        dot_radius = 5
        dot_x = badge_x + 12
        dot_y = badge_y + badge_height // 2

        dot_color = QColor(255, 50, 50, int(255 * pulse))
        painter.setBrush(dot_color)
        painter.drawEllipse(dot_x - dot_radius, dot_y - dot_radius,
                           dot_radius * 2, dot_radius * 2)

        # "TRACKED" text
        font = painter.font()
        font.setPointSize(9)
        font.setBold(True)
        painter.setFont(font)
        painter.setPen(QColor(255, 255, 255, int(255 * pulse)))
        painter.drawText(dot_x + dot_radius + 6, badge_y + 16, "TRACKED")

    def _draw_label(self, painter: QPainter, margin: int, color: QColor):
        """Draw the app label."""
        font = painter.font()
        font.setPointSize(9)
        font.setBold(True)
        painter.setFont(font)

        metrics = painter.fontMetrics()
        text_width = metrics.horizontalAdvance(self.label) + 16
        text_height = metrics.height() + 8

        # Position at top-left corner
        label_x = margin + 5
        label_y = margin + 5

        # Background with rounded corners
        painter.setBrush(color)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(
            label_x, label_y,
            text_width, text_height,
            4, 4
        )

        # Icon (eye symbol for tracking)
        painter.setPen(QColor(255, 255, 255))
        eye_x = label_x + 6
        eye_y = label_y + text_height // 2

        # Simple eye shape
        painter.drawEllipse(eye_x, eye_y - 3, 8, 6)
        painter.setBrush(QColor(255, 255, 255))
        painter.drawEllipse(eye_x + 2, eye_y - 2, 4, 4)

        # Text
        painter.drawText(
            label_x + 18,
            label_y + metrics.ascent() + 4,
            self.label
        )

    def closeEvent(self, event):
        """Clean up timer on close."""
        self._pulse_timer.stop()
        super().closeEvent(event)


class WindowBorderManager(QObject):
    """
    Manages border overlays for windows launched from MarlOS.

    Usage:
        manager = WindowBorderManager()
        manager.start()

        # When launching a process
        pid = launch_some_app()
        manager.track_process(pid, color=(0, 200, 255), label="MarlOS")

        # Later
        manager.stop()
    """

    window_tracked = pyqtSignal(int, int)  # hwnd, pid
    window_lost = pyqtSignal(int, int)  # hwnd, pid

    # Preset colors for different contexts
    COLORS = {
        'default': (0, 200, 255),    # Cyan
        'editor': (100, 200, 100),   # Green
        'browser': (255, 150, 50),   # Orange
        'terminal': (200, 100, 255), # Purple
        'viewer': (255, 200, 50),    # Yellow
    }

    def __init__(self, parent=None):
        super().__init__(parent)

        self._running = False
        self._tracked_pids: Dict[int, Tuple[int, int, int]] = {}  # pid -> color
        self._tracked_windows: Dict[int, TrackedWindow] = {}  # hwnd -> TrackedWindow
        self._labels: Dict[int, str] = {}  # pid -> label

        # Update timer
        self._timer = QTimer()
        self._timer.timeout.connect(self._update_overlays)

        # Window discovery timer (slower)
        self._discovery_timer = QTimer()
        self._discovery_timer.timeout.connect(self._discover_windows)

    def start(self):
        """Start tracking windows."""
        if self._running:
            return

        self._running = True
        self._timer.start(16)  # ~60 FPS for smooth tracking
        self._discovery_timer.start(500)  # Check for new windows every 500ms

        print("[WindowBorder] Started window border tracking")

    def stop(self):
        """Stop tracking and remove all overlays."""
        self._running = False
        self._timer.stop()
        self._discovery_timer.stop()

        # Remove all overlays
        for tracked in self._tracked_windows.values():
            tracked.overlay.close()

        self._tracked_windows.clear()
        print("[WindowBorder] Stopped window border tracking")

    def track_process(self, pid: int, color: Tuple[int, int, int] = None,
                      label: str = "MarlOS"):
        """
        Track all windows belonging to a process.

        Args:
            pid: Process ID to track
            color: RGB tuple for border color (default: cyan)
            label: Label to show on the border (optional)
        """
        if color is None:
            color = self.COLORS['default']

        self._tracked_pids[pid] = color
        self._labels[pid] = label

        print(f"[WindowBorder] Tracking PID {pid} with color {color}")

        # Immediately look for windows
        self._discover_windows()

    def untrack_process(self, pid: int):
        """Stop tracking a process."""
        if pid in self._tracked_pids:
            del self._tracked_pids[pid]

        if pid in self._labels:
            del self._labels[pid]

        # Remove overlays for this process
        to_remove = [
            hwnd for hwnd, tracked in self._tracked_windows.items()
            if tracked.pid == pid
        ]

        for hwnd in to_remove:
            self._remove_overlay(hwnd)

    def track_window(self, hwnd: int, color: Tuple[int, int, int] = None,
                     label: str = ""):
        """
        Track a specific window by handle.

        Args:
            hwnd: Window handle
            color: RGB tuple for border color
            label: Label to show on the border
        """
        if hwnd in self._tracked_windows:
            return

        if color is None:
            color = self.COLORS['default']

        # Get PID for this window
        pid = wintypes.DWORD()
        GetWindowThreadProcessId(hwnd, ctypes.byref(pid))

        self._create_overlay(hwnd, pid.value, color, label)

    def untrack_window(self, hwnd: int):
        """Stop tracking a specific window."""
        self._remove_overlay(hwnd)

    def set_color(self, pid: int, color: Tuple[int, int, int]):
        """Change the border color for a process."""
        self._tracked_pids[pid] = color

        # Update existing overlays
        for tracked in self._tracked_windows.values():
            if tracked.pid == pid:
                tracked.color = color
                tracked.overlay.border_color = QColor(*color)
                tracked.overlay.update()

    def _discover_windows(self):
        """Find new windows for tracked processes."""
        if not self._tracked_pids:
            return

        # Enumerate all windows
        def enum_callback(hwnd, lparam):
            if not IsWindowVisible(hwnd):
                return True

            # Get window's PID
            pid = wintypes.DWORD()
            GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            pid = pid.value

            # Check if we're tracking this PID
            if pid in self._tracked_pids and hwnd not in self._tracked_windows:
                color = self._tracked_pids[pid]
                label = self._labels.get(pid, "")
                self._create_overlay(hwnd, pid, color, label)

            return True

        # Define callback type
        WNDENUMPROC = ctypes.WINFUNCTYPE(
            wintypes.BOOL,
            wintypes.HWND,
            wintypes.LPARAM
        )

        user32.EnumWindows(WNDENUMPROC(enum_callback), 0)

    def _create_overlay(self, hwnd: int, pid: int,
                        color: Tuple[int, int, int], label: str):
        """Create an overlay for a window."""
        if hwnd in self._tracked_windows:
            return

        overlay = BorderOverlay(color=color, label=label)
        overlay.set_target(hwnd)

        if overlay.update_position():
            overlay.show()

            tracked = TrackedWindow(
                hwnd=hwnd,
                pid=pid,
                overlay=overlay,
                color=color,
                label=label
            )

            self._tracked_windows[hwnd] = tracked
            self.window_tracked.emit(hwnd, pid)

            print(f"[WindowBorder] Created overlay for HWND {hwnd}")

    def _remove_overlay(self, hwnd: int):
        """Remove an overlay."""
        if hwnd in self._tracked_windows:
            tracked = self._tracked_windows[hwnd]
            tracked.overlay.close()
            del self._tracked_windows[hwnd]
            self.window_lost.emit(hwnd, tracked.pid)

            print(f"[WindowBorder] Removed overlay for HWND {hwnd}")

    def _update_overlays(self):
        """Update all overlay positions."""
        if not self._tracked_windows:
            return

        # Get currently active window
        active_hwnd = GetForegroundWindow()

        # Update each overlay
        to_remove = []

        for hwnd, tracked in self._tracked_windows.items():
            # Check if window still exists
            if not tracked.overlay.update_position():
                to_remove.append(hwnd)
                continue

            # Update active state
            tracked.overlay.set_active(hwnd == active_hwnd)

        # Remove dead windows
        for hwnd in to_remove:
            self._remove_overlay(hwnd)

    def get_tracked_count(self) -> int:
        """Get number of tracked windows."""
        return len(self._tracked_windows)

    def get_tracked_pids(self) -> List[int]:
        """Get list of tracked PIDs."""
        return list(self._tracked_pids.keys())


# ============== Integration with process launching ==============

class TrackedProcessLauncher:
    """
    Helper to launch processes with automatic border tracking.

    Usage:
        launcher = TrackedProcessLauncher(border_manager)
        launcher.launch("notepad.exe", label="Notes")
        launcher.launch("code.exe", color=(100, 200, 100), label="Editor")
    """

    def __init__(self, border_manager: WindowBorderManager):
        self.border_manager = border_manager

    def launch(self, command: str, color: Tuple[int, int, int] = None,
               label: str = "MarlOS", shell: bool = False) -> Optional[int]:
        """
        Launch a process and track its windows.

        Args:
            command: Command to execute
            color: Border color (RGB tuple)
            label: Label to show on border
            shell: Whether to use shell execution

        Returns:
            Window handle if successful, None otherwise
        """
        import subprocess
        import time

        # Get list of current windows before launch
        existing_windows = self._get_all_visible_windows()

        try:
            if shell:
                proc = subprocess.Popen(command, shell=True)
            else:
                proc = subprocess.Popen(command.split())

            # Wait a bit for window to appear
            time.sleep(0.5)

            # Find new window that appeared
            for _ in range(10):  # Try for up to 5 seconds
                current_windows = self._get_all_visible_windows()
                new_windows = current_windows - existing_windows

                if new_windows:
                    # Track the new window(s)
                    for hwnd in new_windows:
                        self.border_manager.track_window(hwnd, color=color, label=label)
                        print(f"[TrackedLauncher] Tracking new window: {hwnd}")

                    return list(new_windows)[0]

                time.sleep(0.5)

            print("[TrackedLauncher] No new window detected")
            return None

        except Exception as e:
            print(f"[TrackedLauncher] Failed to launch: {e}")
            return None

    def _get_all_visible_windows(self) -> set:
        """Get set of all visible window handles with titles."""
        windows = set()

        def callback(hwnd, lparam):
            if IsWindowVisible(hwnd):
                # Only include windows with titles (main windows)
                length = user32.GetWindowTextLengthW(hwnd)
                if length > 0:
                    windows.add(hwnd)
            return True

        WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        user32.EnumWindows(WNDENUMPROC(callback), 0)

        return windows

    def open_file(self, filepath: str, color: Tuple[int, int, int] = None,
                  label: str = "") -> Optional[int]:
        """
        Open a file with default application and track the window.

        Args:
            filepath: Path to file
            color: Border color
            label: Label (defaults to filename)

        Returns:
            Process ID if successful
        """
        import subprocess
        import os
        from pathlib import Path

        if not label:
            label = Path(filepath).name

        try:
            # Use 'start' on Windows to open with default app
            proc = subprocess.Popen(
                ['cmd', '/c', 'start', '', filepath],
                shell=False
            )

            # Note: 'start' spawns a child process, so we can't track by PID easily
            # This is a limitation - would need to watch for new windows

            return proc.pid

        except Exception as e:
            print(f"[TrackedLauncher] Failed to open file: {e}")
            return None


# ============== Global instance ==============

_border_manager: Optional[WindowBorderManager] = None


def get_border_manager() -> WindowBorderManager:
    """Get or create the global border manager."""
    global _border_manager

    if _border_manager is None:
        _border_manager = WindowBorderManager()

    return _border_manager


def start_border_tracking():
    """Start the global border manager."""
    manager = get_border_manager()
    manager.start()
    return manager


def stop_border_tracking():
    """Stop the global border manager."""
    global _border_manager

    if _border_manager:
        _border_manager.stop()


def track_current_window(label: str = "MarlOS", color: Tuple[int, int, int] = None):
    """
    Track the currently active window.
    Useful for testing - focus a window, then call this.
    """
    manager = get_border_manager()
    if not manager._running:
        manager.start()

    hwnd = GetForegroundWindow()
    if hwnd:
        manager.track_window(hwnd, color=color, label=label)
        return hwnd
    return None


def demo_border():
    """
    Demo the border overlay by launching notepad and tracking it.
    Run this function to see the border in action.
    """
    import time

    # Start the manager
    manager = start_border_tracking()

    # Use the launcher to properly track new windows
    launcher = TrackedProcessLauncher(manager)

    print("Launching notepad with tracking...")
    hwnd = launcher.launch("notepad.exe", label="MarlOS: Notes", color=(0, 200, 255))

    if hwnd:
        print(f"Tracking window handle: {hwnd}")
        print("Notepad should now have a cyan border with 'TRACKED' indicator")
        print("Close notepad to stop the demo")

        # Keep running until window is closed
        while is_window_valid(hwnd):
            time.sleep(0.5)
    else:
        print("Failed to track window")

    manager.stop()
    print("Demo complete")


# Run demo if executed directly
if __name__ == "__main__":
    import sys
    from PyQt6.QtWidgets import QApplication

    app = QApplication(sys.argv)
    demo_border()
    sys.exit(0)
