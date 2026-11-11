//! Source `shareWorld`/`onWorld` codec and guest projection, without network or save IO.
//! The caller supplies the transport sender and keeps Authority synchronized with election.
//! The valid wire contract uses numeric array indices/IDs; malformed index types are rejected.
//! Legacy coercion edge cases outside the emitted numeric schema are not a compatibility promise.
//! New enemies are made synchronously by the existing spawn path (including its RNG draws).
use crate::{
    authority::Authority,
    enemies::{Enemy, Gem, Hazard, HazardKind, Kind},
    missions::{Leg, Mission},
    objectives::{self, Objective},
};
use serde_json::{Value, json};
use std::collections::HashSet;

pub const TYPES: [Kind; 20] = [
    Kind::Cloud,
    Kind::Bigcloud,
    Kind::Bat,
    Kind::Sprite,
    Kind::Urgal,
    Kind::Wolf,
    Kind::Troll,
    Kind::Thunderhead,
    Kind::Chieftain,
    Kind::Cavetroll,
    Kind::Jelly,
    Kind::Leaper,
    Kind::Serpent,
    Kind::Slime,
    Kind::Slimelet,
    Kind::Imp,
    Kind::Bigslime,
    Kind::Gobbler,
    Kind::Moth,
    Kind::Swallower,
];
pub const HAZARDS: [HazardKind; 7] = [
    HazardKind::Rock,
    HazardKind::Boulder,
    HazardKind::Spark,
    HazardKind::Fall,
    HazardKind::Bubble,
    HazardKind::Ember,
    HazardKind::Dust,
];
const WHATS: [&str; 7] = ["barn", "fawn", "goat", "lamp", "boats", "star", "garden"];
const NAMES: [&str; 7] = [
    "the hay barn",
    "the fawn",
    "the mountain goats",
    "the lighthouse lamp",
    "the fishing boats",
    "the star piece",
    "the star garden",
];
const OBJECTIVE_KINDS: [objectives::Kind; 3] = [
    objectives::Kind::Protect,
    objectives::Kind::Escort,
    objectives::Kind::Carry,
];
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub width: f64,
    pub top: f64,
    pub ground: f64,
    pub scale: f64,
}
impl Viewport {
    fn x(self, v: &Value, lo: f64, hi: f64) -> f64 {
        number(v).clamp(lo, hi) * self.width
    }
    fn y(self, v: &Value, lo: f64, hi: f64) -> f64 {
        self.top + number(v).clamp(lo, hi) * (self.ground - self.top)
    }
    fn nx(self, x: f64) -> f64 {
        round(x / self.width * 1000.) / 1000.
    }
    fn ny(self, y: f64) -> f64 {
        round((y - self.top) / (self.ground - self.top).max(1.) * 1000.) / 1000.
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Mirror {
    pub enemy: Enemy,
    pub destination: [f64; 2],
}
#[derive(Clone, Debug, PartialEq)]
pub struct NetworkHazard {
    pub uid: u64,
    pub hazard: Hazard,
    pub spin: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Strike {
    pub uid: u64,
    pub x: f64,
    pub warn: f64,
    pub bolt: f64,
    pub y0: Option<f64>,
    pub hit: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Wave,
    Wrong,
    Win,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankReason {
    Stop,
    End,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Sound(Sound),
    Encouragement,
    Arrival(usize),
    Travel { from: usize, to: usize },
    Bank(BankReason),
    FailureCaption(String),
    Complete,
}
#[derive(Clone, Debug)]
pub struct GuestWorld {
    pub mission_id: String,
    pub wave_count: usize,
    pub mission: Mission,
    pub authority: Authority,
    pub run: String,
    pub next_uid: u64,
    pub last_world: f64,
    pub banner: String,
    pub fail_why: String,
    pub last_queue: Vec<String>,
    pub objective: Option<Objective>,
    pub enemies: Vec<Mirror>,
    pub hazards: Vec<NetworkHazard>,
    pub strikes: Vec<Strike>,
    pub used_hazards: HashSet<u64>,
    pub used_strikes: HashSet<u64>,
}
impl GuestWorld {
    pub fn new(mission_id: String, wave_count: usize, authority: Authority) -> Self {
        let mission = Mission {
            guest: authority.guest,
            ..Mission::new()
        };
        Self {
            mission_id,
            wave_count,
            mission,
            authority,
            run: String::new(),
            next_uid: 0,
            last_world: 0.,
            banner: String::new(),
            fail_why: String::new(),
            last_queue: Vec::new(),
            objective: None,
            enemies: Vec::new(),
            hazards: Vec::new(),
            strikes: Vec::new(),
            used_hazards: HashSet::new(),
            used_strikes: HashSet::new(),
        }
    }
    /// Static-room admission only; no legacy database rider-ID authority fallback.
    /// None means rejected with no state changes. The sender must come from the transport.
    /// Numeric IDs must be nonnegative JS-safe integers, unlike permissive legacy malformed IDs.
    pub fn receive(
        &mut self,
        d: &Value,
        peer: &str,
        view: Viewport,
        time: f64,
        spawn: &mut impl FnMut(Kind, [f64; 2]) -> Enemy,
    ) -> Option<Vec<Event>> {
        let run = d["run"].as_str()?;
        let epoch = integer(&d["epoch"])?;
        if !self.authority.guest
            || !d.is_object()
            || d["mid"].as_str() != Some(&self.mission_id)
            || self.authority.host_peer.as_deref() != Some(peer)
            || (!self.run.is_empty() && self.run != run)
            || run.encode_utf16().count() > 12
            || epoch < self.authority.epoch
            || self.wave_count == 0
        {
            return None;
        }
        self.authority.epoch = epoch;
        self.authority.host_rider = integer(&d["r"]).unwrap_or(0) as usize;
        if let Some(next) = integer(&d["next"]).filter(|n| *n <= 1_000_000_000) {
            self.next_uid = self.next_uid.max(next);
        }
        self.last_world = time;
        self.run = run.to_owned();
        let mut out = Vec::new();
        let m = &mut self.mission;
        let wave = number(&d["wave"])
            .clamp(0., (self.wave_count - 1) as f64)
            .floor() as i32;
        if wave != m.wave {
            m.wave = wave;
            self.banner = truncate(
                &if truthy(&d["banner"]) {
                    js_string(&d["banner"])
                } else {
                    format!("Wave {}", wave + 1)
                },
                40,
            );
            m.banner_t = 2.2;
            out.push(Event::Sound(Sound::Wave));
            out.push(Event::Encouragement);
        }
        let leg = if truthy(&d["lg"]) {
            Leg::Fly
        } else {
            Leg::Fight
        };
        let stop = number(&d["sp"])
            .clamp(0., (self.wave_count - 1) as f64)
            .floor() as usize;
        if leg != m.leg || stop != m.stop {
            if leg == Leg::Fight {
                out.push(Event::Arrival(stop));
            } else if stop > 0 {
                out.push(Event::Bank(BankReason::Stop));
                out.push(Event::Travel {
                    from: stop - 1,
                    to: stop,
                });
            }
            m.leg = leg;
            m.stop = stop;
        }
        m.prog = number(&d["pr"]).clamp(0., 1000.) / 1000.;
        m.total = number(&d["total"]).clamp(0., 99.).floor() as usize;
        m.kills = number(&d["kills"]).clamp(0., 99.).floor() as usize;
        self.last_queue = array(&d["q"])
            .iter()
            .filter_map(|v| {
                v.as_str()
                    .filter(|s| serde_json::from_value::<Kind>(json!(s)).is_ok())
                    .map(str::to_owned)
            })
            .take(40)
            .collect();
        if d["done"] == 2 && !m.done && m.fail_t == 0. {
            m.fail_t = 1.6;
            self.banner = "Mission failed".into();
            m.banner_t = 1.6;
            out.push(Event::Sound(Sound::Wrong));
            if let Some(reason) = d["fw"].as_str().filter(|s| !s.is_empty()) {
                self.fail_why = truncate(reason, 80);
                out.push(Event::FailureCaption(self.fail_why.clone()));
            }
        }
        let ob = &d["ob"];
        let indices = ob
            .as_array()
            .and_then(|a| Some((index(a.first()?, 3)?, index(a.get(1)?, 7)?)));
        if let Some((ki, wi)) = indices {
            let a = array(ob);
            let old_hp = self
                .objective
                .as_ref()
                .filter(|o| o.what == WHATS[wi])
                .map(|o| o.hp);
            if self.objective.as_ref().is_none_or(|o| o.what != WHATS[wi]) {
                self.objective = Some(Objective {
                    kind: OBJECTIVE_KINDS[ki],
                    what: WHATS[wi].into(),
                    name: NAMES[wi].into(),
                    x: 0.,
                    y: 0.,
                    x0: 0.,
                    to: 0.,
                    dur: 0.,
                    t: 0.,
                    hp: 0,
                    max_hp: 0,
                    carrier: -1,
                    done: false,
                    vx: 0.,
                    vy: 0.,
                    steal_cd: 0.,
                    hurt_t: 0.,
                    r: view.scale
                        * match wi {
                            4 => 1.6,
                            5 => 0.45,
                            _ => 0.9,
                        },
                });
            }
            let o = self.objective.as_mut().expect("created above");
            let hp = number(get(a, 4)).clamp(0., 20.).floor() as u8;
            if old_hp.is_some_and(|old| hp < old) {
                o.hurt_t = 1.2;
            }
            o.x = view.x(get(a, 2), -0.2, 1.2);
            o.y = view.y(get(a, 3), -0.5, 1.3);
            o.hp = hp;
            o.max_hp = number(get(a, 5)).clamp(0., 20.).floor() as u8;
            o.carrier = get(a, 6)
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .unwrap_or(-1);
            o.done = get(a, 7) == 1;
            o.to = view.x(get(a, 8), 0., 1.);
            o.t += 0.07;
        } else if !truthy(ob) {
            self.objective = None;
        }
        if d["done"] == 1 && !m.done {
            out.push(Event::Bank(BankReason::End));
            m.done = true;
            m.end_t = 3.4;
            self.banner = "Mission complete!".into();
            m.banner_t = 1.8;
            out.push(Event::Sound(Sound::Win));
            out.push(Event::Complete);
        }
        let mut seen = HashSet::new();
        for a in array(&d["en"]).iter().take(40).filter_map(Value::as_array) {
            let Some(uid) = integer(get(a, 0)) else {
                continue;
            };
            let Some(ti) = index(get(a, 1), TYPES.len()) else {
                continue;
            };
            seen.insert(uid);
            let destination = [view.x(get(a, 2), -0.3, 1.3), view.y(get(a, 3), -0.5, 1.3)];
            let i = self
                .enemies
                .iter()
                .position(|e| e.enemy.target.id == uid)
                .unwrap_or_else(|| {
                    let mut e = spawn(TYPES[ti], destination);
                    e.target.id = uid;
                    e.target.x = destination[0];
                    e.target.y = destination[1];
                    e.entered = true;
                    self.enemies.push(Mirror {
                        enemy: e,
                        destination,
                    });
                    self.enemies.len() - 1
                });
            let mirror = &mut self.enemies[i];
            mirror.destination = destination;
            let e = &mut mirror.enemy;
            let fl = number(get(a, 6)) as i64;
            e.target.hp = number(get(a, 4)).clamp(0., 100.) / 100. * e.target.max_hp;
            e.dir = if get(a, 5) == -1 { -1. } else { 1. };
            e.target.calm = fl & 1 != 0;
            if (fl & 2 != 0 && e.target.flash <= 0.) || (e.air && fl & 16 == 0) {
                e.squash = 0.3;
            }
            if fl & 2 != 0 {
                e.target.flash = 0.1;
            }
            e.target.slow = if fl & 4 != 0 { 0.2 } else { 0. };
            e.windup = if fl & 8 != 0 { 0.3 } else { 0. };
            e.pre = e.windup;
            e.air = fl & 16 != 0;
            e.target.stun = if fl & 32 != 0 { 0.2 } else { 0. };
            e.hurt = fl & 64 != 0;
            e.target.cracked = if fl & 128 != 0 { 0.3 } else { 0. };
            e.target.marked = if fl & 256 != 0 { 0.3 } else { 0. };
        }
        self.enemies.retain(|e| seen.contains(&e.enemy.target.id));
        self.hazards = array(&d["hz"])
            .iter()
            .take(40)
            .filter_map(Value::as_array)
            .filter_map(|a| {
                let uid = integer(get(a, 0))?;
                if self.used_hazards.contains(&uid) {
                    return None;
                }
                let ki = index(get(a, 1), 7).unwrap_or(0);
                let (r, g) = [
                    (0.2, 9.),
                    (0.34, 9.),
                    (0.14, 0.),
                    (0.28, 8.),
                    (0.24, -0.5),
                    (0.2, 9.),
                    (0.26, 1.2),
                ][ki];
                Some(NetworkHazard {
                    uid,
                    spin: 0.,
                    hazard: Hazard {
                        kind: HAZARDS[ki],
                        position: [view.x(get(a, 2), -0.2, 1.2), view.y(get(a, 3), -0.5, 1.3)],
                        velocity: [
                            number(get(a, 4)) * view.width,
                            number(get(a, 5)) * (view.ground - view.top),
                        ],
                        gravity: view.scale * g,
                        radius: view.scale * r,
                        life: 3.,
                        warn: number(get(a, 6)).clamp(0., 2.),
                    },
                })
            })
            .collect();
        self.strikes = array(&d["st"])
            .iter()
            .take(10)
            .filter_map(Value::as_array)
            .filter_map(|a| {
                let uid = integer(get(a, 0))?;
                let y0 = get(a, 4);
                Some(Strike {
                    uid,
                    x: view.x(get(a, 1), 0., 1.),
                    warn: number(get(a, 2)).clamp(0., 2.),
                    bolt: number(get(a, 3)).clamp(0., 1.),
                    hit: self.used_strikes.contains(&uid),
                    y0: if a.len() > 4 && number_raw(y0) >= 0. {
                        Some(view.y(y0, 0., 1.3))
                    } else {
                        None
                    },
                })
            })
            .collect();
        Some(out)
    }
}
/// Caller gates cadence/live peers; this function only encodes the existing wire object.
pub fn encode_world(world: &GuestWorld, rider: usize, view: Viewport) -> Value {
    let m = &world.mission;
    json!({"r":rider,"mid":world.mission_id,"run":world.run,"epoch":world.authority.epoch,"next":world.next_uid,"wave":m.wave,"banner":world.banner,"lg":u8::from(m.leg==Leg::Fly),"sp":m.stop,"pr":round(m.prog*1000.),"total":m.total,"kills":m.kills,"done":if m.done{1}else if m.fail_t>0.{2}else{0},"q":m.queue.iter().take(40).collect::<Vec<_>>(),"fw":world.fail_why,
    "ob":world.objective.as_ref().map(|o|json!([OBJECTIVE_KINDS.iter().position(|k|*k==o.kind).map_or(-1,|i|i as i32),WHATS.iter().position(|w|*w==o.what).map_or(-1,|i|i as i32),view.nx(o.x),view.ny(o.y),o.hp,o.max_hp,o.carrier,u8::from(o.done),view.nx(o.to)])),
    "en":world.enemies.iter().filter(|e|!e.enemy.target.dead).map(|m|{let e=&m.enemy;let t=&e.target;let flags=u32::from(t.calm)|(u32::from(t.flash>0.)<<1)|(u32::from(t.slow>0.)<<2)|(u32::from(e.windup>0.||e.pre>0.)<<3)|(u32::from(e.air)<<4)|(u32::from(t.stun>0.)<<5)|(u32::from(hurts_now(e))<<6)|(u32::from(t.cracked>0.)<<7)|(u32::from(t.marked>0.)<<8);json!([t.id,TYPES.iter().position(|k|*k==e.kind).unwrap(),view.nx(t.x),view.ny(t.y),round((t.hp/t.max_hp).clamp(0.,1.)*100.),e.dir,flags])}).collect::<Vec<_>>(),
    "hz":world.hazards.iter().map(|h|json!([h.uid,HAZARDS.iter().position(|k|*k==h.hazard.kind).unwrap(),view.nx(h.hazard.position[0]),view.ny(h.hazard.position[1]),round(h.hazard.velocity[0]/view.width*1000.)/1000.,round(h.hazard.velocity[1]/(view.ground-view.top).max(1.)*1000.)/1000.,round(h.hazard.warn.max(0.)*100.)/100.])).collect::<Vec<_>>(),
    "st":world.strikes.iter().map(|s|json!([s.uid,view.nx(s.x),round(s.warn.max(0.)*100.)/100.,round(s.bolt.max(0.)*100.)/100.,s.y0.map_or(-1.,|y|view.ny(y))])).collect::<Vec<_>>()})
}
#[derive(Clone, Debug, PartialEq)]
pub enum MirrorEvent {
    Gobble { enemy: u64, gem: u64 },
    Touch { enemy: u64, hurts: bool },
    Sting { enemy: u64 },
}
/// Apply effects immediately; later enemies and sting checks must see changed safety/gems.
pub trait MirrorWorld {
    fn safe(&self) -> bool;
    fn player(&self) -> [f64; 2];
    fn gems(&self) -> &[Gem];
    fn apply(&mut self, event: MirrorEvent);
}
pub fn update_mirrors(
    enemies: &mut [Mirror],
    dt: f64,
    view: Viewport,
    dragon_scale: f64,
    world: &mut impl MirrorWorld,
) {
    let s = view.scale * dragon_scale;
    let k = (dt * 12.).min(1.);
    for mirror in enemies {
        let e = &mut mirror.enemy;
        e.t += dt;
        e.target.flash -= dt;
        e.target.x += (mirror.destination[0] - e.target.x) * k;
        e.target.y += (mirror.destination[1] - e.target.y) * k;
        if e.kind.definition().ground && !e.air {
            e.target.y = view.ground - e.target.r * 0.95;
        }
        if e.kind == Kind::Gobbler
            && !e.target.calm
            && let Some(g) = world.gems().iter().find(|g| {
                (g.position[0] - e.target.x).hypot(g.position[1] - e.target.y)
                    < e.target.r + g.radius
            })
        {
            world.apply(MirrorEvent::Gobble {
                enemy: e.target.id,
                gem: g.id,
            });
        }
        let p = world.player();
        let distance = (p[0] - e.target.x).hypot(p[1] - e.target.y);
        if !e.target.calm
            && !world.safe()
            && !(e.kind == Kind::Leaper && !e.air)
            && e.target.stun <= 0.
            && distance < e.target.r * 0.85 + s * 0.35
        {
            world.apply(MirrorEvent::Touch {
                enemy: e.target.id,
                hurts: hurts_now(e),
            });
        }
        if e.sting > 0. {
            e.sting -= dt;
        }
        let p = world.player();
        if e.sting > 0.
            && !world.safe()
            && !e.target.calm
            && (p[0] - e.target.x).hypot(p[1] - e.target.y) < e.target.r * 1.9 + s * 0.3
        {
            world.apply(MirrorEvent::Sting { enemy: e.target.id });
        }
    }
}
fn hurts_now(e: &Enemy) -> bool {
    e.hurt
        || e.sting > 0.
        || (e.air && (matches!(e.kind, Kind::Wolf | Kind::Leaper) || e.kind.definition().hop))
        || (e.kind == Kind::Swallower && e.windup > 0.)
}
fn round(n: f64) -> f64 {
    (n + 0.5).floor()
}
fn get(a: &[Value], i: usize) -> &Value {
    a.get(i).unwrap_or(&Value::Null)
}
fn array(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}
fn integer(v: &Value) -> Option<u64> {
    let n = v.as_f64()?;
    ((0. ..=9_007_199_254_740_991.).contains(&n) && n.fract() == 0.).then_some(n as u64)
}
fn index(v: &Value, len: usize) -> Option<usize> {
    let n = v.as_f64()?;
    (n >= 0. && n.fract() == 0. && n < (len as f64)).then_some(n as usize)
}
fn number_raw(v: &Value) -> f64 {
    match v {
        Value::Null => 0.,
        Value::Bool(b) => u8::from(*b) as f64,
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => {
            if s.trim().is_empty() {
                0.
            } else {
                s.trim().parse().unwrap_or(f64::NAN)
            }
        }
        Value::Array(a) => {
            if a.is_empty() {
                0.
            } else if a.len() == 1 {
                number_raw(&a[0])
            } else {
                f64::NAN
            }
        }
        _ => f64::NAN,
    }
}
fn number(v: &Value) -> f64 {
    let n = number_raw(v);
    if n.is_nan() { 0. } else { n }
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(_) => "[object Object]".into(),
        _ => v.to_string(),
    }
}
fn truncate(s: &str, max: usize) -> String {
    String::from_utf16_lossy(&s.encode_utf16().take(max).collect::<Vec<_>>())
}
