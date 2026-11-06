<#
Smoke tests for the VOXIS Windows installers (checks C1, C5, C6, C15-C17 in
.specs\features\windows-installer\checks.md). Exits 0 when the check holds.

  powershell -File scripts\windows\installer-smoke.ps1 -Edition cuda -Check install
  powershell -File scripts\windows\installer-smoke.ps1 -Edition cuda -Check overlay
  powershell -File scripts\windows\installer-smoke.ps1 -Edition cuda -Check no-toolkit
  powershell -File scripts\windows\installer-smoke.ps1 -Edition vulkan -Check transcribe
  powershell -File scripts\windows\installer-smoke.ps1 -Edition cuda,vulkan -Check replace
  powershell -File scripts\windows\installer-smoke.ps1 -Edition vulkan -Check uninstall-keeps-models

Installs silently (/S) from this non-elevated shell and uninstalls at the end
unless -Keep. The installers are read from the tray's bundle\nsis folder unless
-InstallerDir says otherwise. overlay and transcribe need the whisper models:
VOXIS_MODELS, else the usual lookup. overlay needs the dev tray and its
dictation daemon closed (one F13 hook, one single-instance tray).
#>
param(
    [Parameter(Mandatory)][string[]]$Edition,
    [Parameter(Mandatory)][ValidateSet('install', 'overlay', 'no-toolkit', 'transcribe', 'replace', 'uninstall-keeps-models')][string]$Check,
    [string]$InstallerDir,
    [switch]$Keep
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path "$PSScriptRoot\..\..").Path
# Windows PowerShell 5.1 leaves $PSScriptRoot empty inside param defaults
if (-not $InstallerDir) { $InstallerDir = Join-Path $repo 'apps\tray\src-tauri\target\release\bundle\nsis' }
$uninstallRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'

function Fail($msg) { Write-Output "FAIL: $msg"; exit 1 }
function Pass($msg) { Write-Output "PASS: $msg" }

function Installer($ed) {
    $f = Get-ChildItem $InstallerDir -Filter "*_$ed-setup.exe" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime | Select-Object -Last 1
    if (-not $f) { Fail "no *_$ed-setup.exe in $InstallerDir" }
    $f.FullName
}

function Entries {
    @(Get-ChildItem $uninstallRoot -ErrorAction SilentlyContinue |
        ForEach-Object { Get-ItemProperty $_.PSPath } |
        Where-Object { $_.DisplayName -eq 'VOXIS' })
}

function InstallDir {
    $e = Entries | Select-Object -First 1
    if (-not $e) { Fail 'no HKCU uninstall entry named VOXIS' }
    # Tauri's NSIS writes the uninstaller into the install dir
    Split-Path ($e.UninstallString -replace '"', '' -replace ' /.*$', '')
}

function Install($ed) {
    $p = Start-Process (Installer $ed) -ArgumentList '/S' -Wait -PassThru
    if ($p.ExitCode -ne 0) { Fail "$ed installer exited $($p.ExitCode)" }
}

function Uninstall {
    foreach ($e in Entries) {
        $exe = $e.UninstallString -replace '"', '' -replace ' /.*$', ''
        if (Test-Path $exe) { Start-Process $exe -ArgumentList '/S' -Wait | Out-Null }
    }
    # the NSIS uninstaller re-launches itself from %TEMP% and returns early
    for ($i = 0; $i -lt 30 -and (Entries).Count; $i++) { Start-Sleep -Milliseconds 500 }
}

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if ($admin) { Fail 'run from a non-elevated shell: the point is that no UAC prompt is needed' }

