//! The hermit crab: pinch and rap, the shell with its stinging anemones, the
//! soft claws when it is out, and the sulk. Headless, scripted crab.
use bevy_math::Vec2;
use crabsim::combat::{Claw, Swing};
use crabsim::creatures::{Extra, hermit};
use crabsim::enemy::State;
use crabsim::net::FakeLink;
use crabsim::world::cues::Cue;
use crabsim::{Controls, Enemy, Feel, Kind, Level, PlayerId, Sim, Tuning};

const P: PlayerId = PlayerId(0);
const G: PlayerId = PlayerId(1);
const X: f32 = 1000.0;
const FLOOR: f32 = 110.0;

fn fight(gap: f32) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(X, FLOOR));
    sim.spawn_enemy(Kind::HermitCrab, Vec2::new(X + gap, FLOOR));
    for _ in 0..30 {
        sim.tick();
    }
    sim
}

fn h(sim: &Sim) -> &Enemy {
    sim.enemies()
        .iter()
        .find(|e| e.kind == Kind::HermitCrab)
        .expect("a hermit")
}

fn hm(sim: &mut Sim) -> &mut Enemy {
    sim.enemies_mut()
        .iter_mut()
        .find(|e| e.kind == Kind::HermitCrab)
        .expect("a hermit")
}

fn tick(sim: &mut Sim, c: Controls) {
    sim.set_controls(P, c);
    sim.tick();
}

fn st(sim: &Sim) -> hermit::State {
    hermit::state_of(h(sim))
}

fn edit(sim: &mut Sim, f: impl FnOnce(&mut hermit::State)) {
    let mut s = st(sim);
    f(&mut s);
    hm(sim).creature = Extra::Hermit(s);
}

/// It will never shut in on its own (the tests that need it out).
fn stubborn(sim: &mut Sim) {
    edit(sim, |s| s.cooldown = u32::MAX / 2);
}

fn enter(sim: &mut Sim, state: State, len: u32) {
    let e = hm(sim);
    e.state = state;
    e.state_tick = 0;
    e.state_len = len;
}

fn health(sim: &Sim) -> f32 {
    sim.crab(P).unwrap().fighter.vitals.health
}

#[test]
fn it_is_placed_on_the_far_basin_rubble_away_from_shelter() {
    let level = Level::test_level();
    let spots: Vec<_> = level
        .foes
        .iter()
        .filter(|(k, _)| *k == Kind::HermitCrab)
        .collect();
    assert_eq!(spots.len(), 1, "one hermit on the far basin");
    let at = spots[0].1;
    assert!(at.x > 2600.0);
    for c in &level.crevices {
        assert!(at.distance(c.den) > 400.0, "{c:?}");
    }
    assert_eq!(at.y, level.floor_height(at.x));
    for l in level.landmarks() {
        use crabsim::level::landmarks::Kind as L;
        let d = at.clamp(l.rect.min, l.rect.max).distance(at);
        match l.kind {
            L::Refuge => assert!(d >= 180.0, "{} only {d} away", l.name),
            L::Tunnel | L::Mouth | L::Chamber { .. } => {
                assert!(d >= 100.0, "{} only {d} away", l.name)
            }
            _ => {}
        }
    }
}

#[test]
fn the_tell_comes_before_each_blow_and_a_cue_is_sent() {
    let mut sim = fight(112.0);
    stubborn(&mut sim);
    let (mut run, mut blows, mut cue) = (0, 0, false);
    let mut last = h(&sim).state;
    for _ in 0..2400 {
        tick(
            &mut sim,
            Controls {
                block_held: false,
                ..Default::default()
            },
        );
        cue |= sim
            .cues()
            .iter()
            .any(|e| matches!(e.cue, Cue::Tell { crab, .. } if crab == P));
        let e = h(&sim);
        if e.state == State::Tell {
            run += 1;
        }
        if e.state == State::Attack && last == State::Tell {
            let need = if hermit::state_of(e).next == hermit::Move::Pinch {
                hermit::PINCH_TELL
            } else {
                hermit::RAP_TELL
            };
            assert!(run >= need, "tell was {run}, wanted {need}");
            blows += 1;
        }
        if e.state != State::Tell {
            run = 0;
        }
        last = e.state;
        if blows >= 3 {
            break;
        }
    }
    assert!(blows >= 3 && cue);
}

