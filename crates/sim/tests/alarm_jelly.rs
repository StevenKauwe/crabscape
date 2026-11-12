//! The alarm is an optional tool, not a mandatory damage gate.
use bevy_math::Vec2;
use crabsim::alarm_jelly::{alarm, investigating};
use crabsim::enemy::State;
use crabsim::{Controls, Kind, PlayerId, Sim};
const P: PlayerId = PlayerId(0);
fn flat(at: Vec2) -> Sim {
    let mut s = Sim::with_one_crab(at);
    s.level.floor = vec![Vec2::ZERO, Vec2::new(20000.0, 0.0)];
    s.level.rocks.clear();
    s.level.climbs.clear();
    s.level.currents.clear();
    s
}
fn tick(s: &mut Sim, c: Controls) {
    s.set_controls(P, c);
    s.tick();
}
#[test]
fn tentacle_guard_and_local_venom_duration() {
    for guard in [false, true] {
        let mut s = flat(Vec2::new(1000.0, 0.0));
        s.spawn_enemy(Kind::AlarmJelly, Vec2::new(1000.0, 80.0));
        s.add_remote_crab(PlayerId(1), Vec2::new(1000.0, 0.0));
        let hp = s.crab(P).unwrap().fighter.vitals.health;
        tick(
            &mut s,
            Controls {
                block_held: guard,
                ..Default::default()
            },
        );
        assert_eq!(s.crab(P).unwrap().fighter.venom.len(), usize::from(!guard));
        assert!(s.crab(PlayerId(1)).unwrap().fighter.venom.is_empty());
        for _ in 0..240 {
            tick(
                &mut s,
                Controls {
                    walk: 1.0,
                    block_held: guard,
                    ..Default::default()
                },
            );
        }
        let lost = hp - s.crab(P).unwrap().fighter.vitals.health;
        assert!(
            (lost - if guard { 0.0 } else { 4.0 }).abs() < 0.01,
            "lost={lost}"
        );
        assert!(s.crab(P).unwrap().fighter.venom.is_empty());
    }
}
#[test]
fn a_real_claw_hit_alarms_once_then_cools_down() {
    let mut s = flat(Vec2::new(1000.0, 0.0));
    s.spawn_enemy(Kind::AlarmJelly, Vec2::new(1080.0, 15.0));
    for i in 0..90 {
        tick(
            &mut s,
            Controls {
                smash_pressed: i == 0,
                ..Default::default()
            },
        );
    }
    assert_eq!(s.enemies()[0].state, State::Attack);
    assert!(!alarm(&mut s.enemies_mut()[0]));
    let before = s.enemies()[0].state_tick;
    for i in 0..40 {
        tick(
            &mut s,
            Controls {
                jab_pressed: i == 0,
                ..Default::default()
            },
        );
    }
    assert!(s.enemies()[0].state_tick > before);
    s.crab_mut(P).unwrap().mover.pos.x = 800.0;
    for _ in 0..1000 {
        s.tick();
    }
    assert_eq!(s.enemies()[0].state, State::Idle);
    assert!(alarm(&mut s.enemies_mut()[0]));
}
#[test]
fn nearest_eligible_hunter_investigates_then_returns_without_teleport() {
    let mut s = flat(Vec2::new(3000.0, 0.0));
    s.spawn_enemy(Kind::AlarmJelly, Vec2::new(1100.0, 200.0));
    s.spawn_enemy(Kind::Moray, Vec2::new(1200.0, 0.0));
    s.spawn_enemy(Kind::Moray, Vec2::new(2400.0, 0.0));
    assert!(alarm(&mut s.enemies_mut()[0]));
    let start = s.enemies()[1].pos;
    s.tick();
    assert!(investigating(&s.enemies()[1]));
    assert!(!investigating(&s.enemies()[2]));
    let mut copy = s.clone();
    let msg = crabsim::net::Msg::Enemy {
        at: s.tick_count(),
        enemy: s.enemies()[1].clone(),
    };
    let crabsim::net::Msg::Enemy { enemy, .. } =
        crabsim::net::Msg::from_bytes(&msg.to_bytes()).unwrap()
    else {
        panic!()
    };
    copy.enemies_mut()[1] = enemy;
    for _ in 0..358 {
        let old = s.enemies()[1].pos;
        s.tick();
        copy.tick();
        assert!(old.distance(s.enemies()[1].pos) <= 2.18);
        assert_eq!(s.enemies(), copy.enemies());
    }
    assert!(s.enemies()[1].pos.distance(start) > 100.0);
    for _ in 0..3 {
        s.tick();
    }
    assert!(!investigating(&s.enemies()[1]));
}
#[test]
fn a_committed_bite_and_a_hiding_hunter_ignore_the_flash() {
    for state in [
        State::Tell,
        State::Attack,
        State::Grab,
        State::Flee,
        State::Idle,
    ] {
        let mut s = flat(Vec2::new(3000.0, 0.0));
        s.spawn_enemy(Kind::AlarmJelly, Vec2::new(1100.0, 200.0));
        s.spawn_enemy(Kind::Moray, Vec2::new(1200.0, 0.0));
        let e = &mut s.enemies_mut()[1];
        e.state = state;
        e.state_len = 1000;
        e.target = Some(P);
        if state == State::Idle {
            e.hiding = 600;
        }
        alarm(&mut s.enemies_mut()[0]);
        s.tick();
        assert!(!investigating(&s.enemies()[1]));
    }
}
#[test]
fn lower_return_stays_below_the_optional_tentacle_for_both_sizes() {
    for moults in [0, 100] {
        let mut s = Sim::with_one_crab(Vec2::new(15040.0, -3460.0));
        s.crab_mut(P).unwrap().mover.moults = moults;
        let at = s
            .level
            .foes
            .iter()
            .find(|(k, _)| *k == Kind::AlarmJelly)
            .unwrap()
            .1;
        s.spawn_enemy(Kind::AlarmJelly, at);
        s.enable_underwater_cells();
        for direction in [1.0, -1.0] {
            for _ in 0..900 {
                tick(
                    &mut s,
                    Controls {
                        walk: direction,
                        ..Default::default()
                    },
                );
                let x = s.crab(P).unwrap().mover.pos.x;
                if (direction > 0.0 && x > 15400.0) || (direction < 0.0 && x < 15030.0) {
                    break;
                }
            }
            let c = s.crab(P).unwrap();
            assert!(c.fighter.venom.is_empty(), "{:?}", c.mover.pos);
            assert!(
                (direction > 0.0 && c.mover.pos.x > 15400.0)
                    || (direction < 0.0 && c.mover.pos.x < 15030.0),
                "{:?}",
                c.mover.pos
            );
        }
    }
}

