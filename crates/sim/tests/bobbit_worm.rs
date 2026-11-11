//! The bobbit worm (`docs/design/creatures/bobbit-worm.md`): a sand ambusher
//! that feels a crab, flicks its antennae for half a second, then lunges.
use bevy_math::Vec2;
use crabsim::combat::{Hit, Source};
use crabsim::creatures::bobbit::{self, FEEL_RADIUS, TOUCH_RADIUS};
use crabsim::enemy::State;
use crabsim::level::Level;
use crabsim::level::landmarks::Kind as Place;
use crabsim::world::cues::Cue;
use crabsim::{Controls, Feel, Kind, PlayerId, Sim};

const P: PlayerId = PlayerId(0);
const X: f32 = 930.0;
/// Crab centre to burrow, for tests that want a particular edge distance.
const HALF: f32 = 36.0;

/// A worm at `X` and a crab `dx` to its right, facing it (feel preset A).
fn world(dx: f32) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(X + dx, 0.0));
    sim.set_feel(Feel::Current);
    sim.spawn_enemy(Kind::BobbitWorm, Vec2::new(X, 0.0));
    let floor = sim.level.floor_height(X + dx);
    let c = sim.crab_mut(P).unwrap();
    c.mover.pos = Vec2::new(X + dx, floor);
    c.fighter.facing = -1.0;
    sim
}

fn worm(sim: &Sim) -> &crabsim::Enemy {
    &sim.enemies()[0]
}

fn health(sim: &Sim) -> f32 {
    sim.crab(P).unwrap().fighter.vitals.health
}

fn tick(sim: &mut Sim, c: Controls) {
    sim.set_controls(P, c);
    sim.tick();
}

fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        tick(sim, Controls::default());
    }
}

/// Put the worm into its tell at once, aimed at the crab.
fn provoke(sim: &mut Sim) {
    let e = &mut sim.enemies_mut()[0];
    e.target = Some(P);
    e.facing = 1.0;
    e.state = State::Tell;
    e.state_tick = 0;
    e.state_len = 60;
}

/// Run until the worm is in `state` (at most `max` ticks).
fn until(sim: &mut Sim, state: State, max: u32, c: Controls) -> bool {
    for _ in 0..max {
        if worm(sim).state == state {
            return true;
        }
        tick(sim, c);
    }
    worm(sim).state == state
}

#[test]
fn it_is_placed_on_the_rim_sand_away_from_rest_places() {
    let level = Level::test_level();
    let worms: Vec<Vec2> = level
        .foes
        .iter()
        .filter(|(k, _)| *k == Kind::BobbitWorm)
        .map(|f| f.1)
        .collect();
    assert_eq!(worms.len(), 2, "{worms:?}");
    assert_eq!(worms[1].x - worms[0].x, bobbit::SPACING);
    let landmarks = level.landmarks();
    for w in &worms {
        assert_eq!(w.y, level.floor_height(w.x));
        for l in &landmarks {
            let near = w.distance(w.clamp(l.rect.min, l.rect.max));
            match l.kind {
                Place::Refuge if l.name.contains("stand") => {
                    assert!(near >= 150.0, "{} is {near} away", l.name);
                }
                Place::Refuge => assert!(near >= 400.0, "{} is {near} away", l.name),
                Place::Tunnel | Place::Mouth | Place::Chamber { .. } => {
                    assert!(near >= 150.0, "{} is {near} away", l.name);
                }
                _ => {}
            }
        }
        for crevice in &level.crevices {
            assert!(w.distance(crevice.den) >= 400.0);
        }
    }
}

#[test]
fn buried_it_cannot_be_hit() {
    let sim = world(300.0);
    let e = worm(&sim);
    assert_eq!(e.phase(&sim.tuning), "buried");
    assert!(!e.hittable(&sim.tuning));
    let mut sim = world(300.0);
    let hit = Hit::new(12.0, 25.0, Source::World);
    let tuning = sim.tuning.clone();
    assert!(
        sim.enemies_mut()[0]
            .takes(&hit, X + 100.0, &tuning)
            .is_none()
    );
    // Not by a claw's box either.
    let e = worm(&sim);
    let claw = bevy_math::Rect::new(X - 20.0, e.pos.y, X + 20.0, e.pos.y + 30.0);
    assert!(e.meets(claw, &sim.tuning).is_none());
}

