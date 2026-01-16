"""
Smart Import System
====================

Repository-aware file import with type-specific parsing strategies.

Features:
- Repository structure detection
- Type-specific import strategies
- Relationship extraction from repo structure
- Section-based markdown parsing
- Structured data import (JSON/YAML)
- Test-to-source linking
- Docs-to-code linking
"""

import os
import json
import re
import ast
import time
import hashlib
from pathlib import Path
from typing import List, Dict, Set, Tuple, Optional, Any
from dataclasses import dataclass, field
from collections import defaultdict
from abc import ABC, abstractmethod
from PyQt6.QtCore import QThread, pyqtSignal


@dataclass
class RepoInfo:
    """Information about a repository."""
    name: str
    root_path: str
    repo_type: str  # "python", "node", "rust", "generic", etc.
    structure: Dict[str, List[str]] = field(default_factory=dict)
    dependencies: List[str] = field(default_factory=list)
    entry_points: List[str] = field(default_factory=list)
    config_files: List[str] = field(default_factory=list)
    test_dirs: List[str] = field(default_factory=list)
    src_dirs: List[str] = field(default_factory=list)
    doc_dirs: List[str] = field(default_factory=list)

    # Detected patterns
    has_git: bool = False
    has_tests: bool = False
    has_docs: bool = False
    has_ci: bool = False

    def to_dict(self) -> Dict:
        """Convert to dictionary."""
        return {
            "name": self.name,
            "root_path": self.root_path,
            "repo_type": self.repo_type,
            "structure": self.structure,
            "dependencies": self.dependencies,
            "entry_points": self.entry_points,
            "config_files": self.config_files,
            "test_dirs": self.test_dirs,
            "src_dirs": self.src_dirs,
            "doc_dirs": self.doc_dirs,
            "has_git": self.has_git,
            "has_tests": self.has_tests,
            "has_docs": self.has_docs,
            "has_ci": self.has_ci,
        }


