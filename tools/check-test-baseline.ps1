#!/usr/bin/env pwsh
# SnapClip: the test-count baseline only grows.
#
# docs/31-scroll-capture-tdd-tasklist.md section 0.3 recorded 477 passed / 10 ignored / 0 failed
# on 2026-10-08. P6.03/P6.04 delete and move code, and a deletion that takes a test with it is
# invisible to `cargo test` (the run is still green - just smaller). This script is the numeric
# half of P6.01: it parses the `test result:` lines of a workspace `--lib` run and refuses a
# tree whose suite shrank below the recorded baseline.
#
# The per-crate attribution comes from the `Running unittests ... (<path>)` headers, and only the
# **last** `test result:` line in each section counts: the apartment pin spawns itself as a child
# process, so its own `1 passed; ... filtered out` line also appears inside the section.
#
# ASCII only, on purpose: `.githooks/pre-push` invokes this through `powershell`, whose default
# `-File` encoding is ANSI, so a non-ASCII string literal would arrive mangled.
#
# Usage:
#   pwsh -NoProfile -File tools/check-test-baseline.ps1                 # run the suite, then check
#   pwsh -NoProfile -File tools/check-test-baseline.ps1 -Log <path>    # check an existing log
#   pwsh -NoProfile -File tools/check-test-baseline.ps1 -Run           # force a fresh run

[CmdletBinding()]
param(
    [string]$Log,
    [switch]$Run
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# Ownership accounting (docs/56): preserve historical totals AND require each
# destination binary. The old 23-test model floor is distributed across product
# capture values, geometry, image values and prepared pin values (31 now).
# The history floor 43 is split into clipboard history 42 + capture storage 5.
# The pre-S1 clipboard lib had 15 cases: 1 domain + 14 system cases.
# No unrelated package can compensate for a missing destination binary/case.
$Baseline = [ordered]@{
    'snapclip-app'       = @{ Passed = 261; Ignored = 6 }
    'yue-pin'            = @{ Passed = 58; Ignored = 0 }
    'yue-shot-capture'   = @{ Passed = 347; Ignored = 7 }
    'snapclip-history'   = @{ Passed = 42; Ignored = 0 }
    'yue-shot-storage'   = @{ Passed = 5; Ignored = 0 }
    'snapclip-clipboard' = @{ Passed = 1; Ignored = 0 }
    'yue-clipboard'      = @{ Passed = 12; Ignored = 2 }
    'snapclip-model'     = @{ Passed = 4; Ignored = 0 }
    'yue-media'          = @{ Passed = 8; Ignored = 0 }
    'yue-geometry'       = @{ Passed = 10; Ignored = 0 }
    'yue-shot-model'     = @{ Passed = 5; Ignored = 0 }
    'yue-pin-model'      = @{ Passed = 4; Ignored = 0 }
}
$HistoricalGroups = @(
    @{ Name = 'original model'; Total = 23; Members = @('snapclip-model', 'yue-media', 'yue-geometry', 'yue-shot-model', 'yue-pin-model') },
    @{ Name = 'original history'; Total = 43; Members = @('snapclip-history', 'yue-shot-storage') },
    @{ Name = 'pre-S1 clipboard'; Total = 15; Members = @('snapclip-clipboard', 'yue-clipboard') }
)

if ($Run -or -not $Log) {
    $Log = Join-Path $root 'target\test-baseline.log'
    Write-Host '[baseline] cargo test --workspace --lib -- --test-threads=1'
    # Serial on purpose: see .githooks/pre-push and docs/31 section 4.2 (R-21).
    #
    # `$ErrorActionPreference` must be relaxed around this one call. `2>&1` turns every line cargo
    # writes to stderr (progress, `Finished ...`, warnings) into an ErrorRecord, and under
    # `Stop` Windows PowerShell 5.1 raises a NativeCommandError on the first one -- which aborts
    # the script before the log is even written. `pwsh` does not do this, so the bug only shows up
    # through the pre-push hook. The call's exit code is still checked on the next line.
    $wasStrict = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = & cargo test --workspace --lib -- --test-threads=1 2>&1
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $wasStrict
    }
    $output | Out-File -Encoding utf8 $Log
    if ($code -ne 0) {
        $output | Select-Object -Last 40 | ForEach-Object { Write-Host $_ }
        Write-Host "[baseline] the suite itself is red: fix that first (see $Log)"
        exit $code
    }
}

