//! Promises about food (the loop's "Prepare" step): clams carried home go
//! into the larder, a crab resting at home eats one and is well fed for a
//! while (a bigger shell and reserve), the clam beds grow back, and all of
//! it is saved.

use bevy_math::Vec2;
use crabsim::{Carry, Controls, Hit, PlayerId, Progress, Shelter, Sim};

const P0: PlayerId = PlayerId(0);

/// Stand still for `n` ticks.
fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.tick();
    }
}

/// Walk left (home is at the left end of the shallows) until tucked in.
/// Returns the events seen on the way, by name.
fn walk_home(sim: &mut Sim) -> Vec<&'static str> {
    let mut seen = Vec::new();
    for _ in 0..1200 {
        sim.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        sim.tick();
        seen.extend(sim.crab(P0).unwrap().events.names());
        if sim.crab(P0).unwrap().mover.tucked_in() {
            break;
        }
    }
    sim.set_controls(P0, Controls::default());
    assert!(sim.crab(P0).unwrap().mover.tucked_in(), "got home");
    seen
}

/// A crab standing on the floor at the first clam bed.
fn at_first_clam() -> Sim {
    let sim = Sim::with_one_crab(Vec2::ZERO);
    let bed = sim.level.clams[0];
    let floor = sim.level.floor_height(bed.x);
    Sim::with_one_crab(Vec2::new(bed.x, floor))
}

/// A crab out on the floor a little way from home, with nothing near.
fn by_home() -> Sim {
    Sim::with_one_crab(Vec2::new(180.0, 0.0))
}

fn blow(damage: f32) -> Hit {
    Hit {
        damage,
        ..Hit::default()
    }
}

#[test]
fn the_level_has_clams_near_home_and_in_harder_places() {
    let sim = Sim::with_one_crab(Vec2::ZERO);
    let clams = &sim.level.clams;
    let (pit_left, pit_right, pit_bottom) = sim.level.pit();
    let (_, trench_right) = sim.level.trench();
    let near_home = clams.iter().filter(|c| c.x < 600.0).count();
    let in_pit = clams
        .iter()
        .any(|c| c.x > pit_left && c.x < pit_right && c.y < pit_bottom + 50.0);
    let past_trench = clams.iter().any(|c| c.x > trench_right && c.x < 2800.0);
    assert!(near_home >= 2, "{clams:?}");
    assert!(in_pit && past_trench, "{clams:?}");
    // Each lies half-buried on the floor.
    for c in clams {
        let floor = sim.level.floor_height(c.x);
        assert!((c.y - floor).abs() < 10.0, "{c:?} on the floor at {floor}");
    }
    assert_eq!(sim.crab(P0).unwrap().clams, vec![0; clams.len()]);
}

#[test]
fn a_clam_picked_up_goes_home_into_the_larder() {
    let mut sim = at_first_clam();
    idle(&mut sim, 1);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.carrying, Some(Carry::Clam));
    assert!(crab.events.picked_up);
    assert!(crab.clams[0] > 0, "the bed is bare for a while");
    assert_eq!(crab.larder, 0);

    let seen = walk_home(&mut sim);
    let crab = sim.crab(P0).unwrap();
    assert!(seen.contains(&"stored"), "{seen:?}");
    assert_eq!(crab.mover.carrying, None);
    assert_eq!(crab.larder, 1);
}

#[test]
fn a_full_larder_leaves_the_clam_on_its_back() {
    let mut sim = at_first_clam();
    idle(&mut sim, 1);
    let full = sim.tuning.food.larder;
    sim.crab_mut(P0).unwrap().larder = full;
    // Already well fed, so it doesn't eat one to make room.
    sim.crab_mut(P0).unwrap().mover.well_fed = 100_000;
    let seen = walk_home(&mut sim);
    let crab = sim.crab(P0).unwrap();
    assert!(seen.contains(&"larder full"), "{seen:?}");
    assert_eq!(crab.larder, full);
    assert_eq!(crab.mover.carrying, Some(Carry::Clam));
}

#[test]
fn resting_at_home_it_eats_one_and_is_well_fed() {
    let mut sim = by_home();
    let t = sim.tuning.clone();
    let base_shell = t.crab.health;
    let base_reserve = t.stamina_reserve as f32;
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.larder = 2;
        assert_eq!(crab.mover.max_shell(&t), base_shell);
        assert_eq!(crab.mover.reserve_strokes(&t), base_reserve);
    }
    let seen = walk_home(&mut sim);
    assert!(!seen.contains(&"ate a clam"), "not the moment it arrives");
    let eat_after = sim.tuning.food.eat_after_ticks;
    let mut ate = 0;
    for _ in 0..eat_after + 10 {
        sim.tick();
        ate += u32::from(sim.crab(P0).unwrap().events.ate_clam);
    }
    assert_eq!(ate, 1);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.larder, 1);
    assert!(crab.mover.well_fed > 0);
    // A bigger shell, filled; a bigger reserve, filled.
    let shell = crab.mover.max_shell(&t);
    assert_eq!(shell, base_shell * (1.0 + t.food.shell_bonus));
    assert!(shell > base_shell);
    assert_eq!(crab.fighter.vitals.health, shell);
    assert_eq!(
        crab.mover.reserve_strokes(&t),
        base_reserve + t.food.reserve_bonus as f32
    );
    assert_eq!(crab.mover.reserve, 1.0);

    // Already well fed, it doesn't eat another.
    idle(&mut sim, 600);
    assert_eq!(sim.crab(P0).unwrap().larder, 1);
}

