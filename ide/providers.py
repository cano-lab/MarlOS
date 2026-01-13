"""Provider implementations for the Document IDE.

This module contains all provider classes that extend the IDE's functionality.
Providers follow the Document IDE Architecture v0 specification:

Provider Categories:
- ALWAYS_ON: Silent, read-only, no UI (e.g., LanguageService, WordCount)
- IMPLIED: Hidden UI by default, no mutation (e.g., Preview, Outline)
- ON_DEMAND: Command-activated, may mutate with permission (e.g., Formatters)

See PROVIDER_AUTHORING_GUIDE.md for how to create new providers.
"""

import re
from enum import Enum

import markdown

from .commands import Command
from .sandbox import get_sandbox, enable_docker_sandbox, disable_sandbox, SandboxMode


class ProviderCategory(Enum):
    """Provider activation categories.

    ALWAYS_ON: Activated automatically, silent, read-only, no UI
    IMPLIED: Activated automatically, UI hidden by default, no mutation
    ON_DEMAND: Activated by command only, may mutate with confirmation
    """
    ALWAYS_ON = "always_on"
    IMPLIED = "implied"
    ON_DEMAND = "on_demand"


class Provider:
    """Base class for all providers.

    Providers are bounded units of capability that observe documents/events,
    compute derived information, and request actions via commands.

    Subclasses must implement activate(context) to register their
    functionality with the IDE.
    """

    def __init__(self, name, category):
        """Initialize a provider.

        Args:
            name: Unique provider name (e.g., "markdown_language_service")
            category: ProviderCategory determining activation behavior
        """
        self.name = name
        self.category = category

    def activate(self, context):
        """Called once when provider is registered with the IDE.

        Args:
            context: IDEContext providing access to document, events, commands
        """
        raise NotImplementedError


