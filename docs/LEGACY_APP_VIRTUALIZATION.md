# Legacy App Virtualization Architecture

## Overview

The Semantic OS needs to run legacy applications (Photoshop, Excel, VS Code, etc.)
while integrating them into the document-centric paradigm. This document outlines
the architecture for virtualizing legacy apps and translating their operations
into semantic events.

## Core Concept: AI Synthesis Layer

Legacy apps don't speak "semantic" - they use traditional GUI paradigms (windows,
menus, toolbars). The AI Synthesis Layer acts as a translator:

```
┌─────────────────────────────────────────────────────────────────┐
│                      SEMANTIC OS KERNEL                         │
│                    (Document-centric layer)                     │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │ Semantic Events
                              │ (ctx.emit, ctx.invoke)
┌─────────────────────────────────────────────────────────────────┐
│                    AI SYNTHESIS LAYER                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │ GUI Monitor  │  │ AI Translator│  │ Event Mapper │         │
│  │ (hooks UI)   │→ │ (understands │→ │ (to semantic │         │
│  │              │  │  intent)     │  │  events)     │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │ GUI Calls (Win32, Cocoa, GTK)
┌─────────────────────────────────────────────────────────────────┐
│                    VIRTUALIZATION LAYER                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │   Docker     │  │  Micro-VM    │  │    WASM      │         │
│  │ (Linux apps) │  │ (Full OS)    │  │ (Web apps)   │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │
┌─────────────────────────────────────────────────────────────────┐
│                      LEGACY APPLICATION                         │
│              (Photoshop, Excel, VS Code, etc.)                 │
└─────────────────────────────────────────────────────────────────┘
```

## Virtualization Strategies

### 1. Docker Containers (Linux Apps)

Best for: Command-line tools, Linux GUI apps via X11 forwarding

```python
class DockerAppContainer:
    """Run Linux apps in Docker with display forwarding."""

    def __init__(self, image: str, app_name: str):
        self.image = image
        self.app_name = app_name

    def launch(self, document_path: str = None):
        """Launch app with optional document."""
        cmd = [
            "docker", "run",
            "-d",  # Detached
            "--rm",
            "-e", f"DISPLAY={os.environ.get('DISPLAY', ':0')}",
            "-v", "/tmp/.X11-unix:/tmp/.X11-unix",  # X11 socket
            "-v", f"{document_path}:/data" if document_path else "",
            self.image,
            self.app_name
        ]
        return subprocess.run(cmd, capture_output=True)
```

### 2. Micro-VMs (Full Isolation)

Best for: Windows apps, untrusted software, full OS simulation

Options:
- **Firecracker**: Fast boot (<125ms), minimal footprint
- **QEMU microvm**: More compatible, slower boot
- **gVisor**: Lightweight sandboxing

```python
class MicroVMManager:
    """Manage micro-VMs for legacy app isolation."""

    SUPPORTED_BACKENDS = ["firecracker", "qemu-microvm", "gvisor"]

    def __init__(self, backend: str = "firecracker"):
        self.backend = backend
        self.vms = {}

    def create_vm(self, vm_id: str, config: VMConfig) -> MicroVM:
        """Create a new micro-VM instance."""
        if self.backend == "firecracker":
            return self._create_firecracker(vm_id, config)
        elif self.backend == "qemu-microvm":
            return self._create_qemu(vm_id, config)
        # ...

    def attach_document(self, vm_id: str, document_path: str):
        """Share a document with the VM via virtio-fs."""
        vm = self.vms.get(vm_id)
        if vm:
            vm.mount_share(document_path, "/shared")
```

### 3. WebAssembly (WASM)

Best for: Cross-platform apps, web technologies, sandboxed execution

```python
class WasmRuntime:
    """Run WASM apps with semantic integration."""

    def __init__(self):
        self.runtime = None  # wasmtime, wasmer, etc.

    def load_app(self, wasm_path: str) -> WasmApp:
        """Load a WASM application."""
        # WASM apps get a virtual filesystem
        # All file operations are intercepted
        pass

    def intercept_syscall(self, syscall: str, args: dict):
        """Intercept WASM syscalls and translate to semantic events."""
        if syscall == "open":
            return self._handle_file_open(args["path"])
        elif syscall == "write":
            return self._handle_file_write(args["fd"], args["data"])
```

## AI Synthesis Layer

The AI layer is the key innovation - it "reads" what the legacy app is doing
and translates that into semantic meaning.

### GUI Monitoring