if (-not (Test-Path $Log)) {
    Write-Host "[baseline] no log at $Log - run without -Log, or pass -Log <path>"
    exit 1
}

$lines = Get-Content -Path $Log
$resultPattern = '^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored'

# crate key (snapclip-app) -> the deps file stem cargo prints (snapclip_app-<hash>)
$byStem = @{}
foreach ($key in $Baseline.Keys) {
    $byStem[$key.Replace('-', '_')] = $key
}

$seen = @{}
$current = $null
foreach ($line in $lines) {
    if ($line -match '^\s*Running unittests .*[\\/]([A-Za-z0-9_]+)-[0-9a-f]+\.exe') {
        $stem = $Matches[1]
        if ($byStem.ContainsKey($stem)) { $current = $byStem[$stem] } else { $current = $null }
        continue
    }
    if ($current -and $line -match $resultPattern) {
        # last one wins: a child process inside this binary prints its own result line
        $seen[$current] = @{
            Passed  = [int]$Matches[2]
            Failed  = [int]$Matches[3]
            Ignored = [int]$Matches[4]
        }
    }
}

if ($seen.Count -eq 0) {
    Write-Host "[baseline] no 'test result:' lines found in $Log - is this a workspace --lib run?"
    exit 1
}

$failures = @()
$rows = foreach ($key in $Baseline.Keys) {
    $base = $Baseline[$key]
    if (-not $seen.ContainsKey($key)) {
        $failures += "$key produced no result line (the whole crate did not run)"
        [pscustomobject]@{ Crate = $key; Baseline = "$($base.Passed)/$($base.Ignored)"; Now = 'missing'; Delta = '-'; Failed = '-' }
        continue
    }
    $now = $seen[$key]
    $baseTotal = $base.Passed + $base.Ignored
    $nowTotal = $now.Passed + $now.Ignored
    $delta = $nowTotal - $baseTotal
    if ($now.Failed -gt 0) {
        $failures += "$key has $($now.Failed) failing test(s)"
    }
    if ($delta -lt 0) {
        $failures += "$key lost $(-$delta) test(s): $nowTotal now, $baseTotal at the section 0.3 baseline"
    }
    $sign = '+'
    if ($delta -lt 0) { $sign = '' }
    [pscustomobject]@{
        Crate    = $key
        Baseline = "$baseTotal ($($base.Passed)+$($base.Ignored))"
        Now      = "$nowTotal ($($now.Passed)+$($now.Ignored))"
        Delta    = "$sign$delta"
        Failed   = $now.Failed
    }
}

foreach ($group in $HistoricalGroups) {
    $total = 0
    foreach ($member in $group.Members) {
        if ($seen.ContainsKey($member)) {
            $total += $seen[$member].Passed + $seen[$member].Ignored
        }
    }
    if ($total -lt $group.Total) {
        $failures += "$($group.Name) lost tests after ownership migration: $total < $($group.Total)"
    }
}

$rows | Format-Table -AutoSize | Out-String | Write-Host
Write-Host '[baseline] failed: 0 required; passed+ignored may only grow (docs/31 section 0.3)'

if ($failures.Count -gt 0) {
    foreach ($failure in $failures) { Write-Host "[baseline] $failure" }
    Write-Host '[baseline] FAILED: the test-count baseline regressed'
    exit 1
}

Write-Host '[baseline] OK'
