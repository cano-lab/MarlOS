"""
Semantic Shell
==============

Interactive shell for Semantic OS.
Query by meaning, not by commands.

Usage:
    python -m kernel.shell

    semantic> show recent documents
    semantic> what did I work on today
    semantic> open design.psd with photoshop
    semantic> find files related to authentication
"""

import os
import sys
import time
import json
from typing import Optional, List, Dict, Any
from dataclasses import dataclass

# readline for input history (optional, not on Windows)
try:
    import readline
except ImportError:
    readline = None

from .core import SemanticKernel, Intent
from .memory import MemoryType, MemoryEntry
from .ai_providers import get_provider, AIProvider, list_available_providers


# ANSI colors
class Colors:
    HEADER = '\033[95m'
    BLUE = '\033[94m'
    CYAN = '\033[96m'
    GREEN = '\033[92m'
    YELLOW = '\033[93m'
    RED = '\033[91m'
    ENDC = '\033[0m'
    BOLD = '\033[1m'
    DIM = '\033[2m'


def colored(text: str, color: str) -> str:
    """Apply color if terminal supports it."""
    if sys.platform == "win32":
        # Enable ANSI on Windows
        os.system("")
    return f"{color}{text}{Colors.ENDC}"


@dataclass
class ShellCommand:
    """A parsed shell command."""
    raw: str
    intent: str = ""
    target: str = ""
    params: Dict[str, Any] = None
    is_system: bool = False  # System command like /help, /quit


