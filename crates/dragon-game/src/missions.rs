//! Journey/wave coordinator ported from web/dragon/index.html.
//! World callbacks are synchronous: subsequent snapshots must reflect spawn,
//! defeat and objective effects. Objective geometry, rewards/endMission, host
//! election and presentation are adapter responsibilities, not implemented here.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MissionDef {
    pub id: String,
    pub name: String,
    pub realm: usize,
    pub boss: Option<String>,
    pub waves: Vec<Vec<(String, usize)>>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Leg {
    #[default]
    Fly,
    Fight,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Mission {
    pub wave: i32,
    pub stop: usize,
    pub leg: Leg,
    pub queue: VecDeque<String>,
    pub total: usize,
    pub kills: usize,
    pub spawn_t: f64,
    pub banner_t: f64,
    pub done: bool,
    pub end_t: f64,
    pub fail_t: f64,
    pub t: f64,
    pub guest: bool,
    pub phase: u8,
    pub carry: f64,
    pub banked: f64,
    pub leg_t: f64,
    pub leg_max: f64,
    pub leg_from: f64,
    pub prog: f64,
    pub trick_t: f64,
    pub ob_nag: bool,
    pub gem_timer: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    ClearObjective,
    StartBanner,
    ArrivalCaption(usize),
    WaveBanner {
        boss: Option<String>,
        wave: usize,
    },
    MakeObjective(usize),
    Spawn {
        kind: String,
        extra: bool,
        at: Option<[f64; 2]>,
    },
    MoveGems(f64),
    Bank(f64),
    CalmEnemies,
    ClearHazardsAndStrikes,
    TravelCaption {
        from: usize,
        to: usize,
    },
    Complete,
    Failed,
    End(bool),
    SpawnGem,
    ReconcileHost,
    BecomeHost,
    DefeatBoss,
    BossPhase(u8),
    SpeedBoss(f64),
    UpdateObjective(f64),
    CarryNag(bool),
}
#[derive(Clone, Debug, Default)]
pub struct BossSnapshot {
    pub hp: f64,
    pub max_hp: f64,
}
#[derive(Clone, Debug)]
pub struct WorldSnapshot {
    pub player_out: bool,
    pub active_others: usize,
    pub stale_world_seconds: f64,
    pub static_mode: bool,
    /// JS intentionally counts dead enemies here until they leave the array.
    pub regular_uncalm: usize,
    pub extra_alive_uncalm: usize,
    pub gems: usize,
    pub boss: Option<BossSnapshot>,
    pub sibling_boss_damage: f64,
    pub sibling_kills: usize,
    pub carry_pending: bool,
    pub carrier_present: bool,
    pub objective_failed: bool,
    pub width: f64,
    pub scale: f64,
    pub top_limit: f64,
    pub sky_bottom: f64,
    pub scroll_v: f64,
}
impl Default for WorldSnapshot {
    fn default() -> Self {
        Self {
            player_out: false,
            active_others: 0,
            stale_world_seconds: 0.,
            static_mode: false,
            regular_uncalm: 0,
            extra_alive_uncalm: 0,
            gems: 0,
            boss: None,
            sibling_boss_damage: 0.,
            sibling_kills: 0,
            carry_pending: false,
            carrier_present: false,
            objective_failed: false,
            width: 1000.,
            scale: 20.,
            top_limit: 0.,
            sky_bottom: 600.,
            scroll_v: 0.,
        }
    }
}
pub trait MissionWorld {
    fn snapshot(&self) -> WorldSnapshot;
    fn apply(&mut self, effect: Effect);
    /// Override for spawning: consume the same draw stream as the coordinator.
    fn apply_random(&mut self, effect: Effect, _draw: &mut impl FnMut() -> f64) {
        self.apply(effect);
    }
}
fn boss(k: &str) -> bool {
    matches!(
        k,
        "thunderhead" | "chieftain" | "cavetroll" | "serpent" | "bigslime" | "swallower"
    )
}
fn sea(k: &str) -> bool {
    matches!(k, "jelly" | "leaper")
}
fn rand(rng: &mut impl FnMut() -> f64, a: f64, b: f64) -> f64 {
    a + rng() * (b - a)
}
impl Mission {
    pub fn new() -> Self {
        Self {
            wave: -1,
            gem_timer: 2.,
            ..Self::default()
        }
    }
    pub fn start_leg(
        &mut self,
        def: &MissionDef,
        i: usize,
        first: bool,
        w: &mut impl MissionWorld,
    ) {
        assert!(i < def.waves.len());
        self.leg = Leg::Fly;
        self.stop = i;
        self.wave = i as i32 - 1;
        self.total = 0;
        self.kills = 0;
        self.queue.clear();
        self.ob_nag = false;
        w.apply(Effect::ClearObjective);
        self.leg_max = if first {
            if i > 0 { 6. } else { 8. }
        } else {
            15.
        };
        self.leg_t = self.leg_max;
        self.leg_from = i as f64 / def.waves.len() as f64;
        self.prog = self.leg_from;
        self.trick_t = if first { 4. } else { 2.5 };
        if first {
            self.banner_t = 2.2;
            w.apply(Effect::StartBanner);
        }
    }
    pub fn next_wave(
        &mut self,
        def: &MissionDef,
        w: &mut impl MissionWorld,
        rng: &mut impl FnMut() -> f64,
    ) {
        self.wave += 1;
        let mut list = Vec::new();
        for (kind, n) in &def.waves[self.wave as usize] {
            for _ in 0..*n {
                list.push(if kind == "boss" {
                    def.boss.as_ref().expect("boss wave has boss").clone()
                } else {
                    kind.clone()
                });
            }
        }
        let boss_type = list.iter().find(|k| boss(k)).cloned();
        self.total = list.len();
        list.retain(|k| !boss(k));
        for i in (1..list.len()).rev() {
            let j = (rng() * (i + 1) as f64).floor() as usize;
            list.swap(i, j);
        }
        if let Some(k) = &boss_type {
            list.push(k.clone());
        }
        self.queue = list.into();
        self.kills = 0;
        self.spawn_t = if self.wave == 0 { 1.8 } else { 1.4 };
        self.banner_t = 2.2;
        w.apply(Effect::WaveBanner {
            boss: boss_type,
            wave: self.wave as usize,
        });
    }
    pub fn arrive_stop(
        &mut self,
        def: &MissionDef,
        w: &mut impl MissionWorld,
        rng: &mut impl FnMut() -> f64,
    ) {
        self.leg = Leg::Fight;
        self.prog = (self.stop + 1) as f64 / def.waves.len() as f64;
        self.next_wave(def, w, rng);
        w.apply(Effect::MakeObjective(self.stop));
        w.apply(Effect::ArrivalCaption(self.stop));
    }
    pub fn bank_gems(&mut self, w: &mut impl MissionWorld) {
        if self.carry > 0. {
            let n = self.carry;
            self.banked += n;
            w.apply(Effect::Bank(n));
            self.carry = 0.;
        }
    }
    pub fn fail(&mut self, w: &mut impl MissionWorld) {
        if self.done || self.fail_t != 0. {
            return;
        }
        self.fail_t = 1.6;
        self.banner_t = 1.6;
        w.apply(Effect::Failed);
    }
    pub fn wave_complete(
        &mut self,
        def: &MissionDef,
        w: &mut impl MissionWorld,
        rng: &mut impl FnMut() -> f64,
    ) {
        self.bank_gems(w);
        w.apply(Effect::CalmEnemies);
        w.apply(Effect::ClearHazardsAndStrikes);
        if (self.wave + 1) < def.waves.len() as i32 {
            let _speech = rng();
            w.apply(Effect::TravelCaption {
                from: self.wave as usize,
                to: (self.wave + 1) as usize,
            });
            self.start_leg(def, (self.wave + 1) as usize, false, w);
        } else {
            self.done = true;
            self.end_t = 3.4;
            self.banner_t = 1.8;
            w.apply(Effect::Complete);
        }
    }
    pub fn update_leg(
        &mut self,
        dt: f64,
        def: &MissionDef,
        w: &mut impl MissionWorld,
        rng: &mut impl FnMut() -> f64,
    ) {
        self.leg_t -= dt;
        self.prog = self.leg_from
            + ((self.stop + 1) as f64 / def.waves.len() as f64 - self.leg_from)
                * (1. - self.leg_t / self.leg_max).clamp(0., 1.);
        self.trick_t -= dt;
        if self.trick_t <= 0. {
            self.trick_t = rand(rng, 2.6, 4.);
            let wave = &def.waves[self.stop];
            let wave = if wave.iter().any(|(k, _)| k == "boss") {
                &def.waves[0]
            } else {
                wave
            };
            let kinds: Vec<_> = wave
                .iter()
                .filter(|(k, _)| !boss(k) && k != "boss")
                .map(|(k, _)| k)
                .collect();
            let s = w.snapshot();
            if !kinds.is_empty() && self.leg_t > 3. && s.extra_alive_uncalm < 3 {
                let kind = kinds[(rng() * kinds.len() as f64).floor() as usize].clone();
                let at = if sea(&kind) {
                    None
                } else {
                    Some([
                        s.width + s.scale,
                        rand(
                            rng,
                            s.top_limit + s.scale,
                            (s.top_limit + s.scale * 1.5).max(s.sky_bottom - s.scale),
                        ),
                    ])
                };
                w.apply_random(
                    Effect::Spawn {
                        kind,
                        extra: true,
                        at,
                    },
                    rng,
                );
            }
        }
        w.apply(Effect::MoveGems(-w.snapshot().scroll_v * 0.35 * dt));
        if self.leg_t <= 0. {
            self.arrive_stop(def, w, rng);
        }
    }
    fn gems(&mut self, dt: f64, w: &mut impl MissionWorld, rng: &mut impl FnMut() -> f64) {
        self.gem_timer -= dt;
        if self.gem_timer <= 0. {
            if w.snapshot().gems < 4 {
                w.apply_random(Effect::SpawnGem, rng);
            }
            self.gem_timer = rand(rng, 2., 3.2);
        }
    }
    pub fn step(
        &mut self,
        dt: f64,
        def: &MissionDef,
        w: &mut impl MissionWorld,
        rng: &mut impl FnMut() -> f64,
    ) {
        if !self.done && self.fail_t == 0. {
            self.t += dt;
        }
        if self.banner_t > 0. {
            self.banner_t -= dt;
        }
        if self.fail_t > 0. {
            self.fail_t -= dt;
            if self.fail_t <= 0. {
                w.apply(Effect::End(false));
            }
            return;
        }
        if self.done {
            self.end_t -= dt;
            if self.end_t <= 0. {
                self.bank_gems(w);
                w.apply(Effect::End(true));
            }
            return;
        }
        let s = w.snapshot();
        if !self.guest && s.player_out && s.active_others == 0 {
            self.fail(w);
            return;
        }
        if self.guest {
            if s.stale_world_seconds > 3.5 {
                w.apply(if s.static_mode {
                    Effect::ReconcileHost
                } else {
                    Effect::BecomeHost
                });
            }
            self.gems(dt, w, rng);
            return;
        }
        if self.leg == Leg::Fly {
            self.update_leg(dt, def, w, rng);
            self.gems(dt, w, rng);
            return;
        }
        if !self.queue.is_empty() {
            self.spawn_t -= dt;
            if self.spawn_t <= 0. && s.regular_uncalm < 9 {
                let kind = self.queue.pop_front().expect("nonempty queue");
                let bat = kind == "bat";
                w.apply_random(
                    Effect::Spawn {
                        kind,
                        extra: false,
                        at: None,
                    },
                    rng,
                );
                self.spawn_t = if bat { 0.35 } else { rand(rng, 0.7, 1.2) };
            }
        }
        let s = w.snapshot();
        if let Some(b) = s.boss {
            if b.hp - s.sibling_boss_damage <= 0. {
                w.apply(Effect::DefeatBoss);
            } else {
                let left = (b.hp - s.sibling_boss_damage) / b.max_hp;
                let want = if left < 1. / 3. {
                    2
                } else if left < 2. / 3. {
                    1
                } else {
                    0
                };
                if want > self.phase {
                    self.phase = want;
                    let kinds = &def.waves[0];
                    for i in 0..if want == 1 { 4 } else { 6 } {
                        let kind = kinds[i % kinds.len()].0.clone();
                        let at = if sea(&kind) {
                            None
                        } else {
                            Some([
                                if i % 2 == 1 {
                                    -s.scale
                                } else {
                                    s.width + s.scale
                                },
                                rand(rng, s.top_limit + s.scale, s.sky_bottom - s.scale),
                            ])
                        };
                        w.apply_random(
                            Effect::Spawn {
                                kind,
                                extra: true,
                                at,
                            },
                            rng,
                        );
                    }
                    self.banner_t = 2.2;
                    w.apply(Effect::BossPhase(want));
                    if want == 2 {
                        w.apply(Effect::SpeedBoss(1.2));
                    }
                }
            }
        }
        w.apply_random(Effect::UpdateObjective(dt), rng);
        let s = w.snapshot();
        if s.objective_failed {
            self.fail(w);
        }
        let left = self.total as i64 - self.kills as i64 - s.sibling_kills as i64;
        if s.carry_pending && left <= 0 && self.queue.is_empty() {
            if !self.ob_nag {
                self.ob_nag = true;
                w.apply(Effect::CarryNag(s.carrier_present));
            }
        } else if left <= 0 || (self.queue.is_empty() && s.regular_uncalm == 0) {
            self.wave_complete(def, w, rng);
        }
        self.gems(dt, w, rng);
    }
}
/// All 24 authored missions, after the browser's stretchMission pass.
pub fn missions() -> Vec<MissionDef> {
    let original: Vec<MissionDef> =
        serde_json::from_str(BASE_DEFS).expect("embedded authored definitions");
    fn mix(waves: impl Iterator<Item = Vec<(String, usize)>>) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = vec![];
        for wave in waves {
            for (k, n) in wave {
                if k == "boss" {
                    continue;
                }
                if let Some(v) = out.iter_mut().find(|v| v.0 == k) {
                    v.1 = v.1.max(n);
                } else {
                    out.push((k, n));
                }
            }
        }
        out
    }
    fn scale(w: &[(String, usize)], k: f64) -> Vec<(String, usize)> {
        w.iter()
            .map(|(t, n)| (t.clone(), (*n as f64 * k).ceil() as usize))
            .collect()
    }
    original
        .iter()
        .map(|m| {
            let mut result = m.clone();
            let base: Vec<_> = m
                .waves
                .iter()
                .filter(|w| !w.iter().any(|(k, _)| k == "boss"))
                .cloned()
                .collect();
            result.waves = if m.boss.is_some() {
                let realm = mix(original
                    .iter()
                    .filter(|x| x.realm == m.realm && x.boss.is_none())
                    .map(|x| x.waves[0].clone()));
                vec![
                    scale(&base[0], 1.3),
                    scale(&realm, 0.9),
                    scale(&mix(base.into_iter()), 1.6),
                    vec![("boss".into(), 1)],
                ]
            } else {
                let mut out: Vec<_> = base.iter().map(|w| scale(w, 1.3)).collect();
                while out.len() < 3 {
                    out.push(scale(&base[out.len() % base.len()], 1.6));
                }
                out.push(scale(&mix(base.into_iter()), 1.5));
                out
            };
            result
        })
        .collect()
}

