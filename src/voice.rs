//! Voice chat (co-op): friends in the room hear each other, straight
//! browser to browser.
//!
//! Each pair of devices gets an audio connection of its own (WebRTC). It is
//! set up by swapping a few notes over the co-op connection's voice channel
//! (see `coop.rs`), so the introducer is never involved. Of each pair, the
//! device whose connection id sorts first makes the offer. Every connection
//! is made ready to send from the start, so turning the microphone on or off
//! only plugs it in or pulls it out: nothing has to be set up again. The
//! browser does the echo cancelling and noise reduction.
//!
//! A friend's voice plays through the game's own audio (`sound.rs`), at a
//! volume from the settings: everyone at full volume, or fading with the
//! distance between the crabs (`settings::Settings::volume_at`), into the
//! sound's voices knob: the "Voices" slider (up to 3x, to lift quiet voices),
//! under "Master" like every other sound.
//!
//! The microphone starts off; the Mic button, beside the invite button while
//! in a room, turns it on (the browser asks first) and off. Web only.
//!
//! What everyone says is written down as text by the host's computer, from
//! its own microphone and each friend's call as it arrives, and sent to
//! every friend over the same notes channel (`scribe.rs`): a pencil on the
//! Mic button shows while this player's voice is being written down.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// What two devices tell each other to set up their audio connection.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
enum Note {
    Offer(String),
    Answer(String),
    /// A way to reach this device (an ICE candidate, as the browser's JSON).
    Reach(String),
    /// What someone said, written down by the host's device (`scribe.rs`):
    /// their crab's name, the words, and how long ago (seconds) they began.
    Said {
        who: String,
        text: String,
        ago: f32,
    },
    /// Whether this device's player wants their voice written down
    /// (Settings, Co-op).
    Writes {
        who: String,
        on: bool,
    },
    /// From the host's device: whether it writes down this friend's voice,
    /// or why it writes nothing.
    Host(crate::scribe::HostSays),
}

/// The longest note accepted (a session description is a few kilobytes).
const NOTE_MAX: usize = 16 * 1024;

impl Note {
    fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    fn from_bytes(bytes: &[u8]) -> Option<Note> {
        if bytes.len() > NOTE_MAX {
            return None;
        }
        serde_json::from_slice(bytes).ok()
    }
}

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, _app: &mut App) {
        #[cfg(target_arch = "wasm32")]
        web::build(_app);
    }
}

/// Why the browser didn't hand over the microphone, so the player knows
/// whether they can fix it (shared with the report form's Speak).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) enum MicTrouble {
    /// The player, the browser or the phone said no.
    Blocked,
    /// There's no microphone.
    Missing,
    /// Another app (a phone call, say) has it.
    Busy,
    /// Anything else, or a page where this browser can't use one at all.
    Other,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl MicTrouble {
    /// From the name of the browser's error (older names included).
    pub(crate) fn from_error(name: &str) -> Self {
        match name {
            "NotAllowedError" | "SecurityError" | "PermissionDeniedError" => Self::Blocked,
            "NotFoundError" | "OverconstrainedError" | "DevicesNotFoundError" => Self::Missing,
            "NotReadableError" | "AbortError" | "TrackStartError" => Self::Busy,
            _ => Self::Other,
        }
    }

    /// A few words for the Mic button.
    pub(crate) fn short(self) -> &'static str {
        match self {
            Self::Blocked => "Blocked",
            Self::Busy => "Mic busy",
            Self::Missing | Self::Other => "No mic",
        }
    }

    /// What happened and what the player can do about it.
    pub(crate) fn advice(self) -> &'static str {
        match self {
            Self::Blocked => {
                "The microphone is blocked. Allow it for this page; on an iPhone, also switch on \
                 Microphone for Chrome or Safari in the Settings app. Then try again, or type."
            }
            Self::Missing => "No microphone found: you can still type.",
            Self::Busy => "Another app is using the microphone. Close it and try again, or type.",
            Self::Other => "This browser can't use the microphone here: you can still type.",
        }
    }
}

/// Mic button size, beside the invite button.
const MIC_WIDTH: f32 = 64.0;
const GAP: f32 = 8.0;

