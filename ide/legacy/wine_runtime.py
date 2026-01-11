"""
Wine/Proton Runtime
===================

Run Windows applications on Linux with near-native GPU performance.
No VM required - Wine translates Windows API calls to Linux.

Supports:
- Wine (standard Windows compatibility)
- Proton (Valve's gaming-focused Wine fork)
- Wine-staging (experimental features)

GPU Performance: 90-100% native (no virtualization overhead)
"""

import subprocess
import os
import shutil
from pathlib import Path
from dataclasses import dataclass, field
from typing import Optional, Dict, List, Any
from enum import Enum


class WineVariant(str, Enum):
    """Wine distribution to use."""
    WINE = "wine"                    # Standard Wine
    PROTON = "proton"                # Valve's Proton (gaming focus)
    WINE_STAGING = "wine-staging"    # Experimental features
    PROTON_GE = "proton-ge"          # Community Proton with extras


@dataclass
class WinePrefix:
    """A Wine prefix (virtual Windows environment)."""
    path: str
    arch: str = "win64"  # win32 or win64
    windows_version: str = "win10"
    created: bool = False


@dataclass
class WineAppConfig:
    """Configuration for a Windows app running under Wine."""
    name: str
    executable: str                          # Path to .exe (relative to prefix or absolute)
    prefix_path: Optional[str] = None        # Wine prefix (None = create new)
    variant: WineVariant = WineVariant.WINE
    arch: str = "win64"

    # Windows version to emulate
    windows_version: str = "win10"

    # File associations
    file_associations: List[str] = field(default_factory=list)

    # Environment
    env_vars: Dict[str, str] = field(default_factory=dict)

    # DXVK/VKD3D for DirectX (better GPU performance)
    use_dxvk: bool = True       # DirectX 9/10/11 → Vulkan
    use_vkd3d: bool = True      # DirectX 12 → Vulkan

    # Sandbox options
    sandbox_network: bool = False    # Disable network access
    sandbox_filesystem: bool = True  # Limit filesystem access

    # Performance
    esync: bool = True          # Eventfd-based synchronization
    fsync: bool = True          # Futex-based sync (faster, needs kernel support)


