//! In-app hosting and invite management. Hosted data lives under
//! `%LOCALAPPDATA%\concord\host` (`VOX_HOST_DIR` in tests).
//! [`voxhub_server::host::HostRunner`] runs the hub in-process after verifying LiveKit.
//! Loopback login claims ownership; closing to tray keeps the server running.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{self, Align, Align2, Layout, RichText, Sense, Vec2};
use vox_client_core::hub::HubClient;
use vox_proto::{
    invite_token_from_hex, Invite, InviteAddr, InviteCode, InviteCodeError, InviteCreateRequest,
    InviteCreated, InviteId, InviteState,
};
use voxhub_server::host::nat::{CgnatSign, Reach};
use voxhub_server::host::{
    self as hosting, livekit_fetch, HostRunner, HostStatus, Hosted, Phase, RunOptions, UpnpState,
};

use crate::app::{App, Screen, GREEN, RED};
use crate::backend::Cmd;
use crate::theme::{self, ph, size, space};
use crate::ui_kit::{self, IconState, Kind, Size};

/// The home screen's two paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HomeTab {
    #[default]
    Connect,
    Host,
}

/// Joining with an invite code: the token and the hub URLs to try, in order. v0.9.1: a saved
/// server's addresses too (no token: an invite is used up by the first login).
#[derive(Debug, Clone)]
pub struct InviteJoin {
    pub token: Option<String>,
    pub urls: Vec<String>,
}

// ---------------------------------------------------------------- backend side

/// The invite calls, run by the backend (`Cmd::Invite`); the answer comes back as `Ev::Invite`.
#[derive(Debug, Clone)]
pub enum InviteCmd {
    List,
    Create(InviteCreateRequest),
    Revoke(InviteId),
}

#[derive(Debug)]
pub enum InviteReply {
    List(Vec<Invite>),
    Created(InviteCreated),
    Revoked,
}

#[derive(Debug)]
pub struct InviteEv {
    pub cmd: InviteCmd,
    pub result: Result<InviteReply, String>,
}

pub async fn run_invite(hub: &HubClient, cmd: &InviteCmd) -> Result<InviteReply, String> {
    let r = match cmd {
        InviteCmd::List => hub.invites().await.map(InviteReply::List),
        InviteCmd::Create(req) => hub.create_invite(req).await.map(InviteReply::Created),
        InviteCmd::Revoke(id) => hub.revoke_invite(*id).await.map(|_| InviteReply::Revoked),
    };
    r.map_err(|e| {
        tracing::warn!(error = %format!("{e:#}"), ?cmd, "invite call failed");
        let l = format!("{e:#}").to_lowercase();
        if l.contains("too many active invites") {
            format!(
                "There are already {} active invites. Revoke some before creating another.",
                vox_proto::INVITE_MAX_ACTIVE
            )
        } else if l.contains("http 404") || l.contains("http 405") {
            "This server is an older version without invites. Ask the host to update concord."
                .into()
        } else {
            crate::roles::admin_error(&e)
        }
    })
}

/// What a code that does not decode means, for people.
pub fn code_error_text(e: InviteCodeError) -> &'static str {
    match e {
        InviteCodeError::NotAnInvite | InviteCodeError::Typo => {
            "The invite code is incomplete or mistyped. Copy and paste the entire code again (starts with vox1-)."
        }
        InviteCodeError::NewerVersion => "This invite was created by a newer version of concord. Update the app and try again.",
        InviteCodeError::Damaged => "This invite code is damaged. Ask the sender for a new one.",
    }
}

/// This computer's interface addresses (to see whether an invite's LAN address is on our network).
pub(crate) fn local_addrs() -> Vec<(IpAddr, u8)> {
    let mut v = voxhub_server::linkpreview::ssrf::interface_addresses();
    v.push((IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 8));
    v
}

// ---------------------------------------------------------------- UI state

/// Validity choices ("Expires after").
pub const EXPIRY: [(Option<u64>, &str); 4] = [
    (Some(3600), "1 hour"),
    (Some(24 * 3600), "24 hours"),
    (Some(7 * 24 * 3600), "7 days"),
    (None, "Never expires"),
];
/// Use limits ("Can be used by").
pub const USES: [(Option<u32>, &str); 4] = [
    (Some(1), "1 person"),
    (Some(5), "5 people"),
    (Some(25), "25 people"),
    (None, "Unlimited"),
];

pub struct InviteUi {
    pub list: Vec<Invite>,
    pub loaded: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub expiry: usize,
    pub uses: usize,
    /// Codes made in this session (the hub keeps only a hash of each token).
    pub codes: HashMap<InviteId, String>,
    /// The code just made, shown big.
    pub last: Option<(InviteId, String)>,
    pub confirm_revoke: Option<InviteId>,
    copied_at: Option<Instant>,
}

impl Default for InviteUi {
    fn default() -> Self {
        InviteUi {
            list: Vec::new(),
            loaded: false,
            busy: false,
            error: None,
            expiry: 1,
            uses: 1,
            codes: HashMap::new(),
            last: None,
            confirm_revoke: None,
            copied_at: None,
        }
    }
}

enum Prepared {
    Progress(u64, Option<u64>),
    Done(Result<Option<PathBuf>, String>),
}

pub struct HostUi {
    pub root: PathBuf,
    pub tab: HomeTab,
    pub servers: Vec<Hosted>,
    pub(crate) listed: bool,
    pub new_name: String,
    pub error: Option<String>,
    pub runner: Option<HostRunner>,
    pub hosted: Option<Hosted>,
    /// Voice-server download progress (bytes, total) while it runs.
    pub fetching: Option<(u64, Option<u64>)>,
    /// Getting the voice server ready for this server (checking / downloading).
    preparing: Option<(Hosted, Receiver<Prepared>)>,
    /// Log into our own server once it runs (once per start).
    pub(crate) auto_login: bool,
    pub panel_open: bool,
    pub confirm_stop: bool,
    pub confirm_quit: bool,
    quit_confirmed: bool,
    pub show_password: bool,
    pub inv: InviteUi,
    /// The server (and me) the invite list belongs to.
    inv_for: String,
}

