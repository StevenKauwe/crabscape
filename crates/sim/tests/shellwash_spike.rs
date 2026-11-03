//! Gameplay, ownership and replay checks for the opening salvage site.
use bevy_math::Vec2;
use crabsim::Sim;

#[test]
fn free_shell_can_be_caught_or_missed_without_disappearing() {
    use crabsim::salvage::{FindPhase, Shellwash, ValvePhase};
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    let world = sim.level.cells.as_ref().unwrap();
    let mut caught = Shellwash {
        phase: FindPhase::Free,
        position: Vec2::new(351.0, 35.0),
        valve_position: Vec2::new(360.0, 15.0),
        ..Default::default()
    };
    let mut missed = Shellwash {
        phase: FindPhase::Free,
        position: Vec2::new(310.0, 55.0),
        valve_position: Vec2::new(360.0, 15.0),
        ..Default::default()
    };
    for _ in 0..600 {
        caught.step(world);
        missed.step(world);
    }
    assert_eq!(caught.valve_phase, ValvePhase::Placed);
    assert_eq!(caught.phase, FindPhase::Caught);
    assert_ne!(missed.phase, FindPhase::Caught);
    assert_ne!(missed.phase, FindPhase::Consumed);
    assert!(world.contains_world(missed.position.x, missed.position.y));
}

#[test]
fn claim_and_crush_are_exclusive_and_require_exposure() {
    use crabsim::{
        PlayerId,
        salvage::{FindPhase, Shellwash},
    };
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    let world = sim.level.cells.as_ref().unwrap();
    let mut buried = Shellwash {
        position: Vec2::new(300.0, -20.0),
        ..Default::default()
    };
    assert!(!buried.take(world, PlayerId(0), 1, Vec2::new(300.0, 40.0), true));
    assert!(!buried.crush(world, Vec2::new(300.0, 40.0)));
    let still_embedded = Shellwash {
        position: Vec2::new(300.0, 64.0),
        ..Default::default()
    };
    assert!(still_embedded.exposed(world));
    assert!(
        !still_embedded.can_take(world, Vec2::new(300.0, 96.0)),
        "an exposed seam still needs its support removed"
    );
    let mut exposed = Shellwash {
        world_id: 42,
        phase: FindPhase::Settled,
        position: Vec2::new(300.0, 64.0),
        ..Default::default()
    };
    assert!(exposed.crush(world, Vec2::new(300.0, 96.0)));
    assert_eq!(exposed.phase, FindPhase::Consumed);
    assert_eq!(exposed.fragments, [true, true]);
    for p in exposed.fragment_positions {
        assert!(!world.game_solid(p.x - 3.0, p.y - 3.0, p.x + 3.0, p.y + 3.0));
    }
    assert!(exposed.request_fragment(world, 0, Vec2::new(310.0, 96.0), PlayerId(1), 2, true));
    assert!(!exposed.take(world, PlayerId(0), 1, Vec2::new(300.0, 96.0), true));
    assert!(!exposed.crush(world, Vec2::new(300.0, 96.0)));
}

#[test]
fn contested_claim_has_one_winner_and_display_is_reversible() {
    use crabsim::{
        PlayerId,
        salvage::{FindPhase, Shellwash},
    };
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    let world = sim.level.cells.as_ref().unwrap();
    let mut shell = Shellwash {
        phase: FindPhase::Settled,
        position: Vec2::new(300.0, 64.0),
        ..Default::default()
    };
    let from = Vec2::new(300.0, 96.0);
    assert!(!shell.take(world, PlayerId(0), 1, from, false));
    assert!(shell.take(world, PlayerId(0), 1, from, true));
    assert!(!shell.take(world, PlayerId(1), 2, from, true));
    assert!(!shell.display(PlayerId(1), true));
    assert!(shell.display(PlayerId(0), true));
    assert_eq!(shell.phase, FindPhase::Displayed);
    assert_eq!(shell.fragment_positions, [shell.position; 2]);
    assert!(shell.retrieve_display(PlayerId(1), 2, true, true));
    assert!(shell.drop_carried(PlayerId(1), Vec2::new(300.0, 96.0)));
    assert_eq!(shell.phase, FindPhase::Free);
}

