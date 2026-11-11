//! Production delegated-step gate exercised against real regional physics.
use super::{DelegatedStep, delegated_step};
use bevy::prelude::Vec2;
use crabsim::{
    Controls, Level, PlayerId, Sim, Tuning,
    underwater::{
        Cell, GAME_BOTTOM, GAME_CELL, World,
        region::CellRegion,
        region_store::{RegionId, TerrainStore},
    },
    world::regions::{
        EntityArchive, RegionError, RegionFrame, RegionKernel, journal::RegionJournal,
    },
};

const GUEST: PlayerId = PlayerId(7);
const BATCH: usize = 4;

fn fixture(speed: f32) -> (TerrainStore, EntityArchive, RegionKernel) {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    sim.enable_underwater_cells();
    // A water-only corridor isolates lease enforcement from terrain collisions.
    // Use the public codec, as the sediment regional tests do.
    let mut world = serde_json::to_value(sim.level.cells.take().unwrap()).unwrap();
    for cell in world["cells"].as_array_mut().unwrap() {
        *cell = serde_json::to_value(Cell::Water).unwrap();
    }
    for owner in world["pebble_owner"].as_array_mut().unwrap() {
        *owner = serde_json::json!(0);
    }
    world["pebbles"] = serde_json::json!([]);
    world["pebble_motion"] = serde_json::json!([]);
    let world: World = serde_json::from_value(world).unwrap();
    sim.add_crab(GUEST, Vec2::new(700.0, 440.0));
    sim.crab_mut(GUEST).unwrap().mover.vel.y = -speed;
    let mut terrain = TerrainStore::new(world).unwrap();
    let mut archive = EntityArchive::new(sim).unwrap();
    let bounds = CellRegion {
        x: 64,
        y: 128,
        width: 256,
        height: 256,
    };
    let (epoch, cells) = terrain
        .grant(RegionId(1), bounds, &archive.sim.level)
        .unwrap();
    let kernel = archive
        .extract(RegionId(1), epoch, &[GUEST], cells)
        .unwrap();
    (terrain, archive, kernel)
}

fn frame(kernel: &RegionKernel) -> RegionFrame {
    RegionFrame {
        tick: kernel.sim.tick_count(),
        controls: vec![(
            GUEST,
            Controls {
                down: true,
                ..Controls::default()
            },
        )],
        actions: vec![],
    }
}

// The same 64-cell vertical margin as regional::near_edge. The fixture moves
// only vertically, well away from either horizontal edge.
fn needs_region(kernel: &RegionKernel) -> bool {
    let b = kernel.bounds();
    let y = (kernel.sim.crab(GUEST).unwrap().mover.pos.y - GAME_BOTTOM) / GAME_CELL;
    y < b.y as f32 + 64.0 || y > (b.y + b.height as i32) as f32 - 64.0
}

