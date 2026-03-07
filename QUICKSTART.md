# MarlOS Quick Start

## Installation

1. Download the latest release for your platform (Windows, Mac, or Linux)
2. Install LM Studio or Ollama for local AI (optional but recommended)
3. Launch MarlOS

## First Steps

### 1. Configure AI Provider

Settings → AI Provider
- **Local (recommended):** Connect to LM Studio or Ollama
- **Cloud:** Add OpenAI or Anthropic API key

### 2. Open a Document

File → Open (or Ctrl+O)
- PDFs: Read with AI assistance
- EPUBs: Navigate chapters
- Markdown: Edit with preview

### 3. Ask Questions

Select text → Ctrl+/ (or sidebar Chat button)
Ask questions about the document using your configured AI.

## Key Features

### Research Hub (Ctrl+R)
Collect sources for papers:
1. Add sources manually or import
2. Tag and organize
3. Generate citations
4. Export paper outline

### Semantic Search (Ctrl+K)
Find anything by meaning:
- Type natural language queries
- Search across all imported content
- Results ranked by relevance

### Plan Space (Ctrl+G)
Manage projects:
- Create goals with milestones
- Track progress
- Export context to Claude Code

### Thinking Debugger
Analyze AI conversations:
- Import from ChatGPT, Claude, Cursor
- Identify reasoning errors
- Learn better questioning

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Ctrl+N | New document |
| Ctrl+O | Open file |
| Ctrl+S | Save |
| Ctrl+P | Print |
| Ctrl+/ | Toggle AI chat |
| Ctrl+R | Research Hub |
| Ctrl+K | Semantic search |
| Ctrl+G | Plan Space |
| Ctrl+\\ | Toggle sidebar |

## Data Location

Your data is stored locally:
- **Linux:** `~/.local/share/marlos/`
- **macOS:** `~/Library/Application Support/marlos/`
- **Windows:** `%APPDATA%\marlos\`

## Tips

- Import your ChatGPT/Claude conversations to build your knowledge base
- Use tags to organize research sources
- Track document time with the built-in timer
- Export goals to Markdown for AI coding assistants

## Troubleshooting

**AI not responding:** Check that LM Studio/Ollama is running and CORS is enabled.

**Search not finding results:** Rebuild index in Settings → Advanced.

**Import fails:** Check file format (supports .json exports from ChatGPT, .md, .txt).
