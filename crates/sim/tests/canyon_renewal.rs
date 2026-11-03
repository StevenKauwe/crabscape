use bevy_math::Vec2;
use crabsim::{
    Level, PlayerId, Tuning,
    canyon_drift::run::{CanyonRun, GrantPhase, SlotPhase},
    underwater::World,
};

fn setup() -> (Level, Tuning, World, CanyonRun) {
    let level = Level::test_level();
    let tuning = Tuning::default();
    let mut world = World::from_level(&level);
    assert!(world.follow_player(&level, 6385.0, -3200.0));
    (level, tuning, world, CanyonRun::new(42))
}
fn crossing(run: &mut CanyonRun, level: &mut Level, world: &World, tuning: &Tuning, start: u64) {
    level.tide = 0.0;
    for tick in start..start + 121 {
        run.step(tick, world, level, tuning);
    }
    level.tide = 1.0;
    run.step(start + 121, world, level, tuning);
}

#[test]
fn occupied_slots_bound_arrivals_and_reload_does_not_catch_up() {
    let (mut level, tuning, world, mut run) = setup();
    crossing(&mut run, &mut level, &world, &tuning, 0);
    assert_eq!(run.slots.iter().filter(|s| s.generation > 0).count(), 1);
    crossing(&mut run, &mut level, &world, &tuning, 122);
    assert_eq!(run.slots.iter().filter(|s| s.generation > 0).count(), 2);
    let before = run.slots.map(|s| s.generation);
    crossing(&mut run, &mut level, &world, &tuning, 244);
    assert_eq!(run.slots.map(|s| s.generation), before);
    let encoded = postcard::to_allocvec(&run).unwrap();
    let mut restored: CanyonRun = postcard::from_bytes(&encoded).unwrap();
    level.tide = -1.0;
    restored.step(10_000, &world, &level, &tuning);
    assert_eq!(restored.slots.map(|s| s.generation), before);
    assert_eq!(restored.practice, run.practice);
}

#[test]
fn generation_claims_are_unique_and_retire_only_after_exact_durable_ack() {
    let (mut level, tuning, world, mut run) = setup();
    crossing(&mut run, &mut level, &world, &tuning, 0);
    let body = match run.slots[0].phase {
        SlotPhase::Free(body) => body,
        other => panic!("unexpected {other:?}"),
    };
    let from = body.position + Vec2::new(0.0, 15.0);
    assert!(
        run.request(0, PlayerId(0), 77, from, false, &world)
            .is_none()
    );
    let first = run.request(0, PlayerId(0), 77, from, true, &world).unwrap();
    assert!(
        run.request(0, PlayerId(1), 88, from, true, &world)
            .is_none()
    );
    assert!(run.offer(first));
    assert!(!run.finalize(first, PlayerId(1), 88));
    assert!(run.finalize(first, PlayerId(0), 77));
    let finalized_revision = run.slots[0].revision;
    assert!(
        !run.retire_durable(first, finalized_revision),
        "receipt is still outstanding"
    );
    assert!(run.confirm_finalized_save(first, finalized_revision));
    assert!(!run.acknowledge_retired_receipt(first, PlayerId(1), 88));
    assert!(run.acknowledge_retired_receipt(first, PlayerId(0), 77));
    let revision = run.slots[0].revision;
    assert!(!run.retire_durable(first, revision - 1));
    assert!(run.retire_durable(first, revision));
    crossing(&mut run, &mut level, &world, &tuning, 122);
    let second_body = match run.slots[0].phase {
        SlotPhase::Free(body) => body,
        other => panic!("unexpected {other:?}"),
    };
    let second = run
        .request(
            0,
            PlayerId(0),
            77,
            second_body.position + Vec2::new(0.0, 15.0),
            true,
            &world,
        )
        .unwrap();
    assert_eq!(second.generation, first.generation + 1);
    assert_ne!(first, second);
    assert!(
        !run.offer(first),
        "stale prior generation cannot grant the new body"
    );
    assert!(matches!(run.slots[0].phase, SlotPhase::Grant(g) if g.phase == GrantPhase::Requested));
}

#[test]
fn practice_remains_usable_when_both_slots_are_occupied() {
    use crabsim::canyon_drift::run::PracticePhase;
    let (mut level, tuning, world, mut run) = setup();
    crossing(&mut run, &mut level, &world, &tuning, 0);
    crossing(&mut run, &mut level, &world, &tuning, 122);
    let generations = run.slots.map(|s| s.generation);
    let at = run.practice.position;
    assert!(run.lift_practice(PlayerId(0), 77, at + Vec2::new(0.0, 35.0), true, &world));
    assert_eq!(run.practice_phase, PracticePhase::Carried(PlayerId(0), 77));
    assert!(run.drop_practice(PlayerId(0), at + Vec2::new(0.0, 10.0)));
    assert_eq!(run.practice_phase, PracticePhase::Free);
    assert_eq!(run.slots.map(|s| s.generation), generations);
    assert!(run.valid());
}

