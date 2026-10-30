# concord-server

Chat, presence, authentication and LiveKit supervision for voice/video.

## Run

From the repository root on Windows:

```powershell
cargo build --locked --release -p voxhub-server
powershell -ExecutionPolicy Bypass -File hub\scripts\fetch-livekit.ps1
.\target\release\concord-server.exe --data-dir .\data
```

The downloader verifies a pinned SHA-256, not just the release's checksum file.
Ctrl+C stops both processes; a Windows Job Object also stops LiveKit if the hub crashes.

| Option | Default / purpose |
|---|---|
| `--data-dir <dir>` | `./data` |
| `--livekit-bin <exe>` | `./bin/livekit-server.exe`, relative to the working directory |
| `--console` | Also log to the terminal |
| `--log-dir <dir>` | Override the log directory |
| `--no-livekit` | Do not start LiveKit |
| `--no-diag` | Skip startup network diagnostics |

## Configuration

First run creates `<data-dir>\config.toml`. Edit and restart:

- `server_password`: shared login password; empty allows anyone who can reach the server.
- `public_host`: public IP/DDNS name clients use for LiveKit. `127.0.0.1` limits voice to this PC.
- `[livekit] use_external_ip = true`: advertise the public IP for remote friends.
- `api_key` / `api_secret`: generated automatically; keep secret.
- `owner = "name"`: optionally pin ownership; place before `[livekit]`.

Data also includes `voxhub.db` (SQLite/WAL) and generated `livekit.yaml`. Existing data,
invites and wire formats remain compatible with voxhub v1.0.0. **Never share the data folder.**
Hub traffic lacks TLS; prefer a VPN. See [SECURITY.md](../SECURITY.md).

## Ownership

An unowned server can be claimed by:

- **Local login:** connect to `http://127.0.0.1:7890` from the server PC. LAN/public addresses
  do not count, even on that PC. The first login or reconnect becomes Owner.
- **Claim code:** sign in, choose **Claim server**, and enter the single-use code from the
  server console/log. It changes each unowned startup; failed attempts incur a server-wide
  exponential delay up to one minute.
- **Configuration:** pin `owner = "name"`, disabling claims and in-app ownership transfers.

Owner permissions activate only after a personal password is set. In **Server → Members**,
the Owner can transfer ownership to someone with a personal password; the old Owner becomes
Admin. Ownership changes are logged.

**Local proxies/tunnels can make remote users appear local.** Forwarding headers and a
non-loopback `Host` prevent automatic claiming, but tunnels stripping those headers can
bypass that protection. Pin `owner`, or claim before exposing the server.

### Recovery

These commands update the database without starting the server. A running server notices
within about two seconds:

```powershell
# Prints a single-use login code, valid until password replacement or at most 3 days.
.\concord-server.exe --data-dir .\data --reset-password <name>
.\concord-server.exe --data-dir .\data --reset-owner
```

Before resetting ownership for someone else, close the previous Owner's app: a loopback
reconnect can reclaim it. Remove any pinned `owner` from `config.toml` separately.

## Private channels

**Server → Channels** lets Admin/Owner grant access by Moderator/Member role or individual.
Owner and Admin always have access. Others cannot see the channel, history, attachments,
reactions or voice participants; losing access hides the channel and disconnects voice.

**Do not downgrade with private channels present.** A database guard blocks servers v0.8.x
and earlier, which would otherwise expose them. First use v0.9.0+ to make every channel
public or delete the private channels.

## Network

For public hosting, forward these ports to the host PC:

| Port | Protocol | Purpose |
|---|---|---|
| 7890 | TCP | Hub API/chat |
| 7880 | TCP | LiveKit signaling |
| 7881 | TCP | LiveKit media fallback |
| 7882 | UDP | LiveKit media |

`allow-firewall.bat` permits only the server/LiveKit executables and configured ports on all
Windows profiles, IPv4 and IPv6. Conflicting Windows block rules are disabled, not deleted;
`remove-firewall.bat` restores them. A home network marked Public needs Public permission.
Stop the server on public Wi-Fi.

The default `bind = "[::]:7890"` accepts IPv4 and IPv6; old all-interface `0.0.0.0` configs
also become dual-stack. Set `ipv6 = false` for IPv4 only. Global IPv6 permits direct access
even behind IPv4 CGNAT, but routers may require an incoming IPv6 firewall exception. Invites
include it when `public_host` is empty or `127.0.0.1`.

IPv4 forwarding cannot bypass CGNAT (for example a WAN address in `100.64.x.x` or differing
from the internet-visible address). Use IPv6, a mesh VPN with its IP in `public_host`, or a
separately configured relay. Startup diagnostics report network/firewall status without changes.

## Diagnostics

Windows logs: `%LOCALAPPDATA%\concord\logs\hub\hub.<date>.log`, daily rotation, five files.
Use `$env:VOX_LOG="debug"` for detail. Logs may contain addresses, names and recovery codes.
For tests, see [CONTRIBUTING.md](../CONTRIBUTING.md).