class MarkdownRenderer:
    """Converts markdown to HTML with styling.

    Provides two rendering modes:
    - render_html(): For in-app preview with dark/light themes
    - render_for_print(): For printing/PDF with fine-grained control

    Uses the markdown library with extensions:
    - extra: Tables, footnotes, abbreviations
    - codehilite: Syntax highlighting with Pygments
    - toc: Table of contents generation
    - sane_lists: Improved list handling

    Additional features:
    - Task list checkboxes (- [ ] and - [x])
    - Strikethrough (~~text~~)
    - Emoji support (:emoji:)
    - Code block language labels
    - External link indicators

    Mermaid diagrams are preserved as styled placeholder blocks.
    """

    # Common emoji mappings
    EMOJI_MAP = {
        ':smile:': '😊', ':grin:': '😁', ':joy:': '😂', ':heart:': '❤️',
        ':thumbsup:': '👍', ':thumbsdown:': '👎', ':clap:': '👏', ':fire:': '🔥',
        ':star:': '⭐', ':check:': '✅', ':x:': '❌', ':warning:': '⚠️',
        ':info:': 'ℹ️', ':question:': '❓', ':exclamation:': '❗', ':bulb:': '💡',
        ':rocket:': '🚀', ':bug:': '🐛', ':gear:': '⚙️', ':wrench:': '🔧',
        ':hammer:': '🔨', ':key:': '🔑', ':lock:': '🔒', ':unlock:': '🔓',
        ':folder:': '📁', ':file:': '📄', ':book:': '📖', ':memo:': '📝',
        ':pencil:': '✏️', ':link:': '🔗', ':calendar:': '📅', ':clock:': '🕐',
        ':email:': '📧', ':phone:': '📱', ':computer:': '💻', ':cloud:': '☁️',
        ':sun:': '☀️', ':moon:': '🌙', ':earth:': '🌍', ':tree:': '🌳',
        ':coffee:': '☕', ':pizza:': '🍕', ':cake:': '🎂', ':gift:': '🎁',
        ':tada:': '🎉', ':sparkles:': '✨', ':zap:': '⚡', ':boom:': '💥',
        ':+1:': '👍', ':-1:': '👎', ':ok:': '👌', ':wave:': '👋',
        ':eyes:': '👀', ':thinking:': '🤔', ':100:': '💯', ':heavy_check_mark:': '✔️',
    }

    def _convert_emojis(self, content):
        """Convert :emoji: codes to actual emoji characters."""
        for code, emoji in self.EMOJI_MAP.items():
            content = content.replace(code, emoji)
        return content

    def _convert_strikethrough(self, content):
        """Convert ~~text~~ to strikethrough HTML."""
        return re.sub(r'~~(.+?)~~', r'<del>\1</del>', content)

    def _convert_task_lists(self, html):
        """Convert task list items to checkboxes."""
        # Convert unchecked: [ ]
        html = re.sub(
            r'<li>\s*\[\s*\]',
            '<li class="task-item"><input type="checkbox" disabled>',
            html
        )
        # Convert checked: [x] or [X]
        html = re.sub(
            r'<li>\s*\[[xX]\]',
            '<li class="task-item task-done"><input type="checkbox" checked disabled>',
            html
        )
        return html

    def _add_code_labels(self, html):
        """Add language labels to code blocks."""
        # Find code blocks and add language label
        def add_label(match):
            style = match.group(1) if match.group(1) else ''
            # Try to extract language from style attribute or class
            lang_match = re.search(r'language-(\w+)', style)
            if not lang_match:
                lang_match = re.search(r'highlight (\w+)', style)
            lang = lang_match.group(1) if lang_match else ''
            if lang:
                return f'<div class="code-block"><div class="code-label">{lang}</div><pre{style}>'
            return match.group(0)

        html = re.sub(r'<pre([^>]*)>', add_label, html)
        # Close the wrapper div after </pre>
        html = re.sub(r'</pre>(?!</div>)', '</pre></div>', html)
        return html

    def _mark_external_links(self, html):
        """Add external link indicator to http/https links."""
        def add_indicator(match):
            attrs = match.group(1)
            url = match.group(2)
            text = match.group(3)
            if url.startswith(('http://', 'https://')):
                return f'<a{attrs}href="{url}" class="external-link">{text}<span class="external-icon">↗</span></a>'
            return match.group(0)

        return re.sub(r'<a([^>]*)href="([^"]+)"[^>]*>([^<]+)</a>', add_indicator, html)

    def render_html(self, content, dark_mode=False):
        mermaid_blocks = []
        code_blocks = []

        def save_mermaid(match):
            code = match.group(1)
            idx = len(mermaid_blocks)
            mermaid_blocks.append(code)
            return f"<!--MERMAID_{idx}-->"

        def save_code_block(match):
            lang = match.group(1) or ''
            code = match.group(2)
            idx = len(code_blocks)
            code_blocks.append((lang, code))
            return f"<!--CODEBLOCK_{idx}-->"

        # Pre-process: save mermaid and code blocks
        content = re.sub(r"```mermaid\s*([\s\S]*?)```", save_mermaid, content)
        content = re.sub(r"```(\w*)\n([\s\S]*?)```", save_code_block, content)

        # Convert emojis and strikethrough before markdown processing
        content = self._convert_emojis(content)
        content = self._convert_strikethrough(content)

        md = markdown.Markdown(
            extensions=["extra", "codehilite", "toc", "sane_lists"],
            extension_configs={
                "codehilite": {
                    "css_class": "highlight",
                    "guess_lang": True,
                    "noclasses": True,
                }
            },
        )

        body = md.convert(content)

        # Post-process: convert task lists
        body = self._convert_task_lists(body)

        # Restore code blocks with language labels
        for idx, (lang, code) in enumerate(code_blocks):
            import html as html_lib
            escaped_code = html_lib.escape(code)
            if lang:
                code_html = (
                    f'<div class="code-block">'
                    f'<div class="code-label">{lang}</div>'
                    f'<pre><code class="language-{lang}">{escaped_code}</code></pre>'
                    f'</div>'
                )
            else:
                code_html = f'<pre><code>{escaped_code}</code></pre>'
            body = body.replace(f"<p><!--CODEBLOCK_{idx}--></p>", code_html)
            body = body.replace(f"<!--CODEBLOCK_{idx}-->", code_html)

        # Restore mermaid blocks
        for idx, code in enumerate(mermaid_blocks):
            mermaid_html = (
                '<div class="mermaid-container">'
                '<div class="mermaid-header">Mermaid Diagram</div>'
                f'<pre class="mermaid-code">{code}</pre>'
                '<div class="mermaid-note">Diagrams render in Mermaid-compatible viewers</div>'
                '</div>'
            )
            body = body.replace(f"<p><!--MERMAID_{idx}--></p>", mermaid_html)
            body = body.replace(f"<!--MERMAID_{idx}-->", mermaid_html)

        # Mark external links
        body = self._mark_external_links(body)

        css = self._get_css(dark_mode)
        return (
            "<!DOCTYPE html>"
            "<html>"
            '<head><meta charset="utf-8"><style>'
            f"{css}</style></head>"
            f"<body>{body}</body>"
            "</html>"
        )

    def render_for_print(
        self,
        content,
        font_size,
        use_black,
        line_spacing=1.5,
        grayscale=False,
        avoid_orphan_headings=True,
    ):
        md = markdown.Markdown(
            extensions=["extra", "codehilite", "toc", "sane_lists"],
            extension_configs={
                "codehilite": {
                    "css_class": "highlight",
                    "guess_lang": True,
                    "noclasses": True,
                }
            },
        )

        body = md.convert(content)
        text_color = "#000000" if use_black else "#24292e"

        if grayscale:
            code_bg = "#f0f0f0"
            code_border = "#999"
            table_header_bg = "#e0e0e0"
            blockquote_border = "#999"
            blockquote_color = "#444"
            link_color = text_color
        else:
            code_bg = "#f8f9fa"
            code_border = "#ddd"
            table_header_bg = "#e8f4fd"
            blockquote_border = "#0078d4"
            blockquote_color = "#555"
            link_color = "#0066cc"

        heading_break = (
            "page-break-after: avoid; page-break-inside: avoid;"
            if avoid_orphan_headings
            else ""
        )

        css = f"""
body {{
    font-family: "Segoe UI", Arial, sans-serif;
    font-size: {font_size}pt;
    line-height: {line_spacing};
    color: {text_color};
    widows: 3;
    orphans: 3;
}}
p {{
    margin-bottom: {int(font_size * 0.8)}pt;
    page-break-inside: avoid;
}}
a {{
    color: {link_color};
    text-decoration: none;
}}
h1, h2, h3, h4, h5, h6 {{
    font-weight: bold;
    color: {text_color};
    margin-top: {int(font_size * 1.2)}pt;
    margin-bottom: {int(font_size * 0.6)}pt;
    {heading_break}
}}
h1 {{ font-size: {font_size + 10}pt; }}
h2 {{ font-size: {font_size + 6}pt; }}
h3 {{ font-size: {font_size + 3}pt; }}
h4 {{ font-size: {font_size + 1}pt; }}
code, pre {{
    font-family: Consolas, monospace;
    font-size: {font_size - 1}pt;
    background-color: {code_bg};
}}
pre {{
    padding: 8pt;
    border: 1px solid {code_border};
    margin: {int(font_size * 0.5)}pt 0;
    page-break-inside: avoid;
}}
ul, ol {{
    margin-bottom: {int(font_size * 0.8)}pt;
}}
li {{
    margin-bottom: {int(font_size * 0.3)}pt;
}}
table {{
    border-collapse: collapse;
    margin: {int(font_size * 0.5)}pt 0;
    page-break-inside: avoid;
}}
th, td {{
    border: 1px solid #666;
    padding: 6pt 10pt;
}}
th {{
    background-color: {table_header_bg};
    font-weight: bold;
}}
blockquote {{
    margin: {int(font_size * 0.5)}pt 0;
    padding-left: 12pt;
    border-left: 3pt solid {blockquote_border};
    color: {blockquote_color};
    page-break-inside: avoid;
}}
img {{
    max-width: 100%;
    page-break-inside: avoid;
}}
"""
        return f"<!DOCTYPE html><html><head><style>{css}</style></head><body>{body}</body></html>"

    def _get_css(self, dark_mode):
        # Shared styles for both themes
        shared = """
/* Task list items */
.task-item { list-style: none; margin-left: -20px; }
.task-item input[type="checkbox"] { margin-right: 8px; transform: scale(1.2); vertical-align: middle; }
.task-done { opacity: 0.7; }
.task-done input[type="checkbox"] { accent-color: #28a745; }

/* Code blocks with language labels */
.code-block { position: relative; margin: 16px 0; }
.code-label {
    position: absolute; top: 0; right: 0;
    padding: 2px 8px; font-size: 11px; font-family: sans-serif;
    border-radius: 0 4px 0 4px; text-transform: uppercase; letter-spacing: 0.5px;
}
.code-block pre { margin: 0; border-radius: 4px; }
.code-block code { display: block; overflow-x: auto; }

/* External links */
.external-link { position: relative; }
.external-icon { font-size: 10px; margin-left: 2px; opacity: 0.7; vertical-align: super; }

/* Strikethrough */
del { text-decoration: line-through; opacity: 0.6; }

/* Better typography */
p { line-height: 1.7; }
li { line-height: 1.6; margin-bottom: 4px; }
"""

        if dark_mode:
            return shared + """
body { font-family: "Segoe UI", -apple-system, Arial, sans-serif; font-size: 14px; line-height: 1.6;
       color: #d4d4d4; background: #1e1e1e; margin: 20px; max-width: 900px; }
h1, h2, h3, h4, h5, h6 { margin-top: 24px; margin-bottom: 12px; font-weight: 600; color: #fff; }
h1 { font-size: 28px; border-bottom: 2px solid #444; padding-bottom: 8px; }
h2 { font-size: 22px; border-bottom: 1px solid #444; padding-bottom: 6px; }
h3 { font-size: 18px; } h4 { font-size: 16px; }
p { margin: 0 0 16px 0; } a { color: #6cb6ff; text-decoration: none; }
a:hover { text-decoration: underline; }
code { font-family: "Cascadia Code", Consolas, monospace; background: #2d2d2d; padding: 2px 6px;
       font-size: 13px; border-radius: 3px; }
pre { font-family: "Cascadia Code", Consolas, monospace; background: #2d2d2d; padding: 16px;
      font-size: 13px; border: 1px solid #444; border-radius: 6px; overflow-x: auto; }
pre code { background: transparent; padding: 0; }
blockquote { margin: 0 0 16px 0; padding: 12px 16px; color: #b0b0b0; border-left: 4px solid #3794ff;
             background: #252526; border-radius: 0 4px 4px 0; }
table { border-collapse: collapse; margin-bottom: 16px; width: 100%; }
th, td { border: 1px solid #444; padding: 10px 14px; }
th { background: #2d2d2d; font-weight: 600; }
tr:nth-child(even) { background: #252526; }
ul, ol { margin: 0 0 16px 0; padding-left: 24px; }
hr { border: none; border-top: 2px solid #444; margin: 32px 0; }
img { max-width: 100%; border-radius: 4px; }
.code-label { background: #3794ff; color: #fff; }
.mermaid-container { background: #252526; border: 2px solid #3794ff; border-radius: 8px; margin: 16px 0; overflow: hidden; }
.mermaid-header { background: #3794ff; color: #fff; padding: 8px 12px; font-weight: bold; font-size: 13px; }
.mermaid-code { margin: 0; padding: 16px; background: #1e1e1e; color: #9cdcfe; font-size: 12px; white-space: pre-wrap; }
.mermaid-note { background: #252526; color: #888; padding: 6px 12px; font-size: 11px; font-style: italic; border-top: 1px solid #444; }
"""
        return shared + """
body { font-family: "Segoe UI", -apple-system, Arial, sans-serif; font-size: 14px; line-height: 1.6;
       color: #24292e; background: #fff; margin: 20px; max-width: 900px; }
h1, h2, h3, h4, h5, h6 { margin-top: 24px; margin-bottom: 12px; font-weight: 600; color: #1a1a1a; }
h1 { font-size: 28px; border-bottom: 2px solid #eaecef; padding-bottom: 8px; }
h2 { font-size: 22px; border-bottom: 1px solid #eaecef; padding-bottom: 6px; }
h3 { font-size: 18px; } h4 { font-size: 16px; }
p { margin: 0 0 16px 0; } a { color: #0366d6; text-decoration: none; }
a:hover { text-decoration: underline; }
code { font-family: "Cascadia Code", Consolas, monospace; background: #f6f8fa; padding: 2px 6px;
       font-size: 13px; border-radius: 3px; }
pre { font-family: "Cascadia Code", Consolas, monospace; background: #f6f8fa; padding: 16px;
      font-size: 13px; border: 1px solid #e1e4e8; border-radius: 6px; overflow-x: auto; }
pre code { background: transparent; padding: 0; }
blockquote { margin: 0 0 16px 0; padding: 12px 16px; color: #57606a; border-left: 4px solid #0078d4;
             background: #f8f9fa; border-radius: 0 4px 4px 0; }
table { border-collapse: collapse; margin-bottom: 16px; width: 100%; }
th, td { border: 1px solid #dfe2e5; padding: 10px 14px; }
th { background: #f6f8fa; font-weight: 600; }
tr:nth-child(even) { background: #f8f9fa; }
ul, ol { margin: 0 0 16px 0; padding-left: 24px; }
hr { border: none; border-top: 2px solid #e1e4e8; margin: 32px 0; }
img { max-width: 100%; border-radius: 4px; }
.code-label { background: #0078d4; color: #fff; }
.mermaid-container { background: #f8f9fa; border: 2px solid #0078d4; border-radius: 8px; margin: 16px 0; overflow: hidden; }
.mermaid-header { background: #0078d4; color: #fff; padding: 8px 12px; font-weight: bold; font-size: 13px; }
.mermaid-code { margin: 0; padding: 16px; background: #fff; color: #24292e; font-size: 12px; white-space: pre-wrap; }
.mermaid-note { background: #f1f3f5; color: #666; padding: 6px 12px; font-size: 11px; font-style: italic; border-top: 1px solid #ddd; }
"""


