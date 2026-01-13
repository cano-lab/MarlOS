"""
Semantic File Explorer
======================

A file explorer that sees files by meaning, not just location.

Every file access is logged semantically. Query by meaning:
- "files I worked on today"
- "documents related to project X"
- "images I opened this week"

Usage:
    python semantic_explorer.py
    python semantic_explorer.py /path/to/start
"""

import sys
import os
import time
from pathlib import Path
from datetime import datetime, timedelta
from typing import Optional, List, Dict

from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QTreeView, QListWidget, QListWidgetItem, QLineEdit, QPushButton,
    QLabel, QSplitter, QFrame, QFileIconProvider, QAbstractItemView,
    QMenu, QMessageBox, QInputDialog, QStatusBar, QToolBar, QGroupBox,
    QScrollArea,
)
from PyQt6.QtCore import (
    Qt, QDir, QFileInfo, QSize, QTimer, pyqtSignal, QThread,
)
from PyQt6.QtGui import (
    QFileSystemModel, QIcon, QAction, QDesktopServices, QPixmap,
    QFont, QColor,
)

from kernel import init_kernel, get_kernel, MemoryType, SemanticKernel


class FileAccessLogger:
    """Logs file access to the semantic kernel."""

    def __init__(self, kernel: SemanticKernel):
        self.kernel = kernel

    def log_access(self, path: str, action: str = "opened"):
        """Log a file access event."""
        file_info = QFileInfo(path)

        # Store the access event
        self.kernel.memory.store(
            content=f"File {action}: {path}",
            type=MemoryType.EVENT,
            metadata={
                "event": "file.accessed",
                "action": action,
                "path": path,
                "filename": file_info.fileName(),
                "suffix": file_info.suffix().lower(),
                "size": file_info.size(),
                "dir": str(Path(path).parent),
                "timestamp": time.time(),
            }
        )

        # Emit kernel event
        self.kernel.emit("file.accessed", {
            "path": path,
            "action": action,
            "filename": file_info.fileName(),
        })

    def log_directory_visit(self, path: str):
        """Log visiting a directory."""
        self.kernel.memory.store(
            content=f"Visited directory: {path}",
            type=MemoryType.EVENT,
            metadata={
                "event": "directory.visited",
                "path": path,
                "timestamp": time.time(),
            }
        )

    def get_recent_files(self, limit: int = 20) -> List[Dict]:
        """Get recently accessed files."""
        events = self.kernel.memory.get_by_type(MemoryType.EVENT, limit=200)

        # Filter for file access events and deduplicate
        seen = set()
        recent = []
        for event in events:
            if event.metadata.get("event") == "file.accessed":
                path = event.metadata.get("path")
                if path and path not in seen:
                    seen.add(path)
                    recent.append({
                        "path": path,
                        "filename": event.metadata.get("filename"),
                        "timestamp": event.metadata.get("timestamp"),
                        "action": event.metadata.get("action"),
                    })
                    if len(recent) >= limit:
                        break

        return recent

    def get_related_files(self, path: str, limit: int = 10) -> List[Dict]:
        """Get files that are often accessed alongside this one."""
        # Query files accessed around the same time
        results = self.kernel.memory.query(
            f"files accessed with {Path(path).name}",
            type=MemoryType.EVENT,
            limit=limit * 2,
        )

        related = []
        seen = {path}
        for entry, score in results:
            if entry.metadata.get("event") == "file.accessed":
                file_path = entry.metadata.get("path")
                if file_path and file_path not in seen:
                    seen.add(file_path)
                    related.append({
                        "path": file_path,
                        "filename": entry.metadata.get("filename"),
                        "score": score,
                    })
                    if len(related) >= limit:
                        break

        return related

    def get_file_history(self, path: str) -> List[Dict]:
        """Get access history for a specific file."""
        events = self.kernel.memory.get_by_type(MemoryType.EVENT, limit=500)

        history = []
        for event in events:
            if (event.metadata.get("event") == "file.accessed" and
                event.metadata.get("path") == path):
                history.append({
                    "timestamp": event.metadata.get("timestamp"),
                    "action": event.metadata.get("action"),
                })

        return history[:20]  # Last 20 accesses


