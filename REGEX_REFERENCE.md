# Document IDE - Regex Pattern Reference

> A comprehensive guide to all regular expressions used in the codebase with explanations.

---

## Table of Contents

1. [Syntax Highlighting Patterns](#syntax-highlighting-patterns)
2. [Task Extraction Patterns](#task-extraction-patterns)
3. [Markdown Language Service Patterns](#markdown-language-service-patterns)
4. [AI/Analysis Patterns](#aianalysis-patterns)
5. [Word Count Patterns](#word-count-patterns)
6. [Mermaid Diagram Patterns](#mermaid-diagram-patterns)
7. [Regex Quick Reference](#regex-quick-reference)

---

## Syntax Highlighting Patterns

**File:** `viewer.py` - `MarkdownHighlighter.highlightBlock()`

### Bold Text

```python
r'\*\*(.+?)\*\*|__(.+?)__'
```

**Breakdown:**
```
\*\*      - Literal **
(.+?)     - Capture 1+ chars (non-greedy) - the bold text
\*\*      - Literal **
|         - OR
__        - Literal __
(.+?)     - Capture 1+ chars (non-greedy)
__        - Literal __
```

**Matches:**
- `**bold text**`
- `__also bold__`

**Non-greedy (`?`):** Without it, `**a** and **b**` would match the entire string instead of two separate matches.

---

### Italic Text

```python
r'(?<!\*)\*(?!\*)(.+?)(?<!\*)\*(?!\*)|(?<!_)_(?!_)(.+?)(?<!_)_(?!_)'
```

**Breakdown:**
```
(?<!\*)   - Negative lookbehind: not preceded by *
\*        - Literal *
(?!\*)    - Negative lookahead: not followed by *
(.+?)     - Capture 1+ chars (non-greedy)
(?<!\*)   - Not preceded by *
\*        - Literal *
(?!\*)    - Not followed by *
|         - OR
[same pattern with _ instead of *]
```

**Purpose:** Matches single `*text*` or `_text_` but NOT `**bold**` or `__bold__`.

**Matches:**
- `*italic*`
- `_also italic_`

**Does NOT match:**
- `**bold**` (double asterisks)
- `__bold__` (double underscores)

---

### Inline Code

```python
r'`[^`]+`'
```

**Breakdown:**
```
`         - Literal backtick
[^`]+     - One or more characters that are NOT backticks
`         - Literal backtick
```

**Matches:**
- `` `code` ``
- `` `any text here` ``

---

### Links

```python
r'\[.+?\]\(.+?\)'
```

**Breakdown:**
```
\[        - Literal [
.+?       - One or more chars (non-greedy) - link text
\]        - Literal ]
\(        - Literal (
.+?       - One or more chars (non-greedy) - URL
\)        - Literal )
```

**Matches:**
- `[link](https://example.com)`
- `[text](url)`

---

### List Items

```python
r'^(\s*[-*+]|\s*\d+\.)\s'
```

**Breakdown:**
```
^         - Start of line
(         - Start capture group
  \s*     - Zero or more whitespace (indentation)
  [-*+]   - One of: dash, asterisk, or plus (bullet markers)
|         - OR
  \s*     - Zero or more whitespace
  \d+     - One or more digits
  \.      - Literal dot
)         - End capture group
\s        - Followed by whitespace
```

**Matches:**
- `- item` (dash list)
- `* item` (asterisk list)
- `+ item` (plus list)
- `1. item` (numbered list)
- `  - nested` (indented)

---

## Task Extraction Patterns

**File:** `ide/tasks.py` - `TaskExtractor`

### Checkbox Tasks

```python
r"^\s*[-*+]\s+\[( |x|X)\]\s+(.*)"
```

**Breakdown:**
```
^         - Start of line
\s*       - Zero or more leading whitespace
[-*+]     - List marker (dash, asterisk, or plus)
\s+       - One or more whitespace
\[        - Literal [
( |x|X)   - Capture group 1: space, x, or X (checkbox state)
\]        - Literal ]
\s+       - One or more whitespace
(.*)      - Capture group 2: task text (rest of line)
```

**Matches:**
- `- [ ] Open task` → status="open", text="Open task"
- `- [x] Done task` → status="done", text="Done task"
- `  * [ ] Indented` → also matches with indentation

**Capture Groups:**
1. Checkbox state: ` ` (space) = open, `x`/`X` = done
2. Task text

---

### TODO Tasks

```python
r"^\s*TODO(?:\([^)]+\))?:\s+(.*)"
```

**Breakdown:**
```
^         - Start of line
\s*       - Zero or more leading whitespace
TODO      - Literal "TODO"
(?:       - Non-capturing group
  \(      - Literal (
  [^)]+   - One or more chars that aren't )
  \)      - Literal )
)?        - End group, optional (zero or one)
:         - Literal colon
\s+       - One or more whitespace
(.*)      - Capture group 1: task text
```

**Matches:**
- `TODO: Do something` → text="Do something"
- `TODO(important): Fix this` → text="Fix this"
- `  TODO: Indented` → also matches

**Non-capturing `(?:...)`:** The parenthesized label is matched but not captured as a group.

---

### Tags

```python
r"@([A-Za-z0-9_-]+)"
```

**Breakdown:**
```
@         - Literal @ symbol
(         - Start capture group
  [A-Za-z0-9_-]+  - One or more: letters, digits, underscore, or hyphen
)         - End capture group
```

**Matches:**
- `@home` → "home"
- `@clientA` → "clientA"
- `@my-tag` → "my-tag"
- `@tag_2` → "tag_2"

---

### Due Dates (Format 1)

```python
r"due:(\d{4}-\d{2}-\d{2})"
```

**Breakdown:**
```
due:      - Literal "due:"
(         - Start capture group
  \d{4}   - Exactly 4 digits (year)
  -       - Literal dash
  \d{2}   - Exactly 2 digits (month)
  -       - Literal dash
  \d{2}   - Exactly 2 digits (day)
)         - End capture group
```

**Matches:**
- `due:2026-01-15` → "2026-01-15"
- `due:2024-12-31` → "2024-12-31"

---

### Due Dates (Format 2)

```python
r"@due\((\d{4}-\d{2}-\d{2})\)"
```

**Breakdown:**
```
@due      - Literal "@due"
\(        - Literal (
(         - Start capture group
  \d{4}-\d{2}-\d{2}  - ISO date format
)         - End capture group
\)        - Literal )
```

**Matches:**
- `@due(2026-01-15)` → "2026-01-15"

---

### Heading Detection

```python
r"^(#{1,6})\s+(.*)"
```

**Breakdown:**
```
^         - Start of line
(#{1,6})  - Capture group 1: 1 to 6 hash characters
\s+       - One or more whitespace
(.*)      - Capture group 2: heading text
```

**Matches:**
- `# Title` → level=1, text="Title"
- `## Section` → level=2, text="Section"
- `###### Deep` → level=6, text="Deep"

**Quantifier `{1,6}`:** Matches minimum 1, maximum 6 occurrences.

---

## Markdown Language Service Patterns

**File:** `ide/providers.py` - `MarkdownLanguageService._parse()`

### Headings

```python
r"(#{1,6})\\s+(.*)"
```

*Same as above, detects heading level and text.*

---

### Links in Text

```python
r"\[([^\]]+)\]\(([^)]+)\)"
```

**Breakdown:**
```
\[        - Literal [
(         - Start capture group 1
  [^\]]+  - One or more chars that aren't ]
)         - End capture group 1 (link text)
\]        - Literal ]
\(        - Literal (
(         - Start capture group 2
  [^)]+   - One or more chars that aren't )
)         - End capture group 2 (URL)
\)        - Literal )
```

**Matches:**
- `[Google](https://google.com)` → ("Google", "https://google.com")

**Character Classes:**
- `[^\]]` means "any character except `]`"
- `[^)]` means "any character except `)`"

---

### Task Checkboxes (Language Service)

```python
r"\\s*[-*+]\\s+\\[( |x|X)\\]\\s+(.*)"
```

*Same pattern as TaskExtractor but using escaped backslashes for string literal.*

---

## AI/Analysis Patterns

**File:** `ide/ai.py` - `LocalAIClient`

### Citation Detection

```python
r"(https?://\\S+|doi:\\S+|"
r"\\[[0-9]{1,3}(?:\\s*[,;-]\\s*[0-9]{1,3})*\\]|"
r"\\((?:[A-Z][A-Za-z-]+(?:\\s+et\\s+al\\.)?[,;\\s]+\\d{4}[a-z]?)"
r"(?:\\s*[,;]\\s*[A-Z][A-Za-z-]+(?:\\s+et\\s+al\\.)?[,;\\s]+\\d{4}[a-z]?)*\\))"
```

**This complex pattern matches multiple citation formats:**

**Part 1: URLs**
```
https?://\\S+     - http:// or https:// followed by non-whitespace
```

**Part 2: DOI**
```
doi:\\S+          - "doi:" followed by non-whitespace
```

**Part 3: Numeric Citations**
```
\\[[0-9]{1,3}(?:\\s*[,;-]\\s*[0-9]{1,3})*\\]
```
Matches: `[1]`, `[1, 2]`, `[1-5]`, `[1, 3, 5]`

**Part 4: Author-Year Citations**
```
\\((?:[A-Z][A-Za-z-]+(?:\\s+et\\s+al\\.)?[,;\\s]+\\d{4}[a-z]?)...\\)
```
Matches: `(Smith, 2020)`, `(Smith et al., 2020)`, `(Smith, 2020a)`

---

### Claim Detection

```python
r"(shows|demonstrates|indicates|suggests|evidence|significant|"
r"statistically|improves|reduces|increases|decreases|outperforms|"
r"causes|correlates|leads\\s+to|results\\s+in|we\\s+find|we\\s+show)"
```

**Purpose:** Find sentences with scientific claim language.

**Matches words like:**
- "shows", "demonstrates", "indicates"
- "significant", "statistically"
- Multi-word: "leads to", "results in", "we find"

---

### Numeric Claims

```python
r"(\\d+\\.\\d+|\\d+%|\\b\\d+\\s*(ms|s|sec|x|%|times)\\b)"
```

**Breakdown:**
```
\\d+\\.\\d+       - Decimal numbers (e.g., "3.14")
|
\\d+%             - Percentage (e.g., "50%")
|
\\b\\d+\\s*(ms|s|sec|x|%|times)\\b
                  - Numbers with units: "100ms", "5x", "10 times"
```

---

### Action Item Detection

```python
r"\\b(todo|action|next step|follow[- ]?up)\\b"
```

**Matches:**
- "todo", "TODO"
- "action"
- "next step"
- "follow-up", "follow up", "followup"

---

### Typo Dictionary Pattern

```python
r"\\b(" + "|".join(re.escape(word) for word in misspellings) + r")\\b"
```

**Dynamic pattern** built from dictionary keys:
- `re.escape()` ensures special chars are literal
- `\\b` word boundaries prevent partial matches
- `re.I` flag makes it case-insensitive

Example generated: `\\b(teh|recieve|occured|...)\\b`

---

## Word Count Patterns

**File:** `ide/providers.py` - `WordCountProvider._update()`

### Primary Word Detection

```python
r"[A-Za-z0-9]+(?:['-][A-Za-z0-9]+)?"
```

**Breakdown:**
```
[A-Za-z0-9]+      - One or more alphanumeric chars (base word)
(?:               - Non-capturing group
  ['-]            - Apostrophe or hyphen
  [A-Za-z0-9]+    - Followed by more alphanumeric
)?                - Zero or one (optional)
```

**Matches:**
- "hello" (simple word)
- "don't" (contraction)
- "well-known" (hyphenated)
- "123" (numbers)

---

### Fallback Word Detection

```python
r"\\s+"
```

**Simple whitespace split** used when primary pattern yields no results.

---

## Mermaid Diagram Patterns

**File:** `ide/providers.py` - `MarkdownRenderer.render_html()`

### Mermaid Block Extraction

```python
r"```mermaid\\s*([\\s\\S]*?)```"
```

**Breakdown:**
```
```mermaid       - Literal fence start
\\s*             - Optional whitespace
(                - Start capture group
  [\\s\\S]*?     - Any character including newlines (non-greedy)
)                - End capture group
```              - Literal fence end
```

**Key Insight:** `[\\s\\S]` matches ANY character:
- `\\s` = whitespace (including newlines)
- `\\S` = non-whitespace
- Together = everything

The standard `.` does NOT match newlines by default.

---

## Regex Quick Reference

### Metacharacters

| Char | Meaning |
|------|---------|
| `.` | Any char except newline |
| `^` | Start of line |
| `$` | End of line |
| `*` | Zero or more |
| `+` | One or more |
| `?` | Zero or one |
| `\|` | OR |
| `()` | Capture group |
| `[]` | Character class |
| `{}` | Quantifier |

### Character Classes

| Pattern | Meaning |
|---------|---------|
| `\d` | Digit [0-9] |
| `\D` | Non-digit |
| `\w` | Word char [A-Za-z0-9_] |
| `\W` | Non-word char |
| `\s` | Whitespace |
| `\S` | Non-whitespace |
| `[abc]` | One of a, b, c |
| `[^abc]` | Not a, b, or c |
| `[a-z]` | Range a to z |

### Quantifiers

| Pattern | Meaning |
|---------|---------|
| `*` | 0 or more (greedy) |
| `+` | 1 or more (greedy) |
| `?` | 0 or 1 |
| `{n}` | Exactly n |
| `{n,}` | n or more |
| `{n,m}` | Between n and m |
| `*?` | 0 or more (non-greedy) |
| `+?` | 1 or more (non-greedy) |

### Anchors & Lookarounds

| Pattern | Meaning |
|---------|---------|
| `^` | Start of string/line |
| `$` | End of string/line |
| `\b` | Word boundary |
| `\B` | Not word boundary |
| `(?=...)` | Positive lookahead |
| `(?!...)` | Negative lookahead |
| `(?<=...)` | Positive lookbehind |
| `(?<!...)` | Negative lookbehind |

### Groups

| Pattern | Meaning |
|---------|---------|
| `(...)` | Capture group |
| `(?:...)` | Non-capturing group |
| `(?P<name>...)` | Named group (Python) |

### Flags (Python)

| Flag | Meaning |
|------|---------|
| `re.I` | Case insensitive |
| `re.M` | Multiline (^ and $ match line boundaries) |
| `re.S` | Dotall (. matches newlines) |
| `re.X` | Verbose (allow comments) |

---

## Testing Regex Patterns

Python REPL example:

```python
import re

# Test checkbox pattern
pattern = re.compile(r"^\s*[-*+]\s+\[( |x|X)\]\s+(.*)")
test = "- [ ] Buy groceries @home"
match = pattern.match(test)
if match:
    print(f"Status: {'done' if match.group(1).lower() == 'x' else 'open'}")
    print(f"Text: {match.group(2)}")
```

Output:
```
Status: open
Text: Buy groceries @home
```

---

## Common Gotchas

1. **Escaping in Python strings:**
   - Raw strings `r"..."` don't need `\\`
   - Regular strings need `\\` for `\`
   - `r"\d+"` equals `"\\d+"`

2. **Greedy vs Non-Greedy:**
   - `.*` matches as much as possible
   - `.*?` matches as little as possible
   - Use `?` for non-greedy when parsing markup

3. **Newlines:**
   - `.` does NOT match `\n` by default
   - Use `[\s\S]` or `re.DOTALL` flag

4. **Word Boundaries:**
   - `\b` matches between word and non-word chars
   - `\bword\b` won't match "swordfish"

5. **Character Class Order:**
   - Put `-` at start or end to match literal dash
   - `[-a-z]` or `[a-z-]` matches dash literally
   - `[a-z]` matches range a to z