#[test]
fn a_walker_is_felt_only_inside_the_touch_radius() {
    let sim = world(0.0);
    let t = &sim.tuning;
    let e = worm(&sim).clone();
    let mut crab = sim.crab(P).unwrap().clone();
    let at = |edge: f32| X + edge + HALF;
    // Walking speed is 270 u/s: slower than the vibration threshold.
    crab.mover.vel.x = 270.0;
    crab.mover.pos.x = at(TOUCH_RADIUS + 4.0);
    assert!(!bobbit::feels_crab(&e, &crab, t));
    crab.mover.pos.x = at(TOUCH_RADIUS - 4.0);
    assert!(bobbit::feels_crab(&e, &crab, t));
    // Standing still counts: it is touch.
    crab.mover.vel.x = 0.0;
    assert!(bobbit::feels_crab(&e, &crab, t));
    // Above it, out of the antennae's reach.
    crab.mover.pos.y += 100.0;
    assert!(!bobbit::feels_crab(&e, &crab, t));
}

#[test]
fn a_sprint_or_a_landing_is_felt_from_the_larger_radius() {
    let sim = world(0.0);
    let t = &sim.tuning;
    let e = worm(&sim).clone();
    let mut crab = sim.crab(P).unwrap().clone();
    crab.mover.pos.x = X + HALF + (TOUCH_RADIUS + FEEL_RADIUS) / 2.0;
    crab.mover.vel.x = 480.0;
    assert!(bobbit::feels_crab(&e, &crab, t));
    crab.mover.vel.x = 0.0;
    assert!(!bobbit::feels_crab(&e, &crab, t));
    crab.events.landed = Some(400.0);
    assert!(bobbit::feels_crab(&e, &crab, t));
    crab.mover.pos.x = X + HALF + FEEL_RADIUS + 10.0;
    assert!(!bobbit::feels_crab(&e, &crab, t));
    crab.mover.vel.x = 480.0;
    assert!(!bobbit::feels_crab(&e, &crab, t));
}

#[test]
fn a_walking_crab_sets_it_off_at_the_touch_radius_and_a_sprinter_earlier() {
    let mut edges = Vec::new();
    for sprint in [false, true] {
        let mut sim = world(330.0);
        let mut started = None;
        for i in 0..400 {
            if worm(&sim).state == State::Tell {
                let c = sim.crab(P).unwrap();
                started = Some(c.mover.pos.x - X - HALF);
                break;
            }
            tick(
                &mut sim,
                Controls {
                    walk: -1.0,
                    dodge_held: sprint,
                    dodge_pressed: sprint && i == 0,
                    ..Default::default()
                },
            );
        }
        edges.push(started.expect("never set off"));
    }
    assert!(
        edges[0] <= TOUCH_RADIUS + 1.0 && edges[0] > TOUCH_RADIUS - 8.0,
        "{edges:?}"
    );
    assert!(
        edges[1] > edges[0] + 20.0 && edges[1] <= FEEL_RADIUS + 1.0,
        "{edges:?}"
    );
}

#[test]
fn the_tell_lasts_half_a_second_and_announces_itself_before_the_strike() {
    let mut sim = world(100.0);
    sim.crab_mut(P).unwrap().mover.vel.x = 0.0;
    assert!(until(&mut sim, State::Tell, 5, Controls::default()));
    let mut tell = 0;
    while worm(&sim).state == State::Tell {
        tell += 1;
        assert_eq!(health(&sim), sim.tuning.crab.health, "hurt during the tell");
        tick(&mut sim, Controls::default());
        assert!(tell < 200);
    }
    assert_eq!(
        tell, 60,
        "tell ticks (the first counted when it was entered)"
    );
    assert_eq!(worm(&sim).state, State::Attack);
    assert!(
        sim.cues()
            .iter()
            .any(|e| matches!(e.cue, Cue::Tell { crab, .. } if crab == P)),
        "no tell swell for the crab"
    );
    assert_eq!(worm(&sim).phase(&sim.tuning), "snap");
}