/// The Mic button: left of the invite button, or on a narrow screen (where
/// the buttons sit lower, level with the health bars) a row lower, under the
/// invite button's right end, below the bars.
pub(crate) fn mic_rect(window: Vec2) -> Rect {
    let invite = crate::coop::invite_rect(window);
    if crate::settings::narrow(window) {
        let row = crate::settings::below(window, 1.0);
        return Rect::new(invite.max.x - MIC_WIDTH, row.min.y, invite.max.x, row.max.y);
    }
    let right = invite.min.x - GAP;
    Rect::new(right - MIC_WIDTH, invite.min.y, right, invite.max.y)
}

/// How loud a friend's voice has been lately, for the browser tests ("hearing N").
#[derive(Default)]
struct Meter {
    /// The loudest reading lately, held a while so a pause between sounds still counts.
    loudest: f32,
    /// When it was last read (seconds).
    read_at: Option<f64>,
}

/// Samples the meter reads at once: the most a browser's meter holds, about 0.7 s, longer than the half second
/// between a test microphone's beeps, so a reading can't fall between two beeps however the frames line up.
const METER_SAMPLES: u32 = 32768;
/// Seconds between readings: a reading is a long window, so a few a second cover all the sound.
const METER_EVERY: f64 = 0.25;
/// What the loudest reading keeps of itself per reading: a voice that stops counts for about a second more.
const METER_HOLD: f32 = 0.5;
/// Louder than this counts as heard.
const HEARD: f32 = 0.01;

impl Meter {
    /// A frame at `now` (seconds): read the voice's level with `level` if it's time.
    fn tick(&mut self, now: f64, level: impl FnOnce() -> f32) {
        if self.read_at.is_some_and(|t| now - t < METER_EVERY) {
            return;
        }
        self.read_at = Some(now);
        self.loudest = (self.loudest * METER_HOLD).max(level());
    }

