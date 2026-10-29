//! Promises about rest stops: crevices under rock out in the level where a
//! crab tucks in the way it does at home. Resting in one mends the shell and
//! fills the reserve (nothing else: no workbench, no larder), and the last
//! one it rested in is where it wakes after dying. Home still counts. Each
//! crab keeps its own, and it is saved.

use bevy_math::Vec2;
use crabsim::{Carry, Controls, Hit, PlayerId, Progress, Shelter, Sim};

const P0: PlayerId = PlayerId(0);

fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.tick();
    }
}

/// Hold `walk` for up to `n` ticks, stopping once tucked in (walking in) or
/// out in the open (walking out). Returns the events seen, by name.
fn walk(sim: &mut Sim, id: PlayerId, walk: f32, n: u32) -> Vec<&'static str> {
    let mut seen = Vec::new();
    for _ in 0..n {
        sim.set_controls(
            id,
            Controls {
                walk,
                ..Controls::default()
            },
        );
        sim.tick();
        let crab = sim.crab(id).unwrap();
        seen.extend(crab.events.names());
        let done = if walk < 0.0 {
            crab.mover.tucked_in()
        } else {
            crab.mover.shelter == Shelter::Out && seen.contains(&"came out")
        };
        if done {
            break;
        }
    }
    sim.set_controls(id, Controls::default());
    seen
}

/// A crab standing in crevice `i`'s doorway, just outside the den.
fn at_crevice(i: usize) -> Sim {
    let sim = Sim::with_one_crab(Vec2::ZERO);
    let c = sim.level.crevices[i];
    Sim::with_one_crab(Vec2::new(c.door.max.x - 5.0, c.den.y))
}

/// Walk into crevice `i`'s den from its doorway, and settle.
fn rest_in(sim: &mut Sim, id: PlayerId) -> Vec<&'static str> {
    let seen = walk(sim, id, -1.0, 600);
    assert!(sim.crab(id).unwrap().mover.tucked_in(), "tucked in: {seen:?}");
    seen
}

fn blow(damage: f32) -> Hit {
    Hit {
        damage,
        ..Hit::default()
    }
}

/// Hit the crab until it dies, then wait until it wakes.
fn die(sim: &mut Sim) {
    for _ in 0..4 {
        sim.hit_crab(P0, &blow(60.0));
        sim.tick();
    }
    assert!(sim.crab(P0).unwrap().fighter.dead.is_some(), "dead");
    let respawn = sim.tuning.respawn_ticks;
    let mut woke = false;
    for _ in 0..respawn + 10 {
        sim.tick();
        woke |= sim.crab(P0).unwrap().events.respawned;
    }
    assert!(woke, "woke again");
}

#[test]
fn the_level_has_crevices_out_past_the_trench_on_solid_floor() {
    let sim = Sim::with_one_crab(Vec2::ZERO);
    let level = &sim.level;
    let crevices = &level.crevices;
    assert!((2..=3).contains(&crevices.len()), "{crevices:?}");
    let (_, trench_right) = level.trench();
    let (pit_left, pit_right, pit_bottom) = level.pit();
    assert!(
        crevices
            .iter()
            .any(|c| c.den.x > pit_left && c.den.x < pit_right && c.den.y == pit_bottom),
        "one at the bottom of the pit"
    );
    for c in crevices {
        // Out in the level, on the floor, in its own doorway.
        assert!(c.den.x > trench_right, "{c:?} past the trench");
        assert_eq!(level.floor_height(c.den.x), c.den.y, "{c:?} on the floor");
        let doorstep = Vec2::new(c.door.max.x, c.den.y);
        assert_eq!(level.floor_height(doorstep.x), c.den.y, "{c:?} flat");
        assert!(c.door.contains(c.den) && c.door.contains(doorstep));
        // Never inside a rock, and never in an animal's burrow.
        for p in [c.den, doorstep] {
            let body = p + Vec2::new(0.0, sim.tuning.body_height / 2.0);
            assert!(!level.rocks.iter().any(|r| r.contains(body)), "{c:?}");
        }
        for (kind, at) in &level.foes {
            let den = Vec2::new(at.x, level.floor_height(at.x));
            assert!(den.distance(c.den) > 400.0, "{c:?} by a {kind:?}");
        }
        assert!(c.door.intersect(level.home).is_empty(), "not home");
    }
}

