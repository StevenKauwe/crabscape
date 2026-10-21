//! A crab's memory (`crates/sim/src/memory.rs`): what zooming out shows is
//! what the crab has seen and remembers, never the live world.

use bevy_math::Vec2;
use crabsim::memory::{FAINT, UNSEEN};
use crabsim::{Kind, Memory, PlayerId, Sim};

const ME: PlayerId = PlayerId(0);

fn sim_at(x: f32) -> Sim {
    Sim::with_one_crab(Vec2::new(x, 0.0))
}

fn run(sim: &mut Sim, seconds: f32) {
    for _ in 0..(seconds * 120.0) as u32 {
        sim.tick();
    }
}

/// Put the crab somewhere (as if it had walked there) and stand still.
fn put(sim: &mut Sim, x: f32) {
    let c = sim.crab_mut(ME).unwrap();
    c.mover.pos = Vec2::new(x, sim_floor(x));
    c.mover.vel = Vec2::ZERO;
}

fn sim_floor(x: f32) -> f32 {
    crabsim::Level::test_level().floor_height(x)
}

fn memory(sim: &Sim) -> &Memory {
    &sim.crab(ME).unwrap().memory
}

fn clarity(sim: &Sim, p: Vec2) -> f32 {
    let t = &sim.tuning.memory;
    let m = memory(sim);
    m.clarity(m.cell_at(p, t).unwrap(), t)
}

#[test]
fn unseen_places_stay_fog() {
    let mut sim = sim_at(400.0);
    run(&mut sim, 5.0);
    assert!(
        clarity(&sim, Vec2::new(400.0, 20.0)) > 0.5,
        "where it stands"
    );
    assert_eq!(clarity(&sim, Vec2::new(2400.0, 20.0)), UNSEEN, "far along");
    assert_eq!(
        clarity(&sim, Vec2::new(400.0, -3000.0)),
        UNSEEN,
        "far below"
    );
}

#[test]
fn home_is_always_known() {
    let mut sim = sim_at(1500.0);
    run(&mut sim, 1.0);
    let home = sim.level.home.center();
    assert_eq!(clarity(&sim, home), 1.0);
}

#[test]
fn seen_places_are_remembered_and_fade_without_visits() {
    let mut sim = sim_at(400.0);
    run(&mut sim, 1.0);
    put(&mut sim, 1600.0);
    run(&mut sim, 1.0);
    let spot = Vec2::new(400.0, 20.0);
    let fresh = clarity(&sim, spot);
    assert!(fresh > FAINT, "remembered after leaving ({fresh})");
    run(&mut sim, 240.0);
    let later = clarity(&sim, spot);
    assert!(later < fresh * 0.8, "fades: {fresh} -> {later}");
    assert!(later >= FAINT, "never gone ({later})");
}

#[test]
fn places_visited_often_stay_sharper() {
    // One visit to A; three visits (30 s apart) to B; then away as long from both.
    let mut sim = sim_at(400.0);
    run(&mut sim, 1.0);
    for _ in 0..3 {
        put(&mut sim, 1600.0);
        run(&mut sim, 1.0);
        put(&mut sim, 2400.0);
        run(&mut sim, 31.0);
    }
    put(&mut sim, 1600.0);
    run(&mut sim, 1.0);
    put(&mut sim, 2400.0);
    run(&mut sim, 1.0);
    let t = &sim.tuning.memory;
    let m = memory(&sim);
    let (a, b) = (
        m.cell_at(Vec2::new(400.0, 20.0), t).unwrap(),
        m.cell_at(Vec2::new(1600.0, sim_floor(1600.0) + 20.0), t)
            .unwrap(),
    );
    assert_eq!(m.visits(a), 1);
    assert!(m.visits(b) >= 3, "{}", m.visits(b));
    run(&mut sim, 300.0);
    assert!(
        clarity(&sim, Vec2::new(1600.0, sim_floor(1600.0) + 20.0))
            > clarity(&sim, Vec2::new(400.0, 20.0))
    );
}

#[test]
fn animals_out_of_sight_are_only_where_they_were_last_seen() {
    let mut sim = sim_at(400.0);
    let far = sim.spawn_enemy(Kind::Fixture, Vec2::new(2600.0, 0.0));
    let near = sim.spawn_enemy(Kind::Fixture, Vec2::new(550.0, 0.0));
    run(&mut sim, 1.0);
    let seen: Vec<u32> = memory(&sim).sightings.iter().map(|s| s.id).collect();
    assert!(seen.contains(&near), "the one in sight");
    assert!(!seen.contains(&far), "never the one out of sight");
    // The crab leaves; the animal it saw moves on.
    let was = memory(&sim)
        .sightings
        .iter()
        .find(|s| s.id == near)
        .unwrap()
        .pos;
    put(&mut sim, 1600.0);
    run(&mut sim, 0.5);
    sim.enemies_mut()
        .iter_mut()
        .find(|e| e.id == near)
        .unwrap()
        .pos
        .x = 900.0;
    run(&mut sim, 2.0);
    let s = memory(&sim)
        .sightings
        .iter()
        .find(|s| s.id == near)
        .unwrap();
    assert_eq!(s.pos, was, "remembered where it was, not where it is");
}

#[test]
fn a_remembered_animal_that_is_gone_is_forgotten_on_seeing_its_spot() {
    let mut sim = sim_at(400.0);
    let id = sim.spawn_enemy(Kind::Fixture, Vec2::new(550.0, 0.0));
    run(&mut sim, 1.0);
    put(&mut sim, 1600.0);
    run(&mut sim, 0.5);
    sim.enemies_mut()
        .iter_mut()
        .find(|e| e.id == id)
        .unwrap()
        .pos
        .x = 2700.0;
    put(&mut sim, 420.0);
    run(&mut sim, 0.5);
    assert!(memory(&sim).sightings.iter().all(|s| s.id != id));
}

#[test]
fn each_crab_has_its_own_memory() {
    let mut sim = sim_at(400.0);
    sim.add_crab(PlayerId(1), Vec2::new(2400.0, 20.0));
    run(&mut sim, 1.0);
    let t = &sim.tuning.memory;
    let other = &sim.crab(PlayerId(1)).unwrap().memory;
    let here = other.cell_at(Vec2::new(400.0, 20.0), t).unwrap();
    assert_eq!(other.clarity(here, t), UNSEEN);
}

#[test]
fn memory_survives_a_save() {
    let mut sim = sim_at(400.0);
    sim.spawn_enemy(Kind::Fixture, Vec2::new(550.0, 0.0));
    run(&mut sim, 1.0);
    put(&mut sim, 1600.0);
    run(&mut sim, 40.0);
    let m = memory(&sim);
    let text = serde_json::to_string(&m.to_save()).unwrap();
    let back = Memory::from_save(
        &serde_json::from_str(&text).unwrap(),
        &sim.level,
        &sim.tuning.memory,
    )
    .unwrap();
    assert_eq!(&back, m);
}

#[test]
fn the_same_inputs_give_the_same_memory() {
    let go = || {
        let mut sim = sim_at(400.0);
        sim.populate();
        run(&mut sim, 3.0);
        put(&mut sim, 1600.0);
        run(&mut sim, 3.0);
        memory(&sim).fingerprint()
    };
    assert_eq!(go(), go());
}
