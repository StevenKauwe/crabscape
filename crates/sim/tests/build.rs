//! Promises about building (the loop's "Build" step): shell grit lies about
//! the level, more of it deeper; a crab carries it home clinging to its
//! shell (a few pieces at most, see `cling.rs`), and the workbench builds it
//! into the home shell. Enough
//! of it and the home grows a stage, each one felt: it mends the crab
//! sooner, then holds a bigger larder, then makes a meal there last longer.
//! Each crab grows its own home, and it is saved.

use bevy_math::Vec2;
use crabsim::{Carry, Controls, Hit, PlayerId, Progress, Shelter, Sim};

const P0: PlayerId = PlayerId(0);

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

/// A crab out on the floor a little way from home, with nothing near.
fn by_home() -> Sim {
    Sim::with_one_crab(Vec2::new(180.0, 0.0))
}

/// A crab by home with `built` grit already in its home and a piece of
/// grit on its shell.
fn bringing_grit(built: u32) -> Sim {
    let mut sim = by_home();
    let crab = sim.crab_mut(P0).unwrap();
    crab.home_grit = built;
    crab.mover.clinging = vec![Carry::Grit];
    sim
}

/// A crab standing on the floor at grit `i`.
fn at_grit(i: usize) -> Sim {
    let sim = Sim::with_one_crab(Vec2::ZERO);
    let g = sim.level.grit[i];
    let floor = sim.level.floor_height(g.x);
    Sim::with_one_crab(Vec2::new(g.x, floor))
}

fn blow(damage: f32) -> Hit {
    Hit {
        damage,
        ..Hit::default()
    }
}

/// Out of the home, hit, then back in: how many ticks until the shell is
/// whole again (None: not within `limit`).
fn ticks_to_mend(sim: &mut Sim, limit: u32) -> Option<u32> {
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.fighter.vitals.health = 40.0;
        crab.fighter.vitals.unhit = 0;
    }
    walk_home(sim);
    let whole = sim.crab(P0).unwrap().mover.max_shell(&sim.tuning);
    (0..limit).find(|_| {
        sim.tick();
        sim.crab(P0).unwrap().fighter.vitals.health >= whole
    })
}

#[test]
fn the_level_has_shell_grit_and_more_of_it_deeper() {
    let sim = Sim::with_one_crab(Vec2::ZERO);
    let grit = &sim.level.grit;
    let (pit_left, pit_right, pit_bottom) = sim.level.pit();
    let shallows = grit.iter().filter(|g| g.y > -100.0).count();
    let deep = grit.iter().filter(|g| g.y < -600.0).count();
    let in_pit = grit
        .iter()
        .filter(|g| g.x > pit_left && g.x < pit_right && g.y < pit_bottom + 50.0)
        .count();
    assert!(shallows >= 1, "{grit:?}");
    assert!(deep > shallows, "more of it deeper: {grit:?}");
    assert!(in_pit >= 1, "{grit:?}");
    // Enough to finish the home, but only by going deep.
    let stages = sim.tuning.build.stages;
    let last = *stages.last().unwrap() as usize;
    assert!(grit.len() >= last, "{grit:?}");
    assert!(shallows < last, "not all of it near home");
    // Each lies on the floor, out of any rock.
    for g in grit {
        let floor = sim.level.floor_height(g.x);
        assert!((g.y - floor).abs() < 10.0, "{g:?} on the floor at {floor}");
        assert!(!sim.level.rocks.iter().any(|r| r.contains(*g)), "{g:?}");
    }
    assert_eq!(sim.crab(P0).unwrap().grit_taken, vec![false; grit.len()]);
}

#[test]
fn grit_picked_up_rides_home_and_is_built_in() {
    let mut sim = at_grit(0);
    idle(&mut sim, 1);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.clinging, vec![Carry::Grit]);
    assert_eq!(crab.events.picked_up, Some(Carry::Grit));
    assert!(crab.grit_taken[0]);
    assert_eq!(crab.home_stage(&sim.tuning), 0);

    // Carried home (put by the doorway, still carrying it).
    let mut m = crabsim::Mover::new(Vec2::new(180.0, 0.0));
    m.clinging = vec![Carry::Grit];
    sim.crab_mut(P0).unwrap().mover = m;
    let seen = walk_home(&mut sim);
    let crab = sim.crab(P0).unwrap();
    assert!(seen.contains(&"built"), "{seen:?}");
    assert!(seen.contains(&"home grew"), "the first piece: {seen:?}");
    assert!(crab.mover.clinging.is_empty());
    assert_eq!(crab.home_grit, 1);
    assert_eq!(crab.home_stage(&sim.tuning), 1);
}

