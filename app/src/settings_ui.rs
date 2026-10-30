//! Settings pages and change dispatch. Native video is clipped around the dialog
//! so calls continue playing outside its bounds.

use std::path::PathBuf;
use std::sync::OnceLock;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Layout, Margin, Pos2, Rect, RichText, Sense,
    Stroke, UiBuilder, Vec2,
};
use vox_client_core::audio::gate::{HOLD_MAX_MS, HOLD_MIN_MS, THRESHOLD_MAX_DB, THRESHOLD_MIN_DB};
use vox_client_core::{GateMode, NoiseLevel};

use super::notify::WinNotify;
use super::{App, MicTestUi, FOCUS_AUDIO_HOVER, FOCUS_AUDIO_LABEL, NOISE_FALLBACK_TEXT};
use crate::backend::Cmd;
use crate::forms::{self, note, note_colored, row};
use crate::settings::Preset;
use crate::theme::{self, ph, size, space, stroke};
use crate::ui_kit::{self, IconState, Kind, Size};
use crate::{meter, ptt};

/// The sections of the settings window, in the order of its left column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsPage {
    #[default]
    Account,
    Voice,
    Video,
    Notifications,
    Chat,
    Sounds,
    About,
}

impl SettingsPage {
    pub const ALL: [SettingsPage; 7] = [
        SettingsPage::Account,
        SettingsPage::Voice,
        SettingsPage::Video,
        SettingsPage::Notifications,
        SettingsPage::Chat,
        SettingsPage::Sounds,
        SettingsPage::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SettingsPage::Account => "Account and profile",
            SettingsPage::Voice => "Voice",
            SettingsPage::Video => "Video",
            SettingsPage::Notifications => "Notifications",
            SettingsPage::Chat => "Chat",
            SettingsPage::Sounds => "Sounds",
            SettingsPage::About => "About",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            SettingsPage::Account => ph::USER,
            SettingsPage::Voice => ph::MICROPHONE,
            SettingsPage::Video => ph::VIDEO_CAMERA,
            SettingsPage::Notifications => ph::BELL,
            SettingsPage::Chat => ph::CHAT_CENTERED_TEXT,
            SettingsPage::Sounds => ph::MUSIC_NOTES,
            SettingsPage::About => ph::INFO,
        }
    }

    /// A plain name for files and logs (the screenshots: `settings-<slug>.png`).
    pub fn slug(self) -> &'static str {
        match self {
            SettingsPage::Account => "conta",
            SettingsPage::Voice => "voz",
            SettingsPage::Video => "video",
            SettingsPage::Notifications => "notificacoes",
            SettingsPage::Chat => "chat",
            SettingsPage::Sounds => "sons",
            SettingsPage::About => "sobre",
        }
    }
}

/// Width of the left column.
const NAV_W: f32 = 212.0;
/// The left column's fill: between [`theme::NOITE`] and [`theme::PAREDE`] (the mockup's user panel).
const NAV_BG: Color32 = Color32::from_rgb(0x17, 0x22, 0x2D);
/// Height of the page header (its title and the ✕).
const HEAD_H: f32 = 58.0;
/// The readable width of a page.
const PAGE_MAX_W: f32 = 680.0;

/// This run's logs folder ("About" → "Open logs folder"), set once at startup.
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_log_dir(dir: PathBuf) {
    let _ = LOG_DIR.set(dir);
}

/// What the page changed this frame (applied after the window is drawn).
#[derive(Default)]
struct Changes {
    /// Something to write to settings.json.
    save: bool,
    /// Push-to-talk changed (re-arms the hook, which also saves).
    ptt: bool,
    gate: bool,
    toggle_test: bool,
    test_alert: bool,
}

