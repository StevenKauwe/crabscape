//! Promises about carrying: small things (clams, shell grit) cling to the
//! crab's shell, several at once, the way a decorator crab wears bits of the
//! floor; big finds (the lump of bacteria, the shed tooth) ride on its back
//! one at a time. Neither blocks the other. At home everything comes off at
//! once (clams into the larder, the rest put by the workbench); dying leaves
//! all of it in the cast-off shell.

use bevy_math::Vec2;
use crabsim::{Carry, Controls, Hit, PlayerId, Progress, Shelter, Sim};

const P0: PlayerId = PlayerId(0);

fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.tick();
    }
}

/// A crab out on the floor a little way from home, with nothing near.
fn by_home() -> Sim {
    Sim::with_one_crab(Vec2::new(180.0, 0.0))
}

/// Put the crab out at `at` (keeping what it carries) and let one tick
/// pass.
fn step_onto(sim: &mut Sim, at: Vec2) {
    let m = &mut sim.crab_mut(P0).unwrap().mover;
    m.pos = at;
    m.vel = Vec2::ZERO;
    m.shelter = Shelter::Out;
    m.tucked = 0.0;
    sim.tick();
}

/// Put the crab by the doorway, then walk home: see [`walk_home`].
fn go_home(sim: &mut Sim) -> Vec<&'static str> {
    {
        let m = &mut sim.crab_mut(P0).unwrap().mover;
        m.pos = Vec2::new(180.0, 0.0);
        m.vel = Vec2::ZERO;
        m.shelter = Shelter::Out;
        m.tucked = 0.0;
    }
    walk_home(sim)
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

fn die(sim: &mut Sim) {
    for _ in 0..4 {
        sim.hit_crab(
            P0,
            &Hit {
                damage: 60.0,
                ..Hit::default()
            },
        );
        sim.tick();
    }
    assert!(sim.crab(P0).unwrap().fighter.dead.is_some());
    let respawn = sim.tuning.respawn_ticks;
    idle(sim, respawn + 2);
}

#[test]
fn small_things_cling_to_the_shell_several_at_once() {
    let mut sim = by_home();
    let (clam0, grit0, clam1) = (sim.level.clams[0], sim.level.grit[0], sim.level.clams[1]);
    step_onto(&mut sim, clam0);
    step_onto(&mut sim, grit0);
    step_onto(&mut sim, clam1);
    let crab = sim.crab(P0).unwrap();
    assert!(crab.clams[0] > 0, "the first clam taken");
    assert!(crab.grit_taken[0], "the grit taken, with a clam on");
    assert!(crab.clams[1] > 0, "the second clam taken too");

    // Home: all of it comes off at once.
    let seen = go_home(&mut sim);
    let crab = sim.crab(P0).unwrap();
    assert!(
        seen.contains(&"stored") && seen.contains(&"put away"),
        "{seen:?}"
    );
    assert_eq!(crab.larder, 2);
    assert_eq!(crab.stash.grit, 1);
}

#[test]
fn small_things_never_block_a_big_find_and_a_big_find_never_blocks_them() {
    let mut sim = by_home();
    let clam = sim.level.clams[0];
    let culture = sim.level.cultures[0];
    let pit_grit = sim.level.grit[4];
    step_onto(&mut sim, clam);
    step_onto(&mut sim, culture);
    assert!(
        sim.crab(P0).unwrap().cultures_taken[0],
        "a clam on the shell doesn't stop it taking the culture"
    );
    step_onto(&mut sim, pit_grit);
    assert!(
        sim.crab(P0).unwrap().grit_taken[4],
        "the culture on its back doesn't stop it taking grit"
    );

    // One trip home brings all three.
    let seen = go_home(&mut sim);
    let crab = sim.crab(P0).unwrap();
    assert!(seen.contains(&"put away"), "{seen:?}");
    assert_eq!(crab.stash.cultures, 1);
    assert_eq!(crab.larder, 1);
    assert_eq!(crab.stash.grit, 1);
}

#[test]
fn dying_leaves_everything_carried_in_the_cast_off_shell() {
    let mut sim = by_home();
    let (clam, grit, culture) = (sim.level.clams[4], sim.level.grit[4], sim.level.cultures[0]);
    step_onto(&mut sim, clam);
    step_onto(&mut sim, grit);
    step_onto(&mut sim, culture);
    die(&mut sim);
    let crab = sim.crab(P0).unwrap();
    assert!(!crab.cast_offs.is_empty(), "left in the cast-off shell");
    let shell = crab.cast_offs[0].pos;

    // Walking back over it takes all of it back.
    let mut recovered = false;
    for _ in 0..3 {
        step_onto(&mut sim, shell);
        recovered |= sim.crab(P0).unwrap().events.recovered;
    }
    assert!(recovered);
    assert!(sim.crab(P0).unwrap().cast_offs.is_empty(), "nothing left");

    go_home(&mut sim);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.stash.cultures, 1);
    assert_eq!(crab.larder, 1);
    assert_eq!(crab.stash.grit, 1);
}

