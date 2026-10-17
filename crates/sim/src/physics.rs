//! How a crab moves: walking, sinking, jumping, paddling, dodging and
//! bumping into the level.
//!
//! This is plain Rust with no Bevy systems in it, so it can be unit tested
//! (see the tests at the bottom). The [`Sim`](crate::Sim) calls [`step`] 120
//! times a simulated second for every crab, with that crab's [`Controls`].
//!
//! The crab's position is the point between its feet, at the bottom-middle
//! of its collision box.

use bevy_math::{Rect, Vec2};

use crate::level::Level;
use crate::tuning::Tuning;

/// The physics runs in fixed steps of this many seconds. A fixed step makes
/// the motion the same on a 60 Hz phone and a 144 Hz monitor (and, later,
/// on every player's computer).
pub const STEP: f32 = 1.0 / TICKS_PER_SECOND as f32;

/// How many physics steps ("ticks") make one second of game time.
pub const TICKS_PER_SECOND: u32 = 120;

/// What a player is asking their crab to do. This is a *command*, not a
/// keyboard or touch-screen reading, so it can come from anywhere: the local
/// player's keys today, a network message from another player later.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    /// Walking direction and strength: -1 = full left, 1 = full right.
    pub walk: f32,
    /// Is the jump button held down? (Let go early for a small hop.)
    pub jump_held: bool,
    /// Was jump pressed since the last step?
    pub jump_pressed: bool,
    /// Was dodge pressed since the last step?
    pub dodge_pressed: bool,
    /// Is the dodge button held down? (Keep holding after a dodge to sprint.)
    pub dodge_held: bool,
    /// Is down held? (Climbs down kelp or coral; so does holding dodge.)
    pub down: bool,
}

/// Something a crab carries home on its back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carry {
    /// A lump of wild glowing bacteria: cultured at home into the lantern.
    Culture,
    /// A shed tooth: fitted to the crusher at home, making scissor claws.
    Tooth,
}

/// A crab's movement state. (In the game, each crab entity also carries a
/// copy as a Bevy component, for the animation and camera to read.)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
pub struct Mover {
    /// Feet position, in world units.
    pub pos: Vec2,
    /// Velocity, in world units per second.
    pub vel: Vec2,
    /// Standing on something?
    pub grounded: bool,
    /// Time left in which a jump still works after leaving the ground.
    pub coyote: f32,
    /// Time left in which an early jump press is remembered.
    pub jump_buffer: f32,
    /// Time left in the crouch before take-off (0 = not crouching).
    pub windup: f32,
    /// Rising because of a jump or paddle (so letting go can cut it short).
    pub rising: bool,
    /// Paddle strokes the crab has grown (one per egg eaten), on top of
    /// [`Tuning::swim_strokes`]. The first one is the double jump.
    pub upgrades: u32,
    /// The paper-shell flag (set by the world while the molt is in its
    /// soft window): paddle legs are soft, so no strokes or jumps, and
    /// climbing slows to a fraction.
    pub soft: bool,
    /// What the crab is carrying home on its back, if anything (one thing
    /// at a time).
    pub carrying: Option<Carry>,
    /// Scissor claws: a shed tooth's edges fitted along both fingers of its
    /// crusher claw (at the workbench at home). They cut through ghost nets.
    pub scissor_claw: bool,
    /// How many cultures have been brought home: each makes the lantern
    /// brighter.
    pub lantern: u32,
    /// Climbing experience: it grows with every bit climbed, and each level
    /// climbs faster (see [`Mover::climb_level`]).
    pub climb_xp: f32,
    /// Paddle strokes left (fractions are strokes still resting). Only
    /// standing on something solid gives them back, and only from the
    /// [`reserve`](Self::reserve).
    pub stamina: f32,
    /// The body's deeper reserve: 1 = full, 0 = empty. Resting the paddle
    /// legs draws on it; it comes back by itself, slowly.
    pub reserve: f32,
    /// Time before the paddle legs can make their next stroke (the rhythm).
    pub stroke_rest: f32,
    /// Time left in the current dodge (0 = not dodging).
    pub dodge_left: f32,
    /// Time before the next dodge is allowed.
    pub dodge_cooldown: f32,
    /// Direction of the current dodge: -1 or 1.
    pub dodge_dir: f32,
    /// Time left in which a dodge press still waits for a direction (or for
    /// the floor, or the cooldown).
    pub dodge_buffer: f32,
    /// Sprinting: holding dodge on the floor, paid from the reserve.
    pub sprinting: bool,
    /// The reserve ran dry mid-sprint: no sprint until dodge is let go.
    pub winded: bool,
    /// Gripping kelp or coral (see [`Level::climbs`](crate::Level::climbs)).
    pub climbing: bool,
    /// Diving: pushing a way while sinking, tilted and streamlined, so a
    /// little faster down and sideways.
    pub diving: bool,
    /// Time left in a flip (dodge in the water), and which way it turns
    /// (1 = clockwise, towards the right).
    pub flip_left: f32,
    pub flip_dir: f32,
    /// The way the crab was already walking when it gripped (-1, 0 or 1):
    /// only a fresh press sideways steps it off.
    pub climb_dir: f32,
    /// Which side of the stalk the crab clings to while climbing: 1 = to the
    /// right of it, -1 = to the left. It's the side the crab came from; it
    /// swings round to the other side when something blocks the way.
    pub climb_side: f32,
}