impl HostUi {
    pub fn new() -> Self {
        HostUi {
            root: hosting::root_dir(),
            tab: HomeTab::Connect,
            servers: Vec::new(),
            listed: false,
            new_name: String::new(),
            error: None,
            runner: None,
            hosted: None,
            fetching: None,
            preparing: None,
            auto_login: false,
            panel_open: false,
            confirm_stop: false,
            confirm_quit: false,
            quit_confirmed: false,
            show_password: false,
            inv: InviteUi::default(),
            inv_for: String::new(),
        }
    }

    /// A server of mine is starting or running.
    pub fn hosting(&self) -> bool {
        self.preparing.is_some() || self.runner.as_ref().is_some_and(|r| r.is_live())
    }

    pub fn status(&self) -> Option<HostStatus> {
        self.runner.as_ref().map(|r| r.status())
    }
}

// ---------------------------------------------------------------- texts

fn phase_text(p: &Phase) -> (String, egui::Color32) {
    match p {
        Phase::Starting => ("Starting...".into(), theme::AVISO),
        Phase::Network => ("Opening router ports...".into(), theme::AVISO),
        Phase::Running => ("On".into(), GREEN),
        Phase::Stopping => ("Stopping...".into(), theme::AVISO),
        Phase::Stopped => ("Off".into(), RED),
        Phase::Failed(e) => (format!("Could not start: {e}"), RED),
    }
}

/// The reachability part of the panel, in plain English: (text, colour).
pub fn reach_text(st: &HostStatus) -> (String, egui::Color32) {
    let ports = st.ports.map(|p| p.describe()).unwrap_or_default();
    let lan = st
        .lan
        .map(|(ip, _)| ip.to_string())
        .unwrap_or_else(|| "this computer".into());
    let manual = format!(
        "For friends outside your home to join, forward these router ports to {lan}: {ports}. \
         Look for \"port forwarding\" on your router's settings page."
    );
    match (st.reach, &st.upnp) {
        (Some(Reach::Cgnat(sign)), _) => {
            let why = match sign {
                CgnatSign::SharedRange => "the router received a CGNAT address (100.64.x.x) from your ISP",
                CgnatSign::PrivateWan => "another router (your ISP's modem) sits in front of yours",
                CgnatSign::Mismatch => "the router's address does not match the address seen on the internet",
            };
            (
                format!(
                    "Your internet connection cannot accept incoming connections ({why}). This is common with CGNAT and cannot be fixed \
                     on this computer: people on your home network can join, but friends on the internet cannot connect directly. \
                     Ask your ISP for a \"public IP\" (sometimes free), or have everyone install Tailscale or ZeroTier to create \
                     a private network. A concord relay is a possible future feature."
                ),
                RED,
            )
        }
        (Some(Reach::Direct), _) => ("This computer has a public address: friends can connect over the internet.".into(), GREEN),
        (_, UpnpState::Mapped(m)) if m.complete() => (
            format!("The router opened the ports automatically ({ports}). Friends can connect over the internet using the invite."),
            GREEN,
        ),
        (_, UpnpState::Mapped(m)) => {
            let bad: Vec<String> = m.failed.iter().map(|(p, n, why)| format!("{n} {} ({why})", p.name())).collect();
            (format!("The router opened only some ports; missing: {}. {manual}", bad.join(", ")), theme::AVISO)
        }
        (_, UpnpState::Unavailable(_)) => (format!("The router could not open ports automatically (UPnP disabled or unsupported). {manual}"), theme::AVISO),
        (_, UpnpState::Searching) => ("Looking for the router...".into(), theme::AVISO),
        (_, UpnpState::Off) => ("Router ports: unchanged (test mode).".into(), theme::AVISO),
    }
}

const FIREWALL_TEXT: &str = "If Windows asks whether concord (and the voice server, \"livekit-server\") can use the network, click \"Allow access\". \
     Otherwise, only this computer can join.";

const HONEST_TEXT: &str = "The code hides the server address from casual observers, but it is not an absolute secret: concord is open source, \
     and anyone who joins learns the IP (that is how connections work). The actual protection is the invite: it expires, \
     has a use limit and can be revoked.";

fn when_text(ms: i64) -> String {
    crate::app::format_time(ms, crate::app::now_ms(), crate::app::local_offset_min())
}

/// `http://203.0.113.5:7890/x` -> (`203.0.113.5`, 7890); `[2001:db8::1]` keeps no brackets.
pub(crate) fn url_host_port(url: &str) -> Option<(String, Option<u16>)> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split('/').next()?;
    if let Some(r) = authority.strip_prefix('[') {
        let (h, after) = r.split_once(']')?;
        return Some((
            h.to_string(),
            after.strip_prefix(':').and_then(|p| p.parse().ok()),
        ));
    }
    match authority.rsplit_once(':') {
        Some((h, p)) => Some((h.to_string(), p.parse().ok())),
        None => Some((authority.to_string(), None)),
    }
}

/// One address for an invite from a host name / IP text (loopback and nonsense are skipped).
fn push_addr(addrs: &mut Vec<InviteAddr>, host: &str, lan_prefix: u8) {
    let host = host.trim();
    let a = match host.parse::<IpAddr>() {
        Ok(ip) if ip.is_loopback() || ip.is_unspecified() => return,
        Ok(IpAddr::V4(v4)) if v4.is_private() => InviteAddr::Lan(v4, lan_prefix),
        Ok(ip) => InviteAddr::Public(ip),
        Err(_)
            if !host.is_empty()
                && !host.eq_ignore_ascii_case("localhost")
                && host.len() <= 253
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) =>
        {
            InviteAddr::Host(host.to_ascii_lowercase())
        }
        Err(_) => return,
    };
    if !addrs.contains(&a) {
        addrs.push(a);
    }
}

// ---------------------------------------------------------------- the app side

