"""
Bulk File Import System
========================

Quickly ingest entire directory trees into semantic memory.

Features:
- Recursive directory scanning
- File type filtering
- Progress tracking
- Chunked ingestion for large files
- Incremental updates (skip unchanged)
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


class FileImporter(QThread):
    """Background thread for importing files into semantic memory."""

    # Signals
    progress = pyqtSignal(int, int, str)  # current, total, current_file
    file_complete = pyqtSignal(str, bool, str)  # file_path, success, message
    finished = pyqtSignal(object)  # ImportStats
    error = pyqtSignal(str)  # error_message

    def __init__(
        self,
        kernel,
        files: List[str],
        chunk_size: int = 10000,
        skip_unchanged: bool = True,
        parent=None
    ):
        super().__init__(parent)
        self.kernel = kernel
        self.files = files
        self.chunk_size = chunk_size
        self.skip_unchanged = skip_unchanged
        self._should_stop = False
        self.stats = ImportStats()
        self.stats.total_files = len(files)
        self.stats.total_size = sum(os.path.getsize(f) for f in files if os.path.exists(f))

    def stop(self):
        """Stop the import process."""
        self._should_stop = True

    def _get_file_hash(self, file_path: str) -> str:
        """Get hash of file for change detection."""
        try:
            with open(file_path, 'rb') as f:
                # Read first and last 4KB for quick hash
                start = f.read(4096)
                f.seek(-4096, 2)
                end = f.read()
                return hashlib.md5(start + end).hexdigest()
        except Exception:
            return ""

    def _should_import_file(self, file_path: str) -> Tuple[bool, str]:
        """Check if file should be imported (not unchanged)."""
        if not self.skip_unchanged:
            return True, "Importing"

        # Check if already in memory and unchanged
        try:
            doc_id = f"doc_{file_path.replace('/', '_').replace('.', '_').replace(':', '_')}"
            existing = self.kernel.memory.get(doc_id)

            if existing:
                current_hash = self._get_file_hash(file_path)
                stored_hash = existing.metadata.get("file_hash", "")

                if current_hash == stored_hash:
                    return False, "Unchanged"

        except Exception:
            pass

        return True, "New or modified"

    def _ingest_file(self, file_path: str) -> Tuple[bool, str]:
        """Import a single file into semantic memory."""
        try:
            # Check if we should import
            should_import, reason = self._should_import_file(file_path)
            if not should_import:
                return False, reason

            # Read file
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read()

            # Get file hash
            file_hash = self._get_file_hash(file_path)
            file_size = len(content)

            # Determine if Python file (for hierarchical parsing)
            is_python = file_path.endswith('.py')

            # Import based on file type and size
            if is_python and file_size > 1000:
                # Python files: use hierarchical parsing
                self._ingest_python_file(file_path, content, file_hash)
            elif file_size > self.chunk_size:
                # Large files: chunk and import
                self._ingest_chunked_file(file_path, content, file_hash)
            else:
                # Small files: import directly
                self._ingest_simple_file(file_path, content, file_hash)

            file_size = os.path.getsize(file_path)
            self.stats.processed_size += file_size

            return True, "Imported"

        except Exception as e:
            return False, str(e)

    def _ingest_simple_file(self, file_path: str, content: str, file_hash: str):
        """Import a small file directly."""
        import ast

        doc_id = f"doc_{file_path.replace('/', '_').replace('.', '_').replace(':', '_')}"

        # Check if already exists
        existing = self.kernel.memory.get(doc_id)
        if existing:
            self.kernel.memory.update(
                doc_id,
                content=content,
                metadata={
                    "path": file_path,
                    "file_hash": file_hash,
                    "file_size": len(content),
                    "last_modified": os.path.getmtime(file_path)
                }
            )
        else:
            self.kernel.memory.store(
                content=content,
                type="document",
                metadata={
                    "path": file_path,
                    "file_hash": file_hash,
                    "file_size": len(content),
                    "last_modified": os.path.getmtime(file_path)
                },
                id=doc_id,
            )

    def _ingest_python_file(self, file_path: str, content: str, file_hash: str):
        """Import a Python file with hierarchical structure."""
        import ast

        try:
            tree = ast.parse(content)
        except Exception:
            # If parsing fails, fall back to simple import
            self._ingest_simple_file(file_path, content, file_hash)
            return

        # Count classes and functions
        classes = [node for node in tree.body if isinstance(node, ast.ClassDef)]
        functions = [node for node in tree.body if isinstance(node, ast.FunctionDef)]

        # File-level summary
        file_info = (
            f"Python file: {file_path}\n"
            f"Classes: {len(classes)}\n"
            f"Functions: {len(functions)}\n"
            f"Lines: {len(content.splitlines())}"
        )

        doc_id = f"doc_{file_path.replace('/', '_').replace('.', '_').replace(':', '_')}"

        # Store file summary
        self.kernel.memory.store(
            content=file_info,
            type="document",
            metadata={
                "path": file_path,
                "file_hash": file_hash,
                "file_size": len(content),
                "last_modified": os.path.getmtime(file_path),
                "classes": [c.name for c in classes],
                "functions": [f.name for f in functions],
                "language": "python"
            },
            id=doc_id,
        )

        # Store classes
        for cls in classes:
            class_code = ast.get_source_segment(content, cls)
            if class_code:
                self.kernel.memory.store(
                    content=f"Class: {cls.name}\n\n{class_code}",
                    type="chunk",
                    metadata={
                        "path": file_path,
                        "type": "class",
                        "name": cls.name,
                        "file_hash": file_hash,
                    }
                )

        # Store functions
        for func in functions:
            func_code = ast.get_source_segment(content, func)
            if func_code:
                self.kernel.memory.store(
                    content=f"Function: {func.name}\n\n{func_code}",
                    type="chunk",
                    metadata={
                        "path": file_path,
                        "type": "function",
                        "name": func.name,
                        "file_hash": file_hash,
                    }
                )

    def _ingest_chunked_file(self, file_path: str, content: str, file_hash: str):
        """Import a large file by chunking."""
        chunks = []
        lines = content.splitlines()
        chunk_lines = []
        chunk_num = 0

        for i, line in enumerate(lines):
            chunk_lines.append(line)

            # Create chunk every N lines
            if len(chunk_lines) >= 500:
                chunk_content = "\n".join(chunk_lines)
                chunks.append(chunk_content)
                chunk_lines = []
                chunk_num += 1

        # Add remaining lines
        if chunk_lines:
            chunks.append("\n".join(chunk_lines))

        # Store chunks
        for i, chunk in enumerate(chunks):
            self.kernel.memory.store(
                content=f"Chunk {i+1}/{len(chunks)}\n\n{chunk}",
                type="chunk",
                metadata={
                    "path": file_path,
                    "chunk_index": i,
                    "total_chunks": len(chunks),
                    "file_hash": file_hash,
                }
            )

        # Store file reference
        doc_id = f"doc_{file_path.replace('/', '_').replace('.', '_').replace(':', '_')}"
        self.kernel.memory.store(
            content=f"File: {file_path}\nChunks: {len(chunks)}\nTotal lines: {len(lines)}",
            type="document",
            metadata={
                "path": file_path,
                "file_hash": file_hash,
                "file_size": len(content),
                "last_modified": os.path.getmtime(file_path),
                "chunked": True
            },
            id=doc_id
        )

    def run(self):
        """Run the import process."""
        self.stats.start_time = time.time()

        for i, file_path in enumerate(self.files):
            if self._should_stop:
                break

            # Emit progress
            self.progress.emit(i + 1, self.stats.total_files, file_path)

            # Import file
            success, message = self._ingest_file(file_path)

            if success:
                self.stats.successful += 1
                self.file_complete.emit(file_path, True, message)
            else:
                if message == "Unchanged":
                    self.stats.skipped += 1
                else:
                    self.stats.failed += 1
                self.file_complete.emit(file_path, False, message)

        self.stats.end_time = time.time()
        self.finished.emit(self.stats)


class BulkImportDialog(QDialog):
    """Dialog for bulk file import."""

    def __init__(self, kernel, parent=None):
        super().__init__(parent)
        self.kernel = kernel
        self.importer: Optional[FileImporter] = None
        self.setup_ui()

    def setup_ui(self):
        """Setup the UI."""
        self.setWindowTitle("Bulk Import Files")
        self.setMinimumSize(700, 600)

        layout = QVBoxLayout(self)

        # Instructions
        instructions = QLabel(
            "Import entire directories into semantic memory.\n"
            "MarlOS will analyze and index your files for intelligent search."
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
        self.max_depth.setRange(1, 50)
        self.max_depth.setValue(10)
        self.max_depth.setToolTip("Maximum directory depth to scan")
        options_layout.addRow("Max Depth:", self.max_depth)

        # Chunk size
        self.chunk_size = QSpinBox()
        self.chunk_size.setRange(1000, 100000)
        self.chunk_size.setValue(10000)
        self.chunk_size.setSingleStep(1000)
        self.chunk_size.setSuffix(" bytes")
        self.chunk_size.setToolTip("Files larger than this will be chunked")
        options_layout.addRow("Chunk Size:", self.chunk_size)

        # Skip unchanged
        self.skip_unchanged = QCheckBox("Skip unchanged files")
        self.skip_unchanged.setChecked(True)
        self.skip_unchanged.setToolTip("Files already in memory that haven't changed")
        options_layout.addRow("", self.skip_unchanged)

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

        self.log_message(f"Starting import of {len(self.scanned_files)} files...")

        # Disable controls
        self.import_btn.setEnabled(False)
        self.stop_btn.setEnabled(True)
        self.dir_input.setEnabled(False)
        self.file_type_combo.setEnabled(False)

        # Clear log
        self.log_text.clear()

        # Create importer thread
        self.importer = FileImporter(
            self.kernel,
            self.scanned_files,
            chunk_size=self.chunk_size.value(),
            skip_unchanged=self.skip_unchanged.isChecked()
        )

        # Connect signals
        self.importer.progress.connect(self.on_import_progress)
        self.importer.file_complete.connect(self.on_file_complete)
        self.importer.finished.connect(self.on_import_finished)
        self.importer.error.connect(self.on_import_error)

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

    def on_import_finished(self, stats: ImportStats):
        """Handle import completion."""
        duration = stats.duration()
        rate = stats.successful / duration if duration > 0 else 0

        summary = f"""
Import Complete!
───────────────
Files scanned: {stats.total_files}
Successful: {stats.successful}
Skipped: {stats.skipped}
Failed: {stats.failed}
Duration: {duration:.1f}s
Rate: {rate:.1f} files/second
"""

        self.log_message(summary)
        self.status_label.setText("Import complete!")
        self.progress_bar.setValue(100)

        # Re-enable controls
        self.import_btn.setEnabled(True)
        self.stop_btn.setEnabled(False)
        self.dir_input.setEnabled(True)
        self.file_type_combo.setEnabled(True)

    def on_import_error(self, error: str):
        """Handle import error."""
        self.log_message(f"Error: {error}")

    def log_message(self, message: str):
        """Add message to log."""
        self.log_text.append(message)


def show_bulk_import_dialog(kernel, parent=None):
    """Show the bulk import dialog."""
    dialog = BulkImportDialog(kernel, parent)
    dialog.exec()