class MarkdownLanguageService(Provider):
    """Always-on provider that parses document structure.

    This is the core "understanding" service that other providers can query.
    It maintains a cached parse of the document structure including:
    - headings: List of {level, text, line}
    - links: List of (text, url) tuples
    - code_blocks: List of {line, fence}
    - tasks: List of {done, text, line}

    The structure is updated on every document_changed event.

    Category: ALWAYS_ON (silent, read-only, no UI)
    """

    def __init__(self):
        super().__init__("markdown_language_service", ProviderCategory.ALWAYS_ON)
        self.structure = {
            "headings": [],
            "links": [],
            "code_blocks": [],
            "tasks": [],
        }

    def activate(self, context):
        """Parse initial content and subscribe to changes."""
        self._parse(context.document.content)
        context.events.document_changed.connect(lambda doc: self._parse(doc.content))

    def _parse(self, content):
        """Parse markdown content and update the structure cache."""
        headings = []
        links = []
        code_blocks = []
        tasks = []
        in_code = False
        for idx, line in enumerate(content.splitlines(), start=1):
            if line.strip().startswith("```"):
                in_code = not in_code
                code_blocks.append({"line": idx, "fence": line.strip()})
                continue
            if in_code:
                continue
            heading_match = re.match(r"(#{1,6})\\s+(.*)", line)
            if heading_match:
                headings.append(
                    {
                        "level": len(heading_match.group(1)),
                        "text": heading_match.group(2).strip(),
                        "line": idx,
                    }
                )
            links.extend(re.findall(r"\[([^\]]+)\]\(([^)]+)\)", line))
            task_match = re.match(r"\\s*[-*+]\\s+\\[( |x|X)\\]\\s+(.*)", line)
            if task_match:
                tasks.append(
                    {
                        "done": task_match.group(1).lower() == "x",
                        "text": task_match.group(2).strip(),
                        "line": idx,
                    }
                )
        self.structure = {
            "headings": headings,
            "links": links,
            "code_blocks": code_blocks,
            "tasks": tasks,
        }


class OutlineProvider(Provider):
    def __init__(self, language_service):
        super().__init__("outline", ProviderCategory.IMPLIED)
        self.language_service = language_service

    def activate(self, context):
        return

    def get_outline(self):
        return self.language_service.structure.get("headings", [])


class WordCountProvider(Provider):
    def __init__(self):
        super().__init__("word_count", ProviderCategory.IMPLIED)
        self.stats = {"words": 0, "chars": 0}

    def activate(self, context):
        self._update(context.document.content)
        context.events.document_changed.connect(lambda doc: self._update(doc.content))

    def _update(self, content):
        tokens = re.findall(r"[A-Za-z0-9]+(?:['-][A-Za-z0-9]+)?", content)
        if not tokens:
            tokens = [word for word in re.split(r"\\s+", content.strip()) if word]
        self.stats = {"words": len(tokens), "chars": len(content)}


class PreviewProvider(Provider):
    def __init__(self, renderer):
        super().__init__("preview", ProviderCategory.IMPLIED)
        self.renderer = renderer
        self._content = ""
        self._cache = {}

    def activate(self, context):
        self._content = context.document.content
        self._cache = {}
        context.events.document_changed.connect(self._on_document_changed)

    def _on_document_changed(self, document):
        self._content = document.content
        self._cache = {}

    def get_html(self, dark_mode):
        if dark_mode not in self._cache:
            self._cache[dark_mode] = self.renderer.render_html(
                self._content, dark_mode=dark_mode
            )
        return self._cache[dark_mode]


class FormattingProvider(Provider):
    """On-demand provider for markdown formatting commands.

    Registers commands for common markdown formatting operations:
    - Headings (H1-H3)
    - Bold, Italic, Inline Code
    - Links, Images
    - Lists (bulleted, numbered)
    - Blockquotes, Tables, Code blocks
    - Mermaid diagrams

    All commands modify the document and are undoable via the editor's
    built-in undo stack.

    Category: ON_DEMAND (command-activated, modifies document)
    """

    def __init__(self):
        super().__init__("markdown_formatting", ProviderCategory.ON_DEMAND)

    def activate(self, context):
        """Register all formatting commands."""
        context.commands.register(
            Command(
                "markdown.format.heading1",
                "Heading 1",
                lambda ctx: self._insert_at_line_start(ctx, "# "),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.heading2",
                "Heading 2",
                lambda ctx: self._insert_at_line_start(ctx, "## "),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.heading3",
                "Heading 3",
                lambda ctx: self._insert_at_line_start(ctx, "### "),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.bold",
                "Bold",
                lambda ctx: self._wrap_selection(ctx, "**", "**"),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.italic",
                "Italic",
                lambda ctx: self._wrap_selection(ctx, "*", "*"),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.code",
                "Inline Code",
                lambda ctx: self._wrap_selection(ctx, "`", "`"),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.link",
                "Insert Link",
                lambda ctx: self._insert_text(ctx, "[link text](url)"),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.image",
                "Insert Image",
                lambda ctx: self._insert_text(ctx, "![alt text](image_url)"),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.list.bulleted",
                "Bulleted List",
                lambda ctx: self._insert_at_line_start(ctx, "- "),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.list.numbered",
                "Numbered List",
                lambda ctx: self._insert_at_line_start(ctx, "1. "),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.quote",
                "Blockquote",
                lambda ctx: self._insert_at_line_start(ctx, "> "),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.table",
                "Insert Table",
                lambda ctx: self._insert_text(
                    ctx,
                    "| Header 1 | Header 2 | Header 3 |\n"
                    "|----------|----------|----------|\n"
                    "| Cell 1   | Cell 2   | Cell 3   |\n"
                    "| Cell 4   | Cell 5   | Cell 6   |\n",
                ),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.codeblock",
                "Insert Code Block",
                lambda ctx: self._insert_text(ctx, "```python\n# code here\n```"),
                modifies_document=True,
            )
        )
        context.commands.register(
            Command(
                "markdown.format.diagram",
                "Insert Mermaid Diagram",
                lambda ctx: self._insert_text(
                    ctx,
                    "```mermaid\n"
                    "graph TD\n"
                    "    A[Start] --> B{Decision}\n"
                    "    B -->|Yes| C[Action 1]\n"
                    "    B -->|No| D[Action 2]\n"
                    "    C --> E[End]\n"
                    "    D --> E\n"
                    "```\n",
                ),
                modifies_document=True,
            )
        )

    def _insert_text(self, context, text):
        context.insert_text(text)

    def _wrap_selection(self, context, before, after):
        editor = context.editor
        if not editor:
            return
        cursor = editor.textCursor()
        selected = cursor.selectedText()
        if selected:
            cursor.insertText(f"{before}{selected}{after}")
        else:
            cursor.insertText(f"{before}text{after}")
        editor.setTextCursor(cursor)
        editor.setFocus()

    def _insert_at_line_start(self, context, prefix):
        editor = context.editor
        if not editor:
            return
        cursor = editor.textCursor()
        cursor.movePosition(cursor.MoveOperation.StartOfLine)
        cursor.insertText(prefix)
        editor.setTextCursor(cursor)
        editor.setFocus()


class DocumentCommandProvider(Provider):
    def __init__(self, save_cb, save_as_cb, print_cb, export_pdf_cb):
        super().__init__("document_commands", ProviderCategory.ON_DEMAND)
        self._save_cb = save_cb
        self._save_as_cb = save_as_cb
        self._print_cb = print_cb
        self._export_pdf_cb = export_pdf_cb

    def activate(self, context):
        context.commands.register(
            Command("document.save", "Save", lambda ctx: self._save_cb())
        )
        context.commands.register(
            Command("document.save_as", "Save As", lambda ctx: self._save_as_cb())
        )
        context.commands.register(
            Command("document.print", "Print", lambda ctx: self._print_cb())
        )
        context.commands.register(
            Command(
                "markdown.export.pdf",
                "Export PDF",
                lambda ctx: self._export_pdf_cb(),
            )
        )


