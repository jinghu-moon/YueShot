# P0/P1 evidence runner. Native probes never register global hotkeys or write the clipboard.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$EvidenceDirectory,
    [switch]$Native
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$evidencePath = if ([IO.Path]::IsPathRooted($EvidenceDirectory)) {
    [IO.Path]::GetFullPath($EvidenceDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path $root $EvidenceDirectory))
}
if (Test-Path -LiteralPath $evidencePath) {
    throw "Use a new evidence directory; do not overwrite prior results: $evidencePath"
}
New-Item -ItemType Directory -Path $evidencePath | Out-Null
$suites = @(
    @{ Name = 'capture'; Arguments = @('test', '-p', 'yue-shot-capture', '--locked', '--lib', '--', '--test-threads=1') },
    @{ Name = 'media'; Arguments = @('test', '-p', 'yue-media', '--locked', '--lib') },
    @{ Name = 'geometry'; Arguments = @('test', '-p', 'yue-geometry', '--locked', '--lib') },
    @{ Name = 'native-baseline'; Arguments = @('test', '-p', 'yue-shot-capture', '--locked', '--lib', 'native_baseline', '--', '--test-threads=1') }
)
if ($Native) {
    foreach ($name in @(
        'native_baseline_controller_preview_replacement_and_manual_path_trace',
        'the_context_owner_assertion_is_the_invariant_we_actually_hold',
        'async_sample_from_an_uploaded_texture_lands_within_a_few_ticks',
        'fp16_region_readback_is_typed_and_keeps_exact_source_bits',
        'hdr_sampler_returns_explicit_sdr_tiles_without_changing_the_source',
        'window_snap_hints_are_painted_but_never_exported',
        'providers_probe_and_capture_a_real_monitor_when_available',
        'a_300x200_selection_reads_back_only_its_own_bytes',
        'native_f5_worker_honors_capture_build_policy'
    )) {
        $suites += @{ Name = $name; ExactOne = $true; Arguments = @('test', '-p', 'yue-shot-capture', '--locked', '--lib', $name, '--', '--ignored', '--test-threads=1') }
    }
}
$results = @()
$revision = (& git rev-parse HEAD).Trim()
$dirty = @(& git status --porcelain)
$rustVersion = & rustc --version
$sourcePaths = @(
    Get-ChildItem -LiteralPath 'apps/yue-shot/crates/yue-shot-capture/src', 'crates/yue-media/src', 'crates/yue-geometry/src' -Filter '*.rs' -Recurse -File
    Get-Item -LiteralPath 'Cargo.lock', 'docs/design/visual-candidate-test-cases.html', 'docs/visual-candidate-pipeline.md', $PSCommandPath
)
$sourceHashes = [ordered]@{}
foreach ($sourceFile in ($sourcePaths | Sort-Object FullName -Unique)) {
    $relativePath = $sourceFile.FullName.Substring($root.Length + 1).Replace('\', '/')
    $sourceHashes[$relativePath] = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceFile.FullName).Hash.ToLowerInvariant()
}
foreach ($suite in $suites) {
    $started = Get-Date -Format o
    $wasStrict = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = & cargo @($suite.Arguments) 2>&1
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $wasStrict
    }
    $lines = @($output | ForEach-Object { $_.ToString() })
    $logName = "$($suite.Name).txt"
    $lines | Out-File -Encoding utf8 (Join-Path $evidencePath $logName)
    # Last result wins: apartment tests launch a child whose result appears earlier.
    $resultLine = $lines | Where-Object { $_ -match '^test result: ' } | Select-Object -Last 1
    $parsed = $resultLine -match '^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored'
    $passed = 0; $failed = 0; $ignored = 0
    if ($parsed) {
        $passed = [int]$Matches[2]; $failed = [int]$Matches[3]; $ignored = [int]$Matches[4]
    }
    $valid = $code -eq 0 -and $parsed -and $passed -gt 0 -and $failed -eq 0
    if ($suite.ExactOne) { $valid = $valid -and $passed -eq 1 -and $ignored -eq 0 }
    $results += [ordered]@{
        name = $suite.Name; command = 'cargo ' + ($suite.Arguments -join ' ')
        started = $started; finished = (Get-Date -Format o); exitCode = $code
        passed = $passed; failed = $failed; ignored = $ignored
        gatePassed = [bool]$valid; log = $logName
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $evidencePath $logName)).Hash.ToLowerInvariant()
    }
    [ordered]@{
        revision = $revision; workingTree = $dirty; rustc = "$rustVersion"
        os = [Environment]::OSVersion.ToString(); nativeRequested = [bool]$Native
        sourceSha256 = $sourceHashes
        results = $results
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $evidencePath 'summary.json')
    Write-Host "$($suite.Name): exit=$code passed=$passed failed=$failed ignored=$ignored gate=$valid"
}
if (@($results | Where-Object { -not $_.gatePassed }).Count -gt 0) { exit 1 }
