//! The Humboldt squid: flashing tell, locked tentacle strike, snare and grip,
//! cut tentacle, ink and flight (`docs/design/creatures/humboldt-squid.md`).
use bevy_math::Vec2;
use crabsim::creatures::squid::{self, State as Own};
use crabsim::enemy::{Enemy, State};
use crabsim::net::FakeLink;
use crabsim::world::cues::Cue;
use crabsim::{Controls, Kind, Level, PlayerId, Sim, Tuning};

const P: PlayerId = PlayerId(0);
/// The plateau at the start of the test level is flat (y 110) from x 820.
const CRAB: Vec2 = Vec2::new(850.0, 110.0);
/// The squid's middle: its arm crown is 420 units from the crab.
const SQUID: Vec2 = Vec2::new(1582.0, 110.0);

fn arena() -> Sim {
    let mut sim = Sim::with_one_crab(CRAB);
    sim.spawn_enemy(Kind::HumboldtSquid, SQUID);
    sim
}

fn e(sim: &Sim) -> &Enemy {
    &sim.enemies()[0]
}

fn own(sim: &Sim) -> &Own {
    squid::state(e(sim)).expect("a squid")
}

fn health(sim: &Sim) -> f32 {
    sim.crab(P).expect("crab").fighter.vitals.health
}

fn step(sim: &mut Sim, c: Controls) {
    sim.set_controls(P, c);
    sim.tick();
}

/// Tick until `done`; true if it was reached within `max` ticks.
fn until(
    sim: &mut Sim,
    max: u32,
    mut script: impl FnMut(&Sim) -> Controls,
    done: impl Fn(&Sim) -> bool,
) -> bool {
    for _ in 0..max {
        if done(sim) {
            return true;
        }
        let c = script(sim);
        step(sim, c);
    }
    done(sim)
}

fn idle(_: &Sim) -> Controls {
    Controls::default()
}

fn in_tell(sim: &Sim) -> bool {
    e(sim).state == State::Tell
}

#[test]
fn it_is_placed_over_the_canyon_shelf_with_a_refuge_in_reach() {
    let level = Level::test_level();
    let at = level
        .foes
        .iter()
        .find(|(k, _)| *k == Kind::HumboldtSquid)
        .map(|(_, p)| *p)
        .expect("the squid is placed");
    let places = level.landmarks();
    let named = |n: &str| places.iter().find(|l| l.name == n).expect(n).rect;
    assert!(named("canyon shelf crossing").contains(at));
    assert!(!named("canyon deep water").contains(at));
    let refuge = places
        .iter()
        .filter(|l| {
            matches!(
                l.kind,
                crabsim::level::landmarks::Kind::Refuge | crabsim::level::landmarks::Kind::Mouth
            )
        })
        .map(|l| at.distance(at.clamp(l.rect.min, l.rect.max)))
        .fold(f32::INFINITY, f32::min);
    assert!((400.0..=600.0).contains(&refuge), "refuge {refuge}");
}

#[test]
fn the_tell_comes_first_lasts_3_2_seconds_and_swells_the_water() {
    let mut sim = arena();
    let (mut tell, mut told, mut blow_at_tell) = (0, false, None);
    for _ in 0..1500 {
        step(&mut sim, Controls::default());
        let en = e(&sim);
        if en.state == State::Tell {
            tell += 1;
        }
        told |= sim
            .cues()
            .iter()
            .any(|c| matches!(c.cue, Cue::Tell { crab, .. } if crab == P));
        if en.state == State::Attack {
            blow_at_tell = Some(tell);
            break;
        }
    }
    assert_eq!(blow_at_tell, Some(squid::TELL_TICKS));
    assert!(told, "no Cue::Tell");
    assert_eq!(
        health(&sim),
        sim.tuning.crab.health,
        "no damage before a blow"
    );
}

#[test]
fn flashing_speeds_up_and_the_aim_locks_for_the_last_beat() {
    let mut sim = arena();
    assert!(until(&mut sim, 1500, idle, in_tell));
    let mut periods = Vec::new();
    let mut locked: Option<Vec2> = None;
    let mut tracked = false;
    let mut last = own(&sim).aim;
    // The crab walks toward it all through the tell.
    for _ in 0..(squid::TELL_TICKS - 1) {
        step(
            &mut sim,
            Controls {
                walk: 0.3,
                ..Default::default()
            },
        );
        if e(&sim).state != State::Tell {
            break;
        }
        let st = own(&sim);
        periods.push(st.period);
        if e(&sim).state_tick <= squid::TELL_TICKS - squid::LOCK_TICKS {
            tracked |= st.aim != last;
            last = st.aim;
        } else {
            let l = *locked.get_or_insert(st.aim);
            assert_eq!(st.aim, l, "aim moved in the locked beat");
        }
    }
    assert!(tracked, "aim should track the crab through the first beat");
    assert!(locked.is_some());
    assert!(periods[5] > 40 && periods[300] < 14, "{periods:?}");
    assert!(periods.windows(2).all(|w| w[1] <= w[0]));
}

