# One-shot cleanup of the Debt audiobook transcript:
#   1. Drop the line-1 markdown title (book.toml owns the title page).
#   2. Update book.toml metadata.
#   3. For each chapter: dedupe consecutive identical sentences (Whisper
#      context-drop artifacts), then break the giant run-on paragraph
#      into ~5-sentence paragraphs.
#
# The .bak from the earlier heading-promotion pass is still on disk; if
# the result looks wrong, restore from there.

$mdPath = "F:\Software\Tools\AudioBooks\Debt the firs 5000 years\Debt-UpdatedandExpandedTheFirst5000Years_ep6.md"
$tomlPath = "F:\Software\Tools\AudioBooks\Debt the firs 5000 years\book.toml"

# --- 1) Strip the line-1 book title line + its trailing blank line ---
$raw = [System.IO.File]::ReadAllText($mdPath)
$raw = $raw -replace "^\s*#\s*Debt[^\r\n]*\r?\n\r?\n", ""

# --- 3 & 4) Per-chapter dedupe + paragraph split ---
# Walk the file line by line. When we hit a chapter heading, the next
# non-blank line(s) are the chapter body — collapse them into one
# string, split into sentences, dedupe consecutive duplicates, then
# regroup into 5-sentence paragraphs.
$lines = [System.Text.RegularExpressions.Regex]::Split($raw, "\r\n")

$headingIndices = @()
for ($i = 0; $i -lt $lines.Length; $i++) {
    if ($lines[$i] -match "^# Chapter \d+") { $headingIndices += $i }
}

$output = New-Object System.Text.StringBuilder
$lastEnd = 0
for ($k = 0; $k -lt $headingIndices.Length; $k++) {
    $hIdx = $headingIndices[$k]

    # Emit anything between previous chapter end and this heading
    # verbatim (front matter, blank lines, etc.).
    for ($j = $lastEnd; $j -lt $hIdx; $j++) {
        [void]$output.AppendLine($lines[$j])
    }

    # Emit the heading line and a blank.
    [void]$output.AppendLine($lines[$hIdx])
    [void]$output.AppendLine("")

    # End of this chapter = one line before next heading, or last line.
    $endIdx = if ($k + 1 -lt $headingIndices.Length) {
        $headingIndices[$k + 1] - 1
    } else {
        $lines.Length - 1
    }

    # Collapse body into one string. The transcript has each chapter
    # on a single line, but be defensive in case there are wrapped lines.
    $bodyParts = New-Object System.Collections.ArrayList
    for ($j = $hIdx + 1; $j -le $endIdx; $j++) {
        $t = $lines[$j].Trim()
        if ($t -ne "") { [void]$bodyParts.Add($t) }
    }
    $body = $bodyParts -join " "

    if ($body.Length -eq 0) {
        $lastEnd = $endIdx + 1
        continue
    }

    # Sentence-split. Lookbehind: terminated by . ! or ?. Lookahead:
    # next char must be uppercase or an opening quote. Doesn't perfectly
    # handle "Mr." / "U.S." abbreviations — we re-merge those below
    # using a length filter.
    $sentences = [System.Text.RegularExpressions.Regex]::Split(
        $body, '(?<=[.!?])\s+(?=[A-Z"`'']|“)'
    )

    # Merge super-short "sentences" (<= 4 chars after trim) into the
    # next one — these are split-on-abbreviation artifacts ("Mr.", "U.S.").
    $glued = New-Object System.Collections.ArrayList
    $carry = ""
    foreach ($s in $sentences) {
        $piece = $s
        if ($carry -ne "") {
            $piece = $carry + " " + $piece
            $carry = ""
        }
        if ($piece.Trim().Length -le 4) {
            $carry = $piece.TrimEnd()
        } else {
            [void]$glued.Add($piece)
        }
    }
    if ($carry -ne "") { [void]$glued.Add($carry) }

    # Dedupe consecutive identical sentences (case-insensitive trim
    # compare). Whisper repeat artifacts are byte-identical, so this
    # catches them reliably.
    $deduped = New-Object System.Collections.ArrayList
    $prev = ""
    $duplicatesRemoved = 0
    foreach ($s in $glued) {
        $key = $s.Trim().ToLowerInvariant()
        if ($key -ne $prev) {
            [void]$deduped.Add($s)
            $prev = $key
        } else {
            $duplicatesRemoved++
        }
    }

    # Regroup into ~5-sentence paragraphs. We prefer to break BEFORE a
    # sentence that starts with a discourse marker once we've hit 4
    # sentences, otherwise hard-break at 6. This makes paragraphs feel
    # less arbitrary while still reining in the wall-of-text problem.
    $discourseMarkers = @(
        "But ", "Now ", "Then ", "Yet ", "So ", "Still ", "Of course",
        "Here", "There ", "Meanwhile", "In other words", "In fact",
        "Actually", "After all", "However", "For instance", "For example",
        "On the other hand", "Indeed", "By contrast", "In the end",
        "What", "Why", "How", "When ", "Yes", "No,", "Well,", "I "
    )
    $isMarker = {
        param($s)
        $t = $s.TrimStart()
        foreach ($m in $discourseMarkers) {
            if ($t.StartsWith($m, [System.StringComparison]::Ordinal)) {
                return $true
            }
        }
        return $false
    }

    $paragraphs = New-Object System.Collections.ArrayList
    $current = New-Object System.Collections.ArrayList
    for ($i2 = 0; $i2 -lt $deduped.Count; $i2++) {
        $s = $deduped[$i2]
        # Look-ahead: should we break BEFORE this sentence?
        if ($current.Count -ge 4 -and (& $isMarker $s)) {
            [void]$paragraphs.Add(($current -join " "))
            $current = New-Object System.Collections.ArrayList
        } elseif ($current.Count -ge 6) {
            [void]$paragraphs.Add(($current -join " "))
            $current = New-Object System.Collections.ArrayList
        }
        [void]$current.Add($s)
    }
    if ($current.Count -gt 0) {
        [void]$paragraphs.Add(($current -join " "))
    }

    # Emit paragraphs separated by blank lines.
    foreach ($p in $paragraphs) {
        [void]$output.AppendLine($p)
        [void]$output.AppendLine("")
    }

    Write-Host ("Chapter " + ($k + 1).ToString() + ": " + $paragraphs.Count + " paragraphs, " + $duplicatesRemoved + " duplicate sentences removed")

    $lastEnd = $endIdx + 1
}

# Emit anything trailing the final chapter.
for ($j = $lastEnd; $j -lt $lines.Length; $j++) {
    [void]$output.AppendLine($lines[$j])
}

# Write back (UTF-8 no BOM, CRLF preserved — System.IO.File.WriteAllText
# defaults to UTF-8 sans BOM on .NET Framework, which is what we want).
[System.IO.File]::WriteAllText($mdPath, $output.ToString())

# --- 2) book.toml metadata ---
$toml = [System.IO.File]::ReadAllText($tomlPath)
$toml = $toml -replace 'title = "[^"]*"', 'title = "Debt"'
$toml = $toml -replace 'subtitle = "[^"]*"', 'subtitle = "The First 5,000 Years"'
$toml = $toml -replace 'author = "[^"]*"', 'author = "David Graeber"'
[System.IO.File]::WriteAllText($tomlPath, $toml)

Write-Host "`nDone."
