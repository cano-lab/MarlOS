#!/usr/bin/env python3
"""
Markdown Editor
A clean markdown editor with toggle between edit and preview modes.
"""

import sys
import os
import json
import re
import time
import platform
from pathlib import Path
from datetime import datetime
from cryptography.fernet import Fernet
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC
import base64

from PyQt6.QtCore import Qt, QFileSystemWatcher, QTimer, QRegularExpression, QDateTime, QThread, pyqtSignal, QUrl, QSize
from PyQt6.QtGui import (
    QAction, QKeySequence, QFont, QTextDocument, QTextCursor, QCursor,
    QColor, QSyntaxHighlighter, QTextCharFormat, QBrush, QPixmap, QIcon
)
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QTabWidget, QFileDialog,
    QMessageBox, QWidget, QVBoxLayout, QHBoxLayout, QTextBrowser,
    QDialog, QSpinBox, QDialogButtonBox, QFormLayout,
    QCheckBox, QLineEdit, QPushButton, QPlainTextEdit, QStackedWidget,
    QLabel, QDockWidget, QListWidget, QListWidgetItem, QToolTip,
    QComboBox, QDateTimeEdit, QScrollArea, QFrame, QInputDialog, QGridLayout, QMenu, QTextEdit
)
from PyQt6.QtPrintSupport import QPrinter, QPrintDialog
from PyQt6.QtGui import QPageLayout, QPageSize
from PyQt6.QtCore import QMarginsF

from ide.commands import CommandRegistry, Command
from ide.context import IDEContext
from ide.document import Document
from ide.events import EventsSpine
from kernel import init_kernel, get_kernel  # Unified semantic kernel
from ide.ai import LocalAIClient, HybridAIClient
from ide.lexicon import LocalLexicon
from ide.tasks import TaskExtractor, TaskIndexStore

try:
    from PyQt6.QtWebEngineWidgets import QWebEngineView
    from PyQt6.QtWebEngineCore import QWebEnginePage
except Exception:
    QWebEngineView = None
    QWebEnginePage = None
try:
    import markdown as markdown_lib
except Exception:
    markdown_lib = None
from ide.license import get_license_manager, LicenseTier
from ide.relation_explorer import create_relation_explorer_dock
from ide.providers import (
    DocumentCommandProvider,
    FormattingProvider,
    IntentMapProvider,
    CitationHelperProvider,
    DiffNarratorProvider,
    OutlineEnhancerProvider,
    ActionExtractorProvider,
    ImageGeneratorProvider,
    TypoFixerProvider,
    SuggestionProvider,
    LexiconProvider,
    TimerProvider,
    CodingAgentProvider,
    MarkdownLanguageService,
    MarkdownRenderer,
    OutlineProvider,
    PreviewProvider,
    WordCountProvider,
    CodeRunnerProvider,
    BuildProvider,
    ShellProvider,
    PythonInterpreterProvider,
    HistoryProvider,
)


class MarkdownHighlighter(QSyntaxHighlighter):
    """Syntax highlighter for Markdown."""

    def __init__(self, parent=None, dark_mode=False):
        super().__init__(parent)
        self.dark_mode = dark_mode
        self.setup_formats()

    def setup_formats(self):
        if self.dark_mode:
            heading_color = QColor("#569cd6")
            bold_color = QColor("#ce9178")
            italic_color = QColor("#b5cea8")
            code_color = QColor("#d7ba7d")
            link_color = QColor("#6cb6ff")
            list_color = QColor("#c586c0")
        else:
            heading_color = QColor("#0000aa")
            bold_color = QColor("#aa0000")
            italic_color = QColor("#008800")
            code_color = QColor("#666600")
            link_color = QColor("#0066cc")
            list_color = QColor("#880088")

        self.heading_format = QTextCharFormat()
        self.heading_format.setForeground(QBrush(heading_color))
        self.heading_format.setFontWeight(QFont.Weight.Bold)

        self.bold_format = QTextCharFormat()
        self.bold_format.setForeground(QBrush(bold_color))
        self.bold_format.setFontWeight(QFont.Weight.Bold)

        self.italic_format = QTextCharFormat()
        self.italic_format.setForeground(QBrush(italic_color))
        self.italic_format.setFontItalic(True)

        self.code_format = QTextCharFormat()
        self.code_format.setForeground(QBrush(code_color))
        self.code_format.setFontFamily("Consolas")

        self.link_format = QTextCharFormat()
        self.link_format.setForeground(QBrush(link_color))

        self.list_format = QTextCharFormat()
        self.list_format.setForeground(QBrush(list_color))

    def highlightBlock(self, text):
        if text.startswith('#'):
            self.setFormat(0, len(text), self.heading_format)
            return

        bold_pattern = QRegularExpression(r'\*\*(.+?)\*\*|__(.+?)__')
        match_iter = bold_pattern.globalMatch(text)
        while match_iter.hasNext():
            match = match_iter.next()
            self.setFormat(match.capturedStart(), match.capturedLength(), self.bold_format)

        italic_pattern = QRegularExpression(r'(?<!\*)\*(?!\*)(.+?)(?<!\*)\*(?!\*)|(?<!_)_(?!_)(.+?)(?<!_)_(?!_)')
        match_iter = italic_pattern.globalMatch(text)
        while match_iter.hasNext():
            match = match_iter.next()
            self.setFormat(match.capturedStart(), match.capturedLength(), self.italic_format)

        code_pattern = QRegularExpression(r'`[^`]+`')
        match_iter = code_pattern.globalMatch(text)
        while match_iter.hasNext():
            match = match_iter.next()
            self.setFormat(match.capturedStart(), match.capturedLength(), self.code_format)

        link_pattern = QRegularExpression(r'\[.+?\]\(.+?\)')
        match_iter = link_pattern.globalMatch(text)
        while match_iter.hasNext():
            match = match_iter.next()
            self.setFormat(match.capturedStart(), match.capturedLength(), self.link_format)

        list_pattern = QRegularExpression(r'^(\s*[-*+]|\s*\d+\.)\s')
        match = list_pattern.match(text)
        if match.hasMatch():
            self.setFormat(match.capturedStart(), match.capturedLength(), self.list_format)


class CodeHighlighter(QSyntaxHighlighter):
    """Syntax highlighter for code files (Python, JavaScript, etc.)."""

    PYTHON_KEYWORDS = [
        'and', 'as', 'assert', 'async', 'await', 'break', 'class', 'continue',
        'def', 'del', 'elif', 'else', 'except', 'finally', 'for', 'from',
        'global', 'if', 'import', 'in', 'is', 'lambda', 'None', 'nonlocal',
        'not', 'or', 'pass', 'raise', 'return', 'True', 'False', 'try',
        'while', 'with', 'yield',
    ]

    JS_KEYWORDS = [
        'async', 'await', 'break', 'case', 'catch', 'class', 'const', 'continue',
        'debugger', 'default', 'delete', 'do', 'else', 'export', 'extends',
        'finally', 'for', 'function', 'if', 'import', 'in', 'instanceof',
        'let', 'new', 'null', 'return', 'static', 'super', 'switch', 'this',
        'throw', 'true', 'false', 'try', 'typeof', 'undefined', 'var', 'void',
        'while', 'with', 'yield',
    ]

    def __init__(self, parent=None, dark_mode=False, language="python"):
        super().__init__(parent)
        self.dark_mode = dark_mode
        self.language = language
        self.setup_formats()

    def setup_formats(self):
        if self.dark_mode:
            keyword_color = QColor("#569cd6")
            string_color = QColor("#ce9178")
            comment_color = QColor("#6a9955")
            number_color = QColor("#b5cea8")
            function_color = QColor("#dcdcaa")
            class_color = QColor("#4ec9b0")
            decorator_color = QColor("#c586c0")
        else:
            keyword_color = QColor("#0000ff")
            string_color = QColor("#a31515")
            comment_color = QColor("#008000")
            number_color = QColor("#098658")
            function_color = QColor("#795e26")
            class_color = QColor("#267f99")
            decorator_color = QColor("#af00db")

        self.keyword_format = QTextCharFormat()
        self.keyword_format.setForeground(QBrush(keyword_color))
        self.keyword_format.setFontWeight(QFont.Weight.Bold)

        self.string_format = QTextCharFormat()
        self.string_format.setForeground(QBrush(string_color))

        self.comment_format = QTextCharFormat()
        self.comment_format.setForeground(QBrush(comment_color))
        self.comment_format.setFontItalic(True)

        self.number_format = QTextCharFormat()
        self.number_format.setForeground(QBrush(number_color))

        self.function_format = QTextCharFormat()
        self.function_format.setForeground(QBrush(function_color))

        self.class_format = QTextCharFormat()
        self.class_format.setForeground(QBrush(class_color))
        self.class_format.setFontWeight(QFont.Weight.Bold)

        self.decorator_format = QTextCharFormat()
        self.decorator_format.setForeground(QBrush(decorator_color))

    def highlightBlock(self, text):
        keywords = self.PYTHON_KEYWORDS if self.language == "python" else self.JS_KEYWORDS

        # Keywords
        for keyword in keywords:
            pattern = QRegularExpression(rf'\b{keyword}\b')
            match_iter = pattern.globalMatch(text)
            while match_iter.hasNext():
                match = match_iter.next()
                self.setFormat(match.capturedStart(), match.capturedLength(), self.keyword_format)

        # Strings (single and double quotes)
        string_patterns = [
            QRegularExpression(r'"[^"\\]*(\\.[^"\\]*)*"'),
            QRegularExpression(r"'[^'\\]*(\\.[^'\\]*)*'"),
        ]
        for pattern in string_patterns:
            match_iter = pattern.globalMatch(text)
            while match_iter.hasNext():
                match = match_iter.next()
                self.setFormat(match.capturedStart(), match.capturedLength(), self.string_format)

        # Numbers
        number_pattern = QRegularExpression(r'\b\d+\.?\d*\b')
        match_iter = number_pattern.globalMatch(text)
        while match_iter.hasNext():
            match = match_iter.next()
            self.setFormat(match.capturedStart(), match.capturedLength(), self.number_format)

        # Function definitions
        if self.language == "python":
            func_pattern = QRegularExpression(r'\bdef\s+(\w+)')
        else:
            func_pattern = QRegularExpression(r'\bfunction\s+(\w+)')
        match_iter = func_pattern.globalMatch(text)
        while match_iter.hasNext():
            match = match_iter.next()
            self.setFormat(match.capturedStart(1), match.capturedLength(1), self.function_format)

        # Class definitions
        class_pattern = QRegularExpression(r'\bclass\s+(\w+)')
        match_iter = class_pattern.globalMatch(text)
        while match_iter.hasNext():
            match = match_iter.next()
            self.setFormat(match.capturedStart(1), match.capturedLength(1), self.class_format)

        # Decorators (Python)
        if self.language == "python":
            decorator_pattern = QRegularExpression(r'@\w+')
            match_iter = decorator_pattern.globalMatch(text)
            while match_iter.hasNext():
                match = match_iter.next()
                self.setFormat(match.capturedStart(), match.capturedLength(), self.decorator_format)

        # Comments
        if self.language == "python":
            comment_pattern = QRegularExpression(r'#.*$')
        else:
            comment_pattern = QRegularExpression(r'//.*$')
        match = comment_pattern.match(text)
        if match.hasMatch():
            self.setFormat(match.capturedStart(), match.capturedLength(), self.comment_format)


