//! Solo mission integration harness, not the full browser game.
//! Uses original update ordering and one explicit gameplay draw stream. Presentation
//! particle draws, networking and persistent storage are
//! not ported. Normalized profile, party combat, growth and abilities run in memory. Consequently a seed is a Rust replay seed, not a
//! promise to reproduce a browser Math.random session. No personal saves are read.
pub mod sound;
use crate::{
    abilities,
    combat::{Breath, Kind as BreathKind, Mouth, Path, Shot},
    damage::{self, Hazard, Strike, Used},
    enemies::{self, Enemy},
    flight::{FlightRules, Input, Player, Viewport},
    gems::{self, Gem},
    missions::{self, Effect, Leg, Mission, MissionDef, MissionWorld, WorldSnapshot},
    objectives::{self, Objective},
    progression,
    projectiles::{self, Target},
    random::Mulberry32,
    roost,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::OnceLock};
fn definitions() -> &'static [MissionDef] {
    static DEFS: OnceLock<Vec<MissionDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let mut defs = missions::missions();
        let champions: Vec<_> = defs
            .iter()
            .map(|base| {
                let mut c = base.clone();
                c.id.push('c');
                for wave in &mut c.waves {
                    for (kind, n) in wave {
                        if kind != "boss" {
                            *n = (*n as f64 * 1.5).ceil() as usize;
                        }
                    }
                }
                if base.boss.is_some() {
                    c.waves
                        .last_mut()
                        .unwrap()
                        .push((base.waves[0].last().unwrap().0.clone(), 3));
                } else {
                    let mut mix: Vec<(String, usize)> = vec![];
                    for wave in &base.waves {
                        for (kind, n) in wave {
                            if let Some((_, count)) = mix.iter_mut().find(|(k, _)| k == kind) {
                                *count = (*count).max(*n);
                            } else {
                                mix.push((kind.clone(), *n));
                            }
                        }
                    }
                    for (_, n) in &mut mix {
                        *n = (*n as f64 * 1.5).ceil() as usize;
                    }
                    c.waves.push(mix);
                }
                c
            })
            .collect();
        defs.extend(champions);
        defs
    })
}
fn random(draw: &mut impl FnMut() -> f64, a: f64, b: f64) -> f64 {
    a + draw() * (b - a)
}
fn make_gem(
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    view: Viewport,
    draw: &mut impl FnMut() -> f64,
) -> Gem {
    let golden = draw() < 0.06;
    let t = draw() * 6.;
    let color = if golden {
        "#ffd76e"
    } else {
        const COLORS: [&str; 5] = ["#62f0d4", "#ff6fb5", "#8fd0ff", "#b9ff6b", "#c79bff"];
        COLORS[((draw() * 5.).floor() as usize).min(4)]
    };
    Gem {
        x,
        y,
        vx,
        vy,
        t,
        life: 10.,
        golden,
        value: if golden { 3. } else { 1. },
        r: view.scale * if golden { 0.3 } else { 0.22 },
        lost_one: false,
        color: color.into(),
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Ally {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub color: String,
    pub name: String,
    pub t: f64,
    pub cd: f64,
}
pub struct Game {
    pub(crate) net: crate::coop::Runtime,
    target_rider: BTreeMap<u64, i32>,
    pub champion: bool,
    /// Effective source timer used for the swift crown, not a failure deadline.
    pub par: f64,
    pub allies: Vec<Ally>,
    pub ally_t: f64,
    pub player: Player,
    pub shots: Vec<Shot>,
    pub enemies: Vec<Enemy>,
    pub hazards: Vec<Hazard>,
    pub strikes: Vec<Strike>,
    pub gems: Vec<Gem>,
    pub mission: Mission,
    pub definition: &'static MissionDef,
    pub objective: Option<Objective>,
    pub objective_events: Vec<objectives::Event>,
    /// Ring/splash presentation requests; cleared at each step, never simulation commands.
    pub presentation_events: Vec<enemies::Event>,
    /// Retained until drained, capped independently from simulation state.
    pub sound_events: Vec<sound::Event>,
    sound_state: sound::State,
    pub fail_reason: Option<String>,
    objective_failed: bool,
    objective_cd: BTreeMap<u64, f64>,
    objective_strikes: std::collections::BTreeSet<u64>,
    target_objective: std::collections::BTreeSet<u64>,
    pub earned: f64,
    pub xp: f64,
    pub result: Option<bool>,
    pub lost: f64,
    pub bonks: usize,
    pub kills: usize,
    pub hit_stop: f64,
    /// Source P.sq presentation impulse; the outer frame's animate pass decays it.
    pub squash: f64,
    pub scroll: f64,
    pub scroll_v: f64,
    pub rng: Mulberry32,
    pub(crate) view: Viewport,
    pub(crate) uid: u64,
    pub(crate) used: Used,
    target_t: BTreeMap<u64, f64>,
    pub profile: Option<Value>,
    team_profiles: Vec<Value>,
    pub progression_events: Vec<progression::Event>,
    pub wings: BTreeMap<String, Wing>,
    pub time: f64,
    pub cast_count: u64,
    pub last_cast: Option<String>,
    pub rain: f64,
    /// Caller-injected local day and timestamp; default zero is a synthetic clock.
    pub completion_day: i64,
    pub completion_now_ms: u64,
    pub completion_events: Vec<roost::CompletionEvent>,
    had_objective: bool,
    pub heart_charm: bool,
    pub pouch_charm: bool,
}
impl Game {
    pub fn drain_sound_events(&mut self) -> std::vec::Drain<'_, sound::Event> {
        self.sound_events.drain(..)
    }
    fn sound(&mut self, event: sound::Event) {
        sound::push(&mut self.sound_events, event);
    }
    pub fn first_flight(view: Viewport, seed: u32) -> Self {
        Self::with_definition(&definitions()[0], view, seed)
    }
    /// All 24 authored missions and their champion variants (explicit harness launch).
    pub fn start(id: &str, view: Viewport, seed: u32) -> Result<Self, &'static str> {
        if !valid_view(view) {
            return Err("invalid viewport");
        }
        let def = definitions()
            .iter()
            .find(|d| d.id == id)
            .ok_or("mission is not integrated yet")?;
        Ok(Self::with_definition(def, view, seed))
    }
    fn with_definition(def: &'static MissionDef, view: Viewport, seed: u32) -> Self {
        Self::with_definition_at(def, view, seed, 0)
    }
    fn with_definition_at(
        def: &'static MissionDef,
        view: Viewport,
        seed: u32,
        from_wave: usize,
    ) -> Self {
        let player = Player {
            x: view.scale * 2.6,
            y: view.ground_y - view.scale * 1.6,
            safe: 1.,
            ..Player::default()
        };
        let mut g = Self {
            net: crate::coop::Runtime::default(),
            target_rider: BTreeMap::new(),
            champion: def.id.ends_with('c'),
            par: 0.,
            allies: vec![],
            ally_t: 4.,
            player,
            shots: vec![],
            enemies: vec![],
            hazards: vec![],
            strikes: vec![],
            gems: vec![],
            mission: Mission::new(),
            definition: def,
            objective: None,
            objective_events: vec![],
            presentation_events: vec![],
            sound_events: Vec::with_capacity(sound::CAPACITY),
            sound_state: sound::State::new(seed),
            fail_reason: None,
            objective_failed: false,
            objective_cd: BTreeMap::new(),
            objective_strikes: Default::default(),
            target_objective: Default::default(),
            earned: 0.,
            xp: 0.,
            result: None,
            lost: 0.,
            bonks: 0,
            kills: 0,
            hit_stop: 0.,
            squash: 0.,
            scroll: 0.,
            scroll_v: 0.,
            rng: Mulberry32 { state: seed },
            view,
            uid: 0,
            used: Used::default(),
            target_t: BTreeMap::new(),
            profile: None,
            team_profiles: vec![],
            progression_events: vec![],
            wings: BTreeMap::new(),
            time: 0.,
            cast_count: 0,
            last_cast: None,
            rain: 0.,
            completion_day: 0,
            completion_now_ms: 0,
            completion_events: vec![],
            had_objective: false,
            heart_charm: false,
            pouch_charm: false,
        };
        if g.champion {
            let scaling = g.scaling();
            let mut par = 10.;
            for wave in &def.waves {
                for (kind, n) in wave {
                    let kind = if kind == "boss" {
                        def.boss.as_ref().unwrap()
                    } else {
                        kind
                    };
                    let k: enemies::Kind =
                        serde_json::from_value(json!(kind)).expect("authored enemy");
                    let d = k.definition();
                    par += *n as f64
                        * (2.4
                            + 0.15 * d.hp * scaling.difficulty
                                / if d.armor == 0. { 1. } else { d.armor });
                }
            }
            let travel = 8. + 15. * (def.waves.len() - 1) as f64;
            let authored = ((par + travel) / 5.).ceil() * 5.;
            g.par = ((travel
                + (authored - travel)
                    * if def.boss.is_some() { 1.5 } else { 1.15 }
                    * (0.5 + 0.5 * scaling.toughness / 3.))
                / 5.)
                .ceil()
                * 5.;
        }
        // Validation is returned by step; do not panic for a malformed initial viewport.
        if valid_view(view) {
            view.clamp(&mut g.player);
        }
        let mut m = std::mem::take(&mut g.mission);
        m.start_leg(def, from_wave, true, &mut Adapter(&mut g));
        g.mission = m;
        g
    }
    fn update_allies(&mut self, dt: f64, draw: &mut impl FnMut() -> f64) {
        let hatched = self
            .profile
            .as_ref()
            .map(|p| team_hatched(p, &self.team_profiles))
            .unwrap_or(0.);
        let wing_on = hatched >= 8. && self.enemies.iter().any(|e| e.target.boss && !e.target.calm);
        self.ally_t -= dt;
        if self.ally_t <= 0. && (wing_on || hatched >= 20.) {
            for _ in 0..if wing_on { 2 } else { 1 } {
                let pool = self.profile.as_ref().and_then(|p| p["wild"].as_array());
                let wild = pool
                    .filter(|a| !a.is_empty())
                    .map(|a| &a[(draw() * a.len() as f64).floor() as usize]);
                let color = wild
                    .and_then(|w| w["color"].as_str())
                    .unwrap_or("#ff6a3d")
                    .to_owned();
                let name = wild
                    .and_then(|w| w["name"].as_str())
                    .unwrap_or("A wild dragon")
                    .to_owned();
                let left = draw() < 0.5;
                self.allies.push(Ally {
                    x: if left {
                        -self.view.scale * 1.5
                    } else {
                        self.view.width + self.view.scale * 1.5
                    },
                    y: random(
                        draw,
                        self.view.top_limit + self.view.scale,
                        (self.view.top_limit + self.view.scale * 1.5).max(self.sky_bottom()),
                    ),
                    vx: if left { 1. } else { -1. } * self.view.scale * 4.5,
                    color,
                    name,
                    t: 0.,
                    cd: 0.6,
                });
                let _toast = draw() < 0.5;
            }
            self.ally_t = if wing_on { 9. } else { 14. };
        }
        let power = 1. + 0.2 * self.craft("bellows");
        for i in (0..self.allies.len()).rev() {
            let a = &mut self.allies[i];
            a.t += dt;
            a.x += a.vx * dt;
            a.y += (a.t * 2.).sin() * self.view.scale * 0.4 * dt;
            a.cd -= dt;
            if a.cd <= 0. && !self.enemies.is_empty() {
                let f = a.vx.signum();
                self.shots.push(Shot {
                    x: a.x + f * self.view.scale * 0.5,
                    y: a.y - self.view.scale * 0.1,
                    vx: f * self.view.scale * 11.,
                    life: 1.1,
                    max: 1.1,
                    r: self.view.scale * 0.18,
                    dmg: power,
                    kind: BreathKind::Flame,
                    owner: "wild".into(),
                    ..Shot::default()
                });
                a.cd = 0.45;
            }
            if a.x < -self.view.scale * 2. || a.x > self.view.width + self.view.scale * 2. {
                self.allies.remove(i);
            }
        }
    }
    fn enemy_world(&self) -> enemies::World {
        enemies::World {
            width: self.view.width,
            ground_y: self.view.ground_y,
            top: self.view.top_limit,
            sky_bottom: self.sky_bottom(),
            scale: self.view.scale,
            player: [self.player.x, self.player.y],
            aim: [self.player.x, self.player.y],
            quiet: self.player.sila > 0.,
            safe: damage::is_safe(&self.player),
            dragon_scale: self.view.dragon_scale,
        }
    }
    fn sky_bottom(&self) -> f64 {
        self.view.ground_y
            - self.view.scale
                * if matches!(self.definition.realm, 1 | 2 | 4) {
                    2.6
                } else {
                    0.5
                }
    }
    pub fn scaling(&self) -> enemies::MissionScaling {
        let realm = self.definition.realm;
        let in_realm = (self.definition.id.trim_end_matches('c')[1..]
            .parse::<usize>()
            .expect("authored id")
            - 1)
            % 4;
        let r0 = realm.min(2) as f64;
        let r1 = realm.saturating_sub(2) as f64;
        enemies::MissionScaling {
            difficulty: (1. + r0 * 0.35 + r1 * 0.25 + in_realm as f64 * 0.08)
                * if self.champion { 1.3 } else { 1. },
            toughness: [1.15, 1.5, 2.3, 4., 5., 6.][realm] * (1. + 0.06 * in_realm as f64),
            sky_toughness: if self.net.enabled { self.net.sky } else { 1. },
            speed: (1. + r0 * 0.12 + r1 * 0.05) * if self.champion { 1.1 } else { 1. },
        }
    }
    fn spawn_baby(
        &mut self,
        at: [f64; 2],
        velocity: [f64; 2],
        dir: f64,
        extra: bool,
        draw: &mut impl FnMut() -> f64,
    ) {
        self.spawn(enemies::Kind::Slimelet, extra, Some(at), draw);
        let b = self.enemies.last_mut().expect("just spawned baby");
        b.air = true;
        b.velocity = velocity;
        b.dir = dir;
        b.entered = true;
    }
    fn drop_carried_piece(&mut self, draw: &mut impl FnMut() -> f64) {
        if !self.mission.guest
            && let Some(ob) = self
                .objective
                .as_mut()
                .filter(|ob| ob.carrier == self.net.rider as i32)
        {
            ob.drop_piece(self.view, draw, &mut self.objective_events);
        }
    }
    fn next_id(&mut self) -> u64 {
        self.uid += 1;
        self.uid
    }
    fn spawn(
        &mut self,
        kind: enemies::Kind,
        extra: bool,
        at: Option<[f64; 2]>,
        draw: &mut impl FnMut() -> f64,
    ) {
        let id = self.next_id();
        let e = Enemy::spawn(
            kind,
            id,
            extra,
            at,
            &self.enemy_world(),
            self.scaling(),
            draw,
        );
        self.enemies.push(e);
    }
    fn retire(&mut self, draw: &mut impl FnMut() -> f64) {
        if self.net.retired
            || self.player.hearts > 0
            || self.mission.done
            || self.mission.fail_t != 0.
        {
            return;
        }
        self.net.retired = true;
        self.player.out = true;
        self.player.dizzy = 0.;
        let lost = if self.pouch_charm {
            0.
        } else {
            (self.mission.carry / 2.).ceil()
        };
        self.lost += lost;
        self.mission.carry -= lost;
        for _ in 0..lost.min(8.) as usize {
            let vx = random(draw, -4., 4.) * self.view.scale;
            let vy = -self.view.scale * random(draw, 2., 5.);
            let mut gem = make_gem(self.player.x, self.player.y, vx, vy, self.view, draw);
            gem.life = 1.2;
            gem.lost_one = true;
            self.gems.push(gem);
        }
        if self.net.enabled {
            if self.mission.guest {
                return;
            }
            if self.net.active_others > 0 {
                self.net.events.push(crate::coop::GameEvent::Handoff);
                self.mission.guest = true;
                return;
            }
        }
        let mut m = std::mem::take(&mut self.mission);
        m.fail(&mut Adapter(self));
        self.mission = m;
    }
    fn defeated(&mut self, id: u64, owner: &str, calmed: bool, draw: &mut impl FnMut() -> f64) {
        let Some(e) = self.enemies.iter().find(|e| e.target.id == id) else {
            return;
        };
        let p = [e.target.x, e.target.y];
        let radius = e.target.r;
        let boss = e.target.boss;
        let kind = e.kind;
        let split = !calmed && e.kind == enemies::Kind::Slime;
        let extra = e.extra;
        if split {
            for dir in [-1., 1.] {
                let x = self
                    .view
                    .scale
                    .max((self.view.width - self.view.scale).min(p[0] + dir * radius * 0.7));
                self.spawn_baby(
                    [x, 0.],
                    [dir * self.view.scale * 3., -self.view.scale * 6.],
                    dir,
                    extra,
                    draw,
                );
            }
            if !extra {
                self.mission.total += 2;
            }
        }
        if !extra {
            self.mission.kills += 1;
            if owner != "remote" {
                self.kills += 1;
            }
        }
        self.defeat_fx(kind, p, radius, calmed, draw);
        if boss {
            for e in &mut self.enemies {
                if e.extra && !e.target.dead {
                    e.target.calm = true;
                }
            }
        } else if owner != "wild" && owner != "remote" {
            self.add_xp(Some(owner), 4.);
        }
        if self.net.enabled {
            self.net.events.push(crate::coop::GameEvent::Defeat {
                id,
                kind,
                at: p,
                calmed,
                by: if owner == "remote" {
                    self.net.remote_by.unwrap_or(self.net.rider)
                } else {
                    self.net.rider
                },
            });
        }
    }
    fn defeat_fx(
        &mut self,
        kind: enemies::Kind,
        p: [f64; 2],
        radius: f64,
        calmed: bool,
        draw: &mut impl FnMut() -> f64,
    ) {
        let boss = kind.definition().boss;
        self.sound(
            if kind.definition().ground && !kind.definition().hop && !calmed {
                sound::Event::Grr
            } else {
                sound::Event::Poof
            },
        );
        if !calmed {
            self.hit_stop = self.hit_stop.max(if boss { 0.12 } else { 0.05 });
        }
        let count = if boss {
            8
        } else if calmed {
            kind.definition().gems.div_ceil(2).min(6)
        } else {
            kind.definition().gems.min(6)
        };
        for _ in 0..count {
            let x = self.view.scale.max(
                (self.view.width - self.view.scale).min(p[0] + random(draw, -0.5, 0.5) * radius),
            );
            let y = (self.view.top_limit + self.view.scale).max(
                (self.view.ground_y - self.view.scale * 0.5).min(p[1] - self.view.scale * 0.3),
            );
            let vx = random(draw, -2.5, 2.5) * self.view.scale;
            let vy = -self.view.scale * random(draw, 1., 3.);
            self.gems.push(make_gem(x, y, vx, vy, self.view, draw));
        }
        if boss {
            for _ in 0..3 {
                let x = p[0] + random(draw, -1., 1.) * radius;
                let y = p[1] - self.view.scale;
                let vx = random(draw, -3., 3.) * self.view.scale;
                let vy = -self.view.scale * random(draw, 2., 4.);
                // gold=true short-circuits the golden draw and color selection.
                self.gems.push(Gem {
                    x,
                    y,
                    vx,
                    vy,
                    t: draw() * 6.,
                    life: 10.,
                    golden: true,
                    color: "#ffd76e".into(),
                    value: 3.,
                    r: self.view.scale * 0.3,
                    lost_one: false,
                });
            }
            self.sound(sound::Event::Win);
            self.add_xp(None, 80.);
        }
    }
    fn enemy_effects(
        &mut self,
        effects: Vec<enemies::Event>,
        view: Viewport,
        draw: &mut impl FnMut() -> f64,
    ) {
        let mut bubbles = 0;
        let mut slam = false;
        let mut fall = false;
        for e in effects {
            match e {
                enemies::Event::Strike { x, y0, warn, bolt } => {
                    let uid = self.next_id();
                    self.strikes.push(Strike {
                        uid,
                        x,
                        y0,
                        warn,
                        bolt,
                        hit: false,
                    });
                }
                enemies::Event::Touch { id, hurts } => {
                    if !damage::is_safe(&self.player) {
                        let t = &self
                            .enemies
                            .iter()
                            .find(|e| e.target.id == id)
                            .expect("touch target exists")
                            .target;
                        let p = [t.x, t.y];
                        if hurts {
                            let armor = self.craft("armor");
                            damage::bonk(&mut self.player, p, view.scale, armor);
                            self.sound(sound::Event::Bonk);
                            self.bonks += 1;
                            self.squash = 0.3;
                            self.hit_stop = self.hit_stop.max(0.06);
                            self.drop_carried_piece(draw);
                            self.retire(draw);
                        } else {
                            damage::bump(&mut self.player, p, view.scale);
                            self.sound(sound::Event::Thud);
                            self.squash = 0.25;
                        }
                    }
                }
                enemies::Event::Spark {
                    position,
                    velocity,
                    radius,
                    life,
                } => {
                    let uid = self.next_id();
                    self.hazards.push(Hazard {
                        uid,
                        x: position[0],
                        y: position[1],
                        vx: velocity[0],
                        vy: velocity[1],
                        r: radius,
                        life,
                        kind: "spark".into(),
                        ..Hazard::default()
                    });
                }
                enemies::Event::Spawn { kind, extra, at } => {
                    self.spawn(kind, extra, Some(at), draw)
                }
                enemies::Event::SpawnSea { kind, extra } => self.spawn(kind, extra, None, draw),
                enemies::Event::SpawnBaby {
                    at,
                    velocity,
                    dir,
                    extra,
                } => self.spawn_baby(at, velocity, dir, extra, draw),
                enemies::Event::Hazard(h) => {
                    use enemies::HazardKind;
                    match h.kind {
                        HazardKind::Rock | HazardKind::Boulder => self.sound(sound::Event::Throw),
                        HazardKind::Bubble => {
                            bubbles += 1;
                            if bubbles % 3 == 0 {
                                self.sound(sound::Event::Throw);
                            }
                        }
                        HazardKind::Ember if !slam => {
                            slam = true;
                            self.sound(sound::Event::Smash);
                        }
                        HazardKind::Fall if !fall => {
                            fall = true;
                            self.sound(sound::Event::Smash);
                        }
                        _ => {}
                    }
                    let uid = self.next_id();
                    self.hazards.push(Hazard {
                        uid,
                        kind: serde_json::to_value(h.kind)
                            .expect("hazard kind")
                            .as_str()
                            .expect("kind string")
                            .into(),
                        x: h.position[0],
                        y: h.position[1],
                        vx: h.velocity[0],
                        vy: h.velocity[1],
                        gravity: h.gravity,
                        r: h.radius,
                        life: h.life,
                        warn: h.warn,
                        spin: 0.,
                    });
                }
                enemies::Event::Gobble { gem_id } => {
                    self.gems.remove(gem_id as usize);
                }
                enemies::Event::Sting { id } => {
                    if !damage::is_safe(&self.player) {
                        let t = &self
                            .enemies
                            .iter()
                            .find(|e| e.target.id == id)
                            .expect("sting target")
                            .target;
                        let pos = [t.x, t.y];
                        let armor = self.craft("armor");
                        damage::bonk(&mut self.player, pos, view.scale, armor);
                        self.sound(sound::Event::Bonk);
                        self.bonks += 1;
                        self.squash = 0.3;
                        self.hit_stop = self.hit_stop.max(0.06);
                        self.drop_carried_piece(draw);
                        self.retire(draw);
                    }
                }
                event @ (enemies::Event::Ring { .. } | enemies::Event::Splash { .. }) => {
                    if matches!(&event, enemies::Event::Ring { .. }) {
                        self.sound(sound::Event::Zap);
                    }
                    self.presentation_events.push(event)
                }
            }
        }
    }
    /// Clamp frame duration like frame(now). Zero duration still runs collisions and
    /// ready fire/spawn callbacks, including during hit-stop. Pause by not calling.
    pub fn step(&mut self, elapsed: f64, input: Input, view: Viewport) -> Result<(), &'static str> {
        self.step_network(elapsed, input, view, &mut |_, _| false)
    }
    pub(crate) fn step_network(
        &mut self,
        elapsed: f64,
        input: Input,
        view: Viewport,
        before_mission: &mut impl FnMut(&mut Self, &mut dyn FnMut() -> f64) -> bool,
    ) -> Result<(), &'static str> {
        if !elapsed.is_finite()
            || !valid_view(view)
            || input
                .stick
                .is_some_and(|v| !v[0].is_finite() || !v[1].is_finite())
        {
            return Err("invalid frame or viewport");
        }
        if self.result.is_some() {
            return Ok(());
        }
        self.objective_events.clear();
        self.presentation_events.clear();
        let mut view = view;
        view.dragon_scale = self.body_scale(view.dragon_scale);
        self.view = view;
        let mut dt = elapsed.clamp(0., 0.05);
        if self.hit_stop > 0. {
            self.hit_stop -= dt;
            dt = 0.;
        }
        self.sound_state.tick(dt);
        self.time += dt;
        let mut rng = self.rng;
        let mut draw = || rng.next_unit();
        let origin = [self.player.x, self.player.y];
        let events = self.player.step(
            dt,
            input,
            view,
            FlightRules {
                mission_done: self.mission.done,
                tea: self.craft("tea"),
                magic: self.skill("magic") >= 1,
                max_energy: abilities::max_energy(self.skill("magic")),
            },
        )?;
        if events.mega_blast {
            self.apply_ability_events(abilities::mega_blast(&self.target_snapshot()), &mut draw);
            self.sound(sound::Event::MegaBoom);
        }
        if events.shadow_burst {
            let mut p = self.player.clone();
            p.x = origin[0];
            p.y = origin[1];
            let effects =
                abilities::shadow_burst(&p, &self.ability_rules(), &self.target_snapshot());
            self.apply_ability_events(effects, &mut draw);
            self.sound(sound::Event::Spell);
        }
        if events.shoot {
            self.shoot_all(origin, &mut draw);
        }
        self.view.dragon_scale = self.body_scale(view.dragon_scale);
        self.view.clamp(&mut self.player);
        self.update_wings(dt, &mut draw);
        let world = projectiles::ShotWorld {
            width: view.width,
            height: view.height,
            scale: view.scale,
            player: [self.player.x, self.player.y],
            authority: projectiles::Authority::default(),
        };
        let mut shots = std::mem::take(&mut self.shots);
        projectiles::update_shots_with_world(
            &mut shots,
            &mut CollisionAdapter {
                game: self,
                draw: &mut draw,
            },
            dt,
            world,
        );
        self.shots = shots;
        self.view.dragon_scale = self.body_scale(view.dragon_scale);
        view = self.view;
        let guest_update = self.mission.guest;
        let mut index = 0;
        while index < self.enemies.len() {
            if guest_update {
                let effects = self.mirror_step(index, dt);
                self.enemy_effects(effects, view, &mut draw);
                index += 1;
                continue;
            }
            if !self.enemies[index].target.dead {
                let id = self.enemies[index].target.id;
                let timer = self.target_t.entry(id).or_default();
                *timer -= dt;
                if *timer <= 0. {
                    let objective_available = self
                        .objective
                        .as_ref()
                        .is_some_and(|o| o.kind != objectives::Kind::Carry && !o.done)
                        && !self.enemies[index].target.boss;
                    let local = !self.player.out || self.net.aims.is_empty();
                    let count =
                        usize::from(local) + self.net.aims.len() + usize::from(objective_available);
                    let pick = (draw() * count as f64).floor() as usize;
                    self.target_objective.remove(&id);
                    self.target_rider.remove(&id);
                    if objective_available && pick == count - 1 {
                        self.target_objective.insert(id);
                    } else if pick >= usize::from(local) {
                        self.target_rider
                            .insert(id, self.net.aims[pick - usize::from(local)].id);
                    }
                    *timer = random(&mut draw, 5., 9.);
                }
            }
            let mut world = self.enemy_world();
            if self
                .target_objective
                .contains(&self.enemies[index].target.id)
                && let Some(o) = self
                    .objective
                    .as_ref()
                    .filter(|o| !o.done && o.kind != objectives::Kind::Carry)
            {
                world.aim = [o.x, o.y];
                world.quiet = false;
            }
            if let Some(rider) = self.target_rider.get(&self.enemies[index].target.id)
                && let Some(p) = self.net.aims.iter().find(|p| &p.id == rider)
            {
                world.aim = [p.x, p.y];
                world.quiet = false;
            }
            let mut effects = vec![];
            let gem_context: Vec<_> = self
                .gems
                .iter()
                .enumerate()
                .map(|(id, g)| enemies::Gem {
                    id: id as u64,
                    position: [g.x, g.y],
                    radius: g.r,
                })
                .collect();
            let context = enemies::Context {
                gems: &gem_context,
                sibling_boss_damage: 0.,
                active_extras: self
                    .enemies
                    .iter()
                    .filter(|e| e.extra && !e.target.dead && !e.target.calm)
                    .count(),
            };
            let previous_pre = self.enemies[index].pre;
            let previous_cool2 = self.enemies[index].cool2;
            self.enemies[index]
                .step_with_context(dt, &world, context, &mut draw, &mut effects)
                .map_err(|_| "unsupported enemy")?;
            if previous_pre <= 0. && self.enemies[index].pre > 0. {
                self.sound(sound::Event::Throw);
            }
            if self.enemies[index].kind == enemies::Kind::Swallower
                && self.enemies[index].cool2 > previous_cool2
            {
                self.sound(sound::Event::Smash);
            }
            self.enemy_effects(effects, view, &mut draw);
            index += 1;
        }
        self.enemies.retain(|e| !e.target.dead);
        for e in &self.enemies {
            if self.player.down > 0.
                || e.kind != enemies::Kind::Swallower
                || e.target.calm
                || e.windup <= 0.
            {
                continue;
            }
            let delta = enemies::inhale_displacement(
                e,
                [self.player.x, self.player.y],
                self.player.down > 0.,
                view.scale,
                dt,
            );
            self.player.x += delta[0];
            self.player.y += delta[1];
            view.clamp(&mut self.player);
        }
        self.target_t
            .retain(|id, _| self.enemies.iter().any(|e| e.target.id == *id));
        let active = !self.mission.done && self.mission.fail_t == 0.;
        let armor = self.craft("armor");
        let damage_events = damage::update_hazards(
            &mut self.hazards,
            &mut self.strikes,
            &mut self.player,
            dt,
            view,
            armor,
            &mut self.used,
            |p| {
                if active && p.hearts == 0 {
                    p.out = true;
                    p.dizzy = 0.;
                }
            },
        );
        for event in damage_events {
            match &event {
                damage::DamageEvent::Bonk { .. } => self.sound(sound::Event::Bonk),
                damage::DamageEvent::Zap => self.sound(sound::Event::Zap),
                _ => {}
            }
            if matches!(event, damage::DamageEvent::Bonk { .. }) {
                self.bonks += 1;
                self.squash = 0.3;
                self.hit_stop = self.hit_stop.max(0.06);
                self.drop_carried_piece(&mut draw);
            }
        }
        self.retire(&mut draw);
        self.update_allies(dt, &mut draw);
        let carry = &mut self.mission.carry;
        let earned = &mut self.earned;
        let xp = &mut self.xp;
        let failing = self.mission.fail_t != 0.;
        let sweeping = self.mission.done && !failing;
        let team_profiles = &self.team_profiles;
        let profile = &mut self.profile;
        let progression_events = &mut self.progression_events;
        let sound_events = &mut self.sound_events;
        let sound_state = &mut self.sound_state;
        let followers: Vec<_> = self.wings.values().map(|w| [w.x, w.y]).collect();
        gems::update_gems(
            &mut self.gems,
            dt,
            &self.player,
            view,
            sweeping,
            &followers,
            |gem| {
                sound::push(sound_events, sound_state.collect(gem.golden));
                if failing {
                    *earned += gem.value;
                    if let Some(p) = profile.as_mut() {
                        p["earned"] = json!(*earned);
                    }
                } else {
                    *carry += gem.value;
                }
                let multiplier = profile
                    .as_ref()
                    .map(|p| {
                        (1. + 0.25 * team_craft(p, team_profiles, "tonic"))
                            * if team_hatched(p, team_profiles) >= 16. {
                                1.25
                            } else {
                                1.
                            }
                    })
                    .unwrap_or(1.);
                award_xp(
                    profile,
                    team_profiles,
                    xp,
                    progression_events,
                    sound_events,
                    None,
                    gem.value * multiplier,
                );
            },
        );
        let took_over = before_mission(self, &mut draw);
        let mut m = std::mem::take(&mut self.mission);
        // A timeout takes authority inside the guest branch; this frame still runs
        // guest gem spawning, not host queue/AI work, exactly like updateMission.
        if took_over {
            m.guest = true;
        }
        m.step(dt, self.definition, &mut Adapter(self), &mut draw);
        if took_over {
            m.guest = false;
        }
        self.mission = m;
        if let Some(ok) = self.result
            && let Some(p) = self.profile.as_mut()
        {
            let outcome = roost::complete_mission(
                p,
                &self.team_profiles,
                false,
                &roost::Completion {
                    mission_id: self.definition.id.trim_end_matches('c').into(),
                    champion: self.champion,
                    ok,
                    carry: self.mission.carry,
                    banked: self.mission.banked,
                    bonks: self.bonks as u32,
                    elapsed: self.mission.t,
                    par: self.par,
                    had_objective: self.had_objective,
                    day: self.completion_day,
                    now_ms: self.completion_now_ms,
                },
            )
            .map_err(|_| "invalid profile completion")?;
            self.mission.carry = outcome.carry;
            self.mission.banked = outcome.banked;
            self.earned = number(&p["earned"]);
            self.completion_events = outcome.events;
        }
        let want =
            if self.mission.leg == Leg::Fly && !self.mission.done && self.mission.fail_t == 0. {
                view.scale * 4.2
            } else {
                0.
            };
        self.scroll_v += (want - self.scroll_v) * (dt * 1.5).min(1.);
        self.scroll += self.scroll_v * dt;
        self.rain = (self.rain - dt).max(0.);
        self.rng = rng;
        Ok(())
    }
}
fn valid_view(v: Viewport) -> bool {
    [
        v.width,
        v.height,
        v.scale,
        v.ground_y,
        v.top_limit,
        v.dragon_scale,
    ]
    .iter()
    .all(|x| x.is_finite())
        && v.height > 0.
        && v.width > v.scale * 2.
        && v.scale > 0.
        && v.ground_y > v.top_limit + v.scale * 2.
        && v.dragon_scale > 0.
}
struct Adapter<'a>(&'a mut Game);
impl MissionWorld for Adapter<'_> {
    fn snapshot(&self) -> WorldSnapshot {
        let g = &self.0;
        WorldSnapshot {
            player_out: g.player.out,
            active_others: g.net.active_others,
            regular_uncalm: g
                .enemies
                .iter()
                .filter(|e| !e.target.calm && !e.extra)
                .count(),
            extra_alive_uncalm: g
                .enemies
                .iter()
                .filter(|e| e.extra && !e.target.dead && !e.target.calm)
                .count(),
            gems: g.gems.len(),
            boss: g
                .enemies
                .iter()
                .find(|e| e.target.boss && !e.target.dead && !e.target.calm)
                .map(|e| missions::BossSnapshot {
                    hp: e.target.hp,
                    max_hp: e.target.max_hp,
                }),
            carry_pending: g
                .objective
                .as_ref()
                .is_some_and(|o| o.kind == objectives::Kind::Carry && !o.done),
            carrier_present: g.objective.as_ref().is_some_and(|o| o.carrier >= 0),
            objective_failed: g.objective_failed,
            width: g.view.width,
            scale: g.view.scale,
            top_limit: g.view.top_limit,
            sky_bottom: g.sky_bottom(),
            scroll_v: g.scroll_v,
            ..WorldSnapshot::default()
        }
    }
    fn apply(&mut self, effect: Effect) {
        let g = &mut self.0;
        match effect {
            Effect::Bank(n) => {
                g.sound(sound::Event::Gold);
                g.earned += n;
                if let Some(p) = g.profile.as_mut() {
                    p["earned"] = json!(g.earned);
                }
            }
            Effect::WaveBanner { wave, .. } if wave > 0 => g.sound(sound::Event::Wave),
            Effect::BossPhase(_) => g.sound(sound::Event::Wave),
            Effect::Complete => g.sound(sound::Event::Win),
            Effect::Failed => g.sound(sound::Event::Wrong),
            Effect::ClearObjective => {
                g.objective = None;
                g.objective_failed = false;
            }
            Effect::MakeObjective(stop) => {
                g.objective =
                    objectives::specification(g.definition.id.trim_end_matches('c'), stop)
                        .map(|s| Objective::new(&s, g.view));
            }
            Effect::SpeedBoss(k) => {
                if let Some(e) = g
                    .enemies
                    .iter_mut()
                    .find(|e| e.target.boss && !e.target.dead && !e.target.calm)
                {
                    e.speed *= k;
                }
            }
            Effect::CalmEnemies => {
                for e in &mut g.enemies {
                    if !e.target.dead {
                        e.target.calm = true;
                    }
                }
            }
            Effect::ClearHazardsAndStrikes => {
                g.hazards.clear();
                g.strikes.clear();
            }
            Effect::MoveGems(dx) => {
                for gem in &mut g.gems {
                    gem.x += dx;
                }
            }
            Effect::End(ok) => g.result = Some(ok),
            _ => {}
        }
    }
    fn apply_random(&mut self, effect: Effect, draw: &mut impl FnMut() -> f64) {
        match effect {
            Effect::Spawn { kind, extra, at } => {
                let kind = serde_json::from_value(serde_json::Value::String(kind))
                    .expect("authored enemy kind");
                self.0.spawn(kind, extra, at, draw);
            }
            Effect::SpawnGem => {
                let v = self.0.view;
                let x = random(draw, v.scale, v.width - v.scale);
                let y = random(
                    draw,
                    v.top_limit + v.scale * 0.6,
                    (v.top_limit + v.scale).max(v.ground_y - v.scale * 0.6),
                );
                self.0.gems.push(make_gem(x, y, 0., 0., v, draw));
            }
            Effect::UpdateObjective(dt) => self.0.update_objective(dt, draw),
            other => self.apply(other),
        }
    }
}