class IntentMapProvider(Provider):
    def __init__(self, ai_client):
        super().__init__("intent_map", ProviderCategory.ON_DEMAND)
        self.ai_client = ai_client

    def activate(self, context):
        context.commands.register(
            Command(
                "document.intent.map",
                "Generate Intent Map",
                lambda ctx: self._run(ctx),
            )
        )

    def _run(self, context):
        content = context.get_selection() or context.document.content
        if not content.strip():
            context.present_text("Intent Map", "No content to analyze.", allow_insert=False)
            return True
        result = self.ai_client.generate("intent_map", content)
        context.present_text("Intent Map", result, allow_insert=True)
        return True


class CitationHelperProvider(Provider):
    def __init__(self, ai_client):
        super().__init__("citation_helper", ProviderCategory.ON_DEMAND)
        self.ai_client = ai_client

    def activate(self, context):
        context.commands.register(
            Command(
                "document.citation.helper",
                "Citation Helper",
                lambda ctx: self._run(ctx),
            )
        )

    def _run(self, context):
        content = context.get_selection() or context.document.content
        if not content.strip():
            context.present_text("Citation Helper", "No content to analyze.", allow_insert=False)
            return True
        result = self.ai_client.generate("citation_helper", content)
        context.present_text("Citation Helper", result, allow_insert=False)
        return True


class DiffNarratorProvider(Provider):
    def __init__(self, ai_client):
        super().__init__("diff_narrator", ProviderCategory.ON_DEMAND)
        self.ai_client = ai_client

    def activate(self, context):
        context.commands.register(
            Command(
                "document.diff.narrator",
                "Diff Narrator (Since Last Save)",
                lambda ctx: self._run(ctx),
            )
        )

    def _run(self, context):
        before = context.document.last_saved_content
        after = context.document.content
        result = self.ai_client.generate("diff_narrator", (before, after))
        context.present_text("Diff Narrator", result, allow_insert=False)
        return True


class OutlineEnhancerProvider(Provider):
    def __init__(self, ai_client):
        super().__init__("outline_enhancer", ProviderCategory.ON_DEMAND)
        self.ai_client = ai_client

    def activate(self, context):
        context.commands.register(
            Command(
                "document.outline.enhancer",
                "Outline Enhancer",
                lambda ctx: self._run(ctx),
            )
        )

    def _run(self, context):
        content = context.get_selection() or context.document.content
        if not content.strip():
            context.present_text("Outline Enhancer", "No content to analyze.", allow_insert=False)
            return True
        result = self.ai_client.generate("outline_enhancer", content)
        context.present_text("Outline Enhancer", result, allow_insert=True)
        return True


class ActionExtractorProvider(Provider):
    def __init__(self, ai_client):
        super().__init__("action_extractor", ProviderCategory.ON_DEMAND)
        self.ai_client = ai_client

    def activate(self, context):
        context.commands.register(
            Command(
                "document.action.extractor",
                "Action Extractor",
                lambda ctx: self._run(ctx),
            )
        )

    def _run(self, context):
        content = context.get_selection() or context.document.content
        if not content.strip():
            context.present_text("Action Extractor", "No content to analyze.", allow_insert=False)
            return True
        result = self.ai_client.generate("action_extractor", content)
        context.present_text("Action Extractor", result, allow_insert=True)
        return True


class ImageGeneratorProvider(Provider):
    def __init__(self):
        super().__init__("image_generator", ProviderCategory.ON_DEMAND)

    def activate(self, context):
        context.commands.register(
            Command(
                "markdown.generate.image",
                "Generate Image (SVG)",
                lambda ctx: self._run(ctx),
                modifies_document=True,
                confirm="Generate a placeholder SVG image?",
            )
        )

    def _run(self, context):
        caption = context.get_selection().strip() or "Generated Image"
        file_path = context.request_save_path(
            "Save Generated Image", "SVG Files (*.svg);;All Files (*)"
        )
        if not file_path:
            return False

        svg = self._build_svg(caption, 768, 432)
        try:
            with open(file_path, "w", encoding="utf-8") as handle:
                handle.write(svg)
        except Exception:
            context.present_text("Image Generator", "Failed to save image.", allow_insert=False)
            return False

        choice = context.choose_option(
            "Image Generator",
            "Image saved. What do you want to do next?",
            ["Insert Link", "Save Only", "Discard"],
        )
        if choice == "Insert Link":
            context.insert_text(f"![{caption}]({file_path})")
        return True

    def _build_svg(self, caption, width, height):
        safe_caption = caption.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        return (
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
            f"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\">\n"
            "  <rect width=\"100%\" height=\"100%\" fill=\"#f2f2f2\" stroke=\"#cccccc\" />\n"
            "  <rect x=\"24\" y=\"24\" width=\"720\" height=\"384\" fill=\"#ffffff\" stroke=\"#dddddd\" />\n"
            f"  <text x=\"50%\" y=\"50%\" text-anchor=\"middle\" fill=\"#555555\" "
            "font-family=\"Segoe UI, Arial\" font-size=\"20\">"
            f"{safe_caption}</text>\n"
            "  <text x=\"50%\" y=\"58%\" text-anchor=\"middle\" fill=\"#888888\" "
            "font-family=\"Segoe UI, Arial\" font-size=\"12\">"
            "Placeholder SVG</text>\n"
            "</svg>\n"
        )


class TypoFixerProvider(Provider):
    def __init__(self, ai_client):
        super().__init__("typo_fixer", ProviderCategory.ON_DEMAND)
        self.ai_client = ai_client

    def activate(self, context):
        context.commands.register(
            Command(
                "document.typo.fix",
                "Fix Typos",
                lambda ctx: self._run(ctx),
                modifies_document=True,
            )
        )

    def _run(self, context):
        selection = context.get_selection()
        content = selection if selection else context.document.content
        result = self.ai_client.generate("typo_fixer", content)
        corrected = result.get("text", content)
        count = result.get("count", 0)

        if count == 0 or corrected == content:
            context.present_text("Fix Typos", "No obvious typos found.", allow_insert=False)
            return True

        options = []
        if selection:
            options.append("Replace Selection")
        options.append("Replace Document")
        options.append("Insert at Cursor")
        options.append("Cancel")

        choice = context.review_text("Fix Typos", content, corrected, options)
        if choice == "Replace Selection" and selection:
            editor = context.editor
            if editor:
                cursor = editor.textCursor()
                cursor.insertText(corrected)
                editor.setTextCursor(cursor)
                editor.setFocus()
            else:
                context.insert_text(corrected)
            return True
        if choice == "Replace Document":
            editor = context.editor
            if editor:
                editor.blockSignals(True)
                editor.setPlainText(corrected)
                editor.blockSignals(False)
                context.set_content(corrected)
            else:
                context.set_content(corrected)
            return True
        if choice == "Insert at Cursor":
            context.insert_text(corrected)
            return True
        return False


class SuggestionProvider(Provider):
    def __init__(self, ai_client):
        super().__init__("suggestion_provider", ProviderCategory.ON_DEMAND)
        self.ai_client = ai_client

    def activate(self, context):
        context.commands.register(
            Command(
                "document.text.suggestions",
                "Text Suggestions",
                lambda ctx: self._run(ctx),
                modifies_document=True,
            )
        )

    def _run(self, context):
        selection = context.get_selection()
        line = ""
        if context.editor:
            line = context.editor.textCursor().block().text()
        suggestions = self.ai_client.generate(
            "suggestions", {"selection": selection, "line": line}
        )
        if not suggestions:
            context.present_text("Text Suggestions", "No suggestions available.", allow_insert=False)
            return True
        choice = context.present_suggestions("Text Suggestions", suggestions)
        if choice:
            context.insert_text(choice)
        return True


class LexiconProvider(Provider):
    def __init__(self, lexicon):
        super().__init__("lexicon_lookup", ProviderCategory.ON_DEMAND)
        self.lexicon = lexicon

    def activate(self, context):
        context.commands.register(
            Command(
                "document.lookup.term",
                "Lookup Definition",
                lambda ctx: self._run(ctx),
            )
        )

    def _run(self, context):
        term = context.get_selection()
        if not term and context.editor:
            term = self._word_under_cursor(context.editor)
        term = (term or "").strip()
        if not term:
            context.present_text("Lookup Definition", "No term selected.", allow_insert=False)
            return True

        entry = self.lexicon.lookup(term)
        if not entry:
            context.present_text(
                "Lookup Definition",
                f"No definition found for '{term}'.",
                allow_insert=False,
            )
            return True

        parts = [f"{term}", ""]
        definition = entry.get("definition", "")
        if definition:
            parts.append(f"Definition: {definition}")
        synonyms = entry.get("synonyms", [])
        if synonyms:
            parts.append(f"Synonyms: {', '.join(synonyms)}")

        context.present_text("Lookup Definition", "\n".join(parts), allow_insert=False)
        return True

    def _word_under_cursor(self, editor):
        cursor = editor.textCursor()
        cursor.select(cursor.SelectionType.WordUnderCursor)
        return cursor.selectedText()