#[test]
fn a_parry_in_the_window_takes_nothing_and_staggers_it() {
    let mut sim = fight(112.0);
    stubborn(&mut sim);
    let hp = health(&sim);
    for _ in 0..3000 {
        let e = h(&sim);
        let go = e.state == State::Tell && e.state_tick + 4 >= e.state_len;
        tick(
            &mut sim,
            Controls {
                block_held: go,
                block_pressed: go,
                ..Default::default()
            },
        );
        if sim.crab(P).unwrap().events.parried {
            assert_eq!(health(&sim), hp);
            assert_eq!(h(&sim).state, State::Stagger);
            assert!(sim.crab(P).unwrap().fighter.venom.is_empty());
            return;
        }
    }
    panic!("never parried");
}

#[test]
fn an_unanswered_pinch_costs_nine_and_a_rap_costs_six_and_venom_unless_blocked() {
    for (mv, damage, block) in [
        (hermit::Move::Pinch, 9.0, false),
        (hermit::Move::Rap, 6.0, false),
        (hermit::Move::Rap, 6.0 * 0.2, true),
    ] {
        let mut sim = fight(112.0);
        stubborn(&mut sim);
        let hp = health(&sim);
        let mut done = false;
        for _ in 0..4000 {
            let tell = matches!(h(&sim).state, State::Tell | State::Attack);
            // Let it pick; hold block through the tell we want.
            tick(
                &mut sim,
                Controls {
                    block_held: block && tell,
                    ..Default::default()
                },
            );
            let e = h(&sim);
            if e.state == State::Recover && hermit::state_of(e).next == mv {
                let lost = hp - health(&sim);
                assert!((lost - damage).abs() < 0.5, "{mv:?} lost {lost}");
                let venom = !sim.crab(P).unwrap().fighter.venom.is_empty();
                assert_eq!(venom, mv == hermit::Move::Rap && !block, "{mv:?} venom");
                done = true;
                break;
            }
            if e.state == State::Recover {
                // The other move came first: reset and wait for ours.
                sim.crab_mut(P).unwrap().fighter.vitals.health = hp;
                sim.crab_mut(P).unwrap().fighter.venom.clear();
            }
        }
        assert!(done, "{mv:?} never landed");
    }
}

#[test]
fn hits_while_it_is_out_and_committed_do_half_again() {
    let mut open = fight(60.0);
    stubborn(&mut open);
    let before = h(&open).vitals.health;
    enter(&mut open, State::Recover, 600);
    tick(
        &mut open,
        Controls {
            smash_pressed: true,
            ..Default::default()
        },
    );
    for _ in 0..40 {
        tick(&mut open, Controls::default());
    }
    let soft = before - h(&open).vitals.health;
    assert!((soft - 18.0).abs() < 0.01, "soft parts took {soft}");

    let mut idle = fight(60.0);
    stubborn(&mut idle);
    // Idle, not yet noticed: a plain 12.
    enter(&mut idle, State::Idle, 0);
    let before = h(&idle).vitals.health;
    tick(
        &mut idle,
        Controls {
            smash_pressed: true,
            ..Default::default()
        },
    );
    for _ in 0..40 {
        tick(&mut idle, Controls::default());
    }
    let plain = before - h(&idle).vitals.health;
    assert!((plain - 12.0).abs() < 0.01, "plain blow took {plain}");
}

#[test]
fn the_withdrawn_shell_takes_a_tenth_cannot_stagger_and_stings_the_attacker() {
    let mut sim = fight(60.0);
    hermit::withdraw(hm(&mut sim));
    assert_eq!(hermit::phase(h(&sim)), Some("withdrawn"));
    let hp = h(&sim).vitals.health;
    tick(
        &mut sim,
        Controls {
            smash_pressed: true,
            ..Default::default()
        },
    );
    for _ in 0..40 {
        tick(&mut sim, Controls::default());
    }
    let s = st(&sim);
    assert!(
        (hp - h(&sim).vitals.health - 1.2).abs() < 0.01,
        "health {} -> {}",
        hp,
        h(&sim).vitals.health
    );
    assert_eq!(h(&sim).state, State::Idle);
    assert!(
        !sim.crab(P).unwrap().fighter.venom.is_empty(),
        "an unblocked attacker is stung"
    );
    assert!(health(&sim) < 100.0);
    assert!(s.flare > 0 || s.sting_gap > 0);
    // It comes out after its 1.5 s with its real health, not the shield.
    for _ in 0..200 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(st(&sim).posture, hermit::Posture::Out);
    assert!((h(&sim).vitals.health - (hp - 1.2)).abs() < 0.01);
}

fn swinging(sim: &mut Sim) {
    let t = Tuning::default();
    let c = &t.crusher;
    let _ = c;
    sim.crab_mut(P).unwrap().fighter.swing = Some(Swing {
        claw: Claw::Crusher,
        tick: 20,
        hits: vec![],
        counter: false,
        windup: 14,
        charged: 0,
        charging: false,
        power: 1.0,
        aim: 0.0,
        full_charge: 0,
        charge_bonus: 0.0,
    });
}

