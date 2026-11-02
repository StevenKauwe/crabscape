//! A proof of the startup ordering needed for a replay after a cellular save.
//!
//! This tests the production recorder and text replay, including its bounded
//! terrain checkpoint, after restoring a saved game's material-step clock.

use bevy_math::Vec2;
use crabsim::{
    Controls, Feel, Level, Memory, MemorySave, PlayerId, Progress, Replay, Sim, Tuning,
    enemy::Kind,
    underwater::{World, WorldCheckpoint},
};

fn start_from_save(
    terrain: World,
    tick: u64,
    pos: Vec2,
    progress: &Progress,
    memory: &MemorySave,
) -> Sim {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    assert!(sim.restore_cellular_terrain(terrain, tick));
    sim.set_feel(Feel::Heavy);
    sim.add_crab(PlayerId(0), pos);
    sim.restore_progress(PlayerId(0), progress);
    let memory = Memory::from_save(memory, &sim.level, &sim.tuning.memory).unwrap();
    sim.crab_mut(PlayerId(0)).unwrap().memory = memory;
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
    let saved_memory = before_save.crab(PlayerId(0)).unwrap().memory.to_save();
    assert!(
        !saved_memory.cells.is_empty(),
        "the saved map contains explored terrain"
    );
    let mut progress = before_save.progress(PlayerId(0)).unwrap();
    progress.lantern = 1;
    let checkpoint = before_save.level.cells.as_ref().unwrap().checkpoint();
    let bytes = postcard::to_allocvec(&checkpoint).unwrap();
    let restore_terrain = || {
        let checkpoint: WorldCheckpoint = postcard::from_bytes(&bytes).unwrap();
        checkpoint.into_world().unwrap()
    };

    let mut live = start_from_save(restore_terrain(), tick, pos, &progress, &saved_memory);
    let started = std::time::Instant::now();
    live.start_recording();
    eprintln!("resumed replay capture: {:?}", started.elapsed());
    assert!(live.recording().is_some(), "loaded startup can be recorded");
    live.populate();
    for relative in 0..120 {
        if relative == 37 {
            let at = Vec2::new(750.0, 0.0);
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
        live.set_controls(PlayerId(0), controls);
        live.tick();
    }
    let replay_text = live.recording().unwrap().to_text();
    eprintln!("resumed replay text: {} bytes", replay_text.len());
    let replay = Replay::parse(&replay_text).unwrap();
    let mut restored = Sim::with_one_crab(replay.start);
    crabsim::play(&mut restored, &replay, replay.length(), |_| {});
    assert_eq!(live.tick_count(), restored.tick_count());
    assert_eq!(live.state_hash(), restored.state_hash());
    assert_eq!(
        live.crab(PlayerId(0)).unwrap().memory,
        restored.crab(PlayerId(0)).unwrap().memory
    );
    assert_eq!(
        live.crab(PlayerId(0)).unwrap().memory.fingerprint(),
        restored.crab(PlayerId(0)).unwrap().memory.fingerprint()
    );
    assert_eq!(replay.hash, Some(restored.state_hash()));

    let original_memory_line = replay_text
        .lines()
        .find(|line| line.starts_with("memory "))
        .unwrap();
    let mut incompatible_memory = saved_memory.clone();
    incompatible_memory.cols = 0;
    let bad_memory_line = format!(
        "memory {}",
        serde_json::to_string(&incompatible_memory).unwrap()
    );
    let bad_replay = replay_text.replacen(original_memory_line, &bad_memory_line, 1);
    assert!(
        Replay::parse(&bad_replay).is_err(),
        "an incompatible remembered map is rejected"
    );

    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = std::env::temp_dir().join(format!(
            "crabscape-resumed-replay-{}.txt",
            std::process::id()
        ));
        std::fs::write(&path, &replay_text).unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_crabsim"))
            .arg("--inputs")
            .arg(&path)
            .output()
            .unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("replay: same state hash as when it was recorded")
        );
    }

    live.start_recording();
    assert!(
        live.recording().is_none(),
        "arbitrary mid-session recordings remain disabled"
    );
}

#[test]
fn corrupted_and_invalid_replay_checkpoint_is_rejected() {
    for text in [
        "world cells\ncell_start 1 not+base64!\n",
        "world cells\ncell_start 0 A\n",
        "world cells\ncell_start 1 A\n",
        "world cells\ncell_start 18446744073709551615 AAA\n",
    ] {
        assert!(Replay::parse(text).is_err(), "{text}");
    }
    let oversized = format!("memory {}\n", "x".repeat(1024 * 1024 + 1));
    assert!(Replay::parse(&oversized).is_err());
}
