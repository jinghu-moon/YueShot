# Dependency-direction gate (docs/23 T1.9).
#
# The whole point of splitting the capability crates is that the dependency arrows only
# ever point one way:
#
#   yue-shot (product) ──► yue-shot-capture ──► yue-shot-model ──► yue-media
#        │                     │                   └──────────► yue-geometry
#        │                     └──► yue-shot-storage
#   snapclip-app ──► snapclip-history / snapclip-clipboard
#        │                   │                  └───────────► yue-geometry
#   yue-shot-overlay-ui ───────────────────────────────────► yue-geometry
#
# `cargo tree` is the cheapest way to prove it, and it is the only check that catches the
# accidental edges a refactor like this one is prone to: capture held a real one into the
# clipboard module until T1.4/T1.5 moved it out.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/check-dependency-direction.ps1
#   powershell ... -File tools/check-dependency-direction.ps1 -Package snapclip-app   # negative control
#
# Exits non-zero and prints every offending edge.

param(
    # Check one package instead of the whole invariant set. Used as the negative control:
    # `-Package snapclip-app` must fail because the shell depends on GPUI and capabilities.
    [string]$Package,
    [string]$CargoConfig = '',
    [switch]$Avif,
    [switch]$Hdr
)

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

# Capability crates may depend on a shared clipboard service, never on sibling views.
# Pin is now a GPUI feature (docs/52); clipboard/model/history remain UI-free.
$SHELL_ONLY = @("snapclip-app", "tauri", "wry", "gpui", "gpui-pre", "gpui-kit", "gpui-base", "gpui-component", "yue-component")
$APP_ONLY = @("snapclip-app", "yue-shot", "tauri", "wry")
$FORBIDDEN = @{
    "yue-avif" = $SHELL_ONLY + @("snapclip-model", "yue-shot-model", "yue-shot-capture", "yue-pin", "snapclip-history", "snapclip-clipboard", "yue-shot-overlay-ui")
    "yue-pin-model" = $SHELL_ONLY + @("snapclip-model", "snapclip-history", "snapclip-clipboard", "yue-clipboard", "yue-pin", "yue-shot", "yue-shot-capture", "yue-shot-model", "rusqlite", "image", "windows-sys")
    "yue-clipboard" = $SHELL_ONLY + @("yue-shot", "snapclip-model", "yue-shot-model", "yue-pin", "snapclip-clipboard", "snapclip-history", "yue-shot-capture", "yue-shot-storage", "rusqlite")
    "yue-pin" = $APP_ONLY + @("snapclip-model", "rusqlite", "arboard", "snapclip-clipboard", "yue-shot-capture", "snapclip-history", "snapclip-recognize")
    "snapclip-clipboard" = $SHELL_ONLY + @("rusqlite", "yue-pin", "yue-shot-capture", "snapclip-history", "snapclip-recognize", "yue-shot-overlay-ui")
    "yue-shot-capture" = $SHELL_ONLY + @("snapclip-model", "yue-pin", "snapclip-clipboard", "rusqlite", "arboard", "snapclip-history", "snapclip-recognize")
    "yue-shot-overlay-ui" = $SHELL_ONLY + @("yue-pin", "snapclip-clipboard", "yue-shot-capture", "snapclip-history", "snapclip-model")
    "snapclip-history" = $SHELL_ONLY + @("yue-pin", "yue-shot-capture", "snapclip-recognize")
    "snapclip-model" = $SHELL_ONLY + @("yue-pin", "snapclip-clipboard", "rusqlite", "arboard", "image", "windows", "windows-sys", "yue-shot-capture", "yue-shot-model", "snapclip-history", "snapclip-recognize")
    "yue-media" = $SHELL_ONLY + @("snapclip-model", "yue-shot-model", "yue-pin", "snapclip-clipboard", "snapclip-history", "yue-avif", "yue-shot-capture", "snapclip-recognize")
    "yue-shot-model" = $SHELL_ONLY + @("snapclip-model", "yue-pin", "snapclip-clipboard", "snapclip-history", "snapclip-recognize", "yue-shot-capture", "yue-shot-storage", "yue-shot-overlay-ui")
    "yue-geometry" = $SHELL_ONLY + @("snapclip-model", "yue-shot-model", "yue-pin", "snapclip-clipboard", "snapclip-history", "yue-shot-capture", "yue-shot-storage", "yue-shot-overlay-ui")
    "yue-shot-storage" = $SHELL_ONLY + @("snapclip-model", "snapclip-history", "yue-pin", "snapclip-clipboard", "yue-shot-capture", "snapclip-recognize")
    "yue-shot" = @("snapclip-model", "snapclip-clipboard", "snapclip-app", "snapclip-history", "snapclip-recognize", "tauri", "wry", "fontdb", "ttf-parser", "rustybuzz", "harfrust")
}