#[test]
fn resting_in_a_crevice_makes_it_where_the_crab_wakes() {
    let mut sim = at_crevice(0);
    assert_eq!(sim.crab(P0).unwrap().wakes_at, None, "home, to begin with");
    let seen = rest_in(&mut sim, P0);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.pos, sim.level.crevices[0].den, "at the back");
    assert_eq!(crab.mover.shelter, Shelter::In);
    assert_eq!(crab.wakes_at, Some(0));
    assert_eq!(
        seen.iter().filter(|e| **e == "wakes here").count(),
        1,
        "{seen:?}"
    );
    // Resting on there says nothing new.
    let mut again = 0;
    for _ in 0..300 {
        sim.tick();
        again += u32::from(sim.crab(P0).unwrap().events.wakes_here);
    }
    assert_eq!(again, 0);
    // Out again, and back in: it already wakes here.
    walk(&mut sim, P0, 1.0, 120);
    let seen = rest_in(&mut sim, P0);
    assert!(!seen.contains(&"wakes here"), "{seen:?}");
}

#[test]
fn dying_wakes_the_crab_in_the_last_crevice_it_rested_in() {
    let mut sim = at_crevice(0);
    rest_in(&mut sim, P0);
    // Then the next one along: that's the one that counts now.
    let next = sim.level.crevices[1];
    sim.crab_mut(P0).unwrap().mover =
        crabsim::Mover::new(Vec2::new(next.door.max.x - 5.0, next.den.y));
    rest_in(&mut sim, P0);
    assert_eq!(sim.crab(P0).unwrap().wakes_at, Some(1));
    // Out, a clam on its back, and killed a little way off.
    walk(&mut sim, P0, 1.0, 120);
    sim.crab_mut(P0).unwrap().mover.carrying = Some(Carry::Clam);
    die(&mut sim);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.pos, next.den, "woke in the crevice");
    assert_eq!(crab.mover.shelter, Shelter::In);
    assert!(crab.mover.tucked_in());
    assert_eq!(crab.fighter.vitals.health, sim.tuning.crab.health);
    assert_eq!(crab.cast_offs.len(), 1, "the clam lies where it fell");
    assert_eq!(crab.wakes_at, Some(1), "and still wakes there");
}

#[test]
fn resting_at_home_makes_home_where_it_wakes_again() {
    let mut sim = at_crevice(0);
    rest_in(&mut sim, P0);
    // Back home (put by the doorway), tucked in there.
    sim.crab_mut(P0).unwrap().mover = crabsim::Mover::new(Vec2::new(180.0, 0.0));
    rest_in(&mut sim, P0);
    assert_eq!(sim.crab(P0).unwrap().wakes_at, None);
    walk(&mut sim, P0, 1.0, 120);
    die(&mut sim);
    assert_eq!(sim.crab(P0).unwrap().mover.pos, sim.level.home_den);
}

#[test]
fn resting_in_a_crevice_mends_and_refills_but_it_is_not_home() {
    let mut sim = at_crevice(0);
    let t = sim.tuning.clone();
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.fighter.vitals.health = 30.0;
        crab.fighter.vitals.unhit = 0;
        crab.mover.upgrades = 2;
        crab.mover.stamina = 0.0;
        crab.mover.reserve = 0.1;
        crab.mover.carrying = Some(Carry::Culture);
        crab.larder = 2;
    }
    let mut seen = rest_in(&mut sim, P0);
    for _ in 0..t.home_heal_after_ticks + t.food.eat_after_ticks + 60 {
        sim.tick();
        seen.extend(sim.crab(P0).unwrap().events.names());
    }
    let crab = sim.crab(P0).unwrap();
    assert!(seen.contains(&"rested"), "{seen:?}");
    assert_eq!(crab.fighter.vitals.health, crab.mover.max_shell(&t), "mended");
    assert_eq!((crab.mover.reserve, crab.mover.stamina), (1.0, 2.0));
    // No workbench and no larder out here.
    assert_eq!(crab.mover.carrying, Some(Carry::Culture), "still on its back");
    assert_eq!(crab.mover.lantern, 0);
    assert_eq!(crab.larder, 2);
    assert_eq!(crab.mover.well_fed, 0, "nothing to eat here");
    for e in ["delivered", "stored", "ate a clam", "larder full"] {
        assert!(!seen.contains(&e), "{e}: {seen:?}");
    }
}