#[test]
fn blocking_stops_the_counter_sting_venom() {
    for block in [false, true] {
        let mut sim = fight(60.0);
        hermit::withdraw(hm(&mut sim));
        swinging(&mut sim);
        sim.crab_mut(P).unwrap().fighter.blocking = block;
        let e = h(&sim).clone();
        let mut blows = Vec::new();
        hermit::sting(
            &e,
            std::slice::from_mut(sim.crab_mut(P).unwrap()),
            &Tuning::default(),
            &mut blows,
        );
        assert_eq!(blows.len(), 1);
        let venom = !sim.crab(P).unwrap().fighter.venom.is_empty();
        assert_eq!(venom, !block, "block={block}");
    }
}

fn reset(sim: &mut Sim) {
    let e = hm(sim);
    e.vitals.health = 50.0;
    e.vitals.poise = 40.0;
    e.vitals.stagger = 0;
    e.vitals.hitstop = 0;
    e.vel_x = 0.0;
    e.pos.x = X + 60.0;
    e.creature = Extra::Hermit(hermit::State::default());
    enter(sim, State::Idle, 0);
    let c = sim.crab_mut(P).unwrap();
    c.fighter.vitals.health = 100.0;
    c.fighter.venom.clear();
    c.fighter.swing = None;
    c.mover.pos.x = X;
    c.mover.vel.x = 0.0;
}

#[test]
fn a_jab_lands_before_it_can_shut_in_but_a_smash_finds_the_shell_sometimes() {
    let mut sim = fight(60.0);
    let (mut jabs, mut stung, mut spared) = (0, 0, 0);
    for _ in 0..24 {
        reset(&mut sim);
        tick(
            &mut sim,
            Controls {
                jab_pressed: true,
                ..Default::default()
            },
        );
        for _ in 0..14 {
            tick(&mut sim, Controls::default());
        }
        let real = h(&sim).vitals.health;
        jabs += (real < 50.0) as u32;
        for _ in 0..400 {
            tick(&mut sim, Controls::default());
        }
    }
    for _ in 0..24 {
        reset(&mut sim);
        tick(
            &mut sim,
            Controls {
                smash_pressed: true,
                ..Default::default()
            },
        );
        for _ in 0..50 {
            tick(&mut sim, Controls::default());
        }
        if sim.crab(P).unwrap().fighter.venom.is_empty() {
            spared += 1;
        } else {
            stung += 1;
        }
        for _ in 0..400 {
            tick(&mut sim, Controls::default());
        }
    }
    assert!(jabs >= 22, "jabs landed {jabs}/24");
    assert!(
        stung >= 6 && spared >= 1,
        "stung {stung}, spared {spared} of 24"
    );
}

#[test]
fn poise_breaks_under_two_smashes() {
    let mut sim = fight(60.0);
    stubborn(&mut sim);
    enter(&mut sim, State::Idle, 0);
    let mut staggered = false;
    for _ in 0..400 {
        let c = sim.crab(P).unwrap();
        let gap = h(&sim).pos.x - c.mover.pos.x;
        let ready = c.mover.grounded && c.fighter.swing.is_none() && gap < 85.0;
        tick(
            &mut sim,
            Controls {
                walk: if gap > 80.0 { 0.8 } else { 0.0 },
                smash_pressed: ready,
                ..Default::default()
            },
        );
        staggered |= h(&sim).state == State::Stagger;
    }
    assert!(staggered);
}

