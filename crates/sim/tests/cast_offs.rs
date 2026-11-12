//! Several cast-off shells can wait at once (`docs/design/world.md`, Death):
//! a crab that dies in the open twice, without going back for the first
//! shell, leaves two, each holding what it carried. Taking one back leaves
//! the other, and the list survives a save.
use bevy_math::Vec2;
use crabsim::{Carry, Controls, Hit, Level, PlayerId, Progress, Shelter, Sim, Tuning};

const P0: PlayerId = PlayerId(0);

fn blow() -> Hit {
    Hit {
        damage: 40.0,
        poise: 0.0,
        ..Hit::default()
    }
}

/// A world with nothing living in it: only the death rule is on trial.
fn quiet_world() -> Sim {
    let mut level = Level::test_level();
    level.foes.clear();
    let mut sim = Sim::new(Tuning::default(), level);
    sim.add_crab(P0, Vec2::new(160.0, 0.0));
    sim
}

fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.tick();
    }
}

/// Die where the crab stands, wake, and leave the shell behind.
fn die_here(sim: &mut Sim) {
    for _ in 0..3 {
        sim.hit_crab(P0, &blow());
        sim.tick();
    }
    assert!(sim.crab(P0).unwrap().fighter.dead.is_some());
    idle(sim, 200);
    assert_eq!(
        sim.crab(P0).unwrap().mover.pos,
        sim.level.home_den,
        "woke at home"
    );
}

/// Put the crab on the floor at `x`, carrying `what` and nothing else.
fn stand_with(sim: &mut Sim, x: f32, what: Carry) {
    let y = sim.level.floor_height(x);
    let crab = sim.crab_mut(P0).unwrap();
    crab.mover.pos = Vec2::new(x, y);
    crab.mover.vel = Vec2::ZERO;
    crab.mover.shelter = Shelter::Out;
    crab.mover.carrying = Some(what);
    crab.mover.clinging.clear();
    sim.shared.clams.fill(u32::MAX);
    sim.shared.grit_taken.fill(true);
}

#[test]
fn two_deaths_leave_two_shells_each_holding_its_load() {
    let mut sim = quiet_world();
    let (first, second) = (300.0, 2500.0);
    stand_with(&mut sim, first, Carry::Culture);
    sim.tick();
    die_here(&mut sim);
    assert_eq!(sim.crab(P0).unwrap().cast_offs.len(), 1);

    stand_with(&mut sim, second, Carry::Tooth);
    sim.tick();
    die_here(&mut sim);
    let shells = sim.crab(P0).unwrap().cast_offs.clone();
    assert_eq!(shells.len(), 2, "both wait: {shells:?}");
    assert_eq!(shells[0].carrying, Carry::Culture);
    assert_eq!(shells[1].carrying, Carry::Tooth);
    assert!((shells[0].pos.x - first).abs() < 40.0, "{shells:?}");
    assert!((shells[1].pos.x - second).abs() < 40.0, "{shells:?}");
    assert!(shells.iter().all(|s| s.recoverable));

    // The list survives a save, through its text and back.
    let saved = sim.progress(P0).unwrap();
    let text = serde_json::to_string(&saved).unwrap();
    let saved: Progress = serde_json::from_str(&text).unwrap();
    let mut loaded = quiet_world();
    loaded.restore_progress(P0, &saved);
    assert_eq!(loaded.crab(P0).unwrap().cast_offs, shells);

    // Going back for the second one leaves the first where it fell.
    let mut sim = loaded;
    let wanted = shells[1];
    stand_with(&mut sim, wanted.pos.x, Carry::Grit);
    sim.crab_mut(P0).unwrap().mover.carrying = None;
    sim.set_controls(P0, Controls::default());
    let mut recovered = false;
    for _ in 0..240 {
        sim.tick();
        recovered |= sim.crab(P0).unwrap().events.recovered;
        if recovered {
            break;
        }
    }
    let crab = sim.crab(P0).unwrap();
    assert!(recovered, "the second shell was taken back");
    assert_eq!(crab.mover.carrying, Some(Carry::Tooth));
    assert_eq!(crab.cast_offs, vec![shells[0]], "the first still waits");
}