impl Mover {
    /// A crab standing still at `pos`.
    pub fn new(pos: Vec2) -> Self {
        Self {
            pos,
            vel: Vec2::ZERO,
            grounded: true,
            coyote: 0.0,
            jump_buffer: 0.0,
            windup: 0.0,
            rising: false,
            upgrades: 0,
            soft: false,
            carrying: None,
            scissor_claw: false,
            lantern: 0,
            climb_xp: 0.0,
            stamina: 0.0,
            reserve: 1.0,
            stroke_rest: 0.0,
            dodge_left: 0.0,
            dodge_cooldown: 0.0,
            dodge_dir: 1.0,
            dodge_buffer: 0.0,
            sprinting: false,
            winded: false,
            climbing: false,
            diving: false,
            flip_left: 0.0,
            flip_dir: 1.0,
            climb_dir: 0.0,
            climb_side: 1.0,
        }
    }

    /// How many paddle strokes the legs hold when rested (the double jump,
    /// then swim strokes).
    pub fn strokes(&self, t: &Tuning) -> u32 {
        // The paper shell: paddle legs are soft, so no strokes at all.
        if self.soft {
            return 0;
        }
        t.swim_strokes + self.upgrades
    }

    /// The Climbing skill's level, from its experience (see
    /// [`Tuning::climb_level_xp`]: each level needs a little more than the
    /// last).
    pub fn climb_level(&self, t: &Tuning) -> u32 {
        let mut level = 1;
        while level < 99 && self.climb_xp >= t.climb_level_xp * (level * (level + 1)) as f32 {
            level += 1;
        }
        level
    }

    /// How far through its current Climbing level the crab is, 0 to 1.
    pub fn climb_progress(&self, t: &Tuning) -> f32 {
        let level = self.climb_level(t) as f32;
        let from = t.climb_level_xp * (level - 1.0) * level;
        let to = t.climb_level_xp * level * (level + 1.0);
        ((self.climb_xp - from) / (to - from)).clamp(0.0, 1.0)
    }

    /// Climbing speed at the crab's Climbing level.
    pub fn climb_speed(&self, t: &Tuning) -> f32 {
        // Climbing is walking legs: it survives the soft shell, slowed.
        let soft = if self.soft { t.soft_climb_factor } else { 1.0 };
        t.climb_speed * soft * (1.0 + t.climb_speed_per_level * (self.climb_level(t) - 1) as f32)
    }

    /// A one-word description of what the crab is doing, for logs and tests.
    pub fn activity(&self) -> &'static str {
        if self.climbing {
            "climbing"
        } else if self.dodge_left > 0.0 {
            "dodging"
        } else if self.sprinting {
            "sprinting"
        } else if self.windup > 0.0 {
            "crouching"
        } else if self.grounded {
            if self.vel.x.abs() > 1.0 {
                "walking"
            } else {
                "idle"
            }
        } else if self.vel.y > 0.0 {
            "rising"
        } else {
            "falling"
        }
    }
}

/// Things that happened during one step, for the animation and effects.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepEvents {
    /// Started crouching for a jump.
    pub crouched: bool,
    /// Pushed off the floor.
    pub jumped: bool,
    /// Made a paddle stroke (the double jump, or a swim stroke after it).
    pub paddled: bool,
    /// Ate an egg: one more swim stroke from now on.
    pub ate: bool,
    /// Gripped kelp or coral.
    pub gripped: bool,
    /// Picked up a lump of glowing bacteria to carry home.
    pub picked_up: bool,
    /// Rested at home (the stamina bar and rings refilled).
    pub rested: bool,
    /// Brought a culture home: the lantern grew brighter.
    pub delivered: bool,
    /// The Climbing skill went up a level.
    pub levelled: bool,
    /// Started a flip (dodge in the water).
    pub flipped: bool,
    /// Found a clue (which one, in [`Level::clues`](crate::Level::clues)).
    pub found_clue: Option<usize>,
    /// Brought a tooth home: the workbench fitted it, scissor claws.
    pub fitted_scissors: bool,
    /// Cut through a ghost net with scissor claws.
    pub cut_net: bool,
    /// Pressed against a ghost net with nothing to cut it.
    pub tangled: bool,
    /// Landed, at this downward speed.
    pub landed: Option<f32>,
    /// Started a dodge.
    pub dodged: bool,
}

impl StepEvents {
    /// Combine the events of several steps.
    pub fn merge(&mut self, other: StepEvents) {
        self.crouched |= other.crouched;
        self.jumped |= other.jumped;
        self.paddled |= other.paddled;
        self.ate |= other.ate;
        self.gripped |= other.gripped;
        self.picked_up |= other.picked_up;
        self.rested |= other.rested;
        self.delivered |= other.delivered;
        self.levelled |= other.levelled;
        self.flipped |= other.flipped;
        self.found_clue = self.found_clue.or(other.found_clue);
        self.fitted_scissors |= other.fitted_scissors;
        self.cut_net |= other.cut_net;
        self.tangled |= other.tangled;
        self.dodged |= other.dodged;
        if other.landed.is_some() {
            self.landed = other.landed;
        }
    }

