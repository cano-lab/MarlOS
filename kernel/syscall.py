"""
Syscall Simulator
=================

Simulates system calls by translating them to semantic kernel operations.
Apps think they're talking to a real OS, but everything is semantic.

The app never touches real filesystem, real clipboard, etc.
Everything goes through the semantic kernel.
"""

import time
import struct
from dataclasses import dataclass, field
from typing import Any, Dict, Optional, Tuple
from enum import IntEnum, IntFlag

from .core import SemanticKernel, Handle, Intent
from .memory import MemoryType


# Windows-like constants for compatibility
class FileAccess(IntFlag):
    GENERIC_READ = 0x80000000
    GENERIC_WRITE = 0x40000000
    GENERIC_EXECUTE = 0x20000000
    GENERIC_ALL = 0x10000000


class FileCreation(IntEnum):
    CREATE_NEW = 1
    CREATE_ALWAYS = 2
    OPEN_EXISTING = 3
    OPEN_ALWAYS = 4
    TRUNCATE_EXISTING = 5


class FileFlags(IntFlag):
    O_RDONLY = 0
    O_WRONLY = 1
    O_RDWR = 2
    O_CREAT = 64
    O_TRUNC = 512
    O_APPEND = 1024


@dataclass
class SyscallResult:
    """Result of a simulated syscall."""
    success: bool
    return_value: Any = None
    error_code: int = 0
    error_message: str = ""