# `yue-media` is the pixel/color substrate and `snapclip-model` builds serializable
# domain values on top of it (docs/53 section 11.1). `half` represents FP16 without an image codec; its
# cfg-if/zerocopy dependencies are listed explicitly rather than allowing its entire
# transitive graph. UI, platform, codec and capability dependencies remain forbidden.
$MODEL_ALLOWED = @("serde", "serde_core", "serde_derive", "serde_json", "serde_derive_internals", "proc-macro2", "quote", "syn", "unicode-ident", "memchr", "itoa", "ryu", "half", "cfg-if", "zerocopy", "zerocopy-derive", "yue-color", "yue-media", "yue-geometry")
$YUE_MODEL_ALLOWED = $MODEL_ALLOWED
$GEOMETRY_ALLOWED = @("serde", "serde_core", "serde_derive", "proc-macro2", "quote", "syn", "unicode-ident")

function Get-TreeLines([string]$target) {
    $featureArgs = @()
    $enabled = @()
    if ($Avif -and $target -in @("yue-pin", "snapclip-app", "yue-shot")) { $enabled += 'avif' }
    if ($Hdr -and $target -in @("yue-shot", "yue-shot-capture")) { $enabled += 'hdr-capture' }
    if ($enabled.Count) { $featureArgs = @('--features', ($enabled -join ',')) }
    $configArgs = @()
    if ($CargoConfig) { $configArgs = @('--config', $CargoConfig) }
    # Windows PowerShell 5.1 turns native stderr progress into ErrorRecord.
    # Cargo's lock-wait notices are not failures; use its exit status.
    $strict = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = & cargo tree -p $target -e normal --locked @featureArgs @configArgs 2>&1
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $strict
    }
    if ($code -ne 0) {
        throw "cargo tree -p $target failed:`n$($output -join "`n")"
    }
    return @($output)
}

# `cargo tree` prints each package as `… ├── name v1.2.3` (optionally with ` (*)`).
function Get-PackageNames([string[]]$lines) {
    $names = New-Object System.Collections.Generic.HashSet[string]
    foreach ($line in $lines) {
        if ($line -match '(?:^|[\s└├│─]+)\s*([a-zA-Z0-9_-]+) v[0-9]') {
            [void]$names.Add($Matches[1])
        }
    }
    return $names
}

$failures = New-Object System.Collections.Generic.List[string]

$metadataConfig = @()
if ($CargoConfig) { $metadataConfig = @('--config', $CargoConfig) }
$metadataText = & cargo metadata --no-deps --format-version 1 --locked @metadataConfig
if ($LASTEXITCODE -ne 0) { throw 'Workspace metadata failed' }
$metadata = $metadataText | ConvertFrom-Json
$members = @($metadata.packages | Where-Object { $_.id -in $metadata.workspace_members } | ForEach-Object { $_.name })
$invariants = @("yue-shot", "yue-shot-capture", "yue-shot-storage", "yue-shot-overlay-ui", "yue-shot-model", "yue-geometry", "snapclip-history", "snapclip-model", "yue-media", "yue-pin", "snapclip-clipboard", "yue-clipboard", "yue-pin-model", "yue-avif")
$targets = if ($Package) { @($Package) } else { @($invariants | Where-Object { $_ -in $members }) }
if ('yue-shot' -notin $members) { throw 'The YueShot product must be a declared workspace member' }

foreach ($target in $targets) {
    $lines = Get-TreeLines $target
    $names = Get-PackageNames $lines

    $forbidden_for_target = $FORBIDDEN[$target]
    if (-not $forbidden_for_target) {
        # `-Package <other>` (the negative control): apply the strictest list.
        $forbidden_for_target = $SHELL_ONLY + @("yue-shot-capture", "snapclip-history", "snapclip-recognize")
    }
    foreach ($dependency in $forbidden_for_target) {
        # The first line of `cargo tree` is the package itself, not a dependency.
        if ($dependency -eq $target) { continue }
        if ($names.Contains($dependency)) {
            $failures.Add("$target depends on $dependency")
        }
    }

    $allowed = switch ($target) {
        "snapclip-model" { $MODEL_ALLOWED }
        "yue-shot-model" { $YUE_MODEL_ALLOWED }
        "yue-pin-model" { $MODEL_ALLOWED }
        "yue-geometry" { $GEOMETRY_ALLOWED }
        default { $null }
    }
    if ($allowed) {
        foreach ($name in $names) {
            # The first line of `cargo tree` is the package itself, not a dependency.
            if ($name -eq $target) { continue }
            if ($allowed -notcontains $name) {
                $failures.Add("$target depends on $name (only its declared pure-value dependency allowlist is allowed)")
            }
        }
    }

    $count = $names.Count
    Write-Host ("checked {0}: {1} packages in its normal graph" -f $target, $count)
}

# Second pass (docs/30 section 28.4): the scroll/ module is the platform-free core. The whole
# "tests need no desktop" story (docs/30 section 29) rests on it, and no dependency scan can see
# this edge because it is a `use` inside the crate rather than a package. So it is a source scan,
# in the same script and with the same exit code, so that there is one gate and not two.
#
# Skipped under -Package: that switch is the negative control for the dependency pass, and running
# the source scan there would make the control's failure ambiguous.
if (-not $Package) {
    # Yue is the styled layer for App and Pin. Enabling Kit's default component
    # feature would pull the obsolete virtualization consumers back into both.
    if ('snapclip-app' -in $members) {
        $appNames = Get-PackageNames (Get-TreeLines 'snapclip-app')
        if ($appNames.Contains('gpui-component')) {
            $failures.Add('snapclip-app/pin must use Yue/Base, not the gpui-component styled layer')
        }
        foreach ($privateCapture in @('yue-shot', 'yue-shot-capture', 'yue-shot-model', 'yue-shot-storage', 'yue-shot-overlay-ui')) {
            if ($appNames.Contains($privateCapture)) {
                $failures.Add("snapclip-app depends on YueShot-private package $privateCapture")
            }
        }
        Write-Host 'checked snapclip-app: no YueShot-private package dependencies'
        Write-Host 'checked snapclip-app/pin: Yue/Base styled-layer boundary'
    }

    $scrollRoot = "apps/yue-shot/crates/yue-shot-capture/src/scroll"
    $scrollFiles = @(Get-ChildItem -Path $scrollRoot -Filter *.rs -Recurse)
    $hits = @($scrollFiles | Select-String -Pattern "crate::windows|crate::sampler|winapi|windows_sys|windows::")
    foreach ($hit in $hits) {
        $relative = Resolve-Path -Relative $hit.Path
        $reference = $hit.Matches[0].Value
        $failures.Add("$relative" + ":" + "$($hit.LineNumber) references " + "$reference (scroll/ must stay platform-free)")
    }
    Write-Host ("checked {0}: {1} files scanned for platform references" -f $scrollRoot, $scrollFiles.Count)
}

if ($failures.Count -gt 0) {
    Write-Host ""
    Write-Host "dependency direction violated:" -ForegroundColor Red
    foreach ($failure in $failures) { Write-Host "  - $failure" -ForegroundColor Red }
    exit 1
}

Write-Host "dependency direction is clean" -ForegroundColor Green