#[test]
fn the_lunge_is_four_ticks_then_it_stays_out_for_a_second_and_a_fifth_and_draws_back() {
    let mut sim = world(400.0);
    provoke(&mut sim);
    assert!(until(&mut sim, State::Attack, 70, Controls::default()));
    let mut active = 0;
    while worm(&sim).state == State::Attack {
        active += 1;
        tick(&mut sim, Controls::default());
    }
    assert_eq!(active, 4);
    assert_eq!(worm(&sim).reach, 78.0);
    let mut out = 0;
    while worm(&sim).state == State::Recover {
        out += 1;
        assert_eq!(worm(&sim).reach, 78.0);
        tick(&mut sim, Controls::default());
    }
    assert_eq!(out, 144);
    idle(&mut sim, 40);
    let e = worm(&sim);
    assert_eq!((e.state, e.reach, e.pos), (State::Idle, 0.0, e.home));
    assert!(!e.hittable(&sim.tuning));
}

#[test]
fn a_crab_outside_the_lunge_reach_is_never_hit() {
    // Head and jaws end ~100 from the burrow: the crab's edge is 106 away.
    for dx in [100.0 + HALF + 8.0, 190.0, 300.0] {
        let mut sim = world(dx);
        provoke(&mut sim);
        idle(&mut sim, 260);
        assert_eq!(health(&sim), sim.tuning.crab.health, "dx {dx}");
        assert!(sim.crab(P).unwrap().fighter.grabbed.is_none());
    }
    // And one inside it is.
    let mut sim = world(100.0 + HALF - 12.0);
    provoke(&mut sim);
    idle(&mut sim, 100);
    assert!(health(&sim) < sim.tuning.crab.health);
}

#[test]
fn an_unblocked_bite_hurts_grips_drags_and_lets_go_within_a_second_and_a_half() {
    let mut sim = world(100.0);
    provoke(&mut sim);
    assert!(until(&mut sim, State::Grab, 80, Controls::default()));
    let before = sim.crab(P).unwrap().mover.pos.x;
    assert_eq!(health(&sim), sim.tuning.crab.health - 16.0);
    assert!(sim.crab(P).unwrap().fighter.grabbed.is_some());
    let mut held = 0;
    while sim.crab(P).unwrap().fighter.grabbed.is_some() {
        held += 1;
        tick(&mut sim, Controls::default());
        assert!(held <= 181, "held too long");
    }
    assert!((150..=181).contains(&held), "{held}");
    let after = sim.crab(P).unwrap().mover.pos.x;
    assert!(after < before - 10.0, "dragged {before} -> {after}");
    assert!(
        after - X >= bobbit::DRAG_STOP - 1.0,
        "dragged into the hole"
    );
    let lost = sim.tuning.crab.health - health(&sim);
    assert!((28.0..=32.0).contains(&lost), "lost {lost}");
    tick(&mut sim, Controls::default());
    tick(&mut sim, Controls::default());
    assert_eq!(worm(&sim).state, State::Recover);
}

#[test]
fn struggling_frees_the_crab_sooner_and_autotomy_at_once() {
    let mut held = Vec::new();
    for mash in [false, true] {
        let mut sim = world(100.0);
        provoke(&mut sim);
        assert!(until(&mut sim, State::Grab, 80, Controls::default()));
        let mut n = 0;
        while sim.crab(P).unwrap().fighter.grabbed.is_some() {
            tick(
                &mut sim,
                Controls {
                    dodge_pressed: mash && n % 4 == 0,
                    ..Default::default()
                },
            );
            n += 1;
        }
        held.push(n);
    }
    assert!(held[1] + 40 < held[0], "{held:?}");
    let mut sim = world(100.0);
    provoke(&mut sim);
    assert!(until(&mut sim, State::Grab, 80, Controls::default()));
    idle(&mut sim, 12);
    tick(
        &mut sim,
        Controls {
            dodge_pressed: true,
            dodge_held: true,
            down: true,
            ..Default::default()
        },
    );
    assert!(sim.crab(P).unwrap().fighter.grabbed.is_none());
    assert_eq!(sim.crab(P).unwrap().fighter.legs_lost, 1);
}

