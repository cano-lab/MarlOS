# MarlOS Universal Import

Import AI conversations and knowledge from external tools into MarlOS for unified semantic search.

## Supported Sources

### ChatGPT

Import conversation history from OpenAI ChatGPT exports.

**How to export:**
1. Go to chat.openai.com
2. Click your profile → Settings
3. Data controls → Export data
4. Download the ZIP file
5. Extract and locate `conversations.json`

**Import command:**
```bash
marlos-cli import-from chatgpt /path/to/conversations.json --embed
```

**What's imported:**
- All conversations as semantic objects
- Message content (user and assistant)
- Timestamps
- Conversation titles
- Model information (when available)

### Cursor

Import AI conversations from Cursor IDE.

**Default locations:**
- Windows: `%APPDATA%\Cursor\User\workspaceStorage\`
- macOS: `~/Library/Application Support/Cursor/User/workspaceStorage/`
- Linux: `~/.config/Cursor/User/workspaceStorage/`

**Import command:**
```bash
# Use default location
marlos-cli import-from cursor --embed

# Or specify a workspace path
marlos-cli import-from cursor --path /path/to/workspace --embed
```

**What's imported:**
- AI chat history
- File references
- Workspace/project context
- Timestamps

### Obsidian

Import notes from an Obsidian vault.

**Import command:**
```bash
marlos-cli import-from obsidian /path/to/vault --embed
```

**What's imported:**
- All markdown notes
- YAML frontmatter (title, tags, aliases, custom fields)
- Wikilinks as relations between notes
- Inline tags (#tag)
- File timestamps

**Supported features:**
- `[[wikilinks]]` → converted to object relations
- `![[embeds]]` → converted to contains relations
- `[[link|display text]]` → parsed correctly
- `[[link#heading]]` → heading references preserved
- Frontmatter aliases for note discovery

## Auto-Detection

Let MarlOS automatically detect the source type:

```bash
marlos-cli import-from auto /path/to/source --embed
```

Detection rules:
- `.json` file with ChatGPT structure → ChatGPT importer
- Directory with `.cursor` folder → Cursor importer
- Directory with `.obsidian` folder → Obsidian importer

## List Available Sources

```bash
marlos-cli import-from list
```

Shows all supported import sources with their expected locations and formats.

## Embedding Flag

The `--embed` flag generates vector embeddings for imported content:

```bash
marlos-cli import-from chatgpt conversations.json --embed
```

Without `--embed`:
- Fast import
- Content is stored but not searchable via semantic search

With `--embed`:
- Slower (each item needs embedding generation)
- Content becomes semantically searchable
- Requires LM Studio or Ollama running

## Architecture

Importers are implemented as a trait:

```rust
pub trait Importer {
    fn source_name(&self) -> &'static str;
    fn can_import(&self, path: &Path) -> bool;
    fn import(&self, path: &Path, embed: bool) -> Result<ImportResult, String>;
}
```

Each importer:
1. Parses source-specific format
2. Converts to intermediate data structures
3. Generates SemanticObjects with appropriate metadata
4. Tags with source identifier for filtering

## Metadata Mapping

| Source | Metadata Fields |
|--------|-----------------|
| ChatGPT | `source`, `conversation_id`, `message_count`, `model` |
| Cursor | `source`, `conversation_id`, `workspace_path`, `files` |
| Obsidian | `source`, `path`, `aliases`, `outgoing_links` |

## Tags

All imported objects are tagged with their source:

- `chatgpt` + `conversation` + `ai`
- `cursor` + `conversation` + `ai` + `ide`
- `obsidian` + `note` + custom tags from frontmatter

## Searching Imported Content

After import, search across all sources:

```bash
# Search everything
marlos-cli search "authentication implementation"

# Filter by source tag
marlos-cli search "how to implement auth" --tag chatgpt

# Search Obsidian notes specifically
marlos-cli search "project ideas" --tag obsidian
```

## Deduplication

MarlOS uses unique identifiers to prevent duplicate imports:
- ChatGPT: conversation ID
- Cursor: conversation ID + workspace hash
- Obsidian: file path within vault

Re-running import will update existing objects rather than create duplicates.

## Example Workflow

```bash
# 1. Check available sources
marlos-cli import-from list

# 2. Import ChatGPT history
marlos-cli import-from chatgpt ~/Downloads/chatgpt-export/conversations.json --embed

# 3. Import Cursor sessions (default path)
marlos-cli import-from cursor --embed

# 4. Import Obsidian vault
marlos-cli import-from obsidian ~/Documents/MyVault --embed

# 5. Search across all imported content
marlos-cli search "how to implement authentication"
```

## Programmatic Usage

Importers can also be used programmatically:

```rust
use marlos_lib::providers::{
    ChatGptImporter, CursorImporter, ObsidianImporter, Importer
};

let importer = ChatGptImporter::new();
let result = importer.import(Path::new("conversations.json"), false)?;

println!("Imported {} items, created {} objects",
    result.items_imported,
    result.objects_created);

for obj in result.objects {
    // Store in object store
    store.create(&obj)?;
}
```

## Adding New Importers

To add support for a new source:

1. Create `src/providers/importers/new_source.rs`
2. Implement the `Importer` trait
3. Add to `importers/mod.rs`
4. Add CLI subcommand in `bin/marlos.rs`
5. Update detection logic in `detect_source()`
