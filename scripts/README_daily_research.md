# Daily Research Automation

Runs headless Claude Code once a day to fetch and summarize academic papers for
each registered topic, storing results in the marlos-memory DB.

## Components

- `daily_research_prompt.md` — instructions fed to headless Claude
- `daily_research.ps1` — PowerShell wrapper that invokes `claude -p`
- `logs/daily_research_<timestamp>.log` — per-run output (created on first run)

## MCP Tools Used

Exposed by `marlos-memory` (see `.mcp.json`):

- `list_research_topics` — returns registered topics + queries
- `add_research_topic(name, queries?)` — register a new topic
- `remove_research_topic(name)` — unregister a topic
- `daily_research_fetch(topic, limit=5)` — fetch fresh candidates (deduped vs. stored summaries)
- `store_research_summary(topic, title, url, summary, doi?, arxiv_id?, authors?, year?, venue?)`
- `list_research_summaries(topic?, since?, limit?)` — retrieve accumulated summaries

## Dedup Strategy

Each stored summary carries tags:
- `dedup-doi:<doi>` (if DOI present)
- `dedup-arxiv:<id>` (if arXiv ID present)
- `dedup-url:<normalized-url>` (always)

`daily_research_fetch` checks these tags and filters out already-summarized papers.

## One-Time Setup

### 1. Register topics (interactively from Claude Code)

In a Claude Code session (with marlos-memory MCP connected), run:

```
Please call add_research_topic for each of: "AI", "Operating Systems",
"Material Sciences", "Construction Sciences".
```

Or call the tool directly if you have tool access.

### 2. Schedule with Windows Task Scheduler

Open Task Scheduler and create a new task:

- **Trigger:** Daily at your preferred time (e.g. 6:00 AM)
- **Action:** Start a program
  - Program: `powershell.exe`
  - Arguments: `-ExecutionPolicy Bypass -File "F:\CanoLab\MarlOS\scripts\daily_research.ps1"`
  - Start in: `F:\CanoLab\MarlOS`
- **Conditions:** Uncheck "Start only if on AC power" if using a laptop
- **Settings:** Enable "Run task as soon as possible after a scheduled start is missed"

Or create it via PowerShell (run as admin):

```powershell
$action = New-ScheduledTaskAction -Execute 'powershell.exe' `
    -Argument '-ExecutionPolicy Bypass -File "F:\CanoLab\MarlOS\scripts\daily_research.ps1"' `
    -WorkingDirectory 'F:\CanoLab\MarlOS'
$trigger = New-ScheduledTaskTrigger -Daily -At 6:00AM
$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -DontStopIfGoingOnBatteries -AllowStartIfOnBatteries
Register-ScheduledTask -TaskName 'MarlOS Daily Research' `
    -Action $action -Trigger $trigger -Settings $settings -Description 'Daily academic paper fetch + summarize via marlos-memory MCP'
```

### 3. Test the pipeline manually

```powershell
F:\CanoLab\MarlOS\scripts\daily_research.ps1
```

Check `scripts\logs\` for output.

## Notes

- The `--allowedTools` flag limits Claude to only the three research MCP tools,
  so the scheduled run can't do anything else on your machine.
- Each run costs ~5 topics × 5 papers = ~25 short summaries worth of tokens.
- If `claude` CLI is not on PATH, edit `daily_research.ps1` to use the full path.
- Pulling accumulated summaries later: in an interactive Claude Code session,
  ask "list recent research summaries for topic X" — I'll call `list_research_summaries`.
