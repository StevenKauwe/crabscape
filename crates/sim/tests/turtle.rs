//! The loggerhead turtle: the hunter that comes down from above.
//! Spec: `docs/design/creatures/loggerhead-turtle.md`.
use bevy_math::{Rect, Vec2};
use crabsim::creatures::turtle::{self, BITE_DAMAGE, MISS_RECOVER, TELL_TICKS};
use crabsim::enemy::{Enemy, State};
use crabsim::net::FakeLink;
use crabsim::{Controls, Hit, Kind, Level, PlayerId, Sim, Source, Tuning, world::cues::Cue};

const P: PlayerId = PlayerId(0);
const G: PlayerId = PlayerId(1);
const FLOOR: f32 = 110.0;

/// A crab on the open plateau and a turtle 1,400 over it, with the level's
/// rocks cleared so the water is open.
fn arena() -> (Sim, u32) {
    let mut sim = Sim::with_one_crab(Vec2::new(1000.0, FLOOR));
    sim.level.rocks.clear();
    let id = sim.spawn_enemy(Kind::LoggerheadTurtle, Vec2::new(1000.0, 1500.0));
    (sim, id)
}

fn turtle_of(sim: &Sim, id: u32) -> &Enemy {
    sim.enemies().iter().find(|e| e.id == id).expect("turtle")
}

fn tick(sim: &mut Sim, c: Controls) {
    sim.set_controls(P, c);
    sim.tick();
}

fn health(sim: &Sim) -> f32 {
    sim.crab(P).expect("crab").fighter.vitals.health
}

/// Tick (idle crab) until the turtle enters `state`; false if it never does.
fn until(sim: &mut Sim, id: u32, state: State, max: u32, c: Controls) -> bool {
    for _ in 0..max {
        if turtle_of(sim, id).state == state {
            return true;
        }
        tick(sim, c);
    }
    false
}

fn lock_of(sim: &Sim, id: u32) -> Option<Vec2> {
    match &turtle_of(sim, id).creature {
        crabsim::creatures::Extra::Turtle(s) => s.lock,
        _ => None,
    }
}

#[test]
fn it_is_placed_over_the_basin_by_name() {
    let level = Level::test_level();
    let basin = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "basin")
        .expect("basin");
    let spots: Vec<_> = level
        .foes
        .iter()
        .filter(|(k, _)| *k == Kind::LoggerheadTurtle)
        .collect();
    assert_eq!(spots.len(), 1);
    let p = spots[0].1;
    assert!(
        p.x < basin.rect.min.x && p.y > basin.rect.max.y + 400.0,
        "{p}"
    );
    // 400+ units from every refuge.
    for l in level.landmarks() {
        if matches!(l.kind, crabsim::level::landmarks::Kind::Refuge) {
            assert!(
                p.distance(p.clamp(l.rect.min, l.rect.max)) >= 400.0,
                "{}",
                l.name
            );
        }
    }
}

#[test]
fn the_tell_is_long_cues_and_comes_before_the_bite() {
    let (mut sim, id) = arena();
    let h = health(&sim);
    let mut tell = 0;
    let mut cued = false;
    for _ in 0..3000 {
        let e = turtle_of(&sim, id);
        if e.state == State::Tell {
            tell += 1;
            assert_eq!(health(&sim), h, "hurt during the tell");
        }
        if e.state == State::Attack {
            break;
        }
        tick(&mut sim, Controls::default());
        cued |= sim
            .cues()
            .iter()
            .any(|c| matches!(c.cue, Cue::Tell { crab: P, .. }));
    }
    assert_eq!(turtle_of(&sim, id).state, State::Attack);
    assert!(tell >= 360 && tell == TELL_TICKS as i32, "tell {tell}");
    assert!(cued, "no Cue::Tell");
}

