//! Co-op: friends play together in their browsers, each on their own device.
//!
//! "Invite a friend" makes this tab the host: it picks a short code, joins a
//! room named after it at the introducer (`tools/introducer`, a tiny server
//! that only swaps connection notes) and shares a link ending `?join=<code>`.
//! A friend who opens the link joins the same room as a guest. The browsers
//! then connect straight to each other (WebRTC) and play goes between them,
//! not through any server. Anyone in the room can share the same link.
//!
//! Who decides what is the simulation's business (`crabsim::net`): this
//! module only carries its messages. Each frame it hands the simulation what
//! arrived, and after the ticks it sends what the simulation wrote. Quick
//! updates (where crabs and enemies are) go on an unreliable channel, where a
//! late one is simply skipped; the rest (blows, hand-overs) on a reliable one.
//!
//! The host's crab is always [`HOST`] (as in solo play); a guest's gets a
//! random number. A guest finds the host's device by its crab's updates.
//!
//! Web only for now: a desktop build plays solo.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bevy::prelude::*;
use crabsim::PlayerId;

/// The introducer on Fly.io (`tools/introducer/fly.toml`). `?introducer=ws://localhost:3536` swaps it for one
/// running here (`cargo run --release` in `tools/introducer`).
const INTRODUCER: &str = "wss://crabscape-introducer.fly.dev";

/// The host's crab: the same as in solo play.
pub const HOST: PlayerId = PlayerId(0);

/// Letters in a room code: no 0/o, 1/l/i, so it can be read out loud.
const CODE_CHARS: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
const CODE_LEN: usize = 6;
/// The longest code a link may carry.
const CODE_MAX: usize = 16;
/// The longest build name in a room name (the introducer takes 80 in all).
const BUILD_MAX: usize = 40;

/// This device's crab.
#[derive(Resource, Clone, Copy, Debug)]
pub struct LocalId(pub PlayerId);

pub struct CoopPlugin;

impl Plugin for CoopPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(target_arch = "wasm32")]
        web::build(app);
        #[cfg(not(target_arch = "wasm32"))]
        app.insert_resource(LocalId(HOST));
    }
}

