@echo off
rem Manage concord firewall rules. Legacy filenames, rule names and markers remain compatible.
setlocal
cd /d "%~dp0"
set "CONCORD_FW_ACAO=liberar"
set "CONCORD_FW_BAT=%~f0"
set "CONCORD_FW_PASTA=%~dp0"
set "CONCORD_FW_MODO=aplicar"
if /i "%~1"=="/teste" set "CONCORD_FW_MODO=teste"
if /i "%~1"=="/teste" goto rodar
if /i "%~1"=="/elevado" goto rodar
echo.
echo  ALLOW CONCORD SERVER THROUGH WINDOWS FIREWALL
echo  ====================================================
echo.
echo  What this script does:
echo  - creates 3 INBOUND Windows Firewall rules, named "concord servidor (...)";
echo  - allows connections only to programs in this folder:
echo      bin\concord-server.exe  (server: TCP 7890)
echo      bin\livekit-server.exe  (voice: TCP 7880 and 7881, UDP 7882)
echo  - applies to all networks (Domain, Private, Public), IPv4 and IPv6;
echo  - if Windows created a BLOCK rule for these programs
echo    (for example, if its prompt was cancelled), it is disabled, not deleted;
echo  - running again replaces the rules; remove-firewall.bat undoes the changes.
echo.
echo  Nothing else changes: no other rules or router ports.
echo  Windows requires administrator permission to change firewall rules:
echo  after pressing a key, click "Yes" in the Windows prompt.
echo  (To cancel, close this window.)
echo.
pause
net session >nul 2>&1
if not errorlevel 1 goto rodar
powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -FilePath $env:CONCORD_FW_BAT -ArgumentList '/elevado' -Verb RunAs"
if errorlevel 1 (
  echo.
  echo Windows did not grant administrator permission: nothing changed.
  pause
)
exit /b
:rodar
powershell -NoProfile -ExecutionPolicy Bypass -Command "$s = [IO.File]::ReadAllText($env:CONCORD_FW_BAT); Invoke-Expression $s.Substring($s.IndexOf('#' + 'POWERSHELL' + '#'))"
set "RESULTADO=%errorlevel%"
echo.
pause
exit /b %RESULTADO%
#POWERSHELL# The part below runs in PowerShell (the batch file never gets here).
$ErrorActionPreference = 'Stop'
$teste = $env:CONCORD_FW_MODO -eq 'teste'
$remover = $env:CONCORD_FW_ACAO -eq 'remover'
$pasta = $env:CONCORD_FW_PASTA.TrimEnd('\')
$srv = Join-Path $pasta 'bin\concord-server.exe'
$lk = Join-Path $pasta 'bin\livekit-server.exe'
$hub = 7890; $sinal = 7880; $tcp = 7881; $udp = 7882
$cfg = Join-Path $pasta 'data\config.toml'
if (Test-Path -LiteralPath $cfg) {
  $t = [IO.File]::ReadAllText($cfg)
  if ($t -match '(?m)^\s*bind\s*=\s*"[^"]*:(\d+)"') { $hub = [int]$Matches[1] }
  if ($t -match '(?m)^\s*signal_port\s*=\s*(\d+)') { $sinal = [int]$Matches[1] }
  if ($t -match '(?m)^\s*rtc_tcp_port\s*=\s*(\d+)') { $tcp = [int]$Matches[1] }
  if ($t -match '(?m)^\s*rtc_udp_port\s*=\s*(\d+)') { $udp = [int]$Matches[1] }
}
$regras = @(
  @{ Nome = "concord servidor (servidor TCP $hub)"; Programa = $srv; Protocolo = 'TCP'; Portas = @("$hub") },
  @{ Nome = "concord servidor (voz TCP $sinal e $tcp)"; Programa = $lk; Protocolo = 'TCP'; Portas = @("$sinal", "$tcp") },
  @{ Nome = "concord servidor (voz UDP $udp)"; Programa = $lk; Protocolo = 'UDP'; Portas = @("$udp") }
)
# Block rules Windows itself made for these two programs (when its question was cancelled, or for
# the kind of network left unchecked) win over any allow rule, so they are switched off (never
# deleted) and marked; remover-firewall.bat switches them back on.
$marca = '[desativada pelo liberar-firewall.bat do concord]'
function Bloqueios {
  foreach ($f in @(Get-NetFirewallApplicationFilter -ErrorAction SilentlyContinue)) {
    $p = [Environment]::ExpandEnvironmentVariables([string]$f.Program)
    if ($p -ieq $srv -or $p -ieq $lk) {
      $f | Get-NetFirewallRule | Where-Object { $_.Direction -eq 'Inbound' -and $_.Action -eq 'Block' -and $_.Enabled -eq 'True' -and $_.Group -ne 'concord servidor' }
    }
  }
}
function Marcadas {
  @(Get-NetFirewallRule -ErrorAction SilentlyContinue | Where-Object { ([string]$_.Description).StartsWith($marca) })
}
if ($teste) { Write-Host 'TEST MODE: no changes, only a preview of what would happen.' }
try {
  $velhas = @(Get-NetFirewallRule -DisplayName 'concord servidor (*' -ErrorAction SilentlyContinue)
  foreach ($v in $velhas) {
    if ($remover) { Write-Host "Deleting rule: $($v.DisplayName)" } else { Write-Host "Replacing old rule: $($v.DisplayName)" }
    $v | Remove-NetFirewallRule -WhatIf:$teste
  }
  if ($remover) {
    $marcadas = Marcadas
    foreach ($m in $marcadas) {
      Write-Host "Re-enabling Windows-created block rule: $($m.DisplayName) ($($m.Profile))"
      $antes = ([string]$m.Description).Substring($marca.Length).TrimStart()
      if (-not $antes) { $antes = [string]$m.DisplayName }
      $m | Set-NetFirewallRule -Enabled True -Description $antes -WhatIf:$teste
    }
    if ($velhas.Count + $marcadas.Count -eq 0) { Write-Host 'No concord rules found: nothing to undo.' }
    elseif ($teste) { Write-Host 'TEST MODE: no rules deleted or re-enabled.' }
    else { Write-Host "Done: $($velhas.Count) rule(s) deleted, $($marcadas.Count) block(s) re-enabled." }
    exit 0
  }
  foreach ($r in $regras) {
    Write-Host "Creating: $($r.Nome)"
    Write-Host "  inbound, allow, $($r.Protocolo) $($r.Portas -join ' and '), all networks (Domain, Private, Public), IPv4 and IPv6"
    Write-Host "  program: $($r.Programa)"
    New-NetFirewallRule -DisplayName $r.Nome -Group 'concord servidor' -Description 'Created by concord allow-firewall.bat. To remove: remove-firewall.bat.' -Direction Inbound -Action Allow -Profile Any -Program $r.Programa -Protocol $r.Protocolo -LocalPort $r.Portas -WhatIf:$teste | Out-Null
  }
  foreach ($b in @(Bloqueios)) {
    Write-Host "Disabling a Windows-created block rule for this program: $($b.DisplayName) ($($b.Profile))"
    $b | Set-NetFirewallRule -Enabled False -Description "$marca $($b.Description)" -WhatIf:$teste
  }
  Write-Host ''
  if ($teste) { Write-Host 'TEST MODE: no rules created, deleted or disabled.'; exit 0 }
  Write-Host 'Done: server and voice are allowed through Windows Firewall on all networks (IPv4 and IPv6).'
  Write-Host 'Friends still need to reach this PC: IPv6, router forwarding or VPN (see README-SERVER.txt).'
} catch {
  Write-Host ''
  Write-Host "Failed: $($_.Exception.Message)"
  exit 1
}
