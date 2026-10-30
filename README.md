<p align="center">
  <img src="docs/img/logo.png" width="112" alt="concord logo">
</p>

<h1 align="center">concord</h1>

<p align="center"><b>Voice, screen sharing and chat for your group, hosted on one of your own PCs.</b></p>

concord is a self-hosted voice, video and chat app for small groups. The server runs on a group member's PC, with no central service. It is independent of Discord.

## Preview

| Voice room with two pinned streams | Voice room with a text channel docked |
|---|---|
| ![Voice room with two pinned streams and cameras below](docs/img/sala.png) | ![Voice room with a text channel next to the video](docs/img/sala-chat.png) |

| Chat with a reply, a mention and reactions | Profile card |
|---|---|
| ![Chat with a reply, a mention and reactions](docs/img/chat.png) | ![Profile card](docs/img/perfil.png) |

Screenshots show an older interface. People, servers and conversations are fictional.

## Platform support

| Setup | Status |
|---|---|
| Windows 11, NVIDIA RTX (NVENC) | Works: voice, cameras, Go Live with app audio, hosting |
| CPU encoding (no hardware encoder, or acceleration turned off) | Works: tested by forcing the CPU encoder on Windows 11 |
| AMD/Intel hardware encoding; dual-GPU laptops | Detected and self-tested; limited real-world testing |
| macOS on Apple Silicon (M1 or later) | Works: chat, voice, watching and sharing (screen, window, app audio, camera). Not yet: menu bar icon, notifications, global push-to-talk, hosting from the app |
| Windows on ARM, Intel Macs, Linux | Not supported yet |

## Requirements

- Windows 10 (version 2004 or later) or Windows 11, 64-bit; or Apple Silicon macOS 13 or later (see the feature limits above).
- A headset is recommended.
- A GPU with hardware H.264 encoding is recommended for cameras and Go Live. Without one, the app encodes on the CPU.
- To host: a PC that stays on while the group is online, plus either a VPN (Tailscale or ZeroTier) or the ability to open ports on your router.

## Download

On the [Releases](../../releases) page:

- **`concord-v1.0.2-PUBLICO.zip`** is the Windows app. Extract the folder (don't run it from inside the zip) and open `concord.exe`.
- **`concord-1.0.2-mac-arm64.zip`** is the Apple Silicon Mac app. Drag `concord.app` to Applications and follow the included first-open instructions.
- **`concord-servidor-v1.0.2.zip`** is the standalone server (optional, see below).

Windows binaries are unsigned; macOS builds are ad-hoc signed, not notarized. Only bypass OS download warnings if you trust the source. See [SECURITY.md](SECURITY.md).

## Host a server

### From the app (the easy way)

1. On the home screen, choose **Create or open server**.
2. The first time, the app downloads the voice server (LiveKit, about 19 MB, verified by a pinned SHA-256). It also asks your router (UPnP) to open the ports.
3. When Windows Firewall asks, allow concord on the networks you use. If Windows marks your network as public, tick public networks too; otherwise friends can't connect.
4. Use **Invite** to create a code and send it in a private message.

The server stays on while the app is open or in the tray. Whoever creates the server becomes its Owner.

### Standalone server

Extract the standalone release and follow its setup instructions (`README-SERVER.txt`, or `LEIA-ME-SERVIDOR.txt` in older downloads). Set `server_password` before inviting anyone and connect locally through `127.0.0.1` to claim ownership.

See [server setup](hub/README.md) for configuration, network/firewall setup and recovery.

## Invite your friends

- **With an invite code:** your friend chooses **Connect**, pastes the `vox1-...` code and types a name. No address or password is needed.
- **Without a code:** they enter the server address, a name and the server password, which you send privately.

Invite codes can expire, have a use limit, and be revoked at any time. A code hides the address from casual eyes, but anyone who joins can still learn the host's IP.

## Compatibility and limits

- **No TLS yet:** traffic between the app and the server isn't encrypted, except voice and video. Prefer a VPN to opening ports. See [SECURITY.md](SECURITY.md).
- **Router and ISP:** if your ISP uses CGNAT, your friends can't reach your PC directly. The app detects this and explains the options; a VPN works in that case, and so does IPv6 with the standalone server from v1.0.2.
- **Encoders:** NVENC, AMD and Intel (through Media Foundation) and a CPU fallback. The app tests each one on first start. If viewers report broken video, it switches automatically.
- **Go Live audio:** captures audio from the app you share only (Windows 10 2004+ process loopback). Your voice chat isn't captured.
- **Private channels:** people without access can't read or join them. They can still notice that messages were sent, because message numbers skip.
- **Downgrades:** while any channel is private, older server versions refuse to open the database. Make all channels public before going back to an older version.
- **Group size:** designed for groups of around 15 people.

## Privacy and local storage

No analytics or centrally hosted accounts. Connections are to your group's server, server-fetched link previews, and—when hosting—GitHub for LiveKit, Google/Cloudflare STUN for address discovery, and your router for UPnP.

| What | Where |
|---|---|
| Settings, saved servers, encoder test results | `%APPDATA%\concord\` |
| Servers you host from the app, soundboard cache, logs | `%LOCALAPPDATA%\concord\` |
| Saved passwords and login tokens | Inside those files, encrypted with Windows DPAPI for your Windows user |
| A standalone server's secrets and messages | The server's `data\` folder |

Never share a server's `data\` folder.

## Build from source

On Windows, use Rust stable (MSVC) and Visual Studio Build Tools with C++. On Apple Silicon macOS, use Rust stable and Xcode Command Line Tools; `tools/package-mac.sh` builds and packages the app.

```powershell
cargo build --release -p voxhub-app -p voxhub-server
```

The executables are in `target\release\` (`concord.exe` and `concord-server.exe`). The Cargo packages keep their old names, `voxhub-app` and `voxhub-server`.

## Diagnostics

Windows app logs: `%LOCALAPPDATA%\concord\logs\app`. Redact addresses, names and secrets before sharing.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development checks and CI.

When you report a problem, include:

- your operating system and version;
- your GPU model and driver version (for video problems);
- steps to reproduce;
- a redacted error message or log excerpt.

Keep IP addresses, passwords, invite codes, logs and `data\` files out of public issues and pull requests. To report a security problem, see [SECURITY.md](SECURITY.md).

## License

The code is MIT. See [LICENSE](LICENSE). The concord logo is not covered by the MIT license; please don't use it for other projects. Third-party libraries, fonts and icons are listed in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

Made by Cezar Callado · cezar.callado@gmail.com