#[test]
fn each_crab_has_its_own_wake_point() {
    let mut sim = at_crevice(0);
    let c = sim.level.crevices[0];
    let other = PlayerId(1);
    sim.add_crab(other, Vec2::new(c.door.max.x + 300.0, c.den.y));
    rest_in(&mut sim, P0);
    assert_eq!(sim.crab(P0).unwrap().wakes_at, Some(0));
    assert_eq!(sim.crab(other).unwrap().wakes_at, None);
}

#[test]
fn the_wake_point_is_saved_and_comes_back() {
    let mut sim = at_crevice(1);
    rest_in(&mut sim, P0);
    let saved = sim.progress(P0).unwrap();
    assert_eq!(saved.wakes_at, Some(1));
    let text = serde_json::to_string(&saved).unwrap();
    let saved: Progress = serde_json::from_str(&text).unwrap();

    let mut fresh = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    fresh.restore_progress(P0, &saved);
    assert_eq!(fresh.crab(P0).unwrap().wakes_at, Some(1));
    assert_eq!(fresh.progress(P0).unwrap(), saved);
    die(&mut fresh);
    assert_eq!(
        fresh.crab(P0).unwrap().mover.pos,
        fresh.level.crevices[1].den
    );
}

#[test]
fn a_save_from_before_rest_stops_wakes_at_home() {
    let old = r#"{"upgrades":1,"lantern":0,"scissor_claw":false,"climb_xp":0.0,"carrying":null,
        "eaten":[true,false,false,false],"cultures_taken":[false],"clues_found":[false],
        "teeth_taken":[false],"nets_cut":[false],"cast_offs":[],"larder":1,"clams":[0,0,0,0,0],
        "well_fed":0}"#;
    let saved: Progress = serde_json::from_str(old).expect("an old save still reads");
    assert_eq!(saved.wakes_at, None);
    let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    sim.restore_progress(P0, &saved);
    let crab = sim.crab(P0).unwrap();
    assert_eq!((crab.wakes_at, crab.mover.upgrades), (None, 1));
    die(&mut sim);
    assert_eq!(sim.crab(P0).unwrap().mover.pos, sim.level.home_den);
}

#[test]
fn a_saved_crevice_the_level_no_longer_has_is_home() {
    let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    let mut saved = sim.progress(P0).unwrap();
    saved.wakes_at = Some(99);
    sim.restore_progress(P0, &saved);
    assert_eq!(sim.crab(P0).unwrap().wakes_at, None);
}

#[test]
fn a_replay_that_rested_in_a_crevice_plays_the_same() {
    let mut live = at_crevice(0);
    let c = live.level.crevices[0];
    live.start_recording();
    walk(&mut live, P0, -1.0, 600);
    idle(&mut live, 120);
    walk(&mut live, P0, 1.0, 120);
    // A mantis turns up and beats the crab: it wakes in the crevice.
    live.spawn_enemy(crabsim::Kind::Mantis, Vec2::new(c.door.max.x + 160.0, c.den.y));
    let mut woke = false;
    for _ in 0..120 * 40 {
        live.tick();
        woke |= live.crab(P0).unwrap().events.respawned;
        if woke {
            break;
        }
    }
    assert!(woke, "the mantis won");
    assert_eq!(live.crab(P0).unwrap().mover.pos, c.den);
    idle(&mut live, 30);

    let text = live.recording().unwrap().to_text();
    let replay = crabsim::Replay::parse(&text).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(again.crab(P0).unwrap().wakes_at, Some(0));
    assert_eq!(replay.hash, Some(again.state_hash()), "{text}");
}

#[test]
fn a_replay_recorded_after_a_reload_starts_with_the_saved_wake_point() {
    let mut live = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    let mut saved = live.progress(P0).unwrap();
    saved.wakes_at = Some(1);
    live.restore_progress(P0, &saved);
    live.start_recording();
    for _ in 0..4 {
        live.hit_crab(P0, &blow(60.0));
        live.tick();
    }
    idle(&mut live, 240);
    // Hits from outside aren't in a replay: only the start matters here.
    let text = live.recording().unwrap().to_text();
    let replay = crabsim::Replay::parse(&text).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, 1, |_| {});
    assert_eq!(again.crab(P0).unwrap().wakes_at, Some(1));
}