#[test]
fn valve_does_not_catch_a_shell_crossing_too_fast() {
    use crabsim::salvage::{FindPhase, Shellwash};
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    let world = sim.level.cells.as_ref().unwrap();
    let mut shell = Shellwash {
        phase: FindPhase::Free,
        position: Vec2::new(320.0, 27.0),
        velocity: Vec2::new(0.0, -46.0),
        valve_position: Vec2::new(320.0, 17.0),
        ..Default::default()
    };
    for _ in 0..30 {
        shell.step(world);
    }
    assert!(
        shell.mouth_missed,
        "must cross a supported valve too fast before missing"
    );
    assert_ne!(shell.phase, FindPhase::Caught);
    assert_ne!(shell.phase, FindPhase::Consumed);
}

#[test]
fn a_real_crab_dig_can_release_the_bank_find() {
    use crabsim::{Controls, PlayerId, salvage::Shellwash};
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(260.0, 80.0));
    sim.enable_underwater_cells();
    let mut shell = Shellwash::default();
    let mut released = false;
    for _ in 0..900 {
        sim.set_controls(
            id,
            Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            },
        );
        sim.tick();
        let event = shell.step(sim.level.cells.as_ref().unwrap());
        released |= event.released;
        if released {
            break;
        }
    }
    assert!(
        released,
        "normal digging must uncover support: crab {:?}; shell {shell:?}",
        sim.crab(id).unwrap().mover.pos
    );
}

#[test]
fn split_authority_and_records_do_not_duplicate_either_object() {
    use crabsim::salvage::{FindPhase, Shellwash, valid_records};
    let mut archive = Shellwash {
        position: Vec2::new(200.0, 20.0),
        fragment_positions: [Vec2::new(200.0, 20.0); 2],
        valve_position: Vec2::new(500.0, 20.0),
        ..Default::default()
    };
    let mut left = archive.extract(|p| p.x < 256.0).unwrap();
    let right = archive.extract(|p| p.x >= 256.0).unwrap();
    assert!(left.has_find && !left.has_valve);
    assert!(!right.has_find && right.has_valve);
    assert!(!archive.has_find && !archive.has_valve);
    assert!(left.absorb(&right));
    assert!(valid_records(&left.records()));
    assert!(!left.absorb(&right), "same valve cannot be granted twice");
    left.phase = FindPhase::Caught;
    left.position = left.valve_position + Vec2::new(0.0, 9.0);
    left.fragment_positions = [left.position; 2];
    let caught = left.extract(|p| p.x >= 256.0).unwrap();
    assert!(
        caught.has_find && caught.has_valve,
        "caught shell follows valve owner"
    );
    let mut records = caught.records();
    assert!(valid_records(&records));
    records
        .iter_mut()
        .find_map(|r| r.find.as_mut())
        .unwrap()
        .position
        .x += 20.0;
    assert!(
        !valid_records(&records),
        "caught contents must stay in their valve"
    );
}

#[test]
fn complete_spatial_records_restore_atomically_and_reject_bad_keys() {
    use crabsim::salvage::{FindPhase, valid_records};
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    let site = sim.salvage.as_mut().unwrap();
    site.phase = FindPhase::Displayed;
    site.position = Vec2::new(95.0, 35.0);
    site.fragment_positions = [site.position; 2];
    site.valve_position = Vec2::new(500.0, 15.0);
    let records = sim.salvage_records();
    assert_eq!(records.len(), 2);
    assert!(valid_records(&records));
    let before = sim.salvage.clone();
    let mut damaged = records.clone();
    damaged[0].region_id.0 += 1;
    assert!(!sim.restore_salvage_records(damaged));
    assert_eq!(sim.salvage, before, "rejected restoration is atomic");
    assert!(sim.restore_salvage_records(records));
    assert_eq!(sim.salvage, before);
}

#[test]
fn live_dig_released_shell_ends_in_recoverable_opening() {
    use crabsim::{
        Controls, PlayerId,
        salvage::{FindPhase, Shellwash},
    };
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(260.0, 80.0));
    sim.enable_underwater_cells();
    let mut shell = Shellwash::default();
    let mut released = false;
    for tick in 0..1800 {
        let controls = if tick < 500 {
            Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            }
        } else if tick < 1000 {
            Controls {
                dig_held: true,
                walk: 1.0,
                ..Default::default()
            }
        } else {
            Controls::default()
        };
        sim.set_controls(id, controls);
        sim.tick();
        let event = shell.step(sim.level.cells.as_ref().unwrap());
        released |= event.released;
    }
    assert!(released);
    assert!(matches!(
        shell.phase,
        FindPhase::Free | FindPhase::Settled | FindPhase::Caught
    ));
    assert!(shell.position.x >= 200.0 && shell.position.x <= 500.0 && shell.position.y > -150.0);
}

