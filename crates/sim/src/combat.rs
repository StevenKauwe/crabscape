//! Fighting: health and poise, hits, knockback, hit-stop and the claw swings.
//!
//! The same rules hold for the crab and for every enemy: anything that fights
//! has [`Vitals`] (health, poise, stagger, hit-stop), and every blow arrives
//! through one door, a [`Hit`]. The crab's own state is a [`Fighter`] on its
//! [`SimCrab`](crate::SimCrab), so each crab in co-op fights for itself.
//!
//! Times are ticks (120 = 1 s). Numbers live in [`Tuning`].

use bevy_math::{Rect, Vec2};

use crate::physics::{Controls, Mover, StepEvents};
use crate::tuning::{ClawTuning, Tuning, VitalsTuning};
use crate::world::PlayerId;

/// Who dealt a hit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    /// A crab's claw.
    Crab(PlayerId),
    /// An enemy (its id).
    Enemy(u32),
    /// Anything else (tests, the level).
    #[default]
    World,
}

/// One blow, the single way damage reaches a fighter.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hit {
    pub damage: f32,
    pub poise: f32,
    /// Push, in world units per second (x already pointing away from the
    /// attacker). It goes through the normal physics, so walls still hold.
    pub knockback: Vec2,
    pub source: Source,
    /// Hit-stop: both fighters freeze this many ticks.
    pub hitstop: u32,
}

/// Health ("shell") and poise, for the crab and enemies alike.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
pub struct Vitals {
    pub health: f32,
    pub poise: f32,
    /// Ticks since the last hit (poise only refills after a while unhit).
    pub unhit: u32,
    /// Ticks of stagger left (poise broken: can't act).
    pub stagger: u32,
    /// Ticks of hit-stop left (frozen for a moment as a blow lands).
    pub hitstop: u32,
}

/// What a hit did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Took {
    /// Poise ran out: staggered.
    pub broke_poise: bool,
    /// Health ran out.
    pub died: bool,
}

impl Vitals {
    pub fn new(t: &VitalsTuning) -> Self {
        Self {
            health: t.health,
            poise: t.poise,
            unhit: u32::MAX / 2,
            stagger: 0,
            hitstop: 0,
        }
    }

    pub fn alive(&self) -> bool {
        self.health > 0.0
    }

    /// One tick passes (not called while frozen in hit-stop).
    pub fn tick(&mut self, t: &VitalsTuning) {
        self.unhit = self.unhit.saturating_add(1);
        if self.stagger > 0 {
            self.stagger -= 1;
            if self.stagger == 0 {
                self.poise = t.poise; // back on its feet, steady
            }
        } else if self.unhit > t.poise_delay_ticks {
            let per_tick = t.poise_refill / crate::physics::TICKS_PER_SECOND as f32;
            self.poise = (self.poise + per_tick).min(t.poise);
        }
    }

    /// Take `damage` and `poise` damage. A poise break staggers.
    pub fn take(&mut self, damage: f32, poise: f32, t: &VitalsTuning) -> Took {
        let was_alive = self.alive();
        self.health = (self.health - damage).max(0.0);
        self.unhit = 0;
        let mut broke_poise = false;
        if self.stagger == 0 && poise > 0.0 {
            self.poise = (self.poise - poise).max(0.0);
            if self.poise <= 0.0 {
                self.stagger = t.stagger_ticks;
                broke_poise = true;
            }
        }
        Took {
            broke_poise,
            died: was_alive && !self.alive(),
        }
    }
}

/// The crab's two claws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claw {
    /// The big claw (the crab's right, screen-left): the heavy smash.
    Crusher,
    /// The small claw: the quick jab.
    Cutter,
}

impl Claw {
    pub fn tuning(self, t: &Tuning) -> &ClawTuning {
        match self {
            Claw::Crusher => &t.crusher,
            Claw::Cutter => &t.cutter,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Claw::Crusher => "smash",
            Claw::Cutter => "jab",
        }
    }
}

/// Where a swing is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Windup,
    Active,
    Recovery,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Windup => "windup",
            Phase::Active => "active",
            Phase::Recovery => "recovery",
        }
    }
}

/// A claw strike under way.
#[derive(Clone, Debug, PartialEq)]
pub struct Swing {
    pub claw: Claw,
    /// Ticks into the swing: 1 on the tick it was pressed.
    pub tick: u32,
    /// Who it has already hit (each swing hits each target once).
    pub hits: Vec<u32>,
}

impl Swing {
    pub fn phase(&self, t: &Tuning) -> Phase {
        let c = self.claw.tuning(t);
        if self.tick <= c.windup_ticks {
            Phase::Windup
        } else if self.tick <= c.windup_ticks + c.active_ticks {
            Phase::Active
        } else {
            Phase::Recovery
        }
    }

    fn done(&self, t: &Tuning) -> bool {
        let c = self.claw.tuning(t);
        self.tick > c.windup_ticks + c.active_ticks + c.recovery_ticks
    }
}

/// A crab's fighting state.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
pub struct Fighter {
    pub vitals: Vitals,
    /// Which way the crab strikes: 1 = right, -1 = left (the last way it
    /// walked).
    pub facing: f32,
    pub swing: Option<Swing>,
    /// A claw pressed during a swing, and ticks left for it to come out.
    pub queued: Option<(Claw, u32)>,
    /// Ticks left with no walking control after a hit (the knockback).
    pub knocked: u32,
    /// Ticks since health ran out (None = alive).
    pub dead: Option<u32>,
}

impl Fighter {
    pub fn new(t: &Tuning) -> Self {
        Self {
            vitals: Vitals::new(&t.crab),
            facing: 1.0,
            swing: None,
            queued: None,
            knocked: 0,
            dead: None,
        }
    }

