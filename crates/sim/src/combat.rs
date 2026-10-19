//! Fighting: health and poise, hits, knockback, hit-stop, the claw swings,
//! and the crab's defence: block, the timed parry and its counter, and the
//! dodge's moment of invulnerability.
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
    /// Can a timed block turn it aside? (A water shock can't be parried.)
    pub parryable: bool,
    /// Can raised claws stop it at all?
    pub blockable: bool,
    /// Share of the damage a block lets through, if not the usual
    /// [`Tuning::block_damage`].
    pub block_damage: Option<f32>,
    /// Blocking it cracks the guard for this many ticks (blocks then let
    /// [`Tuning::cracked_block_damage`] through).
    pub armour_break: u32,
    /// A bite or sting: venom if it isn't blocked (venom itself is #68).
    pub venom: bool,
}

impl Hit {
    /// A plain blow that can be blocked and parried.
    pub fn new(damage: f32, poise: f32, source: Source) -> Self {
        Self {
            damage,
            poise,
            source,
            parryable: true,
            blockable: true,
            ..Self::default()
        }
    }
}

/// What a hit did to a crab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing (dead already).
    Ignored,
    /// Dodged: the invulnerable moment of a dodge or flip.
    Evaded,
    /// Turned aside by a timed block: no damage; the attacker staggers.
    Parried,
    /// Stopped by raised claws: a little damage, full poise damage.
    Blocked(Took),
    /// Took it.
    Hit(Took),
}

impl Outcome {
    /// Did it get through to the body (not dodged, parried or blocked)?
    pub fn landed(self) -> bool {
        matches!(self, Outcome::Hit(_))
    }
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
    /// Started inside a parry's counter window: it hits harder.
    pub counter: bool,
    /// This swing's wind-up before any charging (a counter's is shorter).
    pub windup: u32,
    /// Extra wind-up ticks spent charging (holding smash).
    pub charged: u32,
    /// Still held, still charging.
    pub charging: bool,
    /// How hard it was started (momentum): 1 = plain.
    pub power: f32,
}

impl Swing {
    /// The whole wind-up, charging included.
    pub fn windup_len(&self) -> u32 {
        self.windup + self.charged
    }

    pub fn phase(&self, t: &Tuning) -> Phase {
        let c = self.claw.tuning(t);
        if self.tick <= self.windup_len() {
            Phase::Windup
        } else if self.tick <= self.windup_len() + c.active_ticks {
            Phase::Active
        } else {
            Phase::Recovery
        }
    }

    /// Ticks into the recovery (1 on its first), 0 before it.
    pub fn recovery_tick(&self, t: &Tuning) -> u32 {
        let c = self.claw.tuning(t);
        self.tick.saturating_sub(self.windup_len() + c.active_ticks)
    }

    /// How much harder than plain it hits: momentum, and a charge.
    pub fn strength(&self, t: &Tuning) -> f32 {
        let charge = if t.charge_ticks > 0 {
            t.charge_bonus * self.charged as f32 / t.charge_ticks as f32
        } else {
            0.0
        };
        self.power * (1.0 + charge)
    }

