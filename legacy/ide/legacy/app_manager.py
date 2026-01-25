"""
Legacy App Manager
==================

Coordinates legacy app launching, monitoring, and semantic integration.
"""

from dataclasses import dataclass, field
from enum import Enum
from typing import Optional, Dict, Any, List, Callable
import subprocess
import os
import sys


class RuntimeType(str, Enum):
    """Type of virtualization runtime to use."""
    DOCKER = "docker"           # Docker container
    MICROVM = "microvm"         # Firecracker/QEMU micro-VM
    WASM = "wasm"               # WebAssembly sandbox
    NATIVE = "native"           # Run directly (development only)
    WINE = "wine"               # Wine for Windows apps (full GPU!)
    PROTON = "proton"           # Proton for games (Steam's Wine)


@dataclass
class AppConfig:
    """Configuration for a legacy application."""
    name: str
    runtime: RuntimeType = RuntimeType.DOCKER
    image: str = ""                     # Docker image or VM image
    executable: str = ""                # Path to executable
    file_associations: List[str] = field(default_factory=list)  # [".psd", ".ai"]

    # Resource limits
    memory: str = "2G"
    cpu_cores: float = 2.0
    gpu_passthrough: bool = False

    # Permissions
    network_access: bool = False
    clipboard_access: bool = True
    filesystem_access: str = "document"  # "document", "workspace", "full"

    # Monitoring
    enable_gui_monitoring: bool = True
    enable_ai_interpretation: bool = True


@dataclass
class RunningApp:
    """Represents a running legacy application instance."""
    app_id: str
    config: AppConfig
    process: Any = None
    container_id: Optional[str] = None
    vm_id: Optional[str] = None
    monitor: Any = None
    document_path: Optional[str] = None

    def is_running(self) -> bool:
        """Check if the app is still running."""
        if self.process:
            return self.process.poll() is None
        return False


