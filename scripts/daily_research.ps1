#Requires -Version 5.1
<#
.SYNOPSIS
  Daily research automation — runs headless Claude Code to fetch and summarize
  academic papers for each registered research topic.

.DESCRIPTION
  Invokes `claude -p` with the prompt in daily_research_prompt.md. Claude uses
  the marlos-memory MCP server (configured in the project's .mcp.json) to call
  list_research_topics, daily_research_fetch, and store_research_summary.

  Schedule this via Windows Task Scheduler to run once daily.

.NOTES
  Requires claude CLI on PATH. Output is logged to scripts\logs\.
#>

$ErrorActionPreference = 'Stop'

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$PromptFile  = Join-Path $PSScriptRoot 'daily_research_prompt.md'
$LogDir      = Join-Path $PSScriptRoot 'logs'
$Timestamp   = Get-Date -Format 'yyyy-MM-dd_HHmmss'
$LogFile     = Join-Path $LogDir "daily_research_$Timestamp.log"

if (-not (Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}

Set-Location $ProjectRoot

$prompt = Get-Content -Raw -Path $PromptFile

$allowedTools = @(
    'mcp__marlos-memory__list_research_topics',
    'mcp__marlos-memory__daily_research_fetch',
    'mcp__marlos-memory__store_research_summary'
) -join ','

"=== Daily research run: $Timestamp ===" | Out-File -FilePath $LogFile -Encoding utf8

try {
    $output = & claude -p $prompt --allowedTools $allowedTools 2>&1
    $output | Out-File -FilePath $LogFile -Encoding utf8 -Append
    "=== Exit code: $LASTEXITCODE ===" | Out-File -FilePath $LogFile -Encoding utf8 -Append
    exit $LASTEXITCODE
} catch {
    "ERROR: $_" | Out-File -FilePath $LogFile -Encoding utf8 -Append
    exit 1
}
