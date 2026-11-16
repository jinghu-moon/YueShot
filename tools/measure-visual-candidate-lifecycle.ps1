[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$EvidenceDirectory)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$out = [IO.Path]::GetFullPath((Join-Path $root $EvidenceDirectory))
if (Test-Path -LiteralPath $out) { throw 'Use a new evidence directory.' }
New-Item -ItemType Directory -Path $out | Out-Null
$build = & cargo test -p yue-shot-capture --locked --lib --no-run --message-format=json 2> (Join-Path $out 'build-stderr.txt')
$buildCode = $LASTEXITCODE
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
$arguments = @('--exact', 'windows::overlay::native_baseline::native_baseline_capture_lifecycle_resource_probe', '--ignored', '--test-threads=1', '--nocapture')
$stdout = Join-Path $out 'probe-stdout.txt'
$stderr = Join-Path $out 'probe-stderr.txt'
$started = Get-Date -Format o
$process = Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
$samples = @()
while (-not $process.HasExited) {
    $process.Refresh()
    if ($process.HasExited) { break }
    $phase = 'capture'
    if ((Get-Content -LiteralPath $stdout -Raw -ErrorAction SilentlyContinue) -match '"event":"idle"') { $phase = 'idle' }
    $samples += [ordered]@{
        utc = (Get-Date).ToUniversalTime().ToString('o'); phase = $phase
        privateBytes = $process.PrivateMemorySize64; workingSet = $process.WorkingSet64
        handles = $process.HandleCount; threads = $process.Threads.Count
        cpuSeconds = $process.TotalProcessorTime.TotalSeconds
    }
    Start-Sleep -Milliseconds 500
}
$process.WaitForExit()
$code = $process.ExitCode
$events = @(Get-Content -LiteralPath $stdout | Where-Object { $_ -match '^RESOURCE ' } | ForEach-Object { $_.Substring(9) | ConvertFrom-Json })
$cycles = @($events | Where-Object { $_.event -eq 'cycle' })
$idle = @($events | Where-Object { $_.event -eq 'idle' })
$passed = $code -eq 0 -and $cycles.Count -eq 105 -and $idle.Count -eq 61 -and @(Get-Content -LiteralPath $stdout | Where-Object { $_ -match '^test result: ok\. 1 passed; 0 failed; 0 ignored' }).Count -eq 1
$samples | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $out 'process-samples.json')
$events | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $out 'probe-events.json')
$idleSamples = @($samples | Where-Object { $_.phase -eq 'idle' })
[ordered]@{
    revision = (& git rev-parse HEAD); started = $started; finished = (Get-Date -Format o)
    command = "$exe $($arguments -join ' ')"; executableSha256 = (Get-FileHash -Algorithm SHA256 $exe).Hash.ToLowerInvariant()
    sourceSha256 = $sourceHashes
    exitCode = $code; gatePassed = $passed; warmups = 5; measuredSessions = $cycles.Count - 5
    gateScope = 'lifecycle assertions and event counts only; resource samples establish a baseline, not a post-upgrade resource qualification'
    idleSeconds = 60; idleSamples = $idleSamples.Count; gpuAccounting = 'DXGI per-process local/nonlocal usage, actual adapter recorded in probe-events.json'
    scope = 'real frozen capture, hidden renderer, confirmation/save/cancel; no visible HWND or message pump; process sampling includes test instrumentation'
    idleCpuSeconds = if ($idleSamples.Count -gt 1) { $idleSamples[-1].cpuSeconds - $idleSamples[0].cpuSeconds } else { $null }
    samplesSha256 = (Get-FileHash -Algorithm SHA256 (Join-Path $out 'process-samples.json')).Hash.ToLowerInvariant()
    stdoutSha256 = (Get-FileHash -Algorithm SHA256 $stdout).Hash.ToLowerInvariant()
} | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $out 'summary.json')
Write-Host "lifecycle: exit=$code sessions=$($cycles.Count - 5) idle=$($idle.Count) gate=$passed"
if (-not $passed) { exit 1 }
