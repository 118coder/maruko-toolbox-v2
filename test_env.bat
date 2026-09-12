@echo off
set MSVC=D:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.37.32822
set SDK=D:\Windows Kits\10
set SDKV=10.0.22621.0
set INCLUDE=%MSVC%\include;%SDK%\Include\%SDKV%\um;%SDK%\Include\%SDKV%\shared;%SDK%\Include\%SDKV%\ucrt;%SDK%\Include\%SDKV%\winrt;%SDK%\Include\%SDKV%\cppwinrt
set LIB=%MSVC%\lib\x64;%SDK%\Lib\%SDKV%\um\x64;%SDK%\Lib\%SDKV%\ucrt\x64
set PATH=%MSVC%\bin\HostX64\x64;%SDK%\bin\%SDKV%\x64;%SDK%\bin\%SDKV%\um\x64;%PATH%
cd /d C:\Users\12290\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\vswhom-sys-0.1.3
"D:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.37.32822\bin\HostX64\x64\cl.exe" -nologo -MD -Brepro -W4 /Zm2000 -FoE:\maruko-build\cctest\v2.o -c ext/vswhom.cpp
echo CLEXIT=%errorlevel%
