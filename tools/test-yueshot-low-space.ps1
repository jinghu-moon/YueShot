param(
    [Parameter(Mandatory=$true)][string]$Candidate,
    [Parameter(Mandatory=$true)][string]$Output
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
$candidateRoot = (Resolve-Path -LiteralPath $Candidate).Path
$deliveryRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot 'target/yueshot-delivery')) + '\'
$testRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot $Output))
$allowedTests = [IO.Path]::GetFullPath((Join-Path $projectRoot 'target/low-space')) + '\'
if (-not $candidateRoot.StartsWith($deliveryRoot,[StringComparison]::OrdinalIgnoreCase)) { throw 'Candidate must be under target/yueshot-delivery.' }
if (-not $testRoot.StartsWith($allowedTests,[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $testRoot)) { throw 'Use a fresh output directory under target/low-space.' }
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Creating an isolated VHD requires elevation; this is not ordinary-user qualification.' }
$manifest = Get-Content -LiteralPath (Join-Path $candidateRoot 'delivery.json') -Raw | ConvertFrom-Json
if ($manifest.channel -ne 'dev' -or $manifest.installation.installationId -ne 'com.seeyuer.yueshot.dev.x64') { throw 'Only an isolated dev channel candidate is allowed.' }
$registry = 'HKCU:/' + $manifest.installation.uninstallKey
$productRegistry = 'HKCU:/' + $manifest.installation.registryKey
if ((Test-Path -LiteralPath $registry) -or (Test-Path -LiteralPath $productRegistry) -or (Get-Process YueShot -ErrorAction SilentlyContinue)) { throw 'Normally exit YueShot and uninstall any dev channel installation first.' }
& python tools/yueshot-delivery.py verify $candidateRoot
if ($LASTEXITCODE -ne 0) { throw 'Candidate verification failed.' }
$installers = @(Get-ChildItem -LiteralPath (Join-Path $candidateRoot 'nsis') -Filter '*.exe' -File)
if ($installers.Count -ne 1) { throw 'Expected one actual NSIS installer.' }
New-Item -ItemType Directory -Path $testRoot | Out-Null
$vhd = Join-Path $testRoot 'owned-128MiB.vhd'
$diskpartScript = Join-Path $testRoot 'create-vhd.txt'
if ($vhd.Contains('"') -or $vhd.Contains("`n") -or $vhd.Contains("`r")) { throw 'Unsafe diskpart image path.' }
# diskpart only CREATES a new image. Disk selection/formatting uses objects obtained
# from this exact image path, never a guessed number or any existing physical disk.
Set-Content -LiteralPath $diskpartScript -Encoding ascii -Value "create vdisk file=`"$vhd`" maximum=128 type=fixed`r`nexit"
$result = [ordered]@{
    format=1; scope='elevated isolated dev NSIS disk-full qualification'; status='running'
    version=$manifest.version; buildId=$manifest.buildId
    installerSha256=(Get-FileHash -LiteralPath $installers[0].FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    vhd=$vhd; checks=@(); ordinaryUserQualification=$false; powerLossQualification=$false
}
$mounted = $false
$installRoot = $null
$dataRoot = $null

function Invoke-Installer([int]$Expected) {
    # NSIS /D must be final and unquoted, including paths with spaces.
    $process = Start-Process -FilePath $installers[0].FullName -ArgumentList @('/S','/NS',"/D=$installRoot") -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne $Expected) { throw "Installer exit $($process.ExitCode), expected $Expected." }
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
            foreach ($property in (Get-ItemProperty -LiteralPath $key).PSObject.Properties | Where-Object { $_.Name -notlike 'PS*' } | Sort-Object Name) { $state["$key/$($property.Name)"] = $property.Value }
        }
    }
    ConvertTo-Json -InputObject $state -Compress -Depth 8
}
function Invoke-Smoke {
    $process = Start-Process -FilePath (Join-Path $installRoot 'YueShot.exe') -ArgumentList @('--smoke-test','--data-dir',('"'+$dataRoot+'"')) -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw 'Installed executable smoke failed.' }
}
try {
    & diskpart.exe /s $diskpartScript | Out-File -LiteralPath (Join-Path $testRoot 'diskpart.log')
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $vhd)) { throw 'Creating the new VHD failed.' }
    Mount-DiskImage -ImagePath $vhd -Access ReadWrite -NoDriveLetter | Out-Null
    $mounted = $true
    $image = Get-DiskImage -ImagePath $vhd
    $disks = @($image | Get-Disk)
    if (-not $image.Attached -or [IO.Path]::GetFullPath($image.ImagePath) -ne $vhd -or $disks.Count -ne 1) { throw 'VHD identity guard failed.' }
    $disk = $disks[0]
    if ($disk.IsBoot -or $disk.IsSystem -or $disk.Size -ne 128MB -or $disk.PartitionStyle -ne 'RAW' -or [string]$disk.BusType -ne 'File Backed Virtual') { throw 'Refusing to initialize a disk without the exact new VHD identity.' }
    $result.diskNumber = $disk.Number
    $disk | Initialize-Disk -PartitionStyle MBR
    $partition = $disk | New-Partition -UseMaximumSize -AssignDriveLetter
    $volume = $partition | Format-Volume -FileSystem NTFS -NewFileSystemLabel 'YueShot-LowSpace' -Confirm:$false
    $mapped = Get-Partition -DriveLetter $volume.DriveLetter | Get-Disk
    if ($mapped.Number -ne $disk.Number -or $mapped.IsBoot -or $mapped.IsSystem) { throw 'Volume ownership guard failed.' }
    $volumeRoot = "$($volume.DriveLetter):\"
    $installRoot = Join-Path $volumeRoot 'YueShot dev test'
    $dataRoot = Join-Path $volumeRoot 'isolated data'
    Invoke-Installer 0
    Invoke-Smoke
    $settingsHash = (Get-FileHash -LiteralPath (Join-Path $dataRoot 'settings.json')).Hash
    Set-Content -LiteralPath (Join-Path $installRoot 'user-owned.txt') -Value 'retain this unrelated file'
    $before = Snapshot
    $before | Set-Content -LiteralPath (Join-Path $testRoot 'before.json') -Encoding utf8
    $result.checks += 'actual NSIS install and installed EXE smoke on owned VHD'

    $filler = Join-Path $volumeRoot 'owned-filler.bin'
    $stream = [IO.File]::Open($filler,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    $diskFull = $false
    try {
        $block = [byte[]]::new(64KB)
        # Actual non-sparse writes, bounded to this 128 MiB volume.
        for ($i=0; $i -lt 4096; $i++) { $stream.Write($block,0,$block.Length); $stream.Flush($true) }
    } catch [IO.IOException] {
        if (($_.Exception.HResult -band 0xffff) -ne 112) { throw }
        $diskFull = $true
    } finally { $stream.Dispose() }
    if (-not $diskFull) { throw 'Did not reach actual ERROR_DISK_FULL.' }
    # Leave metadata room so extraction starts and fails during a real write,
    # rather than depending on whether NTFS can create the transaction directory.
    $stream = [IO.File]::Open($filler,[IO.FileMode]::Open,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try { $stream.SetLength($stream.Length - 256KB); $stream.Flush($true) } finally { $stream.Dispose() }
    $result.freeBytesBeforeFailure = (Get-Volume -DriveLetter $volume.DriveLetter).SizeRemaining
    if ($result.freeBytesBeforeFailure -ge 1MB) { throw 'Unexpectedly enough space remained; test would not exercise extraction failure.' }
    $result.checks += 'real VHD write returned Win32 ERROR_DISK_FULL (112)'
    Invoke-Installer 14
    if ((Snapshot) -cne $before) { throw 'Disk-full failure changed existing files, user data or registration.' }
    if (Test-Path -LiteralPath (Join-Path $installRoot '.yueshot-install')) { throw 'Disk-full failure left a transaction.' }
    $result.checks += 'NSIS returned 14; old payload, data and registry byte/value snapshot unchanged'
    Remove-Item -LiteralPath $filler
    Invoke-Smoke
    $result.checks += 'old installed executable still runs after failed reinstall'
    Invoke-Installer 0
    Invoke-Smoke
    if ((Get-FileHash -LiteralPath (Join-Path $dataRoot 'settings.json')).Hash -ne $settingsHash) { throw 'Retry changed settings.' }
    $result.checks += 'space reclaimed; same installer retry and installed EXE smoke succeeded; settings retained'

    $uninstall = Start-Process -FilePath (Join-Path $installRoot 'uninstall.exe') -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
    if ($uninstall.ExitCode -ne 0) { throw 'Normal uninstall launcher failed.' }
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while ((Test-Path -LiteralPath (Join-Path $installRoot 'uninstall.exe')) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    if ((Test-Path -LiteralPath $registry) -or (Test-Path -LiteralPath $productRegistry) -or (Test-Path -LiteralPath (Join-Path $installRoot 'YueShot.exe')) -or (Test-Path -LiteralPath (Join-Path $installRoot 'uninstall.exe'))) { throw 'Uninstall did not finish.' }
    if (-not (Test-Path -LiteralPath (Join-Path $installRoot 'user-owned.txt')) -or -not (Test-Path -LiteralPath (Join-Path $dataRoot 'settings.json'))) { throw 'Uninstall removed unrelated files/data.' }
    $result.checks += 'normal uninstall removed dev payload/registration and preserved isolated user file/data'
    $result.status = 'passed'
} catch {
    $result.status = 'failed'
    $result.error = $_.Exception.Message
    throw
} finally {
    if ($mounted) {
        try { Dismount-DiskImage -ImagePath $vhd | Out-Null; $result.dismounted = $true }
        catch { $result.dismounted = $false; $result.status = 'failed'; $result.dismountError = $_.Exception.Message; Write-Warning 'VHD dismount failed; inspect the exact image path in result.json.' }
    }
    $result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $testRoot 'result.json') -Encoding utf8
}
if ($result.status -ne 'passed') { throw 'Low-space qualification failed; preserve output for investigation.' }
Write-Output "LOW_SPACE=PASS; evidence=$testRoot"
