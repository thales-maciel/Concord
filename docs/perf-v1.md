# v1.0 resource usage: before and after (2026-10-17)

The v1.0 plan's resource budget was at most +10% RAM per scenario, unchanged idle CPU,
and unchanged GPU use. These historical measurements compare each UI phase with the
v0.9.1 release binary (`releases\v0.9.1\voxhub.exe`, identical to a build of that tag).
All numeric tables below retain the original measurements; they are not new results.

## Method

`tools\perf.ps1` runs in background test mode: no focus stealing or audible playback. Each
scenario has its own development hub (7997, LiveKit 7970–7972). The measured client is
always `pfA1`. After settling, it is sampled once per second for 20 seconds:

- Mean working set and private bytes.
- CPU: process CPU time / wall time, as a percentage of one core.
- GPU: `\GPU Engine(pid_<PID>*)\Utilization Percentage`, summed over all engines and,
  separately, over 3D engines; means over the sampling interval.

Scenarios:

- **(a) Home idle:** a normal, non-autotest run with isolated empty APPDATA/LOCALAPPDATA,
  no saved servers and no tray icon.
- **(b) Voice call with chat:** another client connected, text chat visible with a few
  messages, no activity (`voice --show previews`, settled).
- **(c) Watching a stream:** Go Live from `test_window`, inline after role `b` checks
  finish (idle at step 29).
- **(d) Camera on:** synthetic local camera (`VOX_FAKE_CAMERA=1`), another client in the room.

Two runs per scenario/version unless otherwise noted. Summary tables report their means.
Entering voice trims the process working set (`stats::trim_memory`), so private bytes are
more stable for call comparisons. Watching/camera autotests request a frame every 100 ms
in both versions, adding CPU usage that ordinary users would not see.

## Phase 1: theme, fonts, icons and components

Applied without changing screen layout.

| Scenario | Working set (MB) v0.9.1 → phase 1 | Private bytes (MB) v0.9.1 → phase 1 | CPU (% of 1 core) | GPU, all engines (%) | GPU 3D (%) |
|---|---|---|---|---|---|
| (a) Home idle | 81.6 → 81.5 (-0.1%) | 80.2 → 79.9 (-0.3%) | 0.11 → 0.00 | 0.00 → 0.00 | 0.00 → 0.00 |
| (b) Call with chat | 20.3 → 21.0 (+3.7%) | 108.2 → 109.6 (+1.2%) | 1.61 → 1.34 | 0.00 → 0.00 | 0.00 → 0.00 |
| (c) Watching a stream | 59.8 → 59.8 (+0.1%) | 170.5 → 168.3 (-1.3%) | 12.14 → 12.54 | 1.58 → 1.55 | 1.56 → 1.53 |
| (d) Camera on | 43.5 → 44.0 (+1.3%) | 227.4 → 228.1 (+0.3%) | 7.75 → 7.87 | 4.31 → 4.31 | 1.98 → 1.99 |

**Within budget.** At most +3.7% working set (about 0.7 MB for calls) and +1.2% private
bytes. Idle home CPU is zero; GPU remains zero for idle home/calls because nothing repaints.
Other CPU differences are within v0.9.1 run-to-run variation (watching: 11.7–12.6%).

### Individual runs

| Run | Scenario | WS (MB) | Private (MB) | CPU (%) | GPU all (%) | GPU 3D (%) |
|---|---|---|---|---|---|---|
| v0.9.1 #1 | home | 81.7 | 80.3 | 0.07 | 0 | 0 |
| v0.9.1 #2 | home | 81.5 | 80 | 0.15 | 0 | 0 |
| v0.9.1 #1 | call | 20.4 | 107 | 1.53 | 0 | 0 |
| v0.9.1 #2 | call | 20.2 | 109.5 | 1.69 | 0 | 0 |
| v0.9.1 #1 | watch | 59.7 | 170.7 | 12.56 | 1.56 | 1.53 |
| v0.9.1 #2 | watch | 59.8 | 170.3 | 11.73 | 1.6 | 1.58 |
| v0.9.1 #1 | camera | 43.3 | 229.4 | 7.29 | 4.32 | 1.99 |
| v0.9.1 #2 | camera | 43.7 | 225.5 | 8.21 | 4.3 | 1.97 |
| phase 1 #1 | home | 81.5 | 80.2 | 0 | 0 | 0 |
| phase 1 #2 | home | 81.6 | 79.6 | 0 | 0 | 0 |
| phase 1 #1 | call | 21.2 | 111.3 | 1.69 | 0 | 0 |
| phase 1 #2 | call | 20.9 | 107.9 | 1 | 0 | 0 |
| phase 1 #1 | watch | 59.7 | 167.6 | 13.28 | 1.51 | 1.49 |
| phase 1 #2 | watch | 59.9 | 169.1 | 11.8 | 1.59 | 1.57 |
| phase 1 #1 | camera | 44.1 | 229.2 | 7.82 | 4.31 | 2 |
| phase 1 #2 | camera | 44 | 226.9 | 7.91 | 4.31 | 1.98 |

