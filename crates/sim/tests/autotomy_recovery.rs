use bevy_math::Vec2;
use crabsim::{Controls, Kind, PlayerId, Shelter, Sim, Skill};
const P: PlayerId = PlayerId(0);
fn prepare_moult(s: &mut Sim, id: PlayerId) {
    let den = s.level.home_den;
    let fed = s.tuning.food.well_fed_ticks;
    let xp = crabsim::skills::xp_for(s.tuning.moult.feat_level, Skill::Climbing.step(&s.tuning));
    let m = &mut s.crab_mut(id).unwrap().mover;
    m.pos = den;
    m.shelter = Shelter::In;
    m.tucked = 1.;
    m.well_fed = fed;
    m.climb_xp = xp;
}
#[test]
fn actual_octopus_escape_loses_a_leg_until_completed_growth_moult() {
    let mut s = Sim::with_one_crab(Vec2::new(280., 0.));
    s.spawn_enemy(Kind::Octopus, Vec2::new(480., 0.));
    s.enemies_mut()[0].octopus_mut().unwrap().revealed = true;
    for _ in 0..400 {
        s.tick();
        if s.crab(P).unwrap().fighter.grabbed.is_some() {
            break;
        }
    }
    assert!(s.crab(P).unwrap().fighter.grabbed.is_some());
    for _ in 0..20 {
        s.tick();
    }
    s.set_controls(
        P,
        Controls {
            down: true,
            dodge_pressed: true,
            ..Default::default()
        },
    );
    s.tick();
    s.set_controls(P, Controls::default());
    assert!(s.crab(P).unwrap().fighter.grabbed.is_none());
    assert_eq!(s.crab(P).unwrap().fighter.legs_lost, 1);
    prepare_moult(&mut s, P);
    assert!(s.moult(P, Skill::Climbing));
    assert_eq!(
        s.crab(P).unwrap().fighter.legs_lost,
        1,
        "starting a moult is not completion"
    );
    for _ in 0..s.tuning.moult.soft_ticks {
        s.tick();
        if !s.crab(P).unwrap().mover.is_soft() {
            break;
        }
    }
    let c = s.crab(P).unwrap();
    assert_eq!(c.mover.moults, 1);
    assert_eq!(c.fighter.legs_lost, 0);
    assert!(c.fighter.vitals.health > 0.);
}
#[test]
fn failed_moults_and_ordinary_soft_shell_recovery_keep_missing_legs() {
    let mut s = Sim::with_one_crab(Vec2::new(200., 0.));
    s.crab_mut(P).unwrap().fighter.legs_lost = 2;
    assert!(!s.moult(P, Skill::Climbing));
    assert_eq!(s.crab(P).unwrap().fighter.legs_lost, 2);
    s.crab_mut(P).unwrap().mover.soft = 1;
    s.tick();
    assert_eq!(s.crab(P).unwrap().fighter.legs_lost, 2);
    assert!(!s.crab(P).unwrap().mover.is_soft());
}
#[test]
fn remote_recovery_is_left_to_its_owner_and_fighter_state_carries_leg_identity() {
    let mut s = Sim::with_one_crab(Vec2::ZERO);
    let remote = PlayerId(9);
    s.add_remote_crab(remote, Vec2::new(200., 0.));
    prepare_moult(&mut s, remote);
    let c = s.crab_mut(remote).unwrap();
    c.fighter.legs_lost = 3;
    c.fighter.grabbed = Some(crabsim::combat::Grabbed {
        by: 0,
        left: 240,
        struggle: 10,
        held: 0,
    });
    c.mover.soft = 1;
    c.mover.growing = Some(crabsim::Feat::GrippingDactyls);
    assert!(!s.moult(remote, Skill::Climbing));
    s.set_controls(
        remote,
        Controls {
            down: true,
            dodge_pressed: true,
            ..Default::default()
        },
    );
    s.tick();
    assert_eq!(s.crab(remote).unwrap().fighter.legs_lost, 3);
    let c = s.crab(remote).unwrap();
    let state = crabsim::net::CrabState {
        id: remote,
        mover: c.mover.clone(),
        fighter: c.fighter.clone(),
        events: Default::default(),
    };
    let bytes = serde_json::to_vec(&state).unwrap();
    let received: crabsim::net::CrabState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(received.fighter.legs_lost, 3);
}
#[test]
fn growth_recovery_replays_from_a_two_player_session_checkpoint() {
    use crabsim::{CellSessionAction, CellSessionRecording, CellSessionTick};
    let mut s = Sim::with_one_crab(Vec2::new(95., 0.));
    s.enable_underwater_cells();
    let guest = PlayerId(1);
    s.add_crab(guest, Vec2::new(300., 0.));
    prepare_moult(&mut s, P);
    s.crab_mut(P).unwrap().fighter.legs_lost = 2;
    s.crab_mut(guest).unwrap().fighter.legs_lost = 1;
    let packed = s.session_checkpoint().unwrap();
    let mut recording = CellSessionRecording::new(&packed, P, guest).unwrap();
    for n in 0..s.tuning.moult.soft_ticks {
        let mut frame = CellSessionTick::default();
        if n == 0 {
            frame.actions[0] = Some((P, CellSessionAction::Moult(Skill::Climbing)));
        }
        assert!(recording.note(s.tick_count(), frame));
        assert!(s.tick_cell_session(P, guest, &frame));
        if n > 0 && !s.crab(P).unwrap().mover.is_soft() {
            break;
        }
    }
    assert_eq!(s.crab(P).unwrap().fighter.legs_lost, 0);
    assert_eq!(s.crab(guest).unwrap().fighter.legs_lost, 1);
    let replay = crabsim::Replay::parse(&recording.to_text().unwrap()).unwrap();
    let mut restored = Sim::with_one_crab(crabsim::SPAWN);
    assert!(replay.prepare_start(&mut restored));
    for _ in 0..replay.length() {
        assert!(replay.step_cell_session(&mut restored));
    }
    assert_eq!(s.state_hash(), restored.state_hash());
}