impl App {
    /// The server I host, while I am logged into it (its own panel builds invites from what the
    /// router and STUN found instead of from the hub's hints).
    fn own_server(&self) -> Option<HostStatus> {
        let st = self.host.status().filter(|s| s.phase == Phase::Running)?;
        let port = st.hub_addr?.port();
        let (h, p) = url_host_port(&self.session_url)?;
        let loopback = h.eq_ignore_ascii_case("localhost")
            || h.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
        (self.me.is_some() && loopback && p == Some(port)).then_some(st)
    }

    /// The `vox1-` code of a new invite.
    fn invite_code(&self, c: &InviteCreated) -> Result<String, String> {
        let token = invite_token_from_hex(&c.token).ok_or("The server sent an invalid invite.")?;
        let (hub_port, addrs) = match self.own_server() {
            Some(st) => (
                st.hub_addr.map_or(c.hints.hub_port, |a| a.port()),
                st.invite_addrs(),
            ),
            None => {
                let mut addrs = Vec::new();
                let prefix = if c.hints.lan_prefix == 0 {
                    24
                } else {
                    c.hints.lan_prefix
                };
                let used = url_host_port(&self.session_url);
                if let Some((h, _)) = &used {
                    push_addr(&mut addrs, h, prefix);
                }
                push_addr(&mut addrs, &c.hints.public_host, prefix);
                if let Some(ip) = c.hints.lan_ipv4 {
                    push_addr(&mut addrs, &ip.to_string(), prefix);
                }
                let port = used.and_then(|(_, p)| p).unwrap_or(c.hints.hub_port);
                (port, addrs)
            }
        };
        if addrs.is_empty() {
            return Err("Could not build the invite: the server does not know its public address. The host can set \
                        public_host in config.toml."
                .into());
        }
        Ok(InviteCode {
            token,
            hub_port,
            addrs,
        }
        .encode())
    }

    pub fn invite_cmd(&mut self, cmd: InviteCmd) {
        self.host.inv.busy = true;
        self.backend.send(Cmd::Invite(cmd));
    }

    /// "Generate invite" with the chosen validity and use limit.
    pub fn create_invite(&mut self) {
        let req = InviteCreateRequest {
            expires_secs: EXPIRY[self.host.inv.expiry.min(3)].0,
            max_uses: USES[self.host.inv.uses.min(3)].0,
        };
        self.host.inv.error = None;
        self.invite_cmd(InviteCmd::Create(req));
    }

    pub fn on_invite_event(&mut self, ev: InviteEv) {
        let inv = &mut self.host.inv;
        inv.busy = false;
        let reply = match ev.result {
            Ok(r) => r,
            Err(e) => {
                inv.error = Some(e);
                return;
            }
        };
        match reply {
            InviteReply::List(list) => {
                inv.list = list;
                inv.loaded = true;
            }
            InviteReply::Created(c) => match self.invite_code(&c) {
                Ok(code) => {
                    tracing::info!(id = c.invite.id, len = code.len(), "invite created");
                    let inv = &mut self.host.inv;
                    inv.codes.insert(c.invite.id, code.clone());
                    inv.last = Some((c.invite.id, code));
                    inv.list.retain(|i| i.id != c.invite.id);
                    inv.list.insert(0, c.invite);
                }
                Err(e) => self.host.inv.error = Some(e),
            },
            InviteReply::Revoked => {
                inv.confirm_revoke = None;
                if let InviteCmd::Revoke(id) = ev.cmd {
                    if let Some(i) = inv.list.iter_mut().find(|i| i.id == id) {
                        i.revoked = true;
                    }
                    if inv.last.as_ref().is_some_and(|(l, _)| *l == id) {
                        inv.last = None;
                    }
                }
            }
        }
    }

    /// Joining with a `vox1-` code typed in "Server or invite".
    pub(crate) fn connect_invite(&mut self, text: &str, user: &str) {
        let code = match InviteCode::decode(text) {
            Ok(c) => c,
            Err(e) => {
                tracing::info!(error = %e, "invite code refused");
                self.form.error = Some(code_error_text(e).into());
                return;
            }
        };
        let urls = code.candidate_urls(&local_addrs());
        let Some(first) = urls.first().cloned() else {
            self.form.error = Some(code_error_text(InviteCodeError::Damaged).into());
            return;
        };
        self.form.busy = true;
        self.form.error = None;
        let personal = Some(self.form.personal.clone()).filter(|p| !p.is_empty());
        // A token this computer already has for one of the invite's addresses. v1.0.1: it goes
        // only to the server that issued it (its identity, else exactly that address), never to
        // another address of the code: anybody can write an invite code with any addresses.
        let token_at = urls
            .iter()
            .map(|u| crate::backend::normalize_url(u))
            .find_map(|u| self.settings.device_token(&u, user).map(|t| (u, t)));
        let guard = crate::backend::Guard {
            token: token_at.as_ref().map(|(at, _)| crate::backend::Binding {
                home: at.clone(),
                server_id: self.settings.device_token_server(at, user),
            }),
            password: None,
            confirmed: None,
        };
        let device_token = token_at.map(|(_, t)| t);
        tracing::info!(
            candidates = urls.len(),
            user,
            personal = personal.is_some(),
            device_token = device_token.is_some(),
            "connecting with an invite"
        );
        // v0.9.1: the saved entry keeps the invite's addresses (never its token).
        self.servers_login_started(device_token.is_some(), Some((code.hub_port, &code.addrs)));
        self.backend.send(Cmd::Connect {
            url: first,
            user: user.to_string(),
            password: String::new(),
            personal,
            device_token,
            // The device token is what lets this computer back in later: always ask for it.
            remember_device: true,
            invite: Some(InviteJoin {
                token: Some(code.token_hex()),
                urls,
            }),
            guard,
        });
    }

    // ------------------------------------------------------------ hosting

    pub(crate) fn refresh_servers(&mut self) {
        self.host.servers = hosting::list(&self.host.root);
        self.host.listed = true;
    }

