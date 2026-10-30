//! Opt-in League window following. The policy is platform-independent so transitions can be
//! tested on Linux; only the adapter at the bottom enumerates/captures Windows windows.
//!
//! LeagueClientUx.exe is the lobby; League of Legends.exe owns the match (including loading).
//! The lobby may stay open, hide, or restart depending on "Close client during game". Like
//! OBS's separate lobby/game sources, prefer the match whenever it exists, not just when the
//! lobby closes. Never use title substrings, the foreground window, or a monitor fallback.
//! Reference: https://obsproject.com/kb/league-of-legends-capture-guide

use std::time::{Duration, Instant};

use vox_capture::WindowInfo;

const TRANSITION_GRACE: Duration = Duration::from_secs(90);
const STABLE_FOR: Duration = Duration::from_millis(500);
const RETRY_DELAY: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Client,
    Game,
}

fn kind(w: &WindowInfo) -> Option<Kind> {
    if w.exe_name.eq_ignore_ascii_case("LeagueClientUx.exe") {
        Some(Kind::Client)
    } else if w.exe_name.eq_ignore_ascii_case("League of Legends.exe") {
        Some(Kind::Game)
    } else {
        None
    }
}

fn same_window(a: &WindowInfo, b: &WindowInfo) -> bool {
    a.hwnd == b.hwnd && a.process_id == b.process_id && kind(a) == kind(b)
}

/// Prefer a previously chosen window; otherwise only choose an unambiguous candidate.
fn choose<'a>(
    windows: &'a [WindowInfo],
    wanted: Kind,
    previous: Option<&WindowInfo>,
) -> Option<&'a WindowInfo> {
    let mut matches = windows.iter().filter(|w| kind(w) == Some(wanted));
    if let Some(w) = previous.and_then(|p| matches.clone().find(|w| same_window(w, p))) {
        return Some(w);
    }
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Keep,
    Switch(WindowInfo),
    End,
}

pub(super) struct Follow {
    current: WindowInfo,
    client: Option<WindowInfo>,
    // Preserve intent independently of capture success: one failed audio start must not
    // permanently turn audio off for subsequent matches. An explicit override stays explicit.
    audio: bool,
    audio_pid: Option<u32>,
    pending: Option<(WindowInfo, Instant)>,
    retry_at: Instant,
    unhealthy_since: Option<Instant>,
}

impl Follow {
    fn new(
        enabled: bool,
        window: WindowInfo,
        audio: bool,
        audio_pid: Option<u32>,
        now: Instant,
    ) -> Option<Self> {
        if !enabled || window.hwnd == 0 || window.process_id == 0 {
            return None;
        }
        let kind = kind(&window)?;
        Some(Self {
            client: (kind == Kind::Client).then(|| window.clone()),
            current: window,
            audio,
            audio_pid,
            pending: None,
            retry_at: now,
            unhealthy_since: None,
        })
    }

    /// `current_exists` also checks hidden/minimized windows, with HWND *and PID*. Absence
    /// from the capturable-window list alone must not switch a running game back to the lobby.
    /// `None` for `windows` is an enumeration failure, not evidence that League has exited.
    fn decide(
        &mut self,
        windows: Option<&[WindowInfo]>,
        current_exists: bool,
        capture_ready: bool,
        now: Instant,
    ) -> Decision {
        if current_exists && capture_ready {
            self.unhealthy_since = None;
        } else {
            let since = *self.unhealthy_since.get_or_insert(now);
            if now.saturating_duration_since(since) >= TRANSITION_GRACE {
                return Decision::End;
            }
        }
        let Some(windows) = windows else {
            self.pending = None;
            return Decision::Keep;
        };
        let target = if kind(&self.current) == Some(Kind::Game) && current_exists {
            // Alt-tab, minimization, and virtual desktops are not the end of a match.
            Some(&self.current)
        } else if windows.iter().any(|w| kind(w) == Some(Kind::Game)) {
            // An ambiguous game list is deliberately not a reason to share a random client.
            choose(windows, Kind::Game, None)
        } else {
            choose(windows, Kind::Client, self.client.as_ref())
        };
        let Some(target) = target else {
            self.pending = None;
            return Decision::Keep;
        };
        if same_window(target, &self.current) && current_exists && capture_ready {
            self.pending = None;
            return Decision::Keep;
        }
        match &self.pending {
            Some((w, since)) if same_window(w, target) => {
                if now < self.retry_at || now.saturating_duration_since(*since) < STABLE_FOR {
                    return Decision::Keep;
                }
            }
            _ => {
                self.pending = Some((target.clone(), now));
                return Decision::Keep;
            }
        }
        self.retry_at = now + RETRY_DELAY;
        Decision::Switch(target.clone())
    }