```python
class GUIMonitor:
    """Monitor legacy app GUI for semantic events."""

    def __init__(self, app_pid: int):
        self.app_pid = app_pid
        self.window_handle = None
        self.ai_interpreter = AIInterpreter()

    def start_monitoring(self):
        """Begin monitoring the app's GUI."""
        # Platform-specific hooks
        if sys.platform == "win32":
            self._hook_win32()
        elif sys.platform == "darwin":
            self._hook_accessibility()  # macOS Accessibility API
        else:
            self._hook_at_spi()  # Linux AT-SPI

    def _hook_win32(self):
        """Windows UI Automation / MSAA hooks."""
        import comtypes.client
        from comtypes.gen import UIAutomationClient

        uia = comtypes.client.CreateObject(
            "{ff48dba4-60ef-4201-aa87-54103eef594e}",
            interface=UIAutomationClient.IUIAutomation
        )
        # Subscribe to UI events
        # ...

    def on_button_click(self, button_name: str, context: dict):
        """Handle button click event."""
        # AI interprets the intent
        intent = self.ai_interpreter.interpret_action(
            action="button_click",
            target=button_name,
            context=context
        )

        # Emit semantic event
        return SemanticEvent(
            type="user_action",
            intent=intent,  # e.g., "save_document", "apply_filter"
            source_app=self.app_name,
            raw_action={"type": "button_click", "target": button_name}
        )
```

### AI Intent Interpretation

```python
class AIInterpreter:
    """Interpret legacy app actions as semantic intents."""

    # Map common UI patterns to semantic intents
    INTENT_PATTERNS = {
        # File operations
        r"(save|save as|export)": "document.save",
        r"(open|load|import)": "document.open",
        r"(new|create)": "document.create",

        # Edit operations
        r"(undo|ctrl\+z)": "edit.undo",
        r"(redo|ctrl\+y)": "edit.redo",
        r"(copy|ctrl\+c)": "edit.copy",
        r"(paste|ctrl\+v)": "edit.paste",
        r"(cut|ctrl\+x)": "edit.cut",

        # View operations
        r"(zoom in|magnify)": "view.zoom_in",
        r"(zoom out|reduce)": "view.zoom_out",
    }

    def __init__(self, llm_client=None):
        self.llm = llm_client
        self.app_context = {}

    def interpret_action(self, action: str, target: str, context: dict) -> str:
        """Interpret a UI action as semantic intent."""
        # First try pattern matching (fast)
        for pattern, intent in self.INTENT_PATTERNS.items():
            if re.search(pattern, target.lower()):
                return intent

        # Fall back to LLM for complex cases
        if self.llm:
            return self._llm_interpret(action, target, context)

        return f"unknown.{action}"

    def _llm_interpret(self, action: str, target: str, context: dict) -> str:
        """Use LLM to interpret complex UI actions."""
        prompt = f"""
        App: {context.get('app_name', 'unknown')}
        Action: {action}
        Target: {target}
        Window Title: {context.get('window_title', '')}
        Menu Path: {context.get('menu_path', '')}

        What is the user's semantic intent? Reply with a single
        intent string like "document.save" or "image.apply_filter.blur".
        """

        response = self.llm.generate("intent_classification", prompt)
        return response.strip()
```

### Semantic Event Mapping

```python
class SemanticEventMapper:
    """Map interpreted intents to Semantic OS events."""

    def __init__(self, kernel):
        self.kernel = kernel

    def map_intent_to_event(self, intent: str, context: dict) -> dict:
        """Convert an intent into a kernel event."""
        parts = intent.split(".")
        category = parts[0] if parts else "unknown"

        if category == "document":
            return self._map_document_intent(intent, context)
        elif category == "edit":
            return self._map_edit_intent(intent, context)
        elif category == "view":
            return self._map_view_intent(intent, context)
        else:
            return self._map_generic_intent(intent, context)

    def _map_document_intent(self, intent: str, context: dict) -> dict:
        """Map document-related intents."""
        if intent == "document.save":
            # The legacy app saved - we should update our manifest
            return {
                "event": "document_saved",
                "source": context.get("app_name"),
                "path": context.get("file_path"),
                "notify_related": True  # Trigger relation graph updates
            }
        # ...
```

## VR/3D Mode: Semantic Unrolling

For VR Semantic OS, legacy 2D apps can be "unrolled" into 3D spatial interfaces.

