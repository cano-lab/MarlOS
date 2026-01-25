"""AI provider implementations for the Document IDE.

This module provides:
- AIProviderClient: Base interface for AI providers
- LocalAIClient: Heuristic-based local AI (no network)
- LMStudioClient: LM Studio integration (OpenAI-compatible API)
- HybridAIClient: Tries LM Studio, falls back to local heuristics

LM Studio Integration:
    LM Studio provides an OpenAI-compatible API at localhost:1234/v1 by default.
    Configure the endpoint in workspace settings if using a different port.
"""

import re
import difflib
import json


class AIProviderClient:
    """Generic AI provider interface.

    All AI providers implement this interface. The generate() method
    takes a task name and content, returning the AI's response.
    """

    def generate(self, task, content):
        """Generate AI response for a task.

        Args:
            task: Task identifier (e.g., "intent_map", "code_complete")
            content: Input content (string or dict depending on task)

        Returns:
            Task-specific response (string, dict, or list)
        """
        raise NotImplementedError

    def is_available(self):
        """Check if this AI provider is currently available."""
        return True


class LMStudioClient(AIProviderClient):
    """LM Studio API client using OpenAI-compatible endpoint.

    Connects to LM Studio's local server to provide real AI capabilities.
    Default endpoint: http://localhost:1234/v1

    Supported tasks:
    - All existing tasks (intent_map, citation_helper, etc.)
    - code_complete: Complete code at cursor position
    - code_explain: Explain selected code
    - code_edit: Edit code based on instruction
    - code_generate: Generate code from description
    """

    # System prompts for different task types
    SYSTEM_PROMPTS = {
        "intent_map": "You are a document analyst. Summarize the intent, themes, and structure of the given text. Be concise.",
        "citation_helper": "You are an academic writing assistant. Identify claims that lack citations or evidence. List them with line numbers.",
        "diff_narrator": "You are a document reviewer. Describe the changes between the before and after versions clearly and concisely.",
        "outline_enhancer": "You are a writing coach. Suggest improvements to the document's structure and organization.",
        "action_extractor": "You are a task analyst. Extract all actionable items from the text as a checklist.",
        "typo_fixer": "You are a proofreader. Fix spelling and grammar errors. Return JSON with 'text' (corrected) and 'count' (number of fixes).",
        "suggestions": "You are a writing assistant. Suggest completions or continuations for the given text. Return a JSON array of 3-5 suggestions.",
        "code_complete": "You are a code completion assistant. Complete the code at the cursor position marked with <CURSOR>. Return only the completion, no explanation.",
        "code_explain": "You are a code explainer. Explain what the selected code does in clear, concise terms.",
        "code_edit": "You are a code editor. Modify the code according to the instruction. Return only the modified code.",
        "code_generate": "You are a code generator. Write code based on the description. Include appropriate comments.",
    }

    def __init__(self, endpoint="http://localhost:1234/v1", model=None, timeout=30):
        """Initialize LM Studio client.

        Args:
            endpoint: LM Studio API endpoint (default: localhost:1234/v1)
            model: Model name (optional, uses LM Studio's loaded model)
            timeout: Request timeout in seconds
        """
        self.endpoint = endpoint.rstrip("/")
        self.model = model
        self.timeout = timeout
        self._available = None

    def is_available(self):
        """Check if LM Studio is running and accessible."""
        if self._available is not None:
            return self._available

        try:
            import urllib.request
            req = urllib.request.Request(
                f"{self.endpoint}/models",
                headers={"Content-Type": "application/json"},
                method="GET"
            )
            with urllib.request.urlopen(req, timeout=2) as response:
                self._available = response.status == 200
        except Exception:
            self._available = False

        return self._available

    def generate(self, task, content):
        """Generate response using LM Studio API."""
        if not self.is_available():
            raise ConnectionError("LM Studio is not available")

        system_prompt = self.SYSTEM_PROMPTS.get(task, "You are a helpful assistant.")
        user_content = self._format_content(task, content)

        messages = [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_content}
        ]

        response = self._call_api(messages)
        return self._parse_response(task, response)

    def _format_content(self, task, content):
        """Format content for the API based on task type."""
        if task == "diff_narrator" and isinstance(content, tuple):
            before, after = content
            return f"BEFORE:\n{before}\n\nAFTER:\n{after}"

        if task == "suggestions" and isinstance(content, dict):
            selection = content.get("selection", "")
            line = content.get("line", "")
            return f"Current line: {line}\nSelection: {selection}" if selection else f"Current line: {line}"

        if task == "code_edit" and isinstance(content, dict):
            code = content.get("code", "")
            instruction = content.get("instruction", "")
            return f"INSTRUCTION: {instruction}\n\nCODE:\n{code}"

        if task == "code_complete" and isinstance(content, dict):
            code = content.get("code", "")
            cursor_pos = content.get("cursor_pos", len(code))
            # Insert cursor marker
            code_with_cursor = code[:cursor_pos] + "<CURSOR>" + code[cursor_pos:]
            return code_with_cursor

        return str(content)

    def _call_api(self, messages):
        """Make API call to LM Studio."""
        import urllib.request

        payload = {
            "messages": messages,
            "temperature": 0.7,
            "max_tokens": 2048,
            "stream": False
        }
        if self.model:
            payload["model"] = self.model

        data = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            f"{self.endpoint}/chat/completions",
            data=data,
            headers={"Content-Type": "application/json"},
            method="POST"
        )

        with urllib.request.urlopen(req, timeout=self.timeout) as response:
            result = json.loads(response.read().decode("utf-8"))
            return result["choices"][0]["message"]["content"]

    def _parse_response(self, task, response):
        """Parse API response based on task type."""
        if task == "typo_fixer":
            try:
                # Try to parse as JSON
                return json.loads(response)
            except json.JSONDecodeError:
                # Fallback: assume the response is the corrected text
                return {"text": response, "count": 0}

        if task == "suggestions":
            try:
                suggestions = json.loads(response)
                if isinstance(suggestions, list):
                    return suggestions
            except json.JSONDecodeError:
                pass
            # Fallback: split by newlines
            return [line.strip() for line in response.split("\n") if line.strip()][:5]

        return response