class RepositoryAnalyzer:
    """Analyzes repository structure and type."""

    # Known directory patterns
    SRC_PATTERNS = ["src", "source", "lib", "app", "backend", "frontend"]
    TEST_PATTERNS = ["tests", "test", "spec", "__tests__"]
    DOC_PATTERNS = ["docs", "doc", "documentation", "wiki"]
    CONFIG_PATTERNS = ["config", "conf", "settings", ".config"]

    # Known repo indicators
    PYTHON_INDICATORS = [
        "requirements.txt", "setup.py", "pyproject.toml",
        "Pipfile", "poetry.lock", "setup.cfg", "tox.ini"
    ]
    NODE_INDICATORS = [
        "package.json", "package-lock.json", "yarn.lock",
        "pnpm-lock.yaml", "node_modules"
    ]
    RUST_INDICATORS = [
        "Cargo.toml", "Cargo.lock"
    ]
    JAVA_INDICATORS = [
        "pom.xml", "build.gradle", "settings.gradle"
    ]
    GO_INDICATORS = [
        "go.mod", "go.sum"
    ]

    @classmethod
    def analyze(cls, root_path: str) -> RepoInfo:
        """Analyze a repository directory."""
        root = Path(root_path)
        name = root.name

        # Detect repo type
        repo_type = cls._detect_repo_type(root)

        # Scan structure
        structure = cls._scan_structure(root)

        # Extract dependencies
        dependencies = cls._extract_dependencies(root, repo_type)

        # Find entry points
        entry_points = cls._find_entry_points(root, repo_type)

        # Identify special directories
        src_dirs = cls._find_directories(root, cls.SRC_PATTERNS)
        test_dirs = cls._find_directories(root, cls.TEST_PATTERNS)
        doc_dirs = cls._find_directories(root, cls.DOC_PATTERNS)
        config_files = cls._find_config_files(root)

        # Detect features
        has_git = (root / ".git").exists()
        has_tests = len(test_dirs) > 0
        has_docs = len(doc_dirs) > 0
        has_ci = any((root / f".{ci}").exists() for ci in ["github", "gitlab", "travis", "circleci"])

        return RepoInfo(
            name=name,
            root_path=str(root),
            repo_type=repo_type,
            structure=structure,
            dependencies=dependencies,
            entry_points=entry_points,
            config_files=config_files,
            test_dirs=test_dirs,
            src_dirs=src_dirs,
            doc_dirs=doc_dirs,
            has_git=has_git,
            has_tests=has_tests,
            has_docs=has_docs,
            has_ci=has_ci
        )

    @classmethod
    def _detect_repo_type(cls, root: Path) -> str:
        """Detect the type of repository."""
        # Check for indicators
        for indicator in cls.PYTHON_INDICATORS:
            if (root / indicator).exists():
                return "python"

        for indicator in cls.NODE_INDICATORS:
            if (root / indicator).exists():
                return "node"

        for indicator in cls.RUST_INDICATORS:
            if (root / indicator).exists():
                return "rust"

        for indicator in cls.GO_INDICATORS:
            if (root / indicator).exists():
                return "go"

        for indicator in cls.JAVA_INDICATORS:
            if (root / indicator).exists():
                return "java"

        # Check for common source patterns
        if (root / "src").exists():
            # Has src/ directory, likely a project
            return "generic_project"

        return "generic"

    @classmethod
    def _scan_structure(cls, root: Path) -> Dict[str, List[str]]:
        """Scan directory structure."""
        structure = defaultdict(list)

        try:
            for item in root.iterdir():
                if item.is_dir():
                    # Recursively scan subdirectories (limited depth)
                    if not item.name.startswith("."):
                        structure[item.name] = cls._get_files_recursive(item, max_depth=2)
        except PermissionError:
            pass

        return dict(structure)

    @classmethod
    def _get_files_recursive(cls, directory: Path, max_depth: int = 2, current_depth: int = 0) -> List[str]:
        """Get files recursively up to max_depth."""
        if current_depth >= max_depth:
            return []

        files = []
        try:
            for item in directory.iterdir():
                if item.is_file():
                    files.append(item.name)
                elif item.is_dir() and not item.name.startswith("."):
                    files.extend(
                        f"{item.name}/{subfile}"
                        for subfile in cls._get_files_recursive(item, max_depth, current_depth + 1)
                    )
        except PermissionError:
            pass

        return files

    @classmethod
    def _extract_dependencies(cls, root: Path, repo_type: str) -> List[str]:
        """Extract dependencies based on repo type."""
        deps = []

        if repo_type == "python":
            # requirements.txt
            req_file = root / "requirements.txt"
            if req_file.exists():
                with open(req_file) as f:
                    for line in f:
                        line = line.strip()
                        if line and not line.startswith("#"):
                            # Extract package name
                            pkg = line.split(">")[0].split("<")[0].split("=")[0].split("[")[0].strip()
                            if pkg:
                                deps.append(pkg)

            # pyproject.toml
            pyproject = root / "pyproject.toml"
            if pyproject.exists():
                try:
                    import toml
                    with open(pyproject) as f:
                        data = toml.load(f)
                    if "project" in data and "dependencies" in data["project"]:
                        deps.extend(data["project"]["dependencies"])
                except Exception:
                    pass

        elif repo_type == "node":
            # package.json
            package_file = root / "package.json"
            if package_file.exists():
                try:
                    with open(package_file) as f:
                        data = json.load(f)
                    if "dependencies" in data:
                        deps.extend(data["dependencies"].keys())
                    if "devDependencies" in data:
                        deps.extend(data["devDependencies"].keys())
                except Exception:
                    pass

        return deps

    @classmethod
    def _find_entry_points(cls, root: Path, repo_type: str) -> List[str]:
        """Find entry points (main files)."""
        entries = []

        if repo_type == "python":
            # Look for __main__.py, main.py, app.py
            for entry in ["__main__.py", "main.py", "app.py", "run.py"]:
                if (root / entry).exists():
                    entries.append(entry)

        elif repo_type == "node":
            # Look for index.js, server.js, app.js
            for entry in ["index.js", "server.js", "app.js", "main.js"]:
                if (root / entry).exists():
                    entries.append(entry)

        # Check src/ directories
        if (root / "src").exists():
            src = root / "src"
            if repo_type == "python":
                for entry in ["__main__.py", "main.py"]:
                    if (src / entry).exists():
                        entries.append(f"src/{entry}")
            elif repo_type == "node":
                for entry in ["index.js", "server.js", "main.js"]:
                    if (src / entry).exists():
                        entries.append(f"src/{entry}")

        return entries

    @classmethod
    def _find_directories(cls, root: Path, patterns: List[str]) -> List[str]:
        """Find directories matching patterns."""
        found = []

        try:
            for item in root.iterdir():
                if item.is_dir() and item.name.lower() in [p.lower() for p in patterns]:
                    found.append(item.name)
        except PermissionError:
            pass

        return found

    @classmethod
    def _find_config_files(cls, root: Path) -> List[str]:
        """Find configuration files."""
        configs = []

        config_patterns = [
            "*.json", "*.yaml", "*.yml", "*.toml", "*.ini",
            "*.cfg", "*.conf", ".env*", "docker-compose.yml",
            "Dockerfile", "Makefile", "package.json"
        ]

        try:
            for item in root.iterdir():
                if item.is_file():
                    for pattern in config_patterns:
                        if item.match(pattern):
                            configs.append(item.name)
                            break
        except PermissionError:
            pass

        return configs