/// The value of `name` in a page's query string (`?a=1&b=2`), as written.
fn param(search: &str, name: &str) -> Option<String> {
    search
        .trim_start_matches('?')
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

/// A room code from a link: lowercase letters and digits only.
fn clean_code(raw: &str) -> Option<String> {
    let code: String = raw
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(CODE_MAX)
        .collect();
    (!code.is_empty()).then_some(code)
}

/// The introducer room for `code` in this build (`build` is a preview's
/// name; the live game has none). Each build meets only its own players, as
/// two builds may not play by the same rules.
fn room(build: Option<&str>, code: &str) -> String {
    let build: String = build.unwrap_or("live").chars().take(BUILD_MAX).collect();
    format!("crabscape-{build}-{code}")
}

/// A fresh room code, from `random` numbers in 0..1.
fn make_code(mut random: impl FnMut() -> f64) -> String {
    (0..CODE_LEN)
        .map(|_| {
            let i = (random() * CODE_CHARS.len() as f64) as usize;
            char::from(CODE_CHARS[i.min(CODE_CHARS.len() - 1)])
        })
        .collect()
}

/// "1 friend", "2 friends".
fn friends(n: usize) -> String {
    match n {
        1 => "1 friend".into(),
        n => format!("{n} friends"),
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use bevy::prelude::*;
    use crabsim::net::{Msg, Outgoing, To};
    use crabsim::{PlayerId, Sim};
    use js_sys::Reflect;
    use matchbox_socket::{Packet, PeerId, PeerState, WebRtcSocket};
    use wasm_bindgen::{JsCast, JsValue};

    use super::{HOST, INTRODUCER, LocalId, clean_code, friends, make_code, param, room};
    use crate::player::{GameSet, spawn_local_crab};
    use crate::{save, style};

    /// The reliable channel (blows, hand-overs) and the quick one (updates).
    const RELIABLE: usize = 0;
    const QUICK: usize = 1;
    /// Messages kept for a device we can't tell yet (it's just connecting).
    const WAITING_AT_MOST: usize = 256;
    /// How long the button says the link went (seconds).
    const FLASH_FOR: f32 = 2.5;
    /// The button's width, and its gap from the art-style button.
    const WIDTH: f32 = 124.0;
    const GAP: f32 = 8.0;

    /// Where this tab stands in co-op, for the button and its status line.
    #[derive(Resource, Default)]
    struct Coop {
        /// The room's code, while hosting or joining.
        code: Option<String>,
        /// What the line under the button says.
        status: String,
        /// Seconds the button keeps saying the link went out.
        flash: f32,
        /// A guest whose host left: the button offers solo play.
        ended: bool,
    }

    /// The connection to the other devices.
    struct Link {
        socket: WebRtcSocket,
        /// Each connected device, and its player once its first update came.
        peers: Vec<(PeerId, Option<PlayerId>)>,
        /// Messages for a player whose device we can't tell yet.
        waiting: Vec<(PlayerId, Packet, usize)>,
    }

    thread_local! {
        // The socket lives on the browser's one thread (it isn't `Send`).
        static LINK: RefCell<Option<Link>> = const { RefCell::new(None) };
    }

    impl Link {
        fn send(&mut self, peer: PeerId, bytes: Packet, channel: usize) {
            // A failed send means the connection is closing; `take_in` sees it.
            let _ = self.socket.channel_mut(channel).try_send(bytes, peer);
        }

        fn send_to(&mut self, id: PlayerId, bytes: Packet, channel: usize) {
            match self.peers.iter().find(|p| p.1 == Some(id)) {
                Some(&(peer, _)) => self.send(peer, bytes, channel),
                None => {
                    if self.waiting.len() >= WAITING_AT_MOST {
                        self.waiting.remove(0);
                    }
                    self.waiting.push((id, bytes, channel));
                }
            }
        }

        fn known(&self) -> impl Iterator<Item = PlayerId> + '_ {
            self.peers.iter().filter_map(|p| p.1)
        }
    }

    #[derive(Component)]
    struct InviteButton;

    #[derive(Component)]
    struct InviteLabel;

    #[derive(Component)]
    struct StatusLine;

    pub fn build(app: &mut App) {
        let code = query("join").and_then(|c| clean_code(&c));
        let me = if code.is_some() { guest_id() } else { HOST };
        app.insert_resource(LocalId(me))
            .insert_resource(Coop { code, ..default() })
            .add_systems(Startup, (join.after(spawn_local_crab), spawn_button))
            .add_systems(Update, (press, take_in).chain().in_set(GameSet::Input))
            .add_systems(
                Update,
                send_out.after(GameSet::Simulate).before(GameSet::Present),
            )
            .add_systems(Update, (show, place).in_set(GameSet::Present));
    }

    /// A guest's crab: any number but the host's.
    fn guest_id() -> PlayerId {
        PlayerId(((js_sys::Math::random() * f64::from(u32::MAX)) as u32).max(1))
    }

    /// `name` from this page's address, decoded.
    fn query(name: &str) -> Option<String> {
        let search = web_sys::window()?.location().search().ok()?;
        let raw = param(&search, name)?;
        js_sys::decode_uri_component(&raw).ok().map(String::from)
    }

    /// This page's address without its query.
    fn page() -> String {
        web_sys::window()
            .map(|w| w.location())
            .and_then(|l| Some(format!("{}{}", l.origin().ok()?, l.pathname().ok()?)))
            .unwrap_or_default()
    }

    /// The link a friend opens to join `code`.
    fn invite_link(code: &str) -> String {
        let mut link = format!("{}?join={code}", page());
        if let Some(i) = query("introducer") {
            link += &format!("&introducer={}", js_sys::encode_uri_component(&i));
        }
        link
    }

    /// Opened from an invite: join the room as a guest.
    fn join(mut sim: ResMut<Sim>, mut coop: ResMut<Coop>) {
        let Some(code) = coop.code.clone() else {
            return;
        };
        sim.go_online(false);
        connect(&code);
        coop.status = "Joining your friend…".into();
    }

    /// Join `code`'s room at the introducer.
    fn connect(code: &str) {
        let introducer = query("introducer")
            .filter(|u| u.starts_with("ws://") || u.starts_with("wss://"))
            .unwrap_or_else(|| INTRODUCER.into());
        let url = format!("{introducer}/{}", room(save::build_slug().as_deref(), code));
        let (socket, run) = WebRtcSocket::builder(url)
            .add_reliable_channel()
            .add_unreliable_channel()
            .build();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = run.await {
                warn!("co-op connection ended: {e}");
            }
        });
        LINK.with(|l| {
            *l.borrow_mut() = Some(Link {
                socket,
                peers: Vec::new(),
                waiting: Vec::new(),
            })
        });
    }

    /// The button's box: left of the art-style button, on the same row.
    fn button_rect(window: Vec2) -> Rect {
        let style = style::button_rect(window);
        let right = style.min.x - GAP;
        Rect::new(right - WIDTH, style.min.y, right, style.max.y)
    }

    fn spawn_button(mut commands: Commands, window: Single<&Window>) {
        let r = button_rect(window.size());
        let right = px(window.size().x - r.max.x);
        commands
            .spawn((
                InviteButton,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(r.min.y),
                    right,
                    width: px(r.width()),
                    height: px(r.height()),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(r.height() / 2.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.25)),
                BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
            ))
            .with_children(|b| {
                b.spawn((
                    InviteLabel,
                    Text::new("Invite a friend"),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 0.9, 0.7, 0.7)),
                ));
            });
        commands.spawn((
            StatusLine,
            Node {
                position_type: PositionType::Absolute,
                top: px(r.max.y + 4.0),
                right,
                ..default()
            },
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(12.0),
                ..default()
            },
            TextColor(Color::srgba(1.0, 0.9, 0.7, 0.6)),
        ));
    }

    /// A click or tap on the button: start hosting (the first time) and send
    /// the link; for a guest whose host left, go back to solo play.
    fn press(
        time: Res<Time>,
        mouse: Res<ButtonInput<MouseButton>>,
        touches: Res<Touches>,
        window: Single<&Window>,
        me: Res<LocalId>,
        mut sim: ResMut<Sim>,
        mut coop: ResMut<Coop>,
    ) {
        coop.flash = (coop.flash - time.delta_secs()).max(0.0);
        let rect = button_rect(window.size()).inflate(6.0);
        let clicked = mouse.just_pressed(MouseButton::Left)
            && window.cursor_position().is_some_and(|p| rect.contains(p));
        let tapped = touches
            .iter_just_pressed()
            .any(|t| rect.contains(t.position()));
        if !(clicked || tapped) {
            return;
        }
        if coop.ended {
            if let Some(w) = web_sys::window() {
                let _ = w.location().set_href(&page());
            }
            return;
        }
        if coop.code.is_none() && me.0 == HOST {
            let code = make_code(js_sys::Math::random);
            sim.go_online(true);
            connect(&code);
            coop.code = Some(code);
        }
        if let Some(code) = &coop.code {
            share(&invite_link(code));
            coop.flash = FLASH_FOR;
        }
    }

    /// Hand the link on: the share sheet on a phone, the clipboard on a
    /// computer, or a box to copy it from.
    fn share(link: &str) {
        info!("invite link: {link}");
        let Some(win) = web_sys::window() else {
            return;
        };
        let nav = win.navigator();
        let touch = nav.max_touch_points() > 0;
        let nav: JsValue = nav.into();
        let call = |on: &JsValue, name: &str, arg: &JsValue| {
            Reflect::get(on, &name.into())
                .ok()
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
                .is_some_and(|f| f.call1(on, arg).is_ok())
        };
        if touch {
            let data = js_sys::Object::new();
            let _ = Reflect::set(&data, &"url".into(), &link.into());
            let _ = Reflect::set(&data, &"title".into(), &"Play CrabScape with me".into());
            if call(&nav, "share", &data.into()) {
                return;
            }
        }
        if let Ok(clipboard) = Reflect::get(&nav, &"clipboard".into())
            && clipboard.is_object()
            && call(&clipboard, "writeText", &link.into())
        {
            return;
        }
        let _ = win.prompt_with_message_and_default("Send this link to a friend:", link);
    }

    /// What arrived since the last frame: devices that came or went, and
    /// their messages, which go to the simulation.
    fn take_in(mut sim: ResMut<Sim>, mut coop: ResMut<Coop>, me: Res<LocalId>) {
        let ended = LINK.with(|cell| {
            let mut cell = cell.borrow_mut();
            let link = cell.as_mut()?;
            let Ok(changes) = link.socket.try_update_peers() else {
                return Some("Lost the connection: playing alone");
            };
            for (peer, state) in changes {
                match state {
                    PeerState::Connected => {
                        link.peers.push((peer, None));
                        if sim.is_host() {
                            link.send(peer, sim.world().to_bytes().into(), RELIABLE);
                        }
                    }
                    PeerState::Disconnected => {
                        let Some(i) = link.peers.iter().position(|p| p.0 == peer) else {
                            continue;
                        };
                        match link.peers.remove(i).1 {
                            Some(HOST) => return Some("Your friend's game ended"),
                            Some(id) => sim.remove_crab(id),
                            None => {}
                        }
                    }
                }
            }
            for channel in [RELIABLE, QUICK] {
                for (peer, bytes) in link.socket.channel_mut(channel).receive() {
                    let Some(msg) = Msg::from_bytes(&bytes) else {
                        continue;
                    };
                    if let Msg::Crab { crab, .. } = &msg
                        && let Some(p) = link.peers.iter_mut().find(|p| p.0 == peer)
                        && p.1.is_none()
                        && crab.id != me.0
                    {
                        p.1 = Some(crab.id);
                    }
                    sim.receive(msg);
                }
            }
            let n = link.known().count();
            coop.status = if sim.is_host() {
                match n {
                    0 => "Waiting for a friend…".into(),
                    n => format!("Playing with {}", friends(n)),
                }
            } else if link.known().any(|id| id == HOST) {
                format!("Playing with {}", friends(n))
            } else {
                "Joining your friend…".into()
            };
            None
        });
        if let Some(why) = ended {
            LINK.with(|cell| {
                if let Some(mut link) = cell.borrow_mut().take() {
                    link.socket.close();
                }
            });
            sim.go_offline();
            // A guest's crab can't become a host's: it goes back to solo play
            // with the button. A host can simply invite again.
            coop.ended = me.0 != HOST;
            coop.code = None;
            coop.status = why.into();
        }
    }

    /// Send what the simulation wrote this frame.
    fn send_out(mut sim: ResMut<Sim>) {
        let out = sim.take_outbox();
        LINK.with(|cell| {
            let mut cell = cell.borrow_mut();
            let Some(link) = cell.as_mut() else {
                return;
            };
            for (id, bytes, channel) in std::mem::take(&mut link.waiting) {
                link.send_to(id, bytes, channel);
            }
            for Outgoing { to, msg } in out {
                let channel = if msg.reliable() { RELIABLE } else { QUICK };
                let bytes: Packet = msg.to_bytes().into();
                match to {
                    To::Everyone => {
                        let peers: Vec<PeerId> = link.peers.iter().map(|p| p.0).collect();
                        for peer in peers {
                            link.send(peer, bytes.clone(), channel);
                        }
                    }
                    To::Host => link.send_to(HOST, bytes, channel),
                    To::Player(id) => link.send_to(id, bytes, channel),
                }
            }
        });
    }

    /// The button's words, and the status line.
    fn show(
        coop: Res<Coop>,
        mut label: Single<&mut Text, (With<InviteLabel>, Without<StatusLine>)>,
        mut line: Single<&mut Text, (With<StatusLine>, Without<InviteLabel>)>,
    ) {
        let words = if coop.ended {
            "Play solo"
        } else if coop.flash > 0.0 {
            "Link ready!"
        } else {
            "Invite a friend"
        };
        if label.0 != words {
            label.0 = words.into();
        }
        if line.0 != coop.status {
            line.0.clone_from(&coop.status);
        }
    }

    /// Keep the button and its line in place as the window changes.
    fn place(
        window: Single<&Window>,
        mut button: Single<&mut Node, (With<InviteButton>, Without<StatusLine>)>,
        mut line: Single<&mut Node, (With<StatusLine>, Without<InviteButton>)>,
    ) {
        let r = button_rect(window.size());
        let (top, below) = (px(r.min.y), px(r.max.y + 4.0));
        if button.top != top {
            button.top = top;
        }
        if line.top != below {
            line.top = below;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_names_its_room_and_nothing_else_gets_in() {
        let search = "?introducer=ws%3A%2F%2Flocalhost%3A3536&join=Ab3-x9";
        assert_eq!(param(search, "join").as_deref(), Some("Ab3-x9"));
        assert_eq!(param(search, "nope"), None);
        assert_eq!(clean_code("Ab3-x9").as_deref(), Some("ab3x9"));
        assert_eq!(clean_code("../../"), None);
        assert_eq!(clean_code(&"z".repeat(99)).map(|c| c.len()), Some(CODE_MAX));
    }

    #[test]
    fn each_build_meets_only_its_own_players_and_rooms_fit_the_introducer() {
        assert_eq!(room(None, "abc234"), "crabscape-live-abc234");
        assert_eq!(room(Some("pr-12"), "abc234"), "crabscape-pr-12-abc234");
        let long = room(Some(&"b".repeat(200)), &"c".repeat(CODE_MAX));
        assert!(long.len() <= 80, "{}", long.len());
    }

    #[test]
    fn codes_are_short_and_easy_to_read_out() {
        let mut x = 0.0;
        let code = make_code(|| {
            x += 0.37;
            x % 1.0
        });
        assert_eq!(code.len(), CODE_LEN);
        assert!(code.bytes().all(|b| CODE_CHARS.contains(&b)));
        assert_eq!(make_code(|| 0.999_999_9).len(), CODE_LEN);
        assert_eq!(clean_code(&code), Some(code));
    }
}
