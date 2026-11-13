//! The shore crab: a weak, passive roamer. It walks about its home, never
//! starts a fight, pinches back (big tell, long recover) once struck, lets go
//! of a crab that leaves, and comes back 90 s after it is beaten.
use bevy_math::Vec2;
use crabsim::creatures::shore_crab;
use crabsim::economy::win_xp;
use crabsim::enemy::State;
use crabsim::{Controls, Enemy, Kind, PlayerId, Sim};

const P: PlayerId = PlayerId(0);
const FLOOR: f32 = 110.0;
/// Where the crab's home is; the crab starts well away.
const HOME: f32 = 1000.0;

fn world(crab_x: f32) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(crab_x, FLOOR));
    sim.spawn_enemy(Kind::ShoreCrab, Vec2::new(HOME, FLOOR));
    sim
}

fn sc(sim: &Sim) -> &Enemy {
    sim.enemies()
        .iter()
        .find(|e| e.kind == Kind::ShoreCrab)
        .expect("a shore crab")
}

fn health(sim: &Sim) -> f32 {
    sim.crab(P).unwrap().fighter.vitals.health
}

fn xp(sim: &Sim) -> f32 {
    sim.crab(P).unwrap().mover.fight_xp
}

fn tick(sim: &mut Sim, c: Controls) {
    sim.set_controls(P, c);
    sim.tick();
}

/// One tick of the bot's read-the-tell, dodge, punish pattern against the
/// shore crab (crates/sim/tests/common/bot.rs `fight`). Returns false once it
/// is beaten.
fn fight_step(sim: &mut Sim) -> bool {
    let e = sc(sim);
    if e.state == State::Gone {
        return false;
    }
    let me = sim.crab(P).unwrap().mover.pos.x;
    let dir = (e.pos.x - me).signum();
    let gap = (e.pos.x - me).abs();
    let close = gap < 70.0;
    let answer = e.state == State::Tell && e.state_tick + 4 >= e.state_len;
    let punish = matches!(e.state, State::Recover | State::Stagger);
    let first = !matches!(
        e.state,
        State::Tell | State::Attack | State::Recover | State::Stagger
    );
    let c = Controls {
        walk: if answer {
            -dir
        } else if close {
            0.0
        } else {
            0.8 * dir
        },
        dodge_pressed: answer,
        smash_pressed: close && (punish && e.state_tick.is_multiple_of(8) || first),
        ..Controls::default()
    };
    tick(sim, c);
    true
}

#[test]
fn it_is_a_passive_roamer_with_a_leash() {
    const { assert!(shore_crab::PASSIVE) };
    assert_eq!(shore_crab::LEASH, 120.0);
    let level = crabsim::Level::test_level();
    assert!(
        crabsim::creatures::spots(&level)
            .iter()
            .all(|(k, _)| *k != Kind::ShoreCrab)
    );
}

#[test]
fn it_roams_within_its_leash_and_visits_both_sides_of_home() {
    let mut sim = world(HOME - 900.0);
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    let mut picked = false;
    for _ in 0..60 * 120 {
        tick(&mut sim, Controls::default());
        let e = sc(&sim);
        assert_eq!(e.state, State::Idle, "nothing wakes it");
        lo = lo.min(e.pos.x);
        hi = hi.max(e.pos.x);
        picked |= shore_crab::phase(e) == Some("pick");
    }
    println!("roamed {:.0} to {:.0} of home {HOME}", lo - HOME, hi - HOME);
    assert!(lo < HOME - 20.0 && hi > HOME + 20.0, "{lo} {hi}");
    assert!(lo >= HOME - shore_crab::LEASH - 4.0, "{lo}");
    assert!(hi <= HOME + shore_crab::LEASH + 4.0, "{hi}");
    assert!(picked, "it pauses to pick at the floor");
}

#[test]
fn a_crab_standing_beside_it_is_never_touched() {
    let mut sim = world(HOME + 20.0);
    let full = health(&sim);
    for _ in 0..60 * 120 {
        tick(&mut sim, Controls::default());
        assert_eq!(sc(&sim).state, State::Idle);
    }
    assert_eq!(health(&sim), full);
    // And one walking past, back and forth through it.
    let mut sim = world(HOME - 100.0);
    for i in 0..60 * 120 {
        let walk = if (i / 240) % 2 == 0 { 0.6 } else { -0.6 };
        tick(
            &mut sim,
            Controls {
                walk,
                ..Controls::default()
            },
        );
    }
    assert_eq!(health(&sim), full);
    assert_eq!(sc(&sim).state, State::Idle);
}