class LegacyAppManager:
    """Manages legacy application lifecycle and integration.

    This is the main entry point for running legacy apps in Semantic OS.
    It coordinates between different runtimes (Docker, Micro-VM, WASM)
    and the AI synthesis layer.

    Usage:
        manager = LegacyAppManager(kernel)

        # Register an app
        manager.register_app(AppConfig(
            name="photoshop",
            runtime=RuntimeType.MICROVM,
            image="windows-photoshop",
            file_associations=[".psd", ".ai", ".psb"]
        ))

        # Launch with a document
        app = manager.launch("photoshop", document="design.psd")

        # Events are automatically emitted to kernel
    """

    # Default app configurations
    DEFAULT_APPS: Dict[str, AppConfig] = {
        # Native Linux apps
        "gimp": AppConfig(
            name="gimp",
            runtime=RuntimeType.NATIVE,
            executable="gimp",
            file_associations=[".xcf", ".png", ".jpg", ".tiff"],
        ),
        "inkscape": AppConfig(
            name="inkscape",
            runtime=RuntimeType.NATIVE,
            executable="inkscape",
            file_associations=[".svg", ".eps"],
        ),
        "libreoffice": AppConfig(
            name="libreoffice",
            runtime=RuntimeType.NATIVE,
            executable="libreoffice",
            file_associations=[".odt", ".ods", ".odp"],
        ),
        "vscode": AppConfig(
            name="vscode",
            runtime=RuntimeType.NATIVE,
            executable="code",
            file_associations=[".py", ".js", ".ts", ".md", ".json"],
        ),
        "blender": AppConfig(
            name="blender",
            runtime=RuntimeType.NATIVE,
            executable="blender",
            file_associations=[".blend", ".fbx", ".obj"],
        ),
        # Windows apps via Wine (full GPU performance!)
        "photoshop": AppConfig(
            name="photoshop",
            runtime=RuntimeType.WINE,
            executable="C:/Program Files/Adobe/Adobe Photoshop 2024/Photoshop.exe",
            file_associations=[".psd", ".psb"],
        ),
        "illustrator": AppConfig(
            name="illustrator",
            runtime=RuntimeType.WINE,
            executable="C:/Program Files/Adobe/Adobe Illustrator 2024/Illustrator.exe",
            file_associations=[".ai"],
        ),
        "msword": AppConfig(
            name="msword",
            runtime=RuntimeType.WINE,
            executable="C:/Program Files/Microsoft Office/root/Office16/WINWORD.EXE",
            file_associations=[".doc", ".docx"],
        ),
        "msexcel": AppConfig(
            name="msexcel",
            runtime=RuntimeType.WINE,
            executable="C:/Program Files/Microsoft Office/root/Office16/EXCEL.EXE",
            file_associations=[".xls", ".xlsx"],
        ),
    }

    def __init__(self, kernel=None, event_callback: Callable = None):
        """Initialize the legacy app manager.

        Args:
            kernel: Semantic OS kernel for event emission
            event_callback: Optional callback for semantic events
        """
        self.kernel = kernel
        self.event_callback = event_callback
        self.apps: Dict[str, AppConfig] = dict(self.DEFAULT_APPS)
        self.running: Dict[str, RunningApp] = {}

        # Lazy-loaded runtimes
        self._docker_runtime = None
        self._microvm_runtime = None
        self._wasm_runtime = None
        self._wine_runtime = None
        self._gui_monitor_factory = None
        self._ai_interpreter = None

    def register_app(self, config: AppConfig):
        """Register an application configuration."""
        self.apps[config.name] = config

    def get_app_for_file(self, file_path: str) -> Optional[str]:
        """Find the best app for a given file type."""
        ext = os.path.splitext(file_path)[1].lower()

        for name, config in self.apps.items():
            if ext in config.file_associations:
                return name

        return None

    def launch(
        self,
        app_name: str,
        document: str = None,
        config_override: Dict[str, Any] = None
    ) -> Optional[RunningApp]:
        """Launch a legacy application.

        Args:
            app_name: Name of the registered app
            document: Optional document path to open
            config_override: Override specific config values

        Returns:
            RunningApp instance or None if launch failed
        """
        if app_name not in self.apps:
            print(f"[Legacy] Unknown app: {app_name}")
            return None

        config = self.apps[app_name]

        # Apply overrides
        if config_override:
            config = AppConfig(**{**config.__dict__, **config_override})

        # Generate unique app ID
        import uuid
        app_id = f"{app_name}_{str(uuid.uuid4())[:8]}"

        # Launch based on runtime type
        try:
            if config.runtime == RuntimeType.DOCKER:
                running = self._launch_docker(app_id, config, document)
            elif config.runtime == RuntimeType.MICROVM:
                running = self._launch_microvm(app_id, config, document)
            elif config.runtime == RuntimeType.WASM:
                running = self._launch_wasm(app_id, config, document)
            elif config.runtime in (RuntimeType.WINE, RuntimeType.PROTON):
                running = self._launch_wine(app_id, config, document)
            else:
                running = self._launch_native(app_id, config, document)

            if running:
                self.running[app_id] = running

                # Start GUI monitoring if enabled
                if config.enable_gui_monitoring:
                    self._start_monitoring(running)

                # Emit launch event
                self._emit_event({
                    "type": "app_launched",
                    "app_name": app_name,
                    "app_id": app_id,
                    "document": document,
                    "runtime": config.runtime.value,
                })

            return running

        except Exception as e:
            print(f"[Legacy] Failed to launch {app_name}: {e}")
            return None

    def _launch_docker(self, app_id: str, config: AppConfig, document: str) -> Optional[RunningApp]:
        """Launch app in Docker container."""
        # Check Docker availability
        try:
            result = subprocess.run(
                ["docker", "version"],
                capture_output=True,
                timeout=5
            )
            if result.returncode != 0:
                print("[Legacy] Docker not available")
                return None
        except Exception:
            print("[Legacy] Docker not available")
            return None

        # Build docker run command
        cmd = [
            "docker", "run",
            "-d",  # Detached
            "--name", app_id,
            "--memory", config.memory,
            f"--cpus={config.cpu_cores}",
        ]

        # Display forwarding (Linux)
        if sys.platform.startswith("linux"):
            display = os.environ.get("DISPLAY", ":0")
            cmd.extend([
                "-e", f"DISPLAY={display}",
                "-v", "/tmp/.X11-unix:/tmp/.X11-unix",
            ])

        # Network
        if not config.network_access:
            cmd.extend(["--network", "none"])

        # Mount document
        if document:
            doc_dir = os.path.dirname(os.path.abspath(document))
            doc_name = os.path.basename(document)
            cmd.extend([
                "-v", f"{doc_dir}:/data:rw",
                "-e", f"DOCUMENT=/data/{doc_name}",
            ])

        # Image
        cmd.append(config.image)

        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
            if result.returncode == 0:
                container_id = result.stdout.strip()
                return RunningApp(
                    app_id=app_id,
                    config=config,
                    container_id=container_id,
                    document_path=document,
                )
        except Exception as e:
            print(f"[Legacy] Docker launch error: {e}")

        return None

    def _launch_microvm(self, app_id: str, config: AppConfig, document: str) -> Optional[RunningApp]:
        """Launch app in micro-VM (stub - requires Firecracker setup)."""
        print(f"[Legacy] Micro-VM support not yet implemented")
        print(f"[Legacy] Would launch: {config.image} with {config.memory} RAM")

        # Placeholder - actual implementation requires Firecracker/QEMU setup
        return None

    def _launch_wasm(self, app_id: str, config: AppConfig, document: str) -> Optional[RunningApp]:
        """Launch app in WebAssembly sandbox (stub)."""
        print(f"[Legacy] WASM support not yet implemented")
        return None

    def _launch_native(self, app_id: str, config: AppConfig, document: str) -> Optional[RunningApp]:
        """Launch app natively (development/testing)."""
        cmd = [config.executable]
        if document:
            cmd.append(document)

        try:
            process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            return RunningApp(
                app_id=app_id,
                config=config,
                process=process,
                document_path=document,
            )
        except Exception as e:
            print(f"[Legacy] Native launch error: {e}")

        return None

    def _launch_wine(self, app_id: str, config: AppConfig, document: str) -> Optional[RunningApp]:
        """Launch Windows app via Wine/Proton (full GPU performance!)."""
        # Lazy load Wine runtime
        if self._wine_runtime is None:
            try:
                from .wine_runtime import WineRuntime, WineAppConfig, ProtonRuntime
                if config.runtime == RuntimeType.PROTON:
                    self._wine_runtime = ProtonRuntime()
                else:
                    self._wine_runtime = WineRuntime()
            except ImportError as e:
                print(f"[Legacy] Wine runtime not available: {e}")
                return None

        if not self._wine_runtime.is_available():
            print("[Legacy] Wine is not installed")
            print("[Legacy] Install with: sudo apt install wine64 wine32")
            return None

        # Convert AppConfig to WineAppConfig
        from .wine_runtime import WineAppConfig as WineConfig
        wine_config = WineConfig(
            name=config.name,
            executable=config.executable,
            file_associations=config.file_associations,
            use_dxvk=config.gpu_passthrough,  # Use DXVK for GPU apps
        )

        try:
            process = self._wine_runtime.run(wine_config, document)
            return RunningApp(
                app_id=app_id,
                config=config,
                process=process,
                document_path=document,
            )
        except Exception as e:
            print(f"[Legacy] Wine launch error: {e}")
            return None

    def _start_monitoring(self, running: RunningApp):
        """Start GUI monitoring for semantic event capture."""
        if not running.config.enable_gui_monitoring:
            return

        # Platform-specific monitoring (stub)
        # Full implementation in gui_monitor.py
        print(f"[Legacy] GUI monitoring started for {running.app_id}")

    def terminate(self, app_id: str) -> bool:
        """Terminate a running application."""
        if app_id not in self.running:
            return False

        running = self.running[app_id]

        try:
            if running.container_id:
                subprocess.run(
                    ["docker", "stop", running.container_id],
                    capture_output=True,
                    timeout=10
                )
                subprocess.run(
                    ["docker", "rm", running.container_id],
                    capture_output=True,
                    timeout=5
                )
            elif running.process:
                running.process.terminate()
                running.process.wait(timeout=5)

            del self.running[app_id]

            self._emit_event({
                "type": "app_terminated",
                "app_id": app_id,
            })

            return True

        except Exception as e:
            print(f"[Legacy] Error terminating {app_id}: {e}")
            return False

    def terminate_all(self):
        """Terminate all running legacy apps."""
        for app_id in list(self.running.keys()):
            self.terminate(app_id)

    def list_running(self) -> List[RunningApp]:
        """List all running legacy apps."""
        return list(self.running.values())

    def _emit_event(self, event: dict):
        """Emit a semantic event."""
        if self.kernel and hasattr(self.kernel, 'emit_event'):
            self.kernel.emit_event(event)

        if self.event_callback:
            self.event_callback(event)


# Convenience function
def get_legacy_manager(kernel=None) -> LegacyAppManager:
    """Get or create the global legacy app manager."""
    global _manager
    if '_manager' not in globals() or _manager is None:
        _manager = LegacyAppManager(kernel)
    return _manager