### Binary and font sizes

| | Bytes |
|---|---|
| Atkinson Hyperlegible Next Regular (static instance) | 48 128 |
| Atkinson Hyperlegible Next SemiBold (static instance) | 48 036 |
| Bricolage Grotesque SemiBold (static instance) | 90 992 |
| Phosphor regular, 59 selected icons (full file: 488 636) | 18 788 |
| **New fonts total (4 files)** | **205 944** |
| `voxhub.exe` v0.9.1 → phase 1 | 69 417 984 → 69 693 952 (+275 968, +0,4%) |

### Reproduction

```powershell
$env:CARGO_TARGET_DIR = "target-v02"; cargo build --release --workspace
powershell -ExecutionPolicy Bypass -File tools\perf.ps1 -Exe <v0.9.1>\voxhub.exe -Label v0.9.1 -Out base.csv -Repeat 2
powershell -ExecutionPolicy Bypass -File tools\perf.ps1 -Label fase1 -Out fase1.csv -Repeat 2
```

## Phase 2a: sidebar, chat, home and navigation

Two runs (`-Label fase2a -Repeat 2`), commits af677ca and 38037ac, plus the phase-2b stage
as it stood then. Scenario (b) keeps text chat in the center, chat-only mode with members
on the right, as before.

| Scenario | Working set (MB) v0.9.1 → phase 2a | Private bytes (MB) v0.9.1 → phase 2a | CPU (% of 1 core) | GPU, all engines (%) | GPU 3D (%) |
|---|---|---|---|---|---|
| (a) Home idle | 81.6 → 82.1 (+0.6%) | 80.2 → 79.4 (-1.0%) | 0.11 → 0.04 | 0.00 → 0.00 | 0.00 → 0.00 |
| (b) Call with chat | 20.3 → 19.7 (-3.2%) | 108.2 → 105.3 (-2.7%) | 1.61 → 1.35 | 0.00 → 0.01 | 0.00 → 0.01 |
| (c) Watching a stream | 59.8 → 59.8 (-0.1%) | 170.5 → 168.8 (-1.0%) | 12.14 → 11.00 | 1.58 → 1.61 | 1.56 → 1.59 |
| (d) Camera on | 43.5 → 43.6 (+0.1%) | 227.4 → 230.3 (+1.3%) | 7.75 → 7.49 | 4.31 → 4.27 | 1.98 → 1.95 |

**Within budget:** at most +1.3% RAM (camera private bytes), zero idle home CPU, unchanged
GPU (0.01% in idle calls is measurement noise).

| Run | Scenario | WS (MB) | Private (MB) | CPU (%) | GPU all (%) | GPU 3D (%) |
|---|---|---|---|---|---|---|
| phase 2a #1 | home | 82 | 79.7 | 0 | 0 | 0 |
| phase 2a #2 | home | 82.2 | 79.1 | 0.08 | 0 | 0 |
| phase 2a #1 | call | 19.7 | 105.6 | 1.92 | 0.01 | 0.01 |
| phase 2a #2 | call | 19.6 | 105 | 0.77 | 0 | 0 |
| phase 2a #1 | watch | 59.8 | 167.4 | 10.03 | 1.65 | 1.63 |
| phase 2a #2 | watch | 59.7 | 170.1 | 11.96 | 1.57 | 1.55 |
| phase 2a #1 | camera | 43.3 | 229.8 | 6.36 | 4.24 | 1.94 |
| phase 2a #2 | camera | 43.8 | 230.7 | 8.61 | 4.29 | 1.95 |

## Phase 2b: voice-room stage