#[test]
fn hurt_to_thirty_percent_it_sulks_until_the_crab_leaves_then_mends() {
    let mut sim = fight(300.0);
    stubborn(&mut sim);
    hm(&mut sim).vitals.health = 14.0;
    for _ in 0..30 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(st(&sim).posture, hermit::Posture::Sulking);
    // Still near: it stays shut in and does not attack.
    let hp = health(&sim);
    for _ in 0..1200 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(st(&sim).posture, hermit::Posture::Sulking);
    assert_eq!(health(&sim), hp);
    // The crab walks off.
    for _ in 0..1500 {
        tick(
            &mut sim,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
        if st(&sim).posture == hermit::Posture::Out {
            break;
        }
    }
    assert_eq!(st(&sim).posture, hermit::Posture::Out);
    assert_eq!(h(&sim).vitals.health, 50.0);
}

#[test]
fn it_can_be_finished_when_staggered_low() {
    let mut sim = fight(60.0);
    stubborn(&mut sim);
    hm(&mut sim).vitals.health = 10.0;
    enter(&mut sim, State::Stagger, 300);
    tick(
        &mut sim,
        Controls {
            smash_pressed: true,
            ..Default::default()
        },
    );
    for _ in 0..40 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(h(&sim).state, State::Gone);
    assert_eq!(h(&sim).deaths, 1);
    assert_eq!(hermit::DROPS, &["whelk shell", "anemone"]);
}

#[test]
fn a_scripted_crab_beats_or_drives_it_off() {
    let mut sim = fight(150.0);
    let mut out = false;
    for i in 0..6000 {
        let e = h(&sim);
        let close = (e.pos.x - sim.crab(P).unwrap().mover.pos.x).abs() < 95.0;
        let block = e.state == State::Tell && e.state_tick + 4 >= e.state_len;
        tick(
            &mut sim,
            Controls {
                walk: if close { 0.0 } else { 0.5 },
                jab_pressed: close && i % 20 == 0 && !block,
                block_held: block,
                block_pressed: block,
                ..Default::default()
            },
        );
        if h(&sim).state == State::Gone || st(&sim).posture == hermit::Posture::Sulking {
            out = true;
            break;
        }
    }
    assert!(out, "{:?}", h(&sim));
    assert!(health(&sim) > 0.0);
}

#[test]
fn it_can_be_escaped_by_walking_away() {
    let mut sim = fight(150.0);
    for _ in 0..1600 {
        tick(
            &mut sim,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
    }
    let e = h(&sim);
    assert!(matches!(e.state, State::Idle), "{:?}", e.state);
    assert!((e.pos.x - e.home.x).abs() < 200.0);
}

#[test]
fn no_blow_or_undefended_cycle_kills_a_full_crab() {
    let mut sim = fight(112.0);
    stubborn(&mut sim);
    let mut worst: f32 = 0.0;
    let mut at_attack = health(&sim);
    for _ in 0..8000 {
        tick(&mut sim, Controls::default());
        let (state, ticks) = (h(&sim).state, h(&sim).state_tick);
        if state == State::Tell && ticks == 1 {
            sim.crab_mut(P).unwrap().fighter.vitals.health = 100.0;
            sim.crab_mut(P).unwrap().fighter.venom.clear();
            at_attack = 100.0;
        }
        if state == State::Recover {
            worst = worst.max(at_attack - health(&sim));
        }
    }
    assert!(worst > 0.0 && worst < 25.0, "worst cycle {worst}");
}

fn run_hash(seed_walk: f32) -> Vec<u64> {
    let mut sim = fight(112.0);
    let mut out = Vec::new();
    for i in 0..2400 {
        tick(
            &mut sim,
            Controls {
                walk: if (i / 300) % 2 == 0 {
                    seed_walk
                } else {
                    -seed_walk
                },
                jab_pressed: i % 50 == 0,
                ..Default::default()
            },
        );
        if i % 60 == 0 {
            out.push(sim.state_hash());
        }
    }
    out
}

#[test]
fn two_sims_agree_every_second() {
    assert_eq!(run_hash(0.3), run_hash(0.3));
}

#[test]
fn co_op_the_guest_judges_its_own_crab_and_both_devices_agree() {
    let setup = |sim: &mut Sim| {
        sim.set_feel(Feel::Current);
        sim.spawn_enemy(Kind::HermitCrab, Vec2::new(X + 112.0, FLOOR));
    };
    let world = || {
        let mut s = Sim::new(Tuning::default(), Level::test_level());
        setup(&mut s);
        s
    };
    let mut host = world();
    host.add_crab(P, Vec2::new(300.0, FLOOR));
    host.add_remote_crab(G, Vec2::new(X, FLOOR));
    host.go_online(true);
    let mut guest = world();
    guest.add_remote_crab(P, Vec2::new(300.0, FLOOR));
    guest.add_crab(G, Vec2::new(X, FLOOR));
    guest.go_online(false);
    let mut link = FakeLink::steady(2);
    let hp = guest.crab(G).unwrap().fighter.vitals.health;
    let (mut hurt, mut same, mut total) = (false, 0, 0);
    for i in 0..2400 {
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        hurt |= guest.crab(G).unwrap().fighter.vitals.health < hp;
        if i > 300 {
            total += 1;
            same += (host.enemies()[0].state == guest.enemies()[0].state) as u32;
        }
        // The host never changed the guest's crab or its own far one.
        assert_eq!(host.crab(P).unwrap().fighter.vitals.health, 100.0);
    }
    assert!(hurt, "the guest's own device was hit");
    assert!(same * 10 >= total * 7, "agreed {same}/{total}");
    let (a, b) = (
        host.crab(G).unwrap().fighter.vitals.health,
        guest.crab(G).unwrap().fighter.vitals.health,
    );
    assert!((a - b).abs() < 3.0, "host sees {a}, guest has {b}");
}
