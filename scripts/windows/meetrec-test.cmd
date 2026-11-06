@echo off
rem Runs the meetrec tests on Windows (CUDA build). Extra args go to cargo test,
rem e.g. scripts\windows\meetrec-test.cmd models_are_found_in_the_first_place
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
set CMAKE_GENERATOR=Ninja
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
if not defined CARGO_TARGET_DIR set CARGO_TARGET_DIR=C:\vxc
if not defined CUDAARCHS set CUDAARCHS=89
cd /d "%~dp0..\..\spikes\windows-rust"
cargo test -p meetrec --release --no-default-features --features cuda %*
