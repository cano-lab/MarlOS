# Mobile Components for MarlOS

Mobile-optimized UI components for MarlOS, designed for touch interfaces with iOS/Android safe area support.

## Components

### MobileApp
Main mobile app shell with bottom navigation.

**Features:**
- Bottom tab navigation (Memory, Capture, Research, Settings)
- Floating action button for quick capture
- Safe area support for notched phones
- Landscape and tablet responsive layouts

**Tabs:**
- **Memory** - Browse and search your semantic memory
- **Capture** - Quick capture text, photos, or voice notes
- **Research** - Web search and save sources
- **Settings** - Sync status, storage info, account management

### MobileCapture
Quick capture component for text, photos, and voice notes.

**Features:**
- Three capture modes: text, photo, voice
- Photo capture with camera or file picker
- Voice recording with transcription (requires backend support)
- Quick tag buttons for common tags
- Auto-generated titles based on capture type and time

**Backend Commands:**
```typescript
// Save a capture
invoke("quick_capture", {
  captureType: "text" | "photo" | "voice",
  content: JSON.stringify({
    title: "...",
    content: "...",
    photo: "base64...",  // for photos
    tags: ["..."],
    captured_at: "ISO date"
  }),
  tags: ["..."]
})
```

### MobileMemoryViewer
Browse and search semantic memory on mobile.

**Features:**
- Full-text semantic search
- Kind filters (note, decision, conversation, research, code)
- Recent items view
- Expandable detail modal
- Score-based search results

**Backend Commands:**
```typescript
// Get recent objects
invoke("get_recent_objects", { limit: 20 })

// Semantic search
invoke("semantic_search", {
  query: "search text",
  limit: 20,
  kind: "note" | null  // optional filter
})
```

### MobileResearch
Web search and source management.

**Features:**
- Web search integration
- Save sources with notes and tags
- Browse saved sources
- Open URLs in external browser

**Backend Commands:**
```typescript
// Web search
invoke("web_search", { query: "...", limit: 10 })

// Save a source
invoke("save_research_source", {
  title: "...",
  url: "...",
  snippet: "...",
  notes: "...",
  tags: ["..."]
})

// Get saved sources
invoke("get_saved_sources", { limit: 50 })

// Open URL
invoke("open_external_url", { url: "..." })
```

## CSS Variables

The mobile components use the same CSS variables as the desktop app:

```css
--bg-primary: #11111b
--bg-secondary: #1e1e2e
--bg-tertiary: #181825
--bg-hover: #262637
--border-color: #313244
--text-primary: #cdd6f4
--text-secondary: #a6adc8
--text-tertiary: #6c7086
--accent-color: #89b4fa
--accent-hover: #74a8f7
--success-color: #a6e3a1
--warning-color: #f9e2af
--error-color: #f38ba8
```

## Safe Area Support

All components support iOS safe areas for notched phones:

```css
@supports (padding-top: env(safe-area-inset-top)) {
  /* Safe area adjustments */
}
```

## Usage

Import the components:

```typescript
import { MobileApp, MobileCapture, MobileMemoryViewer, MobileResearch } from "./components/mobile";
```

Or use the main app shell:

```tsx
import MobileApp from "./components/mobile/MobileApp";

function App() {
  return <MobileApp />;
}
```

## Architecture

The mobile components use the same backend infrastructure as desktop:
- SemanticSearch for memory storage and retrieval
- Same embedding model for semantic search
- Same object store (SQLite + embeddings)
- Syncs via the same memory database

## Building for Mobile

### Switch to Mobile Entry Point

Edit `src/index.tsx` to use the mobile entry point:

```tsx
/* @refresh reload */
import { render } from "solid-js/web";
import MobileEntry from "./MobileApp";  // Changed from App
import "./index.css";

const root = document.getElementById("root");

render(() => <MobileEntry />, root!);
```

Or use a conditional approach based on platform:

```tsx
import { render } from "solid-js/web";
import App from "./App";
import MobileEntry from "./MobileApp";
import "./index.css";

const isMobile = /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
const root = document.getElementById("root");

render(() => isMobile ? <MobileEntry /> : <App />, root!);
```

### Enable Mobile Build

1. Install Android Studio (for Android) or Xcode (for iOS)
2. Initialize mobile:
   ```bash
   npm run tauri android init
   # or
   npm run tauri ios init
   ```
3. Configure signing certificates
4. Build:
   ```bash
   npm run tauri android build
   # or
   npm run tauri ios build
   ```

### Development

For mobile development:
```bash
npm run tauri android dev
# or
npm run tauri ios dev
```
