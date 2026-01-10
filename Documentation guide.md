# Documentation Guide

## Available Documentation

The following documentation files explain the Document IDE codebase:

| Document | Purpose |
|----------|---------|
| [ARCHITECTURE_REFERENCE.md](ARCHITECTURE_REFERENCE.md) | Complete architecture overview, component explanations, data flow diagrams |
| [REGEX_REFERENCE.md](REGEX_REFERENCE.md) | All regex patterns with breakdowns and explanations |
| [QUICK_REFERENCE.md](QUICK_REFERENCE.md) | One-page cheatsheet for quick lookups |
| [Document IDE Architecture v0.txt](Document%20IDE%20Architecture%20v0.txt) | Original architecture specification (normative) |
| [PROVIDER_AUTHORING_GUIDE.md](PROVIDER_AUTHORING_GUIDE.md) | How to create new providers |
| [TASK_REMINDERS_AGENT_SPEC.md](TASK_REMINDERS_AGENT_SPEC.md) | Task and reminder system specification |

---

## Quick Answers

### "How does the architecture work?"
See [ARCHITECTURE_REFERENCE.md](ARCHITECTURE_REFERENCE.md) - covers the Events Spine, Command Registry, Provider system, and data flow.

### "What do these regex patterns mean?"
See [REGEX_REFERENCE.md](REGEX_REFERENCE.md) - every pattern broken down with explanations.

### "I need a quick lookup"
See [QUICK_REFERENCE.md](QUICK_REFERENCE.md) - one-page cheatsheet with commands, shortcuts, and common patterns.

### "How do I add a new feature?"
See [PROVIDER_AUTHORING_GUIDE.md](PROVIDER_AUTHORING_GUIDE.md) - templates and rules for creating providers.

---

## Documentation Guidelines

### When adding new code:

1. **Add docstrings** to classes and public methods
2. **Explain complex regex** with inline comments
3. **Update ARCHITECTURE_REFERENCE.md** if adding new components
4. **Update QUICK_REFERENCE.md** if adding new commands

### Docstring format:

```python
def extract(self, content, source_path):
    """Extract tasks from markdown content.

    Args:
        content: Raw markdown text
        source_path: File path for task indexing

    Returns:
        List of task dicts with keys: text, status, source_path,
        source_line, source_heading_path, tags, due
    """
```

### Regex comment format:

```python
# Pattern: ^\s*[-*+]\s+\[( |x|X)\]\s+(.*)
# Matches: - [ ] Task text  or  - [x] Done task
# Group 1: checkbox state (space = open, x = done)
# Group 2: task text
self.checkbox_pattern = re.compile(r"^\s*[-*+]\s+\[( |x|X)\]\s+(.*)")
```

---

## Things to Address

- [ ] Add inline code comments to core functions in viewer.py
- [ ] Document the print/export system
- [ ] Add examples for common customizations

