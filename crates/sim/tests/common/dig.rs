//! A scripted dig on the real level: in at the first silt shaft of the home
//! burrow, down to the gallery, along it. Shared by the dig tests by path.

use bevy_math::Vec2;
use crabsim::{Controls, Level, PlayerId, Sim, Skill, Tuning};

pub const P0: PlayerId = PlayerId(0);

/// A sim with the cell window on the first shaft's top.
pub fn dig_sim() -> Sim {
    let mut level = Level::test_level();
    level.foes.clear();
    let at = Vec2::new(172.0, 40.0);
    let mut sim = Sim::new(Tuning::default(), level);
    sim.add_crab(P0, at);
    sim.enable_underwater_cells();
    sim.crab_mut(P0).expect("crab").mover.pos = at;
    follow(&mut sim);
    sim
}

/// What the game does every frame: the cell window follows the crab.
pub fn follow(sim: &mut Sim) {
    let p = sim.crab(P0).expect("crab").mover.pos;
    let mut cells = sim.level.cells.take().expect("cells");
    cells.follow_player(&sim.level, p.x, p.y);
    sim.level.cells = Some(cells);
}

pub fn excavating(sim: &Sim) -> u32 {
    sim.crab(P0)
        .expect("crab")
        .econ
        .skills
        .get(Skill::Excavating)
}

/// Run `ticks` with these controls (the window follows).
#[allow(dead_code)]
pub fn hold(sim: &mut Sim, controls: Controls, ticks: u32) {
    for _ in 0..ticks {
        sim.set_controls(P0, controls);
        follow(sim);
        sim.tick();
    }
}

/// Tunnel steadily: down the shaft to the gallery, then east along it, until
/// the gallery's far end (x 586) or `secs` of sim time. Returns the
/// Excavating experience earned and the sim seconds it took.
pub fn tunnel(sim: &mut Sim, secs: f64) -> (u32, f64) {
    let from = excavating(sim);
    let dig = Controls {
        dig_held: true,
        ..Controls::default()
    };
    let mut ticks = 0;
    while f64::from(ticks) < secs * 120.0 {
        let p = sim.crab(P0).expect("crab").mover.pos;
        if p.x > 586.0 {
            break;
        }
        let c = if p.y > -520.0 {
            Controls { down: true, ..dig }
        } else {
            Controls { walk: 1.0, ..dig }
        };
        sim.set_controls(P0, c);
        follow(sim);
        sim.tick();
        ticks += 1;
    }
    (excavating(sim) - from, f64::from(ticks) / 120.0)
}
