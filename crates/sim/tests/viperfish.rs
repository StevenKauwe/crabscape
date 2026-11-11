//! The viperfish (`docs/design/creatures/viperfish.md`): a lunge ambusher
//! drawn to the crab's light. Crab on the lower canyon's floor, fish beside it.
use bevy_math::Vec2;
use crabsim::combat;
use crabsim::creatures::viperfish as vf;
use crabsim::enemy::State;
use crabsim::level::landmarks::Kind as Spot;
use crabsim::net::FakeLink;
use crabsim::world::cues::Cue;
use crabsim::{Controls, Kind, Level, PlayerId, Sim, Tuning};

const P: PlayerId = PlayerId(0);
const GUEST: PlayerId = PlayerId(1);
const X: f32 = 19900.0;

fn floor() -> f32 {
    Level::test_level().floor_height(X)
}

/// A crab on the floor at X, a viperfish `dx` to its right at crab-middle height.
fn arena(dx: f32, dim: bool) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(X, floor()));
    sim.spawn_enemy(Kind::Viperfish, Vec2::new(X + dx, floor() + 10.0));
    sim.crab_mut(P).unwrap().mover.lantern_dim = dim;
    sim
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

fn health(sim: &Sim) -> f32 {
    sim.crab(P).unwrap().fighter.vitals.health
}

fn extra(sim: &Sim) -> vf::State {
    match &sim.enemies()[0].creature {
        crabsim::creatures::Extra::Viperfish(s) => s.clone(),
        other => panic!("not a viperfish: {other:?}"),
    }
}

fn gap(sim: &Sim) -> f32 {
    let t = Tuning::default();
    let b = combat::body(&sim.crab(P).unwrap().mover, &t);
    let m = vf::mouth(&sim.enemies()[0]);
    m.distance(m.clamp(b.min, b.max))
}

#[test]
fn it_lives_over_the_vent_field_and_never_in_a_shelter() {
    let level = Level::test_level();
    let vents = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "vent field")
        .unwrap();
    let mine: Vec<_> = level
        .foes
        .iter()
        .filter(|f| f.0 == Kind::Viperfish)
        .collect();
    assert!(mine.len() >= 2, "at least two viperfish");
    let ys: Vec<f32> = mine.iter().map(|f| f.1.y).collect();
    assert!(
        ys.iter().any(|y| (y - ys[0]).abs() > 100.0),
        "different depths"
    );
    for (_, p) in mine {
        assert!(vents.rect.contains(*p), "{p:?} over the vent field");
        for l in level.landmarks() {
            if matches!(
                l.kind,
                Spot::Refuge | Spot::Tunnel | Spot::Mouth | Spot::Chamber { .. }
            ) {
                let near = p.clamp(l.rect.min, l.rect.max);
                assert!(near.distance(*p) >= 400.0, "{p:?} too near {}", l.name);
            }
        }
        assert!(!level.tunnel_interiors().any(|r| r.contains(*p)));
    }
}

#[test]
fn a_bright_lantern_is_noticed_from_further_than_a_dim_one() {
    for (dim, far, should) in [
        (false, 600.0, true),
        (true, 600.0, false),
        (true, 120.0, true),
    ] {
        let mut sim = arena(far, dim);
        idle(&mut sim, 30);
        let seen = sim.enemies()[0].target == Some(P);
        assert_eq!(seen, should, "dim={dim} at {far}");
    }
    // Each culture in the lantern adds reach.
    let mut sim = arena(950.0, false);
    idle(&mut sim, 30);
    assert_eq!(sim.enemies()[0].target, None);
    sim.crab_mut(P).unwrap().mover.lantern = 3;
    idle(&mut sim, 30);
    assert_eq!(sim.enemies()[0].target, Some(P));
}