class ImportStrategy(ABC):
    """Base class for import strategies."""

    def __init__(self, kernel):
        self.kernel = kernel

    @abstractmethod
    def can_handle(self, file_path: str) -> bool:
        """Check if this strategy can handle the file."""
        pass

    @abstractmethod
    def import_file(self, file_path: str, repo_info: RepoInfo) -> Tuple[bool, str]:
        """Import file into semantic memory."""
        pass

    def _store(self, content: str, metadata: Dict, doc_id: str = None):
        """Store content in memory."""
        from kernel.memory import MemoryType

        # Map metadata type to MemoryType enum
        mem_type = metadata.get("type", "document")
        type_map = {
            "python_file": MemoryType.DOCUMENT,
            "markdown_file": MemoryType.DOCUMENT,
            "json_file": MemoryType.DOCUMENT,
            "yaml_file": MemoryType.DOCUMENT,
            "text_file": MemoryType.DOCUMENT,
            "class": MemoryType.CHUNK,
            "function": MemoryType.CHUNK,
            "section": MemoryType.CHUNK,
            "chunk": MemoryType.CHUNK,
            "import": MemoryType.CHUNK,
            "config_entry": MemoryType.CHUNK,
            "dependency": MemoryType.LINK,
        }

        # Use mapped type or default to DOCUMENT
        actual_type = type_map.get(mem_type, MemoryType.DOCUMENT)

        self.kernel.memory.store(
            content=content,
            type=actual_type,
            metadata=metadata,
            id=doc_id,
        )

    def _get_doc_id(self, file_path: str) -> str:
        """Get document ID for file path."""
        return f"doc_{file_path.replace('/', '_').replace('.', '_').replace(':', '_')}"

    def _get_file_hash(self, file_path: str) -> str:
        """Get hash of file for change detection."""
        try:
            with open(file_path, 'rb') as f:
                start = f.read(4096)
                f.seek(-4096, 2)
                end = f.read()
                return hashlib.md5(start + end).hexdigest()
        except Exception:
            return ""


