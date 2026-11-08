#!/usr/bin/env pwsh
# Exercise the gate with synthetic cargo output; no desktop or clipboard access.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$gate = Join-Path $root 'tools/check-test-baseline.ps1'
$directory = Join-Path $root ('target/baseline-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$floors = [ordered]@{
    snapclip_app = 267; yue_pin = 58; yue_shot_capture = 354
    snapclip_history = 42; yue_shot_storage = 5
    snapclip_clipboard = 1; yue_clipboard = 14
    snapclip_model = 4; yue_media = 8; yue_geometry = 10
    yue_shot_model = 5; yue_pin_model = 4
}
function Check-Case([string]$name, [hashtable]$counts, [bool]$passes) {
    $log = Join-Path $directory "$name.log"
    $lines = foreach ($stem in $counts.Keys) {
        "     Running unittests src/lib.rs (target/debug/deps/$stem-0123456789abcdef.exe)"
        "test result: ok. $($counts[$stem]) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
    }
    $lines | Set-Content -LiteralPath $log -Encoding utf8
    $output = & powershell -NoProfile -File $gate -Log $log 2>&1
    if (($LASTEXITCODE -eq 0) -ne $passes) { throw "$name produced the wrong gate result`n$output" }
    Write-Host "$name passed"
}
try {
    $counts = @{}; foreach ($key in $floors.Keys) { $counts[$key] = $floors[$key] }
    Check-Case 'preserve-moved-tests' $counts $true
    $counts.yue_clipboard = 13
    Check-Case 'reject-lost-system-clipboard-test' $counts $false
    $counts.yue_clipboard = 15; $counts.snapclip_history = 41
    Check-Case 'reject-history-loss-despite-extra-clipboard-test' $counts $false
    $counts.snapclip_history = 42; $counts.Remove('yue_clipboard')
    Check-Case 'reject-missing-system-clipboard-crate' $counts $false
    $counts.yue_clipboard = 14; $counts.yue_pin_model = 3; $counts.snapclip_model = 5
    Check-Case 'reject-pin-model-loss-despite-extra-product-test' $counts $false
    $counts.yue_pin_model = 4; $counts.snapclip_model = 4; $counts.Remove('yue_shot_storage')
    Check-Case 'reject-missing-capture-storage' $counts $false
} finally {
    Remove-Item -LiteralPath $directory -Recurse -Force
}