    /// The names of the events that happened, for logs and tests.
    pub fn names(&self) -> Vec<&'static str> {
        [
            (self.crouched, "crouched"),
            (self.jumped, "jumped"),
            (self.paddled, "paddled"),
            (self.ate, "ate"),
            (self.gripped, "gripped"),
            (self.picked_up, "picked up"),
            (self.rested, "rested"),
            (self.delivered, "delivered"),
            (self.levelled, "levelled"),
            (self.flipped, "flipped"),
            (self.found_clue.is_some(), "found a clue"),
            (self.fitted_scissors, "fitted scissor claws"),
            (self.cut_net, "cut a net"),
            (self.tangled, "tangled"),
            (self.landed.is_some(), "landed"),
            (self.dodged, "dodged"),
        ]
        .into_iter()
        .filter_map(|(happened, name)| happened.then_some(name))
        .collect()
    }
}

/// Small number for "touching but not overlapping" comparisons.
const EPS: f32 = 0.01;

/// Move `value` towards `target` by at most `amount`.
fn approach(value: f32, target: f32, amount: f32) -> f32 {
    if value < target {
        (value + amount).min(target)
    } else {
        (value - amount).max(target)
    }
}

/// Advance one crab by `dt` seconds.
pub fn step(m: &mut Mover, c: &Controls, t: &Tuning, level: &Level, dt: f32) -> StepEvents {
    let mut ev = StepEvents::default();
    let walk = c.walk.clamp(-1.0, 1.0);
    let walking = walk.abs() > 0.1;

    // --- Timers ---------------------------------------------------------
    if c.jump_pressed {
        m.jump_buffer = t.jump_buffer;
    } else {
        m.jump_buffer = (m.jump_buffer - dt).max(0.0);
    }
    // The reserve comes back by itself, slowly.
    m.reserve = (m.reserve + dt / t.reserve_refill_time).min(1.0);
    if m.grounded {
        m.coyote = t.coyote_time;
        // Standing on something solid rests the paddle legs, drawing on the
        // reserve (both counted in strokes). An empty reserve leaves them
        // tired.
        let full = m.strokes(t) as f32;
        let want = (full * dt / t.stamina_refill_time).min(full - m.stamina);
        let strokes = want.min(m.reserve * t.stamina_reserve as f32);
        if strokes > 0.0 {
            m.stamina = (m.stamina + strokes).min(full);
            m.reserve = (m.reserve - strokes / t.stamina_reserve as f32).max(0.0);
        }
    } else {
        m.coyote = (m.coyote - dt).max(0.0);
    }
    m.stroke_rest = (m.stroke_rest - dt).max(0.0);
    m.dodge_cooldown = (m.dodge_cooldown - dt).max(0.0);
    if c.dodge_pressed {
        m.dodge_buffer = t.dodge_buffer;
    } else {
        m.dodge_buffer = (m.dodge_buffer - dt).max(0.0);
    }

    // --- Dodge: a short, fast sideways scuttle ----------------------------
    // It goes the way you hold, so it needs a direction: dodge on its own
    // does nothing (it never guesses a way you didn't mean).
    // Scuttling needs legs on the floor, so there is no dodge in the water.
    // (Dodge, then jump, and the extra speed carries into the jump.)
    if m.dodge_buffer > 0.0 && walking && m.dodge_cooldown <= 0.0 && m.grounded {
        m.dodge_buffer = 0.0;
        m.dodge_dir = walk.signum();
        m.dodge_left = t.dodge_time;
        m.dodge_cooldown = t.dodge_cooldown;
        ev.dodged = true;
    }
    // In the water (not climbing) dodge is a flip instead: a quick
    // somersault, towards the way the crab is pushing. Only a flourish for
    // now: it doesn't move the crab any further.
    m.flip_left = (m.flip_left - dt).max(0.0);
    if m.dodge_buffer > 0.0
        && !m.grounded
        && !m.climbing
        && m.dodge_cooldown <= 0.0
        && m.flip_left <= 0.0
    {
        m.dodge_buffer = 0.0;
        m.flip_left = t.flip_time;
        m.flip_dir = if walking { walk.signum() } else { 1.0 };
        m.dodge_cooldown = t.dodge_cooldown;
        ev.flipped = true;
    }

    // --- Sprint: keep holding dodge to keep scuttling fast ----------------
    // Paid from the reserve, legs on the floor only. If the reserve runs dry
    // the crab is winded: back to walking until dodge is let go (so it never
    // flickers between the two as the reserve trickles back).
    if !c.dodge_held {
        m.winded = false;
    }
    m.sprinting = c.dodge_held && walking && m.grounded && !m.winded && m.dodge_left <= 0.0;
    if m.sprinting {
        let cost = t.sprint_cost * dt / t.stamina_reserve as f32;
        if m.reserve >= cost {
            m.reserve -= cost;
        } else {
            m.reserve = 0.0;
            m.sprinting = false;
            m.winded = true;
        }
    }

    // --- Climbing kelp or coral ------------------------------------------
    // In the water, press jump while touching one to grip it (instead of a
    // paddle stroke). With the walking legs (no stamina needed): hold jump
    // to climb up, hold down (or dodge) to climb down, nothing to hang on.
    // A fresh press sideways shuffles along the stalk (fidgeting is safe);
    // pushing on past the edge of its reach steps off. Jump with a direction
    // kicks off the stalk that way (free, like a floor jump). Climbing past
    // the top pulls the crab up over it.
    let grip = level.climb_at(m.pos, t.climb_reach);
    let mut shuffle = 0.0;
    if m.climbing {
        if !walking {
            m.climb_dir = 0.0;
        }
        let fresh = walking && walk.signum() != m.climb_dir;
        let off_centre = grip.map_or(0.0, |g| m.pos.x - g.x);
        let at_edge = off_centre * walk.signum() >= t.climb_reach - 1.0;
        let step_off = fresh && at_edge;
        if fresh && !at_edge {
            shuffle = walk * t.climb_shuffle;
        }
        if m.grounded {
            m.climbing = false;
        } else if c.jump_pressed && walking {
            // Kick off the stalk: up and away, the way the crab is pressing.
            m.climbing = false;
            m.jump_buffer = 0.0;
            m.vel = Vec2::new(walk.signum() * t.walk_speed, t.climb_leap);
            m.rising = true;
            ev.jumped = true;
        } else if step_off {
            m.climbing = false;
            m.vel.x = walk.signum() * t.walk_speed;
        } else if grip.is_none() {
            m.climbing = false;
            if c.jump_held {
                m.vel.y = t.climb_pull_up;
                m.rising = true;
            }
        }
    } else if let Some(g) = grip.filter(|_| !m.grounded && m.jump_buffer > 0.0) {
        m.climbing = true;
        m.climb_dir = if walking { walk.signum() } else { 0.0 };
        // Cling on the side it came from (or was heading in from).
        let dx = m.pos.x - g.x;
        m.climb_side = if dx.abs() > 1.0 {
            dx.signum()
        } else if m.vel.x.abs() > 1.0 {
            -m.vel.x.signum()
        } else {
            1.0
        };
        if climb_blocked(m, t, level) {
            m.climb_side = -m.climb_side;
        }
        m.jump_buffer = 0.0;
        m.dodge_left = 0.0;
        ev.gripped = true;
    }
    if m.climbing {
        let down = c.down || c.dodge_held;
        let speed = m.climb_speed(t);
        let climb = match (c.jump_held, down) {
            (true, false) => speed,
            (false, true) => -speed,
            _ => 0.0,
        };
        m.vel = Vec2::new(shuffle, climb);
        m.rising = false;
        let (from, was) = (m.pos.y, m.climb_level(t));
        climb_and_collide(m, t, level, dt);
        // Climbing is a skill: it grows with every bit climbed.
        m.climb_xp += (m.pos.y - from).abs() * t.climb_xp_per_unit;
        ev.levelled = m.climb_level(t) > was;
        // Shuffling stops at the edge of the reach; pushing on steps off.
        if let Some(g) = level.climb_at(m.pos, t.climb_reach + 5.0) {
            let edge = t.climb_reach - 0.5;
            m.pos.x = m.pos.x.clamp(g.x - edge, g.x + edge);
        }
        return ev;
    }

    // --- Sideways speed -------------------------------------------------
    if m.dodge_left > 0.0 {
        m.dodge_left -= dt;
        m.vel.x = m.dodge_dir * t.dodge_speed;
    } else {
        // Pushing a way while sinking: a dive, a little quicker sideways.
        m.diving = !m.grounded && walking && m.vel.y < 0.0;
        let top = if m.sprinting {
            t.sprint_speed
        } else if m.diving {
            t.walk_speed * t.dive_speed
        } else {
            t.walk_speed
        };
        let target = walk * top;
        let reversing = walking && m.vel.x.abs() > 1.0 && target.signum() != m.vel.x.signum();
        let too_fast = m.vel.x.abs() > top && !reversing;
        let rate = match (m.grounded, walking) {
            (true, false) => t.ground_decel,
            (true, true) if reversing => t.ground_turn,
            (true, true) if too_fast => t.overspeed_decel,
            (true, true) => t.ground_accel,
            (false, false) => t.air_decel,
            (false, true) if too_fast => t.overspeed_decel,
            (false, true) => t.air_accel,
        };
        m.vel.x = approach(m.vel.x, target, rate * dt);
    }

    // --- Jumping --------------------------------------------------------
    let launch = |m: &mut Mover, ev: &mut StepEvents| {
        // Jumping ends a dodge, and only some of its speed carries over.
        let carry = t.walk_speed * t.jump_carry;
        m.vel.x = m.vel.x.clamp(-carry, carry);
        m.dodge_left = 0.0;
        m.vel.y = t.jump_speed;
        m.grounded = false;
        m.rising = true;
        m.coyote = 0.0;
        ev.jumped = true;
    };
    if m.windup > 0.0 {
        // Crouching: push off when the crouch is over.
        m.windup -= dt;
        if m.windup <= 0.0 {
            m.windup = 0.0;
            launch(m, &mut ev);
        }
    } else if m.jump_buffer > 0.0 {
        if m.grounded || m.coyote > 0.0 {
            m.jump_buffer = 0.0;
            if m.grounded && t.jump_windup > 0.0 {
                m.windup = t.jump_windup;
                m.coyote = 0.0;
                ev.crouched = true;
            } else {
                launch(m, &mut ev); // a coyote jump: no floor to crouch on
            }
        } else if m.stroke_rest <= 0.0 && m.stamina + 1e-4 >= 1.0 {
            // In the water with no floor: a paddle stroke instead (the
            // double jump, then swim strokes), if the legs have one left.
            // The rhythm spaces them out. (A press during the rhythm stays in
            // the jump buffer, so a little early counts.)
            m.jump_buffer = 0.0;
            m.stamina = (m.stamina - 1.0).max(0.0);
            m.stroke_rest = t.stroke_interval;
            m.vel.y = m.vel.y.max(t.paddle_speed);
            m.rising = true;
            ev.paddled = true;
        }
    }
    // Let go of jump while rising: cut the jump short.
    if m.rising && !c.jump_held && m.vel.y > 0.0 {
        m.vel.y *= t.jump_cut;
        m.rising = false;
    }
    if m.vel.y <= 0.0 {
        m.rising = false;
    }

    // --- Sinking: gravity minus buoyancy, slowed by water drag -----------
    if !m.grounded {
        let mut g = t.net_gravity();
        if m.vel.y < 0.0 {
            g *= t.fall_gravity_scale;
        }
        m.vel.y -= g * dt;
        // Diving, the crab is streamlined: the water holds it back less.
        let drag = if m.diving {
            t.water_drag * t.dive_drag
        } else {
            t.water_drag
        };
        m.vel.y -= drag * m.vel.y * m.vel.y.abs() * dt;
    }

    move_and_collide(m, t, level, dt, &mut ev);
    ev
}

