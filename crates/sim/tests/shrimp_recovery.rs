use bevy_math::Vec2;
use crabsim::enemy::State;
use crabsim::{Controls, Kind, PlayerId, Sim};
const P: PlayerId = PlayerId(0);
fn encounter(size: u32) -> Sim {
    let mut s = Sim::with_one_crab(Vec2::new(13070.0, -5250.0));
    s.crab_mut(P).unwrap().mover.moults = size;
    s.enable_underwater_cells();
    s.start_recording();
    s.spawn_enemy(Kind::Snapper, Vec2::new(13200.0, -5250.0));
    s
}
fn tick(s: &mut Sim, c: Controls) {
    s.set_controls(P, c);
    s.tick();
}
fn bait(s: &mut Sim) -> u32 {
    let mut snaps = 0;
    let mut previous = State::Idle;
    for _ in 0..1800 {
        let e = &s.enemies()[0];
        if e.state == State::Attack && previous != State::Attack {
            snaps += 1;
        }
        previous = e.state;
        if e.phase(&s.tuning) == "rest" {
            return snaps;
        }
        let alerted = e.state != State::Idle;
        let x = s.crab(P).unwrap().mover.pos.x;
        tick(
            s,
            Controls {
                walk: if (13140.0 - x).abs() > 3.0 {
                    (13140.0 - x).signum()
                } else {
                    0.0
                },
                block_held: alerted,
                ..Default::default()
            },
        );
    }
    panic!(
        "no pause: {:?} at {:?}",
        s.enemies()[0],
        s.crab(P).unwrap().mover.pos
    );
}
#[test]
fn five_real_snaps_offer_a_lowered_claw_crossing_at_both_sizes() {
    for size in [0, 100] {
        let mut s = encounter(size);
        assert_eq!(bait(&mut s), 5);
        assert_eq!(s.enemies()[0].state_len, 180);
        assert_eq!(s.enemies()[0].snap_count, 0);
        let health = s.crab(P).unwrap().fighter.vitals.health;
        for _ in 0..150 {
            if s.crab(P).unwrap().mover.pos.x > 13280.0 {
                break;
            }
            tick(
                &mut s,
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
        }
        assert!(s.crab(P).unwrap().mover.pos.x > 13280.0);
        assert_eq!(
            s.crab(P).unwrap().fighter.vitals.health,
            health,
            "cross the rest without another snap"
        );
    }
}
#[test]
fn whole_repeated_snap_encounter_replays_exactly() {
    let mut live = encounter(0);
    assert_eq!(bait(&mut live), 5);
    for _ in 0..80 {
        tick(
            &mut live,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
    }
    let replay = crabsim::Replay::parse(&live.recording().unwrap().to_text()).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(replay.hash, Some(again.state_hash()));
}

#[test]
fn expired_snap_window_starts_a_new_sequence_instead_of_resting() {
    for (left, expected_count, expected_recovery) in [(1, 1, 72), (2, 0, 180)] {
        let mut s = encounter(0);
        let e = &mut s.enemies_mut()[0];
        e.state = State::Attack;
        e.state_tick = 2;
        e.state_len = 2;
        e.target = Some(P);
        e.snap_count = 4;
        e.snap_window = left;
        tick(&mut s, Controls::default());
        let e = &s.enemies()[0];
        assert_eq!(e.snap_count, expected_count);
        assert_eq!(e.state, State::Recover);
        assert_eq!(e.state_len, expected_recovery);
    }
}

#[test]
fn injury_retreat_stagger_death_hold_and_lost_target_reset_the_sequence() {
    use crabsim::combat::Hit;
    for (damage, poise, state) in [
        (1.0, 0.0, State::Flee),
        (0.0, 100.0, State::Stagger),
        (100.0, 0.0, State::Gone),
    ] {
        let mut s = encounter(0);
        let t = s.tuning.clone();
        let e = &mut s.enemies_mut()[0];
        e.state = State::Approach;
        e.snap_count = 4;
        e.snap_window = 500;
        e.takes(
            &Hit {
                damage,
                poise,
                ..Default::default()
            },
            13140.0,
            &t,
        )
        .unwrap();
        assert_eq!(e.state, state);
        assert_eq!((e.snap_count, e.snap_window), (0, 0));
    }
    for held in [false, true] {
        let mut s = encounter(0);
        let e = &mut s.enemies_mut()[0];
        e.state = State::Approach;
        e.target = None;
        e.snap_count = 4;
        e.snap_window = 500;
        e.held = held;
        tick(&mut s, Controls::default());
        assert_eq!(
            (s.enemies()[0].snap_count, s.enemies()[0].snap_window),
            (0, 0)
        );
    }
}

#[test]
fn wire_state_preserves_the_window_and_only_the_local_defender_takes_damage() {
    use crabsim::{enemy, net::Msg, rng::Rng};
    let s = encounter(0);
    let mut e = s.enemies()[0].clone();
    e.state = State::Attack;
    e.state_len = 2;
    e.state_tick = 0;
    e.target = Some(P);
    e.facing = -1.0;
    e.snap_count = 4;
    e.snap_window = 100;
    let wire = Msg::Enemy {
        at: 0,
        enemy: e.clone(),
    }
    .to_bytes();
    let Msg::Enemy {
        enemy: mut copy, ..
    } = Msg::from_bytes(&wire).unwrap()
    else {
        panic!("wrong message")
    };
    assert_eq!(e, copy);
    let mut local = s.crabs().to_vec();
    local[0].mover.pos = Vec2::new(13140.0, -5250.0);
    let mut remote = local.clone();
    remote[0].remote = true;
    let before = local[0].fighter.vitals.health;
    let mut r1 = Rng::new(42);
    let mut r2 = Rng::new(42);
    enemy::step(&mut e, &mut local, &s.tuning, &s.level, &mut r1, &[], false);
    enemy::step(
        &mut copy,
        &mut remote,
        &s.tuning,
        &s.level,
        &mut r2,
        &[],
        false,
    );
    assert!(local[0].fighter.vitals.health < before);
    assert_eq!(remote[0].fighter.vitals.health, before);
    // The remote defender reports its outcome through the existing channel;
    // serialization itself never awards damage to that remote crab.
    let mut restored = s.enemies()[0].clone();
    restored.snap_count = 3;
    restored.snap_window = 800;
    let data = serde_json::to_string(&restored).unwrap();
    assert_eq!(
        restored,
        serde_json::from_str::<crabsim::enemy::Enemy>(&data).unwrap()
    );
}

#[test]
fn a_loaded_maximum_count_enters_rest_without_overflow() {
    let mut s = encounter(0);
    let e = &mut s.enemies_mut()[0];
    e.state = State::Attack;
    e.state_tick = 2;
    e.state_len = 2;
    e.target = Some(P);
    e.snap_count = u8::MAX;
    e.snap_window = 100;
    tick(&mut s, Controls::default());
    let e = &s.enemies()[0];
    assert_eq!(e.phase(&s.tuning), "rest");
    assert_eq!((e.snap_count, e.snap_window), (0, 0));
}

#[test]
fn older_enemy_snapshots_start_with_no_pending_snap_sequence() {
    let e = encounter(0).enemies()[0].clone();
    let mut value = serde_json::to_value(&e).unwrap();
    let fields = value.as_object_mut().unwrap();
    fields.remove("snap_count");
    fields.remove("snap_window");
    let restored = serde_json::from_value::<crabsim::enemy::Enemy>(value).unwrap();
    assert_eq!(restored, e);
}