#[test]
fn a_parry_in_the_window_takes_nothing_and_staggers_it() {
    let mut sim = world(100.0);
    provoke(&mut sim);
    let mut parried = false;
    for _ in 0..90 {
        let e = worm(&sim);
        let press = e.state == State::Tell && e.state_tick + 5 >= e.state_len;
        let fresh = press && e.state_tick + 5 == e.state_len;
        tick(
            &mut sim,
            Controls {
                block_held: press,
                block_pressed: fresh,
                ..Default::default()
            },
        );
        if sim.crab(P).unwrap().events.parried {
            parried = true;
            break;
        }
    }
    assert!(parried);
    assert_eq!(health(&sim), sim.tuning.crab.health);
    assert_eq!(worm(&sim).state, State::Stagger);
    assert!(sim.crab(P).unwrap().fighter.grabbed.is_none());
    // Staggered, then recovering with the neck out.
    assert!(until(&mut sim, State::Recover, 120, Controls::default()));
    assert!(worm(&sim).reach > 0.0);
}

#[test]
fn a_block_takes_a_fifth_and_never_grips() {
    let mut sim = world(100.0);
    provoke(&mut sim);
    idle_blocking(&mut sim, 100);
    let lost = sim.tuning.crab.health - health(&sim);
    assert!(
        (lost - 16.0 * sim.tuning.block_damage).abs() < 0.01,
        "{lost}"
    );
    assert!(sim.crab(P).unwrap().fighter.grabbed.is_none());
    assert_ne!(worm(&sim).state, State::Grab);
}

fn idle_blocking(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        tick(
            sim,
            Controls {
                block_held: true,
                ..Default::default()
            },
        );
    }
}

#[test]
fn a_dodge_through_the_lunge_takes_nothing() {
    let mut sim = world(100.0);
    provoke(&mut sim);
    for _ in 0..90 {
        let e = worm(&sim);
        let go = e.state == State::Tell && e.state_tick + 3 == e.state_len;
        tick(
            &mut sim,
            Controls {
                dodge_pressed: go,
                walk: if go { 1.0 } else { 0.0 },
                ..Default::default()
            },
        );
    }
    assert_eq!(health(&sim), sim.tuning.crab.health);
}

/// A crab out of reach hops in place beside the worm: the landing is felt,
/// the worm lunges at nothing, and its neck is out to be hit.
fn bait(sim: &mut Sim) {
    for i in 0..400 {
        tick(
            sim,
            Controls {
                jump_pressed: i == 0,
                jump_held: i < 12,
                ..Default::default()
            },
        );
        if worm(sim).state == State::Recover {
            return;
        }
    }
    panic!("never baited: {:?}", worm(sim));
}

#[test]
fn baiting_the_snap_from_outside_its_reach_then_punishing_the_neck_drives_it_off() {
    // Edge 139 from the antennae, centre 175: out of the 136 reach, in the 150
    // vibration radius.
    let mut sim = world(HALF + 139.0);
    bait(&mut sim);
    assert_eq!(health(&sim), sim.tuning.crab.health, "the bait cost a bite");
    // Step in to claw range and strike the neck: x1.5 while it recovers.
    sim.crab_mut(P).unwrap().mover.pos.x = X + 150.0;
    let start = worm(&sim).vitals.health;
    let mut first = None;
    for i in 0..400 {
        let strong = worm(&sim).vitals.health >= 16.0;
        tick(
            &mut sim,
            Controls {
                smash_pressed: i % 45 == 0 && strong,
                ..Default::default()
            },
        );
        if first.is_none() && worm(&sim).vitals.health < start {
            first = Some(start - worm(&sim).vitals.health);
        }
        if worm(&sim).fled && worm(&sim).state == State::Idle {
            break;
        }
    }
    assert_eq!(first, Some(18.0), "the neck takes x1.5 while it recovers");
    let e = worm(&sim);
    assert!(e.fled && e.state == State::Idle, "{e:?}");
    assert_eq!(health(&sim), sim.tuning.crab.health);
    assert_eq!(e.phase(&sim.tuning), "withdrawn");
    assert!(!e.hittable(&sim.tuning));
    // A crab standing on it is not felt for the whole 20 s...
    sim.crab_mut(P).unwrap().mover.pos.x = X + 40.0;
    idle(&mut sim, 2300);
    assert_eq!(health(&sim), sim.tuning.crab.health);
    assert_eq!(worm(&sim).state, State::Idle);
    // ...then it is back, whole.
    idle(&mut sim, 200);
    assert_eq!(worm(&sim).vitals.health, 40.0);
    assert!(!worm(&sim).fled);
}