    fn heard(&self) -> bool {
        self.loudest > HEARD
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use web::{
    calls, friend_streams, loudness, mic_track, mic_trouble, say_host, say_line, say_writes,
};
#[cfg(target_arch = "wasm32")]
pub use web::{forget, forget_all, hear, meet, outbox};

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use bevy::prelude::*;
    use crabsim::{PlayerId, Sim};
    use js_sys::{Array, Reflect};
    use matchbox_socket::PeerId;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{
        AnalyserNode, AudioContext, GainNode, HtmlAudioElement, MediaStream,
        MediaStreamConstraints, MediaStreamTrack, RtcConfiguration, RtcIceServer,
        RtcPeerConnection, RtcPeerConnectionIceEvent, RtcRtpTransceiver,
        RtcRtpTransceiverDirection, RtcRtpTransceiverInit, RtcSdpType, RtcSessionDescriptionInit,
        RtcTrackEvent,
    };

    use super::{METER_SAMPLES, Meter, MicTrouble, Note};
    use crate::coop::LocalId;
    use crate::player::{GameSet, LocalPlayer};
    use crate::scribe::web as scribe;
    use crate::settings::Settings;
    use crate::sound::Mic;

    /// The public servers that tell a browser its outside address (the
    /// same ones the co-op connection uses).
    const STUN: [&str; 2] = [
        "stun:stun.l.google.com:19302",
        "stun:stun1.l.google.com:19302",
    ];

    /// One friend's audio connection.
    struct Line {
        peer: PeerId,
        pc: RtcPeerConnection,
        /// Their voice, once it arrives: the volume knob, and the `<audio>`
        /// some browsers need before a remote voice reaches Web Audio.
        gain: Option<GainNode>,
        audio: Option<HtmlAudioElement>,
        /// How loud their voice is arriving (for the browser tests).
        meter: Option<AnalyserNode>,
        /// Their voice as it arrives, for the host to write down.
        stream: Option<MediaStream>,
        level: Meter,
        /// Ways to reach them that came before their description could be
        /// applied; added once it is.
        early: Vec<String>,
        described: bool,
        /// Kept alive while the connection is: the browser calls them.
        _on_reach: Closure<dyn FnMut(RtcPeerConnectionIceEvent)>,
        _on_track: Closure<dyn FnMut(RtcTrackEvent)>,
    }

    #[derive(Default)]
    struct Voice {
        lines: Vec<Line>,
        /// Notes waiting to be sent over the co-op connection.
        outbox: Vec<(PeerId, Vec<u8>)>,
        /// The microphone while it is on.
        mic: Option<MediaStreamTrack>,
        /// Asked the browser for the microphone; waiting for the answer.
        asking: bool,
        /// Why the browser last said no.
        refused: Option<MicTrouble>,
    }

    thread_local! {
        static VOICE: RefCell<Voice> = RefCell::new(Voice::default());
    }

    fn with<R>(f: impl FnOnce(&mut Voice) -> R) -> R {
        VOICE.with(|v| f(&mut v.borrow_mut()))
    }

    fn send(peer: PeerId, note: &Note) {
        let bytes = note.to_bytes();
        with(|v| v.outbox.push((peer, bytes)));
    }

    /// The microphone, while it is on.
    pub(crate) fn mic_track() -> Option<MediaStreamTrack> {
        with(|v| v.mic.clone())
    }

    /// Every friend on a call.
    pub(crate) fn calls() -> Vec<PeerId> {
        with(|v| v.lines.iter().map(|l| l.peer).collect())
    }

    /// Each friend's voice, as it arrives.
    pub(crate) fn friend_streams() -> Vec<(PeerId, MediaStream)> {
        with(|v| {
            v.lines
                .iter()
                .filter_map(|l| Some((l.peer, Clone::clone(l.stream.as_ref()?))))
                .collect()
        })
    }

    /// As the host: tell `to` whether its voice is written down.
    pub(crate) fn say_host(to: PeerId, says: crate::scribe::HostSays) {
        send(to, &Note::Host(says));
    }

    /// Tell every friend on a call what someone said.
    pub(crate) fn say_line(who: &str, text: &str, ago: f32) {
        let note = Note::Said {
            who: who.into(),
            text: text.into(),
            ago,
        };
        for peer in with(|v| v.lines.iter().map(|l| l.peer).collect::<Vec<_>>()) {
            send(peer, &note);
        }
    }

    /// Tell `to` (or every friend on a call) whether this player wants their voice written down.
    pub(crate) fn say_writes(to: Option<PeerId>, who: &str, on: bool) {
        let note = Note::Writes {
            who: who.into(),
            on,
        };
        let peers = match to {
            Some(p) => vec![p],
            None => with(|v| v.lines.iter().map(|l| l.peer).collect()),
        };
        for peer in peers {
            send(peer, &note);
        }
    }

    /// Notes for the co-op connection's voice channel.
    pub fn outbox() -> Vec<(PeerId, Vec<u8>)> {
        with(|v| std::mem::take(&mut v.outbox))
    }

    /// A device connected (`me` is this device's connection id): the one
    /// whose id sorts first starts the audio connection.
    pub fn meet(peer: PeerId, me: PeerId) {
        if me >= peer || with(|v| v.lines.iter().any(|l| l.peer == peer)) {
            return;
        }
        let Some(pc) = line(peer) else {
            return;
        };
        spawn_local(async move {
            let Ok(offer) = JsFuture::from(pc.create_offer()).await else {
                return;
            };
            let sdp = sdp_of(&offer);
            let desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
            desc.set_sdp(&sdp);
            if JsFuture::from(pc.set_local_description(&desc))
                .await
                .is_ok()
            {
                send(peer, &Note::Offer(sdp));
            }
        });
    }

    /// A device left: hang up its line.
    pub fn forget(peer: PeerId) {
        with(|v| {
            v.lines.retain(|l| {
                if l.peer == peer {
                    hang_up(l);
                }
                l.peer != peer
            });
            v.outbox.retain(|(p, _)| *p != peer);
        });
        scribe::forget(peer);
    }

    /// The co-op connection was dropped: hang up every line.
    pub fn forget_all() {
        with(|v| {
            for l in &v.lines {
                hang_up(l);
            }
            v.lines.clear();
            v.outbox.clear();
        });
        scribe::forget_all();
    }

    fn hang_up(l: &Line) {
        l.pc.close();
        if let Some(a) = &l.audio {
            let _ = a.pause();
            a.set_src_object(None);
        }
        if let Some(g) = &l.gain {
            let _ = g.disconnect();
        }
    }

    /// A note from `peer` arrived on the voice channel.
    pub fn hear(peer: PeerId, bytes: &[u8]) {
        match Note::from_bytes(bytes) {
            Some(Note::Offer(sdp)) => answer(peer, sdp),
            Some(Note::Answer(sdp)) => {
                let Some(pc) = with(|v| {
                    v.lines
                        .iter()
                        .find(|l| l.peer == peer)
                        .map(|l| l.pc.clone())
                }) else {
                    return;
                };
                spawn_local(async move {
                    let desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
                    desc.set_sdp(&sdp);
                    if JsFuture::from(pc.set_remote_description(&desc))
                        .await
                        .is_ok()
                    {
                        described(peer);
                    }
                });
            }
            Some(Note::Reach(json)) => {
                let ready = with(|v| {
                    let l = v.lines.iter_mut().find(|l| l.peer == peer)?;
                    if l.described {
                        Some(l.pc.clone())
                    } else {
                        l.early.push(json.clone());
                        None
                    }
                });
                if let Some(pc) = ready {
                    add_reach(&pc, &json);
                }
            }
            Some(Note::Said { who, text, ago }) => scribe::friend_said(&who, &text, ago),
            Some(Note::Writes { who, on }) => scribe::friend_writes(peer, &who, on),
            Some(Note::Host(says)) => scribe::host_says(peer, says),
            None => {}
        }
    }

    /// Answer an offer: a line of our own, their description, ours back.
    fn answer(peer: PeerId, sdp: String) {
        forget(peer);
        let Some(pc) = line(peer) else {
            return;
        };
        spawn_local(async move {
            let theirs = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
            theirs.set_sdp(&sdp);
            if JsFuture::from(pc.set_remote_description(&theirs))
                .await
                .is_err()
            {
                return;
            }
            described(peer);
            // Their offer made the line's audio slot; ours sends as well.
            for t in pc.get_transceivers().iter() {
                if let Ok(t) = t.dyn_into::<RtcRtpTransceiver>() {
                    t.set_direction(RtcRtpTransceiverDirection::Sendrecv);
                    if let Some(mic) = with(|v| v.mic.clone()) {
                        let _ = t.sender().replace_track(Some(&mic));
                    }
                }
            }
            let Ok(ans) = JsFuture::from(pc.create_answer()).await else {
                return;
            };
            let sdp = sdp_of(&ans);
            let ours = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
            ours.set_sdp(&sdp);
            if JsFuture::from(pc.set_local_description(&ours))
                .await
                .is_ok()
            {
                send(peer, &Note::Answer(sdp));
            }
        });
    }

    /// Their description is in: the ways to reach them that came early can go in.
    fn described(peer: PeerId) {
        let pending = with(|v| {
            let l = v.lines.iter_mut().find(|l| l.peer == peer)?;
            l.described = true;
            Some((l.pc.clone(), std::mem::take(&mut l.early)))
        });
        if let Some((pc, early)) = pending {
            for json in early {
                add_reach(&pc, &json);
            }
        }
    }

    fn add_reach(pc: &RtcPeerConnection, json: &str) {
        let Ok(candidate) = js_sys::JSON::parse(json) else {
            return;
        };
        let p =
            pc.add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(candidate.unchecked_ref()));
        spawn_local(async move {
            let _ = JsFuture::from(p).await;
        });
    }

