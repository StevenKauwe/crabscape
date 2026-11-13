//! The first descent (region 2): sand never buries the way from the mouth shaft's foot to the plug shaft.
//!
//! The live world once showed a dune at the shaft's foot. A fresh cellular world with the crab parked at the
//! mouth (where the trench floor's sand can spill in) is ticked for ten game minutes; the entry hall's floor
//! from the shaft foot to the plug shaft's foot must stay a flat walk, and a crab must still walk it.
use bevy_math::Vec2;
use crabsim::level::silt_galleries as sg;
use crabsim::underwater::{Cell, World};
use crabsim::{Controls, Level, PlayerId, Sim, Tuning};

const P: PlayerId = PlayerId(0);
/// Ten game minutes of fixed ticks.
const MINUTES: u32 = 10;
/// The tallest bed of sand allowed on the hall's floor west of the shaft (the crab is 65 tall; measured: 18 at worst).
const SAND_LIMIT: f32 = 24.0;
/// Under the shaft itself the trench's sand settles in a low ramp (measured: 46 units at the east corner after ten
/// minutes, 22 after twenty); it must stay under the crab's own height so the door east of it stays open.
const FOOT_LIMIT: f32 = 56.0;

fn cell_at(w: &World, x: f32, y: f32) -> Cell {
    let (cx, cy) = w.local_cell(x, y);
    w.cell(cx.floor() as i32, cy.floor() as i32)
}

/// A cellular world with a crab parked at `at`.
fn parked(at: Vec2) -> Sim {
    let mut level = Level::test_level();
    level.foes.clear();
    let mut sim = Sim::new(Tuning::default(), level);
    sim.add_crab(P, at);
    sim.enable_underwater_cells();
    sim
}

/// How far above the entry hall's floor the first water is in the column at `x` (0 on a clean floor).
fn bed_height(sim: &Sim, x: f32) -> f32 {
    let w = sim.level.cells.as_ref().expect("cells");
    let hall = sg::rooms()
        .iter()
        .find(|r| r.name == "entry hall")
        .expect("the entry hall")
        .rect;
    let mut y = hall.min.y + 2.0;
    while y < hall.max.y && cell_at(w, x, y) != Cell::Water {
        y += 4.0;
    }
    y - hall.min.y
}

#[test]
fn ten_minutes_at_the_mouth_leave_the_hall_floor_flat_and_walkable() {
    let hall = sg::rooms()
        .iter()
        .find(|r| r.name == "entry hall")
        .expect("the entry hall")
        .rect;
    let plug_foot = sg::PLUG.center().x;
    // The crab parks where the trench floor's sand could spill into the shaft: the mouth's west lip.
    let mut sim = parked(Vec2::new(sg::MOUTH.min.x - 60.0, sg::MOUTH.max.y - 10.0));
    for _ in 0..MINUTES * 60 * 120 {
        sim.tick();
    }
    // The tallest bed west of the shaft, and the tallest under it (to the hall's east wall).
    let (mut worst, mut foot_worst) = ((0.0_f32, 0.0_f32), 0.0_f32);
    let mut x = plug_foot - 40.0;
    while x < hall.max.x {
        let h = bed_height(&sim, x);
        if x < sg::MOUTH.min.x {
            if h > worst.0 {
                worst = (h, x);
            }
        } else {
            foot_worst = foot_worst.max(h);
        }
        x += 8.0;
    }
    let mut profile = String::new();
    let mut px = plug_foot - 40.0;
    while px < hall.max.x {
        profile += &format!(" {}:{}", px, bed_height(&sim, px));
        px += 80.0;
    }
    println!("profile{profile}");
    println!(
        "tallest bed after {MINUTES} min: {} units at x {} west of the shaft, {foot_worst} under it",
        worst.0, worst.1
    );
    assert!(
        foot_worst <= FOOT_LIMIT,
        "a {foot_worst}-unit dune under the shaft"
    );
    assert!(
        worst.0 <= SAND_LIMIT,
        "sand stands {} units high at x {} on the hall floor",
        worst.0,
        worst.1
    );
    // And the walk: the crab, set down at the shaft's foot, walks west to the plug shaft.
    let foot = Vec2::new(sg::MOUTH.center().x, hall.min.y + 1.0);
    {
        let m = &mut sim.crab_mut(P).unwrap().mover;
        m.pos = foot;
        m.vel = Vec2::ZERO;
    }
    for _ in 0..120 {
        sim.tick();
    }
    let mut arrived = false;
    for _ in 0..120 * 30 {
        sim.set_controls(
            P,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
        sim.tick();
        if sim.crab(P).unwrap().mover.pos.x < sg::PLUG.max.x {
            arrived = true;
            break;
        }
    }
    let at = sim.crab(P).unwrap().mover.pos;
    assert!(arrived, "the crab is stuck at {at:?}");
    assert!(
        at.y < hall.min.y + SAND_LIMIT + 70.0,
        "the crab climbed out of the hall: {at:?}"
    );
}
