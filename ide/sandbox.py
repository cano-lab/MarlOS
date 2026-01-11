"""
Sandbox Execution Module
========================

Provides isolated execution environments for code running.
Uses Docker containers to sandbox untrusted code execution.

Security features:
- Network isolation (optional)
- Filesystem isolation (only mount working directory)
- Resource limits (CPU, memory, time)
- No privileged access
- Capability restrictions

Providers communicate through the kernel, not directly.
The sandbox enforces manifest permissions at the execution layer.
"""

import subprocess
import tempfile
import os
import shutil
from pathlib import Path
from dataclasses import dataclass, field
from typing import Dict, Optional, List
from enum import Enum


class SandboxMode(str, Enum):
    """Execution mode for code running."""
    DIRECT = "direct"       # Run directly on host (development only)
    DOCKER = "docker"       # Run in Docker container
    # Future: WASM = "wasm"  # WebAssembly sandbox


@dataclass
class SandboxConfig:
    """Configuration for sandboxed execution."""
    mode: SandboxMode = SandboxMode.DIRECT

    # Docker settings
    docker_image: str = "python:3.11-slim"  # Default image
    memory_limit: str = "256m"              # Memory limit
    cpu_limit: float = 0.5                  # CPU cores
    timeout: int = 30                       # Execution timeout in seconds
    network_enabled: bool = False           # Network access

    # Filesystem
    mount_workdir: bool = True              # Mount working directory
    readonly_workdir: bool = True           # Mount as read-only

    # Custom images per language
    images: Dict[str, str] = field(default_factory=lambda: {
        "python": "python:3.11-slim",
        "node": "node:18-slim",
        "javascript": "node:18-slim",
    })


@dataclass
class ExecutionResult:
    """Result of sandboxed execution."""
    success: bool
    stdout: str = ""
    stderr: str = ""
    exit_code: int = 0
    error: Optional[str] = None
    execution_mode: str = "direct"


