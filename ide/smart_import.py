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


class JavaScriptImportStrategy(ImportStrategy):
    """Import strategy for JavaScript/TypeScript files with regex-based parsing."""

    JS_EXTENSIONS = ('.js', '.jsx', '.ts', '.tsx', '.mjs', '.cjs')

    def can_handle(self, file_path: str) -> bool:
        return file_path.lower().endswith(self.JS_EXTENSIONS)

    def import_file(self, file_path: str, repo_info: RepoInfo) -> Tuple[bool, str]:
        """Import JS/TS file with structural extraction."""
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()

            # Extract structural elements using regex
            classes = self._extract_classes(content)
            functions = self._extract_functions(content)
            imports = self._extract_imports(content)
            exports = self._extract_exports(content)
            components = self._extract_react_components(content)

            # Determine if it's TypeScript
            is_typescript = file_path.lower().endswith(('.ts', '.tsx'))
            is_react = file_path.lower().endswith(('.jsx', '.tsx')) or bool(components)

            file_info = {
                "path": file_path,
                "type": "javascript_file",
                "language": "typescript" if is_typescript else "javascript",
                "is_react": is_react,
                "file_hash": self._get_file_hash(file_path),
                "file_size": len(content),
                "lines": len(content.splitlines()),
                "classes": [c['name'] for c in classes],
                "functions": [f['name'] for f in functions],
                "components": [c['name'] for c in components],
                "imports": imports,
                "exports": exports,
                "repo": repo_info.name,
                "is_test": any(x in file_path for x in ['test', 'spec', '__tests__']),
            }

            doc_id = self._get_doc_id(file_path)

            # Store file summary
            self._store(
                content=f"{'TypeScript' if is_typescript else 'JavaScript'} file: {Path(file_path).name}\n"
                       f"Classes: {len(classes)}\n"
                       f"Functions: {len(functions)}\n"
                       f"Components: {len(components)}\n"
                       f"Lines: {file_info['lines']}",
                metadata={**file_info, "role": "summary"},
                doc_id=doc_id
            )

            # Store classes
            for cls in classes:
                self._store(
                    content=f"Class: {cls['name']}\n\n{cls['code']}",
                    metadata={
                        **file_info,
                        "type": "class",
                        "class_name": cls['name'],
                        "role": "code"
                    }
                )

            # Store functions
            for func in functions:
                self._store(
                    content=f"Function: {func['name']}\n\n{func['code']}",
                    metadata={
                        **file_info,
                        "type": "function",
                        "function_name": func['name'],
                        "is_async": func.get('is_async', False),
                        "is_arrow": func.get('is_arrow', False),
                        "role": "code"
                    }
                )

            # Store React components
            for comp in components:
                self._store(
                    content=f"React Component: {comp['name']}\n\n{comp['code']}",
                    metadata={
                        **file_info,
                        "type": "react_component",
                        "component_name": comp['name'],
                        "role": "code"
                    }
                )

            return True, f"Imported JS/TS: {len(classes)} classes, {len(functions)} functions, {len(components)} components"

        except Exception as e:
            return False, str(e)

    def _extract_classes(self, content: str) -> List[Dict]:
        """Extract class definitions."""
        classes = []
        # Match: class Name { ... } or class Name extends Base { ... }
        pattern = r'(?:export\s+)?class\s+(\w+)(?:\s+extends\s+\w+)?(?:\s+implements\s+[\w,\s]+)?\s*\{'

        for match in re.finditer(pattern, content):
            name = match.group(1)
            start = match.start()
            # Find matching closing brace
            code = self._extract_block(content, start)
            if code:
                classes.append({'name': name, 'code': code})

        return classes

    def _extract_functions(self, content: str) -> List[Dict]:
        """Extract function definitions."""
        functions = []

        # Regular functions: function name(...) { or async function name(...)
        func_pattern = r'(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\([^)]*\)\s*(?::\s*\w+)?\s*\{'
        for match in re.finditer(func_pattern, content):
            name = match.group(1)
            start = match.start()
            code = self._extract_block(content, start)
            if code:
                functions.append({
                    'name': name,
                    'code': code,
                    'is_async': 'async' in match.group(0),
                    'is_arrow': False
                })

        # Arrow functions: const name = (...) => { or const name = async (...) =>
        arrow_pattern = r'(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s+)?\([^)]*\)\s*(?::\s*[\w<>,\s]+)?\s*=>\s*\{'
        for match in re.finditer(arrow_pattern, content):
            name = match.group(1)
            start = match.start()
            code = self._extract_block(content, start)
            if code:
                functions.append({
                    'name': name,
                    'code': code,
                    'is_async': 'async' in match.group(0),
                    'is_arrow': True
                })

        return functions

    def _extract_imports(self, content: str) -> List[str]:
        """Extract import statements."""
        imports = []
        # import ... from '...'
        pattern = r"import\s+.*?\s+from\s+['\"]([^'\"]+)['\"]"
        for match in re.finditer(pattern, content):
            imports.append(match.group(1))
        # require('...')
        pattern = r"require\s*\(\s*['\"]([^'\"]+)['\"]\s*\)"
        for match in re.finditer(pattern, content):
            imports.append(match.group(1))
        return imports

    def _extract_exports(self, content: str) -> List[str]:
        """Extract export names."""
        exports = []
        # export { name1, name2 }
        pattern = r'export\s*\{([^}]+)\}'
        for match in re.finditer(pattern, content):
            names = match.group(1).split(',')
            exports.extend([n.strip().split(' as ')[0].strip() for n in names])
        # export default
        if 'export default' in content:
            exports.append('default')
        return exports

    def _extract_react_components(self, content: str) -> List[Dict]:
        """Extract React component definitions."""
        components = []

        # Function components: function ComponentName(...) { return <
        # or const ComponentName = (...) => { return <
        # or const ComponentName = (...) => (<

        # Look for PascalCase function/const that returns JSX
        pattern = r'(?:export\s+)?(?:const|function)\s+([A-Z]\w+)\s*[=:]\s*(?:(?:\([^)]*\)|[^=])*=>|\([^)]*\)\s*(?::\s*[\w<>]+)?\s*\{)'

        for match in re.finditer(pattern, content):
            name = match.group(1)
            start = match.start()
            code = self._extract_block(content, start)

            # Verify it contains JSX (< followed by tag)
            if code and re.search(r'<[A-Z]\w*|<[a-z]+[\s/>]', code):
                components.append({'name': name, 'code': code})

        return components

    def _extract_block(self, content: str, start: int, max_length: int = 5000) -> Optional[str]:
        """Extract a code block starting from position, matching braces."""
        # Find the opening brace
        brace_pos = content.find('{', start)
        if brace_pos == -1:
            return None

        depth = 0
        end = brace_pos
        in_string = None
        escaped = False

        for i in range(brace_pos, min(len(content), start + max_length)):
            char = content[i]

            if escaped:
                escaped = False
                continue

            if char == '\\':
                escaped = True
                continue

            if in_string:
                if char == in_string:
                    in_string = None
                continue

            if char in '"\'`':
                in_string = char
                continue

            if char == '{':
                depth += 1
            elif char == '}':
                depth -= 1
                if depth == 0:
                    end = i + 1
                    break

        if depth != 0:
            return None

        return content[start:end]