class SemanticPanel(QFrame):
    """Right panel showing semantic information."""

    file_clicked = pyqtSignal(str)  # Emitted when a file is clicked

    def __init__(self, logger: FileAccessLogger, parent=None):
        super().__init__(parent)
        self.logger = logger
        self.current_path = None
        self._setup_ui()

    def _setup_ui(self):
        self.setFrameStyle(QFrame.Shape.StyledPanel)
        self.setMinimumWidth(250)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(12)

        # Recent Files section
        recent_group = QGroupBox("Recent Files")
        recent_layout = QVBoxLayout(recent_group)
        self.recent_list = QListWidget()
        self.recent_list.setMaximumHeight(200)
        self.recent_list.itemDoubleClicked.connect(self._on_recent_clicked)
        recent_layout.addWidget(self.recent_list)
        layout.addWidget(recent_group)

        # Related Files section
        related_group = QGroupBox("Related Files")
        related_layout = QVBoxLayout(related_group)
        self.related_list = QListWidget()
        self.related_list.setMaximumHeight(150)
        self.related_list.itemDoubleClicked.connect(self._on_related_clicked)
        related_layout.addWidget(self.related_list)
        layout.addWidget(related_group)

        # File Info section
        info_group = QGroupBox("File Info")
        info_layout = QVBoxLayout(info_group)
        self.info_label = QLabel("Select a file to see details")
        self.info_label.setWordWrap(True)
        self.info_label.setStyleSheet("color: #666;")
        info_layout.addWidget(self.info_label)
        layout.addWidget(info_group)

        # Access History section
        history_group = QGroupBox("Access History")
        history_layout = QVBoxLayout(history_group)
        self.history_list = QListWidget()
        self.history_list.setMaximumHeight(120)
        history_layout.addWidget(self.history_list)
        layout.addWidget(history_group)

        layout.addStretch()

        # Initial load
        self.refresh_recent()

    def refresh_recent(self):
        """Refresh the recent files list."""
        self.recent_list.clear()
        recent = self.logger.get_recent_files(15)

        icon_provider = QFileIconProvider()
        for item in recent:
            path = item["path"]
            if os.path.exists(path):
                file_info = QFileInfo(path)
                list_item = QListWidgetItem(icon_provider.icon(file_info), item["filename"])
                list_item.setData(Qt.ItemDataRole.UserRole, path)
                list_item.setToolTip(path)
                self.recent_list.addItem(list_item)

    def update_for_file(self, path: str):
        """Update panel for selected file."""
        self.current_path = path

        if not path or not os.path.exists(path):
            self.info_label.setText("File not found")
            self.related_list.clear()
            self.history_list.clear()
            return

        file_info = QFileInfo(path)

        # Update file info
        size = file_info.size()
        if size > 1024 * 1024:
            size_str = f"{size / (1024*1024):.1f} MB"
        elif size > 1024:
            size_str = f"{size / 1024:.1f} KB"
        else:
            size_str = f"{size} bytes"

        modified = file_info.lastModified().toString("yyyy-MM-dd hh:mm")

        info_text = f"""<b>{file_info.fileName()}</b><br>
        <br>
        Size: {size_str}<br>
        Modified: {modified}<br>
        Type: {file_info.suffix().upper() or 'Unknown'}
        """
        self.info_label.setText(info_text)

        # Update related files
        self.related_list.clear()
        related = self.logger.get_related_files(path, 8)
        icon_provider = QFileIconProvider()
        for item in related:
            rel_path = item["path"]
            if os.path.exists(rel_path):
                rel_info = QFileInfo(rel_path)
                list_item = QListWidgetItem(
                    icon_provider.icon(rel_info),
                    item["filename"]
                )
                list_item.setData(Qt.ItemDataRole.UserRole, rel_path)
                list_item.setToolTip(f"{rel_path}\nRelevance: {item['score']:.2f}")
                self.related_list.addItem(list_item)

        # Update history
        self.history_list.clear()
        history = self.logger.get_file_history(path)
        for item in history:
            ts = datetime.fromtimestamp(item["timestamp"])
            time_str = ts.strftime("%m/%d %H:%M")
            self.history_list.addItem(f"{time_str} - {item['action']}")

    def _on_recent_clicked(self, item: QListWidgetItem):
        path = item.data(Qt.ItemDataRole.UserRole)
        if path:
            self.file_clicked.emit(path)

    def _on_related_clicked(self, item: QListWidgetItem):
        path = item.data(Qt.ItemDataRole.UserRole)
        if path:
            self.file_clicked.emit(path)


