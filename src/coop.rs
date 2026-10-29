//! Co-op: friends play together in their browsers, each on their own device.
//!
//! Each device has one lobby, kept for good: the first press of "Invite a
//! friend" picks a short code, keeps it with the save (Start over keeps it)
//! and shares a link ending `?join=<code>`, the same link every day. From
//! then on the lobby opens whenever this device plays: it joins a room named
//! after the code at the introducer (`tools/introducer`, a tiny server that
//! only swaps connection notes) as its host. Opening your own link hosts it
//! too. A friend who opens the link joins the same room as a guest. The
//! browsers then connect straight to each other (WebRTC) and play goes
//! between them, not through any server. Anyone in the room can share the
//! same link. Settings' "New link" swaps the code, so the old link reaches
//! nobody.
//!
//! A friend in the lobby while its owner is away plays alone in their own
//! world ("Waiting for your friend..."), and takes the host's world the moment
//! the host's game is there. Friends waiting together don't share a world
//! nobody runs.
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
//! A device that loses everyone it played with (a phone slept, the network
//! dropped) joins the same room afresh every few seconds until they're back;
//! a guest plays on alone in the meantime, then takes the host's word for the
//! world again. The promises, and the checks that hold us to them:
//! `docs/design/coop-promises.md`.
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

/// The invite button's width, and its gap from the settings button.
const INVITE_WIDTH: f32 = 124.0;
const GAP: f32 = 8.0;

/// The invite button's box: left of the settings button, on the same row.
pub(crate) fn invite_rect(window: Vec2) -> Rect {
    let settings = crate::settings::button_rect(window);
    let right = settings.min.x - GAP;
    Rect::new(right - INVITE_WIDTH, settings.min.y, right, settings.max.y)
}

/// Where the co-op status line starts: under the invite button, or on a
/// narrow screen under the row below it (where the mic button sits, clear of
/// the health bars). The settings panel opens below it.
pub(crate) fn status_top(window: Vec2) -> f32 {
    let rows = if crate::settings::narrow(window) {
        1.0
    } else {
        0.0
    };
    crate::settings::below(window, rows).max.y + 4.0
}

/// This device's crab.
#[derive(Resource, Clone, Copy, Debug)]
pub struct LocalId(pub PlayerId);

/// Settings' "New link": throw this device's lobby code away and make a new
/// one, so the old link no longer reaches it.
#[derive(Message, Clone, Copy, Debug)]
pub struct NewLink;

pub struct CoopPlugin;

impl Plugin for CoopPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<NewLink>();
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

/// What this device does in co-op when the game starts.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Start {
    /// Plays alone (no lobby of its own yet, or it's open in another tab).
    Solo,
    /// Hosts its own lobby.
    Host(String),
    /// Joins a friend's lobby (and plays alone until the friend is there).
    Guest(String),
}

/// Who this device is when the game starts. `invited`: the code in the
/// address (`?join=`); `mine`: this device's lobby code, made the first time
/// "Invite a friend" was pressed and kept for good; `other_tab`: another tab
/// on this device has that lobby open right now. Your own lobby is always
/// yours: opening your own link hosts it, and once you have shared it it
/// opens whenever you play. One tab hosts it, so two never claim it.
fn start(invited: Option<&str>, mine: Option<&str>, other_tab: bool) -> Start {
    match (invited, mine) {
        (Some(code), Some(own)) if code == own => {
            if other_tab {
                Start::Solo
            } else {
                Start::Host(own.to_string())
            }
        }
        (Some(code), _) => Start::Guest(code.to_string()),
        (None, Some(own)) if !other_tab => Start::Host(own.to_string()),
        (None, _) => Start::Solo,
    }
}

/// How long a tab's "my lobby is open here" mark stays fresh (ms): the
/// hosting tab renews it every second, so another tab can tell.
const TAB_MARK_FRESH: f64 = 5000.0;

/// Is the lobby open in another tab? `mark` is the stored "<tab> <time>"
/// (milliseconds since 1970), `me` this tab's id, `now` the time.
fn open_in_other_tab(mark: Option<&str>, me: &str, now: f64) -> bool {
    let Some((tab, at)) = mark.and_then(|m| m.split_once(' ')) else {
        return false;
    };
    let Ok(at) = at.parse::<f64>() else {
        return false;
    };
    tab != me && (0.0..TAB_MARK_FRESH).contains(&(now - at))
}

