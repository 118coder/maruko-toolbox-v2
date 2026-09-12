@echo off
rem 构建环境包装：MinGW(gcc/windres) 追加到 PATH 尾部（避免 DLL 劫持）+ 统一 target 目录
set PATH=%PATH%;C:\mingw64\mingw64\bin
set CARGO_TARGET_DIR=E:\maruko-build
cd /d X:\src-tauri
%*
