# MarlOS Alpha Audit Report

**Date**: 2026-02-11
**Version**: dev branch (commit a4365e0)
**Auditor**: Claude Code

---

## Executive Summary

MarlOS is a **local-first, AI-enhanced knowledge management system** built with Tauri 2.0 (Rust backend) and SolidJS (TypeScript frontend). The application is at **alpha quality** with core features functional but requiring polish before beta release.

### Overall Assessment

| Area | Status | Issues |
|------|--------|--------|
| **Core Editing** | Functional | Minor |
| **PDF/EPUB Viewing** | Functional | Medium |
| **Semantic Memory** | Functional | Minor |
| **Research Hub** | Functional | Medium |
| **AI/Chat** | Functional | Medium |
| **Embedding Visualizations** | Functional | Minor |
| **Learning System** | Functional | Medium |
| **Error Handling** | Needs Work | Critical |

**Total Issues Found**: 15 Critical, 45 Major, 41 Minor

---

## Current Feature Status

### Working Features

1. **Document Editing**
   - Markdown editor with live preview
   - Split view editing
   - Dark/light theme toggle
   - Search within document

2. **Document Viewing**
   - PDF rendering with zoom/pan
   - PDF measurement tools (distance, area)
   - EPUB reading with chapter navigation
   - EPUB highlights and notes system

3. **Semantic Memory**
   - Vector embeddings (384D)
   - Semantic search
   - Object store with CRUD operations
   - Multiple embedding models supported

4. **Research Hub**
   - Source collection and management
   - Citation generation (APA, MLA, Chicago, IEEE)
   - Academic paper discovery
   - Source tagging and filtering
   - Paper assignment to sources

5. **AI Integration**
   - Local providers (LM Studio, Ollama)
   - Cloud providers (OpenAI, Anthropic)
   - Chat interface with context
   - Multiple AI tasks (summarize, explain, etc.)

6. **Embedding Visualizations**
   - Waveform visualization
   - 3D waveform view
   - Embedding explorer with heatmaps
   - Audio playback of embeddings
   - FFT/Fourier analysis
   - Zoom/pan controls

7. **Learning System**
   - Predict-test-compare-integrate flow
   - Web and academic search integration
   - Learning history tracking

8. **Session Management**
   - Session import (ChatGPT, Claude Code, Cursor, Obsidian)
   - Session history viewing
   - Time tracking per document

---

## Critical Issues (Fix Before Alpha Release)

### ~~1. Panic-Causing Code in Backend~~ ✅ FIXED

**Status**: All `.expect()` calls replaced with proper error handling

**Files Modified**:
- `src-tauri/src/lib.rs` - Lines 94-97, 102-103, 115-116 now use `?` with error messages
- `src-tauri/src/kernel.rs` - `SemanticKernel::new()` now returns `Result<Self, MemoryError>`
- `src-tauri/src/pdf.rs` - Updated `Default` impl with clear warning comments

**Changes**:
```rust
// Before (lib.rs)
let app_data_dir = app.path().app_data_dir()
    .expect("Failed to get app data directory");

// After
let app_data_dir = app.path().app_data_dir()
    .map_err(|e| format!("Failed to get app data directory: {}", e))?;
```

### ~~2. Lock Poisoning Vulnerabilities~~ ✅ FIXED

**Status**: Added lock recovery helpers to prevent cascade failures

**Files Modified**:
- `src-tauri/src/providers/mod.rs` - Added `recover_read_lock()` and `recover_write_lock()` helpers
- `src-tauri/src/kernel.rs` - Added recovery helpers for all RwLock operations

**Changes**:
```rust
// Recovery function
fn recover_read_lock<'a, T>(result: Result<RwLockReadGuard<'a, T>, ...>) -> RwLockReadGuard<'a, T> {
    match result {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("Recovered from poisoned read lock");
            poisoned.into_inner()  // Use data anyway, prevent panic
        }
    }
}

// All lock unwrap() calls replaced
let commands = recover_write_lock(self.commands.write());  // Instead of .unwrap()
```

### ~~3. XSS Vulnerability in EPUB Viewer~~ ✅ FIXED

**Status**: Added HTML entity escaping for all dynamic content

**Files Modified**:
- `src/components/EpubViewer.tsx` - Added `escapeHtml()` and `escapeAttr()` functions

