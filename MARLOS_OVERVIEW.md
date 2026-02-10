# MarlOS Overview

**MARLOS** = **M**emory **A**ugmented **R**esearch & **L**earning **O**perating **S**ystem

---

## The Problem

Knowledge workers today:
- Use multiple AI tools (ChatGPT, Claude, Cursor)
- Read documents, PDFs, ebooks
- Take notes scattered everywhere
- Can't find things when they need them

**Knowledge is fragmented across apps, accounts, and devices.**

---

## The Solution

MarlOS is a **personal knowledge companion** that:

| Capability | Description |
|------------|-------------|
| **Remembers** | Semantic memory of everything you work on |
| **Stays Private** | 100% local - no cloud, no subscriptions |
| **Connects** | Imports from ChatGPT, Claude Code, Cursor, Obsidian |
| **Thinks** | Works with local AI (LM Studio, Ollama) |
| **Teaches** | Thinking Debugger helps you reason better |

---

## Key Features

### Document Viewer
- PDF, EPUB, Markdown support
- AI-assisted reading with local LLMs
- Auto-extract citations

### Research Hub
- Collect and organize sources
- Tag-based organization
- Paper generation with citations

### Semantic Search
- Find by meaning, not keywords
- Search across all imported content
- "What was I researching last Tuesday?"

### Thinking Debugger
- Analyzes AI conversations
- Identifies reasoning errors
- Teaches *how* to think, not just answers

### Session Tracking
- Document time tracking
- Activity history
- Usage analytics

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│                   Frontend                       │
│              (SolidJS + TypeScript)              │
├─────────────────────────────────────────────────┤
│                   Tauri Bridge                   │
├─────────────────────────────────────────────────┤
│                  Rust Backend                    │
│  ┌───────────┐  ┌───────────┐  ┌─────────────┐  │
│  │ Providers │  │  Vector   │  │ AI Services │  │
│  │  System   │  │  Search   │  │  (Local)    │  │
│  └───────────┘  └───────────┘  └─────────────┘  │
├─────────────────────────────────────────────────┤
│              Local Storage (JSON)                │
│         ~/.local/share/marlos/                   │
└─────────────────────────────────────────────────┘
```

### Provider System
- **AlwaysOn**: WordCount, MarkdownLanguageService
- **OnDemand**: CodingAgent
- Extensible architecture for new capabilities

### AI Services
- LM Studio, Ollama (local)
- OpenAI, Anthropic (optional cloud)
- Configurable per-use-case

---

## Tech Stack

| Layer | Technology |
|-------|------------|
| Framework | Tauri 2.0 |
| Backend | Rust |
| Frontend | SolidJS + TypeScript |
| Styling | CSS |
| Vector DB | Local embeddings |
| AI | OpenAI-compatible API |

---

## Philosophy

> "AI should amplify your growth, not replace your thinking."

- **Privacy first** - Your data never leaves your machine
- **Local AI** - No API costs, works offline
- **Knowledge compounds** - The more you use it, the smarter it gets
- **Open source** - Transparent, auditable, extensible

---

## Target Users

- Students researching papers
- Researchers managing sources
- Writers organizing ideas
- Professionals with sensitive documents
- Anyone who wants to think more clearly with AI

---

## Current Status

- Desktop app (Windows, Mac, Linux via Tauri)
- Core features functional
- Active development

---

## Links

- Repository: `marlos-rust`
- See `WHAT_IS_MARLOS.md` for detailed documentation