    fn done(&self, t: &Tuning) -> bool {
        let c = self.claw.tuning(t);
        self.tick > self.windup_len() + c.active_ticks + c.recovery_ticks
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
    /// Claws raised (block held, and free to block).
    pub blocking: bool,
    /// Ticks since block was last freshly pressed (a parry needs a fresh
    /// press, not a block held from far ahead).
    pub block_age: u32,
    /// Ticks before a press can open a new parry window (so mashing block
    /// doesn't parry everything).
    pub parry_lockout: u32,
    /// Ticks left in the counter window after a parry.
    pub counter: u32,
    /// Ticks left with the guard cracked (blocks let more through).
    pub armour_break: u32,
    /// Ticks into the current dodge or flip (1 on its first tick, 0 when
    /// not dodging).
    pub dodge_age: u32,
    /// A counter smash that seized its target: crushing it.
    pub crush: Option<Crush>,
    /// Showing its back to the camera (turned round to strike across; its
    /// claws never swap sides, so seen from behind the crusher is on the
    /// screen's right).
    pub back: bool,
    /// Ticks left in a turn (0 = not turning).
    pub turn: u32,
    /// The turn is into a blow: it steps that way as it pivots.
    pub turn_step: bool,
    /// Ticks out of a fight while showing its back (it turns to the front
    /// again after [`Tuning::turn_back_after`]).
    pub calm: u32,
}

impl Claw {
    /// Which side of the screen the claw is on (-1 left, 1 right): the
    /// crusher is the crab's right, screen-left from the front.
    pub fn screen_side(self, back: bool) -> f32 {
        let front = match self {
            Claw::Crusher => -1.0,
            Claw::Cutter => 1.0,
        };
        if back { -front } else { front }
    }
}

/// The crusher clamped on an enemy (see [`Tuning::crush`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crush {
    /// The enemy's id.
    pub target: u32,
    /// Ticks held so far.
    pub tick: u32,
    /// Cracks squeezed in so far.
    pub cracks: u32,
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
            blocking: false,
            block_age: u32::MAX / 2,
            parry_lockout: 0,
            counter: 0,
            armour_break: 0,
            dodge_age: 0,
            crush: None,
            back: false,
            turn: 0,
            turn_step: false,
            calm: 0,
        }
    }

    /// One word (or two) for logs, dumps and tests.
    pub fn status(&self, t: &Tuning) -> String {
        if self.dead.is_some() {
            "dead".into()
        } else if self.crush.is_some() {
            "crushing".into()
        } else if self.vitals.stagger > 0 {
            "staggered".into()
        } else if let Some(s) = &self.swing {
            format!("{} {}", s.claw.name(), s.phase(t).name())
        } else if self.knocked > 0 {
            "knocked".into()
        } else if self.blocking {
            "blocking".into()
        } else {
            String::new()
        }
    }

    /// The swing's hitbox, while its blow is out: a box beside the body on
    /// the facing side.
    /// Inside the dodge's invulnerable ticks?
    pub fn evading(&self, t: &Tuning) -> bool {
        let (from, to) = t.dodge_invulnerable;
        (from..=to).contains(&self.dodge_age)
    }

    /// Would a parryable hit landing now be parried?
    pub fn parrying(&self, t: &Tuning) -> bool {
        self.block_age <= t.parry_window_ticks
            && self.vitals.stagger == 0
            && self
                .swing
                .as_ref()
                .is_none_or(|s| s.phase(t) == Phase::Recovery)
    }

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
    f.counter = f.counter.saturating_sub(1);
    f.armour_break = f.armour_break.saturating_sub(1);
    f.parry_lockout = f.parry_lockout.saturating_sub(1);
    f.block_age = f.block_age.saturating_add(1);
    f.turn = f.turn.saturating_sub(1);
    // Out of a fight for a while with its back shown: it turns to face the
    // camera again.
    let fighting = f.swing.is_some() || f.crush.is_some() || c.block_held;
    f.calm = if f.back && !fighting && f.turn == 0 {
        f.calm + 1
    } else {
        0
    };
    if t.turn_ticks > 0 && f.calm >= t.turn_back_after {
        f.back = false;
        f.turn = t.turn_ticks;
        f.turn_step = false;
        f.calm = 0;
        ev.turned = true;
    }
    let mut out = *c;
    out.smash_pressed = false;
    out.jab_pressed = false;
    out.block_pressed = false;
    if staggered {
        f.swing = None;
        f.queued = None;
        f.blocking = false;
        return Controls::default();
    }
    // Crushing: the crusher is clamped on; the crab stands and squeezes
    // (the sim's world step counts the crush and lets go).
    if f.crush.is_some() {
        f.blocking = false;
        return Controls::default();
    }
    // A fresh block press opens the parry window (unless mashing).
    if c.block_pressed {
        if f.parry_lockout == 0 {
            f.block_age = 0;
        }
        f.parry_lockout = t.parry_lockout_ticks;
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

    // Advance the swing; dodge or block cut its recovery short, and so does
    // a claw press late enough in it (when the claw chains).
    if let Some(s) = &mut f.swing {
        s.tick += 1;
        // Charging: held at the top of the wind-up until let go (or full).
        if s.charging {
            if !c.smash_held || s.charged >= t.charge_ticks {
                s.charging = false;
            } else if s.tick > s.windup_len() {
                s.charged += 1;
            }
        }
        let recovering = s.phase(t) == Phase::Recovery;
        let chains = s
            .claw
            .tuning(t)
            .chain_after
            .is_some_and(|n| s.recovery_tick(t) >= n);
        let cut = c.dodge_pressed || c.block_pressed || (chains && f.queued.is_some());
        if s.done(t) || (recovering && cut) {
            f.swing = None;
        }
    }
    let free = f.swing.is_none() && !m.climbing && m.dodge_left <= 0.0;
    if free && let Some((claw, _)) = f.queued.take() {
        if walking {
            f.facing = c.walk.signum();
        }
        let counter = claw == Claw::Crusher && f.counter > 0;
        if counter {
            f.counter = 0;
            ev.countered = true;
        }
        let ct = claw.tuning(t);
        let haste = if counter { t.counter_haste } else { 0 };
        // Striking across: the body turns round first, so the claw is on the
        // side the blow goes (claws never swap sides). It adds to the wind-up.
        let turn = if t.turn_ticks > 0 && claw.screen_side(f.back) != f.facing {
            f.back = !f.back;
            f.turn = t.turn_ticks;
            f.turn_step = true;
            ev.turned = true;
            t.turn_ticks
        } else {
            f.turn_step = false;
            0
        };
        // The wind-up starts in the last part of the turn (it overlaps up to
        // half of each), so turning never adds its whole length.
        let base = ct.windup_ticks.saturating_sub(haste).max(1);
        let overlap = (base / 2).min(turn / 2);
        let speed = (m.vel.x.abs() / t.sprint_speed).min(1.0);
        f.swing = Some(Swing {
            claw,
            tick: 1,
            hits: Vec::new(),
            counter,
            windup: base + turn - overlap,
            charged: 0,
            charging: claw == Claw::Crusher && t.charge_ticks > 0 && c.smash_held,
            power: 1.0 + t.momentum * speed,
        });
        match claw {
            Claw::Crusher => ev.smashed = true,
            Claw::Cutter => ev.jabbed = true,
        }
    }
    // A press is kept, not counted down, through a turn and the swing it
    // starts (the turn is long; nothing pressed in it is lost): it comes
    // out when the swing can chain or ends.
    let turning = f.turn > 0 || (f.turn_step && f.swing.is_some());
    if let Some((_, left)) = &mut f.queued
        && !turning
    {
        *left = left.saturating_sub(1);
        if *left == 0 {
            f.queued = None;
        }
    }
    // Turning into a blow: a pivoting sidestep that way (the wind-up plants
    // the feet otherwise; the momentum carries on into the blow).
    if f.turn > 0 && f.turn_step && t.turn_ticks > 0 {
        let speed = t.turn_travel / (t.turn_ticks as f32 * crate::physics::STEP);
        // Plus what the planted feet brake off in one step.
        m.vel.x = f.facing * (speed + t.ground_decel * crate::physics::STEP);
    }
    // The blow comes out: the body goes with it.
    if let Some(s) = &f.swing {
        let ct = s.claw.tuning(t);
        if ct.lunge > 0.0 && s.tick == s.windup_len() + 1 {
            let lunge = ct.lunge * s.strength(t);
            if m.vel.x * f.facing < lunge {
                m.vel.x = f.facing * lunge;
            }
            ev.lunged = true;
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
    f.blocking = c.block_held && f.swing.is_none() && !m.climbing && m.dodge_left <= 0.0;
    if f.blocking {
        out.walk *= t.block_walk;
        out.dodge_held = false; // no sprinting with the claws up
    }
    if f.knocked > 0 {
        out.walk = 0.0;
    }
    out
}

/// The crab's side of a tick, after its physics step: counts the dodge.
pub fn after_step(f: &mut Fighter, m: &Mover) {
    f.dodge_age = if m.dodge_left > 0.0 || m.flip_left > 0.0 {
        f.dodge_age + 1
    } else {
        0
    };
}

/// A hit reaches a crab, with all its defences: the dodge's invulnerable
/// moment, a timed parry, raised claws. Otherwise as [`crab_takes`].
pub fn crab_defends(
    f: &mut Fighter,
    m: &mut Mover,
    hit: &Hit,
    t: &Tuning,
    ev: &mut StepEvents,
) -> Outcome {
    if f.dead.is_some() {
        return Outcome::Ignored;
    }
    if f.evading(t) {
        ev.evaded = true;
        return Outcome::Evaded;
    }
    if hit.parryable && f.parrying(t) {
        f.block_age = u32::MAX / 2; // one press parries one blow
        f.parry_lockout = 0;
        f.counter = t.counter_ticks;
        f.swing = None;
        f.vitals.hitstop = f.vitals.hitstop.max(t.parry_hitstop_ticks);
        ev.parried = true;
        return Outcome::Parried;
    }
    if hit.blockable && f.blocking {
        let share = if f.armour_break > 0 {
            t.cracked_block_damage
        } else {
            hit.block_damage.unwrap_or(t.block_damage)
        };
        let took = f.vitals.take(hit.damage * share, hit.poise, &t.crab);
        f.armour_break = f.armour_break.max(hit.armour_break);
        knock(m, hit.knockback * t.block_knockback);
        f.vitals.hitstop = f.vitals.hitstop.max(t.block_hitstop_ticks);
        ev.blocked = true;
        if took.broke_poise {
            f.blocking = false;
            ev.staggered = true;
        }
        if took.died {
            f.dead = Some(0);
            m.vel = Vec2::ZERO;
            ev.died = true;
        }
        return Outcome::Blocked(took);
    }
    match crab_takes(f, m, hit, t, ev) {
        Some(took) => Outcome::Hit(took),
        None => Outcome::Ignored,
    }
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
    f.blocking = false;
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