class HybridAIClient(AIProviderClient):
    """Hybrid AI client that uses LM Studio when available, falls back to local.

    This client provides the best of both worlds:
    - Real AI capabilities when LM Studio is running
    - Instant heuristic responses when offline

    The client checks LM Studio availability on first use and caches the result.
    Call refresh_availability() to recheck.
    """

    def __init__(self, lm_studio_endpoint="http://localhost:1234/v1", model=None):
        """Initialize hybrid client.

        Args:
            lm_studio_endpoint: LM Studio API endpoint
            model: Optional model name for LM Studio
        """
        self.lm_studio = LMStudioClient(endpoint=lm_studio_endpoint, model=model)
        self.local = LocalAIClient()
        self._use_lm_studio = None

    def refresh_availability(self):
        """Recheck LM Studio availability."""
        self.lm_studio._available = None
        self._use_lm_studio = None

    def is_lm_studio_active(self):
        """Check if currently using LM Studio."""
        if self._use_lm_studio is None:
            self._use_lm_studio = self.lm_studio.is_available()
        return self._use_lm_studio

    def generate(self, task, content):
        """Generate response, trying LM Studio first."""
        # Check for code tasks that only work with LM Studio
        code_tasks = {"code_complete", "code_explain", "code_edit", "code_generate"}

        if task in code_tasks:
            if self.is_lm_studio_active():
                try:
                    return self.lm_studio.generate(task, content)
                except Exception:
                    pass
            return f"Code assistance requires LM Studio. Please start LM Studio and reload."

        # For other tasks, try LM Studio then fall back
        if self.is_lm_studio_active():
            try:
                return self.lm_studio.generate(task, content)
            except Exception:
                # Fall back to local on error
                pass

        return self.local.generate(task, content)


