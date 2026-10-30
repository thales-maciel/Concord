CONCORD — STANDALONE SERVER
==========================

SETUP
-----
1. Extract the package to a permanent folder; do not run inside the ZIP.
2. Run download-livekit.bat to download and verify the pinned voice server.
3. Run allow-firewall.bat and accept administrator permission. It allows the server
   and voice on all Windows profiles, IPv4 and IPv6. Allow the Public profile too
   if Windows marks your current network Public.
4. Run start-server.bat to create data\config.toml, then stop with Ctrl+C.
5. Set a strong server_password in that file and restart.
6. Open concord.exe on THIS PC and connect to 127.0.0.1 to become Owner.
   Set the personal password when prompted.
7. Make the server reachable, then use Invite in the app and share the code privately:
   - VPN (Tailscale/ZeroTier): safer, works behind CGNAT, no router forwarding.
   - IPv6: direct access may require a router IPv6 firewall exception.
   - IPv4: forward TCP 7890/7880/7881 and UDP 7882 to this PC; cannot bypass CGNAT.

Startup diagnostics report network/firewall status and suggestions without changing settings.
Set ipv6 = false in data\config.toml if you need IPv4 only.

FIREWALL
--------
Rules cover only bin\concord-server.exe and bin\livekit-server.exe on configured ports.
Conflicting Windows block rules are disabled, not deleted. remove-firewall.bat removes
concord's rules and restores those blocks. Preview changes with allow-firewall.bat /test.

Older script names still work:
  download-livekit.bat  = baixar-livekit.bat
  allow-firewall.bat    = liberar-firewall.bat
  remove-firewall.bat   = remover-firewall.bat
  start-server.bat      = iniciar-servidor.bat

SECURITY
--------
Hub traffic is unencrypted; prefer a VPN. Stop hosting on public Wi-Fi.
Never share data\: it contains passwords, keys and messages. See SECURITY.md in the repository.
