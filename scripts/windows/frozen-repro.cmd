@echo off
rem Runs the frozen-source reproductions (criterion 64) in a worktree of 889d48c that
rem scripts\windows\frozen-repro.sh prepared. %1 is that worktree; the target folder comes
rem from CARGO_TARGET_DIR (set it outside the worktree). Same toolchain setup as meetrec-test.cmd.
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSDIR=%%i"
if not defined VSDIR (echo no Visual Studio with the C++ tools & exit /b 1)
call "%VSDIR%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
set CMAKE_GENERATOR=Ninja
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
if not defined CUDAARCHS set CUDAARCHS=89
if not defined CARGO_TARGET_DIR (echo set CARGO_TARGET_DIR outside the worktree & exit /b 1)
cd /d "%~1\spikes\windows-rust" || exit /b 1
cargo test -p meetrec --release --no-default-features --features cuda frozen_repro -- --test-threads=1
