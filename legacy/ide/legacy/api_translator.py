"""
Semantic API Translator
=======================

Translates legacy API calls (Windows, POSIX, etc.) into semantic operations.
Uses pattern matching for known APIs and AI for unknown/complex cases.

This is the core of the AI Synthesis Layer - it understands INTENT,
not just syntax.

Architecture:
    API Call → Pattern Matcher → [AI Fallback] → Semantic Operation
                    ↓
              Known patterns         Unknown APIs
              (fast, 100%)          (AI inference)
"""

import re
import time
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Callable, Tuple
from enum import Enum

from .semantic_ops import (
    SemanticOp, SemanticPrimitive, Intent, EventType,
    APICallContext, TranslationResult
)


class APICategory(str, Enum):
    """Categories of API calls."""
    FILE = "file"               # File operations
    REGISTRY = "registry"       # Windows registry
    CLIPBOARD = "clipboard"     # Clipboard operations
    DIALOG = "dialog"           # UI dialogs
    WINDOW = "window"           # Window management
    PROCESS = "process"         # Process/thread
    NETWORK = "network"         # Network operations
    GRAPHICS = "graphics"       # Graphics/GDI
    INPUT = "input"             # Keyboard/mouse
    MEMORY = "memory"           # Memory management
    UNKNOWN = "unknown"


@dataclass
class APIPattern:
    """A pattern for matching and translating API calls."""
    api_regex: str                        # Regex to match API name
    category: APICategory
    arg_patterns: Dict[str, Any] = None   # Argument patterns to match
    translator: Callable = None            # Custom translator function
    semantic_template: Dict = None         # Template for semantic op