#[test]
fn dimming_the_lantern_makes_it_lose_you() {
    let mut sim = arena(500.0, false);
    idle(&mut sim, 40);
    assert_eq!(sim.enemies()[0].state, State::Approach);
    sim.crab_mut(P).unwrap().mover.lantern_dim = true;
    let mut lost_at = None;
    for i in 0..200 {
        idle(&mut sim, 1);
        if sim.enemies()[0].target.is_none() {
            lost_at = Some(i);
            break;
        }
    }
    let i = lost_at.expect("never lost the crab");
    assert!(
        (100..=130).contains(&i),
        "lost after {i} ticks, about a second"
    );
    assert_eq!(sim.enemies()[0].phase(&sim.tuning), "lost");
    // It drifts back home and stays deaf to the light for a while.
    let home = sim.enemies()[0].home;
    let before = sim.enemies()[0].pos.distance(home);
    sim.crab_mut(P).unwrap().mover.lantern_dim = false;
    idle(&mut sim, 60);
    assert_eq!(sim.enemies()[0].target, None);
    assert!(sim.enemies()[0].pos.distance(home) < before);
    // Light again once it has settled, and it comes back.
    idle(&mut sim, 400);
    assert!(extra(&sim).lost == 0);
    assert_eq!(sim.enemies()[0].target, Some(P));
}

#[test]
fn the_tell_comes_before_the_blow_and_lasts_a_second_and_a_fifth() {
    let mut sim = arena(160.0, false);
    let full = health(&sim);
    let (mut tell, mut cue, mut lure_dark) = (0, false, 0);
    for _ in 0..400 {
        idle(&mut sim, 1);
        let e = &sim.enemies()[0];
        if e.state == State::Tell {
            tell += 1;
            if e.phase(&sim.tuning) == "dark" {
                lure_dark += 1;
            }
            assert_eq!(health(&sim), full, "no damage during the tell");
        }
        if e.state == State::Attack {
            break;
        }
    }
    cue |= sim
        .cues()
        .iter()
        .any(|ev| matches!(ev.cue, Cue::Tell { crab, .. } if crab == P));
    assert_eq!(tell, vf::TELL_TICKS);
    assert_eq!(lure_dark, vf::COCKED_TICKS, "lure dark for the last 0.4 s");
    assert!(cue, "a Tell cue for the crab");
    assert_eq!(sim.enemies()[0].state, State::Attack);
}

#[test]
fn an_unanswered_lunge_bites_once_for_fourteen_then_it_hangs_open_jawed() {
    let mut sim = arena(160.0, false);
    let full = health(&sim);
    let mut recovering = 0;
    for _ in 0..500 {
        idle(&mut sim, 1);
        if sim.enemies()[0].state == State::Recover {
            recovering += 1;
        }
        if recovering == 1 {
            let h = health(&sim);
            assert_eq!(h, full - 14.0, "one bite, 14 damage");
            assert!(sim.crab(P).unwrap().fighter.vitals.poise <= 100.0 - 20.0 + 0.5);
        }
        if recovering >= vf::RECOVER_TICKS as usize {
            break;
        }
    }
    assert_eq!(recovering, vf::RECOVER_TICKS as usize);
    assert!(sim.crab(P).unwrap().fighter.venom.is_empty(), "no venom");
}

#[test]
fn a_parry_in_the_window_takes_nothing_and_staggers_it() {
    let mut sim = arena(160.0, false);
    let full = health(&sim);
    for _ in 0..600 {
        let e = &sim.enemies()[0];
        let press = e.state == State::Attack && gap(&sim) < 45.0;
        tick(
            &mut sim,
            Controls {
                block_held: press,
                block_pressed: press,
                ..Default::default()
            },
        );
        if sim.crab(P).unwrap().events.parried {
            assert_eq!(health(&sim), full);
            assert_eq!(sim.enemies()[0].state, State::Stagger);
            assert_eq!(sim.enemies()[0].state_len, vf::tuning().parried_ticks);
            return;
        }
    }
    panic!("no parry: {:?}", sim.enemies()[0]);
}

#[test]
fn a_block_takes_a_fifth_and_still_costs_poise() {
    let mut sim = arena(160.0, false);
    let t = Tuning::default();
    let full = health(&sim);
    for _ in 0..600 {
        tick(
            &mut sim,
            Controls {
                block_held: true,
                ..Default::default()
            },
        );
        if sim.crab(P).unwrap().events.blocked {
            assert!((full - health(&sim) - 14.0 * t.block_damage).abs() < 0.01);
            assert!(sim.crab(P).unwrap().fighter.vitals.poise < 100.0);
            return;
        }
    }
    panic!("never hit the guard");
}

