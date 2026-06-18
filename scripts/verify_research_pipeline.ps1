#Requires -Version 5.1
# Smoke test: list topics, then fetch candidates for one topic.

$ErrorActionPreference = 'Stop'
$McpExe = 'F:\CanoLab\MarlOS\src-tauri\target\release\marlos-mcp.exe'
$DbPath = 'C:\Users\jerro\AppData\Roaming\com.marlos.app\objects.db'

$requests = @(
    (@{ jsonrpc='2.0'; id=0; method='initialize'; params=@{} } | ConvertTo-Json -Compress -Depth 6),
    (@{ jsonrpc='2.0'; id=1; method='tools/call'; params=@{ name='list_research_topics'; arguments=@{} } } | ConvertTo-Json -Compress -Depth 6),
    (@{ jsonrpc='2.0'; id=2; method='tools/call'; params=@{ name='daily_research_fetch'; arguments=@{ topic='AI'; limit=2 } } } | ConvertTo-Json -Compress -Depth 6)
)

$input = ($requests -join "`n") + "`n"
$tmpIn = [System.IO.Path]::GetTempFileName()
[System.IO.File]::WriteAllText($tmpIn, $input)
$output = cmd /c """$McpExe"" --db-path ""$DbPath"" < ""$tmpIn"" 2> NUL"
Remove-Item $tmpIn -ErrorAction SilentlyContinue
$output