#[test]
fn loaded_valve_keeps_one_body_while_repositioned_and_bad_state_is_rejected() {
    use crabsim::canyon_drift::{DriftBody, DriftProfile, run::SlotPhase};
    let (_, _, world, mut run) = setup();
    let body = DriftBody {
        profile: DriftProfile::Dense,
        position: run.catcher.position + Vec2::new(0.0, 7.0),
        velocity: Vec2::ZERO,
    };
    run.slots[0].generation = 1;
    run.slots[0].phase = SlotPhase::Caught(body);
    run.slots[0].location = body.position;
    run.catcher.contents = Some(0);
    assert!(run.valid());
    let lower_from = run.catcher.position + Vec2::new(-55.0, 35.0);
    assert!(!run.lift_catcher(PlayerId(0), 77, lower_from, false, &world));
    assert!(run.lift_catcher(PlayerId(0), 77, lower_from, true, &world));
    let upper = Vec2::new(6416.0, -3172.0);
    assert!(run.place_catcher(PlayerId(0), Vec2::new(6350.0, -3150.0), upper, true, &world));
    assert!(
        matches!(run.slots[0].phase, SlotPhase::Caught(b) if b.position == upper + Vec2::new(0.0, 7.0))
    );
    assert!(run.valid());
    let bytes = postcard::to_allocvec(&run).unwrap();
    let mut restored: CanyonRun = postcard::from_bytes(&bytes).unwrap();
    assert!(restored.valid());
    restored.catcher.contents = None;
    assert!(!restored.valid());
}

#[test]
fn caught_generation_transfers_with_valve_without_duplicate_owner() {
    use crabsim::canyon_drift::{DriftBody, DriftProfile};
    let (_, _, _, mut archive) = setup();
    let body = DriftBody {
        profile: DriftProfile::Dense,
        position: archive.catcher.position + Vec2::new(0.0, 7.0),
        velocity: Vec2::ZERO,
    };
    archive.slots[0].generation = 1;
    archive.slots[0].phase = SlotPhase::Caught(body);
    archive.slots[0].location = body.position;
    archive.catcher.contents = Some(0);
    let before = archive.clone();
    let moved = archive.extract(|p| p.x >= 6400.0).unwrap();
    assert!(moved.has_catcher && moved.owned_slots[0]);
    assert!(!archive.has_catcher && !archive.owned_slots[0]);
    assert!(archive.valid() && moved.valid());
    let unchanged = archive.clone();
    assert!(!archive.absorb(&before));
    assert_eq!(archive, unchanged);
    assert!(archive.absorb(&moved));
    assert_eq!(archive, before);
    assert!(archive.complete());
}

#[test]
fn bounded_spatial_records_round_trip_a_caught_body_across_a_page_edge() {
    use crabsim::canyon_drift::{DriftBody, DriftProfile};
    let (_, _, _, mut run) = setup();
    run.catcher.position = Vec2::new(6405.0, -3180.0);
    let body = DriftBody {
        profile: DriftProfile::Flat,
        position: run.catcher.position + Vec2::new(0.0, 7.0),
        velocity: Vec2::ZERO,
    };
    run.slots[0].generation = 1;
    run.slots[0].phase = SlotPhase::Caught(body);
    run.slots[0].location = body.position;
    run.catcher.contents = Some(0);
    assert!(run.complete());
    let records = run.records();
    assert!(records.iter().all(|r| r.valid()));
    assert_eq!(CanyonRun::from_records(&records), Some(run));
    let mut duplicate = records.clone();
    duplicate.push(records[0].clone());
    assert!(CanyonRun::from_records(&duplicate).is_none());
}

#[test]
fn full_sim_spatial_restore_keeps_canyon_state_and_rejects_missing_anchor() {
    use crabsim::Sim;
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    sim.salvage.as_mut().unwrap().world_id = 42;
    sim.canyon.as_mut().unwrap().world_id = 42;
    let records = sim.canyon_records();
    assert!(records.iter().any(|r| r.anchor.is_some()));
    let before = sim.canyon.clone();
    let mut damaged = records.clone();
    for record in &mut damaged {
        record.anchor = None;
    }
    assert!(!sim.restore_canyon_records(damaged));
    assert_eq!(sim.canyon, before);
    assert!(sim.restore_canyon_records(records));
    assert_eq!(sim.canyon, before);
}

