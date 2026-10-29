@echo off
rem 构建环境包装：MinGW(gcc/windres) 追加到 PATH 尾部（避免 DLL 劫持）+ 统一 target 目录
rem CARGO_HOME 必须为纯 ASCII 路径：GNU ld 处理不了中文路径下的 -L（windows_x86_64_gnu 导入库）
set PATH=%PATH%;C:\mingw64\mingw64\bin
set CARGO_TARGET_DIR=E:\maruko-build
set CARGO_HOME=E:\maruko-build\cargo-home
cd /d X:\src-tauri
%*