#[test]
fn guarded_bell_approach_draws_the_actual_hunter_off_the_lower_route() {
    for moults in [0, 100] {
        let mut s = Sim::with_one_crab(Vec2::new(15080.0, -4850.0));
        s.crab_mut(P).unwrap().mover.moults = moults;
        s.crab_mut(P).unwrap().mover.upgrades = 4;
        s.enable_underwater_cells();
        s.spawn_enemy(Kind::AlarmJelly, Vec2::new(15180.0, -4830.0));
        s.spawn_enemy(
            Kind::Moray,
            Vec2::new(15220.0, s.level.floor_height(15220.0)),
        );
        let home = s.enemies()[1].pos;
        let mut called = false;
        let mut highest = home.y;
        for _ in 0..480 {
            tick(
                &mut s,
                Controls {
                    walk: 1.0,
                    block_held: true,
                    ..Default::default()
                },
            );
            called |= investigating(&s.enemies()[1]);
            highest = highest.max(s.enemies()[1].pos.y);
        }
        assert!(
            called,
            "bell missed: crab={:?}, jelly={:?}, hunter={:?}",
            s.crab(P).unwrap().mover.pos,
            s.enemies()[0].state,
            s.enemies()[1].state
        );
        assert!(
            highest > home.y + 250.0,
            "hunter stayed at {highest}, home={home:?}"
        );
        let c = s.crab(P).unwrap();
        assert!(c.fighter.venom.is_empty());
        assert!(c.fighter.dead.is_none());
        assert!(c.mover.pos.x > 15400.0, "{:?}", c.mover.pos);
    }
}
#[test]
fn guest_hit_calls_the_host_hunter_and_sting_is_defender_owned() {
    let mut host = flat(Vec2::new(3000.0, 0.0));
    host.add_remote_crab(PlayerId(1), Vec2::new(1000.0, 0.0));
    host.spawn_enemy(Kind::AlarmJelly, Vec2::new(1080.0, 15.0));
    host.spawn_enemy(Kind::Moray, Vec2::new(1250.0, 0.0));
    let mut guest = host.clone();
    guest.crab_mut(P).unwrap().remote = true;
    guest.crab_mut(PlayerId(1)).unwrap().remote = false;
    host.go_online(true);
    guest.go_online(false);
    let mut link = crabsim::net::FakeLink::steady(6);
    let mut called = false;
    for i in 0..180 {
        guest.set_controls(
            PlayerId(1),
            Controls {
                smash_pressed: i == 0,
                walk: if i > 80 { -1.0 } else { 0.0 },
                ..Default::default()
            },
        );
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        called |= investigating(&host.enemies()[1]);
    }
    assert!(called, "guest strike never reached host alarm");
    assert!(host.crab(P).unwrap().fighter.venom.is_empty());
    assert!(guest.crab(PlayerId(1)).unwrap().fighter.vitals.health < 100.0);
    assert_eq!(host.enemies()[0].state, State::Attack);
}

#[test]
fn dodge_crosses_contact_without_venom_and_rock_blocks_the_alarm() {
    let mut s = flat(Vec2::new(950.0, 0.0));
    s.spawn_enemy(Kind::AlarmJelly, Vec2::new(1000.0, 200.0));
    let mut evaded = false;
    for i in 0..18 {
        tick(
            &mut s,
            Controls {
                walk: 1.0,
                dodge_pressed: i == 0,
                ..Default::default()
            },
        );
        evaded |= s.crab(P).unwrap().events.evaded;
    }
    assert!(evaded);
    assert!(s.crab(P).unwrap().fighter.venom.is_empty());
    let mut s = flat(Vec2::new(3000.0, 0.0));
    s.spawn_enemy(Kind::AlarmJelly, Vec2::new(1000.0, 200.0));
    s.spawn_enemy(Kind::Moray, Vec2::new(1250.0, 0.0));
    s.level
        .rocks
        .push(bevy_math::Rect::new(1100.0, 0.0, 1150.0, 400.0));
    alarm(&mut s.enemies_mut()[0]);
    s.tick();
    assert!(!investigating(&s.enemies()[1]));
}
