#!/usr/bin/env python3
"""
Demo: Semantic API Translator
=============================

Demonstrates how legacy API calls are translated to semantic operations.
This is the core of the AI Synthesis Layer.

Run: python demo_api_translator.py
"""

import sys
sys.path.insert(0, ".")

from ide.legacy import (
    SemanticAPITranslator,
    APICallContext,
    SemanticOp,
    translate_api_call,
)


def print_translation(context, result):
    """Pretty print a translation result."""
    print(f"\n{'='*60}")
    print(f"API Call: {context.api_name}")
    print(f"Args: {context.args}")
    print(f"App: {context.app_name}")
    print(f"{'-'*60}")

    if result.success:
        for op in result.operations:
            print(f"[OK] Semantic: {op.primitive.value}")
            print(f"  Target: {op.target}")
            if op.intent:
                print(f"  Intent: {op.intent}")
            if op.params:
                print(f"  Params: {op.params}")
            print(f"  Confidence: {op.confidence:.0%}")
    else:
        print(f"[FAIL] Failed: {result.reasoning}")

    print(f"  Method: {result.method}")


def main():
    print("="*60)
    print("  SEMANTIC API TRANSLATOR DEMO")
    print("  Legacy API -> Semantic Operations")
    print("="*60)

    # Create translator
    translator = SemanticAPITranslator()

    # Test cases: (api_name, args, app_name)
    test_cases = [
        # File operations
        (
            "CreateFileW",
            {"path": "design.psd", "access": "GENERIC_WRITE", "creation": "OPEN_EXISTING"},
            "photoshop.exe"
        ),
        (
            "CreateFileW",
            {"path": "report.docx", "access": "GENERIC_READ"},
            "winword.exe"
        ),
        (
            "CreateFileW",
            {"path": "new_doc.txt", "access": "GENERIC_WRITE", "creation": "CREATE_ALWAYS"},
            "notepad.exe"
        ),
        (
            "WriteFile",
            {"handle": 0x1234, "nNumberOfBytesToWrite": 4096},
            "notepad.exe"
        ),
        (
            "CloseHandle",
            {"hObject": 0x1234},
            "notepad.exe"
        ),
        (
            "DeleteFileW",
            {"lpFileName": "temp.txt"},
            "explorer.exe"
        ),

        # POSIX equivalents
        (
            "open",
            {"path": "/home/user/doc.md", "flags": 2},  # O_RDWR
            "vim"
        ),
        (
            "write",
            {"fd": 3, "bytes": 512},
            "vim"
        ),

        # Clipboard
        (
            "SetClipboardData",
            {"format": "CF_UNICODETEXT", "data": "Hello World"},
            "notepad.exe"
        ),
        (
            "GetClipboardData",
            {"format": "CF_UNICODETEXT"},
            "word.exe"
        ),

        # Dialogs
        (
            "GetOpenFileNameW",
            {"filter": "*.psd;*.png"},
            "photoshop.exe"
        ),
        (
            "GetSaveFileNameW",
            {"defaultName": "untitled.psd"},
            "photoshop.exe"
        ),
        (
            "PrintDlgW",
            {"flags": 0},
            "word.exe"
        ),
        (
            "MessageBoxW",
            {"lpText": "Do you want to save changes?", "lpCaption": "Notepad"},
            "notepad.exe"
        ),

        # Window management
        (
            "SetWindowTextW",
            {"text": "document.txt - Notepad"},
            "notepad.exe"
        ),
        (
            "SetForegroundWindow",
            {"hwnd": 0x12345},
            "photoshop.exe"
        ),

        # File copy (creates relationship!)
        (
            "CopyFileW",
            {"lpExistingFileName": "original.psd", "lpNewFileName": "backup.psd"},
            "explorer.exe"
        ),

        # Unknown API (would use AI in production)
        (
            "SomeUnknownAPI",
            {"param1": "value1"},
            "unknown.exe"
        ),
    ]

    # Run translations
    for api_name, args, app_name in test_cases:
        context = APICallContext(
            api_name=api_name,
            args=args,
            app_name=app_name,
            pid=1234,
        )
        result = translator.translate(context)
        print_translation(context, result)

    # Print stats
    print(f"\n{'='*60}")
    print("TRANSLATION STATISTICS")
    print("="*60)
    stats = translator.get_stats()
    print(f"Total calls:      {stats['total']}")
    print(f"Pattern matches:  {stats['pattern_matches']}")
    print(f"AI translations:  {stats['ai_translations']}")
    print(f"Failures:         {stats['failures']}")
    print(f"Success rate:     {(stats['pattern_matches'] + stats['ai_translations']) / stats['total'] * 100:.1f}%")

    # Demo: Building semantic operations directly
    print(f"\n{'='*60}")
    print("SEMANTIC OPERATIONS API")
    print("="*60)

    # These are the primitives that the kernel executes
    ops = [
        SemanticOp.attach("design.psd", intent="edit"),
        SemanticOp.emit("content_changed", data={"bytes": 1024}),
        SemanticOp.invoke("document.save"),
        SemanticOp.link("backup.psd", "design.psd", relation="backup_of"),
        SemanticOp.detach("design.psd"),
    ]

    for op in ops:
        print(f"\n{op.primitive.value}(")
        print(f"  target = {repr(op.target)},")
        if op.intent:
            print(f"  intent = {repr(op.intent)},")
        if op.params:
            print(f"  params = {op.params},")
        print(")")


if __name__ == "__main__":
    main()
