param([Parameter(Mandatory=$true)][string]$Candidate)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
$candidateRoot = (Resolve-Path -LiteralPath $Candidate).Path
$allowedRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot 'target/yueshot-delivery')) + [IO.Path]::DirectorySeparatorChar
if (-not $candidateRoot.StartsWith($allowedRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Install smoke candidates must be inside target/yueshot-delivery.'
}
if (Get-Process YueShot -ErrorAction SilentlyContinue) {
    throw 'Exit YueShot normally before the isolated install smoke test.'
}
$manifest = Get-Content -LiteralPath (Join-Path $candidateRoot 'delivery.json') -Raw | ConvertFrom-Json
$identity = $manifest.installation
if ($identity.channel -notin @('dev','beta') -or $identity.installationId -ne "com.seeyuer.yueshot.$($identity.channel).x64") { throw 'Expected a reviewed dev/beta channel identity' }
$registration = 'HKCU:/' + $identity.uninstallKey
if (Test-Path -LiteralPath $registration) {
    throw 'An existing installation of this channel must be uninstalled normally first.'
}
function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit $LASTEXITCODE" }
}
Invoke-Checked python @('tools/yueshot-delivery.py','verify',$candidateRoot)
$installRoot = Join-Path $candidateRoot 'installed-runtime-smoke'
$dataRoot = Join-Path $candidateRoot 'installed-runtime-smoke-data'
if ((Test-Path -LiteralPath $installRoot) -or (Test-Path -LiteralPath $dataRoot)) {
    throw 'Preserve previous smoke evidence and use a new candidate directory.'
}
$installers = @(Get-ChildItem -LiteralPath (Join-Path $candidateRoot 'nsis') -Filter '*.exe' -File)
if ($installers.Count -ne 1) { throw 'Expected exactly one NSIS installer.' }
# NSIS requires /D to be the final argument and consumes its remaining text as the path.
$installerProcess = Start-Process -FilePath $installers[0].FullName -ArgumentList @('/S','/NS',"/D=$installRoot") -WindowStyle Hidden -Wait -PassThru
if ($installerProcess.ExitCode -ne 0) { throw "Installer exited $($installerProcess.ExitCode)" }
$exe = Join-Path $installRoot 'YueShot.exe'
foreach ($entry in $manifest.files.PSObject.Properties) {
    if ($entry.Name.StartsWith('bin/')) {
        $installed = Join-Path $installRoot $entry.Name.Substring(4)
        if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value) {
            throw "Installed payload differs: $($entry.Name)"
        }
    }
}
Invoke-Checked python @('tools/yueshot_runtime.py',$exe,'--report',(Join-Path $candidateRoot 'installed-runtime-audit.json'))
Invoke-Checked python @('tools/yueshot_icons.py',(Join-Path $projectRoot 'apps/yue-shot/packaging/YueShot.ico'),$exe,(Join-Path $installRoot 'uninstall.exe'))
Invoke-Checked python @('tools/yueshot_installer.py',$installers[0].FullName,(Join-Path $installRoot 'uninstall.exe'))
$smokeProcess = Start-Process -FilePath $exe -ArgumentList @('--smoke-test','--data-dir',('"' + $dataRoot + '"')) -WindowStyle Hidden -Wait -PassThru
if ($smokeProcess.ExitCode -ne 0) { throw "Installed smoke exited $($smokeProcess.ExitCode)" }
if (-not (Test-Path -LiteralPath (Join-Path $dataRoot 'settings.json'))) { throw 'Installed smoke did not create settings.' }
$uninstallerProcess = Start-Process -FilePath (Join-Path $installRoot 'uninstall.exe') -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
if ($uninstallerProcess.ExitCode -ne 0) { throw "Uninstaller exited $($uninstallerProcess.ExitCode)" }
# NSIS may delegate removal to its temporary uninstaller. Observe completion; never kill it.
$deadline = [DateTime]::UtcNow.AddSeconds(30)
while ((Test-Path -LiteralPath $installRoot) -and [DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 200
}
if (Test-Path -LiteralPath $installRoot) { throw 'Uninstall left the installation directory behind.' }
if (Test-Path -LiteralPath $registration) { throw 'Uninstall left its registration behind.' }
if (-not (Test-Path -LiteralPath (Join-Path $dataRoot 'settings.json'))) { throw 'Uninstall removed user data.' }
Write-Output 'INSTALLED_RUNTIME_SMOKE=PASS (payload hashes, x64 import closure, installed exe smoke, uninstall, data retained)'