impl App {
    /// Open the settings on `page`.
    pub fn open_settings_at(&mut self, page: SettingsPage) {
        self.settings_ui.page = page;
        self.open_settings();
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        // "Configure camera" opened from here: Settings waits behind it (shown again on close).
        if !self.settings_ui.open || self.settings_ui.cam.window {
            return;
        }
        let mut ch = Changes::default();
        let mut close = false;
        let devices_before = (
            self.settings.input_device.clone(),
            self.settings.output_device.clone(),
        );
        let screen = ctx.content_rect();
        let size = Vec2::new(
            (screen.width() - 56.0).clamp(560.0, 960.0),
            (screen.height() - 44.0).clamp(360.0, 660.0),
        );
        egui::Area::new(egui::Id::new("settings_window"))
            .order(egui::Order::Middle)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                forms::Dialog::frame().show(ui, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    let full = Rect::from_min_size(ui.max_rect().min, size);
                    self.settings_nav(ui, full);
                    let content =
                        Rect::from_min_max(Pos2::new(full.left() + NAV_W, full.top()), full.max);
                    close |= self.settings_page(ui, content, &mut ch);
                    ui.allocate_rect(full, Sense::hover());
                });
            });
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape && !self.settings_ui.capturing_key && !egui::Popup::is_any_open(ctx) {
            close = true;
        }
        self.apply_settings_changes(ch, devices_before, close);
    }

    /// The left column: the window's name and one row per section.
    fn settings_nav(&mut self, ui: &mut egui::Ui, full: Rect) {
        let nav = Rect::from_min_size(full.min, Vec2::new(NAV_W, full.height())).shrink(1.0);
        let r = theme::radius::POPUP;
        ui.painter().rect_filled(
            nav,
            CornerRadius {
                nw: r - 1,
                sw: r - 1,
                ne: 0,
                se: 0,
            },
            NAV_BG,
        );
        ui.painter().vline(
            nav.right(),
            nav.y_range(),
            Stroke::new(stroke::HAIRLINE, theme::RISCO),
        );
        let mut n = ui.new_child(
            UiBuilder::new()
                .max_rect(nav.shrink2(Vec2::new(10.0, 16.0)))
                .layout(Layout::top_down(Align::Min)),
        );
        n.spacing_mut().item_spacing.y = 2.0;
        n.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Settings")
                    .font(theme::display(size::HEADING))
                    .color(theme::GIZ),
            );
        });
        n.add_space(space::L);
        for p in SettingsPage::ALL {
            let label = format!("Settings: {}", p.title());
            let row = ui_kit::Row {
                icon: p.icon(),
                name: p.title(),
                label: &label,
                selected: self.settings_ui.page == p,
                unread: false,
                count: 0,
                mentions: 0,
                hover: None,
            };
            if ui_kit::sidebar_row(&mut n, row).0.clicked() {
                self.settings_ui.page = p;
            }
        }
        ui.painter().text(
            Pos2::new(nav.left() + 18.0, nav.bottom() - 16.0),
            Align2::LEFT_BOTTOM,
            format!("concord {}", env!("CARGO_PKG_VERSION")),
            theme::body(size::SMALL),
            theme::NEVOA,
        );
    }

    /// The chosen section: its title, the ✕ and the page in a scroll area. Returns true when the
    /// ✕ was clicked.
    fn settings_page(&mut self, ui: &mut egui::Ui, content: Rect, ch: &mut Changes) -> bool {
        let page = self.settings_ui.page;
        let head = Rect::from_min_size(content.min, Vec2::new(content.width(), HEAD_H));
        ui.painter().text(
            Pos2::new(head.left() + 28.0, head.center().y),
            Align2::LEFT_CENTER,
            page.title(),
            theme::display(size::HEADING),
            theme::GIZ,
        );
        ui.painter().hline(
            Rect::from_min_max(head.left_bottom(), head.right_bottom())
                .shrink2(Vec2::new(1.0, 0.0))
                .x_range(),
            head.bottom() - 0.5,
            Stroke::new(stroke::HAIRLINE, theme::RISCO),
        );
        let x_rect = Rect::from_center_size(
            Pos2::new(head.right() - 30.0, head.center().y),
            Vec2::splat(30.0),
        );
        let mut x = ui.new_child(
            UiBuilder::new()
                .max_rect(x_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        let close = ui_kit::icon_button(&mut x, ph::X, "Close (Esc)", IconState::Normal).clicked();

        let body = Rect::from_min_max(
            Pos2::new(content.left(), head.bottom()),
            content.max - Vec2::new(1.0, 1.0),
        );
        let mut b = ui.new_child(
            UiBuilder::new()
                .max_rect(body)
                .layout(Layout::top_down(Align::Min)),
        );
        b.set_clip_rect(body);
        egui::ScrollArea::vertical()
            .id_salt(("settings_page", page.slug()))
            .auto_shrink([false, false])
            .show(&mut b, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin {
                        left: 28,
                        right: 24,
                        top: 18,
                        bottom: 28,
                    })
                    .show(ui, |ui| {
                        ui.set_max_width(PAGE_MAX_W.min(ui.available_width()));
                        ui.spacing_mut().slider_width = forms::SLIDER_W;
                        ui.spacing_mut().item_spacing.y = 6.0;
                        match page {
                            SettingsPage::Account => self.page_account(ui),
                            SettingsPage::Voice => self.page_voice(ui, ch),
                            SettingsPage::Video => self.page_video(ui, ch),
                            SettingsPage::Notifications => self.page_notifications(ui, ch),
                            SettingsPage::Chat => self.page_chat(ui, ch),
                            SettingsPage::Sounds => ch.save |= self.soundboard_settings(ui),
                            SettingsPage::About => page_about(ui),
                        }
                        if std::mem::take(&mut self.settings_ui.scroll_end) {
                            // The screenshots' second half of a long page.
                            ui.scroll_to_cursor(Some(Align::BOTTOM));
                        }
                    });
            });
        close
    }

    /// After the window: the mic test, push-to-talk, saving, closing.
    fn apply_settings_changes(
        &mut self,
        ch: Changes,
        devices_before: (Option<String>, Option<String>),
        close: bool,
    ) {
        let mut save = ch.save;
        let mut ptt_changed = ch.ptt;
        if ch.gate {
            self.apply_gate_config();
            save = true;
        }
        if ch.toggle_test {
            if self.settings_ui.mic_test == MicTestUi::Off {
                self.start_mic_test();
            } else {
                self.stop_mic_test();
            }
        } else if self.settings_ui.mic_test != MicTestUi::Off
            && devices_before
                != (
                    self.settings.input_device.clone(),
                    self.settings.output_device.clone(),
                )
        {
            // A different device was picked while testing: test that one.
            self.backend.send(Cmd::StopMicTest);
            self.start_mic_test();
        }
        if close {
            self.flush_camera_settings(false);
            self.stop_mic_test();
            self.settings_ui.mic_test_error = None;
            self.settings_ui.open = false;
            if self.settings_ui.capturing_key {
                self.settings_ui.capturing_key = false;
                ptt::cancel_capture();
                ptt_changed = true;
            }
        }
        if ch.test_alert {
            self.test_alert();
        }
        if ptt_changed {
            self.ptt_settings_changed();
        } else if save {
            self.save_settings();
        }
    }

    // ------------------------------------------------------------ Account and profile

    fn page_account(&mut self, ui: &mut egui::Ui) {
        let mut any = false;
        if self.profile_picture_section(ui) {
            any = true;
            forms::divider(ui);
        }
        if self.profile_section(ui) {
            any = true;
            forms::divider(ui);
        }
        if self.roles.supported {
            any = true;
            self.personal_password_section(ui);
        }
        if !any {
            let text = if self.me.is_none() {
                "Join a server to change your picture, profile and personal password: they are stored on each server."
            } else {
                "This server is an older version: no profile pictures, profiles or personal passwords."
            };
            note(ui, text, false);
        }
    }

    // ------------------------------------------------------------ Voz

    fn page_voice(&mut self, ui: &mut egui::Ui, ch: &mut Changes) {
        forms::group(ui, "Devices");
        let default_in = self.settings_ui.default_input.clone();
        let default_out = self.settings_ui.default_output.clone();
        row(ui, "Microphone", |ui| {
            device_combo(
                ui,
                "input_device",
                &mut self.settings.input_device,
                &self.settings_ui.inputs,
                default_in.as_deref(),
                &mut ch.save,
            )
        });
        row(ui, "Speaker", |ui| {
            device_combo(
                ui,
                "output_device",
                &mut self.settings.output_device,
                &self.settings_ui.outputs,
                default_out.as_deref(),
                &mut ch.save,
            )
        });
        row(ui, "Output volume", |ui| {
            if ui
                .add(egui::Slider::new(&mut self.settings.volume_pct, 0..=200).suffix("%"))
                .changed()
            {
                ch.save = true;
                self.backend.send(Cmd::SetVolume(self.settings.volume_pct));
            }
        });
        note(
            ui,
            "Microphone and speaker changes apply the next time you join voice.",
            true,
        );
        forms::divider(ui);
        self.input_section(ui, ch);
        forms::divider(ui);
        self.noise_section(ui, ch);
        forms::divider(ui);
        self.ptt_section(ui, ch);
        forms::divider(ui);
        self.voice_fx_section(ui, &mut ch.toggle_test);
        forms::divider(ui);
        self.alert_section(ui, ch);
    }

    /// "Test microphone", the level meter and the input sensitivity.
    fn input_section(&mut self, ui: &mut egui::Ui, ch: &mut Changes) {
        forms::group(ui, "Test and sensitivity");
        let test = self.settings_ui.mic_test.clone();
        row(ui, "Test", |ui| {
            let (icon, label) = if test == MicTestUi::Off {
                (ph::MICROPHONE, "Test microphone")
            } else {
                (ph::STOP, "Stop test")
            };
            if ui_kit::button(ui, Kind::Secondary, Some(icon), label).on_hover_text("Hear yourself as others will hear you (use headphones to avoid echo). Nothing is sent.").clicked() {
                ch.toggle_test = true;
            }
            match &test {
                MicTestUi::Off => {}
                MicTestUi::Starting => {
                    ui.label(RichText::new("Opening microphone...").color(theme::NEVOA));
                }
                MicTestUi::On(None) => {
                    ui.label(
                        RichText::new("Speak: you hear yourself as others will hear you.")
                            .color(theme::NEVOA),
                    );
                }
                MicTestUi::On(Some(w)) => {
                    ui.label(RichText::new(format!("{} {w}", ph::WARNING)).color(theme::AVISO));
                }
            }
        });
        if let Some(e) = self.settings_ui.mic_test_error.clone() {
            note_colored(ui, e, true, theme::AO_VIVO);
        }
        // The meter shows the test while it runs, else the call's own microphone.
        let source = if test != MicTestUi::Off {
            Some(self.gate_test.clone())
        } else if self.voice.channel.is_some() && !self.voice.no_mic {
            Some(self.gate_voice.clone())
        } else {
            None
        };
        forms::indented(ui, |ui| {
            let w = ui.available_width().clamp(120.0, 380.0);
            meter::level_bar(ui, source.as_deref(), Vec2::new(w, 10.0));
        });
        match (&source, &test) {
            (None, _) => note(ui, "Click Test microphone to see the level and hear yourself. Use headphones to avoid echo.", true),
            (Some(_), MicTestUi::Off) => note(ui, "Your microphone level in the call.", true),
            _ => {}
        }
        ui.add_space(space::S);
        let cfg = &mut self.settings.input_gate;
        row(ui, "Input sensitivity", |ui| {
            ch.gate |= forms::segmented(
                ui,
                &mut cfg.mode,
                &[(GateMode::Auto, "Automatic"), (GateMode::Manual, "Manual")],
            );
        });
        match cfg.mode {
            GateMode::Manual => {
                row(ui, "Limit", |ui| {
                    ch.gate |= ui
                        .add(
                            egui::Slider::new(
                                &mut cfg.threshold_db,
                                THRESHOLD_MIN_DB..=THRESHOLD_MAX_DB,
                            )
                            .suffix(" dB")
                            .fixed_decimals(0),
                        )
                        .changed();
                });
                note(
                    ui,
                    "Voice passes only when the level (bar) exceeds the white marker.",
                    true,
                );
            }
            GateMode::Auto => {
                let text = match source.as_ref().map(|g| g.threshold_db()) {
                    Some(t) => format!("The threshold follows background noise (currently {t:.0} dB, the white marker)."),
                    None => "The threshold follows background noise.".to_string(),
                };
                note(ui, text, true);
            }
        }
        row(ui, "Keep open after speaking", |ui| {
            ch.gate |= ui
                .add(egui::Slider::new(&mut cfg.hold_ms, HOLD_MIN_MS..=HOLD_MAX_MS).suffix(" ms"))
                .changed();
        });
        if self.settings.ptt_enabled && self.settings.ptt_vk != 0 {
            note(
                ui,
                "With push-to-talk on, the key controls transmission and sensitivity is unused.",
                true,
            );
        }
    }

    fn noise_section(&mut self, ui: &mut egui::Ui, ch: &mut Changes) {
        forms::group(ui, "Noise suppression");
        let before = self.settings.noise_suppression.effective();
        let mut selected = before;
        let options: Vec<(NoiseLevel, &str)> =
            NoiseLevel::ALL.iter().map(|l| (*l, l.label())).collect();
        row(ui, "Level", |ui| {
            forms::segmented(ui, &mut selected, &options)
        });
        if !NoiseLevel::AiMax.available() {
            note(ui, "This lightweight build does not include Maximum AI. Saved Maximum AI preferences use Light AI until you return to a full build.", true);
        }
        let hint = match selected {
            NoiseLevel::Standard => "WebRTC suppression: lightweight, removes hiss and steady noise.",
            NoiseLevel::AiLight => "AI (RNNoise): also removes keyboard, fan and background noise. Uses very little CPU.",
            NoiseLevel::AiMax => {
                "AI (DeepFilterNet): strongest suppression; removes most keyboard, fan and street noise, and reduces background \
                 conversations. Uses more CPU (about 5 to 10% of one core) and adds about 30 ms of delay."
            }
        };
        note(ui, hint, true);
        if self.settings_ui.noise_fell_back {
            note_colored(
                ui,
                format!("{} {NOISE_FALLBACK_TEXT}", ph::WARNING),
                true,
                theme::AVISO,
            );
        }
        if selected != before {
            self.settings.noise_suppression = selected;
            self.settings_ui.noise_fell_back = false;
            // Applies on the next 10 ms of audio: in the call and in the mic test, no rejoin.
            self.noise.set_level(self.settings.noise_suppression);
            ch.gate = true;
        }
    }

    fn ptt_section(&mut self, ui: &mut egui::Ui, ch: &mut Changes) {
        forms::group(ui, "Push-to-talk");
        if !crate::platform::global_ptt() {
            // macOS: a key that works in any app needs the Accessibility permission (phase M4).
            row(ui, "Push-to-talk", |ui| {
                let mut off = false;
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(&mut off, "Usar push-to-talk (tecla global)"),
                )
                .on_disabled_hover_text(crate::platform::SOON_MAC);
            });
            note(
                ui,
                format!(
                    "Push-to-talk: {}. Use the microphone button or voice detection.",
                    crate::platform::SOON_MAC.to_lowercase()
                ),
                true,
            );
            if self.settings.ptt_enabled {
                // (A file from Windows: without the key the microphone would never open.)
                self.settings.ptt_enabled = false;
                ch.ptt = true;
            }
            return;
        }
        row(ui, "Push-to-talk", |ui| {
            ch.ptt |= ui
                .checkbox(
                    &mut self.settings.ptt_enabled,
                    "Usar push-to-talk (tecla global)",
                )
                .changed();
        });
        row(ui, "Key", |ui| {
            let key = if self.settings_ui.capturing_key {
                "press a key...".to_string()
            } else if self.settings.ptt_name.is_empty() {
                "(none)".to_string()
            } else {
                self.settings.ptt_name.clone()
            };
            ui_kit::pill(
                ui,
                &key,
                if self.settings_ui.capturing_key {
                    theme::MOVEL_HOVER
                } else {
                    theme::MOVEL
                },
                theme::GIZ,
            );
            if !self.settings_ui.capturing_key
                && ui_kit::button_sized(ui, Kind::Secondary, Size::Small, None, "Change").clicked()
            {
                self.settings_ui.capturing_key = true;
                self.sync_ptt_hook();
                ptt::begin_capture();
            }
            if self.settings_ui.capturing_key
                && ui_kit::button_sized(ui, Kind::Secondary, Size::Small, None, "Cancel").clicked()
            {
                self.settings_ui.capturing_key = false;
                ptt::cancel_capture();
                ch.ptt = true;
            }
        });
        if self.settings.ptt_enabled && self.settings.ptt_vk == 0 {
            note_colored(
                ui,
                "Choose a key to enable push-to-talk.",
                true,
                theme::AVISO,
            );
        } else {
            note(
                ui,
                "Works while concord is in the background (in a game, for example).",
                true,
            );
        }
    }

    /// "Alert sounds": the switch, the volume and the voice / presence ones (the chat ones are
    /// under Notifications).
    fn alert_section(&mut self, ui: &mut egui::Ui, ch: &mut Changes) {
        forms::group(ui, "Alert sounds");
        row(ui, "Alert sounds", |ui| {
            ch.save |= ui
                .checkbox(
                    &mut self.settings.alerts.enabled,
                    "Play alert sounds (only you hear them)",
                )
                .changed();
        });
        ui.add_enabled_ui(self.settings.alerts.enabled, |ui| {
            row(ui, "Alert volume", |ui| {
                ch.save |= ui
                    .add(
                        egui::Slider::new(&mut self.settings.alerts.volume_pct, 0..=100)
                            .suffix("%"),
                    )
                    .changed();
                if ui_kit::button_sized(ui, Kind::Secondary, Size::Small, Some(ph::PLAY), "Test")
                    .clicked()
                {
                    ch.test_alert = true;
                }
            });
            let a = &mut self.settings.alerts;
            row(ui, "Play when", |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().interact_size.y = forms::ROW_H;
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ch.save |= ui
                        .checkbox(&mut a.room, "Someone joined or left the voice room")
                        .changed();
                    ch.save |= ui
                        .checkbox(
                            &mut a.streams,
                            "Streams: someone started/stopped or joined yours",
                        )
                        .changed();
                    ch.save |= ui
                        .checkbox(&mut a.own_state, "When I mute or deafen")
                        .changed();
                    ch.save |= ui
                        .checkbox(
                            &mut a.presence,
                            "Someone went online or offline on the server",
                        )
                        .changed();
                });
            });
        });
        note(ui, "Message and call sounds are in Notifications.", true);
    }

    // ------------------------------------------------------------ Video

    fn page_video(&mut self, ui: &mut egui::Ui, ch: &mut Changes) {
        ch.save |= self.camera_section(ui);
        forms::divider(ui);
        ch.save |= self.encoder_section(ui);
        forms::divider(ui);
        forms::group(ui, "Transmitir (Go Live)");
        row(ui, "Default quality", |ui| {
            egui::ComboBox::from_id_salt("preset")
                .width(forms::COMBO_W)
                .selected_text(self.settings.preset.label())
                .show_ui(ui, |ui| {
                    for p in Preset::ALL {
                        ch.save |= ui
                            .selectable_value(&mut self.settings.preset, p, p.label())
                            .changed();
                    }
                });
        });
        note(ui, "Viewers receive up to 4, 8, 12 or 16 Mbps (720p, 1080p, 1080p 60, Maximum). You can change this when streaming.", true);
        row(ui, "Audio", |ui| {
            ch.save |= ui
                .checkbox(&mut self.settings.share_audio, "Share app audio by default")
                .changed();
        });
        #[cfg(windows)]
        {
            row(ui, "League of Legends", |ui| {
                ch.save |= ui
                    .checkbox(&mut self.settings.follow_league, "Auto-switch client / game (experimental)")
                    .on_hover_text("Only when sharing a League window. Video and app audio follow the match, then return to the client. Never shares the desktop. Applies to new streams: stop and restart your stream after changing this setting.")
                    .changed();
            });
            note(ui, "Waits up to 90 seconds for League to return if its window closes. Use Borderless mode if fullscreen capture is blank.", true);
        }
        row(ui, "Self preview", |ui| {
            ch.save |= ui
                .checkbox(&mut self.settings.show_self_preview, "Show my stream preview")
                .on_hover_text("Off: your stream shows no image to you (only \"Live\" and viewer count), and capture releases preview memory in RAM and GPU. Viewers are unaffected.")
                .changed();
        });
        forms::divider(ui);
        forms::group(ui, "Watching streams");
        row(ui, "Window", |ui| {
            if ui
                .checkbox(
                    &mut self.settings.stream_popout,
                    "Open streams in a separate window",
                )
                .on_hover_text("Off: the stream appears inside concord, in the call.")
                .changed()
            {
                vox_client_core::golive::Viewer::set_open_inline(!self.settings.stream_popout);
                ch.save = true;
            }
        });
        row(ui, "Sound", |ui| {
            ch.save |= ui
                .checkbox(&mut self.settings.stream_focus_audio, FOCUS_AUDIO_LABEL)
                .on_hover_text(FOCUS_AUDIO_HOVER)
                .changed();
        });
    }

    // ------------------------------------------------------------ Notifications

    fn page_notifications(&mut self, ui: &mut egui::Ui, ch: &mut Changes) {
        forms::group(ui, "Windows and tray");
        row(ui, "Windows notifications", |ui| {
            egui::ComboBox::from_id_salt("win_notify")
                .width(forms::COMBO_W)
                .selected_text(self.settings.notify.windows.label())
                .show_ui(ui, |ui| {
                    for m in WinNotify::ALL {
                        ch.save |= ui
                            .selectable_value(&mut self.settings.notify.windows, m, m.label())
                            .changed();
                    }
                });
        });
        note(ui, "When concord is unfocused, minimized or in the tray: its taskbar button flashes, shows the unread count, and Windows displays the author. Clicking the notice opens the conversation.", true);
        row(ui, "Tray", |ui| {
            ch.save |= ui
                .checkbox(&mut self.settings.notify.tray_on_close, "Minimize to tray on close")
                .on_hover_text("concord stays open (including the call) near the clock. Click its icon to open; right-click and Quit to exit.")
                .changed();
        });
        forms::divider(ui);
        forms::group(ui, "Chat sounds");
        if !self.settings.alerts.enabled {
            let go = forms::indented(ui, |ui| {
                ui.label(RichText::new("Alert sounds are disabled.").color(theme::AVISO));
                ui_kit::button_sized(
                    ui,
                    Kind::Ghost,
                    Size::Small,
                    Some(ph::MICROPHONE),
                    "Enable in Voice",
                )
                .clicked()
            });
            if go {
                self.settings_ui.page = SettingsPage::Voice;
            }
        }
        ui.add_enabled_ui(self.settings.alerts.enabled, |ui| {
            let a = &mut self.settings.alerts;
            row(ui, "Play when", |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().interact_size.y = forms::ROW_H;
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ch.save |= ui.checkbox(&mut a.dms, "Direct message received").changed();
                    ch.save |= ui
                        .checkbox(&mut a.chat, "New message in another channel")
                        .changed();
                    ch.save |= ui.checkbox(&mut a.calls, "Incoming call (ring)").changed();
                });
            });
        });
        note(ui, "Volume follows alert sounds (Voice).", true);
    }

    // ------------------------------------------------------------ Chat

    fn page_chat(&mut self, ui: &mut egui::Ui, ch: &mut Changes) {
        forms::group(ui, "Messages");
        row(ui, "Links", |ui| {
            ch.save |= ui
                .checkbox(&mut self.settings.show_link_previews, "Show link previews")
                .on_hover_text("Cards with titles and thumbnails below links. The server fetches the page (you do not visit the site).")
                .changed();
        });
        row(ui, "Images", |ui| {
            ch.save |= ui
                .checkbox(
                    &mut self.settings.auto_show_images,
                    "Load images automatically",
                )
                .on_hover_text("Off: chat images only load when you click them.")
                .changed();
        });
        forms::divider(ui);
        forms::group(ui, "Images you send");
        row(ui, "Quality", |ui| {
            let current = self.settings.image_quality;
            egui::ComboBox::from_id_salt("image_quality")
                .width(forms::COMBO_W)
                .selected_text(current.label())
                .show_ui(ui, |ui| {
                    for (q, label) in crate::chat_images::quality_choices() {
                        ch.save |= ui
                            .selectable_value(&mut self.settings.image_quality, q, label)
                            .changed();
                    }
                });
        });
        note(ui, "Images are compressed on your PC before sending (no metadata or GPS). Screenshots with text stay sharp in every mode.", true);
    }
}

