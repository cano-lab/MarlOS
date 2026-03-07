# MarlOS

**Memory Augmented Research & Learning Operating System**

A desktop application for researchers, writers, and knowledge workers who want to manage documents, track research, and work with AI—while keeping everything local and private.

## What It Does

MarlOS brings together your reading, writing, and AI conversations in one place:

- **Read** PDFs, EPUBs, and Markdown with AI assistance
- **Research** Collect sources, generate citations, write papers
- **Search** Find anything by meaning, not just keywords
- **Think** Debug your reasoning with AI conversation analysis
- **Plan** Manage goals and projects with milestone tracking

All data stays on your computer. No cloud required.

## Quick Start

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Features

### Document Viewer
Read PDFs, EPUBs, and Markdown files with AI assistance. Ask questions about documents, extract citations automatically.

### Research Hub
Collect and organize sources. Generate citations in APA, MLA, Chicago, and more. Create structured papers from your research.

### Semantic Search
Search across all your content by meaning. "What was I reading about climate last week?" finds relevant documents without exact keyword matches.

### Thinking Debugger
Analyze your AI conversations to identify reasoning errors, wrong facts, and missed questions. Learn to think better, not just get answers.

### Plan Space
Set goals with milestones, track progress, and manage your projects. Export context to AI coding assistants like Claude Code.

## Architecture

**Frontend:** SolidJS + TypeScript  
**Backend:** Rust (Tauri)  
**Storage:** Local SQLite and JSON  
**AI:** Local (LM Studio, Ollama) or cloud (OpenAI, Anthropic)

```
┌─────────────────┐
│  SolidJS (UI)   │
├─────────────────┤
│  Tauri Bridge   │
├─────────────────┤
│  Rust Backend   │
├─────────────────┤
│  Local Storage  │
└─────────────────┘
```

## Project Structure

```
MarlOS/
├── src/              # Frontend (SolidJS)
│   ├── components/   # UI components
│   └── ...
├── src-tauri/        # Backend (Rust)
│   └── src/
├── docs/             # Documentation
└── ...
```

## Philosophy

- **Privacy first:** Your data never leaves your machine
- **Local AI:** No API costs, works offline, complete control
- **Knowledge compounds:** The more you use it, the more valuable it becomes
- **Open source:** Transparent and extensible

## Status

Core features are functional. Active development.

## License

[Your license here]