#[test]
fn authored_bank_keeps_a_walkable_return_route() {
    use crabsim::{Controls, PlayerId};
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    for _ in 0..360 {
        sim.set_controls(
            id,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
        if sim.crab(id).unwrap().mover.pos.x > 440.0 {
            break;
        }
    }
    assert!(
        sim.crab(id).unwrap().mover.pos.x > 440.0,
        "bank blocks the ordinary route: {:?}",
        sim.crab(id).unwrap().mover.pos
    );
}

#[test]
fn fragment_receipt_requires_owner_offer_and_is_idempotent() {
    use crabsim::{
        PlayerId,
        salvage::{ClaimPhase, FindPhase, SalvageAction},
    };
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(260.0, 70.0));
    sim.enable_underwater_cells();
    sim.crab_mut(id).unwrap().salvage_identity = 77;
    {
        let site = sim.salvage.as_mut().unwrap();
        site.world_id = 42;
        site.phase = FindPhase::Consumed;
        site.fragments = [true, true];
        site.fragment_positions = [Vec2::new(260.0, 80.0), Vec2::new(264.0, 80.0)];
    }
    assert!(sim.salvage_action(id, SalvageAction::TakeFragment(0)));
    let claim = sim.salvage.as_ref().unwrap().claims[0].unwrap();
    assert_eq!(claim.phase, ClaimPhase::Requested);
    assert!(sim.crab(id).unwrap().mover.clinging.is_empty());
    assert!(!sim.salvage_action(id, SalvageAction::GrantFragment(0, claim.id)));
    assert!(sim.salvage_action(id, SalvageAction::OfferFragment(0, claim.id)));
    assert!(sim.salvage_action(id, SalvageAction::GrantFragment(0, claim.id)));
    assert!(sim.salvage_action(id, SalvageAction::GrantFragment(0, claim.id)));
    assert_eq!(sim.crab(id).unwrap().mover.clinging.len(), 1);
    assert_eq!(sim.progress(id).unwrap().salvage_receipts, vec![claim.id]);
    assert!(sim.salvage_action(id, SalvageAction::FinalizeFragment(0, claim.id)));
}

#[test]
fn supported_valve_catches_a_real_dig_release() {
    use crabsim::{
        Controls, PlayerId,
        salvage::{FindPhase, SalvageAction},
    };
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(300.0, 30.0));
    sim.enable_underwater_cells();
    sim.crab_mut(id).unwrap().salvage_identity = 1;
    assert!(
        sim.salvage_action(id, SalvageAction::LiftValve),
        "crab {:?}",
        sim.crab(id).unwrap().mover.pos
    );
    for _ in 0..120 {
        if sim.crab(id).unwrap().mover.pos.x <= 267.5 {
            break;
        }
        sim.set_controls(
            id,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
        sim.tick();
    }
    sim.set_controls(
        id,
        Controls {
            walk: 1.0,
            ..Default::default()
        },
    );
    sim.tick();
    let crab = sim.crab(id).unwrap();
    let x = crab.mover.pos.x + 65.0 * crab.fighter.facing.signum();
    let y = sim
        .level
        .cells
        .as_ref()
        .unwrap()
        .game_surface_below(x, 16.0, crab.mover.pos.y + 60.0)
        .unwrap()
        + 3.0;
    let target = Vec2::new(x, y);
    assert!(
        sim.salvage_action(id, SalvageAction::PlaceValve(target)),
        "crab {:?}, target {target:?}",
        sim.crab(id).unwrap().mover.pos
    );
    let mut released = false;
    let mut caught = false;
    for tick in 0..5000 {
        let controls = if tick < 500 {
            Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            }
        } else if tick < 1000 {
            Controls {
                dig_held: true,
                walk: 1.0,
                ..Default::default()
            }
        } else {
            Controls::default()
        };
        sim.set_controls(id, controls);
        sim.tick();
        let phase = sim.salvage.as_ref().unwrap().phase;
        released |= phase != FindPhase::Embedded;
        caught |= phase == FindPhase::Caught;
        if caught {
            break;
        }
    }
    assert!(released);
    assert!(caught, "shell={:?}", sim.salvage);
}

