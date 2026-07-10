@echo off
setlocal
cd /d "%~dp0"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0VERIFY-WINDOWS.ps1"
if errorlevel 1 (
  echo.
  echo VERIFICATION FAILED. Do not use this copy until the damaged file is replaced.
  pause
  exit /b 1
)
echo.
echo VERIFICATION PASSED. RetroPort is intact.
pause