class PythonImportStrategy(ImportStrategy):
    """Import strategy for Python files with AST parsing."""

    def can_handle(self, file_path: str) -> bool:
        return file_path.endswith('.py')

    def import_file(self, file_path: str, repo_info: RepoInfo) -> Tuple[bool, str]:
        """Import Python file with AST parsing."""
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()

            # Parse AST
            tree = ast.parse(content)

            # Extract info
            classes = [node for node in tree.body if isinstance(node, ast.ClassDef)]
            functions = [node for node in tree.body if isinstance(node, ast.FunctionDef)]
            imports = [node for node in tree.body if isinstance(node, (ast.Import, ast.ImportFrom))]

            # File summary
            file_info = {
                "path": file_path,
                "type": "python_file",
                "language": "python",
                "file_hash": self._get_file_hash(file_path),
                "file_size": len(content),
                "lines": len(content.splitlines()),
                "classes": [c.name for c in classes],
                "functions": [f.name for f in functions],
                "imports": self._extract_imports(imports),
                "repo": repo_info.name,
                "is_test": any(x in file_path for x in repo_info.test_dirs),
                "in_src": any(x in file_path for x in repo_info.src_dirs),
            }

            doc_id = self._get_doc_id(file_path)

            # Store file summary
            self._store(
                content=f"Python file: {Path(file_path).name}\n"
                       f"Classes: {len(classes)}\n"
                       f"Functions: {len(functions)}\n"
                       f"Lines: {file_info['lines']}",
                metadata={**file_info, "role": "summary"},
                doc_id=doc_id
            )

            # Store classes
            for cls in classes:
                class_code = ast.get_source_segment(content, cls)
                if class_code:
                    self._store(
                        content=f"Class: {cls.name}\n\n{class_code}",
                        metadata={
                            **file_info,
                            "type": "class",
                            "class_name": cls.name,
                            "methods": [m.name for m in cls.body if isinstance(m, ast.FunctionDef)],
                            "role": "code"
                        }
                    )

            # Store top-level functions
            for func in functions:
                func_code = ast.get_source_segment(content, func)
                if func_code:
                    self._store(
                        content=f"Function: {func.name}\n\n{func_code}",
                        metadata={
                            **file_info,
                            "type": "function",
                            "function_name": func.name,
                            "role": "code"
                        }
                    )

            # Store imports
            for imp in imports:
                import_stmt = ast.get_source_segment(content, imp)
                if import_stmt:
                    self._store(
                        content=import_stmt,
                        metadata={
                            **file_info,
                            "type": "import",
                            "role": "dependency"
                        }
                    )

            # Extract relationships
            self._extract_python_relationships(file_path, tree, repo_info)

            return True, "Imported as Python file"

        except SyntaxError:
            # Not valid Python, fall back to text import
            return self._import_as_text(file_path, repo_info)
        except Exception as e:
            return False, str(e)

    def _extract_imports(self, imports: List) -> List[str]:
        """Extract import names."""
        imported = []
        for imp in imports:
            if isinstance(imp, ast.Import):
                for alias in imp.names:
                    imported.append(alias.name.split('.')[0])
            elif isinstance(imp, ast.ImportFrom):
                if imp.module:
                    imported.append(imp.module.split('.')[0])
        return imported

    def _extract_python_relationships(self, file_path: str, tree: ast.AST, repo_info: RepoInfo):
        """Extract relationships from Python AST."""
        # Import relationships
        for node in tree.body:
            if isinstance(node, ast.Import):
                for alias in node.names:
                    module = alias.name.split('.')[0]
                    # Check if module is in this repo
                    module_file = self._find_module_file(module, repo_info)
                    if module_file:
                        self.kernel.relations.add_link(
                            file_path, module_file, "imports"
                        )

            elif isinstance(node, ast.ImportFrom):
                if node.module:
                    module = node.module.split('.')[0]
                    module_file = self._find_module_file(module, repo_info)
                    if module_file:
                        self.kernel.relations.add_link(
                            file_path, module_file, "imports"
                        )

        # Base class relationships
        for node in tree.body:
            if isinstance(node, ast.ClassDef):
                for base in node.bases:
                    if isinstance(base, ast.Name):
                        # Could be a class from this file or imported
                        base_name = base.id
                        # Store inheritance info in metadata

    def _find_module_file(self, module_name: str, repo_info: RepoInfo) -> Optional[str]:
        """Find file for imported module."""
        # Convert module name to file path
        module_path = module_name.replace('.', '/')
        root = Path(repo_info.root_path)

        # Try common locations
        for base_dir in ["", "src", "lib"]:
            test_path = root / base_dir / f"{module_path}.py"
            if test_path.exists():
                return str(test_path)

            test_init = root / base_dir / module_path / "__init__.py"
            if test_init.exists():
                return str(test_init)

        return None

    def _import_as_text(self, file_path: str, repo_info: RepoInfo) -> Tuple[bool, str]:
        """Fallback to text import."""
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()

            self._store(
                content=content,
                metadata={
                    "path": file_path,
                    "type": "text_file",
                    "file_hash": self._get_file_hash(file_path),
                    "file_size": len(content),
                    "repo": repo_info.name
                },
                doc_id=self._get_doc_id(file_path)
            )

            return True, "Imported as text (invalid Python syntax)"

        except Exception as e:
            return False, str(e)


