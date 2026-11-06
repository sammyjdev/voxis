<#
Check C19 (.specs\features\windows-installer\checks.md): the README's Windows
section links the latest release, names both installers, ties the CUDA one to
NVIDIA, and states the first-run download size. Exits 0 when it does.
#>
$ErrorActionPreference = 'Stop'
$readme = Get-Content (Join-Path $PSScriptRoot '..\..\README.md') -Raw -Encoding UTF8
$m = [regex]::Match($readme, '(?ms)^## Windows\s*$(.*?)(?=^## |\z)')
if (-not $m.Success) { Write-Output 'FAIL: no "## Windows" section'; exit 1 }
$section = $m.Groups[1].Value
$missing = @()
foreach ($needle in '/releases/latest', '_x64_cuda-setup.exe', '_x64_vulkan-setup.exe', 'NVIDIA', '1.6 GB') {
    if (-not $section.Contains($needle)) { $missing += $needle }
}
$cudaRow = ($section -split "`n") | Where-Object { $_ -match '_x64_cuda-setup\.exe' } | Select-Object -First 1
if ($cudaRow -notmatch 'NVIDIA') { $missing += 'NVIDIA beside the CUDA edition' }
if ($missing) { Write-Output "FAIL: Windows section lacks $($missing -join ', ')"; exit 1 }
Write-Output 'PASS: README Windows section links the release, names both editions and the download size'
exit 0