class ImageImportStrategy(ImportStrategy):
    """Import strategy for images using vision models."""

    IMAGE_EXTENSIONS = ('.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp', '.tiff', '.tif')

    def __init__(self, kernel):
        super().__init__(kernel)
        self._vision_available = None
        self._vision_model = None

    def can_handle(self, file_path: str) -> bool:
        return file_path.lower().endswith(self.IMAGE_EXTENSIONS)

    def import_file(self, file_path: str, repo_info: RepoInfo) -> Tuple[bool, str]:
        """Import image with vision model description."""
        try:
            # Get image metadata
            metadata = self._extract_metadata(file_path)

            # Try to get vision description
            description = self._get_vision_description(file_path)

            file_info = {
                "path": file_path,
                "type": "image_file",
                "file_hash": self._get_file_hash(file_path),
                "repo": repo_info.name,
                **metadata
            }

            doc_id = self._get_doc_id(file_path)

            # Store image info
            content_parts = [f"Image: {Path(file_path).name}"]
            if metadata.get('width') and metadata.get('height'):
                content_parts.append(f"Dimensions: {metadata['width']}x{metadata['height']}")
            if metadata.get('format'):
                content_parts.append(f"Format: {metadata['format']}")
            if description:
                content_parts.append(f"\nDescription:\n{description}")

            self._store(
                content='\n'.join(content_parts),
                metadata={**file_info, "description": description, "role": "image"},
                doc_id=doc_id
            )

            if description:
                return True, f"Imported with vision description"
            else:
                return True, f"Imported metadata only (no vision model)"

        except Exception as e:
            return False, str(e)

    def _extract_metadata(self, file_path: str) -> Dict:
        """Extract image metadata."""
        metadata = {}

        try:
            from PIL import Image
            from PIL.ExifTags import TAGS

            with Image.open(file_path) as img:
                metadata['width'] = img.width
                metadata['height'] = img.height
                metadata['format'] = img.format
                metadata['mode'] = img.mode

                # Extract EXIF data
                exif_data = img._getexif()
                if exif_data:
                    exif = {}
                    for tag_id, value in exif_data.items():
                        tag = TAGS.get(tag_id, tag_id)
                        if isinstance(value, bytes):
                            continue  # Skip binary data
                        exif[tag] = str(value)[:100]  # Limit length

                    if 'DateTime' in exif:
                        metadata['date_taken'] = exif['DateTime']
                    if 'Make' in exif:
                        metadata['camera_make'] = exif['Make']
                    if 'Model' in exif:
                        metadata['camera_model'] = exif['Model']

        except ImportError:
            # PIL not available, get basic info
            import os
            stat = os.stat(file_path)
            metadata['file_size'] = stat.st_size
        except Exception as e:
            metadata['metadata_error'] = str(e)

        return metadata

    def _get_vision_description(self, file_path: str) -> Optional[str]:
        """Get description from vision model."""
        # Try Ollama with llava first
        description = self._try_ollama_vision(file_path)
        if description:
            return description

        # Try LM Studio with vision model
        description = self._try_lm_studio_vision(file_path)
        if description:
            return description

        # Try local BLIP model
        description = self._try_blip_local(file_path)
        if description:
            return description

        return None

    def _try_ollama_vision(self, file_path: str) -> Optional[str]:
        """Try to use Ollama with llava or similar vision model."""
        try:
            import requests
            import base64

            # Check if Ollama is running
            resp = requests.get('http://localhost:11434/api/tags', timeout=2)
            if resp.status_code != 200:
                return None

            # Check for vision models
            models = resp.json().get('models', [])
            vision_models = [m['name'] for m in models if any(v in m['name'].lower() for v in ['llava', 'bakllava', 'vision', 'moondream'])]

            if not vision_models:
                return None

            model = vision_models[0]

            # Read and encode image
            with open(file_path, 'rb') as f:
                image_data = base64.b64encode(f.read()).decode('utf-8')

            # Call Ollama vision API
            resp = requests.post(
                'http://localhost:11434/api/generate',
                json={
                    'model': model,
                    'prompt': 'Describe this image in detail. Include: main subject, colors, composition, any text visible, and overall mood or purpose.',
                    'images': [image_data],
                    'stream': False
                },
                timeout=60
            )

            if resp.status_code == 200:
                return resp.json().get('response', '')

        except Exception as e:
            print(f"[Vision] Ollama error: {e}")

        return None

    def _try_lm_studio_vision(self, file_path: str) -> Optional[str]:
        """Try to use LM Studio with a vision model."""
        try:
            import requests
            import base64

            # Check if LM Studio is running
            resp = requests.get('http://localhost:1234/v1/models', timeout=2)
            if resp.status_code != 200:
                return None

            # Check for vision models
            models = resp.json().get('data', [])
            vision_models = [m['id'] for m in models if any(v in m['id'].lower() for v in ['llava', 'vision', 'bakllava', 'moondream'])]

            if not vision_models:
                return None

            model = vision_models[0]

            # Read and encode image
            with open(file_path, 'rb') as f:
                image_data = base64.b64encode(f.read()).decode('utf-8')

            # Determine image type
            ext = Path(file_path).suffix.lower()
            mime_types = {'.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.webp': 'image/webp'}
            mime_type = mime_types.get(ext, 'image/jpeg')

            # Call LM Studio vision API (OpenAI compatible)
            resp = requests.post(
                'http://localhost:1234/v1/chat/completions',
                json={
                    'model': model,
                    'messages': [{
                        'role': 'user',
                        'content': [
                            {'type': 'text', 'text': 'Describe this image in detail. Include: main subject, colors, composition, any text visible, and overall mood or purpose.'},
                            {'type': 'image_url', 'image_url': {'url': f'data:{mime_type};base64,{image_data}'}}
                        ]
                    }],
                    'max_tokens': 500
                },
                timeout=60
            )

            if resp.status_code == 200:
                return resp.json()['choices'][0]['message']['content']

        except Exception as e:
            print(f"[Vision] LM Studio error: {e}")

        return None

    def _try_blip_local(self, file_path: str) -> Optional[str]:
        """Try to use local BLIP model via transformers."""
        try:
            from transformers import BlipProcessor, BlipForConditionalGeneration
            from PIL import Image

            # Load model (will be cached after first use)
            if not hasattr(self, '_blip_processor'):
                print("[Vision] Loading BLIP model (first time may take a while)...")
                self._blip_processor = BlipProcessor.from_pretrained("Salesforce/blip-image-captioning-base")
                self._blip_model = BlipForConditionalGeneration.from_pretrained("Salesforce/blip-image-captioning-base")

            # Process image
            image = Image.open(file_path).convert('RGB')
            inputs = self._blip_processor(image, return_tensors="pt")

            # Generate caption
            out = self._blip_model.generate(**inputs, max_new_tokens=100)
            caption = self._blip_processor.decode(out[0], skip_special_tokens=True)

            return caption

        except ImportError:
            # transformers not available
            pass
        except Exception as e:
            print(f"[Vision] BLIP error: {e}")

        return None


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
            JavaScriptImportStrategy(kernel),
            ImageImportStrategy(kernel),
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
                            print(f"[Smart Import] Attempting {strategy.__class__.__name__} for {file_path}")
                            success, message = strategy.import_file(file_path, repo_info) if repo_info else strategy.import_file(file_path, RepoInfo("", file_path, "generic"))
                            print(f"[Smart Import] {strategy.__class__.__name__} result: success={success}, message={message}")
                            self.stats["strategies_used"][strategy.__class__.__name__] += 1

                            if success:
                                self.stats["successful"] += 1
                                imported = True
                                self.file_complete.emit(file_path, True, message)
                                break
                            elif "skip" not in message.lower():
                                # Try next strategy
                                print(f"[Smart Import] {strategy.__class__.__name__} didn't handle it, trying next...")
                                continue
                        except Exception as e:
                            # Strategy failed, try next one
                            import traceback
                            error_msg = f"Strategy error: {str(e)}"
                            self.file_complete.emit(file_path, False, error_msg)
                            # Log full traceback for debugging
                            print(f"[Smart Import] Error importing {file_path} with {strategy.__class__.__name__}:")
                            print(traceback.format_exc())
                            continue
                    else:
                        print(f"[Smart Import] {strategy.__class__.__name__} can't handle {file_path}")

                if not imported:
                    self.stats["failed"] += 1
                    print(f"[Smart Import] No strategy could handle {file_path}")
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
        # Check database directly (not cache) to avoid stale data
        try:
            doc_id = f"doc_{file_path.replace('/', '_').replace('.', '_').replace(':', '_')}"

            # Get database path from kernel
            if hasattr(self.kernel.memory, '_conn'):
                conn = self.kernel.memory._conn
                cursor = conn.execute(
                    "SELECT metadata FROM memory WHERE id = ?", (doc_id,)
                )
                row = cursor.fetchone()

                if row:
                    # File exists in database, check hash
                    import json
                    metadata = json.loads(row[0]) if row[0] else {}
                    stored_hash = metadata.get("file_hash", "")

                    if stored_hash:
                        # Get current hash
                        with open(file_path, 'rb') as f:
                            start = f.read(4096)
                            f.seek(-4096, 2)
                            end = f.read()
                        current_hash = hashlib.md5(start + end).hexdigest()

                        if current_hash == stored_hash:
                            return True  # Skip unchanged file
        except Exception as e:
            print(f"[Smart Import] Error checking file {file_path}: {e}")

        return False  # Import the file

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

        # If common is a file, use its parent directory
        if common.exists() and common.is_file():
            common = common.parent

        return str(common) if common.exists() and common.is_dir() else None

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
