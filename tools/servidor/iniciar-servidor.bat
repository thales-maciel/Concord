@echo off
rem Start concord's server. Close this window or press Ctrl+C to stop.
rem Startup diagnostics report IPv4/IPv6, router and Windows Firewall status,
rem with suggestions if friends cannot connect.
cd /d "%~dp0"
if not exist bin\livekit-server.exe (
  echo The voice server is missing. Run download-livekit.bat first.
  pause
  exit /b 1
)
echo Tip: run allow-firewall.bat before first use to allow the server through Windows Firewall.
bin\concord-server.exe --data-dir data --livekit-bin bin\livekit-server.exe --console
pause
