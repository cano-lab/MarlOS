# MarlOS MCP Memory Server

The MCP Memory Server exposes your MarlOS semantic memory to AI tools like Claude Code, Cursor, and any other MCP-compatible application.

## Installation

### 1. Build the MCP Server

```bash
cd src-tauri
cargo build --bin marlos-mcp --release
```

The binary will be at `target/release/marlos-mcp.exe` (Windows) or `target/release/marlos-mcp` (Mac/Linux).

### 2. Configure Claude Code

Add to your Claude Code settings file:

**Windows:** `%APPDATA%\Claude\claude_desktop_config.json`
**Mac:** `~/Library/Application Support/Claude/claude_desktop_config.json`
**Linux:** `~/.config/claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "marlos-memory": {
      "command": "C:\\path\\to\\marlos-mcp.exe",
      "args": []
    }
  }
}
```

Or with a custom database path:

```json
{
  "mcpServers": {
    "marlos-memory": {
      "command": "marlos-mcp",
      "args": ["--db-path", "/path/to/your/objects.db"]
    }
  }
}
```

### 3. Restart Claude Code

After adding the configuration, restart Claude Code. You should see the MarlOS tools available.

## Available Tools

### search_memory

Search your semantic memory for relevant past work, sessions, decisions, and research.

**Parameters:**
- `query` (required): The search query - can be a question, topic, or description
- `limit` (optional): Maximum results (default: 10, max: 50)
- `source_type` (optional): Filter by type - "session", "document", "decision", "research", or "all"
- `project` (optional): Filter by project name

**Example:**
```
Search memory for "authentication implementation decisions"
```

### get_context

Get relevant context for the current work. Provide a file path, project name, or description.

**Parameters:**
- `file_path` (optional): Current file being worked on
- `project` (optional): Project name or path
- `description` (optional): Description of current task
- `include_decisions` (optional): Include past decisions (default: true)

**Example:**
```
Get context for the file "src/auth/login.rs" in project "marlos-rust"
```

### log_decision

Record a decision with its reasoning. This helps the AI remember why choices were made.

**Parameters:**
- `topic` (required): What the decision is about
- `choice` (required): The choice that was made
- `reasoning` (required): Why this choice was made
- `alternatives` (optional): Other options that were considered
- `project` (optional): Project this decision relates to
- `file_path` (optional): File this decision relates to
- `tags` (optional): Tags for categorizing

**Example:**
```
Log decision: topic="database choice", choice="SQLite", reasoning="Local-first, single file, no server needed"
```

### get_decisions

Query past decisions by topic, project, or time range.

**Parameters:**
- `topic` (optional): Filter by decision topic (semantic search)
- `project` (optional): Filter by project name
- `limit` (optional): Maximum decisions to return (default: 10)
- `days_back` (optional): Only return decisions from the last N days

**Example:**
```
Get decisions about "authentication" from the last 30 days
```

### get_related

Find content related to a specific piece of work.

**Parameters:**
- `content` (optional): Content to find related items for
- `object_id` (optional): ID of existing object to find related items for
- `limit` (optional): Maximum related items (default: 10)

### get_memory_stats

Get statistics about the memory system.

## Usage Examples

### In Claude Code

Once configured, you can ask Claude to use your memory:

```
"Search my memory for how we implemented the parser last month"

"What decisions have I made about the database schema?"

"Get context for the authentication module I'm working on"

"Log this decision: We're using JWT for auth because we need stateless tokens for mobile"
```

### Via HTTP (Debugging)

Start the server in HTTP mode for debugging:

```bash
marlos-mcp --http --port 3100
```

Then test with curl:

```bash
curl -X POST http://localhost:3100 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

## Security

- The MCP server only exposes content marked as `SecurityTier::Open`
- Guarded and Sealed content is never exposed via MCP
- All data stays local - no cloud communication

## Troubleshooting

### Server won't start

1. Make sure the database exists (run MarlOS app first)
2. Check the database path: `marlos-mcp --db-path /path/to/objects.db`
3. Enable debug logging: `marlos-mcp --log-level debug`

### No results from search

1. Make sure you've imported sessions: `marlos-cli sessions import all --embed`
2. Check that LM Studio is running for embeddings
3. Verify content exists: `marlos-cli search "test query"`

### Claude Code doesn't see the tools

1. Check the config file path is correct for your OS
2. Verify the path to marlos-mcp is absolute
3. Restart Claude Code after config changes
4. Check Claude Code logs for MCP errors