impl Game {
    fn update_objective(&mut self, dt: f64, draw: &mut impl FnMut() -> f64) {
        let Some(ob) = self.objective.as_mut() else {
            return;
        };
        self.had_objective = true;
        let mut riders = if self.player.out {
            vec![]
        } else {
            vec![objectives::Rider {
                id: self.net.rider as i32,
                x: self.player.x,
                y: self.player.y,
            }]
        };
        riders.extend(self.net.others.iter().cloned());
        let mut enemies: Vec<_> = self
            .enemies
            .iter()
            .map(|e| objectives::Enemy {
                id: e.target.id,
                kind: serde_json::to_value(e.kind)
                    .expect("kind serializes")
                    .as_str()
                    .expect("kind string")
                    .into(),
                x: e.target.x,
                y: e.target.y,
                r: e.target.r,
                dir: e.dir,
                dead: e.target.dead,
                calm: e.target.calm,
                boss: e.target.boss,
                stun: e.target.stun,
                ground: e.kind.definition().ground,
                ob_cd: *self.objective_cd.get(&e.target.id).unwrap_or(&0.),
            })
            .collect();
        let mut hazards: Vec<_> = self
            .hazards
            .iter()
            .map(|h| objectives::Hazard {
                id: h.uid,
                kind: h.kind.clone(),
                x: h.x,
                y: h.y,
                r: h.r,
                warn: h.warn,
            })
            .collect();
        let mut strikes: Vec<_> = self
            .strikes
            .iter()
            .map(|s| objectives::Strike {
                id: s.uid,
                x: s.x,
                y0: s.y0,
                warn: s.warn,
                ob_hit: self.objective_strikes.contains(&s.uid),
            })
            .collect();
        let sound_start = self.objective_events.len();
        ob.update(
            dt,
            &mut objectives::World {
                view: self.view,
                mission_done: false,
                mission_failing: false,
                riders: &riders,
                enemies: &mut enemies,
                hazards: &mut hazards,
                strikes: &mut strikes,
            },
            draw,
            &mut self.objective_events,
        );
        for (enemy, updated) in self.enemies.iter_mut().zip(enemies) {
            enemy.target.x = updated.x;
            self.objective_cd.insert(updated.id, updated.ob_cd);
        }
        self.objective_cd
            .retain(|id, _| self.enemies.iter().any(|e| e.target.id == *id));
        self.hazards
            .retain(|h| hazards.iter().any(|kept| kept.id == h.uid));
        self.objective_strikes = strikes
            .into_iter()
            .filter(|s| s.ob_hit)
            .map(|s| s.id)
            .collect();
        for event in &self.objective_events[sound_start..] {
            let cue = match event {
                objectives::Event::Hurt { .. } => Some(sound::Event::Thud),
                objectives::Event::Delivered(_) => Some(sound::Event::Win),
                objectives::Event::PickedUp(_) => Some(sound::Event::Gold),
                objectives::Event::Stolen(_) => Some(sound::Event::Grr),
                _ => None,
            };
            if let Some(cue) = cue {
                sound::push(&mut self.sound_events, cue);
            }
            if let objectives::Event::Failed(reason) = event {
                self.objective_failed = true;
                self.fail_reason = Some(reason.clone());
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Wing {
    pub x: f64,
    pub y: f64,
    pub t: f64,
}
#[derive(Clone)]
struct DragonInfo {
    id: String,
    kind: BreathKind,
    xp: f64,
    bonded: bool,
}
fn number(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.)
}
// Combat reads only scalars; do not allocate a JSON team summary per shot/hit.
fn team_craft(me: &Value, others: &[Value], key: &str) -> f64 {
    std::iter::once(me)
        .chain(others)
        .map(|p| number(&p["craft"][key]))
        .sum::<f64>()
        .min(3.)
}
fn team_hatched(me: &Value, others: &[Value]) -> f64 {
    std::iter::once(me)
        .chain(others)
        .map(|p| number(&p["hatched"]))
        .sum()
}
fn team_riding(me: &Value, others: &[Value]) -> bool {
    crate::level_of(number(&me["party"][0]["xp"])) >= 8
        && std::iter::once(me)
            .chain(others)
            .any(|p| p["craft"]["saddles"] == true)
}
fn stage(xp: f64) -> usize {
    match crate::level_of(xp) {
        12.. => 3,
        8.. => 2,
        4.. => 1,
        _ => 0,
    }
}
fn main_index(p: &Value) -> usize {
    p["party"]
        .as_array()
        .and_then(|a| a.iter().position(|d| d["id"] == p["main"]))
        .unwrap_or(0)
}
fn wing_index(p: &Value) -> Option<usize> {
    let a = p["party"].as_array()?;
    let m = main_index(p);
    if a.get(m)?["bonded"] != true {
        Some(0)
    } else {
        (0..a.len()).find(|i| *i != m)
    }
}
fn award_xp(
    profile: &mut Option<Value>,
    others: &[Value],
    xp: &mut f64,
    events: &mut Vec<progression::Event>,
    sounds: &mut Vec<sound::Event>,
    owner: Option<&str>,
    amount: f64,
) {
    if let Some(p) = profile.as_mut() {
        let ids: Vec<String> = if let Some(owner) = owner {
            let party = p["party"].as_array().expect("validated profile party");
            vec![
                party.iter().find(|d| d["id"] == owner).unwrap_or(&party[0])["id"]
                    .as_str()
                    .expect("validated id")
                    .into(),
            ]
        } else {
            p["party"]
                .as_array()
                .expect("validated party")
                .iter()
                .map(|d| d["id"].as_str().expect("validated id").into())
                .collect()
        };
        for dragon in ids {
            let growth = progression::apply(
                p,
                others,
                false,
                progression::Command::AddXp { dragon, amount },
                0.,
            )
            .expect("validated in-memory XP command");
            for event in &growth {
                if matches!(event,progression::Event::Growth {stage_before,stage_after,..} if stage_after>stage_before)
                {
                    sound::push(sounds, sound::Event::Grow);
                }
            }
            events.extend(growth);
        }
        *xp = number(&p["party"][0]["xp"]);
    } else {
        *xp = (*xp + amount).min(14440.);
    }
}
impl Game {
    /// Takes an already-normalized, synthetic or explicitly supplied profile by value.
    /// Mutations remain in memory. No persistence, account, or browser access occurs.
    pub fn from_profile(
        id: &str,
        profile: Value,
        view: Viewport,
        seed: u32,
    ) -> Result<Self, &'static str> {
        Self::from_profile_team(id, profile, view, seed, vec![])
    }
    pub fn set_live_team(&mut self, profiles: Vec<Value>) {
        self.team_profiles = profiles;
    }
    pub fn from_profile_team(
        id: &str,
        profile: Value,
        view: Viewport,
        seed: u32,
        team_profiles: Vec<Value>,
    ) -> Result<Self, &'static str> {
        Self::from_profile_team_at(id, profile, view, seed, team_profiles, 0)
    }
    /// Fresh run from a validated reached stop. No old world or reward counters are copied.
    pub fn from_profile_team_at(
        id: &str,
        profile: Value,
        view: Viewport,
        seed: u32,
        team_profiles: Vec<Value>,
        from_wave: usize,
    ) -> Result<Self, &'static str> {
        let def = definitions()
            .iter()
            .find(|d| d.id == id)
            .ok_or("unknown mission")?;
        if !valid_view(view)
            || from_wave >= def.waves.len()
            || (def.id.ends_with('c') && from_wave != 0)
        {
            return Err("invalid retry stop");
        }
        let party = profile["party"]
            .as_array()
            .ok_or("profile must be normalized")?;
        if party.is_empty()
            || party[0]["bonded"] != true
            || !profile["craft"].is_object()
            || !profile["skills"].is_object()
        {
            return Err("profile must be normalized");
        }
        for d in party {
            if d["id"].as_str().is_none()
                || d["xp"].as_f64().is_none()
                || !matches!(
                    d["kind"].as_str(),
                    Some("bond" | "flame" | "spark" | "boulder" | "frost" | "star")
                )
            {
                return Err("invalid normalized dragon");
            }
        }
        let mut g = Self::with_definition_at(def, view, seed, from_wave);
        g.earned = number(&profile["earned"]);
        g.xp = number(&profile["party"][0]["xp"]);
        g.profile = Some(profile);
        g.team_profiles = team_profiles;
        // Source startMission computes starting hearts before consuming this run's charms.
        g.player.hearts = g.max_hearts();
        g.player.energy = abilities::max_energy(g.skill("magic"));
        let p = g.profile.as_mut().expect("profile just assigned");
        p["lastRealm"] =
            json!(["hollow", "wood", "frost", "sea", "ember", "stars"][g.definition.realm]);
        let mut consume = |key: &str| {
            let n = number(&p["charms"][key]);
            if n > 0. {
                p["charms"][key] = json!(n - 1.);
                true
            } else {
                false
            }
        };
        g.heart_charm = consume("heart");
        g.pouch_charm = consume("pouch");
        if consume("start") {
            g.player.shield = 10.;
            g.player.magnet = 10.;
        }
        g.view.dragon_scale = g.body_scale(view.dragon_scale);
        g.view.clamp(&mut g.player);
        Ok(g)
    }
    /// Apply a requested in-memory progression command; timestamp is supplied by the caller.
    pub fn apply_progression(
        &mut self,
        command: progression::Command,
        now_ms: f64,
    ) -> Result<Vec<progression::Event>, progression::Error> {
        let p = self
            .profile
            .as_mut()
            .ok_or(progression::Error::InvalidProfile)?;
        let events = progression::apply(p, &self.team_profiles, false, command, now_ms)?;
        self.xp = number(&p["party"][0]["xp"]);
        self.earned = number(&p["earned"]);
        self.view.dragon_scale = self.body_scale(self.view.dragon_scale);
        self.progression_events.extend(events.clone());
        Ok(events)
    }
    fn craft(&self, key: &str) -> f64 {
        self.profile
            .as_ref()
            .map(|p| team_craft(p, &self.team_profiles, key))
            .unwrap_or(0.)
    }
    fn skill(&self, key: &str) -> u8 {
        self.profile
            .as_ref()
            .map(|p| number(&p["skills"][key]) as u8)
            .unwrap_or(0)
    }
    fn daddio(&self) -> bool {
        self.profile
            .as_ref()
            .and_then(|p| p["build"]["name"].as_str())
            .is_some_and(|n| n.trim().eq_ignore_ascii_case("daddio"))
    }
    fn body_scale(&self, fallback: f64) -> f64 {
        if self.profile.is_some() {
            crate::dragon_scale(self.xp, self.daddio())
        } else {
            fallback
        }
    }
    pub fn max_hearts(&self) -> u8 {
        5 + self.craft("armor") as u8
            + u8::from(self.skill("rider") >= 4)
            + u8::from(self.heart_charm)
    }
    pub fn dragon_scale(&self) -> f64 {
        self.body_scale(self.view.dragon_scale)
    }
    fn add_xp(&mut self, owner: Option<&str>, amount: f64) {
        award_xp(
            &mut self.profile,
            &self.team_profiles,
            &mut self.xp,
            &mut self.progression_events,
            &mut self.sound_events,
            owner,
            amount,
        );
        self.view.dragon_scale = self.body_scale(self.view.dragon_scale);
    }
    fn dragon(&self, index: usize) -> DragonInfo {
        if let Some(p) = &self.profile {
            let d = &p["party"][index];
            DragonInfo {
                id: d["id"].as_str().expect("validated id").into(),
                kind: serde_json::from_value(d["kind"].clone()).expect("validated kind"),
                xp: number(&d["xp"]),
                bonded: d["bonded"] == true,
            }
        } else {
            DragonInfo {
                id: "bond".into(),
                kind: BreathKind::Bond,
                xp: 0.,
                bonded: true,
            }
        }
    }
    fn breath(
        &mut self,
        d: DragonInfo,
        follower: bool,
        origin: [f64; 2],
        draw: &mut impl FnMut() -> f64,
    ) -> f64 {
        let b = Breath {
            kind: d.kind,
            stage: stage(d.xp),
            owner: d.id.clone(),
            follower,
            skill: self.skill("breath"),
            bellows: self.craft("bellows"),
            dad: !follower && self.daddio(),
        };
        let scale = if d.bonded {
            self.body_scale(self.view.dragon_scale)
        } else {
            crate::dragon_scale(d.xp, false)
        };
        let s = self.view.scale * scale;
        let pos = if d.bonded {
            origin
        } else {
            self.wings.get(&d.id).map(|w| [w.x, w.y]).unwrap_or(origin)
        };
        self.shots.extend(b.emit(
            Mouth {
                x: pos[0] + f64::from(self.player.facing) * s * 0.95,
                y: pos[1] - s * 0.2,
                facing: f64::from(self.player.facing),
            },
            self.view.scale,
            [self.player.vx, self.player.vy],
            draw,
        ));
        if !follower {
            self.sound(sound::Event::Breath(d.kind));
        }
        b.cooldown()
    }
    fn shoot_all(&mut self, origin: [f64; 2], draw: &mut impl FnMut() -> f64) {
        let tea = 1. - 0.08 * self.craft("tea");
        let bs = self.skill("breath");
        let main = self.profile.as_ref().map(main_index).unwrap_or(0);
        if self.player.main_cool <= 0. {
            let d = self.dragon(main);
            self.player.main_cool =
                self.breath(d, false, origin, draw) * tea * if bs >= 3 { 0.85 } else { 1. };
            self.player.fire_flash = 0.12;
        }
        if let Some(i) = self.profile.as_ref().and_then(wing_index) {
            let d = self.dragon(i);
            let cool = self
                .player
                .follow_cool
                .entry(d.id.clone())
                .or_insert_with(|| random(draw, 0., 0.6));
            if *cool <= 0. {
                let id = d.id.clone();
                let cd =
                    self.breath(d, true, origin, draw) * (if bs >= 4 { 1.85 } else { 2.6 }) * tea;
                self.player.follow_cool.insert(id, cd);
            }
        }
        let riding = self
            .profile
            .as_ref()
            .is_some_and(|p| team_riding(p, &self.team_profiles));
        if riding && self.player.weapon_cool <= 0. {
            let path = self.path();
            self.shots.push(path.weapon(
                origin,
                f64::from(self.player.facing),
                self.view.scale,
                self.body_scale(self.view.dragon_scale),
                self.craft("bellows"),
            ));
            self.sound(sound::Event::Weapon(path));
            self.player.weapon_cool = path.cooldown();
        }
    }
    fn update_wings(&mut self, dt: f64, draw: &mut impl FnMut() -> f64) {
        let Some(p) = &self.profile else {
            return;
        };
        let main = main_index(p);
        let ids: Vec<(usize, String)> = p["party"]
            .as_array()
            .expect("validated party")
            .iter()
            .enumerate()
            .filter(|(_, d)| d["bonded"] != true)
            .map(|(i, d)| (i, d["id"].as_str().expect("id").into()))
            .collect();
        let s = self.view.scale;
        let facing = f64::from(self.player.facing);
        let mut trail = 0.;
        for (i, id) in ids {
            let w = self.wings.entry(id).or_insert_with(|| Wing {
                x: self.player.x - facing * s,
                y: self.player.y,
                t: draw() * 6.,
            });
            let (tx, ty) = if i == main {
                (
                    self.player.x + facing * s * 1.3,
                    self.player.y - s * 0.15 + (self.time * 2.).sin() * s * 0.08,
                )
            } else {
                let at = (
                    self.player.x - facing * s * (1.25 + trail * 0.85),
                    self.player.y + s * 0.3 + (self.time * 2.2 + trail).sin() * s * 0.18,
                );
                trail += 1.;
                at
            };
            w.x += (tx - w.x) * (dt * 5.).min(1.);
            w.y += (ty - w.y) * (dt * 5.).min(1.);
            w.t += dt;
        }
    }
    fn path(&self) -> Path {
        match self
            .profile
            .as_ref()
            .and_then(|p| p["build"]["path"].as_str())
        {
            Some("forge") => Path::Forge,
            Some("storm") => Path::Storm,
            Some("guardian") => Path::Guardian,
            _ => Path::Shadow,
        }
    }
    fn target_snapshot(&self) -> Vec<Target> {
        self.enemies.iter().map(|e| e.target.clone()).collect()
    }
    fn ability_rules(&self) -> abilities::Rules {
        let words = if let Some(p) = &self.profile {
            let cleared: Vec<String> = p["cleared"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let loadout: Vec<abilities::Spell> = p["loadout"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| serde_json::from_value(v.clone()).ok())
                        .collect()
                })
                .unwrap_or_default();
            let rank = ["m4", "m8", "m12", "m16", "m20", "m24"]
                .iter()
                .enumerate()
                .filter(|(_, m)| cleared.iter().any(|c| c == **m))
                .map(|(i, _)| i + 1)
                .max()
                .unwrap_or(0);
            abilities::active_words(Some((&cleared, &loadout, rank)))
        } else {
            vec![abilities::Spell::Naina]
        };
        abilities::Rules {
            playing: self.result.is_none(),
            has_player: true,
            daddio: self.daddio(),
            path: self.path(),
            rider_skill: self.skill("rider"),
            magic_skill: self.skill("magic"),
            bellows: self.craft("bellows"),
            scale: self.view.scale,
            max_hearts: self.max_hearts(),
            active_words: words,
        }
    }
    pub fn active_words(&self) -> Vec<abilities::Spell> {
        self.ability_rules().active_words
    }
    pub fn select_main(&mut self, index: usize) -> Result<(), progression::Error> {
        let id = self
            .profile
            .as_ref()
            .and_then(|p| p["party"].get(index))
            .and_then(|d| d["id"].as_str())
            .ok_or(progression::Error::UnknownDragon)?
            .to_owned();
        self.apply_progression(progression::Command::SelectMain(id), self.time * 1000.)?;
        Ok(())
    }
    pub fn swap_main(&mut self) -> Result<(), progression::Error> {
        self.apply_progression(progression::Command::Swap, self.time * 1000.)?;
        Ok(())
    }
    pub fn special(&mut self) {
        let r = self.ability_rules();
        let targets = self.target_snapshot();
        let before = self.player.special_cd;
        let effects = abilities::do_special(&mut self.player, &r, &targets);
        let mut rng = self.rng;
        self.apply_ability_events(effects, &mut || rng.next_unit());
        if before <= 0. && self.player.special_cd > 0. {
            self.sound(if r.daddio {
                sound::Event::MegaLaunch
            } else {
                match r.path {
                    Path::Shadow => sound::Event::Shadow,
                    Path::Guardian => sound::Event::Spell,
                    _ => sound::Event::Smash,
                }
            });
        }
        self.rng = rng;
    }
    pub fn cast(&mut self, spell: abilities::Spell) {
        let r = self.ability_rules();
        let targets = self.target_snapshot();
        let before = self.player.energy;
        let effects = abilities::cast_spell(&mut self.player, &r, &targets, spell);
        let mut rng = self.rng;
        self.apply_ability_events(effects, &mut || rng.next_unit());
        if self.player.energy < before {
            self.sound(sound::Event::Spell);
        }
        self.rng = rng;
    }
    pub fn dad_power(&mut self, power: abilities::DadPower) {
        let r = self.ability_rules();
        let targets = self.target_snapshot();
        let mut rng = self.rng;
        let effects =
            abilities::dad_power(&mut self.player, &r, &targets, power, || rng.next_unit());
        let accepted = !effects.is_empty();
        if accepted && power == abilities::DadPower::Said {
            self.sound(sound::Event::DadBoom);
        }
        self.apply_ability_events(effects, &mut || rng.next_unit());
        if accepted && power == abilities::DadPower::Joke {
            self.sound(sound::Event::Rimshot);
        }
        self.rng = rng;
    }
    fn apply_ability_events(
        &mut self,
        effects: Vec<abilities::Event>,
        draw: &mut impl FnMut() -> f64,
    ) {
        for effect in effects {
            match effect {
                abilities::Event::Hit { id, shot } => {
                    if let Some(i) = self.enemies.iter().position(|e| e.target.id == id) {
                        self.hit(
                            i,
                            &shot,
                            projectiles::Authority::default(),
                            &mut vec![],
                            draw,
                        );
                    }
                }
                abilities::Event::Calm { id } => {
                    if self.mission.guest {
                        self.net.events.push(crate::coop::GameEvent::Calm(id));
                        continue;
                    }
                    if let Some(e) = self
                        .enemies
                        .iter_mut()
                        .find(|e| e.target.id == id && !e.target.dead && !e.target.calm)
                    {
                        e.target.calm = true;
                        self.defeated(id, "bond", true, draw);
                    }
                }
                abilities::Event::GemRing { count, rich } => {
                    for i in 0..count {
                        let a = f64::from(i) / f64::from(count) * std::f64::consts::TAU;
                        let s = self.view.scale;
                        let x = s.max((self.view.width - s).min(self.player.x + a.cos() * s * 2.));
                        let y = (self.view.top_limit + s * 0.5).max(
                            (self.view.ground_y - s * 0.5).min(self.player.y + a.sin() * s * 1.3),
                        );
                        let gem = if rich {
                            Gem {
                                x,
                                y,
                                t: draw() * 6.,
                                life: 10.,
                                golden: true,
                                color: "#ffd76e".into(),
                                value: 3.,
                                r: s * 0.3,
                                ..Gem::default()
                            }
                        } else {
                            make_gem(x, y, 0., 0., self.view, draw)
                        };
                        self.gems.push(gem);
                    }
                }
                abilities::Event::Cast { key } => {
                    self.cast_count += 1;
                    self.last_cast = Some(key);
                }
                abilities::Event::Rain { duration } => self.rain = duration,
                abilities::Event::TooTired => {}
            }
        }
    }
    fn hit(
        &mut self,
        index: usize,
        shot: &Shot,
        authority: projectiles::Authority,
        events: &mut Vec<projectiles::Event>,
        draw: &mut impl FnMut() -> f64,
    ) {
        let enemy = &mut self.enemies[index];
        let e = &mut enemy.target;
        if e.dead || e.calm {
            return;
        }
        if shot.dmg >= 0.6 || shot.mega {
            self.hit_stop = self.hit_stop.max(0.025);
            enemy.squash = enemy.squash.max(0.3);
        }
        let start = events.len();
        let authority = projectiles::Authority {
            guest: self.mission.guest,
            ..authority
        };
        // Classify the pre-hit effective damage directly: subtracting remaining HP
        // can round an exact 0.6 hit below the source sound threshold.
        let mut audible_damage = shot.dmg;
        if e.marked > 0. && !shot.mega {
            audible_damage *= 1.4;
        }
        if e.armor != 0. && !shot.mega && e.cracked <= 0. {
            audible_damage *= match shot.kind {
                BreathKind::Boulder => 1.6,
                BreathKind::Arrow | BreathKind::Hammer | BreathKind::Area => 1.,
                _ => e.armor,
            };
        }
        let cracked = !authority.guest
            && (shot.kind == BreathKind::Hammer || shot.crack)
            && (e.armor != 0. || e.boss)
            && e.cracked <= 0.;
        projectiles::hit_enemy(e, shot, authority, events);
        let thud = !authority.guest
            && !e.boss
            && !e.dead
            && (audible_damage >= 0.6 || self.sound_state.quiet_hit());
        if cracked {
            self.sound(sound::Event::Smash);
        }
        if thud {
            self.sound(sound::Event::Thud);
        }
        for event in &events[start..] {
            if matches!(event, projectiles::Event::GuestHit { .. }) {
                self.net
                    .events
                    .push(crate::coop::GameEvent::Hit(event.clone()));
            }
        }
        let defeated = events[start..].iter().find_map(|e| {
            if let projectiles::Event::Defeat { id, owner } = e {
                Some((*id, owner.clone()))
            } else {
                None
            }
        });
        if let Some((id, owner)) = defeated {
            self.defeated(id, &owner, false, draw);
        }
    }
}
struct CollisionAdapter<'a, F> {
    game: &'a mut Game,
    draw: &'a mut F,
}
impl<F: FnMut() -> f64> projectiles::CollisionWorld for CollisionAdapter<'_, F> {
    fn target_count(&self) -> usize {
        self.game.enemies.len()
    }
    fn target(&self, index: usize) -> &Target {
        &self.game.enemies[index].target
    }
    fn hit(
        &mut self,
        index: usize,
        shot: &Shot,
        authority: projectiles::Authority,
        events: &mut Vec<projectiles::Event>,
    ) {
        self.game.hit(index, shot, authority, events, self.draw);
    }
}

