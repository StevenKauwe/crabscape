//! The canyon's alarm jelly (`docs/design/creatures/alarm-jelly.md`): one
//! hangs in the deep still water by the far wall, below the fifth egg's
//! cave. It is weather, not a gate: the flood still carries a crab up to the
//! egg past it, and a crab sinking home from the cave with its claws up
//! takes no sting. It is part of the populated world, so in co-op the host
//! runs it like any creature.

use bevy_math::Vec2;
use crabsim::{Controls, Kind, PlayerId, Sim};

const P0: PlayerId = PlayerId(0);

fn populated(at: Vec2) -> Sim {
    let mut sim = Sim::with_one_crab(at);
    sim.populate();
    sim
}

fn canyon_jelly(sim: &Sim) -> &crabsim::Enemy {
    let spot = sim.level.drifting[0];
    sim.enemies()
        .iter()
        .find(|e| e.kind == Kind::AlarmJelly && e.home.distance(spot) < 1.0)
        .expect("a jelly hangs at the canyon's first drift spot")
}

#[test]
fn a_jelly_hangs_in_the_canyons_still_water_and_stays_there() {
    let mut sim = populated(Vec2::new(160.0, 0.0));
    let home = canyon_jelly(&sim).home;
    let moving = sim.level.currents.iter().chain(&sim.level.upwelling);
    assert!(
        !moving.into_iter().any(|c| c.contains(home)),
        "out of the tide"
    );
    for _ in 0..120 * 120 {
        sim.tick();
        let e = canyon_jelly(&sim);
        assert!(
            e.pos.distance(home) < 200.0,
            "drifts near its spot: {}",
            e.pos
        );
        assert!(!sim.level.currents.iter().any(|c| c.contains(e.pos)));
    }
}

/// From inside the far wall's cave, walk out and sink to the floor, the
/// claws up or not; returns the venom doses taken and where it landed.
fn sink_from_the_cave(guard: bool) -> (usize, Vec2) {
    let egg = Sim::with_one_crab(Vec2::ZERO).level.eggs[4];
    let mut sim = populated(egg - Vec2::new(0.0, 25.0));
    let mut doses = 0;
    for i in 0..120 * 20 {
        sim.set_controls(
            P0,
            Controls {
                // Guarding walks at 40%: the same distance out takes longer.
                walk: if i < if guard { 300 } else { 120 } * 2 {
                    -0.5
                } else {
                    0.0
                },
                block_held: guard,
                ..Default::default()
            },
        );
        let before = sim.crab(P0).unwrap().fighter.venom.len();
        sim.tick();
        doses += sim
            .crab(P0)
            .unwrap()
            .fighter
            .venom
            .len()
            .saturating_sub(before);
    }
    (doses, sim.crab(P0).unwrap().mover.pos)
}

#[test]
fn sinking_home_from_the_cave_past_the_jelly_with_claws_up_is_safe() {
    // Claws down, the tentacle finds it on the way: the jelly is a real
    // hazard on the descent...
    let (bare, landed) = sink_from_the_cave(false);
    assert!(
        bare > 0,
        "the tentacle hangs across the way down ({landed})"
    );
    // ...and claws up, the way home is safe.
    let (guarded, landed) = sink_from_the_cave(true);
    assert_eq!(guarded, 0, "claws up block every sting");
    assert!(landed.y < -9000.0, "down on the canyon floor: {landed}");
}

#[test]
fn the_flood_still_carries_a_crab_past_the_jelly_to_the_fifth_egg() {
    let eaten = (0..60).any(|k| {
        let steer = 4.0 + f64::from(k) * 0.25;
        let mut sim = populated(Vec2::new(7200.0, -9400.0));
        for i in 0..120 * 40u32 {
            let t = f64::from(i) / 120.0;
            sim.set_controls(
                P0,
                Controls {
                    jump_pressed: (2.5..2.5 + 1.0 / 120.0).contains(&t),
                    jump_held: (2.5..2.7).contains(&t),
                    walk: if t >= steer { 1.0 } else { 0.0 },
                    ..Default::default()
                },
            );
            sim.tick();
        }
        sim.crabs()[0].eaten[4]
    });
    assert!(eaten);
}