/// How long a device that lost the connection to the introducer waits
/// before joining the room afresh (seconds).
const REJOIN_AFTER: f32 = 5.0;
/// How long a device that lost the friends it played with, but still has
/// a connection, waits before joining afresh, and again after that (seconds).
/// It must be longer than a connection takes to set up (at most 3 s each way
/// for the browser's addresses, `vendor/matchbox_socket`, and a moment to
/// connect), or a friend coming back is cut off again and again.
const ALONE_FOR: f32 = 15.0;

/// Keeps track of being with friends, frame by frame, and says when to
/// join the room afresh.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Watch {
    /// Played together already (a guest: with the host; the host: with
    /// anyone), so being alone now means they were lost.
    met: bool,
    /// Seconds alone, or with a dead connection, since the last (re)join.
    lonely: f32,
}

impl Watch {
    /// One frame of `dt` seconds: `together` (with the host, or as the host
    /// with anyone), `alive` (the connection to the introducer works).
    /// True: join the room afresh now.
    fn step(&mut self, together: bool, alive: bool, dt: f32) -> bool {
        self.met |= together;
        if together && alive {
            self.lonely = 0.0;
            return false;
        }
        if !(self.met || !alive) {
            return false;
        }
        self.lonely += dt;
        let wait = if alive { ALONE_FOR } else { REJOIN_AFTER };
        if self.lonely >= wait {
            self.lonely = 0.0;
            return true;
        }
        false
    }
}

/// The line under the invite button (and `data-coop` on the canvas).
fn status(host: bool, together: bool, n: usize) -> String {
    match (host, together) {
        (_, true) => format!("Playing with {}", friends(n)),
        (true, false) => "Lobby open: waiting for a friend...".into(),
        (false, false) => "Waiting for your friend...".into(),
    }
}

