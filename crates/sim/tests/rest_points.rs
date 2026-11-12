//! Promises about rest points (`docs/design/world.md`, Death): a crab wakes at
//! the reached rest point nearest where it fell (home, a crevice it rested in,
//! an entrance it passed); a death in a boss's arena wakes it at that door
//! with its load kept; overworld deaths leave cast-offs, several at once.

use bevy_math::Vec2;
use crabsim::level::rest::{RestKind, RestPoint};
use crabsim::{Carry, Controls, Hit, PlayerId, Progress, SaveData, Shelter, Sim};

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn point(sim: &Sim, name: &str) -> RestPoint {
    sim.rest_points()
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no rest point {name}"))
        .clone()
}

fn blow(damage: f32) -> Hit {
    Hit {
        damage,
        ..Hit::default()
    }
}

/// Put crab `id` standing at `at` (feet), still, with nothing carried.
fn put(sim: &mut Sim, id: PlayerId, at: Vec2) {
    let crab = sim.crab_mut(id).unwrap();
    crab.mover = crabsim::Mover::new(at);
    crab.prev_pos = at;
}

/// Kill the listed crabs and run until they have all woken.
fn die(sim: &mut Sim, ids: &[PlayerId]) {
    for _ in 0..4 {
        for id in ids {
            sim.hit_crab(*id, &blow(60.0));
        }
        sim.tick();
    }
    for id in ids {
        assert!(sim.crab(*id).unwrap().fighter.dead.is_some(), "{id:?} dead");
    }
    for _ in 0..sim.tuning.respawn_ticks + 10 {
        sim.tick();
    }
    for id in ids {
        assert!(sim.crab(*id).unwrap().fighter.dead.is_none(), "{id:?} woke");
    }
}

/// A solo sim with the crab at `at`.
fn sim_at(at: Vec2) -> Sim {
    Sim::with_one_crab(at)
}

#[test]
fn markers_snap_to_the_ground_and_every_rest_point_is_clear_of_rock() {
    let sim = sim_at(Vec2::ZERO);
    let level = &sim.level;
    let points = level.rest_points();
    assert_eq!(points[0].kind, RestKind::Home, "home first");
    for i in 0..level.crevices.len() {
        assert_eq!(
            points[1 + i].kind,
            RestKind::Crevice(i),
            "then each crevice"
        );
    }
    let rest = &points[1 + level.crevices.len()..];
    assert!(!rest.is_empty());
    for m in crabsim::level::rest::MARKERS {
        let p = point(&sim, m.name);
        assert_eq!(
            p.at.y,
            level
                .ground_below(Vec2::from(m.at))
                .unwrap_or_else(|| level.floor_height(m.at[0])),
            "{} on the ground",
            m.name
        );
    }
    let doors: Vec<_> = points
        .iter()
        .filter(|p| matches!(p.kind, RestKind::BossDoor { .. }))
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(
        doors,
        ["octopus den", "goliath grouper"],
        "exactly two boss doors"
    );
    let mut seen = Vec::new();
    for p in &points {
        assert!(!seen.contains(&p.name), "{} is named twice", p.name);
        seen.push(p.name.clone());
        assert!(
            p.at.x > level.min_x && p.at.x < level.max_x,
            "{} is in the level",
            p.name
        );
        let body = p.at + Vec2::new(0.0, sim.tuning.body_height / 2.0);
        assert!(
            !level.rocks.iter().any(|r| r.contains(body)),
            "{} stands in rock",
            p.name
        );
        if let RestKind::BossDoor { arena } = p.kind {
            assert!(arena.contains(p.at), "{} stands in its own arena", p.name);
        }
    }
}

#[test]
fn dying_near_a_reached_entrance_wakes_the_crab_there_not_at_home() {
    let entrance = point(&sim_at(Vec2::ZERO), "far basin");
    let mut sim = sim_at(entrance.at);
    sim.tick();
    let crab = sim.crab(P0).unwrap();
    assert!(crab.reached.contains(&"far basin".to_string()));
    assert!(crab.events.wakes_here, "told the first time");
    // A way off, carrying a find on its back: it falls there.
    let fell = Vec2::new(
        entrance.at.x + 500.0,
        sim.level.floor_height(entrance.at.x + 500.0),
    );
    put(&mut sim, P0, fell);
    sim.crab_mut(P0).unwrap().mover.carrying = Some(Carry::Culture);
    die(&mut sim, &[P0]);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.pos, entrance.at, "woke at the entrance");
    assert_eq!(crab.mover.shelter, Shelter::Out, "standing, not tucked in");
    assert_eq!(crab.fighter.vitals.health, sim.tuning.crab.health);
    assert_eq!(
        crab.cast_offs.len(),
        1,
        "overworld death: it left a cast-off"
    );
    assert!(
        crab.cast_offs[0].pos.distance(fell) < 100.0,
        "where it fell"
    );
}