#[test]
fn well_fed_wears_off_after_its_time() {
    let mut sim = by_home();
    sim.crab_mut(P0).unwrap().larder = 1;
    walk_home(&mut sim);
    let eat_after = sim.tuning.food.eat_after_ticks;
    idle(&mut sim, eat_after + 1);
    let fed_for = sim.crab(P0).unwrap().mover.well_fed;
    assert!(fed_for > 0 && fed_for <= sim.tuning.food.well_fed_ticks);
    // Out of the shell and standing about, a scratch on it.
    sim.set_controls(
        P0,
        Controls {
            walk: 1.0,
            ..Controls::default()
        },
    );
    idle(&mut sim, 120);
    sim.set_controls(P0, Controls::default());
    sim.hit_crab(P0, &blow(5.0));
    let left = sim.crab(P0).unwrap().mover.well_fed;
    idle(&mut sim, left - 2);
    let crab = sim.crab(P0).unwrap();
    assert!(crab.mover.well_fed > 0, "still fed just before the end");
    assert!(crab.fighter.vitals.health > sim.tuning.crab.health);
    let mut hungry = false;
    for _ in 0..4 {
        sim.tick();
        hungry |= sim.crab(P0).unwrap().events.hungry;
    }
    let t = sim.tuning.clone();
    let crab = sim.crab(P0).unwrap();
    assert!(hungry, "it says so");
    assert_eq!(crab.mover.well_fed, 0);
    assert_eq!(crab.mover.max_shell(&t), t.crab.health);
    assert_eq!(
        crab.fighter.vitals.health, t.crab.health,
        "back to a plain shell"
    );
    assert_eq!(crab.mover.reserve_strokes(&t), t.stamina_reserve as f32);
    assert!(crab.mover.reserve <= 1.0);
}

#[test]
fn a_taken_clam_bed_grows_back_after_its_time() {
    let mut sim = at_first_clam();
    idle(&mut sim, 1);
    let regrow = sim.tuning.food.regrow_ticks;
    // Standing on the bare bed with the clam still on its back.
    idle(&mut sim, regrow - 2);
    assert!(sim.crab(P0).unwrap().clams[0] > 0, "not yet");
    idle(&mut sim, 2);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.clams[0], 0, "grown back");
    assert_eq!(crab.mover.carrying, Some(Carry::Clam), "its back is full");
    // A free back takes the new one.
    sim.crab_mut(P0).unwrap().mover.carrying = None;
    idle(&mut sim, 1);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.carrying, Some(Carry::Clam));
    assert!(crab.clams[0] > 0);
}

#[test]
fn dying_with_a_clam_leaves_it_in_the_cast_off_shell_and_keeps_the_larder() {
    let mut sim = at_first_clam();
    idle(&mut sim, 1);
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.larder = 3;
        crab.mover.well_fed = 10_000;
    }
    for _ in 0..4 {
        sim.hit_crab(P0, &blow(60.0));
        sim.tick();
    }
    assert!(sim.crab(P0).unwrap().fighter.dead.is_some());
    let respawn = sim.tuning.respawn_ticks;
    idle(&mut sim, respawn + 2);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.shelter, Shelter::In, "woke at home");
    assert_eq!(crab.mover.carrying, None);
    assert_eq!(crab.cast_offs.len(), 1);
    assert_eq!(crab.cast_offs[0].carrying, Carry::Clam);
    assert_eq!(crab.larder, 3, "the larder is at home");
    assert_eq!(crab.mover.well_fed, 0, "it wakes hungry");
    assert_eq!(crab.fighter.vitals.health, sim.tuning.crab.health);
}

#[test]
fn food_comes_back_after_a_reload() {
    let mut sim = by_home();
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.larder = 4;
        crab.clams[1] = 1234;
        crab.mover.well_fed = 5678;
    }
    let saved = sim.progress(P0).unwrap();
    assert_eq!(saved.larder, 4);
    assert_eq!(saved.well_fed, 5678);
    let text = serde_json::to_string(&saved).unwrap();
    let saved: Progress = serde_json::from_str(&text).unwrap();

    let mut fresh = by_home();
    fresh.restore_progress(P0, &saved);
    assert_eq!(fresh.progress(P0).unwrap(), saved);
    let t = fresh.tuning.clone();
    let crab = fresh.crab(P0).unwrap();
    assert_eq!(crab.clams[1], 1234);
    assert_eq!(
        crab.fighter.vitals.health,
        crab.mover.max_shell(&t),
        "rested, with the bigger shell"
    );
    assert!(crab.fighter.vitals.health > t.crab.health);
}

#[test]
fn a_save_from_before_food_still_loads() {
    let old = r#"{"upgrades":1,"lantern":1,"scissor_claw":false,"climb_xp":0.0,"carrying":null,
        "eaten":[true,false,false,false],"cultures_taken":[false],"clues_found":[false],
        "teeth_taken":[false],"nets_cut":[false],"cast_offs":[]}"#;
    let saved: Progress = serde_json::from_str(old).expect("an old save still reads");
    assert_eq!((saved.larder, saved.well_fed), (0, 0));

    let mut sim = by_home();
    sim.restore_progress(P0, &saved);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.upgrades, 1);
    assert!(crab.eaten[0], "the rest of the save still counts");
    assert_eq!(crab.larder, 0);
    assert!(crab.clams.iter().all(|&c| c == 0), "every clam is there");
    assert_eq!(crab.clams.len(), sim.level.clams.len());
}