/// The crab's body while climbing: turned sideways onto the stalk, so as
/// tall as its shell is wide and sticking out as far as its shell is high,
/// on the side it clings to (its feet are at the stalk).
pub fn climb_box(m: &Mover, t: &Tuning) -> Rect {
    let (out, half_tall) = (t.body_height, t.body_half_width);
    let (left, right) = if m.climb_side > 0.0 {
        (m.pos.x, m.pos.x + out)
    } else {
        (m.pos.x - out, m.pos.x)
    };
    Rect::new(left, m.pos.y - half_tall, right, m.pos.y + half_tall)
}

/// Does the climbing body overlap anything solid?
fn climb_blocked(m: &Mover, t: &Tuning, level: &Level) -> bool {
    let b = climb_box(m, t);
    level.solids().any(|r| {
        b.min.x < r.max.x - EPS
            && b.max.x > r.min.x + EPS
            && b.min.y < r.max.y - EPS
            && b.max.y > r.min.y + EPS
    })
}

/// Move a climbing crab: sideways (shuffling), then up or down, each only
/// if the turned body stays clear of anything solid. Blocked going up or
/// down, it swings round to the other side of the stalk if that's clear.
/// Down at the floor, it lets go and stands.
fn climb_and_collide(m: &mut Mover, t: &Tuning, level: &Level, dt: f32) {
    let start = m.pos;
    m.pos.x += m.vel.x * dt;
    if climb_blocked(m, t, level) {
        m.pos.x = start.x;
        m.vel.x = 0.0;
    }
    let before = m.pos.y;
    m.pos.y += m.vel.y * dt;
    if climb_blocked(m, t, level) {
        m.climb_side = -m.climb_side;
        if climb_blocked(m, t, level) {
            m.climb_side = -m.climb_side;
            m.pos.y = before;
            m.vel.y = 0.0;
        }
    }
    let floor = level.floor_height(m.pos.x);
    if m.pos.y - t.body_half_width <= floor {
        m.pos.y = floor;
        m.vel = Vec2::ZERO;
        m.climbing = false;
        m.grounded = true;
    }
}