#[test]
fn an_unanswered_strike_snares_reels_in_and_grips_for_a_bounded_cost() {
    let mut sim = arena();
    let before = health(&sim);
    assert!(until(&mut sim, 2000, idle, |s| e(s).state == State::Grab));
    assert!(health(&sim) < before, "the strike hurts");
    assert!(!own(&sim).gripped);
    // Reeled in toward the arms.
    let x0 = sim.crab(P).expect("crab").mover.pos.x;
    assert!(until(&mut sim, 600, idle, |s| own(s).gripped));
    assert!(sim.crab(P).expect("crab").mover.pos.x > x0 + 100.0);
    assert!(sim.crab(P).expect("crab").fighter.grabbed.is_some());
    // Held, then let go; the whole round costs at most 25.
    assert!(until(&mut sim, 600, idle, |s| e(s).state == State::Recover));
    let cost = before - health(&sim);
    assert!(cost > 4.0 && cost <= 25.0, "cost {cost}");
    assert!(sim.crab(P).expect("crab").fighter.grabbed.is_none());
}

#[test]
fn cutting_the_held_tentacle_with_the_cutter_frees_the_crab_and_staggers_it() {
    let mut sim = arena();
    assert!(until(&mut sim, 2000, idle, |s| e(s).state == State::Grab));
    let full = e(&sim).vitals.health;
    let mut n = 0;
    let freed = until(
        &mut sim,
        400,
        |s| Controls {
            jab_pressed: s.tick_count() % 24 == 0 && !own(s).gripped,
            ..Default::default()
        },
        |s| e(s).state == State::Stagger,
    );
    assert!(freed, "never cut: {:?}", own(&sim));
    n += own(&sim).severed;
    assert_eq!(n, 1);
    assert!(!own(&sim).gripped);
    assert!(sim.crab(P).expect("crab").fighter.grabbed.is_none());
    assert_eq!(
        e(&sim).vitals.health,
        full,
        "the tentacle took it, not the body"
    );
    assert!(until(&mut sim, 400, idle, |s| e(s).state != State::Stagger));
}

#[test]
fn the_cutter_cuts_deeper_than_the_crusher() {
    let cut_with = |jab: bool| {
        let mut sim = arena();
        assert!(until(&mut sim, 2000, idle, |s| e(s).state == State::Grab));
        let mut fired = false;
        until(
            &mut sim,
            200,
            |_| {
                let go = !fired;
                fired = true;
                Controls {
                    jab_pressed: jab && go,
                    smash_pressed: !jab && go,
                    ..Default::default()
                }
            },
            |s| own(s).cut > 0.0 || e(s).state != State::Grab,
        );
        for _ in 0..40 {
            step(&mut sim, Controls::default());
        }
        own(&sim).cut
    };
    let (jab, smash) = (cut_with(true), cut_with(false));
    assert!(jab > 0.0 && smash > 0.0, "{jab} {smash}");
    assert!(jab > smash, "jab {jab} smash {smash}");
}

#[test]
fn a_parry_at_the_strike_takes_no_damage_and_staggers_it() {
    let mut sim = arena();
    let before = health(&sim);
    let parried = until(
        &mut sim,
        2000,
        |s| {
            let en = e(s);
            let now = en.state == State::Tell && en.state_tick + 4 >= en.state_len;
            Controls {
                block_held: now,
                block_pressed: now,
                ..Default::default()
            }
        },
        |s| s.crab(P).expect("crab").events.parried,
    );
    assert!(parried, "{:?}", e(&sim).state);
    assert_eq!(health(&sim), before);
    assert_eq!(e(&sim).state, State::Stagger);
    assert!(sim.crab(P).expect("crab").fighter.grabbed.is_none());
}

#[test]
fn a_block_keeps_you_out_of_the_grab_and_costs_only_poise() {
    let mut sim = arena();
    let (before, poise) = (
        health(&sim),
        sim.crab(P).expect("crab").fighter.vitals.poise,
    );
    let blocked = until(
        &mut sim,
        2000,
        |s| Controls {
            block_held: in_tell(s) || e(s).state == State::Attack,
            ..Default::default()
        },
        |s| s.crab(P).expect("crab").events.blocked,
    );
    assert!(blocked);
    assert_eq!(health(&sim), before);
    assert!(sim.crab(P).expect("crab").fighter.vitals.poise < poise);
    assert_eq!(e(&sim).state, State::Recover);
    assert!(sim.crab(P).expect("crab").fighter.grabbed.is_none());
}