#[test]
fn delayed_stop_can_cross_a_real_lease_but_holding_the_requested_batch_preserves_it() {
    for speed in [480.0, 720.0] {
        let (mut terrain, mut archive, mut kernel) = fixture(speed);
        let mut journal = RegionJournal::new(&kernel, GUEST).unwrap();
        let mut requested = false;
        for _ in 0..60 {
            let mut frames = Vec::new();
            for _ in 0..BATCH {
                let f = frame(&kernel);
                kernel.tick(&f).unwrap();
                frames.push(f);
            }
            journal.append(GUEST, kernel.epoch, &frames).unwrap();
            if needs_region(&kernel) {
                requested = true;
                break;
            }
        }
        assert!(
            requested,
            "fixture must request renewal at a complete batch"
        );
        let tick = kernel.sim.tick_count();
        let checkpoint = kernel.checkpoint().unwrap();
        let mover = &kernel.sim.crab(GUEST).unwrap().mover;
        println!(
            "speed={speed}, renewal requested at tick={tick}, position={:?}, velocity={:?}",
            mover.pos, mover.vel
        );
        assert_eq!(journal.acknowledged_tick(), tick);
        assert_eq!(
            journal.materialize().unwrap().state_hash(),
            kernel.state_hash()
        );

        // Before #591, queued Drive messages still permit physics while Stop
        // is delayed. Applied acknowledgements can replenish the 60-tick credit.
        // Probe several delays, including multiple complete four-frame batches.
        // This is an adversarial protocol schedule, not a claim that the failed
        // browser run received these acknowledgements before its Stop.
        let mut failures = 0;
        for delay in [240, 4, 16, 60, 120] {
            let mut advancing = RegionKernel::from_checkpoint(&checkpoint).unwrap();
            let mut owner_journal = RegionJournal::new(&advancing, GUEST).unwrap();
            let mut applied = tick;
            let mut partial = Vec::new();
            let mut error = None;
            for _ in 0..delay {
                // Model valid Applied messages independently of the delayed
                // topology stop. Never advance past the real credit limit.
                assert!(advancing.sim.tick_count().saturating_sub(applied) < 60);
                let f = frame(&advancing);
                if let Err(e) = advancing.tick(&f) {
                    error = Some(e);
                    break;
                }
                partial.push(f);
                if partial.len() == BATCH {
                    owner_journal
                        .append(GUEST, advancing.epoch, &partial)
                        .unwrap();
                    partial.clear();
                    if (advancing.sim.tick_count() - tick).is_multiple_of(16) {
                        applied = owner_journal.acknowledged_tick();
                    }
                }
            }
            println!(
                "speed={speed}, Stop delay={delay} ticks, advanced={}, error={error:?}",
                advancing.sim.tick_count() - tick
            );
            if delay == 4 {
                assert!(
                    error.is_none(),
                    "one extra batch must still fit this fixture"
                );
            }
            if delay == 240 {
                assert_eq!(
                    error,
                    Some(RegionError::OutsideRegion),
                    "a long delay must expose the baseline bug"
                );
            }
            if let Some(e) = error {
                assert_eq!(
                    e,
                    RegionError::OutsideRegion,
                    "speed={speed}, delay={delay}"
                );
                failures += 1;
            }
            // Feed the identical delayed-Stop schedule through the production
            // helper. Advance must really run physics: removing the renewal
            // condition then reaches OutsideRegion, rather than merely failing
            // a Boolean assertion. Incoming Drive leaves `enabled` true.
            let mut held = RegionKernel::from_checkpoint(&checkpoint).unwrap();
            let mut held_journal = RegionJournal::new(&held, GUEST).unwrap();
            let mut partial = Vec::new();
            let mut applied = tick;
            for _ in 0..delay {
                match delegated_step(
                    true,
                    false,
                    false,
                    partial.len(),
                    false,
                    held.sim.tick_count().saturating_sub(applied) >= 60,
                    requested,
                ) {
                    DelegatedStep::Wait => {}
                    DelegatedStep::FinishStop => panic!("Stop has not arrived"),
                    DelegatedStep::Advance => {
                        let f = frame(&held);
                        held.tick(&f).expect(
                            "renewal hold must prevent OutsideRegion while Stop is delayed",
                        );
                        partial.push(f);
                        if partial.len() == BATCH {
                            held_journal.append(GUEST, held.epoch, &partial).unwrap();
                            partial.clear();
                            if (held.sim.tick_count() - tick).is_multiple_of(16) {
                                applied = held_journal.acknowledged_tick();
                            }
                        }
                    }
                }
            }
            assert_eq!(held_journal.acknowledged_tick(), tick);
            assert_eq!(held.sim.tick_count(), tick);
            assert_eq!(held.checkpoint().unwrap(), checkpoint);
            assert_eq!(
                journal.materialize().unwrap().state_hash(),
                held.state_hash()
            );
        }
        assert!(
            failures > 0,
            "baseline must leave the real lease at speed={speed}"
        );

        // Stop arrives while a checkpoint backup is in flight. Even with no
        // tick credit it must finish once that backup is acknowledged, exactly
        // once and without taking another simulation step.
        assert_eq!(
            delegated_step(true, true, false, 0, true, true, requested),
            DelegatedStep::Wait
        );
        assert_eq!(
            delegated_step(true, true, false, 0, false, true, requested),
            DelegatedStep::FinishStop
        );
        assert_eq!(
            delegated_step(false, true, true, 0, false, true, requested),
            DelegatedStep::Wait
        );
        assert_eq!(kernel.checkpoint().unwrap(), checkpoint);

        // Stop can now acknowledge exactly the archived cut. Collect that cut,
        // grant a new lease, and resume without losing or inventing any ticks.
        terrain
            .commit_state(
                kernel.id,
                kernel.epoch,
                kernel.sim.level.cells.as_ref().unwrap(),
                kernel.arrivals_delivered(),
            )
            .unwrap();
        terrain.release(kernel.id, kernel.epoch).unwrap();
        archive.absorb(kernel).unwrap();
        let bounds = CellRegion {
            x: 64,
            y: 0,
            width: 256,
            height: 256,
        };
        let (epoch, cells) = terrain
            .grant(RegionId(2), bounds, &archive.sim.level)
            .unwrap();
        let mut renewed = archive
            .extract(RegionId(2), epoch, &[GUEST], cells)
            .unwrap();
        assert_eq!(renewed.sim.tick_count(), tick);
        let mut renewed_journal = RegionJournal::new(&renewed, GUEST).unwrap();
        // Installing a new checkpoint reconstructs Multi::default, clearing
        // its request/stop flags. Ready precedes the first Drive.
        requested = false;
        assert_eq!(
            delegated_step(false, false, false, 0, false, false, requested),
            DelegatedStep::Wait
        );
        for n in 0..BATCH * 4 {
            assert_eq!(
                delegated_step(true, false, false, n % BATCH, false, false, requested),
                DelegatedStep::Advance
            );
            let f = frame(&renewed);
            renewed.tick(&f).unwrap();
            renewed_journal.append(GUEST, epoch, &[f]).unwrap();
        }
        assert_eq!(renewed.sim.tick_count(), tick + 16);
        assert_eq!(
            renewed_journal.materialize().unwrap().state_hash(),
            renewed.state_hash()
        );
    }
}

