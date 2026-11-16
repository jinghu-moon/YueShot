# P4 product-path replay evidence. This suite alone is not the complete stage gate.
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$EvidenceDirectory)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$destination = [IO.Path]::GetFullPath((Join-Path $root $EvidenceDirectory))
if (Test-Path -LiteralPath $destination) { throw 'Use a new evidence directory; prior evidence is immutable.' }
New-Item -ItemType Directory -Path $destination | Out-Null
$hashes = [ordered]@{}
$files = @(Get-ChildItem apps/yue-shot/crates/yue-shot-capture/src -Recurse -Filter '*.rs' -File)
$files += Get-Item Cargo.lock, $PSCommandPath
foreach ($file in $files) {
    $name = $file.FullName.Substring($root.Length + 1).Replace('\', '/')
    $hashes[$name] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}
$report = [ordered]@{
    stage = 'P4'; scope = 'Selected production path software and real-GPU fixture replays, not complete stage qualification'
    revision = (& git rev-parse HEAD).Trim(); started = Get-Date -Format o
    sourceSha256 = $hashes; suites = @(); checksPassed = $false; completeAcceptance = $false
    userAcceptance = 'Deferred by explicit user instruction; not counted as passed'
}
$suites = @(
    @{ Name = 'visual-reference'; Filter = 'visual'; Passed = 33; Ignored = 31; All = $false },
    @{ Name = 'product-replays'; Filter = 'visual_replay'; Passed = 29; Ignored = 0; All = $true },
    @{ Name = 'owner-lifetimes'; Filter = 'owner_lifetime'; Passed = 5; Ignored = 0; All = $false },
    @{ Name = 'transfer-faults'; Filter = 'visual_transfer_formats_capacity_and_map_failures_preserve_lifecycle'; Passed = 1; Ignored = 0; All = $true }
)
foreach ($suite in $suites) {
    $arguments = @('test', '-p', 'yue-shot-capture', '--release', '--locked', '--lib', $suite.Filter, '--', '--nocapture', '--test-threads=1')
    if ($suite.All) { $arguments += '--include-ignored' }
    if ($suite.Name -eq 'product-replays') { $arguments += @('--skip', 'latency_diagnostic', '--skip', 'resource_probe') }
    $started = Get-Date -Format o
    $ErrorActionPreference = 'Continue'
    try { $output = & cargo @arguments 2>&1; $code = $LASTEXITCODE } finally { $ErrorActionPreference = 'Stop' }
    $lines = @($output | ForEach-Object { $_.ToString() })
    $path = Join-Path $destination "$($suite.Name).txt"
    $lines | Out-File -Encoding utf8 $path
    $result = $lines | Where-Object { $_ -match '^test result: ' } | Select-Object -Last 1
    $parsed = $result -match '^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored'
    $valid = $false
    if ($parsed) { $valid = $code -eq 0 -and [int]$Matches[2] -eq $suite.Passed -and [int]$Matches[3] -eq 0 -and [int]$Matches[4] -eq $suite.Ignored }
    $report.suites += [ordered]@{ name = $suite.Name; command = 'cargo ' + ($arguments -join ' '); started = $started; finished = Get-Date -Format o; exitCode = $code; result = $result; passed = $valid; sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() }
    $report | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $destination 'summary.json')
    Write-Host "$($suite.Name): $result gate=$valid"
}
$report.checksPassed = @($report.suites | Where-Object { -not $_.passed }).Count -eq 0
$report.sourceUnchanged = $true
foreach ($name in $hashes.Keys) {
    if ((Get-FileHash -LiteralPath (Join-Path $root $name) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hashes[$name]) { $report.sourceUnchanged = $false }
}
$report | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $destination 'summary.json')
if (-not $report.checksPassed -or -not $report.sourceUnchanged) { exit 1 }