    /// Called only after the existing in-place switch commits. Failure leaves the old target
    /// and grace deadline intact. A new capture must still survive the next health check.
    fn switched(&mut self, window: WindowInfo) {
        if kind(&window) == Some(Kind::Client) {
            self.client = Some(window.clone());
        }
        self.current = window;
        self.pending = None;
    }
}

#[cfg(windows)]
impl Follow {
    pub(super) fn for_source(
        source: &super::ShareSource,
        opts: super::ShareOptions,
    ) -> Option<Self> {
        if !opts.follow_league {
            return None; // no enumeration or extra capture work with the feature off
        }
        let super::ShareSource::Window(hwnd) = source else {
            return None;
        };
        let window = vox_capture::list_windows()
            .ok()?
            .into_iter()
            .find(|w| w.hwnd == *hwnd)?;
        Self::new(true, window, opts.audio, opts.audio_pid, Instant::now())
    }

    fn current_exists(&self) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut pid = 0;
        // SAFETY: read-only Win32 query; a destroyed HWND returns 0. Comparing the PID avoids
        // treating a recycled HWND belonging to another application as the same window.
        let thread = unsafe {
            GetWindowThreadProcessId(self.current.hwnd as *mut core::ffi::c_void, &mut pid)
        };
        thread != 0 && pid == self.current.process_id
    }
}