#[test]
fn a_crab_gathering_beside_it_is_never_touched() {
    let level = crabsim::Level::test_level();
    let probe = Sim::new(crabsim::Tuning::default(), level.clone());
    let node = probe
        .placed_nodes()
        .iter()
        .find(|n| n.pos.x < 600.0 && (n.pos.y - level.floor_height(n.pos.x)).abs() < 20.0)
        .copied()
        .expect("a node on foot");
    let mut sim = Sim::new(crabsim::Tuning::default(), level);
    let at = Vec2::new(node.pos.x, sim.level.floor_height(node.pos.x));
    sim.add_crab(P, at);
    sim.spawn_enemy(Kind::ShoreCrab, at + Vec2::new(30.0, 0.0));
    let full = health(&sim);
    for _ in 0..12 * 120 {
        tick(
            &mut sim,
            Controls {
                gather_held: true,
                ..Controls::default()
            },
        );
        let c = sim.crab(P).unwrap();
        assert!(health(&sim) >= full && !c.events.hurt && !c.events.staggered);
        assert_eq!(sc(&sim).state, State::Idle);
    }
}

#[test]
fn struck_it_fights_back_and_a_scripted_level_one_crab_beats_it() {
    let mut sim = world(HOME - 160.0);
    let (full, xp0) = (health(&sim), xp(&sim));
    let mut pinches = 0;
    let mut last = sc(&sim).state;
    let mut ticks = 0;
    let mut worst: f32 = 0.0;
    while fight_step(&mut sim) {
        ticks += 1;
        let e = sc(&sim);
        if e.state == State::Tell && last != State::Tell {
            pinches += 1;
        }
        last = e.state;
        worst = worst.max(full - health(&sim));
        assert!(ticks < 15 * 120, "not beaten in 15 s");
    }
    let secs = ticks as f32 / 120.0;
    let t = sim.tuning.clone();
    println!(
        "beaten in {secs:.1} s, {pinches} tells, lost {:.2}, earned {:.1} Fighting XP (win {})",
        full - health(&sim),
        xp(&sim) - xp0,
        win_xp(Kind::ShoreCrab, &t)
    );
    assert!(pinches >= 1, "it fought back (a pinch was telegraphed)");
    assert!(xp(&sim) > xp0 + win_xp(Kind::ShoreCrab, &t), "Fighting XP");
    assert!(health(&sim) > 0.0);
}

#[test]
fn an_undefended_pinch_costs_a_quarter_of_the_hermits_and_a_win_pays_like_the_shrimp() {
    assert!((shore_crab::PINCH_DAMAGE - 9.0 / 4.0).abs() < 0.01);
    let t = Sim::with_one_crab(Vec2::ZERO).tuning.clone();
    assert_eq!(win_xp(Kind::ShoreCrab, &t), win_xp(Kind::Snapper, &t));
    // The tell is long and the recover longer than the blow: readable.
    const { assert!(shore_crab::PINCH_TELL >= 36 && shore_crab::PINCH_RECOVER >= 40) };
    const { assert!(shore_crab::PINCH_ACTIVE <= 4) };
}

/// Strike it once, then stand off at `gap` and watch what it does.
fn provoke(gap: f32, walk_away: bool, ticks: u32) -> Sim {
    let mut sim = world(HOME - 60.0);
    // Walk up and strike it once.
    for _ in 0..600 {
        let e = sc(&sim);
        if e.target.is_some() {
            break;
        }
        let me = sim.crab(P).unwrap().mover.pos.x;
        let close = (e.pos.x - me).abs() < 60.0;
        tick(
            &mut sim,
            Controls {
                walk: if close { 0.0 } else { 0.8 },
                smash_pressed: close,
                ..Controls::default()
            },
        );
    }
    assert!(sc(&sim).target.is_some(), "a blow wakes it");
    let _ = gap;
    for _ in 0..ticks {
        let c = if walk_away {
            Controls {
                walk: -1.0,
                ..Controls::default()
            }
        } else {
            Controls::default()
        };
        tick(&mut sim, c);
    }
    sim
}

