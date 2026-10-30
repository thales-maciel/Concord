//! Item 12 in the app: roles, the personal password, moderation and "Server settings".
//!
//! The hub enforces every permission; this module only shows what the user's role allows (the
//! matrix lives in `vox_proto::Role`). On a hub without roles (`Ready` has no `account`), none of
//! it is shown: no badges, no moderation menus, no message deletion, no server dialog.

use eframe::egui::{self, Color32, RichText, Sense, Vec2};
use vox_client_core::hub::HubClient;
use vox_proto::{
    Account, AuditEntry, ChannelId, ChannelKind, ClaimOwnerResponse, MemberInfo, MessageId,
    ModAction, PrivateAccess, ResetPasswordResponse, Role, ServerEvent, SetPasswordRequest,
    SetPasswordResponse, User, UserId, PERSONAL_PASSWORD_MAX, PERSONAL_PASSWORD_MIN,
};

use crate::app::{App, Center};
use crate::backend::Cmd;
use crate::theme::{self, ph, radius, size, space};
use crate::ui_kit::{self, IconState, Kind, Size};

// v1.0: the role colours stay away from the lamp (gold = speaking / main action / mentions) and
// from "live" red: lavender, rose and teal, soft enough for the night-room palette.
const OWNER_COLOR: Color32 = Color32::from_rgb(0xC3, 0xA6, 0xF5);
const ADMIN_COLOR: Color32 = Color32::from_rgb(0xE9, 0x9A, 0xC0);
const MOD_COLOR: Color32 = Color32::from_rgb(0x62, 0xC6, 0xD0);
/// Audit log page size.
const AUDIT_PAGE: u32 = 100;

pub fn role_label(role: Role) -> &'static str {
    match role {
        Role::Owner => "Owner",
        Role::Admin => "Admin",
        Role::Moderator => "Moderator",
        Role::Member => "Member",
    }
}

/// Staff names are drawn in their role's color (members keep their usual color).
pub fn role_color(role: Role) -> Option<Color32> {
    match role {
        Role::Owner => Some(OWNER_COLOR),
        Role::Admin => Some(ADMIN_COLOR),
        Role::Moderator => Some(MOD_COLOR),
        Role::Member => None,
    }
}

pub(crate) fn role_icon(role: Role) -> Option<&'static str> {
    match role {
        Role::Owner => Some(crate::theme::ph::CROWN),
        Role::Admin => Some(crate::theme::ph::SHIELD),
        Role::Moderator => Some(crate::theme::ph::GAVEL),
        Role::Member => None,
    }
}

/// A small icon after a staff member's name (nothing for members).
pub fn role_badge(ui: &mut egui::Ui, role: Role) {
    if let (Some(icon), Some(color)) = (role_icon(role), role_color(role)) {
        ui.label(RichText::new(icon).small().color(color))
            .on_hover_text(role_label(role));
    }
}

/// `name` in its role's color, or `fallback` for members.
pub fn role_name(name: &str, role: Role, fallback: Option<Color32>) -> RichText {
    let t = RichText::new(name);
    match role_color(role).or(fallback) {
        Some(c) => t.color(c),
        None => t,
    }
}

fn audit_action_pt(action: &str) -> String {
    match action {
        vox_proto::AUDIT_SERVER_MUTE => "server-muted".into(),
        vox_proto::AUDIT_SERVER_UNMUTE => "unmuted".into(),
        vox_proto::AUDIT_DISCONNECT => "disconnected from voice".into(),
        vox_proto::AUDIT_MOVE => "moved".into(),
        vox_proto::AUDIT_STOP_STREAM => "ended the stream of".into(),
        vox_proto::AUDIT_DELETE_MESSAGE => "deleted a message from".into(),
        vox_proto::AUDIT_SET_ROLE => "changed the role of".into(),
        vox_proto::AUDIT_RESET_PASSWORD => "reset the personal password of".into(),
        vox_proto::AUDIT_CHANNEL_CREATE => "created channel".into(),
        vox_proto::AUDIT_CHANNEL_RENAME => "renamed channel".into(),
        vox_proto::AUDIT_CHANNEL_DELETE => "deleted channel".into(),
        vox_proto::AUDIT_CHANNEL_REORDER => "reordered channels".into(),
        vox_proto::AUDIT_CHANNEL_ACCESS => "changed channel access".into(),
        vox_proto::AUDIT_REMOVE_AVATAR => "removed the profile picture of".into(),
        vox_proto::AUDIT_CLEAR_PROFILE => "cleared the profile of".into(),
        vox_proto::AUDIT_CLAIM_OWNER => "became the server Owner".into(),
        vox_proto::AUDIT_TRANSFER_OWNER => "transferred server ownership to".into(),
        vox_proto::AUDIT_RESET_OWNER => {
            "removed the server owner (command line), previously".into()
        }
        vox_proto::AUDIT_SOUND_DELETE => "deleted a soundboard sound from".into(),
        vox_proto::AUDIT_SOUND_RENAME => "renamed a soundboard sound from".into(),
        vox_proto::AUDIT_INVITE_CREATE => "created invite".into(),
        vox_proto::AUDIT_INVITE_REVOKE => "revoked invite".into(),
        vox_proto::AUDIT_INVITE_JOIN => "joined with invite".into(),
        other => other.to_string(),
    }
}

/// Translate legacy audit tokens without changing their persisted representation.
/// The hub's "#3 86400s 5x" becomes "#3 · valid for 24 hours · up to 5 people".
fn invite_detail_pt(detail: &str) -> String {
    let detail = detail
        .replace("sem prazo", "sem-prazo")
        .replace("no expiry", "sem-prazo");
    let mut out = Vec::new();
    for p in detail.split_whitespace() {
        let secs = p.strip_suffix('s').and_then(|n| n.parse::<u64>().ok());
        let uses = p.strip_suffix('x').and_then(|n| n.parse::<u32>().ok());
        out.push(match (secs, uses, p) {
            (Some(secs), _, _) => {
                match crate::host::EXPIRY.iter().find(|(e, _)| *e == Some(secs)) {
                    Some((_, label)) => format!("valid for {label}"),
                    None if secs % 3600 == 0 => format!("valid for {} h", secs / 3600),
                    None => format!("valid for {secs} s"),
                }
            }
            (_, Some(1), _) => "1 person".to_string(),
            (_, Some(n), _) => format!("up to {n} people"),
            (_, _, "sem-prazo") => "no expiry".to_string(),
            (_, _, "ilimitado") => "unlimited people".to_string(),
            (_, _, other) => other.to_string(),
        });
    }
    out.join(" · ")
}

/// "member -> moderator" -> "from Member to Moderator" (the font has no arrow glyph).
fn audit_detail_pt(action: &str, detail: &str) -> String {
    if action == vox_proto::AUDIT_CLAIM_OWNER {
        return match detail {
            "pc-do-servidor" => "by signing in from the server PC itself".into(),
            "codigo" => "with the server code".into(),
            other => other.to_string(),
        };
    }
    if action == vox_proto::AUDIT_INVITE_CREATE {
        return invite_detail_pt(detail);
    }
    if action == vox_proto::AUDIT_SET_ROLE {
        if let Some((a, b)) = detail.split_once(" -> ") {
            return format!(
                "from {} to {}",
                role_label(Role::from_wire(a)),
                role_label(Role::from_wire(b))
            );
        }
    }
    match detail.split_once(" -> ") {
        Some((a, b)) => format!("from {a} to {b}"),
        None => detail.to_string(),
    }
}

// ---------------------------------------------------------------- backend side

/// The item-12 HTTP calls, run by the backend ([`Cmd::Admin`]); the answer comes back as
/// `Ev::Admin` with the same command.
#[derive(Debug, Clone)]
pub enum AdminCmd {
    SetPassword(SetPasswordRequest),
    Mute {
        user: UserId,
        muted: bool,
    },
    Disconnect {
        user: UserId,
    },
    Move {
        user: UserId,
        channel: ChannelId,
    },
    StopStream {
        user: UserId,
    },
    DeleteMessage {
        id: MessageId,
    },
    ListUsers,
    SetRole {
        user: UserId,
        role: Role,
    },
    ResetPassword {
        user: User,
    },
    Audit {
        before: Option<i64>,
    },
    /// v0.9: `private` makes it private from the start.
    CreateChannel {
        name: String,
        kind: ChannelKind,
        private: Option<PrivateAccess>,
    },
    /// "Canal privado" (v0.9): private with this list, or public again (`None`).
    SetChannelAccess {
        id: ChannelId,
        private: Option<PrivateAccess>,
    },
    RenameChannel {
        id: ChannelId,
        name: String,
    },
    DeleteChannel {
        id: ChannelId,
    },
    ReorderChannels {
        ids: Vec<ChannelId>,
    },
    /// "Claim server": take a server with no owner with the code its console shows.
    ClaimOwner {
        code: String,
    },
    /// "Transfer server ownership" (owner only).
    TransferOwner {
        user: User,
    },
}

#[derive(Debug)]
pub enum AdminReply {
    Done,
    Password(SetPasswordResponse),
    Users(Vec<MemberInfo>),
    Audit(Vec<AuditEntry>),
    ResetCode(ResetPasswordResponse),
    Claimed(ClaimOwnerResponse),
}