#[test]
fn solo_salvage_action_round_trips_in_replay() {
    use crabsim::{PlayerId, Replay, salvage::SalvageAction};
    let id = PlayerId(0);
    let mut live = Sim::with_one_crab(Vec2::new(320.0, 30.0));
    live.enable_underwater_cells();
    live.crab_mut(id).unwrap().salvage_identity = 77;
    live.salvage.as_mut().unwrap().world_id = 42;
    live.canyon.as_mut().unwrap().world_id = 42;
    live.start_recording();
    assert!(live.salvage_action(id, SalvageAction::LiftValve));
    for _ in 0..10 {
        live.tick();
    }
    let replay = Replay::parse(&live.recording().unwrap().to_text()).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(again.salvage, live.salvage);
    assert_eq!(replay.hash, Some(again.state_hash()));
}

#[test]
fn four_independent_objects_round_trip_without_page_clamping() {
    use crabsim::salvage::{FindPhase, Shellwash, valid_records};
    let site = Shellwash {
        world_id: 42,
        phase: FindPhase::Consumed,
        position: Vec2::new(10.0, 10.0),
        valve_position: Vec2::new(300.0, 10.0),
        fragments: [true, true],
        fragment_positions: [Vec2::new(600.0, 10.0), Vec2::new(900.0, 10.0)],
        ..Default::default()
    };
    let records = site.records();
    assert_eq!(records.len(), 4);
    assert!(valid_records(&records));
    let mut lost = records.clone();
    lost.iter_mut()
        .find_map(|r| r.fragments[0].as_mut())
        .unwrap()
        .active = false;
    assert!(!valid_records(&lost), "an unclaimed child cannot disappear");
    let mut sim = Sim::with_one_crab(Vec2::ZERO);
    sim.enable_underwater_cells();
    assert!(sim.restore_salvage_records(records.clone()));
    assert_eq!(sim.salvage_records(), records);
    let mut duplicate = records;
    let copy = duplicate
        .iter()
        .find_map(|r| r.fragments[0].clone())
        .unwrap();
    let page = duplicate
        .iter_mut()
        .find(|r| r.fragments[0].is_none())
        .unwrap();
    page.fragments[0] = Some(copy);
    assert!(!valid_records(&duplicate));
}

#[test]
fn physical_fragment_crosses_page_and_transfers_as_one_identity() {
    use crabsim::salvage::{FindPhase, Shellwash};
    let mut sim = Sim::with_one_crab(Vec2::new(200.0, 100.0));
    sim.enable_underwater_cells();
    let mut site = Shellwash {
        phase: FindPhase::Consumed,
        fragments: [true, true],
        fragment_positions: [Vec2::new(511.5, 600.0), Vec2::new(500.0, 600.0)],
        fragment_velocities: [Vec2::new(30.0, 0.0), Vec2::ZERO],
        ..Default::default()
    };
    for _ in 0..30 {
        site.step(sim.level.cells.as_ref().unwrap());
    }
    assert!(
        site.fragment_positions[0].x > 512.0,
        "{:?}",
        site.fragment_positions[0]
    );
    let mut moved = site
        .extract(|p| p.x >= 512.0)
        .expect("fragment crosses the boundary");
    assert!(moved.has_fragments[0]);
    assert!(!site.has_fragments[0]);
    assert!(!moved.has_fragments[1]);
    let mut foreign = moved.clone();
    foreign.world_id = 77;
    let before = site.clone();
    assert!(!site.absorb(&foreign));
    assert_eq!(site, before, "foreign-world rejection is atomic");
    assert!(site.absorb(&moved));
    assert!(!site.absorb(&moved), "duplicate ownership must be rejected");
    moved.has_fragments[0] = false;
    assert!(moved.records().is_empty());
}

#[test]
fn a_departing_carrier_releases_the_valve() {
    use crabsim::{
        PlayerId,
        salvage::{SalvageAction, ValvePhase},
    };
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(300.0, 30.0));
    sim.enable_underwater_cells();
    sim.crab_mut(id).unwrap().salvage_identity = 1;
    assert!(sim.salvage_action(id, SalvageAction::LiftValve));
    assert!(sim.remove_cell_player(id));
    assert_eq!(sim.salvage.as_ref().unwrap().valve_phase, ValvePhase::Free);
}

