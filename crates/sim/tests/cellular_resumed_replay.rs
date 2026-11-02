//! A proof of the startup ordering needed for a replay after a cellular save.
//!
//! This deliberately uses a small test harness: the product replay format
//! does not yet carry the initial terrain checkpoint, and resumed recordings
//! must remain disabled until it does (including a bounded report transport).

use bevy_math::Vec2;
use crabsim::{
    Controls, Level, PlayerId, Progress, Replay, Sim, Tuning,
    enemy::Kind,
    replay::Recorder,
    underwater::{World, WorldCheckpoint},
};

fn start_from_save(terrain: World, tick: u64, pos: Vec2, progress: &Progress) -> Sim {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    assert!(sim.restore_cellular_terrain(terrain, tick));
    sim.add_crab(PlayerId(0), pos);
    sim.restore_progress(PlayerId(0), progress);
    sim
}

#[test]
fn saved_terrain_player_and_absolute_clock_replay_with_relative_events() {
    let mut before_save = Sim::new(Tuning::default(), Level::test_level());
    before_save.enable_underwater_cells();
    before_save.add_crab(PlayerId(0), Vec2::new(400.0, 0.0));
    for _ in 0..81 {
        before_save.set_controls(
            PlayerId(0),
            Controls {
                dig_held: true,
                down: true,
                ..Controls::default()
            },
        );
        before_save.tick();
    }
    let tick = before_save.tick_count();
    assert!(!tick.is_multiple_of(4), "exercise the material-step phase");
    let pos = before_save.crab(PlayerId(0)).unwrap().mover.pos;
    let mut progress = before_save.progress(PlayerId(0)).unwrap();
    progress.lantern = 1;
    let checkpoint = before_save.level.cells.as_ref().unwrap().checkpoint();
    let bytes = postcard::to_allocvec(&checkpoint).unwrap();
    let restore_terrain = || {
        let checkpoint: WorldCheckpoint = postcard::from_bytes(&bytes).unwrap();
        checkpoint.into_world().unwrap()
    };

    // The real game populates the level after it starts recording. Record
    // those initial creatures, plus a later spawn, on the relative timeline.
    let mut live = start_from_save(restore_terrain(), tick, pos, &progress);
    let mut recorder = Recorder::new(pos, tick, &live.tuning);
    recorder.note_progress(progress.clone());
    for (kind, at) in live.level.foes.clone() {
        recorder.note_spawn(tick, kind, at);
        live.spawn_enemy(kind, at);
    }
    for relative in 0..120 {
        if relative == 37 {
            let at = Vec2::new(750.0, 0.0);
            recorder.note_spawn(live.tick_count(), Kind::Mantis, at);
            live.spawn_enemy(Kind::Mantis, at);
        }
        let controls = if relative < 60 {
            Controls {
                dig_held: true,
                down: true,
                walk: 0.5,
                ..Controls::default()
            }
        } else {
            Controls {
                dig_held: true,
                dig_up: true,
                walk: -0.5,
                ..Controls::default()
            }
        };
        recorder.note(live.tick_count(), controls);
        live.set_controls(PlayerId(0), controls);
        live.tick();
    }
    let replay = recorder.replay(live.tick_count());
    let replay = Replay::parse(&replay.to_text()).unwrap();
    let mut restored = start_from_save(
        restore_terrain(),
        tick,
        pos,
        replay.progress.as_ref().unwrap(),
    );
    let mut playback = replay.playback();
    for relative in 0..replay.length() {
        for spawn in replay.spawns_at(relative) {
            restored.spawn_enemy(spawn.kind, spawn.pos);
        }
        restored.set_controls(PlayerId(0), playback.next_controls());
        restored.tick();
    }
    assert_eq!(live.tick_count(), restored.tick_count());
    assert_eq!(live.state_hash(), restored.state_hash());
}