class SemanticShell:
    """Interactive semantic shell.

    Interprets natural language commands using AI,
    maps them to kernel operations.
    """

    SYSTEM_PROMPT = """You are the Semantic OS shell assistant.
You help users interact with their documents and system using natural language.

Available actions you can recommend:
- QUERY: Search documents/events by meaning (e.g., "find design documents")
- OPEN: Open a document (e.g., "open report.docx")
- CREATE: Create new document (e.g., "create new note")
- LIST: List documents, processes, events
- STATUS: Show system status
- HISTORY: Show recent activity

When the user asks something, respond with:
1. A brief explanation of what you'll do
2. The action in format: ACTION: <action_type> | <target> | <params>

Example:
User: "show me recent documents"
Response: I'll search for recently modified documents.
ACTION: QUERY | recent documents | type=document,limit=10

User: "what was I working on yesterday"
Response: Let me find your activity from yesterday.
ACTION: QUERY | work activity yesterday | type=event,limit=20

Keep responses concise. Focus on helping the user find and manage their documents."""

    def __init__(
        self,
        kernel: SemanticKernel = None,
        ai_provider: AIProvider = None,
        db_path: str = None,
    ):
        """Initialize the shell.

        Args:
            kernel: Existing kernel (creates new if None)
            ai_provider: AI provider (auto-detects if None)
            db_path: Database path for persistence
        """
        self.kernel = kernel or SemanticKernel(db_path)
        self.ai = ai_provider
        self.running = False
        self.history: List[str] = []

        # Create shell process
        self.process = self.kernel.create_process("semantic_shell")

    def start(self):
        """Start the interactive shell."""
        self.running = True
        self._print_banner()

        # Try to get AI provider
        if not self.ai:
            print(colored("\nDetecting AI providers...", Colors.DIM))
            self.ai = get_provider()
            if self.ai:
                print(colored(f"AI ready: {self.ai.config.provider_type.value}\n", Colors.GREEN))
            else:
                print(colored("No AI provider found. Using basic mode.\n", Colors.YELLOW))
                print("Start LM Studio or Ollama for natural language support.\n")

        # Main loop
        while self.running:
            try:
                user_input = input(colored("semantic> ", Colors.CYAN)).strip()

                if not user_input:
                    continue

                self.history.append(user_input)
                self._process_input(user_input)

            except KeyboardInterrupt:
                print("\n")
                continue
            except EOFError:
                self.running = False
                break

        self._shutdown()

    def _print_banner(self):
        """Print welcome banner."""
        banner = """
+-----------------------------------------------------------+
|                    SEMANTIC OS SHELL                      |
|              Query by meaning, not commands               |
+-----------------------------------------------------------+
|  Try: "show recent documents"                             |
|       "what did I work on today"                          |
|       "find files about authentication"                   |
|                                                           |
|  Commands: /help  /status  /providers  /quit              |
+-----------------------------------------------------------+
"""
        print(colored(banner, Colors.BLUE))

    def _process_input(self, user_input: str):
        """Process user input."""
        # System commands
        if user_input.startswith("/"):
            self._handle_system_command(user_input)
            return

        # Natural language query
        if self.ai:
            self._handle_ai_query(user_input)
        else:
            self._handle_basic_query(user_input)

    def _handle_system_command(self, cmd: str):
        """Handle system commands (/help, /quit, etc.)."""
        parts = cmd.split()
        command = parts[0].lower()
        args = parts[1:] if len(parts) > 1 else []

        if command in ("/quit", "/exit", "/q"):
            self.running = False
            print("Goodbye!")

        elif command in ("/help", "/h", "/?"):
            self._show_help()

        elif command == "/status":
            self._show_status()

        elif command == "/providers":
            self._show_providers()

        elif command == "/docs":
            self._list_documents()

        elif command == "/events":
            self._list_events(int(args[0]) if args else 10)

        elif command == "/processes":
            self._list_processes()

        elif command == "/create":
            if args:
                self._create_document(" ".join(args))
            else:
                print("Usage: /create <filename>")

        elif command == "/open":
            if args:
                self._open_document(" ".join(args))
            else:
                print("Usage: /open <filename>")

        elif command == "/write":
            if len(args) >= 2:
                handle_id = args[0]
                content = " ".join(args[1:])
                self._write_to_handle(handle_id, content)
            else:
                print("Usage: /write <handle_id> <content>")

        elif command == "/query":
            if args:
                self._query(" ".join(args))
            else:
                print("Usage: /query <search terms>")

        elif command == "/clear":
            os.system('cls' if os.name == 'nt' else 'clear')

        elif command == "/history":
            for i, h in enumerate(self.history[-20:], 1):
                print(f"  {i}. {h}")

        else:
            print(f"Unknown command: {command}")
            print("Type /help for available commands")

    def _handle_ai_query(self, query: str):
        """Process query using AI."""
        # Build context from kernel state
        context = self._build_context()

        print(colored("Thinking...", Colors.DIM))

        try:
            response = self.ai.generate(
                prompt=query,
                context=context,
                system_prompt=self.SYSTEM_PROMPT,
                max_tokens=500,
            )

            # Parse response
            content = response.content
            lines = content.strip().split("\n")

            # Print explanation
            for line in lines:
                if line.startswith("ACTION:"):
                    # Parse and execute action
                    self._execute_action(line)
                else:
                    print(colored(line, Colors.GREEN))

            print(colored(f"\n[{response.latency_ms:.0f}ms, {response.tokens_used} tokens]", Colors.DIM))

        except Exception as e:
            print(colored(f"AI Error: {e}", Colors.RED))
            print("Falling back to basic query...")
            self._handle_basic_query(query)

    def _handle_basic_query(self, query: str):
        """Handle query without AI (basic keyword search)."""
        print(colored("Searching...", Colors.DIM))

        results = self.kernel.query(query, limit=10)

        if results:
            print(f"\nFound {len(results)} results:\n")
            for entry, score in results:
                self._print_entry(entry, score)
        else:
            print("No results found.")

    def _execute_action(self, action_line: str):
        """Execute a parsed action from AI."""
        # Format: ACTION: <type> | <target> | <params>
        try:
            parts = action_line.replace("ACTION:", "").strip().split("|")
            action_type = parts[0].strip().upper()
            target = parts[1].strip() if len(parts) > 1 else ""
            params_str = parts[2].strip() if len(parts) > 2 else ""

            # Parse params
            params = {}
            if params_str:
                for p in params_str.split(","):
                    if "=" in p:
                        k, v = p.split("=", 1)
                        params[k.strip()] = v.strip()

            print(colored(f"\n[{action_type}] {target}", Colors.YELLOW))

            if action_type == "QUERY":
                type_filter = params.get("type")
                limit = int(params.get("limit", 10))
                results = self.kernel.query(
                    target,
                    type=MemoryType(type_filter) if type_filter else None,
                    limit=limit,
                )
                if results:
                    print()
                    for entry, score in results:
                        self._print_entry(entry, score)
                else:
                    print("No results found.")

            elif action_type == "OPEN":
                self._open_document(target)

            elif action_type == "CREATE":
                self._create_document(target)

            elif action_type == "LIST":
                if "document" in target.lower():
                    self._list_documents()
                elif "event" in target.lower():
                    self._list_events()
                elif "process" in target.lower():
                    self._list_processes()

            elif action_type == "STATUS":
                self._show_status()

            elif action_type == "HISTORY":
                self._list_events(20)

        except Exception as e:
            print(colored(f"Action error: {e}", Colors.RED))

    def _build_context(self) -> str:
        """Build context string for AI."""
        stats = self.kernel.stats()
        recent_docs = self.kernel.get_documents(limit=5)
        recent_events = self.kernel.get_events(limit=5)

        context = f"""System State:
- Documents: {stats['memory']['by_type'].get('document', 0)}
- Events: {stats['memory']['by_type'].get('event', 0)}
- Open handles: {stats['open_handles']}
- Uptime: {stats['uptime']:.0f}s

Recent Documents:
"""
        for doc in recent_docs:
            path = doc.metadata.get("path", doc.id)
            context += f"- {path}\n"

        context += "\nRecent Events:\n"
        for event in recent_events[:5]:
            event_type = event.metadata.get("event", "unknown")
            context += f"- {event_type}: {event.content[:50]}...\n"

        return context

    def _print_entry(self, entry: MemoryEntry, score: float):
        """Print a memory entry."""
        type_colors = {
            MemoryType.DOCUMENT: Colors.BLUE,
            MemoryType.EVENT: Colors.YELLOW,
            MemoryType.HANDLE: Colors.CYAN,
            MemoryType.PROCESS: Colors.GREEN,
            MemoryType.LINK: Colors.DIM,
        }

        color = type_colors.get(entry.type, Colors.ENDC)
        type_str = entry.type.value.upper()[:4]

        # Get display name
        if entry.type == MemoryType.DOCUMENT:
            name = entry.metadata.get("path", entry.id)
        elif entry.type == MemoryType.EVENT:
            name = entry.metadata.get("event", entry.content[:40])
        else:
            name = entry.content[:40]

        print(f"  {colored(f'[{type_str}]', color)} {colored(f'{score:.2f}', Colors.DIM)} {name}")

    def _show_help(self):
        """Show help message."""
        help_text = """
Natural Language Commands:
  "show recent documents"      - List recently modified documents
  "what did I work on today"   - Show today's activity
  "find files about X"         - Search for documents by content
  "create new document"        - Create a new document
  "open filename"              - Open a document

System Commands:
  /help, /h       - Show this help
  /status         - Show system status
  /providers      - List AI providers
  /docs           - List all documents
  /events [n]     - Show recent events
  /processes      - List processes
  /create <name>  - Create document
  /open <name>    - Open document
  /query <text>   - Direct query
  /history        - Command history
  /clear          - Clear screen
  /quit, /q       - Exit shell
"""
        print(help_text)

    def _show_status(self):
        """Show system status."""
        stats = self.kernel.stats()

        print(colored("\n=== SEMANTIC OS STATUS ===\n", Colors.BOLD))
        print(f"  Uptime:        {stats['uptime']:.1f}s")
        print(f"  Processes:     {stats['processes']}")
        print(f"  Open Handles:  {stats['open_handles']}")
        print(f"\n  Memory Entries:")
        for type_name, count in stats['memory']['by_type'].items():
            print(f"    {type_name}: {count}")
        print(f"    Total: {stats['memory']['total_entries']}")
        print(f"\n  Embeddings:    {'Enabled' if stats['memory']['has_embedder'] else 'Disabled'}")
        print(f"  AI Provider:   {self.ai.config.provider_type.value if self.ai else 'None'}")
        print()

    def _show_providers(self):
        """Show available AI providers."""
        print(colored("\n=== AI PROVIDERS ===\n", Colors.BOLD))

        providers = list_available_providers()
        for p in providers:
            status = colored("Available", Colors.GREEN) if p["available"] else colored("Not Available", Colors.RED)
            local = "(local)" if p["local"] else "(cloud)"
            print(f"  {p['name']:12} {local:8} {status}")
            print(f"    {Colors.DIM}{p['url']}{Colors.ENDC}")
        print()

    def _list_documents(self):
        """List all documents."""
        docs = self.kernel.get_documents(limit=20)

        print(colored("\n=== DOCUMENTS ===\n", Colors.BOLD))
        if docs:
            for doc in docs:
                path = doc.metadata.get("path", doc.id)
                size = len(doc.content)
                print(f"  {colored(path, Colors.BLUE)} ({size} bytes, v{doc.version})")
        else:
            print("  No documents found.")
        print()

    def _list_events(self, limit: int = 10):
        """List recent events."""
        events = self.kernel.get_events(limit=limit)

        print(colored(f"\n=== RECENT EVENTS ({len(events)}) ===\n", Colors.BOLD))
        for event in events:
            event_type = event.metadata.get("event", "unknown")
            timestamp = time.strftime("%H:%M:%S", time.localtime(event.created_at))
            print(f"  {colored(timestamp, Colors.DIM)} {colored(event_type, Colors.YELLOW)}")
            print(f"    {event.content[:60]}...")
        print()

    def _list_processes(self):
        """List processes."""
        print(colored("\n=== PROCESSES ===\n", Colors.BOLD))
        for pid, proc in self.kernel.processes.items():
            handles = len(proc.handles)
            print(f"  {colored(pid, Colors.GREEN)} {proc.name} ({handles} handles)")
        print()

    def _create_document(self, name: str):
        """Create a new document."""
        handle = self.kernel.attach(name, Intent.CREATE, self.process.id)
        print(f"Created: {colored(name, Colors.GREEN)} (handle: {handle.id})")

    def _open_document(self, name: str):
        """Open a document."""
        handle = self.kernel.attach(name, Intent.EDIT, self.process.id)
        content_preview = handle.content_buffer[:100] if handle.content_buffer else "(empty)"
        print(f"Opened: {colored(name, Colors.GREEN)} (handle: {handle.id})")
        print(f"Content: {content_preview}...")

    def _write_to_handle(self, handle_id: str, content: str):
        """Write content to a handle."""
        handle = self.kernel.handles.get(handle_id)
        if handle:
            self.kernel.write(handle, content)
            print(f"Wrote {len(content)} bytes to {handle.path}")
        else:
            print(f"Handle not found: {handle_id}")

    def _query(self, query: str):
        """Direct query to kernel."""
        results = self.kernel.query(query, limit=10)

        print(f"\nResults for '{query}':\n")
        if results:
            for entry, score in results:
                self._print_entry(entry, score)
        else:
            print("  No results found.")
        print()

    def _shutdown(self):
        """Shutdown the shell."""
        print("\nShutting down...")
        self.kernel.terminate_process(self.process.id)
        self.kernel.shutdown()


def main():
    """Main entry point."""
    import argparse

    parser = argparse.ArgumentParser(description="Semantic OS Shell")
    parser.add_argument("--db", help="Database path for persistence")
    parser.add_argument("--provider", help="AI provider (lm_studio, ollama, openai, anthropic)")
    parser.add_argument("--model", help="Model name")
    parser.add_argument("--url", help="Provider URL (for local providers)")

    args = parser.parse_args()

    # Create AI provider if specified
    ai = None
    if args.provider:
        from .ai_providers import ProviderType, _create_provider
        provider_type = ProviderType(args.provider)
        kwargs = {}
        if args.model:
            kwargs["model"] = args.model
        if args.url:
            kwargs["base_url"] = args.url
        ai = _create_provider(provider_type, **kwargs)

    # Start shell
    shell = SemanticShell(db_path=args.db, ai_provider=ai)
    shell.start()


if __name__ == "__main__":
    main()
