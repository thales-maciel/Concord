//! The Go Live window (v1.0, phase 3a): what to stream ("Screen + audio" or "Audio only"), the grid
//! of thumbnail cards ([`crate::sources`]), the quality and the button that goes live. A child
//! module of `app` (it uses the app's private helpers).
//!
//! Drawn as a [`forms::Dialog`] (no egui title bar). The picker's thumbnails live only while the
//! window is open: closing it, by any path, releases them and their textures.

use std::sync::Arc;

use eframe::egui::{self, Vec2};

use super::{App, Pick, Share, ShareMode, ICON_LIVE};
use crate::backend::Cmd;
use crate::forms::{self, note, note_colored};
use crate::settings::Preset;
use crate::theme::{self, ph, space};
use crate::ui_kit::{self, Kind};

/// The window's size (clamped to the app's window by the dialog).
const SIZE: Vec2 = Vec2::new(820.0, 640.0);
/// What the window takes besides the grid (the header and the mode row above it, the options and
/// the footer under it), so the grid gets the rest. (v1.0 phase 4: was 268, the part under the grid
/// only, so a full grid or a short window pushed the footer out of the window.)
const BESIDE_GRID: f32 = 416.0;

/// macOS: the Screen Recording permission is off. Shows what to do and a button to System
/// Settings; returns whether it is off. Nothing (and `false`) elsewhere.
fn screen_permission_note(ui: &mut egui::Ui) -> bool {
    if vox_capture::screen_capture_permission() == vox_capture::Permission::Granted {
        return false;
    }
    note_colored(
        ui,
        format!(
            "{} macOS has not allowed concord to record the screen yet. Enable concord in System Settings > Privacy & Security > Screen & System Audio Recording, then quit and reopen concord.",
            ph::WARNING
        ),
        false,
        theme::AVISO,
    );
    if ui_kit::button(
        ui,
        Kind::Secondary,
        Some(ph::GEAR_SIX),
        "Open System Settings",
    )
    .clicked()
    {
        // The first time this shows macOS's own prompt; afterwards the app is listed, switched off.
        vox_capture::request_screen_capture_permission();
        if let Some(url) = vox_capture::privacy_settings_url(false) {
            if let Err(e) = std::process::Command::new("open").arg(url).spawn() {
                tracing::warn!(error = %e, "cannot open System Settings");
            }
        }
    }
    ui.add_space(space::S);
    true
}