#[test]
fn a_crab_that_walks_away_is_not_followed_and_it_goes_back_to_roaming() {
    let mut sim = provoke(0.0, true, 6 * 120);
    let me = sim.crab(P).unwrap().mover.pos.x;
    assert!((sc(&sim).pos.x - me).abs() > 200.0, "walked 200 away");
    for _ in 0..5 * 120 {
        tick(&mut sim, Controls::default());
    }
    let e = sc(&sim);
    assert_eq!(e.state, State::Idle);
    assert!(e.target.is_none());
    assert!((e.pos.x - e.home.x).abs() <= shore_crab::LEASH + 4.0);
}

#[test]
fn it_gives_up_after_ten_seconds_without_a_blow() {
    let mut sim = provoke(0.0, false, 0);
    // Stand just out of its reach, never striking again.
    let mut calm_at = None;
    for i in 0..14 * 120 {
        let e = sc(&sim);
        let me = sim.crab(P).unwrap().mover.pos.x;
        let gap = e.pos.x - me;
        // Keep 100 away so it never reaches (and never loses interest by
        // distance).
        let walk = if gap.abs() < 100.0 {
            -gap.signum()
        } else if gap.abs() > 110.0 {
            gap.signum() * 0.3
        } else {
            0.0
        };
        tick(
            &mut sim,
            Controls {
                walk,
                block_held: true,
                ..Controls::default()
            },
        );
        if calm_at.is_none() && sc(&sim).target.is_none() {
            calm_at = Some(i);
        }
    }
    let at = calm_at.expect("it gave up");
    println!("gave up after {:.1} s", at as f32 / 120.0);
    assert!(at as f32 / 120.0 > 7.0 && at as f32 / 120.0 <= 11.0, "{at}");
}

#[test]
fn it_respawns_ninety_seconds_after_it_is_beaten() {
    let mut sim = world(HOME - 160.0);
    while fight_step(&mut sim) {}
    let beaten = sim.tick_count();
    assert_eq!(sc(&sim).state, State::Gone);
    // Out of its way, so nothing crowds the spot.
    sim.crab_mut(P).unwrap().mover.pos.x = HOME - 900.0;
    let mut back = None;
    for _ in 0..100 * 120 {
        tick(&mut sim, Controls::default());
        if back.is_none() && sc(&sim).state == State::Idle {
            back = Some(sim.tick_count() - beaten);
        }
    }
    let secs = back.expect("it respawned") as f32 / 120.0;
    println!("back after {secs:.1} s");
    assert!((89.0..=92.0).contains(&secs), "{secs}");
    assert_eq!(sc(&sim).vitals.health, shore_crab::HEALTH);
}

fn run_hash() -> Vec<u64> {
    let mut sim = world(HOME - 160.0);
    let mut out = Vec::new();
    for i in 0..3000 {
        if !fight_step(&mut sim) {
            tick(&mut sim, Controls::default());
        }
        if i % 60 == 0 {
            out.push(sim.state_hash());
        }
    }
    out
}

#[test]
fn two_sims_with_the_same_inputs_agree_every_half_second() {
    assert_eq!(run_hash(), run_hash());
}

#[test]
fn struck_and_left_standing_in_reach_it_pinches_for_a_quarter_of_the_hermits() {
    let mut sim = provoke(0.0, false, 0);
    let before = health(&sim);
    let mut tells = 0;
    let mut last = sc(&sim).state;
    for _ in 0..6 * 120 {
        tick(&mut sim, Controls::default());
        let e = sc(&sim);
        tells += (e.state == State::Tell && last != State::Tell) as u32;
        last = e.state;
        if health(&sim) < before {
            break;
        }
    }
    assert!(tells >= 1, "the tell comes first");
    let lost = before - health(&sim);
    println!("a pinch costs {lost:.2}");
    assert!(
        lost > 0.0 && lost <= shore_crab::PINCH_DAMAGE + 0.01,
        "{lost}"
    );
}
