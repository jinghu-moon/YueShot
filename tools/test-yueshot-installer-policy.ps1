param([Parameter(Mandatory=$true)][string]$Candidate)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
$candidateRoot = (Resolve-Path -LiteralPath $Candidate).Path
$allowedRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot 'target/yueshot-delivery')) + [IO.Path]::DirectorySeparatorChar
if (-not $candidateRoot.StartsWith($allowedRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Use a candidate inside target/yueshot-delivery.' }
$manifest = Get-Content -LiteralPath (Join-Path $candidateRoot 'delivery.json') -Raw | ConvertFrom-Json
$identity = $manifest.installation
if ($identity.channel -notin @('dev','beta') -or $identity.installationId -ne "com.seeyuer.yueshot.$($identity.channel).x64") { throw 'Expected a reviewed dev/beta channel identity' }
$registry = 'HKCU:/' + $identity.uninstallKey
$productRegistry = 'HKCU:/' + $identity.registryKey
if ((Test-Path -LiteralPath $registry) -or (Get-Process YueShot -ErrorAction SilentlyContinue)) {
    throw 'Normally exit YueShot and uninstall any installation of this channel first.'
}
& python tools/yueshot-delivery.py verify $candidateRoot
if ($LASTEXITCODE -ne 0) { throw 'Candidate verification failed' }
$installers = @(Get-ChildItem -LiteralPath (Join-Path $candidateRoot 'nsis') -Filter '*.exe' -File)
if ($installers.Count -ne 1) { throw 'Expected one installer.' }
$installRoot = Join-Path $candidateRoot 'installed-policy-check'
$dataRoot = Join-Path $candidateRoot 'installed-policy-data'
$fixtureRoot = Join-Path $candidateRoot 'installer-policy-fixture'
foreach ($path in @($installRoot,$dataRoot,$fixtureRoot)) {
    if (Test-Path -LiteralPath $path) { throw 'Preserve previous evidence and use a fresh candidate.' }
}
New-Item -ItemType Directory -Path $fixtureRoot -ErrorAction Stop | Out-Null
$helper = Join-Path $fixtureRoot 'YueShot.exe'
& rustc (Join-Path $PSScriptRoot 'tests/fixtures/installer_busy.rs') -o $helper
if ($LASTEXITCODE -ne 0) { throw 'Build controlled busy-process fixture failed' }

function Invoke-Installer([int]$Expected) {
    $process = Start-Process -FilePath $installers[0].FullName -ArgumentList @('/S','/NS',"/D=$installRoot") -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne $Expected) { throw "Installer exit $($process.ExitCode), expected $Expected" }
}
function Snapshot {
    $state = [ordered]@{}
    foreach ($root in @($installRoot,$dataRoot)) {
        if (Test-Path -LiteralPath $root) {
            foreach ($file in Get-ChildItem -LiteralPath $root -File -Recurse -Force | Sort-Object FullName) {
                $state[$file.FullName] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
            }
        }
    }
    foreach ($key in @($registry,$productRegistry)) {
        if (Test-Path -LiteralPath $key) {
            foreach ($property in (Get-ItemProperty -LiteralPath $key).PSObject.Properties | Where-Object { $_.Name -notlike 'PS*' } | Sort-Object Name) {
                $state["$key/$($property.Name)"] = $property.Value
            }
        }
    }
    ConvertTo-Json -InputObject $state -Compress -Depth 8
}
function Assert-Unchanged([string]$Before) {
    if ((Snapshot) -cne $Before) { throw 'Rejected operation changed installed files, data or registration.' }
}

Invoke-Installer 0
$exe = Join-Path $installRoot 'YueShot.exe'
$smoke = Start-Process -FilePath $exe -ArgumentList @('--smoke-test','--data-dir',('"'+$dataRoot+'"')) -WindowStyle Hidden -Wait -PassThru
if ($smoke.ExitCode -ne 0) { throw 'Installed smoke failed' }
Set-Content -LiteralPath (Join-Path $installRoot 'user-owned.txt') -Value 'not installer owned'
$settingsHash = (Get-FileHash -LiteralPath (Join-Path $dataRoot 'settings.json') -Algorithm SHA256).Hash

try {
    foreach ($fixture in @(@('99.0.0',12),@('not-semver',13),@('',13),@($null,13))) {
        if ($null -eq $fixture[0]) { Remove-ItemProperty -LiteralPath $registry -Name DisplayVersion }
        else { Set-ItemProperty -LiteralPath $registry -Name DisplayVersion -Value $fixture[0] }
        $before = Snapshot
        Invoke-Installer $fixture[1]
        Assert-Unchanged $before
    }
} finally {
    Set-ItemProperty -LiteralPath $registry -Name DisplayVersion -Value $manifest.version
}

# Both same-version reinstall and an older registered version must remain permitted.
# The latter is a registration fixture, not a claim of real cross-version data migration.
Invoke-Installer 0
Set-ItemProperty -LiteralPath $registry -Name DisplayVersion -Value '0.0.0-0'
Invoke-Installer 0
if ((Get-ItemProperty -LiteralPath $registry).DisplayVersion -ne $manifest.version) { throw 'Registration did not advance' }
if ((Get-FileHash -LiteralPath (Join-Path $dataRoot 'settings.json') -Algorithm SHA256).Hash -ne $settingsHash) { throw 'Reinstall changed user configuration' }

$eventName = 'Local\YueShotInstallerPolicy-' + [Guid]::NewGuid().ToString('N')
$stopEvent = New-Object Threading.EventWaitHandle($false,[Threading.EventResetMode]::ManualReset,$eventName)
$ready = Join-Path $fixtureRoot 'ready'
$busy = Start-Process -FilePath $helper -ArgumentList @($eventName,('"'+$ready+'"')) -WindowStyle Hidden -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $ready) -and -not $busy.HasExited -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
    if ($busy.HasExited -or -not (Test-Path -LiteralPath $ready)) { throw 'Controlled busy process did not start' }
    $before = Snapshot
    Invoke-Installer 10
    Assert-Unchanged $before
    # NSIS normally delegates to a temporary child and the outer launcher can return 0.
    # _?= executes the actual uninstaller synchronously, exposing its refusal code.
    # Only use direct mode for a refused operation: a normal uninstall needs its
    # temporary copy to delete the original uninstaller after it exits.
    $uninstall = Start-Process -FilePath (Join-Path $installRoot 'uninstall.exe') -ArgumentList @('/S',"_?=$installRoot") -WindowStyle Hidden -Wait -PassThru
    if ($uninstall.ExitCode -ne 10) { throw "Running-process uninstall exit $($uninstall.ExitCode), expected 10" }
    Assert-Unchanged $before
    if ($busy.HasExited) { throw 'Installer/uninstaller terminated the running process' }
} finally {
    $stopEvent.Set() | Out-Null
    if (-not $busy.WaitForExit(10000)) { throw 'Controlled process did not exit normally' }
    $stopEvent.Dispose()
}
if ($busy.ExitCode -ne 0) { throw 'Controlled process exit failed' }

# A normal uninstall removes only installer-owned files and retains data/unknown files.
$uninstall = Start-Process -FilePath (Join-Path $installRoot 'uninstall.exe') -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw 'Normal uninstall failed' }
$deadline = [DateTime]::UtcNow.AddSeconds(30)
while (((Test-Path -LiteralPath $exe) -or (Test-Path -LiteralPath $registry) -or (Test-Path -LiteralPath (Join-Path $installRoot 'uninstall.exe'))) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
if ((Test-Path -LiteralPath $exe) -or (Test-Path -LiteralPath $registry)) { throw 'Normal uninstall left product payload/registration' }
if (-not (Test-Path -LiteralPath (Join-Path $installRoot 'user-owned.txt'))) { throw 'Uninstaller deleted an unowned file' }
if ((Get-FileHash -LiteralPath (Join-Path $dataRoot 'settings.json') -Algorithm SHA256).Hash -ne $settingsHash) { throw 'Uninstaller changed user configuration' }
Write-Output 'INSTALLER_POLICY=PASS (downgrade/corrupt registration refusal, reinstall, running-process protection, no forced termination, user files/data retained)'
