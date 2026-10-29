//! Home: an empty sea-snail shell lying on the floor near the start, and how
//! a crab goes into it, or into a crevice out in the level (a rest stop:
//! [`crate::level::Crevice`]), the same way.
//!
//! Crabs shelter in crevices, empty shells and litter, and this one goes
//! right inside. Walking into the doorway on the floor, it backs in, claws to
//! the door (that crabs back in claws first: not verified), and is gone
//! inside.
//! - It walks in at its walking pace and settles over
//!   [`Tuning::home_in_time`]. Nothing else moves the crab meanwhile, but
//!   it's never stuck: walking out turns it round at once.
//! - Tucked right in ([`Mover::tucked_in`]), it rests (see
//!   [`Sim::tick`](crate::Sim::tick)): the stroke rings and the reserve fill,
//!   the shell mends after a moment unhit, and what it carried home is put to
//!   use at the workbench beside it.
//! - Walking out (or a jump or dodge press) brings it back out through the
//!   doorway in [`Tuning::home_out_time`], walking on as it steps clear.
//! - Inside, the claws stay folded: no swings, no guard.
//!
//! A crevice is the same in and out, but it has no workbench and no larder:
//! resting there only mends and refills, and makes it where the crab wakes.
//!
//! **Going home** ([`Sim::go_home`], the Home button): the crab is put
//! straight back in its home shell, tucked in, as if it had walked in. When
//! it may is one rule, [`may_go_home`]: any time, for now, even mid-fight.
//! It goes through the sim like a code ([`crate::codes`]): it changes only a
//! crab of this device, between two ticks, and a running recording notes it
//! (`home` in a replay).

use bevy_math::Vec2;

use crate::combat::{Fighter, Vitals};
use crate::level::{Level, Place};
use crate::physics::{Controls, Mover, StepEvents};
use crate::tuning::Tuning;
use crate::world::{PlayerId, Sim, SimCrab};

/// Where a crab is with its home shell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Shelter {
    /// Out in the open.
    #[default]
    Out,
    /// Backing in, or tucked in ([`Mover::tucked`] = 1).
    In,
    /// Coming back out through the doorway.
    Leaving,
}

/// The shell's opening faces right, so in is to the left.
const INWARD: f32 = -1.0;

/// How far the stick must push before it counts as walking in or out.
const PUSH: f32 = 0.3;

/// Does a crab out in the open walk into the doorway now? It must be on the
/// floor in the doorway, `free` (no swing, guard or hit holding it) and
/// walking in, by the controls the fight leaves it (planted feet don't), and
/// not about to jump or dodge.
pub fn walks_in(m: &Mover, c: &Controls, free: bool, level: &Level) -> bool {
    m.shelter == Shelter::Out
        && free
        && m.grounded
        && !m.climbing
        && m.dodge_left <= 0.0
        && m.windup <= 0.0
        && m.jump_buffer <= 0.0
        && !c.jump_pressed
        && !c.dodge_pressed
        && c.walk * INWARD > PUSH
        && level.place_at(m.pos).is_some()
}

/// What reaches the fight from a crab in its shell: walking and the jump
/// and dodge presses (which bring it out), never the claws.
pub fn claws_folded(c: Controls) -> Controls {
    Controls {
        smash_pressed: false,
        smash_held: false,
        jab_pressed: false,
        block_held: false,
        block_pressed: false,
        ..c
    }
}

/// One step of a crab going into, resting in, or coming out of its home
/// shell (or a crevice), instead of its physics step: it moves only as this
/// says.
pub fn step(m: &mut Mover, c: &Controls, level: &Level, t: &Tuning, dt: f32) -> StepEvents {
    let mut ev = StepEvents::default();
    // Which shelter: the one whose doorway it is in (it never leaves the
    // doorway while going in or out).
    let (door, den) = level.door_and_den(level.place_at(m.pos).unwrap_or(Place::Home));
    let walk_in = c.walk * INWARD > PUSH;
    let walk_out = c.walk * INWARD < -PUSH || c.jump_pressed || c.dodge_pressed;
    match m.shelter {
        Shelter::Out => {
            m.shelter = Shelter::In;
            ev.went_in = true;
            // Whatever it was about to do out there, it isn't now.
            m.dodge_buffer = 0.0;
            m.rising = false;
            m.sprinting = false;
            m.diving = false;
        }
        Shelter::In if walk_out => m.shelter = Shelter::Leaving,
        Shelter::Leaving if walk_in => {
            m.shelter = Shelter::In;
            ev.went_in = true;
        }
        _ => {}
    }
    // It walks in or out at its walking pace, and settles in (or straightens
    // up out of the doorway) over the time it takes.
    let from = m.pos.x;
    let stride = t.walk_speed * dt;
    if m.shelter == Shelter::In {
        // In to the den at the back, then settling there.
        m.pos.x = approach(from, den.x, stride);
        let was = m.tucked;
        m.tucked = (was + dt / t.home_in_time).min(1.0);
        if m.tucked >= 1.0 {
            m.pos.x = den.x;
            if was < 1.0 {
                ev.tucked_in = true;
            }
        }
    } else {
        // Out through the doorway to the doorstep.
        m.pos.x = approach(from, door.max.x, stride);
        m.tucked = (m.tucked - dt / t.home_out_time).max(0.0);
        if m.tucked <= 0.0 {
            m.pos.x = door.max.x;
            m.shelter = Shelter::Out;
            ev.came_out = true;
        }
    }
    // Walking as it goes, always on the floor. Walking out, it steps clear
    // at walking speed and carries on.
    m.vel.x = (m.pos.x - from) / dt;
    if ev.came_out && c.walk * INWARD < -PUSH {
        m.vel.x = c.walk.clamp(-1.0, 1.0) * t.walk_speed;
    }
    m.vel.y = 0.0;
    m.grounded = true;
    ev
}