#[test]
fn competing_guest_claim_and_reconnect_keep_one_durable_recipient() {
    use crabsim::{
        PlayerId,
        salvage::{ClaimPhase, FindPhase, SalvageAction},
    };
    let host = PlayerId(0);
    let guest = PlayerId(7);
    let mut sim = Sim::with_one_crab(Vec2::new(260.0, 70.0));
    sim.enable_underwater_cells();
    sim.crab_mut(host).unwrap().salvage_identity = 11;
    let mut guest_progress = sim.fresh_progress();
    guest_progress.salvage_identity = 22;
    assert!(sim.join_cell_player(guest, Vec2::new(260.0, 70.0), &guest_progress));
    let site = sim.salvage.as_mut().unwrap();
    site.world_id = 42;
    site.phase = FindPhase::Consumed;
    site.fragments = [true, true];
    site.fragment_positions = [Vec2::new(260.0, 80.0), Vec2::new(264.0, 80.0)];
    assert!(sim.salvage_action(guest, SalvageAction::TakeFragment(0)));
    assert!(!sim.salvage_action(host, SalvageAction::TakeFragment(0)));
    let claim = sim.salvage.as_ref().unwrap().claims[0].unwrap();
    assert!(sim.salvage_action(host, SalvageAction::OfferFragment(0, claim.id)));
    assert!(sim.remove_cell_player(guest));
    let returning = PlayerId(9);
    assert!(sim.join_cell_player(returning, Vec2::new(260.0, 70.0), &guest_progress));
    assert_eq!(
        sim.salvage.as_ref().unwrap().claims[0].unwrap().recipient,
        returning
    );
    assert!(sim.salvage_action(returning, SalvageAction::GrantFragment(0, claim.id)));
    assert!(sim.salvage_action(returning, SalvageAction::GrantFragment(0, claim.id)));
    assert_eq!(sim.crab(returning).unwrap().mover.clinging.len(), 1);
    assert!(sim.salvage_action(returning, SalvageAction::FinalizeFragment(0, claim.id)));
    assert_eq!(
        sim.salvage.as_ref().unwrap().claims[0].unwrap().phase,
        ClaimPhase::Finalized
    );
}

#[test]
fn a_region_without_the_find_cannot_display_or_duplicate_it() {
    use crabsim::{
        PlayerId,
        salvage::{FindPhase, Shellwash},
    };
    let mut site = Shellwash {
        has_find: false,
        phase: FindPhase::Displayed,
        ..Default::default()
    };
    assert!(!site.retrieve_display(PlayerId(0), 1, true, true));
    site.phase = FindPhase::Carried(PlayerId(0));
    assert!(!site.display(PlayerId(0), true));
    assert!(!site.drop_carried(PlayerId(0), Vec2::ZERO));
}

#[test]
fn a_gentle_shell_roll_into_the_valve_is_caught() {
    use crabsim::salvage::{FindPhase, Shellwash};
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    let mut site = Shellwash {
        phase: FindPhase::Free,
        position: Vec2::new(311.95, 34.0),
        velocity: Vec2::new(20.0, 0.0),
        valve_position: Vec2::new(324.0, 26.0),
        ..Default::default()
    };
    let events = site.step(sim.level.cells.as_ref().unwrap());
    assert!(
        events.caught,
        "gentle swept entry should meet the valve lip: {site:?}"
    );
}