#[test]
fn sim_claim_survives_saved_receipts_and_retirement() {
    use crabsim::{
        Carry, Sim,
        canyon_drift::{
            CanyonAction, DriftBody, DriftProfile,
            run::{GrantPhase, SlotPhase},
        },
    };
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(6380.0, -3200.0));
    sim.enable_underwater_cells();
    sim.crab_mut(id).unwrap().salvage_identity = 77;
    sim.salvage.as_mut().unwrap().world_id = 42;
    sim.canyon.as_mut().unwrap().world_id = 42;
    sim.tick();
    let from = sim.crab(id).unwrap().mover.pos
        + Vec2::Y * (sim.crab(id).unwrap().mover.height(&sim.tuning) * 0.5);
    let body = DriftBody {
        profile: DriftProfile::Flat,
        position: from + Vec2::new(10.0, 0.0),
        velocity: Vec2::ZERO,
    };
    let run = sim.canyon.as_mut().unwrap();
    run.slots[0].generation = 1;
    run.slots[0].location = body.position;
    run.slots[0].phase = SlotPhase::Free(body);
    assert!(sim.canyon_action(id, CanyonAction::Take(0)));
    let grant = match sim.canyon.as_ref().unwrap().slots[0].phase {
        SlotPhase::Grant(g) => g,
        other => panic!("{other:?}"),
    };
    assert!(sim.canyon_action(id, CanyonAction::Offer(grant.id)));
    assert!(sim.canyon_action(id, CanyonAction::Grant(grant.id)));
    assert!(sim.canyon_action(id, CanyonAction::Grant(grant.id)));
    assert_eq!(sim.crab(id).unwrap().mover.clinging, [Carry::Valve]);
    let player_saved = sim.progress(id).unwrap();
    assert_eq!(player_saved.drift_receipts, [grant.id]);
    assert!(sim.canyon_action(id, CanyonAction::Finalize(grant.id)));
    let finalized_revision = sim.canyon.as_ref().unwrap().slots[0].revision;
    assert!(sim.canyon_action(
        id,
        CanyonAction::ConfirmFinalized(grant.id, finalized_revision)
    ));
    let finalized_world = sim.canyon_records();
    let proof = match sim.canyon.as_ref().unwrap().slots[0].phase {
        SlotPhase::Grant(g) => g,
        other => panic!("{other:?}"),
    };
    assert_eq!(proof.phase, GrantPhase::FinalizedSaved);
    assert!(sim.canyon_action(id, CanyonAction::ForgetReceipt(proof)));
    assert!(sim.progress(id).unwrap().drift_receipts.is_empty());
    let mut resumed = Sim::with_one_crab(Vec2::new(6380.0, -3200.0));
    resumed.enable_underwater_cells();
    resumed.crab_mut(id).unwrap().salvage_identity = 77;
    resumed.salvage.as_mut().unwrap().world_id = 42;
    assert!(resumed.restore_canyon_records(finalized_world));
    let mut saved_without_receipt = player_saved.clone();
    saved_without_receipt.drift_receipts.clear();
    resumed.restore_progress(id, &saved_without_receipt);
    assert!(resumed.canyon_action(id, CanyonAction::AcknowledgeReceipt(grant.id)));
    let revision = resumed.canyon.as_ref().unwrap().slots[0].revision;
    assert!(resumed.canyon_action(id, CanyonAction::Retire(grant.id, revision)));
    assert_eq!(
        resumed.canyon.as_ref().unwrap().slots[0].phase,
        SlotPhase::Vacant
    );
    assert_eq!(resumed.crab(id).unwrap().mover.clinging, [Carry::Valve]);
    assert!(!resumed.canyon_action(id, CanyonAction::Grant(grant.id)));
}

#[test]
fn canyon_practice_action_and_state_replay_exactly() {
    use crabsim::{Replay, Sim, canyon_drift::CanyonAction};
    let id = PlayerId(0);
    let level = Level::test_level();
    let mut terrain = World::from_level(&level);
    assert!(terrain.follow_player(&level, 6340.0, -3200.0));
    let mut live = Sim::new(Tuning::default(), level);
    assert!(live.restore_cellular_terrain(terrain, 1));
    live.add_crab(id, Vec2::new(6340.0, -3200.0));
    live.crab_mut(id).unwrap().salvage_identity = 77;
    live.salvage.as_mut().unwrap().world_id = 42;
    live.canyon.as_mut().unwrap().world_id = 42;
    live.start_recording();
    assert!(live.recording().is_some());
    assert!(
        live.canyon_action(id, CanyonAction::LiftPractice),
        "crab {:?}",
        live.crab(id).unwrap().mover.pos
    );
    assert!(live.canyon_action(id, CanyonAction::DropPractice));
    assert!(live.canyon_action(id, CanyonAction::ReleasePractice));
    for _ in 0..10 {
        live.tick();
    }
    let replay = Replay::parse(&live.recording().unwrap().to_text()).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(again.canyon, live.canyon);
    assert_eq!(replay.hash, Some(again.state_hash()));
}

