#!/usr/bin/env python3
"""
Markdown Editor
A clean markdown editor with toggle between edit and preview modes.
"""

import sys
import os
import json
import re
from pathlib import Path

from PyQt6.QtCore import Qt, QFileSystemWatcher, QTimer, QRegularExpression, QDateTime, QThread, pyqtSignal
from PyQt6.QtGui import (
    QAction, QKeySequence, QFont, QTextDocument, QTextCursor, QCursor,
    QColor, QSyntaxHighlighter, QTextCharFormat, QBrush
)
from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QTabWidget, QFileDialog,
    QMessageBox, QWidget, QVBoxLayout, QHBoxLayout, QTextBrowser,
    QDialog, QSpinBox, QDialogButtonBox, QFormLayout,
    QCheckBox, QLineEdit, QPushButton, QPlainTextEdit, QStackedWidget,
    QLabel, QDockWidget, QListWidget, QListWidgetItem, QToolTip,
    QComboBox, QDateTimeEdit, QScrollArea
)
from PyQt6.QtPrintSupport import QPrinter, QPrintDialog
from PyQt6.QtGui import QPageLayout, QPageSize
from PyQt6.QtCore import QMarginsF

from ide.commands import CommandRegistry, Command
from ide.context import IDEContext
from ide.document import Document
from ide.events import EventsSpine
from ide.kernel import init_kernel, get_kernel
from ide.ai import LocalAIClient, HybridAIClient
from ide.lexicon import LocalLexicon
from ide.tasks import TaskExtractor, TaskIndexStore

try:
    from PyQt6.QtWebEngineWidgets import QWebEngineView
except Exception:
    QWebEngineView = None
try:
    import markdown as markdown_lib
except Exception:
    markdown_lib = None
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


