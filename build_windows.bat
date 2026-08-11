@echo off
setlocal
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build_windows.ps1"
if errorlevel 1 exit /b %errorlevel%
echo.
echo SleufBase.exe staat in dist\
endlocal
