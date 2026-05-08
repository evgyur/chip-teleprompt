@echo off
cd /d "%~dp0"
echo Starting Chip Teleprompter...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0Teleprompter.ps1"
if errorlevel 1 (
  echo.
  echo ERROR: Teleprompter failed to start. Please send a screenshot of this window to ChipGPT.
  echo.
  pause
)