pub async fn run_admin(hub: &HubClient, cmd: &AdminCmd) -> Result<AdminReply, String> {
    use AdminCmd as C;
    let r = match cmd.clone() {
        C::SetPassword(req) => hub.set_password(&req).await.map(AdminReply::Password),
        C::Mute { user, muted } => hub.mod_mute(user, muted).await.map(|_| AdminReply::Done),
        C::Disconnect { user } => hub.mod_disconnect(user).await.map(|_| AdminReply::Done),
        C::Move { user, channel } => hub.mod_move(user, channel).await.map(|_| AdminReply::Done),
        C::StopStream { user } => hub.mod_stop_stream(user).await.map(|_| AdminReply::Done),
        C::DeleteMessage { id } => hub.delete_message(id).await.map(|_| AdminReply::Done),
        C::ListUsers => hub.admin_users().await.map(AdminReply::Users),
        C::SetRole { user, role } => hub.set_role(user, role).await.map(|_| AdminReply::Done),
        C::ResetPassword { user } => hub.reset_password(user.id).await.map(AdminReply::ResetCode),
        C::Audit { before } => hub
            .audit_log(before, AUDIT_PAGE)
            .await
            .map(AdminReply::Audit),
        C::CreateChannel {
            name,
            kind,
            private,
        } => hub
            .create_channel(&name, kind, private)
            .await
            .map(|_| AdminReply::Done),
        C::SetChannelAccess { id, private } => hub
            .set_channel_access(id, private)
            .await
            .map(|_| AdminReply::Done),
        C::RenameChannel { id, name } => hub
            .rename_channel(id, &name)
            .await
            .map(|_| AdminReply::Done),
        C::DeleteChannel { id } => hub.delete_channel(id).await.map(|_| AdminReply::Done),
        C::ReorderChannels { ids } => hub.reorder_channels(ids).await.map(|_| AdminReply::Done),
        C::ClaimOwner { code } => hub.claim_owner(&code).await.map(AdminReply::Claimed),
        C::TransferOwner { user } => hub.transfer_owner(user.id).await.map(|_| AdminReply::Done),
    };
    r.map_err(|e| {
        // Passwords and codes never go to the log: only which call failed.
        let what = match cmd {
            C::SetPassword(_) => "SetPassword".to_string(),
            C::ClaimOwner { .. } => "ClaimOwner".to_string(),
            other => format!("{other:?}"),
        };
        tracing::warn!(error = %format!("{e:#}"), cmd = %what, "admin action failed");
        admin_error(&e)
    })
}

/// Hub answers in English, for the people.
pub fn admin_error(e: &anyhow::Error) -> String {
    let full = format!("{e:#}");
    let l = full.to_lowercase();
    if l.contains("owner proof required") {
        "For safety, the owner's first personal password can only be set from the app on the server PC itself \
         (address 127.0.0.1), or with the code shown in the server window. Enter that code in \
         \"Server code\" and save again."
            .into()
    } else if l.contains("wrong claim code") {
        "Incorrect code. Check the code shown in the server window.".into()
    } else if l.contains("already has an owner") {
        "This server already has an owner.".into()
    } else if l.contains("config.toml") {
        "This server's owner is pinned in config.toml (owner = ...): the host must edit that file and restart the server to change it.".into()
    } else if l.contains("ownership changed meanwhile") {
        "Server ownership changed during your request. Open the list again.".into()
    } else if l.contains("you are already the owner") {
        "You are already the owner.".into()
    } else if l.contains("personal password first") {
        "This person must set a personal password first (Settings > Personal password).".into()
    } else if l.contains(vox_proto::ERR_WRONG_PERSONAL_PASSWORD) {
        "The current password is incorrect.".into()
    } else if l.contains(vox_proto::ERR_TOO_MANY_ATTEMPTS) {
        "Too many attempts. Wait a few seconds and try again.".into()
    } else if l.contains("personal password must be") {
        format!("Personal passwords must contain {PERSONAL_PASSWORD_MIN} to {PERSONAL_PASSWORD_MAX} characters.")
    } else if l.contains("your role needs a personal password") {
        "Your role requires a personal password: it cannot be removed.".into()
    } else if l.contains("not in voice") {
        "This person is not in a voice channel.".into()
    } else if l.contains("not streaming") {
        "This person is not streaming.".into()
    } else if l.contains("no access to that private channel") {
        "This person does not have access to this private channel.".into()
    } else if l.contains("already in that channel") {
        "This person is already in this channel.".into()
    } else if l.contains("last text channel") {
        "The last text channel cannot be deleted.".into()
    } else if l.contains("channel name must be") {
        format!(
            "Channel names must contain 1 to {} characters.",
            vox_proto::CHANNEL_NAME_MAX
        )
    } else if l.contains("http 403") {
        "Your role does not allow this.".into()
    } else if l.contains("http 404") || l.contains("http 405") {
        "Not found: it may have been deleted, or the server is an older version.".into()
    } else {
        crate::backend::friendly(e)
    }
}

// ---------------------------------------------------------------- UI state

/// The pages of the server window ("Server").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServerTab {
    /// "Members and roles".
    #[default]
    Members,
    Channels,
    /// v0.8 (`host.rs`).
    Invites,
    /// "Moderation log".
    Audit,
    /// "Server settings": about the server, its ownership, my password.
    Settings,
}

#[derive(Default)]
pub struct RolesUi {
    /// The hub knows roles (its `Ready` carried an account).
    pub supported: bool,
    pub account: Option<Account>,
    /// Our voice session says LiveKit does not accept our microphone.
    pub session_server_muted: bool,
    /// The server window ("Server").
    pub server_open: bool,
    pub tab: ServerTab,
    pub members: Vec<MemberInfo>,
    /// "Members and roles": the search box.
    pub members_search: String,
    /// "Server settings": who "Transfer ownership" would go to.
    pub transfer_pick: Option<UserId>,
    pub members_loaded: bool,
    pub audit: Vec<AuditEntry>,
    pub audit_loaded: bool,
    pub audit_more: bool,
    pub new_channel: String,
    pub new_voice: bool,
    pub renaming: Option<(ChannelId, String)>,
    /// "Canal privado" of the new channel (v0.9).
    pub new_access: AccessDraft,
    /// The access editor open under one channel (v0.9).
    pub editing_access: Option<(ChannelId, AccessDraft)>,
    pub confirm_delete: Option<ChannelId>,
    pub confirm_reset: Option<User>,
    /// "Transfer server ownership": waiting for the owner's confirmation for this person.
    pub confirm_transfer: Option<User>,
    // "Claim server" (a server with no owner).
    pub claim_open: bool,
    pub claim_code: String,
    pub claim_busy: bool,
    pub claim_msg: Option<String>,
    /// The last reset code to pass on: (username, code).
    pub reset_code: Option<(String, ResetPasswordResponse)>,
    pub server_error: Option<String>,
    // The personal password form (Settings, and the prompt after login).
    pub pw_current: String,
    pub pw_new: String,
    pub pw_confirm: String,
    pub pw_busy: bool,
    pub pw_msg: Option<(bool, String)>,
    /// The code the server's window shows, for the owner's first password from another PC.
    pub pw_owner_code: String,
    /// "Not now" on the prompt (it comes back at the next login).
    pub prompt_later: bool,
    /// Moderation notices received (for the autotest), newest last.
    pub moderated: Vec<ModAction>,
}

/// What the "Canal privado" fields show (v0.9): the new channel's, or one channel's being edited.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessDraft {
    pub private: bool,
    pub moderator: bool,
    pub member: bool,
    pub users: Vec<UserId>,
    /// The member picker's search box.
    pub search: String,
}

impl AccessDraft {
    pub fn of(p: Option<&PrivateAccess>) -> Self {
        match p {
            None => AccessDraft::default(),
            Some(p) => AccessDraft {
                private: true,
                moderator: p.roles.contains(&Role::Moderator),
                member: p.roles.contains(&Role::Member),
                users: p.users.clone(),
                search: String::new(),
            },
        }
    }

    /// What to send to the hub (`None`: public).
    pub fn to_access(&self) -> Option<PrivateAccess> {
        if !self.private {
            return None;
        }
        let roles = [
            (self.moderator, Role::Moderator),
            (self.member, Role::Member),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, r)| r)
        .collect();
        Some(
            PrivateAccess {
                roles,
                users: self.users.clone(),
            }
            .normalized(),
        )
    }
}

/// People the picker offers for `query`: Moderator and Member (Owner and Admin always have access),
/// not already chosen, by username or display name, at most eight.
fn pick_candidates<'a>(
    members: &'a [MemberInfo],
    chosen: &[UserId],
    query: &str,
) -> Vec<&'a MemberInfo> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    members
        .iter()
        .filter(|m| m.user.role < Role::Admin && !chosen.contains(&m.user.id))
        .filter(|m| {
            m.user.username.to_lowercase().contains(&q)
                || m.user
                    .display_name
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(&q))
        })
        .take(8)
        .collect()
}

/// The "Canal privado" fields (v0.9): the switch, "Roles with access", "People with access" (a
/// member picker with search) and the reminder that Owner and Admin always have access.
fn access_fields(
    ui: &mut egui::Ui,
    d: &mut AccessDraft,
    members: &[MemberInfo],
    loaded: bool,
    salt: (&str, ChannelId),
) {
    ui.checkbox(&mut d.private, format!("{}  Private channel", crate::app::ICON_PRIVATE))
        .on_hover_text("Only people with access can see this channel, its history and images, or join voice. Losing access removes them immediately.");
    if !d.private {
        return;
    }
    ui.indent(salt, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui_kit::hint(ui, "Owner and Admin always have access.");
        ui_kit::field_label(ui, "Roles with access");
        ui.horizontal(|ui| {
            ui.checkbox(&mut d.moderator, "Moderator");
            ui.checkbox(&mut d.member, "Member");
        });
        ui_kit::field_label(ui, "People with access");
        let name_of = |id: UserId| {
            members
                .iter()
                .find(|m| m.user.id == id)
                .map(|m| m.user.shown_name().to_string())
                .unwrap_or_else(|| format!("#{id}"))
        };
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if d.users.is_empty() {
                ui_kit::hint(ui, "Nobody selected.");
            }
            for &id in &d.users {
                let chip = ui_kit::button_sized(
                    ui,
                    Kind::Secondary,
                    Size::Small,
                    Some(ph::X),
                    &name_of(id),
                );
                if chip.on_hover_text("Remove this person's access").clicked() {
                    remove = Some(id);
                }
            }
        });
        if let Some(id) = remove {
            d.users.retain(|u| *u != id);
        }
        ui.add(
            ui_kit::field(&mut d.search)
                .hint_text("Find a person to grant access")
                .desired_width(240.0)
                .id_salt(salt),
        );
        if !loaded {
            ui_kit::hint(ui, "Loading members...");
            return;
        }
        let found = pick_candidates(members, &d.users, &d.search);
        if found.is_empty() && !d.search.trim().is_empty() {
            ui_kit::hint(ui, "Nobody found (Owner and Admin already have access).");
        }
        let mut add = None;
        for m in found {
            ui.horizontal(|ui| {
                ui.label(role_name(
                    m.user.shown_name(),
                    m.user.role,
                    Some(theme::GIZ),
                ));
                if ui_kit::button_sized(ui, Kind::Secondary, Size::Small, Some(ph::PLUS), "Add")
                    .clicked()
                {
                    add = Some(m.user.id);
                }
            });
        }
        if let Some(id) = add {
            d.users.push(id);
            d.search.clear();
        }
    });
}