#[test]
fn a_dodge_through_the_lunge_takes_nothing() {
    let mut sim = arena(160.0, false);
    let full = health(&sim);
    let mut dodged = false;
    for _ in 0..600 {
        let e = &sim.enemies()[0];
        let go = !dodged && e.state == State::Tell && e.state_tick + 3 >= e.state_len;
        dodged |= go;
        tick(
            &mut sim,
            Controls {
                walk: if dodged { -1.0 } else { 0.0 },
                dodge_pressed: go,
                ..Default::default()
            },
        );
        if dodged && sim.enemies()[0].state == State::Recover {
            assert_eq!(health(&sim), full);
            return;
        }
    }
    panic!("no dodge: {:?}", sim.enemies()[0]);
}

#[test]
fn the_crabs_blow_breaks_its_poise_and_it_staggers() {
    let mut sim = arena(140.0, false);
    idle(&mut sim, 40);
    let mut staggered = false;
    for i in 0..60 {
        tick(
            &mut sim,
            Controls {
                walk: if i == 0 { 0.05 } else { 0.0 },
                smash_pressed: i == 0,
                ..Default::default()
            },
        );
        staggered |= sim.enemies()[0].state == State::Stagger;
    }
    assert!(staggered, "{:?}", sim.enemies()[0]);
    assert!(sim.enemies()[0].vitals.health < vf::tuning().vitals.health);
}

#[test]
fn it_flees_at_half_and_a_second_bout_can_kill_it() {
    let mut sim = arena(140.0, false);
    idle(&mut sim, 40);
    let mut fled = false;
    for i in 0..600 {
        tick(
            &mut sim,
            Controls {
                walk: if i % 30 == 0 { 0.05 } else { 0.0 },
                jab_pressed: i % 30 == 0,
                ..Default::default()
            },
        );
        fled |= sim.enemies()[0].state == State::Flee;
        if fled {
            break;
        }
    }
    assert!(fled, "driven off: {:?}", sim.enemies()[0]);
    assert!(sim.enemies()[0].vitals.health <= 0.5 * vf::tuning().vitals.health);
    // Already fled once: it fights on and dies.
    let mut sim = arena(140.0, false);
    sim.enemies_mut()[0].fled = true;
    idle(&mut sim, 40);
    for i in 0..600 {
        tick(
            &mut sim,
            Controls {
                walk: if i % 50 == 0 { 0.05 } else { 0.0 },
                smash_pressed: i % 50 == 0,
                ..Default::default()
            },
        );
        if sim.enemies()[0].state == State::Gone {
            assert!(!vf::DROPS.is_empty());
            return;
        }
    }
    panic!("never died: {:?}", sim.enemies()[0]);
}

#[test]
fn it_can_be_escaped_by_dimming_by_stepping_out_of_the_aim_and_by_shelter() {
    // Out of the aim: the crab leaves once the lure goes dark and the aim is locked.
    let mut sim = arena(160.0, false);
    let full = health(&sim);
    let mut moved = false;
    for _ in 0..600 {
        let e = &sim.enemies()[0];
        if !moved && e.state == State::Tell && e.state_tick > vf::TELL_TICKS - vf::COCKED_TICKS + 4
        {
            moved = true;
            sim.crab_mut(P).unwrap().mover.pos.x += 400.0;
        }
        idle(&mut sim, 1);
        if moved && sim.enemies()[0].state == State::Recover {
            break;
        }
    }
    assert!(moved);
    assert_eq!(health(&sim), full, "the locked lunge missed");
    // Shelter: standing in a refuge, it never even notices.
    let level = Level::test_level();
    let home = level.home;
    let mut sim = Sim::with_one_crab(home.center());
    sim.spawn_enemy(Kind::Viperfish, home.center() + Vec2::new(300.0, 0.0));
    let full = health(&sim);
    for _ in 0..600 {
        idle(&mut sim, 1);
        assert_ne!(sim.enemies()[0].state, State::Tell);
    }
    assert_eq!(health(&sim), full);
}

#[test]
fn it_gives_up_after_three_lunges() {
    let mut sim = arena(160.0, false);
    let mut lunges = 0;
    let mut was = State::Idle;
    for _ in 0..3000 {
        idle(&mut sim, 1);
        let s = sim.enemies()[0].state;
        if s == State::Attack && was != State::Attack {
            lunges += 1;
        }
        if s == State::Idle && lunges > 0 {
            break;
        }
        was = s;
    }
    assert_eq!(lunges, vf::MAX_LUNGES);
    assert_eq!(sim.enemies()[0].target, None);
}