class MarkdownImportStrategy(ImportStrategy):
    """Import strategy for Markdown files with section-based parsing."""

    def can_handle(self, file_path: str) -> bool:
        return file_path.endswith(('.md', '.markdown', '.rst'))

    def import_file(self, file_path: str, repo_info: RepoInfo) -> Tuple[bool, str]:
        """Import Markdown file by sections."""
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()

            # Split into sections
            sections = self._split_into_sections(content)

            # File metadata
            file_meta = {
                "path": file_path,
                "type": "markdown_file",
                "file_hash": self._get_file_hash(file_path),
                "file_size": len(content),
                "repo": repo_info.name,
                "in_docs": any(x in file_path for x in repo_info.doc_dirs),
                "sections": len(sections)
            }

            doc_id = self._get_doc_id(file_path)

            # Store file summary
            self._store(
                content=f"# {Path(file_path).name}\n\n{sections[0]['text'][:500] if sections else ''}...",
                metadata={**file_meta, "role": "summary"},
                doc_id=doc_id
            )

            # Store sections
            for i, section in enumerate(sections):
                # Extract code blocks
                code_blocks = self._extract_code_blocks(section['text'])

                self._store(
                    content=section['text'],
                    metadata={
                        **file_meta,
                        "section_index": i,
                        "header": section['header'],
                        "level": section['level'],
                        "code_blocks": len(code_blocks),
                        "role": "section"
                    }
                )

                # Link code blocks to source files
                for code in code_blocks:
                    self._link_code_to_source(file_path, code, repo_info)

            return True, f"Imported {len(sections)} sections"

        except Exception as e:
            return False, str(e)

    def _split_into_sections(self, content: str) -> List[Dict]:
        """Split markdown into sections by headers."""
        sections = []
        current_section = {"header": "Introduction", "level": 0, "text": ""}

        lines = content.split('\n')
        for line in lines:
            # Check for header
            if line.startswith('#'):
                # Save previous section
                if current_section["text"].strip():
                    sections.append(current_section)

                # Start new section
                level = len(line) - len(line.lstrip('#'))
                header = line.lstrip('#').strip()
                current_section = {"header": header, "level": level, "text": ""}
            else:
                current_section["text"] += line + '\n'

        # Add last section
        if current_section["text"].strip():
            sections.append(current_section)

        return sections

    def _extract_code_blocks(self, content: str) -> List[Dict]:
        """Extract code blocks from markdown."""
        blocks = []
        lines = content.split('\n')
        in_code_block = False
        current_block = []
        lang = ""

        for line in lines:
            if line.strip().startswith('```'):
                if in_code_block:
                    # End of code block
                    blocks.append({
                        "language": lang,
                        "code": '\n'.join(current_block)
                    })
                    current_block = []
                    in_code_block = False
                else:
                    # Start of code block
                    in_code_block = True
                    lang = line.strip()[3:].strip()
            elif in_code_block:
                current_block.append(line)

        return blocks

    def _link_code_to_source(self, doc_path: str, code_block: Dict, repo_info: RepoInfo):
        """Link code block in documentation to source file."""
        lang = code_block["language"]

        # For Python code, try to find referenced classes/functions
        if lang in ["python", "py"]:
            # Extract class/function names
            import re
            classes = re.findall(r'class\s+(\w+)', code_block["code"])
            functions = re.findall(r'def\s+(\w+)', code_block["code"])

            # Search for matching files in repo
            for name in classes + functions:
                # This is simplified - could be more sophisticated
                for src_dir in repo_info.src_dirs:
                    src_path = Path(repo_info.root_path) / src_dir
                    if src_path.exists():
                        for py_file in src_path.rglob("*.py"):
                            try:
                                with open(py_file) as f:
                                    content = f.read()
                                    if name in content:
                                        self.kernel.relations.add_link(
                                            doc_path, str(py_file), "documents"
                                        )
                                        break
                            except Exception:
                                pass