#[test]
fn smashes_break_its_poise_and_it_staggers() {
    let mut sim = world(HALF + 139.0);
    bait(&mut sim);
    sim.crab_mut(P).unwrap().mover.pos.x = X + 150.0;
    let mut staggered = false;
    for i in 0..120 {
        tick(
            &mut sim,
            Controls {
                smash_pressed: i % 40 == 0,
                ..Default::default()
            },
        );
        staggered |= worm(&sim).state == State::Stagger;
    }
    assert!(staggered, "{:?}", worm(&sim));
}

#[test]
fn it_never_leaves_its_burrow_and_a_crab_can_walk_away() {
    let mut sim = world(60.0);
    let home = worm(&sim).home;
    // Bitten once, then running away from it past the give-up range.
    for i in 0..1200 {
        tick(
            &mut sim,
            Controls {
                walk: if i > 300 { 1.0 } else { 0.0 },
                dodge_held: i > 300,
                dodge_pressed: i == 301,
                ..Default::default()
            },
        );
        assert_eq!(worm(&sim).pos, home);
    }
    let d = sim.crab(P).unwrap().mover.pos.x - X;
    assert!(d > 400.0, "{d}");
    assert_ne!(worm(&sim).state, State::Grab);
}

#[test]
fn no_single_blow_or_undefended_cycle_kills_a_full_crab() {
    let mut sim = world(100.0);
    provoke(&mut sim);
    let mut lowest: f32 = 100.0;
    for _ in 0..450 {
        tick(&mut sim, Controls::default());
        lowest = lowest.min(health(&sim));
        if worm(&sim).state == State::Recover && worm(&sim).state_tick > 70 {
            break;
        }
    }
    assert!(lowest >= 100.0 - 32.0 - 0.01, "{lowest}");
    let per_blow = bobbit::tuning().strike.damage;
    assert!(per_blow < 100.0);
}

#[test]
fn its_touch_is_a_minefield_with_a_gap_between_two_worms() {
    let level = Level::test_level();
    let tuning = crabsim::Tuning::default();
    let spots: Vec<_> = level
        .foes
        .iter()
        .filter(|f| f.0 == Kind::BobbitWorm)
        .map(|f| f.1)
        .collect();
    let mut sim = Sim::with_one_crab(Vec2::new(spots[0].x, spots[0].y));
    let a = crabsim::Enemy::new(0, Kind::BobbitWorm, spots[0], &tuning);
    let b = crabsim::Enemy::new(1, Kind::BobbitWorm, spots[1], &tuning);
    let mut crab = sim.crab(P).unwrap().clone();
    let mid = (spots[0].x + spots[1].x) / 2.0;
    crab.mover.pos = Vec2::new(mid, spots[0].y);
    crab.mover.vel.x = 270.0;
    assert!(!bobbit::feels_crab(&a, &crab, &tuning));
    assert!(!bobbit::feels_crab(&b, &crab, &tuning));
    crab.mover.vel.x = 0.0;
    assert!(!bobbit::feels_crab(&a, &crab, &tuning));
    let _ = &mut sim;
}

fn walk_hash() -> Vec<u64> {
    let mut sim = world(300.0);
    let mut out = Vec::new();
    for i in 0..2400u32 {
        tick(
            &mut sim,
            Controls {
                walk: if (i / 180) % 2 == 0 { -1.0 } else { 1.0 },
                dodge_held: (i / 90) % 3 == 0,
                dodge_pressed: i % 90 == 0,
                smash_pressed: i % 37 == 0,
                block_held: i % 53 < 8,
                block_pressed: i % 53 == 0,
                ..Default::default()
            },
        );
        if i % 60 == 59 {
            out.push(sim.state_hash());
        }
    }
    out
}

#[test]
fn it_is_deterministic() {
    assert_eq!(walk_hash(), walk_hash());
}

#[test]
fn a_remote_crab_is_never_changed_by_it() {
    let mut sim = world(100.0);
    sim.crab_mut(P).unwrap().remote = true;
    let before = sim.crab(P).unwrap().fighter.clone();
    for _ in 0..600 {
        sim.tick();
    }
    assert_eq!(sim.crab(P).unwrap().fighter, before);
}

#[test]
fn it_can_be_beaten_by_a_crab_that_keeps_hitting() {
    let mut sim = world(HALF + 139.0);
    bait(&mut sim);
    sim.crab_mut(P).unwrap().mover.pos.x = X + 150.0;
    for i in 0..600 {
        tick(
            &mut sim,
            Controls {
                smash_pressed: i % 45 == 0,
                ..Default::default()
            },
        );
        if worm(&sim).state == State::Gone {
            return;
        }
    }
    panic!("never beaten: {:?}", worm(&sim));
}

