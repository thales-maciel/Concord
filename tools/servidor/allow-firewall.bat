@echo off
if /i "%~1"=="/test" (
  call "%~dp0liberar-firewall.bat" /teste
) else (
  call "%~dp0liberar-firewall.bat" %*
)
exit /b %errorlevel%