    fn sdp_of(desc: &JsValue) -> String {
        Reflect::get(desc, &"sdp".into())
            .ok()
            .and_then(|s| s.as_string())
            .unwrap_or_default()
    }

    /// A new audio connection to `peer`, ready to send and to receive, with
    /// the microphone in it if it's on.
    fn line(peer: PeerId) -> Option<RtcPeerConnection> {
        let config = RtcConfiguration::new();
        let servers = Array::new();
        for url in STUN {
            let server = RtcIceServer::new();
            server.set_urls(&JsValue::from_str(url));
            servers.push(&server);
        }
        config.set_ice_servers(&servers);
        let pc = RtcPeerConnection::new_with_configuration(&config).ok()?;
        let init = RtcRtpTransceiverInit::new();
        init.set_direction(RtcRtpTransceiverDirection::Sendrecv);
        let slot = pc.add_transceiver_with_str_and_init("audio", &init);
        if let Some(mic) = with(|v| v.mic.clone()) {
            let _ = slot.sender().replace_track(Some(&mic));
        }
        let on_reach = Closure::<dyn FnMut(RtcPeerConnectionIceEvent)>::new(
            move |e: RtcPeerConnectionIceEvent| {
                if let Some(c) = e.candidate()
                    && let Ok(json) = js_sys::JSON::stringify(&c.to_json())
                    && let Some(json) = json.as_string()
                {
                    send(peer, &Note::Reach(json));
                }
            },
        );
        pc.set_onicecandidate(Some(on_reach.as_ref().unchecked_ref()));
        let on_track = Closure::<dyn FnMut(RtcTrackEvent)>::new(move |e: RtcTrackEvent| {
            let stream = e
                .streams()
                .get(0)
                .dyn_into::<MediaStream>()
                .ok()
                .or_else(|| {
                    let s = MediaStream::new().ok()?;
                    s.add_track(&e.track());
                    Some(s)
                });
            if let Some(stream) = stream {
                play(peer, &stream);
            }
        });
        pc.set_ontrack(Some(on_track.as_ref().unchecked_ref()));
        with(|v| {
            v.lines.push(Line {
                peer,
                pc: pc.clone(),
                gain: None,
                audio: None,
                meter: None,
                stream: None,
                level: Meter::default(),
                early: Vec::new(),
                described: false,
                _on_reach: on_reach,
                _on_track: on_track,
            })
        });
        Some(pc)
    }