// Co-op: the guest's crab is judged on the guest's device; the host sees the
// same bite once, and both worms end in the same state.
mod coop {
    use super::*;
    use crabsim::net::FakeLink;

    const HOST: PlayerId = PlayerId(0);
    const GUEST: PlayerId = PlayerId(1);

    fn build(at: Vec2, host: bool) -> Sim {
        let mut sim = Sim::new(crabsim::Tuning::default(), Level::test_level());
        sim.set_feel(Feel::Current);
        let far = Vec2::new(1150.0, 110.0);
        if host {
            sim.add_crab(HOST, far);
            sim.add_remote_crab(GUEST, at);
        } else {
            sim.add_remote_crab(HOST, far);
            sim.add_crab(GUEST, at);
        }
        sim.spawn_enemy(Kind::BobbitWorm, Vec2::new(X, 0.0));
        sim.go_online(host);
        sim
    }

    #[test]
    fn a_bite_is_judged_by_the_guest_defender_once_and_both_worms_agree() {
        let at = Vec2::new(X + 100.0, 110.0);
        let (mut host, mut guest) = (build(at, true), build(at, false));
        let mut link = FakeLink::new(3, 0, 0.0, 7);
        let full = host.tuning.crab.health;
        let (mut bitten, mut reflected) = (false, false);
        for _ in 0..420 {
            host.set_controls(HOST, Controls::default());
            guest.set_controls(GUEST, Controls::default());
            host.tick();
            guest.tick();
            link.carry(&mut [&mut host, &mut guest]);
            let local = guest.crab(GUEST).unwrap().fighter.vitals.health;
            bitten |= local < full;
            reflected |= bitten && host.crab(GUEST).unwrap().fighter.vitals.health == local;
            assert_eq!(host.crab(HOST).unwrap().fighter.vitals.health, full);
            assert!(local >= full - 34.0, "duplicate bite damage: {local}");
        }
        assert!(
            bitten && reflected,
            "bitten {bitten}, reflected {reflected}"
        );
        let (a, b) = (&host.enemies()[0], &guest.enemies()[0]);
        assert_eq!((a.pos, a.facing), (b.pos, b.facing));
        assert_eq!(a.state, b.state, "{a:?} vs {b:?}");
    }
}

#[test]
fn a_crab_held_by_a_staggered_worm_is_freed_at_once() {
    let mut sim = world(100.0);
    provoke(&mut sim);
    assert!(until(&mut sim, State::Grab, 80, Controls::default()));
    assert!(sim.crab(P).unwrap().fighter.grabbed.is_some());
    let e = &mut sim.enemies_mut()[0];
    e.state = State::Stagger;
    e.state_tick = 0;
    e.state_len = 60;
    // (The bite's hit-stop still freezes the crab for a few ticks.)
    idle(&mut sim, 12);
    assert!(
        sim.crab(P).unwrap().fighter.grabbed.is_none(),
        "the hold ends with the worm's stagger, not the hold timer"
    );
}

/// Feel pass (2026-11-11): a crab that walks straight on over the burrow is
/// still struck (the worm turns after it through the flick); one that hops at
/// the flick clears it.
#[test]
fn walking_on_over_the_burrow_is_struck_but_a_hop_clears_it() {
    let walk = Controls {
        walk: -1.0,
        ..Controls::default()
    };
    let mut sim = world(160.0);
    for _ in 0..150 {
        tick(&mut sim, walk);
    }
    assert!(health(&sim) < 100.0, "a walker over the burrow is bitten");

    let mut sim = world(160.0);
    let mut hopped = false;
    for _ in 0..150 {
        let telling = worm(&sim).state == State::Tell;
        let c = if telling && !hopped {
            hopped = true;
            Controls {
                walk: -1.0,
                jump_pressed: true,
                jump_held: true,
                ..Controls::default()
            }
        } else if hopped && worm(&sim).state == State::Tell {
            Controls {
                walk: -1.0,
                jump_held: true,
                ..Controls::default()
            }
        } else {
            walk
        };
        tick(&mut sim, c);
    }
    assert_eq!(health(&sim), 100.0, "a hop at the flick clears the lunge");
}