class JsonImportStrategy(ImportStrategy):
    """Import strategy for JSON files as structured data."""

    def can_handle(self, file_path: str) -> bool:
        return file_path.endswith('.json')

    def import_file(self, file_path: str, repo_info: RepoInfo) -> Tuple[bool, str]:
        """Import JSON as structured key-value data."""
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                data = json.load(f)

            # Flatten nested structure
            flat_data = self._flatten_json(data)

            file_meta = {
                "path": file_path,
                "type": "json_file",
                "file_hash": self._get_file_hash(file_path),
                "repo": repo_info.name,
                "is_config": any(x in file_path for x in ["config", "conf", "settings"]),
                "keys": list(flat_data.keys())
            }

            # Store each key-value pair
            for key, value in flat_data.items():
                self._store(
                    content=f"{key}: {json.dumps(value, ensure_ascii=False)}",
                    metadata={
                        **file_meta,
                        "config_key": key,
                        "value_type": type(value).__name__,
                        "role": "config_entry"
                    }
                )

            # Special handling for package.json
            if "package.json" in file_path:
                self._import_package_json(file_path, data, repo_info)

            return True, f"Imported {len(flat_data)} keys"

        except json.JSONDecodeError as e:
            return False, f"Invalid JSON: {str(e)}"
        except Exception as e:
            return False, str(e)

    def _flatten_json(self, data: Any, prefix: str = "") -> Dict[str, Any]:
        """Flatten nested JSON structure."""
        items = {}

        if isinstance(data, dict):
            for key, value in data.items():
                new_prefix = f"{prefix}.{key}" if prefix else key
                items.update(self._flatten_json(value, new_prefix))

        elif isinstance(data, list):
            for i, value in enumerate(data):
                new_prefix = f"{prefix}[{i}]" if prefix else f"[{i}]"
                items.update(self._flatten_json(value, new_prefix))

        else:
            items[prefix] = data

        return items

    def _import_package_json(self, file_path: str, data: Dict, repo_info: RepoInfo):
        """Special handling for package.json files."""
        # Dependencies → relationships
        if "dependencies" in data:
            for dep in data["dependencies"].keys():
                # Create relationship to dependency
                self._store(
                    content=f"Dependency: {dep} ({data['dependencies'][dep]})",
                    metadata={
                        "path": file_path,
                        "type": "dependency",
                        "dependency_name": dep,
                        "version": data["dependencies"][dep],
                        "repo": repo_info.name,
                        "role": "relationship"
                    }
                )

        # Scripts → entry points
        if "scripts" in data:
            for script_name, script_cmd in data["scripts"].items():
                self._store(
                    content=f"Script: {script_name}\n{script_cmd}",
                    metadata={
                        "path": file_path,
                        "type": "npm_script",
                        "script_name": script_name,
                        "command": script_cmd,
                        "repo": repo_info.name,
                        "role": "entry_point"
                    }
                )


class YamlImportStrategy(ImportStrategy):
    """Import strategy for YAML files."""

    def can_handle(self, file_path: str) -> bool:
        return file_path.endswith(('.yaml', '.yml'))

    def import_file(self, file_path: str, repo_info: RepoInfo) -> Tuple[bool, str]:
        """Import YAML as structured data."""
        try:
            import yaml

            with open(file_path, 'r', encoding='utf-8') as f:
                data = yaml.safe_load(f)

            # Flatten if it's a dict
            if isinstance(data, dict):
                # Similar to JSON handling
                flat_data = self._flatten_dict(data, "")

                file_meta = {
                    "path": file_path,
                    "type": "yaml_file",
                    "file_hash": self._get_file_hash(file_path),
                    "repo": repo_info.name,
                }

                for key, value in flat_data.items():
                    self._store(
                        content=f"{key}: {str(value)}",
                        metadata={
                            **file_meta,
                            "config_key": key,
                            "role": "config_entry"
                        }
                    )

                return True, f"Imported {len(flat_data)} keys"

            else:
                # Not a dict, store as-is
                self._store(
                    content=str(data),
                    metadata={
                        "path": file_path,
                        "type": "yaml_file",
                        "repo": repo_info.name
                    },
                    doc_id=self._get_doc_id(file_path)
                )
                return True, "Imported as YAML"

        except Exception as e:
            return False, str(e)

    def _flatten_dict(self, data: Dict, prefix: str) -> Dict[str, Any]:
        """Flatten nested dict."""
        items = {}
        for key, value in data.items():
            new_key = f"{prefix}.{key}" if prefix else key
            if isinstance(value, dict):
                items.update(self._flatten_dict(value, new_key))
            else:
                items[new_key] = value
        return items