    /// Their voice arrived: play it through the game's audio, with a volume
    /// knob of its own.
    fn play(peer: PeerId, stream: &MediaStream) {
        let Some(ctx): Option<AudioContext> = crate::sound::context() else {
            return;
        };
        // Some browsers (Chrome) only feed a remote voice into Web Audio while
        // an `<audio>` element plays it too; that one stays muted.
        let audio = HtmlAudioElement::new().ok();
        if let Some(a) = &audio {
            a.set_muted(true);
            a.set_src_object(Some(stream));
            let _ = a.play();
        }
        let (Ok(source), Ok(gain)) = (ctx.create_media_stream_source(stream), ctx.create_gain())
        else {
            return;
        };
        let _ = source.connect_with_audio_node(&gain);
        match crate::sound::voices_input() {
            Some(voices) => {
                let _ = gain.connect_with_audio_node(&voices);
            }
            None => {
                let _ = gain.connect_with_audio_node(&ctx.destination());
            }
        }
        let meter = ctx.create_analyser().ok();
        if let Some(m) = &meter {
            m.set_fft_size(METER_SAMPLES);
            let _ = source.connect_with_audio_node(m);
        }
        with(|v| {
            if let Some(l) = v.lines.iter_mut().find(|l| l.peer == peer) {
                l.gain = Some(gain);
                l.audio = audio;
                l.meter = meter;
                l.stream = Some(Clone::clone(stream));
            }
        });
        scribe::friend_joined(peer);
    }

    /// Turn the microphone on (the browser asks the player first) or off.
    fn set_mic(on: bool) {
        if !on {
            let mic = with(|v| v.mic.take());
            if let Some(mic) = mic {
                mic.stop();
                plug(None);
                crate::sound::recording(Mic::Voice, false);
            }
            return;
        }
        if with(|v| v.mic.is_some() || v.asking) {
            return;
        }
        let Some(devices) = web_sys::window().and_then(|w| w.navigator().media_devices().ok())
        else {
            with(|v| v.refused = Some(MicTrouble::Other));
            return;
        };
        let audio = js_sys::Object::new();
        for key in ["echoCancellation", "noiseSuppression", "autoGainControl"] {
            let _ = Reflect::set(&audio, &key.into(), &JsValue::TRUE);
        }
        let wanted = MediaStreamConstraints::new();
        wanted.set_audio(&audio);
        crate::sound::recording(Mic::Voice, true);
        let ask = match devices.get_user_media_with_constraints(&wanted) {
            Ok(ask) => ask,
            Err(e) => {
                crate::sound::recording(Mic::Voice, false);
                with(|v| v.refused = Some(mic_trouble(&e)));
                return;
            }
        };
        with(|v| v.asking = true);
        spawn_local(async move {
            let track = match JsFuture::from(ask).await {
                Ok(s) => s
                    .dyn_into::<MediaStream>()
                    .ok()
                    .and_then(|s| {
                        s.get_audio_tracks()
                            .get(0)
                            .dyn_into::<MediaStreamTrack>()
                            .ok()
                    })
                    .ok_or(MicTrouble::Missing),
                Err(e) => Err(mic_trouble(&e)),
            };
            let track = with(|v| {
                v.asking = false;
                v.refused = track.as_ref().err().copied();
                v.mic = track.ok();
                v.mic.clone()
            });
            match track {
                Some(track) => plug(Some(&track)),
                None => crate::sound::recording(Mic::Voice, false),
            }
        });
    }