Two runs (`-Label fase2b -Repeat 2`), commits through ec2d02d: kit cards, multiple pins,
hidden non-video cards, fullscreen stage and voice dock. In (c), the pinned stream is
large with participant cards below. Layout is recalculated only when cards, pins, hidden
state or bounds change; native video windows move only when their rectangles change.

| Scenario | Working set (MB) v0.9.1 → phase 2b | Private bytes (MB) v0.9.1 → phase 2b | CPU (% of 1 core) | GPU, all engines (%) | GPU 3D (%) |
|---|---|---|---|---|---|
| (a) Home idle | 81.6 → 81.9 (+0.4%) | 80.2 → 79.7 (-0.6%) | 0.11 → 0.08 | 0.00 → 0.00 | 0.00 → 0.00 |
| (b) Call with chat | 20.3 → 20.0 (-1.6%) | 108.2 → 105.6 (-2.4%) | 1.61 → 0.86 | 0.00 → 0.01 | 0.00 → 0.01 |
| (c) Watching a stream | 59.8 → 59.7 (-0.2%) | 170.5 → 168.7 (-1.1%) | 12.14 → 11.05 | 1.58 → 1.59 | 1.56 → 1.57 |
| (d) Camera on | 43.5 → 43.9 (+0.8%) | 227.4 → 227.6 (+0.1%) | 7.75 → 8.42 | 4.31 → 4.28 | 1.98 → 1.95 |

**Within budget:** at most +0.8% RAM (camera working set), unchanged idle home CPU and GPU.
Watching CPU is 11.05% versus 12.14% for v0.9.1. Camera CPU is within run-to-run variation
(7.8–9.0%; phase 2a was 6.4–8.6%).

The call summary uses three separate runs (`-Only call -Repeat 3`). In the first full
measurement, call run #2 was an outlier: 22.6% CPU, 1.7% GPU, 1096 frames in 30 seconds.
Chat was visible, not the stage (`camera tiles visibility visible=false`). It did not
repeat in the other four call runs or phase 2a; retained for final review rather than
silently discarded.

| Run | Scenario | WS (MB) | Private (MB) | CPU (%) | GPU all (%) | GPU 3D (%) |
|---|---|---|---|---|---|---|
| phase 2b #1 | home | 82 | 80.4 | 0.07 | 0 | 0 |
| phase 2b #2 | home | 81.8 | 79 | 0.08 | 0 | 0 |
| phase 2b #1 | call | 20.1 | 104.6 | 1.46 | 0 | 0 |
| phase 2b #2 | call (outlier) | 36.1 | 109.5 | 22.63 | 1.68 | 1.68 |
| phase 2b #3 | call | 20 | 104.8 | 0.9 | 0.01 | 0.01 |
| phase 2b #4 | call | 19.8 | 104.8 | 1.23 | 0.01 | 0.01 |
| phase 2b #5 | call | 20.1 | 107.2 | 0.46 | 0.01 | 0.01 |
| phase 2b #1 | watch | 59.5 | 168.7 | 9.95 | 1.55 | 1.52 |
| phase 2b #2 | watch | 59.9 | 168.7 | 12.14 | 1.63 | 1.61 |
| phase 2b #1 | camera | 43.9 | 225.8 | 7.8 | 4.25 | 1.93 |
| phase 2b #2 | camera | 43.8 | 229.3 | 9.04 | 4.31 | 1.97 |

## Phase 3a: secondary windows

Settings, camera, sounds, voice changer and Go Live; commits through 4715227 plus the
phase-3b work available then. Two runs (`-Label fase3a -Repeat 2`). The repaint watcher
reported `busy=0` throughout. These scenarios keep secondary windows closed. When closed,
Go Live releases thumbnails/textures and the camera window releases its preview.

| Scenario | Working set (MB) v0.9.1 → phase 3a | Private bytes (MB) v0.9.1 → phase 3a | CPU (% of 1 core) | GPU, all engines (%) | GPU 3D (%) |
|---|---|---|---|---|---|
| (a) Home idle | 81.6 → 82.2 (+0.7%) | 80.2 → 81.1 (+1.1%) | 0.11 → 0.00 | 0.00 → 0.00 | 0.00 → 0.00 |
| (b) Call with chat | 20.3 → 19.9 (-2.0%) | 108.2 → 104.8 (-3.1%) | 1.61 → 1.53 | 0.00 → 0.00 | 0.00 → 0.00 |
| (c) Watching a stream | 59.8 → 60.4 (+1.0%) | 170.5 → 168.7 (-1.1%) | 12.14 → 13.62 | 1.58 → 1.62 | 1.56 → 1.60 |
| (d) Camera on | 43.5 → 43.8 (+0.6%) | 227.4 → 226.5 (-0.4%) | 7.75 → 7.95 | 4.31 → 4.43 | 1.98 → 2.07 |

