param([Parameter(Mandatory=$true)][string]$Candidate)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
$candidateRoot = (Resolve-Path -LiteralPath $Candidate).Path
$allowedRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot 'target/yueshot-delivery')) + [IO.Path]::DirectorySeparatorChar
if (-not $candidateRoot.StartsWith($allowedRoot,[StringComparison]::OrdinalIgnoreCase)) { throw 'Use a local delivery candidate' }
& python tools/yueshot-delivery.py verify $candidateRoot
if ($LASTEXITCODE -ne 0) { throw 'Candidate verification failed' }
$manifest = Get-Content -LiteralPath (Join-Path $candidateRoot 'delivery.json') -Raw | ConvertFrom-Json
$identity = $manifest.installation
if ($identity.channel -notin @('dev','beta','stable') -or $identity.installationId -ne "com.seeyuer.yueshot.$($identity.channel).x64") { throw 'Expected reviewed channel identity' }
$registration = 'HKCU:/' + $identity.uninstallKey
$productRegistry = 'HKCU:/' + $identity.registryKey
$installRoot = Join-Path $candidateRoot 'installed-channel-check'
$desktop = [Environment]::GetFolderPath('DesktopDirectory')
$programs = [Environment]::GetFolderPath('Programs')
$startMenuFolder = Join-Path $programs $identity.displayName
$links = @((Join-Path $desktop "$($identity.displayName).lnk"),(Join-Path $startMenuFolder "$($identity.displayName).lnk"))
foreach ($path in @($registration,$installRoot,$startMenuFolder) + $links) {
    if (Test-Path -LiteralPath $path) { throw "Preserve existing user state: $path" }
}
if (Get-Process YueShot -ErrorAction SilentlyContinue) { throw 'Exit YueShot normally first' }
$installers = @(Get-ChildItem -LiteralPath (Join-Path $candidateRoot 'nsis') -Filter '*.exe' -File)
if ($installers.Count -ne 1) { throw 'Expected one installer' }
$installed = Start-Process -FilePath $installers[0].FullName -ArgumentList @('/S',"/D=$installRoot") -WindowStyle Hidden -Wait -PassThru
if ($installed.ExitCode -ne 0) { throw 'Install failed' }
$exe = Join-Path $installRoot 'YueShot.exe'
$infoFile = Join-Path $candidateRoot 'installed-product-info.json'
$infoProcess = Start-Process -FilePath $exe -ArgumentList '--product-info' -RedirectStandardOutput $infoFile -WindowStyle Hidden -Wait -PassThru
if ($infoProcess.ExitCode -ne 0) { throw 'Installed identity query failed' }
$actual = Get-Content -LiteralPath $infoFile -Raw | ConvertFrom-Json
foreach ($field in $identity.PSObject.Properties) {
    if ($actual.($field.Name) -cne $field.Value) { throw "Installed identity mismatch: $($field.Name)" }
}
$registered = Get-ItemProperty -LiteralPath $registration
if ($registered.DisplayName -ne $identity.displayName -or $registered.DisplayVersion -ne $manifest.version) { throw 'Installed channel registration mismatch' }
$shell = New-Object -ComObject WScript.Shell
$explorer = New-Object -ComObject Shell.Application
foreach ($linkPath in $links) {
    if (-not (Test-Path -LiteralPath $linkPath)) { throw "Missing channel shortcut: $linkPath" }
    $link = $shell.CreateShortcut($linkPath)
    if ($link.TargetPath -ne $exe -or $link.Arguments -ne '') { throw 'Shortcut must launch this exe with its default channel data root' }
    $folder = $explorer.NameSpace((Split-Path -Parent $linkPath))
    $item = $folder.ParseName((Split-Path -Leaf $linkPath))
    if ($item.ExtendedProperty('System.AppUserModel.ID') -cne $identity.aumid) { throw 'Shortcut AUMID differs from product runtime' }
}
$uninstall = Start-Process -FilePath (Join-Path $installRoot 'uninstall.exe') -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw 'Uninstall failed' }
$deadline = [DateTime]::UtcNow.AddSeconds(30)
while (((Test-Path -LiteralPath $installRoot) -or (Test-Path -LiteralPath $registration)) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
foreach ($path in @($registration,$productRegistry,$installRoot,$startMenuFolder) + $links) {
    if (Test-Path -LiteralPath $path) { throw "Channel uninstall left owned state: $path" }
}
Write-Output 'CHANNEL_INSTALL=PASS (runtime/installer identity, registration, default-root shortcuts, AUMID, uninstall cleanup)'
