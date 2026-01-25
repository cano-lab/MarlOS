"""
Meta-Document Manifest
======================

The manifest is a document's "passport" - it declares:
- What providers can attach to this context
- What permissions each provider has
- Document metadata and relationships

Manifests can be:
1. Embedded in markdown frontmatter (---manifest:---)
2. Stored as sidecar file (.manifest.json)
3. Inherited from parent folder (.folder-manifest.json)

The kernel checks the manifest before allowing provider operations.
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Any, Set, Union
from pathlib import Path
from enum import Enum
import json
import re


class LinkRelation(str, Enum):
    """Semantic relationship types between documents."""

    # Structural relationships
    CONTAINS = "contains"           # This document contains the target (parent)
    PART_OF = "part_of"            # This document is part of the target (child)

    # Reference relationships
    REFERENCES = "references"       # General reference
    CITES = "cites"                # Academic/formal citation
    QUOTES = "quotes"              # Direct quotation

    # Dependency relationships
    IMPLEMENTS = "implements"       # This implements a spec/interface
    EXTENDS = "extends"            # This extends/builds upon target
    REQUIRES = "requires"          # This depends on target
    USED_BY = "used_by"           # Target depends on this

    # Versioning relationships
    SUPERSEDES = "supersedes"      # This replaces the target
    SUPERSEDED_BY = "superseded_by"  # This is replaced by target
    VERSION_OF = "version_of"      # This is a version of target
    DERIVED_FROM = "derived_from"  # This was derived from target

    # Semantic relationships
    RELATED_TO = "related_to"      # General semantic relation
    CONTRADICTS = "contradicts"    # This conflicts with target
    SUPPORTS = "supports"          # This provides evidence for target
    EXPLAINS = "explains"          # This explains the target
    EXAMPLE_OF = "example_of"      # This is an example of target concept

    # Workflow relationships
    REVIEWED_BY = "reviewed_by"    # Target is reviewer of this
    APPROVED_BY = "approved_by"    # Target approved this
    BLOCKS = "blocks"              # This blocks target
    BLOCKED_BY = "blocked_by"      # This is blocked by target


# Inverse relationship mapping
INVERSE_RELATIONS = {
    LinkRelation.CONTAINS: LinkRelation.PART_OF,
    LinkRelation.PART_OF: LinkRelation.CONTAINS,
    LinkRelation.IMPLEMENTS: LinkRelation.USED_BY,
    LinkRelation.USED_BY: LinkRelation.IMPLEMENTS,
    LinkRelation.REQUIRES: LinkRelation.USED_BY,
    LinkRelation.SUPERSEDES: LinkRelation.SUPERSEDED_BY,
    LinkRelation.SUPERSEDED_BY: LinkRelation.SUPERSEDES,
    LinkRelation.BLOCKS: LinkRelation.BLOCKED_BY,
    LinkRelation.BLOCKED_BY: LinkRelation.BLOCKS,
    LinkRelation.SUPPORTS: LinkRelation.SUPPORTED_BY if hasattr(LinkRelation, 'SUPPORTED_BY') else LinkRelation.RELATED_TO,
}


@dataclass
class SemanticLink:
    """A semantic link to another document."""
    target: str                              # Path to target document
    relation: LinkRelation = LinkRelation.RELATED_TO
    label: Optional[str] = None              # Human-readable label
    bidirectional: bool = True               # Create inverse link in target?
    metadata: Dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> Dict:
        result = {
            "target": self.target,
            "relation": self.relation.value,
        }
        if self.label:
            result["label"] = self.label
        if not self.bidirectional:
            result["bidirectional"] = False
        if self.metadata:
            result["metadata"] = self.metadata
        return result

    @classmethod
    def from_dict(cls, data: Union[str, Dict]) -> 'SemanticLink':
        """Parse from dict or simple string path."""
        if isinstance(data, str):
            return cls(target=data)

        relation_str = data.get("relation", "related_to")
        try:
            relation = LinkRelation(relation_str)
        except ValueError:
            relation = LinkRelation.RELATED_TO

        return cls(
            target=data.get("target", data.get("path", "")),
            relation=relation,
            label=data.get("label"),
            bidirectional=data.get("bidirectional", True),
            metadata=data.get("metadata", {}),
        )

    def get_inverse(self, source_path: str) -> 'SemanticLink':
        """Get the inverse link (for bidirectional linking)."""
        inverse_relation = INVERSE_RELATIONS.get(self.relation, LinkRelation.RELATED_TO)
        return SemanticLink(
            target=source_path,
            relation=inverse_relation,
            label=f"(inverse) {self.label}" if self.label else None,
            bidirectional=False,  # Don't create infinite loop
            metadata={"inverse_of": self.target},
        )


@dataclass
class ProviderPermission:
    """Permissions for a specific provider."""
    provider_id: str
    can_read: bool = True
    can_write: bool = False
    can_emit: bool = True
    can_invoke: bool = False
    allowed_commands: List[str] = field(default_factory=list)  # Empty = all allowed
    denied_commands: List[str] = field(default_factory=list)

    def allows_command(self, command: str) -> bool:
        """Check if a specific command is allowed."""
        if command in self.denied_commands:
            return False
        if self.allowed_commands and command not in self.allowed_commands:
            return False
        return self.can_invoke

    def to_dict(self) -> Dict:
        return {
            "provider": self.provider_id,
            "can_read": self.can_read,
            "can_write": self.can_write,
            "can_emit": self.can_emit,
            "can_invoke": self.can_invoke,
            "allowed_commands": self.allowed_commands,
            "denied_commands": self.denied_commands,
        }

    @classmethod
    def from_dict(cls, data: Dict) -> 'ProviderPermission':
        return cls(
            provider_id=data.get("provider", data.get("provider_id", "unknown")),
            can_read=data.get("can_read", True),
            can_write=data.get("can_write", False),
            can_emit=data.get("can_emit", True),
            can_invoke=data.get("can_invoke", False),
            allowed_commands=data.get("allowed_commands", []),
            denied_commands=data.get("denied_commands", []),
        )


@dataclass
class DocumentManifest:
    """The manifest for a document context."""

    # Document identity
    document_type: str = "document"  # document, code, spec, note, etc.
    tags: List[str] = field(default_factory=list)

    # Provider permissions
    providers: Dict[str, ProviderPermission] = field(default_factory=dict)

    # Default permissions for unlisted providers
    default_can_read: bool = True
    default_can_write: bool = False
    default_can_emit: bool = True
    default_can_invoke: bool = False

    # AI access level
    ai_access: str = "observe"  # "none", "observe", "suggest", "edit"

    # Relationships (semantic links)
    links: List[SemanticLink] = field(default_factory=list)
    parent: Optional[str] = None  # Parent document (for hierarchies)

    # Metadata
    created_at: Optional[str] = None
    modified_at: Optional[str] = None
    version: int = 1

    def get_provider_permission(self, provider_id: str) -> ProviderPermission:
        """Get permissions for a provider (uses defaults if not specified)."""
        if provider_id in self.providers:
            return self.providers[provider_id]

        # Return default permissions
        return ProviderPermission(
            provider_id=provider_id,
            can_read=self.default_can_read,
            can_write=self.default_can_write,
            can_emit=self.default_can_emit,
            can_invoke=self.default_can_invoke,
        )

    def set_provider_permission(self, perm: ProviderPermission):
        """Set permissions for a provider."""
        self.providers[perm.provider_id] = perm

    def allow_provider(self, provider_id: str,
                       can_read=True, can_write=False,
                       can_emit=True, can_invoke=False):
        """Convenience method to allow a provider with permissions."""
        self.providers[provider_id] = ProviderPermission(
            provider_id=provider_id,
            can_read=can_read,
            can_write=can_write,
            can_emit=can_emit,
            can_invoke=can_invoke,
        )

    def deny_provider(self, provider_id: str):
        """Completely deny a provider."""
        self.providers[provider_id] = ProviderPermission(
            provider_id=provider_id,
            can_read=False,
            can_write=False,
            can_emit=False,
            can_invoke=False,
        )

    # ----- Semantic Link Methods -----

    def add_link(self, target: str, relation: LinkRelation = LinkRelation.RELATED_TO,
                 label: Optional[str] = None, bidirectional: bool = True) -> SemanticLink:
        """Add a semantic link to another document."""
        link = SemanticLink(
            target=target,
            relation=relation,
            label=label,
            bidirectional=bidirectional,
        )
        # Check for duplicates
        if not any(l.target == target and l.relation == relation for l in self.links):
            self.links.append(link)
        return link

    def remove_link(self, target: str, relation: Optional[LinkRelation] = None) -> bool:
        """Remove a link. If relation is None, removes all links to target."""
        original_len = len(self.links)
        if relation:
            self.links = [l for l in self.links
                         if not (l.target == target and l.relation == relation)]
        else:
            self.links = [l for l in self.links if l.target != target]
        return len(self.links) < original_len

    def get_links_by_relation(self, relation: LinkRelation) -> List[SemanticLink]:
        """Get all links with a specific relation type."""
        return [l for l in self.links if l.relation == relation]

    def get_link_to(self, target: str) -> Optional[SemanticLink]:
        """Get the primary link to a specific target."""
        for link in self.links:
            if link.target == target:
                return link
        return None

    def has_link_to(self, target: str) -> bool:
        """Check if this document links to target."""
        return any(l.target == target for l in self.links)

    def get_related_paths(self) -> Set[str]:
        """Get all unique paths this document links to."""
        paths = {l.target for l in self.links}
        if self.parent:
            paths.add(self.parent)
        return paths

    def to_dict(self) -> Dict:
        return {
            "version": self.version,
            "type": self.document_type,
            "tags": self.tags,
            "ai_access": self.ai_access,
            "defaults": {
                "can_read": self.default_can_read,
                "can_write": self.default_can_write,
                "can_emit": self.default_can_emit,
                "can_invoke": self.default_can_invoke,
            },
            "providers": {
                pid: perm.to_dict()
                for pid, perm in self.providers.items()
            },
            "links": [link.to_dict() for link in self.links],
            "parent": self.parent,
            "created_at": self.created_at,
            "modified_at": self.modified_at,
        }

    @classmethod
    def from_dict(cls, data: Dict) -> 'DocumentManifest':
        defaults = data.get("defaults", {})
        providers = {}
        for pid, pdata in data.get("providers", {}).items():
            if isinstance(pdata, dict):
                pdata["provider_id"] = pid
                providers[pid] = ProviderPermission.from_dict(pdata)

        # Parse links as SemanticLink objects
        raw_links = data.get("links", [])
        links = [SemanticLink.from_dict(link) for link in raw_links]

        return cls(
            document_type=data.get("type", "document"),
            tags=data.get("tags", []),
            ai_access=data.get("ai_access", "observe"),
            default_can_read=defaults.get("can_read", True),
            default_can_write=defaults.get("can_write", False),
            default_can_emit=defaults.get("can_emit", True),
            default_can_invoke=defaults.get("can_invoke", False),
            providers=providers,
            links=links,
            parent=data.get("parent"),
            created_at=data.get("created_at"),
            modified_at=data.get("modified_at"),
            version=data.get("version", 1),
        )

    def to_yaml(self) -> str:
        """Convert to YAML for frontmatter embedding."""
        lines = []
        lines.append(f"type: {self.document_type}")
        if self.tags:
            lines.append(f"tags: [{', '.join(self.tags)}]")
        lines.append(f"ai_access: {self.ai_access}")

        if self.providers:
            lines.append("providers:")
            for pid, perm in self.providers.items():
                perms = []
                if perm.can_read: perms.append("read")
                if perm.can_write: perms.append("write")
                if perm.can_emit: perms.append("emit")
                if perm.can_invoke: perms.append("invoke")
                lines.append(f"  {pid}: [{', '.join(perms)}]")

        if self.links:
            lines.append("links:")
            for link in self.links:
                if link.label:
                    lines.append(f"  - target: {link.target}")
                    lines.append(f"    relation: {link.relation.value}")
                    lines.append(f"    label: {link.label}")
                else:
                    lines.append(f"  - {link.target} ({link.relation.value})")

        return "\n".join(lines)


class ManifestLoader:
    """Loads manifests from various sources."""

    # Pattern for YAML frontmatter with manifest
    FRONTMATTER_PATTERN = re.compile(
        r'^---\s*\n(.*?)\n---',
        re.DOTALL
    )

    # Pattern for manifest section in frontmatter
    MANIFEST_SECTION = re.compile(
        r'manifest:\s*\n((?:  .+\n)*)',
        re.MULTILINE
    )

    @classmethod
    def load_for_path(cls, file_path: str) -> DocumentManifest:
        """Load manifest for a file, checking all sources."""
        path = Path(file_path)

        # 1. Check for sidecar manifest
        sidecar = path.with_suffix(path.suffix + ".manifest.json")
        if sidecar.exists():
            return cls._load_sidecar(sidecar)

        # 2. Check for embedded manifest in markdown
        if path.suffix.lower() in (".md", ".markdown"):
            embedded = cls._load_embedded(path)
            if embedded:
                return embedded

        # 3. Check for folder manifest
        folder_manifest = path.parent / ".folder-manifest.json"
        if folder_manifest.exists():
            return cls._load_sidecar(folder_manifest)

        # 4. Return default manifest based on file type
        return cls._default_manifest(path)

    @classmethod
    def _load_sidecar(cls, path: Path) -> DocumentManifest:
        """Load from sidecar JSON file."""
        try:
            with open(path, "r", encoding="utf-8") as f:
                data = json.load(f)
            return DocumentManifest.from_dict(data)
        except Exception:
            return DocumentManifest()

    @classmethod
    def _load_embedded(cls, path: Path) -> Optional[DocumentManifest]:
        """Load manifest embedded in markdown frontmatter."""
        try:
            with open(path, "r", encoding="utf-8") as f:
                content = f.read(4096)  # Only read beginning

            # Look for frontmatter
            match = cls.FRONTMATTER_PATTERN.match(content)
            if not match:
                return None

            frontmatter = match.group(1)

            # Parse simple YAML-like structure
            data = cls._parse_simple_yaml(frontmatter)

            # Check for manifest section or top-level manifest keys
            if "manifest" in data:
                return DocumentManifest.from_dict(data["manifest"])
            elif "type" in data or "ai_access" in data or "providers" in data:
                return DocumentManifest.from_dict(data)

            return None
        except Exception:
            return None

    @classmethod
    def _parse_simple_yaml(cls, text: str) -> Dict:
        """Parse simple YAML-like frontmatter (not full YAML)."""
        result = {}
        current_key = None
        current_dict = None

        for line in text.split("\n"):
            line = line.rstrip()
            if not line:
                continue

            # Check indentation
            stripped = line.lstrip()
            indent = len(line) - len(stripped)

            if indent == 0 and ":" in stripped:
                # Top-level key
                key, _, value = stripped.partition(":")
                key = key.strip()
                value = value.strip()

                if value:
                    # Inline value
                    if value.startswith("[") and value.endswith("]"):
                        # List
                        result[key] = [
                            v.strip().strip("'\"")
                            for v in value[1:-1].split(",")
                            if v.strip()
                        ]
                    else:
                        result[key] = value.strip("'\"")
                else:
                    # Nested structure
                    current_key = key
                    current_dict = {}
                    result[key] = current_dict

            elif indent > 0 and current_dict is not None and ":" in stripped:
                # Nested key
                key, _, value = stripped.partition(":")
                key = key.strip()
                value = value.strip()

                if value.startswith("[") and value.endswith("]"):
                    current_dict[key] = [
                        v.strip().strip("'\"")
                        for v in value[1:-1].split(",")
                        if v.strip()
                    ]
                else:
                    current_dict[key] = value.strip("'\"")

        return result

    @classmethod
    def _default_manifest(cls, path: Path) -> DocumentManifest:
        """Create default manifest based on file type."""
        suffix = path.suffix.lower()

        if suffix in (".py", ".js", ".ts", ".go", ".rs", ".c", ".cpp", ".java"):
            return DocumentManifest(
                document_type="code",
                ai_access="suggest",
                default_can_invoke=True,
            )
        elif suffix in (".md", ".markdown", ".txt"):
            return DocumentManifest(
                document_type="document",
                ai_access="observe",
            )
        elif suffix in (".json", ".yaml", ".yml", ".toml"):
            return DocumentManifest(
                document_type="config",
                ai_access="observe",
            )
        elif suffix in (".pdf", ".epub"):
            return DocumentManifest(
                document_type="reader",
                ai_access="none",
                default_can_write=False,
            )
        else:
            return DocumentManifest()

    @classmethod
    def save_sidecar(cls, file_path: str, manifest: DocumentManifest):
        """Save manifest as sidecar JSON file."""
        path = Path(file_path)
        sidecar = path.with_suffix(path.suffix + ".manifest.json")

        with open(sidecar, "w", encoding="utf-8") as f:
            json.dump(manifest.to_dict(), f, indent=2)

    @classmethod
    def embed_in_markdown(cls, file_path: str, manifest: DocumentManifest):
        """Embed manifest in markdown frontmatter."""
        path = Path(file_path)

        with open(path, "r", encoding="utf-8") as f:
            content = f.read()

        yaml_content = manifest.to_yaml()

        # Check if frontmatter exists
        match = cls.FRONTMATTER_PATTERN.match(content)
        if match:
            # Update existing frontmatter
            old_fm = match.group(1)
            # Remove old manifest keys
            new_lines = []
            skip_until_unindent = False
            for line in old_fm.split("\n"):
                if skip_until_unindent:
                    if line and not line[0].isspace():
                        skip_until_unindent = False
                    else:
                        continue
                if line.startswith(("type:", "tags:", "ai_access:", "providers:", "links:", "manifest:")):
                    if ":" in line and not line.rstrip().endswith(":"):
                        continue  # Single line, skip
                    skip_until_unindent = True
                    continue
                new_lines.append(line)

            # Add new manifest
            new_fm = "\n".join(new_lines).strip()
            if new_fm:
                new_fm += "\n"
            new_fm += yaml_content

            content = f"---\n{new_fm}\n---" + content[match.end():]
        else:
            # Add new frontmatter
            content = f"---\n{yaml_content}\n---\n\n{content}"

        with open(path, "w", encoding="utf-8") as f:
            f.write(content)


# Preset manifests for common document types
PRESET_MANIFESTS = {
    "private": DocumentManifest(
        document_type="private",
        ai_access="none",
        default_can_read=False,
        default_can_write=False,
        default_can_emit=False,
        default_can_invoke=False,
    ),
    "readonly": DocumentManifest(
        document_type="document",
        ai_access="observe",
        default_can_read=True,
        default_can_write=False,
        default_can_emit=True,
        default_can_invoke=False,
    ),
    "collaborative": DocumentManifest(
        document_type="document",
        ai_access="suggest",
        default_can_read=True,
        default_can_write=True,
        default_can_emit=True,
        default_can_invoke=True,
    ),
    "code": DocumentManifest(
        document_type="code",
        ai_access="suggest",
        default_can_read=True,
        default_can_write=True,
        default_can_emit=True,
        default_can_invoke=True,
    ),
}


def get_preset(name: str) -> Optional[DocumentManifest]:
    """Get a preset manifest by name."""
    return PRESET_MANIFESTS.get(name)
