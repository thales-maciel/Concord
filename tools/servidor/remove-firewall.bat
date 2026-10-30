@echo off
if /i "%~1"=="/test" (
  call "%~dp0remover-firewall.bat" /teste
) else (
  call "%~dp0remover-firewall.bat" %*
)
exit /b %errorlevel%
