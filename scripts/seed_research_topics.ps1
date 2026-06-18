#Requires -Version 5.1
<#
.SYNOPSIS
  One-time seed: register the initial research topics into the marlos-memory DB.

.DESCRIPTION
  Spawns marlos-mcp.exe in stdio mode and sends JSON-RPC calls to register
  the initial topics: AI, Operating Systems, Material Sciences, Construction Sciences.
  Safe to re-run — add_research_topic upserts on duplicate names.
#>

$ErrorActionPreference = 'Stop'

$McpExe = 'F:\CanoLab\MarlOS\src-tauri\target\release\marlos-mcp.exe'
$DbPath = 'C:\Users\jerro\AppData\Roaming\com.marlos.app\objects.db'

if (-not (Test-Path $McpExe)) { throw "Binary not found: $McpExe" }
if (-not (Test-Path $DbPath)) { throw "Database not found: $DbPath" }

$topics = @(
    @{ name = 'AI';                   queries = @('artificial intelligence', 'machine learning', 'large language models') },
    @{ name = 'Operating Systems';    queries = @('operating systems', 'kernel design', 'distributed systems') },
    @{ name = 'Material Sciences';    queries = @('materials science', 'nanomaterials', 'composite materials') },
    @{ name = 'Construction Sciences'; queries = @('construction engineering', 'structural engineering', 'building materials') }
)

$requests = New-Object System.Collections.Generic.List[string]
$requests.Add((@{ jsonrpc='2.0'; id=0; method='initialize'; params=@{} } | ConvertTo-Json -Compress -Depth 6))

$id = 1
foreach ($t in $topics) {
    $req = @{
        jsonrpc = '2.0'
        id = $id
        method = 'tools/call'
        params = @{
            name = 'add_research_topic'
            arguments = @{
                name = $t.name
                queries = $t.queries
            }
        }
    }
    $requests.Add(($req | ConvertTo-Json -Compress -Depth 6))
    $id++
}

$input = ($requests -join "`n") + "`n"

Write-Host "Spawning marlos-mcp.exe and seeding $($topics.Count) topics..."
$output = $input | & $McpExe --db-path $DbPath 2>&1

$output | ForEach-Object {
    if ($_ -match '"result"|"error"') {
        Write-Host $_
    }
}

Write-Host "`nDone. Run: '$McpExe --db-path $DbPath' + call list_research_topics to verify."