class LocalAIClient(AIProviderClient):
    """Local heuristic implementation for AI hooks (no network).

    Provides basic functionality using pattern matching and heuristics.
    Used as fallback when LM Studio is not available.
    """

    def generate(self, task, content):
        if task == "intent_map":
            return self._intent_map(content)
        if task == "citation_helper":
            return self._citation_helper(content)
        if task == "diff_narrator":
            return self._diff_narrator(content)
        if task == "outline_enhancer":
            return self._outline_enhancer(content)
        if task == "action_extractor":
            return self._action_extractor(content)
        if task == "typo_fixer":
            return self._typo_fixer(content)
        if task == "suggestions":
            return self._suggestions(content)
        return "No output for requested task."

    def _intent_map(self, content):
        lines = [line.strip() for line in content.splitlines() if line.strip()]
        headings = [line.lstrip("#").strip() for line in lines if line.startswith("#")]
        questions = [line for line in lines if line.endswith("?")]
        todos = [line for line in lines if "todo" in line.lower()]
        bullets = [line for line in lines if line.startswith(("-", "*"))]

        parts = []
        if headings:
            parts.append("Intent Map")
            parts.append("Core Themes:")
            parts.extend([f"- {heading}" for heading in headings[:8]])
        if bullets:
            parts.append("")
            parts.append("Key Points:")
            parts.extend([f"- {bullet.lstrip('-* ').strip()}" for bullet in bullets[:8]])
        if questions:
            parts.append("")
            parts.append("Open Questions:")
            parts.extend([f"- {question}" for question in questions[:6]])
        if todos:
            parts.append("")
            parts.append("TODOs / Actions:")
            parts.extend([f"- {todo}" for todo in todos[:6]])

        if not parts:
            parts.append("Intent Map")
            parts.append("No clear structure found. Add headings or bullets to enrich the map.")

        return "\n".join(parts)

    def _citation_helper(self, content):
        citation_pattern = re.compile(
            r"(https?://\\S+|doi:\\S+|"
            r"\\[[0-9]{1,3}(?:\\s*[,;-]\\s*[0-9]{1,3})*\\]|"
            r"\\((?:[A-Z][A-Za-z-]+(?:\\s+et\\s+al\\.)?[,;\\s]+\\d{4}[a-z]?)"
            r"(?:\\s*[,;]\\s*[A-Z][A-Za-z-]+(?:\\s+et\\s+al\\.)?[,;\\s]+\\d{4}[a-z]?)*\\))"
        )
        claim_pattern = re.compile(
            r"(shows|demonstrates|indicates|suggests|evidence|significant|"
            r"statistically|improves|reduces|increases|decreases|outperforms|"
            r"causes|correlates|leads\\s+to|results\\s+in|we\\s+find|we\\s+show)"
        )
        numeric_pattern = re.compile(r"(\\d+\\.\\d+|\\d+%|\\b\\d+\\s*(ms|s|sec|x|%|times)\\b)")
        section_skip = re.compile(r"^#{1,6}\\s*(references|bibliography|related\\s+work)\\b", re.I)
        figure_table = re.compile(r"\\b(Figure|Table)\\s+\\d+\\b")

        findings = []
        in_references = False

        for idx, line in enumerate(content.splitlines(), start=1):
            stripped = line.strip()
            if not stripped:
                continue
            if section_skip.search(stripped):
                in_references = True
                continue
            if in_references:
                continue
            if citation_pattern.search(stripped):
                continue
            if figure_table.search(stripped):
                continue

            sentences = [s.strip() for s in re.split(r"(?<=[.!?])\\s+", stripped) if s.strip()]
            for sentence in sentences:
                if citation_pattern.search(sentence):
                    continue
                if claim_pattern.search(sentence) or numeric_pattern.search(sentence):
                    findings.append((idx, sentence))

        if not findings:
            return "Citation Helper\nNo obvious claims without citations detected."

        parts = ["Citation Helper", "Potential claims without citations:"]
        for line_no, text in findings[:20]:
            parts.append(f"- Line {line_no}: {text}")
        if len(findings) > 20:
            parts.append(f"- ...and {len(findings) - 20} more")
        return "\n".join(parts)

    def _diff_narrator(self, content):
        before, after = content
        before_lines = before.splitlines()
        after_lines = after.splitlines()
        diff = list(difflib.unified_diff(before_lines, after_lines, lineterm=""))

        added = [line for line in diff if line.startswith("+") and not line.startswith("+++")]
        removed = [line for line in diff if line.startswith("-") and not line.startswith("---")]

        parts = ["Diff Narrator", f"Added lines: {len(added)}", f"Removed lines: {len(removed)}"]

        if added:
            parts.append("")
            parts.append("Notable additions:")
            parts.extend([f"- {line[1:].strip()}" for line in added[:8]])
        if removed:
            parts.append("")
            parts.append("Notable removals:")
            parts.extend([f"- {line[1:].strip()}" for line in removed[:8]])

        if len(diff) <= 2:
            parts.append("")
            parts.append("No differences detected.")

        return "\n".join(parts)

    def _outline_enhancer(self, content):
        lines = [line.rstrip() for line in content.splitlines()]
        headings = [line for line in lines if line.lstrip().startswith("#")]
        if not headings:
            return (
                "Outline Enhancer\n"
                "No headings found. Suggested starter outline:\n"
                "- # Title\n- ## Abstract\n- ## Introduction\n- ## Methods\n- ## Results\n"
                "- ## Discussion\n- ## Conclusion\n- ## References"
            )

        suggestions = ["Outline Enhancer", "Potential improvements:"]
        max_level = max(len(line.split(" ")[0].strip()) for line in headings if line.strip())
        min_level = min(len(line.split(" ")[0].strip()) for line in headings if line.strip())

        if max_level - min_level >= 3:
            suggestions.append("- Consider reducing heading depth for readability.")

        sections = [line.lstrip("#").strip().lower() for line in headings]
        expected = ["introduction", "methods", "results", "discussion", "conclusion"]
        missing = [name for name in expected if name not in sections]
        if missing:
            suggestions.append("- Consider adding: " + ", ".join(missing))

        if len(headings) < 3:
            suggestions.append("- The outline is short; consider adding more structure.")

        return "\n".join(suggestions)

    def _action_extractor(self, content):
        lines = [line.strip() for line in content.splitlines() if line.strip()]
        tasks = []
        for line in lines:
            if re.match(r"^[-*+]\\s+\\[( |x|X)\\]\\s+", line):
                tasks.append(line)
            elif re.search(r"\\b(todo|action|next step|follow[- ]?up)\\b", line, re.I):
                tasks.append(line)
            elif re.match(r"^[-*+]\\s+.+", line) and re.search(r"\\b(need to|should|must)\\b", line, re.I):
                tasks.append(line)

        if not tasks:
            return "Action Extractor\nNo clear action items found."

        parts = ["Action Extractor", "Proposed action list:"]
        for task in tasks[:20]:
            cleaned = re.sub(r"^[-*+]\\s+", "", task)
            cleaned = re.sub(r"^\\[( |x|X)\\]\\s+", "", cleaned)
            parts.append(f"- [ ] {cleaned}")
        if len(tasks) > 20:
            parts.append(f"- ...and {len(tasks) - 20} more")
        return "\n".join(parts)

    def _typo_fixer(self, content):
        misspellings = {
            "teh": "the",
            "recieve": "receive",
            "occured": "occurred",
            "seperate": "separate",
            "definately": "definitely",
            "becuase": "because",
            "adress": "address",
            "arguement": "argument",
            "calander": "calendar",
            "enviroment": "environment",
            "thier": "their",
            "wierd": "weird",
            "alot": "a lot",
            "idempotant": "idempotent",
        }
        pattern = re.compile(r"\\b(" + "|".join(re.escape(word) for word in misspellings) + r")\\b", re.I)
        count = 0

        def replace(match):
            nonlocal count
            original = match.group(0)
            replacement = misspellings[original.lower()]
            count += 1
            if original.isupper():
                return replacement.upper()
            if original[0].isupper():
                return replacement.capitalize()
            return replacement

        fixed = pattern.sub(replace, content)
        return {"text": fixed, "count": count}

    def _suggestions(self, payload):
        selection = payload.get("selection", "").strip()
        line = payload.get("line", "").strip()

        if selection:
            base = selection
        else:
            base = line

        suggestions = []

        if not base:
            suggestions = [
                "## Overview",
                "## Methods",
                "## Results",
                "## Discussion",
                "## Next Steps",
                "- TODO: ",
            ]
        elif base.startswith("#"):
            title = base.lstrip("#").strip() or "Section"
            suggestions = [
                f"## {title} - Overview",
                f"## {title} - Details",
                f"## {title} - Decisions",
                f"## {title} - Risks",
            ]
        elif base.startswith(("-", "*")):
            item = base.lstrip("-* ").strip() or "Action item"
            suggestions = [
                f"- {item} (why?)",
                f"- {item} (owner?)",
                f"- {item} (deadline?)",
            ]
        else:
            suggestions = [
                f"{base} (example?)",
                f"{base} (evidence?)",
                f"{base} (citation?)",
            ]

        return [s for s in suggestions if s]