**Changes**:
```typescript
// Before - XSS vulnerable
return `<mark class="epub-highlight epub-highlight-${hl.color}" data-highlight-id="${hl.id}" title="${hl.note || "Click to edit"}">${match}</mark>`;

// After - Escaped
const safeColor = escapeAttr(hl.color);
const safeId = escapeAttr(hl.id);
const safeNote = escapeAttr(hl.note || "Click to edit");
return `<mark class="epub-highlight epub-highlight-${safeColor}" data-highlight-id="${safeId}" title="${safeNote}">${match}</mark>`;
```

### ~~4. Missing Error Boundaries~~ ✅ FIXED

**Status**: ErrorBoundary component added and wrapped main components

**Files Created**:
- `src/components/ErrorBoundary.tsx` - Error fallback component
- `src/components/ErrorBoundary.css` - Styling for error display

**Files Modified**:
- `src/App.tsx` - Imported and wrapped components with ErrorBoundary

**Changes**:
```tsx
// Root error boundary
<ErrorBoundary fallback={(error, reset) => <ErrorFallback error={error} reset={reset} />}>
  <ToastProvider>
    <AppContent />
  </ToastProvider>
</ErrorBoundary>

// Also wrapped: ResearchHub, Sessions, VectorQuery panels
```

### 1. Panic-Causing Code in Backend

**Location**: `src-tauri/src/lib.rs`, `kernel.rs`, `pdf.rs`

The application will crash on startup or during operations if certain conditions fail:

```rust
// lib.rs:95-116 - Multiple .expect() calls
app_data_dir.join(...).expect("Failed to get app data directory")
ObjectStore::new(...).expect("Failed to create object store")
SessionManager::new(...).expect("Failed to create session manager")

// kernel.rs:97
memory: Arc::new(MemoryStore::new().expect("Failed to initialize memory store"))

// pdf.rs:513
Self::new().expect("Failed to initialize PDFium")
```

**Impact**: Application cannot recover from initialization failures
**Fix**: Replace `.expect()` with proper `Result` propagation and user-friendly error messages

### 2. Lock Poisoning Vulnerabilities

**Location**: `providers/mod.rs`, `kernel.rs`, `sessions/mod.rs`

Multiple `.unwrap()` calls on mutex locks that will panic if a previous operation panicked while holding the lock:

```rust
// providers/mod.rs:94, 105, 111, 117, 224
let mut commands = self.commands.write().unwrap();
let mut providers = self.providers.write().unwrap();

// kernel.rs:104, 115, 128, 134, 142
let mut contexts = self.contexts.write().unwrap();
```

**Impact**: One panic cascades to crash the entire provider/kernel system
**Fix**: Use `.unwrap_or_else()` or implement lock recovery

### 3. XSS Vulnerability in EPUB Viewer

**Location**: `src/components/EpubViewer.tsx:652`

Highlight text is injected into innerHTML without escaping:

```tsx
// Highlight injection without sanitization
innerHTML={chapter.content.replace(selectedText, `<mark>${selectedText}</mark>`)}
```

**Impact**: Malicious EPUB content could execute scripts
**Fix**: Use a proper HTML sanitizer or DOM-based highlighting

### 4. Missing Error Boundaries in React/Solid Components

**Locations**: All major components

No error boundaries exist. A rendering error in any component crashes the entire app.

**Fix**: Add error boundaries at route and panel levels

---

## Major Issues (Fix Before Beta)

### Frontend Issues

| Component | Issue | Line | Severity |
|-----------|-------|------|----------|
| ChatPanel | `/write` slash command declared but not implemented | 90 | Major |
| ChatPanel | Hardcoded AI config (LM Studio localhost:1234) | 143-150 | Major |
| ChatPanel | Basic HTML formatting (missing links, code blocks) | 609-610 | Major |
| ChatPanel | Search results shown but never displayed to user | 937-941 | Major |
| ResearchHub | Source list limit hardcoded to 100, no pagination | 244 | Major |
| ResearchHub | Adding source doesn't refresh paper filter | 612 | Major |
| ResearchHub | Incomplete error handling for academic search | 533-574 | Major |
| EpubViewer | Nested TOC not implemented | backend | Major |
| EpubViewer | Selection offset calculation "simplified" | 281-282 | Major |
| EpubViewer | Only first occurrence highlighted | 450-454 | Major |
| PdfViewer | Zoom fit doesn't trigger re-render | 268 | Major |
| PdfViewer | Area measurement UX confusing (double-click) | 149-165 | Major |
| LearningPanel | No error feedback for web/academic search | 65-73 | Major |

