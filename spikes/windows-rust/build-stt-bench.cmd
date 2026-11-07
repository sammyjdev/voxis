@echo off
rem Builds stt-bench (whisper.cpp + Vulkan) on Windows. Needs: VS C++ tools,
rem Vulkan SDK, LLVM (libclang for bindgen), CMake and Ninja. The VS generator
rem fails on ggml-vulkan's nested vulkan-shaders-gen project, so Ninja is used,
rem and a short target dir keeps the nested paths under MAX_PATH.
rem vswhere finds whichever Visual Studio has the C++ tools (18 Community here,
rem the Enterprise edition on GitHub's windows runners)
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSDIR=%%i"
if not defined VSDIR (echo no Visual Studio with the C++ tools & exit /b 1)
call "%VSDIR%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
set CMAKE_GENERATOR=Ninja
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
if not defined CARGO_TARGET_DIR set CARGO_TARGET_DIR=C:\vxt
cd /d "%~dp0"
cargo build -p stt-bench -p meetrec --release %*
