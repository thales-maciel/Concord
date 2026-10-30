@echo off
rem Download official LiveKit 1.13.7 (~19 MB) and verify the pinned SHA-256.
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "hub\scripts\fetch-livekit.ps1"
pause
