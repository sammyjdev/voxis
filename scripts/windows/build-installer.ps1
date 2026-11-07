<#
Builds one VOXIS installer edition for Windows.

  powershell -File scripts\windows\build-installer.ps1 -Edition cuda
  powershell -File scripts\windows\build-installer.ps1 -Edition vulkan

Needs what spikes\windows-rust\build-stt-bench.cmd needs (VS C++ tools, LLVM,
CMake, Ninja, the Vulkan SDK) plus, for cuda, the CUDA Toolkit, and Node for the
tray. Prints the path of the edition-named installer last.
#>
param(
    [Parameter(Mandatory)][ValidateSet('cuda', 'vulkan')][string]$Edition,
    # arches baked into the CUDA build; CI passes the release set
    [string]$CudaArchs = '89',
    # the app and installer version; CI passes the tag without its leading v
    [string]$Version
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path "$PSScriptRoot\..\..").Path
$tauri = Join-Path $repo 'apps\tray\src-tauri'
$bin = Join-Path $tauri 'binaries'

# GUI-launched shells miss the machine PATH entries the toolchains add
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
foreach ($v in 'CUDA_PATH', 'VULKAN_SDK') {
    if (-not (Get-Item "env:$v" -ErrorAction SilentlyContinue)) {
        $val = [Environment]::GetEnvironmentVariable($v, 'Machine')
        if ($val) { Set-Item "env:$v" $val }
    }
}

# a short, edition-specific target dir: MAX_PATH, and never the dev build the
# running tray holds open
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = "C:\vxi$($Edition[0])" }
$features = if ($Edition -eq 'cuda') { '--no-default-features --features cuda' } else { '' }
if ($Edition -eq 'cuda') { $env:CUDAARCHS = $CudaArchs }
cmd /c "`"$repo\spikes\windows-rust\build-stt-bench.cmd`" $features"
if ($LASTEXITCODE) { throw "meetrec build failed ($LASTEXITCODE)" }

New-Item -ItemType Directory -Force $bin | Out-Null
Copy-Item "$env:CARGO_TARGET_DIR\release\meetrec.exe" "$bin\meetrec-x86_64-pc-windows-msvc.exe" -Force

$extra = @()
if ($Edition -eq 'cuda') {
    New-Item -ItemType Directory -Force "$bin\cuda" | Out-Null
    foreach ($dll in 'cublas64_13.dll', 'cublasLt64_13.dll') {
        Copy-Item "$env:CUDA_PATH\bin\x64\$dll" "$bin\cuda\$dll" -Force
    }
    $extra = @('--config', 'src-tauri/tauri.cuda.conf.json')
}
if ($Version) {
    $versionConf = Join-Path $tauri 'target\version.conf.json'
    New-Item -ItemType Directory -Force (Split-Path $versionConf) | Out-Null
    Set-Content $versionConf "{`"version`": `"$Version`"}" -Encoding ASCII
    $extra += @('--config', $versionConf)
}

# the tray's own cargo build must not reuse the meetrec target dir
$meetrecTarget = $env:CARGO_TARGET_DIR
Remove-Item env:CARGO_TARGET_DIR
Push-Location (Join-Path $repo 'apps\tray')
try {
    npx tauri build --bundles nsis @extra
    if ($LASTEXITCODE) { throw "tauri build failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
    $env:CARGO_TARGET_DIR = $meetrecTarget
}

$nsis = Join-Path $tauri 'target\release\bundle\nsis'
$built = Get-ChildItem $nsis -Filter '*-setup.exe' | Where-Object { $_.Name -notmatch '_(cuda|vulkan)-setup' } |
    Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $built) { throw "no installer in $nsis" }
$named = Join-Path $nsis ($built.Name -replace '-setup\.exe$', "_$Edition-setup.exe")
Move-Item $built.FullName $named -Force
Write-Output $named
