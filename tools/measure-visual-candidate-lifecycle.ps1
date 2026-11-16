[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$EvidenceDirectory,
    [ValidateSet('baseline', 'visual', 'controlled-positive', 'controlled-control')]
    [string]$Probe = 'baseline',
    [switch]$Release
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$out = if ([IO.Path]::IsPathRooted($EvidenceDirectory)) {
    [IO.Path]::GetFullPath($EvidenceDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path $root $EvidenceDirectory))
}
if (Test-Path -LiteralPath $out) { throw 'Use a new evidence directory.' }
New-Item -ItemType Directory -Path $out | Out-Null
$buildArguments = @('test', '-p', 'yue-shot-capture', '--locked', '--lib', '--no-run', '--message-format=json')
if ($Release) { $buildArguments += '--release' }
$ErrorActionPreference = 'Continue'
try {
    $build = & cargo @buildArguments 2> (Join-Path $out 'build-stderr.txt')
    $buildCode = $LASTEXITCODE
} finally { $ErrorActionPreference = 'Stop' }
$build | Set-Content -Encoding utf8 (Join-Path $out 'build.jsonl')
if ($buildCode -ne 0) { throw "Build failed: $buildCode" }
$artifacts = @($build | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'yue_shot_capture' -and $_.executable })
if ($artifacts.Count -ne 1) { throw 'Expected exactly one capture test binary.' }
$exe = $artifacts[0].executable
$sourceHashes = [ordered]@{}
foreach ($sourceFile in Get-ChildItem apps/yue-shot/crates/yue-shot-capture/src -Recurse -File | Sort-Object FullName) {
    $relativePath = $sourceFile.FullName.Substring($root.Length + 1).Replace('\', '/')
    $sourceHashes[$relativePath] = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceFile.FullName).Hash.ToLowerInvariant()
}
$testName = switch ($Probe) {
    'baseline' { 'native_baseline_capture_lifecycle_resource_probe' }
    'visual' { 'visual_replay_capture_lifecycle_resource_probe' }
    'controlled-positive' { 'visual_replay_controlled_capture_lifecycle_resource_probe' }
    'controlled-control' { 'visual_replay_controlled_capture_without_analysis_resource_probe' }
}
$sourceHashes['tools/measure-visual-candidate-lifecycle.ps1'] = (Get-FileHash -Algorithm SHA256 -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
$sourceHashes['Cargo.lock'] = (Get-FileHash -Algorithm SHA256 -LiteralPath 'Cargo.lock').Hash.ToLowerInvariant()
$arguments = @('--exact', "windows::overlay::native_baseline::$testName", '--ignored', '--test-threads=1', '--nocapture')
$stdout = Join-Path $out 'probe-stdout.txt'
$stderr = Join-Path $out 'probe-stderr.txt'
$started = Get-Date -Format o
$process = Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
# Keep the process handle alive so Windows PowerShell retains the native exit code.
$processHandle = $process.Handle
if ($processHandle -eq [IntPtr]::Zero) { throw 'Failed to retain probe process handle.' }
$samples = @()
while (-not $process.HasExited) {
    $process.Refresh()
    if ($process.HasExited) { break }
    $phase = 'capture'
    if ((Get-Content -LiteralPath $stdout -Raw -ErrorAction SilentlyContinue) -match '"event":"idle"') { $phase = 'idle' }
    $samples += [ordered]@{
        utc = (Get-Date).ToUniversalTime().ToString('o'); phase = $phase
        privateBytes = $process.PrivateMemorySize64; workingSet = $process.WorkingSet64
        peakWorkingSet = $process.PeakWorkingSet64; peakPagedMemory = $process.PeakPagedMemorySize64
        handles = $process.HandleCount; threads = $process.Threads.Count
        cpuSeconds = $process.TotalProcessorTime.TotalSeconds
    }
    Start-Sleep -Milliseconds 500
}
$process.WaitForExit()
$code = $process.ExitCode
if ($null -eq $code) { throw 'Probe exit code unavailable; this run cannot qualify.' }
$events = @(Get-Content -LiteralPath $stdout | Where-Object { $_ -match '^RESOURCE ' } | ForEach-Object { $_.Substring(9) | ConvertFrom-Json })
$cycles = @($events | Where-Object { $_.event -eq 'cycle' })
$idle = @($events | Where-Object { $_.event -eq 'idle' })
$visual = @($events | Where-Object { $_.event -eq 'visual' })
$shutdown = @($events | Where-Object { $_.event -eq 'shutdown' })
$passed = $code -eq 0 -and $cycles.Count -eq 105 -and $idle.Count -eq 61 -and @(Get-Content -LiteralPath $stdout | Where-Object { $_ -match '^test result: ok\. 1 passed; 0 failed; 0 ignored' }).Count -eq 1
if ($Probe -in @('visual', 'controlled-positive')) {
    $passed = $passed -and $visual.Count -eq 105 -and $shutdown.Count -eq 1 -and $shutdown[0].visual_workers_balanced
}
if ($Probe -eq 'controlled-positive') {
    $passed = $passed -and @($visual | Where-Object { $_.completed -ne 1 -or $_.unavailable -ne 0 }).Count -eq 0
}
$sourceUnchanged = $true
foreach ($name in $sourceHashes.Keys) {
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $root $name)).Hash.ToLowerInvariant() -ne $sourceHashes[$name]) { $sourceUnchanged = $false }
}
$passed = $passed -and $sourceUnchanged
$samples | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $out 'process-samples.json')
$events | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $out 'probe-events.json')
$idleSamples = @($samples | Where-Object { $_.phase -eq 'idle' })
[ordered]@{
    revision = (& git rev-parse HEAD); started = $started; finished = (Get-Date -Format o)
    command = "$exe $($arguments -join ' ')"; executableSha256 = (Get-FileHash -Algorithm SHA256 $exe).Hash.ToLowerInvariant()
    sourceSha256 = $sourceHashes; sourceUnchanged = $sourceUnchanged; probe = $Probe; release = [bool]$Release
    exitCode = $code; gatePassed = $passed; warmups = 5; measuredSessions = $cycles.Count - 5
    gateScope = 'lifecycle assertions, event counts and unchanged source; raw resource samples require paired plateau/peak analysis, not complete P4 qualification'
    idleSeconds = 60; idleSamples = $idleSamples.Count; gpuAccounting = 'DXGI per-process local/nonlocal usage, actual adapter recorded in probe-events.json'
    scope = 'real frozen capture, hidden capture renderer and controller confirmation/save handoff/cancel; controlled probes additionally show an independent D2D host; not visible product UI interaction; process sampling includes test instrumentation'
    idleCpuSeconds = if ($idleSamples.Count -gt 1) { $idleSamples[-1].cpuSeconds - $idleSamples[0].cpuSeconds } else { $null }
    samplesSha256 = (Get-FileHash -Algorithm SHA256 (Join-Path $out 'process-samples.json')).Hash.ToLowerInvariant()
    stdoutSha256 = (Get-FileHash -Algorithm SHA256 $stdout).Hash.ToLowerInvariant()
} | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $out 'summary.json')
Write-Host "lifecycle: exit=$code sessions=$($cycles.Count - 5) idle=$($idle.Count) gate=$passed"
if (-not $passed) { exit 1 }