class SyscallSimulator:
    """Simulates syscalls using the semantic kernel.

    Apps make syscalls, we translate to semantic operations,
    and return simulated responses.

    Usage:
        kernel = SemanticKernel()
        sim = SyscallSimulator(kernel)

        # App calls CreateFileW
        result = sim.CreateFileW("doc.txt", GENERIC_WRITE, CREATE_ALWAYS)
        handle = result.return_value  # Fake handle

        # App calls WriteFile
        result = sim.WriteFile(handle, b"Hello World")
        bytes_written = result.return_value

        # App calls CloseHandle
        sim.CloseHandle(handle)
    """

    def __init__(self, kernel: SemanticKernel):
        self.kernel = kernel
        self._current_process: Optional[str] = None

        # Handle mapping: fake OS handle (int) -> semantic Handle
        self._handle_map: Dict[int, Handle] = {}
        self._next_handle = 0x1000

        # Clipboard simulation
        self._clipboard: Dict[str, Any] = {}

        # Window simulation
        self._windows: Dict[int, Dict] = {}
        self._next_hwnd = 0x10000

    def set_process(self, process_id: str):
        """Set current process context."""
        self._current_process = process_id

    def _alloc_handle(self, semantic_handle: Handle) -> int:
        """Allocate a fake OS handle."""
        fake_handle = self._next_handle
        self._next_handle += 4
        self._handle_map[fake_handle] = semantic_handle
        return fake_handle

    def _get_handle(self, fake_handle: int) -> Optional[Handle]:
        """Get semantic handle from fake handle."""
        return self._handle_map.get(fake_handle)

    def _free_handle(self, fake_handle: int):
        """Free a fake handle."""
        if fake_handle in self._handle_map:
            del self._handle_map[fake_handle]

    # ==================== WINDOWS FILE API ====================

    def CreateFileW(
        self,
        path: str,
        access: int = FileAccess.GENERIC_READ,
        creation: int = FileCreation.OPEN_EXISTING,
        **kwargs
    ) -> SyscallResult:
        """Simulate CreateFileW."""

        # Determine intent from flags
        if access & FileAccess.GENERIC_WRITE:
            if creation in (FileCreation.CREATE_NEW, FileCreation.CREATE_ALWAYS):
                intent = Intent.CREATE
            else:
                intent = Intent.EDIT
        else:
            intent = Intent.READ

        # Create semantic handle
        try:
            handle = self.kernel.attach(path, intent, self._current_process)
            fake_handle = self._alloc_handle(handle)

            return SyscallResult(
                success=True,
                return_value=fake_handle,
            )
        except Exception as e:
            return SyscallResult(
                success=False,
                return_value=-1,  # INVALID_HANDLE_VALUE
                error_code=2,  # ERROR_FILE_NOT_FOUND
                error_message=str(e),
            )

    def ReadFile(
        self,
        handle: int,
        size: int,
        **kwargs
    ) -> SyscallResult:
        """Simulate ReadFile."""
        semantic_handle = self._get_handle(handle)
        if not semantic_handle:
            return SyscallResult(
                success=False,
                error_code=6,  # ERROR_INVALID_HANDLE
            )

        try:
            content = self.kernel.read(semantic_handle, size)
            return SyscallResult(
                success=True,
                return_value=(content.encode('utf-8'), len(content)),
            )
        except Exception as e:
            return SyscallResult(
                success=False,
                error_code=5,  # ERROR_ACCESS_DENIED
                error_message=str(e),
            )

    def WriteFile(
        self,
        handle: int,
        data: bytes,
        **kwargs
    ) -> SyscallResult:
        """Simulate WriteFile."""
        semantic_handle = self._get_handle(handle)
        if not semantic_handle:
            return SyscallResult(
                success=False,
                error_code=6,  # ERROR_INVALID_HANDLE
            )

        try:
            # Convert bytes to string for semantic storage
            if isinstance(data, bytes):
                content = data.decode('utf-8', errors='replace')
            else:
                content = str(data)

            bytes_written = self.kernel.write(semantic_handle, content)
            return SyscallResult(
                success=True,
                return_value=bytes_written,
            )
        except PermissionError:
            return SyscallResult(
                success=False,
                error_code=5,  # ERROR_ACCESS_DENIED
            )
        except Exception as e:
            return SyscallResult(
                success=False,
                error_code=1,  # ERROR_INVALID_FUNCTION
                error_message=str(e),
            )

    def CloseHandle(self, handle: int, **kwargs) -> SyscallResult:
        """Simulate CloseHandle."""
        semantic_handle = self._get_handle(handle)
        if not semantic_handle:
            return SyscallResult(
                success=False,
                error_code=6,
            )

        self.kernel.detach(semantic_handle)
        self._free_handle(handle)

        return SyscallResult(success=True, return_value=True)

    def DeleteFileW(self, path: str, **kwargs) -> SyscallResult:
        """Simulate DeleteFileW."""
        try:
            self.kernel.invoke("document.delete", path=path)
            return SyscallResult(success=True, return_value=True)
        except Exception as e:
            return SyscallResult(
                success=False,
                error_code=2,
                error_message=str(e),
            )

    def CopyFileW(
        self,
        source: str,
        dest: str,
        **kwargs
    ) -> SyscallResult:
        """Simulate CopyFileW."""
        try:
            self.kernel.invoke("document.copy", source=source, destination=dest)
            return SyscallResult(success=True, return_value=True)
        except Exception as e:
            return SyscallResult(
                success=False,
                error_code=2,
                error_message=str(e),
            )

    def MoveFileW(
        self,
        source: str,
        dest: str,
        **kwargs
    ) -> SyscallResult:
        """Simulate MoveFileW."""
        try:
            self.kernel.invoke("document.move", source=source, destination=dest)
            return SyscallResult(success=True, return_value=True)
        except Exception as e:
            return SyscallResult(
                success=False,
                error_code=2,
                error_message=str(e),
            )

    # ==================== POSIX FILE API ====================

    def open(self, path: str, flags: int = 0, mode: int = 0o644) -> SyscallResult:
        """Simulate POSIX open()."""
        # Determine intent from flags
        if flags & FileFlags.O_CREAT:
            intent = Intent.CREATE
        elif flags & FileFlags.O_WRONLY or flags & FileFlags.O_RDWR:
            intent = Intent.EDIT
        else:
            intent = Intent.READ

        try:
            handle = self.kernel.attach(path, intent, self._current_process)
            fd = self._alloc_handle(handle)

            return SyscallResult(success=True, return_value=fd)
        except Exception as e:
            return SyscallResult(
                success=False,
                return_value=-1,
                error_code=2,  # ENOENT
                error_message=str(e),
            )

    def read(self, fd: int, size: int) -> SyscallResult:
        """Simulate POSIX read()."""
        return self.ReadFile(fd, size)

    def write(self, fd: int, data: bytes) -> SyscallResult:
        """Simulate POSIX write()."""
        return self.WriteFile(fd, data)

    def close(self, fd: int) -> SyscallResult:
        """Simulate POSIX close()."""
        return self.CloseHandle(fd)

    def unlink(self, path: str) -> SyscallResult:
        """Simulate POSIX unlink()."""
        return self.DeleteFileW(path)

    def rename(self, old: str, new: str) -> SyscallResult:
        """Simulate POSIX rename()."""
        return self.MoveFileW(old, new)

    # ==================== CLIPBOARD ====================

    def SetClipboardData(self, format: str, data: Any) -> SyscallResult:
        """Simulate SetClipboardData."""
        self._clipboard[format] = data

        self.kernel.emit("clipboard.set", {
            "format": format,
            "size": len(str(data)) if data else 0,
        })

        # Store in memory for semantic access
        self.kernel.memory.store(
            content=str(data) if data else "",
            type=MemoryType.EVENT,
            metadata={
                "event": "clipboard",
                "format": format,
                "operation": "copy",
            },
        )

        return SyscallResult(success=True, return_value=True)

    def GetClipboardData(self, format: str) -> SyscallResult:
        """Simulate GetClipboardData."""
        data = self._clipboard.get(format)

        self.kernel.emit("clipboard.get", {"format": format})

        return SyscallResult(
            success=data is not None,
            return_value=data,
            error_code=0 if data else 1,
        )

    def EmptyClipboard(self) -> SyscallResult:
        """Simulate EmptyClipboard."""
        self._clipboard.clear()
        self.kernel.emit("clipboard.cleared", {})
        return SyscallResult(success=True, return_value=True)

    # ==================== WINDOW MANAGEMENT ====================

    def CreateWindowExW(
        self,
        class_name: str,
        window_name: str,
        style: int = 0,
        x: int = 0, y: int = 0,
        width: int = 800, height: int = 600,
        **kwargs
    ) -> SyscallResult:
        """Simulate CreateWindowExW."""
        hwnd = self._next_hwnd
        self._next_hwnd += 1

        self._windows[hwnd] = {
            "class": class_name,
            "title": window_name,
            "style": style,
            "rect": (x, y, width, height),
            "visible": False,
        }

        self.kernel.emit("window.created", {
            "hwnd": hwnd,
            "title": window_name,
        })

        return SyscallResult(success=True, return_value=hwnd)

    def SetWindowTextW(self, hwnd: int, text: str) -> SyscallResult:
        """Simulate SetWindowTextW."""
        if hwnd in self._windows:
            self._windows[hwnd]["title"] = text

            self.kernel.emit("window.title_changed", {
                "hwnd": hwnd,
                "title": text,
            })

            return SyscallResult(success=True, return_value=True)

        return SyscallResult(success=False, error_code=1400)  # ERROR_INVALID_WINDOW_HANDLE

    def DestroyWindow(self, hwnd: int) -> SyscallResult:
        """Simulate DestroyWindow."""
        if hwnd in self._windows:
            del self._windows[hwnd]
            self.kernel.emit("window.destroyed", {"hwnd": hwnd})
            return SyscallResult(success=True, return_value=True)

        return SyscallResult(success=False, error_code=1400)

    def ShowWindow(self, hwnd: int, cmd: int) -> SyscallResult:
        """Simulate ShowWindow."""
        if hwnd in self._windows:
            self._windows[hwnd]["visible"] = cmd != 0
            self.kernel.emit("window.visibility_changed", {
                "hwnd": hwnd,
                "visible": cmd != 0,
            })
            return SyscallResult(success=True, return_value=True)

        return SyscallResult(success=False, error_code=1400)

    def SetForegroundWindow(self, hwnd: int) -> SyscallResult:
        """Simulate SetForegroundWindow."""
        if hwnd in self._windows:
            self.kernel.emit("window.focused", {
                "hwnd": hwnd,
                "title": self._windows[hwnd].get("title", ""),
            })
            return SyscallResult(success=True, return_value=True)

        return SyscallResult(success=False, error_code=1400)

    # ==================== DIALOGS ====================

    def MessageBoxW(
        self,
        hwnd: int,
        text: str,
        caption: str,
        type: int = 0,
    ) -> SyscallResult:
        """Simulate MessageBoxW."""
        self.kernel.emit("dialog.message", {
            "text": text,
            "caption": caption,
            "type": type,
        })

        # Return IDOK (1) for now
        return SyscallResult(success=True, return_value=1)

    def GetOpenFileNameW(self, filter: str = "*.*") -> SyscallResult:
        """Simulate GetOpenFileNameW."""
        self.kernel.emit("dialog.open_file", {"filter": filter})

        # Query recent documents
        results = self.kernel.query(
            "recent documents",
            type=MemoryType.DOCUMENT,
            limit=10,
        )

        if results:
            # Return first document path
            path = results[0][0].metadata.get("path", "document.txt")
            return SyscallResult(success=True, return_value=path)

        return SyscallResult(success=False, error_code=1)

    def GetSaveFileNameW(self, default_name: str = "untitled.txt") -> SyscallResult:
        """Simulate GetSaveFileNameW."""
        self.kernel.emit("dialog.save_file", {"default": default_name})

        return SyscallResult(success=True, return_value=default_name)

    # ==================== PROCESS ====================

    def GetCurrentProcessId(self) -> SyscallResult:
        """Simulate GetCurrentProcessId."""
        return SyscallResult(
            success=True,
            return_value=int(self._current_process or "0", 16) if self._current_process else 0,
        )

    def CreateProcessW(
        self,
        application: str,
        command_line: str = None,
        **kwargs
    ) -> SyscallResult:
        """Simulate CreateProcessW."""
        proc = self.kernel.create_process(application)

        self.kernel.emit("process.spawned", {
            "name": application,
            "pid": proc.id,
            "parent": self._current_process,
        })

        return SyscallResult(
            success=True,
            return_value={"pid": proc.id, "handle": self._alloc_handle(None)},
        )

    # ==================== DISPATCH ====================

    def dispatch(self, syscall: str, **args) -> SyscallResult:
        """Dispatch a syscall by name.

        Args:
            syscall: Syscall name (e.g., "CreateFileW", "open")
            **args: Syscall arguments

        Returns:
            SyscallResult
        """
        # Map syscall to method
        method = getattr(self, syscall, None)

        if method and callable(method):
            return method(**args)
        else:
            return SyscallResult(
                success=False,
                error_code=1,
                error_message=f"Unknown syscall: {syscall}",
            )

    def stats(self) -> Dict[str, Any]:
        """Get simulator statistics."""
        return {
            "open_handles": len(self._handle_map),
            "windows": len(self._windows),
            "clipboard_formats": list(self._clipboard.keys()),
            "current_process": self._current_process,
        }
