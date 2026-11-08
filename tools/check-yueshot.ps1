param([switch]$Native)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit $LASTEXITCODE" }
}
Invoke-Checked python @('tools/vendor-yue.py')
Invoke-Checked python @('tools/yueshot_vendor.py')
Invoke-Checked python @('-m','unittest','discover','-s','tools/tests','-p','test_yueshot_*.py')
Invoke-Checked python @('tools/check-hdr-codec-licenses.py')
Invoke-Checked python @('tools/yueshot_assets.py')
Invoke-Checked python @('tools/yueshot_svg.py')
Invoke-Checked python @('tools/yueshot_licenses.py')
Invoke-Checked cargo @('deny','--manifest-path','apps/yue-shot/Cargo.toml','--config','tools/delivery/yueshot-deny.toml','--target','x86_64-pc-windows-msvc','--exclude-dev','check','licenses')
Invoke-Checked cargo @('fmt','-p','yue-shot','-p','yue-shot-capture','-p','yue-shot-overlay-ui','-p','yue-shot-storage','-p','yue-shell','-p','xtask','--','--check')
# The source workspace also contains SnapClip; the independent YueShot checkout
# does not. Derive the consumer regression set from declared workspace members.
$metadataText = & cargo metadata --no-deps --format-version 1 --locked
if ($LASTEXITCODE -ne 0) { throw 'Workspace metadata failed' }
$metadata = $metadataText | ConvertFrom-Json
$hasSnapClip = @($metadata.packages | Where-Object { $_.id -in $metadata.workspace_members -and $_.name -eq 'snapclip-app' }).Count -eq 1
$consumerArgs = @()
$testFeatures = 'yue-shot/test-support'
if ($hasSnapClip) {
    $consumerArgs = @('-p','snapclip-app')
    $testFeatures += ',snapclip-app/test-support'
}
Invoke-Checked cargo (@('test','-p','yue-shot','-p','yue-shot-capture','-p','yue-shot-storage','-p','yue-shell') + $consumerArgs + @('--features',$testFeatures,'--locked','--','--test-threads=1'))
Invoke-Checked cargo @('test','-p','yue-shot-overlay-ui','--lib','--locked')
Invoke-Checked cargo (@('clippy','-p','yue-shot','-p','yue-shot-capture','-p','yue-shot-overlay-ui','-p','yue-shot-storage','-p','yue-shell','-p','xtask','--all-targets') + $consumerArgs + @('--features',($testFeatures+',yue-shot/avif,yue-shot/hdr-capture'),'--locked','--','-D','warnings'))
Invoke-Checked cargo @('check','--workspace','--all-targets','--features',$testFeatures,'--locked')
Invoke-Checked powershell @('-NoProfile','-ExecutionPolicy','Bypass','-File','tools/check-dependency-direction.ps1','-Avif','-Hdr')
if ($Native) {
    if (Get-Process YueShot -ErrorAction SilentlyContinue) { throw 'Exit YueShot normally before native hotkey tests.' }
    Invoke-Checked cargo @('test','-p','yue-shell','--locked','--','--include-ignored','--test-threads=1')
    Invoke-Checked cargo @('test','-p','yue-shot','--features','test-support','--locked','--lib','tests::a_conflicting_product_hotkey','--','--ignored','--test-threads=1')
}
