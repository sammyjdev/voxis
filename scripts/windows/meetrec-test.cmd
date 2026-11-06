@echo off
rem Runs the meetrec tests on Windows (CUDA build). Extra args go to cargo test,
rem e.g. scripts\windows\meetrec-test.cmd models_are_found_in_the_first_place
rem vswhere finds whichever Visual Studio has the C++ tools
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSDIR=%%i"
if not defined VSDIR (echo no Visual Studio with the C++ tools & exit /b 1)
call "%VSDIR%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
set CMAKE_GENERATOR=Ninja
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
if not defined CARGO_TARGET_DIR set CARGO_TARGET_DIR=C:\vxc
if not defined CUDAARCHS set CUDAARCHS=89
cd /d "%~dp0..\..\spikes\windows-rust"
cargo test -p meetrec --release --no-default-features --features cuda %*