class ChunkingImportStrategy(ImportStrategy):
    """Fallback strategy for generic text files."""

    def can_handle(self, file_path: str) -> bool:
        return True  # Can handle any file

    def import_file(self, file_path: str, repo_info: RepoInfo, chunk_size: int = 10000) -> Tuple[bool, str]:
        """Import file by chunking."""
        try:
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read()

            file_size = len(content)

            # Small files - import directly
            if file_size <= chunk_size:
                self._store(
                    content=content,
                    metadata={
                        "path": file_path,
                        "type": "text_file",
                        "file_hash": self._get_file_hash(file_path),
                        "file_size": file_size,
                        "repo": repo_info.name
                    },
                    doc_id=self._get_doc_id(file_path)
                )
                return True, "Imported as single chunk"

            # Large files - chunk it
            chunks = []
            lines = content.splitlines()
            chunk_lines = []
            chunk_num = 0

            for line in lines:
                chunk_lines.append(line)

                if len('\n'.join(chunk_lines)) >= chunk_size:
                    chunks.append('\n'.join(chunk_lines))
                    chunk_lines = []
                    chunk_num += 1

            if chunk_lines:
                chunks.append('\n'.join(chunk_lines))

            # Store chunks
            doc_id = self._get_doc_id(file_path)
            for i, chunk in enumerate(chunks):
                self._store(
                    content=f"Chunk {i+1}/{len(chunks)}\n\n{chunk}",
                    metadata={
                        "path": file_path,
                        "type": "chunk",
                        "chunk_index": i,
                        "total_chunks": len(chunks),
                        "file_hash": self._get_file_hash(file_path),
                        "repo": repo_info.name
                    }
                )

            return True, f"Imported as {len(chunks)} chunks"

        except Exception as e:
            return False, str(e)