#[test]
fn a_dodge_through_the_strike_takes_nothing() {
    let mut sim = arena();
    let before = health(&sim);
    let mut dodged = false;
    assert!(until(
        &mut sim,
        2000,
        |s| {
            let en = e(s);
            let go = !dodged && en.state == State::Tell && en.state_tick + 3 >= en.state_len;
            dodged |= go;
            Controls {
                dodge_pressed: go,
                walk: if go { -1.0 } else { 0.0 },
                ..Default::default()
            }
        },
        |s| e(s).state == State::Recover,
    ));
    assert_eq!(health(&sim), before);
    assert_eq!(e(&sim).state, State::Recover);
}

#[test]
fn walking_out_of_reach_during_the_tell_makes_it_miss() {
    let mut sim = arena();
    let before = health(&sim);
    let mut lowest = before;
    for _ in 0..2500 {
        step(
            &mut sim,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
        lowest = lowest.min(health(&sim));
    }
    assert_eq!(lowest, before, "it hit a crab that walked away");
}

#[test]
fn going_past_its_leash_loses_it() {
    let mut sim = arena();
    until(
        &mut sim,
        3000,
        |_| Controls {
            walk: -1.0,
            ..Default::default()
        },
        |s| s.tick_count() > 2500,
    );
    // Far from home: it has let go of the hunt.
    let en = e(&sim);
    assert!(sim.crab(P).expect("crab").mover.pos.distance(en.home) > squid::CRAB_LEASH);
    assert!(en.target.is_none() || en.state == State::Idle);
}

#[test]
fn a_crab_in_a_refuge_is_never_told_at() {
    // The home shell's refuge is x 40-150 at the start of the level.
    let mut sim = Sim::with_one_crab(Vec2::new(100.0, 0.0));
    sim.spawn_enemy(Kind::HumboldtSquid, Vec2::new(832.0, 110.0));
    for _ in 0..3000 {
        step(&mut sim, Controls::default());
        assert!(!matches!(e(&sim).state, State::Tell | State::Attack));
    }
    assert_eq!(health(&sim), sim.tuning.crab.health);
}

#[test]
fn it_breaks_off_a_tell_when_the_crab_reaches_shelter() {
    let mut sim = arena();
    assert!(until(&mut sim, 3000, idle, in_tell));
    for _ in 0..30 {
        step(&mut sim, Controls::default());
    }
    // The crab is suddenly in the home shell's refuge (and its water).
    sim.crab_mut(P).expect("crab").mover.pos = Vec2::new(100.0, 0.0);
    sim.enemies_mut()[0].home = Vec2::new(300.0, 110.0);
    let mut broke = false;
    for _ in 0..(squid::TELL_TICKS + 100) {
        step(&mut sim, Controls::default());
        assert_ne!(e(&sim).state, State::Attack, "it struck a sheltered crab");
        broke |= e(&sim).state == State::Recover;
    }
    assert!(broke, "it never broke off");
    assert_eq!(health(&sim), sim.tuning.crab.health);
}

#[test]
fn no_round_costs_a_full_crab_more_than_25() {
    let mut sim = arena();
    let full = sim.tuning.crab.health;
    let (mut round, mut worst, mut inside) = (0.0_f32, 0.0_f32, false);
    for _ in 0..9000 {
        let before = health(&sim);
        step(&mut sim, Controls::default());
        let en = e(&sim);
        if en.state == State::Tell {
            inside = true;
        }
        if inside {
            round += (before - health(&sim)).max(0.0);
        }
        if inside && matches!(en.state, State::Approach | State::Idle | State::Notice) {
            worst = worst.max(round);
            (round, inside) = (0.0, false);
        }
        // Keep the crab alive for the next round.
        sim.crab_mut(P).expect("crab").fighter.vitals.health = full;
    }
    assert!(worst > 4.0 && worst <= 25.0, "worst round {worst}");
}

#[test]
fn smashes_break_its_poise_then_hurt_it_below_40_percent_it_inks_and_leaves() {
    let mut sim = Sim::with_one_crab(Vec2::new(900.0, 110.0));
    sim.spawn_enemy(Kind::HumboldtSquid, Vec2::new(1120.0, 110.0));
    let mut staggered = false;
    let mut fled = false;
    for i in 0..4000 {
        step(
            &mut sim,
            Controls {
                walk: 0.3,
                smash_pressed: i % 40 == 0,
                ..Default::default()
            },
        );
        sim.crab_mut(P).expect("crab").fighter.vitals.health = sim.tuning.crab.health;
        staggered |= e(&sim).state == State::Stagger;
        if e(&sim).state == State::Flee {
            fled = true;
            break;
        }
    }
    assert!(staggered && fled);
    assert!(own(&sim).ink_left > 0, "no ink");
    sim.crab_mut(P).expect("crab").mover.pos = Vec2::new(100.0, 0.0);
    // It jets off, then is out of the world (no body to eat) for a long while.
    for _ in 0..(squid::FLEE_JET_TICKS + 10) {
        step(&mut sim, Controls::default());
    }
    assert!(own(&sim).away);
    assert!(e(&sim).pos.y > 10_000.0);
    assert_eq!(squid::phase(e(&sim)), Some("away"));
    // And comes back whole.
    for _ in 0..squid::AWAY_TICKS {
        step(&mut sim, Controls::default());
        if !own(&sim).away {
            break;
        }
    }
    assert!(!own(&sim).away);
    assert_eq!(e(&sim).vitals.health, 140.0);
}

#[test]
fn it_never_enters_a_tunnel() {
    let level = Level::test_level();
    let tunnels: Vec<_> = level
        .landmarks()
        .into_iter()
        .filter(|l| l.kind == crabsim::level::landmarks::Kind::Tunnel)
        .collect();
    let spot = Vec2::new(5900.0, -1050.0);
    let mut sim = Sim::with_one_crab(Vec2::new(5440.0, -1000.0));
    sim.spawn_enemy(Kind::HumboldtSquid, spot);
    for _ in 0..3000 {
        step(&mut sim, Controls::default());
        let p = e(&sim).pos;
        assert!(tunnels.iter().all(|t| !t.rect.contains(p)), "{p}");
    }
}

#[test]
fn two_runs_hash_the_same_every_second() {
    let script = |i: u32| Controls {
        walk: if (i / 300).is_multiple_of(2) {
            0.4
        } else {
            -0.4
        },
        jab_pressed: i.is_multiple_of(150),
        block_held: (i / 100).is_multiple_of(3),
        ..Default::default()
    };
    let (mut a, mut b) = (arena(), arena());
    for i in 0..(20 * 120) {
        step(&mut a, script(i));
        step(&mut b, script(i));
        if i % 60 == 0 {
            assert_eq!(a.state_hash(), b.state_hash(), "tick {i}");
        }
    }
}

const HOST: PlayerId = PlayerId(0);
const GUEST: PlayerId = PlayerId(1);

#[test]
fn co_op_the_guest_judges_the_blow_and_both_devices_hold_the_squid() {
    let setup = |sim: &mut Sim| {
        sim.spawn_enemy(Kind::HumboldtSquid, SQUID);
    };
    let world = |setup: &dyn Fn(&mut Sim)| {
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        setup(&mut sim);
        sim
    };
    let far = Vec2::new(150.0, 0.0);
    let mut host = world(&setup);
    host.add_crab(HOST, far);
    host.add_remote_crab(GUEST, CRAB);
    host.go_online(true);
    let mut guest = world(&setup);
    guest.add_remote_crab(HOST, far);
    guest.add_crab(GUEST, CRAB);
    guest.go_online(false);
    let mut link = FakeLink::new(3, 0, 0.0, 7);
    let initial = guest.crab(GUEST).expect("crab").fighter.vitals.health;
    let (mut hurt, mut both_grab) = (false, false);
    for _ in 0..1800 {
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        let local = guest.crab(GUEST).expect("crab");
        hurt |= local.fighter.vitals.health < initial;
        // The host never changes the guest's crab itself, and its own crab
        // is untouched.
        assert_eq!(
            host.crab(HOST).expect("crab").fighter.vitals.health,
            host.tuning.crab.health
        );
        assert!(local.fighter.vitals.health >= initial - 25.0);
        both_grab |=
            host.enemies()[0].state == State::Grab && guest.enemies()[0].state == State::Grab;
    }
    assert!(hurt, "the guest's crab was never hit");
    assert!(both_grab, "both devices should have seen it hold on");
}

#[test]
fn feels_is_the_one_place_it_reacts_to_an_outcome() {
    // Feeding the outcomes straight in (as a guest's message would) gives
    // the same reactions as a local blow.
    use crabsim::Outcome;
    let t = Tuning::default();
    for (outcome, want) in [
        (Outcome::Parried, State::Stagger),
        (
            Outcome::Hit(crabsim::combat::Took {
                broke_poise: false,
                died: false,
            }),
            State::Grab,
        ),
        (
            Outcome::Blocked(crabsim::combat::Took {
                broke_poise: false,
                died: false,
            }),
            State::Recover,
        ),
    ] {
        let mut en = Enemy::new(1, Kind::HumboldtSquid, SQUID, &t);
        en.state = State::Attack;
        assert!(crabsim::enemy::feels(&mut en, P, outcome, &t));
        assert_eq!(en.state, want, "{outcome:?}");
    }
    let mut en = Enemy::new(1, Kind::HumboldtSquid, SQUID, &t);
    assert!(!crabsim::enemy::feels(&mut en, P, Outcome::Evaded, &t));
}