    /// "Create server": a new server, opened at once.
    pub fn host_create(&mut self, name: &str) {
        self.host.error = None;
        if self.form.user.trim().chars().count() < 2 {
            self.host.error =
                Some("Enter your name (at least 2 letters) before creating the server.".into());
            return;
        }
        match hosting::create(&self.host.root, name) {
            Ok(h) => {
                self.host.new_name.clear();
                self.refresh_servers();
                self.host_open(h);
            }
            Err(e) => self.host.error = Some(format!("{e:#}")),
        }
    }

    /// "Open servidor": make sure the voice server is there (download once), then run it.
    pub fn host_open(&mut self, h: Hosted) {
        if self.host.hosting() {
            self.host.error =
                Some("You already have a running server. Stop it before opening another.".into());
            return;
        }
        if self.form.user.trim().chars().count() < 2 {
            self.host.error =
                Some("Enter your name (at least 2 letters) before opening the server.".into());
            return;
        }
        self.host.error = None;
        let (tx, rx) = channel();
        let bin_dir = hosting::bin_dir(&self.host.root);
        let wake = self.repaint.clone();
        tracing::info!(id = h.id, name = h.name, "host: opening");
        let spawned = std::thread::Builder::new()
            .name("voice-server".into())
            .spawn(move || {
                let r = prepare_voice_server(&bin_dir, &|n, total| {
                    let _ = tx.send(Prepared::Progress(n, total));
                    wake.wake();
                });
                let _ = tx.send(Prepared::Done(r));
                wake.wake();
            });
        if spawned.is_err() {
            self.host.error = Some("Could not prepare the server.".into());
            return;
        }
        self.host.preparing = Some((h, rx));
    }

    fn host_start(&mut self, h: Hosted, livekit: Option<PathBuf>) {
        let r = self.repaint.clone();
        let runner = HostRunner::start(
            &h,
            RunOptions::from_env(livekit),
            Arc::new(move || r.wake()),
        );
        self.host.runner = Some(runner);
        self.host.hosted = Some(h);
        self.host.auto_login = true;
    }

    /// "Stop server" (also logs out of it first).
    pub fn host_stop(&mut self) {
        if self.own_server().is_some()
            || (self.me.is_some()
                && self.host.runner.is_some()
                && self.session_url.contains("127.0.0.1"))
        {
            self.logout();
        }
        if let Some(r) = &self.host.runner {
            r.stop();
        }
        self.host.confirm_stop = false;
        self.host.auto_login = false;
    }