#[test]
fn an_entrance_it_never_reached_is_ignored() {
    let entrance = point(&sim_at(Vec2::ZERO), "far basin");
    // Close to it, but never within reach.
    let x = entrance.at.x + 300.0;
    let mut sim = sim_at(Vec2::new(x, sim_at(Vec2::ZERO).level.floor_height(x)));
    sim.tick();
    assert!(sim.crab(P0).unwrap().reached.is_empty());
    die(&mut sim, &[P0]);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.pos, sim.level.home_den, "woke at home");
    assert!(crab.mover.tucked_in());
}

fn boss_arena_death(name: &str) {
    let probe = sim_at(Vec2::ZERO);
    let door = point(&probe, name);
    let RestKind::BossDoor { arena } = door.kind else {
        panic!("{name} is a boss door");
    };
    // Fell inside the arena, well away from the door, carrying a find on its
    // back and a clam on its shell, the vent meal among them.
    let fell = Vec2::new(
        arena.max.x - 100.0,
        probe
            .level
            .floor_height(arena.max.x - 100.0)
            .max(arena.min.y + 50.0),
    );
    assert!(arena.contains(fell) && fell.distance(door.at) > 300.0);
    let mut sim = sim_at(fell);
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.mover.carrying = Some(Carry::Culture);
        crab.mover.clinging = vec![Carry::Clam];
        crab.deep_food = crabsim::world::DeepFood::Carried(0);
    }
    assert!(!sim.crab(P0).unwrap().reached.iter().any(|r| r == name));
    die(&mut sim, &[P0]);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(
        crab.mover.pos, door.at,
        "{name}: woke at its door, unreached or not"
    );
    assert_eq!(crab.mover.shelter, Shelter::Out);
    assert!(crab.cast_offs.is_empty(), "{name}: no cast-off");
    assert_eq!(
        crab.mover.carrying,
        Some(Carry::Culture),
        "{name}: back cargo kept"
    );
    assert_eq!(
        crab.mover.clinging,
        vec![Carry::Clam],
        "{name}: shell load kept"
    );
    assert_eq!(
        crab.deep_food,
        crabsim::world::DeepFood::Carried(0),
        "{name}: the vent meal is still on its shell"
    );
    assert_eq!(crab.fighter.vitals.health, sim.tuning.crab.health);
    assert!(
        crab.reached.iter().any(|r| r == name),
        "{name}: now reached"
    );
}

#[test]
fn a_death_in_the_octopus_arena_wakes_at_its_door_with_the_load_kept() {
    boss_arena_death("octopus den");
}

#[test]
fn a_death_in_the_grouper_arena_wakes_at_its_door_with_the_load_kept() {
    boss_arena_death("goliath grouper");
}

#[test]
fn an_overworld_death_still_drops_the_vent_meal_into_its_cast_off() {
    let mut sim = sim_at(Vec2::new(900.0, 0.0));
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.mover.clinging = vec![Carry::Clam];
        crab.deep_food = crabsim::world::DeepFood::Carried(0);
    }
    die(&mut sim, &[P0]);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.deep_food, crabsim::world::DeepFood::Dropped(0));
    assert_eq!(crab.cast_offs.len(), 1);
    assert!(crab.mover.clinging.is_empty());
}

#[test]
fn three_overworld_deaths_leave_three_recoverable_cast_offs_waiting_at_once() {
    let mut sim = sim_at(Vec2::ZERO);
    let spots = [900.0, 1100.0, 1300.0].map(|x| Vec2::new(x, sim.level.floor_height(x)));
    for at in spots {
        put(&mut sim, P0, at);
        sim.crab_mut(P0).unwrap().mover.carrying = Some(Carry::Culture);
        die(&mut sim, &[P0]);
        assert_eq!(sim.crab(P0).unwrap().mover.pos, sim.level.home_den);
    }
    let offs = &sim.crab(P0).unwrap().cast_offs;
    assert_eq!(offs.len(), 3, "all three wait");
    assert!(
        offs.iter()
            .all(|o| o.recoverable && o.carrying == Carry::Culture)
    );
    for (off, at) in offs.iter().zip(spots) {
        assert_eq!(off.pos, at);
    }
    // And each can still be collected.
    for (n, at) in spots.into_iter().enumerate() {
        put(&mut sim, P0, at);
        sim.tick();
        assert_eq!(
            sim.crab(P0).unwrap().cast_offs.len(),
            2 - n,
            "took one back"
        );
        sim.crab_mut(P0).unwrap().mover.carrying = None;
    }
}