/// Move by the velocity, then sort out collisions: sideways first, then up
/// and down.
fn move_and_collide(m: &mut Mover, t: &Tuning, level: &Level, dt: f32, ev: &mut StepEvents) {
    let hw = t.body_half_width;
    let h = t.body_height;
    let was_grounded = m.grounded;

    // Sideways.
    m.pos.x += m.vel.x * dt;
    let (lo, hi) = (level.min_x + hw, level.max_x - hw);
    if m.pos.x < lo || m.pos.x > hi {
        m.pos.x = m.pos.x.clamp(lo, hi);
        m.vel.x = 0.0;
    }
    for r in level.solids() {
        // A rock whose top is within a step of our feet doesn't block: we
        // step up onto it below.
        if r.max.y <= m.pos.y + t.step_up {
            continue;
        }
        let overlaps = m.pos.x + hw > r.min.x + EPS
            && m.pos.x - hw < r.max.x - EPS
            && m.pos.y + h > r.min.y + EPS
            && m.pos.y < r.max.y - EPS;
        if overlaps {
            // Push back out of the side we came from.
            m.pos.x = if m.pos.x < r.center().x {
                r.min.x - hw
            } else {
                r.max.x + hw
            };
            m.vel.x = 0.0;
        }
    }

    // Up and down.
    let old_y = m.pos.y;
    m.pos.y += m.vel.y * dt;
    let over = |r: &Rect, x: f32| x + hw > r.min.x + EPS && x - hw < r.max.x - EPS;
    if m.vel.y > 0.0 {
        // Bump our shell on the underside of a rock.
        for r in level.solids() {
            if over(&r, m.pos.x) && old_y + h <= r.min.y + EPS && m.pos.y + h > r.min.y {
                m.pos.y = r.min.y - h;
                m.vel.y = 0.0;
                m.rising = false;
            }
        }
        m.grounded = false;
        return;
    }

    // Moving down or standing: find the highest surface under us, from the
    // floor line (at our middle) or a rock top (anywhere under our shell).
    // While walking (not jumping), we also stick to a floor a little below.
    let reach_down = if was_grounded { t.ground_snap } else { 0.0 };
    let low = m.pos.y - reach_down;
    let high = old_y.max(m.pos.y) + t.step_up;
    let mut support: Option<f32> = None;
    let mut consider = |s: f32| {
        if s >= low - EPS && s <= high && support.is_none_or(|best| s > best) {
            support = Some(s);
        }
    };
    consider(level.floor_height(m.pos.x));
    for r in level.solids() {
        if over(&r, m.pos.x) {
            consider(r.max.y);
        }
    }

    if let Some(s) = support {
        if !was_grounded {
            let impact = -m.vel.y;
            ev.landed = Some(impact);
            if impact > t.heavy_landing_speed {
                m.vel.x *= t.heavy_landing_keep;
            }
        }
        m.pos.y = s;
        m.vel.y = 0.0;
        m.grounded = true;
    } else {
        m.grounded = false;
    }

    // Safety net: never end up inside the sea floor.
    let floor = level.floor_height(m.pos.x);
    if m.pos.y < floor - t.step_up {
        m.pos.y = floor;
        m.vel.y = m.vel.y.max(0.0);
    }
}

