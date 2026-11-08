@echo off
rem Runs the frozen-source tray reproduction (criterion 64, F23) in the worktree of 889d48c that
rem scripts\windows\frozen-repro.sh prepared. %1 is that worktree; CARGO_TARGET_DIR is set by the script.
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSDIR=%%i"
if not defined VSDIR (echo no Visual Studio with the C++ tools & exit /b 1)
call "%VSDIR%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
if not defined CARGO_TARGET_DIR (echo set CARGO_TARGET_DIR outside the worktree & exit /b 1)
cd /d "%~1" || exit /b 1
cargo test --release --manifest-path apps/tray/src-tauri/Cargo.toml frozen_repro -- --test-threads=1