try {
    switch ($Check) {
        'install' {
            Install $Edition[0]
            $dir = InstallDir
            if (-not $dir.StartsWith($env:LOCALAPPDATA, 'OrdinalIgnoreCase')) { Fail "installed outside %LOCALAPPDATA%: $dir" }
            foreach ($f in 'VOXIS.exe', 'meetrec.exe') {
                if (-not (Test-Path (Join-Path $dir $f))) { Fail "$f missing in $dir" }
            }
            $lnk = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\VOXIS.lnk'
            if (-not (Test-Path $lnk)) { Fail "no Start menu shortcut at $lnk" }
            Pass "per-user install in $dir with VOXIS.exe, meetrec.exe and $lnk"
        }
        'no-toolkit' {
            Install $Edition[0]
            $dir = InstallDir
            $saved = $env:Path
            $env:Path = 'C:\Windows\System32;C:\Windows'
            try {
                & (Join-Path $dir 'meetrec.exe') devices --json | Out-Null
                $code = $LASTEXITCODE
            } finally { $env:Path = $saved }
            if ($code -ne 0) { Fail "meetrec devices --json exited $code without the CUDA Toolkit on PATH" }
            Pass 'meetrec devices --json exits 0 with PATH = System32;Windows'
        }
        'overlay' {
            if (Get-Process tray, VOXIS -ErrorAction SilentlyContinue) { Fail 'close the running VOXIS tray first' }
            Install $Edition[0]
            $dir = InstallDir
            $trayJson = Join-Path $env:USERPROFILE '.config\voice-at\tray.json'
            $frontend = Join-Path $repo 'packages\laris\frontend'
            $peers = Join-Path $env:USERPROFILE '.config\voice-at\peers.json'
            # the installed copy must not lean on the dev config or the checkout
            if (Test-Path $trayJson) { Move-Item $trayJson "$trayJson.smoke" -Force }
            Rename-Item $frontend 'frontend.smoke'
            $started = Get-Date
            $app = Start-Process (Join-Path $dir 'VOXIS.exe') -PassThru
            try {
                $peer = $null
                for ($i = 0; $i -lt 120 -and -not $peer; $i++) {
                    Start-Sleep -Milliseconds 500
                    if ((Test-Path $peers) -and (Get-Item $peers).LastWriteTime -gt $started) {
                        $peer = (Get-Content $peers -Raw | ConvertFrom-Json).laris
                    }
                }
                if (-not $peer) { Fail 'the dictation daemon never published peers.json' }
                $r = Invoke-WebRequest "$($peer.url)/overlay?token=$($peer.token)" -UseBasicParsing
                if ($r.StatusCode -ne 200) { Fail "GET /overlay -> $($r.StatusCode)" }
                if ($r.Content -notmatch '<title>') { Fail 'GET /overlay body has no <title>' }
                Pass "GET /overlay -> 200 from $dir with the checkout's frontend absent"
            } finally {
                Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
                Get-Process meetrec -ErrorAction SilentlyContinue |
                    Where-Object { $_.Path -like "$dir*" } | Stop-Process -Force -ErrorAction SilentlyContinue
                Start-Sleep 1
                Rename-Item (Join-Path $repo 'packages\laris\frontend.smoke') 'frontend'
                if (Test-Path "$trayJson.smoke") { Move-Item "$trayJson.smoke" $trayJson -Force }
            }
        }
        'transcribe' {
            Install $Edition[0]
            $dir = InstallDir
            $bad = Get-ChildItem $dir -Filter 'cublas*.dll'
            if ($Edition[0] -eq 'vulkan' -and $bad) { Fail "vulkan install carries $($bad.Name -join ', ')" }
            $work = Join-Path $env:TEMP "voxis-smoke-$PID"
            New-Item -ItemType Directory -Force $work | Out-Null
            Add-Type -AssemblyName System.Speech
            $s = New-Object System.Speech.Synthesis.SpeechSynthesizer
            $s.SelectVoiceByHints('NotSet', 'NotSet', 0, [Globalization.CultureInfo]'pt-BR')
            $s.SetOutputToWaveFile((Join-Path $work 'mic.wav'))
            $s.Speak('Este é um teste de transcrição do voxis.')
            $s.Dispose()
            $models = if ($env:VOXIS_MODELS) { $env:VOXIS_MODELS } else { Join-Path $env:LOCALAPPDATA 'voxis\models' }
            if (-not (Test-Path "$models\ggml-large-v3-turbo.bin")) { $models = Join-Path $env:USERPROFILE '.cache\voxis-bench\models' }
            & (Join-Path $dir 'meetrec.exe') transcribe $work "$models\ggml-large-v3-turbo.bin" "$models\ggml-silero-v5.1.2.bin" pt 2>$null | Out-Null
            $t = Get-Content (Join-Path $work 'transcript.md') -Raw -Encoding UTF8
            Remove-Item $work -Recurse -Force
            if ($t -notmatch 'transcrição') { Fail "transcript lacks 'transcrição': $t" }
            Pass "$($Edition[0]) meetrec transcribed the synthesised sentence"
        }
        'replace' {
            Install $Edition[0]
            Install $Edition[1]
            $n = (Entries).Count
            if ($n -ne 1) { Fail "$n uninstall entries named VOXIS" }
            $dir = InstallDir
            if ($Edition[1] -eq 'vulkan' -and (Get-ChildItem $dir -Filter 'cublas*.dll')) { Fail 'cublas DLLs left behind after switching to vulkan' }
            Pass "one VOXIS entry after $($Edition -join ' -> ')"
        }
        'uninstall-keeps-models' {
            $models = Join-Path $env:LOCALAPPDATA 'voxis\models'
            New-Item -ItemType Directory -Force $models | Out-Null
            $marker = Join-Path $models 'smoke-marker.bin'
            Set-Content $marker 'keep me'
            Install $Edition[0]
            Uninstall
            $left = Test-Path $marker
            Remove-Item $marker -ErrorAction SilentlyContinue
            if ((Entries).Count) { Fail 'the uninstaller left its entry behind' }
            if (-not $left) { Fail "uninstall removed $models" }
            Pass "uninstall kept $models"
        }
    }
} finally {
    if (-not $Keep -and $Check -ne 'uninstall-keeps-models') { Uninstall }
}
exit 0