    /// Every frame: the voice server download, the server's state, the automatic login into it,
    /// and "leaving the app stops the server" (before `notify_tick` turns a close into the tray).
    pub(crate) fn host_tick(&mut self, ctx: &egui::Context) {
        if let Some((_, rx)) = &self.host.preparing {
            let mut done = None;
            while let Ok(m) = rx.try_recv() {
                match m {
                    Prepared::Progress(n, t) => self.host.fetching = Some((n, t)),
                    Prepared::Done(r) => done = Some(r),
                }
            }
            if let Some(r) = done {
                let (h, _) = self.host.preparing.take().expect("preparing");
                self.host.fetching = None;
                match r {
                    Ok(bin) => self.host_start(h, bin),
                    Err(e) => {
                        tracing::error!(error = %e, "host: voice server not ready");
                        self.host.error = Some(e);
                    }
                }
            }
        }

        if let Some(st) = self.host.status() {
            match &st.phase {
                Phase::Running
                    if self.host.auto_login
                        && self.screen == Screen::Connect
                        && !self.form.busy =>
                {
                    self.host.auto_login = false;
                    if let (Some(addr), Some(h)) = (st.hub_addr, self.host.hosted.clone()) {
                        tracing::info!(port = addr.port(), "host: logging into my own server");
                        self.form.url = format!("http://127.0.0.1:{}", addr.port());
                        self.form.password = h.password.clone();
                        self.connect();
                    }
                }
                Phase::Failed(e) => {
                    self.host.error = Some(format!("The server did not start: {e}"));
                    self.host.runner = None;
                    self.host.hosted = None;
                    self.host.panel_open = false;
                }
                Phase::Stopped => {
                    tracing::info!("host: server stopped");
                    self.host.runner = None;
                    self.host.hosted = None;
                    self.host.panel_open = false;
                }
                _ => {}
            }
        }

        // Logging into my own server needs my personal password (no remembered device for it yet):
        // the field for it is on the "Connect" side.
        if self.form.need_personal
            && self.host.tab == HomeTab::Host
            && self.screen == Screen::Connect
        {
            self.host.tab = HomeTab::Connect;
        }

        // The invite list is per server (and per login).
        let key = format!("{}|{:?}", self.session_url, self.me.as_ref().map(|u| u.id));
        if key != self.host.inv_for {
            self.host.inv_for = key;
            self.host.inv = InviteUi::default();
        }

        // Joined with a code: from now on the field shows the address that worked.
        if self.screen == Screen::Main && vox_proto::looks_like_invite(&self.form.url) {
            self.form.url = self.session_url.clone();
        }

        // Leaving the app (tray "Quit", or the close button when it does not go to the tray).
        let closing = ctx.input(|i| i.viewport().close_requested());
        if closing
            && self.host.hosting()
            && !self.host.quit_confirmed
            && self.auto.is_none()
            && !self
                .notify
                .hides_on_close(self.settings.notify.tray_on_close)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.notify.hidden {
                self.show_window(ctx);
            }
            self.host.confirm_quit = true;
        }
    }

    /// The app is closing: stop the server (and remove the router's port mappings).
    pub(crate) fn host_on_exit(&mut self) {
        if let Some(mut r) = self.host.runner.take() {
            tracing::info!("host: app closing, stopping my server");
            r.stop_and_wait(std::time::Duration::from_secs(8));
        }
    }

    // ------------------------------------------------------------ UI

    /// "Create or open server" on the home screen: my running server (with "Join server", its
    /// panel and "Stop server"), or my name, a new server and the ones made before.
    pub(crate) fn host_home(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 6.0;
        if let Some(st) = self.host.status() {
            let (text, color) = phase_text(&st.phase);
            ui_kit::group_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = space::M;
                    ui.label(
                        RichText::new(ph::HARD_DRIVES)
                            .font(theme::body(22.0))
                            .color(theme::NEVOA),
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.add(
                            egui::Label::new(
                                RichText::new(&st.name)
                                    .font(theme::display(size::TITLE))
                                    .color(theme::GIZ),
                            )
                            .truncate()
                            .selectable(false),
                        );
                        status_line(ui, &text, color);
                    });
                });
                ui.add_space(space::L);
                ui.horizontal_wrapped(|ui| {
                    if st.phase == Phase::Running
                        && ui_kit::button(ui, Kind::Primary, Some(ph::ARROW_RIGHT), "Join server")
                            .clicked()
                    {
                        self.host.auto_login = true;
                    }
                    if ui_kit::button(ui, Kind::Secondary, Some(ph::WRENCH), "Server panel")
                        .clicked()
                    {
                        self.host.panel_open = true;
                    }
                    if ui_kit::button(ui, Kind::Ghost, Some(ph::STOP), "Stop server").clicked() {
                        self.host.confirm_stop = true;
                    }
                });
            });
            if let Some(e) = &self.form.error {
                ui.add_space(space::M);
                notice(ui, ui_kit::Notice::Error, e);
            }
            return;
        }
        // macOS: the voice server (LiveKit) has no official macOS build to download yet (M4).
        if crate::platform::HOSTING_SOON {
            notice(ui, ui_kit::Notice::Info, &format!("Hosting a server from the app: {}. For now, join a friend's server using its address or an invite.", crate::platform::SOON_MAC.to_lowercase()));
            return;
        }
        let busy = self.host.preparing.is_some();
        ui_kit::field_label(ui, "Your name");
        ui.add(
            ui_kit::field(&mut self.form.user)
                .hint_text("how others will see you")
                .desired_width(ui.available_width().min(420.0)),
        );
        ui.add_space(space::L);
        ui_kit::section(ui, "Create a new server");
        let mut create = false;
        ui.horizontal(|ui| {
            let r = ui.add(
                ui_kit::field(&mut self.host.new_name)
                    .hint_text("server name")
                    .desired_width((ui.available_width() - 150.0).clamp(160.0, 270.0)),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let can = !busy && !self.host.new_name.trim().is_empty();
            create = (ui
                .add_enabled_ui(can, |ui| {
                    ui_kit::button(ui, Kind::Primary, Some(ph::PLUS), "Create server")
                })
                .inner
                .clicked()
                || enter)
                && can;
        });
        if create {
            let name = self.host.new_name.clone();
            self.host_create(&name);
        }
        if !self.host.servers.is_empty() {
            ui.add_space(space::L);
            ui_kit::section(ui, "Your servers");
            let mut open = None;
            ui_kit::group_frame()
                .inner_margin(egui::Margin::symmetric(6, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for h in &self.host.servers {
                        let (rect, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 40.0),
                            Sense::hover(),
                        );
                        let mut row = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
                                .layout(Layout::left_to_right(Align::Center)),
                        );
                        row.spacing_mut().item_spacing.x = space::M;
                        row.label(
                            RichText::new(ph::HARD_DRIVES)
                                .font(theme::body(size::ICON))
                                .color(theme::NEVOA),
                        );
                        row.add(
                            egui::Label::new(
                                RichText::new(&h.name)
                                    .font(theme::semibold(13.5))
                                    .color(theme::GIZ),
                            )
                            .truncate()
                            .selectable(false),
                        );
                        row.label(
                            RichText::new(format!("porta {}", h.ports.hub))
                                .font(theme::body(12.0))
                                .color(theme::NEVOA),
                        );
                        row.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_enabled_ui(!busy, |ui| {
                                    ui_kit::button_sized(
                                        ui,
                                        Kind::Secondary,
                                        Size::Small,
                                        None,
                                        "Open",
                                    )
                                })
                                .inner
                                .clicked()
                            {
                                open = Some(h.clone());
                            }
                        });
                    }
                });
            if let Some(h) = open {
                self.host_open(h);
            }
        }
        if busy {
            ui.add_space(space::L);
            match self.host.fetching {
                Some((n, total)) => {
                    ui_kit::body_text(ui, "Downloading the voice server (first time only)...");
                    let frac = total.map_or(0.0, |t| n as f32 / t.max(1) as f32);
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .show_percentage()
                            .desired_width(ui.available_width().min(420.0)),
                    );
                    ui_kit::hint(ui, format!("{:.1} MB", n as f64 / 1_048_576.0));
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui_kit::body_text(ui, "Preparing server...");
                    });
                }
            }
        }
        if let Some(e) = &self.host.error {
            ui.add_space(space::M);
            notice(ui, ui_kit::Notice::Error, e);
        }
        ui.add_space(space::XL);
        ui_kit::hint(
            ui,
            "The server runs on this computer while concord is open (or in the tray). \
             The app downloads the voice server on first use (about 20 MB). Then send an invite to your friends.",
        );
    }

    /// The host panel and the hosting confirmations ("Stop server", leaving the app while
    /// hosting, "Revoke invite"): the kit's dialog, the action on the right.
    pub(crate) fn host_windows(&mut self, ctx: &egui::Context) {
        self.host_panel(ctx);
        let name = self
            .host
            .hosted
            .as_ref()
            .map(|h| h.name.clone())
            .unwrap_or_default();
        if self.host.confirm_quit {
            let answer = ui_kit::confirm(
                ctx,
                "host_quit",
                "Quit concord?",
                "Quit and shut down",
                Kind::Danger,
                true,
                |ui| {
                    ui_kit::body_text(
                        ui,
                        format!("Your server \"{name}\" will shut down, disconnecting everyone."),
                    );
                    ui_kit::hint(ui, "To keep the server running, close the window with X: concord stays in the tray.");
                },
            );
            match answer {
                Some(true) => {
                    tracing::info!("host: leaving the app, the server goes down");
                    self.host.confirm_quit = false;
                    self.host.quit_confirmed = true;
                    if let Some(r) = &self.host.runner {
                        r.stop();
                    }
                    self.notify.allow_quit();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Some(false) => {
                    self.host.confirm_quit = false;
                    self.notify.cancel_quit();
                }
                None => {}
            }
        } else if self.host.confirm_stop {
            if self.host.status().is_none() {
                self.host.confirm_stop = false;
                return;
            }
            let answer = ui_kit::confirm(
                ctx,
                "host_stop",
                "Stop server",
                "Stop server",
                Kind::Danger,
                true,
                |ui| {
                    ui_kit::body_text(ui, format!("Stop server \"{name}\"?"));
                    ui_kit::hint(ui, "Everyone disconnects, including you. You can reopen it later in \"Create or open server\".");
                },
            );
            match answer {
                Some(true) => self.host_stop(),
                Some(false) => self.host.confirm_stop = false,
                None => {}
            }
        } else if let Some(id) = self.host.inv.confirm_revoke {
            let answer = ui_kit::confirm(
                ctx,
                "invite_revoke",
                "Revoke invite",
                "Revoke",
                Kind::Danger,
                !self.host.inv.busy,
                |ui| {
                    ui_kit::body_text(ui, format!("Revoke invite #{id}?"));
                    ui_kit::hint(ui, "Nobody else can join with this code. People who already joined stay on the server.");
                },
            );
            match answer {
                Some(true) => {
                    self.host.inv.error = None;
                    self.invite_cmd(InviteCmd::Revoke(id));
                }
                Some(false) => self.host.inv.confirm_revoke = None,
                None => {}
            }
        }
    }

    /// Any of the hosting confirmations is up (Esc belongs to it).
    pub(crate) fn host_confirming(&self) -> bool {
        self.host.confirm_quit || self.host.confirm_stop || self.host.inv.confirm_revoke.is_some()
    }

    /// "My server": the status and the addresses, what the router and the internet allow (UPnP,
    /// CGNAT), the invites and "Stop server", in the kit's groups.
    fn host_panel(&mut self, ctx: &egui::Context) {
        if !self.host.panel_open {
            return;
        }
        let Some(st) = self.host.status() else {
            self.host.panel_open = false;
            return;
        };
        let mine = self.own_server().is_some();
        if mine && !self.host.inv.loaded && !self.host.inv.busy && self.my_role().can_admin() {
            self.invite_cmd(InviteCmd::List);
        }
        let mut open = !(ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && !self.host_confirming()
            && !egui::Popup::is_any_open(ctx));
        let screen = ctx.content_rect();
        let h = (screen.height() - 48.0).clamp(300.0, 640.0);
        egui::Window::new("My server")
            .id(egui::Id::new("host_panel"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .fixed_size(Vec2::new(PANEL_W, h))
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                ui_kit::dialog_frame(&ctx.global_style()).inner_margin(egui::Margin {
                    left: 22,
                    right: 10,
                    top: 18,
                    bottom: 12,
                }),
            )
            .show(ctx, |ui| {
                ui.set_width(PANEL_W);
                // The head: icon, name, state, close.
                let (text, color) = phase_text(&st.phase);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = space::M;
                    ui.label(
                        RichText::new(ph::HARD_DRIVES)
                            .font(theme::body(24.0))
                            .color(theme::NEVOA),
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(
                            RichText::new("My server")
                                .font(theme::body(12.5))
                                .color(theme::NEVOA),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&st.name)
                                    .font(theme::display(size::HEADING))
                                    .color(theme::GIZ),
                            )
                            .truncate()
                            .selectable(false),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        ui.add_space(space::M);
                        if ui_kit::icon_button(ui, ph::X, "Close", IconState::Normal).clicked() {
                            open = false;
                        }
                        status_pill(ui, &text, color);
                    });
                });
                ui.add_space(space::L);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(PANEL_W - 14.0);
                        ui.spacing_mut().item_spacing.y = 6.0;
                        self.host_panel_body(ui, &st, mine);
                        ui.add_space(space::M);
                    });
            });
        if !open {
            self.host.panel_open = false;
        }
    }

    fn host_panel_body(&mut self, ui: &mut egui::Ui, st: &HostStatus, mine: bool) {
        let label_w = 150.0;
        ui_kit::section(ui, "Addresses");
        ui_kit::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let value = |ui: &mut egui::Ui, t: String| ui.label(RichText::new(t).color(theme::GIZ));
            let mono = |ui: &mut egui::Ui, t: String| {
                ui.label(RichText::new(t).monospace().color(theme::GIZ))
            };
            ui_kit::info_row(ui, label_w, "Ports", |ui| {
                value(
                    ui,
                    st.ports
                        .map(|p| p.describe())
                        .unwrap_or_else(|| "...".into()),
                )
            });
            ui_kit::info_row(ui, label_w, "On the home network", |ui| {
                mono(
                    ui,
                    st.lan
                        .map(|(ip, _)| ip.to_string())
                        .unwrap_or_else(|| "?".into()),
                )
            });
            let public = match (st.public_ipv4(), st.public_v6) {
                (Some(v4), _) => v4.to_string(),
                (None, Some(v6)) => v6.to_string(),
                (None, None) => "not discovered".into(),
            };
            ui_kit::info_row(ui, label_w, "Public address", |ui| mono(ui, public));
            let online = if mine {
                self.online.len().to_string()
            } else {
                "join the server to view".into()
            };
            ui_kit::info_row(ui, label_w, "People online", |ui| value(ui, online));
            ui_kit::info_row(ui, label_w, "Server password", |ui| {
                if self.host.show_password {
                    ui.label(RichText::new(&st.password).monospace().color(theme::GIZ));
                    if ui_kit::button_sized(ui, Kind::Ghost, Size::Small, Some(ph::COPY), "Copy")
                        .clicked()
                    {
                        ui.ctx().copy_text(st.password.clone());
                    }
                } else if ui_kit::button_sized(
                    ui,
                    Kind::Secondary,
                    Size::Small,
                    Some(ph::EYE),
                    "Show",
                )
                .on_hover_text("For people joining without an invite (invites bypass the password)")
                .clicked()
                {
                    self.host.show_password = true;
                }
            });
        });
        ui.add_space(space::L);
        ui_kit::section(ui, "Internet access");
        let (reach, rc) = reach_text(st);
        let kind = if rc == theme::ONLINE {
            None
        } else if rc == theme::AO_VIVO {
            Some(ui_kit::Notice::Error)
        } else {
            Some(ui_kit::Notice::Warning)
        };
        match kind {
            None => {
                ui_kit::group_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui_kit::result_text(ui, true, &reach);
                });
            }
            Some(k) => notice(ui, k, &reach),
        }
        ui_kit::hint(ui, FIREWALL_TEXT);
        ui.add_space(space::L);
        ui_kit::section(ui, "Invites");
        if !mine {
            ui_kit::hint(ui, "Join your server (\"Create or open server\", then \"Join server\") to create invites.");
        } else if !self.my_role().can_admin() {
            ui_kit::hint(ui, "Set your personal password (Settings, Personal password) to enable Owner permissions and create invites.");
        } else {
            ui_kit::hint(ui, INVITE_HINT);
            ui.add_space(space::S);
            self.invite_tools(ui);
        }
        ui.add_space(space::L);
        ui_kit::section(ui, "Turn off");
        ui_kit::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui_kit::button(ui, Kind::Danger, Some(ph::STOP), "Stop server").clicked() {
                        self.host.confirm_stop = true;
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui_kit::hint(
                            ui,
                            "Shuts down the server: everyone disconnects, including you.",
                        );
                    });
                });
            });
        });
    }

    /// "Generate invite", the code to copy, and the list with "Revoke" (host panel, and the server
    /// window's "Invites" page). Revoking asks first (`host_windows`).
    pub(crate) fn invite_tools(&mut self, ui: &mut egui::Ui) {
        if !self.host.inv.loaded && !self.host.inv.busy {
            self.invite_cmd(InviteCmd::List);
        }
        let inv_busy = self.host.inv.busy;
        ui_kit::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = space::M;
                ui.vertical(|ui| {
                    ui_kit::field_label(ui, "Expires after");
                    egui::ComboBox::from_id_salt("inv_expiry").width(150.0).selected_text(EXPIRY[self.host.inv.expiry].1).show_ui(ui, |ui| {
                        for (i, (_, label)) in EXPIRY.iter().enumerate() {
                            ui.selectable_value(&mut self.host.inv.expiry, i, *label);
                        }
                    });
                });
                ui.vertical(|ui| {
                    ui_kit::field_label(ui, "Can be used by");
                    egui::ComboBox::from_id_salt("inv_uses").width(150.0).selected_text(USES[self.host.inv.uses].1).show_ui(ui, |ui| {
                        for (i, (_, label)) in USES.iter().enumerate() {
                            ui.selectable_value(&mut self.host.inv.uses, i, *label);
                        }
                    });
                });
            });
            ui.add_space(space::S);
            if ui.add_enabled_ui(!inv_busy, |ui| ui_kit::button(ui, Kind::Primary, Some(ph::PLUS), "Generate invite")).inner.clicked() {
                self.create_invite();
            }
            if let Some((_, code)) = self.host.inv.last.clone() {
                ui.add_space(space::M);
                ui_kit::body_text(ui, "Invite ready! Send this code to friends (WhatsApp, Discord...). They paste it into concord's \"Server or invite\" field:");
                egui::Frame::new().fill(theme::FUNDO).corner_radius(theme::radius::ROW).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(RichText::new(&code).monospace().color(theme::GIZ)).wrap());
                });
                ui.horizontal(|ui| {
                    if ui_kit::button(ui, Kind::Secondary, Some(ph::COPY), "Copy invite").clicked() {
                        ui.ctx().copy_text(code.clone());
                        self.host.inv.copied_at = Some(Instant::now());
                    }
                    if let Some(t) = self.host.inv.copied_at {
                        let left = std::time::Duration::from_secs(3).saturating_sub(t.elapsed());
                        if !left.is_zero() {
                            ui_kit::result_text(ui, true, "Copied!");
                            ui.ctx().request_repaint_after(left);
                        }
                    }
                });
            }
            if let Some(e) = &self.host.inv.error {
                ui_kit::result_text(ui, false, e);
            }
        });
        ui.add_space(space::M);
        ui.horizontal(|ui| {
            ui_kit::section(ui, "Created invites");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled_ui(!inv_busy, |ui| {
                        ui_kit::icon_button(
                            ui,
                            ph::ARROWS_CLOCKWISE,
                            "Refresh list",
                            IconState::Normal,
                        )
                    })
                    .inner
                    .clicked()
                {
                    self.invite_cmd(InviteCmd::List);
                }
            });
        });
        let now = crate::app::now_ms();
        let list = self.host.inv.list.clone();
        if list.is_empty() && self.host.inv.loaded {
            ui_kit::hint(ui, "No invites yet.");
        }
        if !list.is_empty() {
            ui_kit::group_frame()
                .inner_margin(egui::Margin::symmetric(6, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for i in &list {
                        self.invite_row(ui, i, now);
                    }
                });
        }
        ui.add_space(space::S);
        ui_kit::hint(ui, HONEST_TEXT);
    }

    /// One invite: who made it, uses and validity; "Copy" (codes made in this session) and
    /// "Revoke" while it is valid, its state otherwise.
    fn invite_row(&mut self, ui: &mut egui::Ui, i: &Invite, now: i64) {
        let state = i.state(now);
        let active = state == InviteState::Active;
        let uses = match i.max_uses {
            Some(m) => format!("usado {}/{m}", i.uses),
            None => format!("usado {}x", i.uses),
        };
        let until = match (state, i.expires_at_ms) {
            (InviteState::Revoked, _) => "revogado".to_string(),
            (InviteState::Expired, _) => "vencido".to_string(),
            (InviteState::UsedUp, _) => "esgotado".to_string(),
            (InviteState::Active, Some(e)) => format!("valid until {}", when_text(e)),
            (InviteState::Active, None) => "no expiry".to_string(),
        };
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::hover());
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, theme::radius::ROW, theme::ROW_HOVER);
        }
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        let ui = &mut row;
        ui.spacing_mut().item_spacing.x = space::M;
        let fg = if active { theme::GIZ } else { theme::NEVOA };
        ui.label(
            RichText::new(ph::ENVELOPE_SIMPLE)
                .font(theme::body(size::ICON))
                .color(theme::NEVOA),
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(
                RichText::new(format!("#{} by {}", i.id, i.created_by_name))
                    .font(theme::semibold(13.5))
                    .color(fg),
            );
            ui.label(
                RichText::new(format!("{uses} · {until}"))
                    .font(theme::body(12.0))
                    .color(theme::NEVOA),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = space::S;
            if !active {
                ui_kit::pill(ui, &until, theme::MOVEL, theme::NEVOA);
                return;
            }
            if ui_kit::button_sized(ui, Kind::Secondary, Size::Small, None, "Revoke")
                .on_hover_text(
                    "Nobody else can join with this invite (people who already joined stay)",
                )
                .clicked()
            {
                self.host.inv.confirm_revoke = Some(i.id);
            }
            if let Some(code) = self.host.inv.codes.get(&i.id) {
                if ui_kit::button_sized(ui, Kind::Ghost, Size::Small, Some(ph::COPY), "Copy")
                    .clicked()
                {
                    ui.ctx().copy_text(code.clone());
                }
            }
        });
    }
}