class Sandbox:
    """Sandboxed execution environment.

    Usage:
        sandbox = Sandbox(SandboxConfig(mode=SandboxMode.DOCKER))
        result = sandbox.execute("print('hello')", "python", workdir="/path/to/project")
    """

    def __init__(self, config: SandboxConfig = None):
        self.config = config or SandboxConfig()
        self._docker_available = None

    def is_docker_available(self) -> bool:
        """Check if Docker is available on the system."""
        if self._docker_available is None:
            try:
                result = subprocess.run(
                    ["docker", "version"],
                    capture_output=True,
                    timeout=5
                )
                self._docker_available = result.returncode == 0
            except (subprocess.TimeoutExpired, FileNotFoundError):
                self._docker_available = False
        return self._docker_available

    def execute(self, code: str, language: str, workdir: str = None) -> ExecutionResult:
        """Execute code in the configured sandbox mode."""
        if self.config.mode == SandboxMode.DOCKER:
            if not self.is_docker_available():
                return ExecutionResult(
                    success=False,
                    error="Docker is not available. Install Docker or switch to direct mode.",
                    execution_mode="docker"
                )
            return self._execute_docker(code, language, workdir)
        else:
            return self._execute_direct(code, language, workdir)

    def _execute_direct(self, code: str, language: str, workdir: str = None) -> ExecutionResult:
        """Execute code directly on the host (no sandboxing)."""
        lang_config = {
            "python": {"cmd": "python", "ext": ".py"},
            "py": {"cmd": "python", "ext": ".py"},
            "node": {"cmd": "node", "ext": ".js"},
            "javascript": {"cmd": "node", "ext": ".js"},
            "js": {"cmd": "node", "ext": ".js"},
        }

        config = lang_config.get(language.lower())
        if not config:
            return ExecutionResult(
                success=False,
                error=f"Unsupported language: {language}",
                execution_mode="direct"
            )

        # Write to temp file
        with tempfile.NamedTemporaryFile(
            mode='w',
            suffix=config["ext"],
            delete=False,
            encoding='utf-8'
        ) as f:
            f.write(code)
            temp_path = f.name

        try:
            result = subprocess.run(
                [config["cmd"], temp_path],
                capture_output=True,
                text=True,
                timeout=self.config.timeout,
                cwd=workdir
            )

            return ExecutionResult(
                success=result.returncode == 0,
                stdout=result.stdout,
                stderr=result.stderr,
                exit_code=result.returncode,
                execution_mode="direct"
            )
        except subprocess.TimeoutExpired:
            return ExecutionResult(
                success=False,
                error=f"Execution timed out after {self.config.timeout}s",
                execution_mode="direct"
            )
        except FileNotFoundError:
            return ExecutionResult(
                success=False,
                error=f"Command not found: {config['cmd']}",
                execution_mode="direct"
            )
        finally:
            os.unlink(temp_path)

    def _execute_docker(self, code: str, language: str, workdir: str = None) -> ExecutionResult:
        """Execute code in a Docker container."""
        # Get image for language
        image = self.config.images.get(language.lower(), self.config.docker_image)

        # Language-specific command
        lang_config = {
            "python": {"cmd": "python", "ext": ".py", "file": "/code/script.py"},
            "py": {"cmd": "python", "ext": ".py", "file": "/code/script.py"},
            "node": {"cmd": "node", "ext": ".js", "file": "/code/script.js"},
            "javascript": {"cmd": "node", "ext": ".js", "file": "/code/script.js"},
            "js": {"cmd": "node", "ext": ".js", "file": "/code/script.js"},
        }

        config = lang_config.get(language.lower())
        if not config:
            return ExecutionResult(
                success=False,
                error=f"Unsupported language for Docker: {language}",
                execution_mode="docker"
            )

        # Create temp directory for code
        temp_dir = tempfile.mkdtemp(prefix="semantic_sandbox_")
        code_file = os.path.join(temp_dir, f"script{config['ext']}")

        try:
            # Write code to temp file
            with open(code_file, 'w', encoding='utf-8') as f:
                f.write(code)

            # Build docker command
            docker_cmd = [
                "docker", "run",
                "--rm",                                    # Remove container after exit
                "--memory", self.config.memory_limit,     # Memory limit
                f"--cpus={self.config.cpu_limit}",        # CPU limit
                "--security-opt", "no-new-privileges",    # No privilege escalation
                "--cap-drop", "ALL",                      # Drop all capabilities
                "--read-only",                            # Read-only root filesystem
                "--tmpfs", "/tmp:rw,noexec,nosuid,size=64m",  # Writable /tmp
            ]

            # Network
            if not self.config.network_enabled:
                docker_cmd.extend(["--network", "none"])

            # Mount code directory
            docker_cmd.extend([
                "-v", f"{temp_dir}:/code:ro",  # Mount code as read-only
            ])

            # Mount working directory if requested
            if workdir and self.config.mount_workdir:
                mount_mode = "ro" if self.config.readonly_workdir else "rw"
                docker_cmd.extend([
                    "-v", f"{workdir}:/workdir:{mount_mode}",
                    "-w", "/workdir",
                ])

            # Image and command
            docker_cmd.extend([
                image,
                config["cmd"], config["file"]
            ])

            # Execute
            result = subprocess.run(
                docker_cmd,
                capture_output=True,
                text=True,
                timeout=self.config.timeout + 10,  # Extra time for container overhead
            )

            return ExecutionResult(
                success=result.returncode == 0,
                stdout=result.stdout,
                stderr=result.stderr,
                exit_code=result.returncode,
                execution_mode="docker"
            )

        except subprocess.TimeoutExpired:
            # Kill the container if it's still running
            return ExecutionResult(
                success=False,
                error=f"Docker execution timed out after {self.config.timeout}s",
                execution_mode="docker"
            )
        except Exception as e:
            return ExecutionResult(
                success=False,
                error=f"Docker execution error: {str(e)}",
                execution_mode="docker"
            )
        finally:
            # Clean up temp directory
            shutil.rmtree(temp_dir, ignore_errors=True)

    def pull_image(self, language: str) -> bool:
        """Pull Docker image for a language."""
        if not self.is_docker_available():
            return False

        image = self.config.images.get(language.lower(), self.config.docker_image)
        try:
            result = subprocess.run(
                ["docker", "pull", image],
                capture_output=True,
                timeout=300  # 5 minutes for pull
            )
            return result.returncode == 0
        except:
            return False


# Global sandbox instance (configured at startup)
_sandbox: Optional[Sandbox] = None


def get_sandbox() -> Sandbox:
    """Get the global sandbox instance."""
    global _sandbox
    if _sandbox is None:
        _sandbox = Sandbox(SandboxConfig(mode=SandboxMode.DIRECT))
    return _sandbox


def configure_sandbox(config: SandboxConfig):
    """Configure the global sandbox."""
    global _sandbox
    _sandbox = Sandbox(config)


def enable_docker_sandbox(
    memory_limit: str = "256m",
    cpu_limit: float = 0.5,
    timeout: int = 30,
    network: bool = False
):
    """Enable Docker sandboxing with specified limits."""
    config = SandboxConfig(
        mode=SandboxMode.DOCKER,
        memory_limit=memory_limit,
        cpu_limit=cpu_limit,
        timeout=timeout,
        network_enabled=network,
    )
    configure_sandbox(config)
    return get_sandbox()


def disable_sandbox():
    """Disable sandboxing (use direct execution)."""
    configure_sandbox(SandboxConfig(mode=SandboxMode.DIRECT))
