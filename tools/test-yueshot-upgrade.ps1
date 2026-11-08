param([Parameter(Mandatory=$true)][string]$From,[Parameter(Mandatory=$true)][string]$To)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
$allowedRoot=[IO.Path]::GetFullPath((Join-Path $projectRoot 'target/yueshot-delivery'))+[IO.Path]::DirectorySeparatorChar
$candidates=@()
foreach ($path in @($From,$To)) {
    $root=(Resolve-Path -LiteralPath $path).Path
    if (-not $root.StartsWith($allowedRoot,[StringComparison]::OrdinalIgnoreCase)) { throw 'Use local delivery candidates' }
    & python tools/yueshot-delivery.py verify $root
    if ($LASTEXITCODE -ne 0) { throw 'Candidate verification failed' }
    $candidates+=@{root=$root;manifest=(Get-Content -LiteralPath (Join-Path $root 'delivery.json') -Raw | ConvertFrom-Json)}
}
$a=$candidates[0];$b=$candidates[1]
if ([Version]$b.manifest.version -le [Version]$a.manifest.version) { throw 'Use two genuine increasing product versions' }
$identity=$a.manifest.installation
if ($identity.channel -notin @('dev','beta') -or $identity.installationId -ne $b.manifest.installation.installationId) { throw 'Upgrade requires the same dev/beta installation identity' }
$registry='HKCU:/'+$identity.uninstallKey
if ((Test-Path -LiteralPath $registry) -or (Get-Process YueShot -ErrorAction SilentlyContinue)) { throw 'Normally exit YueShot and uninstall this channel before testing' }
$work=Join-Path $b.root 'installed-upgrade-check'
if (Test-Path -LiteralPath $work) { throw 'Preserve existing evidence; use a fresh candidate' }
New-Item -ItemType Directory -Path $work -ErrorAction Stop | Out-Null
$installRoot=Join-Path $work 'program'
$dataRoot=Join-Path $work 'data'
$backup=Join-Path $work 'backup'
$png=Join-Path $work 'one-pixel.png'
$exe=Join-Path $installRoot 'YueShot.exe'
& python tools/yueshot_lifecycle_fixture.py png $png
if ($LASTEXITCODE -ne 0) { throw 'Create one-pixel PNG failed' }
function Install($Candidate,[int]$Expected=0) {
    $installers=@(Get-ChildItem -LiteralPath (Join-Path $Candidate.root 'nsis') -File -Filter '*.exe')
    if ($installers.Count -ne 1) { throw 'Expected one installer' }
    $process=Start-Process -FilePath $installers[0].FullName -ArgumentList @('/S','/NS',"/D=$installRoot") -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne $Expected) { throw "Install exit $($process.ExitCode); expected $Expected" }
}
function Run-Product([string[]]$Arguments,[int]$Expected=0) {
    $process=Start-Process -FilePath $exe -ArgumentList $Arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne $Expected) { throw "Product exit $($process.ExitCode); expected $Expected" }
}
function Smoke([int]$Expected=0) { Run-Product @('--smoke-test','--data-dir',('"'+$dataRoot+'"')) $Expected }
function Fixture([string]$Mode) {
    & cargo run -p yue-shot-storage --example lifecycle_fixture --locked -- $Mode $dataRoot $png
    if ($LASTEXITCODE -ne 0) { throw 'Real storage fixture failed' }
}
function Assert-Version($Candidate) {
    $output=Join-Path $work ('version-'+$Candidate.manifest.version+'.txt')
    $process=Start-Process -FilePath $exe -ArgumentList '--version' -RedirectStandardOutput $output -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0 -or (Get-Content -LiteralPath $output -Raw).Trim() -ne "YueShot $($Candidate.manifest.version) ($($identity.channel), windows-x64)") { throw 'Installed executable version mismatch' }
    $expected=$Candidate.manifest.files.'bin/YueShot.exe'
    if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'Installed executable bytes mismatch' }
}
Install $a
Assert-Version $a
Smoke
Fixture 'seed'
$settingsPath=Join-Path $dataRoot 'settings.json'
$settings=Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json
$settings.closeToTray=$false;$settings.showCoordinates=$true
[IO.File]::WriteAllText($settingsPath,($settings | ConvertTo-Json -Depth 12),(New-Object Text.UTF8Encoding($false)))
Smoke
Fixture 'verify'
Run-Product @('--backup-to',('"'+$backup+'"'),'--data-dir',('"'+$dataRoot+'"'))
$before=@{}
foreach ($file in Get-ChildItem -LiteralPath $dataRoot -File -Recurse -Force) { $before[$file.FullName]=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash }
Install $b
Assert-Version $b
foreach ($path in $before.Keys) { if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $before[$path]) { throw 'Installer changed user data' } }
Smoke
Fixture 'verify'
$settings=Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json
if ($settings.closeToTray -or -not $settings.showCoordinates) { throw 'Upgrade lost customized configuration' }
# Genuine A installer must reject a genuine installed B, preserving its binary and data.
Install $a 12
Assert-Version $b
Fixture 'verify'

# Future database schema must be refused without modifying that database.
$db=Join-Path $dataRoot 'capture-history/data/yue-shot-history.db'
& python tools/yueshot_lifecycle_fixture.py future-schema $db
if ($LASTEXITCODE -ne 0) { throw 'Create future-schema fixture failed' }
$futureHash=(Get-FileHash -LiteralPath $db -Algorithm SHA256).Hash
Smoke 1
if ((Get-FileHash -LiteralPath $db -Algorithm SHA256).Hash -ne $futureHash) { throw 'Future schema was modified' }
# Preserve failed data as evidence; restore the original A backup into the same canonical root.
$failed=Join-Path $work 'future-schema-data'
Move-Item -LiteralPath $dataRoot -Destination $failed -ErrorAction Stop
Run-Product @('--restore-from',('"'+$backup+'"'),'--data-dir',('"'+$dataRoot+'"'))
Smoke
Fixture 'verify'
$restored=Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json
if ($restored.closeToTray -or -not $restored.showCoordinates) { throw 'Backup restore lost customized settings' }
$uninstall=Start-Process -FilePath (Join-Path $installRoot 'uninstall.exe') -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw 'Normal uninstall launcher failed' }
$deadline=[DateTime]::UtcNow.AddSeconds(30)
while (((Test-Path -LiteralPath $installRoot) -or (Test-Path -LiteralPath $registry)) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
if ((Test-Path -LiteralPath $installRoot) -or (Test-Path -LiteralPath $registry) -or -not (Test-Path -LiteralPath $settingsPath)) { throw 'Uninstall did not preserve expected state' }
Install $b
Assert-Version $b
Smoke
Fixture 'verify'
$uninstall=Start-Process -FilePath (Join-Path $installRoot 'uninstall.exe') -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw 'Final uninstall launcher failed' }
$deadline=[DateTime]::UtcNow.AddSeconds(30)
while (((Test-Path -LiteralPath $installRoot) -or (Test-Path -LiteralPath $registry)) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
if ((Test-Path -LiteralPath $installRoot) -or (Test-Path -LiteralPath $registry)) { throw 'Final uninstall incomplete' }
Write-Output "INSTALLED_UPGRADE=PASS ($($a.manifest.version) -> $($b.manifest.version), actual exe hashes, custom settings, real history/PNG, downgrade refusal, future-schema refusal, backup restore, uninstall/reinstall)"