class SmartImporter(QThread):
    """Smart file importer with repo-aware analysis."""

    progress = pyqtSignal(int, int, str)
    file_complete = pyqtSignal(str, bool, str)
    finished = pyqtSignal(object)

    def __init__(self, kernel, files: List[str], parent=None):
        super().__init__(parent)
        self.kernel = kernel
        self.files = files
        self._should_stop = False

        # Import strategies (ordered by priority)
        self.strategies = [
            PythonImportStrategy(kernel),
            MarkdownImportStrategy(kernel),
            JsonImportStrategy(kernel),
            YamlImportStrategy(kernel),
            ChunkingImportStrategy(kernel),  # Fallback
        ]

        # Statistics
        self.stats = {
            "total": 0,
            "successful": 0,
            "skipped": 0,
            "failed": 0,
            "strategies_used": defaultdict(int)
        }

    def stop(self):
        """Stop the import process."""
        self._should_stop = True

    def run(self):
        """Run the smart import process."""
        try:
            start_time = time.time()
            self.stats["total"] = len(self.files)

            # Step 1: Analyze repository structure
            if self.files:
                # Find common root directory
                root_dir = self._find_common_root(self.files)
                if root_dir:
                    self.progress.emit(0, len(self.files), f"Analyzing repository: {root_dir}")
                    repo_info = RepositoryAnalyzer.analyze(root_dir)
                else:
                    repo_info = None
            else:
                repo_info = None

            # Step 2: Import each file with appropriate strategy
            for i, file_path in enumerate(self.files):
                if self._should_stop:
                    break

                self.progress.emit(i + 1, len(self.files), file_path)

                # Check if file changed
                if self._should_skip_file(file_path):
                    self.stats["skipped"] += 1
                    self.file_complete.emit(file_path, False, "Unchanged")
                    continue

                # Try each strategy until one handles it
                imported = False
                for strategy in self.strategies:
                    if strategy.can_handle(file_path):
                        try:
                            success, message = strategy.import_file(file_path, repo_info) if repo_info else strategy.import_file(file_path, RepoInfo("", file_path, "generic"))
                            self.stats["strategies_used"][strategy.__class__.__name__] += 1

                            if success:
                                self.stats["successful"] += 1
                                imported = True
                                self.file_complete.emit(file_path, True, message)
                                break
                            elif "skip" not in message.lower():
                                # Try next strategy
                                continue
                        except Exception as e:
                            # Strategy failed, try next one
                            import traceback
                            error_msg = f"Strategy error: {str(e)}"
                            self.file_complete.emit(file_path, False, error_msg)
                            # Log full traceback for debugging
                            print(f"[Smart Import] Error importing {file_path}:")
                            print(traceback.format_exc())
                            continue

                if not imported:
                    self.stats["failed"] += 1
                    self.file_complete.emit(file_path, False, "No strategy could handle this file")

            # Step 3: Extract repo-level relationships
            if repo_info and not self._should_stop:
                try:
                    self._extract_repo_relationships(repo_info)
                except Exception as e:
                    import traceback
                    print(f"[Smart Import] Error extracting relationships: {str(e)}")
                    print(traceback.format_exc())

            self.stats["duration"] = time.time() - start_time
            self.finished.emit(self.stats)

        except Exception as e:
            import traceback
            # Catch-all error handler
            error_msg = f"Import failed: {str(e)}\n\n{traceback.format_exc()}"
            print(f"[Smart Import] Fatal error:\n{error_msg}")
            self.stats["duration"] = time.time() - start_time if start_time > 0 else 0
            self.stats["failed"] = self.stats.get("total", 0)
            self.finished.emit(self.stats)

    def _should_skip_file(self, file_path: str) -> bool:
        """Check if file should be skipped (unchanged)."""
        try:
            doc_id = f"doc_{file_path.replace('/', '_').replace('.', '_').replace(':', '_')}"
            existing = self.kernel.memory.get(doc_id)

            if existing:
                # Get current hash
                try:
                    with open(file_path, 'rb') as f:
                        start = f.read(4096)
                        f.seek(-4096, 2)
                        end = f.read()
                        current_hash = hashlib.md5(start + end).hexdigest()

                    stored_hash = existing.metadata.get("file_hash", "")
                    return current_hash == stored_hash
                except Exception:
                    pass

        except Exception:
            pass

        return False

    def _find_common_root(self, files: List[str]) -> Optional[str]:
        """Find common root directory of files."""
        if not files:
            return None

        # Get all parent directories
        paths = [Path(f).resolve() for f in files]

        # Find common prefix
        common = paths[0]
        for path in paths[1:]:
            parts = zip(common.parts, path.parts)
            for i, (c, p) in enumerate(parts):
                if c != p:
                    common = Path(*common.parts[:i])
                    break
            else:
                common = path

        return str(common) if common.exists() else None

    def _extract_repo_relationships(self, repo_info: RepoInfo):
        """Extract repository-level relationships."""
        # Test files → source files
        if repo_info.test_dirs:
            for test_dir in repo_info.test_dirs:
                test_path = Path(repo_info.root_path) / test_dir
                if test_path.exists():
                    for test_file in test_path.rglob("*_test.py"):
                        # Find corresponding source file
                        src_name = test_file.stem.replace("_test", "")
                        for src_dir in repo_info.src_dirs:
                            src_path = Path(repo_info.root_path) / src_dir / f"{src_name}.py"
                            if src_path.exists():
                                self.kernel.relations.add_link(
                                    str(test_file), str(src_path), "tests"
                                )

        # Docs → source files (already handled by MarkdownImportStrategy)
        # Config → source files
        for config_file in repo_info.config_files:
            if config_file.endswith(".json"):
                config_path = Path(repo_info.root_path) / config_file
                if config_path.exists():
                    try:
                        with open(config_path) as f:
                            data = json.load(f)

                        # Look for source references
                        content = json.dumps(data)
                        self._link_config_to_sources(str(config_path), content, repo_info)
                    except Exception:
                        pass

    def _link_config_to_sources(self, config_path: str, content: str, repo_info: RepoInfo):
        """Link config file to source files it references."""
        import re

        # Look for Python module references
        python_modules = re.findall(r'["\']([\w_]+)["\']', content)

        for module in python_modules:
            if len(module) < 3:  # Skip very short matches
                continue

            # Try to find matching Python file
            for src_dir in repo_info.src_dirs:
                src_path = Path(repo_info.root_path) / src_dir / f"{module}.py"
                if src_path.exists():
                    self.kernel.relations.add_link(
                        config_path, str(src_path), "configures"
                    )
                    break