/// Is this word from the host? Its crab's updates, or the world it sends a
/// device that just connected (often before its crab). A guest playing alone
/// takes the host's world on the first of these; it would drop the world if
/// it waited for the crab.
fn from_host(msg: &crabsim::net::Msg) -> bool {
    use crabsim::net::Msg;
    match msg {
        Msg::World { .. } => true,
        Msg::Crab { crab, .. } => crab.id == HOST,
        _ => false,
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use web::{in_room, introducer, players};

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use bevy::prelude::*;
    use crabsim::net::{Msg, Outgoing, To};
    use crabsim::{PlayerId, Sim};
    use js_sys::Reflect;
    use matchbox_socket::{Packet, PeerId, PeerState, WebRtcSocket};
    use wasm_bindgen::{JsCast, JsValue};

    use super::{
        HOST, INTRODUCER, LocalId, NewLink, Start, Watch, clean_code, from_host, make_code,
        open_in_other_tab, param, room, start, status, status_top,
    };
    use crate::player::{GameSet, spawn_local_crab};
    use crate::save;

    /// The reliable channel (blows, hand-overs), the quick one (updates),
    /// and voice chat's notes (`voice.rs`).
    const RELIABLE: usize = 0;
    const QUICK: usize = 1;
    const VOICE: usize = 2;
    /// Messages kept for a device we can't tell yet (it's just connecting).
    const WAITING_AT_MOST: usize = 256;
    /// How long the button says the link went (seconds).
    const FLASH_FOR: f32 = 2.5;
    /// A device that has sent nothing for this long is gone (seconds). Every
    /// device sends its crab many times a second, and a dropped connection
    /// isn't always reported, so silence is the sure sign.
    const SILENT_FOR: f32 = 5.0;
    /// How often the hosting tab renews its "lobby open here" mark (seconds).
    const MARK_EVERY: f32 = 1.0;

    /// Where this tab stands in co-op, for the button and its status line.
    #[derive(Resource, Default)]
    struct Coop {
        /// The room's code, while hosting or joining.
        code: Option<String>,
        /// What the line under the button says.
        status: String,
        /// Seconds the button keeps saying the link went out.
        flash: f32,
        /// Being with friends, and when to join the room afresh.
        watch: Watch,
        /// Seconds until the "lobby open here" mark is renewed.
        mark: f32,
    }

    /// A connected device.
    struct Peer {
        id: PeerId,
        /// Its player, once its first crab update came.
        player: Option<PlayerId>,
        /// Seconds since it last sent anything.
        quiet: f32,
    }

    /// The connection to the other devices.
    struct Link {
        socket: WebRtcSocket,
        /// This device's connection id, once the introducer gave it.
        me: Option<PeerId>,
        peers: Vec<Peer>,
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
            match self.peers.iter().find(|p| p.player == Some(id)) {
                Some(p) => {
                    let peer = p.id;
                    self.send(peer, bytes, channel);
                }
                None => {
                    if self.waiting.len() >= WAITING_AT_MOST {
                        self.waiting.remove(0);
                    }
                    self.waiting.push((id, bytes, channel));
                }
            }
        }

        fn known(&self) -> impl Iterator<Item = PlayerId> + '_ {
            self.peers.iter().filter_map(|p| p.player)
        }
    }

    /// In a co-op room (hosting or joined)?
    pub(crate) fn in_room() -> bool {
        LINK.with(|l| l.borrow().is_some())
    }

    /// Each connected device and its player, once known (for voice volumes).
    pub(crate) fn players() -> Vec<(PeerId, PlayerId)> {
        LINK.with(|l| {
            l.borrow().as_ref().map_or(Vec::new(), |link| {
                link.peers
                    .iter()
                    .filter_map(|p| Some((p.id, p.player?)))
                    .collect()
            })
        })
    }

    #[derive(Component)]
    struct InviteButton;

    #[derive(Component)]
    struct InviteLabel;

    #[derive(Component)]
    struct StatusLine;

    pub fn build(app: &mut App) {
        let invited = query("join").and_then(|c| clean_code(&c));
        let mine = lobby();
        let other_tab = open_in_other_tab(
            save::read_key(MARK_KEY).as_deref(),
            &tab_id(),
            js_sys::Date::now(),
        );
        let (me, code) = match start(invited.as_deref(), mine.as_deref(), other_tab) {
            Start::Solo => (HOST, None),
            Start::Host(code) => (HOST, Some(code)),
            Start::Guest(code) => (guest_id(), Some(code)),
        };
        app.insert_resource(LocalId(me))
            .insert_resource(Coop { code, ..default() })
            .add_systems(Startup, (join.after(spawn_local_crab), spawn_button))
            .add_systems(
                Update,
                (press, new_link, take_in).chain().in_set(GameSet::Input),
            )
            .add_systems(
                Update,
                send_out.after(GameSet::Simulate).before(GameSet::Present),
            )
            .add_systems(Update, (show, place, mark).in_set(GameSet::Present));
    }

    /// Where this device keeps its lobby code (per build, with the save; Start
    /// over keeps it, `save.rs`).
    pub(crate) const LOBBY_KEY: &str = "-lobby";
    /// The hosting tab's "lobby open here" mark: "<tab> <time>".
    const MARK_KEY: &str = "-lobby-tab";

    /// This device's lobby code, if it has shared one. (A tab that hosted
    /// before this kept its code only in the tab: that becomes the lobby, so
    /// links sent then still work.)
    fn lobby() -> Option<String> {
        if let Some(code) = save::read_key(LOBBY_KEY).and_then(|c| clean_code(&c)) {
            return Some(code);
        }
        let old = tab_storage()?.get_item(&old_hosting_key()).ok()??;
        let code = clean_code(&old)?;
        save::write_key(LOBBY_KEY, &code);
        Some(code)
    }

    /// Where a hosting tab kept its room code before lobbies were kept for
    /// good.
    fn old_hosting_key() -> String {
        let build = save::build_slug();
        format!("crabscape-hosting-{}", build.as_deref().unwrap_or("live"))
    }

    fn tab_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.session_storage().ok()?
    }

    /// This tab's id: random, kept over a reload (the tab's own storage), so
    /// a reloaded host still owns its "lobby open here" mark.
    fn tab_id() -> String {
        let key = save::browser_key("-tab");
        let storage = tab_storage();
        if let Some(id) = storage
            .as_ref()
            .and_then(|s| s.get_item(&key).ok().flatten())
        {
            return id;
        }
        let id = make_code(js_sys::Math::random);
        if let Some(s) = storage {
            let _ = s.set_item(&key, &id);
        }
        id
    }

    /// While this tab hosts, renew its "lobby open here" mark, so another tab
    /// on this device opened meanwhile plays alone instead of hosting too.
    fn mark(time: Res<Time>, me: Res<LocalId>, mut coop: ResMut<Coop>) {
        if me.0 != HOST || coop.code.is_none() {
            return;
        }
        coop.mark -= time.delta_secs();
        if coop.mark <= 0.0 {
            coop.mark = MARK_EVERY;
            save::write_key(MARK_KEY, &format!("{} {}", tab_id(), js_sys::Date::now()));
        }
    }

    /// Settings' "New link": a fresh code for this device's lobby. Hosting
    /// it now? Move to the new room, so the old link reaches nobody.
    fn new_link(
        mut asked: MessageReader<NewLink>,
        me: Res<LocalId>,
        mut sim: ResMut<Sim>,
        mut coop: ResMut<Coop>,
    ) {
        if asked.read().count() == 0 {
            return;
        }
        let code = make_code(js_sys::Math::random);
        save::write_key(LOBBY_KEY, &code);
        info!("co-op: a new lobby link");
        if me.0 == HOST && coop.code.is_some() {
            coop.code = Some(code.clone());
            coop.watch = Watch::default();
            rejoin(&mut sim, &code, true);
        }
        coop.flash = 0.0;
        show_lobby(Some(&code));
    }

    /// The canvas's `data-lobby`: this device's lobby code (for the browser
    /// tests, which can't read the game's own text).
    fn show_lobby(code: Option<&str>) {
        if let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("crabscape"))
        {
            let _ = canvas.set_attribute("data-lobby", code.unwrap_or(""));
        }
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

    /// Open the lobby: this device's own as its host, or a friend's as a
    /// guest. A guest plays alone, in its own world, until the friend's game
    /// is there (`take_in` then takes the host's world), so friends waiting
    /// in an empty lobby never share a world nobody runs.
    fn join(mut sim: ResMut<Sim>, mut coop: ResMut<Coop>, me: Res<LocalId>) {
        show_lobby(lobby().as_deref());
        let Some(code) = coop.code.clone() else {
            return;
        };
        let host = me.0 == HOST;
        if host {
            sim.go_online(true);
        }
        connect(&code);
        coop.status = status(host, false, 0);
    }

    /// The introducer's address: the live one, or `?introducer=` for testing.
    pub(crate) fn introducer() -> String {
        query("introducer")
            .filter(|u| u.starts_with("ws://") || u.starts_with("wss://"))
            .unwrap_or_else(|| INTRODUCER.into())
    }

    /// Join `code`'s room at the introducer.
    fn connect(code: &str) {
        let url = format!(
            "{}/{}",
            introducer(),
            room(save::build_slug().as_deref(), code)
        );
        let (socket, run) = WebRtcSocket::builder(url)
            .add_reliable_channel()
            .add_unreliable_channel()
            .add_reliable_channel()
            .build();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = run.await {
                warn!("co-op connection ended: {e}");
            }
        });
        LINK.with(|l| {
            *l.borrow_mut() = Some(Link {
                socket,
                me: None,
                peers: Vec::new(),
                waiting: Vec::new(),
            })
        });
    }

    pub(crate) use super::invite_rect as button_rect;

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
                top: px(status_top(window.size())),
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
    /// the link.
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
        // The first time: this device's lobby code, kept for good. Then the
        // lobby opens here, unless another tab on this device has it open.
        let mine = lobby().unwrap_or_else(|| {
            let code = make_code(js_sys::Math::random);
            save::write_key(LOBBY_KEY, &code);
            show_lobby(Some(&code));
            code
        });
        let other_tab = open_in_other_tab(
            save::read_key(MARK_KEY).as_deref(),
            &tab_id(),
            js_sys::Date::now(),
        );
        if coop.code.is_none() && me.0 == HOST && !other_tab {
            sim.go_online(true);
            connect(&mine);
            coop.code = Some(mine.clone());
            coop.status = status(true, false, 0);
        }
        // A guest shares the lobby it's in; everyone else their own.
        let link = if me.0 == HOST {
            mine
        } else {
            coop.code.clone().unwrap_or(mine)
        };
        share(&invite_link(&link));
        coop.flash = FLASH_FOR;
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
    /// their messages, which go to the simulation. Lost everyone (or the
    /// connection itself)? Join the room afresh every few seconds.
    fn take_in(time: Res<Time>, mut sim: ResMut<Sim>, mut coop: ResMut<Coop>, me: Res<LocalId>) {
        let Some(code) = coop.code.clone() else {
            return;
        };
        let host = me.0 == HOST;
        let alive = LINK.with(|cell| {
            let mut cell = cell.borrow_mut();
            let Some(link) = cell.as_mut() else {
                return false;
            };
            let Ok(changes) = link.socket.try_update_peers() else {
                return false;
            };
            if link.me.is_none() {
                link.me = link.socket.id();
            }
            let mut gone = Vec::new();
            for (peer, state) in changes {
                match state {
                    PeerState::Connected => {
                        info!("co-op: a device connected");
                        link.peers.push(Peer {
                            id: peer,
                            player: None,
                            quiet: 0.0,
                        });
                        if host {
                            link.send(peer, sim.world().to_bytes().into(), RELIABLE);
                        }
                    }
                    PeerState::Disconnected => gone.push(peer),
                }
            }
            let mut met = Vec::new();
            for channel in [RELIABLE, QUICK] {
                for (peer, bytes) in link.socket.channel_mut(channel).receive() {
                    let Some(p) = link.peers.iter_mut().find(|p| p.id == peer) else {
                        continue;
                    };
                    p.quiet = 0.0;
                    let Some(msg) = Msg::from_bytes(&bytes) else {
                        continue;
                    };
                    // Met for the first time, or again after going silent.
                    if let Msg::Crab { crab, .. } = &msg
                        && p.player.is_none()
                        && crab.id != me.0
                    {
                        p.player = Some(crab.id);
                        info!("co-op: met player {}", crab.id.0);
                        met.push(peer);
                    }
                    // The host is here (again): back to its world.
                    if from_host(&msg) && !host && !sim.online() {
                        sim.go_online(false);
                    }
                    sim.receive(msg);
                }
            }
            // Met again after a silence: how the world stands now.
            if host {
                for peer in met {
                    link.send(peer, sim.world().to_bytes().into(), RELIABLE);
                }
            }
            for (peer, bytes) in link.socket.channel_mut(VOICE).receive() {
                crate::voice::hear(peer, &bytes);
            }
            if let Some(me) = link.me {
                for p in &link.peers {
                    crate::voice::meet(p.id, me);
                }
            }
            // A friend who goes silent (its game froze or its network stalled, or it plays alone, waiting for the
            // host) is no longer played with, but its connection and voice call stay up: it's met again the moment
            // its crab speaks. Only a connection that ends drops the device.
            let mut lost = Vec::new();
            for p in &mut link.peers {
                p.quiet += time.delta_secs();
                if p.quiet > SILENT_FOR
                    && let Some(id) = p.player.take()
                {
                    info!("co-op: a device went silent");
                    lost.push(id);
                }
            }
            for peer in gone {
                crate::voice::forget(peer);
                let Some(i) = link.peers.iter().position(|p| p.id == peer) else {
                    continue;
                };
                info!("co-op: a device left");
                lost.extend(link.peers.remove(i).player);
            }
            for id in lost {
                if id == HOST {
                    // The host went: play on alone until it's back.
                    sim.go_offline();
                } else {
                    sim.remove_crab(id);
                }
            }
            true
        });
        let (n, with_host) = LINK.with(|cell| {
            cell.borrow().as_ref().map_or((0, false), |l| {
                (l.known().count(), l.known().any(|id| id == HOST))
            })
        });
        let together = if host { n > 0 } else { with_host };
        if coop.watch.step(together, alive, time.delta_secs()) {
            rejoin(&mut sim, &code, host);
        }
        coop.status = status(host, together, n);
    }

    /// Drop the connection (and everyone on it) and join `code`'s room
    /// afresh: whoever is still there, or comes back, meets this device anew.
    fn rejoin(sim: &mut Sim, code: &str, host: bool) {
        info!("co-op: joining the room afresh");
        crate::voice::forget_all();
        let gone: Vec<PlayerId> = LINK.with(|cell| {
            cell.borrow_mut().take().map_or(Vec::new(), |mut link| {
                link.socket.close();
                link.known().collect()
            })
        });
        if host {
            for id in gone {
                sim.remove_crab(id);
            }
        } else {
            sim.go_offline();
        }
        connect(code);
    }

    /// Send what the simulation wrote this frame.
    fn send_out(mut sim: ResMut<Sim>) {
        let out = sim.take_outbox();
        LINK.with(|cell| {
            let mut cell = cell.borrow_mut();
            let Some(link) = cell.as_mut() else {
                return;
            };
            for (peer, bytes) in crate::voice::outbox() {
                if link.peers.iter().any(|p| p.id == peer) {
                    link.send(peer, bytes.into(), VOICE);
                }
            }
            for (id, bytes, channel) in std::mem::take(&mut link.waiting) {
                link.send_to(id, bytes, channel);
            }
            for Outgoing { to, msg } in out {
                let channel = if msg.reliable() { RELIABLE } else { QUICK };
                let bytes: Packet = msg.to_bytes().into();
                match to {
                    To::Everyone => {
                        let peers: Vec<PeerId> = link.peers.iter().map(|p| p.id).collect();
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
        sim: Res<Sim>,
        mut world: Local<&'static str>,
        mut label: Single<&mut Text, (With<InviteLabel>, Without<StatusLine>)>,
        mut line: Single<&mut Text, (With<StatusLine>, Without<InviteLabel>)>,
    ) {
        // Whose world this browser plays in, for the browser tests: a friend waiting for a host who isn't there
        // plays its own ("solo"), never half of someone else's.
        let now = match (sim.online(), sim.is_host()) {
            (false, _) => "solo",
            (true, true) => "host",
            (true, false) => "guest",
        };
        if *world != now {
            *world = now;
            if let Some(canvas) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("crabscape"))
            {
                let _ = canvas.set_attribute("data-world", now);
            }
        }
        let words = if coop.flash > 0.0 {
            "Link ready!"
        } else {
            "Invite a friend"
        };
        if label.0 != words {
            label.0 = words.into();
        }
        if line.0 != coop.status {
            line.0.clone_from(&coop.status);
            // For the browser tests, which can't read the game's own text.
            if let Some(canvas) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("crabscape"))
            {
                let _ = canvas.set_attribute("data-coop", &coop.status);
            }
        }
    }

    /// Keep the button and its line in place as the window changes.
    fn place(
        window: Single<&Window>,
        mut button: Single<&mut Node, (With<InviteButton>, Without<StatusLine>)>,
        mut line: Single<&mut Node, (With<StatusLine>, Without<InviteButton>)>,
    ) {
        let r = button_rect(window.size());
        let (top, below) = (px(r.min.y), px(status_top(window.size())));
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

    // The promises of docs/design/coop-promises.md, in plain logic with a pretend clock. The browser scenarios
    // (`cargo cooptest`) check the same promises in real browsers.

    #[test]
    fn your_own_link_makes_you_host_and_your_lobby_opens_whenever_you_play() {
        let host = |c: &str| Start::Host(c.to_string());
        let guest = |c: &str| Start::Guest(c.to_string());
        // Never shared a link: solo, and a friend's link makes you their guest.
        assert_eq!(start(None, None, false), Start::Solo);
        assert_eq!(start(Some("abc234"), None, false), guest("abc234"));
        // Shared once: your lobby opens every time, with or without your own link.
        assert_eq!(start(None, Some("mine22"), false), host("mine22"));
        assert_eq!(start(Some("mine22"), Some("mine22"), false), host("mine22"));
        // A friend's link still makes you their guest.
        assert_eq!(
            start(Some("abc234"), Some("mine22"), false),
            guest("abc234")
        );
        // Already open in another tab of this device: this one plays alone, never a second host.
        assert_eq!(start(None, Some("mine22"), true), Start::Solo);
        assert_eq!(start(Some("mine22"), Some("mine22"), true), Start::Solo);
    }

    #[test]
    fn a_tab_knows_when_another_tab_has_the_lobby_open() {
        let now = 1_000_000.0;
        assert!(open_in_other_tab(Some("tabB 999000"), "tabA", now));
        // Its own mark (a reload of the hosting tab), an old mark, or rubbish: not open elsewhere.
        assert!(!open_in_other_tab(Some("tabA 999000"), "tabA", now));
        assert!(!open_in_other_tab(Some("tabB 990000"), "tabA", now));
        assert!(!open_in_other_tab(Some("tabB soon"), "tabA", now));
        assert!(!open_in_other_tab(None, "tabA", now));
    }

    #[test]
    fn the_status_line_says_where_you_stand() {
        assert_eq!(
            status(true, false, 0),
            "Lobby open: waiting for a friend..."
        );
        assert_eq!(status(false, false, 0), "Waiting for your friend...");
        assert_eq!(status(true, true, 2), "Playing with 2 friends");
        assert_eq!(status(false, true, 1), "Playing with 1 friend");
    }

    /// Run `seconds` of a [`Watch`] with `together` and `alive` as given; the times (seconds) it rejoined.
    fn rejoins(w: &mut Watch, seconds: f32, together: bool, alive: bool) -> Vec<f32> {
        let dt = 0.1;
        (1..=(seconds / dt).round() as u32)
            .filter(|_| w.step(together, alive, dt))
            .map(|i| i as f32 * dt)
            .collect()
    }

    #[test]
    fn a_friend_waiting_in_an_empty_lobby_stays_put_and_a_dead_connection_is_redone() {
        // Waiting for a host who never came, with a working connection: nothing to redo.
        assert!(rejoins(&mut Watch::default(), 120.0, false, true).is_empty());
        // The introducer went away (restarted, or a deploy): join afresh every 5 s until it's back.
        let times = rejoins(&mut Watch::default(), 12.0, false, false);
        assert_eq!(times.len(), 2, "{times:?}");
        assert!((times[0] - REJOIN_AFTER).abs() < 0.15, "{times:?}");
    }

    #[test]
    fn lost_company_is_looked_for_every_15_seconds() {
        let mut w = Watch::default();
        assert!(rejoins(&mut w, 3.0, true, true).is_empty());
        let times = rejoins(&mut w, 31.0, false, true);
        assert_eq!(times.len(), 2, "{times:?}");
        assert!((times[0] - ALONE_FOR).abs() < 0.15, "{times:?}");
    }

    /// The host's side of a friend reloading, with a pretend introducer: the friend's page is back in the room
    /// after `reload` seconds; a connection then takes `setup` seconds and is cut off if the host joins the room
    /// afresh before it's done. When are they together again (seconds), if within `limit`?
    fn back_together(reload: f32, setup: f32, limit: f32) -> Option<f32> {
        let dt = 0.05;
        let mut host = Watch::default();
        rejoins(&mut host, 5.0, true, true); // playing together before the reload
        let mut setting_up = None::<f32>;
        let mut t = 0.0;
        while t < limit {
            t += dt;
            if t >= reload && setting_up.is_none() {
                setting_up = Some(0.0);
            }
            if let Some(s) = &mut setting_up {
                *s += dt;
                if *s >= setup {
                    return Some(t);
                }
            }
            if host.step(false, true, dt) {
                // Joining afresh cuts off a connection still being set up; it starts again in the new room.
                setting_up = setting_up.map(|_| 0.0);
            }
        }
        None
    }

    #[test]
    fn a_friend_who_reloads_is_back_within_10_seconds() {
        // A page load of 2 s, then at most 3 s for addresses on each side and a moment to connect (the cap in
        // vendor/matchbox_socket): well inside the 10 s promise, and inside the host's 15 s patience.
        let setup = 3.0 + 3.0 + 1.0;
        let back = back_together(2.0, setup, 60.0).expect("back together");
        assert!(back <= 10.0, "{back}");
        assert!(setup < ALONE_FOR);
        // Before the cap: a browser that can't reach the address servers took ~40 s each way, longer than the
        // host's 15 s, so every attempt was cut off and the friend never got back in.
        assert_eq!(back_together(2.0, 80.0, 300.0), None);
    }

    #[test]
    fn a_waiting_friend_takes_the_hosts_world_from_its_first_word() {
        use crabsim::Sim;
        use crabsim::net::Msg;
        let mut host = Sim::with_one_crab(Vec2::new(0.0, 0.0));
        host.go_online(true);
        for _ in 0..crabsim::net::SEND_EVERY {
            host.tick();
        }
        let crabs: Vec<Msg> = host
            .take_outbox()
            .into_iter()
            .map(|o| o.msg)
            .filter(|m| matches!(m, Msg::Crab { .. }))
            .collect();
        assert!(!crabs.is_empty(), "the host sends its crab");
        // The world comes first, often before the crab: either one is the host.
        assert!(from_host(&host.world()));
        assert!(crabs.iter().all(from_host));
        // Another friend waiting in the lobby is not the host: no taking its world.
        let mut other = crabs[0].clone();
        if let Msg::Crab { crab, .. } = &mut other {
            crab.id = PlayerId(7);
        }
        assert!(!from_host(&other));
        assert!(!from_host(&Msg::NetCut(0)));
    }
}