/// None = ordinary share/watchdog behaviour; Some = following owns the lifetime decision.
/// Runs under the same share mutex as manual switching/stopping and encoder fallback.
#[cfg(windows)]
pub(super) async fn tick(slot: &mut super::ShareSlot, inner: &super::Inner) -> Option<bool> {
    let share = slot.active.as_mut()?;
    let follow = slot.league.as_mut()?;
    let windows = vox_capture::list_windows().ok();
    let was_waiting = follow.unhealthy_since.is_some();
    let decision = follow.decide(
        windows.as_deref(),
        follow.current_exists(),
        share.video_ready(),
        Instant::now(),
    );
    if !was_waiting && follow.unhealthy_since.is_some() {
        tracing::info!("League handoff: waiting up to 90 seconds for a capturable window");
    }
    match decision {
        Decision::End => {
            tracing::warn!("League handoff timed out; ending the stream");
            Some(false)
        }
        Decision::Keep => Some(true),
        Decision::Switch(window) => {
            // No stop/start or republish: reuse the proven frame gate, keyframe handoff,
            // preview, encoder and audio switch. The target's PID is resolved afresh for audio.
            let req = super::SwitchRequest {
                source: super::ShareSource::Window(window.hwnd),
                audio: follow.audio,
                audio_pid: follow.audio_pid,
            };
            // Revalidate just before opening capture, rather than trust the previous poll.
            // switch_source also handles a target disappearing during capture startup.
            let valid = vox_capture::list_windows()
                .is_ok_and(|ws| ws.iter().any(|w| same_window(w, &window)));
            if !valid {
                return Some(true);
            }
            match share.switch_source(&inner.room, req).await {
                Ok(report) => {
                    let from_hwnd = follow.current.hwnd;
                    let hwnd = window.hwnd;
                    follow.switched(window);
                    let _ = inner
                        .events
                        .send(super::VoiceEvent::LocalShareWindowChanged {
                            from_hwnd,
                            hwnd,
                            audio: report.audio_after,
                            audio_error: report.audio_error,
                        });
                }
                Err(error) => {
                    tracing::warn!(%error, "League handoff failed; keeping the previous source and retrying");
                }
            }
            Some(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(hwnd: isize, pid: u32, exe: &str) -> WindowInfo {
        WindowInfo {
            hwnd,
            process_id: pid,
            exe_name: exe.into(),
            title: "League of Legends".into(),
        }
    }

    fn client() -> WindowInfo {
        window(1, 10, "LeagueClientUx.exe")
    }
    fn game() -> WindowInfo {
        window(2, 20, "League of Legends.exe")
    }
    fn follow(now: Instant) -> Follow {
        Follow::new(true, client(), true, None, now).unwrap()
    }

    #[test]
    fn opt_in_and_exact_executable_not_title() {
        let now = Instant::now();
        assert!(Follow::new(false, client(), true, None, now).is_none());
        for exe in [
            "chrome.exe",
            "RiotClientServices.exe",
            "LeagueClient.exe",
            "LeagueClientUxRender.exe",
            "not-League of Legends.exe",
            "",
        ] {
            assert!(
                Follow::new(true, window(1, 10, exe), true, None, now).is_none(),
                "{exe}"
            );
        }
        assert!(Follow::new(true, window(1, 10, "LEAGUECLIENTUX.EXE"), false, None, now).is_some());
        assert!(Follow::new(true, window(0, 10, "LeagueClientUx.exe"), true, None, now).is_none());
        assert!(Follow::new(true, window(1, 0, "LeagueClientUx.exe"), true, None, now).is_none());
    }

    #[test]
    fn game_takes_priority_even_if_client_stays_open() {
        let now = Instant::now();
        let mut f = follow(now);
        let ws = [client(), game()];
        assert_eq!(f.decide(Some(&ws), true, true, now), Decision::Keep);
        assert_eq!(
            f.decide(Some(&ws), true, true, now + STABLE_FOR),
            Decision::Switch(game())
        );
        f.switched(game());
        assert_eq!(
            f.decide(Some(&ws), true, true, now + RETRY_DELAY),
            Decision::Keep
        );
    }

    #[test]
    fn client_closes_then_relaunches_and_multiple_games_follow() {
        let now = Instant::now();
        let mut f = follow(now);
        assert_eq!(f.decide(Some(&[]), false, false, now), Decision::Keep);
        assert_eq!(
            f.decide(Some(&[game()]), false, false, now + Duration::from_secs(5)),
            Decision::Keep
        );
        assert_eq!(
            f.decide(Some(&[game()]), false, false, now + Duration::from_secs(6)),
            Decision::Switch(game())
        );
        f.switched(game());
        assert_eq!(
            f.decide(Some(&[game()]), true, true, now + Duration::from_secs(7)),
            Decision::Keep
        );
        let later = now + Duration::from_secs(600);
        assert_eq!(f.decide(Some(&[]), false, false, later), Decision::Keep);
        let new_client = window(3, 30, "LeagueClientUx.exe");
        assert_eq!(
            f.decide(
                Some(std::slice::from_ref(&new_client)),
                false,
                false,
                later + Duration::from_secs(20)
            ),
            Decision::Keep
        );
        assert_eq!(
            f.decide(
                Some(std::slice::from_ref(&new_client)),
                false,
                false,
                later + Duration::from_secs(21)
            ),
            Decision::Switch(new_client.clone())
        );
        f.switched(new_client.clone());
        assert_eq!(
            f.decide(
                Some(std::slice::from_ref(&new_client)),
                true,
                true,
                later + Duration::from_secs(22)
            ),
            Decision::Keep
        );
        let next_game = window(4, 40, "League of Legends.exe");
        let ws = [new_client, next_game.clone()];
        assert_eq!(
            f.decide(Some(&ws), true, true, later + Duration::from_secs(60)),
            Decision::Keep
        );
        assert_eq!(
            f.decide(Some(&ws), true, true, later + Duration::from_secs(61)),
            Decision::Switch(next_game)
        );
    }

    #[test]
    fn can_start_in_game_and_return_to_client_without_focus_switching() {
        let now = Instant::now();
        let mut f = Follow::new(true, game(), false, None, now).unwrap();
        // Game missing from enumeration, but HWND still exists (hidden / alt-tab).
        assert_eq!(f.decide(Some(&[client()]), true, true, now), Decision::Keep);
        // A destroyed game HWND is different, even if the capture thread has not exited yet.
        assert_eq!(
            f.decide(Some(&[client()]), false, true, now + Duration::from_secs(1)),
            Decision::Keep
        );
        assert_eq!(
            f.decide(Some(&[client()]), false, true, now + Duration::from_secs(2)),
            Decision::Switch(client())
        );
        assert!(!f.audio);
    }

    #[test]
    fn failed_switch_retries_without_committing_or_resetting_grace() {
        let now = Instant::now();
        let mut f = follow(now);
        let ws = [game()];
        assert_eq!(f.decide(Some(&ws), false, false, now), Decision::Keep);
        let attempt = now + STABLE_FOR;
        assert_eq!(
            f.decide(Some(&ws), false, false, attempt),
            Decision::Switch(game())
        );
        // Do not call switched: simulate startup failure, including failure to capture audio.
        assert!(same_window(&f.current, &client()));
        assert!(f.audio);
        assert_eq!(f.audio_pid, None);
        assert_eq!(
            f.decide(Some(&ws), false, false, attempt + STABLE_FOR),
            Decision::Keep
        );
        assert_eq!(
            f.decide(Some(&ws), false, false, attempt + RETRY_DELAY),
            Decision::Switch(game())
        );
        assert_eq!(
            f.decide(Some(&ws), false, false, now + TRANSITION_GRACE),
            Decision::End
        );
    }

    #[test]
    fn failed_capture_can_restart_the_same_window_but_not_forever() {
        let now = Instant::now();
        let mut f = follow(now);
        let ws = [client()];
        assert_eq!(f.decide(Some(&ws), true, false, now), Decision::Keep);
        assert_eq!(
            f.decide(Some(&ws), true, false, now + STABLE_FOR),
            Decision::Switch(client())
        );
        f.switched(client());
        // A capture that immediately dies again does not extend the grace indefinitely.
        assert_eq!(
            f.decide(Some(&ws), true, false, now + TRANSITION_GRACE),
            Decision::End
        );
    }

    #[test]
    fn exit_or_enumeration_failure_times_out_but_healthy_stream_is_kept() {
        let now = Instant::now();
        let mut f = follow(now);
        assert_eq!(f.decide(None, true, true, now), Decision::Keep);
        assert_eq!(
            f.decide(None, true, true, now + TRANSITION_GRACE),
            Decision::Keep
        );
        let later = now + TRANSITION_GRACE;
        assert_eq!(f.decide(Some(&[]), false, false, later), Decision::Keep);
        assert_eq!(
            f.decide(None, false, false, later + TRANSITION_GRACE),
            Decision::End
        );
    }

    #[test]
    fn transient_or_ambiguous_windows_do_not_trigger_handoff() {
        let now = Instant::now();
        let mut f = follow(now);
        assert_eq!(f.decide(Some(&[game()]), true, true, now), Decision::Keep);
        assert_eq!(
            f.decide(Some(&[client()]), true, true, now + STABLE_FOR),
            Decision::Keep
        );
        let games = [game(), window(3, 30, "League of Legends.exe")];
        assert_eq!(
            f.decide(Some(&games), true, true, now + Duration::from_secs(1)),
            Decision::Keep
        );
        assert_eq!(
            f.decide(Some(&games), true, true, now + Duration::from_secs(2)),
            Decision::Keep
        );
        let mut f = Follow::new(true, game(), true, None, now).unwrap();
        let clients = [client(), window(3, 30, "LeagueClientUx.exe")];
        assert_eq!(f.decide(Some(&clients), false, false, now), Decision::Keep);
        assert_eq!(
            f.decide(Some(&clients), false, false, now + STABLE_FOR),
            Decision::Keep
        );
    }

    #[test]
    fn hwnd_reuse_is_not_the_old_window_and_titles_can_change() {
        let now = Instant::now();
        let mut f = follow(now);
        let mut renamed = client();
        renamed.title = "another localized title".into();
        assert_eq!(f.decide(Some(&[renamed]), true, true, now), Decision::Keep);
        let reused = window(1, 99, "chrome.exe");
        assert_eq!(f.decide(Some(&[reused]), false, false, now), Decision::Keep);
        let replacement = window(1, 30, "LeagueClientUx.exe");
        assert_eq!(
            f.decide(
                Some(std::slice::from_ref(&replacement)),
                false,
                false,
                now + Duration::from_secs(1)
            ),
            Decision::Keep
        );
        assert_eq!(
            f.decide(
                Some(std::slice::from_ref(&replacement)),
                false,
                false,
                now + Duration::from_secs(2)
            ),
            Decision::Switch(replacement)
        );
    }

    #[test]
    fn audio_override_and_preferred_client_survive_handoffs() {
        let now = Instant::now();
        let mut f = Follow::new(true, client(), true, Some(123), now).unwrap();
        f.switched(game());
        let clients = [window(3, 30, "LeagueClientUx.exe"), client()];
        assert_eq!(f.decide(Some(&clients), false, false, now), Decision::Keep);
        assert_eq!(
            f.decide(Some(&clients), false, false, now + STABLE_FOR),
            Decision::Switch(client())
        );
        assert!(f.audio);
        assert_eq!(f.audio_pid, Some(123));
    }
}
