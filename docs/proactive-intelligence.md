# MarlOS Proactive Intelligence

Automatically surface relevant context, decisions, and patterns without explicit queries. This is what differentiates MarlOS from passive knowledge bases.

## Features

### 1. Context Triggers

Automatically detect when past work is relevant to current context.

**Triggers on:**
- File being opened
- Query/prompt being typed
- Code patterns being written

**Example:**
```
You open `auth/login.rs`
→ MarlOS surfaces: "You discussed JWT vs session tokens 3 days ago and chose JWT"
```

### 2. Decision Reminders

Surface past decisions when revisiting related topics.

**Capabilities:**
- Find decisions relevant to current file/query
- Detect potential conflicts with past decisions
- Show reasoning behind past choices

**Example:**
```
You're about to implement session-based auth
→ MarlOS warns: "You decided to use JWT 3 days ago because of mobile API requirements"
```

### 3. Pattern Detection

Identify recurring themes across sessions.

**Detects:**
- Recurring problems/questions
- Files frequently accessed together
- Topic clusters

**Example:**
```
→ "You've asked about database migrations 4 times this week. Consider creating a checklist."
```

### 4. Work Continuity

Remember where you left off in a project.

**Tracks:**
- Last files accessed
- Recent work summary
- Pending notes/TODOs

**Example:**
```
Starting new session on MarlOS project
→ "Last session: Implementing paper generator. Files: pipeline.rs, export.rs"
```

## API

### Tauri Commands

#### `proactive_on_file_opened`
Get suggestions when opening a file.

```typescript
const suggestions = await invoke('proactive_on_file_opened', {
  filePath: '/path/to/file.rs'
});
```

#### `proactive_on_query`
Get suggestions when typing a query.

```typescript
const suggestions = await invoke('proactive_on_query', {
  query: 'how to implement authentication'
});
```

#### `proactive_on_session_start`
Get continuity context when starting work.

```typescript
const suggestions = await invoke('proactive_on_session_start', {
  projectPath: '/path/to/project'
});
```

#### `proactive_check_decision`
Check if a proposed decision conflicts with past ones.

```typescript
const conflicts = await invoke('proactive_check_decision', {
  topic: 'authentication method',
  proposedChoice: 'session cookies'
});
```

#### `proactive_log_decision`
Record a decision for future reference.

```typescript
const id = await invoke('proactive_log_decision', {
  topic: 'Database choice',
  choice: 'SQLite',
  reasoning: 'Local-first, single file, no server needed',
  project: 'marlos-rust',
  tags: ['database', 'architecture']
});
```

#### `proactive_decision_stats`
Get statistics about logged decisions.

```typescript
const stats = await invoke('proactive_decision_stats');
// { total_decisions: 42, recent_decisions: 8, top_topics: [...] }
```

## Suggestion Types

| Type | Description | When Triggered |
|------|-------------|----------------|
| `RelatedContext` | Related past work | File open, query |
| `PastDecision` | A relevant past decision | File open, query |
| `DetectedPattern` | Recurring theme identified | Session start |
| `WorkContinuity` | Previous work context | Session start |
| `DecisionConflict` | Conflict with past decision | Decision context |
| `SimilarSolution` | Similar problem solved before | Query, code |

## Suggestion Structure

```typescript
interface ProactiveSuggestion {
  id: string;
  suggestion_type: SuggestionType;
  relevance: number;  // 0.0 to 1.0
  title: string;
  content: string;
  reason: string;
  source_ids: string[];
  generated_at: string;
  seen: boolean;
}
```

## Configuration

```rust
ProactiveConfig {
    // Minimum relevance score to surface (0.0-1.0)
    min_relevance: 0.7,

    // Maximum suggestions shown at once
    max_suggestions: 5,

    // Days to look back for patterns
    pattern_lookback_days: 30,

    // Track file access for pattern detection
    track_file_access: true,

    // Detect decision conflicts
    detect_conflicts: true,
}
```

## Integration Points

### IDE Integration (via MCP)

```json
{
  "mcpServers": {
    "marlos-memory": {
      "command": "marlos-mcp",
      "args": []
    }
  }
}
```

The MCP Memory Server exposes proactive features to Claude Code, Cursor, etc.

### Frontend Integration

```tsx
function FileEditor({ file }) {
  const [suggestions, setSuggestions] = useState([]);

  useEffect(() => {
    invoke('proactive_on_file_opened', { filePath: file.path })
      .then(setSuggestions);
  }, [file.path]);

  return (
    <div>
      {suggestions.length > 0 && (
        <SuggestionPanel suggestions={suggestions} />
      )}
      <Editor file={file} />
    </div>
  );
}
```

## Architecture

```
                         ┌─────────────────────┐
                         │  ProactiveEngine    │
                         └──────────┬──────────┘
                                    │
          ┌─────────────────────────┼─────────────────────────┐
          │                         │                         │
          ▼                         ▼                         ▼
┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ ContextTriggers  │    │ DecisionTracker  │    │ PatternDetector  │
│                  │    │                  │    │                  │
│ - File triggers  │    │ - Find relevant  │    │ - Similar probs  │
│ - Query triggers │    │ - Check conflicts│    │ - File clusters  │
│ - Code patterns  │    │ - Log decisions  │    │ - Topic clusters │
└──────────────────┘    └──────────────────┘    └──────────────────┘
          │                         │                         │
          └─────────────────────────┼─────────────────────────┘
                                    │
                                    ▼
                         ┌──────────────────┐
                         │   WorkJournal    │
                         │                  │
                         │ - File access    │
                         │ - Session context│
                         │ - Continuity     │
                         └──────────────────┘
```

## Future Enhancements

1. **Learning from dismissals** - Reduce similar suggestions if dismissed
2. **Time-aware suggestions** - Consider time of day/week patterns
3. **Team patterns** - Share patterns across team (opt-in)
4. **Custom triggers** - User-defined rules for when to suggest
5. **Embedding-based clustering** - Better pattern detection using vectors