#[test]
fn the_bite_lands_exactly_where_the_tell_locked() {
    let (mut sim, id) = arena();
    assert!(until(&mut sim, id, State::Tell, 3000, Controls::default()));
    let mut locked = None;
    while turtle_of(&sim, id).state == State::Tell {
        tick(&mut sim, Controls::default());
        locked = locked.or(lock_of(&sim, id));
    }
    let lock = locked.expect("locked");
    // The lock came in the last half second, at where the crab stood.
    let e = turtle_of(&sim, id);
    let mouth = turtle::mouth(e.pos, e.facing);
    assert!(mouth.distance(lock) < 6.0, "mouth {mouth} lock {lock}");
    let h = health(&sim);
    for _ in 0..12 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(health(&sim), h - BITE_DAMAGE);
    assert!(h - health(&sim) < 100.0);
}

#[test]
fn one_hit_never_kills_and_two_passes_leave() {
    let (mut sim, id) = arena();
    for _ in 0..6000 {
        tick(&mut sim, Controls::default());
        if turtle_of(&sim, id).state == State::Flee {
            break;
        }
    }
    assert_eq!(
        turtle_of(&sim, id).state,
        State::Flee,
        "should leave after two passes"
    );
    assert!(
        health(&sim) > 30.0,
        "undefended, two bites: {}",
        health(&sim)
    );
    for _ in 0..4000 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(turtle_of(&sim, id).state, State::Gone);
}

#[test]
fn a_slab_overhead_is_cover() {
    let (mut sim, _) = arena();
    sim.level.rocks.push(Rect::new(820.0, 190.0, 1180.0, 215.0));
    let h = health(&sim);
    for _ in 0..4000 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(health(&sim), h, "bitten under cover");
}

#[test]
fn a_refuge_is_not_even_noticed() {
    // The home refuge (x 40..150, y -1..40).
    let mut sim = Sim::with_one_crab(Vec2::new(100.0, 0.0));
    let id = sim.spawn_enemy(Kind::LoggerheadTurtle, Vec2::new(100.0, 1400.0));
    for _ in 0..1500 {
        tick(&mut sim, Controls::default());
        assert_eq!(turtle_of(&sim, id).state, State::Idle);
    }
    assert_eq!(health(&sim), sim.tuning.crab.health);
}

#[test]
fn getting_into_a_refuge_during_the_dive_breaks_it_off() {
    // Far enough from the wall at x 0 that a 90 cm turtle's way down to the
    // crab is clear of it (its body is 656 long).
    let mut sim = Sim::with_one_crab(Vec2::new(700.0, 0.0));
    sim.level.rocks.clear();
    sim.level.climbs.clear();
    let id = sim.spawn_enemy(Kind::LoggerheadTurtle, Vec2::new(700.0, 1400.0));
    assert!(until(&mut sim, id, State::Tell, 3000, Controls::default()));
    let h = health(&sim);
    // Walk home (left) and wait in the refuge.
    for _ in 0..1500 {
        let walking = sim.crab(P).unwrap().mover.pos.x > 100.0;
        tick(
            &mut sim,
            Controls {
                walk: if walking { -1.0 } else { 0.0 },
                ..Default::default()
            },
        );
        assert!(health(&sim) >= h, "hurt in the refuge");
    }
}

#[test]
fn a_block_does_no_damage_but_cracks_the_guard_and_it_cannot_be_parried() {
    let (mut sim, id) = arena();
    let c = Controls {
        block_held: true,
        ..Default::default()
    };
    let h = health(&sim);
    let mut parried = false;
    for _ in 0..3000 {
        let e = turtle_of(&sim, id);
        // Press the parry in the last ticks of the locked tell.
        let press = e.state == State::Tell && e.state_tick + 3 >= TELL_TICKS;
        tick(
            &mut sim,
            Controls {
                block_pressed: press,
                ..c
            },
        );
        parried |= sim.crab(P).unwrap().events.parried;
        if sim.crab(P).unwrap().fighter.armour_break > 0 {
            break;
        }
    }
    assert!(!parried, "a beak from above is not parryable");
    assert!(
        sim.crab(P).unwrap().fighter.armour_break > 0,
        "guard not cracked"
    );
    assert_eq!(health(&sim), h, "a block takes no damage");
    assert_ne!(turtle_of(&sim, id).state, State::Stagger);
    assert!(sim.crab(P).unwrap().fighter.vitals.poise < sim.tuning.crab.poise);
}