#[test]
fn no_blow_and_no_undefended_run_kills_a_full_crab() {
    let mut sim = arena(160.0, false);
    let mut last = health(&sim);
    for _ in 0..4000 {
        idle(&mut sim, 1);
        let h = health(&sim);
        assert!(last - h <= 14.0 + 0.01, "a single blow over 14");
        last = h;
    }
    assert!(health(&sim) >= 100.0 - 14.0 * vf::MAX_LUNGES as f32 - 0.01);
    assert!(sim.crab(P).unwrap().fighter.dead.is_none());
}

#[test]
fn it_never_enters_a_tunnel() {
    let level = Level::test_level();
    let tunnels: Vec<_> = level
        .landmarks()
        .into_iter()
        .filter(|l| matches!(l.kind, Spot::Tunnel | Spot::Mouth))
        .collect();
    assert!(!tunnels.is_empty());
    for l in tunnels.iter().take(4) {
        let mut sim = Sim::with_one_crab(l.rect.center());
        let at = Vec2::new(l.rect.center().x, l.rect.max.y + 200.0);
        sim.spawn_enemy(Kind::Viperfish, at);
        for _ in 0..1200 {
            idle(&mut sim, 1);
            let p = sim.enemies()[0].pos;
            assert!(
                !sim.level.tunnel_interiors().any(|r| r.contains(p)),
                "{}",
                l.name
            );
        }
    }
}

fn scripted(i: u32) -> Controls {
    Controls {
        walk: match (i / 240) % 3 {
            0 => 1.0,
            1 => -1.0,
            _ => 0.0,
        },
        block_held: i % 400 > 300,
        block_pressed: i % 400 == 301,
        smash_pressed: i % 500 == 100,
        ..Default::default()
    }
}

fn dim_at(i: u32) -> bool {
    (i / 700) % 2 == 1
}

#[test]
fn two_sims_agree_on_every_hash() {
    let build = || {
        let mut sim = arena(300.0, false);
        sim.spawn_enemy(Kind::Viperfish, Vec2::new(X - 500.0, floor() + 200.0));
        sim
    };
    let (mut a, mut b) = (build(), build());
    for i in 0..2400 {
        for s in [&mut a, &mut b] {
            s.crab_mut(P).unwrap().mover.lantern_dim = dim_at(i);
            tick(s, scripted(i));
        }
        if i % 60 == 59 {
            assert_eq!(a.state_hash(), b.state_hash(), "tick {i}");
        }
    }
}

fn game() -> (Sim, Sim, FakeLink) {
    let setup = |sim: &mut Sim| {
        sim.spawn_enemy(Kind::Viperfish, Vec2::new(X + 160.0, floor() + 10.0));
    };
    let world = || Sim::new(Tuning::default(), Level::test_level());
    let far = Vec2::new(1150.0, 110.0);
    let at = Vec2::new(X, floor());
    let mut host = world();
    setup(&mut host);
    host.add_crab(P, far);
    host.add_remote_crab(GUEST, at);
    host.go_online(true);
    let mut guest = world();
    setup(&mut guest);
    guest.add_remote_crab(P, far);
    guest.add_crab(GUEST, at);
    guest.go_online(false);
    (host, guest, FakeLink::steady(0))
}

#[test]
fn co_op_the_defenders_device_judges_the_bite_once_and_both_agree() {
    let (mut host, mut guest, mut link) = game();
    let start = guest.crab(GUEST).unwrap().fighter.vitals.health;
    let (mut bitten, mut agreed) = (false, false);
    let mut prev = start;
    for _ in 0..700 {
        host.set_controls(P, Controls::default());
        guest.set_controls(GUEST, Controls::default());
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        let local = guest.crab(GUEST).unwrap().fighter.vitals.health;
        bitten |= local < start;
        agreed |= bitten && host.crab(GUEST).unwrap().fighter.vitals.health == local;
        assert!(prev - local <= 14.0 + 0.01, "duplicate bite damage");
        prev = local;
        assert_eq!(host.crab(P).unwrap().fighter.vitals.health, 100.0);
    }
    assert!(bitten && agreed);
    assert_eq!(host.enemies()[0].state, guest.enemies()[0].state);
}

#[test]
fn a_remote_crab_is_never_changed_by_the_fish_on_the_host() {
    let mut sim = arena(160.0, false);
    sim.crab_mut(P).unwrap().remote = true;
    let before = sim.crab(P).unwrap().fighter.clone();
    idle(&mut sim, 800);
    assert_eq!(sim.crab(P).unwrap().fighter, before);
}
