<#
Check C18 (.specs\features\windows-installer\checks.md): the release for -Tag
lists both Windows installers. Exits 1 naming any that is missing.

  powershell -File scripts\windows\check-release-assets.ps1 -Tag v0.2.0-rc1
#>
param([Parameter(Mandatory)][string]$Tag)
$ErrorActionPreference = 'Stop'
$version = $Tag -replace '^v', ''
$names = gh release view $Tag --repo sammyjdev/voxis --json assets --jq '.assets[].name'
if ($LASTEXITCODE) { Write-Output "FAIL: no release for $Tag"; exit 1 }
$missing = foreach ($ed in 'cuda', 'vulkan') {
    $want = "VOXIS_${version}_x64_$ed-setup.exe"
    if ($names -notcontains $want) { $want }
}
if ($missing) { Write-Output "FAIL: $Tag lacks $($missing -join ', ')"; exit 1 }
Write-Output "PASS: $Tag lists VOXIS_${version}_x64_cuda-setup.exe and VOXIS_${version}_x64_vulkan-setup.exe"
exit 0