#[test]
fn a_dodge_or_a_few_steps_off_the_locked_point_takes_nothing() {
    for dodge in [true, false] {
        let (mut sim, id) = arena();
        let h = health(&sim);
        let mut moved = false;
        for _ in 0..3000 {
            let e = turtle_of(&sim, id).clone();
            let go =
                e.state == State::Tell && e.state_tick + if dodge { 3 } else { 58 } >= TELL_TICKS;
            moved |= go;
            tick(
                &mut sim,
                Controls {
                    walk: if moved { 1.0 } else { 0.0 },
                    dodge_pressed: dodge && go && e.state_tick + 3 == TELL_TICKS,
                    ..Default::default()
                },
            );
            if turtle_of(&sim, id).state == State::Recover {
                break;
            }
        }
        assert_eq!(turtle_of(&sim, id).state, State::Recover, "dodge={dodge}");
        assert_eq!(health(&sim), h, "dodge={dodge}");
        assert!(!turtle_of(&sim, id).landed);
    }
}

#[test]
fn a_miss_hangs_long_with_the_head_exposed() {
    let (mut sim, id) = arena();
    let c = |walk| Controls {
        walk,
        ..Default::default()
    };
    assert!(until(&mut sim, id, State::Tell, 3000, c(0.0)));
    while turtle_of(&sim, id).state == State::Tell {
        let e = turtle_of(&sim, id);
        let w = if e.state_tick + 60 >= TELL_TICKS {
            1.0
        } else {
            0.0
        };
        tick(&mut sim, c(w));
    }
    while turtle_of(&sim, id).state != State::Recover {
        tick(&mut sim, c(1.0));
    }
    let mut ticks = 0;
    while turtle_of(&sim, id).state == State::Recover {
        tick(&mut sim, c(0.0));
        ticks += 1;
    }
    assert!(
        (MISS_RECOVER as i32 - 2..=MISS_RECOVER as i32 + 2).contains(&ticks),
        "{ticks}"
    );
    // The head counts 1.5x while it hangs; the same blow in flight does not.
    let t = Tuning::default();
    let hit = Hit::new(12.0, 0.0, Source::Crab(P));
    let mut hanging = turtle_of(&sim, id).clone();
    hanging.state = State::Recover;
    hanging.facing = 1.0;
    let mut cruising = hanging.clone();
    cruising.state = State::Approach;
    let from_x = hanging.pos.x + 400.0;
    let a = hanging.vitals.health;
    hanging.takes(&hit, from_x, &t);
    cruising.takes(&hit, from_x, &t);
    let (hung, cruise) = (a - hanging.vitals.health, a - cruising.vitals.health);
    assert!((hung / cruise - 1.5).abs() < 1e-3, "{hung} {cruise}");
}

#[test]
fn poise_breaks_and_hurt_drives_it_off_without_a_body() {
    let t = Tuning::default();
    let (mut sim, id) = arena();
    let hit = Hit::new(12.0, 25.0, Source::Crab(P));
    let mut staggered = false;
    for _ in 0..30 {
        let e = sim.enemies_mut().iter_mut().find(|e| e.id == id).unwrap();
        e.takes(&hit, e.pos.x - 100.0, &t);
        staggered |= e.state == State::Stagger;
        if e.state == State::Flee {
            break;
        }
    }
    assert!(staggered, "poise never broke");
    let e = turtle_of(&sim, id);
    assert_eq!(e.state, State::Flee, "should leave below 60%");
    assert!(e.vitals.health > 0.0, "driven off, not killed");
    assert!(turtle::DROPS.is_empty());
}