/// Width of the host panel's content.
const PANEL_W: f32 = 540.0;

/// What an invite does, said once above the invite tools (and as the server window's subtitle).
pub const INVITE_HINT: &str =
    "An invite bypasses the server password. The person's computer is then remembered.";

/// The server's state: a dot in its colour and the text.
fn status_line(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter().circle_filled(dot.center(), 4.0, color);
        ui.add(
            egui::Label::new(RichText::new(text).font(theme::semibold(12.5)).color(color)).wrap(),
        );
    });
}

/// The server's state as a pill (the panel's head).
fn status_pill(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    let fill = if color == theme::ONLINE {
        egui::Color32::from_rgb(0x1E, 0x3A, 0x2E)
    } else if color == theme::AO_VIVO {
        theme::PERIGO_FUNDO
    } else {
        theme::MOVEL
    };
    ui_kit::pill(ui, text, fill, color);
}

/// A notice in the kit's frame: its icon, the text.
fn notice(ui: &mut egui::Ui, kind: ui_kit::Notice, text: &str) {
    ui_kit::notice_frame(kind).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui_kit::notice_text(ui, kind, text);
    });
}

/// The voice server for a hosted server: `VOX_HOST_NO_LIVEKIT=1` runs without one (tests);
/// `VOX_HOST_LIVEKIT_BIN` names an exe to use in place (tests; still checked against the pin);
/// otherwise the installed copy, else `VOX_LIVEKIT_SOURCE` (a local folder with the release zip,
/// or the exe), else the download. Runs on its own thread.
fn prepare_voice_server(
    bin_dir: &std::path::Path,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<Option<PathBuf>, String> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    if env("VOX_HOST_NO_LIVEKIT").is_some_and(|v| v != "0") {
        return Ok(None);
    }
    if let Some(p) = env("VOX_HOST_LIVEKIT_BIN").map(PathBuf::from) {
        return match livekit_fetch::sha256_file(&p) {
            Ok(h) if h == livekit_fetch::PINNED.exe_sha256 => Ok(Some(p)),
            _ => Err("The specified voice server does not match the expected SHA-256 hash.".into()),
        };
    }
    if let Some(exe) = livekit_fetch::installed(bin_dir) {
        return Ok(Some(exe));
    }
    if let Some(src) = env("VOX_LIVEKIT_SOURCE").map(PathBuf::from) {
        return livekit_fetch::install_from(bin_dir, &src, &livekit_fetch::PINNED)
            .map(Some)
            .map_err(|e| format!("{e:#}"));
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(livekit_fetch::download(bin_dir, progress))
        .map(Some)
        .map_err(|e| {
            tracing::error!(error = %format!("{e:#}"), "livekit download failed");
            // The first line of the chain is the message for people.
            format!("{e}")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_addresses_from_what_the_client_knows() {
        assert_eq!(
            url_host_port("http://203.0.113.5:7890"),
            Some(("203.0.113.5".into(), Some(7890)))
        );
        assert_eq!(
            url_host_port("http://[2001:db8::1]:7990/x"),
            Some(("2001:db8::1".into(), Some(7990)))
        );
        assert_eq!(
            url_host_port("http://meu.example.org"),
            Some(("meu.example.org".into(), None))
        );
        let mut a = Vec::new();
        push_addr(&mut a, "127.0.0.1", 24);
        push_addr(&mut a, "localhost", 24);
        push_addr(&mut a, "192.168.0.20", 24);
        push_addr(&mut a, "203.0.113.5", 24);
        push_addr(&mut a, "203.0.113.5", 24);
        push_addr(&mut a, "Casa.Example.org", 24);
        push_addr(&mut a, "nao vale", 24);
        assert_eq!(
            a,
            [
                InviteAddr::Lan("192.168.0.20".parse().unwrap(), 24),
                InviteAddr::Public("203.0.113.5".parse().unwrap()),
                InviteAddr::Host("casa.example.org".into())
            ]
        );
    }

    #[test]
    fn code_errors_read_in_english() {
        for e in [
            InviteCodeError::Typo,
            InviteCodeError::NewerVersion,
            InviteCodeError::Damaged,
            InviteCodeError::NotAnInvite,
        ] {
            assert!(!code_error_text(e).is_empty());
        }
        assert!(code_error_text(InviteCodeError::Typo).contains("vox1-"));
    }
}