class SemanticFileExplorer(QMainWindow):
    """Main file explorer window."""

    def __init__(self, start_path: str = None):
        super().__init__()

        # Initialize kernel
        self.kernel = init_kernel(enable_qt=True)
        self.logger = FileAccessLogger(self.kernel)

        # Set starting path
        self.current_path = start_path or str(Path.home())

        self._setup_ui()
        self._setup_actions()
        self._connect_signals()

        # Navigate to start path
        self._navigate_to(self.current_path)

        self.setWindowTitle("Semantic File Explorer")
        self.resize(1200, 700)

    def _setup_ui(self):
        # Central widget
        central = QWidget()
        self.setCentralWidget(central)

        main_layout = QVBoxLayout(central)
        main_layout.setContentsMargins(0, 0, 0, 0)
        main_layout.setSpacing(0)

        # Search bar
        search_frame = QFrame()
        search_frame.setStyleSheet("background: #f5f5f5; border-bottom: 1px solid #ddd;")
        search_layout = QHBoxLayout(search_frame)
        search_layout.setContentsMargins(8, 6, 8, 6)

        self.path_edit = QLineEdit()
        self.path_edit.setPlaceholderText("Path or semantic search...")
        self.path_edit.returnPressed.connect(self._on_search)
        search_layout.addWidget(self.path_edit, 1)

        self.search_btn = QPushButton("Go")
        self.search_btn.clicked.connect(self._on_search)
        search_layout.addWidget(self.search_btn)

        self.semantic_btn = QPushButton("Semantic Search")
        self.semantic_btn.clicked.connect(self._on_semantic_search)
        search_layout.addWidget(self.semantic_btn)

        main_layout.addWidget(search_frame)

        # Main splitter
        splitter = QSplitter(Qt.Orientation.Horizontal)

        # Left: Directory tree
        self.tree_model = QFileSystemModel()
        self.tree_model.setRootPath("")
        self.tree_model.setFilter(QDir.Filter.Dirs | QDir.Filter.NoDotAndDotDot)

        self.tree_view = QTreeView()
        self.tree_view.setModel(self.tree_model)
        self.tree_view.setRootIndex(self.tree_model.index(""))
        self.tree_view.setMinimumWidth(200)
        self.tree_view.setMaximumWidth(350)

        # Hide columns except name
        for i in range(1, self.tree_model.columnCount()):
            self.tree_view.hideColumn(i)

        self.tree_view.clicked.connect(self._on_tree_clicked)
        splitter.addWidget(self.tree_view)

        # Center: File list
        self.file_list = QListWidget()
        self.file_list.setViewMode(QListWidget.ViewMode.IconMode)
        self.file_list.setIconSize(QSize(48, 48))
        self.file_list.setSpacing(8)
        self.file_list.setResizeMode(QListWidget.ResizeMode.Adjust)
        self.file_list.setSelectionMode(QAbstractItemView.SelectionMode.ExtendedSelection)
        self.file_list.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.file_list.customContextMenuRequested.connect(self._show_context_menu)
        self.file_list.itemDoubleClicked.connect(self._on_file_double_clicked)
        self.file_list.itemClicked.connect(self._on_file_clicked)
        splitter.addWidget(self.file_list)

        # Right: Semantic panel
        self.semantic_panel = SemanticPanel(self.logger)
        self.semantic_panel.file_clicked.connect(self._on_panel_file_clicked)
        splitter.addWidget(self.semantic_panel)

        splitter.setSizes([250, 600, 300])
        main_layout.addWidget(splitter, 1)

        # Status bar
        self.status_bar = QStatusBar()
        self.setStatusBar(self.status_bar)
        self._update_status()

    def _setup_actions(self):
        # Toolbar
        toolbar = QToolBar("Navigation")
        self.addToolBar(toolbar)

        self.back_action = QAction("Back", self)
        self.back_action.setShortcut("Alt+Left")
        self.back_action.triggered.connect(self._go_back)
        toolbar.addAction(self.back_action)

        self.up_action = QAction("Up", self)
        self.up_action.setShortcut("Alt+Up")
        self.up_action.triggered.connect(self._go_up)
        toolbar.addAction(self.up_action)

        self.refresh_action = QAction("Refresh", self)
        self.refresh_action.setShortcut("F5")
        self.refresh_action.triggered.connect(self._refresh)
        toolbar.addAction(self.refresh_action)

        toolbar.addSeparator()

        self.home_action = QAction("Home", self)
        self.home_action.triggered.connect(lambda: self._navigate_to(str(Path.home())))
        toolbar.addAction(self.home_action)

        # History for back navigation
        self.history = []
        self.history_index = -1

    def _connect_signals(self):
        # Kernel events
        self.kernel.event_emitted.connect(self._on_kernel_event)

    def _navigate_to(self, path: str):
        """Navigate to a directory."""
        path = os.path.abspath(path)

        if not os.path.isdir(path):
            path = os.path.dirname(path)

        if not os.path.exists(path):
            self.status_bar.showMessage(f"Path not found: {path}", 3000)
            return

        # Update history
        if self.current_path != path:
            self.history = self.history[:self.history_index + 1]
            self.history.append(path)
            self.history_index = len(self.history) - 1

        self.current_path = path
        self.path_edit.setText(path)

        # Log directory visit
        self.logger.log_directory_visit(path)

        # Update tree selection
        index = self.tree_model.index(path)
        self.tree_view.setCurrentIndex(index)
        self.tree_view.scrollTo(index)

        # Load files
        self._load_files(path)
        self._update_status()

    def _load_files(self, path: str):
        """Load files in the file list."""
        self.file_list.clear()

        icon_provider = QFileIconProvider()

        try:
            entries = sorted(os.listdir(path), key=lambda x: (not os.path.isdir(os.path.join(path, x)), x.lower()))
        except PermissionError:
            self.status_bar.showMessage("Permission denied", 3000)
            return

        for name in entries:
            if name.startswith('.'):
                continue  # Skip hidden files

            full_path = os.path.join(path, name)
            file_info = QFileInfo(full_path)

            item = QListWidgetItem(icon_provider.icon(file_info), name)
            item.setData(Qt.ItemDataRole.UserRole, full_path)

            if file_info.isDir():
                item.setToolTip(f"{name}\nDirectory")
            else:
                size = file_info.size()
                if size > 1024 * 1024:
                    size_str = f"{size / (1024*1024):.1f} MB"
                elif size > 1024:
                    size_str = f"{size / 1024:.1f} KB"
                else:
                    size_str = f"{size} bytes"
                item.setToolTip(f"{name}\n{size_str}")

            self.file_list.addItem(item)

    def _on_tree_clicked(self, index):
        path = self.tree_model.filePath(index)
        if path:
            self._navigate_to(path)

    def _on_file_clicked(self, item: QListWidgetItem):
        path = item.data(Qt.ItemDataRole.UserRole)
        if path and os.path.isfile(path):
            self.semantic_panel.update_for_file(path)

    def _on_file_double_clicked(self, item: QListWidgetItem):
        path = item.data(Qt.ItemDataRole.UserRole)
        if not path:
            return

        if os.path.isdir(path):
            self._navigate_to(path)
        else:
            # Log file access
            self.logger.log_access(path, "opened")

            # Open file with default application
            from PyQt6.QtCore import QUrl
            QDesktopServices.openUrl(QUrl.fromLocalFile(path))

            # Refresh semantic panel
            self.semantic_panel.refresh_recent()
            self.semantic_panel.update_for_file(path)

    def _on_panel_file_clicked(self, path: str):
        """Handle file clicked in semantic panel."""
        if os.path.isdir(path):
            self._navigate_to(path)
        elif os.path.isfile(path):
            # Navigate to containing directory and select file
            self._navigate_to(os.path.dirname(path))

            # Find and select the item
            for i in range(self.file_list.count()):
                item = self.file_list.item(i)
                if item.data(Qt.ItemDataRole.UserRole) == path:
                    self.file_list.setCurrentItem(item)
                    self.semantic_panel.update_for_file(path)
                    break

    def _on_search(self):
        """Handle search/navigate."""
        text = self.path_edit.text().strip()
        if not text:
            return

        # Check if it's a path
        if os.path.exists(text):
            self._navigate_to(text)
        else:
            # Treat as semantic search
            self._do_semantic_search(text)

    def _on_semantic_search(self):
        """Open semantic search dialog."""
        text, ok = QInputDialog.getText(
            self, "Semantic Search",
            "Search by meaning (e.g., 'python files I edited today'):"
        )
        if ok and text:
            self._do_semantic_search(text)

    def _do_semantic_search(self, query: str):
        """Perform semantic search."""
        self.status_bar.showMessage(f"Searching: {query}...")

        # Search in kernel memory
        results = self.kernel.memory.query(query, limit=30)

        # Filter for file access events
        files = []
        seen = set()
        for entry, score in results:
            path = entry.metadata.get("path")
            if path and path not in seen and os.path.exists(path):
                seen.add(path)
                files.append((path, score))

        if not files:
            self.status_bar.showMessage("No results found", 3000)
            return

        # Show results in file list
        self.file_list.clear()
        icon_provider = QFileIconProvider()

        for path, score in files:
            file_info = QFileInfo(path)
            name = file_info.fileName()

            item = QListWidgetItem(icon_provider.icon(file_info), name)
            item.setData(Qt.ItemDataRole.UserRole, path)
            item.setToolTip(f"{path}\nRelevance: {score:.2f}")
            self.file_list.addItem(item)

        self.path_edit.setText(f"Search: {query}")
        self.status_bar.showMessage(f"Found {len(files)} results", 3000)

    def _show_context_menu(self, pos):
        """Show context menu for file list."""
        item = self.file_list.itemAt(pos)
        if not item:
            return

        path = item.data(Qt.ItemDataRole.UserRole)
        if not path:
            return

        menu = QMenu(self)

        open_action = menu.addAction("Open")
        open_action.triggered.connect(lambda: self._open_file(path))

        if os.path.isfile(path):
            menu.addSeparator()

            open_with = menu.addAction("Open With...")
            # Could add "open with" functionality

            menu.addSeparator()

            show_history = menu.addAction("Show Access History")
            show_history.triggered.connect(lambda: self._show_file_history(path))

            find_related = menu.addAction("Find Related Files")
            find_related.triggered.connect(lambda: self._find_related(path))

        menu.addSeparator()

        copy_path = menu.addAction("Copy Path")
        copy_path.triggered.connect(lambda: QApplication.clipboard().setText(path))

        menu.exec(self.file_list.mapToGlobal(pos))

    def _open_file(self, path: str):
        """Open a file."""
        self.logger.log_access(path, "opened")

        if os.path.isdir(path):
            self._navigate_to(path)
        else:
            from PyQt6.QtCore import QUrl
            QDesktopServices.openUrl(QUrl.fromLocalFile(path))

        self.semantic_panel.refresh_recent()

    def _show_file_history(self, path: str):
        """Show access history for a file."""
        history = self.logger.get_file_history(path)

        if not history:
            QMessageBox.information(self, "File History", "No access history for this file.")
            return

        lines = [f"Access History for: {Path(path).name}\n"]
        for item in history:
            ts = datetime.fromtimestamp(item["timestamp"])
            lines.append(f"  {ts.strftime('%Y-%m-%d %H:%M')} - {item['action']}")

        QMessageBox.information(self, "File History", "\n".join(lines))

    def _find_related(self, path: str):
        """Find files related to this one."""
        self.status_bar.showMessage("Finding related files...")

        related = self.logger.get_related_files(path, 20)

        if not related:
            self.status_bar.showMessage("No related files found", 3000)
            return

        # Show in file list
        self.file_list.clear()
        icon_provider = QFileIconProvider()

        for item in related:
            file_path = item["path"]
            if os.path.exists(file_path):
                file_info = QFileInfo(file_path)
                list_item = QListWidgetItem(
                    icon_provider.icon(file_info),
                    item["filename"]
                )
                list_item.setData(Qt.ItemDataRole.UserRole, file_path)
                list_item.setToolTip(f"{file_path}\nRelevance: {item['score']:.2f}")
                self.file_list.addItem(list_item)

        self.path_edit.setText(f"Related to: {Path(path).name}")
        self.status_bar.showMessage(f"Found {len(related)} related files", 3000)

    def _go_back(self):
        """Go back in history."""
        if self.history_index > 0:
            self.history_index -= 1
            path = self.history[self.history_index]
            self.current_path = path
            self.path_edit.setText(path)
            self._load_files(path)

    def _go_up(self):
        """Go up one directory."""
        parent = str(Path(self.current_path).parent)
        if parent != self.current_path:
            self._navigate_to(parent)

    def _refresh(self):
        """Refresh current view."""
        self._load_files(self.current_path)
        self.semantic_panel.refresh_recent()
        self._update_status()

    def _update_status(self):
        """Update status bar."""
        stats = self.kernel.memory.stats()
        self.status_bar.showMessage(
            f"Files tracked: {stats.get('total_entries', 0)} | "
            f"Relations: {stats.get('relations', 0)} | "
            f"DB: {Path(stats.get('db_path', '')).name}"
        )

    def _on_kernel_event(self, event: str, data):
        """Handle kernel events."""
        if event == "file.accessed":
            # Could update UI based on events
            pass

    def closeEvent(self, event):
        """Clean shutdown."""
        self.kernel.shutdown()
        event.accept()


def main():
    app = QApplication(sys.argv)
    app.setApplicationName("Semantic File Explorer")

    # Get starting path from args
    start_path = sys.argv[1] if len(sys.argv) > 1 else None

    window = SemanticFileExplorer(start_path)
    window.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