impl App {
    /// The role permissions are checked against: Member on a hub without roles, and while a
    /// needed personal password is not set.
    pub fn my_role(&self) -> Role {
        if !self.roles.supported || self.roles.account.is_some_and(|a| a.must_set_password) {
            return Role::Member;
        }
        self.me.as_ref().map(|u| u.role).unwrap_or_default()
    }

    /// Someone's current role, as far as we know.
    pub fn role_of(&self, id: UserId) -> Role {
        if let Some(me) = self.me.as_ref().filter(|m| m.id == id) {
            return me.role;
        }
        self.online
            .iter()
            .find(|u| u.id == id)
            .map(|u| u.role)
            .or_else(|| self.voice_members.get(&id).map(|m| m.user.role))
            .unwrap_or_default()
    }

    /// A moderator server-muted us: the hub says so, or our own voice session noticed.
    pub fn i_am_server_muted(&self) -> bool {
        let hub = self
            .my_id()
            .and_then(|id| self.voice_members.get(&id))
            .is_some_and(|m| m.server_muted);
        hub || (self.voice.channel.is_some() && self.roles.session_server_muted)
    }

    pub fn admin(&mut self, cmd: AdminCmd) {
        self.backend.send(Cmd::Admin(cmd));
    }

    /// The hub's snapshot: whether it knows roles, and my account.
    pub fn on_roles_ready(&mut self, account: Option<Account>) {
        self.roles.supported = account.is_some();
        self.roles.account = account;
        if !self.roles.supported || !self.my_role().can_admin() {
            self.roles.server_open = false;
        }
        tracing::info!(supported = self.roles.supported, ?account, role = ?self.me.as_ref().map(|u| u.role), "roles");
    }

    pub fn reset_roles(&mut self) {
        let later = self.roles.prompt_later;
        self.roles = RolesUi {
            prompt_later: later,
            ..Default::default()
        };
    }

    fn update_user(&mut self, user: &User) {
        if let Some(me) = self.me.as_mut().filter(|m| m.id == user.id) {
            *me = user.clone();
        }
        for u in self.online.iter_mut().filter(|u| u.id == user.id) {
            *u = user.clone();
        }
        if let Some(m) = self.voice_members.get_mut(&user.id) {
            m.user = user.clone();
        }
        for log in self.logs.values_mut() {
            for m in log.msgs.iter_mut().filter(|m| m.author.id == user.id) {
                m.author = user.clone();
            }
        }
        for m in self
            .roles
            .members
            .iter_mut()
            .filter(|m| m.user.id == user.id)
        {
            m.user = user.clone();
        }
        if user.role == Role::Owner {
            // The server has an owner now: nobody needs to claim it any more.
            if let Some(a) = self.roles.account.as_mut() {
                a.no_owner = false;
            }
            self.roles.claim_open = false;
        }
        if Some(user.id) == self.my_id() && user.role > Role::Member {
            // A role above Member needs a personal password; the powers stay off until it is set.
            if let Some(a) = self.roles.account.as_mut().filter(|a| !a.has_password) {
                a.must_set_password = true;
            }
        }
        if Some(user.id) == self.my_id() && !self.my_role().can_admin() {
            self.roles.server_open = false;
        }
    }

    /// The item-12 events from the hub.
    pub fn on_roles_event(&mut self, ev: ServerEvent) {
        match ev {
            ServerEvent::MessageDeleted {
                channel_id,
                message_id,
            } => {
                let gone = self.logs.get_mut(&channel_id).and_then(|log| {
                    log.msgs
                        .iter()
                        .position(|m| m.id == message_id)
                        .map(|i| log.msgs.remove(i))
                });
                tracing::info!(
                    channel = channel_id,
                    message = message_id,
                    had_it = gone.is_some(),
                    "message deleted"
                );
                // Its images, chips, unread count and a reply being written to it go; replies to
                // it now quote "message deleted" (v0.8).
                let atts = gone.map(|m| m.attachments).unwrap_or_default();
                self.forget_message(
                    crate::chat_images::Target::Channel(channel_id),
                    message_id,
                    &atts,
                );
            }
            ServerEvent::ChannelsChanged { mut channels } => {
                channels.sort_by_key(|c| (c.position, c.id));
                self.channels = channels;
                // A channel that went takes all its client state with it (v0.8.2).
                self.prune_channels();
                let chans = self.channels.clone();
                let alive = |id: ChannelId, kind: ChannelKind| {
                    chans.iter().any(|c| c.id == id && c.kind == kind)
                };
                if self.selected.is_some_and(|s| !alive(s, ChannelKind::Text)) {
                    self.selected = None;
                    if let Some(first) = chans
                        .iter()
                        .find(|c| c.kind == ChannelKind::Text)
                        .map(|c| c.id)
                    {
                        self.select_text(first);
                    }
                }
                if self
                    .voice
                    .channel
                    .is_some_and(|v| !alive(v, ChannelKind::Voice))
                {
                    self.leave_voice();
                    self.notice = Some(
                        "Your voice channel was deleted, or you no longer have access to it."
                            .into(),
                    );
                }
            }
            ServerEvent::UserUpdated { user } => {
                self.chatx.note_user(&user);
                self.update_user(&user);
            }
            ServerEvent::Moderated { action, by } => self.on_moderated(action, &by),
            _ => {}
        }
    }

    /// Forget every text channel the hub no longer lists (messages, counters, drafts, replies,
    /// scroll), so nothing of it can show up again under the same id.
    pub(crate) fn prune_channels(&mut self) {
        let alive: Vec<ChannelId> = self.channels.iter().map(|c| c.id).collect();
        let mut gone: Vec<ChannelId> = self
            .logs
            .keys()
            .chain(self.unread.keys())
            .chain(self.chatx.mention_unread.keys())
            .chain(self.chatx.unread_ids.keys())
            .copied()
            .chain(self.chatx.replying.keys().filter_map(|t| match t {
                crate::chat_images::Target::Channel(c) => Some(*c),
                crate::chat_images::Target::Dm(_) => None,
            }))
            .chain(self.selected)
            .filter(|id| !alive.contains(id))
            .collect();
        gone.sort_unstable();
        gone.dedup();
        for ch in gone {
            self.forget_channel(ch);
        }
    }