#[test]
fn the_dive_clock_sends_it_away() {
    let (mut sim, id) = arena();
    // A crab it cannot reach: covered, but noticed (it climbs a stalk).
    sim.level.rocks.push(Rect::new(820.0, 190.0, 1180.0, 215.0));
    for _ in 0..2 {
        tick(&mut sim, Controls::default());
    }
    {
        let e = sim.enemies_mut().iter_mut().find(|e| e.id == id).unwrap();
        e.target = Some(P);
        e.state = State::Recover;
        e.state_tick = 0;
        if let crabsim::creatures::Extra::Turtle(s) = &mut e.creature {
            s.hunt = turtle::DIVE_CLOCK;
        }
    }
    for _ in 0..400 {
        tick(&mut sim, Controls::default());
        if turtle_of(&sim, id).state == State::Flee {
            return;
        }
    }
    panic!("still hunting: {:?}", turtle_of(&sim, id).state);
}

#[test]
fn a_crab_beyond_its_range_is_let_go() {
    let (mut sim, id) = arena();
    assert!(until(
        &mut sim,
        id,
        State::Approach,
        600,
        Controls::default()
    ));
    sim.crab_mut(P).unwrap().mover.pos.x = 1000.0 + turtle::GIVE_UP_RANGE + 200.0;
    for _ in 0..300 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(turtle_of(&sim, id).state, State::Idle);
    assert_eq!(health(&sim), sim.tuning.crab.health);
}

#[test]
fn it_never_enters_a_tunnel_or_mouth() {
    // The home burrow lies under the plateau's west end; the crab is inside.
    let mut sim = Sim::with_one_crab(Vec2::new(400.0, -600.0));
    sim.level.rocks.clear();
    let id = sim.spawn_enemy(Kind::LoggerheadTurtle, Vec2::new(400.0, 900.0));
    let places = sim.level.landmarks();
    for _ in 0..3000 {
        tick(&mut sim, Controls::default());
        let e = turtle_of(&sim, id);
        let body = e.hurtbox(&sim.tuning);
        for l in places.iter().filter(|l| {
            matches!(
                l.kind,
                crabsim::level::landmarks::Kind::Tunnel | crabsim::level::landmarks::Kind::Mouth
            )
        }) {
            assert!(body.intersect(l.rect).is_empty(), "inside {}", l.name);
        }
    }
}

fn scripted(sim: &mut Sim, i: u32) {
    tick(
        sim,
        Controls {
            walk: if (i / 400).is_multiple_of(2) {
                0.6
            } else {
                -0.6
            },
            ..Default::default()
        },
    );
}

#[test]
fn it_replays_exactly() {
    let run = || {
        let (mut sim, _) = arena();
        let mut hashes = Vec::new();
        for i in 0..2400 {
            scripted(&mut sim, i);
            if i % 60 == 0 {
                hashes.push(sim.state_hash());
            }
        }
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn co_op_both_devices_reach_the_same_turtle() {
    let setup = |sim: &mut Sim| {
        sim.level.rocks.clear();
        sim.spawn_enemy(Kind::LoggerheadTurtle, Vec2::new(1000.0, 1500.0));
    };
    let far = Vec2::new(250.0, 0.0);
    let at = Vec2::new(1000.0, FLOOR);
    let mut host = Sim::new(Tuning::default(), Level::test_level());
    setup(&mut host);
    host.add_crab(P, far);
    host.add_remote_crab(G, at);
    host.go_online(true);
    let mut guest = Sim::new(Tuning::default(), Level::test_level());
    setup(&mut guest);
    guest.add_remote_crab(P, far);
    guest.add_crab(G, at);
    guest.go_online(false);
    let mut link = FakeLink::steady(0);
    let hp = guest.crab(G).unwrap().fighter.vitals.health;
    let mut bitten = false;
    for _ in 0..2400 {
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        bitten |= guest.crab(G).unwrap().fighter.vitals.health < hp;
    }
    assert!(bitten, "the guest was never bitten");
    let id = |s: &Sim| {
        let e = s
            .enemies()
            .iter()
            .filter(|e| e.kind == Kind::LoggerheadTurtle)
            .max_by_key(|e| e.id)
            .expect("turtle");
        (e.state, e.landed, e.pos.distance(Vec2::ZERO).round())
    };
    let (a, b) = (id(&host), id(&guest));
    assert_eq!(a.0, b.0, "{a:?} {b:?}");
    assert_eq!(a.1, b.1);
}