**Within RAM budget:** at most +1.1% (home private bytes), zero idle home CPU and unchanged
GPU. Watching CPU (13.0 and 14.2%) was above the v0.9.1 range (11.7–12.6%). Phase 3a did
not change the active video path; the Go Live picker was closed. Earlier phases measured
10.0–12.1%, so this remained open for final measurement with the completed phase-3b tree.

| Run | Scenario | WS (MB) | Private (MB) | CPU (%) | GPU all (%) | GPU 3D (%) |
|---|---|---|---|---|---|---|
| phase 3a #1 | home | 81.8 | 79.3 | 0 | 0 | 0 |
| phase 3a #2 | home | 82.5 | 82.8 | 0 | 0 | 0 |
| phase 3a #1 | call | 19.7 | 105.6 | 1.61 | 0 | 0 |
| phase 3a #2 | call | 20.1 | 104 | 1.45 | 0 | 0 |
| phase 3a #1 | watch | 60.2 | 168.6 | 14.21 | 1.53 | 1.51 |
| phase 3a #2 | watch | 60.6 | 168.8 | 13.02 | 1.71 | 1.69 |
| phase 3a #1 | camera | 43.7 | 225.4 | 9.62 | 4.3 | 1.98 |
| phase 3a #2 | camera | 43.8 | 227.5 | 6.28 | 4.55 | 2.16 |

## Phase 3b: server window, menus, profiles, DMs and dialogs

Commits through 3b82ba8. Two full runs (`-Label fase3b -Repeat 2`), plus two five-run call
series (`-Only call -Repeat 5`). The new `busy` column counts logged `continuous repaint`
warnings during sampling.

| Scenario | Working set (MB) v0.9.1 → phase 3b | Private bytes (MB) v0.9.1 → phase 3b | CPU (% of 1 core) | GPU, all engines (%) | GPU 3D (%) |
|---|---|---|---|---|---|
| (a) Home idle | 81.6 → 82.0 (+0.5%) | 80.2 → 79.2 (-1.2%) | 0.11 → 0.04 | 0.00 → 0.00 | 0.00 → 0.00 |
| (b) Call with chat | 20.3 → 19.9 (-2.0%) | 108.2 → 107.0 (-1.1%) | 1.61 → 0.85 | 0.00 → 0.01 | 0.00 → 0.01 |
| (c) Watching a stream | 59.8 → 60.7 (+1.5%) | 170.5 → 169.5 (-0.6%) | 12.14 → 11.26 | 1.58 → 1.66 | 1.56 → 1.64 |
| (d) Camera on | 43.5 → 43.9 (+0.9%) | 227.4 → 229.4 (+0.9%) | 7.75 → 8.68 | 4.31 → 4.43 | 1.98 → 2.08 |

**Within budget:** at most +1.5% RAM (watching working set), unchanged idle home CPU
(0.04%) and GPU. The call summary averages all 12 call runs: CPU 0.46–1.68%, no outlier.
All runs after the watcher fix had `busy=0`; earlier watcher versions incorrectly counted
the quiet interval after an autotest tick. Watching CPU returned to the v0.9.1 range
(10.0 and 12.5%); camera CPU remained within prior variation (6.3–9.6%).

### Investigating phase 2b's continuous repaint

The 22.6% call outlier did not recur in these 12 calls or the previous 10 phase-2a/2b/3a
runs. Its logs were no longer available, so the exact original cause could not be proven.
Phase 3b changed:

- **Hidden microphone meter no longer wakes the window.** `MeterWatch` previously ran
  throughout calls, up to 15 repaints/second while speaking, even with chat-only mode
  where the meter was not visible. It now runs only when the previous frame drew the
  level (`App::meter_visible` / `CallUi::meter_frame`). Tests use no microphone, so this
  is not reflected in their measurements; real speech previously caused unnecessary
  wakes with each 3 dB change.