    fn on_moderated(&mut self, action: ModAction, by: &User) {
        tracing::info!(?action, by = %by.username, "moderated");
        self.roles.moderated.push(action);
        let who = by.shown_name();
        match action {
            ModAction::ServerMute => {
                self.notice = Some(format!("{who} server-muted you: others cannot hear your microphone until you are unmuted."));
            }
            ModAction::ServerUnmute => {
                self.notice = Some(format!("{who} unmuted your microphone."));
            }
            ModAction::Disconnect => {
                if self.voice.channel.is_some() || self.voice.joining.is_some() {
                    self.leave_voice();
                }
                self.voice.error = None;
                self.notice = Some(format!("{who} disconnected you from voice."));
            }
            ModAction::Move { channel_id } => {
                let name = self
                    .channels
                    .iter()
                    .find(|c| c.id == channel_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                if self.voice.channel.is_some() || self.voice.joining.is_some() {
                    // Keep whatever the center shows (chat or call).
                    let center: Center = self.center;
                    self.join_voice(channel_id);
                    self.center = center;
                }
                self.notice = Some(format!("{who} moved you to {name}."));
            }
            ModAction::StopStream => {
                if self.my_share_live() {
                    self.stop_share();
                }
                self.notice = Some(format!("{who} ended your stream."));
            }
        }
    }

    /// The answer to an [`AdminCmd`].
    pub fn on_admin_reply(&mut self, cmd: AdminCmd, result: Result<AdminReply, String>) {
        let err = match result {
            Ok(reply) => {
                self.on_admin_ok(cmd, reply);
                return;
            }
            Err(e) => e,
        };
        match cmd {
            AdminCmd::SetPassword(_) => {
                self.roles.pw_busy = false;
                self.roles.pw_msg = Some((false, err));
            }
            AdminCmd::ClaimOwner { .. } => {
                self.roles.claim_busy = false;
                self.roles.claim_msg = Some(err);
            }
            _ if self.roles.server_open => self.roles.server_error = Some(err),
            _ => self.notice = Some(err),
        }
    }

    fn on_admin_ok(&mut self, cmd: AdminCmd, reply: AdminReply) {
        self.roles.server_error = None;
        match (cmd, reply) {
            (AdminCmd::SetPassword(req), AdminReply::Password(resp)) => {
                self.roles.account = Some(resp.account);
                self.roles.pw_busy = false;
                self.roles.pw_current.clear();
                self.roles.pw_new.clear();
                self.roles.pw_confirm.clear();
                self.roles.pw_owner_code.clear();
                let text = if req.new.is_some() {
                    "Personal password saved."
                } else {
                    "Personal password removed: your name is no longer protected."
                };
                self.roles.pw_msg = Some((true, text.into()));
                if let Some(me) = self.me.clone() {
                    let server = self.session_url.clone();
                    self.settings.set_device_token(
                        &server,
                        &me.username,
                        resp.device_token.as_deref(),
                    );
                    // v1.0.1: the new token belongs to this server.
                    let id = self.session_server_id.clone();
                    self.settings
                        .set_device_token_server(&server, &me.username, id.as_deref());
                    // v0.9.1: the saved entry knows whether the name is protected.
                    self.settings
                        .set_server_personal(&server, &me.username, req.new.is_some());
                    self.save_settings();
                }
            }
            (_, AdminReply::Users(mut list)) => {
                list.sort_by(|a, b| {
                    b.user.role.cmp(&a.user.role).then_with(|| {
                        a.user
                            .username
                            .to_lowercase()
                            .cmp(&b.user.username.to_lowercase())
                    })
                });
                self.roles.members = list;
                self.roles.members_loaded = true;
            }
            (AdminCmd::ClaimOwner { .. }, AdminReply::Claimed(resp)) => {
                // The server is mine: the hub wants a personal password right away (the prompt).
                self.roles.account = Some(resp.account);
                if let Some(me) = self.me.as_mut() {
                    *me = resp.user.clone();
                }
                self.roles.claim_busy = false;
                self.roles.claim_open = false;
                self.roles.claim_code.clear();
                self.roles.claim_msg = None;
                self.roles.prompt_later = false;
                self.notice = Some("You are now this server's Owner. Set your personal password to enable permissions.".into());
            }
            (AdminCmd::TransferOwner { user }, _) => {
                self.roles.confirm_transfer = None;
                self.notice = Some(format!(
                    "Server ownership transferred to {}. You are now Admin.",
                    user.username
                ));
                self.admin(AdminCmd::ListUsers);
            }
            (AdminCmd::Audit { before }, AdminReply::Audit(entries)) => {
                self.roles.audit_more = entries.len() as u32 >= AUDIT_PAGE;
                if before.is_none() {
                    self.roles.audit = entries;
                } else {
                    self.roles.audit.extend(entries);
                }
                self.roles.audit_loaded = true;
            }
            (AdminCmd::ResetPassword { user }, AdminReply::ResetCode(code)) => {
                self.roles.reset_code = Some((user.username, code));
                self.admin(AdminCmd::ListUsers);
            }
            (AdminCmd::SetRole { .. } | AdminCmd::Mute { .. }, _) if self.roles.server_open => {
                self.admin(AdminCmd::ListUsers)
            }
            (AdminCmd::CreateChannel { .. }, _) => {
                self.roles.new_channel.clear();
                self.roles.new_access = AccessDraft::default();
            }
            (AdminCmd::SetChannelAccess { id, .. }, _)
                if self
                    .roles
                    .editing_access
                    .as_ref()
                    .is_some_and(|(e, _)| *e == id) =>
            {
                self.roles.editing_access = None
            }
            (AdminCmd::RenameChannel { .. }, _) => self.roles.renaming = None,
            _ => {}
        }
        if self.roles.server_open && self.roles.tab == ServerTab::Audit {
            self.admin(AdminCmd::Audit { before: None });
        }
    }

    // ------------------------------------------------------------ menus

    /// Would the person menu offer me a moderation action on this person (enabled)?
    pub fn moderation_available(&self, user: &User) -> bool {
        use crate::context_menu::{enabled_ids, Id, Lead, Origin};
        let rows = self.menu_rows(user, Origin::Sidebar, Lead::None);
        enabled_ids(&rows).iter().any(|i| {
            matches!(
                i,
                Id::ModServerMute
                    | Id::ModServerUnmute
                    | Id::ModDisconnect
                    | Id::ModMove
                    | Id::ModStopStream
                    | Id::ModSetRole
                    | Id::ModRemovePhoto
                    | Id::ModClearProfile
            )
        })
    }

    // ------------------------------------------------------------ personal password

    /// Set (or change) the personal password; `new = None` removes it (members only).
    pub fn set_personal_password(&mut self, current: Option<String>, new: Option<String>) {
        self.roles.pw_busy = true;
        self.roles.pw_msg = None;
        let code = self.roles.pw_owner_code.trim();
        let owner_code =
            (self.owner_first_password() && !code.is_empty()).then(|| code.to_string());
        self.admin(AdminCmd::SetPassword(SetPasswordRequest {
            current,
            new,
            remember_device: true,
            owner_code,
        }));
    }

    /// I am the owner and my name has no personal password yet: the first one needs proof of
    /// being the host (this PC is the server, or the code its window shows).
    fn owner_first_password(&self) -> bool {
        self.my_role() == Role::Owner && self.roles.account.is_some_and(|a| !a.has_password)
    }

    /// The "Server code" field, for the owner's first password from another PC.
    fn owner_code_field(&mut self, ui: &mut egui::Ui) {
        if !self.owner_first_password() {
            return;
        }
        ui_kit::hint(
            ui,
            "Owner: if you are not on the server PC, also enter the code shown in the server window \
             (protects ownership if someone signs in with your name before you).",
        );
        ui_kit::field_label(ui, "Server code");
        ui.add(
            ui_kit::field(&mut self.roles.pw_owner_code)
                .hint_text("XXXX-XXXX")
                .font(egui::TextStyle::Monospace)
                .desired_width(140.0),
        );
        ui.add_space(space::S);
    }

    /// Checks the form; `Some(error)` when it cannot be sent.
    fn pw_form_error(&self) -> Option<String> {
        let n = self.roles.pw_new.chars().count();
        if !(PERSONAL_PASSWORD_MIN..=PERSONAL_PASSWORD_MAX).contains(&n) {
            return Some(format!("Personal passwords must contain {PERSONAL_PASSWORD_MIN} to {PERSONAL_PASSWORD_MAX} characters."));
        }
        if self.roles.pw_new != self.roles.pw_confirm {
            return Some("The two passwords do not match.".into());
        }
        None
    }

    /// The fields; returns true when "Save password" was pressed with a valid form.
    fn pw_fields(&mut self, ui: &mut egui::Ui, has_password: bool) -> bool {
        let mut submit = false;
        let w = 260.0;
        if has_password {
            ui_kit::field_label(ui, "Current password");
            ui.add(
                ui_kit::field(&mut self.roles.pw_current)
                    .password(true)
                    .desired_width(w),
            );
        }
        ui_kit::field_label(
            ui,
            if has_password {
                "New password"
            } else {
                "Personal password"
            },
        );
        ui.add(
            ui_kit::field(&mut self.roles.pw_new)
                .password(true)
                .desired_width(w),
        );
        ui_kit::field_label(ui, "Repeat password");
        let r = ui.add(
            ui_kit::field(&mut self.roles.pw_confirm)
                .password(true)
                .desired_width(w),
        );
        submit |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(space::S);
        let label = if self.roles.pw_busy {
            "Saving..."
        } else {
            "Save password"
        };
        submit |= ui
            .add_enabled_ui(!self.roles.pw_busy, |ui| {
                ui_kit::button(ui, Kind::Primary, None, label)
            })
            .inner
            .clicked();
        if submit {
            if let Some(e) = self.pw_form_error() {
                self.roles.pw_msg = Some((false, e));
                return false;
            }
        }
        submit
    }

    fn pw_message(&self, ui: &mut egui::Ui) {
        if let Some((ok, msg)) = &self.roles.pw_msg {
            ui_kit::result_text(ui, *ok, msg);
        }
    }

    /// "Personal password" in Settings.
    pub fn personal_password_section(&mut self, ui: &mut egui::Ui) {
        if !self.roles.supported {
            return;
        }
        self.claim_owner_section(ui);
        ui.add_space(space::M);
        ui_kit::section(ui, "Personal password");
        let account = self.roles.account.unwrap_or_default();
        let role = self.me.as_ref().map(|u| u.role).unwrap_or_default();
        ui_kit::hint(
            ui,
            if account.has_password {
                "Your name is protected: signing in requires this password (this computer is remembered)."
            } else if role > Role::Member {
                "Your role requires a personal password. Set one to use role permissions."
            } else {
                "Optional: protects your name so nobody can sign in as you."
            },
        );
        ui.add_space(space::S);
        self.owner_code_field(ui);
        if self.pw_fields(ui, account.has_password) {
            let current = account.has_password.then(|| self.roles.pw_current.clone());
            let new = self.roles.pw_new.clone();
            self.set_personal_password(current, Some(new));
        }
        if account.has_password && role == Role::Member && !self.roles.pw_busy {
            ui.add_space(space::S);
            ui.horizontal(|ui| {
                ui_kit::hint(ui, "To remove it, enter your current password:");
                if ui_kit::button_sized(ui, Kind::Secondary, Size::Small, None, "Remove password")
                    .clicked()
                {
                    let current = Some(self.roles.pw_current.clone());
                    self.set_personal_password(current, None);
                }
            });
        }
        self.pw_message(ui);
    }

    /// Right after logging in: the owner's first login after the update (or a login with a
    /// reset code, or a promoted role without a password) must choose a personal password.
    pub fn password_prompt_window(&mut self, ctx: &egui::Context) {
        let must = self.roles.supported && self.roles.account.is_some_and(|a| a.must_set_password);
        if !must || self.roles.prompt_later || self.me.is_none() {
            return;
        }
        let role = self.me.as_ref().map(|u| u.role).unwrap_or_default();
        let mut later = false;
        egui::Window::new("Set your personal password")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(ui_kit::dialog_frame(&ctx.global_style()))
            .show(ctx, |ui| {
                ui.set_width(ui_kit::DIALOG_W);
                ui.spacing_mut().item_spacing.y = 6.0;
                ui_kit::dialog_title(ui, "Set your personal password");
                let why = if role > Role::Member {
                    format!(
                        "You are {} on this server. To use role permissions, protect your name with a personal password: \
                         nobody else can then sign in as you. This computer is remembered.",
                        role_label(role)
                    )
                } else {
                    "Your personal password was reset. Choose a new one to keep your name protected.".to_string()
                };
                ui_kit::body_text(ui, why);
                ui.add_space(space::S);
                self.owner_code_field(ui);
                if self.pw_fields(ui, false) {
                    let new = self.roles.pw_new.clone();
                    self.set_personal_password(None, Some(new));
                }
                self.pw_message(ui);
                ui.add_space(space::S);
                if ui_kit::button_sized(ui, Kind::Ghost, Size::Small, None, "Not now")
                    .on_hover_text("Role permissions stay disabled until you set a password (in Settings)")
                    .clicked()
                {
                    later = true;
                }
            });
        if later {
            self.roles.prompt_later = true;
        }
    }

    // ------------------------------------------------------------ claiming a server with no owner

    /// "Claim server": the claim code the server's console shows, for a server with no owner.
    pub fn claim_owner_window(&mut self, ctx: &egui::Context) {
        if !self.roles.claim_open {
            return;
        }
        if !self.roles.supported || !self.roles.account.is_some_and(|a| a.no_owner) {
            self.roles.claim_open = false;
            return;
        }
        let mut open = !ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut submit = false;
        egui::Window::new("Claim server")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(ui_kit::dialog_frame(&ctx.global_style()))
            .show(ctx, |ui| {
                ui.set_width(ui_kit::DIALOG_W + 20.0);
                ui.spacing_mut().item_spacing.y = 6.0;
                ui_kit::dialog_title(ui, "Claim server");
                ui_kit::body_text(
                    ui,
                    "This server has no owner yet. The host becomes owner automatically by signing in from the server PC itself \
                     (address 127.0.0.1). If you are elsewhere, enter the code shown in the server window:",
                );
                ui_kit::field_label(ui, "Server code");
                let r = ui.add(ui_kit::field(&mut self.roles.claim_code).hint_text("XXXX-XXXX").font(egui::TextStyle::Monospace).desired_width(160.0));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if let Some(msg) = &self.roles.claim_msg {
                    ui_kit::result_text(ui, false, msg);
                }
                ui_kit::hint(ui, "Next, set a personal password to protect your name and enable Owner permissions.");
                ui.add_space(space::L);
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 32.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can = !self.roles.claim_busy && !self.roles.claim_code.trim().is_empty();
                    let label = if self.roles.claim_busy { "Claiming..." } else { "Claim server" };
                    submit = ui.add_enabled_ui(can, |ui| ui_kit::button(ui, Kind::Primary, None, label)).inner.clicked() || (enter && can);
                    if ui_kit::button(ui, Kind::Secondary, None, "Cancel").clicked() {
                        open = false;
                    }
                });
            });
        if submit {
            self.roles.claim_busy = true;
            self.roles.claim_msg = None;
            let code = self.roles.claim_code.trim().to_string();
            self.admin(AdminCmd::ClaimOwner { code });
        }
        if !open {
            self.roles.claim_open = false;
        }
    }

    /// Settings: a server with no owner can be claimed.
    fn claim_owner_section(&mut self, ui: &mut egui::Ui) {
        if !self.roles.account.is_some_and(|a| a.no_owner) {
            return;
        }
        ui.add_space(space::M);
        ui_kit::section(ui, "Server without an owner");
        ui_kit::hint(ui, "The host becomes owner by signing in from the server PC, or using the code shown in the server window.");
        if ui_kit::button(ui, Kind::Secondary, Some(ph::CROWN), "Claim server...").clicked() {
            self.roles.claim_open = true;
            self.roles.claim_msg = None;
        }
    }

    // ------------------------------------------------------------ the server window

    pub fn open_server_settings(&mut self) {
        self.roles.server_open = true;
        self.roles.server_error = None;
        self.roles.members_loaded = false;
        self.roles.audit_loaded = false;
        self.admin(AdminCmd::ListUsers);
        self.admin(AdminCmd::Audit { before: None });
    }

    /// Any of the server window's confirmations is up.
    fn server_confirming(&self) -> bool {
        self.roles.confirm_delete.is_some()
            || self.roles.confirm_reset.is_some()
            || self.roles.confirm_transfer.is_some()
            || self.host_confirming()
    }

    /// "Server": the Discord-like settings of the server (Admin and Owner). A page list on the
    /// left ("Members and roles", "Channels", "Invites", "Moderation log", "Settings do
    /// servidor"), the open page on the right; its confirmations float over it.
    pub fn server_settings_window(&mut self, ctx: &egui::Context) {
        if !self.roles.server_open {
            return;
        }
        if !self.roles.supported || !self.my_role().can_admin() {
            self.roles.server_open = false;
            return;
        }
        let tab = self.roles.tab;
        let name = self.server_name.clone();
        let mut picked = None;
        let esc = !self.server_confirming()
            && !egui::Popup::is_any_open(ctx)
            && self.cmenu.open.is_none()
            && self.cmenu.confirm.is_none();
        let open = ui_kit::settings_window(
            ctx,
            "Server",
            Vec2::new(860.0, 580.0),
            esc,
            |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    ui.add_space(space::M);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(
                            RichText::new(format!("{}  Server", ph::WRENCH))
                                .font(theme::body(12.5))
                                .color(theme::NEVOA),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&name)
                                    .font(theme::display(size::TITLE))
                                    .color(theme::GIZ),
                            )
                            .truncate()
                            .selectable(false),
                        );
                    });
                });
                ui.add_space(space::L);
                let pages: Vec<(&str, &str)> = SERVER_PAGES
                    .iter()
                    .map(|(_, icon, label)| (*icon, *label))
                    .collect();
                let at = SERVER_PAGES
                    .iter()
                    .position(|(t, _, _)| *t == tab)
                    .unwrap_or(0);
                if let Some(i) = ui_kit::nav_list(ui, &pages, at) {
                    picked = Some(SERVER_PAGES[i].0);
                }
            },
            |ui| {
                if let Some(e) = self.roles.server_error.clone() {
                    ui_kit::notice_frame(ui_kit::Notice::Error).show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                if ui_kit::icon_button(
                                    ui,
                                    ph::X,
                                    "Dismiss notice",
                                    IconState::Normal,
                                )
                                .clicked()
                                {
                                    self.roles.server_error = None;
                                }
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Min),
                                    |ui| ui_kit::notice_text(ui, ui_kit::Notice::Error, &e),
                                );
                            });
                        });
                    });
                    ui.add_space(space::L);
                }
                match tab {
                    ServerTab::Members => self.members_tab(ui),
                    ServerTab::Channels => self.channels_tab(ui),
                    ServerTab::Invites => {
                        ui_kit::page_title(ui, "Invites", Some(crate::host::INVITE_HINT));
                        self.invite_tools(ui);
                    }
                    ServerTab::Audit => self.audit_tab(ui),
                    ServerTab::Settings => self.server_tab(ui),
                }
            },
        );
        if let Some(t) = picked.filter(|t| *t != tab) {
            self.roles.tab = t;
            if t == ServerTab::Audit {
                self.admin(AdminCmd::Audit { before: None });
            }
        }
        self.server_confirmations(ctx);
        if !open {
            self.roles.server_open = false;
            self.roles.reset_code = None;
            self.roles.confirm_delete = None;
            self.roles.confirm_reset = None;
            self.roles.confirm_transfer = None;
            self.roles.editing_access = None;
            self.roles.renaming = None;
        }
    }

    /// "Delete channel", "Reset personal password" and "Transfer server ownership": the kit's
    /// confirmation, over the server window.
    fn server_confirmations(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.roles.confirm_delete {
            let Some(c) = self.channels.iter().find(|c| c.id == id).cloned() else {
                self.roles.confirm_delete = None;
                return;
            };
            let (what, body) = if c.kind == ChannelKind::Text {
                (format!("Delete #{}?", c.name), "The channel disappears for everyone, including all its messages and images. This cannot be undone.")
            } else {
                (
                    format!("Delete room {}?", c.name),
                    "Everyone in it leaves voice. This cannot be undone.",
                )
            };
            match ui_kit::confirm(
                ctx,
                "server_delete_channel",
                "Delete channel",
                "Delete channel",
                Kind::Danger,
                true,
                |ui| {
                    ui_kit::body_text(ui, what);
                    ui_kit::hint(ui, body);
                },
            ) {
                Some(true) => {
                    self.admin(AdminCmd::DeleteChannel { id });
                    self.roles.confirm_delete = None;
                }
                Some(false) => self.roles.confirm_delete = None,
                None => {}
            }
        } else if let Some(u) = self.roles.confirm_reset.clone() {
            match ui_kit::confirm(
                ctx,
                "server_reset_password",
                "Reset personal password",
                "Reset password",
                Kind::Danger,
                true,
                |ui| {
                    ui_kit::body_text(ui, format!("Reset {}'s personal password?", u.shown_name()));
                    ui_kit::hint(ui, "Their current password is deleted and a single-use code lets them sign in and choose a new one. Recorded in the moderation log.");
                },
            ) {
                Some(true) => {
                    self.admin(AdminCmd::ResetPassword { user: u });
                    self.roles.confirm_reset = None;
                }
                Some(false) => self.roles.confirm_reset = None,
                None => {}
            }
        } else if let Some(u) = self.roles.confirm_transfer.clone() {
            match ui_kit::confirm(
                ctx,
                "server_transfer_owner",
                "Transfer server ownership",
                "Transfer ownership",
                Kind::Danger,
                true,
                |ui| {
                    ui_kit::body_text(ui, format!("Transfer the server to {}?", u.shown_name()));
                    ui_kit::hint(ui, "This person becomes the server owner and you become Admin. Only they can return ownership.");
                },
            ) {
                Some(true) => {
                    self.admin(AdminCmd::TransferOwner { user: u });
                    self.roles.confirm_transfer = None;
                }
                Some(false) => self.roles.confirm_transfer = None,
                None => {}
            }
        }
    }

    // ------------------------------------------------------------ "Members and roles"

    fn members_tab(&mut self, ui: &mut egui::Ui) {
        ui_kit::page_title(ui, "Members and roles", Some("People who have joined this server. Roles determine permissions; the moderation log records each change."));
        if let Some((name, code)) = self.roles.reset_code.clone() {
            ui_kit::notice_frame(ui_kit::Notice::Warning).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui_kit::body_text(ui, format!("{name}'s personal password was reset. Send them this code (valid until they choose a new password, for up to 3 days):"));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&code.code).monospace().strong().size(18.0).color(theme::GIZ));
                    if ui_kit::button_sized(ui, Kind::Secondary, Size::Small, Some(ph::COPY), "Copy").clicked() {
                        ui.ctx().copy_text(code.code.clone());
                    }
                    if ui_kit::button_sized(ui, Kind::Ghost, Size::Small, None, "OK").clicked() {
                        self.roles.reset_code = None;
                    }
                });
                ui_kit::hint(ui, "They sign in with their name and the code in \"Personal password\", then choose a new password.");
            });
            ui.add_space(space::L);
        }
        if !self.roles.members_loaded {
            ui_kit::hint(ui, "Loading...");
            return;
        }
        ui.add(
            ui_kit::field(&mut self.roles.members_search)
                .hint_text(format!("{}  Search by name", ph::MAGNIFYING_GLASS_PLUS))
                .desired_width(280.0),
        );
        ui.add_space(space::M);
        let q = self.roles.members_search.trim().to_lowercase();
        let members: Vec<MemberInfo> = self
            .roles
            .members
            .iter()
            .filter(|m| {
                q.is_empty()
                    || m.user.username.to_lowercase().contains(&q)
                    || m.user
                        .display_name
                        .as_deref()
                        .is_some_and(|d| d.to_lowercase().contains(&q))
            })
            .cloned()
            .collect();
        ui_kit::section(ui, &format!("Members — {}", members.len()));
        ui.add_space(space::S);
        if members.is_empty() {
            ui_kit::hint(ui, "Nobody found.");
            return;
        }
        ui_kit::group_frame()
            .inner_margin(egui::Margin::symmetric(6, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for m in &members {
                    self.server_member_row(ui, m);
                }
            });
    }

    /// One person of "Members and roles": picture, name (in the role's colour), badges, then the
    /// role (a menu when I may change it), "Unmute microphone" when muted, and the "⋯" menu
    /// (reset the personal password, transfer the server, and the person's usual menu).
    fn server_member_row(&mut self, ui: &mut egui::Ui, m: &MemberInfo) {
        let mine = self.my_role();
        let my_id = self.my_id();
        let theirs = m.user.role;
        let me = Some(m.user.id) == my_id;
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::hover());
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, radius::ROW, theme::ROW_HOVER);
        }
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui = &mut row;
        ui.spacing_mut().item_spacing.x = space::M;
        let user = self.known_user(m.user.id).unwrap_or_else(|| m.user.clone());
        crate::avatars::avatar_widget(
            &mut self.avatars.store,
            ui,
            &user,
            32.0,
            crate::avatars::Surface::Online,
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    role_name(user.shown_name(), theirs, Some(theme::GIZ))
                        .font(theme::semibold(13.5)),
                );
                role_badge(ui, theirs);
                if m.has_password {
                    ui.label(
                        RichText::new(ph::LOCK_SIMPLE)
                            .font(theme::body(13.0))
                            .color(theme::NEVOA),
                    )
                    .on_hover_text("Name protected by a personal password");
                }
                if me {
                    ui.label(
                        RichText::new("(you)")
                            .font(theme::body(12.0))
                            .color(theme::NEVOA),
                    );
                }
            });
            ui.label(
                RichText::new(format!("@{}", user.username))
                    .font(theme::body(12.0))
                    .color(theme::NEVOA),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            // "⋯": the admin's own items, then the person's usual menu (moderation included).
            let more = ui_kit::icon_button(ui, ph::DOTS_THREE, "More options", IconState::Normal);
            egui::Popup::menu(&more).frame(ui_kit::popup_frame(ui.style())).style(ui_kit::menu_style).show(|ui| {
                ui.set_min_width(250.0);
                let mut any = false;
                if mine.can_reset_passwords() && !me {
                    any = true;
                    if ui_kit::menu_item(ui, ph::LOCK_SIMPLE, "Reset personal password...", false, None) {
                        self.roles.confirm_reset = Some(m.user.clone());
                        ui.close();
                    }
                }
                if mine == Role::Owner && !me {
                    any = true;
                    let pinned = self.roles.account.is_some_and(|a| a.owner_pinned);
                    let why = if pinned {
                        Some("The owner is pinned in the server's config.toml (owner = ...): the host must edit the file and restart to change it.")
                    } else if !m.has_password {
                        Some("This person must set a personal password first.")
                    } else {
                        None
                    };
                    if ui_kit::menu_item(ui, ph::CROWN, "Transfer server ownership...", true, why) {
                        self.roles.confirm_transfer = Some(m.user.clone());
                        ui.close();
                    }
                }
                if any {
                    ui_kit::menu_separator(ui);
                }
                let pos = ui.min_rect().left_top();
                self.person_menu_ui(ui, &user, crate::context_menu::Origin::Online, crate::context_menu::Lead::None, pos);
            });
            if m.server_muted && mine.outranks(theirs) {
                if ui_kit::button_sized(ui, Kind::Secondary, Size::Small, Some(ph::MICROPHONE), "Unmute microphone").on_hover_text("Server-muted").clicked() {
                    self.admin(AdminCmd::Mute { user: m.user.id, muted: false });
                }
            } else if m.server_muted {
                ui_kit::pill(ui, &format!("{} silenciada", crate::app::ICON_MUTED), theme::PERIGO_FUNDO, theme::AO_VIVO);
            }
            let editable = !me && theirs != Role::Owner && (mine == Role::Owner || theirs < mine) && mine.can_admin();
            if editable {
                let mut pick = theirs;
                egui::ComboBox::from_id_salt(("role", m.user.id)).width(120.0).selected_text(role_label(theirs)).show_ui(ui, |ui| {
                    for r in mine.assignable() {
                        let enabled = r == Role::Member || m.has_password;
                        ui.add_enabled_ui(enabled, |ui| {
                            ui.selectable_value(&mut pick, r, role_label(r)).on_disabled_hover_text("This person must set a personal password first");
                        });
                    }
                });
                if pick != theirs {
                    self.admin(AdminCmd::SetRole { user: m.user.id, role: pick });
                }
            } else {
                ui_kit::pill(ui, role_label(theirs), theme::MOVEL, role_color(theirs).unwrap_or(theme::NEVOA));
            }
        });
    }

    // ------------------------------------------------------------ "Channels"

    fn channels_tab(&mut self, ui: &mut egui::Ui) {
        ui_kit::page_title(ui, "Channels", Some("This order matches the sidebar. Deleting a text channel also deletes its messages."));
        let mut chans = self.channels.clone();
        chans.sort_by_key(|c| (c.position, c.id));
        let ids: Vec<ChannelId> = chans.iter().map(|c| c.id).collect();
        let mut reorder: Option<Vec<ChannelId>> = None;
        for (kind, title) in [
            (ChannelKind::Text, "Text channels"),
            (ChannelKind::Voice, "Voice channels"),
        ] {
            let of_kind: Vec<&vox_proto::Channel> =
                chans.iter().filter(|c| c.kind == kind).collect();
            if of_kind.is_empty() {
                continue;
            }
            ui_kit::section(ui, title);
            ui.add_space(space::S);
            ui_kit::group_frame()
                .inner_margin(egui::Margin::symmetric(6, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, c) in of_kind.iter().enumerate() {
                        // Up / down swap with the neighbour of the same kind (what the sidebar shows).
                        let swap = |other: ChannelId| {
                            let mut v = ids.clone();
                            let (a, b) = (
                                v.iter().position(|x| *x == c.id),
                                v.iter().position(|x| *x == other),
                            );
                            if let (Some(a), Some(b)) = (a, b) {
                                v.swap(a, b);
                            }
                            v
                        };
                        let up = (i > 0).then(|| of_kind[i - 1].id);
                        let down = of_kind.get(i + 1).map(|n| n.id);
                        if let Some(v) = self.channel_row(ui, c, up.map(swap), down.map(swap)) {
                            reorder = Some(v);
                        }
                    }
                });
            ui.add_space(space::L);
        }
        if let Some(ids) = reorder {
            self.admin(AdminCmd::ReorderChannels { ids });
        }
        self.new_channel_group(ui);
    }

    /// One channel: its icon and name (or the rename field), then up, down, rename, "Acesso"
    /// (v0.9) and delete; the access editor opens right under it. Returns a new order.
    fn channel_row(
        &mut self,
        ui: &mut egui::Ui,
        c: &vox_proto::Channel,
        up: Option<Vec<ChannelId>>,
        down: Option<Vec<ChannelId>>,
    ) -> Option<Vec<ChannelId>> {
        let mut reorder = None;
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::hover());
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, radius::ROW, theme::ROW_HOVER);
        }
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        if self.channel_row_line(&mut row, c, up, down, &mut reorder) {
            return None;
        }
        // "Canal privado" of this channel (v0.9), right under its row.
        let mine = self
            .roles
            .editing_access
            .as_ref()
            .is_some_and(|(id, _)| *id == c.id);
        if let Some((id, mut draft)) = self.roles.editing_access.take_if(|_| mine) {
            let mut keep = true;
            egui::Frame::new()
                .fill(theme::NOITE)
                .corner_radius(radius::ROW)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .outer_margin(egui::Margin {
                    left: 28,
                    right: 4,
                    top: 2,
                    bottom: 6,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 6.0;
                    access_fields(
                        ui,
                        &mut draft,
                        &self.roles.members,
                        self.roles.members_loaded,
                        ("access", id),
                    );
                    ui.add_space(space::S);
                    // (One lamp button per view: a rename open at the same time keeps it.)
                    let kind = if self.roles.renaming.is_some() {
                        Kind::Secondary
                    } else {
                        Kind::Primary
                    };
                    ui.horizontal(|ui| {
                        if ui_kit::button_sized(ui, kind, Size::Small, None, "Save access")
                            .clicked()
                        {
                            self.admin(AdminCmd::SetChannelAccess {
                                id,
                                private: draft.to_access(),
                            });
                        }
                        if ui_kit::button_sized(ui, Kind::Ghost, Size::Small, None, "Cancel")
                            .clicked()
                        {
                            keep = false;
                        }
                    });
                });
            if keep && self.roles.editing_access.is_none() {
                self.roles.editing_access = Some((id, draft));
            }
        }
        reorder
    }

    /// The line of [`App::channel_row`]; true while it shows the rename field.
    fn channel_row_line(
        &mut self,
        ui: &mut egui::Ui,
        c: &vox_proto::Channel,
        up: Option<Vec<ChannelId>>,
        down: Option<Vec<ChannelId>>,
        reorder: &mut Option<Vec<ChannelId>>,
    ) -> bool {
        let private_ok = self.chatx.features.private_channels;
        ui.spacing_mut().item_spacing.x = 4.0;
        let icon = match (c.kind, c.private.is_some()) {
            (_, true) => crate::app::ICON_PRIVATE,
            (ChannelKind::Text, false) => ph::HASH,
            (ChannelKind::Voice, false) => ph::SPEAKER_HIGH,
        };
        ui.label(
            RichText::new(icon)
                .font(theme::body(size::ICON))
                .color(theme::NEVOA),
        );
        let renaming = matches!(&self.roles.renaming, Some((id, _)) if *id == c.id);
        if renaming {
            if let Some((id, name)) = self.roles.renaming.as_mut() {
                let r = ui.add(ui_kit::field(name).desired_width(200.0));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let (id, text) = (*id, name.trim().to_string());
                if ui_kit::button_sized(ui, Kind::Primary, Size::Small, None, "Save").clicked()
                    || enter
                {
                    self.admin(AdminCmd::RenameChannel { id, name: text });
                }
                if ui_kit::button_sized(ui, Kind::Ghost, Size::Small, None, "Cancel").clicked() {
                    self.roles.renaming = None;
                }
            }
            return true;
        }
        ui.add(
            egui::Label::new(RichText::new(&c.name).color(theme::GIZ))
                .truncate()
                .selectable(false),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let hover = if c.kind == ChannelKind::Text {
                "Delete channel (and all its messages)"
            } else {
                "Delete room (everyone in it leaves voice)"
            };
            if ui_kit::icon_button(ui, ph::TRASH, hover, IconState::Normal).clicked() {
                self.roles.confirm_delete = Some(c.id);
            }
            let editing = self
                .roles
                .editing_access
                .as_ref()
                .is_some_and(|(id, _)| *id == c.id);
            if private_ok {
                let state = if editing {
                    IconState::On
                } else {
                    IconState::Normal
                };
                if ui_kit::icon_button(
                    ui,
                    ph::LOCK_SIMPLE,
                    "Access: who can see and join this channel (private channel)",
                    state,
                )
                .clicked()
                {
                    self.roles.editing_access = if editing {
                        None
                    } else {
                        Some((c.id, AccessDraft::of(c.private.as_ref())))
                    };
                }
            }
            if ui_kit::icon_button(ui, ph::PENCIL_SIMPLE, "Rename", IconState::Normal).clicked() {
                self.roles.renaming = Some((c.id, c.name.clone()));
            }
            if ui
                .add_enabled_ui(down.is_some(), |ui| {
                    ui_kit::icon_button(ui, ph::CARET_DOWN, "Move down the list", IconState::Normal)
                })
                .inner
                .clicked()
            {
                *reorder = down.clone();
            }
            if ui
                .add_enabled_ui(up.is_some(), |ui| {
                    ui_kit::icon_button(ui, ph::CARET_UP, "Move up the list", IconState::Normal)
                })
                .inner
                .clicked()
            {
                *reorder = up.clone();
            }
            if c.private.is_some() {
                ui_kit::pill(ui, "privado", theme::MOVEL, theme::NEVOA);
            }
        });
        false
    }

    /// "New channel": text or voice, the name and (v0.9) private from the start.
    fn new_channel_group(&mut self, ui: &mut egui::Ui) {
        let private_ok = self.chatx.features.private_channels;
        ui_kit::section(ui, "New channel");
        ui.add_space(space::S);
        let mut create = false;
        ui_kit::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut self.roles.new_voice,
                    false,
                    format!("{}  Texto", ph::HASH),
                );
                ui.radio_value(
                    &mut self.roles.new_voice,
                    true,
                    format!("{}  Voice", ph::SPEAKER_HIGH),
                );
            });
            ui_kit::field_label(ui, "Channel name");
            let r = ui.add(
                ui_kit::field(&mut self.roles.new_channel)
                    .hint_text(if self.roles.new_voice {
                        "e.g. Room 3"
                    } else {
                        "ex.: jogatina"
                    })
                    .desired_width(280.0),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if private_ok {
                let mut draft = std::mem::take(&mut self.roles.new_access);
                access_fields(
                    ui,
                    &mut draft,
                    &self.roles.members,
                    self.roles.members_loaded,
                    ("access", -1),
                );
                self.roles.new_access = draft;
            }
            ui.add_space(space::S);
            let name = self.roles.new_channel.trim().to_string();
            // (One lamp button per view: while a channel's rename or access editor is open, its
            // "Save" is the main action.)
            let kind = if self.roles.renaming.is_some() || self.roles.editing_access.is_some() {
                Kind::Secondary
            } else {
                Kind::Primary
            };
            create = (ui
                .add_enabled_ui(!name.is_empty(), |ui| {
                    ui_kit::button(ui, kind, Some(ph::PLUS), "Create channel")
                })
                .inner
                .clicked()
                || enter)
                && !name.is_empty();
        });
        if create {
            let name = self.roles.new_channel.trim().to_string();
            let kind = if self.roles.new_voice {
                ChannelKind::Voice
            } else {
                ChannelKind::Text
            };
            let private = if private_ok {
                self.roles.new_access.to_access()
            } else {
                None
            };
            self.admin(AdminCmd::CreateChannel {
                name,
                kind,
                private,
            });
        }
    }

    // ------------------------------------------------------------ "Moderation log"

    fn audit_tab(&mut self, ui: &mut egui::Ui) {
        ui_kit::page_title(
            ui,
            "Moderation log",
            Some("Every moderation and role action, newest first."),
        );
        if !self.roles.audit_loaded {
            ui_kit::hint(ui, "Loading...");
            return;
        }
        if self.roles.audit.is_empty() {
            ui_kit::hint(ui, "No moderation actions yet.");
            return;
        }
        let now = crate::app::now_ms();
        let offset = crate::app::local_offset_min();
        ui_kit::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            for e in &self.roles.audit {
                ui.horizontal_top(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(92.0, 18.0), Sense::hover());
                    ui.painter().text(
                        r.left_center(),
                        egui::Align2::LEFT_CENTER,
                        crate::app::format_time(e.at_ms, now, offset),
                        theme::body(12.0),
                        theme::NEVOA,
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        match &e.actor {
                            Some(a) => ui.label(
                                role_name(&a.username, a.role, Some(theme::GIZ))
                                    .font(theme::semibold(13.5)),
                            ),
                            None => ui.label(
                                RichText::new("Host (command line)")
                                    .font(theme::semibold(13.5))
                                    .color(theme::GIZ),
                            ),
                        };
                        ui.label(RichText::new(audit_action_pt(&e.action)).color(theme::TEXTO));
                        if let Some(t) = &e.target {
                            ui.label(
                                role_name(&t.username, t.role, Some(theme::GIZ))
                                    .font(theme::semibold(13.5)),
                            );
                        }
                        let d = audit_detail_pt(&e.action, &e.detail);
                        if !d.is_empty() {
                            ui.label(RichText::new(d).color(theme::NEVOA));
                        }
                    });
                });
            }
        });
        if self.roles.audit_more {
            ui.add_space(space::M);
            if ui_kit::button(ui, Kind::Secondary, None, "Older").clicked() {
                let before = self.roles.audit.last().map(|e| e.id);
                self.admin(AdminCmd::Audit { before });
            }
        }
    }

    // ------------------------------------------------------------ "Server settings"

    fn server_tab(&mut self, ui: &mut egui::Ui) {
        ui_kit::page_title(ui, "Server settings", None);
        let mine = self.my_role();
        let account = self.roles.account.unwrap_or_default();
        let label_w = 150.0;
        ui_kit::section(ui, "About this server");
        ui.add_space(space::S);
        ui_kit::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui_kit::info_row(ui, label_w, "Name", |ui| {
                ui.label(RichText::new(&self.server_name).color(theme::GIZ))
            });
            let address = crate::host::url_host_port(&self.session_url)
                .map(|(h, p)| p.map_or(h.clone(), |p| format!("{h}:{p}")))
                .unwrap_or_default();
            ui_kit::info_row(ui, label_w, "Address", |ui| {
                ui.label(RichText::new(address).monospace().color(theme::GIZ))
            });
            ui_kit::info_row(ui, label_w, "Your role", |ui| {
                ui_kit::pill(
                    ui,
                    role_label(mine),
                    theme::MOVEL,
                    role_color(mine).unwrap_or(theme::GIZ),
                )
            });
            if self.roles.members_loaded {
                let n = self.roles.members.len();
                ui_kit::info_row(ui, label_w, "Members", |ui| {
                    ui.label(RichText::new(n.to_string()).color(theme::GIZ))
                });
            }
            ui_kit::info_row(ui, label_w, "Online now", |ui| {
                ui.label(RichText::new(self.online.len().to_string()).color(theme::GIZ))
            });
        });
        if self.host.runner.is_some() {
            ui.add_space(space::S);
            if ui_kit::button(
                ui,
                Kind::Secondary,
                Some(ph::HARD_DRIVES),
                "My server (ports, router, shutdown)",
            )
            .clicked()
            {
                self.host.panel_open = true;
            }
        }
        ui.add_space(space::L);
        ui_kit::section(ui, "Server ownership");
        ui.add_space(space::S);
        ui_kit::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            let owner = self.roles.members.iter().find(|m| m.user.role == Role::Owner).map(|m| m.user.shown_name().to_string());
            if account.no_owner {
                ui_kit::body_text(ui, "This server does not have an owner yet.");
                ui_kit::hint(ui, "The host becomes owner by signing in from the server PC, or using the code shown in the server window.");
                if ui_kit::button(ui, Kind::Secondary, Some(ph::CROWN), "Claim server...").clicked() {
                    self.roles.claim_open = true;
                    self.roles.claim_msg = None;
                }
            } else if let Some(o) = &owner {
                ui_kit::info_row(ui, label_w, "Owner", |ui| ui.label(role_name(o, Role::Owner, None).font(theme::semibold(13.5))));
            }
            if account.owner_pinned {
                ui_kit::hint(ui, "The owner is pinned in the server's config.toml (owner = ...): the host must edit that file and restart the server to change it.");
            } else if mine == Role::Owner {
                ui_kit::field_label(ui, "Transfer server ownership");
                ui_kit::hint(ui, "This person becomes owner and you become Admin. Only people with personal passwords are listed.");
                let my_id = self.my_id();
                let candidates: Vec<MemberInfo> = self.roles.members.iter().filter(|m| Some(m.user.id) != my_id && m.has_password).cloned().collect();
                ui.horizontal(|ui| {
                    let chosen = self.roles.transfer_pick.and_then(|id| candidates.iter().find(|m| m.user.id == id));
                    let text = chosen.map(|m| m.user.shown_name().to_string()).unwrap_or_else(|| "Choose a person".into());
                    egui::ComboBox::from_id_salt("transfer_owner_pick").width(220.0).selected_text(text).show_ui(ui, |ui| {
                        for m in &candidates {
                            ui.selectable_value(&mut self.roles.transfer_pick, Some(m.user.id), format!("{} ({})", m.user.shown_name(), role_label(m.user.role)));
                        }
                        if candidates.is_empty() {
                            ui.label(RichText::new("Nobody has a personal password yet.").color(theme::NEVOA));
                        }
                    });
                    let pick = self.roles.transfer_pick.and_then(|id| candidates.iter().find(|m| m.user.id == id)).map(|m| m.user.clone());
                    if ui.add_enabled_ui(pick.is_some(), |ui| ui_kit::button(ui, Kind::Danger, Some(ph::CROWN), "Transfer ownership...")).inner.clicked() {
                        self.roles.confirm_transfer = pick;
                    }
                });
            }
        });
        ui.add_space(space::L);
        ui_kit::section(ui, "Your personal password");
        ui.add_space(space::S);
        ui_kit::group_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui_kit::hint(
                ui,
                if account.has_password {
                    "Your name is protected by a personal password. Open Settings to change it."
                } else {
                    "Your role requires a personal password. Set one in Settings to use its permissions."
                },
            );
            ui.add_space(space::S);
            if ui_kit::button(ui, Kind::Secondary, Some(ph::GEAR_SIX), "Open settings").clicked() {
                self.open_settings();
            }
        });
    }
}