#[test]
fn guest_durable_receipt_survives_owner_crash_before_offer_is_saved() {
    use crabsim::{
        PlayerId,
        salvage::{ClaimPhase, FindPhase, SalvageAction},
    };
    let host = PlayerId(0);
    let guest = PlayerId(7);
    let mut live = Sim::with_one_crab(Vec2::new(260.0, 70.0));
    live.enable_underwater_cells();
    live.crab_mut(host).unwrap().salvage_identity = 11;
    let mut guest_progress = live.fresh_progress();
    guest_progress.salvage_identity = 22;
    assert!(live.join_cell_player(guest, Vec2::new(260.0, 70.0), &guest_progress));
    let site = live.salvage.as_mut().unwrap();
    site.world_id = 42;
    site.phase = FindPhase::Consumed;
    site.fragments = [true, true];
    site.fragment_positions = [Vec2::new(260.0, 80.0), Vec2::new(264.0, 80.0)];
    assert!(live.salvage_action(guest, SalvageAction::TakeFragment(0)));
    let claim = live.salvage.as_ref().unwrap().claims[0].unwrap();
    let owner_durable_pending = live.salvage_records();
    assert!(live.salvage_action(host, SalvageAction::OfferFragment(0, claim.id)));
    assert!(live.salvage_action(guest, SalvageAction::GrantFragment(0, claim.id)));
    let guest_durable_receipt = live.progress(guest).unwrap();
    assert_eq!(guest_durable_receipt.salvage_receipts, [claim.id]);

    let mut resumed = Sim::with_one_crab(Vec2::new(260.0, 70.0));
    resumed.enable_underwater_cells();
    resumed.crab_mut(host).unwrap().salvage_identity = 11;
    assert!(resumed.restore_salvage_records(owner_durable_pending));
    let returning = PlayerId(9);
    assert!(resumed.join_cell_player(returning, Vec2::new(260.0, 70.0), &guest_durable_receipt));
    assert!(resumed.salvage_action(host, SalvageAction::OfferFragment(0, claim.id)));
    assert!(resumed.salvage_action(returning, SalvageAction::GrantFragment(0, claim.id)));
    assert_eq!(resumed.crab(returning).unwrap().mover.clinging.len(), 1);
    assert!(resumed.salvage_action(returning, SalvageAction::FinalizeFragment(0, claim.id)));
    assert_eq!(
        resumed.salvage.as_ref().unwrap().claims[0].unwrap().phase,
        ClaimPhase::Finalized
    );
}

#[test]
fn second_excavation_approach_still_catches() {
    use crabsim::{Controls, PlayerId};
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(249.0, 44.0));
    sim.enable_underwater_cells();
    sim.salvage.as_mut().unwrap().valve_position = Vec2::new(314.7, 31.0);
    for tick in 0..4500 {
        let controls = if tick < 500 {
            Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            }
        } else if tick < 1000 {
            Controls {
                dig_held: true,
                walk: 1.0,
                ..Default::default()
            }
        } else {
            Controls::default()
        };
        sim.set_controls(id, controls);
        sim.tick();
        if sim.salvage.as_ref().unwrap().phase == crabsim::salvage::FindPhase::Caught {
            break;
        }
    }
    assert_eq!(
        sim.salvage.as_ref().unwrap().phase,
        crabsim::salvage::FindPhase::Caught
    );
    assert!(sim.crab(id).unwrap().mover.pos.x > 400.0);
}

#[test]
fn dropping_a_caught_valve_keeps_its_contents_and_children_together() {
    use crabsim::{
        PlayerId,
        salvage::{FindPhase, Shellwash, ValvePhase, valid_records},
    };
    let mut site = Shellwash {
        phase: FindPhase::Caught,
        position: Vec2::new(320.0, 29.0),
        valve_phase: ValvePhase::Carried(PlayerId(0)),
        valve_position: Vec2::new(320.0, 20.0),
        fragment_positions: [Vec2::new(320.0, 29.0); 2],
        ..Default::default()
    };
    assert!(site.drop_valve(PlayerId(0), Vec2::new(600.0, 80.0)));
    assert_eq!(site.position, Vec2::new(600.0, 89.0));
    assert_eq!(site.fragment_positions, [site.position; 2]);
    assert!(valid_records(&site.records()));
}

#[test]
fn idle_opening_does_not_release_the_embedded_find() {
    use crabsim::salvage::FindPhase;
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    for _ in 0..3600 {
        sim.tick();
    }
    assert_eq!(sim.salvage.as_ref().unwrap().phase, FindPhase::Embedded);
}

#[test]
fn home_display_has_a_visible_surface_above_the_terrain() {
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    let world = sim.level.cells.as_ref().unwrap();
    let visible = (-2..=2)
        .flat_map(|x| (-1..=1).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let p = Vec2::new(95.0 + x as f32 * 4.0, 35.0 + y as f32 * 4.0);
            !world.game_solid(p.x - 1.0, p.y - 1.0, p.x + 1.0, p.y + 1.0)
        })
        .count();
    assert!(
        visible >= 8,
        "display should be visibly above the sediment, got {visible} pixels"
    );
}
