# MarlOS Redesign: Research & Learning Focus

## Current Problems

1. **Provider architecture** — Over-engineered, hard to extend
2. **Plan Space** — Belongs in Brise, not core MarlOS
3. **AI is separate** — Chat panel is detached from where you write
4. **No predictive assistance** — Missing inline AI in text editors

## New Vision: AI-Everywhere Research Tool

**Core principle:** Every text input has AI. Your data predicts your writing.

## What's Being Removed

| Feature | Destination | Reason |
|---------|-------------|--------|
| Plan Space | Brise | Task/planning belongs in separate app |
| Provider architecture | Replaced | Too complex, not serving users |
| Widgets | Removed | Distraction from core research flow |
| Complex plugin system | Simplified | Direct integrations instead |

## New Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    MarlOS v2                            │
│              Research & Learning Environment            │
├─────────────────────────────────────────────────────────┤
│  Core Services (Rust)                                   │
│  ├─ Vector Engine (semantic search, embeddings)        │
│  ├─ AI Bridge (OpenAI, Anthropic, Local)               │
│  ├─ Document Parser (PDF, EPUB, MD)                    │
│  ├─ Session Memory (what you're working on)            │
│  └─ Prediction Engine (sentence completion)            │
├─────────────────────────────────────────────────────────┤
│  UI Layer (SolidJS)                                     │
│  ├─ Document Viewer (AI-assisted reading)              │
│  ├─ Markdown Editor (inline AI, autocomplete)          │
│  ├─ Research Hub (sources, citations)                  │
│  └─ Chat Interface (contextual, everywhere)            │
└─────────────────────────────────────────────────────────┘
```

## New Features

### 1. Inline AI Everywhere

**Where it appears:**
- Markdown editor: Type `/ai` or press `Ctrl+Shift+A` for inline suggestions
- PDF viewer: Select text → inline explanation appears
- Research notes: Auto-suggest connections to existing notes

**How it works:**
- Cursor-aware: AI knows where you are in document
- Contextual: Uses surrounding text + your knowledge base
- Unobtrusive: Ghost text you can accept or ignore

### 2. Semantic Autocomplete

**The Prediction Game feature:**
- As you type, ghost text appears completing your thought
- Based on: your writing style + your knowledge base + semantic context
- Tab to accept, Esc to dismiss, Ctrl+→ to accept word-by-word

**Data sources for prediction:**
- Your previous writing (similar style/context)
- Documents you've read (related concepts)
- Your research notes (connections)
- Citations you've collected (academic phrasing)

**Example:**
```
You type: "The building code requires"
Ghost text: "minimum egress widths for assembly occupancies to ensure occupant safety."
Source: Your previous notes on code compliance + your PDF on egress requirements
```

### 3. Cursor-Aware Context

**What the AI knows:**
- What you're currently reading (open document)
- What you've written in this session
- Your research topic (current goal/focus)
- Your writing history (style, common phrases)

**Not:**
- Generic AI responses
- Responses requiring manual context pasting

### 4. Research Hub v2

**Simplified, focused:**
- Add sources by URL, file, or paste
- Auto-extract key points (AI)
- Tag with meaning (not just labels)
- Cite while you write (inline autocomplete)
- Generate bibliography (one click)

**No:**
- Complex paper generation (leave for later)
- Fact-checking rabbit holes
- Overly complex citation management

## Technical Changes

### Remove Provider System

**Current:**
```rust
// Complex provider registration, lifecycle, etc.
impl Provider for MyProvider {
    fn on_document_change(&self, ...) { ... }
    fn on_activation(&self) { ... }
    // ...
}
```

**New: Direct integration**
```rust
// Simple service calls
let completion = ai.complete_sentence(&context, &cursor_pos).await?;
let embedding = vector.embed(&text).await?;
```

### Add Prediction Engine

```rust
pub struct PredictionEngine {
    vector_db: VectorDatabase,
    ai_client: AIClient,
    user_history: WritingHistory,
}

impl PredictionEngine {
    pub async fn predict(
        &self,
        current_text: &str,
        cursor_pos: usize,
    ) -> Option<Prediction> {
        // 1. Get semantic context from vector DB
        let context = self.vector_db.similar_contexts(current_text, 5).await?;
        
        // 2. Get user's writing patterns
        let style = self.user_history.similar_sentences(current_text, 3);
        
        // 3. Ask AI for completion
        let prompt = format!(
            "Context: {}\nUser's style: {}\nComplete this sentence: {}",
            context, style, current_text
        );
        
        self.ai_client.complete(&prompt).await
    }
}
```

### Simplified Frontend

**Remove:**
- Sidebar widgets
- Plan Space routes/components
- Complex provider settings panels
- Research Hub complexity

**Keep/Add:**
- Document viewer (simplified)
- Markdown editor (with inline AI)
- Research sidebar (sources list)
- Chat overlay (contextual, not panel)

## User Experience

### Reading Flow
1. Open PDF/EPUB
2. Select text → inline explanation appears
3. Ask questions in context
4. Highlight → auto-saves to notes with citation

### Writing Flow
1. Open or create Markdown
2. Start typing
3. Ghost text suggests completions (Tab to accept)
4. Type `/ask` followed by question for inline AI
5. Type `/cite` for citation suggestions
6. All notes auto-link to research sources

### Research Flow
1. Add source (URL, file, paste)
2. AI extracts key points
3. Tag with meaning (what is this about?)
4. While writing, related sources auto-suggest
5. Cite inline, generate bibliography

## Implementation Priority

### Phase 1: Strip & Simplify
- [ ] Remove Plan Space
- [ ] Remove Provider architecture
- [ ] Remove widgets
- [ ] Simplify Research Hub

### Phase 2: Inline AI Core
- [ ] Add ghost text component to editor
- [ ] Cursor tracking service
- [ ] Context gathering (what user is doing)
- [ ] Basic inline completions

### Phase 3: Semantic Prediction
- [ ] Writing history tracking
- [ ] Vector-based similar sentence finding
- [ ] Prediction engine integration
- [ ] Tab/Esc/Ctrl+→ interactions

### Phase 4: Polish
- [ ] Ghost text styling
- [ ] Settings for prediction aggressiveness
- [ ] Performance optimization
- [ ] Documentation

## Key Files to Modify

```
src/
├── components/
│   ├── MarkdownEditor.tsx       # Add ghost text, inline AI
│   ├── PdfViewer.tsx            # Inline explanations on select
│   └── GhostText.tsx            # NEW: Visual prediction layer
├── services/
│   ├── ai.ts                    # Simplified, no providers
│   ├── prediction.ts            # NEW: Prediction engine client
│   └── context.ts               # NEW: Cursor/context tracking
└── App.tsx                      # Remove Plan Space routes

src-tauri/src/
├── main.rs                      # Remove provider system
├── ai.rs                        # Direct AI calls
├── prediction.rs                # NEW: Prediction engine
├── vector.rs                    # Simplified embedding search
└── session.rs                   # Track user context
```

## Success Metrics

- User can write 3 sentences without touching mouse
- Prediction accepted >30% of the time
- AI suggestions feel "like me, but smarter"
- Research to writing flow <5 seconds

## Comparison: Old vs New

| Aspect | Old | New |
|--------|-----|-----|
| AI access | Separate panel | Inline everywhere |
| Context | Manual paste | Automatic, cursor-aware |
| Prediction | None | Semantic autocomplete |
| Architecture | Complex providers | Direct services |
| Focus | Do everything | Research & writing only |
| Planning | Built-in | Moved to Brise |

## Why This Works

**For you:**
- Simpler codebase
- Clearer purpose
- Less maintenance

**For users:**
- AI where they need it
- Less UI to learn
- Faster workflow

**For CANO:**
- MarlOS = research & learning
- Brise = planning & execution
- Clear separation, both excellent