class TimerProvider(Provider):
    def __init__(self, start_cb, stop_cb, status_cb):
        super().__init__("timer", ProviderCategory.ON_DEMAND)
        self._start_cb = start_cb
        self._stop_cb = stop_cb
        self._status_cb = status_cb

    def activate(self, context):
        context.commands.register(
            Command("timer.start", "Start Timer", lambda ctx: self._start_cb())
        )
        context.commands.register(
            Command("timer.stop", "Stop Timer", lambda ctx: self._stop_cb())
        )
        context.commands.register(
            Command("timer.status", "Timer Status", lambda ctx: self._status_cb())
        )


class CodingAgentProvider(Provider):
    """On-demand provider for AI-powered code assistance.

    Provides inline and block code operations powered by LM Studio:
    - Code completion at cursor
    - Code explanation for selected code
    - Code editing with natural language instructions
    - Code generation from descriptions

    Requires LM Studio to be running. Falls back to a message if unavailable.

    Category: ON_DEMAND (command-activated, may modify document)
    """

    def __init__(self, ai_client):
        super().__init__("coding_agent", ProviderCategory.ON_DEMAND)
        self.ai_client = ai_client

    def activate(self, context):
        """Register coding agent commands."""
        # Inline completion - complete code at cursor position
        context.commands.register(
            Command(
                "code.complete",
                "Complete Code at Cursor",
                lambda ctx: self._complete_code(ctx),
                modifies_document=True,
            )
        )

        # Explain selected code
        context.commands.register(
            Command(
                "code.explain",
                "Explain Selected Code",
                lambda ctx: self._explain_code(ctx),
                requires_selection=True,
            )
        )

        # Edit code with instruction
        context.commands.register(
            Command(
                "code.edit",
                "Edit Code with Instruction",
                lambda ctx: self._edit_code(ctx),
                requires_selection=True,
                modifies_document=True,
            )
        )

        # Generate code from description
        context.commands.register(
            Command(
                "code.generate",
                "Generate Code from Description",
                lambda ctx: self._generate_code(ctx),
                modifies_document=True,
            )
        )

        # Check LM Studio status
        context.commands.register(
            Command(
                "code.status",
                "Check AI Status",
                lambda ctx: self._check_status(ctx),
            )
        )

    def _complete_code(self, context):
        """Complete code at the current cursor position."""
        if not context.editor:
            return False

        cursor = context.editor.textCursor()
        cursor_pos = cursor.position()
        content = context.document.content

        result = self.ai_client.generate("code_complete", {
            "code": content,
            "cursor_pos": cursor_pos
        })

        if isinstance(result, str) and not result.startswith("Code assistance requires"):
            context.insert_text(result)
            return True

        context.present_text("Code Complete", result, allow_insert=False)
        return False

    def _explain_code(self, context):
        """Explain the selected code."""
        selection = context.get_selection()
        if not selection:
            return False

        result = self.ai_client.generate("code_explain", selection)
        context.present_text("Code Explanation", result, allow_insert=False)
        return True

    def _edit_code(self, context):
        """Edit selected code with a natural language instruction."""
        selection = context.get_selection()
        if not selection:
            return False

        # Ask for instruction
        instruction = context.choose_option(
            "Edit Code",
            "Enter instruction for how to modify the code:",
            ["Refactor for clarity", "Add error handling", "Optimize performance", "Add comments"]
        )
        if not instruction:
            return False

        result = self.ai_client.generate("code_edit", {
            "code": selection,
            "instruction": instruction
        })

        if isinstance(result, str) and not result.startswith("Code assistance requires"):
            # Show review dialog
            choice = context.review_text(
                "Code Edit",
                selection,
                result,
                ["Accept", "Cancel"]
            )
            if choice == "Accept":
                context.insert_text(result)
                return True

        return False

    def _generate_code(self, context):
        """Generate code from a description."""
        description = context.choose_option(
            "Generate Code",
            "Describe what code to generate:",
            ["Function to...", "Class for...", "Script that...", "Test for..."]
        )
        if not description:
            return False

        result = self.ai_client.generate("code_generate", description)

        if isinstance(result, str) and not result.startswith("Code assistance requires"):
            context.present_text("Generated Code", result, allow_insert=True)
            return True

        context.present_text("Generate Code", result, allow_insert=False)
        return False

    def _check_status(self, context):
        """Check if LM Studio is available."""
        if hasattr(self.ai_client, 'is_lm_studio_active'):
            active = self.ai_client.is_lm_studio_active()
            if active:
                status = "LM Studio is connected and active.\n\nAI-powered code assistance is available."
            else:
                status = (
                    "LM Studio is not connected.\n\n"
                    "To enable AI code assistance:\n"
                    "1. Start LM Studio\n"
                    "2. Load a model\n"
                    "3. Start the local server (default: localhost:1234)\n"
                    "4. Use View > Settings to configure the endpoint if needed"
                )
        else:
            status = "AI client does not support status checking."

        context.present_text("AI Status", status, allow_insert=False)
        return True


class CodeRunnerProvider(Provider):
    """Provider for running code in various languages.

    Executes code with proper sandboxing and permission checks.
    Only works on documents with ai_access="edit" or explicit permission.
    """

    SUPPORTED_LANGUAGES = {
        "python": {"cmd": "python", "ext": ".py"},
        "py": {"cmd": "python", "ext": ".py"},
        "node": {"cmd": "node", "ext": ".js"},
        "javascript": {"cmd": "node", "ext": ".js"},
        "js": {"cmd": "node", "ext": ".js"},
    }

    def __init__(self):
        super().__init__("code_runner", ProviderCategory.ON_DEMAND)

    def activate(self, context):
        context.commands.register(
            Command(
                "code.run",
                "Compiler: Run Code File",
                lambda ctx: self._run_code(ctx),
            )
        )
        context.commands.register(
            Command(
                "code.run.selection",
                "Compiler: Run Selection",
                lambda ctx: self._run_selection(ctx),
            )
        )
        context.commands.register(
            Command(
                "sandbox.enable",
                "Sandbox: Enable Docker Isolation",
                lambda ctx: self._enable_sandbox(ctx),
            )
        )
        context.commands.register(
            Command(
                "sandbox.disable",
                "Sandbox: Disable (Direct Execution)",
                lambda ctx: self._disable_sandbox(ctx),
            )
        )
        context.commands.register(
            Command(
                "sandbox.status",
                "Sandbox: Show Status",
                lambda ctx: self._show_sandbox_status(ctx),
            )
        )

    def _enable_sandbox(self, context):
        """Enable Docker sandboxing."""
        sandbox = enable_docker_sandbox()
        if sandbox.is_docker_available():
            context.write_console_line("[Sandbox] Docker isolation ENABLED", "success")
            context.write_console_line(f"  Memory limit: {sandbox.config.memory_limit}", "info")
            context.write_console_line(f"  CPU limit: {sandbox.config.cpu_limit} cores", "info")
            context.write_console_line(f"  Network: {'enabled' if sandbox.config.network_enabled else 'disabled'}", "info")
        else:
            context.write_console_line("[Sandbox] Docker not available - install Docker Desktop", "error")
            disable_sandbox()

    def _disable_sandbox(self, context):
        """Disable sandboxing."""
        disable_sandbox()
        context.write_console_line("[Sandbox] Docker isolation DISABLED - running directly on host", "warning")

    def _show_sandbox_status(self, context):
        """Show current sandbox status."""
        sandbox = get_sandbox()
        mode = sandbox.config.mode.value
        context.write_console_line(f"[Sandbox] Mode: {mode.upper()}", "info")
        if sandbox.config.mode == SandboxMode.DOCKER:
            available = "YES" if sandbox.is_docker_available() else "NO"
            context.write_console_line(f"  Docker available: {available}", "info")
            context.write_console_line(f"  Memory limit: {sandbox.config.memory_limit}", "info")
            context.write_console_line(f"  CPU limit: {sandbox.config.cpu_limit} cores", "info")
            context.write_console_line(f"  Network: {'enabled' if sandbox.config.network_enabled else 'disabled'}", "info")
        else:
            context.write_console_line("  Code runs directly on host (no isolation)", "warning")

    def _check_permission(self, context):
        """Check if code execution is permitted for this context."""
        # Check kernel context if available
        if hasattr(context, 'kernel_ctx') and context.kernel_ctx:
            perms = context.kernel_ctx.check_provider(self.name)
            if not perms.get('can_invoke', False):
                return False, "Code execution not permitted by document manifest"

        return True, None

    def _run_code(self, context):
        """Run the entire document as code."""
        allowed, msg = self._check_permission(context)
        if not allowed:
            context.present_text("Permission Denied", msg, allow_insert=False)
            return False

        content = context.document.content
        lang = self._detect_language(context)

        if not lang:
            context.present_text(
                "Run Code",
                "Could not detect language. Supported: Python, JavaScript/Node",
                allow_insert=False
            )
            return False

        return self._execute(context, content, lang)

    def _run_selection(self, context):
        """Run selected code."""
        allowed, msg = self._check_permission(context)
        if not allowed:
            context.present_text("Permission Denied", msg, allow_insert=False)
            return False

        selection = context.get_selection()
        if not selection:
            context.present_text("Run Selection", "No code selected", allow_insert=False)
            return False

        lang = self._detect_language(context)
        if not lang:
            # Try to guess from selection
            if "def " in selection or "import " in selection:
                lang = "python"
            elif "function " in selection or "const " in selection or "let " in selection:
                lang = "node"
            else:
                lang = "python"  # Default

        return self._execute(context, selection, lang)

    def _detect_language(self, context):
        """Detect language from file extension or content."""
        if context.document.file_path:
            ext = context.document.file_path.split(".")[-1].lower()
            if ext in ("py", "pyw"):
                return "python"
            elif ext in ("js", "mjs", "cjs"):
                return "node"

        # Check content for hints
        content = context.document.content[:500]
        if "#!/usr/bin/env python" in content or "import " in content:
            return "python"
        if "#!/usr/bin/env node" in content or "require(" in content:
            return "node"

        return None

    def _execute(self, context, code, lang):
        """Execute code and show output in console (sandboxed)."""
        from ide.sandbox import get_sandbox
        import os

        lang_info = self.SUPPORTED_LANGUAGES.get(lang)
        if not lang_info:
            context.write_console_line(f"[Error] Unsupported language: {lang}", "error")
            return False

        # Get working directory
        workdir = None
        if context.document.file_path:
            workdir = os.path.dirname(context.document.file_path)

        # Execute through sandbox
        sandbox = get_sandbox()
        context.write_console_command(f"{lang_info['cmd']} <code> [{sandbox.config.mode.value}]")

        result = sandbox.execute(code, lang, workdir)

        # Show execution mode indicator
        if result.execution_mode == "docker":
            context.write_console_line("[Running in Docker sandbox]", "info")

        if result.error:
            context.write_console_line(f"[Error] {result.error}", "error")
            return False

        if result.stdout:
            context.write_console(result.stdout, "info")
        if result.stderr:
            context.write_console(result.stderr, "error")
        if result.exit_code != 0:
            context.write_console_line(f"[Exit code: {result.exit_code}]", "warning")
        elif not result.stdout and not result.stderr:
            context.write_console_line("[No output]", "info")
        else:
            context.write_console_line("[Done]", "success")

        return result.success