/// The pages of the server window, in order: (tab, icon, name).
const SERVER_PAGES: [(ServerTab, &str, &str); 5] = [
    (ServerTab::Members, ph::USERS, "Members and roles"),
    (ServerTab::Channels, ph::HASH, "Channels"),
    (ServerTab::Invites, ph::ENVELOPE_SIMPLE, "Invites"),
    (ServerTab::Audit, ph::CLIPBOARD_TEXT, "Moderation log"),
    (ServerTab::Settings, ph::GEAR_SIX, "Server settings"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_errors_read_in_english() {
        let e = |s: &str| anyhow::anyhow!("{s}");
        assert!(admin_error(&e(
            "set role failed: HTTP 409 Conflict: they must set a personal password first"
        ))
        .contains("personal password first"));
        assert_eq!(
            admin_error(&e(
                "server mute failed: HTTP 403 Forbidden: your role does not allow this"
            )),
            "Your role does not allow this."
        );
        assert!(
            admin_error(&e("disconnect failed: HTTP 409 Conflict: not in voice"))
                .contains("not in a voice channel")
        );
        assert!(admin_error(&e(
            "set password failed: HTTP 401 Unauthorized: wrong personal password"
        ))
        .contains("current password"));
        assert!(admin_error(&e(
            "delete channel failed: HTTP 409 Conflict: the last text channel cannot be deleted"
        ))
        .contains("last text channel"));
        assert!(admin_error(&e("members failed: HTTP 404 Not Found: ")).contains("older version"));
    }

    #[test]
    fn audit_lines_read_in_english() {
        assert_eq!(audit_action_pt(vox_proto::AUDIT_MOVE), "moved");
        assert_eq!(audit_action_pt("something_new"), "something_new");
        assert_eq!(
            audit_detail_pt(vox_proto::AUDIT_SET_ROLE, "member -> moderator"),
            "from Member to Moderator"
        );
        assert_eq!(
            audit_detail_pt(vox_proto::AUDIT_MOVE, "Room 1 -> Room 2"),
            "from Room 1 to Room 2"
        );
        assert_eq!(
            audit_action_pt(vox_proto::AUDIT_INVITE_CREATE),
            "created invite"
        );
        assert_eq!(
            audit_detail_pt(vox_proto::AUDIT_INVITE_CREATE, "#1 86400s 5x"),
            "#1 · valid for 24 hours · up to 5 people"
        );
        assert_eq!(
            audit_detail_pt(vox_proto::AUDIT_INVITE_CREATE, "#2 sem prazo ilimitado"),
            "#2 · no expiry · unlimited people"
        );
    }

    #[test]
    fn owner_messages_read_in_english() {
        let e = |s: &str| anyhow::anyhow!("{s}");
        assert!(admin_error(&e(
            "claim server failed: HTTP 403 Forbidden: wrong claim code"
        ))
        .starts_with("Incorrect code"));
        let proof = admin_error(&e(&format!(
            "set password failed: HTTP 403 Forbidden: {}",
            vox_proto::ERR_OWNER_PROOF_REQUIRED
        )));
        assert!(
            proof.contains("127.0.0.1") && proof.contains("Server code"),
            "{proof}"
        );
        assert!(admin_error(&e(
            "claim server failed: HTTP 409 Conflict: this server already has an owner"
        ))
        .contains("already has an owner"));
        assert!(admin_error(&e(
            "claim server failed: HTTP 429 Too Many Requests: too many attempts, wait 2 s"
        ))
        .contains("Too many attempts"));
        assert!(admin_error(&e("transfer ownership failed: HTTP 409 Conflict: the owner is set in config.toml: change it there")).contains("config.toml"));
        assert!(admin_error(&e(
            "transfer ownership failed: HTTP 409 Conflict: they must set a personal password first"
        ))
        .contains("personal password first"));
        assert_eq!(
            audit_action_pt(vox_proto::AUDIT_CLAIM_OWNER),
            "became the server Owner"
        );
        assert_eq!(
            audit_detail_pt(vox_proto::AUDIT_CLAIM_OWNER, "pc-do-servidor"),
            "by signing in from the server PC itself"
        );
        assert_eq!(
            audit_detail_pt(vox_proto::AUDIT_CLAIM_OWNER, "codigo"),
            "with the server code"
        );
        assert!(audit_action_pt(vox_proto::AUDIT_TRANSFER_OWNER)
            .contains("transferred server ownership"));
    }

    #[test]
    fn private_channel_fields_round_trip() {
        assert_eq!(AccessDraft::default().to_access(), None, "public");
        let p = PrivateAccess {
            roles: vec![Role::Member],
            users: vec![9, 4],
        };
        let d = AccessDraft::of(Some(&p));
        assert!(d.private && d.member && !d.moderator);
        assert_eq!(
            d.to_access(),
            Some(PrivateAccess {
                roles: vec![Role::Member],
                users: vec![4, 9]
            })
        );
        let only_staff = AccessDraft {
            private: true,
            ..Default::default()
        };
        assert_eq!(
            only_staff.to_access(),
            Some(PrivateAccess::default()),
            "private with nobody: Owner and Admin only"
        );
        assert_eq!(
            audit_action_pt(vox_proto::AUDIT_CHANNEL_ACCESS),
            "changed channel access"
        );
        let e = anyhow::anyhow!(
            "move failed: HTTP 403 Forbidden: they have no access to that private channel"
        );
        assert!(admin_error(&e).contains("private channel"));
    }

    #[test]
    fn the_people_picker_offers_members_and_moderators_only() {
        let info = |id, name: &str, role| MemberInfo {
            user: User {
                id,
                username: name.into(),
                role,
                avatar: None,
                display_name: None,
            },
            has_password: false,
            server_muted: false,
            created_at_ms: 0,
        };
        let ms = vec![
            info(1, "dono", Role::Owner),
            info(2, "adm", Role::Admin),
            info(3, "ana", Role::Member),
            info(4, "anabela", Role::Moderator),
            info(5, "bia", Role::Member),
        ];
        let names = |v: Vec<&MemberInfo>| {
            v.iter()
                .map(|m| m.user.username.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(pick_candidates(&ms, &[], "an")), ["ana", "anabela"]);
        assert_eq!(
            names(pick_candidates(&ms, &[3], "AN")),
            ["anabela"],
            "already chosen"
        );
        assert!(
            pick_candidates(&ms, &[], "d").is_empty(),
            "Owner and Admin always have access: not offered"
        );
        assert!(pick_candidates(&ms, &[], " ").is_empty());
    }

    #[test]
    fn staff_names_get_a_color_members_keep_theirs() {
        assert_eq!(role_color(Role::Member), None);
        assert!(
            role_color(Role::Owner).is_some()
                && role_color(Role::Moderator) != role_color(Role::Admin)
        );
        assert_eq!(role_label(Role::Moderator), "Moderator");
    }
}