/// The Home button's one rule: may this crab go straight home now? Any
/// time, for now (the owner's call), even in the middle of a fight. Only a
/// dead crab can't: dying already takes it home (or to its crevice), leaving
/// what it carried behind. To limit the button later (say, not while a
/// creature is hunting it), change this alone.
pub fn may_go_home(crab: &SimCrab) -> bool {
    crab.fighter.dead.is_none()
}

impl Sim {
    /// Could crab `id` go home now? It must be one of this device's crabs,
    /// allowed by [`may_go_home`], and not already tucked in at home.
    pub fn can_go_home(&self, id: PlayerId) -> bool {
        self.crab(id).is_some_and(|c| {
            let home = c.mover.tucked_in() && self.level.place_at(c.mover.pos) == Some(Place::Home);
            !c.remote && may_go_home(c) && !home
        })
    }

    /// Crab `id` goes straight home: at once (between ticks), tucked into
    /// its home shell as if it had walked in, and a running recording notes
    /// it. Returns false, changing nothing, if it can't
    /// ([`Sim::can_go_home`]).
    ///
    /// It keeps what it is and what it carries (its load comes off at the
    /// workbench, as when it walks in) and its hurts (the shell mends after
    /// the usual moment unhit, venom wears off). The fight is left behind:
    /// a swing, guard, stagger or knockback stops, a creature in its crusher
    /// or holding it in its jaws is let go, and creatures lose it as they
    /// lose any crab tucked in.
    pub fn go_home(&mut self, id: PlayerId) -> bool {
        if !self.can_go_home(id) {
            return false;
        }
        let recorded = self.crabs().iter().find(|c| !c.remote).map(|c| c.id) == Some(id);
        let tick = self.tick_count();
        let (_, den) = self.level.door_and_den(Place::Home);
        let fresh = Fighter::new(&self.tuning);
        let seized = self
            .crab(id)
            .and_then(|c| c.fighter.crush.map(|cr| cr.target));
        if let Some(e) = self.enemies_mut().iter_mut().find(|e| Some(e.id) == seized) {
            e.held = false;
        }
        let Some(crab) = self.crab_mut(id) else {
            return false;
        };
        let f = &mut crab.fighter;
        *f = Fighter {
            vitals: Vitals {
                health: f.vitals.health,
                unhit: f.vitals.unhit,
                ..fresh.vitals
            },
            facing: f.facing,
            venom: std::mem::take(&mut f.venom),
            legs_lost: f.legs_lost,
            ..fresh
        };
        let still = Mover::new(den);
        let m = &mut crab.mover;
        *m = Mover {
            pos: den,
            vel: Vec2::ZERO,
            grounded: true,
            coyote: still.coyote,
            jump_buffer: still.jump_buffer,
            windup: still.windup,
            rising: still.rising,
            stroke_queued: still.stroke_queued,
            dodge_left: still.dodge_left,
            dodge_buffer: still.dodge_buffer,
            sprinting: still.sprinting,
            climbing: still.climbing,
            diving: still.diving,
            flip_left: still.flip_left,
            dash_left: still.dash_left,
            climb_dir: still.climb_dir,
            shelter: Shelter::In,
            tucked: 1.0,
            ..m.clone()
        };
        crab.prev_pos = den;
        crab.controls = crab.controls.held_only();
        crab.reset_rest();
        crab.pending.went_home = true;
        crab.pending.tucked_in = true;
        if recorded {
            self.note_home(tick);
        }
        true
    }
}

/// Move `value` towards `target` by at most `amount`.
fn approach(value: f32, target: f32, amount: f32) -> f32 {
    if value < target {
        (value + amount).min(target)
    } else {
        (value - amount).max(target)
    }
}
