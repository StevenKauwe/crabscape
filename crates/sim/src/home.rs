//! Home: an empty sea-snail shell lying on the floor near the start, and how
//! a crab goes into it.
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

use crate::level::Level;
use crate::physics::{Controls, Mover, StepEvents};
use crate::tuning::Tuning;

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
        && level.home.contains(m.pos)
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
/// shell, instead of its physics step: it moves only as this says.
pub fn step(m: &mut Mover, c: &Controls, level: &Level, t: &Tuning, dt: f32) -> StepEvents {
    let mut ev = StepEvents::default();
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
        m.pos.x = approach(from, level.home_den.x, stride);
        let was = m.tucked;
        m.tucked = (was + dt / t.home_in_time).min(1.0);
        if m.tucked >= 1.0 {
            m.pos.x = level.home_den.x;
            if was < 1.0 {
                ev.tucked_in = true;
            }
        }
    } else {
        // Out through the doorway to the doorstep.
        m.pos.x = approach(from, level.home.max.x, stride);
        m.tucked = (m.tucked - dt / t.home_out_time).max(0.0);
        if m.tucked <= 0.0 {
            m.pos.x = level.home.max.x;
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

/// Move `value` towards `target` by at most `amount`.
fn approach(value: f32, target: f32, amount: f32) -> f32 {
    if value < target {
        (value + amount).min(target)
    } else {
        (value - amount).max(target)
    }
}
