//! Explicit opt-in data-only room. No save payloads, voice or microphone.
use dragon_game::{adventure::Adventure, authority::Authority};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Connection(pub Option<Lobby>);
pub struct Lobby {
    transport: Transport,
    pub id: Option<String>,
    pub notice: String,
    rider: usize,
    peers: BTreeMap<String, Peer>,
    hello_at: f64,
    blocked: bool,
    pub party: dragon_game::party::Party,
    last_state: String,
    party_sent: f64,
    launch_hint: Option<(String, usize)>,
    visuals: BTreeMap<String, PartnerVisual>,
}
#[derive(Default)]
struct PartnerVisual {
    time: f64,
    followers: Vec<PartnerFollower>,
}
#[derive(Default)]
struct PartnerFollower {
    time: f64,
    position: Option<[f64; 2]>,
}
struct Peer {
    rider: Option<usize>,
    presence: Option<Value>,
    seen: f64,
    daddio: bool,
}
#[cfg(target_arch = "wasm32")]
struct Transport(dragon_peer::DragonPeer);
#[cfg(not(target_arch = "wasm32"))]
struct Transport {
    peer: dragon_peer::NativePeer,
    runtime: tokio::runtime::Runtime,
}
impl Transport {
    fn connect(code: &str) -> Result<Self, String> {
        let introducer = "wss://crabscape-introducer.fly.dev";
        let build = &dragon_peer::VERSION[..dragon_peer::VERSION.len().min(12)];
        #[cfg(target_arch = "wasm32")]
        {
            dragon_peer::DragonPeer::without_voice(introducer, build, code)
                .map(Self)
                .map_err(|_| "Could not open data-only room".into())
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;
            let (peer, pump) = {
                let _guard = runtime.enter();
                dragon_peer::NativePeer::connect(introducer, build, code).map_err(str::to_owned)?
            };
            runtime.spawn(async move {
                let _ = pump.await;
            });
            Ok(Self { peer, runtime })
        }
    }
    #[cfg(all(test, not(target_arch = "wasm32")))]
    fn connect_test(introducer: &str) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let (peer, pump) = {
            let _guard = runtime.enter();
            dragon_peer::NativePeer::connect_host_only(introducer, "valmora-test", "synthetic")
                .unwrap()
        };
        runtime.spawn(async move {
            let _ = pump.await;
        });
        Self { peer, runtime }
    }
    fn poll(&mut self) -> Value {
        #[cfg(target_arch = "wasm32")]
        {
            serde_json::from_str(&self.0.poll()).unwrap_or_else(|_| json!({"ended":true}))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _guard = self.runtime.enter();
            let poll = self.peer.poll();
            let events:Vec<Value> = poll.events.into_iter().map(|event|match event {
                dragon_peer::Event::Joined(peer)=>json!({"type":"joined","peer":peer.to_string()}),
                dragon_peer::Event::Left(peer)=>json!({"type":"left","peer":peer.to_string()}),
                dragon_peer::Event::Message(message)=>json!({"type":"message","peer":message.peer.to_string(),"topic":message.topic,"data":message.data}),
            }).collect();
            json!({"id":poll.id.map(|id|id.to_string()),"ended":poll.ended,"events":events})
        }
    }
    fn send(&mut self, topic: &str, data: Value) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            self.0.send(topic, &data.to_string(), topic != "presence")
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.peer.send(
                topic,
                data,
                if topic == "presence" {
                    dragon_peer::Delivery::Unreliable
                } else {
                    dragon_peer::Delivery::Reliable
                },
            )
        }
    }
}
impl Lobby {
    pub fn connect(code: &str, rider: usize) -> Result<Self, String> {
        if rider >= 6 {
            return Err("Invalid rider".into());
        }
        Ok(Self {
            transport: Transport::connect(code)?,
            id: None,
            notice: "Connecting…".into(),
            rider,
            peers: BTreeMap::new(),
            hello_at: f64::NEG_INFINITY,
            blocked: false,
            party: Default::default(),
            last_state: String::new(),
            party_sent: f64::NEG_INFINITY,
            launch_hint: None,
            visuals: BTreeMap::new(),
        })
    }
    pub fn connected_count(&self) -> usize {
        self.peers
            .values()
            .filter(|peer| peer.rider.is_some())
            .count()
    }
    pub fn poll(&mut self, adventure: &mut Adventure, now: f64) {
        if self.blocked {
            return;
        }
        let packet = self.transport.poll();
        self.id = packet["id"].as_str().map(str::to_owned);
        if packet["ended"] == true {
            self.blocked = true;
            self.notice = "Connection ended. Leave the room to continue offline.".into();
            return;
        }
        if let Some(events) = packet["events"].as_array() {
            for event in events {
                let Some(peer) = event["peer"].as_str() else {
                    continue;
                };
                match event["type"].as_str() {
                    Some("joined") => {
                        if self.peers.len() < 5 {
                            self.peers.entry(peer.into()).or_insert(Peer {
                                rider: None,
                                presence: None,
                                seen: now,
                                daddio: false,
                            });
                        }
                        self.hello_at = f64::NEG_INFINITY;
                    }
                    Some("left") => {
                        self.peers.remove(peer);
                        if let Some(session) = &mut adventure.shared {
                            session.disconnect(peer);
                        }
                    }
                    Some("message") => {
                        let data = &event["data"];
                        let topic = event["topic"].as_str().unwrap_or("");
                        if topic == "hello" {
                            let Some(rider) =
                                data["r"].as_u64().filter(|r| *r < 6).map(|r| r as usize)
                            else {
                                continue;
                            };
                            if rider == self.rider
                                || self
                                    .peers
                                    .iter()
                                    .any(|(id, p)| id != peer && p.rider == Some(rider))
                            {
                                self.blocked = true;
                                self.notice="Two players selected the same rider slot. Leave and choose different riders.".into();
                                continue;
                            }
                            if let Some(binding) = self.peers.get_mut(peer) {
                                if binding.rider.is_some_and(|old| old != rider) {
                                    continue;
                                }
                                binding.rider = Some(rider);
                                binding.seen = now;
                                binding.daddio = data["build"]["name"]
                                    .as_str()
                                    .is_some_and(|name| name.trim().eq_ignore_ascii_case("daddio"));
                                if let Some(session) = &mut adventure.shared {
                                    let _ = session.bind_peer(peer.into(), rider, binding.daddio);
                                }
                            }
                        } else if let Some(binding) =
                            self.peers.get_mut(peer).filter(|p| p.rider.is_some())
                        {
                            if topic == "presence"
                                && data["r"].as_u64() == binding.rider.map(|r| r as u64)
                            {
                                if let (Some(mission), Some(at), Some(state)) = (
                                    data["pt"]["m"].as_str(),
                                    data["pt"]["at"].as_f64(),
                                    data["s"].as_str(),
                                ) {
                                    self.party.receive_pick(mission, at, state);
                                }
                                binding.presence = Some(data.clone());
                                binding.seen = now;
                            }
                            if let Some(session) = &mut adventure.shared {
                                session.receive(peer, topic, data, now);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if self.id.is_some() && now - self.hello_at >= 1. {
            self.transport.send(
                "hello",
                json!({"r":self.rider,"build":adventure.profile()["build"]}),
            );
            if adventure.game().is_none() && adventure.state() != "party" {
                self.transport.send("presence", json!({"r":self.rider,"s":adventure.state(),"x":0.,"y":0.,"ms":null,"cs":[0,""]}));
            }
            self.hello_at = now;
        }
        if !self.blocked {
            self.notice = if self.id.is_none() {
                "Connecting…".into()
            } else {
                format!("Data-only room: {} other rider(s)", self.connected_count())
            };
        }
    }
    /// Called after local launch, before its first simulation frame.
    pub fn attach_flight(&mut self, adventure: &mut Adventure, now_ms: u64) -> Result<(), String> {
        if self.blocked {
            return Err(self.notice.clone());
        }
        if adventure.shared.is_some() || adventure.state() != "play" {
            return Ok(());
        }
        let peer = self.id.clone().ok_or("Room is still connecting")?;
        let mission = &adventure.game().ok_or("No flight")?.definition.id;
        let candidate = self
            .peers
            .iter()
            .filter_map(|(peer, p)| {
                if now_ms as f64 / 1000. - p.seen > 6. {
                    return None;
                }
                let presence = p.presence.as_ref()?;
                if !matches!(presence["s"].as_str(), Some("play" | "pause")) || presence["o"] == 1 {
                    return None;
                }
                let ms = presence["ms"].as_array()?;
                if ms.len() != 7 || ms[0].as_str() != Some(mission) || ms[5].as_u64() != Some(1) {
                    return None;
                }
                let run = ms[4].as_str()?;
                if run.is_empty() || run.encode_utf16().count() > 12 {
                    return None;
                }
                Some((peer.clone(), p.rider?, run.to_owned(), ms[6].as_u64()?))
            })
            .min_by(|a, b| b.3.cmp(&a.3).then_with(|| a.0.cmp(&b.0)));
        let (run, authority) = if let Some((host, rider, run, epoch)) = candidate {
            (
                run,
                Authority {
                    guest: true,
                    host_peer: Some(host),
                    host_rider: rider,
                    epoch,
                },
            )
        } else if let Some((host, rider)) = self.launch_hint.take() {
            (
                String::new(),
                Authority {
                    guest: true,
                    host_peer: Some(host),
                    host_rider: rider,
                    epoch: 0,
                },
            )
        } else {
            (
                format!("{:x}", now_ms & 0xffffffffff),
                Authority {
                    guest: false,
                    host_peer: None,
                    host_rider: self.rider,
                    epoch: 0,
                },
            )
        };
        adventure
            .share_flight(peer, self.rider, run, authority)
            .map_err(|e| format!("{e:?}"))?;
        if let Some(session) = &mut adventure.shared {
            for (peer, p) in &self.peers {
                if let Some(rider) = p.rider {
                    session
                        .bind_peer(peer.clone(), rider, p.daddio)
                        .map_err(str::to_owned)?;
                    if let Some(presence) = &p.presence {
                        session.receive(peer, "presence", presence, now_ms as f64 / 1000.);
                    }
                }
            }
        }
        Ok(())
    }
    pub fn flush(&mut self, adventure: &mut Adventure) {
        if self.blocked {
            return;
        }
        for mut packet in adventure.drain_network_outbound() {
            if packet.topic == "presence" {
                if adventure.gate.frozen.is_some() {
                    packet.data["s"] = json!("frozen");
                }
                let profile = adventure.profile();
                packet.data["build"] = profile["build"].clone();
                packet.data["p"] = Value::Array(
                    profile["party"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(5)
                        .map(|dragon| {
                            let level = dragon_game::level_of(dragon["xp"].as_f64().unwrap_or(0.));
                            let stage = match level {
                                12.. => 3,
                                8.. => 2,
                                4.. => 1,
                                _ => 0,
                            };
                            json!([dragon["kind"], dragon["color"], stage])
                        })
                        .collect(),
                );
            }
            if !self.transport.send(packet.topic, packet.data) {
                self.notice = "A room update could not be sent".into();
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use dragon_game::{
        adventure::Clock,
        flight::{Input, Viewport},
    };
    fn adventure(rider: usize) -> Adventure {
        let key = if rider == 0 { "alexa" } else { "tavian" };
        let profile=dragon_game::profile::normalize_profile(&json!({"rid":format!("fixture-{rider}"),"seenIntro":true,"briefed":["m1"],"build":{"name":"Fixture","dname":"Ember","path":"forge"}}),key,rider,||"extra".into()).unwrap();
        Adventure::new(profile, vec![], view(), 42, rider)
            .unwrap()
            .0
    }
    fn view() -> Viewport {
        Viewport::from_screen(1000., 700., 1., 40., 0.62)
    }
    fn lobby(url: &str, rider: usize) -> Lobby {
        Lobby {
            transport: Transport::connect_test(url),
            id: None,
            notice: String::new(),
            rider,
            peers: BTreeMap::new(),
            hello_at: f64::NEG_INFINITY,
            blocked: false,
            party: Default::default(),
            last_state: String::new(),
            party_sent: f64::NEG_INFINITY,
            launch_hint: None,
            visuals: BTreeMap::new(),
        }
    }
    #[test]
    #[ignore = "explicit local-only signaling/WebRTC test, no GUI, STUN, microphone or saves"]
    fn two_real_clients_join_one_adventure_and_keep_local_profiles() {
        let server_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let admitted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = admitted.clone();
        let mut server = matchbox_signaling::SignalingServer::full_mesh_builder(
            std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
        )
        .on_peer_connected(move |_| {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .build();
        let addr = {
            let _guard = server_runtime.enter();
            server.bind().unwrap()
        };
        server_runtime.spawn(async move {
            server.serve().await.unwrap();
        });
        let url = format!("ws://{addr}");
        let mut a = lobby(&url, 0);
        // Test joining an established room. Upstream full_mesh's add_peer clones
        // then inserts under separate locks, so simultaneous first arrivals can
        // both miss the other's announcement. Wait for actual server admission.
        let first_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while admitted.load(std::sync::atomic::Ordering::SeqCst) < 1 {
            assert!(
                std::time::Instant::now() < first_deadline,
                "first signaling admission timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let mut b = lobby(&url, 1);
        let mut ga = adventure(0);
        let mut gb = adventure(1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut now = 1000u64;
        loop {
            a.poll(&mut ga, now as f64 / 1000.);
            b.poll(&mut gb, now as f64 / 1000.);
            if a.connected_count() == 1 && b.connected_count() == 1 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "peer admission timed out: {} / {}; transport peers {} / {}; ids {:?} / {:?}",
                a.notice,
                b.notice,
                a.peers.len(),
                b.peers.len(),
                a.id,
                b.id
            );
            now += 20;
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        ga.to_party().unwrap();
        gb.to_party().unwrap();
        a.tick_party(&mut ga, 0., now).unwrap();
        b.tick_party(&mut gb, 0., now).unwrap();
        a.party.pick("m1", now as f64);
        loop {
            now += 20;
            a.poll(&mut ga, now as f64 / 1000.);
            b.poll(&mut gb, now as f64 / 1000.);
            if b.party.mission.as_deref() == Some("m1") && b.party.ready.is_none() {
                b.party.toggle_ready();
            }
            a.tick_party(&mut ga, 0.02, now).unwrap();
            b.tick_party(&mut gb, 0.02, now).unwrap();
            a.attach_flight(&mut ga, now).unwrap();
            b.attach_flight(&mut gb, now).unwrap();
            for (lobby, game) in [(&mut a, &mut ga), (&mut b, &mut gb)] {
                game.tick(
                    0.02,
                    Input::default(),
                    view(),
                    Clock {
                        now_ms: now,
                        day: 0,
                    },
                )
                .unwrap();
                lobby.flush(game);
            }
            if ga.shared.is_some() && gb.shared.is_some() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "party launch timed out: {:?} / {:?}",
                a.party,
                b.party
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(gb.shared.as_ref().unwrap().authority().guest);
        for _ in 0..100 {
            now += 20;
            a.poll(&mut ga, now as f64 / 1000.);
            b.poll(&mut gb, now as f64 / 1000.);
            ga.tick(
                0.02,
                Input::default(),
                view(),
                Clock {
                    now_ms: now,
                    day: 0,
                },
            )
            .unwrap();
            gb.tick(
                0.02,
                Input::default(),
                view(),
                Clock {
                    now_ms: now,
                    day: 0,
                },
            )
            .unwrap();
            a.flush(&mut ga);
            b.flush(&mut gb);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            ga.shared.as_ref().unwrap().run(),
            gb.shared.as_ref().unwrap().run()
        );
        assert!(!ga.shared.as_ref().unwrap().authority().guest);
        assert!(gb.shared.as_ref().unwrap().authority().guest);
        assert_eq!(ga.profile()["rid"], "fixture-0");
        assert_eq!(gb.profile()["rid"], "fixture-1");
        assert!(!gb.shared.as_ref().unwrap().ghosts.is_empty());
        drop(a);
        for _ in 0..220 {
            now += 20;
            b.poll(&mut gb, now as f64 / 1000.);
            gb.tick(
                0.02,
                Input::default(),
                view(),
                Clock {
                    now_ms: now,
                    day: 0,
                },
            )
            .unwrap();
            b.flush(&mut gb);
            if !gb.shared.as_ref().unwrap().authority().guest {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            !gb.shared.as_ref().unwrap().authority().guest,
            "remaining rider must take over after host leaves"
        );
        assert_eq!(gb.profile()["rid"], "fixture-1");
    }
}

impl Lobby {
    /// Presentation clocks advance once per Update, never once per egui pass.
    pub fn tick_visuals(
        &mut self,
        dt: f64,
        view: dragon_game::flight::Viewport,
        session: &dragon_game::coop::Session,
    ) {
        self.visuals
            .retain(|peer, _| session.ghosts.contains_key(peer));
        for (peer, ghost) in &session.ghosts {
            if ghost.mission.as_deref() != Some(&session.game.definition.id) {
                continue;
            }
            let Some(position) = ghost.position else {
                continue;
            };
            let Some(presence) = self.peers.get(peer).and_then(|p| p.presence.as_ref()) else {
                continue;
            };
            let count = presence["p"]
                .as_array()
                .map_or(0, |a| a.len().min(5).saturating_sub(1));
            let visual = self.visuals.entry(peer.clone()).or_default();
            visual.update(dt, view.scale, position, ghost.facing as f64, count);
        }
    }

    pub fn paint_partners(
        &self,
        painter: &bevy_egui::egui::Painter,
        session: &dragon_game::coop::Session,
        time: f64,
        view: dragon_game::flight::Viewport,
        zoom: f64,
    ) {
        use dragon_game::drawing::{Canvas, DragonArt, RiderArt, draw_dragon};
        let mut canvas = Canvas::default();
        for (peer, ghost) in &session.ghosts {
            if ghost.mission.as_deref() != Some(&session.game.definition.id) {
                continue;
            }
            let Some(position) = ghost.position else {
                continue;
            };
            let Some(presence) = self.peers.get(peer).and_then(|p| p.presence.as_ref()) else {
                continue;
            };
            let Ok(build) = dragon_game::profile::clean_build(&presence["build"]) else {
                continue;
            };
            let lead = &presence["p"][0];
            let stage = lead[2].as_u64().unwrap_or(0).min(3) as usize;
            let color = lead[1]
                .as_str()
                .filter(|s| {
                    s.len() == 7
                        && s.starts_with('#')
                        && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
                })
                .unwrap_or("#7f88aa");
            let text = |key: &str| build[key].as_str().unwrap_or("").to_owned();
            let size = view.scale * [0.62, 0.8, 1., 1.18][stage];
            canvas.scale(zoom, zoom);
            canvas.alpha(if ghost.status == "play" { 0.95 } else { 0.35 });
            if let Some(visual) = self.visuals.get(peer) {
                for (idx, follower) in visual.followers.iter().enumerate() {
                    let Some([x, y]) = follower.position else {
                        continue;
                    };
                    let member = &presence["p"][idx + 1];
                    let stage = member[2].as_u64().unwrap_or(0).min(3) as usize;
                    let horn = match member[0].as_str().unwrap_or("bond") {
                        "flame" => "#ffd0a0",
                        "spark" => "#fff27a",
                        "boulder" => "#d8c6a8",
                        "frost" => "#e6fbff",
                        "star" => "#fff8d8",
                        _ => "#ffe7a6",
                    };
                    let color = member[1]
                        .as_str()
                        .filter(|s| {
                            s.len() == 7
                                && s.starts_with('#')
                                && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
                        })
                        .unwrap_or("#ff6a3d");
                    draw_dragon(
                        &mut canvas,
                        x,
                        y,
                        view.scale * 0.7 * [0.62, 0.8, 1., 1.18][stage],
                        color,
                        ghost.facing as f64,
                        follower.time * 1.3,
                        &DragonArt {
                            horn: Some(horn.into()),
                            ..Default::default()
                        },
                    );
                }
            }
            let dad = self.peers.get(peer).is_some_and(|p| p.daddio);
            draw_dragon(
                &mut canvas,
                position[0],
                position[1],
                size,
                color,
                ghost.facing as f64,
                if ghost.status == "frozen" {
                    0.4
                } else {
                    self.visuals.get(peer).map_or(time, |v| v.time)
                },
                &DragonArt {
                    rider: ghost.riding.then(|| RiderArt {
                        hair: text("hair"),
                        skin: text("skin"),
                        cloak: text("cloak"),
                        style: text("style"),
                        path: text("path"),
                        crown: build["crown"] == true,
                        daddio: dad,
                    }),
                    dad,
                    ..Default::default()
                },
            );
            canvas.alpha(1.);
            if ghost.shield || ghost.status == "frozen" {
                canvas.fill_style("rgba(168,228,255,0.25)");
                canvas.stroke_style("rgba(225,246,255,0.8)");
                canvas.width(2.);
                canvas.begin();
                canvas.arc(
                    position[0],
                    position[1] - size * 0.1,
                    size * 1.1,
                    0.,
                    std::f64::consts::TAU,
                );
                canvas.fill();
                canvas.stroke();
            }
            super::art::paint(painter, &canvas);
            canvas.reset();
            let name = text("name");
            let label = if ghost.out {
                format!("{name} retired")
            } else if ghost.status == "pause" {
                format!("{name} paused")
            } else {
                name
            };
            painter.text(
                bevy_egui::egui::pos2(
                    (position[0] * zoom) as f32,
                    ((position[1] + size * 0.62 + view.scale * 0.1) * zoom) as f32,
                ),
                bevy_egui::egui::Align2::CENTER_TOP,
                label,
                bevy_egui::egui::FontId::proportional(
                    ((view.scale * 0.24).round().max(11.) * zoom) as f32,
                ),
                bevy_egui::egui::Color32::WHITE,
            );
        }
        super::art::paint(painter, &canvas);
    }
}

impl Lobby {
    pub fn launch_party(
        &mut self,
        adventure: &mut Adventure,
        mission: &str,
    ) -> Result<Vec<dragon_game::adventure::Event>, String> {
        self.party.enter();
        self.launch_hint = self
            .peers
            .iter()
            .filter_map(|(peer, p)| {
                let presence = p.presence.as_ref()?;
                (presence["s"] == "party" && presence["pt"]["rdy"] == mission)
                    .then_some((peer.clone(), p.rider?))
            })
            .filter(|(_, rider)| *rider < self.rider)
            .min_by_key(|(_, rider)| *rider);
        adventure
            .choose_mission(mission, true)
            .map_err(|e| format!("{e:?}"))
    }
    pub fn tick_party(
        &mut self,
        adventure: &mut Adventure,
        dt: f64,
        now_ms: u64,
    ) -> Result<Vec<dragon_game::adventure::Event>, String> {
        if adventure.state() != self.last_state {
            if adventure.state() == "party" {
                self.party.enter();
            }
            self.last_state = adventure.state().into();
        }
        if self.blocked || adventure.state() != "party" {
            return Ok(vec![]);
        }
        let peers: Vec<_> = self
            .peers
            .values()
            .filter_map(|p| {
                let presence = p.presence.as_ref()?;
                let state = presence["s"].as_str()?;
                if matches!(state, "menu" | "loading") {
                    return None;
                }
                Some(dragon_game::party::Peer {
                    state,
                    mission: presence["ms"][0].as_str(),
                    ready: presence["pt"]["rdy"].as_str(),
                })
            })
            .collect();
        let ready = self.party.all_ready(&peers);
        let launch = self
            .party
            .tick(dt.clamp(0., 0.05), adventure.state(), ready);
        if let Some(mission) = launch {
            return self.launch_party(adventure, &mission);
        }
        let now = now_ms as f64 / 1000.;
        if now - self.party_sent >= 0.08 {
            self.transport.send("presence",json!({"r":self.rider,"s":"party","x":0.,"y":0.,"ms":null,"cs":[0,""],"pt":{"m":self.party.mission,"at":self.party.picked_at,"rdy":self.party.ready}}));
            self.party_sent = now;
        }
        Ok(vec![])
    }
}

impl PartnerVisual {
    fn update(&mut self, dt: f64, scale: f64, position: [f64; 2], facing: f64, count: usize) {
        self.time += dt;
        self.followers
            .resize_with(count.min(4), PartnerFollower::default);
        for (idx, f) in self.followers.iter_mut().enumerate() {
            f.time += dt;
            let target = [
                position[0] - facing * scale * (1.25 + idx as f64 * 0.85),
                position[1] + scale * 0.3 + (f.time * 2.2 + idx as f64).sin() * scale * 0.18,
            ];
            let p = f.position.get_or_insert(target);
            for axis in 0..2 {
                p[axis] += (target[axis] - p[axis]) * (dt * 4.).min(1.);
            }
        }
    }
}
#[cfg(test)]
mod partner_visual_tests {
    use super::PartnerVisual;
    #[test]
    fn followers_use_source_offsets_then_smooth_and_cap_at_four() {
        let mut visual = PartnerVisual::default();
        visual.update(0., 40., [300., 200.], 1., 20);
        assert_eq!(visual.followers.len(), 4);
        assert_eq!(visual.followers[0].position, Some([250., 212.]));
        visual.update(0.1, 40., [400., 200.], 1., 4);
        let p = visual.followers[0].position.expect("initialized");
        assert!((p[0] - 290.).abs() < 1e-9);
        assert!((p[1] - (212. + 0.22_f64.sin() * 40. * 0.18 * 0.4)).abs() < 1e-9);
        visual.update(0., 40., [400., 200.], 1., 1);
        assert_eq!(visual.followers.len(), 1);
        assert_eq!(visual.time, 0.1);
    }
}

#[cfg(test)]
mod partner_source_oracle {
    use super::PartnerVisual;
    use serde_json::Value;
    #[test]
    #[ignore = "requires macOS JavaScriptCore CLI; synthetic follower positions only"]
    fn follower_motion_matches_original_update_ghosts() {
        let source = include_str!("../../web/dragon/index.html");
        let tail = &source[source.find("function updateGhosts(dt)").unwrap()..];
        let function = &tail[..tail.find("\nfunction drawGhosts()").unwrap()];
        let script = format!(
            r#"
const W=1000,S=48,topLimit=80,groundY=600;
const g={{viaDb:false,x:300,y:200,tx:0.3,ty:(200-80)/520,t:0,facing:1,follow:Array.from({{length:4}},()=>({{x:null,y:null,t:0}}))}};
const ghosts=new Map([['peer',g]]);
{function}
const result=[];
for(let i=0;i<120;i++){{const dt=[0,1/120,1/60,0.1,0.4][i%5];g.tx=(300+i*2)/W;g.ty=(200+Math.sin(i*0.1)*50-topLimit)/(groundY-topLimit);g.facing=i<60?1:-1;updateGhosts(dt);result.push({{position:[g.x,g.y],facing:g.facing,dt,time:g.t,followers:g.follow.map(f=>[f.x,f.y,f.t])}});}}
print(JSON.stringify(result));
"#
        );
        use std::process::Command;
        let out = Command::new(
            "/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc",
        )
        .args(["-e", &script])
        .output()
        .unwrap();
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let frames: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
        let mut visual = PartnerVisual::default();
        for frame in frames {
            let n = |v: &Value| v.as_f64().unwrap();
            visual.update(
                n(&frame["dt"]),
                48.,
                [n(&frame["position"][0]), n(&frame["position"][1])],
                n(&frame["facing"]),
                4,
            );
            assert!((visual.time - n(&frame["time"])).abs() < 1e-9);
            for (f, want) in visual
                .followers
                .iter()
                .zip(frame["followers"].as_array().unwrap())
            {
                let p = f.position.unwrap();
                for (actual, expected) in [p[0], p[1], f.time]
                    .into_iter()
                    .zip(want.as_array().unwrap())
                {
                    assert!(
                        (actual - n(expected)).abs() < 1e-9,
                        "follower state diverged: {actual} != {expected}"
                    );
                }
            }
        }
        assert_eq!(visual.followers.len(), 4);
    }
}