class SemanticAPITranslator:
    """Translates legacy API calls to semantic operations.

    Usage:
        translator = SemanticAPITranslator()

        # Translate a Windows API call
        result = translator.translate(
            APICallContext(
                api_name="CreateFileW",
                args={"path": "doc.txt", "access": "GENERIC_WRITE"},
                app_name="notepad"
            )
        )

        if result.success:
            for op in result.operations:
                kernel.execute(op)
    """

    def __init__(self, ai_client=None, kernel=None):
        self.ai_client = ai_client
        self.kernel = kernel

        # Statistics
        self.stats = {
            "total": 0,
            "pattern_matches": 0,
            "ai_translations": 0,
            "failures": 0,
        }

        # Call history for AI context
        self._call_history: List[Dict] = []
        self._max_history = 100

        # Build pattern table
        self._patterns = self._build_patterns()

        # Cache for AI translations
        self._ai_cache: Dict[str, TranslationResult] = {}

    def _build_patterns(self) -> List[APIPattern]:
        """Build the pattern matching table."""
        patterns = []

        # ============= FILE OPERATIONS =============

        # CreateFile variants (Windows)
        patterns.append(APIPattern(
            api_regex=r"^(CreateFile[AW]?|NtCreateFile|NtOpenFile)$",
            category=APICategory.FILE,
            translator=self._translate_create_file,
        ))

        # Read/Write file
        patterns.append(APIPattern(
            api_regex=r"^(ReadFile|NtReadFile)$",
            category=APICategory.FILE,
            semantic_template={
                "primitive": "ctx.emit",
                "event": "content_read",
            }
        ))

        patterns.append(APIPattern(
            api_regex=r"^(WriteFile|NtWriteFile)$",
            category=APICategory.FILE,
            translator=self._translate_write_file,
        ))

        # Close file
        patterns.append(APIPattern(
            api_regex=r"^(CloseHandle|NtClose)$",
            category=APICategory.FILE,
            translator=self._translate_close_handle,
        ))

        # Delete file
        patterns.append(APIPattern(
            api_regex=r"^(DeleteFile[AW]?|NtDeleteFile|RemoveDirectory)$",
            category=APICategory.FILE,
            semantic_template={
                "primitive": "ctx.invoke",
                "action": "document.delete",
            }
        ))

        # Move/Rename file
        patterns.append(APIPattern(
            api_regex=r"^(MoveFile[AW]?|MoveFileEx|rename)$",
            category=APICategory.FILE,
            translator=self._translate_move_file,
        ))

        # Copy file
        patterns.append(APIPattern(
            api_regex=r"^(CopyFile[AW]?|CopyFileEx)$",
            category=APICategory.FILE,
            translator=self._translate_copy_file,
        ))

        # ============= POSIX FILE OPERATIONS =============

        patterns.append(APIPattern(
            api_regex=r"^open$",
            category=APICategory.FILE,
            translator=self._translate_posix_open,
        ))

        patterns.append(APIPattern(
            api_regex=r"^(write|pwrite|writev)$",
            category=APICategory.FILE,
            semantic_template={
                "primitive": "ctx.emit",
                "event": "content_changed",
            }
        ))

        patterns.append(APIPattern(
            api_regex=r"^close$",
            category=APICategory.FILE,
            semantic_template={
                "primitive": "ctx.detach",
            }
        ))

        patterns.append(APIPattern(
            api_regex=r"^(read|pread|readv)$",
            category=APICategory.FILE,
            semantic_template={
                "primitive": "ctx.emit",
                "event": "content_read",
            }
        ))

        patterns.append(APIPattern(
            api_regex=r"^(unlink|remove)$",
            category=APICategory.FILE,
            semantic_template={
                "primitive": "ctx.invoke",
                "action": "document.delete",
            }
        ))

        # fopen/fclose (C library)
        patterns.append(APIPattern(
            api_regex=r"^fopen(64)?$",
            category=APICategory.FILE,
            translator=self._translate_fopen,
        ))

        patterns.append(APIPattern(
            api_regex=r"^fclose$",
            category=APICategory.FILE,
            semantic_template={
                "primitive": "ctx.detach",
            }
        ))

        # ============= CLIPBOARD =============

        patterns.append(APIPattern(
            api_regex=r"^(SetClipboardData|EmptyClipboard)$",
            category=APICategory.CLIPBOARD,
            semantic_template={
                "primitive": "ctx.emit",
                "event": "content_copied",
            }
        ))

        patterns.append(APIPattern(
            api_regex=r"^GetClipboardData$",
            category=APICategory.CLIPBOARD,
            semantic_template={
                "primitive": "ctx.emit",
                "event": "content_pasted",
            }
        ))

        # ============= DIALOGS =============

        patterns.append(APIPattern(
            api_regex=r"^GetOpenFileName[AW]?$",
            category=APICategory.DIALOG,
            semantic_template={
                "primitive": "ctx.invoke",
                "action": "document.open_dialog",
            }
        ))

        patterns.append(APIPattern(
            api_regex=r"^GetSaveFileName[AW]?$",
            category=APICategory.DIALOG,
            semantic_template={
                "primitive": "ctx.invoke",
                "action": "document.save_dialog",
            }
        ))

        patterns.append(APIPattern(
            api_regex=r"^PrintDlg[AW]?$",
            category=APICategory.DIALOG,
            semantic_template={
                "primitive": "ctx.invoke",
                "action": "document.print",
            }
        ))

        patterns.append(APIPattern(
            api_regex=r"^(MessageBox[AW]?|MessageBoxEx)$",
            category=APICategory.DIALOG,
            translator=self._translate_message_box,
        ))

        # ============= WINDOW MANAGEMENT =============

        patterns.append(APIPattern(
            api_regex=r"^(SetForegroundWindow|SetActiveWindow|SetFocus)$",
            category=APICategory.WINDOW,
            semantic_template={
                "primitive": "ctx.emit",
                "event": "app_focused",
            }
        ))

        patterns.append(APIPattern(
            api_regex=r"^SetWindowText[AW]?$",
            category=APICategory.WINDOW,
            translator=self._translate_set_window_text,
        ))

        patterns.append(APIPattern(
            api_regex=r"^(CreateWindow|CreateWindowEx)[AW]?$",
            category=APICategory.WINDOW,
            semantic_template={
                "primitive": "ctx.emit",
                "event": "dialog_opened",
            }
        ))

        patterns.append(APIPattern(
            api_regex=r"^DestroyWindow$",
            category=APICategory.WINDOW,
            semantic_template={
                "primitive": "ctx.emit",
                "event": "dialog_closed",
            }
        ))

        # ============= PROCESS =============

        patterns.append(APIPattern(
            api_regex=r"^(CreateProcess[AW]?|ShellExecute[AW]?)$",
            category=APICategory.PROCESS,
            translator=self._translate_create_process,
        ))

        return patterns

    def translate(self, context: APICallContext) -> TranslationResult:
        """Translate an API call to semantic operation(s).

        Args:
            context: The API call context

        Returns:
            TranslationResult with semantic operations
        """
        self.stats["total"] += 1

        # Add to history
        self._add_to_history(context)

        # Try pattern matching first (fast path)
        result = self._try_pattern_match(context)
        if result.success:
            self.stats["pattern_matches"] += 1
            return result

        # Try AI translation (slow path)
        if self.ai_client:
            result = self._try_ai_translation(context)
            if result.success:
                self.stats["ai_translations"] += 1
                return result

        # Failed to translate
        self.stats["failures"] += 1
        return TranslationResult(
            success=False,
            method="none",
            reasoning=f"Unknown API: {context.api_name}",
        )

    def _try_pattern_match(self, context: APICallContext) -> TranslationResult:
        """Try to match API call against known patterns."""
        for pattern in self._patterns:
            if re.match(pattern.api_regex, context.api_name):
                # Check argument patterns if specified
                if pattern.arg_patterns:
                    if not self._match_args(context.args, pattern.arg_patterns):
                        continue

                # Use custom translator if available
                if pattern.translator:
                    return pattern.translator(context, pattern)

                # Use template
                if pattern.semantic_template:
                    return self._apply_template(context, pattern)

        return TranslationResult(success=False)

    def _match_args(self, args: Dict, patterns: Dict) -> bool:
        """Check if arguments match patterns."""
        for key, pattern in patterns.items():
            if key not in args:
                return False
            if isinstance(pattern, str):
                if not re.match(pattern, str(args[key])):
                    return False
            elif args[key] != pattern:
                return False
        return True

    def _apply_template(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Apply a semantic template to create operation."""
        template = pattern.semantic_template

        primitive = SemanticPrimitive(template["primitive"])

        # Determine target based on primitive type
        if primitive == SemanticPrimitive.ATTACH:
            target = context.args.get("path", context.args.get("filename", "unknown"))
        elif primitive == SemanticPrimitive.EMIT:
            target = template.get("event", "unknown_event")
        elif primitive == SemanticPrimitive.INVOKE:
            target = template.get("action", "unknown_action")
        else:
            target = template.get("target", "unknown")

        op = SemanticOp(
            primitive=primitive,
            target=target,
            intent=template.get("intent"),
            params=template.get("params", {}),
            source_api=context.api_name,
            source_app=context.app_name,
            source_pid=context.pid,
        )

        return TranslationResult(
            success=True,
            operation=op,
            method="pattern",
            confidence=1.0,
        )

    # ============= CUSTOM TRANSLATORS =============

    def _translate_create_file(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Translate CreateFile to semantic operation."""
        args = context.args
        path = args.get("path", args.get("lpFileName", "unknown"))

        # Determine intent from access flags
        access = args.get("access", args.get("dwDesiredAccess", 0))
        creation = args.get("creation", args.get("dwCreationDisposition", 0))

        # Parse Windows constants
        GENERIC_READ = 0x80000000
        GENERIC_WRITE = 0x40000000
        CREATE_ALWAYS = 2
        CREATE_NEW = 1
        OPEN_EXISTING = 3

        if isinstance(access, str):
            if "WRITE" in access.upper():
                intent = Intent.EDIT
            elif "READ" in access.upper():
                intent = Intent.READ
            else:
                intent = Intent.READ
        elif isinstance(access, int):
            if access & GENERIC_WRITE:
                intent = Intent.EDIT
            else:
                intent = Intent.READ
        else:
            intent = Intent.READ

        # Check for create intent
        if isinstance(creation, str):
            if "CREATE" in creation.upper():
                intent = Intent.CREATE
        elif isinstance(creation, int):
            if creation in (CREATE_ALWAYS, CREATE_NEW):
                intent = Intent.CREATE

        op = SemanticOp.attach(
            path=path,
            intent=intent,
            source_api=context.api_name,
            source_app=context.app_name,
        )
        op.source_api = context.api_name
        op.source_app = context.app_name
        op.source_pid = context.pid

        return TranslationResult(
            success=True,
            operation=op,
            method="pattern",
            confidence=1.0,
            reasoning=f"CreateFile with {intent.value} intent",
        )

    def _translate_write_file(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Translate WriteFile to semantic operation."""
        args = context.args
        bytes_written = args.get("nNumberOfBytesToWrite", args.get("bytes", 0))

        # Determine if this is a significant write
        significant = bytes_written > 100  # More than 100 bytes

        op = SemanticOp.emit(
            event=EventType.CONTENT_CHANGED,
            data={
                "bytes": bytes_written,
                "significant": significant,
            },
        )
        op.source_api = context.api_name
        op.source_app = context.app_name
        op.source_pid = context.pid

        return TranslationResult(
            success=True,
            operation=op,
            method="pattern",
            confidence=1.0,
        )

    def _translate_close_handle(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Translate CloseHandle to semantic operation."""
        # Try to get the path from context or handle mapping
        handle = context.args.get("handle", context.args.get("hObject"))
        path = context.active_document or "unknown"

        op = SemanticOp.detach(path=path)
        op.source_api = context.api_name
        op.source_app = context.app_name
        op.source_pid = context.pid

        return TranslationResult(
            success=True,
            operation=op,
            method="pattern",
            confidence=0.8,  # Lower confidence without path
        )

    def _translate_move_file(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Translate MoveFile to semantic operation."""
        args = context.args
        source = args.get("lpExistingFileName", args.get("source", ""))
        dest = args.get("lpNewFileName", args.get("dest", ""))

        op = SemanticOp.invoke(
            action="document.move",
            source=source,
            destination=dest,
        )
        op.source_api = context.api_name
        op.source_app = context.app_name

        return TranslationResult(
            success=True,
            operation=op,
            method="pattern",
            confidence=1.0,
        )

    def _translate_copy_file(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Translate CopyFile to semantic operation."""
        args = context.args
        source = args.get("lpExistingFileName", args.get("source", ""))
        dest = args.get("lpNewFileName", args.get("dest", ""))

        # This creates two operations: invoke copy + link relationship
        ops = [
            SemanticOp.invoke(
                action="document.copy",
                source=source,
                destination=dest,
            ),
            SemanticOp.link(
                source=dest,
                target=source,
                relation="copied_from",
            ),
        ]

        for op in ops:
            op.source_api = context.api_name
            op.source_app = context.app_name

        return TranslationResult(
            success=True,
            operations=ops,
            method="pattern",
            confidence=1.0,
        )

    def _translate_posix_open(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Translate POSIX open() to semantic operation."""
        args = context.args
        path = args.get("path", args.get("pathname", "unknown"))
        flags = args.get("flags", 0)

        # POSIX flags
        O_RDONLY = 0
        O_WRONLY = 1
        O_RDWR = 2
        O_CREAT = 64
        O_TRUNC = 512

        if isinstance(flags, int):
            if flags & O_CREAT:
                intent = Intent.CREATE
            elif flags & O_WRONLY or flags & O_RDWR:
                intent = Intent.EDIT
            else:
                intent = Intent.READ
        else:
            intent = Intent.READ

        op = SemanticOp.attach(path=path, intent=intent)
        op.source_api = context.api_name
        op.source_app = context.app_name
        op.source_pid = context.pid

        return TranslationResult(
            success=True,
            operation=op,
            method="pattern",
            confidence=1.0,
        )

    def _translate_fopen(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Translate C library fopen() to semantic operation."""
        args = context.args
        path = args.get("path", args.get("pathname", "unknown"))
        mode = args.get("mode", "r")

        # Parse fopen mode string
        if isinstance(mode, str):
            if "w" in mode or "a" in mode:
                if "+" in mode:
                    intent = Intent.EDIT
                else:
                    intent = Intent.CREATE if "w" in mode else Intent.EDIT
            elif "r" in mode:
                intent = Intent.EDIT if "+" in mode else Intent.READ
            else:
                intent = Intent.READ
        else:
            intent = Intent.READ

        op = SemanticOp.attach(path=path, intent=intent)
        op.source_api = context.api_name
        op.source_app = context.app_name
        op.source_pid = context.pid

        return TranslationResult(
            success=True,
            operation=op,
            method="pattern",
            confidence=1.0,
        )

    def _translate_message_box(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Translate MessageBox to semantic operation."""
        args = context.args
        text = args.get("lpText", args.get("text", ""))
        caption = args.get("lpCaption", args.get("caption", ""))

        # Analyze the message to determine intent
        text_lower = text.lower() if text else ""

        if any(w in text_lower for w in ["save", "unsaved", "changes"]):
            action = "document.save_prompt"
        elif any(w in text_lower for w in ["error", "failed", "cannot"]):
            action = "error_notification"
        elif any(w in text_lower for w in ["confirm", "are you sure", "delete"]):
            action = "confirmation_prompt"
        else:
            action = "user_notification"

        op = SemanticOp.invoke(
            action=action,
            message=text,
            title=caption,
        )
        op.source_api = context.api_name
        op.source_app = context.app_name

        return TranslationResult(
            success=True,
            operation=op,
            method="pattern",
            confidence=0.9,
        )

    def _translate_set_window_text(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Translate SetWindowText to semantic operation."""
        args = context.args
        text = args.get("lpString", args.get("text", ""))

        # Window title often contains document name
        # e.g., "document.txt - Notepad"
        document_match = re.match(r"^(.+?)\s*[-–—]\s*", text)
        if document_match:
            document_hint = document_match.group(1).strip()
        else:
            document_hint = None

        op = SemanticOp.emit(
            event=EventType.VIEW_CHANGED,
            data={
                "window_title": text,
                "document_hint": document_hint,
            },
        )
        op.source_api = context.api_name
        op.source_app = context.app_name

        return TranslationResult(
            success=True,
            operation=op,
            method="pattern",
            confidence=1.0,
        )

    def _translate_create_process(self, context: APICallContext, pattern: APIPattern) -> TranslationResult:
        """Translate CreateProcess/ShellExecute to semantic operation."""
        args = context.args
        cmd = args.get("lpCommandLine", args.get("lpFile", ""))

        op = SemanticOp.invoke(
            action="process.spawn",
            command=cmd,
            parent_app=context.app_name,
        )
        op.source_api = context.api_name
        op.source_app = context.app_name

        return TranslationResult(
            success=True,
            operation=op,
            method="pattern",
            confidence=1.0,
        )

    # ============= AI TRANSLATION =============

    def _try_ai_translation(self, context: APICallContext) -> TranslationResult:
        """Use AI to translate unknown API calls."""
        if not self.ai_client:
            return TranslationResult(success=False)

        # Check cache first
        cache_key = f"{context.api_name}:{hash(str(sorted(context.args.items())))}"
        if cache_key in self._ai_cache:
            return self._ai_cache[cache_key]

        # Build prompt
        prompt = self._build_ai_prompt(context)

        try:
            response = self.ai_client.generate("api_translation", prompt)

            # Parse response
            result = self._parse_ai_response(response, context)

            # Cache successful translations
            if result.success:
                self._ai_cache[cache_key] = result

            return result

        except Exception as e:
            return TranslationResult(
                success=False,
                reasoning=f"AI translation error: {e}",
            )

    def _build_ai_prompt(self, context: APICallContext) -> str:
        """Build prompt for AI translation."""
        recent = self._call_history[-5:] if self._call_history else []

        return f"""Translate this API call to a semantic operation.

API Call: {context.api_name}
Arguments: {context.args}
Application: {context.app_name}
Window Title: {context.window_title}
Recent API Calls: {[c['api'] for c in recent]}
Open Documents: {context.open_documents}

Semantic Primitives:
- ctx.attach(path, intent) - Open/access document (intents: read, edit, create)
- ctx.detach(path) - Close document
- ctx.emit(event, data) - Emit event (content_changed, content_copied, etc.)
- ctx.invoke(action, params) - Request action (document.save, document.print, etc.)
- ctx.link(source, target, relation) - Create relationship

What is the semantic INTENT of this API call?

Reply with JSON only:
{{"primitive": "ctx.xxx", "target": "...", "intent": "...", "params": {{}}, "confidence": 0.0-1.0, "reasoning": "..."}}
"""

    def _parse_ai_response(self, response: str, context: APICallContext) -> TranslationResult:
        """Parse AI response into TranslationResult."""
        import json

        try:
            # Extract JSON from response
            json_match = re.search(r'\{[^{}]*\}', response, re.DOTALL)
            if not json_match:
                return TranslationResult(success=False, reasoning="No JSON in response")

            data = json.loads(json_match.group())

            op = SemanticOp(
                primitive=SemanticPrimitive(data["primitive"]),
                target=data.get("target", "unknown"),
                intent=data.get("intent"),
                params=data.get("params", {}),
                source_api=context.api_name,
                source_app=context.app_name,
                source_pid=context.pid,
                confidence=data.get("confidence", 0.5),
            )

            return TranslationResult(
                success=True,
                operation=op,
                method="ai",
                confidence=data.get("confidence", 0.5),
                reasoning=data.get("reasoning", ""),
            )

        except (json.JSONDecodeError, KeyError, ValueError) as e:
            return TranslationResult(
                success=False,
                reasoning=f"Failed to parse AI response: {e}",
            )

    def _add_to_history(self, context: APICallContext):
        """Add API call to history for AI context."""
        self._call_history.append({
            "api": context.api_name,
            "args": context.args,
            "app": context.app_name,
            "time": time.time(),
        })

        # Trim history
        if len(self._call_history) > self._max_history:
            self._call_history = self._call_history[-self._max_history:]

    def get_stats(self) -> Dict[str, int]:
        """Get translation statistics."""
        return dict(self.stats)

    def clear_cache(self):
        """Clear AI translation cache."""
        self._ai_cache.clear()


# Global translator instance
_translator: Optional[SemanticAPITranslator] = None


def get_translator(ai_client=None, kernel=None) -> SemanticAPITranslator:
    """Get or create global translator."""
    global _translator
    if _translator is None:
        _translator = SemanticAPITranslator(ai_client, kernel)
    return _translator


def translate_api_call(
    api_name: str,
    args: Dict[str, Any],
    app_name: str = "",
    **context_kwargs
) -> TranslationResult:
    """Convenience function to translate an API call."""
    context = APICallContext(
        api_name=api_name,
        args=args,
        app_name=app_name,
        **context_kwargs
    )
    return get_translator().translate(context)