const BASE_DEFS: &str = r##"[{"id":"m1","name":"First Flight","realm":0,"boss":null,"waves":[[["cloud",4]],[["cloud",5]]]},{"id":"m2","name":"Bat Trouble","realm":0,"boss":null,"waves":[[["cloud",3]],[["bat",7]],[["cloud",2],["bat",5]]]},{"id":"m3","name":"Thunder Sprites","realm":0,"boss":null,"waves":[[["sprite",3],["cloud",2]],[["sprite",4],["bat",5]]]},{"id":"m4","name":"The Thunderhead","realm":0,"boss":"thunderhead","waves":[[["cloud",3],["bat",4]],[["boss",1]]]},{"id":"m5","name":"Into the Whisperwood","realm":1,"boss":null,"waves":[[["urgal",3]],[["urgal",3],["bat",4]]]},{"id":"m6","name":"Thornwolf Run","realm":1,"boss":null,"waves":[[["wolf",3]],[["wolf",4],["urgal",2]],[["wolf",3],["sprite",3]]]},{"id":"m7","name":"Urgal Camp","realm":1,"boss":null,"waves":[[["urgal",4],["wolf",2]],[["urgal",4],["bigcloud",2]],[["urgal",3],["wolf",3]]]},{"id":"m8","name":"The Urgal Chieftain","realm":1,"boss":"chieftain","waves":[[["urgal",3],["wolf",3]],[["boss",1]]]},{"id":"m9","name":"Icy Winds","realm":2,"boss":null,"waves":[[["bigcloud",3],["sprite",3]],[["bat",8],["sprite",3]]]},{"id":"m10","name":"Troll Bridge","realm":2,"boss":null,"waves":[[["urgal",3],["troll",1]],[["troll",2],["wolf",3]],[["troll",2],["urgal",2]]]},{"id":"m11","name":"Avalanche Pass","realm":2,"boss":null,"waves":[[["wolf",4],["bat",6]],[["troll",2],["sprite",3]],[["urgal",3],["bigcloud",2],["troll",1]]]},{"id":"m12","name":"The Cave Troll","realm":2,"boss":"cavetroll","waves":[[["troll",2],["urgal",3]],[["boss",1]]]},{"id":"m13","name":"Lights on the Water","realm":3,"boss":null,"waves":[[["jelly",4]],[["jelly",4],["cloud",3]],[["jelly",6]]]},{"id":"m14","name":"Splashfin Bay","realm":3,"boss":null,"waves":[[["leaper",4]],[["leaper",4],["jelly",3]],[["leaper",5],["bat",4]]]},{"id":"m15","name":"Stormy Waves","realm":3,"boss":null,"waves":[[["jelly",4],["bigcloud",2]],[["leaper",5],["sprite",3]],[["jelly",4],["leaper",4],["bigcloud",1]]]},{"id":"m16","name":"Old Grumblegill","realm":3,"boss":"serpent","waves":[[["leaper",4],["jelly",3]],[["boss",1]]]},{"id":"m17","name":"Hot Rocks","realm":4,"boss":null,"waves":[[["slime",3]],[["slime",3],["bat",4]],[["slime",4],["wolf",2]]]},{"id":"m18","name":"Imp Trouble","realm":4,"boss":null,"waves":[[["imp",4]],[["imp",4],["slime",2]],[["imp",5],["sprite",3]]]},{"id":"m19","name":"The Lava River","realm":4,"boss":null,"waves":[[["slime",3],["imp",3]],[["troll",1],["slime",3],["bat",4]],[["imp",4],["slime",3],["wolf",2]]]},{"id":"m20","name":"The Great Magma Slime","realm":4,"boss":"bigslime","waves":[[["slime",3],["imp",3]],[["boss",1]]]},{"id":"m21","name":"Above the Clouds","realm":5,"boss":null,"waves":[[["gobbler",4]],[["gobbler",4],["sprite",3]],[["gobbler",5],["cloud",3]]]},{"id":"m22","name":"Moth Meadow","realm":5,"boss":null,"waves":[[["moth",3]],[["moth",3],["gobbler",3]],[["moth",4],["bat",5]]]},{"id":"m23","name":"The Dark Patch","realm":5,"boss":null,"waves":[[["moth",3],["gobbler",3]],[["jelly",3],["imp",3],["sprite",2]],[["moth",3],["gobbler",4],["bigcloud",1]]]},{"id":"m24","name":"The Star Swallower","realm":5,"boss":"swallower","waves":[[["gobbler",4],["moth",2]],[["boss",1]]]}]"##;