class WineRuntime:
    """Runtime for executing Windows apps via Wine.

    Usage:
        runtime = WineRuntime()

        # Check Wine availability
        if runtime.is_available():
            # Run an app
            process = runtime.run(
                WineAppConfig(
                    name="photoshop",
                    executable="C:/Program Files/Adobe/Photoshop.exe"
                ),
                document="image.psd"
            )
    """

    # Default prefix location
    DEFAULT_PREFIX_BASE = os.path.expanduser("~/.semantic_os/wine_prefixes")

    # Known app configurations (can be expanded)
    KNOWN_APPS: Dict[str, WineAppConfig] = {
        "photoshop": WineAppConfig(
            name="photoshop",
            executable="C:/Program Files/Adobe/Adobe Photoshop 2024/Photoshop.exe",
            file_associations=[".psd", ".psb", ".png", ".jpg", ".tiff"],
            use_dxvk=True,
        ),
        "illustrator": WineAppConfig(
            name="illustrator",
            executable="C:/Program Files/Adobe/Adobe Illustrator 2024/Illustrator.exe",
            file_associations=[".ai", ".eps", ".svg"],
        ),
        "word": WineAppConfig(
            name="word",
            executable="C:/Program Files/Microsoft Office/root/Office16/WINWORD.EXE",
            file_associations=[".doc", ".docx", ".rtf"],
            use_dxvk=False,  # Not GPU heavy
        ),
        "excel": WineAppConfig(
            name="excel",
            executable="C:/Program Files/Microsoft Office/root/Office16/EXCEL.EXE",
            file_associations=[".xls", ".xlsx", ".csv"],
            use_dxvk=False,
        ),
        "notepadpp": WineAppConfig(
            name="notepadpp",
            executable="C:/Program Files/Notepad++/notepad++.exe",
            file_associations=[".txt", ".ini", ".conf"],
            use_dxvk=False,
        ),
    }

    def __init__(self, prefix_base: str = None):
        self.prefix_base = prefix_base or self.DEFAULT_PREFIX_BASE
        self._wine_path = None
        self._proton_path = None
        self._available = None

    def is_available(self) -> bool:
        """Check if Wine is installed."""
        if self._available is None:
            self._wine_path = shutil.which("wine")
            self._available = self._wine_path is not None
        return self._available

    def get_wine_version(self) -> Optional[str]:
        """Get Wine version string."""
        if not self.is_available():
            return None
        try:
            result = subprocess.run(
                ["wine", "--version"],
                capture_output=True,
                text=True,
                timeout=5
            )
            return result.stdout.strip()
        except Exception:
            return None

    def is_proton_available(self) -> bool:
        """Check if Proton is available (Steam)."""
        # Proton is typically in Steam's directory
        steam_proton = Path.home() / ".steam/steam/steamapps/common"
        if steam_proton.exists():
            proton_dirs = list(steam_proton.glob("Proton*"))
            if proton_dirs:
                self._proton_path = proton_dirs[-1]  # Latest version
                return True
        return False

    def create_prefix(self, name: str, arch: str = "win64") -> WinePrefix:
        """Create a new Wine prefix."""
        prefix_path = os.path.join(self.prefix_base, name)
        os.makedirs(prefix_path, exist_ok=True)

        env = os.environ.copy()
        env["WINEPREFIX"] = prefix_path
        env["WINEARCH"] = arch

        # Initialize prefix
        try:
            subprocess.run(
                ["wineboot", "--init"],
                env=env,
                capture_output=True,
                timeout=120  # Can take a while
            )
        except subprocess.TimeoutExpired:
            pass  # Sometimes hangs but prefix is created

        return WinePrefix(
            path=prefix_path,
            arch=arch,
            created=True
        )

    def get_or_create_prefix(self, config: WineAppConfig) -> WinePrefix:
        """Get existing prefix or create new one for app."""
        if config.prefix_path:
            return WinePrefix(path=config.prefix_path, arch=config.arch)

        # Create app-specific prefix
        prefix_name = f"{config.name}_{config.arch}"
        prefix_path = os.path.join(self.prefix_base, prefix_name)

        if os.path.exists(prefix_path):
            return WinePrefix(path=prefix_path, arch=config.arch, created=True)

        return self.create_prefix(prefix_name, config.arch)

    def setup_dxvk(self, prefix: WinePrefix) -> bool:
        """Install DXVK in a prefix for better DirectX performance."""
        # Check if DXVK setup script exists
        dxvk_setup = shutil.which("setup_dxvk")
        if not dxvk_setup:
            print("[Wine] DXVK not found. Install with: apt install dxvk")
            return False

        try:
            env = os.environ.copy()
            env["WINEPREFIX"] = prefix.path
            subprocess.run(
                [dxvk_setup, "install"],
                env=env,
                capture_output=True,
                timeout=60
            )
            return True
        except Exception as e:
            print(f"[Wine] DXVK setup failed: {e}")
            return False

    def run(
        self,
        config: WineAppConfig,
        document: str = None,
        working_dir: str = None,
        callback: callable = None
    ) -> subprocess.Popen:
        """Run a Windows application via Wine.

        Args:
            config: App configuration
            document: Optional document to open
            working_dir: Working directory
            callback: Called with process events

        Returns:
            Popen process object
        """
        if not self.is_available():
            raise RuntimeError("Wine is not installed")

        # Get or create prefix
        prefix = self.get_or_create_prefix(config)

        # Setup DXVK if requested
        if config.use_dxvk and prefix.created:
            self.setup_dxvk(prefix)

        # Build environment
        env = self._build_environment(config, prefix)

        # Build command
        cmd = self._build_command(config, document)

        # Working directory
        cwd = working_dir
        if document and not cwd:
            cwd = os.path.dirname(os.path.abspath(document))

        print(f"[Wine] Launching: {config.name}")
        print(f"[Wine] Command: {' '.join(cmd)}")
        print(f"[Wine] Prefix: {prefix.path}")

        # Launch
        process = subprocess.Popen(
            cmd,
            env=env,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        return process

    def _build_environment(self, config: WineAppConfig, prefix: WinePrefix) -> dict:
        """Build environment variables for Wine."""
        env = os.environ.copy()

        # Wine prefix
        env["WINEPREFIX"] = prefix.path
        env["WINEARCH"] = config.arch

        # Windows version
        # Set via winecfg or registry, not env var

        # Performance options
        if config.esync:
            env["WINEESYNC"] = "1"
        if config.fsync:
            env["WINEFSYNC"] = "1"

        # DXVK options
        if config.use_dxvk:
            env["DXVK_HUD"] = "0"  # Disable HUD by default
            env["DXVK_LOG_LEVEL"] = "none"

        # Sandbox options
        if config.sandbox_network:
            # This requires additional setup (firejail/bubblewrap)
            pass

        # User environment vars
        env.update(config.env_vars)

        return env

    def _build_command(self, config: WineAppConfig, document: str = None) -> List[str]:
        """Build Wine command."""
        cmd = ["wine"]

        # Add executable
        cmd.append(config.executable)

        # Add document if provided
        if document:
            # Convert Linux path to Windows path for Wine
            wine_path = self._to_wine_path(document)
            cmd.append(wine_path)

        return cmd

    def _to_wine_path(self, linux_path: str) -> str:
        """Convert Linux path to Wine/Windows path."""
        abs_path = os.path.abspath(linux_path)
        # Wine maps Z: to /
        return "Z:" + abs_path.replace("/", "\\")

    def _from_wine_path(self, wine_path: str) -> str:
        """Convert Wine/Windows path to Linux path."""
        if wine_path.startswith("Z:"):
            return wine_path[2:].replace("\\", "/")
        # Handle C: drive (in prefix)
        # Would need to resolve through prefix/drive_c
        return wine_path

    def install_app(
        self,
        installer_path: str,
        config: WineAppConfig,
        silent: bool = False
    ) -> bool:
        """Run a Windows installer.

        Args:
            installer_path: Path to .exe/.msi installer
            config: App configuration (for prefix)
            silent: Try silent installation

        Returns:
            True if installation completed
        """
        prefix = self.get_or_create_prefix(config)
        env = self._build_environment(config, prefix)

        cmd = ["wine", installer_path]

        # Add silent flags for common installers
        if silent:
            ext = os.path.splitext(installer_path)[1].lower()
            if ext == ".msi":
                cmd = ["wine", "msiexec", "/i", installer_path, "/quiet"]
            elif ext == ".exe":
                # Common silent flags (not universal)
                cmd.extend(["/S", "/silent", "/quiet"])

        print(f"[Wine] Running installer: {installer_path}")

        try:
            result = subprocess.run(
                cmd,
                env=env,
                capture_output=True,
                timeout=600  # 10 min timeout for installs
            )
            return result.returncode == 0
        except subprocess.TimeoutExpired:
            print("[Wine] Installation timed out")
            return False
        except Exception as e:
            print(f"[Wine] Installation error: {e}")
            return False

    def list_installed(self, prefix_path: str = None) -> List[str]:
        """List installed programs in a prefix."""
        if not prefix_path:
            prefix_path = os.path.join(self.prefix_base, "default_win64")

        programs_dir = os.path.join(prefix_path, "drive_c", "Program Files")
        if not os.path.exists(programs_dir):
            return []

        return [d for d in os.listdir(programs_dir)
                if os.path.isdir(os.path.join(programs_dir, d))]

    def kill(self, prefix_path: str = None):
        """Kill all Wine processes in a prefix."""
        env = os.environ.copy()
        if prefix_path:
            env["WINEPREFIX"] = prefix_path

        subprocess.run(["wineserver", "-k"], env=env, capture_output=True)


class ProtonRuntime(WineRuntime):
    """Proton-specific runtime (Steam's Wine fork).

    Better for games, includes more patches and DXVK by default.
    """

    def __init__(self, proton_path: str = None):
        super().__init__()
        self._proton_path = proton_path

    def is_available(self) -> bool:
        """Check if Proton is available."""
        if self._proton_path and os.path.exists(self._proton_path):
            return True
        return self.is_proton_available()

    def _build_command(self, config: WineAppConfig, document: str = None) -> List[str]:
        """Build Proton command."""
        proton_exe = os.path.join(self._proton_path, "proton")

        cmd = [proton_exe, "run", config.executable]

        if document:
            wine_path = self._to_wine_path(document)
            cmd.append(wine_path)

        return cmd


# Convenience functions
_wine_runtime: Optional[WineRuntime] = None


def get_wine_runtime() -> WineRuntime:
    """Get global Wine runtime instance."""
    global _wine_runtime
    if _wine_runtime is None:
        _wine_runtime = WineRuntime()
    return _wine_runtime


def run_windows_app(
    app_name: str,
    document: str = None,
    config_override: Dict[str, Any] = None
) -> Optional[subprocess.Popen]:
    """Convenience function to run a known Windows app.

    Args:
        app_name: Name of app (photoshop, word, etc.)
        document: Optional document to open
        config_override: Override config values

    Returns:
        Process object or None
    """
    runtime = get_wine_runtime()

    if not runtime.is_available():
        print("[Wine] Wine is not installed")
        print("[Wine] Install with: apt install wine64 wine32")
        return None

    if app_name not in WineRuntime.KNOWN_APPS:
        print(f"[Wine] Unknown app: {app_name}")
        print(f"[Wine] Known apps: {list(WineRuntime.KNOWN_APPS.keys())}")
        return None

    config = WineRuntime.KNOWN_APPS[app_name]

    if config_override:
        # Apply overrides
        for key, value in config_override.items():
            if hasattr(config, key):
                setattr(config, key, value)

    return runtime.run(config, document)
