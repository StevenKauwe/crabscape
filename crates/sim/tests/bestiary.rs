//! The bestiary scaffolding: every new kind works as a generic enemy.
use bevy_math::Vec2;
use crabsim::creatures;
use crabsim::enemy::{KINDS, Kind};
use crabsim::{Controls, PlayerId, Sim};

const P: PlayerId = PlayerId(0);

fn bestiary() -> Vec<Kind> {
    KINDS
        .into_iter()
        .filter(|k| {
            !matches!(
                k,
                // The shore crab is the world lead's to place (its `spots` are
                // empty for now); `tests/shore_crab.rs` covers it.
                Kind::Fixture | Kind::Mantis | Kind::Snapper | Kind::Moray | Kind::ShoreCrab
            )
        })
        .filter(|k| creatures::tuning(*k).is_some())
        .collect()
}

#[test]
fn ten_new_kinds_are_listed() {
    assert_eq!(bestiary().len(), 10);
}

#[test]
fn names_round_trip_and_bodies_exist() {
    let tuning = crabsim::tuning::Tuning::default();
    for kind in bestiary() {
        assert_eq!(Kind::parse(kind.name()), Some(kind));
        let k = kind.tuning(&tuning);
        assert!(k.vitals.health > 0.0, "{kind:?} has no health");
        assert!(k.body.0 > 0.0 && k.body.1 > 0.0, "{kind:?} has no body");
    }
}

#[test]
fn module_consts_agree_with_the_dispatch() {
    use creatures::*;
    assert_eq!(drops(Kind::LoggerheadTurtle), turtle::DROPS);
    assert_eq!(drops(Kind::HumboldtSquid), squid::DROPS);
    assert_eq!(drops(Kind::Viperfish), viperfish::DROPS);
    assert_eq!(drops(Kind::GoliathGrouper), grouper::DROPS);
    assert_eq!(drops(Kind::Lobster), lobster::DROPS);
    assert_eq!(drops(Kind::HermitCrab), hermit::DROPS);
    assert_eq!(drops(Kind::ShoreCrab), shore_crab::DROPS);
    assert_eq!(drops(Kind::BobbitWorm), bobbit::DROPS);
    assert_eq!(drops(Kind::GiantIsopod), isopod::DROPS);
    assert_eq!(drops(Kind::Hagfish), hagfish::DROPS);
    assert_eq!(drops(Kind::AmphipodSwarm), amphipods::DROPS);
    assert_eq!(midwater(Kind::LoggerheadTurtle), turtle::MIDWATER);
    assert_eq!(midwater(Kind::HumboldtSquid), squid::MIDWATER);
    assert_eq!(midwater(Kind::Viperfish), viperfish::MIDWATER);
    assert_eq!(midwater(Kind::GoliathGrouper), grouper::MIDWATER);
    assert_eq!(midwater(Kind::Lobster), lobster::MIDWATER);
    assert_eq!(midwater(Kind::HermitCrab), hermit::MIDWATER);
    assert_eq!(midwater(Kind::ShoreCrab), shore_crab::MIDWATER);
    assert_eq!(midwater(Kind::BobbitWorm), bobbit::MIDWATER);
    assert_eq!(midwater(Kind::GiantIsopod), isopod::MIDWATER);
    assert_eq!(midwater(Kind::Hagfish), hagfish::MIDWATER);
    assert_eq!(midwater(Kind::AmphipodSwarm), amphipods::MIDWATER);
    for kind in bestiary() {
        assert_eq!(kind.swims(), midwater(kind));
        assert_eq!(kind.keeps_depth(), midwater(kind));
    }
}

fn run(kind: Kind) -> u64 {
    let x = 1500.0;
    let mut sim = Sim::with_one_crab(Vec2::new(x, 0.0));
    let floor = sim.level.floor_height(x + 200.0);
    sim.spawn_enemy(kind, Vec2::new(x + 200.0, floor));
    for i in 0..1200 {
        sim.set_controls(
            P,
            Controls {
                walk: if (i / 300) % 2 == 0 { 1.0 } else { -1.0 },
                ..Default::default()
            },
        );
        sim.tick();
    }
    for e in sim.enemies() {
        assert!(e.pos.is_finite() && e.vel_x.is_finite(), "{kind:?} blew up");
    }
    let c = sim.crab(P).expect("crab");
    assert!(c.mover.pos.is_finite());
    sim.state_hash()
}

