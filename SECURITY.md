# Security

concord is intended for small groups of trusted friends, not public communities.

## Transport and exposure

**Hub traffic has no HTTPS/TLS.** Passwords, invites, remembered logins and messages travel
unencrypted and can be intercepted. Call audio/video encryption does not protect that traffic.
Prefer Tailscale or ZeroTier to public port forwarding.

Hosting from the app requests UPnP mappings, exposing the server until it stops. Disable
router UPnP and use a VPN if you do not want public exposure. For manual forwarding, use only
TCP 7890/7880/7881 and UDP 7882; never the router's DMZ setting.

Keep Windows Firewall enabled. `allow-firewall.bat` permits concord on all network profiles;
stop hosting on public Wi-Fi or undo its rules with `remove-firewall.bat`.

## Credentials and invites

- Set a strong, unique `server_password`. Empty means anyone who can reach the server may join.
- Set a unique personal password in **Settings → Personal password** to protect your username.
  Owner, Admin and Moderator permissions require one.
- Treat `vox1-...` invites as passwords and share privately. They bypass the server password,
  can expire or be revoked, and may have use limits. Revocation does not remove existing members.
- Invites create new accounts; they cannot take over an existing protected name.
- Saved passwords/tokens are withheld when an address answers as a different server identity.
  This check does not replace authenticated TLS.

## Storage and privacy

| Data | Location |
|---|---|
| App-hosted servers | `%LOCALAPPDATA%\concord\host\<id>\` |
| Standalone server | `data\` |
| Windows app credentials | `%APPDATA%\concord\`, protected by user-scoped DPAPI |
| macOS app credentials | Login Keychain; configuration stores opaque references |

**Never share server data folders.** They contain passwords, keys and message databases.
The host can read stored messages, including DMs, through the database even though the app
hides other people's DMs from Admin/Owner. Logs may contain addresses, names and recovery
codes; share only redacted excerpts privately.

## Private channels and downgrades

Unauthorized users cannot see private channels or their content and cannot join their voice
rooms. Revoking access disconnects voice; short-lived tokens and repeated enforcement limit
re-entry. Global message IDs still reveal activity through gaps, though not hidden content.

Servers v0.8.x and earlier would expose private channels. A database guard blocks them while
any private channel exists. Before downgrading, make all channels public or delete private
ones with the newer server. See [server documentation](hub/README.md#private-channels).

## Downloads

Use trusted sources. Windows executables are unsigned; the macOS app is ad-hoc signed,
not Developer ID signed or notarized.

## Reporting vulnerabilities

Use GitHub's **Report a vulnerability** if enabled. Otherwise open an issue without exploit
details asking for a private contact. Include the version and reproduction steps using test
accounts; never publish credentials, invites, tokens or raw server data.
