"""
Bulk File Import System
========================

Quickly ingest entire directory trees into semantic memory.

Features:
- Recursive directory scanning
- File type filtering
- Progress tracking
- Smart parsing based on file type
- Repository-aware analysis
- Background processing
"""

import os
import time
import hashlib
from pathlib import Path
from typing import List, Set, Tuple, Optional, Callable
from dataclasses import dataclass
from PyQt6.QtCore import QThread, pyqtSignal, QObject
from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel,
    QPushButton, QProgressBar, QListWidget, QCheckBox,
    QLineEdit, QFileDialog, QGroupBox, QFormLayout,
    QTextEdit, QSpinBox, QComboBox
)


@dataclass
class ImportStats:
    """Statistics for bulk import operation."""
    total_files: int = 0
    successful: int = 0
    skipped: int = 0
    failed: int = 0
    total_size: int = 0
    processed_size: int = 0
    start_time: float = 0
    end_time: float = 0

    def duration(self) -> float:
        """Get duration in seconds."""
        if self.end_time > 0:
            return self.end_time - self.start_time
        return time.time() - self.start_time

    def progress_percent(self) -> float:
        """Get progress percentage."""
        if self.total_files == 0:
            return 0
        return (self.processed_size / self.total_size) * 100


class FileScanner:
    """Scans directories for files matching criteria."""

    # File categories with extensions
    CATEGORIES = {
        "Code": [
            ".py", ".js", ".ts", ".jsx", ".tsx",
            ".java", ".c", ".cpp", ".h", ".hpp",
            ".cs", ".go", ".rs", ".swift", ".kt"
        ],
        "Web": [
            ".html", ".htm", ".css", ".scss", ".sass",
            ".vue", ".svelte", ".jsx", ".tsx"
        ],
        "Data": [
            ".json", ".yaml", ".yml", ".xml", ".csv",
            ".toml", ".ini", ".conf", ".config"
        ],
        "Docs": [
            ".md", ".markdown", ".rst", ".txt",
            ".pdf", ".doc", ".docx"
        ],
        "Scripts": [
            ".sh", ".bash", ".zsh", ".fish",
            ".bat", ".cmd", ".ps1"
        ]
    }

    @classmethod
    def get_all_extensions(cls) -> Set[str]:
        """Get all supported file extensions."""
        exts = set()
        for category_exts in cls.CATEGORIES.values():
            exts.update(category_exts)
        return exts

    @classmethod
    def scan_directory(
        cls,
        root_path: str,
        extensions: Optional[Set[str]] = None,
        max_depth: int = 10,
        max_files: int = 10000,
        exclude_dirs: Optional[Set[str]] = None,
        progress_callback: Optional[Callable] = None
    ) -> List[str]:
        """Scan directory for files matching criteria.

        Args:
            root_path: Root directory to scan
            extensions: File extensions to include (None = all)
            max_depth: Maximum directory depth
            max_files: Maximum files to return
            exclude_dirs: Directory names to exclude
            progress_callback: Called with (current, total, path)

        Returns:
            List of file paths
        """
        if exclude_dirs is None:
            exclude_dirs = {
                "__pycache__", "node_modules", ".git", ".svn",
                "venv", "env", ".venv", ".env",
                "build", "dist", "target", "bin", "obj",
                ".vscode", ".idea", "coverage",
                ".pytest_cache", ".mypy_cache"
            }

        files = []
        root = Path(root_path)

        if not root.exists() or not root.is_dir():
            return files

        def scan_dir(current_dir: Path, current_depth: int):
            """Recursively scan directory."""
            if len(files) >= max_files:
                return

            if current_depth > max_depth:
                return

            try:
                for item in current_dir.iterdir():
                    # Skip excluded directories
                    if item.is_dir() and item.name in exclude_dirs:
                        continue

                    # Recurse into subdirectories
                    if item.is_dir():
                        scan_dir(item, current_depth + 1)
                        continue

                    # Check file extension
                    if item.is_file():
                        if extensions is None or item.suffix.lower() in extensions:
                            files.append(str(item))
                            if progress_callback:
                                progress_callback(len(files), max_files, str(item))

            except PermissionError:
                # Skip directories we can't read
                pass

        scan_dir(root, 0)
        return files


# Use the new SmartImporter
from ide.smart_import import SmartImporter