class SearchBar(QWidget):
    """Search bar widget."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.target_widget = None

        layout = QHBoxLayout(self)
        layout.setContentsMargins(5, 5, 5, 5)

        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("Search...")
        self.search_input.returnPressed.connect(self.find_next)
        layout.addWidget(self.search_input)

        prev_btn = QPushButton("Prev")
        prev_btn.clicked.connect(self.find_prev)
        layout.addWidget(prev_btn)

        next_btn = QPushButton("Next")
        next_btn.clicked.connect(self.find_next)
        layout.addWidget(next_btn)

        close_btn = QPushButton("X")
        close_btn.setFixedWidth(30)
        close_btn.clicked.connect(self.hide)
        layout.addWidget(close_btn)

        self.hide()

    def set_target(self, widget):
        self.target_widget = widget

    def show_and_focus(self):
        self.show()
        self.search_input.setFocus()
        self.search_input.selectAll()

    def find_next(self):
        if self.target_widget and self.search_input.text():
            self.target_widget.find(self.search_input.text())

    def find_prev(self):
        if self.target_widget and self.search_input.text():
            self.target_widget.find(self.search_input.text(), QTextDocument.FindFlag.FindBackward)


class ConsolePanel(QWidget):
    """Console panel for compiler output and messages. Always dark themed."""

    def __init__(self, parent=None, dark_mode=True):
        super().__init__(parent)
        self.dark_mode = True  # Console is always dark

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Dark themed container
        self.setStyleSheet("background-color: #1e1e1e;")

        # Header with title and clear button
        header = QWidget()
        header_layout = QHBoxLayout(header)
        header_layout.setContentsMargins(8, 4, 8, 4)

        title = QLabel("Console")
        title.setStyleSheet("font-weight: bold; color: #ccc;")
        header_layout.addWidget(title)

        header_layout.addStretch()

        clear_btn = QPushButton("Clear")
        clear_btn.setFixedWidth(60)
        clear_btn.setStyleSheet("""
            QPushButton {
                background-color: #333;
                color: #ccc;
                border: 1px solid #555;
                padding: 2px 8px;
                border-radius: 3px;
            }
            QPushButton:hover {
                background-color: #444;
            }
        """)
        clear_btn.clicked.connect(self.clear)
        header_layout.addWidget(clear_btn)

        layout.addWidget(header)

        # Output area - always dark
        self.output = QPlainTextEdit()
        self.output.setReadOnly(True)
        self.output.setFont(QFont("Consolas", 10))
        self.output.setMaximumBlockCount(5000)
        self.output.setStyleSheet("""
            QPlainTextEdit {
                background-color: #1e1e1e;
                color: #d4d4d4;
                border: none;
                padding: 8px;
                selection-background-color: #264f78;
            }
        """)

        layout.addWidget(self.output)

    def write(self, text, category="info"):
        """Write text to console with optional category styling."""
        cursor = self.output.textCursor()
        cursor.movePosition(cursor.MoveOperation.End)

        # Color based on category
        if category == "error":
            color = "#f44336" if self.dark_mode else "#c62828"
        elif category == "success":
            color = "#4caf50" if self.dark_mode else "#2e7d32"
        elif category == "warning":
            color = "#ff9800" if self.dark_mode else "#ef6c00"
        elif category == "command":
            color = "#2196f3" if self.dark_mode else "#1565c0"
        else:
            color = "#d4d4d4" if self.dark_mode else "#333333"

        # Insert with color
        fmt = cursor.charFormat()
        fmt.setForeground(QBrush(QColor(color)))
        cursor.setCharFormat(fmt)
        cursor.insertText(text)

        # Auto-scroll to bottom
        self.output.setTextCursor(cursor)
        self.output.ensureCursorVisible()

    def write_line(self, text, category="info"):
        """Write a line to console."""
        self.write(text + "\n", category)

    def write_command(self, cmd):
        """Write a command being executed."""
        self.write_line(f"$ {cmd}", "command")

    def write_output(self, text):
        """Write command output."""
        self.write(text, "info")

    def write_error(self, text):
        """Write error output."""
        self.write(text, "error")

    def write_success(self, text):
        """Write success message."""
        self.write_line(text, "success")

    def clear(self):
        """Clear console output."""
        self.output.clear()

    def set_dark_mode(self, dark_mode):
        """Console is always dark, this method is kept for compatibility."""
        pass  # Console stays dark regardless of app theme


class LinkNavigatorPanel(QWidget):
    """Panel showing semantic links for the current document."""

    link_clicked = pyqtSignal(str)  # Emits target path when link is clicked

    def __init__(self, parent=None):
        super().__init__(parent)
        self.current_path = None

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

        # Outgoing links section
        out_label = QLabel("This document links to:")
        out_label.setStyleSheet("font-weight: bold; color: #3d3929;")
        layout.addWidget(out_label)

        self.outgoing_list = QListWidget()
        self.outgoing_list.setMaximumHeight(150)
        self.outgoing_list.itemDoubleClicked.connect(self._on_link_clicked)
        layout.addWidget(self.outgoing_list)

        # Incoming links section
        in_label = QLabel("Linked from:")
        in_label.setStyleSheet("font-weight: bold; color: #3d3929;")
        layout.addWidget(in_label)

        self.incoming_list = QListWidget()
        self.incoming_list.setMaximumHeight(150)
        self.incoming_list.itemDoubleClicked.connect(self._on_link_clicked)
        layout.addWidget(self.incoming_list)

        # Related documents section
        related_label = QLabel("Related documents:")
        related_label.setStyleSheet("font-weight: bold; color: #3d3929;")
        layout.addWidget(related_label)

        self.related_list = QListWidget()
        self.related_list.itemDoubleClicked.connect(self._on_link_clicked)
        layout.addWidget(self.related_list)

        # Impact section (documents affected if this changes)
        impact_label = QLabel("Change impact:")
        impact_label.setStyleSheet("font-weight: bold; color: #3d3929;")
        layout.addWidget(impact_label)

        self.impact_list = QListWidget()
        self.impact_list.setMaximumHeight(100)
        self.impact_list.itemDoubleClicked.connect(self._on_link_clicked)
        layout.addWidget(self.impact_list)

        layout.addStretch()

        # Apply panel styling
        self.setStyleSheet("""
            QWidget { background-color: #faf8f5; color: #3d3929; }
            QListWidget {
                background-color: #ffffff;
                color: #3d3929;
                border: 1px solid #d5d0c4;
                border-radius: 4px;
            }
            QListWidget::item {
                padding: 4px 8px;
            }
            QListWidget::item:selected {
                background: #e8d5b5;
                color: #3d3929;
            }
            QListWidget::item:hover {
                background: #f0ebe3;
            }
        """)

    def update_for_path(self, path: str, kernel):
        """Update the panel for a document path."""
        self.current_path = path
        self.outgoing_list.clear()
        self.incoming_list.clear()
        self.related_list.clear()
        self.impact_list.clear()

        if not path or not kernel:
            return

        # Get outgoing links
        outgoing = kernel.relations.get_outgoing(path)
        for link in outgoing:
            target = Path(link["target"]).name
            rel = link["relation"]
            item = QListWidgetItem(f"{target}  ({rel})")
            item.setData(Qt.ItemDataRole.UserRole, link["target"])
            item.setToolTip(link["target"])
            self.outgoing_list.addItem(item)

        # Get incoming links
        incoming = kernel.relations.get_incoming(path)
        for link in incoming:
            source = Path(link["source"]).name
            rel = link["relation"]
            item = QListWidgetItem(f"{source}  ({rel})")
            item.setData(Qt.ItemDataRole.UserRole, link["source"])
            item.setToolTip(link["source"])
            self.incoming_list.addItem(item)

        # Get related documents (2-hop)
        related = kernel.relations.get_related(path, max_depth=2)
        # Filter out already shown
        shown = {link["target"] for link in outgoing}
        shown.update(link["source"] for link in incoming)
        for rel_path in related:
            if rel_path not in shown:
                name = Path(rel_path).name
                item = QListWidgetItem(name)
                item.setData(Qt.ItemDataRole.UserRole, rel_path)
                item.setToolTip(rel_path)
                self.related_list.addItem(item)

        # Get change impact
        impacted = kernel.relations.get_affected_by_change(path)
        for doc in impacted:
            name = Path(doc["path"]).name
            rel = doc["relation"]
            item = QListWidgetItem(f"{name}  ({rel})")
            item.setData(Qt.ItemDataRole.UserRole, doc["path"])
            item.setToolTip(f"Changes here affect: {doc['path']}")
            self.impact_list.addItem(item)

    def _on_link_clicked(self, item):
        """Handle double-click on a link item."""
        path = item.data(Qt.ItemDataRole.UserRole)
        if path:
            self.link_clicked.emit(path)

    def clear_panel(self):
        """Clear all lists."""
        self.current_path = None
        self.outgoing_list.clear()
        self.incoming_list.clear()
        self.related_list.clear()
        self.impact_list.clear()


class SemanticPanel(QWidget):
    """Panel showing semantic features - related files, recent, tags, search."""

    file_requested = pyqtSignal(str)  # Emits path when file is clicked

    def __init__(self, parent=None):
        super().__init__(parent)
        self.kernel = None
        self.current_file = None
        self._file_access_times = {}  # Track access times for recent files

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

        # Search section
        search_label = QLabel("Semantic Search:")
        search_label.setStyleSheet("font-weight: bold; color: #3d3929;")
        layout.addWidget(search_label)

        search_row = QHBoxLayout()
        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("Search files...")
        self.search_input.returnPressed.connect(self._on_search)
        search_row.addWidget(self.search_input)

        search_btn = QPushButton("Search")
        search_btn.clicked.connect(self._on_search)
        search_row.addWidget(search_btn)
        layout.addLayout(search_row)

        self.search_results = QListWidget()
        self.search_results.setMaximumHeight(100)
        self.search_results.itemDoubleClicked.connect(self._on_file_clicked)
        self.search_results.hide()
        layout.addWidget(self.search_results)

        # Related files section
        related_label = QLabel("Related Files:")
        related_label.setStyleSheet("font-weight: bold; color: #3d3929;")
        layout.addWidget(related_label)

        self.related_list = QListWidget()
        self.related_list.setMaximumHeight(120)
        self.related_list.itemDoubleClicked.connect(self._on_file_clicked)
        layout.addWidget(self.related_list)

        # Recent files section
        recent_label = QLabel("Recent Files:")
        recent_label.setStyleSheet("font-weight: bold; color: #3d3929;")
        layout.addWidget(recent_label)

        self.recent_list = QListWidget()
        self.recent_list.setMaximumHeight(120)
        self.recent_list.itemDoubleClicked.connect(self._on_file_clicked)
        layout.addWidget(self.recent_list)

        # Tags section
        tags_label = QLabel("Tags:")
        tags_label.setStyleSheet("font-weight: bold; color: #3d3929;")
        layout.addWidget(tags_label)

        self.tags_list = QListWidget()
        self.tags_list.setMaximumHeight(80)
        self.tags_list.itemClicked.connect(self._on_tag_clicked)
        layout.addWidget(self.tags_list)

        # Current Focus section (Context Layer)
        focus_label = QLabel("Current Focus:")
        focus_label.setStyleSheet("font-weight: bold; color: #3d3929;")
        layout.addWidget(focus_label)

        self.focus_list = QListWidget()
        self.focus_list.setMaximumHeight(120)
        self.focus_list.itemDoubleClicked.connect(self._on_file_clicked)
        layout.addWidget(self.focus_list)

        # File stats section
        stats_label = QLabel("Current File:")
        stats_label.setStyleSheet("font-weight: bold; color: #3d3929;")
        layout.addWidget(stats_label)

        self.stats_text = QLabel("No file selected")
        self.stats_text.setWordWrap(True)
        self.stats_text.setStyleSheet("color: #666; font-size: 11px;")
        layout.addWidget(self.stats_text)

        layout.addStretch()

        # Styling
        self.setStyleSheet("""
            QWidget { background-color: #faf8f5; color: #3d3929; }
            QLineEdit {
                background-color: #ffffff;
                border: 1px solid #d5d0c4;
                padding: 4px;
                border-radius: 3px;
            }
            QPushButton {
                background-color: #ebe7df;
                border: 1px solid #d5d0c4;
                padding: 4px 8px;
                border-radius: 3px;
            }
            QPushButton:hover { background-color: #e0dbd1; }
            QListWidget {
                background-color: #ffffff;
                border: 1px solid #d5d0c4;
                border-radius: 4px;
            }
            QListWidget::item { padding: 4px 8px; }
            QListWidget::item:selected { background: #e8d5b5; color: #3d3929; }
            QListWidget::item:hover { background: #f0ebe3; }
        """)

    def set_kernel(self, kernel):
        """Set the semantic kernel reference."""
        self.kernel = kernel

    def record_file_access(self, path: str):
        """Record that a file was accessed."""
        import time
        self._file_access_times[path] = time.time()
        self._update_recent_files()

    def update_for_file(self, path: str):
        """Update panel for the current file."""
        self.current_file = path
        self.record_file_access(path)
        self._update_related_files()
        self._update_tags()
        self._update_stats()

    def _update_recent_files(self):
        """Update the recent files list."""
        self.recent_list.clear()
        # Sort by access time, most recent first
        sorted_files = sorted(
            self._file_access_times.items(),
            key=lambda x: x[1],
            reverse=True
        )[:10]

        for path, _ in sorted_files:
            name = Path(path).name
            item = QListWidgetItem(name)
            item.setData(Qt.ItemDataRole.UserRole, path)
            item.setToolTip(path)
            self.recent_list.addItem(item)

    def _update_related_files(self):
        """Update related files based on co-access patterns."""
        self.related_list.clear()
        if not self.current_file:
            return

        # Find files accessed around the same time as current file
        import time
        current_time = self._file_access_times.get(self.current_file, time.time())
        related = []

        for path, access_time in self._file_access_times.items():
            if path != self.current_file:
                # Within 60 seconds = related
                if abs(access_time - current_time) < 60:
                    related.append((path, abs(access_time - current_time)))

        # Sort by time proximity
        related.sort(key=lambda x: x[1])

        for path, _ in related[:5]:
            name = Path(path).name
            item = QListWidgetItem(name)
            item.setData(Qt.ItemDataRole.UserRole, path)
            item.setToolTip(path)
            self.related_list.addItem(item)

    def _update_tags(self):
        """Update tags list from all tracked files."""
        self.tags_list.clear()
        all_tags = set()

        # Collect tags from kernel if available
        if self.kernel and hasattr(self.kernel, 'memory'):
            # Get tags from documents
            pass  # Kernel integration

        # Add default tags based on file types
        for path in self._file_access_times.keys():
            ext = Path(path).suffix.lower()
            if ext == '.md':
                all_tags.add('markdown')
            elif ext == '.py':
                all_tags.add('python')
            elif ext == '.js':
                all_tags.add('javascript')
            elif ext in ('.json', '.yaml', '.yml', '.toml'):
                all_tags.add('config')
            elif ext in ('.txt', '.log'):
                all_tags.add('text')

        for tag in sorted(all_tags):
            item = QListWidgetItem(f"#{tag}")
            item.setData(Qt.ItemDataRole.UserRole, tag)
            self.tags_list.addItem(item)

    def _update_stats(self):
        """Update file statistics display."""
        if not self.current_file:
            self.stats_text.setText("No file selected")
            return

        path = Path(self.current_file)
        if path.exists():
            size = path.stat().st_size
            access_count = sum(1 for p in self._file_access_times if p == self.current_file)
            self.stats_text.setText(
                f"{path.name}\n"
                f"Size: {size:,} bytes\n"
                f"Accesses this session: {access_count}"
            )
        else:
            self.stats_text.setText(f"{path.name}\n(File not on disk)")

    def update_focus(self):
        """Update current focus list from context layer."""
        self.focus_list.clear()
        if not self.kernel:
            return

        try:
            # Get current focus from kernel
            focus_docs = self.kernel.get_current_focus(limit=5)
            for doc in focus_docs:
                name = Path(doc.path).name
                focus_time = int(doc.focus_duration)
                item = QListWidgetItem(f"● {name} ({focus_time}min, {doc.edit_count} edits)")
                item.setData(Qt.ItemDataRole.UserRole, doc.path)
                item.setToolTip(doc.path)
                self.focus_list.addItem(item)
        except Exception as e:
            # Context layer might not be available yet
            pass

    def _on_search(self):
        """Handle search."""
        query = self.search_input.text().strip()
        if not query:
            self.search_results.hide()
            return

        self.search_results.clear()
        self.search_results.show()

        # Search through tracked files
        query_lower = query.lower()
        for path in self._file_access_times.keys():
            name = Path(path).name.lower()
            if query_lower in name or query_lower in path.lower():
                item = QListWidgetItem(Path(path).name)
                item.setData(Qt.ItemDataRole.UserRole, path)
                item.setToolTip(path)
                self.search_results.addItem(item)

    def _on_file_clicked(self, item):
        """Handle file item click."""
        path = item.data(Qt.ItemDataRole.UserRole)
        if path:
            self.file_requested.emit(path)

    def _on_tag_clicked(self, item):
        """Handle tag click - filter by tag."""
        tag = item.data(Qt.ItemDataRole.UserRole)
        self.search_input.setText(f"#{tag}")
        self._on_search()


class BrowserTab(QWidget):
    """A browser tab with semantic tracking of web activity."""

    def __init__(self, kernel=None, dark_mode=False, parent=None):
        super().__init__(parent)
        self.kernel = kernel
        self.dark_mode = dark_mode
        self.current_url = ""
        self.current_title = ""
        self.navigation_history = []
        self.history_index = -1

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Toolbar
        self.toolbar = QWidget()
        toolbar_layout = QHBoxLayout(self.toolbar)
        toolbar_layout.setContentsMargins(8, 6, 8, 6)
        toolbar_layout.setSpacing(4)

        # Navigation buttons
        self.back_btn = QPushButton("←")
        self.back_btn.setToolTip("Back")
        self.back_btn.clicked.connect(self.go_back)
        self.back_btn.setEnabled(False)
        toolbar_layout.addWidget(self.back_btn)

        self.forward_btn = QPushButton("→")
        self.forward_btn.setToolTip("Forward")
        self.forward_btn.clicked.connect(self.go_forward)
        self.forward_btn.setEnabled(False)
        toolbar_layout.addWidget(self.forward_btn)

        self.refresh_btn = QPushButton("↻")
        self.refresh_btn.setToolTip("Refresh")
        self.refresh_btn.clicked.connect(self.refresh)
        toolbar_layout.addWidget(self.refresh_btn)

        toolbar_layout.addWidget(self._separator())

        # URL bar
        self.url_bar = QLineEdit()
        self.url_bar.setPlaceholderText("Enter URL or search...")
        self.url_bar.returnPressed.connect(self.navigate_to_url_bar)
        toolbar_layout.addWidget(self.url_bar)

        self.go_btn = QPushButton("Go")
        self.go_btn.clicked.connect(self.navigate_to_url_bar)
        toolbar_layout.addWidget(self.go_btn)

        toolbar_layout.addWidget(self._separator())

        # Search toggle
        self.search_toggle = QPushButton("🔍")
        self.search_toggle.setToolTip("Toggle Find")
        self.search_toggle.setMaximumWidth(40)
        self.search_toggle.clicked.connect(self.toggle_find)
        toolbar_layout.addWidget(self.search_toggle)

        layout.addWidget(self.toolbar)

        # Web view (only if QWebEngineView is available)
        if QWebEngineView:
            self.web_view = QWebEngineView()
            self.web_view.urlChanged.connect(self.on_url_changed)
            self.web_view.titleChanged.connect(self.on_title_changed)
            self.web_view.loadProgress.connect(self.on_load_progress)
            layout.addWidget(self.web_view)

            # Load start page
            self.load_start_page()
        else:
            # Fallback if QtWebEngine is not available
            error_label = QLabel(
                "QtWebEngine is not available.\n\n"
                "Please install PyQt6-WebEngine:\n"
                "pip install PyQt6-WebEngine"
            )
            error_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
            error_label.setStyleSheet("color: #666; padding: 20px;")
            layout.addWidget(error_label)
            self.web_view = None

        # Status bar
        self.status_label = QLabel("Ready")
        self.status_label.setStyleSheet("padding: 2px 8px; color: #666; font-size: 10px;")
        layout.addWidget(self.status_label)

        # Apply dark mode styling
        if self.dark_mode:
            self._apply_dark_mode()

    def _separator(self):
        """Create a separator widget."""
        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.VLine)
        sep.setFrameShadow(QFrame.Shadow.Sunken)
        return sep

    def load_start_page(self):
        """Load the semantic OS start page."""
        if not self.web_view:
            return

        html = """
<!DOCTYPE html>
<html>
<head>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            max-width: 800px;
            margin: 40px auto;
            padding: 20px;
            line-height: 1.6;
            color: #333;
        }
        h1 { color: #2563eb; }
        .shortcut {
            background: #f0f9ff;
            border: 1px solid #bae6fd;
            border-radius: 8px;
            padding: 16px;
            margin: 10px 0;
        }
        .shortcut h3 { margin-top: 0; color: #0369a1; }
        .shortcut a { color: #2563eb; text-decoration: none; }
        .shortcut a:hover { text-decoration: underline; }
        input {
            width: 100%;
            padding: 12px;
            font-size: 16px;
            border: 2px solid #e5e7eb;
            border-radius: 8px;
            box-sizing: border-box;
            margin: 10px 0;
        }
        input:focus {
            outline: none;
            border-color: #2563eb;
        }
        button {
            background: #2563eb;
            color: white;
            border: none;
            padding: 12px 24px;
            font-size: 16px;
            border-radius: 8px;
            cursor: pointer;
            margin-top: 10px;
        }
        button:hover { background: #1d4ed8; }
    </style>
</head>
<body>
    <h1>🌐 Semantic Browser</h1>
    <p>Your web activity is tracked semantically alongside your file work.</p>

    <div class="shortcut">
        <h3>🔍 Quick Search</h3>
        <input type="text" id="searchBox" placeholder="Search the web..." autofocus>
        <button onclick="search()">Search</button>
    </div>

    <div class="shortcut">
        <h3>📚 Quick Links</h3>
        <p><a href="https://github.com">GitHub</a></p>
        <p><a href="https://stackoverflow.com">Stack Overflow</a></p>
        <p><a href="https://docs.python.org">Python Docs</a></p>
        <p><a href="https://developer.mozilla.org">MDN Web Docs</a></p>
    </div>

    <script>
        const searchBox = document.getElementById('searchBox');
        searchBox.addEventListener('keypress', function (e) {
            if (e.key === 'Enter') search();
        });

        function search() {
            const query = searchBox.value;
            if (query) {
                // Check if it's a URL
                if (query.startsWith('http://') || query.startsWith('https://')) {
                    window.location.href = query;
                } else {
                    // Treat as search query
                    window.location.href = 'https://www.google.com/search?q=' + encodeURIComponent(query);
                }
            }
        }
    </script>
</body>
</html>
"""
        self.web_view.setHtml(html)

    def navigate_to_url_bar(self):
        """Navigate to the URL in the URL bar."""
        url = self.url_bar.text().strip()
        if not url:
            return

        # Check if it's a URL or a search query
        if url.startswith('http://') or url.startswith('https://'):
            self.navigate(url)
        else:
            # Treat as search query
            search_url = f"https://www.google.com/search?q={url}"
            self.navigate(search_url)

    def navigate(self, url):
        """Navigate to a URL."""
        if not self.web_view:
            return

        # Add to history
        if self.history_index < len(self.navigation_history) - 1:
            # Truncate forward history
            self.navigation_history = self.navigation_history[:self.history_index + 1]

        self.navigation_history.append(url)
        self.history_index = len(self.navigation_history) - 1

        self._update_nav_buttons()
        self.web_view.setUrl(QUrl(url))

        # Track semantically
        self._track_navigation(url)

    def go_back(self):
        """Go back in history."""
        if self.history_index > 0:
            self.history_index -= 1
            url = self.navigation_history[self.history_index]
            self.web_view.setUrl(QUrl(url))
            self._update_nav_buttons()

    def go_forward(self):
        """Go forward in history."""
        if self.history_index < len(self.navigation_history) - 1:
            self.history_index += 1
            url = self.navigation_history[self.history_index]
            self.web_view.setUrl(QUrl(url))
            self._update_nav_buttons()

    def refresh(self):
        """Refresh the current page."""
        if self.web_view:
            self.web_view.reload()

    def toggle_find(self):
        """Show find dialog."""
        # QWebEngineView has built-in find
        if self.web_view and QWebEnginePage:
            self.web_view.triggerPageAction(QWebEnginePage.WebAction.Find)
            self.url_bar.setFocus()
            self.url_bar.selectAll()

    def on_url_changed(self, url):
        """Handle URL change."""
        self.current_url = url.toString()
        self.url_bar.setText(self.current_url)

    def on_title_changed(self, title):
        """Handle page title change."""
        self.current_title = title
        # Update window title if this is the current tab
        parent = self.parent()
        while parent:
            if isinstance(parent, MarkdownEditor):
                parent.setWindowTitle(f"Markdown Editor - {title[:50]}")
                break
            parent = parent.parent() if hasattr(parent, 'parent') else None

    def on_load_progress(self, progress):
        """Handle load progress."""
        if progress < 100:
            self.status_label.setText(f"Loading: {progress}%")
        else:
            self.status_label.setText(f"Done: {self.current_title or self.current_url}")

            # Track page load completion
            self._track_page_load()

    def _update_nav_buttons(self):
        """Update navigation button states."""
        self.back_btn.setEnabled(self.history_index > 0)
        self.forward_btn.setEnabled(self.history_index < len(self.navigation_history) - 1)

    def _track_navigation(self, url):
        """Track navigation semantically."""
        if not self.kernel:
            return

        try:
            # Store in semantic memory
            self.kernel.memory.store(
                content=f"Navigated to: {url}",
                type="web_navigation",
                metadata={
                    "url": url,
                    "timestamp": time.time(),
                    "type": "navigation"
                }
            )

            # Emit event
            self.kernel.emit("web.navigation", {
                "url": url,
                "action": "navigate"
            })

            # Record focus for context layer
            self.kernel.record_focus(url)

        except Exception as e:
            print(f"[Semantic Browser] Error tracking navigation: {e}")

    def _track_page_load(self):
        """Track completed page load semantically."""
        if not self.kernel:
            return

        try:
            self.kernel.memory.store(
                content=f"Page loaded: {self.current_title or self.current_url}",
                type="web_page_load",
                metadata={
                    "url": self.current_url,
                    "title": self.current_title,
                    "timestamp": time.time(),
                    "type": "page_load"
                }
            )

            # Emit event
            self.kernel.emit("web.page_loaded", {
                "url": self.current_url,
                "title": self.current_title
            })

        except Exception as e:
            print(f"[Semantic Browser] Error tracking page load: {e}")

    def _apply_dark_mode(self):
        """Apply dark mode styling."""
        dark_style = """
            QWidget {
                background-color: #1e1e1e;
                color: #d4d4d4;
            }
            QLineEdit {
                background-color: #2d2d2d;
                color: #d4d4d4;
                border: 1px solid #444;
                padding: 4px;
                border-radius: 3px;
            }
            QPushButton {
                background-color: #3d3d3d;
                color: #d4d4d4;
                border: 1px solid #555;
                padding: 4px 12px;
                border-radius: 3px;
            }
            QPushButton:hover {
                background-color: #4d4d4d;
            }
            QPushButton:disabled {
                background-color: #2d2d2d;
                color: #666;
            }
        """
        self.setStyleSheet(dark_style)

    def set_url(self, url):
        """Set and navigate to a URL."""
        self.url_bar.setText(url)
        self.navigate(url)


class TerminalTab(QWidget):
    """A terminal emulator with semantic tracking of command-line work.

    Features:
    - Full shell/terminal integration (cmd.exe, bash, zsh)
    - Command history and tracking
    - Semantic correlation with file edits
    - Auto-completion
    - Syntax highlighting for output
    - Exportable command history for AI context
    """

    def __init__(self, kernel=None, dark_mode=False, parent=None):
        super().__init__(parent)
        self.kernel = kernel
        self.dark_mode = dark_mode

        # Terminal state
        self.command_history = []
        self.history_index = -1
        self.current_directory = os.path.abspath(os.getcwd())
        self.session_start = time.time()
        self.commands_run = 0
        self.failed_commands = 0

        # Determine shell to use
        self.shell = self._detect_shell()

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Toolbar
        self._create_toolbar(layout)

        # Terminal output area
        self.terminal_output = QPlainTextEdit()
        self.terminal_output.setReadOnly(True)
        self.terminal_output.setStyleSheet(self._get_terminal_style())
        layout.addWidget(self.terminal_output, 1)  # Give stretch factor 1

        # Command input
        input_layout = QHBoxLayout()
        input_layout.setContentsMargins(8, 4, 8, 4)

        self.prompt_label = QLabel(self._get_prompt())
        self.prompt_label.setStyleSheet("font-family: Consolas, monospace; font-weight: bold;")
        input_layout.addWidget(self.prompt_label)

        self.command_input = QLineEdit()
        self.command_input.setPlaceholderText("Enter command...")
        self.command_input.setStyleSheet(self._get_input_style())
        self.command_input.returnPressed.connect(self._execute_command)
        self.command_input.textChanged.connect(self._on_input_changed)
        input_layout.addWidget(self.command_input)

        layout.addLayout(input_layout)

        # Process for running shell
        from PyQt6.QtCore import QProcess
        self.shell_process = QProcess()
        self.shell_process.readyReadStandardOutput.connect(self._on_stdout)
        self.shell_process.readyReadStandardError.connect(self._on_stderr)
        self.shell_process.finished.connect(self._on_process_finished)

        # Initialize
        self._print_welcome()
        self._start_shell()

        # Apply dark mode
        if self.dark_mode:
            self._apply_dark_mode()

        # Track session in kernel
        if self.kernel:
            self.kernel.emit("terminal.session_started", {
                "shell": self.shell,
                "directory": self.current_directory,
                "timestamp": self.session_start
            })

    def _detect_shell(self):
        """Detect the appropriate shell for the platform."""
        if sys.platform == 'win32':
            # Check for PowerShell
            try:
                import subprocess
                result = subprocess.run(['pwsh', '--version'], capture_output=True)
                if result.returncode == 0:
                    return 'pwsh'
            except:
                pass

            # Check for Windows PowerShell
            try:
                import subprocess
                result = subprocess.run(['powershell', '--version'], capture_output=True)
                if result.returncode == 0:
                    return 'powershell'
            except:
                pass

            # Fallback to cmd.exe
            return 'cmd'
        else:
            # On Unix-like systems, prefer bash but check for others
            shells = ['zsh', 'bash', 'sh']
            for shell in shells:
                try:
                    import subprocess
                    result = subprocess.run(['which', shell], capture_output=True)
                    if result.returncode == 0:
                        return shell
                except:
                    pass
            return 'sh'

    def _create_toolbar(self, layout):
        """Create terminal toolbar."""
        toolbar = QWidget()
        toolbar_layout = QHBoxLayout(toolbar)
        toolbar_layout.setContentsMargins(8, 6, 8, 6)
        toolbar_layout.setSpacing(4)

        # Clear button
        clear_btn = QPushButton("Clear")
        clear_btn.setToolTip("Clear terminal output")
        clear_btn.clicked.connect(self._clear_output)
        toolbar_layout.addWidget(clear_btn)

        toolbar_layout.addWidget(self._separator())

        # Shell indicator
        shell_label = QLabel(f"Shell: {self.shell}")
        shell_label.setStyleSheet("color: #666; font-size: 11px;")
        toolbar_layout.addWidget(shell_label)

        # Directory indicator
        self.dir_label = QLabel(os.path.basename(self.current_directory))
        self.dir_label.setStyleSheet("color: #666; font-size: 11px;")
        self.dir_label.setToolTip(self.current_directory)
        toolbar_layout.addWidget(self.dir_label)

        toolbar_layout.addStretch()

        # Stats
        self.stats_label = QLabel("0 commands")
        self.stats_label.setStyleSheet("color: #666; font-size: 11px;")
        toolbar_layout.addWidget(self.stats_label)

        # Export button
        export_btn = QPushButton("Export History")
        export_btn.setToolTip("Export command history for AI context")
        export_btn.clicked.connect(self._export_history)
        toolbar_layout.addWidget(export_btn)

        layout.addWidget(toolbar)

    def _separator(self):
        """Create a separator widget."""
        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.VLine)
        sep.setFrameShadow(QFrame.Shadow.Sunken)
        return sep

    def _get_terminal_style(self):
        """Get terminal output style."""
        if self.dark_mode:
            return """
                QPlainTextEdit {
                    background-color: #0c0c0c;
                    color: #cccccc;
                    font-family: 'Cascadia Code', 'Consolas', 'Courier New', monospace;
                    font-size: 13px;
                    border: none;
                    padding: 8px;
                }
            """
        else:
            return """
                QPlainTextEdit {
                    background-color: #ffffff;
                    color: #000000;
                    font-family: 'Cascadia Code', 'Consolas', 'Courier New', monospace;
                    font-size: 13px;
                    border: 1px solid #ccc;
                    padding: 8px;
                }
            """

    def _get_input_style(self):
        """Get command input style."""
        if self.dark_mode:
            return """
                QLineEdit {
                    background-color: #1e1e1e;
                    color: #cccccc;
                    font-family: 'Cascadia Code', 'Consolas', 'Courier New', monospace;
                    font-size: 13px;
                    border: 1px solid #444;
                    padding: 6px 8px;
                    border-radius: 3px;
                }
                QLineEdit:focus {
                    border: 1px solid #007acc;
                }
            """
        else:
            return """
                QLineEdit {
                    background-color: #ffffff;
                    color: #000000;
                    font-family: 'Cascadia Code', 'Consolas', 'Courier New', monospace;
                    font-size: 13px;
                    border: 1px solid #ccc;
                    padding: 6px 8px;
                    border-radius: 3px;
                }
                QLineEdit:focus {
                    border: 1px solid #007acc;
                }
            """

    def _get_prompt(self):
        """Get the current prompt string."""
        if self.shell in ['bash', 'zsh', 'sh']:
            return f"$ "
        elif self.shell == 'powershell' or self.shell == 'pwsh':
            return "PS> "
        else:  # cmd
            return "> "

    def _print_welcome(self):
        """Print welcome message."""
        welcome = f"""
╔═══════════════════════════════════════════════════════════════╗
║           MarlOS Terminal - Context-Aware Shell           ║
╠═══════════════════════════════════════════════════════════════╣
║  All commands are tracked semantically for AI context          ║
║  Type 'help' for available commands                            ║
║  Shell: {self.shell:<15} Working Dir: {os.path.basename(self.current_directory):<20}     ║
╚═══════════════════════════════════════════════════════════════╝

"""
        self.terminal_output.appendPlainText(welcome.strip())

    def _start_shell(self):
        """Start the shell process."""
        if sys.platform == 'win32':
            if self.shell == 'powershell' or self.shell == 'pwsh':
                self.shell_process.start(self.shell, ['-NoExit', '-NoLogo'])
            else:
                self.shell_process.start('cmd.exe', [])
        else:
            self.shell_process.start(self.shell, [])

    def _execute_command(self):
        """Execute a command."""
        command = self.command_input.text().strip()
        if not command:
            return

        # Add to history
        self.command_history.append(command)
        self.history_index = len(self.command_history)

        # Display command
        self.terminal_output.appendPlainText(f"\n{self._get_prompt()}{command}")

        # Handle special commands
        if command.lower() in ['clear', 'cls']:
            self._clear_output()
            self.command_input.clear()
            return
        elif command.lower() == 'history':
            self._show_history()
            self.command_input.clear()
            return
        elif command.lower() in ['exit', 'quit']:
            self._close_session()
            self.command_input.clear()
            return
        elif command.lower() == 'help':
            self._show_help()
            self.command_input.clear()
            return
        elif command.lower().startswith('cd '):
            self._change_directory(command[3:].strip())
            self.command_input.clear()
            return

        # Track command in kernel
        self._track_command(command)

        # Execute the command
        try:
            if sys.platform == 'win32':
                # On Windows, use subprocess
                import subprocess
                result = subprocess.run(
                    command,
                    shell=True,
                    capture_output=True,
                    text=True,
                    cwd=self.current_directory
                )

                if result.stdout:
                    self.terminal_output.appendPlainText(result.stdout)

                if result.stderr:
                    self.terminal_output.appendPlainText(result.stderr)
                    self.failed_commands += 1
                else:
                    self.commands_run += 1

            else:
                # On Unix, could use shell process
                import subprocess
                result = subprocess.run(
                    command,
                    shell=True,
                    capture_output=True,
                    text=True,
                    cwd=self.current_directory
                )

                if result.stdout:
                    self.terminal_output.appendPlainText(result.stdout)

                if result.stderr:
                    self.terminal_output.appendPlainText(result.stderr)
                    self.failed_commands += 1
                else:
                    self.commands_run += 1

        except Exception as e:
            self.terminal_output.appendPlainText(f"Error: {e}")
            self.failed_commands += 1

        # Update stats
        self._update_stats()

        # Clear input
        self.command_input.clear()

        # Scroll to bottom
        cursor = self.terminal_output.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        self.terminal_output.setTextCursor(cursor)

    def _track_command(self, command):
        """Track command in semantic kernel."""
        if not self.kernel:
            return

        try:
            # Store command in memory
            self.kernel.memory.store(
                content=f"Terminal command: {command}",
                type="terminal_command",
                metadata={
                    "command": command,
                    "directory": self.current_directory,
                    "shell": self.shell,
                    "timestamp": time.time(),
                    "session_start": self.session_start
                }
            )

            # Emit event
            self.kernel.emit("terminal.command_executed", {
                "command": command,
                "directory": self.current_directory,
                "shell": self.shell,
                "timestamp": time.time()
            })

            # Try to extract files referenced
            self._extract_file_references(command)

        except Exception as e:
            print(f"[Terminal] Error tracking command: {e}")

    def _extract_file_references(self, command):
        """Extract file references from command."""
        import re
        import pathlib

        # Common patterns for file references
        patterns = [
            r'\b[\w\-./\\]+\.(py|js|ts|md|txt|json|yaml|yml|html|css)\b',
            r'\b[\w\-./\\]+/[\w\-./\\]*\b',
        ]

        files_found = []
        for pattern in patterns:
            matches = re.findall(pattern, command)
            files_found.extend(matches)

        # If files found, record them
        if files_found and self.kernel:
            for file_path in files_found:
                # Normalize path
                if not os.path.isabs(file_path):
                    file_path = os.path.join(self.current_directory, file_path)

                self.kernel.emit("terminal.file_referenced", {
                    "file_path": file_path,
                    "command": command,
                    "timestamp": time.time()
                })

    def _change_directory(self, path):
        """Change current directory."""
        try:
            if path == '~':
                new_dir = os.path.expanduser(path)
            elif not os.path.isabs(path):
                new_dir = os.path.join(self.current_directory, path)
            else:
                new_dir = path

            new_dir = os.path.abspath(new_dir)

            if os.path.isdir(new_dir):
                self.current_directory = new_dir
                self.dir_label.setText(os.path.basename(new_dir))
                self.dir_label.setToolTip(new_dir)
                self.prompt_label.setText(self._get_prompt())

                # Track in kernel
                if self.kernel:
                    self.kernel.emit("terminal.directory_changed", {
                        "directory": new_dir,
                        "timestamp": time.time()
                    })
            else:
                self.terminal_output.appendPlainText(f"cd: {path}: No such directory")

        except Exception as e:
            self.terminal_output.appendPlainText(f"cd: {e}")

    def _clear_output(self):
        """Clear terminal output."""
        self.terminal_output.clear()

    def _show_history(self):
        """Show command history."""
        self.terminal_output.appendPlainText("\n--- Command History ---")
        for i, cmd in enumerate(self.command_history, 1):
            self.terminal_output.appendPlainText(f"{i:4d}  {cmd}")

    def _show_help(self):
        """Show help message."""
        help_text = """
Semantic Terminal Commands:
  help        - Show this help
  history     - Show command history
  clear/cls   - Clear terminal output
  cd <path>   - Change directory
  exit/quit   - Close terminal session

All other commands are executed by the shell.
"""
        self.terminal_output.appendPlainText(help_text)

    def _update_stats(self):
        """Update statistics display."""
        self.stats_label.setText(f"{self.commands_run} commands")

    def _on_input_changed(self, text):
        """Handle input text changes."""
        # Could implement auto-completion here
        pass

    def _on_stdout(self):
        """Handle stdout from shell process."""
        data = self.shell_process.readAllStandardOutput()
        text = bytes(data).decode('utf-8', errors='ignore')
        self.terminal_output.appendPlainText(text)

    def _on_stderr(self):
        """Handle stderr from shell process."""
        data = self.shell_process.readAllStandardError()
        text = bytes(data).decode('utf-8', errors='ignore')
        self.terminal_output.appendPlainText(text)

    def _on_process_finished(self, exit_code, exit_status):
        """Handle process completion."""
        if exit_code != 0:
            self.terminal_output.appendPlainText(f"\n[Process exited with code {exit_code}]")

    def _export_history(self):
        """Export command history."""
        history = {
            "shell": self.shell,
            "session_start": self.session_start,
            "duration_minutes": (time.time() - self.session_start) / 60,
            "commands_run": self.commands_run,
            "failed_commands": self.failed_commands,
            "current_directory": self.current_directory,
            "commands": self.command_history
        }

        # Show dialog with export options
        dialog = QDialog(self)
        dialog.setWindowTitle("Export Terminal History")
        layout = QVBoxLayout(dialog)

        # Preview
        preview = QPlainTextEdit()
        preview.setReadOnly(True)
        preview.setPlainText(json.dumps(history, indent=2))
        layout.addWidget(QLabel("Command History (JSON):"))
        layout.addWidget(preview)

        # Buttons
        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Close
        )
        buttons.closeRequested.connect(dialog.close)
        buttons.addButton("Copy to Clipboard", QDialogButtonBox.ButtonRole.ActionRole).clicked.connect(
            lambda: QApplication.clipboard().setText(json.dumps(history, indent=2))
        )
        layout.addWidget(buttons)

        dialog.exec()

    def _close_session(self):
        """Close terminal session."""
        # Track session end in kernel
        if self.kernel:
            duration = time.time() - self.session_start
            self.kernel.emit("terminal.session_ended", {
                "shell": self.shell,
                "duration_seconds": duration,
                "commands_run": self.commands_run,
                "failed_commands": self.failed_commands,
                "timestamp": time.time()
            })

        # Emit signal to close tab
        self.parent().parent().tabs.removeTab(
            self.parent().parent().tabs.indexOf(self)
        )

    def _apply_dark_mode(self):
        """Apply dark mode styling."""
        pass  # Already handled in individual components

    def keyPressEvent(self, event):
        """Handle key press events."""
        # Handle up/down arrows for command history
        if event.key() == Qt.Key.Key_Up:
            if self.history_index > 0:
                self.history_index -= 1
                self.command_input.setText(self.command_history[self.history_index])
        elif event.key() == Qt.Key.Key_Down:
            if self.history_index < len(self.command_history) - 1:
                self.history_index += 1
                self.command_input.setText(self.command_history[self.history_index])
            else:
                self.history_index = len(self.command_history)
                self.command_input.clear()
        else:
            super().keyPressEvent(event)

    def closeEvent(self, event):
        """Handle tab close - clean up shell process."""
        try:
            # Disconnect signals to prevent crashes
            if hasattr(self, 'shell_process'):
                self.shell_process.readyReadStandardOutput.disconnect()
                self.shell_process.readyReadStandardError.disconnect()
                self.shell_process.finished.disconnect()

                # Kill the process if it's running
                if self.shell_process.state() == QProcess.ProcessState.Running:
                    self.shell_process.kill()
                    self.shell_process.waitForFinished(1000)  # Wait up to 1 second

            # Track session end
            if self.kernel:
                try:
                    duration = time.time() - self.session_start
                    self.kernel.emit("terminal.session_ended", {
                        "shell": self.shell,
                        "duration_seconds": duration,
                        "commands_run": self.commands_run,
                        "failed_commands": self.failed_commands,
                        "timestamp": time.time()
                    })
                except:
                    pass

        except Exception as e:
            print(f"[Terminal] Error during cleanup: {e}")

        # Accept the close event
        event.accept()


class AppLauncherTab(QWidget):
    """An application launcher that tracks external applications semantically.

    Features:
    - Launch any installed application
    - Track application lifetime and usage
    - Monitor window title changes
    - Correlate app usage with files and projects
    - Export app history for AI context
    - Favorite apps for quick access
    """

    def __init__(self, kernel=None, dark_mode=False, parent=None):
        super().__init__(parent)
        self.kernel = kernel
        self.dark_mode = dark_mode

        # App tracking state
        self.running_apps = {}  # {app_name: {pid, start_time, process}}
        self.app_history = []
        self.favorite_apps = []
        self.session_start = time.time()

        # Don't detect apps at startup - do it lazily in background
        self.installed_apps = []
        self.apps_loaded = False
        self.loading_label = None

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Toolbar
        self._create_toolbar(layout)

        # Status bar
        self.status_label = QLabel("Ready")
        self.status_label.setStyleSheet("padding: 2px 8px; color: #666; font-size: 10px;")

        # Main content area
        content = QWidget()
        content_layout = QVBoxLayout(content)
        content_layout.setContentsMargins(20, 20, 20, 20)

        # Loading indicator
        self.loading_label = QLabel("Scanning for applications...")
        self.loading_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.loading_label.setStyleSheet("color: #666; font-size: 14px; padding: 40px;")
        content_layout.addWidget(self.loading_label)

        # Quick launch section
        quick_launch_label = QLabel("Quick Launch")
        quick_launch_label.setStyleSheet("font-size: 16px; font-weight: bold; color: #333;")
        content_layout.addWidget(quick_launch_label)

        # Search/filter apps
        search_layout = QHBoxLayout()
        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("Search applications...")
        self.search_input.textChanged.connect(self._filter_apps)
        search_layout.addWidget(self.search_input)
        content_layout.addLayout(search_layout)

        # Apps grid
        self.apps_scroll = QScrollArea()
        self.apps_scroll.setWidgetResizable(True)
        self.apps_scroll.setMinimumHeight(400)

        self.apps_widget = QWidget()
        self.apps_layout = QGridLayout(self.apps_widget)
        self.apps_layout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self.apps_scroll.setWidget(self.apps_widget)

        content_layout.addWidget(self.apps_scroll)

        # Running apps section
        running_label = QLabel("Running Applications")
        running_label.setStyleSheet("font-size: 16px; font-weight: bold; color: #333; margin-top: 20px;")
        content_layout.addWidget(running_label)

        self.running_list = QListWidget()
        self.running_list.setMinimumHeight(150)
        self.running_list.itemDoubleClicked.connect(self._focus_app)
        content_layout.addWidget(self.running_list)

        layout.addWidget(content)

        # Start background app scanning when widget is shown
        self.load_apps_timer = QTimer()
        self.load_apps_timer.setSingleShot(True)
        self.load_apps_timer.timeout.connect(self._load_apps_background)

        # Start monitoring timer
        self.monitor_timer = QTimer()
        self.monitor_timer.timeout.connect(self._monitor_apps)
        self.monitor_timer.start(2000)  # Check every 2 seconds

        # Apply dark mode
        if self.dark_mode:
            self._apply_dark_mode()

    def showEvent(self, event):
        """Called when the widget is shown. Load apps in background if not loaded."""
        super().showEvent(event)
        # Start background loading when first shown
        if not self.apps_loaded:
            self.load_apps_timer.start(100)  # Small delay to let UI render first

    def _load_apps_background(self):
        """Load applications in background thread."""
        if self.apps_loaded:
            return

        # Update loading label
        if self.loading_label:
            self.loading_label.setText("Scanning for applications...")

        # Use QTimer to do scanning in small chunks, keeping UI responsive
        QTimer.singleShot(50, self._detect_apps_step1)

    def _detect_apps_step1(self):
        """First step of app detection - update UI and start detection."""
        # Load apps (custom + system)
        self._refresh_apps()

        # Hide loading label
        if self.loading_label:
            self.loading_label.hide()

        self.apps_loaded = True

        # Track session in kernel
        if self.kernel:
            self.kernel.emit("app_launcher.session_started", {
                "installed_apps": len(self.installed_apps),
                "timestamp": self.session_start
            })

    def _detect_installed_apps(self):
        """Detect installed applications on the system."""
        apps = []

        if sys.platform == 'win32':
            # Use shutil.which for fast lookup (uses system PATH)
            import shutil
            import os

            # Common Windows apps - just check if they're in PATH
            # Much faster than walking directories!
            app_names = [
                ("code.exe", "Visual Studio Code", "code"),
                ("notepad++.exe", "Notepad++", "notepad++"),
                ("notepad.exe", "Notepad", "notepad"),
                ("mspaint.exe", "Paint", "paint"),
                ("chrome.exe", "Google Chrome", "chrome"),
                ("firefox.exe", "Mozilla Firefox", "firefox"),
                ("msedge.exe", "Microsoft Edge", "edge"),
                ("spotify.exe", "Spotify", "spotify"),
                ("discord.exe", "Discord", "discord"),
                ("slack.exe", "Slack", "slack"),
                ("zoom.exe", "Zoom", "zoom"),
                ("teams.exe", "Microsoft Teams", "teams"),
                ("explorer.exe", "File Explorer", "explorer"),
            ]

            # Fast lookup using shutil.which (uses system PATH)
            for exe_name, display_name, icon_name in app_names:
                exe_path = shutil.which(exe_name)
                if exe_path:
                    apps.append({
                        "name": display_name,
                        "exe": exe_name,
                        "path": exe_path,
                        "icon": icon_name,
                        "category": self._get_category(display_name)
                    })

            # Also add some hard-coded common paths for apps not in PATH
            # This is instant - no scanning required
            hardcoded_apps = [
                (r"C:\Program Files\Microsoft Office\root\Office16\EXCEL.EXE", "Microsoft Excel", "excel"),
                (r"C:\Program Files\Microsoft Office\root\Office16\WINWORD.EXE", "Microsoft Word", "word"),
                (r"C:\Program Files\Microsoft Office\root\Office16\POWERPNT.EXE", "Microsoft PowerPoint", "powerpoint"),
            ]

            for path, display_name, icon_name in hardcoded_apps:
                if os.path.exists(path):
                    apps.append({
                        "name": display_name,
                        "exe": os.path.basename(path),
                        "path": path,
                        "icon": icon_name,
                        "category": self._get_category(display_name)
                    })

        elif sys.platform == 'darwin':
            # macOS: Check /Applications
            import os

            apps_dir = "/Applications"
            if os.path.exists(apps_dir):
                for app in os.listdir(apps_dir):
                    if app.endswith(".app"):
                        app_name = app.replace(".app", "")
                        app_path = os.path.join(apps_dir, app)
                        apps.append({
                            "name": app_name,
                            "exe": app,
                            "path": app_path,
                            "icon": app_name.lower(),
                            "category": self._get_category(app_name)
                        })

        else:
            # Linux: Check common desktop files
            import os

            desktop_dirs = [
                "/usr/share/applications",
                os.path.expanduser("~/.local/share/applications"),
            ]

            for desktop_dir in desktop_dirs:
                if os.path.exists(desktop_dir):
                    for desktop_file in os.listdir(desktop_dir):
                        if desktop_file.endswith(".desktop"):
                            # Parse .desktop file
                            desktop_path = os.path.join(desktop_dir, desktop_file)
                            app_info = self._parse_desktop_file(desktop_path)
                            if app_info:
                                apps.append(app_info)

        return apps

    def _find_executable(self, exe_name, search_paths):
        """Find executable in common paths."""
        import os

        for base_path in search_paths:
            if not os.path.exists(base_path):
                continue

            for root, dirs, files in os.walk(base_path):
                if exe_name in files:
                    return os.path.join(root, exe_name)

                # Limit depth to avoid long searches
                if len(root.split(os.sep)) - len(base_path.split(os.sep)) > 3:
                    dirs[:] = []  # Don't recurse further

        # Try system PATH
        import shutil
        system_path = shutil.which(exe_name)
        if system_path:
            return system_path

        return None

    def _parse_desktop_file(self, desktop_path):
        """Parse Linux .desktop file."""
        try:
            with open(desktop_path, 'r') as f:
                name = None
                exec_cmd = None
                icon = None

                for line in f:
                    if line.startswith('Name='):
                        name = line.split('=', 1)[1].strip()
                    elif line.startswith('Exec='):
                        exec_cmd = line.split('=', 1)[1].strip()
                    elif line.startswith('Icon='):
                        icon = line.split('=', 1)[1].strip()

                if name and exec_cmd:
                    return {
                        "name": name,
                        "exe": exec_cmd.split()[0] if exec_cmd else "",
                        "path": exec_cmd,
                        "icon": icon or name.lower(),
                        "category": self._get_category(name)
                    }
        except:
            pass

        return None

    def _get_category(self, app_name):
        """Categorize application."""
        app_name_lower = app_name.lower()

        if any(x in app_name_lower for x in ['code', 'vim', 'emacs', 'intellij', 'pycharm', 'sublime']):
            return 'Development'
        elif any(x in app_name_lower for x in ['chrome', 'firefox', 'edge', 'safari']):
            return 'Web'
        elif any(x in app_name_lower for x in ['word', 'excel', 'powerpoint', 'office']):
            return 'Office'
        elif any(x in app_name_lower for x in ['photoshop', 'illustrator', 'paint', 'gimp']):
            return 'Graphics'
        elif any(x in app_name_lower for x in ['spotify', 'vlc', 'itunes']):
            return 'Media'
        elif any(x in app_name_lower for x in ['slack', 'discord', 'teams', 'zoom']):
            return 'Communication'
        else:
            return 'Other'

    def _create_toolbar(self, layout):
        """Create toolbar."""
        toolbar = QWidget()
        toolbar_layout = QHBoxLayout(toolbar)
        toolbar_layout.setContentsMargins(8, 6, 8, 6)
        toolbar_layout.setSpacing(4)

        # Add app button
        add_app_btn = QPushButton("Add App")
        add_app_btn.setToolTip("Add any application to the launcher")
        add_app_btn.clicked.connect(self._add_custom_app)
        toolbar_layout.addWidget(add_app_btn)

        toolbar_layout.addWidget(self._separator())

        # Refresh button
        refresh_btn = QPushButton("Refresh Apps")
        refresh_btn.setToolTip("Re-scan installed applications")
        refresh_btn.clicked.connect(self._refresh_apps)
        toolbar_layout.addWidget(refresh_btn)

        toolbar_layout.addStretch()

        # Stats
        self.stats_label = QLabel(f"{len(self.installed_apps)} apps")
        self.stats_label.setStyleSheet("color: #666; font-size: 11px;")
        toolbar_layout.addWidget(self.stats_label)

        # Export button
        export_btn = QPushButton("Export History")
        export_btn.setToolTip("Export app usage history for AI context")
        export_btn.clicked.connect(self._export_history)
        toolbar_layout.addWidget(export_btn)

        layout.addWidget(toolbar)

    def _populate_apps(self, filter_text=""):
        """Populate apps grid."""
        # Clear existing
        for i in reversed(range(self.apps_layout.count())):
            self.apps_layout.itemAt(i).widget().setParent(None)

        # Filter and sort apps
        filtered_apps = [
            app for app in self.installed_apps
            if filter_text.lower() in app["name"].lower()
        ]

        # Sort by category, then name
        filtered_apps.sort(key=lambda x: (x["category"], x["name"]))

        # Group by category
        categories = {}
        for app in filtered_apps:
            cat = app["category"]
            if cat not in categories:
                categories[cat] = []
            categories[cat].append(app)

        # Display
        row, col = 0, 0
        max_cols = 4

        for category, apps in sorted(categories.items()):
            # Category header
            cat_label = QLabel(category)
            cat_label.setStyleSheet("font-weight: bold; color: #666; margin-top: 10px;")
            self.apps_layout.addWidget(cat_label, row, 0, 1, max_cols)
            row += 1

            # Apps in category
            for app in apps:
                app_btn = self._create_app_button(app)
                self.apps_layout.addWidget(app_btn, row, col)
                col += 1
                if col >= max_cols:
                    col = 0
                    row += 1

            # Add spacing after category
            row += 1
            col = 0

    def _create_app_button(self, app):
        """Create app launch button."""
        btn = QPushButton()
        btn.setMinimumSize(120, 100)
        btn.setMaximumSize(120, 100)

        # Simple layout with icon and name
        layout = QVBoxLayout(btn)
        layout.setAlignment(Qt.AlignmentFlag.AlignCenter)

        # Icon (using emoji for now)
        icon_map = {
            'Development': '',
            'Web': '',
            'Office': '',
            'Graphics': "",
            'Media': "",
            'Communication': "",
            'Other': ""
        }
        icon = icon_map.get(app['category'], "")

        icon_label = QLabel(icon)
        icon_label.setStyleSheet("font-size: 32px;")
        icon_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(icon_label)

        # Name (wrapped)
        name_label = QLabel(app["name"])
        name_label.setWordWrap(True)
        name_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        name_label.setStyleSheet("font-size: 11px;")
        layout.addWidget(name_label)

        # Click handler
        btn.clicked.connect(lambda: self._launch_app(app))

        return btn

    def _launch_app(self, app):
        """Launch an application."""
        pid = None
        process = None

        try:
            import subprocess
            import os

            if sys.platform == 'win32':
                # Windows: use start command or subprocess
                if app["path"]:
                    process = subprocess.Popen(
                        app["path"],
                        shell=True
                    )
                    pid = process.pid
                else:
                    # Fallback: use start command
                    subprocess.Popen(["start", app["exe"]], shell=True)
                    pid = None
            elif sys.platform == 'darwin':
                # macOS: use open command
                process = subprocess.Popen(
                    ["open", app["path"]],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL
                )
                pid = process.pid
            else:
                # Linux: use app path directly
                process = subprocess.Popen(
                    app["path"].split(),
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL
                )
                pid = process.pid

            # Track the launched app
            if pid:
                self.running_apps[app["name"]] = {
                    "pid": pid,
                    "start_time": time.time(),
                    "process": process,
                    "app_info": app
                }

                # Update running list
                self._update_running_list()

            # Track in kernel (with error handling)
            try:
                if self.kernel and pid:
                    self.kernel.emit("app.launched", {
                        "app_name": app["name"],
                        "exe": app["exe"],
                        "pid": pid,
                        "timestamp": time.time()
                    })

                    # Store in memory
                    self.kernel.memory.store(
                        content=f"Launched application: {app['name']}",
                        type="app_launch",
                        metadata={
                            "app_name": app["name"],
                            "exe": app["exe"],
                            "category": app["category"],
                            "pid": pid,
                            "timestamp": time.time()
                        }
                    )
            except Exception as kernel_error:
                print(f"[AppLauncher] Warning: Kernel tracking failed: {kernel_error}")

            # Show feedback
            if hasattr(self, 'status_label'):
                self.status_label.setText(f"Launched {app['name']}")

        except Exception as e:
            import traceback
            print(f"[AppLauncher] Launch error: {e}")
            traceback.print_exc()

            QMessageBox.warning(
                self,
                "Launch Error",
                f"Failed to launch {app.get('name', 'application')}: {e}"
            )

    def _monitor_apps(self):
        """Monitor running applications."""
        import subprocess

        still_running = {}

        for app_name, app_info in self.running_apps.items():
            pid = app_info["pid"]

            # Check if process is still running
            if sys.platform == 'win32':
                try:
                    # On Windows, use tasklist
                    result = subprocess.run(
                        ["tasklist", "/FI", f"PID eq {pid}"],
                        capture_output=True,
                        text=True
                    )
                    is_running = str(pid) in result.stdout
                except:
                    is_running = False
            else:
                try:
                    # On Unix, use ps
                    os.kill(pid, 0)  # Test if process exists
                    is_running = True
                except OSError:
                    is_running = False

            if is_running:
                still_running[app_name] = app_info
            else:
                # App ended
                duration = time.time() - app_info["start_time"]

                # Track in kernel
                if self.kernel:
                    self.kernel.emit("app.closed", {
                        "app_name": app_name,
                        "pid": pid,
                        "duration_seconds": duration,
                        "timestamp": time.time()
                    })

                    # Store in memory
                    self.kernel.memory.store(
                        content=f"Closed application: {app_name} (ran for {duration:.1f} seconds)",
                        type="app_close",
                        metadata={
                            "app_name": app_name,
                            "duration": duration,
                            "timestamp": time.time()
                        }
                    )

        self.running_apps = still_running
        self._update_running_list()

    def _update_running_list(self):
        """Update the running apps list."""
        self.running_list.clear()

        for app_name, app_info in self.running_apps.items():
            duration = time.time() - app_info["start_time"]
            item = QListWidgetItem(
                f"{app_name} (PID: {app_info['pid']}, {int(duration)}s)"
            )
            item.setData(Qt.ItemDataRole.UserRole, app_name)
            self.running_list.addItem(item)

    def _focus_app(self, item):
        """Focus on a running application."""
        app_name = item.data(Qt.ItemDataRole.UserRole)
        # Could implement bringing app to foreground here
        QMessageBox.information(
            self,
            "App Info",
            f"{app_name} is running.\n\n"
            f"PID: {self.running_apps[app_name]['pid']}\n"
            f"Running for: {int(time.time() - self.running_apps[app_name]['start_time'])}s"
        )

    def _filter_apps(self, text):
        """Filter apps by search text."""
        self._populate_apps(text)

    def _separator(self):
        """Create a separator widget."""
        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.VLine)
        sep.setFrameShadow(QFrame.Shadow.Sunken)
        return sep

    def _add_custom_app(self):
        """Add a custom application to the launcher."""
        # Open file dialog to select executable
        file_path, _ = QFileDialog.getOpenFileName(
            self,
            "Select Application",
            "",
            "Executables (*.exe *.bat *.cmd *.lnk);;All Files (*)"
        )

        if not file_path:
            return

        # Ask for app name
        import os
        default_name = os.path.splitext(os.path.basename(file_path))[0]
        app_name, ok = QInputDialog.getText(
            self,
            "Add Application",
            "Application name:",
            text=default_name
        )

        if not ok or not app_name.strip():
            return

        # Add to installed apps
        new_app = {
            "name": app_name.strip(),
            "exe": os.path.basename(file_path),
            "path": file_path,
            "icon": app_name.strip().lower(),
            "category": self._get_category(app_name),
            "custom": True  # Mark as custom app
        }

        self.installed_apps.append(new_app)
        self._populate_apps()
        self.stats_label.setText(f"{len(self.installed_apps)} apps")

        # Save custom apps to file for persistence
        self._save_custom_apps()

    def _save_custom_apps(self):
        """Save custom apps to a JSON file for persistence."""
        import os
        custom_apps = [app for app in self.installed_apps if app.get("custom", False)]

        if not custom_apps:
            return

        try:
            config_dir = os.path.expanduser("~/.semantic_os")
            os.makedirs(config_dir, exist_ok=True)
            config_file = os.path.join(config_dir, "custom_apps.json")

            with open(config_file, 'w') as f:
                json.dump(custom_apps, f, indent=2)

        except Exception as e:
            print(f"[AppLauncher] Failed to save custom apps: {e}")

    def _load_custom_apps(self):
        """Load custom apps from JSON file."""
        import os

        try:
            config_dir = os.path.expanduser("~/.semantic_os")
            config_file = os.path.join(config_dir, "custom_apps.json")

            if not os.path.exists(config_file):
                return []

            with open(config_file, 'r') as f:
                custom_apps = json.load(f)

            return custom_apps

        except Exception as e:
            print(f"[AppLauncher] Failed to load custom apps: {e}")
            return []

    def _refresh_apps(self):
        """Refresh installed apps list."""
        # First, load custom apps
        custom_apps = self._load_custom_apps()

        # Then detect system apps
        system_apps = self._detect_installed_apps()

        # Combine (custom apps first, then system apps)
        self.installed_apps = custom_apps + system_apps

        # Remove duplicates based on path
        seen_paths = set()
        unique_apps = []
        for app in self.installed_apps:
            if app["path"] not in seen_paths:
                unique_apps.append(app)
                seen_paths.add(app["path"])

        self.installed_apps = unique_apps
        self._populate_apps()
        self.stats_label.setText(f"{len(self.installed_apps)} apps")

    def _export_history(self):
        """Export app usage history."""
        history = {
            "session_start": self.session_start,
            "duration_minutes": (time.time() - self.session_start) / 60,
            "installed_apps": len(self.installed_apps),
            "currently_running": len(self.running_apps),
            "running_apps": [
                {
                    "name": name,
                    "pid": info["pid"],
                    "duration_seconds": time.time() - info["start_time"]
                }
                for name, info in self.running_apps.items()
            ]
        }

        # Show dialog
        dialog = QDialog(self)
        dialog.setWindowTitle("Export App History")
        layout = QVBoxLayout(dialog)

        preview = QPlainTextEdit()
        preview.setReadOnly(True)
        preview.setPlainText(json.dumps(history, indent=2))
        layout.addWidget(QLabel("App Usage History (JSON):"))
        layout.addWidget(preview)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Close
        )
        buttons.closeRequested.connect(dialog.close)
        buttons.addButton("Copy to Clipboard", QDialogButtonBox.ButtonRole.ActionRole).clicked.connect(
            lambda: QApplication.clipboard().setText(json.dumps(history, indent=2))
        )
        layout.addWidget(buttons)

        dialog.exec()

    def _apply_dark_mode(self):
        """Apply dark mode styling."""
        dark_style = """
            QWidget {
                background-color: #1e1e1e;
                color: #d4d4d4;
            }
            QLineEdit {
                background-color: #2d2d2d;
                color: #d4d4d4;
                border: 1px solid #444;
                padding: 6px;
                border-radius: 3px;
            }
            QPushButton {
                background-color: #3d3d3d;
                color: #d4d4d4;
                border: 1px solid #555;
                padding: 8px;
                border-radius: 4px;
                min-height: 60px;
            }
            QPushButton:hover {
                background-color: #4d4d4d;
            }
            QListWidget {
                background-color: #2d2d2d;
                color: #d4d4d4;
                border: 1px solid #444;
                border-radius: 3px;
            }
            QScrollArea {
                border: none;
            }
        """
        self.setStyleSheet(dark_style)


class GridWindowManager(QWidget):
    """A grid-based window manager with drag-and-drop functionality.

    Allows arranging tabs/windows in a customizable grid layout.
    You can drag tabs into different grid cells, resize cells, and
    rearrange your workspace.
    """

    def __init__(self, kernel=None, dark_mode=False, rows=2, cols=2, parent=None):
        super().__init__(parent)
        self.kernel = kernel
        self.dark_mode = dark_mode
        self.rows = rows
        self.cols = cols

        # Store widgets in grid cells
        self.grid_widgets = {}  # (row, col) -> widget

        self.setAcceptDrops(True)
        self.setup_ui()

    def setup_ui(self):
        """Setup the grid UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Toolbar
        toolbar = self._create_toolbar()
        layout.addWidget(toolbar)

        # Grid container
        self.grid_widget = QWidget()
        self.grid_layout = QGridLayout(self.grid_widget)
        self.grid_layout.setSpacing(2)
        self.grid_layout.setContentsMargins(2, 2, 2, 2)

        # Create initial grid cells
        self._create_grid_cells()

        layout.addWidget(self.grid_widget)

        # Apply styling
        if self.dark_mode:
            self._apply_dark_mode()

    def _create_toolbar(self):
        """Create toolbar for grid controls."""
        from PyQt6.QtWidgets import QToolBar

        toolbar = QWidget()
        toolbar_layout = QHBoxLayout(toolbar)
        toolbar_layout.setContentsMargins(8, 6, 8, 6)

        # Add row button
        add_row_btn = QPushButton("+ Row")
        add_row_btn.clicked.connect(self.add_row)
        toolbar_layout.addWidget(add_row_btn)

        # Add column button
        add_col_btn = QPushButton("+ Column")
        add_col_btn.clicked.connect(self.add_column)
        toolbar_layout.addWidget(add_col_btn)

        # Reset button
        reset_btn = QPushButton("Reset Grid")
        reset_btn.clicked.connect(lambda: self._reset_grid(2, 2))
        toolbar_layout.addWidget(reset_btn)

        toolbar_layout.addStretch()

        # Info label
        self.info_label = QLabel(f"Grid: {self.rows}x{self.cols}")
        toolbar_layout.addWidget(self.info_label)

        return toolbar

    def _create_grid_cells(self):
        """Create initial grid cells."""
        # Clear existing
        for i in reversed(range(self.grid_layout.count())):
            self.grid_layout.itemAt(i).widget().setParent(None)

        self.grid_widgets = {}

        # Create cells
        for row in range(self.rows):
            for col in range(self.cols):
                cell = GridCell(row, col, self.dark_mode, self)
                cell.widget_dropped.connect(self._on_widget_dropped)
                cell.widget_removed.connect(self._on_widget_removed)
                self.grid_layout.addWidget(cell, row, col)
                self.grid_widgets[(row, col)] = cell

        # Make rows/columns resizable
        for row in range(self.rows):
            self.grid_layout.setRowStretch(row, 1)
        for col in range(self.cols):
            self.grid_layout.setColumnStretch(col, 1)

    def _on_widget_dropped(self, row, col, widget):
        """Handle widget dropped into a cell."""
        self.grid_widgets[(row, col)].set_widget(widget)

        # Track in kernel
        if self.kernel:
            self.kernel.emit("grid.widget_added", {
                "row": row,
                "col": col,
                "widget_type": type(widget).__name__,
                "timestamp": time.time()
            })

    def _on_widget_removed(self, row, col):
        """Handle widget removed from a cell."""
        if self.kernel:
            self.kernel.emit("grid.widget_removed", {
                "row": row,
                "col": col,
                "timestamp": time.time()
            })

    def add_row(self):
        """Add a new row to the grid."""
        # Remove old widgets
        for col in range(self.cols):
            widget = self.grid_widgets.get((self.rows - 1, col))
            if widget:
                self.grid_layout.removeWidget(widget)

        self.rows += 1
        self._create_grid_cells()
        self.info_label.setText(f"Grid: {self.rows}x{self.cols}")

    def add_column(self):
        """Add a new column to the grid."""
        self.cols += 1
        self._create_grid_cells()
        self.info_label.setText(f"Grid: {self.rows}x{self.cols}")

    def _reset_grid(self, rows, cols):
        """Reset grid to specified dimensions."""
        self.rows = rows
        self.cols = cols
        self._create_grid_cells()
        self.info_label.setText(f"Grid: {self.rows}x{self.cols}")

    def _apply_dark_mode(self):
        """Apply dark mode styling."""
        self.setStyleSheet("""
            QWidget {
                background-color: #1e1e1e;
                color: #d4d4d4;
            }
            QPushButton {
                background-color: #3d3d3d;
                color: #d4d4d4;
                border: 1px solid #555;
                padding: 4px 8px;
                border-radius: 3px;
            }
            QPushButton:hover {
                background-color: #4d4d4d;
            }
        """)


class GridCell(QFrame):
    """A single cell in the grid that can hold a widget."""

    widget_dropped = pyqtSignal(int, int, QWidget)  # row, col, widget
    widget_removed = pyqtSignal(int, int)  # row, col

    def __init__(self, row, col, dark_mode=False, parent=None):
        super().__init__(parent)
        self.row = row
        self.col = col
        self.dark_mode = dark_mode
        self.widget = None

        self.setAcceptDrops(True)
        self.setFrameShape(QFrame.Shape.StyledPanel)
        self.setup_ui()

    def setup_ui(self):
        """Setup the cell UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Placeholder when empty
        self.placeholder = QLabel("Drop tab here")
        self.placeholder.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.placeholder.setStyleSheet("color: #666; font-size: 11px;")
        layout.addWidget(self.placeholder)

        # Apply cell styling
        self._update_style()

    def _update_style(self):
        """Update cell style based on state."""
        if self.widget:
            self.setStyleSheet("""
                QFrame {
                    border: 1px solid #444;
                    background-color: #2d2d2d;
                    border-radius: 4px;
                }
            """)
        else:
            self.setStyleSheet("""
                QFrame {
                    border: 2px dashed #666;
                    background-color: #1e1e1e;
                    border-radius: 4px;
                }
            """)

    def set_widget(self, widget):
        """Set the widget in this cell."""
        # Remove existing widget if any
        if self.widget:
            self.layout().removeWidget(self.widget)
            self.widget.setParent(None)

        self.widget = widget
        self.placeholder.setVisible(False)

        # Add widget to layout
        self.layout().addWidget(widget)

        # Make sure widget is visible
        widget.setVisible(True)
        widget.show()

        # Update the layout
        self.layout().update()

        self._update_style()

    def remove_widget(self):
        """Remove the widget from this cell."""
        if self.widget:
            self.layout().removeWidget(self.widget)
            self.widget = None
            self.placeholder.setVisible(True)
            self._update_style()
            self.widget_removed.emit(self.row, self.col)

    def dragEnterEvent(self, event):
        """Handle drag enter event."""
        if event.mimeData().hasFormat("application/x-tab"):
            event.acceptProposedAction()
            self.setStyleSheet("""
                QFrame {
                    border: 2px dashed #007acc;
                    background-color: #2a2d2e;
                    border-radius: 4px;
                }
            """)

    def dragLeaveEvent(self, event):
        """Handle drag leave event."""
        self._update_style()

    def dropEvent(self, event):
        """Handle drop event."""
        # Get the widget data
        data = event.mimeData().data("application/x-tab")
        # For now, this is a placeholder - actual implementation would
        # need to serialize/deserialize widgets

        self._update_style()

        # Emit signal (in real implementation, would pass actual widget)
        self.widget_dropped.emit(self.row, self.col, None)


class DiffViewer(QWidget):
    """Widget to display file changes with before/after diff view."""

    applied = pyqtSignal()  # Signal when changes are applied
    rejected = pyqtSignal()  # Signal when changes are rejected

    def __init__(self, file_path, old_content, new_content, parent=None):
        super().__init__(parent)
        self.file_path = file_path
        self.old_content = old_content
        self.new_content = new_content

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

        # Header with file path
        header = QLabel(f"📝 {file_path}")
        header.setStyleSheet("font-weight: bold; font-size: 13px; color: #333;")
        layout.addWidget(header)

        # Diff view
        self.diff_view = QPlainTextEdit()
        self.diff_view.setReadOnly(True)
        self.diff_view.setFont(QFont("Consolas", 10))
        self.diff_view.setStyleSheet("""
            QPlainTextEdit {
                background-color: #f5f5f5;
                border: 1px solid #ddd;
                padding: 8px;
            }
        """)
        self.diff_view.setPlainText(self._generate_diff())
        layout.addWidget(self.diff_view)

        # Button row
        button_row = QHBoxLayout()
        button_row.setSpacing(8)

        self.apply_btn = QPushButton("✓ Apply Changes")
        self.apply_btn.setStyleSheet("""
            QPushButton {
                background-color: #4CAF50;
                color: white;
                padding: 6px 16px;
                border-radius: 4px;
                font-weight: bold;
            }
            QPushButton:hover {
                background-color: #45a049;
            }
        """)
        self.apply_btn.clicked.connect(self._apply_changes)
        button_row.addWidget(self.apply_btn)

        self.reject_btn = QPushButton("✗ Reject")
        self.reject_btn.setStyleSheet("""
            QPushButton {
                background-color: #f44336;
                color: white;
                padding: 6px 16px;
                border-radius: 4px;
                font-weight: bold;
            }
            QPushButton:hover {
                background-color: #da190b;
            }
        """)
        self.reject_btn.clicked.connect(self._reject_changes)
        button_row.addWidget(self.reject_btn)

        button_row.addStretch()
        layout.addLayout(button_row)

    def _generate_diff(self):
        """Generate a unified diff showing changes."""
        import difflib
        old_lines = self.old_content.splitlines(keepends=True)
        new_lines = self.new_content.splitlines(keepends=True)

        diff = difflib.unified_diff(
            old_lines,
            new_lines,
            fromfile=f"Original: {self.file_path}",
            tofile=f"Modified: {self.file_path}",
            lineterm=""
        )

        return "".join(diff)

    def _apply_changes(self):
        """Apply the changes to the file."""
        try:
            # Write new content to file
            with open(self.file_path, 'w', encoding='utf-8') as f:
                f.write(self.new_content)

            self.applied.emit()
        except Exception as e:
            QMessageBox.warning(self, "Error", f"Failed to apply changes: {e}")

    def _reject_changes(self):
        """Reject the changes."""
        self.rejected.emit()


class FileOperationWidget(QWidget):
    """Widget to display a file operation in chat."""

    def __init__(self, operation, file_path, details="", parent=None):
        super().__init__(parent)
        self.operation = operation  # "read", "write", "create", "delete"
        self.file_path = file_path
        self.details = details

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(4)

        # Icon based on operation
        icons = {
            "read": "📖",
            "write": "✏️",
            "create": "📄",
            "delete": "🗑️",
            "ingest": "📦",
        }
        icon = icons.get(operation, "📁")

        # Operation label
        op_label = QLabel(f"{icon} {operation.upper()}: {file_path}")
        op_label.setStyleSheet("font-weight: bold; font-size: 12px;")

        # Color by operation type
        colors = {
            "read": "#2196F3",
            "write": "#FF9800",
            "create": "#4CAF50",
            "delete": "#F44336",
            "ingest": "#9C27B0",
        }
        color = colors.get(operation, "#666")
        op_label.setStyleSheet(f"font-weight: bold; font-size: 12px; color: {color};")

        layout.addWidget(op_label)

        # Details if provided
        if details:
            details_label = QLabel(details)
            details_label.setStyleSheet("color: #666; font-size: 11px; padding-left: 20px;")
            details_label.setWordWrap(True)
            layout.addWidget(details_label)


class LLMWorker(QThread):
    """Background worker for LLM API calls."""
    finished = pyqtSignal(object)  # Changed from str to object to support dict
    error = pyqtSignal(str)

    def __init__(self, call_fn, messages, parent=None):
        super().__init__(parent)
        self.call_fn = call_fn
        self.messages = messages

    def run(self):
        try:
            result = self.call_fn(self.messages)
            self.finished.emit(result)
        except Exception as e:
            self.error.emit(str(e))


class ChatPanel(QWidget):
    """Chat panel for interacting with the LLM.

    Provides a conversational interface to LM Studio with:
    - Message history display
    - Input field for user messages
    - Context buttons to include selection or document
    - Provider quick actions (Intent Map, Explain, etc.)
    - Slash commands for power users
    """

    # Slash commands - only essential file operations
    SLASH_COMMANDS = {
        "/read": ("file_read", "Read a file"),
        "/write": ("file_write", "Write to a file"),
        "/ingest": ("file_ingest", "Ingest a large file into context"),
        "/ls": ("file_list", "List files"),
        "/help": (None, "Show available commands"),
    }

    def __init__(self, ai_client, get_context_callback=None, execute_provider_callback=None, get_spine_callback=None, get_kernel_callback=None, get_manifest_callback=None, parent=None):
        super().__init__(parent)
        self.ai_client = ai_client
        self.get_context_callback = get_context_callback
        self.execute_provider_callback = execute_provider_callback
        self.get_spine_callback = get_spine_callback
        self.get_kernel_callback = get_kernel_callback
        self.get_manifest_callback = get_manifest_callback
        self.messages = []  # Chat history for context

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

        # Chat display area
        self.chat_display = QTextBrowser()
        self.chat_display.setOpenExternalLinks(True)
        self.chat_display.setFont(QFont("Segoe UI", 10))
        self.chat_display.setStyleSheet("""
            QTextBrowser {
                border: 1px solid #ccc;
                border-radius: 4px;
                padding: 8px;
            }
        """)
        layout.addWidget(self.chat_display, stretch=1)

        # Input area
        input_row = QHBoxLayout()
        input_row.setSpacing(4)

        self.input_field = QPlainTextEdit()
        self.input_field.setPlaceholderText("Ask anything about your files...")
        self.input_field.setMaximumHeight(80)
        self.input_field.setFont(QFont("Segoe UI", 10))
        input_row.addWidget(self.input_field, stretch=1)

        self.send_btn = QPushButton("Send")
        self.send_btn.setMinimumWidth(60)
        self.send_btn.setMinimumHeight(40)
        self.send_btn.clicked.connect(self.send_message)
        input_row.addWidget(self.send_btn)

        self.clear_chat_btn = QPushButton("Clear")
        self.clear_chat_btn.setToolTip("Clear conversation history")
        self.clear_chat_btn.setMinimumWidth(50)
        self.clear_chat_btn.setMinimumHeight(40)
        self.clear_chat_btn.clicked.connect(self.clear_chat)
        input_row.addWidget(self.clear_chat_btn)

        layout.addLayout(input_row)

        # Install event filter for Ctrl+Enter
        self.input_field.installEventFilter(self)

        # Loading indicator with cancel button
        loading_row = QHBoxLayout()
        self.loading_label = QLabel("")
        self.loading_label.setStyleSheet("color: #666; font-style: italic;")
        loading_row.addWidget(self.loading_label)

        self.cancel_btn = QPushButton("Cancel")
        self.cancel_btn.setMaximumWidth(60)
        self.cancel_btn.setStyleSheet("color: #c00;")
        self.cancel_btn.clicked.connect(self._cancel_request)
        loading_row.addWidget(self.cancel_btn)
        loading_row.addStretch()

        self.loading_widget = QWidget()
        self.loading_widget.setLayout(loading_row)
        self.loading_widget.setVisible(False)
        layout.addWidget(self.loading_widget)

        # Loading animation timer
        self.loading_timer = QTimer()
        self.loading_timer.timeout.connect(self._animate_loading)
        self.loading_dots = 0

        # Worker reference
        self.current_worker = None

        # Initial message
        self._append_system_message("Chat ready. Use buttons above or type /help for commands.")

    def eventFilter(self, obj, event):
        """Handle Ctrl+Enter to send message."""
        if obj == self.input_field and event.type() == event.Type.KeyPress:
            if event.key() == Qt.Key.Key_Return and event.modifiers() == Qt.KeyboardModifier.ControlModifier:
                self.send_message()
                return True
        return super().eventFilter(obj, event)

    def _append_system_message(self, text):
        """Add a system/info message to the display."""
        self.chat_display.append(f'<p style="color: #888; font-style: italic;">{text}</p>')

    def _append_user_message(self, text):
        """Add a user message to the display."""
        escaped = text.replace("<", "&lt;").replace(">", "&gt;").replace("\n", "<br>")
        self.chat_display.append(
            f'<p><b style="color: #2962ff;">You:</b></p>'
            f'<p style="margin-left: 12px; white-space: pre-wrap;">{escaped}</p>'
        )

    def _append_assistant_message(self, text):
        """Add an assistant message to the display."""
        escaped = text.replace("<", "&lt;").replace(">", "&gt;").replace("\n", "<br>")
        self.chat_display.append(
            f'<p><b style="color: #00897b;">Assistant:</b></p>'
            f'<p style="margin-left: 12px; white-space: pre-wrap;">{escaped}</p>'
        )

    def _append_error_message(self, text):
        """Add an error message to the display."""
        self.chat_display.append(f'<p style="color: #d32f2f;"><b>Error:</b> {text}</p>')

    def show_file_operation(self, operation, file_path, details=""):
        """Show a file operation in the chat."""
        widget = FileOperationWidget(operation, file_path, details)

        # Insert as a widget in the chat
        cursor = self.chat_display.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        self.chat_display.setTextCursor(cursor)

        # Insert the widget
        self.chat_display.insertPlainText("\n")
        cursor.insertText(f"[{operation.upper()}] {file_path}")
        if details:
            cursor.insertText(f"\n{details}")
        self.chat_display.insertPlainText("\n")

    def show_file_diff(self, file_path, old_content, new_content):
        """Show a diff viewer for file changes."""
        # Create a dialog with the diff viewer
        dialog = QDialog(self)
        dialog.setWindowTitle(f"Review Changes: {file_path}")
        dialog.setMinimumSize(700, 500)

        layout = QVBoxLayout(dialog)

        diff_viewer = DiffViewer(file_path, old_content, new_content)
        layout.addWidget(diff_viewer)

        # Handle apply/reject
        diff_viewer.applied.connect(lambda: self._on_diff_applied(dialog, file_path))
        diff_viewer.rejected.connect(dialog.reject)

        dialog.exec()

    def _on_diff_applied(self, dialog, file_path):
        """Handle when diff changes are applied."""
        dialog.accept()
        self._append_system_message(f"✓ Changes applied to {file_path}")
        self._scroll_to_bottom()

    def clear_chat(self):
        """Clear the chat history."""
        self.messages = []
        self.chat_display.clear()
        self._append_system_message("Chat cleared. Ready for new conversation.")

    def _run_provider(self, provider_name, args=""):
        """Run a provider and display the result in chat."""
        if not self.execute_provider_callback:
            self._append_error_message("Provider execution not available.")
            return

        self._start_loading()
        self.current_provider = provider_name

        # Run provider in background thread
        self.current_worker = LLMWorker(
            lambda _: self.execute_provider_callback(provider_name, args),
            None
        )
        self.current_worker.finished.connect(self._on_provider_response)
        self.current_worker.error.connect(self._on_provider_error)
        self.current_worker.start()

    def _on_provider_response(self, result):
        """Handle provider result."""
        self._stop_loading()
        if result:
            # Check if result is a file operation
            if isinstance(result, dict):
                operation = result.get("operation")
                file_path = result.get("file_path")
                old_content = result.get("old_content", "")
                new_content = result.get("new_content", "")
                message = result.get("message", "")

                if operation in ("write", "create"):
                    # Show diff for file changes
                    self._append_assistant_message(message or f"Proposed changes to {file_path}")
                    self.show_file_diff(file_path, old_content, new_content)
                elif operation == "read":
                    self.show_file_operation("read", file_path)
                    self._append_assistant_message(new_content)
                elif operation == "ingest":
                    # Show file ingestion summary
                    self._append_assistant_message(message)
                    self.show_file_operation("ingest", file_path, "File stored in semantic memory for AI context")
                else:
                    # Unknown operation, just show the message
                    self._append_assistant_message(str(result))
            else:
                # Regular text result
                self._append_assistant_message(result)
        else:
            self._append_system_message(f"Provider returned no output.")
        self._scroll_to_bottom()

    def _on_provider_error(self, error_msg):
        """Handle provider error."""
        self._stop_loading()
        self._append_error_message(f"Provider error: {error_msg}")
        self._scroll_to_bottom()

    def _show_help(self):
        """Display available slash commands."""
        help_text = "**Available Commands:**\n\n"
        for cmd, (provider, desc) in self.SLASH_COMMANDS.items():
            help_text += f"`{cmd}` - {desc}\n"
        help_text += "\n**Quick Buttons:** Use the buttons above for common actions."
        help_text += "\n**Context:** Toggle '+ Selection' or '+ Document' before sending to include content."
        self._append_system_message(help_text.replace("\n", "<br>"))

    def _show_history(self):
        """Display the spine event history (episodic memory)."""
        if not self.get_spine_callback:
            self._append_system_message("No spine available.")
            return

        spine = self.get_spine_callback()
        if not spine:
            self._append_system_message("No active document context.")
            return

        narrative = spine.get_narrative(limit=30)
        event_count = len(spine.history)

        history_text = f"**Spine History** (Context: {spine.context_id})<br>"
        history_text += f"Total events: {event_count}<br><br>"
        history_text += f"<pre>{narrative}</pre>"

        self._append_system_message(history_text)

    def _show_kernel(self):
        """Display the kernel state (contexts, global events)."""
        if not self.get_kernel_callback:
            self._append_system_message("No kernel available.")
            return

        state = self.get_kernel_callback()
        if not state:
            self._append_system_message("Failed to get kernel state.")
            return

        kernel_text = "**Kernel State**<br>"
        kernel_text += f"Active contexts: {state.get('contexts', 0)}<br><br>"

        # List contexts
        ctx_list = state.get('context_list', [])
        if ctx_list:
            kernel_text += "<b>Contexts:</b><br>"
            for ctx in ctx_list:
                path = ctx.get('path', 'untitled')
                if path:
                    path = path.split('\\')[-1].split('/')[-1]  # Just filename
                ctx_type = ctx.get('type', 'unknown')
                links = ctx.get('links', [])
                link_str = f" (linked: {len(links)})" if links else ""
                kernel_text += f"  [{ctx['id']}] {path} <i>({ctx_type})</i>{link_str}<br>"
        else:
            kernel_text += "<i>No active contexts</i><br>"

        # Recent global events
        kernel_text += "<br><b>Recent Global Events:</b><br>"
        events = state.get('recent_events', [])
        if events:
            for e in events[-10:]:
                time = e['timestamp'].split('T')[1].split('.')[0] if e.get('timestamp') else '??'
                src = e.get('source', 'kernel') or 'kernel'
                kernel_text += f"  [{time}] {src}: {e['event_type']}<br>"
        else:
            kernel_text += "  <i>No events yet</i><br>"

        self._append_system_message(kernel_text)

    def _show_manifest(self):
        """Display the manifest for the current document."""
        if not self.get_manifest_callback:
            self._append_system_message("Manifest not available.")
            return

        manifest_info = self.get_manifest_callback()
        if not manifest_info:
            self._append_system_message("No active document context.")
            return

        manifest = manifest_info.get("manifest")
        path = manifest_info.get("path", "untitled")
        if path:
            path = path.split('\\')[-1].split('/')[-1]

        manifest_text = f"**Document Manifest** - {path}<br><br>"

        if manifest:
            manifest_text += f"<b>Type:</b> {manifest.document_type}<br>"
            manifest_text += f"<b>AI Access:</b> {manifest.ai_access}<br>"

            if manifest.tags:
                manifest_text += f"<b>Tags:</b> {', '.join(manifest.tags)}<br>"

            manifest_text += "<br><b>Default Permissions:</b><br>"
            manifest_text += f"  Read: {manifest.default_can_read}<br>"
            manifest_text += f"  Write: {manifest.default_can_write}<br>"
            manifest_text += f"  Emit: {manifest.default_can_emit}<br>"
            manifest_text += f"  Invoke: {manifest.default_can_invoke}<br>"

            if manifest.providers:
                manifest_text += "<br><b>Provider Overrides:</b><br>"
                for pid, perm in manifest.providers.items():
                    perms = []
                    if perm.can_read: perms.append("read")
                    if perm.can_write: perms.append("write")
                    if perm.can_emit: perms.append("emit")
                    if perm.can_invoke: perms.append("invoke")
                    manifest_text += f"  {pid}: [{', '.join(perms)}]<br>"

            if manifest.links:
                manifest_text += f"<br><b>Links:</b> {', '.join(manifest.links)}<br>"
        else:
            manifest_text += "<i>Using default manifest (no overrides)</i><br>"
            manifest_text += "<br>Add a manifest to control provider access:<br>"
            manifest_text += "<code>---<br>type: document<br>ai_access: observe<br>---</code>"

        self._append_system_message(manifest_text)

    def send_message(self):
        """Send the user's message to the LLM."""
        user_text = self.input_field.toPlainText().strip()
        if not user_text:
            return

        # Check for slash commands
        if user_text.startswith("/"):
            parts = user_text.split(" ", 1)
            cmd = parts[0].lower()
            args = parts[1] if len(parts) > 1 else ""

            if cmd in self.SLASH_COMMANDS:
                self.input_field.clear()
                provider_name, desc = self.SLASH_COMMANDS[cmd]
                if cmd == "/help":
                    self._show_help()
                elif provider_name:
                    self._append_user_message(user_text)
                    self._run_provider(provider_name, args)
                return

        # Build the full message with automatic context
        full_message = user_text
        context_parts = []

        # Automatically include semantic memory if available
        if self.get_kernel_callback:
            try:
                main_window = self.window()
                if hasattr(main_window, 'tabs'):
                    for i in range(main_window.tabs.count()):
                        tab = main_window.tabs.widget(i)
                        if hasattr(tab, 'kernel') and tab.kernel:
                            results = tab.kernel.memory.search(user_text, limit=3)  # Get 3 results for better context
                            if results:
                                context_parts.append("=== RELEVANT CODE FROM YOUR FILES ===")
                                for result in results:
                                    metadata = result.get('metadata', {})
                                    file_path = metadata.get('file_path', 'Unknown')
                                    chunk_idx = metadata.get('chunk_index', '?')
                                    content = result.get('content', '')[:1000]  # More context
                                    context_parts.append(f"\n--- File: {os.path.basename(file_path)} (chunk {chunk_idx}) ---\n{content}")
                                context_parts.append("=== END CONTEXT ===\n")
                            break
            except:
                pass  # Silent fail if memory search doesn't work

        # Automatically include selection if there is one
        if self.get_context_callback:
            selection, document = self.get_context_callback()
            if selection:
                context_parts.append(f"[Selected text]\n{selection}\n[/Selected text]")

        if context_parts:
            full_message = "\n\n".join(context_parts) + "\n\n" + user_text

        # Display user message (show only the user text, not the context)
        self._append_user_message(user_text)
        if context_parts:
            self._append_system_message("(Included context with message)")

        # Clear input
        self.input_field.clear()

        # Add to history
        self.messages.append({"role": "user", "content": full_message})

        # Send to LLM in background
        self._start_loading()

        self.current_worker = LLMWorker(self._call_llm, self.messages.copy())
        self.current_worker.finished.connect(self._on_llm_response)
        self.current_worker.error.connect(self._on_llm_error)
        self.current_worker.start()

    def _start_loading(self):
        """Show loading indicator."""
        self.send_btn.setEnabled(False)
        self.input_field.setEnabled(False)
        self.loading_widget.setVisible(True)
        self.loading_dots = 0
        self._animate_loading()
        self.loading_timer.start(400)

    def _stop_loading(self):
        """Hide loading indicator."""
        self.loading_timer.stop()
        self.loading_widget.setVisible(False)
        self.send_btn.setEnabled(True)
        self.input_field.setEnabled(True)
        self.input_field.setFocus()

    def _cancel_request(self):
        """Cancel the current LLM request."""
        if self.current_worker and self.current_worker.isRunning():
            self.current_worker.terminate()
            self.current_worker.wait(1000)
        self._stop_loading()
        self._append_system_message("Request cancelled.")

    def _animate_loading(self):
        """Animate the loading dots."""
        dots = "." * (self.loading_dots % 4)
        self.loading_label.setText(f"Thinking{dots}")
        self.loading_dots += 1

    def _on_llm_response(self, response):
        """Handle successful LLM response."""
        self._stop_loading()
        self._append_assistant_message(response)
        self.messages.append({"role": "assistant", "content": response})
        self._scroll_to_bottom()

    def _on_llm_error(self, error_msg):
        """Handle LLM error."""
        self._stop_loading()
        self._append_error_message(error_msg)
        self._scroll_to_bottom()

    def _scroll_to_bottom(self):
        """Scroll chat to bottom."""
        scrollbar = self.chat_display.verticalScrollBar()
        scrollbar.setValue(scrollbar.maximum())

    def _call_llm(self, messages):
        """Call the LLM with the message history."""
        import json
        import urllib.request
        import re

        # Build system prompt with explicit context instructions
        system_prompt = """You are an AI coding assistant with access to the user's files through semantic search.

**IMPORTANT: Use the provided context from the user's files to answer accurately.**

When you see [Relevant context from your files], this is actual code/content from the user's project. Use it to:
- Answer questions about their code
- Explain how things work
- Help with debugging
- Make informed suggestions

Be precise and reference the actual code shown in the context."""

        # Add lightweight document context (just structure, not full content)
        if self.get_context_callback:
            selection, document = self.get_context_callback()
            if document:
                # Extract just the headings for lightweight context
                headings = re.findall(r'^(#{1,6}\s+.+)$', document, re.MULTILINE)
                word_count = len(document.split())

                context_info = f"\n\nThe user is working on a document ({word_count} words)."
                if headings:
                    context_info += f"\nDocument outline:\n" + "\n".join(headings[:15])  # First 15 headings
                    if len(headings) > 15:
                        context_info += f"\n... and {len(headings) - 15} more sections"

                system_prompt += context_info

                # Always include selection if present - this is what the user is focused on
                if selection:
                    system_prompt += f"\n\nCurrently selected text:\n{selection}"

        # Check if we can use LM Studio directly
        if hasattr(self.ai_client, 'lm_studio') and self.ai_client.is_lm_studio_active():
            client = self.ai_client.lm_studio
            endpoint = client.endpoint

            payload = {
                "messages": [
                    {"role": "system", "content": system_prompt},
                    *messages
                ],
                "temperature": 0.3,  # Lower for more accurate, deterministic answers
                "max_tokens": 4096,  # Allow longer responses
                "stream": False
            }
            if client.model:
                payload["model"] = client.model

            data = json.dumps(payload).encode("utf-8")
            req = urllib.request.Request(
                f"{endpoint}/chat/completions",
                data=data,
                headers={"Content-Type": "application/json"},
                method="POST"
            )

            with urllib.request.urlopen(req, timeout=120) as response:
                result = json.loads(response.read().decode("utf-8"))
                return result["choices"][0]["message"]["content"]
        else:
            raise ConnectionError("LM Studio is not connected. Please start LM Studio and check Settings.")

    def set_ai_client(self, ai_client):
        """Update the AI client reference."""
        self.ai_client = ai_client


class ContextLauncher(QWidget):
    """Workspace panel showing registered contexts.

    The launcher displays all contexts (files) that have been added
    to the workspace. This is the "desktop" of the MarlOS -
    you curate what's here, not everything on disk.
    """

    context_selected = pyqtSignal(str)  # Emits file path
    context_added = pyqtSignal(str)     # Emits file path

    # Consistent side panel styling (cream/warm theme)
    PANEL_STYLE = """
        QWidget {
            background-color: #faf8f5;
            color: #3d3929;
        }
        QLabel {
            color: #3d3929;
        }
        QPushButton {
            background-color: #ebe7df;
            color: #3d3929;
            border: 1px solid #d5d0c4;
            padding: 4px 8px;
            border-radius: 3px;
        }
        QPushButton:hover {
            background-color: #e0dbd1;
        }
        QListWidget {
            background-color: #ffffff;
            color: #3d3929;
            border: 1px solid #d5d0c4;
            border-radius: 4px;
        }
        QListWidget::item {
            padding: 6px;
            border-bottom: 1px solid #ebe7df;
            color: #3d3929;
        }
        QListWidget::item:hover {
            background: #f5f3ef;
        }
        QListWidget::item:selected {
            background: #e8d5b5;
            color: #3d3929;
        }
    """

    def __init__(self, parent=None):
        super().__init__(parent)
        self._contexts = {}  # path -> {name, status, context_id}
        self.setStyleSheet(self.PANEL_STYLE)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

        # Header
        header = QHBoxLayout()
        title = QLabel("Workspace")
        title.setStyleSheet("font-weight: bold; font-size: 12px; color: #3d3929;")
        header.addWidget(title)
        header.addStretch()

        add_btn = QPushButton("+")
        add_btn.setMaximumWidth(30)
        add_btn.setToolTip("Add context to workspace")
        add_btn.clicked.connect(self._add_context)
        header.addWidget(add_btn)

        remove_btn = QPushButton("-")
        remove_btn.setMaximumWidth(30)
        remove_btn.setToolTip("Remove selected from workspace")
        remove_btn.clicked.connect(self._remove_selected)
        header.addWidget(remove_btn)

        layout.addLayout(header)

        # Context list
        self.context_list = QListWidget()
        self.context_list.itemDoubleClicked.connect(self._on_item_double_clicked)
        self.context_list.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.context_list.customContextMenuRequested.connect(self._show_context_menu)
        self.context_list.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        # Install event filter for delete key
        self.context_list.installEventFilter(self)
        layout.addWidget(self.context_list)

        # Status bar
        self.status_label = QLabel("0 contexts")
        self.status_label.setStyleSheet("color: #6b6555; font-size: 10px;")
        layout.addWidget(self.status_label)

    def _add_context(self):
        """Add a new context to the workspace."""
        from PyQt6.QtWidgets import QFileDialog
        path, _ = QFileDialog.getOpenFileName(
            self,
            "Add Context",
            "",
            "All Supported (*.md *.txt *.py *.js *.json);;Markdown (*.md);;All Files (*)"
        )
        if path:
            self.add_context(path)
            self.context_added.emit(path)

    def _normalize_path(self, path):
        """Normalize path to prevent duplicates from different path formats."""
        from pathlib import Path
        # Resolve to absolute, normalize slashes
        resolved = str(Path(path).resolve())
        # Convert backslashes to forward slashes and lowercase everything on Windows
        normalized = resolved.replace('\\', '/')
        if sys.platform == "win32":
            normalized = normalized.lower()
        return normalized

    def add_context(self, path, context_id=None, status="closed"):
        """Register a context in the workspace (no duplicates)."""
        from pathlib import Path
        normalized = self._normalize_path(path)

        # Check if already exists
        if normalized in self._contexts:
            # Just update status if already present
            old_status = self._contexts[normalized]["status"]
            self._contexts[normalized]["status"] = status
            if context_id:
                self._contexts[normalized]["context_id"] = context_id
            # Only refresh if status changed
            if old_status != status:
                self._refresh_list()
            return

        name = Path(path).name
        self._contexts[normalized] = {
            "name": name,
            "status": status,
            "context_id": context_id
        }
        self._refresh_list()

    def update_context_status(self, path, status, context_id=None):
        """Update a context's status (open, modified, closed)."""
        normalized = self._normalize_path(path)
        if normalized in self._contexts:
            self._contexts[normalized]["status"] = status
            if context_id:
                self._contexts[normalized]["context_id"] = context_id
            self._refresh_list()

    def remove_context(self, path):
        """Remove a context from the workspace."""
        from PyQt6.QtWidgets import QMessageBox

        # Try exact match first
        if path in self._contexts:
            del self._contexts[path]
            self._refresh_list()
            return

        # Try case-insensitive match
        normalized = self._normalize_path(path)
        for stored_path in list(self._contexts.keys()):
            if self._normalize_path(stored_path) == normalized:
                del self._contexts[stored_path]
                self._refresh_list()
                return

    def _refresh_list(self):
        """Refresh the context list display."""
        self.context_list.clear()
        for path, info in self._contexts.items():
            status_icon = {
                "open": "●",      # Green dot
                "modified": "◐",  # Half dot
                "closed": "○"     # Empty dot
            }.get(info["status"], "○")

            status_color = {
                "open": "#4a4",
                "modified": "#c80",
                "closed": "#888"
            }.get(info["status"], "#888")

            item = QListWidgetItem(f"{status_icon} {info['name']}")
            item.setData(Qt.ItemDataRole.UserRole, path)
            item.setToolTip(f"{path}\nStatus: {info['status']}")
            # Color the status icon
            item.setForeground(QColor(status_color) if info["status"] != "closed" else QColor("#333"))
            self.context_list.addItem(item)

        self.status_label.setText(f"{len(self._contexts)} contexts")

    def _on_item_double_clicked(self, item):
        """Open the selected context."""
        path = item.data(Qt.ItemDataRole.UserRole)
        if path:
            self.context_selected.emit(path)

    def _show_context_menu(self, position):
        """Show right-click context menu for list items."""
        from PyQt6.QtWidgets import QMenu

        item = self.context_list.itemAt(position)
        if not item:
            return

        path = item.data(Qt.ItemDataRole.UserRole)
        menu = QMenu(self)

        # Open action
        open_action = menu.addAction("Open")
        open_action.triggered.connect(lambda checked=False, p=path: self.context_selected.emit(p))

        # Remove action
        remove_action = menu.addAction("Remove from Workspace")
        remove_action.triggered.connect(lambda checked=False, p=path: self._remove_from_workspace(p))

        menu.exec(self.context_list.mapToGlobal(position))

    def _remove_selected(self):
        """Remove the currently selected item."""
        from PyQt6.QtWidgets import QMessageBox

        current_item = self.context_list.currentItem()
        if current_item:
            path = current_item.data(Qt.ItemDataRole.UserRole)
            if path:
                self._remove_from_workspace(path)
            else:
                QMessageBox.warning(self, "Error", "No path data on selected item")
        else:
            QMessageBox.warning(self, "Error", "No item selected")

    def _remove_from_workspace(self, path):
        """Remove a file from the workspace and close if open."""
        # Get the main window to access tabs
        main_window = self.window()

        if main_window and hasattr(main_window, 'tabs'):
            # Find and close the tab if open
            for i in range(main_window.tabs.count()):
                tab = main_window.tabs.widget(i)
                if hasattr(tab, 'file_path'):
                    try:
                        normalized_tab = self._normalize_path(tab.file_path)
                        normalized_path = self._normalize_path(path)
                        if normalized_tab == normalized_path:
                            # Close the tab
                            main_window.tabs.removeTab(i)
                            break
                    except:
                        pass

        # Remove from workspace list
        self.remove_context(path)

    def get_contexts(self):
        """Get all registered context paths."""
        return list(self._contexts.keys())

    def to_dict(self):
        """Serialize workspace for persistence."""
        return {"contexts": self._contexts}

    def from_dict(self, data):
        """Load workspace from serialized data."""
        self._contexts = data.get("contexts", {})
        # Reset all to closed on load
        for path in self._contexts:
            self._contexts[path]["status"] = "closed"
        self._refresh_list()

    def eventFilter(self, obj, event):
        """Handle key press events for the context list."""
        if obj == self.context_list and event.type() == event.Type.KeyPress:
            if event.key() == Qt.Key.Key_Delete:
                # Get selected item
                current_item = self.context_list.currentItem()
                if current_item:
                    path = current_item.data(Qt.ItemDataRole.UserRole)
                    if path:
                        self._remove_from_workspace(path)
                return True
        return super().eventFilter(obj, event)


class DesktopView(QWidget):
    """Desktop home screen for the MarlOS.

    Shows when no documents are open. Provides:
    - Context icons for quick access
    - Terminal/chat for commands
    - Quiet, minimal interface
    """

    file_requested = pyqtSignal(str)  # Emits path to open
    new_file_requested = pyqtSignal()

    def __init__(self, context_launcher=None, chat_panel=None, parent=None):
        super().__init__(parent)
        self.context_launcher = context_launcher

        layout = QVBoxLayout(self)
        layout.setContentsMargins(40, 40, 40, 40)
        layout.setSpacing(20)

        # Title
        title = QLabel("MarlOS")
        title.setStyleSheet("font-size: 24px; font-weight: bold; color: #444;")
        title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(title)

        subtitle = QLabel("quiet and capable")
        subtitle.setStyleSheet("font-size: 12px; color: #888; font-style: italic;")
        subtitle.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(subtitle)

        layout.addSpacing(20)

        # Context icons grid
        self.icons_widget = QWidget()
        self.icons_layout = QHBoxLayout(self.icons_widget)
        self.icons_layout.setSpacing(20)
        self.icons_layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self.icons_widget)

        layout.addStretch()

        # Quick actions
        actions_layout = QHBoxLayout()
        actions_layout.setAlignment(Qt.AlignmentFlag.AlignCenter)

        new_btn = QPushButton("New Document")
        new_btn.setMinimumWidth(120)
        new_btn.setStyleSheet("""
            QPushButton {
                background: #4a90d9;
                color: white;
                border: none;
                border-radius: 4px;
                padding: 8px 16px;
                font-size: 12px;
            }
            QPushButton:hover { background: #3a80c9; }
        """)
        new_btn.clicked.connect(self.new_file_requested.emit)
        actions_layout.addWidget(new_btn)

        open_btn = QPushButton("Open File")
        open_btn.setMinimumWidth(120)
        open_btn.setStyleSheet("""
            QPushButton {
                background: #5a5a5a;
                color: white;
                border: none;
                border-radius: 4px;
                padding: 8px 16px;
                font-size: 12px;
            }
            QPushButton:hover { background: #4a4a4a; }
        """)
        open_btn.clicked.connect(self._open_file)
        actions_layout.addWidget(open_btn)

        layout.addLayout(actions_layout)

        layout.addSpacing(20)

        # Scratchpad / Quick notes (journal-style input)
        scratch_frame = QWidget()
        scratch_frame.setStyleSheet("""
            QWidget {
                background: #fffef0;
                border: 1px solid #e0d890;
                border-radius: 8px;
            }
        """)
        scratch_layout = QVBoxLayout(scratch_frame)
        scratch_layout.setContentsMargins(16, 12, 16, 12)

        scratch_header = QHBoxLayout()
        scratch_label = QLabel("Quick Notes")
        scratch_label.setStyleSheet("color: #665; font-size: 11px; font-weight: bold;")
        scratch_header.addWidget(scratch_label)
        scratch_header.addStretch()

        hint_label = QLabel("type > for commands")
        hint_label.setStyleSheet("color: #998; font-size: 9px;")
        scratch_header.addWidget(hint_label)
        scratch_layout.addLayout(scratch_header)

        self.scratch_input = QPlainTextEdit()
        self.scratch_input.setStyleSheet("""
            QPlainTextEdit {
                background: transparent;
                border: none;
                color: #333;
                font-family: 'Segoe UI', sans-serif;
                font-size: 13px;
            }
        """)
        self.scratch_input.setPlaceholderText("Jot something down... (Enter to save, > for commands)")
        self.scratch_input.setMaximumHeight(60)
        self.scratch_input.installEventFilter(self)
        scratch_layout.addWidget(self.scratch_input)

        self.scratch_output = QLabel("")
        self.scratch_output.setStyleSheet("color: #665; font-size: 11px;")
        self.scratch_output.setWordWrap(True)
        scratch_layout.addWidget(self.scratch_output)

        scratch_frame.setMaximumHeight(140)
        layout.addWidget(scratch_frame)

        # Load scratch file path
        self.scratch_file = Path(__file__).parent / "scratch.md"

        self.setStyleSheet("background: #f8f8f8;")

    def eventFilter(self, obj, event):
        """Handle Enter key in scratch input."""
        if obj == self.scratch_input and event.type() == event.Type.KeyPress:
            if event.key() == Qt.Key.Key_Return and not event.modifiers():
                self._handle_input()
                return True
        return super().eventFilter(obj, event)

    def _open_file(self):
        """Open file dialog."""
        from PyQt6.QtWidgets import QFileDialog
        path, _ = QFileDialog.getOpenFileName(
            self,
            "Open File",
            "",
            "Documents (*.md *.txt *.py *.pdf *.epub);;All Files (*)"
        )
        if path:
            self.file_requested.emit(path)

    def _handle_input(self):
        """Handle scratchpad input - notes or commands."""
        text = self.scratch_input.toPlainText().strip()
        self.scratch_input.clear()

        if not text:
            return

        # Command mode: starts with >
        if text.startswith(">"):
            self._handle_command(text[1:].strip())
            return

        # Journal mode: save to scratch.md
        self._save_note(text)

    def _save_note(self, text):
        """Save a quick note to scratch.md."""
        from datetime import datetime
        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M")

        try:
            # Append to scratch file
            with open(self.scratch_file, "a", encoding="utf-8") as f:
                f.write(f"\n---\n**{timestamp}**\n{text}\n")
            self.scratch_output.setText(f"Saved to scratch.md")
        except Exception as e:
            self.scratch_output.setText(f"Error: {e}")

    def _handle_command(self, cmd):
        """Handle a command (prefixed with >)."""
        if not cmd:
            self.scratch_output.setText("Commands: new, open, scratch, help")
            return

        # Check if it's a file path
        if os.path.isfile(cmd):
            self.file_requested.emit(cmd)
            self.scratch_output.setText(f"Opening {cmd}...")
            return

        cmd_lower = cmd.lower()

        if cmd_lower in ("new", "n"):
            self.new_file_requested.emit()
            self.scratch_output.setText("Creating new document...")
        elif cmd_lower in ("open", "o"):
            self._open_file()
        elif cmd_lower in ("scratch", "s"):
            # Open the scratch file
            if self.scratch_file.exists():
                self.file_requested.emit(str(self.scratch_file))
                self.scratch_output.setText("Opening scratch.md...")
            else:
                self.scratch_output.setText("No scratch file yet. Write a note first.")
        elif cmd_lower in ("help", "?", ""):
            self.scratch_output.setText("Commands: new, open, scratch, help")
        elif cmd_lower.startswith("open "):
            path = cmd[5:].strip().strip('"').strip("'")
            if os.path.isfile(path):
                self.file_requested.emit(path)
                self.scratch_output.setText(f"Opening {path}...")
            else:
                self.scratch_output.setText(f"File not found: {path}")
        else:
            self.scratch_output.setText(f"Unknown: {cmd}. Try 'help'")

    def refresh_icons(self):
        """Refresh the context icons from workspace."""
        # Clear existing
        while self.icons_layout.count():
            child = self.icons_layout.takeAt(0)
            if child.widget():
                child.widget().deleteLater()

        if not self.context_launcher:
            return

        # Add icons for each context
        contexts = self.context_launcher._contexts
        for path, info in list(contexts.items())[:8]:  # Max 8 icons
            icon_btn = QPushButton()
            icon_btn.setFixedSize(80, 90)

            # Determine icon based on extension
            ext = Path(path).suffix.lower()
            icon_char = {
                ".md": "📄", ".txt": "📝", ".py": "🐍",
                ".js": "📜", ".pdf": "📕", ".epub": "📗",
                ".json": "📋", ".html": "🌐",
            }.get(ext, "📄")

            icon_btn.setText(f"{icon_char}\n{info['name'][:10]}")
            icon_btn.setStyleSheet("""
                QPushButton {
                    background: white;
                    color: #333;
                    border: 1px solid #ccc;
                    border-radius: 8px;
                    font-size: 11px;
                    padding: 8px;
                }
                QPushButton:hover {
                    background: #e8e8ff;
                    border-color: #88f;
                    color: #222;
                }
            """)
            icon_btn.setToolTip(path)
            icon_btn.clicked.connect(lambda checked, p=path: self.file_requested.emit(p))
            self.icons_layout.addWidget(icon_btn)


class TaskBadgeButton(QPushButton):
    """A button with an optional red dot indicator.

    Shows a subtle red dot badge when there are actionable items (like
    overdue tasks or due reminders) without showing counts or being
    intrusive. The user chooses when to look at tasks.

    The badge follows the "quiet-by-default" philosophy:
    - No numbers, just a dot
    - No popups or notifications
    - User-initiated exploration only
    """

    def __init__(self, text="Tasks", parent=None):
        super().__init__(text, parent)
        self._show_badge = False
        self.setMinimumWidth(60)
        self._update_style()

    def set_badge_visible(self, visible):
        """Show or hide the red dot badge.

        Args:
            visible: True to show the badge, False to hide it
        """
        if self._show_badge != visible:
            self._show_badge = visible
            self._update_style()

    def has_badge(self):
        """Return whether the badge is currently visible."""
        return self._show_badge

    def _update_style(self):
        """Update the button stylesheet based on badge state."""
        if self._show_badge:
            # Red dot indicator in the top-right corner
            self.setStyleSheet("""
                QPushButton {
                    padding: 4px 12px;
                    position: relative;
                }
                QPushButton::after {
                    content: '';
                    position: absolute;
                    top: 4px;
                    right: 4px;
                    width: 8px;
                    height: 8px;
                    background: #e74c3c;
                    border-radius: 4px;
                }
            """)
            # Note: Qt stylesheet ::after doesn't work like CSS
            # We'll use paintEvent override instead
            self.update()
        else:
            self.setStyleSheet("padding: 4px 12px;")
            self.update()

    def paintEvent(self, event):
        """Override to draw the red dot badge."""
        super().paintEvent(event)
        if self._show_badge:
            from PyQt6.QtGui import QPainter
            painter = QPainter(self)
            painter.setRenderHint(QPainter.RenderHint.Antialiasing)
            painter.setBrush(QColor("#e74c3c"))
            painter.setPen(Qt.PenStyle.NoPen)
            # Draw a small red circle in the top-right area
            dot_size = 8
            x = self.width() - dot_size - 6
            y = 6
            painter.drawEllipse(x, y, dot_size, dot_size)
            painter.end()


class ScaledPDFViewer(QWidget):
    """PDF viewer with measurement and scaling tools.

    For technical drawings - calibrate scale, measure distances,
    export at 1:1 for permit submissions.
    """

    def __init__(self, parent=None):
        super().__init__(parent)
        self.pdf_doc = None
        self.current_page = 0
        self.zoom = 1.0
        self.dpi = 150  # Render DPI
        self.pixmap = None

        # Calibration
        self.scale_factor = None  # pixels per real unit
        self.scale_unit = "mm"  # mm, cm, in, ft
        self.calibration_points = []  # [(x, y), (x, y)]
        self.calibration_distance = None  # real-world distance

        # Measurement
        self.measure_mode = False
        self.measure_points = []
        self.measurements = []  # List of saved measurements

        # Coordinate capture for permit submissions
        self.coordinate_points = []  # [(label, x_real, y_real, x_px, y_px), ...]
        self.coord_origin = None  # (x_px, y_px) - origin point for relative coords

        # Mode: "view", "calibrate", "measure", "coords"
        self.tool_mode = "view"

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        # Toolbar
        toolbar = QHBoxLayout()
        toolbar.setContentsMargins(8, 6, 8, 6)

        self.prev_btn = QPushButton("◀")
        self.prev_btn.setMaximumWidth(40)
        self.prev_btn.clicked.connect(self.prev_page)
        toolbar.addWidget(self.prev_btn)

        self.page_label = QLabel("Page 1/1")
        toolbar.addWidget(self.page_label)

        self.next_btn = QPushButton("▶")
        self.next_btn.setMaximumWidth(40)
        self.next_btn.clicked.connect(self.next_page)
        toolbar.addWidget(self.next_btn)

        toolbar.addSpacing(20)

        self.zoom_out_btn = QPushButton("−")
        self.zoom_out_btn.setMaximumWidth(30)
        self.zoom_out_btn.clicked.connect(self.zoom_out)
        toolbar.addWidget(self.zoom_out_btn)

        self.zoom_label = QLabel("100%")
        self.zoom_label.setMinimumWidth(50)
        toolbar.addWidget(self.zoom_label)

        self.zoom_in_btn = QPushButton("+")
        self.zoom_in_btn.setMaximumWidth(30)
        self.zoom_in_btn.clicked.connect(self.zoom_in)
        toolbar.addWidget(self.zoom_in_btn)

        toolbar.addSpacing(20)

        self.calibrate_btn = QPushButton("Calibrate")
        self.calibrate_btn.setCheckable(True)
        self.calibrate_btn.setToolTip("Click two points on a known dimension")
        self.calibrate_btn.clicked.connect(self.toggle_calibrate)
        toolbar.addWidget(self.calibrate_btn)

        self.measure_btn = QPushButton("Measure")
        self.measure_btn.setCheckable(True)
        self.measure_btn.setToolTip("Measure distances on the drawing")
        self.measure_btn.clicked.connect(self.toggle_measure)
        toolbar.addWidget(self.measure_btn)

        self.scale_label = QLabel("Scale: Not calibrated")
        self.scale_label.setStyleSheet("color: #666;")
        toolbar.addWidget(self.scale_label)

        toolbar.addSpacing(10)

        self.clear_btn = QPushButton("Clear")
        self.clear_btn.setToolTip("Clear all measurements")
        self.clear_btn.clicked.connect(self.clear_measurements)
        toolbar.addWidget(self.clear_btn)

        self.export_btn = QPushButton("Export 1:1")
        self.export_btn.setToolTip("Export page at true 1:1 scale for printing")
        self.export_btn.clicked.connect(self.export_one_to_one)
        self.export_btn.setEnabled(False)  # Enable after calibration
        toolbar.addWidget(self.export_btn)

        self.coords_btn = QPushButton("Coordinates")
        self.coords_btn.setCheckable(True)
        self.coords_btn.setToolTip("Click corners to capture coordinates for permit submission")
        self.coords_btn.clicked.connect(self.toggle_coords_mode)
        self.coords_btn.setEnabled(False)  # Enable after calibration
        toolbar.addWidget(self.coords_btn)

        self.export_coords_btn = QPushButton("Export Coords")
        self.export_coords_btn.setToolTip("Export captured coordinates to CSV")
        self.export_coords_btn.clicked.connect(self.export_coordinates)
        self.export_coords_btn.setEnabled(False)
        toolbar.addWidget(self.export_coords_btn)

        toolbar.addStretch()

        toolbar_widget = QWidget()
        toolbar_widget.setLayout(toolbar)
        layout.addWidget(toolbar_widget)

        # Scroll area for PDF display
        self.scroll = QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.scroll.setAlignment(Qt.AlignmentFlag.AlignCenter)

        self.pdf_label = QLabel()
        self.pdf_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.pdf_label.setMouseTracking(True)
        self.scroll.setWidget(self.pdf_label)
        layout.addWidget(self.scroll)

        # Status bar
        self.status_label = QLabel("Load a PDF to begin")
        self.status_label.setStyleSheet("background: #f0f0f0; padding: 4px 8px;")
        layout.addWidget(self.status_label)

        # Install event filter for mouse events
        self.pdf_label.installEventFilter(self)

    def load_pdf(self, file_path):
        """Load a PDF file."""
        try:
            import fitz  # PyMuPDF
            self.pdf_doc = fitz.open(file_path)
            self.current_page = 0
            self.render_page()
            self.status_label.setText(f"Loaded: {Path(file_path).name}")
        except ImportError:
            self.status_label.setText("PyMuPDF not installed. Run: pip install PyMuPDF")
        except Exception as e:
            self.status_label.setText(f"Error: {e}")

    def render_page(self):
        """Render current page at current zoom."""
        if not self.pdf_doc:
            return

        try:
            import fitz
        except ImportError:
            return

        page = self.pdf_doc[self.current_page]

        # Calculate matrix for zoom and DPI
        mat = fitz.Matrix(self.zoom * self.dpi / 72, self.zoom * self.dpi / 72)
        pix = page.get_pixmap(matrix=mat)

        # Convert to QPixmap
        from PyQt6.QtGui import QImage, QPixmap
        img = QImage(pix.samples, pix.width, pix.height, pix.stride, QImage.Format.Format_RGB888)
        self.pixmap = QPixmap.fromImage(img)

        # Draw measurements overlay
        self._draw_overlay()

        self.page_label.setText(f"Page {self.current_page + 1}/{len(self.pdf_doc)}")
        self.zoom_label.setText(f"{int(self.zoom * 100)}%")

    def _draw_overlay(self):
        """Draw calibration and measurement lines on the pixmap."""
        if not self.pixmap:
            return

        from PyQt6.QtGui import QPainter, QPen

        overlay = self.pixmap.copy()
        painter = QPainter(overlay)

        # Draw calibration line
        if len(self.calibration_points) == 2:
            pen = QPen(QColor("#00aa00"))
            pen.setWidth(2)
            painter.setPen(pen)
            p1, p2 = self.calibration_points
            painter.drawLine(int(p1[0]), int(p1[1]), int(p2[0]), int(p2[1]))
            # Draw endpoints
            painter.setBrush(QColor("#00aa00"))
            painter.drawEllipse(int(p1[0])-4, int(p1[1])-4, 8, 8)
            painter.drawEllipse(int(p2[0])-4, int(p2[1])-4, 8, 8)
        elif len(self.calibration_points) == 1:
            pen = QPen(QColor("#00aa00"))
            pen.setWidth(2)
            painter.setPen(pen)
            painter.setBrush(QColor("#00aa00"))
            p1 = self.calibration_points[0]
            painter.drawEllipse(int(p1[0])-4, int(p1[1])-4, 8, 8)

        # Draw measurements
        for m in self.measurements:
            pen = QPen(QColor("#0066cc"))
            pen.setWidth(2)
            painter.setPen(pen)
            p1, p2, dist = m
            painter.drawLine(int(p1[0]), int(p1[1]), int(p2[0]), int(p2[1]))
            # Draw label
            mid_x = (p1[0] + p2[0]) / 2
            mid_y = (p1[1] + p2[1]) / 2
            painter.drawText(int(mid_x) + 5, int(mid_y) - 5, f"{dist:.1f} {self.scale_unit}")

        # Draw current measurement in progress
        if len(self.measure_points) == 1:
            pen = QPen(QColor("#cc0066"))
            pen.setWidth(2)
            painter.setPen(pen)
            painter.setBrush(QColor("#cc0066"))
            p1 = self.measure_points[0]
            painter.drawEllipse(int(p1[0])-4, int(p1[1])-4, 8, 8)

        # Draw coordinate points
        for i, (label, x_real, y_real, x_px, y_px) in enumerate(self.coordinate_points):
            # Draw point marker
            if i == 0:  # Origin point
                pen = QPen(QColor("#ff6600"))
                painter.setBrush(QColor("#ff6600"))
            else:
                pen = QPen(QColor("#9900cc"))
                painter.setBrush(QColor("#9900cc"))
            pen.setWidth(2)
            painter.setPen(pen)
            painter.drawEllipse(int(x_px)-5, int(y_px)-5, 10, 10)

            # Draw label
            painter.drawText(int(x_px) + 8, int(y_px) - 8, label)

        # Draw lines connecting coordinate points (polygon outline)
        if len(self.coordinate_points) > 1:
            pen = QPen(QColor("#9900cc"))
            pen.setWidth(1)
            pen.setStyle(Qt.PenStyle.DashLine)
            painter.setPen(pen)
            for i in range(len(self.coordinate_points) - 1):
                p1 = self.coordinate_points[i]
                p2 = self.coordinate_points[i + 1]
                painter.drawLine(int(p1[3]), int(p1[4]), int(p2[3]), int(p2[4]))
            # Close the polygon if 3+ points
            if len(self.coordinate_points) >= 3:
                p1 = self.coordinate_points[-1]
                p2 = self.coordinate_points[0]
                painter.drawLine(int(p1[3]), int(p1[4]), int(p2[3]), int(p2[4]))

        painter.end()
        self.pdf_label.setPixmap(overlay)

    def eventFilter(self, obj, event):
        """Handle mouse events for calibration and measurement."""
        if obj == self.pdf_label:
            if event.type() == event.Type.MouseButtonPress:
                if event.button() == Qt.MouseButton.LeftButton:
                    pos = event.position()
                    x, y = pos.x(), pos.y()

                    if self.tool_mode == "calibrate":
                        self.calibration_points.append((x, y))
                        if len(self.calibration_points) == 2:
                            self._finish_calibration()
                        else:
                            self.status_label.setText("Click second point...")
                        self._draw_overlay()

                    elif self.tool_mode == "measure":
                        self.measure_points.append((x, y))
                        if len(self.measure_points) == 2:
                            self._finish_measurement()
                        else:
                            self.status_label.setText("Click second point...")
                        self._draw_overlay()

                    elif self.tool_mode == "coords":
                        self._add_coordinate_point(x, y)
                        self._draw_overlay()

                    return True
        return super().eventFilter(obj, event)

    def _finish_calibration(self):
        """Complete calibration after two points selected."""
        import math
        p1, p2 = self.calibration_points
        pixel_dist = math.sqrt((p2[0]-p1[0])**2 + (p2[1]-p1[1])**2)

        # Ask for real distance
        from PyQt6.QtWidgets import QInputDialog
        text, ok = QInputDialog.getText(
            self, "Calibrate Scale",
            f"Pixel distance: {pixel_dist:.1f}px\nEnter real-world distance (e.g., '100 mm' or '3.5 ft'):"
        )

        if ok and text:
            try:
                # Parse distance and unit
                parts = text.strip().split()
                if len(parts) == 1:
                    dist = float(parts[0])
                    unit = "mm"
                else:
                    dist = float(parts[0])
                    unit = parts[1].lower()

                self.calibration_distance = dist
                self.scale_unit = unit
                self.scale_factor = pixel_dist / dist

                # Calculate and display drawing scale
                drawing_scale = self.get_drawing_scale()
                self.scale_label.setText(f"Drawing Scale: {drawing_scale}")
                self.status_label.setText(f"Calibrated: {dist} {unit} = {pixel_dist:.1f}px (Scale {drawing_scale})")

                # Enable export and coordinate buttons
                self.export_btn.setEnabled(True)
                self.coords_btn.setEnabled(True)

            except ValueError:
                self.status_label.setText("Invalid input. Try '100 mm' or '3.5 ft'")
                self.calibration_points = []
        else:
            self.calibration_points = []

        self.tool_mode = "view"
        self.calibrate_btn.setChecked(False)
        self._draw_overlay()

    def _finish_measurement(self):
        """Complete measurement after two points selected."""
        import math
        p1, p2 = self.measure_points
        pixel_dist = math.sqrt((p2[0]-p1[0])**2 + (p2[1]-p1[1])**2)

        if self.scale_factor:
            real_dist = pixel_dist / self.scale_factor
            self.measurements.append((p1, p2, real_dist))
            self.status_label.setText(f"Measured: {real_dist:.2f} {self.scale_unit}")
        else:
            self.status_label.setText(f"Pixels: {pixel_dist:.1f}px (calibrate first for real units)")

        self.measure_points = []
        self._draw_overlay()

    def toggle_calibrate(self):
        """Toggle calibration mode."""
        if self.calibrate_btn.isChecked():
            self.tool_mode = "calibrate"
            self.calibration_points = []
            self.measure_btn.setChecked(False)
            self.coords_btn.setChecked(False)
            self.status_label.setText("Click first point on a known dimension...")
        else:
            self.tool_mode = "view"

    def toggle_measure(self):
        """Toggle measurement mode."""
        if self.measure_btn.isChecked():
            self.tool_mode = "measure"
            self.measure_points = []
            self.calibrate_btn.setChecked(False)
            self.coords_btn.setChecked(False)
            if not self.scale_factor:
                self.status_label.setText("Measuring in pixels (calibrate for real units)")
            else:
                self.status_label.setText("Click first point to measure...")
        else:
            self.tool_mode = "view"

    def toggle_coords_mode(self):
        """Toggle coordinate capture mode."""
        if self.coords_btn.isChecked():
            self.tool_mode = "coords"
            self.calibrate_btn.setChecked(False)
            self.measure_btn.setChecked(False)
            if not self.coordinate_points:
                self.status_label.setText("Click ORIGIN point first (will be 0,0)...")
            else:
                self.status_label.setText(f"Click to add point (have {len(self.coordinate_points)} points)")
        else:
            self.tool_mode = "view"

    def _add_coordinate_point(self, x_px, y_px):
        """Add a coordinate point at the clicked position."""
        from PyQt6.QtWidgets import QInputDialog

        # Calculate real-world coordinates
        if not self.coordinate_points:
            # First point is the origin
            x_real, y_real = 0.0, 0.0
            self.coord_origin = (x_px, y_px)
            default_label = "Origin"
        else:
            # Calculate relative to origin
            ox, oy = self.coord_origin
            x_real = (x_px - ox) / self.scale_factor
            y_real = (oy - y_px) / self.scale_factor  # Y inverted (screen coords)
            default_label = f"P{len(self.coordinate_points)}"

        # Ask for label
        label, ok = QInputDialog.getText(
            self, "Point Label",
            f"Coordinates: ({x_real:.2f}, {y_real:.2f}) {self.scale_unit}\n\nEnter label for this point:",
            text=default_label
        )

        if ok and label:
            self.coordinate_points.append((label, x_real, y_real, x_px, y_px))
            self.export_coords_btn.setEnabled(True)
            self.status_label.setText(
                f"Added {label}: ({x_real:.2f}, {y_real:.2f}) {self.scale_unit} "
                f"[{len(self.coordinate_points)} points total]"
            )

    def export_coordinates(self):
        """Export captured coordinates to CSV."""
        if not self.coordinate_points:
            self.status_label.setText("No coordinates captured")
            return

        from PyQt6.QtWidgets import QFileDialog

        default_name = "coordinates.csv"
        if self.pdf_doc:
            default_name = Path(self.pdf_doc.name).stem + "_coords.csv"

        save_path, _ = QFileDialog.getSaveFileName(
            self, "Export Coordinates",
            default_name,
            "CSV Files (*.csv);;Text Files (*.txt)"
        )

        if not save_path:
            return

        try:
            with open(save_path, "w", encoding="utf-8") as f:
                # Header
                f.write(f"# Coordinate Export\n")
                f.write(f"# Unit: {self.scale_unit}\n")
                f.write(f"# Scale: {self.get_drawing_scale()}\n")
                f.write(f"# Points: {len(self.coordinate_points)}\n")
                f.write(f"#\n")
                f.write(f"Label,X ({self.scale_unit}),Y ({self.scale_unit})\n")

                for label, x_real, y_real, x_px, y_px in self.coordinate_points:
                    f.write(f"{label},{x_real:.4f},{y_real:.4f}\n")

                # Add polygon area if 3+ points
                if len(self.coordinate_points) >= 3:
                    area = self._calculate_polygon_area()
                    f.write(f"#\n")
                    f.write(f"# Polygon Area: {area:.4f} sq {self.scale_unit}\n")

            self.status_label.setText(f"Exported {len(self.coordinate_points)} coordinates to {Path(save_path).name}")

        except Exception as e:
            self.status_label.setText(f"Export error: {e}")

    def _calculate_polygon_area(self):
        """Calculate polygon area using shoelace formula."""
        if len(self.coordinate_points) < 3:
            return 0

        # Extract x, y coordinates
        coords = [(p[1], p[2]) for p in self.coordinate_points]
        n = len(coords)

        # Shoelace formula
        area = 0
        for i in range(n):
            j = (i + 1) % n
            area += coords[i][0] * coords[j][1]
            area -= coords[j][0] * coords[i][1]

        return abs(area) / 2

    def clear_coordinates(self):
        """Clear all coordinate points."""
        self.coordinate_points = []
        self.coord_origin = None
        self.export_coords_btn.setEnabled(False)
        self._draw_overlay()
        self.status_label.setText("Coordinates cleared")

    def zoom_in(self):
        self.zoom = min(4.0, self.zoom * 1.25)
        self.render_page()

    def zoom_out(self):
        self.zoom = max(0.25, self.zoom / 1.25)
        self.render_page()

    def next_page(self):
        if self.pdf_doc and self.current_page < len(self.pdf_doc) - 1:
            self.current_page += 1
            self.measurements = []  # Clear measurements on page change
            self.render_page()

    def prev_page(self):
        if self.pdf_doc and self.current_page > 0:
            self.current_page -= 1
            self.measurements = []
            self.render_page()

    def clear_measurements(self):
        """Clear all measurements and coordinates from the page."""
        self.measurements = []
        self.measure_points = []
        self.coordinate_points = []
        self.coord_origin = None
        self.export_coords_btn.setEnabled(False)
        self._draw_overlay()
        self.status_label.setText("All measurements and coordinates cleared")

    def export_one_to_one(self):
        """Export current page at 1:1 scale for accurate printing."""
        try:
            self._do_export_one_to_one()
        except Exception as e:
            self.status_label.setText(f"Export error: {e}")
            import traceback
            traceback.print_exc()

    def _do_export_one_to_one(self):
        """Internal export implementation."""
        self.status_label.setText("Preparing 1:1 export...")

        if not self.pdf_doc:
            self.status_label.setText("No PDF loaded")
            return

        if not self.scale_factor:
            self.status_label.setText("Calibrate first to enable 1:1 export")
            return

        try:
            import fitz
        except ImportError:
            self.status_label.setText("PyMuPDF not installed")
            return

        # Calculate the DPI needed for 1:1 printing
        # scale_factor = pixels / real_unit at current render DPI
        # For 1:1 printing, we need: printed_size = real_size
        # At print time, printers use 72 DPI for PDF points
        # So we need to calculate what DPI renders the drawing at 1:1

        # Get page size in points (72 points = 1 inch)
        page = self.pdf_doc[self.current_page]
        page_rect = page.rect
        page_width_pts = page_rect.width
        page_height_pts = page_rect.height

        # Convert scale unit to inches for calculation
        unit_to_inch = {
            "mm": 1 / 25.4,
            "cm": 1 / 2.54,
            "m": 39.37,
            "in": 1.0,
            "ft": 12.0,
            "inch": 1.0,
            "inches": 1.0,
        }
        inch_factor = unit_to_inch.get(self.scale_unit.lower(), 1/25.4)

        # Calculate the scale ratio
        # At current DPI, scale_factor pixels = 1 real unit
        # Pixels at current DPI: scale_factor
        # Real size in inches: 1 * inch_factor
        # Current render: self.dpi pixels per inch
        # Drawing scale = (scale_factor / self.dpi) / inch_factor
        drawing_scale = (self.scale_factor / self.dpi) / inch_factor

        # For 1:1 export, we need to render at a DPI that makes drawing_scale = 1
        # target_dpi such that (scale_factor / target_dpi) / inch_factor = 1
        # target_dpi = scale_factor / inch_factor
        target_dpi = self.scale_factor / inch_factor

        # Calculate resulting paper size in inches
        paper_width_in = page_width_pts / 72 * (target_dpi / 72)
        paper_height_in = page_height_pts / 72 * (target_dpi / 72)

        # Ask for output file
        from PyQt6.QtWidgets import QFileDialog, QMessageBox

        # Show info about the export
        info_msg = f"""1:1 Export Information:

Current drawing scale: 1:{1/drawing_scale:.1f}
Target scale: 1:1 (actual size)

Rendering at {target_dpi:.0f} DPI

The exported image will print at actual size when printed
at 100% scale (no fit-to-page).

Note: Very large drawings may create large files."""

        reply = QMessageBox.information(
            self, "1:1 Export",
            info_msg,
            QMessageBox.StandardButton.Ok | QMessageBox.StandardButton.Cancel
        )

        if reply != QMessageBox.StandardButton.Ok:
            return

        # Get save path
        default_name = Path(self.pdf_doc.name).stem + "_1to1.pdf"
        save_path, _ = QFileDialog.getSaveFileName(
            self, "Export 1:1 PDF",
            default_name,
            "PDF Files (*.pdf);;PNG Image (*.png)"
        )

        if not save_path:
            return

        try:
            # Render at target DPI
            mat = fitz.Matrix(target_dpi / 72, target_dpi / 72)
            pix = page.get_pixmap(matrix=mat)

            if save_path.lower().endswith('.png'):
                # Save as PNG
                pix.save(save_path)
                self.status_label.setText(f"Exported: {Path(save_path).name} ({pix.width}x{pix.height}px)")
            else:
                # Create a new PDF with the rendered image
                # The image is embedded at the size that makes it print 1:1
                out_doc = fitz.open()

                # Calculate page size in points for 1:1 printing at 72 DPI
                # At 72 DPI, 1 inch = 72 points
                # Our image is target_dpi pixels per inch of real drawing
                # So image dimensions / target_dpi = size in inches
                # Size in points = (image dimensions / target_dpi) * 72
                img_width_pts = (pix.width / target_dpi) * 72
                img_height_pts = (pix.height / target_dpi) * 72

                out_page = out_doc.new_page(width=img_width_pts, height=img_height_pts)

                # Insert the image to fill the page
                out_page.insert_image(
                    out_page.rect,
                    pixmap=pix
                )

                out_doc.save(save_path)
                out_doc.close()

                self.status_label.setText(
                    f"Exported 1:1: {Path(save_path).name} "
                    f"({img_width_pts/72:.1f}\" x {img_height_pts/72:.1f}\")"
                )

        except Exception as e:
            self.status_label.setText(f"Export error: {e}")

    def get_drawing_scale(self):
        """Calculate and return the drawing scale ratio (e.g., 1:100)."""
        if not self.scale_factor:
            return None

        # Convert to inches for consistent calculation
        unit_to_inch = {
            "mm": 1 / 25.4,
            "cm": 1 / 2.54,
            "m": 39.37,
            "in": 1.0,
            "ft": 12.0,
        }
        inch_factor = unit_to_inch.get(self.scale_unit.lower(), 1/25.4)

        # scale_factor = pixels per real unit at current DPI
        # 1 inch on screen = self.dpi pixels
        # 1 real unit = scale_factor pixels
        # ratio = screen_size / real_size = (scale_factor / self.dpi) / inch_factor
        ratio = (self.scale_factor / self.dpi) / inch_factor

        if ratio >= 1:
            return f"{ratio:.1f}:1"  # Enlarged
        else:
            return f"1:{1/ratio:.0f}"  # Reduced (typical for drawings)


class ReaderTab(QWidget):
    """Reader view for PDF/EPUB."""

    def __init__(self, file_path: str, parent=None):
        super().__init__(parent)
        self.file_path = file_path
        self.modified = False
        self.mode = "text"
        self.full_text = ""
        self.pages = []
        self.current_page = 0
        self.font_size = 12

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        self.toolbar = QWidget()
        toolbar_layout = QHBoxLayout(self.toolbar)
        toolbar_layout.setContentsMargins(8, 6, 8, 6)
        toolbar_layout.setSpacing(6)

        self.prev_btn = QPushButton("Prev")
        self.prev_btn.clicked.connect(self.prev_page)
        toolbar_layout.addWidget(self.prev_btn)

        self.page_label = QLabel("Page 1/1")
        toolbar_layout.addWidget(self.page_label)

        self.next_btn = QPushButton("Next")
        self.next_btn.clicked.connect(self.next_page)
        toolbar_layout.addWidget(self.next_btn)

        self.font_minus_btn = QPushButton("A-")
        self.font_minus_btn.clicked.connect(self.decrease_font)
        toolbar_layout.addWidget(self.font_minus_btn)

        self.font_plus_btn = QPushButton("A+")
        self.font_plus_btn.clicked.connect(self.increase_font)
        toolbar_layout.addWidget(self.font_plus_btn)

        toolbar_layout.addStretch()
        layout.addWidget(self.toolbar)

        self.stack = QStackedWidget()
        layout.addWidget(self.stack)

        self.reader = QTextBrowser()
        self.reader.setOpenExternalLinks(True)
        self.reader.setFont(QFont("Segoe UI", self.font_size))
        self.stack.addWidget(self.reader)

        if QWebEngineView:
            self.web_view = QWebEngineView()
            self.stack.addWidget(self.web_view)
        else:
            self.web_view = None

        # Scaled PDF viewer with measurement tools
        self.scaled_pdf_viewer = ScaledPDFViewer()
        self.stack.addWidget(self.scaled_pdf_viewer)

        self.load_file(file_path)

    def load_file(self, file_path):
        suffix = Path(file_path).suffix.lower()
        if suffix == ".pdf":
            self.mode = "pdf"
            self.load_pdf(file_path)
        elif suffix == ".epub":
            self.mode = "epub"
            self.load_epub(file_path)
        elif suffix in (".md", ".markdown"):
            self.mode = "markdown"
            self.load_markdown(file_path)
        else:
            self.mode = "text"
            self.load_text(file_path)

    def load_pdf(self, file_path):
        # Use the scaled PDF viewer with measurement tools
        self.scaled_pdf_viewer.load_pdf(file_path)
        self.stack.setCurrentWidget(self.scaled_pdf_viewer)
        self.toolbar.setVisible(False)  # Use the scaled viewer's toolbar

    def load_epub(self, file_path):
        try:
            from ebooklib import epub
        except Exception:
            self.reader.setHtml(
                "<h3>EPUB support unavailable</h3>"
                "<p>Install ebooklib to read EPUB files.</p>"
            )
            self.stack.setCurrentWidget(self.reader)
            return

        book = epub.read_epub(file_path)
        parts = []
        for item in book.get_items():
            if item.get_type() == epub.ITEM_DOCUMENT:
                parts.append(item.get_content().decode("utf-8", errors="ignore"))
        html = "\n".join(parts)
        text = self._html_to_text(html)
        self.full_text = text
        self._paginate_text(self.full_text)
        self.show_page(0)
        self.stack.setCurrentWidget(self.reader)

    def load_text(self, file_path):
        try:
            content = Path(file_path).read_text(encoding="utf-8")
        except Exception:
            content = ""
        self.full_text = content
        self._paginate_text(self.full_text)
        self.show_page(0)
        self.stack.setCurrentWidget(self.reader)

    def load_markdown(self, file_path):
        try:
            content = Path(file_path).read_text(encoding="utf-8")
        except Exception:
            content = ""
        if markdown_lib:
            md = markdown_lib.Markdown(extensions=["extra", "sane_lists"])
            html = md.convert(content)
            text = self._html_to_text(html)
        else:
            text = content
        self.full_text = text
        self._paginate_text(self.full_text)
        self.show_page(0)
        self.stack.setCurrentWidget(self.reader)

    def _html_to_text(self, html):
        text = re.sub(r"<[^>]+>", " ", html)
        return re.sub(r"\s+", " ", text).strip()

    def _paginate_text(self, text):
        words = text.split()
        if not words:
            self.pages = [""]
            self.current_page = 0
            return

        words_per_page = self._estimate_words_per_page()
        pages = []
        for i in range(0, len(words), words_per_page):
            pages.append(" ".join(words[i : i + words_per_page]))
        self.pages = pages
        self.current_page = 0

    def _estimate_words_per_page(self):
        width = max(1, self.reader.viewport().width())
        height = max(1, self.reader.viewport().height())
        metrics = self.reader.fontMetrics()
        avg_char = max(6, metrics.averageCharWidth())
        line_height = max(12, metrics.height())
        lines = max(1, height // line_height)
        chars_per_line = max(20, width // avg_char)
        return max(200, int(lines * chars_per_line * 0.6))

    def show_page(self, index):
        index = max(0, min(index, len(self.pages) - 1))
        self.current_page = index
        content = self.pages[index] if self.pages else ""
        self.reader.setPlainText(content)
        self.page_label.setText(f"Page {index + 1}/{len(self.pages)}")
        self.prev_btn.setEnabled(index > 0)
        self.next_btn.setEnabled(index < len(self.pages) - 1)

    def next_page(self):
        if self.current_page + 1 < len(self.pages):
            self.show_page(self.current_page + 1)

    def prev_page(self):
        if self.current_page > 0:
            self.show_page(self.current_page - 1)

    def increase_font(self):
        self.font_size = min(24, self.font_size + 1)
        self._apply_font()

    def decrease_font(self):
        self.font_size = max(9, self.font_size - 1)
        self._apply_font()

    def _apply_font(self):
        self.reader.setFont(QFont("Segoe UI", self.font_size))
        if self.mode in ("text", "markdown", "epub"):
            ratio = 0 if not self.pages else self.current_page / max(1, len(self.pages) - 1)
            self._paginate_text(self.full_text)
            new_index = int(ratio * max(1, len(self.pages) - 1))
            self.show_page(new_index)

    def resizeEvent(self, event):
        super().resizeEvent(event)
        if self.mode in ("text", "markdown", "epub"):
            ratio = 0 if not self.pages else self.current_page / max(1, len(self.pages) - 1)
            self._paginate_text(self.full_text)
            new_index = int(ratio * max(1, len(self.pages) - 1))
            self.show_page(new_index)

class CommandPalette(QDialog):
    """Command palette for on-demand commands."""

    def __init__(self, command_registry, context, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Command Palette")
        self.setMinimumWidth(480)
        self.command_registry = command_registry
        self.context = context
        self.commands = []

        layout = QVBoxLayout(self)

        self.filter_input = QLineEdit()
        self.filter_input.setPlaceholderText("Type to filter commands...")
        self.filter_input.textChanged.connect(self._filter_commands)
        layout.addWidget(self.filter_input)

        self.list_widget = QListWidget()
        self.list_widget.itemActivated.connect(self._run_selected)
        layout.addWidget(self.list_widget)

        self._load_commands()

    def _load_commands(self):
        self.commands = list(self.command_registry._commands.values())
        self.commands.sort(key=lambda cmd: cmd.title.lower())
        self._filter_commands()

    def _filter_commands(self):
        query = self.filter_input.text().strip().lower()
        self.list_widget.clear()
        for command in self.commands:
            if query and query not in command.title.lower() and query not in command.command_id.lower():
                continue
            item = QListWidgetItem(f"{command.title}  ({command.command_id})")
            item.setData(Qt.ItemDataRole.UserRole, command.command_id)
            self.list_widget.addItem(item)
        if self.list_widget.count() > 0:
            self.list_widget.setCurrentRow(0)

    def _run_selected(self):
        item = self.list_widget.currentItem()
        if not item:
            return
        command_id = item.data(Qt.ItemDataRole.UserRole)
        self.command_registry.execute(command_id, self.context)
        self.accept()

    def showEvent(self, event):
        super().showEvent(event)
        self.filter_input.setFocus()
        self.filter_input.selectAll()


class TextResultDialog(QDialog):
    """Simple dialog for AI text results with optional insert."""

    def __init__(self, title, content, allow_insert, parent=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setMinimumWidth(520)
        self._content = content
        self._allow_insert = allow_insert
        self._insert_requested = False

        layout = QVBoxLayout(self)
        self.text_area = QPlainTextEdit()
        self.text_area.setReadOnly(True)
        self.text_area.setPlainText(content)
        layout.addWidget(self.text_area)

        button_row = QHBoxLayout()
        button_row.addStretch()

        if allow_insert:
            insert_btn = QPushButton("Insert at Cursor")
            insert_btn.clicked.connect(self._request_insert)
            button_row.addWidget(insert_btn)

        close_btn = QPushButton("Close")
        close_btn.clicked.connect(self.reject)
        button_row.addWidget(close_btn)
        layout.addLayout(button_row)

    def _request_insert(self):
        self._insert_requested = True
        self.accept()

    @property
    def insert_requested(self):
        return self._insert_requested

    @property
    def content(self):
        return self._content


class ReviewTextDialog(QDialog):
    """Review text changes with optional replace/insert actions."""

    def __init__(self, title, original, revised, options, parent=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setMinimumWidth(760)
        self._choice = ""

        layout = QVBoxLayout(self)

        label = QLabel("Original")
        layout.addWidget(label)
        original_view = QPlainTextEdit()
        original_view.setReadOnly(True)
        original_view.setPlainText(original)
        layout.addWidget(original_view)

        label = QLabel("Revised")
        layout.addWidget(label)
        revised_view = QPlainTextEdit()
        revised_view.setReadOnly(True)
        revised_view.setPlainText(revised)
        layout.addWidget(revised_view)

        button_row = QHBoxLayout()
        button_row.addStretch()

        for option in options:
            if option == "Cancel":
                continue
            btn = QPushButton(option)
            btn.clicked.connect(lambda _checked=False, value=option: self._set_choice(value))
            button_row.addWidget(btn)

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        button_row.addWidget(cancel_btn)
        layout.addLayout(button_row)

    def _set_choice(self, value):
        self._choice = value
        self.accept()

    @property
    def choice(self):
        return self._choice


class SuggestionDialog(QDialog):
    """Suggestion list with insert action."""

    def __init__(self, title, suggestions, parent=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setMinimumWidth(420)
        self._choice = ""

        layout = QVBoxLayout(self)
        self.list_widget = QListWidget()
        for suggestion in suggestions:
            self.list_widget.addItem(QListWidgetItem(suggestion))
        self.list_widget.itemActivated.connect(self._accept_current)
        layout.addWidget(self.list_widget)

        button_row = QHBoxLayout()
        button_row.addStretch()
        insert_btn = QPushButton("Insert")
        insert_btn.clicked.connect(self._accept_current)
        button_row.addWidget(insert_btn)
        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        button_row.addWidget(cancel_btn)
        layout.addLayout(button_row)

        if self.list_widget.count() > 0:
            self.list_widget.setCurrentRow(0)

    def _accept_current(self):
        item = self.list_widget.currentItem()
        if not item:
            return
        self._choice = item.text()
        self.accept()

    @property
    def choice(self):
        return self._choice


class TaskSelectionDialog(QDialog):
    """Select a task from a list."""

    def __init__(self, title, tasks, parent=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setMinimumWidth(520)
        self._choice = ""

        layout = QVBoxLayout(self)
        self.list_widget = QListWidget()
        for task in tasks:
            status = "[x]" if task["status"] == "done" else "[ ]"
            due = f" due:{task['due']}" if task.get("due") else ""
            item = QListWidgetItem(f"{status} {task['text']}{due}")
            item.setData(Qt.ItemDataRole.UserRole, task["task_id"])
            self.list_widget.addItem(item)
        self.list_widget.itemActivated.connect(self._accept_current)
        layout.addWidget(self.list_widget)

        button_row = QHBoxLayout()
        button_row.addStretch()
        open_btn = QPushButton("Select")
        open_btn.clicked.connect(self._accept_current)
        button_row.addWidget(open_btn)
        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        button_row.addWidget(cancel_btn)
        layout.addLayout(button_row)

        if self.list_widget.count() > 0:
            self.list_widget.setCurrentRow(0)

    def _accept_current(self):
        item = self.list_widget.currentItem()
        if not item:
            return
        self._choice = item.data(Qt.ItemDataRole.UserRole)
        self.accept()

    @property
    def choice(self):
        return self._choice
class MarkdownTab(QWidget):
    """A single tab with toggle between edit and view mode."""

    def __init__(
        self,
        file_path: str = None,
        dark_mode: bool = False,
        graphics_enabled: bool = False,
        context_menu_config=None,
        lexicon=None,
        ai_client=None,
        write_console_callback=None,
        parent=None,
    ):
        super().__init__(parent)
        self.events = EventsSpine()
        self.document = Document(self.events)
        self.commands = CommandRegistry()
        self.renderer = MarkdownRenderer()
        self.language_service = MarkdownLanguageService()
        self.preview_provider = PreviewProvider(self.renderer)
        self.outline_provider = OutlineProvider(self.language_service)
        self.word_count_provider = WordCountProvider()
        self.formatting_provider = FormattingProvider()
        # Use provided AI client or create a hybrid one (LM Studio + local fallback)
        self.ai_client = ai_client or HybridAIClient()
        self.lexicon = lexicon or LocalLexicon()
        self.intent_map_provider = IntentMapProvider(self.ai_client)
        self.citation_helper_provider = CitationHelperProvider(self.ai_client)
        self.diff_narrator_provider = DiffNarratorProvider(self.ai_client)
        self.outline_enhancer_provider = OutlineEnhancerProvider(self.ai_client)
        self.action_extractor_provider = ActionExtractorProvider(self.ai_client)
        self.image_generator_provider = ImageGeneratorProvider()
        self.typo_fixer_provider = TypoFixerProvider(self.ai_client)
        self.suggestion_provider = SuggestionProvider(self.ai_client)
        self.lexicon_provider = LexiconProvider(self.lexicon)
        self.coding_agent_provider = CodingAgentProvider(self.ai_client)
        self.code_runner_provider = CodeRunnerProvider()
        self.build_provider = BuildProvider()
        self.shell_provider = ShellProvider()
        self.python_interpreter_provider = PythonInterpreterProvider()
        self.history_provider = HistoryProvider(self.ai_client)
        self.dark_mode = dark_mode
        self.edit_mode = False
        self._write_console_callback = write_console_callback
        self.graphics_enabled = graphics_enabled
        self.context_menu_config = context_menu_config or {"enabled": set()}

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Mode/toolbar bar
        self.toolbar = QWidget()
        toolbar_layout = QHBoxLayout(self.toolbar)
        toolbar_layout.setContentsMargins(8, 6, 8, 6)
        toolbar_layout.setSpacing(4)

        # Toggle button (prominent)
        self.toggle_btn = QPushButton("Edit (F2)")
        self.toggle_btn.setMinimumWidth(100)
        self.toggle_btn.clicked.connect(self.toggle_mode)
        toolbar_layout.addWidget(self.toggle_btn)

        # Separator
        toolbar_layout.addWidget(self._separator())

        # Formatting buttons (only visible in edit mode)
        self.format_buttons = QWidget()
        fmt_layout = QHBoxLayout(self.format_buttons)
        fmt_layout.setContentsMargins(0, 0, 0, 0)
        fmt_layout.setSpacing(2)

        self.btn_h1 = self._make_cmd_btn("H1", "# Heading 1", "markdown.format.heading1")
        self.btn_h2 = self._make_cmd_btn("H2", "## Heading 2", "markdown.format.heading2")
        self.btn_h3 = self._make_cmd_btn("H3", "### Heading 3", "markdown.format.heading3")
        fmt_layout.addWidget(self.btn_h1)
        fmt_layout.addWidget(self.btn_h2)
        fmt_layout.addWidget(self.btn_h3)

        fmt_layout.addWidget(self._separator())

        self.btn_bold = self._make_cmd_btn("B", "**Bold**", "markdown.format.bold")
        self.btn_bold.setStyleSheet("font-weight: bold;")
        self.btn_italic = self._make_cmd_btn("I", "*Italic*", "markdown.format.italic")
        self.btn_italic.setStyleSheet("font-style: italic;")
        self.btn_code = self._make_cmd_btn("</>", "`Code`", "markdown.format.code")
        fmt_layout.addWidget(self.btn_bold)
        fmt_layout.addWidget(self.btn_italic)
        fmt_layout.addWidget(self.btn_code)

        fmt_layout.addWidget(self._separator())

        self.btn_link = self._make_cmd_btn("Link", "[text](url)", "markdown.format.link")
        self.btn_image = self._make_cmd_btn("Img", "![alt](url)", "markdown.format.image")
        fmt_layout.addWidget(self.btn_link)
        fmt_layout.addWidget(self.btn_image)

        fmt_layout.addWidget(self._separator())

        self.btn_ul = self._make_cmd_btn("* List", "Bullet list", "markdown.format.list.bulleted")
        self.btn_ol = self._make_cmd_btn("1. List", "Numbered list", "markdown.format.list.numbered")
        self.btn_quote = self._make_cmd_btn("> Quote", "Blockquote", "markdown.format.quote")
        fmt_layout.addWidget(self.btn_ul)
        fmt_layout.addWidget(self.btn_ol)
        fmt_layout.addWidget(self.btn_quote)

        fmt_layout.addWidget(self._separator())

        self.btn_table = self._make_cmd_btn("Table", "Insert table", "markdown.format.table")
        self.btn_codeblock = self._make_cmd_btn("Code Block", "```code```", "markdown.format.codeblock")
        self.btn_mermaid = self._make_cmd_btn("Diagram", "Mermaid diagram", "markdown.format.diagram")
        fmt_layout.addWidget(self.btn_table)
        fmt_layout.addWidget(self.btn_codeblock)
        fmt_layout.addWidget(self.btn_mermaid)

        self.format_buttons.hide()
        toolbar_layout.addWidget(self.format_buttons)

        toolbar_layout.addStretch()

        # Mode label
        self.mode_label = QLabel("VIEW")
        self.mode_label.setStyleSheet("font-weight: bold; padding: 4px 8px;")
        toolbar_layout.addWidget(self.mode_label)

        layout.addWidget(self.toolbar)

        # Search bar
        self.search_bar = SearchBar()
        layout.addWidget(self.search_bar)

        # Stacked widget for switching between views
        self.stack = QStackedWidget()
        layout.addWidget(self.stack)

        # View mode (rendered preview)
        self.preview = QTextBrowser()
        self.preview.setOpenExternalLinks(True)
        self.preview.setFont(QFont("Segoe UI", 11))
        self.stack.addWidget(self.preview)

        # Edit mode (plain text editor)
        self.editor = QPlainTextEdit()
        self.editor.setFont(QFont("Consolas", 11))
        self.editor.setTabStopDistance(40)
        self.editor.textChanged.connect(self.on_text_changed)
        self.editor.cursorPositionChanged.connect(self.on_cursor_moved)
        self.editor.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.editor.customContextMenuRequested.connect(self.show_editor_context_menu)
        self.highlighter = None
        self._setup_highlighter(dark_mode)
        self.stack.addWidget(self.editor)

        self.context = IDEContext(
            self.document,
            self.events,
            self.commands,
            editor=self.editor,
            preview=self.preview,
            confirm_callback=self._confirm_action,
            present_text_callback=self.present_text,
            request_save_path_callback=self.request_save_path,
            choose_option_callback=self.choose_option,
            review_text_callback=self.review_text,
            present_suggestions_callback=self.present_suggestions,
            write_console_callback=self._write_console_callback,
        )
        self.document_command_provider = DocumentCommandProvider(
            self.save_file,
            self.save_file_as,
            self.print_document,
            self.export_pdf,
        )
        self.providers = [
            self.language_service,
            self.preview_provider,
            self.outline_provider,
            self.word_count_provider,
            self.formatting_provider,
            self.intent_map_provider,
            self.citation_helper_provider,
            self.diff_narrator_provider,
            self.outline_enhancer_provider,
            self.action_extractor_provider,
            self.typo_fixer_provider,
            self.suggestion_provider,
            self.lexicon_provider,
            self.coding_agent_provider,
            self.code_runner_provider,
            self.build_provider,
            self.shell_provider,
            self.python_interpreter_provider,
            self.history_provider,
        ]
        if self.graphics_enabled:
            self.providers.append(self.image_generator_provider)
        self.providers.append(self.document_command_provider)
        for provider in self.providers:
            provider.activate(self.context)

        self.events.document_changed.connect(self.refresh_view)
        self.events.document_changed.connect(self.refresh_outline)
        self.events.document_changed.connect(self.refresh_word_count)
        self.events.document_saved.connect(self.refresh_word_count)

        self.search_bar.set_target(self.preview)
        self.apply_theme()

        if file_path:
            self.load_file(file_path)
        else:
            self.context.set_content("# New Document\n\nStart writing...\n")
            self.refresh_view()
        self.toggle_mode()

    def _make_btn(self, text, tooltip, callback):
        btn = QPushButton(text)
        btn.setToolTip(tooltip)
        btn.setMaximumWidth(70)
        btn.setMinimumWidth(35)
        btn.clicked.connect(callback)
        return btn

    def _make_cmd_btn(self, text, tooltip, command_id):
        return self._make_btn(
            text, tooltip, lambda: self.commands.execute(command_id, self.context)
        )

    def _separator(self):
        sep = QLabel("|")
        sep.setStyleSheet("color: #aaa; padding: 0 4px;")
        return sep

    @property
    def file_path(self):
        return self.document.file_path

    @property
    def modified(self):
        return self.document.modified

    def _confirm_action(self, title):
        reply = QMessageBox.question(
            self,
            title,
            title,
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
        )
        return reply == QMessageBox.StandardButton.Yes

    def present_text(self, title, content, allow_insert):
        dialog = TextResultDialog(title, content, allow_insert, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return False
        if dialog.insert_requested:
            self.context.insert_text(dialog.content)
            return True
        return False

    def review_text(self, title, original, revised, options):
        dialog = ReviewTextDialog(title, original, revised, options, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return ""
        return dialog.choice

    def present_suggestions(self, title, suggestions):
        dialog = SuggestionDialog(title, suggestions, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return ""
        return dialog.choice

    def show_editor_context_menu(self, pos):
        menu = self.editor.createStandardContextMenu()
        menu.addSeparator()

        enabled = self.context_menu_config.get("enabled", set())

        # Detect language from file extension
        lang = self._detect_file_language()
        has_selection = bool(self.context.get_selection())

        actions = [
            ("quick_stats", "Quick Stats", self.show_quick_stats),
            ("lookup_definition", "Lookup Definition", self.show_definition_bubble),
            ("show_synonyms", "Show Synonyms", self.show_synonyms_bubble),
            ("text_suggestions", "Text Suggestions", lambda: self.commands.execute("document.text.suggestions", self.context)),
            ("fix_typos", "Fix Typos", lambda: self.commands.execute("document.typo.fix", self.context)),
            ("intent_map", "Intent Map", lambda: self.commands.execute("document.intent.map", self.context)),
            ("citation_helper", "Citation Helper", lambda: self.commands.execute("document.citation.helper", self.context)),
            ("outline_enhancer", "Outline Enhancer", lambda: self.commands.execute("document.outline.enhancer", self.context)),
            ("action_extractor", "Action Extractor", lambda: self.commands.execute("document.action.extractor", self.context)),
            ("diff_narrator", "Diff Narrator", lambda: self.commands.execute("document.diff.narrator", self.context)),
            ("generate_image", "Generate Image", lambda: self.commands.execute("markdown.generate.image", self.context)),
        ]

        # Add language-specific code actions
        if has_selection:
            if lang == "python":
                actions.append(("run_selection", "Run Python Selection", lambda: self.commands.execute("python.eval", self.context)))
                actions.append(("python_eval", "Python: Evaluate", lambda: self.commands.execute("python.eval", self.context)))
            elif lang in ("javascript", "node"):
                actions.append(("run_selection", f"Run {lang.title()} Selection", lambda: self.commands.execute("code.run.selection", self.context)))
            else:
                actions.append(("run_selection", "Run Selection", lambda: self.commands.execute("code.run.selection", self.context)))

        # File-level code actions
        if lang:
            lang_label = {"python": "Python", "javascript": "JavaScript", "node": "Node.js"}.get(lang, lang.title())
            actions.append(("run_file", f"Run {lang_label} File", lambda: self.commands.execute("code.run", self.context)))

        actions.append(("build_project", "Build Project", lambda: self.commands.execute("build.run", self.context)))

        term = self._current_term()
        if term:
            entry = self.lexicon.lookup(term)
            if entry:
                definition = entry.get("definition", "").strip()
                synonyms = entry.get("synonyms", [])
            else:
                definition = ""
                synonyms = []
            definition_text = definition or "No definition found."
            definition_action = QAction(f"Definition: {definition_text}", self)
            definition_action.setEnabled(False)
            menu.addAction(definition_action)

            if synonyms:
                synonyms_text = ", ".join(synonyms)
            else:
                synonyms_text = "No synonyms found."
            synonym_action = QAction(f"Synonyms: {synonyms_text}", self)
            synonym_action.setEnabled(False)
            menu.addAction(synonym_action)
            menu.addSeparator()

        for action_id, label, handler in actions:
            if action_id not in enabled:
                continue
            if action_id == "generate_image" and not self.commands.get("markdown.generate.image"):
                continue
            menu_action = QAction(label, self)
            menu_action.triggered.connect(handler)
            menu.addAction(menu_action)

        menu.exec(self.editor.mapToGlobal(pos))

    def show_quick_stats(self):
        stats = self.word_count_provider.stats
        QMessageBox.information(
            self,
            "Quick Stats",
            f"Words: {stats['words']}\nCharacters: {stats['chars']}",
        )

    def _current_term(self):
        term = self.context.get_selection()
        if not term:
            cursor = self.editor.textCursor()
            cursor.select(cursor.SelectionType.WordUnderCursor)
            term = cursor.selectedText()
        return (term or "").strip()

    def _detect_file_language(self):
        """Detect programming language from file extension."""
        if not self.file_path:
            return None
        ext = self.file_path.split(".")[-1].lower() if "." in self.file_path else ""
        ext_map = {
            "py": "python",
            "pyw": "python",
            "js": "javascript",
            "mjs": "javascript",
            "cjs": "javascript",
            "ts": "typescript",
            "tsx": "typescript",
            "rs": "rust",
            "go": "go",
            "rb": "ruby",
            "sh": "shell",
            "bash": "shell",
            "ps1": "powershell",
            "c": "c",
            "cpp": "cpp",
            "h": "c",
            "hpp": "cpp",
            "java": "java",
            "kt": "kotlin",
            "swift": "swift",
            "md": "markdown",
        }
        return ext_map.get(ext)

    def _setup_highlighter(self, dark_mode=None):
        """Set up the appropriate syntax highlighter based on file type."""
        if dark_mode is None:
            dark_mode = self.dark_mode

        lang = self._detect_file_language()

        # Remove old highlighter
        if self.highlighter:
            self.highlighter.setDocument(None)

        # Choose highlighter based on language
        if lang in ("python", "javascript", "typescript"):
            hl_lang = "python" if lang == "python" else "javascript"
            self.highlighter = CodeHighlighter(self.editor.document(), dark_mode, hl_lang)
        else:
            # Default to markdown highlighter
            self.highlighter = MarkdownHighlighter(self.editor.document(), dark_mode)

    def show_definition_bubble(self):
        term = self._current_term()
        if not term:
            return
        entry = self.lexicon.lookup(term)
        if not entry:
            text = f"No definition found for '{term}'."
        else:
            definition = entry.get("definition", "")
            synonyms = entry.get("synonyms", [])
            parts = [f"{term}", definition]
            if synonyms:
                parts.append(f"Synonyms: {', '.join(synonyms)}")
            text = "\n".join([p for p in parts if p])
        QToolTip.showText(QCursor.pos(), text, self.editor)

    def show_synonyms_bubble(self):
        term = self._current_term()
        if not term:
            return
        entry = self.lexicon.lookup(term)
        if not entry or not entry.get("synonyms"):
            text = f"No synonyms found for '{term}'."
        else:
            text = f"{term}\nSynonyms: {', '.join(entry['synonyms'])}"
        QToolTip.showText(QCursor.pos(), text, self.editor)

    def request_save_path(self, title, file_filter):
        file_path, _ = QFileDialog.getSaveFileName(self, title, "", file_filter)
        return file_path

    def choose_option(self, title, message, options):
        dialog = QMessageBox(self)
        dialog.setWindowTitle(title)
        dialog.setText(message)
        for option in options:
            dialog.addButton(option, QMessageBox.ButtonRole.ActionRole)
        dialog.addButton("Cancel", QMessageBox.ButtonRole.RejectRole)
        dialog.exec()
        clicked = dialog.clickedButton()
        if not clicked:
            return ""
        return clicked.text()

    def on_cursor_moved(self):
        if self.events:
            self.events.cursor_moved.emit(self.editor.textCursor().position())

    def set_graphics_enabled(self, enabled):
        if self.graphics_enabled == enabled:
            return
        self.graphics_enabled = enabled
        self.commands.unregister("markdown.generate.image")
        if enabled:
            self.image_generator_provider.activate(self.context)

    def toggle_mode(self):
        """Toggle between edit and view mode."""
        if self.edit_mode:
            # Switch to view mode
            self.context.set_content(self.editor.toPlainText())
            self.refresh_view()
            self.stack.setCurrentWidget(self.preview)
            self.search_bar.set_target(self.preview)
            self.mode_label.setText("VIEW MODE")
            self.toggle_btn.setText("Edit (F2)")
            self.format_buttons.hide()
            self.edit_mode = False
        else:
            # Switch to edit mode
            self.editor.blockSignals(True)
            self.editor.setPlainText(self.document.content)
            self.editor.blockSignals(False)
            self.stack.setCurrentWidget(self.editor)
            self.search_bar.set_target(self.editor)
            self.mode_label.setText("EDIT MODE")
            self.toggle_btn.setText("Preview (F2)")
            self.format_buttons.show()
            self.edit_mode = True
            self.editor.setFocus()

    def on_text_changed(self):
        """Handle text changes in editor."""
        self.context.set_content(self.editor.toPlainText())

    def refresh_view(self):
        """Refresh the preview."""
        html = self.preview_provider.get_html(self.dark_mode)
        scroll_pos = self.preview.verticalScrollBar().value()
        self.preview.setHtml(html)
        self.preview.verticalScrollBar().setValue(scroll_pos)

    def refresh_outline(self, _document=None):
        if not hasattr(self, "outline_list"):
            return
        self.outline_list.clear()
        for heading in self.outline_provider.get_outline():
            indent = "  " * (heading["level"] - 1)
            item = QListWidgetItem(f"{indent}{heading['text']}")
            item.setData(Qt.ItemDataRole.UserRole, heading["line"])
            self.outline_list.addItem(item)

    def refresh_word_count(self, _document=None):
        if not hasattr(self, "word_count_label"):
            return
        stats = self.word_count_provider.stats
        self.word_count_label.setText(
            f"Words: {stats['words']}  Characters: {stats['chars']}"
        )

    def apply_theme(self):
        if self.dark_mode:
            self.editor.setStyleSheet("QPlainTextEdit { background: #1e1e1e; color: #d4d4d4; border: none; }")
            self.preview.setStyleSheet("QTextBrowser { background: #1e1e1e; color: #d4d4d4; border: none; }")
            self.toolbar.setStyleSheet("background: #2d2d2d;")
            self.mode_label.setStyleSheet("font-weight: bold; color: #888; padding: 4px 8px;")
            self.toggle_btn.setStyleSheet("""
                QPushButton {
                    background: #0078d4; color: white; border: none;
                    padding: 6px 12px; border-radius: 4px; font-weight: bold;
                }
                QPushButton:hover { background: #1084d8; }
            """)
            btn_style = """
                QPushButton {
                    background: #3c3c3c; color: #d4d4d4; border: 1px solid #555;
                    padding: 4px 8px; border-radius: 3px;
                }
                QPushButton:hover { background: #505050; }
            """
        else:
            self.editor.setStyleSheet("QPlainTextEdit { background: white; color: black; border: none; }")
            self.preview.setStyleSheet("QTextBrowser { background: white; color: black; border: none; }")
            self.toolbar.setStyleSheet("background: #f0f0f0;")
            self.mode_label.setStyleSheet("font-weight: bold; color: #666; padding: 4px 8px;")
            self.toggle_btn.setStyleSheet("""
                QPushButton {
                    background: #0078d4; color: white; border: none;
                    padding: 6px 12px; border-radius: 4px; font-weight: bold;
                }
                QPushButton:hover { background: #1084d8; }
            """)
            btn_style = """
                QPushButton {
                    background: #e0e0e0; color: #333; border: 1px solid #ccc;
                    padding: 4px 8px; border-radius: 3px;
                }
                QPushButton:hover { background: #d0d0d0; }
            """

        # Apply style to all format buttons
        for btn in [self.btn_h1, self.btn_h2, self.btn_h3, self.btn_bold, self.btn_italic,
                    self.btn_code, self.btn_link, self.btn_image, self.btn_ul, self.btn_ol,
                    self.btn_quote, self.btn_table, self.btn_codeblock, self.btn_mermaid]:
            btn.setStyleSheet(btn_style)

        self.highlighter.dark_mode = self.dark_mode
        self.highlighter.setup_formats()
        self.highlighter.rehighlight()

    def set_dark_mode(self, enabled: bool):
        self.dark_mode = enabled
        self.apply_theme()
        self.refresh_view()

    def show_search(self):
        self.search_bar.show_and_focus()

    def load_file(self, file_path: str):
        """Load a markdown file."""
        if self.context.load(file_path):
            self.editor.blockSignals(True)
            self.editor.setPlainText(self.document.content)
            self.editor.blockSignals(False)
            # Update highlighter based on file type
            self._setup_highlighter()
            self.refresh_view()
            if file_path:
                self.preview.setSearchPaths([os.path.dirname(os.path.abspath(file_path))])
            if self.events:
                self.events.document_opened.emit(self.document)
        else:
            QMessageBox.critical(self, "Error", f"Could not open file:\n{file_path}")

    def save_file(self) -> bool:
        """Save the current file."""
        if self.edit_mode:
            self.context.set_content(self.editor.toPlainText())

        if not self.document.file_path:
            return self.save_file_as()

        if self.context.save():
            return True
        else:
            QMessageBox.critical(self, "Error", "Could not save file")
            return False

    def save_file_as(self) -> bool:
        """Save with a new name."""
        file_path, _ = QFileDialog.getSaveFileName(
            self, "Save Markdown File", "",
            "Markdown Files (*.md);;All Files (*)"
        )
        if file_path:
            if self.edit_mode:
                self.context.set_content(self.editor.toPlainText())
            if self.context.save_as(file_path):
                return True
            else:
                QMessageBox.critical(self, "Error", "Could not save file")
        return False

    def reload_file(self):
        """Reload the current file."""
        if self.document.file_path and os.path.exists(self.document.file_path):
            if self.document.modified:
                reply = QMessageBox.question(
                    self, "Unsaved Changes",
                    "Reload and lose unsaved changes?",
                    QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
                )
                if reply != QMessageBox.StandardButton.Yes:
                    return
            self.load_file(self.document.file_path)

    def print_document(self):
        """Print the current document."""
        try:
            from PyQt6.QtWidgets import QComboBox, QGroupBox, QGridLayout

            dialog = QDialog(self)
            dialog.setWindowTitle("Print Settings")
            dialog.setMinimumWidth(400)
            main_layout = QVBoxLayout(dialog)

            # Page setup group
            page_group = QGroupBox("Page Setup")
            page_layout = QFormLayout(page_group)

            page_size = QComboBox()
            page_size.addItems(["Letter (8.5 x 11 in)", "A4 (210 x 297 mm)", "Legal (8.5 x 14 in)"])
            page_layout.addRow("Page Size:", page_size)

            orientation = QComboBox()
            orientation.addItems(["Portrait", "Landscape"])
            page_layout.addRow("Orientation:", orientation)

            main_layout.addWidget(page_group)

            # Margins group
            margin_group = QGroupBox("Margins (mm)")
            margin_layout = QGridLayout(margin_group)

            margin_top = QSpinBox()
            margin_top.setRange(0, 100)
            margin_top.setValue(20)
            margin_layout.addWidget(QLabel("Top:"), 0, 0)
            margin_layout.addWidget(margin_top, 0, 1)

            margin_bottom = QSpinBox()
            margin_bottom.setRange(0, 100)
            margin_bottom.setValue(20)
            margin_layout.addWidget(QLabel("Bottom:"), 0, 2)
            margin_layout.addWidget(margin_bottom, 0, 3)

            margin_left = QSpinBox()
            margin_left.setRange(0, 100)
            margin_left.setValue(25)
            margin_layout.addWidget(QLabel("Left:"), 1, 0)
            margin_layout.addWidget(margin_left, 1, 1)

            margin_right = QSpinBox()
            margin_right.setRange(0, 100)
            margin_right.setValue(25)
            margin_layout.addWidget(QLabel("Right:"), 1, 2)
            margin_layout.addWidget(margin_right, 1, 3)

            main_layout.addWidget(margin_group)

            # Text settings group
            text_group = QGroupBox("Text Settings")
            text_layout = QFormLayout(text_group)

            font_spin = QSpinBox()
            font_spin.setRange(8, 36)
            font_spin.setValue(12)
            font_spin.setSuffix(" pt")
            text_layout.addRow("Font Size:", font_spin)

            line_spacing = QComboBox()
            line_spacing.addItems(["1.0 (Single)", "1.5", "2.0 (Double)"])
            line_spacing.setCurrentIndex(1)
            text_layout.addRow("Line Spacing:", line_spacing)

            main_layout.addWidget(text_group)

            # Color settings group
            color_group = QGroupBox("Color Settings")
            color_layout = QFormLayout(color_group)

            color_mode = QComboBox()
            color_mode.addItems(["Color", "Grayscale", "Black & White"])
            color_mode.setCurrentIndex(0)
            color_layout.addRow("Print Mode:", color_mode)

            main_layout.addWidget(color_group)

            # Layout settings group
            layout_group = QGroupBox("Layout Options")
            layout_form = QFormLayout(layout_group)

            avoid_orphans = QCheckBox("Keep headings with following content")
            avoid_orphans.setChecked(True)
            avoid_orphans.setToolTip("Prevents headings from appearing alone at the bottom of a page")
            layout_form.addRow(avoid_orphans)

            keep_tables = QCheckBox("Keep tables on single page when possible")
            keep_tables.setChecked(True)
            layout_form.addRow(keep_tables)

            keep_code = QCheckBox("Keep code blocks together")
            keep_code.setChecked(True)
            layout_form.addRow(keep_code)

            main_layout.addWidget(layout_group)

            # Buttons
            buttons = QDialogButtonBox(
                QDialogButtonBox.StandardButton.Ok |
                QDialogButtonBox.StandardButton.Cancel
            )
            buttons.accepted.connect(dialog.accept)
            buttons.rejected.connect(dialog.reject)
            main_layout.addWidget(buttons)

            if dialog.exec() != QDialog.DialogCode.Accepted:
                return

            if self.edit_mode:
                self.context.set_content(self.editor.toPlainText())

            # Configure printer
            printer = QPrinter(QPrinter.PrinterMode.HighResolution)
            printer.setResolution(150)

            # Set page size
            page_sizes = [QPageSize.PageSizeId.Letter, QPageSize.PageSizeId.A4, QPageSize.PageSizeId.Legal]
            selected_size = QPageSize(page_sizes[page_size.currentIndex()])

            # Set orientation
            orient = QPageLayout.Orientation.Portrait if orientation.currentIndex() == 0 else QPageLayout.Orientation.Landscape

            # Set margins
            margins = QMarginsF(margin_left.value(), margin_top.value(),
                               margin_right.value(), margin_bottom.value())

            page_layout = QPageLayout(selected_size, orient, margins, QPageLayout.Unit.Millimeter)
            printer.setPageLayout(page_layout)

            print_dialog = QPrintDialog(printer, self)
            if print_dialog.exec() == QPrintDialog.DialogCode.Accepted:
                # Get line spacing
                spacing_values = [1.0, 1.5, 2.0]
                spacing = spacing_values[line_spacing.currentIndex()]

                # Get color settings
                color_idx = color_mode.currentIndex()
                use_black = color_idx == 2  # Black & White
                grayscale = color_idx >= 1  # Grayscale or Black & White

                html = self.renderer.render_for_print(
                    self.document.content,
                    font_spin.value(),
                    use_black,
                    spacing,
                    grayscale=grayscale,
                    avoid_orphan_headings=avoid_orphans.isChecked(),
                )
                doc = QTextDocument()
                doc.setHtml(html)
                doc.setPageSize(printer.pageRect(QPrinter.Unit.Point).size())
                doc.print(printer)
        except Exception as e:
            QMessageBox.critical(self, "Print Error", f"Failed to print:\n{e}")

    def export_pdf(self):
        """Export to PDF."""
        try:
            from PyQt6.QtWidgets import QComboBox, QGroupBox, QGridLayout

            # Show settings dialog FIRST
            dialog = QDialog(self)
            dialog.setWindowTitle("PDF Export Settings")
            dialog.setMinimumWidth(400)
            main_layout = QVBoxLayout(dialog)

            # Page setup group
            page_group = QGroupBox("Page Setup")
            page_layout = QFormLayout(page_group)

            page_size = QComboBox()
            page_size.addItems(["Letter (8.5 x 11 in)", "A4 (210 x 297 mm)", "Legal (8.5 x 14 in)"])
            page_layout.addRow("Page Size:", page_size)

            orientation = QComboBox()
            orientation.addItems(["Portrait", "Landscape"])
            page_layout.addRow("Orientation:", orientation)

            main_layout.addWidget(page_group)

            # Margins group
            margin_group = QGroupBox("Margins (mm)")
            margin_layout = QGridLayout(margin_group)

            margin_top = QSpinBox()
            margin_top.setRange(0, 100)
            margin_top.setValue(20)
            margin_layout.addWidget(QLabel("Top:"), 0, 0)
            margin_layout.addWidget(margin_top, 0, 1)

            margin_bottom = QSpinBox()
            margin_bottom.setRange(0, 100)
            margin_bottom.setValue(20)
            margin_layout.addWidget(QLabel("Bottom:"), 0, 2)
            margin_layout.addWidget(margin_bottom, 0, 3)

            margin_left = QSpinBox()
            margin_left.setRange(0, 100)
            margin_left.setValue(25)
            margin_layout.addWidget(QLabel("Left:"), 1, 0)
            margin_layout.addWidget(margin_left, 1, 1)

            margin_right = QSpinBox()
            margin_right.setRange(0, 100)
            margin_right.setValue(25)
            margin_layout.addWidget(QLabel("Right:"), 1, 2)
            margin_layout.addWidget(margin_right, 1, 3)

            main_layout.addWidget(margin_group)

            # Text settings group
            text_group = QGroupBox("Text Settings")
            text_layout = QFormLayout(text_group)

            font_spin = QSpinBox()
            font_spin.setRange(8, 36)
            font_spin.setValue(12)
            font_spin.setSuffix(" pt")
            text_layout.addRow("Font Size:", font_spin)

            line_spacing = QComboBox()
            line_spacing.addItems(["1.0 (Single)", "1.5", "2.0 (Double)"])
            line_spacing.setCurrentIndex(1)
            text_layout.addRow("Line Spacing:", line_spacing)

            main_layout.addWidget(text_group)

            # Color settings group
            color_group = QGroupBox("Color Settings")
            color_layout = QFormLayout(color_group)

            color_mode = QComboBox()
            color_mode.addItems(["Color", "Grayscale", "Black & White"])
            color_mode.setCurrentIndex(0)
            color_layout.addRow("PDF Mode:", color_mode)

            main_layout.addWidget(color_group)

            # Layout settings group
            layout_group = QGroupBox("Layout Options")
            layout_form = QFormLayout(layout_group)

            avoid_orphans = QCheckBox("Keep headings with following content")
            avoid_orphans.setChecked(True)
            layout_form.addRow(avoid_orphans)

            main_layout.addWidget(layout_group)

            # Buttons
            buttons = QDialogButtonBox(
                QDialogButtonBox.StandardButton.Ok |
                QDialogButtonBox.StandardButton.Cancel
            )
            buttons.accepted.connect(dialog.accept)
            buttons.rejected.connect(dialog.reject)
            main_layout.addWidget(buttons)

            if dialog.exec() != QDialog.DialogCode.Accepted:
                return

            # Now ask for file path
            default_name = ""
            if self.document.file_path:
                default_name = os.path.splitext(self.document.file_path)[0] + ".pdf"

            file_path, _ = QFileDialog.getSaveFileName(
                self, "Export to PDF", default_name, "PDF Files (*.pdf)"
            )
            if not file_path:
                return

            if self.edit_mode:
                self.context.set_content(self.editor.toPlainText())

            # Configure printer for PDF
            printer = QPrinter(QPrinter.PrinterMode.HighResolution)
            printer.setOutputFormat(QPrinter.OutputFormat.PdfFormat)
            printer.setOutputFileName(file_path)
            printer.setResolution(150)

            # Set page size
            page_sizes = [QPageSize.PageSizeId.Letter, QPageSize.PageSizeId.A4, QPageSize.PageSizeId.Legal]
            selected_size = QPageSize(page_sizes[page_size.currentIndex()])

            # Set orientation
            orient = QPageLayout.Orientation.Portrait if orientation.currentIndex() == 0 else QPageLayout.Orientation.Landscape

            # Set margins
            margins = QMarginsF(margin_left.value(), margin_top.value(),
                               margin_right.value(), margin_bottom.value())

            pdf_layout = QPageLayout(selected_size, orient, margins, QPageLayout.Unit.Millimeter)
            printer.setPageLayout(pdf_layout)

            # Get settings
            spacing_values = [1.0, 1.5, 2.0]
            spacing = spacing_values[line_spacing.currentIndex()]

            color_idx = color_mode.currentIndex()
            use_black = color_idx == 2
            grayscale = color_idx >= 1

            html = self.renderer.render_for_print(
                self.document.content,
                font_spin.value(),
                use_black,
                spacing,
                grayscale=grayscale,
                avoid_orphan_headings=avoid_orphans.isChecked(),
            )
            doc = QTextDocument()
            doc.setHtml(html)
            doc.setPageSize(printer.pageRect(QPrinter.Unit.Point).size())
            doc.print(printer)

            QMessageBox.information(self, "Export Complete", f"PDF saved to:\n{file_path}")
        except Exception as e:
            QMessageBox.critical(self, "Export Error", f"Failed to export:\n{e}")

    def keyPressEvent(self, event):
        """Handle keyboard shortcuts."""
        if event.key() == Qt.Key.Key_F2:
            self.toggle_mode()
        else:
            super().keyPressEvent(event)


class MarkdownEditor(QMainWindow):
    """Main window with tabbed markdown editor."""

    def __init__(self):
        super().__init__()
        self.setWindowTitle("Markdown Editor")
        self.setMinimumSize(800, 600)
        self.resize(1000, 700)

        self.dark_mode = False
        self.graphics_enabled = False
        self.workspace_index_scope = "open_docs"
        self.context_menu_items = [
            ("Core", [
                ("quick_stats", "Quick Stats"),
            ]),
            ("Lexicon", [
                ("lookup_definition", "Lookup Definition"),
                ("show_synonyms", "Show Synonyms"),
            ]),
            ("Writing", [
                ("text_suggestions", "Text Suggestions"),
                ("fix_typos", "Fix Typos"),
            ]),
            ("Analysis", [
                ("intent_map", "Intent Map"),
                ("citation_helper", "Citation Helper"),
                ("outline_enhancer", "Outline Enhancer"),
                ("action_extractor", "Action Extractor"),
                ("diff_narrator", "Diff Narrator"),
            ]),
            ("Graphics", [
                ("generate_image", "Generate Image"),
            ]),
            ("Code", [
                ("run_selection", "Run Selection"),
                ("python_eval", "Python: Evaluate"),
                ("run_file", "Run File"),
                ("build_project", "Build Project"),
            ]),
        ]
        self.context_menu_config = {
            "enabled": {"quick_stats", "lookup_definition", "show_synonyms", "run_selection", "python_eval", "run_file", "build_project"},
        }
        self.workspace_root = Path(__file__).resolve().parent
        self.workspace_config = {}
        self.load_settings()

        # Initialize license manager
        self.license = get_license_manager()

        # Initialize kernel (MarlOS core)
        self.kernel = init_kernel()
        self.kernel.spine.event_broadcast.connect(self._on_kernel_event)

        # Initialize visual memory capture (screen recording)
        self.visual_memory = None
        self.visual_memory_running = False

        # Create shared AI client (LM Studio + local fallback)
        lm_studio_endpoint = self.workspace_config.get(
            "lm_studio_endpoint", "http://localhost:1234/v1"
        )
        lm_studio_model = self.workspace_config.get("lm_studio_model", None)
        self.ai_client = HybridAIClient(
            lm_studio_endpoint=lm_studio_endpoint,
            model=lm_studio_model
        )

        self.lexicon = LocalLexicon(
            extra_paths=[
                str(self.workspace_root / path)
                for path in self.workspace_config.get("lexicon_pack_paths", [])
            ]
        )
        self.task_extractor = TaskExtractor()
        self.task_store = TaskIndexStore(self.workspace_root)
        self.timer = QTimer(self)
        self.timer.timeout.connect(self.on_timer_tick)
        self.timer_running = False
        self.timer_remaining = 0
        self.timer_label = ""
        self.file_watcher = QFileSystemWatcher()
        self.file_watcher.fileChanged.connect(self.on_file_changed)
        self.pending_reloads = set()

        self.setAcceptDrops(True)

        # Tab widget with Home tab always present
        self.tabs = QTabWidget()
        self.tabs.setTabsClosable(True)
        self.tabs.tabCloseRequested.connect(self.close_tab)
        self.tabs.setDocumentMode(True)
        self.tabs.currentChanged.connect(self.on_tab_changed)
        self.tabs.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.tabs.customContextMenuRequested.connect(self.show_tab_context_menu)

        # Track floating windows
        self.floating_windows = []

        # Desktop view as permanent Home tab
        self.desktop_view = DesktopView()
        self.desktop_view.file_requested.connect(self.open_file)
        self.desktop_view.new_file_requested.connect(self.new_file)
        self.tabs.addTab(self.desktop_view, "Home")
        # Make Home tab non-closable
        self.tabs.tabBar().setTabButton(0, self.tabs.tabBar().ButtonPosition.RightSide, None)

        self.setCentralWidget(self.tabs)

        self.setup_menu()
        self.setup_side_panels()

        # Connect desktop to workspace
        self.desktop_view.context_launcher = self.context_launcher

        # Load saved workspace
        self.load_workspace()

        # Refresh desktop icons
        self.desktop_view.refresh_icons()

        if len(sys.argv) > 1:
            for path in sys.argv[1:]:
                if os.path.isfile(path):
                    self.open_file(path)
        else:
            # Start with desktop view (not a blank file)
            self.show_desktop()

    def show_desktop(self):
        """Show the desktop/home tab."""
        self.tabs.setCurrentIndex(0)  # Home tab is always index 0
        self.desktop_view.refresh_icons()
        self.setWindowTitle("MarlOS")

    def show_editor(self):
        """Show the most recent document tab (or stay if already on one)."""
        if self.tabs.currentIndex() == 0 and self.tabs.count() > 1:
            self.tabs.setCurrentIndex(1)  # Switch to first document tab

    def setup_menu(self):
        menubar = self.menuBar()

        # File menu
        file_menu = menubar.addMenu("&File")

        new_action = QAction("&New", self)
        new_action.setShortcut(QKeySequence.StandardKey.New)
        new_action.triggered.connect(self.new_file)
        file_menu.addAction(new_action)

        open_action = QAction("&Open...", self)
        open_action.setShortcut(QKeySequence.StandardKey.Open)
        open_action.triggered.connect(self.open_file_dialog)
        file_menu.addAction(open_action)

        file_menu.addSeparator()

        save_action = QAction("&Save", self)
        save_action.setShortcut(QKeySequence.StandardKey.Save)
        save_action.triggered.connect(self.save_current)
        file_menu.addAction(save_action)

        save_as_action = QAction("Save &As...", self)
        save_as_action.setShortcut(QKeySequence("Ctrl+Shift+S"))
        save_as_action.triggered.connect(self.save_current_as)
        file_menu.addAction(save_as_action)

        file_menu.addSeparator()

        print_action = QAction("&Print...", self)
        print_action.setShortcut(QKeySequence.StandardKey.Print)
        print_action.triggered.connect(self.print_current)
        file_menu.addAction(print_action)

        export_action = QAction("Export to &PDF...", self)
        export_action.setShortcut(QKeySequence("Ctrl+Shift+E"))
        export_action.triggered.connect(self.export_pdf_current)
        file_menu.addAction(export_action)

        file_menu.addSeparator()

        # Export/Import semantic memory
        export_memory_action = QAction("Export &Memory...", self)
        export_memory_action.setStatusTip("Export your semantic memory to an encrypted file")
        export_memory_action.triggered.connect(self.export_memory)
        file_menu.addAction(export_memory_action)

        import_memory_action = QAction("&Import Memory...", self)
        import_memory_action.setStatusTip("Import semantic memory from an encrypted file")
        import_memory_action.triggered.connect(self.import_memory)
        file_menu.addAction(import_memory_action)

        file_menu.addSeparator()

        # Export context for cloud AI
        export_context_action = QAction("Export Context for &AI...", self)
        export_context_action.setStatusTip("Export your current context to share with cloud AI models")
        export_context_action.triggered.connect(self.export_context_for_ai)
        file_menu.addAction(export_context_action)

        file_menu.addSeparator()

        close_action = QAction("&Close Tab", self)
        close_action.setShortcut(QKeySequence("Ctrl+W"))
        close_action.triggered.connect(self.close_current_tab)
        file_menu.addAction(close_action)

        file_menu.addSeparator()

        exit_action = QAction("E&xit", self)
        exit_action.setShortcut(QKeySequence.StandardKey.Quit)
        exit_action.triggered.connect(self.close)
        file_menu.addAction(exit_action)

        # Edit menu
        edit_menu = menubar.addMenu("&Edit")

        toggle_action = QAction("Toggle &Edit/View", self)
        toggle_action.setShortcut(QKeySequence("F2"))
        toggle_action.triggered.connect(self.toggle_current_mode)
        edit_menu.addAction(toggle_action)

        edit_menu.addSeparator()

        find_action = QAction("&Find...", self)
        find_action.setShortcut(QKeySequence.StandardKey.Find)
        find_action.triggered.connect(self.show_search)
        edit_menu.addAction(find_action)

        palette_action = QAction("Command &Palette...", self)
        palette_action.setShortcut(QKeySequence("Ctrl+Shift+P"))
        palette_action.triggered.connect(self.show_command_palette)
        edit_menu.addAction(palette_action)

        # View menu
        view_menu = menubar.addMenu("&View")

        self.dark_action = QAction("&Dark Mode", self)
        self.dark_action.setCheckable(True)
        self.dark_action.setShortcut(QKeySequence("Ctrl+D"))
        self.dark_action.triggered.connect(self.toggle_dark_mode)
        view_menu.addAction(self.dark_action)

        view_menu.addSeparator()

        reload_action = QAction("&Reload", self)
        reload_action.setShortcut(QKeySequence.StandardKey.Refresh)
        reload_action.triggered.connect(self.reload_current)
        view_menu.addAction(reload_action)

        view_menu.addSeparator()

        self.tasks_action = QAction("&Tasks Panel", self)
        self.tasks_action.setCheckable(True)
        self.tasks_action.setChecked(False)
        self.tasks_action.triggered.connect(self.toggle_tasks_dock)
        view_menu.addAction(self.tasks_action)

        self.reader_action = QAction("&Reader Mode", self)
        self.reader_action.setCheckable(True)
        self.reader_action.setChecked(False)
        self.reader_action.setShortcut(QKeySequence("Ctrl+R"))
        self.reader_action.triggered.connect(self.toggle_reader_mode)
        view_menu.addAction(self.reader_action)

        self.outline_action = QAction("&Outline", self)
        self.outline_action.setCheckable(True)
        self.outline_action.setChecked(False)
        self.outline_action.triggered.connect(self.toggle_outline_dock)
        view_menu.addAction(self.outline_action)

        self.word_count_action = QAction("&Word Count", self)
        self.word_count_action.setCheckable(True)
        self.word_count_action.setChecked(False)
        self.word_count_action.triggered.connect(self.toggle_word_count_dock)
        view_menu.addAction(self.word_count_action)

        self.chat_action = QAction("&Chat (AI)", self)
        self.chat_action.setCheckable(True)
        self.chat_action.setChecked(False)
        self.chat_action.setShortcut(QKeySequence("Ctrl+Shift+C"))
        self.chat_action.triggered.connect(self.toggle_chat_dock)
        view_menu.addAction(self.chat_action)

        self.console_action = QAction("Con&sole", self)
        self.console_action.setCheckable(True)
        self.console_action.setChecked(False)
        self.console_action.setShortcut(QKeySequence("Ctrl+`"))
        self.console_action.triggered.connect(self.toggle_console_dock)
        view_menu.addAction(self.console_action)

        self.links_action = QAction("&Links", self)
        self.links_action.setCheckable(True)
        self.links_action.setChecked(False)
        self.links_action.setShortcut(QKeySequence("Ctrl+L"))
        self.links_action.triggered.connect(self.toggle_links_dock)
        view_menu.addAction(self.links_action)

        self.semantic_action = QAction("Se&mantic", self)
        self.semantic_action.setCheckable(True)
        self.semantic_action.setChecked(False)
        self.semantic_action.setShortcut(QKeySequence("Ctrl+M"))
        self.semantic_action.triggered.connect(self.toggle_semantic_dock)
        view_menu.addAction(self.semantic_action)

        self.relation_action = QAction("Relation &Graph (Pro)", self)
        self.relation_action.setCheckable(True)
        self.relation_action.setChecked(False)
        self.relation_action.setShortcut(QKeySequence("Ctrl+G"))
        self.relation_action.triggered.connect(self.toggle_relation_dock)
        view_menu.addAction(self.relation_action)

        view_menu.addSeparator()

        # Visual Memory controls
        self.visual_memory_start_action = QAction("&Start Visual Memory", self)
        self.visual_memory_start_action.setToolTip("Start capturing screenshots of your work")
        self.visual_memory_start_action.triggered.connect(self.start_visual_memory)
        view_menu.addAction(self.visual_memory_start_action)

        self.visual_memory_stop_action = QAction("St&op Visual Memory", self)
        self.visual_memory_stop_action.setToolTip("Stop capturing screenshots")
        self.visual_memory_stop_action.setEnabled(False)
        self.visual_memory_stop_action.triggered.connect(self.stop_visual_memory)
        view_menu.addAction(self.visual_memory_stop_action)

        self.visual_memory_browse_action = QAction("&Browse Visual Memory", self)
        self.visual_memory_browse_action.setToolTip("Browse captured screenshots")
        self.visual_memory_browse_action.triggered.connect(self.browse_visual_memory)
        view_menu.addAction(self.visual_memory_browse_action)

        view_menu.addSeparator()

        settings_action = QAction("&Settings...", self)
        settings_action.setShortcut(QKeySequence("Ctrl+,"))
        settings_action.triggered.connect(self.show_settings)
        view_menu.addAction(settings_action)

        # Web menu
        web_menu = menubar.addMenu("&Web")

        new_browser_action = QAction("&New Browser Tab", self)
        new_browser_action.setShortcut(QKeySequence("Ctrl+Shift+B"))
        new_browser_action.triggered.connect(self.new_browser_tab)
        web_menu.addAction(new_browser_action)

        open_url_action = QAction("&Open URL...", self)
        open_url_action.setShortcut(QKeySequence("Ctrl+U"))
        open_url_action.triggered.connect(self.open_url_dialog)
        web_menu.addAction(open_url_action)

        # Terminal menu
        terminal_menu = menubar.addMenu("&Terminal")

        new_terminal_action = QAction("&New Terminal Tab", self)
        new_terminal_action.setShortcut(QKeySequence("Ctrl+Shift+T"))
        new_terminal_action.triggered.connect(self.new_terminal_tab)
        terminal_menu.addAction(new_terminal_action)

        terminal_menu.addSeparator()

        terminal_grid_action = QAction("Terminal &Grid (2x2)", self)
        terminal_grid_action.setShortcut(QKeySequence("Ctrl+Shift+G"))
        terminal_grid_action.triggered.connect(self.new_terminal_grid)
        terminal_menu.addAction(terminal_grid_action)

        terminal_quad_action = QAction("&4 Terminals (Quad)", self)
        terminal_quad_action.triggered.connect(lambda: self.new_terminal_grid(2, 2))
        terminal_menu.addAction(terminal_quad_action)

        # Apps menu
        apps_menu = menubar.addMenu("&Apps")

        new_app_launcher_action = QAction("&App Launcher", self)
        new_app_launcher_action.setShortcut(QKeySequence("Ctrl+Shift+A"))
        new_app_launcher_action.triggered.connect(self.new_app_launcher_tab)
        apps_menu.addAction(new_app_launcher_action)

        # Settings menu
        settings_menu = menubar.addMenu("&Settings")

        settings_action = QAction("&Preferences...", self)
        settings_action.setShortcut(QKeySequence("Ctrl+,"))
        settings_action.triggered.connect(self.show_settings)
        settings_menu.addAction(settings_action)

        # License menu
        license_menu = menubar.addMenu("&License")

        license_info_action = QAction("&View License...", self)
        license_info_action.triggered.connect(self.show_license_info)
        license_menu.addAction(license_info_action)

        license_activate_action = QAction("&Activate License...", self)
        license_activate_action.triggered.connect(self.show_activate_license_dialog)
        license_menu.addAction(license_activate_action)

        # Window menu
        window_menu = menubar.addMenu("&Window")

        undock_action = QAction("&Open Current Tab in New Window", self)
        undock_action.setShortcut(QKeySequence("Ctrl+Shift+U"))
        undock_action.triggered.connect(self.undock_current_tab)
        window_menu.addAction(undock_action)

        window_menu.addSeparator()

        grid_workspace_action = QAction("New &Grid Workspace", self)
        grid_workspace_action.setShortcut(QKeySequence("Ctrl+Shift+G"))
        grid_workspace_action.triggered.connect(self.new_grid_workspace)
        window_menu.addAction(grid_workspace_action)

        grid_workspace_3x3_action = QAction("New 3x3 Grid", self)
        grid_workspace_3x3_action.triggered.connect(lambda: self.new_grid_workspace(3, 3))
        window_menu.addAction(grid_workspace_3x3_action)

    def setup_side_panels(self):
        # Consistent cream/warm theme for all side panels
        dock_style = """
            QDockWidget {
                background-color: #faf8f5;
                color: #3d3929;
                titlebar-close-icon: url(close.png);
            }
            QDockWidget::title {
                background-color: #ebe7df;
                color: #3d3929;
                padding: 6px;
                font-weight: bold;
            }
        """
        panel_style = """
            QWidget {
                background-color: #faf8f5;
                color: #3d3929;
            }
            QLabel {
                color: #3d3929;
            }
            QLineEdit {
                background-color: #ffffff;
                color: #3d3929;
                border: 1px solid #d5d0c4;
                padding: 4px;
                border-radius: 3px;
            }
            QPushButton {
                background-color: #ebe7df;
                color: #3d3929;
                border: 1px solid #d5d0c4;
                padding: 4px 8px;
                border-radius: 3px;
            }
            QPushButton:hover {
                background-color: #e0dbd1;
            }
            QPushButton:checked {
                background-color: #d5c9a8;
            }
            QComboBox {
                background-color: #ffffff;
                color: #3d3929;
                border: 1px solid #d5d0c4;
                padding: 4px;
                border-radius: 3px;
            }
            QListWidget {
                background-color: #ffffff;
                color: #3d3929;
                border: 1px solid #d5d0c4;
                border-radius: 4px;
            }
            QListWidget::item {
                padding: 6px;
                border-bottom: 1px solid #ebe7df;
                color: #3d3929;
            }
            QListWidget::item:hover {
                background: #f5f3ef;
            }
            QListWidget::item:selected {
                background: #e8d5b5;
                color: #3d3929;
            }
        """

        # Context Launcher - the "desktop" of the MarlOS
        self.workspace_dock = QDockWidget("Workspace", self)
        self.workspace_dock.setStyleSheet(dock_style)
        self.context_launcher = ContextLauncher()
        self.context_launcher.context_selected.connect(self._on_context_selected)
        self.context_launcher.context_added.connect(self._on_context_added)
        self.workspace_dock.setWidget(self.context_launcher)
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.workspace_dock)
        self.workspace_dock.setMinimumWidth(200)

        self.outline_dock = QDockWidget("Outline", self)
        self.outline_dock.setStyleSheet(dock_style)
        self.outline_list = QListWidget()
        self.outline_list.setStyleSheet(panel_style)
        self.outline_list.itemActivated.connect(self.jump_to_heading)
        self.outline_dock.setWidget(self.outline_list)
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.outline_dock)
        self.outline_dock.setVisible(False)

        self.word_count_dock = QDockWidget("Word Count", self)
        self.word_count_dock.setStyleSheet(dock_style)
        word_count_widget = QWidget()
        word_count_widget.setStyleSheet(panel_style)
        word_count_layout = QVBoxLayout(word_count_widget)
        word_count_layout.setContentsMargins(12, 8, 12, 8)
        self.word_count_label = QLabel("Words: 0  Characters: 0")
        word_count_layout.addWidget(self.word_count_label)
        self.word_count_dock.setWidget(word_count_widget)
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.word_count_dock)
        self.word_count_dock.setVisible(False)

        self.tasks_dock = QDockWidget("Tasks", self)
        self.tasks_dock.setStyleSheet(dock_style)
        tasks_widget = QWidget()
        tasks_widget.setStyleSheet(panel_style)
        tasks_layout = QVBoxLayout(tasks_widget)
        tasks_layout.setContentsMargins(8, 8, 8, 8)

        self.tasks_search = QLineEdit()
        self.tasks_search.setPlaceholderText("Search tasks...")
        self.tasks_search.textChanged.connect(self.refresh_tasks_panel)
        tasks_layout.addWidget(self.tasks_search)

        pill_row = QHBoxLayout()
        self.pill_all = QPushButton("All")
        self.pill_open = QPushButton("Open")
        self.pill_done = QPushButton("Done")
        for pill in (self.pill_all, self.pill_open, self.pill_done):
            pill.setCheckable(True)
            pill_row.addWidget(pill)
        self.pill_all.setChecked(True)
        self.pill_all.clicked.connect(lambda: self.set_tasks_status_filter("All"))
        self.pill_open.clicked.connect(lambda: self.set_tasks_status_filter("Open"))
        self.pill_done.clicked.connect(lambda: self.set_tasks_status_filter("Done"))
        tasks_layout.addLayout(pill_row)

        tags_row = QHBoxLayout()
        self.tasks_tag_filter = QLineEdit()
        self.tasks_tag_filter.setPlaceholderText("Filter by tag (e.g. @client)")
        self.tasks_tag_filter.textChanged.connect(self.refresh_tasks_panel)
        tags_row.addWidget(self.tasks_tag_filter)
        tasks_layout.addLayout(tags_row)

        filter_row = QHBoxLayout()
        self.tasks_status_filter = QComboBox()
        self.tasks_status_filter.addItems(["All", "Open", "Done"])
        self.tasks_status_filter.currentIndexChanged.connect(self.refresh_tasks_panel)
        filter_row.addWidget(self.tasks_status_filter)

        self.tasks_group_filter = QComboBox()
        self.tasks_group_filter.addItems(["None", "File", "Due", "Tag"])
        self.tasks_group_filter.currentIndexChanged.connect(self.refresh_tasks_panel)
        filter_row.addWidget(self.tasks_group_filter)
        tasks_layout.addLayout(filter_row)

        self.tasks_list = QListWidget()
        self.tasks_list.itemActivated.connect(self.open_task_from_list)
        tasks_layout.addWidget(self.tasks_list)

        self.tasks_dock.setWidget(tasks_widget)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self.tasks_dock)
        self.tasks_dock.setVisible(False)

        # Task badge button in status bar - subtle indicator for actionable items
        self.task_badge_button = TaskBadgeButton("Tasks", self)
        self.task_badge_button.setToolTip("Open Tasks Panel")
        self.task_badge_button.clicked.connect(self.on_task_badge_clicked)
        self.statusBar().addPermanentWidget(self.task_badge_button)

        # Timer to periodically check for actionable tasks (every 60 seconds)
        self.task_badge_timer = QTimer(self)
        self.task_badge_timer.timeout.connect(self.update_task_badge)
        self.task_badge_timer.start(60000)  # Check every minute

        # Initial badge update
        QTimer.singleShot(1000, self.update_task_badge)

        # Chat panel dock - LLM conversation interface
        self.chat_dock = QDockWidget("Chat (AI)", self)
        self.chat_dock.setStyleSheet(dock_style)
        self.chat_panel = ChatPanel(
            self.ai_client,
            get_context_callback=self._get_chat_context,
            execute_provider_callback=self._execute_chat_provider,
            get_spine_callback=self._get_chat_spine,
            get_kernel_callback=self.get_kernel_state,
            get_manifest_callback=self._get_current_manifest
        )
        self.chat_dock.setWidget(self.chat_panel)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self.chat_dock)
        self.chat_dock.setVisible(False)
        self.chat_dock.setMinimumWidth(350)

        # Console panel for compiler output
        self.console_dock = QDockWidget("Console", self)
        self.console_dock.setObjectName("console_dock")
        self.console_panel = ConsolePanel(dark_mode=self.dark_mode)
        self.console_dock.setWidget(self.console_panel)
        self.addDockWidget(Qt.DockWidgetArea.BottomDockWidgetArea, self.console_dock)
        self.console_dock.setVisible(False)
        self.console_dock.setMinimumHeight(150)

        # Link Navigator panel for semantic relationships
        self.links_dock = QDockWidget("Links", self)
        self.links_dock.setStyleSheet(dock_style)
        self.links_panel = LinkNavigatorPanel()
        self.links_panel.link_clicked.connect(self._on_link_navigate)
        self.links_dock.setWidget(self.links_panel)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self.links_dock)
        self.links_dock.setVisible(False)
        self.links_dock.setMinimumWidth(250)

        # Semantic panel - related files, recent, tags, search
        self.semantic_dock = QDockWidget("Semantic", self)
        self.semantic_dock.setStyleSheet(dock_style)
        self.semantic_panel = SemanticPanel()
        self.semantic_panel.setStyleSheet(panel_style)
        self.semantic_panel.file_requested.connect(self.open_file)
        self.semantic_dock.setWidget(self.semantic_panel)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self.semantic_dock)
        self.semantic_dock.setVisible(False)
        self.semantic_dock.setMinimumWidth(250)

        # Connect semantic panel to kernel
        self.semantic_panel.set_kernel(self.kernel)

        # Relation Graph panel (Pro feature) - visualize file relationships
        self.relation_dock = create_relation_explorer_dock(self.kernel, self)
        self.relation_dock.setStyleSheet(dock_style)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self.relation_dock)
        self.relation_dock.setVisible(False)
        self.relation_dock.setMinimumWidth(400)

        # Connect context layer signals (if PyQt is available)
        if hasattr(self.kernel, 'focus_changed'):
            self.kernel.focus_changed.connect(self._on_focus_changed)

        # Timer to periodically update focus display (every 30 seconds)
        self.focus_update_timer = QTimer(self)
        self.focus_update_timer.timeout.connect(self._update_focus_display)
        self.focus_update_timer.start(30000)

    def _on_link_navigate(self, path):
        """Handle navigation to a linked document."""
        if os.path.exists(path):
            self.open_file(path)
        else:
            QMessageBox.warning(self, "Link Target Not Found",
                              f"Could not find: {path}")

    def _get_chat_context(self):
        """Get current selection and document for chat context."""
        tab = self.tabs.currentWidget()
        selection = ""
        document = ""
        if isinstance(tab, MarkdownTab):
            if tab.editor and tab.editor.textCursor().hasSelection():
                selection = tab.editor.textCursor().selectedText()
            document = tab.document.content
        return selection, document

    def _get_chat_spine(self):
        """Get the spine from the current tab for history viewing."""
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            return tab.events
        return None

    def _get_current_manifest(self):
        """Get the manifest from the current tab's kernel context."""
        tab = self.tabs.currentWidget()
        if not tab or not hasattr(tab, 'kernel_ctx'):
            return None

        ctx = tab.kernel_ctx
        return {
            "path": ctx.path,
            "manifest": ctx.manifest,
            "ai_access": ctx.ai_access,
        }

    def _on_focus_changed(self, doc_path: str):
        """Handle focus changed signal from context layer."""
        # Update the semantic panel focus display
        if self.semantic_dock.isVisible():
            self.semantic_panel.update_focus()

    def _update_focus_display(self):
        """Periodically update focus display in semantic panel."""
        if self.semantic_dock.isVisible():
            self.semantic_panel.update_focus()

    def _on_context_selected(self, path):
        """Handle context selection from the workspace launcher."""
        # Check if already open in a tab
        for i in range(self.tabs.count()):
            tab = self.tabs.widget(i)
            if isinstance(tab, MarkdownTab) and tab.document.file_path == path:
                self.tabs.setCurrentIndex(i)
                return
        # Open the file
        self.open_file(path)

    def _on_context_added(self, path):
        """Handle new context added to workspace."""
        # Open it immediately
        self.open_file(path)

    def _update_workspace_status(self, path, status, context_id=None):
        """Update a context's status in the workspace."""
        if hasattr(self, 'context_launcher'):
            self.context_launcher.update_context_status(path, status, context_id)

    def _ingest_python_file(self, kernel, file_path, content, chunk_size):
        """Ingest a Python file with hierarchical structure parsing."""
        import ast
        import os

        try:
            # Parse the Python file
            tree = ast.parse(content)

            layers = {
                "file": [],
                "class": [],
                "function": [],
                "code": []
            }

            # File-level summary
            file_info = f"File: {file_path}\n"

            # Count components
            classes = [node for node in tree.body if isinstance(node, ast.ClassDef)]
            functions = [node for node in tree.body if isinstance(node, ast.FunctionDef)]
            imports = [node for node in tree.body if isinstance(node, (ast.Import, ast.ImportFrom))]

            file_info += f"Classes: {len(classes)}\n"
            file_info += f"Functions: {len(functions)}\n"
            file_info += f"Imports: {len(imports)}\n"

            # Store file summary
            kernel.memory.store(
                content=file_info,
                type="file_summary",
                metadata={
                    "semantic_layer": "file",
                    "file_path": file_path,
                    "classes": [c.name for c in classes],
                    "functions": [f.name for f in functions],
                    "timestamp": time.time()
                }
            )

            # Process each class
            for cls in classes:
                class_code = ast.get_source_segment(content, cls)
                if class_code:
                    # Store full class code
                    kernel.memory.store(
                        content=class_code,
                        type="class_code",
                        metadata={
                            "semantic_layer": "class",
                            "file_path": file_path,
                            "class_name": cls.name,
                            "methods": [m.name for m in cls.body if isinstance(m, ast.FunctionDef)],
                            "timestamp": time.time()
                        }
                    )

            # Process top-level functions
            for func in functions:
                func_code = ast.get_source_segment(content, func)
                if func_code:
                    kernel.memory.store(
                        content=func_code,
                        type="function_code",
                        metadata={
                            "semantic_layer": "function",
                            "file_path": file_path,
                            "function_name": func.name,
                            "timestamp": time.time()
                        }
                    )

            # Also store chunks for detailed code lookup
            chunks = []
            for i in range(0, len(content), chunk_size):
                chunk = content[i:i+chunk_size]
                chunks.append(chunk)
                kernel.memory.store(
                    content=chunk,
                    type="code_chunk",
                    metadata={
                        "semantic_layer": "code",
                        "file_path": file_path,
                        "chunk_index": len(chunks) - 1,
                        "timestamp": time.time()
                    }
                )

            return {
                "operation": "ingest",
                "file_path": file_path,
                "message": f"✓ Ingested {file_path}\n📊 {len(classes)} classes, {len(functions)} functions\n📦 {len(chunks)} code chunks stored"
            }

        except SyntaxError:
            # If parsing fails, fall back to generic ingestion
            return self._ingest_generic_file(kernel, file_path, content, chunk_size)
        except Exception as e:
            return f"Error parsing Python file: {e}"

    def _ingest_generic_file(self, kernel, file_path, content, chunk_size):
        """Ingest a non-Python file with simple chunking."""
        import os

        chunks = []
        for i in range(0, len(content), chunk_size):
            chunk = content[i:i+chunk_size]
            chunks.append(chunk)
            kernel.memory.store(
                content=chunk,
                type="file_chunk",
                metadata={
                    "semantic_layer": "code",
                    "file_path": file_path,
                    "chunk_index": len(chunks) - 1,
                    "timestamp": time.time()
                }
            )

        # Store summary
        kernel.memory.store(
            content=f"File: {file_path}\nSize: {len(content)} bytes\nChunks: {len(chunks)}",
            type="file_summary",
            metadata={
                "semantic_layer": "file",
                "file_path": file_path,
                "timestamp": time.time()
            }
        )

        return {
            "operation": "ingest",
            "file_path": file_path,
            "message": f"✓ Ingested {file_path}\n📊 {len(content)} bytes\n📦 {len(chunks)} chunks"
        }

    def _execute_chat_provider(self, provider_name, args=""):
        """Execute a provider from the chat panel and return the result."""
        # Handle file operations directly
        if provider_name == "file_read":
            if not args:
                return "Usage: /read <file_path>"
            try:
                with open(args, 'r', encoding='utf-8') as f:
                    content = f.read()
                return {
                    "operation": "read",
                    "file_path": args,
                    "old_content": "",
                    "new_content": content,
                    "message": f"Read {len(content)} bytes from {args}"
                }
            except Exception as e:
                return f"Error reading file: {e}"

        elif provider_name == "file_write":
            # Format: /write <file_path> <content>
            parts = args.split(" ", 1)
            if len(parts) < 2:
                return "Usage: /write <file_path> <content>"

            file_path, content = parts[0], parts[1]
            try:
                import os
                old_content = ""
                if os.path.exists(file_path):
                    with open(file_path, 'r', encoding='utf-8') as f:
                        old_content = f.read()

                operation = "create" if not old_content else "write"
                return {
                    "operation": operation,
                    "file_path": file_path,
                    "old_content": old_content,
                    "new_content": content,
                    "message": f"{'Creating' if operation == 'create' else 'Modifying'} {file_path}"
                }
            except Exception as e:
                return f"Error preparing file write: {e}"

        elif provider_name == "file_list":
            import os
            import glob
            directory = args if args else os.path.dirname(self.tabs.currentWidget().file_path) if hasattr(self.tabs.currentWidget(), 'file_path') else "."
            try:
                files = glob.glob(os.path.join(directory, "*"))
                files = [f for f in files if os.path.isfile(f)]
                if not files:
                    return f"No files found in {directory}"

                result = f"Files in {directory}:\n\n"
                for f in sorted(files)[:20]:  # Limit to 20 files
                    size = os.path.getsize(f)
                    result += f"  {os.path.basename(f)} ({size} bytes)\n"
                if len(files) > 20:
                    result += f"\n... and {len(files) - 20} more files"
                return result
            except Exception as e:
                return f"Error listing files: {e}"

        elif provider_name == "file_ingest":
            # Ingest a large file into semantic memory
            if not args:
                return "Usage: /ingest <file_path> [chunk_size]\nExample: /ingest viewer.py 50000"

            parts = args.split()
            file_path = parts[0]
            chunk_size = int(parts[1]) if len(parts) > 1 else 50000  # Default 50KB chunks

            try:
                import os
                if not os.path.exists(file_path):
                    return f"Error: File not found: {file_path}"

                file_size = os.path.getsize(file_path)
                with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                    content = f.read()

                # Store in semantic kernel with hierarchical structure
                tab = self.tabs.currentWidget()
                if hasattr(tab, 'kernel') and tab.kernel:
                    # Check if it's a Python file for structured parsing
                    if file_path.endswith('.py'):
                        result = self._ingest_python_file(tab.kernel, file_path, content, chunk_size)
                    else:
                        result = self._ingest_generic_file(tab.kernel, file_path, content, chunk_size)

                    return result
                else:
                    # No kernel available, just report the file stats
                    return {
                        "operation": "read",
                        "file_path": file_path,
                        "old_content": "",
                        "new_content": content[:1000] + "\n... (truncated for display)" if len(content) > 1000 else content,
                        "message": f"Read {file_size} bytes from {file_path}\n({len(content.splitlines())} lines)"
                    }

            except Exception as e:
                return f"Error ingesting file: {e}"

        elif provider_name == "file_query":
            # Query the semantic memory for relevant context
            if not args:
                return "Usage: /query <search_query>\nExample: /query what does the TerminalTab class do?"

            query = args
            tab = self.tabs.currentWidget()

            if not hasattr(tab, 'kernel') or not tab.kernel:
                return "No semantic kernel available. Ingest files first with /ingest."

            try:
                # Search for relevant chunks
                results = tab.kernel.memory.search(query, limit=5)

                if not results:
                    return f"No relevant information found for: {query}\n\nTry ingesting files first with /ingest <file_path>"

                # Format results
                response = f"🔍 Found {len(results)} relevant result(s) for: {query}\n\n"
                for i, result in enumerate(results, 1):
                    metadata = result.get('metadata', {})
                    file_path = metadata.get('file_path', 'Unknown')
                    chunk_idx = metadata.get('chunk_index', '?')

                    response += f"--- Result {i} (from {file_path}, chunk {chunk_idx}) ---\n"
                    response += result.get('content', '')[:500]  # Show first 500 chars
                    if len(result.get('content', '')) > 500:
                        response += "\n... (truncated)"
                    response += "\n\n"

                return response

            except Exception as e:
                return f"Error querying semantic memory: {e}"

        # Handle regular AI providers
        tab = self.tabs.currentWidget()
        if not isinstance(tab, MarkdownTab):
            return "No document open. Please open a markdown file first."

        # Get content - prefer selection, fall back to full document
        selection = ""
        if tab.editor and tab.editor.textCursor().hasSelection():
            selection = tab.editor.textCursor().selectedText()
        content = selection if selection else tab.document.content

        if not content.strip():
            return "No content to analyze. The document is empty."

        # Map provider names to AI task names
        task_map = {
            "intent_map": "intent_map",
            "citation_helper": "citation_helper",
            "diff_narrator": "diff_narrator",
            "outline_enhancer": "outline_enhancer",
            "action_extractor": "action_extractor",
            "typo_fixer": "typo_fixer",
            "suggestions": "suggestions",
            "code_complete": "code_complete",
            "code_explain": "code_explain",
            "code_edit": "code_edit",
            "code_generate": "code_generate",
        }

        task = task_map.get(provider_name)
        if not task:
            return f"Unknown provider: {provider_name}"

        # Execute the AI task
        try:
            result = self.ai_client.generate(task, content)
            return result
        except Exception as e:
            return f"Error running {provider_name}: {str(e)}"

    def toggle_chat_dock(self):
        """Toggle the chat panel visibility."""
        visible = self.chat_action.isChecked()
        self.chat_dock.setVisible(visible)
        if visible:
            self.chat_panel.input_field.setFocus()

    def toggle_console_dock(self):
        """Toggle the console panel visibility."""
        visible = self.console_action.isChecked()
        self.console_dock.setVisible(visible)

    def show_console(self):
        """Show the console panel."""
        self.console_dock.setVisible(True)
        self.console_action.setChecked(True)

    def write_to_console(self, text, category="info"):
        """Write text to the console panel."""
        self.show_console()
        self.console_panel.write(text, category)

    def _write_to_console(self, text, category="info"):
        """Callback for providers to write to console."""
        self.show_console()
        self.console_panel.write(text, category)

    def on_task_badge_clicked(self):
        """Handle task badge button click - open the tasks panel."""
        self.tasks_dock.setVisible(True)
        self.tasks_action.setChecked(True)
        self.refresh_tasks_panel()

    def update_task_badge(self):
        """Check for actionable items and update the badge indicator.

        Shows the red dot if there are:
        - Open tasks with due dates that are today or overdue
        - Scheduled reminders that are due or overdue

        This follows the quiet-by-default philosophy - just a subtle
        indicator, no counts, no popups.
        """
        from datetime import date

        has_actionable = False
        today = date.today().isoformat()

        # Check for tasks with due dates today or earlier
        try:
            tasks = self.task_store.list_tasks(status="open")
            for task in tasks:
                due = task.get("due")
                if due and due <= today:
                    has_actionable = True
                    break
        except Exception:
            pass

        # Check for scheduled reminders that are due
        if not has_actionable:
            try:
                from datetime import datetime
                now = datetime.now().isoformat()
                reminders = self.task_store.list_reminders(status="scheduled")
                for reminder in reminders:
                    scheduled = reminder.get("scheduled_for", "")
                    if scheduled and scheduled <= now:
                        has_actionable = True
                        break
            except Exception:
                pass

        self.task_badge_button.set_badge_visible(has_actionable)

    def _settings_path(self):
        return self.workspace_root / "settings.json"

    def _workspace_path(self):
        return self.workspace_root / "workspace.json"

    def load_settings(self):
        workspace_path = self._workspace_path()
        legacy_path = self._settings_path()
        data = {}

        if workspace_path.exists():
            try:
                with workspace_path.open("r", encoding="utf-8") as handle:
                    data = json.load(handle)
            except Exception:
                data = {}
        elif legacy_path.exists():
            try:
                with legacy_path.open("r", encoding="utf-8") as handle:
                    data = json.load(handle)
            except Exception:
                data = {}

        if not data:
            self.workspace_config = self._default_workspace_config()
            return

        try:
            self.graphics_enabled = bool(
                data.get("graphics_enabled", self.graphics_enabled)
            )
            self.workspace_index_scope = data.get(
                "task_index_scope", self.workspace_index_scope
            )
            enabled = data.get("context_menu_enabled", None)
            if isinstance(enabled, list):
                self.context_menu_config["enabled"] = set(enabled)
            self.workspace_config = {**self._default_workspace_config(), **data}
        except Exception:
            return

    def save_settings(self):
        path = self._workspace_path()
        data = {**self._default_workspace_config(), **self.workspace_config}
        data.update(
            {
            "graphics_enabled": self.graphics_enabled,
                "task_index_scope": self.workspace_index_scope,
            "context_menu_enabled": sorted(self.context_menu_config.get("enabled", set())),
            }
        )
        try:
            with path.open("w", encoding="utf-8") as handle:
                json.dump(data, handle, indent=2)
        except Exception:
            return

    def export_memory(self):
        """Export semantic memory to encrypted file for backup/transfer."""
        from PyQt6.QtWidgets import QInputDialog, QLineEdit

        # Let user choose where to save
        file_path, _ = QFileDialog.getSaveFileName(
            self,
            "Export Semantic Memory",
            str(Path.home() / f"marlos-memory-{datetime.now():%Y%m%d-%H%M%S}.marlos"),
            "MarlOS Memory (*.marlos)"
        )

        if not file_path:
            return

        # Get password from user
        password, ok = QInputDialog.getText(
            self, "Encrypt Memory",
            "Enter password to encrypt your memory:\n\n"
            "This password will be required to import this file.\n"
            "Keep it safe! There's no way to recover it if forgotten.",
            QLineEdit.Password
        )

        if not ok or not password:
            return

        try:
            # Export from kernel
            memory_data = self.kernel.memory.export_all()

            # Prepare export package
            export_data = {
                "version": "1.0",
                "export_date": datetime.now().isoformat(),
                "device": {
                    "system": platform.system(),
                    "node": platform.node(),
                    "release": platform.release(),
                    "version": platform.version(),
                    "machine": platform.machine(),
                },
                "memory": memory_data,
                "preferences": self.workspace_config,
            }

            # Serialize to JSON
            json_data = json.dumps(export_data, indent=2)

            # Generate encryption key from password
            kdf = PBKDF2HMAC(
                algorithm=hashes.SHA256(),
                length=32,
                salt=b'MarlOS Memory Export',  # Fixed salt for reproducibility
                iterations=100000,
            )
            key = base64.urlsafe_b64encode(kdf.derive(password.encode()))
            f = Fernet(key)

            # Encrypt data
            encrypted_data = f.encrypt(json_data.encode())

            # Write to file
            with open(file_path, 'wb') as f:
                f.write(b'MarlOS encrypted memory v1\n')
                f.write(encrypted_data)

            QMessageBox.information(
                self, "Export Complete",
                f"Memory exported successfully!\n\n"
                f"Location: {file_path}\n\n"
                f"This file contains:\n"
                f"  • {memory_data['stats']['total_entries']} memory entries\n"
                f"  • {memory_data['stats'].get('total_relations', 0)} relations\n"
                f"  • Your workspace preferences\n\n"
                f"Keep this file safe and don't lose your password!"
            )

        except Exception as e:
            QMessageBox.critical(
                self, "Export Failed",
                f"Failed to export memory:\n{str(e)}"
            )

    def import_memory(self):
        """Import semantic memory from encrypted file."""
        from PyQt6.QtWidgets import QInputDialog, QLineEdit

        file_path, _ = QFileDialog.getOpenFileName(
            self,
            "Import Semantic Memory",
            str(Path.home()),
            "MarlOS Memory (*.marlos)"
        )

        if not file_path:
            return

        # Get password
        password, ok = QInputDialog.getText(
            self, "Decrypt Memory",
            "Enter password for this memory file:",
            QLineEdit.Password
        )

        if not ok or not password:
            return

        try:
            # Read file
            with open(file_path, 'rb') as f:
                header = f.readline().decode().strip()
                if header != 'MarlOS encrypted memory v1':
                    raise ValueError("Invalid file format or version")

                encrypted_data = f.read()

            # Derive decryption key
            kdf = PBKDF2(
                algorithm=hashes.SHA256(),
                length=32,
                salt=b'MarlOS Memory Export',
                iterations=100000,
            )
            key = base64.urlsafe_b64encode(kdf.derive(password.encode()))
            fernet = Fernet(key)

            # Decrypt data
            decrypted_data = fernet.decrypt(encrypted_data)
            export_data = json.loads(decrypted_data.decode())

            # Validate format
            if "version" not in export_data or "memory" not in export_data:
                raise ValueError("Invalid export format")

            # Show import confirmation
            source_device = export_data.get("device", {}).get("system", "Unknown")
            export_date = export_data.get("export_date", "Unknown")
            memory_stats = export_data["memory"].get("stats", {})

            reply = QMessageBox.question(
                self, "Confirm Import",
                f"Import memory from:\n"
                f"  Device: {source_device}\n"
                f"  Date: {export_date}\n"
                f"  Entries: {memory_stats.get('total_entries', 0)}\n"
                f"  Relations: {memory_stats.get('total_relations', 0)}\n\n"
                f"Options:\n"
                f"• Yes: Merge with current memory (keep existing)\n"
                f"• No: Replace current memory (clear existing)\n\n"
                f"Note: Work patterns may be different on this device.",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
            )

            merge = (reply == QMessageBox.StandardButton.Yes)

            # Import into memory
            result = self.kernel.memory.import_data(
                export_data["memory"],
                merge=merge
            )

            # Optionally import preferences
            if "preferences" in export_data:
                pref_reply = QMessageBox.question(
                    self, "Import Preferences",
                    "Would you like to import workspace preferences too?\n\n"
                    "(This will update your settings with values from the imported file)",
                    QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
                )

                if pref_reply == QMessageBox.StandardButton.Yes:
                    # Merge preferences
                    self.workspace_config.update(export_data["preferences"])
                    self.save_settings()

            QMessageBox.information(
                self, "Import Complete",
                f"Memory imported successfully!\n\n"
                f"Results:\n"
                f"  • Entries imported: {result['entries_imported']}\n"
                f"  • Relations imported: {result['relations_imported']}\n"
                f"  • Entries skipped: {result['entries_skipped']}\n\n"
                f"Your semantic memory has been updated."
            )

            # Refresh UI
            self.desktop_view.refresh_icons()

        except Exception as e:
            QMessageBox.critical(
                self, "Import Failed",
                f"Failed to import memory:\n\n{str(e)}\n\n"
                f"Common causes:\n"
                f"• Wrong password\n"
                f"• Corrupted file\n"
                f"• Incompatible file format"
            )

    def check_pro_feature(self, feature: str) -> bool:
        """Check if user has access to a Pro feature.

        Returns True if feature is available, False if upgrade needed.
        """
        if self.license.has_feature(feature):
            return True

        # Show upgrade dialog
        self.show_upgrade_dialog(feature)
        return False

    def show_upgrade_dialog(self, feature: str):
        """Show upgrade to Pro dialog."""
        from PyQt6.QtWidgets import QDialog, QVBoxLayout, QLabel, QPushButton

        dialog = QDialog(self)
        dialog.setWindowTitle("Upgrade to MarlOS Pro")
        dialog.setMinimumWidth(500)

        layout = QVBoxLayout()

        # Title
        title = QLabel("🚀 Unlock Full Power")
        title.setStyleSheet("font-size: 18px; font-weight: bold;")
        layout.addWidget(title)

        layout.addSpacing(10)

        # Message
        message = QLabel(self.license.get_upgrade_message(feature))
        message.setWordWrap(True)
        layout.addWidget(message)

        layout.addSpacing(20)

        # Feature list
        features = QLabel(
            "<b>Pro Features:</b><br>"
            "• Full semantic memory with embeddings<br>"
            "• Screen memory capture (visual history)<br>"
            "• Workflow analytics & insights<br>"
            "• Document focus tracking<br>"
            "• File relationship graph<br>"
            "• Advanced semantic search<br>"
            "• Priority support<br><br>"
            "<b>One-time purchase: $49</b>"
        )
        features.setWordWrap(True)
        layout.addWidget(features)

        layout.addSpacing(20)

        # Buttons
        button_layout = QHBoxLayout()

        buy_button = QPushButton("Buy Pro License")
        buy_button.setMinimumHeight(40)
        buy_button.setStyleSheet("""
            QPushButton {
                background-color: #4CAF50;
                color: white;
                font-weight: bold;
                font-size: 14px;
                border-radius: 5px;
                padding: 10px;
            }
            QPushButton:hover {
                background-color: #45a049;
            }
        """)
        buy_button.clicked.connect(lambda: self.open_license_purchase())
        button_layout.addWidget(buy_button)

        activate_button = QPushButton("Enter License Key")
        activate_button.setMinimumHeight(40)
        activate_button.clicked.connect(lambda: self.show_activate_license_dialog())
        button_layout.addWidget(activate_button)

        layout.addLayout(button_layout)

        # Close button
        close_button = QPushButton("Continue with Free")
        close_button.clicked.connect(dialog.accept)
        layout.addWidget(close_button)

        dialog.setLayout(layout)
        dialog.exec()

    def open_license_purchase(self):
        """Open browser to purchase license."""
        from PyQt6.QtGui import QDesktopServices
        from PyQt6.QtCore import QUrl

        # TODO: Replace with actual purchase URL
        QDesktopServices.openUrl(QUrl("https://marlos.io/purchase"))

    def show_activate_license_dialog(self):
        """Show license activation dialog."""
        from PyQt6.QtWidgets import QDialog, QVBoxLayout, QLabel, QLineEdit, QPushButton, QFormLayout

        dialog = QDialog(self)
        dialog.setWindowTitle("Activate MarlOS Pro")
        dialog.setMinimumWidth(400)

        layout = QVBoxLayout()

        # Title
        title = QLabel("Enter Your License Key")
        title.setStyleSheet("font-size: 16px; font-weight: bold;")
        layout.addWidget(title)

        layout.addSpacing(10)

        # Instructions
        instructions = QLabel(
            "Enter your license key to activate MarlOS Pro.\n\n"
            "Your license key looks like: MARLOS-PRO-XXXX-XXXX-XXXX"
        )
        instructions.setWordWrap(True)
        layout.addWidget(instructions)

        layout.addSpacing(15)

        # Form
        form = QFormLayout()

        email_input = QLineEdit()
        email_input.setPlaceholderText("your@email.com")
        form.addRow("Email:", email_input)

        key_input = QLineEdit()
        key_input.setPlaceholderText("MARLOS-PRO-XXXX-XXXX-XXXX")
        form.addRow("License Key:", key_input)

        layout.addLayout(form)

        layout.addSpacing(15)

        # Buttons
        button_layout = QHBoxLayout()

        activate_btn = QPushButton("Activate")
        activate_btn.setMinimumHeight(40)
        activate_btn.clicked.connect(
            lambda: self.activate_license(key_input.text(), email_input.text(), dialog)
        )
        button_layout.addWidget(activate_btn)

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(dialog.reject)
        button_layout.addWidget(cancel_btn)

        layout.addLayout(button_layout)

        dialog.setLayout(layout)
        dialog.exec()

    def activate_license(self, license_key: str, email: str, dialog: QDialog):
        """Activate a license key."""
        license_key = license_key.strip()
        email = email.strip()

        if not license_key or not email:
            QMessageBox.warning(
                self, "Missing Information",
                "Please enter both your license key and email address."
            )
            return

        success, message = self.license.activate_license(license_key, email)

        if success:
            QMessageBox.information(
                self, "License Activated",
                f"{message}\n\n"
                f"Tier: {self.license.get_tier().value.title()}\n"
                f"Email: {email}"
            )
            dialog.accept()
            # Refresh UI to enable new features
            self.refresh_ui_for_license()
        else:
            QMessageBox.critical(
                self, "Activation Failed",
                f"Could not activate license:\n\n{message}"
            )

    def refresh_ui_for_license(self):
        """Refresh UI after license change."""
        # Update window title to show tier
        tier = self.license.get_tier()
        if tier == LicenseTier.PRO:
            self.setWindowTitle("MarlOS Pro")
        elif tier == LicenseTier.TEAM:
            self.setWindowTitle("MarlOS Team")
        else:
            self.setWindowTitle("MarlOS")

        # Enable/disable features based on license
        # (This is handled by check_pro_feature() calls throughout the app)

    def show_license_info(self):
        """Show current license information."""
        tier = self.license.get_tier()
        features = self.license.get_available_features()

        info = f"""
MarlOS License Information
─────────────────────────

Tier: {tier.value.title()}
Installation ID: {self.license._installation_id}

Available Features:
"""
        for feat_key, feat_desc in features.items():
            info += f"  ✓ {feat_desc}\n"

        if self.license.is_free_tier():
            info += "\nUpgrade to Pro to unlock all features!"

        QMessageBox.information(self, "License Info", info)

    def export_context_for_ai(self):
        """Export current context for cloud AI models."""
        from PyQt6.QtWidgets import QDialog, QVBoxLayout, QTextEdit, QComboBox, QPushButton, QLabel, QCheckBox

        dialog = QDialog(self)
        dialog.setWindowTitle("Export Context for AI")
        dialog.setMinimumWidth(700)
        dialog.setMinimumHeight(500)

        layout = QVBoxLayout()

        # Title
        title = QLabel("Export Your Context to Clipboard")
        title.setStyleSheet("font-size: 16px; font-weight: bold;")
        layout.addWidget(title)

        layout.addSpacing(10)

        # Instructions
        instructions = QLabel(
            "This exports your current MarlOS context so you can paste it into "
            "Claude, ChatGPT, or other cloud AI models. The AI will have full "
            "context of what you're working on."
        )
        instructions.setWordWrap(True)
        layout.addWidget(instructions)

        layout.addSpacing(10)

        # Options
        options_layout = QHBoxLayout()

        # Time window selector
        options_layout.addWidget(QLabel("Time window:"))
        time_window = QComboBox()
        time_window.addItems(["Current session", "Last hour", "Today", "This week", "All time"])
        time_window.setCurrentIndex(2)  # Default to "Today"
        options_layout.addWidget(time_window)

        layout.addLayout(options_layout)

        layout.addSpacing(10)

        # Include options
        include_layout = QHBoxLayout()

        include_focus = QCheckBox("Include focus")
        include_focus.setChecked(True)
        include_layout.addWidget(include_focus)

        include_files = QCheckBox("Include file contents")
        include_files.setChecked(True)
        include_layout.addWidget(include_files)

        include_relations = QCheckBox("Include file relations")
        include_relations.setChecked(True)
        include_layout.addWidget(include_relations)

        layout.addLayout(include_layout)

        layout.addSpacing(10)

        # Format selector
        format_layout = QHBoxLayout()
        format_layout.addWidget(QLabel("Format:"))
        format_combo = QComboBox()
        format_combo.addItems(["Markdown", "JSON", "Compact"])
        format_combo.setCurrentIndex(0)
        format_layout.addWidget(format_combo)
        layout.addLayout(format_layout)

        layout.addSpacing(10)

        # Preview text area
        preview_label = QLabel("Preview:")
        layout.addWidget(preview_label)

        preview = QTextEdit()
        preview.setReadOnly(True)
        preview.setPlainText("Click 'Generate' to create context export...")
        layout.addWidget(preview)

        # Buttons
        button_layout = QHBoxLayout()

        generate_btn = QPushButton("Generate Preview")
        generate_btn.clicked.connect(
            lambda: self.generate_context_preview(
                time_window.currentText(),
                include_focus.isChecked(),
                include_files.isChecked(),
                include_relations.isChecked(),
                format_combo.currentText(),
                preview
            )
        )
        button_layout.addWidget(generate_btn)

        copy_btn = QPushButton("Copy to Clipboard")
        copy_btn.clicked.connect(lambda: self.copy_context_to_clipboard(preview.toPlainText()))
        button_layout.addWidget(copy_btn)

        close_btn = QPushButton("Close")
        close_btn.clicked.connect(dialog.accept)
        button_layout.addWidget(close_btn)

        layout.addLayout(button_layout)

        dialog.setLayout(layout)
        dialog.exec()

    def generate_context_preview(self, time_window: str, include_focus: bool,
                               include_files: bool, include_relations: bool,
                               format_type: str, preview: QTextEdit):
        """Generate context preview."""
        try:
            # Map time window to kernel format
            time_map = {
                "Current session": "all",
                "Last hour": "all",  # TODO: Implement hour filtering
                "Today": "today",
                "This week": "this_week",
                "All time": "all"
            }
            kernel_window = time_map.get(time_window, "today")

            # Get context from kernel
            context = self.kernel.export_context(
                format=format_type.lower(),
                time_window=kernel_window
            )

            # Add file contents if requested
            if include_files:
                context += "\n\n## Open Files\n\n"
                for tab_idx in range(self.tabs.count()):
                    widget = self.tabs.widget(tab_idx)
                    if hasattr(widget, 'file_path') and widget.file_path:
                        try:
                            with open(widget.file_path, 'r', encoding='utf-8') as f:
                                content = f.read()
                                # Limit content size
                                if len(content) > 5000:
                                    content = content[:5000] + "\n\n... (truncated)"
                                context += f"\n### {widget.file_path}\n\n```\n{content}\n```\n\n"
                        except Exception:
                            pass

            preview.setPlainText(context)

        except Exception as e:
            preview.setPlainText(f"Error generating context:\n{str(e)}")

    def copy_context_to_clipboard(self, text: str):
        """Copy context to clipboard."""
        from PyQt6.QtWidgets import QApplication
        clipboard = QApplication.clipboard()
        clipboard.setText(text)
        QMessageBox.information(self, "Copied", "Context copied to clipboard!\n\nPaste it into your AI chat.")

    def _default_workspace_config(self):
        return {
            "name": self.workspace_root.name,
            "description": "",
            "roots": ["."],
            "notes_dir": "notes",
            "task_index_scope": "open_docs",
            "indexes": {
                "tasks": "tasks_index.db",
            },
            "lexicon_pack_paths": ["lexicon_packs/*.json"],
            "panels": {
                "tasks": False,
                "outline": False,
                "word_count": False,
                "chat": False,
                "notes": False,
            },
            "graphics_enabled": self.graphics_enabled,
            "context_menu_enabled": sorted(self.context_menu_config.get("enabled", set())),
        }

    def toggle_outline_dock(self):
        self.outline_dock.setVisible(self.outline_action.isChecked())

    def toggle_word_count_dock(self):
        self.word_count_dock.setVisible(self.word_count_action.isChecked())

    def toggle_tasks_dock(self):
        self.tasks_dock.setVisible(self.tasks_action.isChecked())

    def toggle_links_dock(self):
        visible = self.links_action.isChecked()
        self.links_dock.setVisible(visible)
        if visible:
            self._update_links_panel()

    def toggle_semantic_dock(self):
        visible = self.semantic_action.isChecked()
        self.semantic_dock.setVisible(visible)
        if visible:
            tab = self.tabs.currentWidget()
            if isinstance(tab, MarkdownTab) and tab.file_path:
                self.semantic_panel.record_file_access(tab.file_path)
                self.semantic_panel.update_for_file(tab.file_path)

    def toggle_relation_dock(self):
        """Toggle relation graph dock (Pro feature)."""
        # Check if user has Pro license
        if not self.check_pro_feature("pro.relation_graph"):
            self.relation_action.setChecked(False)
            return

        visible = self.relation_action.isChecked()
        self.relation_dock.setVisible(visible)
        if visible:
            # Refresh the graph when shown
            from ide.relation_explorer import RelationExplorer
            explorer = self.relation_dock.widget()
            if isinstance(explorer, RelationExplorer):
                explorer.load_graph()

    def _update_links_panel(self):
        """Update the links panel for the current document."""
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab) and tab.file_path:
            self.links_panel.update_for_path(tab.file_path, get_kernel())
        else:
            self.links_panel.clear_panel()

    def toggle_reader_mode(self):
        index = self.tabs.currentIndex()
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            if not tab.file_path:
                QMessageBox.information(self, "Reader Mode", "Save the file to enter reader mode.")
                self.reader_action.setChecked(False)
                return
            reader = ReaderTab(tab.file_path)
            self._replace_tab(index, reader, Path(tab.file_path).name, tab.file_path)
            self.reader_action.setChecked(True)
            return

        if isinstance(tab, ReaderTab):
            suffix = Path(tab.file_path).suffix.lower()
            if suffix not in (".md", ".markdown", ".txt"):
                QMessageBox.information(self, "Reader Mode", "Reader mode is only toggleable for text files.")
                self.reader_action.setChecked(True)
                return
            edit_tab = MarkdownTab(
                tab.file_path,
                dark_mode=self.dark_mode,
                graphics_enabled=self.graphics_enabled,
                context_menu_config=self.context_menu_config,
                lexicon=self.lexicon,
                ai_client=self.ai_client,
                write_console_callback=self._write_to_console,
            )
            self._replace_tab(index, edit_tab, Path(tab.file_path).name, tab.file_path)
            edit_tab.outline_list = self.outline_dock.widget()
            edit_tab.word_count_label = self.word_count_label
            edit_tab.refresh_outline()
            edit_tab.refresh_word_count()
            edit_tab.events.document_saved.connect(self.on_document_saved)
            edit_tab.events.document_opened.connect(self.on_document_opened)
            edit_tab.events.document_closed.connect(self.on_document_closed)
            self.register_task_commands(edit_tab)
            self.reader_action.setChecked(False)
            return

        self.reader_action.setChecked(False)

    def _replace_tab(self, index, new_widget, title, tooltip):
        old = self.tabs.widget(index)
        self.tabs.removeTab(index)
        self.tabs.insertTab(index, new_widget, title)
        self.tabs.setCurrentIndex(index)
        self.tabs.setTabToolTip(index, tooltip)
        if old:
            old.deleteLater()

    def show_tasks_panel(self):
        self.tasks_dock.setVisible(True)
        self.refresh_tasks_panel()

    def refresh_tasks_panel(self):
        status_filter = self.tasks_status_filter.currentText().lower()
        status = None
        if status_filter == "open":
            status = "open"
        elif status_filter == "done":
            status = "done"

        tasks = self.task_store.list_tasks(status=status)
        query = self.tasks_search.text().strip().lower()
        if query:
            tasks = [task for task in tasks if query in task["text"].lower()]

        tag_query = self.tasks_tag_filter.text().strip().lower()
        if tag_query:
            tag_query = tag_query.lstrip("@")
            tasks = [task for task in tasks if tag_query in [tag.lower() for tag in task.get("tags", [])]]

        group_by = self.tasks_group_filter.currentText().lower()
        self.tasks_list.clear()

        if group_by == "file":
            groups = {}
            for task in tasks:
                name = Path(task["source_path"]).name
                groups.setdefault(name, []).append(task)
            self._add_task_groups(groups)
        elif group_by == "due":
            groups = {}
            for task in tasks:
                due = task.get("due") or "(no due)"
                groups.setdefault(due, []).append(task)
            self._add_task_groups(groups)
        elif group_by == "tag":
            groups = {}
            for task in tasks:
                tags = task.get("tags") or []
                if not tags:
                    groups.setdefault("(no tag)", []).append(task)
                for tag in tags:
                    groups.setdefault(f"@{tag}", []).append(task)
            self._add_task_groups(groups)
        else:
            for task in tasks:
                item = self._build_task_item(task)
                self.tasks_list.addItem(item)

    def _add_task_groups(self, groups):
        for group_name in sorted(groups.keys()):
            header = QListWidgetItem(group_name)
            header.setFlags(Qt.ItemFlag.NoItemFlags)
            self.tasks_list.addItem(header)
            for task in groups[group_name]:
                item = self._build_task_item(task)
                self.tasks_list.addItem(item)

    def set_tasks_status_filter(self, value):
        for pill, label in (
            (self.pill_all, "All"),
            (self.pill_open, "Open"),
            (self.pill_done, "Done"),
        ):
            pill.setChecked(label == value)
        index = self.tasks_status_filter.findText(value)
        if index >= 0:
            self.tasks_status_filter.setCurrentIndex(index)

    def _build_task_item(self, task):
        status = "[x]" if task["status"] == "done" else "[ ]"
        due = f" due:{task['due']}" if task.get("due") else ""
        item = QListWidgetItem(f"{status} {task['text']}{due}")
        item.setData(Qt.ItemDataRole.UserRole, task["task_id"])
        return item

    def open_task_from_list(self, item):
        task_id = item.data(Qt.ItemDataRole.UserRole)
        if not task_id:
            return
        self.open_task_source(task_id)

    def on_document_opened(self, document):
        if document and document.file_path:
            self.index_document(document)

    def on_document_saved(self, document):
        if document and document.file_path:
            self.index_document(document)

    def on_document_closed(self, document):
        self.refresh_tasks_panel()

    def index_document(self, document):
        tasks = self.task_extractor.extract(document.content, document.file_path)
        self.task_store.upsert_document_tasks(document.file_path, tasks)
        self.refresh_tasks_panel()
        self.update_task_badge()

    def refresh_task_index(self):
        if self.workspace_index_scope == "workspace":
            self.index_workspace()
        else:
            for i in range(self.tabs.count()):
                tab = self.tabs.widget(i)
                if isinstance(tab, MarkdownTab) and tab.document.file_path:
                    self.index_document(tab.document)

    def index_workspace(self):
        base = Path(__file__).resolve().parent
        for path in base.rglob("*.md"):
            try:
                content = path.read_text(encoding="utf-8")
            except Exception:
                continue
            tasks = self.task_extractor.extract(content, str(path))
            self.task_store.upsert_document_tasks(str(path), tasks)
        self.refresh_tasks_panel()
        self.update_task_badge()

    def open_task_source(self, task_id):
        task = self.task_store.get_task(task_id)
        if not task:
            return
        self.open_file(task["source_path"])
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            self.jump_to_line(tab, task["source_line"])

    def jump_to_line(self, tab, line_index):
        if not tab.edit_mode:
            tab.toggle_mode()
        editor = tab.editor
        cursor = editor.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.Start)
        for _ in range(max(line_index, 0)):
            cursor.movePosition(QTextCursor.MoveOperation.Down)
        editor.setTextCursor(cursor)
        editor.setFocus()

    def command_open_task_source(self):
        tasks = self.task_store.list_tasks()
        if not tasks:
            QMessageBox.information(self, "Open Task Source", "No tasks found.")
            return
        dialog = TaskSelectionDialog("Open Task Source", tasks, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        task_id = dialog.choice
        if task_id:
            self.open_task_source(task_id)

    def command_toggle_task_done(self):
        tasks = self.task_store.list_tasks()
        if not tasks:
            QMessageBox.information(self, "Toggle Task", "No tasks found.")
            return
        dialog = TaskSelectionDialog("Toggle Task Done", tasks, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        task_id = dialog.choice
        if task_id:
            self.toggle_task_done(task_id)

    def toggle_task_done(self, task_id):
        task = self.task_store.get_task(task_id)
        if not task:
            return
        self.open_file(task["source_path"])
        tab = self.tabs.currentWidget()
        if not isinstance(tab, MarkdownTab):
            return
        lines = tab.document.content.splitlines()
        line_index = self._find_task_line(lines, task)
        if line_index is None:
            QMessageBox.information(self, "Toggle Task", "Task line not found.")
            return
        new_line = self._toggle_task_line(lines[line_index])
        if new_line is None:
            QMessageBox.information(self, "Toggle Task", "Task format not recognized.")
            return
        self._replace_line_in_editor(tab, line_index, new_line)

    def _find_task_line(self, lines, task):
        if 0 <= task["source_line"] < len(lines):
            if task["text"] in lines[task["source_line"]]:
                return task["source_line"]
        for idx, line in enumerate(lines):
            if task["text"] in line:
                return idx
        return None

    def _toggle_task_line(self, line):
        checkbox = re.match(r"^(\s*[-*+]\s+\[)( |x|X)(\]\s+)(.*)", line)
        if checkbox:
            current = checkbox.group(2).lower()
            new_mark = " " if current == "x" else "x"
            return f"{checkbox.group(1)}{new_mark}{checkbox.group(3)}{checkbox.group(4)}"
        todo = re.match(r"^(\s*)TODO(?:\([^)]+\))?:\s+(.*)", line, re.I)
        if todo:
            text = todo.group(2)
            return f"- [x] {text}"
        done = re.match(r"^(\s*)DONE:\s+(.*)", line, re.I)
        if done:
            text = done.group(2)
            return f"- [ ] {text}"
        return None

    def _replace_line_in_editor(self, tab, line_index, new_line):
        editor = tab.editor
        if editor:
            cursor = editor.textCursor()
            cursor.movePosition(QTextCursor.MoveOperation.Start)
            for _ in range(max(line_index, 0)):
                cursor.movePosition(QTextCursor.MoveOperation.Down)
            cursor.select(QTextCursor.SelectionType.LineUnderCursor)
            cursor.insertText(new_line)
            editor.setTextCursor(cursor)
            editor.setFocus()
            tab.context.set_content(editor.toPlainText())
        else:
            lines = tab.document.content.splitlines()
            lines[line_index] = new_line
            tab.context.set_content("\n".join(lines))

    def command_create_reminder_from_selection(self, context):
        selection = context.get_selection().strip()
        if not selection:
            QMessageBox.information(self, "Create Reminder", "Select text to create a reminder.")
            return
        self._create_reminder_dialog(None, selection, "")

    def command_create_reminder_from_task(self):
        tasks = self.task_store.list_tasks(status="open")
        if not tasks:
            QMessageBox.information(self, "Create Reminder", "No open tasks found.")
            return
        dialog = TaskSelectionDialog("Create Reminder From Task", tasks, self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        task_id = dialog.choice
        task = self.task_store.get_task(task_id)
        if not task:
            return
        self._create_reminder_dialog(task_id, task["text"], "")

    def _create_reminder_dialog(self, task_id, title_default, notes_default):
        dialog = QDialog(self)
        dialog.setWindowTitle("Create Reminder")
        layout = QVBoxLayout(dialog)

        form = QFormLayout()
        title_input = QLineEdit(title_default)
        notes_input = QPlainTextEdit()
        notes_input.setPlainText(notes_default)
        scheduled_input = QDateTimeEdit()
        scheduled_input.setCalendarPopup(True)
        scheduled_input.setDateTime(QDateTime.currentDateTime())

        form.addRow("Title:", title_input)
        form.addRow("Notes:", notes_input)
        form.addRow("Scheduled for:", scheduled_input)
        layout.addLayout(form)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok |
            QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)

        if dialog.exec() != QDialog.DialogCode.Accepted:
            return

        title = title_input.text().strip()
        if not title:
            QMessageBox.information(self, "Create Reminder", "Title is required.")
            return

        scheduled_for = scheduled_input.dateTime().toString(Qt.DateFormat.ISODate)
        self.task_store.create_reminder(task_id, title, notes_input.toPlainText().strip(), scheduled_for)
        self.update_task_badge()
        QMessageBox.information(self, "Create Reminder", "Reminder created.")

    def command_list_reminders(self):
        reminders = self.task_store.list_reminders()
        if not reminders:
            QMessageBox.information(self, "Reminders", "No reminders found.")
            return
        dialog = QDialog(self)
        dialog.setWindowTitle("Reminders")
        dialog.setMinimumWidth(520)
        layout = QVBoxLayout(dialog)

        list_widget = QListWidget()
        for reminder in reminders:
            status = reminder["status"]
            item = QListWidgetItem(
                f"[{status}] {reminder['title']} ({reminder['scheduled_for']})"
            )
            list_widget.addItem(item)
        layout.addWidget(list_widget)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(dialog.reject)
        buttons.accepted.connect(dialog.accept)
        layout.addWidget(buttons)
        dialog.exec()

    def command_dismiss_reminder(self):
        reminders = self.task_store.list_reminders(status="scheduled")
        if not reminders:
            QMessageBox.information(self, "Dismiss Reminder", "No scheduled reminders.")
            return
        dialog = QDialog(self)
        dialog.setWindowTitle("Dismiss Reminder")
        dialog.setMinimumWidth(520)
        layout = QVBoxLayout(dialog)

        list_widget = QListWidget()
        for reminder in reminders:
            item = QListWidgetItem(
                f"{reminder['title']} ({reminder['scheduled_for']})"
            )
            item.setData(Qt.ItemDataRole.UserRole, reminder["reminder_id"])
            list_widget.addItem(item)
        layout.addWidget(list_widget)

        button_row = QHBoxLayout()
        button_row.addStretch()
        dismiss_btn = QPushButton("Dismiss")
        dismiss_btn.clicked.connect(dialog.accept)
        button_row.addWidget(dismiss_btn)
        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(dialog.reject)
        button_row.addWidget(cancel_btn)
        layout.addLayout(button_row)

        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        item = list_widget.currentItem()
        if not item:
            return
        reminder_id = item.data(Qt.ItemDataRole.UserRole)
        self.task_store.dismiss_reminder(reminder_id)
        self.update_task_badge()
        QMessageBox.information(self, "Dismiss Reminder", "Reminder dismissed.")

    def command_agent_summarize_open_tasks(self):
        tasks = self.task_store.list_open_tasks()
        if not tasks:
            QMessageBox.information(self, "Summarize Open Tasks", "No open tasks found.")
            return
        lines = [f"- {task['text']}" for task in tasks[:20]]
        summary = "Open Tasks Summary\n" + "\n".join(lines)
        self.current_tab_present_text("Open Tasks Summary", summary)

    def command_agent_suggest_due_dates(self, context):
        selection = context.get_selection().strip()
        message = selection or "Select a task or text to suggest a due date."
        suggestion = (
            "Suggested due date: " + QDateTime.currentDateTime().addDays(7).date().toString(Qt.DateFormat.ISODate)
        )
        self.current_tab_present_text("Suggest Due Dates", f"{message}\n\n{suggestion}")

    def command_agent_propose_schedule(self, context):
        selection = context.get_selection().strip()
        base = selection or "Selected task(s)"
        proposal = (
            f"{base}\n\nProposed schedule:\n"
            f"- Reminder 1: {QDateTime.currentDateTime().addDays(1).toString(Qt.DateFormat.ISODate)}\n"
            f"- Reminder 2: {QDateTime.currentDateTime().addDays(3).toString(Qt.DateFormat.ISODate)}\n"
        )
        self.current_tab_present_text("Propose Reminder Schedule", proposal)

    def current_tab_present_text(self, title, content):
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            tab.present_text(title, content, allow_insert=False)

    def command_timer_start(self):
        dialog = QDialog(self)
        dialog.setWindowTitle("Start Timer")
        layout = QVBoxLayout(dialog)

        form = QFormLayout()
        minutes_input = QSpinBox()
        minutes_input.setRange(1, 480)
        minutes_input.setValue(25)
        label_input = QLineEdit()
        form.addRow("Minutes:", minutes_input)
        form.addRow("Label:", label_input)
        layout.addLayout(form)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok |
            QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)

        if dialog.exec() != QDialog.DialogCode.Accepted:
            return

        minutes = minutes_input.value()
        label = label_input.text().strip()
        self.start_timer(minutes, label)

    def start_timer(self, minutes, label):
        self.timer_remaining = minutes * 60
        self.timer_label = label
        self.timer_running = True
        self.timer.start(1000)
        self.update_timer_status()

    def command_timer_stop(self):
        if not self.timer_running:
            QMessageBox.information(self, "Timer", "No active timer.")
            return
        self.timer.stop()
        self.timer_running = False
        self.timer_remaining = 0
        self.timer_label = ""
        self.statusBar().clearMessage()
        QMessageBox.information(self, "Timer", "Timer stopped.")

    def command_timer_status(self):
        if not self.timer_running:
            QMessageBox.information(self, "Timer", "No active timer.")
            return
        minutes, seconds = divmod(self.timer_remaining, 60)
        label = f" ({self.timer_label})" if self.timer_label else ""
        QMessageBox.information(
            self,
            "Timer Status",
            f"Time remaining: {minutes:02d}:{seconds:02d}{label}",
        )

    def on_timer_tick(self):
        if not self.timer_running:
            return
        self.timer_remaining -= 1
        if self.timer_remaining <= 0:
            self.timer.stop()
            self.timer_running = False
            self.timer_remaining = 0
            label = f" ({self.timer_label})" if self.timer_label else ""
            self.timer_label = ""
            self.statusBar().clearMessage()
            QMessageBox.information(self, "Timer", f"Timer complete{label}.")
        else:
            self.update_timer_status()

    def update_timer_status(self):
        minutes, seconds = divmod(self.timer_remaining, 60)
        label = f" {self.timer_label}" if self.timer_label else ""
        self.statusBar().showMessage(f"Timer {minutes:02d}:{seconds:02d}{label}")

    def command_reload_lexicon(self):
        self.lexicon = LocalLexicon(
            extra_paths=[
                str(self.workspace_root / path)
                for path in self.workspace_config.get("lexicon_pack_paths", [])
            ]
        )
        for i in range(self.tabs.count()):
            tab = self.tabs.widget(i)
            if isinstance(tab, MarkdownTab):
                tab.lexicon = self.lexicon
                tab.lexicon_provider.lexicon = self.lexicon
        QMessageBox.information(self, "Lexicon Packs", "Lexicon packs reloaded.")

    def show_settings(self):
        dialog = QDialog(self)
        dialog.setWindowTitle("Settings")
        dialog.setMinimumWidth(420)
        layout = QVBoxLayout(dialog)

        graphics_checkbox = QCheckBox("Enable graphics and image generation")
        graphics_checkbox.setChecked(self.graphics_enabled)
        layout.addWidget(graphics_checkbox)

        workspace_checkbox = QCheckBox("Enable workspace task indexing")
        workspace_checkbox.setChecked(self.workspace_index_scope == "workspace")
        layout.addWidget(workspace_checkbox)

        # LM Studio Settings
        layout.addWidget(QLabel(""))  # Spacer
        layout.addWidget(QLabel("LM Studio (AI) Settings"))

        lm_endpoint_label = QLabel("LM Studio Endpoint:")
        lm_endpoint_input = QLineEdit()
        lm_endpoint_input.setPlaceholderText("http://localhost:1234/v1")
        lm_endpoint_input.setText(
            self.workspace_config.get("lm_studio_endpoint", "http://localhost:1234/v1")
        )
        layout.addWidget(lm_endpoint_label)
        layout.addWidget(lm_endpoint_input)

        lm_model_label = QLabel("Model Name (optional):")
        lm_model_input = QLineEdit()
        lm_model_input.setPlaceholderText("Leave empty to use loaded model")
        lm_model_input.setText(self.workspace_config.get("lm_studio_model", "") or "")
        layout.addWidget(lm_model_label)
        layout.addWidget(lm_model_input)

        # Status check button
        def check_lm_status():
            if hasattr(self.ai_client, 'refresh_availability'):
                self.ai_client.refresh_availability()
            if hasattr(self.ai_client, 'is_lm_studio_active'):
                if self.ai_client.is_lm_studio_active():
                    QMessageBox.information(dialog, "LM Studio Status", "Connected to LM Studio!")
                else:
                    QMessageBox.warning(dialog, "LM Studio Status", "Cannot connect to LM Studio.\nMake sure it's running and the endpoint is correct.")
            else:
                QMessageBox.information(dialog, "LM Studio Status", "Status check not available.")

        check_btn = QPushButton("Check LM Studio Connection")
        check_btn.clicked.connect(check_lm_status)
        layout.addWidget(check_btn)

        layout.addWidget(QLabel(""))  # Spacer
        layout.addWidget(QLabel("Context Menu Items"))

        search_input = QLineEdit()
        search_input.setPlaceholderText("Search menu items...")
        layout.addWidget(search_input)

        list_widget = QListWidget()
        list_widget.setSelectionMode(QListWidget.SelectionMode.NoSelection)
        layout.addWidget(list_widget)

        states = {}
        for _group, items in self.context_menu_items:
            for item_id, _label in items:
                states[item_id] = item_id in self.context_menu_config["enabled"]

        def populate_list(filter_text=""):
            list_widget.blockSignals(True)
            list_widget.clear()
            filter_text = filter_text.strip().lower()
            for group, items in self.context_menu_items:
                group_matches = False
                for _item_id, label in items:
                    if not filter_text or filter_text in label.lower():
                        group_matches = True
                        break
                if not group_matches:
                    continue

                header = QListWidgetItem(group)
                header.setFlags(Qt.ItemFlag.NoItemFlags)
                list_widget.addItem(header)

                for item_id, label in items:
                    if filter_text and filter_text not in label.lower():
                        continue
                    item = QListWidgetItem(label)
                    item.setData(Qt.ItemDataRole.UserRole, item_id)
                    item.setFlags(item.flags() | Qt.ItemFlag.ItemIsUserCheckable)
                    item.setCheckState(
                        Qt.CheckState.Checked if states.get(item_id, False) else Qt.CheckState.Unchecked
                    )
                    list_widget.addItem(item)
            list_widget.blockSignals(False)

        def on_item_changed(item):
            item_id = item.data(Qt.ItemDataRole.UserRole)
            states[item_id] = item.checkState() == Qt.CheckState.Checked

        search_input.textChanged.connect(populate_list)
        list_widget.itemChanged.connect(on_item_changed)
        populate_list()

        layout.addStretch()

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok |
            QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)

        if dialog.exec() != QDialog.DialogCode.Accepted:
            return

        self.graphics_enabled = graphics_checkbox.isChecked()
        self.workspace_index_scope = "workspace" if workspace_checkbox.isChecked() else "open_docs"
        self.context_menu_config["enabled"] = {item_id for item_id, checked in states.items() if checked}

        # Save LM Studio settings
        new_endpoint = lm_endpoint_input.text().strip() or "http://localhost:1234/v1"
        new_model = lm_model_input.text().strip() or None
        self.workspace_config["lm_studio_endpoint"] = new_endpoint
        self.workspace_config["lm_studio_model"] = new_model

        # Update AI client with new settings
        self.ai_client = HybridAIClient(
            lm_studio_endpoint=new_endpoint,
            model=new_model
        )

        self.save_settings()
        for i in range(self.tabs.count()):
            tab = self.tabs.widget(i)
            if isinstance(tab, MarkdownTab):
                tab.set_graphics_enabled(self.graphics_enabled)
                # Update tab's AI client reference
                tab.ai_client = self.ai_client

        # Update chat panel's AI client
        if hasattr(self, 'chat_panel'):
            self.chat_panel.set_ai_client(self.ai_client)

    def register_task_commands(self, tab):
        tab.commands.register(
            Command("tasks.showPanel", "Show Tasks Panel", lambda ctx: self.show_tasks_panel())
        )
        tab.commands.register(
            Command("tasks.refreshIndex", "Refresh Task Index", lambda ctx: self.refresh_task_index())
        )
        tab.commands.register(
            Command("tasks.openSource", "Open Task Source", lambda ctx: self.command_open_task_source())
        )
        tab.commands.register(
            Command("tasks.toggleDone", "Toggle Task Done", lambda ctx: self.command_toggle_task_done())
        )
        tab.commands.register(
            Command(
                "reminders.createFromSelection",
                "Create Reminder From Selection",
                lambda ctx: self.command_create_reminder_from_selection(ctx),
            )
        )
        tab.commands.register(
            Command(
                "reminders.createFromTask",
                "Create Reminder From Task",
                lambda ctx: self.command_create_reminder_from_task(),
            )
        )
        tab.commands.register(
            Command("reminders.list", "List Reminders", lambda ctx: self.command_list_reminders())
        )
        tab.commands.register(
            Command(
                "reminders.dismiss",
                "Dismiss Reminder",
                lambda ctx: self.command_dismiss_reminder(),
            )
        )
        tab.commands.register(
            Command(
                "agent.summarizeOpenTasks",
                "Summarize Open Tasks",
                lambda ctx: self.command_agent_summarize_open_tasks(),
            )
        )
        tab.commands.register(
            Command(
                "agent.suggestDueDates",
                "Suggest Due Dates",
                lambda ctx: self.command_agent_suggest_due_dates(ctx),
            )
        )
        tab.commands.register(
            Command(
                "agent.proposeReminderSchedule",
                "Propose Reminder Schedule",
                lambda ctx: self.command_agent_propose_schedule(ctx),
            )
        )
        tab.commands.register(
            Command("timer.start", "Start Timer", lambda ctx: self.command_timer_start())
        )
        tab.commands.register(
            Command("timer.stop", "Stop Timer", lambda ctx: self.command_timer_stop())
        )
        tab.commands.register(
            Command("timer.status", "Timer Status", lambda ctx: self.command_timer_status())
        )
        tab.commands.register(
            Command("lexicon.reload", "Reload Lexicon Packs", lambda ctx: self.command_reload_lexicon())
        )

    def show_command_palette(self):
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            palette = CommandPalette(tab.commands, tab.context, self)
            palette.exec()
        else:
            # For Home tab, PDF tab, or any other - find a MarkdownTab to use
            for i in range(1, self.tabs.count()):
                t = self.tabs.widget(i)
                if isinstance(t, MarkdownTab):
                    self.tabs.setCurrentIndex(i)
                    palette = CommandPalette(t.commands, t.context, self)
                    palette.exec()
                    return
            # No document tabs open
            QMessageBox.information(self, "Command Palette", "Open a document first to use the command palette.")

    def jump_to_heading(self, item):
        tab = self.tabs.currentWidget()
        if not isinstance(tab, MarkdownTab):
            return
        line = item.data(Qt.ItemDataRole.UserRole)
        if line is None:
            return
        if not tab.edit_mode:
            tab.toggle_mode()
        editor = tab.editor
        cursor = editor.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.Start)
        for _ in range(max(line - 1, 0)):
            cursor.movePosition(QTextCursor.MoveOperation.Down)
        editor.setTextCursor(cursor)
        editor.setFocus()

    def new_file(self):
        # Switch to editor view
        self.show_editor()

        tab = MarkdownTab(
            dark_mode=self.dark_mode,
            graphics_enabled=self.graphics_enabled,
            context_menu_config=self.context_menu_config,
            lexicon=self.lexicon,
            ai_client=self.ai_client,
            write_console_callback=self._write_to_console,
        )
        index = self.tabs.addTab(tab, "Untitled")
        self.tabs.setCurrentIndex(index)
        self.outline_list = self.outline_dock.widget()
        tab.outline_list = self.outline_list
        tab.word_count_label = self.word_count_label
        tab.refresh_outline()
        tab.refresh_word_count()
        tab.events.document_saved.connect(self.on_document_saved)
        tab.events.document_opened.connect(self.on_document_opened)
        tab.events.document_closed.connect(self.on_document_closed)
        self.register_task_commands(tab)

    def new_browser_tab(self, url=None):
        """Create a new browser tab."""
        if not QWebEngineView:
            QMessageBox.warning(
                self,
                "WebEngine Not Available",
                "PyQt6-WebEngine is not installed.\n\n"
                "Please install it using:\n"
                "pip install PyQt6-WebEngine"
            )
            return

        tab = BrowserTab(
            kernel=self.kernel,
            dark_mode=self.dark_mode,
        )

        if url:
            tab.set_url(url)

        index = self.tabs.addTab(tab, "Browser")
        self.tabs.setCurrentIndex(index)

    def new_terminal_tab(self):
        """Create a new terminal tab."""
        tab = TerminalTab(
            kernel=self.kernel,
            dark_mode=self.dark_mode,
        )

        index = self.tabs.addTab(tab, "Terminal")
        self.tabs.setCurrentIndex(index)

    def new_terminal_grid(self, rows=2, cols=2):
        """Create a grid of terminals in a new tab.

        Args:
            rows: Number of rows (default: 2)
            cols: Number of columns (default: 2)
        """
        from PyQt6.QtWidgets import QSplitter

        # Create a container widget for the grid
        container = QWidget()
        layout = QVBoxLayout(container)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        # Create rows
        for row in range(rows):
            row_splitter = QSplitter(Qt.Orientation.Horizontal)

            # Create columns in this row
            for col in range(cols):
                terminal = TerminalTab(
                    kernel=self.kernel,
                    dark_mode=self.dark_mode,
                )
                terminal.setMinimumHeight(150)
                row_splitter.addWidget(terminal)

            layout.addWidget(row_splitter)

        # Add as a new tab
        index = self.tabs.addTab(container, f"Terminal Grid ({rows}x{cols})")
        self.tabs.setCurrentIndex(index)

    def new_app_launcher_tab(self):
        """Create a new app launcher tab."""
        tab = AppLauncherTab(
            kernel=self.kernel,
            dark_mode=self.dark_mode,
        )

        index = self.tabs.addTab(tab, "App Launcher")
        self.tabs.setCurrentIndex(index)

    def new_grid_workspace(self, rows=2, cols=2):
        """Create a new drag-and-drop grid workspace tab.

        Args:
            rows: Number of rows (default: 2)
            cols: Number of columns (default: 2)
        """
        grid_manager = GridWindowManager(
            kernel=self.kernel,
            dark_mode=self.dark_mode,
            rows=rows,
            cols=cols
        )

        # Pre-populate with terminals (only up to 2x2 for now)
        for row in range(min(rows, 2)):
            for col in range(min(cols, 2)):
                terminal = TerminalTab(
                    kernel=self.kernel,
                    dark_mode=self.dark_mode
                )
                terminal.setMinimumSize(200, 150)
                cell_key = (row, col)
                if cell_key in grid_manager.grid_widgets:
                    grid_manager.grid_widgets[cell_key].set_widget(terminal)

        # Add as a new tab
        index = self.tabs.addTab(grid_manager, f"Grid Workspace ({rows}x{cols})")
        self.tabs.setCurrentIndex(index)

    def open_url_dialog(self):
        """Show dialog to open a URL."""
        if not QWebEngineView:
            QMessageBox.warning(
                self,
                "WebEngine Not Available",
                "PyQt6-WebEngine is not installed.\n\n"
                "Please install it using:\n"
                "pip install PyQt6-WebEngine"
            )
            return

        url, ok = QInputDialog.getText(
            self,
            "Open URL",
            "Enter URL:",
            QLineEdit.EchoMode.Normal
        )

        if ok and url.strip():
            self.open_url(url.strip())

    def open_url(self, url):
        """Open a URL in a new browser tab."""
        if not url.startswith(('http://', 'https://')):
            url = 'https://' + url

        # Check if there's already a browser tab
        for i in range(self.tabs.count()):
            tab = self.tabs.widget(i)
            if isinstance(tab, BrowserTab):
                # Reuse existing browser tab
                tab.set_url(url)
                self.tabs.setCurrentIndex(i)
                return

        # Create new browser tab
        self.new_browser_tab(url)

    def search_web(self, engine="google"):
        """Open a search query dialog."""
        if not QWebEngineView:
            QMessageBox.warning(
                self,
                "WebEngine Not Available",
                "PyQt6-WebEngine is not installed.\n\n"
                "Please install it using:\n"
                "pip install PyQt6-WebEngine"
            )
            return

        query, ok = QInputDialog.getText(
            self,
            f"Search {engine.title()}",
            "Enter search query:",
            QLineEdit.EchoMode.Normal
        )

        if ok and query.strip():
            if engine == "google":
                url = f"https://www.google.com/search?q={query}"
            else:
                url = f"https://www.google.com/search?q={query}"
            self.open_url(url)

    def toggle_current_mode(self):
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            tab.toggle_mode()

    def on_tab_changed(self, index):
        tab = self.tabs.widget(index)
        if isinstance(tab, MarkdownTab):
            name = Path(tab.file_path).name if tab.file_path else "Untitled"
            self.setWindowTitle(f"Markdown Editor - {name}")
            self.outline_list = self.outline_dock.widget()
            tab.outline_list = self.outline_list
            tab.word_count_label = self.word_count_label
            tab.refresh_outline()
            tab.refresh_word_count()
            self.reader_action.setChecked(False)
            # Update links panel if visible
            if self.links_dock.isVisible():
                self._update_links_panel()
            # Update semantic panel
            if self.semantic_dock.isVisible() and tab.file_path:
                self.semantic_panel.record_file_access(tab.file_path)
                self.semantic_panel.update_for_file(tab.file_path)
                self.semantic_panel.update_focus()
            # Record focus in context layer
            if tab.file_path:
                self.kernel.record_focus(tab.file_path)
        elif isinstance(tab, ReaderTab):
            name = Path(tab.file_path).name if tab.file_path else "Reader"
            self.setWindowTitle(f"Markdown Editor - {name}")
            self.reader_action.setChecked(True)
            if self.links_dock.isVisible():
                self.links_panel.clear_panel()
        elif isinstance(tab, BrowserTab):
            # Browser tab - update window title with page title
            title = tab.current_title or tab.current_url or "Browser"
            self.setWindowTitle(f"Markdown Editor - {title[:50]}")
            self.reader_action.setChecked(False)
            # Update semantic panel focus display
            if self.semantic_dock.isVisible():
                self.semantic_panel.update_focus()
            # Record focus in context layer
            if tab.current_url:
                self.kernel.record_focus(tab.current_url)

    def show_search(self):
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            tab.show_search()

    def toggle_dark_mode(self):
        self.dark_mode = self.dark_action.isChecked()

        if self.dark_mode:
            self.setStyleSheet("""
                QMainWindow { background: #1e1e1e; }
                QTabWidget::pane { background: #1e1e1e; border: none; }
                QTabBar::tab { background: #2d2d2d; color: #d4d4d4; padding: 8px 16px; }
                QTabBar::tab:selected { background: #1e1e1e; }
                QMenuBar { background: #2d2d2d; color: #d4d4d4; }
                QMenuBar::item:selected { background: #444; }
                QMenu { background: #2d2d2d; color: #d4d4d4; }
                QMenu::item:selected { background: #444; }
            """)
        else:
            self.setStyleSheet("")

        for i in range(self.tabs.count()):
            tab = self.tabs.widget(i)
            if isinstance(tab, MarkdownTab):
                tab.set_dark_mode(self.dark_mode)

    def reload_current(self):
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            tab.reload_file()

    def on_file_changed(self, path: str):
        self.pending_reloads.add(path)
        QTimer.singleShot(500, lambda: self.do_reload(path))

    def do_reload(self, path: str):
        if path not in self.pending_reloads:
            return
        self.pending_reloads.discard(path)

        if path not in self.file_watcher.files():
            self.file_watcher.addPath(path)

        for i in range(self.tabs.count()):
            tab = self.tabs.widget(i)
            if isinstance(tab, MarkdownTab) and tab.file_path == path:
                if not tab.modified:
                    tab.reload_file()

    def open_file_dialog(self):
        files, _ = QFileDialog.getOpenFileNames(
            self, "Open Markdown Files", "",
            "Documents (*.md *.markdown *.txt *.epub *.pdf);;All Files (*)"
        )
        for f in files:
            self.open_file(f)

    def open_file(self, file_path: str):
        file_path = os.path.abspath(file_path)

        # Switch to editor view
        self.show_editor()

        for i in range(self.tabs.count()):
            tab = self.tabs.widget(i)
            if isinstance(tab, (MarkdownTab, ReaderTab)) and tab.file_path == file_path:
                self.tabs.setCurrentIndex(i)
                return

        suffix = Path(file_path).suffix.lower()
        if suffix in (".pdf", ".epub"):
            tab = ReaderTab(file_path)
        else:
            tab = MarkdownTab(
                file_path,
                dark_mode=self.dark_mode,
                graphics_enabled=self.graphics_enabled,
                context_menu_config=self.context_menu_config,
                lexicon=self.lexicon,
                ai_client=self.ai_client,
                write_console_callback=self._write_to_console,
            )
        name = Path(file_path).name
        index = self.tabs.addTab(tab, name)
        self.tabs.setCurrentIndex(index)
        self.tabs.setTabToolTip(index, file_path)
        self.file_watcher.addPath(file_path)
        self.outline_list = self.outline_dock.widget()
        if isinstance(tab, MarkdownTab):
            tab.outline_list = self.outline_list
            tab.word_count_label = self.word_count_label
            tab.refresh_outline()
            tab.refresh_word_count()
            tab.events.document_saved.connect(self.on_document_saved)
            tab.events.document_opened.connect(self.on_document_opened)
            tab.events.document_closed.connect(self.on_document_closed)
            self.register_task_commands(tab)

            # Create kernel context for this document
            ctx = self.kernel.create_context(
                path=file_path,
                context_type="document",
                metadata={"format": suffix.lstrip(".") if suffix else "md"}
            )
            tab.kernel_ctx = ctx
            # Bridge local spine to kernel
            tab.events.event_emitted.connect(
                lambda e, ctx=ctx: ctx.emit(e.event_type, e.payload)
            )

            # Register in workspace
            if hasattr(self, 'context_launcher'):
                self.context_launcher.add_context(
                    file_path,
                    context_id=ctx.id,
                    status="open"
                )
        else:
            # PDF/EPUB - create kernel context without local spine bridge
            ctx = self.kernel.create_context(
                path=file_path,
                context_type="reader",
                metadata={"format": suffix.lstrip(".")}
            )
            tab.kernel_ctx = ctx
            if hasattr(self, 'context_launcher'):
                self.context_launcher.add_context(
                    file_path,
                    context_id=ctx.id,
                    status="open"
                )

    def save_current(self):
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            if tab.commands.execute("document.save", tab.context):
                if tab.file_path:
                    name = Path(tab.file_path).name
                    self.tabs.setTabText(self.tabs.currentIndex(), name)
                    self.setWindowTitle(f"Markdown Editor - {name}")

    def save_current_as(self):
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            if tab.commands.execute("document.save_as", tab.context):
                if tab.file_path:
                    name = Path(tab.file_path).name
                    self.tabs.setTabText(self.tabs.currentIndex(), name)
                    self.tabs.setTabToolTip(self.tabs.currentIndex(), tab.file_path)
                    self.setWindowTitle(f"Markdown Editor - {name}")
                    self.file_watcher.addPath(tab.file_path)

    def close_tab(self, index: int):
        # Don't close the Home tab (index 0)
        if index == 0:
            return

        tab = self.tabs.widget(index)
        if isinstance(tab, MarkdownTab):
            if tab.modified:
                reply = QMessageBox.question(
                    self, "Unsaved Changes",
                    "Save changes before closing?",
                    QMessageBox.StandardButton.Save |
                    QMessageBox.StandardButton.Discard |
                    QMessageBox.StandardButton.Cancel
                )
                if reply == QMessageBox.StandardButton.Save:
                    if not tab.save_file():
                        return
                elif reply == QMessageBox.StandardButton.Cancel:
                    return

            if tab.file_path:
                self.file_watcher.removePath(tab.file_path)
            if tab.events:
                tab.events.document_closed.emit(tab.document)

        # Destroy kernel context (works for any tab type)
        if hasattr(tab, 'kernel_ctx') and tab.kernel_ctx:
            self.kernel.destroy_context(tab.kernel_ctx.id)

        # Update workspace status
        if hasattr(tab, 'file_path') and tab.file_path:
            if hasattr(self, 'context_launcher'):
                self.context_launcher.update_context_status(tab.file_path, "closed")

        self.tabs.removeTab(index)

        # If only Home tab left, switch to it
        if self.tabs.count() == 1:
            self.show_desktop()

    def close_current_tab(self):
        self.close_tab(self.tabs.currentIndex())

    def show_tab_context_menu(self, position):
        """Show context menu for tabs."""
        try:
            # Get the tab at the clicked position
            tab_index = self.tabs.tabBar().tabAt(position)

            # If tabAt didn't work, use current tab
            if tab_index == -1:
                tab_index = self.tabs.currentIndex()

            # Don't allow undocking Home tab (index 0)
            if tab_index == 0:
                return

            tab = self.tabs.widget(tab_index)
            if not tab:
                return

            # Create menu
            menu = QMenu(self)
            menu.setWindowTitle("Tab Menu")

            # Undock/Floating action
            undock_action = QAction("Open in New Window", self)
            undock_action.setStatusTip("Undock this tab into a floating window")
            undock_action.triggered.connect(lambda checked=False, idx=tab_index: self.undock_tab(idx))
            menu.addAction(undock_action)

            menu.addSeparator()

            # Close action
            close_action = QAction("Close", self)
            close_action.triggered.connect(lambda checked=False, idx=tab_index: self.close_tab(idx))
            menu.addAction(close_action)

            # Show menu at cursor position
            global_pos = self.tabs.tabBar().mapToGlobal(position)
            menu.exec(global_pos)

        except Exception as e:
            print(f"[Error] Tab context menu failed: {e}")
            import traceback
            traceback.print_exc()

    def undock_tab(self, tab_index):
        """Undock a tab into a floating window."""
        tab = self.tabs.widget(tab_index)
        if not tab or tab_index == 0:
            return

        # Get tab info
        tab_text = self.tabs.tabText(tab_index)

        # Remove from tab widget (but don't destroy)
        self.tabs.removeTab(tab_index)

        # Create new floating window
        floating_window = QMainWindow(self)
        floating_window.setWindowTitle(f"Floating - {tab_text}")
        floating_window.setMinimumSize(400, 300)
        floating_window.resize(800, 600)

        # IMPORTANT: Reparent the widget to the new window
        tab.setParent(floating_window)

        # Set the tab as central widget
        floating_window.setCentralWidget(tab)

        # Ensure widget is visible
        tab.setVisible(True)
        tab.show()

        # Force a repaint/refresh
        tab.update()
        tab.repaint()

        # Show window AFTER setting up the widget
        floating_window.show()

        # Raise and activate the floating window
        floating_window.raise_()
        floating_window.activateWindow()

        # Track floating window
        self.floating_windows.append({
            'window': floating_window,
            'widget': tab,
            'original_index': tab_index
        })

        # Override closeEvent properly
        original_close = floating_window.closeEvent
        def new_close_event(event):
            self.on_floating_window_close(floating_window, tab, event)
        floating_window.closeEvent = new_close_event

    def undock_current_tab(self):
        """Undock the current tab into a floating window."""
        current_index = self.tabs.currentIndex()
        if current_index > 0:  # Don't undock Home tab
            self.undock_tab(current_index)

    def on_floating_window_close(self, window, widget, event):
        """Handle floating window close event."""
        # Remove from tracking
        self.floating_windows = [fw for fw in self.floating_windows if fw['window'] != window]

        # Option: re-dock tab back to main window
        # For now, just destroy the widget
        widget.deleteLater()

        window.close()
        event.accept()

    def print_current(self):
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            tab.commands.execute("document.print", tab.context)

    def export_pdf_current(self):
        tab = self.tabs.currentWidget()
        if isinstance(tab, MarkdownTab):
            tab.commands.execute("markdown.export.pdf", tab.context)

    def dragEnterEvent(self, event):
        if event.mimeData().hasUrls():
            event.acceptProposedAction()

    def dropEvent(self, event):
        for url in event.mimeData().urls():
            if url.isLocalFile():
                f = url.toLocalFile()
                if f.lower().endswith(('.md', '.markdown', '.txt')):
                    self.open_file(f)

    def closeEvent(self, event):
        # Stop visual memory capture
        if self.visual_memory and self.visual_memory_running:
            self.visual_memory.stop()

        for i in range(self.tabs.count()):
            tab = self.tabs.widget(i)
            if isinstance(tab, MarkdownTab) and tab.modified:
                reply = QMessageBox.question(
                    self, "Unsaved Changes",
                    "You have unsaved changes. Exit anyway?",
                    QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
                )
                if reply != QMessageBox.StandardButton.Yes:
                    event.ignore()
                    return
                break
        # Save workspace before closing
        self.save_workspace()
        # Shutdown kernel
        self.kernel.shutdown()
        event.accept()

    def get_workspace_path(self):
        """Get the path to the workspace file."""
        return Path(__file__).parent / ".workspace.json"

    def save_workspace(self):
        """Save the workspace state to disk."""
        if not hasattr(self, 'context_launcher'):
            return
        try:
            data = {
                "version": 1,
                "contexts": self.context_launcher.to_dict()["contexts"],
                "window": {
                    "geometry": self.saveGeometry().toBase64().data().decode(),
                    "state": self.saveState().toBase64().data().decode(),
                }
            }
            with open(self.get_workspace_path(), "w") as f:
                json.dump(data, f, indent=2)
        except Exception as e:
            print(f"Failed to save workspace: {e}")

    def load_workspace(self):
        """Load the workspace state from disk."""
        path = self.get_workspace_path()
        if not path.exists():
            return
        try:
            with open(path, "r") as f:
                data = json.load(f)
            if hasattr(self, 'context_launcher'):
                self.context_launcher.from_dict({"contexts": data.get("contexts", {})})
            # Restore window geometry
            if "window" in data:
                from PyQt6.QtCore import QByteArray
                if "geometry" in data["window"]:
                    self.restoreGeometry(QByteArray.fromBase64(data["window"]["geometry"].encode()))
                if "state" in data["window"]:
                    self.restoreState(QByteArray.fromBase64(data["window"]["state"].encode()))
        except Exception as e:
            print(f"Failed to load workspace: {e}")

    def _on_kernel_event(self, event):
        """Handle kernel-level events for cross-context coordination."""
        # Log significant events to console for debugging
        if event.event_type.startswith("context."):
            pass  # Context lifecycle events handled elsewhere

    def get_kernel_state(self):
        """Get the kernel state (useful for AI context)."""
        return self.kernel.get_system_state()

    # ==================== VISUAL MEMORY METHODS ====================

    def start_visual_memory(self):
        """Start capturing visual memory (screenshots)."""
        if self.visual_memory is None:
            try:
                from ide.screen_memory import VisualMemoryCapture
                self.visual_memory = VisualMemoryCapture(
                    kernel=self.kernel,
                    capture_interval=30,  # 30 seconds
                    change_threshold=0.05,  # 5% change
                )
            except ImportError as e:
                QMessageBox.warning(
                    self,
                    "Missing Dependencies",
                    f"Visual memory requires additional packages:\n\n{e}\n\n"
                    "Install with: pip install Pillow opencv-python"
                )
                return

        if not self.visual_memory_running:
            self.visual_memory.start()
            self.visual_memory_running = True
            self.visual_memory_start_action.setEnabled(False)
            self.visual_memory_stop_action.setEnabled(True)

            # Show in status bar
            self.statusBar().showMessage("Visual memory: Capturing screenshots every 30 seconds", 5000)

    def stop_visual_memory(self):
        """Stop capturing visual memory."""
        if self.visual_memory and self.visual_memory_running:
            self.visual_memory.stop()
            self.visual_memory_running = False
            self.visual_memory_start_action.setEnabled(True)
            self.visual_memory_stop_action.setEnabled(False)

            # Show in status bar
            self.statusBar().showMessage("Visual memory: Stopped", 3000)

    def browse_visual_memory(self):
        """Browse captured visual memory."""
        if not self.visual_memory:
            QMessageBox.information(
                self,
                "No Visual Memory",
                "Visual memory hasn't been initialized yet.\n\n"
                "Click 'Start Visual Memory' in the View menu to begin capturing screenshots."
            )
            return

        captures = self.visual_memory.get_captures(limit=100)

        if not captures:
            QMessageBox.information(
                self,
                "No Captures",
                "No screenshots have been captured yet.\n\n"
                "Start visual memory and work for a while, then check back!"
            )
            return

        # Create simple browser dialog
        dialog = QDialog(self)
        dialog.setWindowTitle("Visual Memory Browser")
        dialog.setMinimumSize(800, 600)

        layout = QVBoxLayout(dialog)

        # Info label
        info = QLabel(f"Recent captures: {len(captures)}")
        layout.addWidget(info)

        # List widget for captures
        capture_list = QListWidget()
        capture_list.setIconSize(QSize(300, 200))
        layout.addWidget(capture_list)

        # Preview area
        preview_layout = QHBoxLayout()

        # Thumbnail preview
        preview_label = QLabel("Select a capture to preview")
        preview_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        preview_label.setMinimumSize(400, 300)
        preview_label.setStyleSheet("border: 1px solid #ccc; background: #f5f5f5;")
        preview_layout.addWidget(preview_label)

        # Details
        details_text = QTextEdit()
        details_text.setReadOnly(True)
        details_text.setMaximumWidth(300)
        preview_layout.addWidget(details_text)

        layout.addLayout(preview_layout)

        # Populate list
        for capture in captures:
            from datetime import datetime
            time_str = datetime.fromtimestamp(capture.timestamp).strftime("%Y-%m-%d %H:%M:%S")

            # Load thumbnail
            if capture.thumbnail_path and os.path.exists(capture.thumbnail_path):
                from PyQt6.QtGui import QPixmap
                pixmap = QPixmap(capture.thumbnail_path)
                icon = QIcon(pixmap)
            else:
                icon = QIcon()

            item = QListWidgetItem(icon, f"{time_str} - {capture.summary[:50]}")
            item.setData(Qt.ItemDataRole.UserRole, capture)
            capture_list.addItem(item)

        # Show preview on selection
        def show_preview(item):
            capture = item.data(Qt.ItemDataRole.UserRole)
            if not capture:
                return

            # Show image
            if capture.image_path and os.path.exists(capture.image_path):
                from PyQt6.QtGui import QPixmap
                pixmap = QPixmap(capture.image_path)
                scaled = pixmap.scaled(
                    400, 300,
                    Qt.AspectRatioMode.KeepAspectRatio,
                    Qt.TransformationMode.SmoothTransformation
                )
                preview_label.setPixmap(scaled)
            else:
                preview_label.setText("Image not found")

            # Show details
            from datetime import datetime
            time_str = datetime.fromtimestamp(capture.timestamp).strftime("%Y-%m-%d %H:%M:%S")

            details = f"""Timestamp: {time_str}
Active Window: {capture.active_window}
App: {capture.app_name}

Summary:
{capture.summary}

OCR Text:
{capture.ocr_text[:200]}...
"""
            details_text.setPlainText(details)

        capture_list.itemClicked.connect(show_preview)

        # Close button
        close_btn = QPushButton("Close")
        close_btn.clicked.connect(dialog.accept)
        layout.addWidget(close_btn)

        dialog.exec()


def main():
    app = QApplication(sys.argv)
    app.setApplicationName("Markdown Editor")
    app.setStyle("Fusion")

    editor = MarkdownEditor()
    editor.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()