```python
class SemanticUnroller:
    """Convert 2D app UI into 3D spatial components for VR."""

    def __init__(self, ai_client):
        self.ai = ai_client

    def unroll_window(self, window_tree: UITree) -> SpatialLayout:
        """Convert a window's UI tree into spatial 3D layout."""

        # Analyze UI structure
        components = self._extract_components(window_tree)

        # AI determines optimal 3D arrangement
        layout = self.ai.generate("spatial_layout", {
            "components": components,
            "app_type": window_tree.app_type,
            "user_prefs": self._get_user_prefs()
        })

        return SpatialLayout(
            # Toolbar becomes floating panel to the left
            toolbar=SpatialPanel(
                position=Vector3(-1.5, 1.0, -2.0),
                components=layout["toolbar_items"]
            ),
            # Main canvas becomes large surface in front
            canvas=SpatialSurface(
                position=Vector3(0, 1.2, -2.5),
                size=Vector2(2.0, 1.5),
                content=layout["main_content"]
            ),
            # Properties panel floats to the right
            properties=SpatialPanel(
                position=Vector3(1.5, 1.0, -2.0),
                components=layout["properties"]
            ),
            # Timeline at bottom in curved arc
            timeline=SpatialArc(
                center=Vector3(0, 0.5, -2.0),
                radius=1.0,
                content=layout["timeline"]
            )
        )
```

## Implementation Phases

### Phase 1: Docker + Basic Monitoring
- Docker container management
- X11/Wayland forwarding
- Basic accessibility hooks
- Pattern-based intent matching

### Phase 2: Micro-VM Support
- Firecracker integration
- virtio-fs document sharing
- GPU passthrough for graphics apps

### Phase 3: AI Synthesis Layer
- Full GUI monitoring
- LLM-based intent interpretation
- Semantic event emission
- Relation graph integration

### Phase 4: VR Unrolling
- Spatial layout generation
- Hand tracking integration
- Voice command overlay
- Collaborative multi-user

## Security Considerations

1. **Isolation**: Legacy apps run in containers/VMs, cannot access host directly
2. **Permission Model**: Apps must request document access through manifest
3. **AI Monitoring**: All interpreted actions are logged for audit
4. **Network**: Legacy apps can be network-isolated by default
5. **Clipboard**: Clipboard sharing is opt-in and monitored

## File Structure

```
ide/
├── legacy/
│   ├── __init__.py
│   ├── docker_runtime.py      # Docker container management
│   ├── microvm_runtime.py     # Firecracker/QEMU micro-VMs
│   ├── wasm_runtime.py        # WebAssembly sandbox
│   ├── gui_monitor.py         # Platform-specific GUI hooks
│   ├── ai_interpreter.py      # Intent interpretation
│   ├── event_mapper.py        # Semantic event mapping
│   └── vr_unroller.py         # 3D spatial conversion
```

## Example: Running Photoshop

```python
# User opens a PSD file in Semantic OS
kernel.open_document("design.psd")

# Kernel checks manifest - Photoshop is the preferred handler
manifest = kernel.get_manifest("design.psd")
if manifest.preferred_app == "photoshop":

    # Launch Photoshop in micro-VM
    vm = microvm_manager.create_vm("photoshop-session", VMConfig(
        image="windows-11-photoshop",
        memory="8G",
        gpu_passthrough=True
    ))

    # Share the document
    vm.mount_document("design.psd", "/shared/design.psd")

    # Start GUI monitoring
    monitor = GUIMonitor(vm.get_app_pid("Photoshop.exe"))
    monitor.on_event = lambda e: kernel.emit_event(
        event_mapper.map_intent_to_event(e.intent, e.context)
    )

    # Photoshop opens, user works...
    # Every action is interpreted and emitted as semantic events
    # Related documents are notified of changes
    # History is tracked with semantic compression
```

## Dependencies

- **Docker**: Container runtime
- **Firecracker/QEMU**: Micro-VM backends
- **pyatspi2**: Linux accessibility (AT-SPI)
- **pywinauto**: Windows UI Automation
- **pyobjc-framework-ApplicationServices**: macOS accessibility

## References

- [Firecracker MicroVM](https://firecracker-microvm.github.io/)
- [Windows UI Automation](https://docs.microsoft.com/en-us/windows/win32/winauto/entry-uiauto-win32)
- [AT-SPI (Linux Accessibility)](https://www.freedesktop.org/wiki/Accessibility/AT-SPI2/)
- [WebAssembly System Interface (WASI)](https://wasi.dev/)