class BulkImportDialog(QDialog):
    """Dialog for bulk file import."""

    def __init__(self, kernel, parent=None):
        super().__init__(parent)
        self.kernel = kernel
        self.importer: Optional[SmartImporter] = None
        self.setup_ui()

    def setup_ui(self):
        """Setup the UI."""
        self.setWindowTitle("Bulk Import Files")
        self.setMinimumSize(700, 600)

        layout = QVBoxLayout(self)

        # Instructions
        instructions = QLabel(
            "Import entire directories into semantic memory.\n"
            "MarlOS will analyze your repo structure and intelligently import files."
        )
        instructions.setWordWrap(True)
        layout.addWidget(instructions)

        layout.addSpacing(10)

        # Directory selection
        dir_group = QGroupBox("Source Directory")
        dir_layout = QHBoxLayout()

        self.dir_input = QLineEdit()
        self.dir_input.setPlaceholderText("/path/to/your/project")
        dir_layout.addWidget(self.dir_input)

        browse_btn = QPushButton("Browse...")
        browse_btn.clicked.connect(self.browse_directory)
        dir_layout.addWidget(browse_btn)

        dir_group.setLayout(dir_layout)
        layout.addWidget(dir_group)

        # Options
        options_group = QGroupBox("Import Options")
        options_layout = QFormLayout()

        # File types
        self.file_type_combo = QComboBox()
        self.file_type_combo.addItems([
            "All Code Files",
            "Python Only",
            "Web Files (HTML/CSS/JS)",
            "Documentation (Markdown)",
            "Config Files (JSON/YAML)",
            "All Files"
        ])
        self.file_type_combo.setCurrentIndex(0)
        options_layout.addRow("File Types:", self.file_type_combo)

        # Max depth
        self.max_depth = QSpinBox()
        self.max_depth.setRange(1, 100)
        self.max_depth.setValue(50)  # High default = scan everything
        self.max_depth.setToolTip("Maximum directory depth to scan (50 = practically unlimited)")
        options_layout.addRow("Max Depth:", self.max_depth)

        options_group.setLayout(options_layout)
        layout.addWidget(options_group)

        # Preview section
        preview_group = QGroupBox("File Preview")
        preview_layout = QVBoxLayout()

        self.file_list = QListWidget()
        self.file_list.setMinimumHeight(150)
        preview_layout.addWidget(self.file_list)

        scan_btn = QPushButton("Scan Directory")
        scan_btn.clicked.connect(self.scan_directory)
        preview_layout.addWidget(scan_btn)

        preview_group.setLayout(preview_layout)
        layout.addWidget(preview_group)

        # Progress section
        progress_group = QGroupBox("Import Progress")
        progress_layout = QVBoxLayout()

        self.progress_bar = QProgressBar()
        self.progress_bar.setMinimumHeight(25)
        progress_layout.addWidget(self.progress_bar)

        self.status_label = QLabel("Ready to import")
        progress_layout.addWidget(self.status_label)

        self.log_text = QTextEdit()
        self.log_text.setReadOnly(True)
        self.log_text.setMaximumHeight(150)
        progress_layout.addWidget(self.log_text)

        progress_group.setLayout(progress_layout)
        layout.addWidget(progress_group)

        # Buttons
        button_layout = QHBoxLayout()

        self.import_btn = QPushButton("Import Files")
        self.import_btn.setEnabled(False)
        self.import_btn.setMinimumHeight(40)
        self.import_btn.clicked.connect(self.start_import)
        button_layout.addWidget(self.import_btn)

        self.stop_btn = QPushButton("Stop")
        self.stop_btn.setEnabled(False)
        self.stop_btn.clicked.connect(self.stop_import)
        button_layout.addWidget(self.stop_btn)

        self.close_btn = QPushButton("Close")
        self.close_btn.clicked.connect(self.accept)
        button_layout.addWidget(self.close_btn)

        layout.addLayout(button_layout)

        # Store scanned files
        self.scanned_files: List[str] = []

    def browse_directory(self):
        """Browse for directory."""
        dir_path = QFileDialog.getExistingDirectory(
            self,
            "Select Directory to Import",
            ""
        )

        if dir_path:
            self.dir_input.setText(dir_path)

    def scan_directory(self):
        """Scan directory for files."""
        dir_path = self.dir_input.text().strip()

        if not dir_path or not os.path.isdir(dir_path):
            self.log_message("Error: Invalid directory")
            return

        self.log_message(f"Scanning {dir_path}...")

        # Get file extensions based on selection
        file_type = self.file_type_combo.currentText()
        extensions = None

        if file_type == "Python Only":
            extensions = {".py"}
        elif file_type == "Web Files (HTML/CSS/JS)":
            extensions = FileScanner.CATEGORIES["Web"]
        elif file_type == "Documentation (Markdown)":
            extensions = {".md", ".markdown", ".rst"}
        elif file_type == "Config Files (JSON/YAML)":
            extensions = FileScanner.CATEGORIES["Data"]
        elif file_type == "All Code Files":
            extensions = FileScanner.CATEGORIES["Code"] | FileScanner.CATEGORIES["Scripts"]

        # Scan directory
        max_depth = self.max_depth.value()

        try:
            files = FileScanner.scan_directory(
                dir_path,
                extensions=extensions,
                max_depth=max_depth,
                progress_callback=lambda cur, tot, path: self.update_scan_progress(cur, tot, path)
            )

            self.scanned_files = files
            self.file_list.clear()

            for file_path in files:
                self.file_list.addItem(Path(file_path).name)

            self.log_message(f"Found {len(files)} files")
            self.import_btn.setEnabled(len(files) > 0)

        except Exception as e:
            self.log_message(f"Error scanning: {str(e)}")

    def update_scan_progress(self, current: int, total: int, path: str):
        """Update scan progress."""
        self.status_label.setText(f"Scanning: {current} files found...")
        self.log_message(f"Found: {Path(path).name}")

    def start_import(self):
        """Start the import process."""
        if not self.scanned_files:
            self.log_message("No files to import")
            return

        self.log_message(f"Starting smart import of {len(self.scanned_files)} files...")

        # Disable controls
        self.import_btn.setEnabled(False)
        self.stop_btn.setEnabled(True)
        self.dir_input.setEnabled(False)
        self.file_type_combo.setEnabled(False)

        # Clear log
        self.log_text.clear()

        # Create smart importer thread
        self.importer = SmartImporter(self.kernel, self.scanned_files)

        # Connect signals
        self.importer.progress.connect(self.on_import_progress)
        self.importer.file_complete.connect(self.on_file_complete)
        self.importer.finished.connect(self.on_import_finished)

        # Start import
        self.importer.start()

    def stop_import(self):
        """Stop the import process."""
        if self.importer:
            self.importer.stop()
            self.log_message("Stopping import...")

    def on_import_progress(self, current: int, total: int, file_path: str):
        """Handle import progress."""
        percent = int((current / total) * 100)
        self.progress_bar.setValue(percent)
        self.status_label.setText(f"Importing: {current}/{total} ({percent}%)")

    def on_file_complete(self, file_path: str, success: bool, message: str):
        """Handle file completion."""
        status = "✓" if success else "⊘"
        icon = Path(file_path).name
        self.log_message(f"{status} {icon}: {message}")

    def on_import_finished(self, stats: dict):
        """Handle import completion."""
        duration = stats.get("duration", 0)
        successful = stats.get("successful", 0)
        total = stats.get("total", 0)
        skipped = stats.get("skipped", 0)
        failed = stats.get("failed", 0)
        strategies_used = stats.get("strategies_used", {})

        rate = successful / duration if duration > 0 else 0

        summary = f"""
Smart Import Complete!
──────────────────────
Files scanned: {total}
Successful: {successful}
Skipped: {skipped}
Failed: {failed}
Duration: {duration:.1f}s
Rate: {rate:.1f} files/second

Strategies Used:
"""
        for strategy, count in strategies_used.items():
            if "Strategy" not in strategy:
                summary += f"  {strategy}: {count}\n"

        self.log_message(summary)
        self.status_label.setText("Import complete!")
        self.progress_bar.setValue(100)

        # Re-enable controls
        self.import_btn.setEnabled(True)
        self.stop_btn.setEnabled(False)
        self.dir_input.setEnabled(True)
        self.file_type_combo.setEnabled(True)

    def log_message(self, message: str):
        """Add message to log."""
        self.log_text.append(message)


def show_bulk_import_dialog(kernel, parent=None):
    """Show the bulk import dialog."""
    dialog = BulkImportDialog(kernel, parent)
    dialog.exec()