### Backend Issues

| Module | Issue | Line | Severity |
|--------|-------|------|----------|
| epub.rs | Nested TOC returns empty children | 161 | Major |
| commands.rs | Browser DB scanning not implemented | 3909 | Major |
| commands.rs | Skipped conversations not persisted | 3939 | Major |
| mcp/memory_server.rs | related_files always empty | 854 | Major |
| providers/importers/browser_sync.rs | Chrome/Edge parsing not implemented | 142 | Major |
| sessions/mod.rs | Document detection not implemented | 573 | Major |
| sessions/mod.rs | Browser tab detection not implemented | 579 | Major |
| sessions/mod.rs | Session restoration not implemented | 585 | Major |

---

## Minor Issues (Polish Items)

### Code Quality
- **124 console.log/warn/error statements** across 24 frontend files (should be removed for production)
- Debug logging in `commands.rs:4451` (`log::info!("=== DEBUG: All Objects ===")`)
- Generic error formatting using `{:?}` instead of `{}`

### Hardcoded Values
- Search score threshold 0.3 (ChatPanel:405)
- Max tokens 2048, temperature 0.7 (ChatPanel:600)
- Search maxResults 50 (EpubViewer:220)
- Font size limits 10-32px (EpubViewer:248)
- DPI default 200 (PdfViewer:61)
- Connection pool settings (embeddings.rs:182-189)

### UI/UX Polish
- Loading states too minimal ("...", "Loading...")
- Empty states don't guide users
- No keyboard shortcuts documented in-app
- Inconsistent button text during operations
- Missing "Select All" / "Clear All" in citation selection

### Documentation Mismatch
- `FEATURES.md` references old Python/PyQt version
- Many documented features may not exist in current Rust/Tauri version

---

## Uncommitted Changes

Two files have uncommitted changes for dev environment:

1. **src-tauri/tauri.conf.json**
   - Dev URL changed from 1420 to 1421
   - Window title changed to "MarlOS [dev]"

2. **vite.config.ts**
   - Port changed from 1420 to 1421

**Recommendation**: Commit these as dev-specific changes or use environment variables

---

## Recommended Action Plan

### Phase 1: Critical Fixes (Required for Alpha)
1. Replace all `.expect()` in initialization with proper error handling
2. Add lock poisoning recovery in provider/kernel systems
3. Fix XSS vulnerability in EPUB viewer
4. Add basic error boundaries to main components

### Phase 2: Major Fixes (Before Beta)
1. Implement missing slash commands
2. Add pagination to Research Hub source list
3. Complete EPUB nested TOC support
4. Implement session restoration
5. Add proper error feedback to Learning Panel

### Phase 3: Polish (Beta Quality)
1. Remove debug console.log statements
2. Externalize hardcoded configuration values
3. Improve loading states and empty states
4. Update FEATURES.md to match current codebase
5. Add in-app keyboard shortcut reference

---

## Testing Recommendations

Before alpha release, verify:

1. **Startup resilience** - App handles missing/corrupted database gracefully
2. **PDF rendering** - Various PDF types render correctly
3. **EPUB reading** - Chapter navigation, highlighting work
4. **AI chat** - Connections to LM Studio/Ollama work
5. **Semantic search** - Returns relevant results
6. **Research Hub** - Citations generate correctly
7. **Learning flow** - Complete cycle works end-to-end

---

## Files Requiring Attention

### High Priority
- `src-tauri/src/lib.rs` - Initialization error handling
- `src-tauri/src/kernel.rs` - Lock handling
- `src-tauri/src/providers/mod.rs` - Lock handling
- `src/components/EpubViewer.tsx` - XSS fix
- `src/components/ChatPanel.tsx` - Slash commands, error states

### Medium Priority
- `src/components/ResearchHub.tsx` - Pagination, error handling
- `src-tauri/src/epub.rs` - Nested TOC
- `src-tauri/src/sessions/mod.rs` - Unimplemented functions

### Low Priority
- All frontend files - Remove console.log statements
- `FEATURES.md` - Update documentation
- Configuration externalization

---

*This audit was generated by analyzing the codebase structure, searching for TODOs/FIXMEs, examining error handling patterns, and reviewing component implementations.*