#[test]
fn reached_points_round_trip_through_progress_and_old_saves_load() {
    let entrance = point(&sim_at(Vec2::ZERO), "far basin");
    let mut sim = sim_at(entrance.at);
    sim.tick();
    // Rest in crevice 1 as well.
    let c = sim.level.crevices[1];
    put(&mut sim, P0, Vec2::new(c.door.max.x - 5.0, c.den.y));
    for _ in 0..600 {
        sim.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        sim.tick();
        if sim.crab(P0).unwrap().mover.tucked_in() {
            break;
        }
    }
    sim.set_controls(P0, Controls::default());
    sim.tick();
    let saved = sim.progress(P0).unwrap();
    assert_eq!(saved.reached, ["far basin", "crevice 1"]);
    let text = serde_json::to_string(&saved).unwrap();
    let saved: Progress = serde_json::from_str(&text).unwrap();
    let mut fresh = sim_at(Vec2::new(400.0, 0.0));
    fresh.restore_progress(P0, &saved);
    assert_eq!(fresh.crab(P0).unwrap().reached, saved.reached);
    assert_eq!(fresh.progress(P0).unwrap(), saved);
    // The record checks out against the level, and a made-up point does not.
    let record = SaveData {
        version: crabsim::save_data::SAVE_VERSION,
        x: 0.0,
        y: 0.0,
        memory: None,
        progress: Some(saved.clone()),
    };
    assert!(crabsim::save_data::player_fits_world(&fresh, &record));
    let mut bad = record.clone();
    bad.progress
        .as_mut()
        .unwrap()
        .reached
        .push("nowhere".into());
    assert!(!crabsim::save_data::player_fits_world(&fresh, &bad));
    bad.progress.as_mut().unwrap().reached = vec!["home".into()];
    assert!(
        !crabsim::save_data::player_fits_world(&fresh, &bad),
        "home is never stored"
    );
    // The old field: a crab last rested in crevice 2 reaches it, and wakes there.
    let mut old = serde_json::to_value(Progress::default()).unwrap();
    old.as_object_mut().unwrap().remove("reached");
    old["wakes_at"] = serde_json::json!(2);
    let old: Progress = serde_json::from_value(old).unwrap();
    assert!(old.reached.is_empty());
    let mut sim = sim_at(Vec2::new(400.0, 0.0));
    sim.restore_progress(P0, &old);
    assert_eq!(sim.crab(P0).unwrap().reached, ["crevice 2"]);
    let den = sim.level.crevices[2].den;
    put(&mut sim, P0, Vec2::new(den.x + 200.0, den.y));
    die(&mut sim, &[P0]);
    assert_eq!(sim.crab(P0).unwrap().mover.pos, den);
    // Out of range data is dropped, like a crevice the level lacks.
    let odd = Progress {
        reached: vec!["nowhere".into(), "far basin".into(), "far basin".into()],
        ..Progress::default()
    };
    sim.restore_progress(P0, &odd);
    assert_eq!(sim.crab(P0).unwrap().reached, ["far basin"]);
}

#[test]
fn two_crabs_reach_points_independently() {
    let entrance = point(&sim_at(Vec2::ZERO), "far basin");
    let mut sim = sim_at(entrance.at);
    let ground = |sim: &Sim, x: f32| Vec2::new(x, sim.level.floor_height(x));
    let far = ground(&sim, entrance.at.x + 500.0);
    sim.add_crab(P1, far);
    sim.tick();
    assert_eq!(sim.crab(P0).unwrap().reached, ["far basin"]);
    assert!(sim.crab(P1).unwrap().reached.is_empty());
    // Both fall at the same spot: one wakes at the entrance, the other home.
    put(&mut sim, P0, far);
    put(&mut sim, P1, far);
    die(&mut sim, &[P0, P1]);
    assert_eq!(sim.crab(P0).unwrap().mover.pos, entrance.at);
    assert_eq!(sim.crab(P1).unwrap().mover.pos, sim.level.home_den);
}
