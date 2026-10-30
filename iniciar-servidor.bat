@echo off
rem Start concord's chat and voice server. Close this window or press Ctrl+C to stop.
rem Uses the stable release in bin\, not a development build.
rem Retains the legacy executable fallback for existing installations.
cd /d "%~dp0"
set SERVIDOR=bin\concord-server.exe
if not exist %SERVIDOR% set SERVIDOR=bin\voxhub-server.exe
if not exist %SERVIDOR% (
  echo Neither bin\concord-server.exe nor bin\voxhub-server.exe was found.
  echo Extract the standalone server release into this folder first.
  pause
  exit /b 1
)
%SERVIDOR% --data-dir data --livekit-bin bin\livekit-server.exe --console
pause
