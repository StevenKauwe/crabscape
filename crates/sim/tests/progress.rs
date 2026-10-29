//! Promises about saved progress: what a crab grew, fitted and took comes
//! back after a reload, and what it took stays taken.

use bevy_math::Vec2;
use crabsim::{Carry, CastOff, PlayerId, Progress, Sim};

const P0: PlayerId = PlayerId(0);

/// Stand still for `n` ticks.
fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.tick();
    }
}

#[test]
fn progress_comes_back_in_a_fresh_world() {
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.mover.upgrades = 2;
        crab.mover.lantern = 1;
        crab.mover.scissor_claw = true;
        crab.mover.climb_xp = 30.0;
        crab.mover.carrying = Some(Carry::Tooth);
        crab.eaten[0] = true;
        crab.cultures_taken[0] = true;
        crab.teeth_taken[0] = true;
        crab.cast_offs.push(CastOff {
            pos: Vec2::new(700.0, 0.0),
            carrying: Carry::Culture,
        });
    }
    sim.level.nets[0].cut = true;
    let saved = sim.progress(P0).unwrap();
    // Through the save's text and back, as the game keeps it.
    let text = serde_json::to_string(&saved).unwrap();
    let saved: Progress = serde_json::from_str(&text).unwrap();

    let mut fresh = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    fresh.restore_progress(P0, &saved);
    assert_eq!(fresh.progress(P0).unwrap(), saved);
    let crab = fresh.crab(P0).unwrap();
    assert_eq!(
        crab.mover.stamina,
        crab.mover.strokes(&fresh.tuning) as f32,
        "rested, with the eggs' strokes"
    );
}

#[test]
fn a_lump_taken_before_the_reload_is_not_there_to_take_again() {
    let sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    let patch = sim.level.cultures[0];
    let mut saved = sim.progress(P0).unwrap();
    saved.cultures_taken[0] = true;

    let mut fresh = Sim::with_one_crab(patch);
    fresh.restore_progress(P0, &saved);
    idle(&mut fresh, 60);
    assert_eq!(fresh.crab(P0).unwrap().mover.carrying, None);

    // Without the save, standing there picks it up.
    let mut new = Sim::with_one_crab(patch);
    idle(&mut new, 60);
    assert_eq!(new.crab(P0).unwrap().mover.carrying, Some(Carry::Culture));
}

#[test]
fn a_save_from_a_different_level_keeps_only_what_the_crab_grew() {
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    let mut saved = sim.progress(P0).unwrap();
    saved.lantern = 2;
    saved.carrying = Some(Carry::Culture);
    saved.eaten.push(true); // one egg more than this level has
    sim.restore_progress(P0, &saved);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.lantern, 2);
    assert_eq!(crab.mover.carrying, None);
    assert_eq!(crab.eaten.len(), sim.level.eggs.len());
}