class BuildProvider(Provider):
    """Provider for running build commands.

    Detects project type and runs appropriate build commands.
    """

    BUILD_CONFIGS = {
        "package.json": {"name": "npm", "commands": ["npm install", "npm run build", "npm test"]},
        "Cargo.toml": {"name": "cargo", "commands": ["cargo build", "cargo run", "cargo test"]},
        "Makefile": {"name": "make", "commands": ["make", "make clean", "make test"]},
        "pyproject.toml": {"name": "python", "commands": ["pip install -e .", "pytest", "python -m build"]},
        "setup.py": {"name": "python", "commands": ["pip install -e .", "pytest"]},
        "go.mod": {"name": "go", "commands": ["go build", "go run .", "go test"]},
    }

    def __init__(self):
        super().__init__("build_runner", ProviderCategory.ON_DEMAND)

    def activate(self, context):
        context.commands.register(
            Command(
                "build.run",
                "Compiler: Build Project",
                lambda ctx: self._run_build(ctx),
            )
        )
        context.commands.register(
            Command(
                "build.command",
                "Compiler: Custom Build Command",
                lambda ctx: self._run_custom(ctx),
            )
        )

    def _detect_project(self, context):
        """Detect project type from workspace files."""
        import os
        from pathlib import Path

        if not context.document.file_path:
            return None, None

        work_dir = Path(context.document.file_path).parent

        # Walk up to find project root
        for _ in range(5):  # Max 5 levels up
            for config_file, config in self.BUILD_CONFIGS.items():
                if (work_dir / config_file).exists():
                    return work_dir, config
            parent = work_dir.parent
            if parent == work_dir:
                break
            work_dir = parent

        return None, None

    def _run_build(self, context):
        """Run detected build system."""
        work_dir, config = self._detect_project(context)

        if not config:
            context.write_console_line("[Build] No recognized build system found.", "warning")
            context.write_console_line("Supported: npm, cargo, make, python (pyproject.toml), go", "info")
            return False

        # Let user choose which command
        choice = context.choose_option(
            f"Build ({config['name']})",
            f"Select command to run in {work_dir}:",
            config['commands']
        )

        if not choice:
            return False

        return self._execute_command(context, choice, work_dir)

    def _run_custom(self, context):
        """Run a custom build command."""
        from pathlib import Path

        if not context.document.file_path:
            context.write_console_line("[Build] Save file first to set working directory", "warning")
            return False

        work_dir = Path(context.document.file_path).parent

        # Common commands to suggest
        commands = ["npm run", "python -m pytest", "make", "cargo build", "go build"]

        choice = context.choose_option(
            "Run Command",
            f"Select or enter command (runs in {work_dir}):",
            commands
        )

        if not choice:
            return False

        return self._execute_command(context, choice, work_dir)

    def _execute_command(self, context, command, work_dir):
        """Execute a shell command and output to console."""
        import subprocess
        import os

        try:
            context.write_console_command(command)

            # Use shell=True for commands with arguments
            result = subprocess.run(
                command,
                shell=True,
                capture_output=True,
                text=True,
                timeout=120,  # 2 minute timeout for builds
                cwd=str(work_dir),
                env={**os.environ, "PYTHONUNBUFFERED": "1"}
            )

            if result.stdout:
                context.write_console(result.stdout, "info")
            if result.stderr:
                context.write_console(result.stderr, "error")

            if result.returncode == 0:
                context.write_console_line("[Build complete]", "success")
            else:
                context.write_console_line(f"[Exit code: {result.returncode}]", "warning")

            return result.returncode == 0

        except subprocess.TimeoutExpired:
            context.write_console_line("[Error] Command timed out (2 min limit)", "error")
            return False
        except Exception as e:
            context.write_console_line(f"[Error] {e}", "error")
            return False


class ShellProvider(Provider):
    """Provider for running shell commands with output capture."""

    def __init__(self):
        super().__init__("shell_runner", ProviderCategory.ON_DEMAND)

    def activate(self, context):
        context.commands.register(
            Command(
                "shell.run",
                "Compiler: Shell Command",
                lambda ctx: self._run_shell(ctx),
            )
        )

    def _run_shell(self, context):
        """Run a shell command and show output."""
        from pathlib import Path
        import subprocess
        import os

        work_dir = "."
        if context.document.file_path:
            work_dir = str(Path(context.document.file_path).parent)

        # Get command from user
        command = context.choose_option(
            "Shell Command",
            f"Enter command (runs in {work_dir}):",
            ["ls -la", "pwd", "git status", "git log --oneline -10"]
        )

        if not command:
            return False

        try:
            context.write_console_command(command)

            result = subprocess.run(
                command,
                shell=True,
                capture_output=True,
                text=True,
                timeout=30,
                cwd=work_dir
            )

            if result.stdout:
                context.write_console(result.stdout, "info")
            if result.stderr:
                context.write_console(result.stderr, "error")

            if result.returncode == 0:
                context.write_console_line("[Done]", "success")
            else:
                context.write_console_line(f"[Exit code: {result.returncode}]", "warning")

            return True

        except subprocess.TimeoutExpired:
            context.write_console_line("[Error] Command timed out", "error")
            return False
        except Exception as e:
            context.write_console_line(f"[Error] {e}", "error")
            return False