#[test]
fn solo_crab_can_lift_and_stage_the_basic_valve_on_upper_shelf() {
    use crabsim::{Controls, Sim, canyon_drift::CanyonAction};
    let id = PlayerId(0);
    let level = Level::test_level();
    let mut terrain = World::from_level(&level);
    assert!(terrain.follow_player(&level, 6390.0, -3240.0));
    let mut sim = Sim::new(Tuning::default(), level);
    assert!(sim.restore_cellular_terrain(terrain, 1));
    sim.add_crab(id, Vec2::new(6390.0, -3240.0));
    sim.crab_mut(id).unwrap().salvage_identity = 77;
    sim.salvage.as_mut().unwrap().world_id = 40;
    sim.canyon.as_mut().unwrap().world_id = 40;
    assert!(
        sim.canyon_action(id, CanyonAction::LiftCatcher),
        "crab {:?}",
        sim.crab(id).unwrap().mover.pos
    );
    let target = Vec2::new(6416.0, -3172.0);
    let mut placed = false;
    for _ in 0..600 {
        sim.set_controls(
            id,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
        sim.tick();
        if sim.canyon_action(id, CanyonAction::PlaceCatcher(target)) {
            placed = true;
            break;
        }
    }
    assert!(
        placed,
        "could not reach a valid upper placement: {:?}",
        sim.crab(id).unwrap().mover.pos
    );
    for _ in 0..300 {
        sim.set_controls(
            id,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
        if sim.crab(id).unwrap().mover.pos.x >= 6410.0 {
            break;
        }
    }
    for _ in 0..1600 {
        sim.set_controls(id, Controls::default());
        sim.tick();
        if sim
            .canyon
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .any(|slot| matches!(slot.phase, SlotPhase::Caught(_)))
        {
            return;
        }
    }
    panic!(
        "the naturally admitted body did not reach the staged catcher: crab {:?}; run {:?}",
        sim.crab(id).unwrap().mover.pos,
        sim.canyon
    );
}

#[test]
fn natural_dense_arrival_uses_the_lower_basic_valve() {
    use crabsim::{Controls, Sim};
    let id = PlayerId(0);
    let level = Level::test_level();
    let mut terrain = World::from_level(&level);
    assert!(terrain.follow_player(&level, 6390.0, -3240.0));
    let mut sim = Sim::new(Tuning::default(), level);
    assert!(sim.restore_cellular_terrain(terrain, 1));
    sim.add_crab(id, Vec2::new(6390.0, -3240.0));
    sim.salvage.as_mut().unwrap().world_id = 42;
    sim.canyon.as_mut().unwrap().world_id = 42;
    for _ in 0..300 {
        sim.set_controls(
            id,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
        if sim.crab(id).unwrap().mover.pos.x >= 6410.0 {
            break;
        }
    }
    for _ in 0..1200 {
        sim.set_controls(id, Controls::default());
        sim.tick();
        if sim
            .canyon
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .any(|slot| matches!(slot.phase, SlotPhase::Caught(_)))
        {
            return;
        }
    }
    panic!(
        "natural dense arrival missed lower catcher: crab {:?}; run {:?}",
        sim.crab(id).unwrap().mover.pos,
        sim.canyon
    );
}

#[test]
fn ten_thousand_crossings_keep_fixed_records_and_distinct_generations() {
    let (mut level, tuning, world, mut run) = setup();
    let mut tick = 0u64;
    let mut previous = None;
    for _ in 0..10_000 {
        level.tide = 0.0;
        for _ in 0..121 {
            run.step(tick, &world, &level, &tuning);
            tick += 1;
        }
        level.tide = 1.0;
        run.step(tick, &world, &level, &tuning);
        tick += 1;
        let (index, body) = run
            .slots
            .iter()
            .enumerate()
            .find_map(|(i, s)| match s.phase {
                SlotPhase::Free(body) => Some((i, body)),
                _ => None,
            })
            .expect("one arrival per crossing");
        let claim = run
            .request(
                index,
                PlayerId(0),
                77,
                body.position + Vec2::new(0.0, 15.0),
                true,
                &world,
            )
            .unwrap();
        assert_ne!(previous, Some(claim));
        assert!(run.offer(claim));
        assert!(run.finalize(claim, PlayerId(0), 77));
        let revision = run.slots[index].revision;
        assert!(run.confirm_finalized_save(claim, revision));
        assert!(run.acknowledge_retired_receipt(claim, PlayerId(0), 77));
        let revision = run.slots[index].revision;
        assert!(run.retire_durable(claim, revision));
        previous = Some(claim);
    }
    assert_eq!(run.slots.len(), 2);
    assert!(run.complete());
    let records = run.records();
    assert!(records.len() <= 5);
    assert!(postcard::to_allocvec(&records).unwrap().len() < 4096);
}

#[test]
fn two_crabs_contest_one_generation_and_session_replay_is_exact() {
    use crabsim::{
        Carry, Replay, Sim,
        canyon_drift::{CanyonAction, DriftBody, DriftProfile, run::GrantPhase},
        replay::{CellSessionAction, CellSessionRecording, CellSessionTick},
    };
    let host = PlayerId(0);
    let guest = PlayerId(7);
    let level = Level::test_level();
    let mut terrain = World::from_level(&level);
    assert!(terrain.follow_player(&level, 6380.0, -3200.0));
    let mut sim = Sim::new(Tuning::default(), level);
    assert!(sim.restore_cellular_terrain(terrain, 1));
    sim.add_crab(host, Vec2::new(6380.0, -3200.0));
    let mut guest_progress = sim.fresh_progress();
    guest_progress.salvage_identity = 88;
    assert!(sim.join_cell_player(guest, Vec2::new(6370.0, -3200.0), &guest_progress));
    sim.crab_mut(host).unwrap().salvage_identity = 77;
    sim.salvage.as_mut().unwrap().world_id = 42;
    sim.canyon.as_mut().unwrap().world_id = 42;
    let from = sim.crab(host).unwrap().mover.pos
        + Vec2::Y * (sim.crab(host).unwrap().mover.height(&sim.tuning) * 0.5);
    let body = DriftBody {
        profile: DriftProfile::Dense,
        position: from + Vec2::new(10.0, 0.0),
        velocity: Vec2::ZERO,
    };
    let run = sim.canyon.as_mut().unwrap();
    run.slots[0].generation = 1;
    run.slots[0].location = body.position;
    run.slots[0].phase = SlotPhase::Free(body);
    let checkpoint = sim.session_checkpoint().unwrap();
    let mut recording = CellSessionRecording::new(&checkpoint, host, guest).unwrap();
    let mut first = CellSessionTick::default();
    first.actions[0] = Some((host, CellSessionAction::Canyon(CanyonAction::Take(0))));
    first.actions[1] = Some((guest, CellSessionAction::Canyon(CanyonAction::Take(0))));
    assert!(recording.note(sim.tick_count(), first));
    assert!(sim.tick_cell_session(host, guest, &first));
    let grant = match sim.canyon.as_ref().unwrap().slots[0].phase {
        SlotPhase::Grant(g) => g,
        other => panic!("{other:?}"),
    };
    assert_eq!(grant.player, host);
    assert!(sim.crab(guest).unwrap().mover.clinging.is_empty());
    fn step(
        sim: &mut Sim,
        recording: &mut CellSessionRecording,
        host: PlayerId,
        guest: PlayerId,
        action: CanyonAction,
    ) {
        let mut frame = CellSessionTick::default();
        frame.actions[0] = Some((host, CellSessionAction::Canyon(action)));
        assert!(recording.note(sim.tick_count(), frame));
        assert!(sim.tick_cell_session(host, guest, &frame));
    }
    step(
        &mut sim,
        &mut recording,
        host,
        guest,
        CanyonAction::Offer(grant.id),
    );
    step(
        &mut sim,
        &mut recording,
        host,
        guest,
        CanyonAction::Grant(grant.id),
    );
    assert_eq!(sim.crab(host).unwrap().mover.clinging, [Carry::Grit]);
    step(
        &mut sim,
        &mut recording,
        host,
        guest,
        CanyonAction::Finalize(grant.id),
    );
    let revision = sim.canyon.as_ref().unwrap().slots[0].revision;
    step(
        &mut sim,
        &mut recording,
        host,
        guest,
        CanyonAction::ConfirmFinalized(grant.id, revision),
    );
    let proof = match sim.canyon.as_ref().unwrap().slots[0].phase {
        SlotPhase::Grant(g) => g,
        other => panic!("{other:?}"),
    };
    assert_eq!(proof.phase, GrantPhase::FinalizedSaved);
    step(
        &mut sim,
        &mut recording,
        host,
        guest,
        CanyonAction::ForgetReceipt(proof),
    );
    step(
        &mut sim,
        &mut recording,
        host,
        guest,
        CanyonAction::AcknowledgeReceipt(grant.id),
    );
    let revision = sim.canyon.as_ref().unwrap().slots[0].revision;
    step(
        &mut sim,
        &mut recording,
        host,
        guest,
        CanyonAction::Retire(grant.id, revision),
    );
    assert_eq!(
        sim.canyon.as_ref().unwrap().slots[0].phase,
        SlotPhase::Vacant
    );
    assert!(sim.crab(host).unwrap().drift_receipts.is_empty());
    let replay = Replay::parse(&recording.to_text().unwrap()).unwrap();
    let mut restored = Sim::with_one_crab(crabsim::level::SPAWN);
    assert!(replay.prepare_start(&mut restored));
    for _ in 0..replay.length() {
        assert!(replay.step_cell_session(&mut restored));
    }
    assert_eq!(restored.state_hash(), sim.state_hash());
}

#[test]
fn guest_receipt_survives_reconnect_without_a_second_commodity() {
    use crabsim::{
        Carry, Sim,
        canyon_drift::{CanyonAction, DriftBody, DriftProfile},
    };
    let host = PlayerId(0);
    let guest = PlayerId(7);
    let level = Level::test_level();
    let mut terrain = World::from_level(&level);
    assert!(terrain.follow_player(&level, 6380.0, -3200.0));
    let mut sim = Sim::new(Tuning::default(), level.clone());
    assert!(sim.restore_cellular_terrain(terrain.clone(), 1));
    sim.add_crab(host, Vec2::new(6340.0, -3200.0));
    sim.crab_mut(host).unwrap().salvage_identity = 11;
    let mut progress = sim.fresh_progress();
    progress.salvage_identity = 22;
    assert!(sim.join_cell_player(guest, Vec2::new(6380.0, -3200.0), &progress));
    sim.salvage.as_mut().unwrap().world_id = 42;
    sim.canyon.as_mut().unwrap().world_id = 42;
    let from = sim.crab(guest).unwrap().mover.pos
        + Vec2::Y * (sim.crab(guest).unwrap().mover.height(&sim.tuning) * 0.5);
    let body = DriftBody {
        profile: DriftProfile::Dense,
        position: from + Vec2::new(10.0, 0.0),
        velocity: Vec2::ZERO,
    };
    let run = sim.canyon.as_mut().unwrap();
    run.slots[0].generation = 1;
    run.slots[0].location = body.position;
    run.slots[0].phase = SlotPhase::Free(body);
    assert!(sim.canyon_action(guest, CanyonAction::Take(0)));
    let claim = match sim.canyon.as_ref().unwrap().slots[0].phase {
        SlotPhase::Grant(g) => g.id,
        other => panic!("{other:?}"),
    };
    assert!(sim.canyon_action(host, CanyonAction::Offer(claim)));
    assert!(sim.canyon_action(guest, CanyonAction::Grant(claim)));
    let saved_guest = sim.progress(guest).unwrap();
    let saved_world = sim.canyon_records();
    let mut resumed = Sim::new(Tuning::default(), level);
    assert!(resumed.restore_cellular_terrain(terrain, 1));
    resumed.add_crab(host, Vec2::new(6340.0, -3200.0));
    resumed.crab_mut(host).unwrap().salvage_identity = 11;
    resumed.salvage.as_mut().unwrap().world_id = 42;
    assert!(resumed.restore_canyon_records(saved_world));
    let returning = PlayerId(9);
    assert!(resumed.join_cell_player(returning, Vec2::new(6380.0, -3200.0), &saved_guest));
    assert!(resumed.canyon_action(returning, CanyonAction::Grant(claim)));
    assert_eq!(
        resumed.crab(returning).unwrap().mover.clinging,
        [Carry::Grit]
    );
    assert!(resumed.canyon_action(returning, CanyonAction::Finalize(claim)));
    let revision = resumed.canyon.as_ref().unwrap().slots[0].revision;
    assert!(resumed.canyon_action(host, CanyonAction::ConfirmFinalized(claim, revision)));
    let proof = match resumed.canyon.as_ref().unwrap().slots[0].phase {
        SlotPhase::Grant(g) => g,
        other => panic!("{other:?}"),
    };
    assert!(resumed.canyon_action(returning, CanyonAction::ForgetReceipt(proof)));
    assert!(resumed.canyon_action(returning, CanyonAction::AcknowledgeReceipt(claim)));
    let revision = resumed.canyon.as_ref().unwrap().slots[0].revision;
    assert!(resumed.canyon_action(host, CanyonAction::Retire(claim, revision)));
    assert!(resumed.crab(returning).unwrap().drift_receipts.is_empty());
    assert_eq!(
        resumed.crab(returning).unwrap().mover.clinging,
        [Carry::Grit]
    );
}

#[test]
fn solo_crab_reaches_reusable_practice_from_near_canyon_ledge() {
    use crabsim::{Controls, Sim, canyon_drift::CanyonAction};
    let id = PlayerId(0);
    let level = Level::test_level();
    let mut terrain = World::from_level(&level);
    assert!(terrain.follow_player(&level, 6050.0, -3400.0));
    let mut sim = Sim::new(Tuning::default(), level);
    assert!(sim.restore_cellular_terrain(terrain, 1));
    sim.add_crab(id, Vec2::new(6050.0, -3400.0));
    sim.crab_mut(id).unwrap().salvage_identity = 77;
    sim.salvage.as_mut().unwrap().world_id = 42;
    sim.canyon.as_mut().unwrap().world_id = 42;
    for _ in 0..3600 {
        let x = sim.crab(id).unwrap().mover.pos.x;
        sim.set_controls(
            id,
            Controls {
                walk: if x < 6410.0 { 1.0 } else { 0.0 },
                ..Default::default()
            },
        );
        sim.tick();
        if sim.canyon_action(id, CanyonAction::LiftPractice) {
            return;
        }
    }
    panic!(
        "practice was not reachable from the ledge: crab {:?}, tick {}, dead {:?}",
        sim.crab(id).unwrap().mover.pos,
        sim.tick_count(),
        sim.crab(id).unwrap().fighter.dead
    );
}

#[test]
fn interrupted_handoff_rejects_duplicate_and_stale_canyon_owners() {
    use crabsim::canyon_drift::{DriftBody, DriftProfile};
    let (_, _, _, mut archive) = setup();
    let body = DriftBody {
        profile: DriftProfile::Dense,
        position: archive.catcher.position + Vec2::new(0.0, 7.0),
        velocity: Vec2::ZERO,
    };
    archive.slots[0].generation = 4;
    archive.slots[0].revision = 19;
    archive.slots[0].phase = SlotPhase::Caught(body);
    archive.slots[0].location = body.position;
    archive.catcher.contents = Some(0);
    let before = archive.clone();
    let mut preview = archive.clone();
    let transfer = preview.extract(|p| p.x >= 6400.0).unwrap();
    assert!(
        !archive.absorb(&transfer),
        "unacknowledged destination cannot create another owner"
    );
    assert_eq!(archive, before);
    assert!(archive.subtract(&transfer));
    let after_subtract = archive.clone();
    assert!(
        !archive.subtract(&transfer),
        "stale acknowledgement cannot remove it twice"
    );
    assert_eq!(archive, after_subtract);
    assert!(archive.absorb(&transfer));
    assert_eq!(archive, before);
}

#[test]
fn full_inventory_can_send_an_ordinary_piece_out_without_a_reward() {
    use crabsim::canyon_drift::{
        DriftBody, DriftProfile,
        run::{ANCHOR, DriftClaimId},
    };
    let (_, _, world, mut run) = setup();
    let body = DriftBody {
        profile: DriftProfile::Dense,
        position: Vec2::new(6420.0, -3236.0),
        velocity: Vec2::ZERO,
    };
    run.slots[0].generation = 3;
    run.slots[0].location = body.position;
    run.slots[0].phase = SlotPhase::Free(body);
    let from = Vec2::new(6400.0, -3205.0);
    assert!(
        run.request(0, PlayerId(0), 77, from, false, &world)
            .is_none()
    );
    assert!(run.release_outflow(0, from, &world));
    assert_eq!(run.slots[0].phase, SlotPhase::WashedOut);
    let id = DriftClaimId {
        world: 42,
        anchor: ANCHOR,
        slot: 0,
        generation: 3,
    };
    let revision = run.slots[0].revision;
    assert!(!run.retire_outflow_durable(id, revision + 1));
    assert!(run.retire_outflow_durable(id, revision));
    assert_eq!(run.slots[0].phase, SlotPhase::Vacant);
    assert!(!run.retire_outflow_durable(id, revision));
}

#[test]
fn full_inventory_can_release_the_non_rewarding_practice_piece() {
    use crabsim::{
        Carry, Sim,
        canyon_drift::{CanyonAction, run::PracticePhase},
    };
    let id = PlayerId(0);
    let level = Level::test_level();
    let mut terrain = World::from_level(&level);
    assert!(terrain.follow_player(&level, 6340.0, -3200.0));
    let mut sim = Sim::new(Tuning::default(), level);
    assert!(sim.restore_cellular_terrain(terrain, 1));
    sim.add_crab(id, Vec2::new(6340.0, -3200.0));
    sim.crab_mut(id).unwrap().salvage_identity = 77;
    sim.salvage.as_mut().unwrap().world_id = 42;
    sim.canyon.as_mut().unwrap().world_id = 42;
    let mut depleted = sim.progress(id).unwrap();
    depleted.eaten.fill(true);
    depleted.cultures_taken.fill(true);
    depleted.clues_found.fill(true);
    depleted.teeth_taken.fill(true);
    depleted.nets_cut.fill(true);
    depleted.rocks_cracked.fill(true);
    depleted.grit_taken.fill(true);
    depleted.sponges_taken.fill(true);
    depleted.scissor_claw = true;
    depleted.glass_grips = true;
    depleted.clam_guards = true;
    depleted.feats = vec![
        crabsim::moult::Feat::GrippingDactyls,
        crabsim::moult::Feat::BroadPaddles,
        crabsim::moult::Feat::HeavyCrusher,
    ];
    depleted.larder = sim.tuning.food.larder;
    sim.restore_progress(id, &depleted);
    sim.crab_mut(id).unwrap().mover.carrying = Some(Carry::Glass);
    let slots = sim.crab(id).unwrap().mover.carries(&sim.tuning);
    sim.crab_mut(id).unwrap().mover.clinging = vec![Carry::Grit; slots];
    let checkpoint = sim.session_checkpoint().unwrap();
    let mut sim = Sim::from_session_checkpoint(&checkpoint).unwrap();
    assert!(sim.canyon_action(id, CanyonAction::ReleasePractice));
    assert_eq!(
        sim.canyon.as_ref().unwrap().practice_phase,
        PracticePhase::Free
    );
    assert_eq!(sim.crab(id).unwrap().mover.carrying, Some(Carry::Glass));
    assert_eq!(sim.crab(id).unwrap().mover.clinging.len(), slots);
    assert!(sim.crab(id).unwrap().drift_receipts.is_empty());
}

#[test]
fn caught_practice_can_be_released_without_any_inventory_capacity() {
    use crabsim::canyon_drift::run::PracticePhase;
    let (_, _, world, mut run) = setup();
    run.practice.position = run.catcher.position + Vec2::new(0.0, 7.0);
    run.practice_phase = PracticePhase::Caught;
    run.catcher.contents = Some(2);
    assert!(run.valid());
    let from = run.practice.position + Vec2::new(-20.0, 25.0);
    assert!(run.release_practice(from, &world));
    assert_eq!(run.practice_phase, PracticePhase::Free);
    assert_eq!(run.catcher.contents, None);
    assert!(run.valid());
}

#[test]
fn loaded_catcher_stays_available_when_the_shell_is_full() {
    use crabsim::{
        Carry, Sim,
        canyon_drift::{CanyonAction, DriftBody, DriftProfile},
    };
    let id = PlayerId(0);
    let level = Level::test_level();
    let mut terrain = World::from_level(&level);
    assert!(terrain.follow_player(&level, 6390.0, -3240.0));
    let mut sim = Sim::new(Tuning::default(), level);
    assert!(sim.restore_cellular_terrain(terrain, 1));
    sim.add_crab(id, Vec2::new(6390.0, -3240.0));
    sim.crab_mut(id).unwrap().salvage_identity = 77;
    sim.salvage.as_mut().unwrap().world_id = 42;
    sim.canyon.as_mut().unwrap().world_id = 42;
    let run = sim.canyon.as_mut().unwrap();
    let body = DriftBody {
        profile: DriftProfile::Dense,
        position: run.catcher.position + Vec2::new(0.0, 7.0),
        velocity: Vec2::ZERO,
    };
    run.slots[0].generation = 1;
    run.slots[0].location = body.position;
    run.slots[0].phase = SlotPhase::Caught(body);
    run.catcher.contents = Some(0);
    let capacity = sim.crab(id).unwrap().mover.carries(&sim.tuning);
    sim.crab_mut(id).unwrap().mover.clinging = vec![Carry::Grit; capacity];
    assert!(!sim.canyon_action(id, CanyonAction::LiftCatcher));
    assert_eq!(sim.canyon.as_ref().unwrap().catcher.contents, Some(0));
    assert!(matches!(
        sim.canyon.as_ref().unwrap().slots[0].phase,
        SlotPhase::Caught(_)
    ));
    sim.crab_mut(id).unwrap().mover.clinging.pop();
    assert!(sim.canyon_action(id, CanyonAction::LiftCatcher));
    assert_eq!(sim.canyon.as_ref().unwrap().catcher.contents, Some(0));
}

#[test]
fn practice_pass_stays_physical_and_reaches_a_next_decision() {
    use crabsim::canyon_drift::run::PracticePhase;
    let (level, tuning, mut world, mut run) = setup();
    let from = run.practice.position + Vec2::new(-50.0, 25.0);
    assert!(run.release_practice(from, &world));
    let start = run.practice.position;
    for tick in 0..960 {
        if tick % 4 == 0 {
            world.step(0.35);
        }
        run.step(tick, &world, &level, &tuning);
        if matches!(
            run.practice_phase,
            PracticePhase::Settled | PracticePhase::Caught
        ) && run.practice.position.distance(start) > 4.0
        {
            assert!(run.valid());
            let from = run.practice.position + Vec2::new(0.0, 25.0);
            assert!(
                run.release_practice(from, &world),
                "the same practice body must be immediately reusable"
            );
            return;
        }
    }
    panic!(
        "practice did not reach a new decision in eight seconds: {:?} {:?}",
        run.practice_phase, run.practice
    );
}

#[test]
fn second_piece_remains_physical_when_the_catcher_fills() {
    use crabsim::canyon_drift::{DriftBody, DriftProfile};
    let (level, tuning, mut world, mut run) = setup();
    run.has_anchor = false;
    for (i, y) in [-3204.0, -3196.0].into_iter().enumerate() {
        let body = DriftBody {
            profile: DriftProfile::Dense,
            position: Vec2::new(6388.0, y),
            velocity: Vec2::ZERO,
        };
        run.slots[i].generation = 1;
        run.slots[i].location = body.position;
        run.slots[i].phase = SlotPhase::Free(body);
    }
    for tick in 0..1200 {
        if tick % 4 == 0 {
            world.step(0.35);
        }
        run.step(tick, &world, &level, &tuning);
    }
    assert_eq!(run.catcher.contents, Some(0));
    assert!(matches!(run.slots[0].phase, SlotPhase::Caught(_)));
    let second = match run.slots[1].phase {
        SlotPhase::Free(body) => body,
        other => panic!("second piece disappeared: {other:?}"),
    };
    assert!(world.contains_world(second.position.x, second.position.y));
    assert!(run.valid());
}