- **Repaint watcher (`repaint_watch.rs`).** Counts two-second intervals. More than 12 fps
  without user input in two consecutive intervals records repaint callers using egui's
  `Context::repaint_causes`, logging top causes at most every 10 seconds. `perf.ps1`
  reports these as `busy`.
- **Idle regression test (`nav` gate).** With chat-only mode during a quiet call, eight
  seconds may contain at most 16 frames (measured eight), and the meter thread must be off.

| Run | Scenario | WS (MB) | Private (MB) | CPU (%) | GPU all (%) | GPU 3D (%) | busy |
|---|---|---|---|---|---|---|---|
| phase 3b #1 | home | 81.9 | 78.9 | 0.07 | 0 | 0 | 0 |
| phase 3b #2 | home | 82.1 | 79.5 | 0 | 0 | 0 | 0 |
| phase 3b #1 | call | 20 | 105.6 | 0.92 | 0.01 | 0.01 | (1) |
| phase 3b #2 | call | 19.6 | 107.8 | 1.68 | 0 | 0 | (1) |
| phase 3b #1 | watch | 60.7 | 168 | 10.03 | 1.75 | 1.73 | 1 |
| phase 3b #2 | watch | 60.7 | 171 | 12.48 | 1.56 | 1.54 | 1 |
| phase 3b #1 | camera | 43.9 | 231.7 | 8.73 | 4.52 | 2.15 | 2 |
| phase 3b #2 | camera | 43.8 | 227 | 8.63 | 4.33 | 2 | 2 |
| phase 3b call #1 | call | 20.5 | 109.4 | 1.27 | 0.01 | 0.01 | (1) |
| phase 3b call #2 | call | 20.1 | 105.4 | 0.61 | 0 | 0 | (1) |
| phase 3b call #3 | call | 20.1 | 106.8 | 0.92 | 0.01 | 0.01 | (1) |
| phase 3b call #4 | call | 19.6 | 110.5 | 0.92 | 0.01 | 0.01 | (1) |
| phase 3b call #5 | call | 19.9 | 106.6 | 0.61 | 0.01 | 0.01 | (1) |
| phase 3b call2 #1 | call | 20.1 | 107.5 | 0.68 | 0.01 | 0.01 | 0 |
| phase 3b call2 #2 | call | 19.3 | 106.2 | 0.84 | 0.01 | 0.01 | 0 |
| phase 3b call2 #3 | call | 19.6 | 105 | 0.69 | 0.01 | 0.01 | 0 |
| phase 3b call2 #4 | call | 19.7 | 106.3 | 0.61 | 0.01 | 0.01 | 0 |
| phase 3b call2 #5 | call | 19.8 | 107 | 0.46 | 0.01 | 0.01 | 0 |

`(1)` marks false positives from the first watcher version, fixed in 3b82ba8. Watching
and camera warnings also came from that version, which counted bursts; those autotests
request a frame every 100 ms in addition to video events. Their logs were not retained,
so the final review remeasured them.

## Phase 4: final review

`tools\perf.ps1 -Label v1.0 -Repeat 3`: three runs of each scenario using the final
`v1-ui` branch (phases 1–4). v0.9.1 was not rerun because it lacks background test mode
and steals focus; its reference remains phase 1's measurement with the same script and
scenarios. Parenthesized CPU ranges show minimum and maximum runs.

Verdict criteria:

- RAM: no more than +10% working set or private bytes.
- CPU unchanged: v1.0's mean within or below v0.9.1's run-to-run range.
- GPU unchanged: v1.0 mean at most +0.2 percentage points. This matches same-build
  variation in phases 3a (1.53/1.71 watching) and 3b (1.75/1.56); only two baseline runs exist.

