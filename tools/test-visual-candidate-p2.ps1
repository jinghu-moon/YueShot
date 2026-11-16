# P2 reference evidence, not the complete stage gate. No desktop capture or input.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$EvidenceDirectory,
    [switch]$Measure
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$destination = [IO.Path]::GetFullPath((Join-Path $root $EvidenceDirectory))
if (Test-Path -LiteralPath $destination) { throw 'Use a new evidence directory; previous evidence is immutable.' }
New-Item -ItemType Directory -Path $destination | Out-Null
$paths = @(
    Get-ChildItem apps/yue-shot/crates/yue-shot-capture/src/visual_detection -File
    Get-Item apps/yue-shot/crates/yue-shot-capture/src/windows/win/d3d11/visual_readback.rs
    Get-Item apps/yue-shot/crates/yue-shot-capture/tests/fixtures/visual-candidate-v4.1/dataset-lock.json
    Get-Item apps/yue-shot/crates/yue-shot-capture/tests/fixtures/visual-candidate-8k-v4/dataset-lock.json
    Get-Item Cargo.lock, apps/yue-shot/crates/yue-shot-capture/Cargo.toml, docs/design/visual-candidate-test-cases.html, $PSCommandPath
)
$hashes = [ordered]@{}
foreach ($path in $paths) {
    $hashes[$path.FullName.Substring($root.Length + 1).Replace('\', '/')] = (Get-FileHash -LiteralPath $path.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}
$arguments = @('test', '-p', 'yue-shot-capture', '--release', '--locked', '--lib', 'visual', '--', '--nocapture', '--test-threads=1')
if ($Measure) { $arguments += '--include-ignored' }
$started = Get-Date -Format o
$prior = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try { $output = & cargo @arguments 2>&1; $code = $LASTEXITCODE } finally { $ErrorActionPreference = $prior }
$lines = @($output | ForEach-Object { $_.ToString() })
$log = Join-Path $destination 'reference-tests.txt'
$lines | Set-Content -Encoding utf8 $log
$reports = [ordered]@{}
foreach ($marker in @('VISUAL_COMPARISON', 'VISUAL_STAGE', 'VISUAL_LOCATION', 'VISUAL_CPU_TIMING', 'VISUAL_READBACK','VISUAL_WORKER', 'VISUAL_DEMAND', 'VISUAL_BOUNDED_READBACK')) {
    $line = $lines | Where-Object { $_.Contains("$marker ") } | Select-Object -Last 1
    if ($line) {
        $json = $line.Substring($line.IndexOf("$marker ") + $marker.Length + 1)
        $reports[$marker] = $json | ConvertFrom-Json
        $json | Set-Content -Encoding utf8 (Join-Path $destination "$marker.json")
    }
}
$last = $lines | Where-Object { $_ -match '^test result: ' } | Select-Object -Last 1
$parsed = $last -match '^test result: ok\. (\d+) passed; 0 failed; (\d+) ignored'
$passed = if ($parsed) { [int]$Matches[1] } else { 0 }
$ignored = if ($parsed) { [int]$Matches[2] } else { -1 }
$routes = @()
if ($reports.Contains('VISUAL_COMPARISON')) {
    foreach ($route in @('A', 'B')) {
        $queries = @($reports['VISUAL_COMPARISON'].rows | Where-Object { $_.route -eq $route -and $_.applicable })
        $routes += [ordered]@{
            route = $route; hardQueries = $queries.Count
            geometryPassed = @($queries | Where-Object query_ok).Count
            complete = @($queries | Where-Object { $_.detection.completion -eq 'Complete' }).Count
            failedQueries = @($queries | Where-Object { -not $_.query_ok } | ForEach-Object query)
        }
    }
}
$valid = $code -eq 0 -and $passed -eq $(if ($Measure) { 22 } else { 18 }) -and $reports.Contains('VISUAL_COMPARISON') -and $reports.Contains('VISUAL_STAGE') -and $reports.Contains('VISUAL_LOCATION') -and $reports.Contains('VISUAL_DEMAND')
if ($Measure) { $valid = $valid -and $ignored -eq 0 -and $reports.Contains('VISUAL_CPU_TIMING') -and $reports.Contains('VISUAL_READBACK') -and $reports.Contains('VISUAL_WORKER') -and $reports.Contains('VISUAL_BOUNDED_READBACK') }
$summary = [ordered]@{
    scope = 'P2 reference experiments only; not complete pipeline or stage qualification'
    revision = (& git rev-parse HEAD).Trim(); workingTree = @(& git status --porcelain)
    command = 'cargo ' + ($arguments -join ' '); started = $started; finished = Get-Date -Format o
    rustc = (& rustc --version); exitCode = $code; passed = $passed; ignored = $ignored
    referenceChecksPassed = [bool]$valid; stageStatus = 'in_progress'
    sourceSha256 = $hashes; logSha256 = (Get-FileHash -LiteralPath $log -Algorithm SHA256).Hash.ToLowerInvariant()
    routes = $routes; measured = [bool]$Measure
    remaining = @('applicable contract coverage audit and profile/backend decision', 'production asynchronous bounded transport and UI contention qualification remain P3/P4', 'independent real qualification remains P4/P5')
}
$summary | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 (Join-Path $destination 'reference-summary.json')
Write-Host "reference checks=$valid passed=$passed ignored=$ignored; P2 remains in progress"
if (-not $valid) { exit 1 }