// ------------------------------------------------------------ About

fn page_about(ui: &mut egui::Ui) {
    // v1.0.1: the concord logo and wordmark, and the signature.
    crate::brand::wordmark(ui, 34.0, 30.0);
    ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(theme::TEXTO));
    note(
        ui,
        "Voice, screen sharing and chat for your group, hosted on one of your PCs.",
        false,
    );
    ui.add_space(space::S);
    crate::brand::signature(ui, 13.0, theme::NEVOA);
    forms::divider(ui);
    forms::group(ui, "Logs");
    note(ui, "If something goes wrong, logs help identify the problem. They stay on this computer; nothing is sent.", false);
    ui.add_space(space::S);
    let dir = LOG_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| vox_log::default_log_dir("app"));
    ui.horizontal(|ui| {
        if ui_kit::button(
            ui,
            Kind::Secondary,
            Some(ph::FOLDER_OPEN),
            "Open logs folder",
        )
        .clicked()
        {
            open_folder(&dir);
        }
    });
    ui.add(
        egui::Label::new(
            RichText::new(dir.display().to_string())
                .monospace()
                .size(12.0)
                .color(theme::NEVOA),
        )
        .truncate(),
    );
    forms::divider(ui);
    forms::group(ui, "Credits");
    note(ui, "Atkinson Hyperlegible Next and Bricolage Grotesque fonts (OFL), Phosphor icons (MIT). See THIRD-PARTY-NOTICES.md for details.", false);
}