#[test]
fn failed_installed_view_tick_keeps_diagnostic_bounds_in_the_live_sim() {
    let (_, _, kernel) = fixture(720.0);
    let expected_bounds = kernel.bounds();
    let mut live = Sim::new(Tuning::default(), Level::test_level());
    let mut kernel = kernel.install_view(&mut live);
    assert!(kernel.sim.level.cells.is_none());
    assert!(kernel.sim.crabs().is_empty());

    let mut observed_fault = false;
    for _ in 0..240 {
        let frame = RegionFrame {
            tick: live.tick_count(),
            controls: vec![(
                GUEST,
                Controls {
                    down: true,
                    ..Controls::default()
                },
            )],
            actions: vec![],
        };
        if let Err(error) = kernel.tick_view(&mut live, &frame) {
            assert_eq!(error, RegionError::OutsideRegion);
            assert_eq!(live.tick_count(), frame.tick);
            assert!(live.crab(GUEST).is_some());
            // tick_view must swap back on failure too. Calling kernel.bounds()
            // here would read its empty placeholder and panic. Diagnostics use
            // this fallible access to the live view, as regional::bounds does.
            assert!(kernel.sim.level.cells.is_none());
            assert!(kernel.sim.crabs().is_empty());
            let diagnostic_bounds = live
                .level
                .cells
                .as_ref()
                .map(|cells| cells.region())
                .ok_or("Missing regional terrain");
            assert_eq!(diagnostic_bounds, Ok(expected_bounds));
            observed_fault = true;
            break;
        }
        assert!(kernel.sim.level.cells.is_none());
    }
    assert!(
        observed_fault,
        "fixture must exercise the real error return path"
    );
}
