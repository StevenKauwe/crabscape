//! Solo Hollowbrook (m1–m4) integration harness, not the full browser game.
//! Uses original update ordering and one explicit gameplay draw stream. Presentation
//! particle/audio draws, party/allies, skills, growth changes, network and persistent
//! result rewards are not ported. Consequently a seed is a Rust replay seed, not a
//! promise to reproduce a browser Math.random session. No personal saves are read.
use crate::{
    combat::{Breath, Kind as BreathKind, Mouth, Shot},
    damage::{self, Hazard, Strike, Used},
    enemies::{self, Enemy},
    flight::{FlightRules, Input, Player, Viewport},
    gems::{self, Gem},
    missions::{self, Effect, Leg, Mission, MissionDef, MissionWorld, WorldSnapshot},
    objectives::{self, Objective},
    projectiles::{self, Target},
    random::Mulberry32,
};
use std::{collections::BTreeMap, sync::OnceLock};
fn definitions() -> &'static [MissionDef] {
    static DEFS: OnceLock<Vec<MissionDef>> = OnceLock::new();
    DEFS.get_or_init(missions::missions)
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
    if !golden {
        let _color = draw();
    }
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
    }
}
pub struct Game {
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
    pub scroll: f64,
    pub scroll_v: f64,
    pub rng: Mulberry32,
    view: Viewport,
    uid: u64,
    used: Used,
    target_t: BTreeMap<u64, f64>,
    targets: Vec<Target>,
}
impl Game {
    pub fn first_flight(view: Viewport, seed: u32) -> Self {
        Self::with_definition(&definitions()[0], view, seed)
    }
    /// Only the first four missions currently have integrated world callbacks.
    pub fn start(id: &str, view: Viewport, seed: u32) -> Result<Self, &'static str> {
        if !valid_view(view) {
            return Err("invalid viewport");
        }
        let def = definitions()
            .iter()
            .take(4)
            .find(|d| d.id == id)
            .ok_or("mission is not integrated yet")?;
        Ok(Self::with_definition(def, view, seed))
    }
    fn with_definition(def: &'static MissionDef, view: Viewport, seed: u32) -> Self {
        let player = Player {
            x: view.scale * 2.6,
            y: view.ground_y - view.scale * 1.6,
            safe: 1.,
            ..Player::default()
        };
        let mut g = Self {
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
            scroll: 0.,
            scroll_v: 0.,
            rng: Mulberry32 { state: seed },
            view,
            uid: 0,
            used: Used::default(),
            target_t: BTreeMap::new(),
            targets: vec![],
        };
        // Validation is returned by step; do not panic for a malformed initial viewport.
        if valid_view(view) {
            view.clamp(&mut g.player);
        }
        let mut m = std::mem::take(&mut g.mission);
        m.start_leg(def, 0, true, &mut Adapter(&mut g));
        g.mission = m;
        g
    }
    fn enemy_world(&self) -> enemies::World {
        enemies::World {
            width: self.view.width,
            ground_y: self.view.ground_y,
            top: self.view.top_limit,
            sky_bottom: self.view.ground_y - self.view.scale * 0.5,
            scale: self.view.scale,
            player: [self.player.x, self.player.y],
            aim: [self.player.x, self.player.y],
            quiet: self.player.sila > 0.,
            safe: damage::is_safe(&self.player),
            dragon_scale: self.view.dragon_scale,
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
            enemies::MissionScaling {
                difficulty: 1.
                    + (self.definition.id[1..]
                        .parse::<f64>()
                        .expect("authored numeric id")
                        - 1.)
                        * 0.08,
                toughness: 1.15
                    * (1.
                        + 0.06
                            * (self.definition.id[1..]
                                .parse::<f64>()
                                .expect("authored numeric id")
                                - 1.)),
                sky_toughness: 1.,
                speed: 1.,
            },
            draw,
        );
        self.enemies.push(e);
    }
    fn retire(&mut self, draw: &mut impl FnMut() -> f64) {
        if self.player.hearts > 0 || self.mission.done || self.mission.fail_t != 0. {
            return;
        }
        self.player.out = true;
        self.player.dizzy = 0.;
        let lost = (self.mission.carry / 2.).ceil();
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
        let mut m = std::mem::take(&mut self.mission);
        m.fail(&mut Adapter(self));
        self.mission = m;
    }
    fn defeated(&mut self, id: u64, draw: &mut impl FnMut() -> f64) {
        let Some(e) = self.enemies.iter().find(|e| e.target.id == id) else {
            return;
        };
        let p = [e.target.x, e.target.y];
        let radius = e.target.r;
        let boss = e.target.boss;
        let count = if boss {
            8
        } else {
            e.kind.definition().gems.min(6)
        };
        if !e.extra {
            self.mission.kills += 1;
            self.kills += 1;
        }
        self.xp = (self.xp + if boss { 80. } else { 4. }).min(14440.);
        self.hit_stop = self.hit_stop.max(if boss { 0.12 } else { 0.05 });
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
                    value: 3.,
                    r: self.view.scale * 0.3,
                    lost_one: false,
                });
            }
            for e in &mut self.enemies {
                if e.extra && !e.target.dead {
                    e.target.calm = true;
                }
            }
        }
    }
    /// Clamp frame duration like frame(now). Zero duration still runs collisions and
    /// ready fire/spawn callbacks, including during hit-stop. Pause by not calling.
    pub fn step(&mut self, elapsed: f64, input: Input, view: Viewport) -> Result<(), &'static str> {
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
        self.view = view;
        let mut dt = elapsed.clamp(0., 0.05);
        if self.hit_stop > 0. {
            self.hit_stop -= dt;
            dt = 0.;
        }
        let mut rng = self.rng;
        let mut draw = || rng.next_unit();
        let origin = [self.player.x, self.player.y];
        let events = self.player.step(
            dt,
            input,
            view,
            FlightRules {
                mission_done: self.mission.done,
                ..FlightRules::default()
            },
        )?;
        if events.shoot && self.player.main_cool <= 0. {
            let breath = Breath {
                kind: BreathKind::Bond,
                stage: 0,
                owner: "bond".into(),
                follower: false,
                skill: 0,
                bellows: 0.,
                dad: false,
            };
            let s = view.scale * view.dragon_scale;
            self.shots.extend(breath.emit(
                Mouth {
                    x: origin[0] + f64::from(self.player.facing) * s * 0.95,
                    y: origin[1] - s * 0.2,
                    facing: f64::from(self.player.facing),
                },
                view.scale,
                [self.player.vx, self.player.vy],
                &mut draw,
            ));
            self.player.main_cool = breath.cooldown();
            self.player.fire_flash = 0.12;
        }
        // One shot at a time, backwards, so a boss defeat calms adds before the
        // next projectile can hit them. Reuse one-shot storage across the frame.
        let mut pending = std::mem::take(&mut self.shots);
        let mut one = Vec::with_capacity(1);
        while let Some(shot) = pending.pop() {
            one.push(shot);
            self.targets.clear();
            self.targets
                .extend(self.enemies.iter().map(|e| e.target.clone()));
            let events = projectiles::update_shots(
                &mut one,
                &mut self.targets,
                dt,
                projectiles::ShotWorld {
                    width: view.width,
                    height: view.height,
                    scale: view.scale,
                    player: [self.player.x, self.player.y],
                    authority: projectiles::Authority::default(),
                },
            );
            for (e, t) in self.enemies.iter_mut().zip(&self.targets) {
                if t.hp < e.target.hp {
                    self.hit_stop = self.hit_stop.max(0.025);
                }
                e.target.clone_from(t);
            }
            for event in events {
                if let projectiles::Event::Defeat { id, .. } = event {
                    self.defeated(id, &mut draw);
                }
            }
            if let Some(shot) = one.pop() {
                self.shots.push(shot);
            }
        }
        self.shots.reverse();
        let mut index = 0;
        while index < self.enemies.len() {
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
                    if objective_available && draw() >= 0.5 {
                        self.target_objective.insert(id);
                    } else {
                        if !objective_available {
                            let _target = draw();
                        }
                        self.target_objective.remove(&id);
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
            let mut effects = vec![];
            self.enemies[index]
                .step(dt, &world, &mut draw, &mut effects)
                .map_err(|_| "unsupported enemy")?;
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
                                damage::bonk(&mut self.player, p, view.scale, 0.);
                                self.bonks += 1;
                                self.hit_stop = self.hit_stop.max(0.06);
                                self.retire(&mut draw);
                            } else {
                                damage::bump(&mut self.player, p, view.scale);
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
                        self.spawn(kind, extra, Some(at), &mut draw)
                    }
                    _ => return Err("effect outside Hollowbrook"),
                }
            }
            index += 1;
        }
        self.enemies.retain(|e| !e.target.dead);
        self.target_t
            .retain(|id, _| self.enemies.iter().any(|e| e.target.id == *id));
        let active = !self.mission.done && self.mission.fail_t == 0.;
        let damage_events = damage::update_hazards(
            &mut self.hazards,
            &mut self.strikes,
            &mut self.player,
            dt,
            view,
            0.,
            &mut self.used,
            |p| {
                if active && p.hearts == 0 {
                    p.out = true;
                    p.dizzy = 0.;
                }
            },
        );
        for event in damage_events {
            if matches!(event, damage::DamageEvent::Bonk { .. }) {
                self.bonks += 1;
                self.hit_stop = self.hit_stop.max(0.06);
            }
        }
        self.retire(&mut draw);
        let carry = &mut self.mission.carry;
        let earned = &mut self.earned;
        let xp = &mut self.xp;
        let failing = self.mission.fail_t != 0.;
        let sweeping = self.mission.done && !failing;
        gems::update_gems(
            &mut self.gems,
            dt,
            &self.player,
            view,
            sweeping,
            &[],
            |gem| {
                if failing {
                    *earned += gem.value;
                } else {
                    *carry += gem.value;
                }
                *xp = (*xp + gem.value).min(14440.);
            },
        );
        let mut m = std::mem::take(&mut self.mission);
        m.step(dt, self.definition, &mut Adapter(self), &mut draw);
        self.mission = m;
        let want =
            if self.mission.leg == Leg::Fly && !self.mission.done && self.mission.fail_t == 0. {
                view.scale * 4.2
            } else {
                0.
            };
        self.scroll_v += (want - self.scroll_v) * (dt * 1.5).min(1.);
        self.scroll += self.scroll_v * dt;
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
            sky_bottom: g.view.ground_y - g.view.scale * 0.5,
            scroll_v: g.scroll_v,
            ..WorldSnapshot::default()
        }
    }
    fn apply(&mut self, effect: Effect) {
        let g = &mut self.0;
        match effect {
            Effect::Bank(n) => g.earned += n,
            Effect::ClearObjective => {
                g.objective = None;
                g.objective_failed = false;
            }
            Effect::MakeObjective(stop) => {
                g.objective = objectives::specification(&g.definition.id, stop)
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
        let riders = if self.player.out {
            vec![]
        } else {
            vec![objectives::Rider {
                id: 0,
                x: self.player.x,
                y: self.player.y,
            }]
        };
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
        for event in &self.objective_events {
            if let objectives::Event::Failed(reason) = event {
                self.objective_failed = true;
                self.fail_reason = Some(reason.clone());
            }
        }
    }
}

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
