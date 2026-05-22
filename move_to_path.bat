@echo off
if not exist "%USERPROFILE%\.local\bin" mkdir "%USERPROFILE%\.local\bin"
copy /Y ".\target\x86_64-pc-windows-msvc\release\compr.exe" "%USERPROFILE%\.local\bin\"