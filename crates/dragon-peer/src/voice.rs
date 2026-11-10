//! Pairwise opt-in voice. Notes travel only on Dragon's reliable P2P channel;
//! audio travels directly between browsers, never through a recording service.
use serde::{Deserialize, Serialize};

const NOTE_MAX: usize = 16 * 1024;
const ICE_MAX: usize = 4096;
const SDP_MAX: usize = 12 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Note {
    version: String,
    call: String,
    signal: Signal,
}
#[derive(Serialize, Deserialize)]
enum Signal {
    Offer(String),
    Answer(String),
    Ice(String),
}
impl Note {
    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > NOTE_MAX {
            return None;
        }
        let note: Self = serde_json::from_slice(bytes).ok()?;
        let valid_call = !note.call.is_empty()
            && note.call.len() <= 64
            && note
                .call
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-');
        let valid_signal = match &note.signal {
            Signal::Offer(s) | Signal::Answer(s) => !s.is_empty() && s.len() <= SDP_MAX,
            Signal::Ice(s) => !s.is_empty() && s.len() <= ICE_MAX,
        };
        (note.version == super::VERSION && valid_call && valid_signal).then_some(note)
    }
}

/// A late permission result must not undo Cancel, Mute, Leave, or a newer request.
#[derive(Default)]
struct MicRequest {
    generation: u64,
    wanted: bool,
}
impl MicRequest {
    fn set(&mut self, on: bool) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.wanted = on;
        self.generation
    }
    fn accepts(&self, generation: u64) -> bool {
        self.wanted && self.generation == generation
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) mod browser {
    use super::*;
    use js_sys::{Array, Reflect};
    use matchbox_socket::PeerId;
    use std::{
        cell::RefCell,
        collections::{HashMap, HashSet},
        rc::{Rc, Weak},
    };
    use wasm_bindgen::{JsCast, JsValue, closure::Closure};
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{
        AnalyserNode, AudioContext, AudioContextState, GainNode, HtmlAudioElement, MediaStream,
        MediaStreamAudioSourceNode, MediaStreamConstraints, MediaStreamTrack,
        MediaStreamTrackState, RtcConfiguration, RtcIceServer, RtcPeerConnection,
        RtcPeerConnectionIceEvent, RtcPeerConnectionState, RtcRtpTransceiver,
        RtcRtpTransceiverDirection, RtcRtpTransceiverInit, RtcSdpType, RtcSessionDescriptionInit,
        RtcTrackEvent,
    };
    const EARLY_MAX: usize = 32;
    type Shared = Rc<RefCell<State>>;
    type WeakState = Weak<RefCell<State>>;

    pub(crate) struct Voice(Shared);
    struct State {
        alive: bool,
        me: Option<PeerId>,
        next_call: u64,
        calls: HashMap<PeerId, Call>,
        outbox: Vec<(PeerId, Vec<u8>)>,
        request: MicRequest,
        asking: bool,
        mic: Option<MediaStream>,
        error: Option<&'static str>,
        context: Option<AudioContext>,
        volume: f32,
    }
    struct Call {
        id: String,
        pc: RtcPeerConnection,
        described: bool,
        answer_pending: bool,
        early: Vec<String>,
        announced: bool,
        pending_ice: Vec<String>,
        stream: Option<MediaStream>,
        playback: Option<Playback>,
        last_heard: f64,
        last_meter: f64,
        _ice: Closure<dyn FnMut(RtcPeerConnectionIceEvent)>,
        _track: Closure<dyn FnMut(RtcTrackEvent)>,
    }
    struct Playback {
        audio: HtmlAudioElement,
        source: MediaStreamAudioSourceNode,
        gain: GainNode,
        meter: AnalyserNode,
    }
    impl Drop for Playback {
        fn drop(&mut self) {
            let _ = self.audio.pause();
            self.audio.set_src_object(None);
            let _ = self.source.disconnect();
            let _ = self.gain.disconnect();
            let _ = self.meter.disconnect();
        }
    }
    impl Drop for Call {
        fn drop(&mut self) {
            // Detach before releasing the Rust closures, including pending calls.
            self.pc.set_onicecandidate(None);
            self.pc.set_ontrack(None);
            self.pc.close();
            if let Some(stream) = &self.stream {
                stop_stream(stream);
            }
        }
    }
    fn stop_stream(stream: &MediaStream) {
        for track in stream.get_tracks().iter() {
            if let Ok(track) = track.dyn_into::<MediaStreamTrack>() {
                track.stop();
            }
        }
    }
    fn settle(promise: js_sys::Promise) {
        spawn_local(async move {
            let _ = JsFuture::from(promise).await;
        });
    }
    fn active(state: &WeakState, peer: PeerId, id: &str) -> Option<Shared> {
        let shared = state.upgrade()?;
        let valid = {
            let state = shared.borrow();
            state.alive && state.calls.get(&peer).is_some_and(|c| c.id == id)
        };
        valid.then_some(shared)
    }
    fn enqueue(state: &mut State, peer: PeerId, call: &str, signal: Signal) {
        // Cap local callback queues too, even if a browser/network is stalled.
        if state.outbox.len() >= 256 {
            return;
        }
        let note = Note {
            version: super::super::VERSION.into(),
            call: call.into(),
            signal,
        };
        if let Ok(bytes) = serde_json::to_vec(&note)
            && bytes.len() <= NOTE_MAX
        {
            state.outbox.push((peer, bytes));
        }
    }
    fn announce(state: &WeakState, peer: PeerId, id: &str, signal: Signal) {
        let Some(shared) = active(state, peer, id) else {
            return;
        };
        let mut state = shared.borrow_mut();
        let Some(call) = state.calls.get_mut(&peer) else {
            return;
        };
        call.announced = true;
        let ice = std::mem::take(&mut call.pending_ice);
        // The offer/answer precedes all of this call's candidates.
        enqueue(&mut state, peer, id, signal);
        for candidate in ice {
            enqueue(&mut state, peer, id, Signal::Ice(candidate));
        }
    }
    fn sdp(value: JsValue) -> Option<String> {
        Reflect::get(&value, &JsValue::from_str("sdp"))
            .ok()?
            .as_string()
            .filter(|s| !s.is_empty() && s.len() <= SDP_MAX)
    }
    fn add_ice(pc: &RtcPeerConnection, json: &str) {
        if let Ok(candidate) = js_sys::JSON::parse(json) {
            settle(pc.add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(
                candidate.unchecked_ref(),
            )));
        }
    }
    fn described(state: &WeakState, peer: PeerId, id: &str) {
        let Some(shared) = active(state, peer, id) else {
            return;
        };
        let mut state = shared.borrow_mut();
        if let Some(call) = state.calls.get_mut(&peer) {
            call.described = true;
            for candidate in std::mem::take(&mut call.early) {
                add_ice(&call.pc, &candidate);
            }
        }
    }
    fn mic_track(state: &State) -> Option<MediaStreamTrack> {
        state
            .mic
            .as_ref()?
            .get_audio_tracks()
            .get(0)
            .dyn_into()
            .ok()
    }
    fn plug(pc: &RtcPeerConnection, track: Option<&MediaStreamTrack>) {
        for transceiver in pc.get_transceivers().iter() {
            if let Ok(slot) = transceiver.dyn_into::<RtcRtpTransceiver>() {
                settle(slot.sender().replace_track(track));
            }
        }
    }
    fn playback(context: &AudioContext, stream: &MediaStream, volume: f32) -> Option<Playback> {
        let audio = HtmlAudioElement::new().ok()?;
        audio.set_muted(true);
        audio.set_src_object(Some(stream));
        if let Ok(promise) = audio.play() {
            settle(promise);
        }
        let source = context.create_media_stream_source(stream).ok()?;
        let gain = context.create_gain().ok()?;
        gain.gain().set_value(volume);
        let meter = context.create_analyser().ok()?;
        // Covers the silent gap between the fake test microphone's beeps too.
        meter.set_fft_size(32768);
        source.connect_with_audio_node(&gain).ok()?;
        gain.connect_with_audio_node(&context.destination()).ok()?;
        gain.connect_with_audio_node(&meter).ok()?;
        Some(Playback {
            audio,
            source,
            gain,
            meter,
        })
    }
    fn make_call(shared: &Shared, peer: PeerId, id: String) -> Option<RtcPeerConnection> {
        let config = RtcConfiguration::new();
        let servers = Array::new();
        for url in [
            "stun:stun.l.google.com:19302",
            "stun:stun1.l.google.com:19302",
        ] {
            let server = RtcIceServer::new();
            server.set_urls(&JsValue::from_str(url));
            servers.push(&server);
        }
        config.set_ice_servers(&servers);
        let pc = RtcPeerConnection::new_with_configuration(&config).ok()?;
        let init = RtcRtpTransceiverInit::new();
        init.set_direction(RtcRtpTransceiverDirection::Sendrecv);
        pc.add_transceiver_with_str_and_init("audio", &init);
        plug(&pc, mic_track(&shared.borrow()).as_ref());
        let weak = Rc::downgrade(shared);
        let call_id = id.clone();
        let ice = Closure::<dyn FnMut(RtcPeerConnectionIceEvent)>::new(
            move |event: RtcPeerConnectionIceEvent| {
                let Some(shared) = active(&weak, peer, &call_id) else {
                    return;
                };
                let Some(json) = event
                    .candidate()
                    .and_then(|c| js_sys::JSON::stringify(&c.to_json()).ok())
                    .and_then(|s| s.as_string())
                    .filter(|s| s.len() <= ICE_MAX)
                else {
                    return;
                };
                let mut state = shared.borrow_mut();
                let Some(call) = state.calls.get_mut(&peer) else {
                    return;
                };
                if call.announced {
                    enqueue(&mut state, peer, &call_id, Signal::Ice(json));
                } else if call.pending_ice.len() < EARLY_MAX {
                    call.pending_ice.push(json);
                }
            },
        );
        pc.set_onicecandidate(Some(ice.as_ref().unchecked_ref()));
        let weak = Rc::downgrade(shared);
        let call_id = id.clone();
        let track = Closure::<dyn FnMut(RtcTrackEvent)>::new(move |event: RtcTrackEvent| {
            let Some(shared) = active(&weak, peer, &call_id) else {
                event.track().stop();
                return;
            };
            let Ok(stream) = MediaStream::new() else {
                return;
            };
            stream.add_track(&event.track());
            let mut state = shared.borrow_mut();
            let playing = state
                .context
                .as_ref()
                .and_then(|ctx| playback(ctx, &stream, state.volume));
            if let Some(call) = state.calls.get_mut(&peer) {
                call.stream = Some(stream);
                call.playback = playing;
            }
        });
        pc.set_ontrack(Some(track.as_ref().unchecked_ref()));
        shared.borrow_mut().calls.insert(
            peer,
            Call {
                id,
                pc: pc.clone(),
                described: false,
                answer_pending: false,
                early: Vec::new(),
                announced: false,
                pending_ice: Vec::new(),
                stream: None,
                playback: None,
                last_heard: -10.0,
                last_meter: -10.0,
                _ice: ice,
                _track: track,
            },
        );
        Some(pc)
    }
    fn offer(shared: &Shared, peer: PeerId, id: String) {
        let Some(pc) = make_call(shared, peer, id.clone()) else {
            return;
        };
        let weak = Rc::downgrade(shared);
        spawn_local(async move {
            let Ok(value) = JsFuture::from(pc.create_offer()).await else {
                return;
            };
            if active(&weak, peer, &id).is_none() {
                return;
            }
            let Some(sdp) = sdp(value) else { return };
            let desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
            desc.set_sdp(&sdp);
            if JsFuture::from(pc.set_local_description(&desc))
                .await
                .is_ok()
            {
                announce(&weak, peer, &id, Signal::Offer(sdp));
            }
        });
    }
    fn answer(shared: &Shared, peer: PeerId, id: String, sdp: String) {
        let Some(pc) = make_call(shared, peer, id.clone()) else {
            return;
        };
        let weak = Rc::downgrade(shared);
        spawn_local(async move {
            let desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
            desc.set_sdp(&sdp);
            if JsFuture::from(pc.set_remote_description(&desc))
                .await
                .is_err()
                || active(&weak, peer, &id).is_none()
            {
                return;
            }
            described(&weak, peer, &id);
            for t in pc.get_transceivers().iter() {
                if let Ok(t) = t.dyn_into::<RtcRtpTransceiver>() {
                    t.set_direction(RtcRtpTransceiverDirection::Sendrecv);
                }
            }
            if let Some(shared) = active(&weak, peer, &id) {
                plug(&pc, mic_track(&shared.borrow()).as_ref());
            }
            let Ok(value) = JsFuture::from(pc.create_answer()).await else {
                return;
            };
            if active(&weak, peer, &id).is_none() {
                return;
            }
            let Some(sdp) = super::browser::sdp(value) else {
                return;
            };
            let desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
            desc.set_sdp(&sdp);
            if JsFuture::from(pc.set_local_description(&desc))
                .await
                .is_ok()
            {
                announce(&weak, peer, &id, Signal::Answer(sdp));
            }
        });
    }
    impl Voice {
        pub(crate) fn new() -> Self {
            Self(Rc::new(RefCell::new(State {
                alive: true,
                me: None,
                next_call: 0,
                calls: HashMap::new(),
                outbox: Vec::new(),
                request: MicRequest::default(),
                asking: false,
                mic: None,
                error: None,
                context: None,
                volume: 1.0,
            })))
        }
        pub(crate) fn sync(&self, me: Option<PeerId>, peers: &HashSet<PeerId>) {
            let Some(me) = me else { return };
            let mut state = self.0.borrow_mut();
            state.me = Some(me);
            state.calls.retain(|id, _| peers.contains(id));
            state.outbox.retain(|(id, _)| peers.contains(id));
            let mut start = Vec::new();
            for &peer in peers {
                if me < peer && !state.calls.contains_key(&peer) {
                    state.next_call = state.next_call.wrapping_add(1);
                    start.push((peer, format!("{me}-{}", state.next_call)));
                }
            }
            drop(state);
            for (peer, id) in start {
                offer(&self.0, peer, id);
            }
        }
        pub(crate) fn hear(&self, peer: PeerId, bytes: &[u8]) {
            let Some(note) = Note::decode(bytes) else {
                return;
            };
            let Some(me) = self.0.borrow().me else {
                return;
            };
            if let Signal::Offer(sdp) = note.signal {
                // One deterministic offerer avoids glare and binds offers to their real sender.
                if peer < me && !self.0.borrow().calls.contains_key(&peer) {
                    answer(&self.0, peer, note.call, sdp);
                }
                return;
            }
            let weak = Rc::downgrade(&self.0);
            let Some(shared) = active(&weak, peer, &note.call) else {
                return;
            };
            let mut state = shared.borrow_mut();
            let Some(call) = state.calls.get_mut(&peer) else {
                return;
            };
            match note.signal {
                Signal::Answer(sdp) if me < peer && !call.described && !call.answer_pending => {
                    call.answer_pending = true;
                    let pc = call.pc.clone();
                    let id = note.call;
                    spawn_local(async move {
                        let desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
                        desc.set_sdp(&sdp);
                        if JsFuture::from(pc.set_remote_description(&desc))
                            .await
                            .is_ok()
                        {
                            described(&weak, peer, &id);
                        }
                    });
                }
                Signal::Ice(candidate) => {
                    if call.described {
                        add_ice(&call.pc, &candidate);
                    } else if call.early.len() < EARLY_MAX {
                        call.early.push(candidate);
                    }
                }
                _ => {}
            }
        }
        pub(crate) fn outbox(&self) -> Vec<(PeerId, Vec<u8>)> {
            std::mem::take(&mut self.0.borrow_mut().outbox)
        }
        /// Called in the player's click/tap stack, without requesting capture.
        pub(crate) fn listen(&self) {
            let mut state = self.0.borrow_mut();
            if state.context.is_none() {
                state.context = AudioContext::new().ok();
            }
            let Some(context) = state.context.clone() else {
                state.error = Some("Voice audio is unavailable in this browser.");
                return;
            };
            if let Ok(promise) = context.resume() {
                settle(promise);
            }
            let volume = state.volume;
            for call in state.calls.values_mut() {
                if call.playback.is_none()
                    && let Some(stream) = &call.stream
                {
                    call.playback = playback(&context, stream, volume);
                }
            }
        }
        pub(crate) fn volume(&self, volume: f32) {
            if !volume.is_finite() {
                return;
            }
            let mut state = self.0.borrow_mut();
            state.volume = volume.clamp(0.0, 1.0);
            for call in state.calls.values() {
                if let Some(playback) = &call.playback {
                    playback.gain.gain().set_value(state.volume);
                }
            }
        }
        pub(crate) fn mic(&self, on: bool) {
            if on {
                self.listen();
            }
            let mut state = self.0.borrow_mut();
            if on && (state.mic.is_some() || state.asking) {
                return;
            }
            let generation = state.request.set(on);
            state.error = None;
            if !on {
                state.asking = false;
                if let Some(stream) = state.mic.take() {
                    stop_stream(&stream);
                }
                for call in state.calls.values() {
                    plug(&call.pc, None);
                }
                return;
            }
            let Some(devices) = web_sys::window().and_then(|w| w.navigator().media_devices().ok())
            else {
                state.error = Some("Microphone unavailable. You can still listen.");
                return;
            };
            let audio = js_sys::Object::new();
            for key in ["echoCancellation", "noiseSuppression", "autoGainControl"] {
                let _ = Reflect::set(&audio, &JsValue::from_str(key), &JsValue::TRUE);
            }
            let constraints = MediaStreamConstraints::new();
            constraints.set_audio(&audio);
            let Ok(promise) = devices.get_user_media_with_constraints(&constraints) else {
                state.error = Some("Microphone unavailable. You can still listen.");
                return;
            };
            state.asking = true;
            let weak = Rc::downgrade(&self.0);
            drop(state);
            spawn_local(async move {
                let result = JsFuture::from(promise).await;
                let stream = result
                    .as_ref()
                    .ok()
                    .and_then(|s| s.clone().dyn_into::<MediaStream>().ok());
                let Some(shared) = weak.upgrade() else {
                    if let Some(stream) = &stream {
                        stop_stream(stream);
                    }
                    return;
                };
                let mut state = shared.borrow_mut();
                if !state.alive || !state.request.accepts(generation) {
                    if let Some(stream) = &stream {
                        stop_stream(stream);
                    }
                    return;
                }
                state.asking = false;
                if let Some(stream) = stream {
                    let track = stream
                        .get_audio_tracks()
                        .get(0)
                        .dyn_into::<MediaStreamTrack>()
                        .ok();
                    if track.is_none() {
                        stop_stream(&stream);
                        state.error = Some("No microphone found. You can still listen.");
                        return;
                    }
                    for call in state.calls.values() {
                        plug(&call.pc, track.as_ref());
                    }
                    state.mic = Some(stream);
                } else {
                    let name = result.err().and_then(|e| {
                        Reflect::get(&e, &JsValue::from_str("name"))
                            .ok()
                            .and_then(|s| s.as_string())
                    });
                    state.error = Some(match name.as_deref() {
                        Some("NotAllowedError" | "SecurityError") => {
                            "Microphone blocked. Allow it in your browser, then try again. You can still listen."
                        }
                        Some("NotFoundError") => "No microphone found. You can still listen.",
                        Some("NotReadableError" | "AbortError") => {
                            "Microphone busy. Close the other call or app, then try again."
                        }
                        _ => "Microphone unavailable. You can still listen.",
                    });
                }
            });
        }
        pub(crate) fn status(&self) -> String {
            let mut state = self.0.borrow_mut();
            if state.mic.is_some()
                && mic_track(&state)
                    .is_none_or(|track| track.ready_state() == MediaStreamTrackState::Ended)
            {
                if let Some(stream) = state.mic.take() {
                    stop_stream(&stream);
                }
                state.request.set(false);
                state.error = Some("Microphone stopped. Turn it on again when you are ready.");
                for call in state.calls.values() {
                    plug(&call.pc, None);
                }
            }
            let now = web_sys::window()
                .and_then(|w| w.performance())
                .map_or(0.0, |p| p.now() / 1000.0);
            let mut hearing = 0;
            for call in state.calls.values_mut() {
                if now - call.last_meter >= 0.25 {
                    call.last_meter = now;
                    if let Some(playback) = &call.playback {
                        let mut samples = vec![0.0; playback.meter.fft_size() as usize];
                        playback.meter.get_float_time_domain_data(&mut samples);
                        if samples.iter().any(|s| s.abs() > 0.001) {
                            call.last_heard = now;
                        }
                    }
                }
                if now - call.last_heard < 1.0 {
                    hearing += 1;
                }
            }
            serde_json::json!({
                "mic":if state.asking {"asking"} else if state.mic.is_some() {"on"} else {"off"},
                "error":state.error,
                "listening":state.context.as_ref().is_some_and(|c| c.state() == AudioContextState::Running),
                "volume":state.volume,
                "calls":state.calls.len(),
                "connected":state.calls.values().filter(|c|c.pc.connection_state()==RtcPeerConnectionState::Connected).count(),
                "hearing":hearing,
            }).to_string()
        }
    }
    impl Drop for Voice {
        fn drop(&mut self) {
            let mut state = self.0.borrow_mut();
            state.alive = false;
            state.request.set(false);
            if let Some(stream) = state.mic.take() {
                stop_stream(&stream);
            }
            state.calls.clear();
            state.outbox.clear();
            if let Some(context) = state.context.take()
                && let Ok(promise) = context.close()
            {
                settle(promise);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn microphone_is_off_by_default_and_late_permissions_cannot_restart_it() {
        let mut mic = MicRequest::default();
        assert!(!mic.accepts(0));
        let first = mic.set(true);
        assert!(mic.accepts(first));
        mic.set(false);
        assert!(!mic.accepts(first));
        let second = mic.set(true);
        assert!(!mic.accepts(first));
        assert!(mic.accepts(second));
        mic.set(false);
        assert!(!mic.accepts(second));
    }
    #[test]
    fn voice_notes_are_bounded_versioned_and_call_scoped() {
        let note = |call: &str, version: &str, signal| {
            serde_json::to_vec(&Note {
                version: version.into(),
                call: call.into(),
                signal,
            })
            .expect("fixture")
        };
        assert!(
            Note::decode(&note(
                "peer-1",
                super::super::VERSION,
                Signal::Offer("sdp".into())
            ))
            .is_some()
        );
        assert!(Note::decode(&note("peer-1", "other", Signal::Offer("sdp".into()))).is_none());
        assert!(
            Note::decode(&note(
                "bad/id",
                super::super::VERSION,
                Signal::Offer("sdp".into())
            ))
            .is_none()
        );
        assert!(
            Note::decode(&note(
                "peer-1",
                super::super::VERSION,
                Signal::Ice("x".repeat(ICE_MAX + 1))
            ))
            .is_none()
        );
        assert!(
            Note::decode(&note(
                "peer-1",
                super::super::VERSION,
                Signal::Answer("x".repeat(SDP_MAX + 1))
            ))
            .is_none()
        );
        assert!(Note::decode(&vec![b' '; NOTE_MAX + 1]).is_none());
        assert!(Note::decode(b"broken").is_none());
    }
}
