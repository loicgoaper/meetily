@echo off
rem Meetily perso: run a command inside the MSVC developer environment so that
rem bindgen (whisper-rs-sys, llama-cpp-sys-2) finds the Windows SDK and CRT headers.
rem Requires LLVM 18.x: whisper-rs-sys pins bindgen 0.69, which emits an opaque
rem whisper_full_params with libclang 22.
rem Usage: scripts\fork-dev-env.cmd cargo build
rem        scripts\fork-dev-env.cmd pnpm tauri:build:cpu
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %errorlevel%
if not defined LIBCLANG_PATH set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"
if not defined BINDGEN_EXTRA_CLANG_ARGS set "BINDGEN_EXTRA_CLANG_ARGS=-IC:/PROGRA~1/LLVM/lib/clang/18/include"
rem Tests: use the bundled ONNX Runtime, not the older C:\Windows\System32\onnxruntime.dll
if not defined ORT_DYLIB_PATH if exist "%~dp0..\src-tauri\binaries\onnxruntime\onnxruntime.dll" set "ORT_DYLIB_PATH=%~dp0..\src-tauri\binaries\onnxruntime\onnxruntime.dll"
%*
