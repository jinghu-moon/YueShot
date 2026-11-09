param(
    [ValidateSet('Auto','StartManual','FinishManual')][string]$Mode = 'Auto'
)
$ErrorActionPreference = 'Stop'
$kitRoot = $PSScriptRoot
$expected = Get-Content -LiteralPath (Join-Path $kitRoot 'expected.json') -Raw | ConvertFrom-Json
$work = Join-Path $kitRoot 'results'
$program = Join-Path $work 'program'
$data = Join-Path $work 'data'
$exe = Join-Path $program 'YueShot.exe'
$registration = 'HKCU:/' + $expected.installation.uninstallKey
$reportPath = Join-Path $work ($Mode + '.json')
$report = [ordered]@{ format=1; mode=$Mode; version=$expected.version; channel=$expected.channel; buildId=$expected.buildId; status='running'; checks=@(); manualQualification='pending' }

function Record([string]$Check) { $script:report.checks += $Check }
function Run-Checked([string]$File, [string[]]$Arguments) {
    $process = Start-Process -FilePath $File -ArgumentList $Arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Process failed with exit $($process.ExitCode)" }
}
function Check-Payload {
    foreach ($entry in $expected.payload.PSObject.Properties) {
        $file = Join-Path $program $entry.Name
        if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value) { throw "Installed payload hash differs: $($entry.Name)" }
    }
    $actual = Get-ItemProperty -LiteralPath $registration
    if ($actual.DisplayName -cne $expected.installation.displayName -or $actual.DisplayVersion -cne $expected.version) { throw 'Installation registration differs' }
    Record 'installed payload hashes and channel registration'
}
function Uninstall-TestCopy {
    # Refuse cleanup while any real product is active; never kill user processes.
    if (Get-Process YueShot -ErrorAction SilentlyContinue) { throw 'Exit YueShot using its Exit button before finishing' }
    Run-Checked (Join-Path $program 'uninstall.exe') @('/S')
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (((Test-Path -LiteralPath $program) -or (Test-Path -LiteralPath $registration)) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
    if ((Test-Path -LiteralPath $program) -or (Test-Path -LiteralPath $registration)) { throw 'Test uninstall left product files or registration' }
    if (-not (Test-Path -LiteralPath (Join-Path $data 'settings.json'))) { throw 'Test uninstall deleted settings' }
    Record 'uninstall removed owned program/registration and retained test data'
}

try {
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run this acceptance in a normal, non-elevated user session' }
    if (-not [Environment]::Is64BitOperatingSystem) { throw 'Windows x64 is required' }
    $report.osBuild = [Environment]::OSVersion.Version.ToString()
    $report.elevated = $false
    if (-not (Test-Path -LiteralPath $work)) { New-Item -ItemType Directory -Path $work | Out-Null }
    if (Test-Path -LiteralPath $reportPath) { throw 'Preserve existing results; extract a fresh kit for a new acceptance run' }
    Record 'non-elevated Windows x64 session'
    $installer = Join-Path $kitRoot $expected.installer
    if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected.installerSha256) { throw 'Installer checksum mismatch' }
    Record 'installer bytes verified (integrity check, not update authentication)'
    if (Get-Process YueShot -ErrorAction SilentlyContinue) { throw 'Exit existing YueShot normally first' }

    if ($Mode -eq 'FinishManual') {
        if (-not (Test-Path -LiteralPath (Join-Path $work 'manual-state.json'))) { throw 'Run StartManual first' }
        $state = Get-Content -LiteralPath (Join-Path $work 'manual-state.json') -Raw | ConvertFrom-Json
        if ($state.buildId -cne $expected.buildId) { throw 'Manual session belongs to a different build' }
        Check-Payload
        # Read-only snapshots establish that uninstall preserves actual user-created artifacts.
        $before = @{}
        foreach ($file in Get-ChildItem -LiteralPath $data -Recurse -File -Force) { $before[$file.FullName] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash }
        Uninstall-TestCopy
        foreach ($file in $before.Keys) {
            if (-not (Test-Path -LiteralPath $file) -or (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $before[$file]) { throw 'Uninstall changed retained test data' }
        }
        Record 'all manual-session data bytes retained'
    } else {
        if ((Test-Path -LiteralPath $registration) -or (Test-Path -LiteralPath $program)) { throw 'Preserve existing channel installation; use a clean machine/profile' }
        # /D must be final; only this kit-owned test directory is installed/uninstalled.
        Run-Checked $installer @('/S','/NS',"/D=$program")
        Check-Payload
        Run-Checked $exe @('--smoke-test','--data-dir',('"'+$data+'"'))
        if (-not (Test-Path -LiteralPath (Join-Path $data 'settings.json'))) { throw 'Smoke produced no settings' }
        Record 'installed exe loaded its runtime and read/wrote isolated settings/storage'
        if ($Mode -eq 'StartManual') {
            @{ buildId=$expected.buildId } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'manual-state.json') -Encoding UTF8
            Start-Process -FilePath $exe -ArgumentList @('--show','--data-dir',('"'+$data+'"')) -WindowStyle Normal | Out-Null
            Record 'opened installed GUI with isolated manual-test data'
            $report.status = 'awaiting-manual-tests'
        } else {
            # Same-version reinstallation must preserve configuration.
            $settings = Join-Path $data 'settings.json'
            $before = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash
            Run-Checked $installer @('/S','/NS',"/D=$program")
            Check-Payload
            if ((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -ne $before) { throw 'Reinstallation changed settings' }
            Record 'same-version reinstallation preserved settings'
            Uninstall-TestCopy
        }
    }
    if ($report.status -eq 'running') { $report.status = 'passed-automated-checks' }
} catch {
    $report.status = 'failed-or-blocked'
    # Do not export exception paths, account names or captured content.
    $report.error = $_.Exception.Message -replace [regex]::Escape($kitRoot), '<kit>'
    Write-Error -Message $report.error -ErrorAction Continue
} finally {
    if (-not (Test-Path -LiteralPath $work)) { New-Item -ItemType Directory -Path $work | Out-Null }
    if (-not (Test-Path -LiteralPath $reportPath)) {
        $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $reportPath -Encoding UTF8
    }
}
Write-Output ($report | ConvertTo-Json -Depth 8)
if ($report.status -eq 'failed-or-blocked') { exit 1 }