class LLMWorker(QThread):
    """Background worker for LLM API calls."""
    finished = pyqtSignal(str)
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

    # Slash commands mapped to provider actions
    SLASH_COMMANDS = {
        "/intent": ("intent_map", "Generate Intent Map"),
        "/cite": ("citation_helper", "Find claims without citations"),
        "/diff": ("diff_narrator", "Describe changes since last save"),
        "/outline": ("outline_enhancer", "Suggest outline improvements"),
        "/actions": ("action_extractor", "Extract action items"),
        "/typos": ("typo_fixer", "Fix typos in document"),
        "/suggest": ("suggestions", "Get text suggestions"),
        "/complete": ("code_complete", "Complete code at cursor"),
        "/explain": ("code_explain", "Explain selected code"),
        "/edit": ("code_edit", "Edit code with instruction"),
        "/generate": ("code_generate", "Generate code from description"),
        "/history": (None, "Show spine event history (episodic memory)"),
        "/kernel": (None, "Show kernel state (contexts, events)"),
        "/manifest": (None, "Show/edit document manifest (permissions)"),
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

        # Provider quick actions row
        provider_row = QHBoxLayout()
        provider_row.setSpacing(4)

        provider_label = QLabel("Quick:")
        provider_label.setStyleSheet("color: #666; font-size: 11px;")
        provider_row.addWidget(provider_label)

        self.intent_btn = QPushButton("Intent")
        self.intent_btn.setToolTip("Generate Intent Map of document")
        self.intent_btn.setMaximumWidth(60)
        self.intent_btn.clicked.connect(lambda: self._run_provider("intent_map"))
        provider_row.addWidget(self.intent_btn)

        self.explain_btn = QPushButton("Explain")
        self.explain_btn.setToolTip("Explain selected code/text")
        self.explain_btn.setMaximumWidth(60)
        self.explain_btn.clicked.connect(lambda: self._run_provider("code_explain"))
        provider_row.addWidget(self.explain_btn)

        self.complete_btn = QPushButton("Complete")
        self.complete_btn.setToolTip("Complete code at cursor")
        self.complete_btn.setMaximumWidth(70)
        self.complete_btn.clicked.connect(lambda: self._run_provider("code_complete"))
        provider_row.addWidget(self.complete_btn)

        self.actions_btn = QPushButton("Actions")
        self.actions_btn.setToolTip("Extract action items")
        self.actions_btn.setMaximumWidth(60)
        self.actions_btn.clicked.connect(lambda: self._run_provider("action_extractor"))
        provider_row.addWidget(self.actions_btn)

        provider_row.addStretch()
        layout.addLayout(provider_row)

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

        # Context buttons row
        context_row = QHBoxLayout()
        context_row.setSpacing(4)

        self.include_selection_btn = QPushButton("+ Selection")
        self.include_selection_btn.setToolTip("Include selected text in your message")
        self.include_selection_btn.setCheckable(True)
        self.include_selection_btn.setMaximumWidth(100)
        context_row.addWidget(self.include_selection_btn)

        self.include_doc_btn = QPushButton("+ Document")
        self.include_doc_btn.setToolTip("Include full document in your message")
        self.include_doc_btn.setCheckable(True)
        self.include_doc_btn.setMaximumWidth(100)
        context_row.addWidget(self.include_doc_btn)

        self.clear_btn = QPushButton("Clear")
        self.clear_btn.setMaximumWidth(50)
        self.clear_btn.clicked.connect(self.clear_chat)
        context_row.addWidget(self.clear_btn)

        context_row.addStretch()
        layout.addLayout(context_row)

        # Input area
        input_row = QHBoxLayout()
        input_row.setSpacing(4)

        self.input_field = QPlainTextEdit()
        self.input_field.setPlaceholderText("Type message or /help for commands... (Ctrl+Enter to send)")
        self.input_field.setMaximumHeight(80)
        self.input_field.setFont(QFont("Segoe UI", 10))
        input_row.addWidget(self.input_field, stretch=1)

        self.send_btn = QPushButton("Send")
        self.send_btn.setMinimumWidth(60)
        self.send_btn.setMinimumHeight(40)
        self.send_btn.clicked.connect(self.send_message)
        input_row.addWidget(self.send_btn)

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

    def clear_chat(self):
        """Clear the chat history."""
        self.messages = []
        self.chat_display.clear()
        self._append_system_message("Chat cleared. Ready for new conversation.")

    def _run_provider(self, provider_name):
        """Run a provider and display the result in chat."""
        if not self.execute_provider_callback:
            self._append_error_message("Provider execution not available.")
            return

        self._start_loading()
        self.current_provider = provider_name

        # Run provider in background thread
        self.current_worker = LLMWorker(
            lambda _: self.execute_provider_callback(provider_name),
            None
        )
        self.current_worker.finished.connect(self._on_provider_response)
        self.current_worker.error.connect(self._on_provider_error)
        self.current_worker.start()

    def _on_provider_response(self, result):
        """Handle provider result."""
        self._stop_loading()
        if result:
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
            cmd = user_text.split()[0].lower()
            if cmd in self.SLASH_COMMANDS:
                self.input_field.clear()
                provider_name, desc = self.SLASH_COMMANDS[cmd]
                if cmd == "/help":
                    self._show_help()
                elif cmd == "/history":
                    self._show_history()
                elif cmd == "/kernel":
                    self._show_kernel()
                elif cmd == "/manifest":
                    self._show_manifest()
                elif provider_name:
                    self._append_user_message(user_text)
                    self._run_provider(provider_name)
                return

        # Build the full message with context
        full_message = user_text
        context_parts = []

        if self.get_context_callback:
            selection, document = self.get_context_callback()

            if self.include_selection_btn.isChecked() and selection:
                context_parts.append(f"[Selected text]\n{selection}\n[/Selected text]")
                self.include_selection_btn.setChecked(False)

            if self.include_doc_btn.isChecked() and document:
                context_parts.append(f"[Document]\n{document}\n[/Document]")
                self.include_doc_btn.setChecked(False)

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

        # Build system prompt with lightweight document context
        system_prompt = "You are a helpful assistant integrated into a document editor. Help the user with writing, coding, analysis, and any questions they have. Be concise but thorough."

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
                "temperature": 0.7,
                "max_tokens": 2048,
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
    to the workspace. This is the "desktop" of the Semantic OS -
    you curate what's here, not everything on disk.
    """

    context_selected = pyqtSignal(str)  # Emits file path
    context_added = pyqtSignal(str)     # Emits file path

    def __init__(self, parent=None):
        super().__init__(parent)
        self._contexts = {}  # path -> {name, status, context_id}

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

        # Header
        header = QHBoxLayout()
        title = QLabel("Workspace")
        title.setStyleSheet("font-weight: bold; font-size: 12px;")
        header.addWidget(title)
        header.addStretch()

        add_btn = QPushButton("+")
        add_btn.setMaximumWidth(30)
        add_btn.setToolTip("Add context to workspace")
        add_btn.clicked.connect(self._add_context)
        header.addWidget(add_btn)
        layout.addLayout(header)

        # Context list
        self.context_list = QListWidget()
        self.context_list.setStyleSheet("""
            QListWidget {
                border: 1px solid #ccc;
                border-radius: 4px;
            }
            QListWidget::item {
                padding: 6px;
                border-bottom: 1px solid #eee;
            }
            QListWidget::item:hover {
                background: #f0f0f0;
            }
            QListWidget::item:selected {
                background: #e0e0ff;
            }
        """)
        self.context_list.itemDoubleClicked.connect(self._on_item_double_clicked)
        layout.addWidget(self.context_list)

        # Status bar
        self.status_label = QLabel("0 contexts")
        self.status_label.setStyleSheet("color: #666; font-size: 10px;")
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

    def add_context(self, path, context_id=None, status="closed"):
        """Register a context in the workspace."""
        from pathlib import Path
        name = Path(path).name
        self._contexts[path] = {
            "name": name,
            "status": status,
            "context_id": context_id
        }
        self._refresh_list()

    def update_context_status(self, path, status, context_id=None):
        """Update a context's status (open, modified, closed)."""
        if path in self._contexts:
            self._contexts[path]["status"] = status
            if context_id:
                self._contexts[path]["context_id"] = context_id
            self._refresh_list()

    def remove_context(self, path):
        """Remove a context from the workspace."""
        if path in self._contexts:
            del self._contexts[path]
            self._refresh_list()

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


class DesktopView(QWidget):
    """Desktop home screen for the Semantic OS.

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
        title = QLabel("Semantic OS")
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

        # Mode: "view", "calibrate", "measure"
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

                self.scale_label.setText(f"Scale: {self.scale_factor:.2f} px/{unit}")
                self.status_label.setText(f"Calibrated: {dist} {unit} = {pixel_dist:.1f}px")

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
            self.status_label.setText("Click first point on a known dimension...")
        else:
            self.tool_mode = "view"

    def toggle_measure(self):
        """Toggle measurement mode."""
        if self.measure_btn.isChecked():
            self.tool_mode = "measure"
            self.measure_points = []
            self.calibrate_btn.setChecked(False)
            if not self.scale_factor:
                self.status_label.setText("Measuring in pixels (calibrate for real units)")
            else:
                self.status_label.setText("Click first point to measure...")
        else:
            self.tool_mode = "view"

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
        self.dark_mode = dark_mode
        self.edit_mode = False
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
        self.highlighter = MarkdownHighlighter(self.editor.document(), dark_mode)
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
        ]
        self.context_menu_config = {
            "enabled": {"quick_stats", "lookup_definition", "show_synonyms"},
        }
        self.workspace_root = Path(__file__).resolve().parent
        self.workspace_config = {}
        self.load_settings()

        # Initialize kernel (Semantic OS core)
        self.kernel = init_kernel()
        self.kernel.spine.event_broadcast.connect(self._on_kernel_event)

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

        # Central stacked widget: Desktop view + Tabs
        self.central_stack = QStackedWidget()

        # Desktop view (shown when no files open)
        self.desktop_view = DesktopView()
        self.desktop_view.file_requested.connect(self.open_file)
        self.desktop_view.new_file_requested.connect(self.new_file)
        self.central_stack.addWidget(self.desktop_view)

        # Tab widget for documents
        self.tabs = QTabWidget()
        self.tabs.setTabsClosable(True)
        self.tabs.tabCloseRequested.connect(self.close_tab)
        self.tabs.setDocumentMode(True)
        self.tabs.currentChanged.connect(self.on_tab_changed)
        self.central_stack.addWidget(self.tabs)

        self.setCentralWidget(self.central_stack)

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
        """Show the desktop view."""
        self.central_stack.setCurrentWidget(self.desktop_view)
        self.desktop_view.refresh_icons()
        self.setWindowTitle("Semantic OS")

    def show_editor(self):
        """Show the tabs/editor view."""
        self.central_stack.setCurrentWidget(self.tabs)

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
        export_action.setShortcut(QKeySequence("Ctrl+Shift+P"))
        export_action.triggered.connect(self.export_pdf_current)
        file_menu.addAction(export_action)

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

        view_menu.addSeparator()

        settings_action = QAction("&Settings...", self)
        settings_action.setShortcut(QKeySequence("Ctrl+,"))
        settings_action.triggered.connect(self.show_settings)
        view_menu.addAction(settings_action)

    def setup_side_panels(self):
        # Context Launcher - the "desktop" of the Semantic OS
        self.workspace_dock = QDockWidget("Workspace", self)
        self.context_launcher = ContextLauncher()
        self.context_launcher.context_selected.connect(self._on_context_selected)
        self.context_launcher.context_added.connect(self._on_context_added)
        self.workspace_dock.setWidget(self.context_launcher)
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.workspace_dock)
        self.workspace_dock.setMinimumWidth(200)

        self.outline_dock = QDockWidget("Outline", self)
        self.outline_list = QListWidget()
        self.outline_list.itemActivated.connect(self.jump_to_heading)
        self.outline_dock.setWidget(self.outline_list)
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.outline_dock)
        self.outline_dock.setVisible(False)

        self.word_count_dock = QDockWidget("Word Count", self)
        self.word_count_label = QLabel("Words: 0  Characters: 0")
        self.word_count_label.setContentsMargins(12, 8, 12, 8)
        self.word_count_dock.setWidget(self.word_count_label)
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self.word_count_dock)
        self.word_count_dock.setVisible(False)

        self.tasks_dock = QDockWidget("Tasks", self)
        tasks_widget = QWidget()
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

    def _execute_chat_provider(self, provider_name):
        """Execute a provider from the chat panel and return the result."""
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
            )
            self._replace_tab(index, edit_tab, Path(tab.file_path).name, tab.file_path)
            edit_tab.outline_list = self.outline_dock.widget()
            edit_tab.word_count_label = self.word_count_dock.widget()
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
        )
        index = self.tabs.addTab(tab, "Untitled")
        self.tabs.setCurrentIndex(index)
        self.outline_list = self.outline_dock.widget()
        self.word_count_label = self.word_count_dock.widget()
        tab.outline_list = self.outline_list
        tab.word_count_label = self.word_count_label
        tab.refresh_outline()
        tab.refresh_word_count()
        tab.events.document_saved.connect(self.on_document_saved)
        tab.events.document_opened.connect(self.on_document_opened)
        tab.events.document_closed.connect(self.on_document_closed)
        self.register_task_commands(tab)

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
            self.word_count_label = self.word_count_dock.widget()
            tab.outline_list = self.outline_list
            tab.word_count_label = self.word_count_label
            tab.refresh_outline()
            tab.refresh_word_count()
            self.reader_action.setChecked(False)
        elif isinstance(tab, ReaderTab):
            name = Path(tab.file_path).name if tab.file_path else "Reader"
            self.setWindowTitle(f"Markdown Editor - {name}")
            self.reader_action.setChecked(True)

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
            )
        name = Path(file_path).name
        index = self.tabs.addTab(tab, name)
        self.tabs.setCurrentIndex(index)
        self.tabs.setTabToolTip(index, file_path)
        self.file_watcher.addPath(file_path)
        self.outline_list = self.outline_dock.widget()
        self.word_count_label = self.word_count_dock.widget()
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

        # If no tabs left, show desktop
        if self.tabs.count() == 0:
            self.show_desktop()

    def close_current_tab(self):
        self.close_tab(self.tabs.currentIndex())

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


def main():
    app = QApplication(sys.argv)
    app.setApplicationName("Markdown Editor")
    app.setStyle("Fusion")

    editor = MarkdownEditor()
    editor.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()