/// Is `pos` a fair place for a crab to stand (for example, a loaded save)?
/// Inside the level's walls, not inside a rock, and not under the floor.
pub fn is_valid_spot(pos: Vec2, t: &Tuning, level: &Level) -> bool {
    let hw = t.body_half_width;
    let inside_walls = pos.x >= level.min_x + hw && pos.x <= level.max_x - hw;
    let in_rock = level.solids().any(|r| {
        pos.x + hw > r.min.x + EPS
            && pos.x - hw < r.max.x - EPS
            && pos.y + t.body_height > r.min.y + EPS
            && pos.y < r.max.y - EPS
    });
    inside_walls && !in_rock && pos.y >= level.floor_height(pos.x) - 1.0 && pos.y.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::SPAWN;

    /// Shorthand for `Default::default()`, as Bevy's prelude has.
    fn default<T: Default>() -> T {
        T::default()
    }

    /// Run the physics for `seconds` with the same controls every step
    /// (`jump_pressed` and `dodge_pressed` only count on the first step).
    /// Returns all the events, merged.
    fn run(m: &mut Mover, c: Controls, seconds: f32, level: &Level) -> StepEvents {
        let t = Tuning::default();
        let mut all = StepEvents::default();
        let mut c = c;
        for _ in 0..(seconds / STEP).round() as usize {
            all.merge(step(m, &c, &t, level, STEP));
            c.jump_pressed = false;
            c.dodge_pressed = false;
        }
        all
    }

    /// Run until the crab lands (or `max` seconds pass); return the highest
    /// point reached.
    fn run_until_landed(m: &mut Mover, c: Controls, level: &Level, max: f32) -> f32 {
        let t = Tuning::default();
        let mut c = c;
        let mut top = m.pos.y;
        let mut left_ground = false;
        for _ in 0..(max / STEP) as usize {
            step(m, &c, &t, level, STEP);
            c.jump_pressed = false;
            c.dodge_pressed = false;
            top = top.max(m.pos.y);
            left_ground |= !m.grounded;
            if left_ground && m.grounded {
                break;
            }
        }
        top
    }

    const HOLD_JUMP: Controls = Controls {
        walk: 0.0,
        jump_held: true,
        jump_pressed: true,
        dodge_pressed: false,
        dodge_held: false,
        down: false,
    };

    #[test]
    fn standing_still_stays_put() {
        let level = Level::test_level();
        let mut m = Mover::new(SPAWN);
        run(&mut m, Controls::default(), 2.0, &level);
        assert!(m.grounded);
        assert!(m.pos.distance(SPAWN) < 0.01, "drifted to {}", m.pos);
    }

    #[test]
    fn walking_is_heavy_but_responsive() {
        let level = Level::test_level();
        let t = Tuning::default();
        let mut m = Mover::new(SPAWN);
        let right = Controls {
            walk: 1.0,
            ..default()
        };
        run(&mut m, right, 0.2, &level);
        assert_eq!(m.vel.x, t.walk_speed, "full speed within 0.2 s");
        run(&mut m, Controls::default(), 0.15, &level);
        assert_eq!(m.vel.x, 0.0, "stopped within 0.15 s");
    }

    #[test]
    fn full_jump_is_about_one_and_a_half_crabs_high() {
        let level = Level::test_level();
        let mut m = Mover::new(SPAWN);
        let top = run_until_landed(&mut m, HOLD_JUMP, &level, 3.0);
        assert!((170.0..220.0).contains(&top), "full jump reached {top}");
    }

    #[test]
    fn tapping_jump_gives_a_small_hop() {
        let level = Level::test_level();
        let mut m = Mover::new(SPAWN);
        let tap = Controls {
            jump_held: false,
            ..HOLD_JUMP
        };
        let top = run_until_landed(&mut m, tap, &level, 3.0);
        assert!((30.0..100.0).contains(&top), "hop reached {top}");
    }

    #[test]
    fn falls_are_capped_by_water_drag() {
        let level = Level::test_level();
        let mut m = Mover::new(Vec2::new(SPAWN.x, 5000.0));
        m.grounded = false;
        run(&mut m, Controls::default(), 3.0, &level);
        // A steady, gentle sink: about 350, not a free fall.
        assert!((-380.0..-320.0).contains(&m.vel.y), "fell at {}", m.vel.y);
    }

    #[test]
    fn paddle_strokes_use_stamina() {
        let level = Level::test_level();
        let t = Tuning::default();
        let mut m = Mover::new(SPAWN);
        let first = run(&mut m, HOLD_JUMP, 0.4, &level);
        assert!(first.jumped && !first.paddled);
        let none = run(&mut m, HOLD_JUMP, 0.2, &level);
        assert!(!none.paddled, "no strokes grown yet: no double jump");
        run_until_landed(&mut m, Controls::default(), &level, 5.0);

        m.upgrades = 3;
        m.stamina = 3.0;
        run(&mut m, HOLD_JUMP, 0.4, &level);
        for n in 0..3 {
            let ev = run(&mut m, HOLD_JUMP, t.stroke_interval + STEP, &level);
            assert!(ev.paddled, "stroke {n} (the first is the double jump)");
        }
        assert_eq!(m.stamina, 0.0);
        let spent = run(&mut m, HOLD_JUMP, 0.2, &level);
        assert!(
            !spent.paddled && !spent.jumped,
            "spent: a press does nothing"
        );
        run_until_landed(&mut m, Controls::default(), &level, 5.0);
        let again = run(&mut m, HOLD_JUMP, 0.2, &level);
        assert!(again.jumped, "landing restores the jump");
    }

    /// Put a crab walking right at full speed, a few steps from walking off
    /// the plateau into the trench. (Jumping from here, the crab leaves the
    /// floor just as its crouch ends.)
    fn at_trench_edge(level: &Level) -> Mover {
        let t = Tuning::default();
        let (left, _) = level.trench();
        let x = left + t.body_half_width - t.walk_speed * t.jump_windup;
        let mut m = Mover::new(Vec2::new(x, level.floor_height(left - 1.0)));
        m.vel.x = t.walk_speed;
        m.upgrades = 1; // the double jump, for the trench
        m.stamina = 1.0;
        m
    }

    #[test]
    fn coyote_time_allows_a_late_jump() {
        let level = Level::test_level();
        let mut m = at_trench_edge(&level);
        let right = Controls {
            walk: 1.0,
            ..default()
        };
        // Walk until the floor is gone, then a little longer.
        while m.grounded {
            run(&mut m, right, STEP, &level);
        }
        run(&mut m, right, 0.06, &level);
        let late = run(
            &mut m,
            Controls {
                walk: 1.0,
                ..HOLD_JUMP
            },
            STEP,
            &level,
        );
        assert!(late.jumped, "a jump 0.06 s after the edge still counts");

        // Too late: then the press becomes a paddle stroke instead.
        let mut m = at_trench_edge(&level);
        while m.grounded {
            run(&mut m, right, STEP, &level);
        }
        run(&mut m, right, 0.2, &level);
        let too_late = run(
            &mut m,
            Controls {
                walk: 1.0,
                ..HOLD_JUMP
            },
            STEP,
            &level,
        );
        assert!(!too_late.jumped && too_late.paddled);
    }

    #[test]
    fn early_jump_press_is_buffered_until_landing() {
        let level = Level::test_level();
        let mut m = Mover::new(SPAWN + Vec2::new(0.0, 30.0));
        m.grounded = false;
        m.stamina = 0.0; // so the early press can't become a paddle
        // Fall until just above the floor, then press jump.
        while m.pos.y > SPAWN.y + 8.0 {
            run(&mut m, Controls::default(), STEP, &level);
        }
        let events = run(&mut m, HOLD_JUMP, 0.12, &level);
        assert!(events.landed.is_some() && events.jumped, "{events:?}");
    }

    /// Run at the trench holding right and jump at the edge, optionally
    /// mid-dodge (the fastest take-off there is) and/or paddling at the top of
    /// the jump. Returns where the crab lands.
    fn try_trench(dodging: bool, paddle: bool) -> Vec2 {
        let level = Level::test_level();
        let t = Tuning::default();
        let mut m = at_trench_edge(&level);
        if dodging {
            m.vel.x = t.dodge_speed;
            m.dodge_left = t.dodge_time;
            m.dodge_dir = 1.0;
        }
        let mut c = Controls {
            walk: 1.0,
            ..HOLD_JUMP
        };
        let (mut jumped, mut paddled) = (false, false);
        for _ in 0..600 {
            let ev = step(&mut m, &c, &t, &level, STEP);
            c.jump_pressed = false;
            jumped |= ev.jumped;
            if paddle && jumped && !paddled && m.vel.y <= 0.0 {
                c.jump_pressed = true;
                paddled = true;
            }
            if ev.landed.is_some() {
                break;
            }
        }
        m.pos
    }

    #[test]
    fn the_trench_needs_the_paddle_double_jump() {
        let level = Level::test_level();
        let (_, right) = level.trench();
        let single = try_trench(false, false);
        let paddle = try_trench(false, true);
        println!(
            "landed: one jump {single}, dodge-jump {}, with paddle {paddle}",
            try_trench(true, false)
        );
        assert!(
            single.x < right && single.y < 0.0,
            "one jump falls in: {single}"
        );
        let dodge_jump = try_trench(true, false);
        assert!(
            dodge_jump.y < 0.0,
            "a dodge-jump still falls in: {dodge_jump}"
        );
        let double = try_trench(false, true);
        assert!(double.y > 100.0, "jump + paddle gets across: {double}");
    }

    #[test]
    fn no_dodge_in_the_water() {
        let level = Level::test_level();
        let mut m = Mover::new(SPAWN);
        run(&mut m, HOLD_JUMP, 0.3, &level);
        let ev = run(
            &mut m,
            Controls {
                dodge_pressed: true,
                ..default()
            },
            STEP,
            &level,
        );
        assert!(!ev.dodged);
    }

    #[test]
    fn dodge_is_a_quick_burst_with_a_cooldown() {
        let level = Level::test_level();
        let mut walker = Mover::new(SPAWN);
        let mut dodger = Mover::new(SPAWN);
        let right = Controls {
            walk: 1.0,
            ..default()
        };
        run(&mut walker, right, 0.3, &level);
        let ev = run(
            &mut dodger,
            Controls {
                dodge_pressed: true,
                ..right
            },
            0.3,
            &level,
        );
        assert!(ev.dodged);
        assert!(dodger.pos.x > walker.pos.x + 50.0, "dodge goes further");
        let again = run(
            &mut dodger,
            Controls {
                dodge_pressed: true,
                ..right
            },
            STEP,
            &level,
        );
        assert!(!again.dodged, "no second dodge during the cooldown");
    }

    #[test]
    fn crab_cannot_walk_through_a_rock_or_a_cliff() {
        let level = Level::test_level();
        let t = Tuning::default();
        // The boulder on the slope sits at x 640..720.
        let mut m = Mover::new(Vec2::new(560.0, level.floor_height(560.0)));
        run(
            &mut m,
            Controls {
                walk: 1.0,
                ..default()
            },
            2.0,
            &level,
        );
        assert!(
            m.pos.x <= 640.0 - t.body_half_width + 0.1,
            "stopped at the boulder: {}",
            m.pos
        );
        // In the trench, walking left stops at the cliff.
        let mut m = Mover::new(Vec2::new(1500.0, -260.0));
        run(
            &mut m,
            Controls {
                walk: -1.0,
                ..default()
            },
            3.0,
            &level,
        );
        assert!(
            m.pos.x >= 1301.0 + t.body_half_width - 0.1,
            "stopped at the cliff: {}",
            m.pos
        );
        assert_eq!(m.pos.y, -260.0);
    }

    #[test]
    fn walks_up_and_down_the_slope_without_falling() {
        let level = Level::test_level();
        let mut m = Mover::new(Vec2::new(560.0, level.floor_height(560.0)));
        // Up the slope. The boulder is in the way, so jump over it first.
        run(
            &mut m,
            Controls {
                walk: 1.0,
                ..HOLD_JUMP
            },
            1.2,
            &level,
        );
        run(
            &mut m,
            Controls {
                walk: 1.0,
                ..default()
            },
            1.0,
            &level,
        );
        assert!(m.grounded && m.pos.x > 820.0, "on the plateau: {}", m.pos);
        // Down the far slope: grounded the whole way.
        let mut m = Mover::new(Vec2::new(2050.0, level.floor_height(2050.0)));
        for _ in 0..120 {
            run(
                &mut m,
                Controls {
                    walk: 1.0,
                    ..default()
                },
                STEP,
                &level,
            );
            assert!(m.grounded, "left the ground at {}", m.pos);
        }
    }

    #[test]
    fn valid_spots() {
        let level = Level::test_level();
        let t = Tuning::default();
        assert!(is_valid_spot(SPAWN, &t, &level));
        assert!(
            !is_valid_spot(Vec2::new(680.0, 50.0), &t, &level),
            "inside the boulder"
        );
        assert!(
            !is_valid_spot(Vec2::new(-100.0, 0.0), &t, &level),
            "outside the walls"
        );
        assert!(
            !is_valid_spot(Vec2::new(300.0, -50.0), &t, &level),
            "under the floor"
        );
        assert!(!is_valid_spot(Vec2::new(f32::NAN, 0.0), &t, &level));
    }
}