    /// One word (or two) for logs, dumps and tests.
    pub fn status(&self, t: &Tuning) -> String {
        if self.dead.is_some() {
            "dead".into()
        } else if self.vitals.stagger > 0 {
            "staggered".into()
        } else if let Some(s) = &self.swing {
            format!("{} {}", s.claw.name(), s.phase(t).name())
        } else if self.knocked > 0 {
            "knocked".into()
        } else {
            String::new()
        }
    }

    /// The swing's hitbox, while its blow is out: a box beside the body on
    /// the facing side.
    pub fn hitbox(&self, m: &Mover, t: &Tuning) -> Option<Rect> {
        let s = self
            .swing
            .as_ref()
            .filter(|s| s.phase(t) == Phase::Active)?;
        let c = s.claw.tuning(t);
        let near = m.pos.x + self.facing * t.body_half_width;
        let far = near + self.facing * c.reach;
        let mid = m.pos.y + t.body_height / 2.0;
        Some(Rect::new(
            near.min(far),
            mid - c.height / 2.0,
            near.max(far),
            mid + c.height / 2.0,
        ))
    }
}

/// The crab's body box (its hurtbox).
pub fn body(m: &Mover, t: &Tuning) -> Rect {
    Rect::new(
        m.pos.x - t.body_half_width,
        m.pos.y,
        m.pos.x + t.body_half_width,
        m.pos.y + t.body_height,
    )
}

/// The crab's side of a tick, before its physics step: timers, stagger,
/// death, starting and advancing swings. Returns the controls the physics
/// should use (a swing plants the feet, a stagger takes them away).
pub fn before_step(
    f: &mut Fighter,
    m: &mut Mover,
    c: &Controls,
    t: &Tuning,
    ev: &mut StepEvents,
) -> Controls {
    if let Some(dead) = &mut f.dead {
        *dead += 1;
        return Controls::default();
    }
    let staggered = f.vitals.stagger > 0;
    f.vitals.tick(&t.crab);
    f.knocked = f.knocked.saturating_sub(1);
    let mut out = *c;
    out.smash_pressed = false;
    out.jab_pressed = false;
    if staggered {
        f.swing = None;
        f.queued = None;
        return Controls::default();
    }

    let walking = c.walk.abs() > 0.1;
    if walking && f.swing.is_none() {
        f.facing = c.walk.signum();
    }

    // A claw press: now if free, or queued for a moment.
    let pressed = if c.smash_pressed {
        Some(Claw::Crusher)
    } else if c.jab_pressed {
        Some(Claw::Cutter)
    } else {
        None
    };
    if let Some(claw) = pressed {
        f.queued = Some((claw, t.swing_buffer_ticks));
    }

    // Advance the swing; dodge cancels its recovery.
    if let Some(s) = &mut f.swing {
        s.tick += 1;
        let recovering = s.phase(t) == Phase::Recovery;
        if s.done(t) || (recovering && c.dodge_pressed) {
            f.swing = None;
        }
    }
    let free = f.swing.is_none() && !m.climbing && m.dodge_left <= 0.0;
    if free && let Some((claw, _)) = f.queued.take() {
        if walking {
            f.facing = c.walk.signum();
        }
        f.swing = Some(Swing {
            claw,
            tick: 1,
            hits: Vec::new(),
        });
        match claw {
            Claw::Crusher => ev.smashed = true,
            Claw::Cutter => ev.jabbed = true,
        }
    }
    if let Some((_, left)) = &mut f.queued {
        *left = left.saturating_sub(1);
        if *left == 0 {
            f.queued = None;
        }
    }

    match f.swing.as_ref().map(|s| s.phase(t)) {
        // Wind-up and blow: feet planted, no jumping or dodging away.
        Some(Phase::Windup | Phase::Active) => {
            out.walk = 0.0;
            out.jump_pressed = false;
            out.dodge_pressed = false;
            out.dodge_held = false;
        }
        Some(Phase::Recovery) => {
            out.walk *= t.recovery_walk;
            out.jump_pressed = false;
        }
        None => {}
    }
    if f.knocked > 0 {
        out.walk = 0.0;
    }
    out
}

/// A hit reaches a crab. Lowers health and poise by exactly the hit's
/// numbers, pushes it through the normal physics and freezes it for the
/// hit-stop. Returns what it did (None: ignored, as when already dead).
pub fn crab_takes(
    f: &mut Fighter,
    m: &mut Mover,
    hit: &Hit,
    t: &Tuning,
    ev: &mut StepEvents,
) -> Option<Took> {
    if f.dead.is_some() {
        return None;
    }
    let took = f.vitals.take(hit.damage, hit.poise, &t.crab);
    ev.hurt = true;
    knock(m, hit.knockback);
    f.knocked = t.hit_stun_ticks;
    f.vitals.hitstop = f.vitals.hitstop.max(hit.hitstop);
    if took.broke_poise {
        f.swing = None;
        f.queued = None;
        ev.staggered = true;
    }
    if took.died {
        f.dead = Some(0);
        f.swing = None;
        f.queued = None;
        m.vel = Vec2::ZERO;
        ev.died = true;
    }
    Some(took)
}

/// Push a crab: the push replaces its sideways speed, and an upward push
/// lifts it off the floor. Anything it was doing with its legs stops.
fn knock(m: &mut Mover, push: Vec2) {
    if push == Vec2::ZERO {
        return;
    }
    m.vel.x = push.x;
    if push.y > 0.0 {
        m.vel.y = m.vel.y.max(push.y);
        m.grounded = false;
    }
    m.dodge_left = 0.0;
    m.sprinting = false;
    m.windup = 0.0;
    m.climbing = false;
}