/// Open a folder in Explorer (never in an autotest: it would take the user's foreground).
fn open_folder(dir: &std::path::Path) {
    if vox_client_core::testmode::background() {
        tracing::info!("background test mode: folder not opened");
        return;
    }
    let _ = std::fs::create_dir_all(dir);
    #[cfg(target_os = "macos")]
    crate::platform::open(dir.as_os_str());
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let op: Vec<u16> = "open\0".encode_utf16().collect();
        let target: Vec<u16> = dir
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call.
        let r = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                op.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW returns a value > 32 on success.
        if r as isize <= 32 {
            tracing::warn!(code = r as isize, "could not open the logs folder");
        }
    }
}

/// A device picker: "System default (name)" or one of the devices.
fn device_combo(
    ui: &mut egui::Ui,
    salt: &str,
    value: &mut Option<String>,
    devices: &[String],
    default: Option<&str>,
    changed: &mut bool,
) {
    let current = match value {
        Some(v) => v.clone(),
        None => format!(
            "System default{}",
            default.map(|d| format!(" ({d})")).unwrap_or_default()
        ),
    };
    egui::ComboBox::from_id_salt(salt)
        .width(forms::COMBO_W)
        .selected_text(current)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(value.is_none(), "System default")
                .clicked()
            {
                *value = None;
                *changed = true;
            }
            for d in devices {
                if ui
                    .selectable_label(value.as_deref() == Some(d.as_str()), d)
                    .clicked()
                {
                    *value = Some(d.clone());
                    *changed = true;
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_has_a_title_an_icon_and_a_distinct_slug() {
        let mut slugs: Vec<&str> = SettingsPage::ALL.iter().map(|p| p.slug()).collect();
        for p in SettingsPage::ALL {
            assert!(!p.title().is_empty() && !p.icon().is_empty());
            // Sentence case, never ALL CAPS.
            assert_ne!(p.title(), p.title().to_uppercase(), "{p:?}");
        }
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), SettingsPage::ALL.len());
        assert_eq!(SettingsPage::default(), SettingsPage::Account);
    }
}