#[test]
fn taken_grit_stays_taken_and_each_crab_finds_its_own() {
    let mut sim = at_grit(0);
    let g = sim.level.grit[0];
    let floor = sim.level.floor_height(g.x);
    sim.add_crab(PlayerId(1), Vec2::new(g.x + 400.0, floor));
    idle(&mut sim, 1);
    // Its shell free again, standing on the spot: nothing more to take.
    sim.crab_mut(P0).unwrap().mover.clinging.clear();
    idle(&mut sim, 600);
    assert!(sim.crab(P0).unwrap().mover.clinging.is_empty());
    // The other crab's piece is still there for it.
    assert!(!sim.crab(PlayerId(1)).unwrap().grit_taken[0]);
    sim.crab_mut(PlayerId(1)).unwrap().mover = crabsim::Mover::new(Vec2::new(g.x, floor));
    idle(&mut sim, 1);
    let other = sim.crab(PlayerId(1)).unwrap();
    assert_eq!(other.mover.clinging, vec![Carry::Grit]);
    assert_eq!(other.home_grit, 0, "its own home");
}

#[test]
fn each_stage_needs_more_grit() {
    let t = Sim::with_one_crab(Vec2::ZERO).tuning;
    let stages = t.build.stages;
    assert!(stages.windows(2).all(|w| w[0] < w[1]), "{stages:?}");
    for (stage, &needs) in stages.iter().enumerate() {
        // One short: the next piece grows it.
        let mut sim = bringing_grit(needs - 1);
        assert_eq!(sim.crab(P0).unwrap().home_stage(&t), stage as u32);
        let seen = walk_home(&mut sim);
        assert!(seen.contains(&"home grew"), "stage {}: {seen:?}", stage + 1);
        assert_eq!(sim.crab(P0).unwrap().home_stage(&t), stage as u32 + 1);
    }
    // A piece that doesn't reach the next stage is still built in.
    let mut sim = bringing_grit(stages[0]);
    let seen = walk_home(&mut sim);
    assert!(seen.contains(&"built") && !seen.contains(&"home grew"));
    assert_eq!(sim.crab(P0).unwrap().home_grit, stages[0] + 1);
}

#[test]
fn stage_one_mends_the_crab_sooner_at_home() {
    let t = Sim::with_one_crab(Vec2::ZERO).tuning;
    let plain = ticks_to_mend(&mut by_home(), 2000).expect("mends");
    let mut sim = by_home();
    sim.crab_mut(P0).unwrap().home_grit = t.build.stages[0];
    let built = ticks_to_mend(&mut sim, 2000).expect("mends");
    assert!(
        built + 100 < plain,
        "stage one: {built} ticks, a plain shell {plain}"
    );
}

#[test]
fn stage_two_holds_a_bigger_larder() {
    let t = Sim::with_one_crab(Vec2::ZERO).tuning;
    assert!(t.build.larder > t.food.larder);
    let bring_clam = |grit: u32| {
        let mut sim = by_home();
        let crab = sim.crab_mut(P0).unwrap();
        crab.home_grit = grit;
        crab.larder = t.food.larder;
        crab.mover.clinging = vec![Carry::Clam];
        crab.mover.well_fed = 100_000; // not hungry: it keeps the larder full
        let seen = walk_home(&mut sim);
        (seen, sim.crab(P0).unwrap().larder)
    };
    let (seen, larder) = bring_clam(t.build.stages[1] - 1);
    assert!(seen.contains(&"larder full"), "{seen:?}");
    assert_eq!(larder, t.food.larder);
    let (seen, larder) = bring_clam(t.build.stages[1]);
    assert!(seen.contains(&"stored"), "{seen:?}");
    assert_eq!(larder, t.food.larder + 1);
}

#[test]
fn stage_three_makes_a_meal_at_home_last_longer() {
    let t = Sim::with_one_crab(Vec2::ZERO).tuning;
    let fed_for = |grit: u32| {
        let mut sim = by_home();
        sim.crab_mut(P0).unwrap().home_grit = grit;
        sim.crab_mut(P0).unwrap().larder = 1;
        walk_home(&mut sim);
        let mut fed = 0;
        for _ in 0..t.food.eat_after_ticks + 10 {
            sim.tick();
            if sim.crab(P0).unwrap().events.ate_clam {
                fed = sim.crab(P0).unwrap().mover.well_fed;
            }
        }
        fed
    };
    let plain = fed_for(t.build.stages[2] - 1);
    let built = fed_for(t.build.stages[2]);
    assert_eq!(plain, t.food.well_fed_ticks);
    assert!(built > plain, "{built} > {plain}");
    assert_eq!(
        built,
        (t.food.well_fed_ticks as f32 * t.build.meal_longer).round() as u32
    );
}