#[test]
fn an_old_save_with_a_clam_on_its_back_still_carries_it() {
    // A save from when one thing rode on the back: a clam there, and a piece
    // of grit in a cast-off shell. (Built from this level's lists, so it
    // loads whatever the level holds.)
    let mut sim = by_home();
    let fell = Vec2::new(2600.0, sim.level.floor_height(2600.0));
    let mut old = serde_json::to_value(sim.fresh_progress()).unwrap();
    let fields = old.as_object_mut().unwrap();
    fields.retain(|k, _| k != "clinging");
    fields.insert("carrying".into(), "Clam".into());
    fields.insert(
        "cast_offs".into(),
        serde_json::json!([{ "pos": [fell.x, fell.y], "carrying": "Grit" }]),
    );
    let saved: Progress = serde_json::from_value(old).expect("an old save still reads");
    sim.restore_progress(P0, &saved);
    // The clam moves onto its shell, and saves that way from now on.
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.carrying, None);
    assert_eq!(crab.mover.clinging, vec![Carry::Clam]);
    let resaved = sim.progress(P0).unwrap();
    assert_eq!(
        (resaved.carrying, resaved.clinging),
        (None, vec![Carry::Clam])
    );

    // The clam doesn't fill its back: it takes grit too...
    let grit0 = sim.level.grit[0];
    step_onto(&mut sim, grit0);
    assert!(sim.crab(P0).unwrap().grit_taken[0]);
    // ...and gets the grit in its cast-off shell back.
    step_onto(&mut sim, fell);
    assert!(sim.crab(P0).unwrap().cast_offs.is_empty());

    go_home(&mut sim);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.larder, 1);
    assert_eq!(crab.stash.grit, 2);
}

#[test]
fn a_full_shell_leaves_a_small_thing_where_it_lies() {
    let mut sim = by_home();
    let cling = sim.tuning.cling;
    let small: Vec<Vec2> = sim
        .level
        .clams
        .iter()
        .chain(&sim.level.grit)
        .copied()
        .collect();
    assert!(small.len() > cling, "more small things lie about than fit");
    for &at in &small[..cling] {
        step_onto(&mut sim, at);
    }
    assert_eq!(sim.crab(P0).unwrap().mover.clinging.len(), cling);
    let before = sim.crab(P0).unwrap().clone();
    step_onto(&mut sim, small[cling]);
    let crab = sim.crab(P0).unwrap();
    assert!(crab.events.shell_full, "it says so");
    assert_eq!(crab.events.picked_up, None, "no swap");
    assert_eq!(crab.mover.clinging, before.mover.clinging);
    let bare = |clams: &[u32]| clams.iter().map(|&c| c > 0).collect::<Vec<_>>();
    assert_eq!(
        (bare(&crab.clams), &crab.grit_taken),
        (bare(&before.clams), &before.grit_taken),
        "it lies where it was"
    );
}

#[test]
fn big_finds_still_ride_on_its_back_one_at_a_time() {
    let mut sim = by_home();
    let (culture, tooth) = (sim.level.cultures[0], sim.level.teeth[0]);
    step_onto(&mut sim, culture);
    step_onto(&mut sim, tooth);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.carrying, Some(Carry::Culture));
    assert!(crab.mover.clinging.is_empty(), "a big find never clings");
    assert!(!crab.teeth_taken[0], "the tooth waits for a free back");
    assert!(!crab.events.shell_full);
}

#[test]
fn what_clings_is_saved_and_a_replay_carries_it() {
    let mut live = by_home();
    let mut saved = live.progress(P0).unwrap();
    saved.carrying = Some(Carry::Culture);
    saved.clinging = vec![Carry::Clam, Carry::Grit];
    let text = serde_json::to_string(&saved).unwrap();
    let saved: Progress = serde_json::from_str(&text).unwrap();
    live.restore_progress(P0, &saved);
    assert_eq!(live.progress(P0).unwrap(), saved);

    live.start_recording();
    walk_home(&mut live);
    idle(&mut live, 30);
    let crab = live.crab(P0).unwrap();
    let kept = (crab.stash.cultures, crab.larder, crab.stash.grit);
    assert_eq!(kept, (1, 1, 1));
    assert!(crab.mover.load().next().is_none(), "all of it unloaded");

    let text = live.recording().unwrap().to_text();
    let replay = crabsim::Replay::parse(&text).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(again.progress(P0), live.progress(P0));
    assert_eq!(replay.hash, Some(again.state_hash()), "{text}");
}