    /// Why the browser refused the microphone with `error`, logged so it
    /// reaches bug reports.
    pub(crate) fn mic_trouble(error: &JsValue) -> MicTrouble {
        let field = |key: &str| {
            Reflect::get(error, &key.into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default()
        };
        let name = field("name");
        warn!("microphone refused: {name}: {}", field("message"));
        MicTrouble::from_error(&name)
    }

    /// Put `track` (or nothing) into every line's sending side.
    fn plug(track: Option<&MediaStreamTrack>) {
        let pcs: Vec<RtcPeerConnection> = with(|v| v.lines.iter().map(|l| l.pc.clone()).collect());
        for pc in pcs {
            for t in pc.get_transceivers().iter() {
                if let Ok(t) = t.dyn_into::<RtcRtpTransceiver>() {
                    let _ = t.sender().replace_track(track);
                }
            }
        }
    }

    pub fn build(app: &mut App) {
        app.add_systems(Startup, spawn_button)
            .add_systems(Update, press.in_set(GameSet::Input))
            .add_systems(Update, (volumes, show, place).in_set(GameSet::Present));
    }

    #[derive(Component)]
    struct MicButton;

    #[derive(Component)]
    struct MicLabel;

    /// The pencil on the Mic button: this device writes down its player's voice.
    #[derive(Component)]
    struct Pencil;

    use super::mic_rect as button_rect;

    fn spawn_button(mut commands: Commands, window: Single<&Window>) {
        let size = window.size();
        let r = button_rect(size);
        commands
            .spawn((
                MicButton,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(r.min.y),
                    right: px(size.x - r.max.x),
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
                Visibility::Hidden,
            ))
            .with_children(|b| {
                b.spawn((
                    MicLabel,
                    Text::new("Mic off"),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 0.9, 0.7, 0.7)),
                ));
                // A little pencil leaning on the button's top-right edge: a
                // pale wooden body with a dark lead tip.
                b.spawn((
                    Pencil,
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(-9.0),
                        right: px(-2.0),
                        width: px(5.0),
                        height: px(18.0),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::FlexEnd,
                        border_radius: BorderRadius::all(px(1.5)),
                        ..default()
                    },
                    UiTransform::from_rotation(Rot2::degrees(35.0)),
                    BackgroundColor(Color::srgb(0.98, 0.82, 0.45)),
                    Visibility::Hidden,
                ))
                .with_children(|p| {
                    p.spawn((
                        Node {
                            width: px(5.0),
                            height: px(4.0),
                            border_radius: BorderRadius::bottom(px(2.5)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.18, 0.14, 0.12)),
                    ));
                });
            });
    }

    /// A tap on the Mic button, while in a room.
    fn press(mouse: Res<ButtonInput<MouseButton>>, touches: Res<Touches>, window: Single<&Window>) {
        if !crate::coop::in_room() {
            return;
        }
        let rect = button_rect(window.size()).inflate(6.0);
        let clicked = mouse.just_pressed(MouseButton::Left)
            && window.cursor_position().is_some_and(|p| rect.contains(p));
        let tapped = touches
            .iter_just_pressed()
            .any(|t| rect.contains(t.position()));
        if clicked || tapped {
            set_mic(with(|v| v.mic.is_none()));
        }
    }

    /// Each friend's voice at the volume the settings give their distance
    /// (the voices slider is the sound's voices knob, after this).
    fn volumes(
        sim: Res<Sim>,
        settings: Res<Settings>,
        me: Res<LocalId>,
        crab: Query<&Transform, With<LocalPlayer>>,
    ) {
        let here = crab
            .single()
            .map(|t| t.translation.truncate())
            .ok()
            .or_else(|| sim.crab(me.0).map(|c| c.mover.pos));
        let players = crate::coop::players();
        with(|v| {
            for l in &v.lines {
                let Some(gain) = &l.gain else {
                    continue;
                };
                let theirs = players
                    .iter()
                    .find(|(p, _)| *p == l.peer)
                    .and_then(|(_, id): &(PeerId, PlayerId)| sim.crab(*id))
                    .map(|c| c.mover.pos);
                let volume = match (here, theirs) {
                    (Some(a), Some(b)) => settings.volume_at(a.distance(b)),
                    _ => settings.volume_at(0.0),
                };
                gain.gain().set_value(volume);
            }
        });
    }

    /// The Mic button shows in a room, with what the microphone is doing.
    fn show(
        time: Res<Time<Real>>,
        mut button: Single<&mut Visibility, (With<MicButton>, Without<Pencil>)>,
        mut pencil: Single<&mut Visibility, (With<Pencil>, Without<MicButton>)>,
        mut label: Single<(&mut Text, &mut TextColor), With<MicLabel>>,
    ) {
        let room = crate::coop::in_room();
        button.set_if_neq(if room {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if !room {
            set_mic(false);
        }
        pencil.set_if_neq(if scribe::writing() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let (words, colour) = with(|v| match (&v.mic, v.asking, v.refused) {
            (Some(_), _, _) => ("Mic on", Color::srgba(0.7, 1.0, 0.8, 1.0)),
            (None, true, _) => ("Mic...", Color::srgba(1.0, 0.9, 0.7, 0.7)),
            (None, false, Some(trouble)) => (trouble.short(), Color::srgba(1.0, 0.6, 0.5, 0.8)),
            (None, false, None) => ("Mic off", Color::srgba(1.0, 0.9, 0.7, 0.7)),
        });
        if label.0.0 != words {
            label.0.0 = words.into();
            label.1.0 = colour;
        }
        // For the browser tests: the mic, lines, and friends heard right now.
        let state = with(|v| {
            let now = time.elapsed_secs_f64();
            for l in &mut v.lines {
                let meter = l.meter.as_ref();
                l.level.tick(now, || meter.map_or(0.0, peak));
            }
            let heard = v.lines.iter().filter(|l| l.level.heard()).count();
            let lines: Vec<String> = v
                .lines
                .iter()
                .map(|l| {
                    let volume = l.gain.as_ref().map(|g| g.gain().value());
                    format!(
                        "{:?}{} level {:.3}",
                        l.pc.connection_state(),
                        volume.map_or(String::new(), |v| format!(" volume {v:.2}")),
                        l.level.loudest
                    )
                })
                .collect();
            format!(
                // "hearing N" last: the co-op tests wait for the line to end with it.
                "{words}; lines {} {lines:?}; {}; hearing {heard}",
                v.lines.len(),
                scribe::state()
            )
        });
        if let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("crabscape"))
            && canvas.get_attribute("data-voice").as_deref() != Some(&state)
        {
            let _ = canvas.set_attribute("data-voice", &state);
        }
    }

    /// How loud a voice is right now (root mean square, 0 to 1).
    pub(crate) fn loudness(meter: &AnalyserNode) -> f32 {
        let mut samples = vec![0.0_f32; meter.fft_size() as usize];
        meter.get_float_time_domain_data(&mut samples);
        (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    /// The loudest sample in the meter's window (0 to 1).
    fn peak(meter: &AnalyserNode) -> f32 {
        let mut samples = vec![0.0_f32; meter.fft_size() as usize];
        meter.get_float_time_domain_data(&mut samples);
        samples.iter().fold(0.0, |m, x| m.max(x.abs()))
    }

    /// Keep the button in place as the window changes (or the phone turns).
    fn place(window: Single<&Window>, mut node: Single<&mut Node, With<MicButton>>) {
        let size = window.size();
        let r = button_rect(size);
        let (top, right) = (px(r.min.y), px(size.x - r.max.x));
        if node.top != top {
            node.top = top;
        }
        if node.right != right {
            node.right = right;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blocked_microphone_is_told_apart_from_a_missing_or_busy_one() {
        for (error, trouble) in [
            ("NotAllowedError", MicTrouble::Blocked),
            ("SecurityError", MicTrouble::Blocked),
            ("NotFoundError", MicTrouble::Missing),
            ("NotReadableError", MicTrouble::Busy),
            ("TypeError", MicTrouble::Other),
            ("", MicTrouble::Other),
        ] {
            assert_eq!(MicTrouble::from_error(error), trouble, "{error}");
        }
        assert!(MicTrouble::Blocked.advice().contains("iPhone"));
    }

    /// A browser's test microphone: a 20 ms beep every half second. The level of its sound over the `window` seconds
    /// before `t`, with the beeps starting at `phase`.
    fn beeps(t: f64, window: f64, phase: f64) -> f32 {
        let (every, beep) = (0.5, 0.02);
        // The last beep to start before `t`; it, or the one before, may reach into the window.
        let last = phase + ((t - phase) / every).floor() * every;
        let heard = [last, last - every]
            .iter()
            .any(|&start| start <= t && start + beep > t - window);
        if heard { 0.3 } else { 0.0 }
    }

    /// The meter's window, at the lowest sample rate an audio context has (44.1 kHz gives a longer one).
    fn window() -> f64 {
        f64::from(METER_SAMPLES) / 48_000.0
    }

    #[test]
    fn a_beeping_friend_is_heard_however_the_frames_line_up_with_the_beeps() {
        // A busy computer draws a frame every few screen refreshes (1/60 s), steadily, so the frames can keep landing
        // at the same moments between two beeps.
        for refreshes in 2..=30 {
            let frame = f64::from(refreshes) / 60.0;
            for ms in 0..500 {
                let phase = f64::from(ms) / 1000.0;
                let mut meter = Meter::default();
                let mut t = 0.0;
                while t < 15.0 {
                    meter.tick(t, || beeps(t, window(), phase));
                    assert!(
                        t < 1.0 || meter.heard(),
                        "not heard at {t:.2} s, a frame every {frame:.3} s, beeps from {phase} s"
                    );
                    t += frame;
                }
            }
        }
    }

    #[test]
    fn silence_is_not_heard_and_a_voice_that_stops_is_soon_not_heard() {
        let mut meter = Meter::default();
        for frame in 0..600 {
            meter.tick(f64::from(frame) / 60.0, || 0.0);
        }
        assert!(!meter.heard(), "nothing arrived");
        let mut faint = Meter::default();
        faint.tick(0.0, || HEARD);
        assert!(
            !faint.heard(),
            "a hiss no louder than the line counts for nothing"
        );
        for frame in 0..60 {
            meter.tick(10.0 + f64::from(frame) / 60.0, || 0.3);
        }
        assert!(meter.heard());
        for frame in 0..180 {
            meter.tick(11.0 + f64::from(frame) / 60.0, || 0.0);
        }
        assert!(!meter.heard(), "still heard 3 s after the voice stopped");
    }

    #[test]
    fn the_meter_is_read_four_times_a_second_however_fast_the_frames() {
        let mut meter = Meter::default();
        let mut reads = 0;
        for frame in 0..=100 {
            meter.tick(f64::from(frame) / 100.0, || {
                reads += 1;
                0.0
            });
        }
        assert_eq!(reads, 5, "at 0, 0.25, 0.5, 0.75 and 1 s");
    }

    #[test]
    fn a_line_sent_by_a_friends_device_lands_in_my_transcript_with_their_name() {
        let bytes = Note::Said {
            who: "Crab 41".into(),
            text: "the door won't open".into(),
            ago: 2.0,
        }
        .to_bytes();
        let Some(Note::Said { who, text, ago }) = Note::from_bytes(&bytes) else {
            panic!("a line comes back a line");
        };
        let mut book = crate::scribe::Book::default();
        book.friend_said(&who, &text, ago, 100.0);
        assert_eq!(
            book.transcript.text(100.0, 0, 1000),
            "[00:01:38] Crab 41: the door won't open"
        );
    }

    #[test]
    fn notes_come_back_whole_and_bad_bytes_are_dropped() {
        for note in [
            Note::Offer("v=0\r\no=- 1 2 IN IP4 127.0.0.1".into()),
            Note::Answer("v=0".into()),
            Note::Reach(r#"{"candidate":"candidate:1 1 udp 1 1.2.3.4 5 typ host"}"#.into()),
            Note::Said {
                who: "Crab 41".into(),
                text: "over here".into(),
                ago: 3.5,
            },
            Note::Writes {
                who: "Crab 41".into(),
                on: false,
            },
            Note::Host(crate::scribe::HostSays {
                skip: Some(crate::scribe::Skip::Phone),
                you: false,
            }),
            Note::Host(crate::scribe::HostSays {
                skip: None,
                you: true,
            }),
        ] {
            assert_eq!(Note::from_bytes(&note.to_bytes()), Some(note));
        }
        assert_eq!(Note::from_bytes(b"\xff\x00 nonsense"), None);
        let huge = Note::Offer("x".repeat(NOTE_MAX)).to_bytes();
        assert_eq!(Note::from_bytes(&huge), None, "too long to be a real note");
    }
}