impl Game {
    pub(crate) fn known_mission(id: &str) -> bool {
        definitions().iter().any(|d| d.id == id)
    }
    pub(crate) fn resize_network(&mut self, mut view: Viewport) -> Result<(), &'static str> {
        if !valid_view(view) {
            return Err("invalid viewport");
        }
        view.dragon_scale = self.body_scale(view.dragon_scale);
        self.view = view;
        view.clamp(&mut self.player);
        Ok(())
    }
    pub(crate) fn network_riding(&self) -> bool {
        self.profile
            .as_ref()
            .is_some_and(|p| team_riding(p, &self.team_profiles))
    }
    pub(crate) fn make_mirror(
        &self,
        kind: enemies::Kind,
        at: [f64; 2],
        draw: &mut impl FnMut() -> f64,
    ) -> Enemy {
        Enemy::spawn(
            kind,
            0,
            false,
            Some(at),
            &self.enemy_world(),
            self.scaling(),
            draw,
        )
    }
    pub(crate) fn bank_network_gems(&mut self) {
        let mut m = std::mem::take(&mut self.mission);
        m.bank_gems(&mut Adapter(self));
        self.mission = m;
    }
    pub(crate) fn drop_network_piece(&mut self) {
        if self.mission.guest {
            return;
        }
        let mut rng = self.rng;
        if let Some(o) = self.objective.as_mut() {
            o.drop_piece(
                self.view,
                &mut || rng.next_unit(),
                &mut self.objective_events,
            );
        }
        self.rng = rng;
    }
    pub(crate) fn apply_team_spell(&mut self, spell: abilities::Spell) {
        let rules = self.ability_rules();
        let targets = self.target_snapshot();
        let effects = abilities::apply_spell(&mut self.player, &rules, &targets, spell);
        let mut rng = self.rng;
        self.apply_ability_events(effects, &mut || rng.next_unit());
        self.sound(sound::Event::Spell);
        self.rng = rng;
    }
    pub(crate) fn apply_network_hit(&mut self, hit: crate::network_actions::AdmittedHit) {
        let Some(i) = self.enemies.iter().position(|e| e.target.id == hit.uid) else {
            return;
        };
        self.net.remote_by = Some(hit.rider);
        let mut rng = self.rng;
        let mut draw = || rng.next_unit();
        match hit.action {
            Some(crate::network_actions::HitAction::Hit(shot)) => self.hit(
                i,
                &shot,
                projectiles::Authority::default(),
                &mut vec![],
                &mut draw,
            ),
            Some(crate::network_actions::HitAction::Calm) => {
                self.enemies[i].target.calm = true;
                self.defeated(hit.uid, "remote", true, &mut draw);
            }
            None => {}
        }
        self.net.remote_by = None;
        self.rng = rng;
    }
    pub(crate) fn apply_network_defeat(
        &mut self,
        effects: Vec<crate::network_actions::DefeatEffect>,
    ) {
        use crate::network_actions::DefeatEffect;
        let mut rng = self.rng;
        let mut draw = || rng.next_unit();
        for effect in effects {
            match effect {
                DefeatEffect::RemoveEnemy(id) => {
                    self.enemies.retain(|e| e.target.id != id);
                    self.net.destinations.remove(&id);
                }
                DefeatEffect::Defeat {
                    kind,
                    position,
                    radius,
                    calmed,
                } => self.defeat_fx(kind, position, radius, calmed, &mut draw),
                DefeatEffect::CreditLocalKill { xp } => {
                    self.kills += 1;
                    let main = self
                        .profile
                        .as_ref()
                        .map(|p| self.dragon(main_index(p)).id)
                        .unwrap_or_else(|| "bond".into());
                    self.add_xp(Some(&main), xp as f64);
                }
            }
        }
        self.rng = rng;
    }
    // Same order as updateMirrors. The effect adapter rechecks safety after an
    // earlier touch/bonk, and gem indices refer to the current local gem vector.
    fn mirror_step(&mut self, index: usize, dt: f64) -> Vec<enemies::Event> {
        let e = &mut self.enemies[index];
        let t = &mut e.target;
        let mut effects = vec![];
        e.t += dt;
        t.flash -= dt;
        if let Some(at) = self.net.destinations.get(&t.id) {
            let k = (dt * 12.).min(1.);
            t.x += (at[0] - t.x) * k;
            t.y += (at[1] - t.y) * k;
        }
        if e.kind.definition().ground && !e.air {
            t.y = self.view.ground_y - t.r * 0.95;
        }
        if e.kind == enemies::Kind::Gobbler
            && !t.calm
            && let Some(i) = self
                .gems
                .iter()
                .position(|g| (g.x - t.x).hypot(g.y - t.y) < t.r + g.r)
        {
            effects.push(enemies::Event::Gobble { gem_id: i as u64 });
        }
        let s = self.view.scale * self.view.dragon_scale;
        let distance = (self.player.x - t.x).hypot(self.player.y - t.y);
        let hurts = e.hurt
            || e.sting > 0.
            || (e.air
                && (matches!(e.kind, enemies::Kind::Wolf | enemies::Kind::Leaper)
                    || e.kind.definition().hop))
            || (e.kind == enemies::Kind::Swallower && e.windup > 0.);
        if !t.calm
            && !damage::is_safe(&self.player)
            && !(e.kind == enemies::Kind::Leaper && !e.air)
            && t.stun <= 0.
            && distance < t.r * 0.85 + s * 0.35
        {
            effects.push(enemies::Event::Touch { id: t.id, hurts });
        }
        if e.sting > 0. {
            e.sting -= dt;
        }
        if e.sting > 0.
            && !t.calm
            && !damage::is_safe(&self.player)
            && distance < t.r * 1.9 + s * 0.3
        {
            effects.push(enemies::Event::Sting { id: t.id });
        }
        effects
    }
}

#[cfg(test)]
mod sound_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hollowbrook_sky_is_not_a_ground_realm() {
        let view = Viewport::from_screen(1000., 700., 1., 40., 0.62);
        let mut game = Game::first_flight(view, 1);
        let bottom = view.ground_y - view.scale * 0.5;
        assert_eq!(game.enemy_world().sky_bottom, bottom);
        assert_eq!(Adapter(&mut game).snapshot().sky_bottom, bottom);
    }
}
