# League of Legends screen-share handoff (experimental)

## Enable / disable

On **Windows**, open **Settings > Video > League of Legends** and enable
**Auto-switch client / game (experimental)**, then start sharing either the League client
window or an active game's window. The setting defaults to **off**, including when loading
older settings files. It is saved as `follow_league` in `settings.json`.

The setting applies to new streams. To revert to ordinary screen sharing, stop the stream,
turn the setting off, and start again. It does not affect macOS, monitor shares, audio-only
shares, or windows belonging to other applications. Successfully choosing another app or
monitor with **Change what you share** cancels League following for that source.

## Behaviour and limits

- Recognizes `LeagueClientUx.exe` (lobby) and `League of Legends.exe` (loading/game), ignoring
  executable-name case. Window titles and foreground focus are not used for detection.
- Prefers the game even if the lobby remains open. Returns to the lobby when the game window
  is destroyed, and discovers new window handles/PIDs when the client relaunches or another
  match starts. Alt-tab/minimizing the game does not by itself trigger a return to the lobby.
- Reuses the existing in-place source switch: no stream restart, viewer rejoin, or video-track
  republish. The preview and shared app audio follow the source. Audio remains off if it was
  not requested; a temporary audio failure does not disable it for the next handoff.
- Polls every 500 ms, requires a candidate to remain stable for at least 500 ms, and spaces
  failed capture attempts by at least 3 seconds between attempt starts. An unsuccessful
  switch preserves the old source. Multiple ambiguous game/client candidates are not guessed.
- Keeps the stream alive for up to **90 seconds** while the current window/capture is missing
  or has not delivered a first frame. Viewers may see a frozen last frame during the gap.
  If recovery fails (including quitting League entirely), the stream ends normally. **Stop
  sharing** still cancels it; it never falls back to the desktop or system-wide audio.
- Uses the existing Windows Graphics Capture backend, not an injected game-capture hook.
  Exclusive fullscreen can still be unsupported/blank; use League's **Borderless** mode in
  that case. This feature cannot bypass protected capture, elevation restrictions, or GPU
  failures.

## Research behind the approach

- [OBS's League capture guide](https://obsproject.com/kb/league-of-legends-capture-guide)
  identifies the two executables and uses separate lobby/game sources with automatic scene
  switching. Concord follows window lifetime rather than foreground focus, so alt-tabbing
  does not change what is shared.
- Riot's **Close client during game** option means the lobby can remain open or be restarted;
  the controller must handle both. Riot described the restart delay in its
  [client plans](https://web.archive.org/web/20170602135510/na.leagueoflegends.com/en/news/client/client-features/future-plans-client).
- [Discord's Windows capture documentation](https://support.discord.com/hc/en-us/articles/9410427556375--Windows-Capturing-Application-Window-for-Screen-Share-and-Go-Live)
  documents Windows Graphics Capture's exclusive-fullscreen limitation and recommends
  borderless mode. It is not evidence that automatic League switching always works there.

## Validation

The policy tests in `crates/vox-client-core/src/voice/league.rs` have no Win32 or media-runtime
requirements. They cover opt-in/exact detection, client staying open and restarting, multiple
matches, starting in-game, minimization, transient/ambiguous candidates, recycled HWNDs,
retry limits, silent/failed captures, and audio intent. Settings tests cover the default and
JSON round-trip. On a supported desktop build:

```sh
cargo test --locked -p vox-client-core --lib voice::league::tests
cargo test --locked -p voxhub-app --bin concord settings::tests
```

To run just the policy tests on Linux without linking LiveKit or enabling a desktop backend:

```sh
cargo build --locked -p vox-capture --lib
scratch=$(mktemp -d)
printf '#[path = "%s/crates/vox-client-core/src/voice/league.rs"]\nmod league;\n' "$PWD" > "$scratch/harness.rs"
capture=$(find target/debug/deps -name 'libvox_capture-*.rlib' -print -quit)
rustc --edition 2021 --test "$scratch/harness.rs" -L dependency=target/debug/deps \
  --extern "vox_capture=$capture" -o "$scratch/league-tests"
"$scratch/league-tests"
```

### Windows acceptance checklist (still required)

Use a second viewer and a custom/practice game; do not use a ranked match for initial testing.

1. **Toggle off:** ordinary League and non-League window/monitor sharing behaves as before.
2. **Toggle on, client stays open:** share the lobby, enter loading/game, then finish/leave.
   The viewer sees game then lobby without rewatching; run a second match.
3. Repeat with **Close client during game = Always**, if available. Confirm the relaunched
   client and its new process/window are captured automatically after the game ends.
4. Start sharing from an already-running game. End it and confirm the client is picked up.
5. Alt-tab/minimize during a match. It must not switch to the lobby, browser, or desktop.
6. Repeat with shared audio on and off. With audio on, confirm the match's sound and then the
   lobby's sound reach the viewer, without voice-chat echo or unrelated system audio. Check
   that the local preview follows and **Change what you share** marks the current window.
7. During a transition, manually switch to a non-League app/monitor, stop sharing, or leave
   voice. No later automatic handoff may override that action or revive the stream.
8. Quit all League windows. The stream should end after the grace period; stopping it manually
   during that wait should work. Exercise a failed/blank capture and ensure retries stop.
9. Test the usual hardware encoder and **Hardware acceleration off** (software). Prefer
   Borderless initially, then separately test the user's normal fullscreen mode.

For failures, collect the Windows/GPU/encoder details, League's window mode and client-close
setting, and redacted log lines containing `League handoff`, `screenshare source switched`,
`capture error`, or `Go Live ended by capture`. Windows app logs are under
`%LOCALAPPDATA%\concord\logs\app`.
