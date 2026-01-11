#!/usr/bin/env python3
"""
Demo: Semantic Kernel
=====================

Shows the semantic kernel simulating syscalls.
Everything is semantic memory - no real filesystem.

Run: python demo_kernel.py
"""

import sys
sys.path.insert(0, ".")

from kernel import SemanticKernel, SyscallSimulator
from kernel.memory import MemoryType
from kernel.syscall import FileAccess, FileCreation, FileFlags


def print_sep(title):
    print(f"\n{'='*60}")
    print(f"  {title}")
    print(f"{'='*60}")


def main():
    print("="*60)
    print("  SEMANTIC KERNEL DEMO")
    print("  Everything is Semantic Memory")
    print("="*60)

    # Initialize kernel
    kernel = SemanticKernel()
    sim = SyscallSimulator(kernel)

    # Create a process (simulating notepad.exe)
    print_sep("1. CREATE PROCESS")
    proc = kernel.create_process("notepad.exe")
    sim.set_process(proc.id)
    print(f"Created process: {proc.name} (PID: {proc.id})")

    # Subscribe to events
    events_log = []
    def log_event(event, data):
        events_log.append((event, data))
        # Only print important events
        if event in ("content.changed", "content.saved", "documents.linked"):
            print(f"  [EVENT] {event}: {data}")

    kernel.subscribe("*", log_event)

    # Simulate Windows app creating a file
    print_sep("2. CREATE FILE (Windows API)")
    result = sim.CreateFileW(
        "document.txt",
        access=FileAccess.GENERIC_WRITE,
        creation=FileCreation.CREATE_ALWAYS,
    )
    handle1 = result.return_value
    print(f"CreateFileW('document.txt', WRITE, CREATE) -> Handle: 0x{handle1:X}")

    # Write to file
    print_sep("3. WRITE FILE")
    content = b"Hello from Semantic OS!\n\nThis is stored as semantic memory, not a file."
    result = sim.WriteFile(handle1, content)
    print(f"WriteFile(handle, {len(content)} bytes) -> {result.return_value} bytes written")

    # Write more
    result = sim.WriteFile(handle1, b"\n\nAppending more content...")
    print(f"WriteFile(handle, 25 bytes) -> {result.return_value} bytes written")

    # Close file
    print_sep("4. CLOSE FILE")
    result = sim.CloseHandle(handle1)
    print(f"CloseHandle(0x{handle1:X}) -> {result.success}")

    # Simulate POSIX app opening same file
    print_sep("5. OPEN FILE (POSIX API)")
    result = sim.open("document.txt", FileFlags.O_RDWR)
    fd = result.return_value
    print(f"open('document.txt', O_RDWR) -> fd: {fd}")

    # Read content
    result = sim.read(fd, 1000)
    data, bytes_read = result.return_value
    print(f"read(fd, 1000) -> {bytes_read} bytes")
    print(f"Content preview: {data[:50].decode()}...")

    sim.close(fd)

    # Copy file
    print_sep("6. COPY FILE")
    result = sim.CopyFileW("document.txt", "document_backup.txt")
    print(f"CopyFileW('document.txt', 'document_backup.txt') -> {result.success}")

    # The copy creates a semantic link!
    print("  (This automatically creates a 'copied_from' relationship)")

    # Use clipboard
    print_sep("7. CLIPBOARD")
    sim.SetClipboardData("CF_UNICODETEXT", "Semantic clipboard data!")
    print("SetClipboardData(CF_UNICODETEXT, 'Semantic clipboard data!')")

    result = sim.GetClipboardData("CF_UNICODETEXT")
    print(f"GetClipboardData(CF_UNICODETEXT) -> '{result.return_value}'")

    # Create a window
    print_sep("8. WINDOW MANAGEMENT")
    result = sim.CreateWindowExW("EDIT", "document.txt - Notepad")
    hwnd = result.return_value
    print(f"CreateWindowExW('EDIT', 'document.txt - Notepad') -> hwnd: 0x{hwnd:X}")

    sim.ShowWindow(hwnd, 1)
    print(f"ShowWindow(hwnd, SW_SHOW)")

    sim.SetWindowTextW(hwnd, "document.txt* - Notepad")
    print(f"SetWindowTextW(hwnd, 'document.txt* - Notepad')")

    # Query semantic memory
    print_sep("9. SEMANTIC QUERIES")

    print("\nQuery: 'documents'")
    results = kernel.query("documents", type=MemoryType.DOCUMENT, limit=5)
    for entry, score in results:
        path = entry.metadata.get("path", entry.id)
        print(f"  [{score:.2f}] {path}")

    print("\nQuery: 'clipboard operations'")
    results = kernel.query("clipboard", type=MemoryType.EVENT, limit=3)
    for entry, score in results:
        event = entry.metadata.get("event", "unknown")
        print(f"  [{score:.2f}] {event}: {entry.content[:40]}...")

    print("\nQuery: 'content changes'")
    results = kernel.query("content changed", type=MemoryType.EVENT, limit=3)
    for entry, score in results:
        print(f"  [{score:.2f}] {entry.content[:50]}...")

    # Show kernel state
    print_sep("10. KERNEL STATE")

    stats = kernel.stats()
    print(f"\nKernel Statistics:")
    print(f"  Uptime: {stats['uptime']:.2f}s")
    print(f"  Processes: {stats['processes']}")
    print(f"  Open Handles: {stats['open_handles']}")
    print(f"  Memory Entries: {stats['memory']['total_entries']}")
    print(f"  By Type: {stats['memory']['by_type']}")

    sim_stats = sim.stats()
    print(f"\nSyscall Simulator:")
    print(f"  Open Handles: {sim_stats['open_handles']}")
    print(f"  Windows: {sim_stats['windows']}")
    print(f"  Clipboard Formats: {sim_stats['clipboard_formats']}")

    print(f"\nTotal Events Logged: {len(events_log)}")

    # Show all documents
    print_sep("11. ALL DOCUMENTS IN MEMORY")
    docs = kernel.get_documents()
    for doc in docs:
        path = doc.metadata.get("path", doc.id)
        size = len(doc.content)
        print(f"  {path}: {size} bytes, v{doc.version}")
        if doc.metadata.get("copied_from"):
            print(f"    -> copied from: {doc.metadata['copied_from']}")

    # Cleanup
    print_sep("12. SHUTDOWN")
    kernel.shutdown()
    print("Kernel shutdown complete.")

    print("\n" + "="*60)
    print("  DEMO COMPLETE")
    print("  All operations were semantic - no real files created!")
    print("="*60)


if __name__ == "__main__":
    main()