impl App {
    pub(super) fn golive_window(&mut self, ctx: &egui::Context) {
        if !self.golive.open {
            // Closed (by any path): free the thumbnails and their textures.
            if !self.golive.picker.is_idle() {
                self.golive.picker.release();
                ctx.request_repaint(); // one more frame so egui frees the textures
            }
            return;
        }
        if self.golive.switching && !matches!(self.voice.share, Share::Live { .. }) {
            // The live ended while the picker was open.
            self.golive.open = false;
            self.golive.switching = false;
            self.golive.picker.release();
            ctx.request_repaint();
            return;
        }
        let mut start: Option<(Pick, Preset, bool)> = None;
        let mut refresh = false;
        let mut cancel = false;
        let switching = self.golive.switching;
        // The kind of the running live: choosing the same kind (and quality) only changes the
        // source; anything else restarts the live (viewers stay in the room and follow).
        let live_mode = if self.voice.share_audio_only {
            ShareMode::AudioOnly
        } else {
            ShareMode::ScreenAudio
        };
        let title = if switching {
            "Change what you share"
        } else {
            "Stream"
        };
        let current = self.voice.share_pick.clone();
        let screen = ctx.content_rect();
        let size = SIZE.min(screen.size() - Vec2::splat(24.0));
        let grid_h = (size.y - BESIDE_GRID).clamp(150.0, 430.0);
        let open = forms::Dialog::new("golive_window", title).icon(ph::SCREENCAST).size(size).show(ctx, |ui| {
            let mut mode = self.golive.mode;
            ui.horizontal(|ui| {
                forms::segmented(ui, &mut mode, &[(ShareMode::ScreenAudio, "Screen + audio"), (ShareMode::AudioOnly, "Audio only")]);
                ui.add_space(space::M);
                let hint = if mode == ShareMode::AudioOnly {
                    "An app's audio without video: great for listening to music together."
                } else {
                    "A window or an entire screen, with app audio."
                };
                ui.label(egui::RichText::new(hint).color(theme::NEVOA).size(12.5));
            });
            if mode != self.golive.mode {
                self.golive.mode = mode;
                self.golive.pick = None;
                if !self.golive.loading || !self.golive.windows.is_empty() {
                    // Same lists, other cards: icons of the apps, or pictures of the windows.
                    let repaint = self.repaint.clone();
                    self.golive.picker.sources_listed(&self.golive.windows, &self.golive.monitors, Arc::new(move || repaint.wake()), mode == ShareMode::AudioOnly);
                }
            }
            let audio_only = mode == ShareMode::AudioOnly;
            // Same kind and quality as the running live: a plain source switch.
            let switch_in_place = switching && mode == live_mode && (!audio_only || self.golive.audio_high == self.voice.share_audio_high);
            ui.add_space(space::M);
            // macOS: without Screen Recording there is nothing to list (screens, windows and an
            // app's sound all need it). Say so plainly, with the way to System Settings.
            let blocked = screen_permission_note(ui);
            if let Some(e) = self.golive.error.as_ref().filter(|_| !blocked) {
                note_colored(ui, format!("{} {e}", ph::WARNING), false, theme::AO_VIVO);
            }
            let out = crate::sources::show(
                ui,
                &mut self.golive.picker,
                crate::sources::GridInput {
                    windows: &self.golive.windows,
                    monitors: &self.golive.monitors,
                    loading: self.golive.loading,
                    picked: self.golive.pick.as_ref(),
                    current: if switching && mode == live_mode { current.as_ref() } else { None },
                    max_height: grid_h,
                    audio: audio_only,
                },
            );
            if let Some(p) = out.picked {
                self.golive.pick = Some(p);
            }
            if out.go {
                start = self.golive.pick.clone().map(|p| (p, self.golive.preset, self.golive.audio));
            }
            forms::divider(ui);
            if audio_only {
                forms::row(ui, "Audio quality", |ui| {
                    forms::segmented(ui, &mut self.golive.audio_high, &[(false, "Normal (128 kbps)"), (true, "High (256 kbps, stereo)")]);
                });
                let q = if self.golive.audio_high {
                    "More detailed stereo. Each listener receives a copy, requiring more upload bandwidth."
                } else {
                    "Stereo, good for music and light on bandwidth."
                };
                note(ui, q, true);
                note(
                    ui,
                    "Only audio goes to the room: no video, noise suppression or silence gate. People in the room click \"Listen\". You do not hear your own stream.",
                    true,
                );
            } else {
                if switch_in_place {
                    note(ui, "Quality and connection mode stay unchanged. Viewers do not need to do anything.", false);
                } else {
                    forms::row(ui, "Quality", |ui| {
                        let options: Vec<(Preset, &str)> = Preset::ALL.iter().map(|p| (*p, p.label())).collect();
                        forms::segmented(ui, &mut self.golive.preset, &options);
                    });
                    note(ui, format!("Up to {} Mbps of video. The server sends a copy to each viewer.", self.golive.preset.bitrate() / 1_000_000), true);
                    forms::row(ui, "When bandwidth is limited", |ui| {
                        forms::segmented(ui, &mut self.golive.smooth, &[(false, "Prioritize sharpness"), (true, "Prioritize smoothness")]);
                    });
                    let s = if self.golive.smooth {
                        "Maintains frame rate and allows a blurrier image. Good for fast-paced games."
                    } else {
                        "Reduces frame rate (from 60 to 30, then 20) to keep each frame sharp. Good for text, code and detail."
                    };
                    note(ui, s, true);
                }
                forms::row(ui, "Audio", |ui| {
                    ui.checkbox(&mut self.golive.audio, "Share app audio");
                });
                note(ui, "Window: only that app's audio. Entire screen: PC audio except concord.", true);
            }
            if switching && !switch_in_place {
                note_colored(
                    ui,
                    format!("{} Changing type (screen or audio only) or audio quality briefly restarts the stream. Viewers stay in the room.", ph::WARNING),
                    false,
                    theme::AVISO,
                );
            }
            forms::footer(ui, |ui| {
                let can = self.golive.pick.is_some();
                let go = if switch_in_place {
                    "Switch"
                } else if switching {
                    "Switch and restart"
                } else {
                    "Stream"
                };
                let icon = if switching { ph::ARROWS_LEFT_RIGHT } else { ICON_LIVE };
                let r = ui.add_enabled_ui(can, |ui| ui_kit::button(ui, Kind::Primary, Some(icon), go)).inner;
                if r.on_disabled_hover_text("Choose a window, screen or app above.").clicked() {
                    start = self.golive.pick.clone().map(|p| (p, self.golive.preset, self.golive.audio));
                }
                if ui_kit::button(ui, Kind::Ghost, None, "Cancel").clicked() {
                    cancel = true;
                }
                if ui_kit::button(ui, Kind::Secondary, Some(ph::ARROWS_CLOCKWISE), "Refresh list").clicked() {
                    refresh = true;
                }
            });
        });
        if !open || cancel {
            self.golive.open = false;
        }
        if refresh {
            self.golive.loading = true;
            self.backend.send(Cmd::ListSources);
        }
        if let Some((pick, preset, audio)) = start {
            let audio_only = self.golive.mode == ShareMode::AudioOnly;
            let same = self.golive.mode == live_mode
                && (!audio_only || self.golive.audio_high == self.voice.share_audio_high);
            if switching && same {
                self.switch_share(pick, audio);
            } else {
                // A new live, or the running one becoming another kind (the core replaces it).
                self.start_share(pick, preset, audio);
            }
        }
        if !self.golive.open {
            self.golive.picker.release();
            ctx.request_repaint();
        }
    }
}
