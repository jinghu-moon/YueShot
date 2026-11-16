# Runs real captures, paired resource probes and visible test-surface competition.
# This does not automate product F5/F7/save/pin or certify the complete P4 gate.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$EvidenceDirectory,
    [Parameter(Mandatory = $true)][string]$SourceManifest
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$destination = if ([IO.Path]::IsPathRooted($EvidenceDirectory)) {
    [IO.Path]::GetFullPath($EvidenceDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path $root $EvidenceDirectory))
}
if (Test-Path -LiteralPath $destination) { throw 'Use a new evidence directory; previous evidence is immutable.' }
$source = Get-Content -LiteralPath $SourceManifest -Raw -Encoding utf8 | ConvertFrom-Json
if (-not $source.checksPassed -or -not $source.sourceUnchanged -or -not $source.sourceSha256) { throw 'A completed stable-source software batch is required.' }
foreach ($entry in $source.sourceSha256.PSObject.Properties) {
    if ((Get-FileHash -LiteralPath $entry.Name -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value) { throw "Source changed: $($entry.Name)" }
}
$batch = Join-Path $destination 'final-desktop-requalification'
New-Item -ItemType Directory -Path $batch | Out-Null
$report = [ordered]@{
    started = Get-Date -Format o; sourceManifest = $SourceManifest; sourceSha256 = $source.sourceSha256
    runnerSha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
    results = @(); checksPassed = $false; canAdvance = $false
}
function Record-Result([string]$Name, [string]$Command, [int]$Code, [string]$Log, [bool]$Passed) {
    $report.results += [ordered]@{
        name = $Name; command = $Command; exitCode = $Code; log = $Log
        sha256 = (Get-FileHash -LiteralPath $Log -Algorithm SHA256).Hash.ToLowerInvariant()
        gatePassed = $Passed; finished = Get-Date -Format o
    }
    $report | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $batch 'summary.json')
    Write-Host "$Name exit=$Code gate=$Passed"
    if (-not $Passed) { throw "Desktop subgate failed: $Name; raw evidence is retained." }
}
foreach ($name in @('providers_probe_and_capture_a_real_monitor_when_available', 'a_300x200_selection_reads_back_only_its_own_bytes', 'native_f5_worker_honors_capture_build_policy')) {
    $log = Join-Path $batch "$name.txt"
    $ErrorActionPreference = 'Continue'
    try {
        & cargo test -p yue-shot-capture --locked --lib $name -- --ignored --test-threads=1 2>&1 | Out-File -Encoding utf8 $log
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = 'Stop' }
    $result = Get-Content -LiteralPath $log -Encoding utf8 | Where-Object { $_ -match '^test result: ' } | Select-Object -Last 1
    Record-Result $name "cargo test -p yue-shot-capture --locked --lib $name -- --ignored --test-threads=1" $code $log ($code -eq 0 -and $result -match '^test result: ok\. 1 passed; 0 failed; 0 ignored')
}
foreach ($probe in @('controlled-control', 'controlled-positive')) {
    $directory = Join-Path $destination "qualified-$probe"
    $log = Join-Path $batch "$probe-runner.txt"
    $ErrorActionPreference = 'Continue'
    try {
        & powershell -NoProfile -File tools/measure-visual-candidate-lifecycle.ps1 -EvidenceDirectory $directory -Probe $probe -Release 2>&1 | Out-File -Encoding utf8 $log
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = 'Stop' }
    $measured = Get-Content -LiteralPath (Join-Path $directory 'summary.json') -Raw -Encoding utf8 | ConvertFrom-Json
    Record-Result $probe "tools/measure-visual-candidate-lifecycle.ps1 -EvidenceDirectory $directory -Probe $probe -Release" $code $log ($code -eq 0 -and $measured.gatePassed -and $measured.sourceUnchanged)
}
$previousEnvironment = @{}
foreach ($name in @('YUESHOT_P4_SAMPLES', 'YUESHOT_P4_SESSIONS', 'YUESHOT_P4_PRESENT')) { $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
try {
    $env:YUESHOT_P4_SAMPLES = '1000'
    $env:YUESHOT_P4_SESSIONS = '3'
    $env:YUESHOT_P4_PRESENT = '1'
    $log = Join-Path $batch 'product-4k-present.txt'
    $ErrorActionPreference = 'Continue'
    try {
        & cargo test -p yue-shot-capture --release --locked --lib visual_replay_product_controller_latency_diagnostic -- --ignored --nocapture --test-threads=1 2>&1 | Out-File -Encoding utf8 $log
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = 'Stop' }
    $result = Get-Content -LiteralPath $log -Encoding utf8 | Where-Object { $_ -match '^test result: ' } | Select-Object -Last 1
    Record-Result 'product-4k-present' 'YUESHOT_P4_SAMPLES=1000 YUESHOT_P4_SESSIONS=3 YUESHOT_P4_PRESENT=1 cargo test -p yue-shot-capture --release --locked --lib visual_replay_product_controller_latency_diagnostic -- --ignored --nocapture --test-threads=1' $code $log ($code -eq 0 -and $result -match '^test result: ok\. 1 passed; 0 failed; 0 ignored')
} finally {
    foreach ($name in $previousEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], 'Process') }
}
$report.sourceUnchanged = $true
foreach ($entry in $source.sourceSha256.PSObject.Properties) {
    if ((Get-FileHash -LiteralPath $entry.Name -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value) { $report.sourceUnchanged = $false }
}
$report.finished = Get-Date -Format o
$report.checksPassed = $report.sourceUnchanged -and @($report.results | Where-Object { -not $_.gatePassed }).Count -eq 0
$report | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $batch 'summary.json')
if (-not $report.checksPassed) { exit 1 }