#[test]
fn a_crevice_has_no_workbench_to_build_with() {
    let sim = Sim::with_one_crab(Vec2::ZERO);
    let c = sim.level.crevices[0];
    let mut sim = Sim::with_one_crab(Vec2::new(c.door.max.x - 5.0, c.den.y));
    sim.crab_mut(P0).unwrap().mover.clinging = vec![Carry::Grit];
    for _ in 0..600 {
        sim.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        sim.tick();
    }
    let crab = sim.crab(P0).unwrap();
    assert!(crab.mover.tucked_in());
    assert_eq!(crab.mover.clinging, vec![Carry::Grit]);
    assert_eq!(crab.home_grit, 0);
}

#[test]
fn dying_with_grit_leaves_it_in_the_cast_off_shell_and_keeps_the_home() {
    let mut sim = at_grit(0);
    idle(&mut sim, 1);
    sim.crab_mut(P0).unwrap().home_grit = 2;
    for _ in 0..4 {
        sim.hit_crab(P0, &blow(60.0));
        sim.tick();
    }
    assert!(sim.crab(P0).unwrap().fighter.dead.is_some());
    let respawn = sim.tuning.respawn_ticks;
    idle(&mut sim, respawn + 2);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.shelter, Shelter::In, "woke at home");
    assert!(crab.mover.clinging.is_empty(), "not built in on waking");
    assert_eq!(crab.home_grit, 2, "the home stays grown");
    assert_eq!(crab.cast_offs.len(), 1);
    assert_eq!(crab.cast_offs[0].carrying, Carry::Grit);
    assert!(crab.grit_taken[0], "it lies in the cast-off now");
}

#[test]
fn the_home_and_the_grit_taken_come_back_after_a_reload() {
    let t = Sim::with_one_crab(Vec2::ZERO).tuning;
    let mut sim = by_home();
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.home_grit = t.build.stages[1];
        crab.grit_taken[0] = true;
        crab.grit_taken[2] = true;
        // A larder bigger than a plain home holds.
        crab.larder = t.food.larder + 2;
        crab.mover.clinging = vec![Carry::Grit];
    }
    let saved = sim.progress(P0).unwrap();
    assert_eq!(saved.home_grit, t.build.stages[1]);
    let text = serde_json::to_string(&saved).unwrap();
    let saved: Progress = serde_json::from_str(&text).unwrap();

    let mut fresh = by_home();
    fresh.restore_progress(P0, &saved);
    assert_eq!(fresh.progress(P0).unwrap(), saved);
    let crab = fresh.crab(P0).unwrap();
    assert_eq!(crab.home_stage(&t), 2);
    assert_eq!(crab.larder, t.food.larder + 2, "the bigger larder kept");
    assert_eq!(crab.mover.clinging, vec![Carry::Grit]);
}

#[test]
fn a_save_from_before_building_still_loads() {
    let old = r#"{"upgrades":1,"lantern":0,"scissor_claw":false,"climb_xp":0.0,"carrying":null,
        "eaten":[true,false,false,false],"cultures_taken":[false],"clues_found":[false],
        "teeth_taken":[false],"nets_cut":[false],"cast_offs":[],"larder":2,"clams":[0,0,0,0,0],
        "well_fed":0,"wakes_at":null}"#;
    let saved: Progress = serde_json::from_str(old).expect("an old save still reads");
    assert_eq!(saved.home_grit, 0);
    let mut sim = by_home();
    sim.restore_progress(P0, &saved);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(
        (crab.mover.upgrades, crab.larder),
        (1, 2),
        "the rest counts"
    );
    assert_eq!(crab.home_grit, 0);
    assert_eq!(crab.grit_taken, vec![false; sim.level.grit.len()]);
}

#[test]
fn a_replay_of_building_plays_the_same() {
    // A reload with grit on the shell (and its spot taken), then home to
    // build it in: a bug report from here must play the same.
    let mut live = by_home();
    let mut saved = live.progress(P0).unwrap();
    saved.clinging = vec![Carry::Grit];
    saved.grit_taken[0] = true;
    live.restore_progress(P0, &saved);
    live.start_recording();
    walk_home(&mut live);
    idle(&mut live, 30);
    assert_eq!(live.crab(P0).unwrap().home_grit, 1);

    let text = live.recording().unwrap().to_text();
    let replay = crabsim::Replay::parse(&text).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(again.crab(P0).unwrap().home_grit, 1);
    assert_eq!(again.progress(P0), live.progress(P0));
    assert_eq!(replay.hash, Some(again.state_hash()), "{text}");
}
