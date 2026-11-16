# P3 prototype evidence. Never changes product settings, captures desktop pixels,
# installs hotkeys, or writes clipboard. -Present creates a no-activate test HWND.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$EvidenceDirectory,
    [ValidateRange(100, 10000)][int]$Samples = 1000,
    [ValidateRange(1, 10)][int]$Sessions = 3,
    [switch]$Present
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$destination = [IO.Path]::GetFullPath((Join-Path $root $EvidenceDirectory))
if (Test-Path -LiteralPath $destination) { throw 'Use a new evidence directory; prior evidence is immutable.' }
New-Item -ItemType Directory -Path $destination | Out-Null
$paths = @(
    Get-ChildItem apps/yue-shot/crates/yue-shot-capture/src/visual_detection -File
    Get-Item apps/yue-shot/crates/yue-shot-capture/src/windows/win/d3d11.rs,
        apps/yue-shot/crates/yue-shot-capture/src/windows/win/d3d11/visual_transfer.rs,
        apps/yue-shot/crates/yue-shot-capture/src/windows/proc_mem.rs,
        apps/yue-shot/crates/yue-shot-capture/tests/fixtures/visual-candidate-v4.1/dataset-lock.json,
        apps/yue-shot/crates/yue-shot-capture/tests/fixtures/visual-candidate-8k-v4/dataset-lock.json,
        Cargo.lock, $PSCommandPath
)
$hashes = [ordered]@{}
foreach ($path in $paths) {
    $key = $path.FullName.Substring($root.Length + 1).Replace('\', '/')
    $hashes[$key] = (Get-FileHash -LiteralPath $path.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}
$summary = [ordered]@{
    scope = 'P3 test prototype on this device; not product integration or independent accuracy qualification'
    revision = (& git rev-parse HEAD).Trim(); started = Get-Date -Format o
    sourceSha256 = $hashes; samples = $Samples; sessions = $Sessions
    visibleSurface = [bool]$Present; suites = @(); metrics = @(); prototypeChecksPassed = $false
    stageStatus = 'in_progress'
}
$reports = @{}
$suites = @(
    @{ Name = 'reference'; Filter = 'visual'; Passed = 20; Ignored = 8 },
    @{ Name = 'transfer'; Filter = 'visual_transfer_progressive_worker_development_costs'; Passed = 1; Ignored = 0 },
    @{ Name = 'interleave'; Filter = 'visual_transfer_atlas_keeps_exact_pixels_while_drawing_and_sampling'; Passed = 1; Ignored = 0 },
    @{ Name = 'resource'; Filter = 'visual_transfer_cold_sessions_and_idle_resource_reference'; Passed = 1; Ignored = 0 },
    @{ Name = 'faults'; Filter = 'visual_transfer_formats_capacity_and_map_failures_preserve_lifecycle'; Passed = 1; Ignored = 0 }
)
$environmentNames = @('YUESHOT_VISUAL_P3_SAMPLES', 'YUESHOT_VISUAL_P3_SESSIONS', 'YUESHOT_VISUAL_P3_PRESENT')
$savedEnvironment = @{}
foreach ($name in $environmentNames) { $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
try {
    $env:YUESHOT_VISUAL_P3_SAMPLES = "$Samples"
    $env:YUESHOT_VISUAL_P3_SESSIONS = "$Sessions"
    if ($Present) { $env:YUESHOT_VISUAL_P3_PRESENT = '1' } else { Remove-Item -LiteralPath Env:YUESHOT_VISUAL_P3_PRESENT -ErrorAction SilentlyContinue }
    foreach ($suite in $suites) {
        $arguments = @('test', '-p', 'yue-shot-capture', '--release', '--locked', '--lib', $suite.Filter, '--', '--nocapture', '--test-threads=1')
        if ($suite.Name -ne 'reference') { $arguments += '--ignored' }
        $started = Get-Date -Format o
        $prior = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try { $output = & cargo @arguments 2>&1; $code = $LASTEXITCODE } finally { $ErrorActionPreference = $prior }
        $lines = @($output | ForEach-Object { $_.ToString() })
        $log = Join-Path $destination "$($suite.Name).txt"
        $lines | Set-Content -Encoding utf8 $log
        $last = $lines | Where-Object { $_ -match '^test result: ' } | Select-Object -Last 1
        $parsed = $last -match '^test result: ok\. (\d+) passed; 0 failed; (\d+) ignored'
        $valid = $code -eq 0 -and $parsed -and [int]$Matches[1] -eq $suite.Passed -and [int]$Matches[2] -eq $suite.Ignored
        foreach ($marker in @('VISUAL_PROGRESSIVE', 'VISUAL_TRANSFER', 'VISUAL_PRESENT', 'VISUAL_INTERLEAVE', 'VISUAL_TRANSFER_RESOURCE', 'VISUAL_TRANSFER_FAULTS')) {
            $line = $lines | Where-Object { $_.Contains("$marker ") } | Select-Object -Last 1
            if ($line) {
                $json = $line.Substring($line.IndexOf("$marker ") + $marker.Length + 1)
                $reports[$marker] = $json | ConvertFrom-Json
                $json | Set-Content -Encoding utf8 (Join-Path $destination "$marker.json")
            }
        }
        $summary.suites += [ordered]@{
            name = $suite.Name; command = 'cargo ' + ($arguments -join ' ')
            started = $started; finished = Get-Date -Format o; exitCode = $code
            expectedPassed = $suite.Passed; expectedIgnored = $suite.Ignored
            result = $last; passed = [bool]$valid
            logSha256 = (Get-FileHash -LiteralPath $log -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        $summary | ConvertTo-Json -Depth 12 | Set-Content -Encoding utf8 (Join-Path $destination 'summary.json')
        Write-Host "$($suite.Name): checks=$valid exit=$code"
    }
} finally {
    foreach ($name in $environmentNames) {
        if ($null -eq $savedEnvironment[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
    }
}
function P95($Values) {
    if (@($Values).Count -eq 0) { return [double]::PositiveInfinity }
    $sorted = @($Values | Sort-Object)
    return $sorted[[int][Math]::Ceiling($sorted.Count * 0.95) - 1]
}
$latency = $reports.ContainsKey('VISUAL_TRANSFER')
if ($latency) {
    foreach ($row in $reports.VISUAL_TRANSFER.rows) {
        $elapsed = P95 $row.warm_end_to_end_us
        $owner = P95 $row.owner_events_us
        $pass = $elapsed -le 25000 -and $owner -le 1000 -and $row.max_extra_bytes -le 1048576 -and $null -ne $row.cpu_buffer_bound_bytes -and $row.cpu_buffer_bound_bytes -le 16 * 1048576
        $latency = $latency -and $pass
        $summary.metrics += [ordered]@{ session = $row.session; fixture = $row.fixture; p95_us = $elapsed; owner_p95_us = $owner; cpuBufferBoundBytes = $row.cpu_buffer_bound_bytes; passed = [bool]$pass }
    }
    $latency = $latency -and @($reports.VISUAL_TRANSFER.rows).Count -eq 4 * $Sessions
}
$presentPass = $false
if ($reports.ContainsKey('VISUAL_PRESENT')) {
    $record = $reports.VISUAL_PRESENT
    $delta = (P95 $record.analysis_intervals_us) - (P95 $record.baseline_intervals_us)
    $presentPass = $delta -le 1000 -and @($record.analysis_intervals_us).Count -ge 100
    $summary.presentSubmissionP95Delta_us = $delta
    $summary.displayStatisticsAvailable = @($record.analysis_frame_statistics | Where-Object { $null -ne $_.present_count -and $_.present_count -gt 0 }).Count -gt 0
    $displayDelta = (P95 $record.analysis_display_intervals_us) - (P95 $record.baseline_display_intervals_us)
    $summary.displayedP95Delta_us = $displayDelta
    $presentPass = $presentPass -and @($record.baseline_display_intervals_us).Count -ge 1000 -and @($record.analysis_display_intervals_us).Count -ge 1000 -and $displayDelta -le 1000
}
$unchanged = $true
foreach ($key in $hashes.Keys) {
    $unchanged = $unchanged -and (Get-FileHash -LiteralPath (Join-Path $root $key) -Algorithm SHA256).Hash.ToLowerInvariant() -eq $hashes[$key]
}
$summary.sourceUnchanged = $unchanged
$summary.prototypeChecksPassed = $unchanged -and $latency -and $presentPass -and $Samples -ge 1000 -and $Sessions -ge 3 -and @($summary.suites | Where-Object { -not $_.passed }).Count -eq 0
$summary.finished = Get-Date -Format o
$summary | ConvertTo-Json -Depth 12 | Set-Content -Encoding utf8 (Join-Path $destination 'summary.json')
Write-Host "prototype checks=$($summary.prototypeChecksPassed); stage requires separate full regression, load/display qualification and engineering gate"
if (-not $summary.prototypeChecksPassed) { exit 1 }