#[test]
fn each_kind_runs_the_generic_skeleton_deterministically() {
    for kind in bestiary() {
        assert_eq!(run(kind), run(kind), "{kind:?} is not deterministic");
    }
}

#[test]
fn midwater_kinds_keep_their_authored_depth() {
    for kind in bestiary() {
        let x = 1500.0;
        let mut sim = Sim::with_one_crab(Vec2::new(x, 0.0));
        let y = sim.level.floor_height(x + 300.0) + 400.0;
        sim.spawn_enemy(kind, Vec2::new(x + 300.0, y));
        let at = sim.enemies()[0].pos.y;
        if creatures::midwater(kind) {
            assert_eq!(at, y, "{kind:?} should keep its depth");
        } else {
            assert!(at < y, "{kind:?} should settle on the bed");
        }
    }
}

/// Deleted by the lanes as each creature gets placed.
#[test]
fn every_bestiary_creature_is_placed_in_the_world() {
    let level = crabsim::level::Level::test_level();
    for kind in bestiary() {
        assert!(
            level.foes.iter().any(|(k, _)| *k == kind),
            "{kind:?} has no encounter in the world"
        );
    }
}

/// One blow of `damage` lands on a lone creature of `kind` with `health` left
/// (put in `pose` first); the health it lost, and whether the blow killed it.
fn one_blow(
    kind: Kind,
    health: f32,
    damage: f32,
    pose: impl FnOnce(&mut crabsim::Enemy),
) -> (f32, bool) {
    use crabsim::combat::{Hit, Source};
    let mut sim = Sim::with_one_crab(Vec2::new(0.0, 0.0));
    sim.spawn_enemy(kind, Vec2::new(300.0, 0.0));
    let t = sim.tuning.clone();
    let e = &mut sim.enemies_mut()[0];
    e.vitals.health = health;
    pose(e);
    let hit = Hit::new(damage, 5.0, Source::Crab(P));
    let took = e.takes(&hit, 0.0, &t).expect("it can be hit");
    (health - e.vitals.health, took.died)
}

/// The old bug: the kill check used the raw damage and the armour was
/// refunded a step later, so a blow its armour shrugs off could still kill.
#[test]
fn a_blow_its_armour_shrugs_off_does_not_kill_it() {
    use creatures::{hermit, lobster};
    // Guarded lobster (approaching, claws up): a fifth of the damage.
    let (lost, died) = one_blow(Kind::Lobster, 10.0, 20.0, |e| {
        e.state = crabsim::enemy::State::Approach;
        e.facing = 1.0;
    });
    assert!(!died, "a guarded lobster at 10 survives a 20 blow");
    assert!((lost - 20.0 * lobster::GUARD_DAMAGE).abs() < 1e-4, "{lost}");
    // Withdrawn hermit: a tenth.
    let (lost, died) = one_blow(Kind::HermitCrab, 10.0, 50.0, hermit::withdraw);
    assert!(!died, "a withdrawn hermit at 10 survives a 50 blow");
    assert!((lost - 50.0 * hermit::SHELL_SHARE).abs() < 1e-4, "{lost}");
    // Isopod under its back armour: a quarter.
    let (lost, died) = one_blow(Kind::GiantIsopod, 10.0, 30.0, |_| {});
    assert!(!died, "an isopod at 10 survives a 30 blow on its back");
    assert!(
        (lost - 30.0 * creatures::isopod::ARMOUR).abs() < 1e-4,
        "{lost}"
    );
    // The kill check is on the real damage: flipped, the same blow kills.
    let (_, died) = one_blow(Kind::GiantIsopod, 8.0, 30.0, |_| {});
    assert!(!died);
    let (_, died) = one_blow(Kind::GiantIsopod, 8.0, 30.0, |e| {
        e.state = crabsim::enemy::State::Stagger; // flipped: double
    });
    assert!(died, "flipped, the belly takes double");
}