class HistoryProvider(Provider):
    """History Provider with semantic compression and vector search.

    Tracks document interactions and compresses them into semantic summaries
    for efficient AI context. Instead of storing raw edit history, stores
    compressed "what happened" summaries.

    Features:
    - Tracks: edits, commands, AI interactions, file operations
    - Compresses old events into semantic summaries
    - Provides compressed history for AI context windows
    - Semantic search with vector embeddings (optional)
    - Persistent storage with SQLite or MySQL

    Category: ALWAYS_ON (silent tracking) + ON_DEMAND (commands)
    """

    # Events older than this get compressed
    COMPRESSION_THRESHOLD_EVENTS = 50
    # Maximum compressed summaries to keep
    MAX_COMPRESSED_SUMMARIES = 100

    def __init__(self, ai_client=None, enable_vectors: bool = True, vector_db_path: str = None):
        super().__init__("history_provider", ProviderCategory.ALWAYS_ON)
        self.ai_client = ai_client
        self._enable_vectors = enable_vectors
        self._vector_db_path = vector_db_path

        # Recent raw events (kept for undo/detailed view)
        self._raw_events = []

        # Compressed semantic summaries
        self._compressed_history = []

        # Vector store (lazy loaded)
        self._vector_store = None
        self._vectors_available = None

        # Session metadata
        self._session_start = None
        self._session_id = None
        self._document_path = None
        self._edit_count = 0
        self._command_count = 0

    def _get_vector_store(self):
        """Lazy load vector store."""
        if not self._enable_vectors:
            return None

        if self._vectors_available is False:
            return None

        if self._vector_store is None:
            try:
                from ide.vector_store import SQLiteVectorStore
                import os

                # Default path in user's app data or project directory
                db_path = self._vector_db_path
                if not db_path:
                    db_path = os.path.join(
                        os.path.dirname(os.path.abspath(__file__)),
                        "..", "history_vectors.db"
                    )

                self._vector_store = SQLiteVectorStore(db_path)
                self._vectors_available = True
                print(f"[History] Vector store initialized: {db_path}")

            except Exception as e:
                print(f"[History] Vector store unavailable: {e}")
                self._vectors_available = False
                return None

        return self._vector_store

    def activate(self, context):
        """Start tracking history for this context."""
        import time
        import uuid

        self._session_start = time.time()
        self._session_id = str(uuid.uuid4())[:8]
        self._document_path = context.document.file_path

        # Track document changes
        context.events.document_changed.connect(self._on_document_changed)

        # Register history commands
        context.commands.register(
            Command(
                "history.view",
                "History: View Session History",
                lambda ctx: self._view_history(ctx),
            )
        )
        context.commands.register(
            Command(
                "history.summary",
                "History: Get Compressed Summary",
                lambda ctx: self._get_summary(ctx),
            )
        )
        context.commands.register(
            Command(
                "history.context",
                "History: AI Context Export",
                lambda ctx: self._export_context(ctx),
            )
        )
        context.commands.register(
            Command(
                "history.clear",
                "History: Clear Session History",
                lambda ctx: self._clear_history(ctx),
            )
        )
        context.commands.register(
            Command(
                "history.search",
                "History: Semantic Search",
                lambda ctx: self._semantic_search(ctx),
            )
        )
        context.commands.register(
            Command(
                "history.vectors.status",
                "History: Vector Store Status",
                lambda ctx: self._vector_status(ctx),
            )
        )

    def _on_document_changed(self, document):
        """Track document edits."""
        import time
        self._edit_count += 1

        event = {
            "type": "edit",
            "timestamp": time.time(),
            "chars_changed": len(document.content),
            "edit_number": self._edit_count,
        }
        self._add_event(event)

    def record_command(self, command_id, command_name):
        """Record a command execution (called by IDE context)."""
        import time
        self._command_count += 1

        event = {
            "type": "command",
            "timestamp": time.time(),
            "command_id": command_id,
            "command_name": command_name,
        }
        self._add_event(event)

    def record_ai_interaction(self, provider_name, prompt_summary, response_summary):
        """Record an AI interaction with semantic compression."""
        import time

        event = {
            "type": "ai_interaction",
            "timestamp": time.time(),
            "provider": provider_name,
            "prompt": prompt_summary[:200],  # Truncate for storage
            "response": response_summary[:200],
        }
        self._add_event(event)

    def record_file_operation(self, operation, path):
        """Record file operations (save, open, export)."""
        import time

        event = {
            "type": "file_operation",
            "timestamp": time.time(),
            "operation": operation,  # "save", "open", "export"
            "path": path,
        }
        self._add_event(event)

    def _add_event(self, event):
        """Add event and trigger compression if needed."""
        self._raw_events.append(event)

        # Compress if threshold reached
        if len(self._raw_events) > self.COMPRESSION_THRESHOLD_EVENTS:
            self._compress_events()

    def _compress_events(self):
        """Compress old events into semantic summaries."""
        import time

        if len(self._raw_events) < 20:
            return

        # Take oldest 30 events for compression
        to_compress = self._raw_events[:30]
        self._raw_events = self._raw_events[30:]

        # Build semantic summary
        summary = self._build_summary(to_compress)
        timestamp = time.time()
        block_id = f"hist_{self._session_id}_{int(timestamp)}"

        compressed_block = {
            "id": block_id,
            "timestamp": timestamp,
            "start_time": to_compress[0]["timestamp"],
            "end_time": to_compress[-1]["timestamp"],
            "event_count": len(to_compress),
            "summary": summary,
        }
        self._compressed_history.append(compressed_block)

        # Store in vector DB for semantic search
        store = self._get_vector_store()
        if store:
            try:
                store.add(
                    id=block_id,
                    text=summary,
                    metadata={
                        "session_id": self._session_id,
                        "document": self._document_path,
                        "event_count": len(to_compress),
                        "start_time": to_compress[0]["timestamp"],
                        "end_time": to_compress[-1]["timestamp"],
                    },
                    collection="history"
                )
            except Exception as e:
                print(f"[History] Failed to store in vector DB: {e}")

        # Limit compressed history size
        if len(self._compressed_history) > self.MAX_COMPRESSED_SUMMARIES:
            self._compressed_history = self._compressed_history[-self.MAX_COMPRESSED_SUMMARIES:]

    def _build_summary(self, events):
        """Build a semantic summary from raw events."""
        # Count event types
        edit_count = sum(1 for e in events if e["type"] == "edit")
        command_events = [e for e in events if e["type"] == "command"]
        ai_events = [e for e in events if e["type"] == "ai_interaction"]
        file_events = [e for e in events if e["type"] == "file_operation"]

        parts = []

        if edit_count > 0:
            parts.append(f"{edit_count} edits")

        if command_events:
            cmd_names = list(set(e["command_name"] for e in command_events))
            if len(cmd_names) <= 3:
                parts.append(f"Commands: {', '.join(cmd_names)}")
            else:
                parts.append(f"{len(command_events)} commands ({len(cmd_names)} unique)")

        if ai_events:
            providers = list(set(e["provider"] for e in ai_events))
            parts.append(f"AI interactions: {', '.join(providers)}")

        if file_events:
            ops = list(set(e["operation"] for e in file_events))
            parts.append(f"File ops: {', '.join(ops)}")

        return "; ".join(parts) if parts else "Session activity"

    def _view_history(self, context):
        """Show session history in console."""
        import time

        context.write_console_line("=== Session History ===", "command")

        # Session info
        if self._session_start:
            duration = time.time() - self._session_start
            mins = int(duration // 60)
            secs = int(duration % 60)
            context.write_console_line(f"Session duration: {mins}m {secs}s", "info")

        context.write_console_line(f"Total edits: {self._edit_count}", "info")
        context.write_console_line(f"Commands executed: {self._command_count}", "info")

        # Compressed history
        if self._compressed_history:
            context.write_console_line(f"\nCompressed blocks: {len(self._compressed_history)}", "info")
            for i, block in enumerate(self._compressed_history[-5:], 1):  # Last 5
                context.write_console_line(f"  [{i}] {block['summary']}", "info")

        # Recent raw events
        context.write_console_line(f"\nRecent events: {len(self._raw_events)}", "info")
        for event in self._raw_events[-10:]:  # Last 10
            event_type = event["type"]
            if event_type == "edit":
                context.write_console_line(f"  [edit] Edit #{event['edit_number']}", "info")
            elif event_type == "command":
                context.write_console_line(f"  [cmd] {event['command_name']}", "info")
            elif event_type == "ai_interaction":
                context.write_console_line(f"  [ai] {event['provider']}", "info")
            elif event_type == "file_operation":
                context.write_console_line(f"  [file] {event['operation']}", "info")

        return True

    def _get_summary(self, context):
        """Get a full compressed summary of the session."""
        import time

        parts = []

        # Session header
        if self._document_path:
            parts.append(f"Document: {self._document_path}")

        if self._session_start:
            duration = time.time() - self._session_start
            parts.append(f"Session: {int(duration // 60)}m")

        parts.append(f"Edits: {self._edit_count}, Commands: {self._command_count}")

        # Compressed history summaries
        if self._compressed_history:
            parts.append("\nHistory:")
            for block in self._compressed_history:
                parts.append(f"  - {block['summary']}")

        # Recent activity summary
        if self._raw_events:
            recent_summary = self._build_summary(self._raw_events)
            parts.append(f"\nRecent: {recent_summary}")

        summary = "\n".join(parts)
        context.present_text("Session Summary", summary, allow_insert=True)
        return True

    def _export_context(self, context):
        """Export compressed history for AI context window."""
        # Build minimal context for AI
        context_lines = []

        # Document context
        if self._document_path:
            import os
            filename = os.path.basename(self._document_path)
            context_lines.append(f"[File: {filename}]")

        # Compressed history (most relevant)
        for block in self._compressed_history[-10:]:  # Last 10 blocks
            context_lines.append(f"[{block['summary']}]")

        # Recent activity
        if self._raw_events:
            recent = self._build_summary(self._raw_events[-20:])
            context_lines.append(f"[Recent: {recent}]")

        # This compressed format is efficient for AI context windows
        compressed = " ".join(context_lines)

        context.write_console_line("=== AI Context Export ===", "command")
        context.write_console_line(f"Compressed to {len(compressed)} chars", "info")
        context.write_console_line(compressed, "info")
        context.write_console_line("\n[Copied to clipboard]", "success")

        # Copy to clipboard if possible
        try:
            from PyQt6.QtWidgets import QApplication
            clipboard = QApplication.clipboard()
            clipboard.setText(compressed)
        except Exception:
            pass

        return True

    def _clear_history(self, context):
        """Clear session history."""
        self._raw_events = []
        self._compressed_history = []
        self._edit_count = 0
        self._command_count = 0
        context.write_console_line("[History] Session history cleared", "success")
        return True

    def get_context_for_ai(self, max_chars=500):
        """Get compressed history string for AI prompts.

        Returns a compact representation of session history
        suitable for including in AI context windows.
        """
        parts = []

        # Add compressed blocks (semantic summaries)
        for block in self._compressed_history[-5:]:
            parts.append(block['summary'])

        # Add recent activity
        if self._raw_events:
            recent = self._build_summary(self._raw_events[-10:])
            parts.append(f"Recent: {recent}")

        result = " | ".join(parts)

        # Truncate if needed
        if len(result) > max_chars:
            result = result[:max_chars-3] + "..."

        return result

    def _semantic_search(self, context):
        """Perform semantic search across history."""
        store = self._get_vector_store()

        if not store:
            context.write_console_line("[History] Vector store not available", "warning")
            context.write_console_line("Install sentence-transformers: pip install sentence-transformers", "info")
            return False

        # Get search query from user
        query = context.choose_option(
            "Semantic Search",
            "Enter search query (searches by meaning, not just keywords):",
            ["formatting changes", "code execution", "file operations", "AI interactions"]
        )

        if not query:
            return False

        context.write_console_line(f"=== Searching: \"{query}\" ===", "command")

        try:
            results = store.search(query, k=10, collection="history")

            if not results:
                context.write_console_line("No matching history found.", "info")
                return True

            context.write_console_line(f"Found {len(results)} results:\n", "success")

            for i, (doc_id, text, score, metadata) in enumerate(results, 1):
                score_pct = int(score * 100)
                context.write_console_line(f"[{i}] ({score_pct}% match) {text}", "info")

                if metadata.get("document"):
                    import os
                    doc_name = os.path.basename(metadata["document"])
                    context.write_console_line(f"    Document: {doc_name}", "info")

            return True

        except Exception as e:
            context.write_console_line(f"[Error] Search failed: {e}", "error")
            return False

    def _vector_status(self, context):
        """Show vector store status."""
        context.write_console_line("=== Vector Store Status ===", "command")

        store = self._get_vector_store()

        if not store:
            context.write_console_line("Status: NOT AVAILABLE", "warning")
            context.write_console_line("\nTo enable semantic search:", "info")
            context.write_console_line("  pip install sentence-transformers", "info")
            return True

        context.write_console_line("Status: ACTIVE", "success")

        try:
            # Get embedder info
            from ide.embeddings import get_embedder
            embedder = get_embedder()
            context.write_console_line(f"Embedding model: {embedder.model_name}", "info")
            context.write_console_line(f"Dimensions: {embedder.dimensions}D", "info")
            context.write_console_line(f"Semantic mode: {'YES' if embedder.is_semantic else 'NO (fallback)'}", "info")

            # Get store stats
            total_count = store.count()
            history_count = store.count(collection="history")
            context.write_console_line(f"\nStored vectors: {total_count} total, {history_count} history", "info")

            collections = store.list_collections()
            if collections:
                context.write_console_line(f"Collections: {', '.join(collections)}", "info")

        except Exception as e:
            context.write_console_line(f"[Error] Could not get status: {e}", "error")

        return True

    def semantic_recall(self, query: str, k: int = 5) -> list:
        """Recall relevant history for AI context.

        Use this to get semantically relevant history when
        building AI prompts.

        Args:
            query: What to search for
            k: Number of results

        Returns:
            List of (text, score, metadata) tuples
        """
        store = self._get_vector_store()
        if not store:
            return []

        try:
            results = store.search(query, k=k, collection="history")
            return [(text, score, meta) for _, text, score, meta in results]
        except Exception:
            return []


class PythonInterpreterProvider(Provider):
    """Interactive Python interpreter/REPL provider.

    Provides a persistent Python session that maintains state across
    executions. Variables and imports persist between runs.
    """

    def __init__(self):
        super().__init__("python_interpreter", ProviderCategory.ON_DEMAND)
        self._globals = {}
        self._locals = {}
        self._history = []

    def activate(self, context):
        context.commands.register(
            Command(
                "python.eval",
                "Python: Evaluate Selection",
                lambda ctx: self._eval_selection(ctx),
            )
        )
        context.commands.register(
            Command(
                "python.repl",
                "Python: Interactive REPL",
                lambda ctx: self._show_repl(ctx),
            )
        )
        context.commands.register(
            Command(
                "python.reset",
                "Python: Reset Interpreter",
                lambda ctx: self._reset(ctx),
            )
        )

    def _eval_selection(self, context):
        """Evaluate selected Python code."""
        code = context.get_selection()
        if not code:
            context.write_console_line("[Python] Select some code to evaluate", "warning")
            return False

        return self._execute(context, code)

    def _execute(self, context, code):
        """Execute Python code and output to console."""
        import sys
        import io
        import traceback

        # Show the code being executed
        code_lines = code.strip().split('\n')
        for i, line in enumerate(code_lines):
            prefix = ">>> " if i == 0 else "... "
            context.write_console_line(prefix + line, "command")

        # Capture stdout/stderr
        old_stdout = sys.stdout
        old_stderr = sys.stderr
        sys.stdout = io.StringIO()
        sys.stderr = io.StringIO()

        result_value = None
        error = None

        try:
            # Try to eval first (for expressions)
            try:
                result_value = eval(code, self._globals, self._locals)
            except SyntaxError:
                # Fall back to exec for statements
                exec(code, self._globals, self._locals)
        except Exception:
            error = traceback.format_exc()

        stdout_output = sys.stdout.getvalue()
        stderr_output = sys.stderr.getvalue()

        sys.stdout = old_stdout
        sys.stderr = old_stderr

        # Output to console
        if stdout_output:
            context.write_console(stdout_output, "info")

        if result_value is not None:
            context.write_console_line(repr(result_value), "success")

        if stderr_output:
            context.write_console(stderr_output, "warning")

        if error:
            context.write_console(error, "error")

        if not stdout_output and result_value is None and not stderr_output and not error:
            context.write_console_line("[No output]", "info")

        # Add to history
        self._history.append({"code": code, "output": stdout_output or repr(result_value) or error or ""})

        return True

    def _show_repl(self, context):
        """Show interpreter status in console."""
        context.write_console_line("=== Python Interpreter Status ===", "command")

        # Show current namespace
        user_vars = {k: type(v).__name__ for k, v in self._locals.items()
                     if not k.startswith('_')}
        if user_vars:
            context.write_console_line("Defined variables:", "info")
            for k, t in user_vars.items():
                context.write_console_line(f"  {k}: {t}", "info")
        else:
            context.write_console_line("No variables defined.", "info")

        context.write_console_line(f"History: {len(self._history)} entries", "info")
        context.write_console_line("Select code and use 'Python: Evaluate' to run.", "info")
        return True

    def _reset(self, context):
        """Reset the interpreter state."""
        self._globals = {}
        self._locals = {}
        self._history = []
        context.write_console_line("[Python] Interpreter reset. All variables cleared.", "success")
        return True