| Scenario | Working set (MB) v0.9.1 → v1.0 | Private bytes (MB) v0.9.1 → v1.0 | CPU (% of 1 core) v0.9.1 → v1.0 | GPU all (%) | GPU 3D (%) | Verdict |
|---|---|---|---|---|---|---|
| (a) Home idle | 81.6 → 82.2 (+0.7%) | 80.2 → 79.7 (-0.6%) | 0.11 → 0.05 (v0.9.1 0.07 to 0.15; v1.0 0.00 to 0.08) | 0.00 → 0.00 | 0.00 → 0.00 | **within budget** |
| (b) Call with chat | 20.3 → 19.7 (-2.8%) | 108.2 → 108.6 (+0.4%) | 1.61 → 1.15 (v0.9.1 1.53 to 1.69; v1.0 0.92 to 1.30) | 0.00 → 0.01 | 0.00 → 0.01 | **within budget** |
| (c) Watching a stream | 59.8 → 60.7 (+1.6%) | 170.5 → 170.0 (-0.3%) | 12.14 → 11.40 (v0.9.1 11.73 to 12.56; v1.0 7.82 to 15.90) | 1.58 → 1.74 | 1.56 → 1.72 | **within budget** |
| (d) Camera on | 43.5 → 43.7 (+0.4%) | 227.4 → 227.3 (-0.1%) | 7.75 → 8.07 (v0.9.1 7.29 to 8.21; v1.0 7.28 to 8.52) | 4.31 → 4.47 | 1.98 → 2.10 | **within budget** |

**All four scenarios are within budget.** At most +1.6% working set (watching, about
0.9 MB); private bytes essentially unchanged. Home CPU rounds to zero; call CPU is lower
(1.15% vs 1.61%); watching is 11.40% vs 12.14%; camera remains in the baseline range.
GPU is unchanged at home/in calls, and +0.16 points for watching/camera, within same-build
variation.

### Watching CPU and phase 3a's 13.6%

The PC was in use during final sampling, producing wider watching variation (7.8, 10.5,
15.9%). The app's own 30-second `process stats` samples were steadier: 12.8, 10.2 and
12.8% of one core, broadly consistent with the 11.7–12.6% baseline. The final mean
(11.40%) and phase 3b (11.26%) are below baseline: phase 3a was a high run, not evidence
of a sustained regression.

### Repaint watcher during video

Phase-3b watching/camera warnings named `autotest.rs` (the 100 ms tick) and
`profile.rs:674` (a scheduling-only `request_repaint_after`). Video uses native windows,
not egui frame requests: D3D11 presented 30.5 fps while UI drew about 12.5 fps, of which
10 were test ticks and 2.5 backend events. The watcher now subtracts the declared
`repaint_watch::set_expected_fps` tick and never names it as a cause; additional genuine
loops are still detected (`a_declared_tick_is_not_a_loop`). Final `busy` is zero everywhere.

The same tick explains the +0.16 GPU points: both builds draw about 13 UI fps, with
slightly more per-frame v1.0 work (rounded cards, dock, rings). Ordinary usage draws about
2.5 UI fps in these scenarios.

| Run | Scenario | WS (MB) | Private (MB) | CPU (%) | GPU all (%) | GPU 3D (%) | busy |
|---|---|---|---|---|---|---|---|
| v1.0 #1 | home | 82.3 | 80.6 | 0.08 | 0 | 0 | 0 |
| v1.0 #1 | call | 19.8 | 109.9 | 1.23 | 0.01 | 0.01 | 0 |
| v1.0 #1 | watch | 60.6 | 167.2 | 15.9 | 1.8 | 1.77 | 0 |
| v1.0 #1 | camera | 43.5 | 228.3 | 8.52 | 4.49 | 2.12 | 0 |
| v1.0 #2 | home | 82.2 | 80 | 0.08 | 0 | 0 | 0 |
| v1.0 #2 | call | 19.8 | 105.4 | 1.3 | 0 | 0 | 0 |
| v1.0 #2 | watch | 61 | 170.9 | 7.82 | 1.76 | 1.73 | 0 |
| v1.0 #2 | camera | 43.8 | 228.9 | 8.42 | 4.5 | 2.13 | 0 |
| v1.0 #3 | home | 82 | 78.5 | 0 | 0 | 0 | 0 |
| v1.0 #3 | call | 19.6 | 110.6 | 0.92 | 0.01 | 0.01 | 0 |
| v1.0 #3 | watch | 60.6 | 171.8 | 10.49 | 1.67 | 1.65 | 0 |
| v1.0 #3 | camera | 43.7 | 224.6 | 7.28 | 4.42 | 2.06 | 0 |

Repeat the new build only (retain phase 1's baseline, or run the old binary on a private
desktop like the gate's legacy viewer):

```powershell
powershell -ExecutionPolicy Bypass -File tools\perf.ps1 -Label v1.0 -Out v1.csv -Repeat 3
```
