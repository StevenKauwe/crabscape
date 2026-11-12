//! The American lobster: a guarding rival with a counter beat, two blows with
//! tells, and a tail flip. Headless, with ordinary controls.
use bevy_math::Vec2;
use crabsim::creatures::Extra;
use crabsim::creatures::lobster::{self as lob, Sub};
use crabsim::enemy::State;
use crabsim::net::FakeLink;
use crabsim::world::cues::Cue;
use crabsim::{Controls, Kind, Level, PlayerId, Replay, Sim, Tuning};

const P: PlayerId = PlayerId(0);
// On the shelf: the trench floor that was once its arena now holds the Silt
// Galleries' mouth (a hole in the floor between 1,540 and 1,700).
const X: f32 = 900.0;

fn arena() -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(X, 0.0));
    let floor = sim.level.floor_height(X);
    sim.crab_mut(P).unwrap().mover.pos = Vec2::new(X, floor);
    sim.crab_mut(P).unwrap().fighter.facing = 1.0;
    let lx = X + 300.0;
    sim.spawn_enemy(Kind::Lobster, Vec2::new(lx, sim.level.floor_height(lx)));
    sim
}

fn tick(sim: &mut Sim, c: Controls) {
    sim.set_controls(P, c);
    sim.tick();
}

fn lobster(sim: &Sim) -> &crabsim::Enemy {
    &sim.enemies()[0]
}

fn phase(sim: &Sim) -> &'static str {
    lobster(sim).phase(&sim.tuning)
}

fn sub(sim: &Sim) -> Sub {
    match &lobster(sim).creature {
        Extra::Lobster(s) => s.sub,
        _ => panic!("not a lobster"),
    }
}

fn guard(sim: &Sim) -> f32 {
    match &lobster(sim).creature {
        Extra::Lobster(s) => s.guard,
        _ => panic!("not a lobster"),
    }
}

fn idle() -> Controls {
    Controls::default()
}

fn crab_health(sim: &Sim) -> f32 {
    sim.crab(P).unwrap().fighter.vitals.health
}

/// Run until the lobster stands guarding in a fresh pause, in reach.
fn to_pause(sim: &mut Sim) {
    for _ in 0..2000 {
        tick(sim, idle());
        let e = lobster(sim);
        if phase(sim) == "guard"
            && e.state == State::Notice
            && e.state_tick <= 2
            && e.state_len >= 30
        {
            let d = (sim.crab(P).unwrap().mover.pos.x - (e.pos.x + e.facing * 95.0)) * e.facing;
            if d < 140.0 {
                return;
            }
        }
    }
    panic!("never settled into a guard pause: {:?}", lobster(sim));
}

/// Blow by blow: how long each tell lasted, from an undefended crab.
#[test]
fn the_tell_comes_before_every_blow_and_lasts_its_length() {
    let mut sim = arena();
    let start = crab_health(&sim);
    let (mut tell, mut seen_cue, mut checked) = (0u32, false, 0);
    let mut kind = Sub::Rest;
    for _ in 0..6000 {
        tick(&mut sim, idle());
        let e = lobster(&sim);
        seen_cue |= sim
            .cues()
            .iter()
            .any(|ev| matches!(ev.cue, Cue::Tell { crab: P, .. }));
        match e.state {
            State::Tell => {
                tell += 1;
                kind = sub(&sim);
                assert_eq!(crab_health(&sim), crab_health(&sim).min(start));
            }
            State::Attack if tell == 0 => {}
            State::Attack => {
                assert!(tell > 0, "a blow with no tell");
                let want = if kind == Sub::Crush {
                    lob::CRUSH_TELL
                } else {
                    lob::SNIP_TELL
                };
                assert_eq!(tell, want, "{kind:?} tell length");
                assert!(seen_cue, "no Cue::Tell");
                tell = 0;
                checked += 1;
                if checked == 2 {
                    break;
                }
            }
            _ => {
                if e.state != State::Recover {
                    assert_eq!(crab_health(&sim), start.min(crab_health(&sim)));
                }
            }
        }
    }
    assert!(checked >= 2, "only {checked} blows");
}

#[test]
fn no_single_blow_or_undefended_pair_kills_a_full_crab() {
    let mut sim = arena();
    let start = crab_health(&sim);
    let mut blows = 0;
    let mut was = State::Idle;
    let mut biggest: f32 = 0.0;
    let mut before = start;
    for _ in 0..8000 {
        tick(&mut sim, idle());
        let now = lobster(&sim).state;
        if was != State::Recover && now == State::Recover {
            blows += 1;
            biggest = biggest.max(before - crab_health(&sim));
            before = crab_health(&sim);
        }
        was = now;
        if blows == 2 {
            break;
        }
    }
    assert!(blows >= 2);
    assert!(biggest <= lob::CRUSH_DAMAGE + 0.01, "a blow did {biggest}");
    assert!(crab_health(&sim) > start - 2.0 * lob::CRUSH_DAMAGE - 0.01);
    assert!(crab_health(&sim) > 50.0);
}

#[test]
fn a_parry_takes_no_damage_and_staggers_it() {
    let mut sim = arena();
    let start = crab_health(&sim);
    for _ in 0..6000 {
        let e = lobster(&sim);
        let press = e.state == State::Tell && e.state_tick + 4 >= e.state_len;
        tick(
            &mut sim,
            Controls {
                block_held: press,
                block_pressed: press,
                ..idle()
            },
        );
        if sim.crab(P).unwrap().events.parried {
            assert_eq!(crab_health(&sim), start);
            assert_eq!(lobster(&sim).state, State::Stagger);
            assert_eq!(phase(&sim), "parried");
            assert!(
                sim.crab(P).unwrap().fighter.counter > 0,
                "no counter window"
            );
            return;
        }
    }
    panic!("no parry");
}

#[test]
fn blocking_the_crusher_cracks_the_guard_and_a_dodge_takes_nothing() {
    // Block through blows until a crush is blocked.
    let mut sim = arena();
    let mut cracked = false;
    for _ in 0..12000 {
        tick(
            &mut sim,
            Controls {
                block_held: true,
                ..idle()
            },
        );
        if sim.crab(P).unwrap().fighter.armour_break > 0 {
            cracked = true;
            break;
        }
    }
    assert!(cracked, "a blocked crush never cracked the guard");
    // Dodge through the first blow.
    let mut sim = arena();
    let start = crab_health(&sim);
    let mut dodged = false;
    for _ in 0..6000 {
        let e = lobster(&sim);
        let press = e.state == State::Tell && e.state_tick + 3 == e.state_len;
        tick(
            &mut sim,
            Controls {
                dodge_pressed: press,
                dodge_held: press,
                walk: if press { -1.0 } else { 0.0 },
                ..idle()
            },
        );
        if lobster(&sim).state == State::Recover {
            dodged = true;
            break;
        }
    }
    assert!(dodged, "the blow never finished");
    assert_eq!(crab_health(&sim), start);
}

#[test]
fn blows_on_the_guard_do_a_fifth_and_drain_the_guard() {
    let mut sim = arena();
    to_pause(&mut sim);
    let (h, p, g) = (
        lobster(&sim).vitals.health,
        lobster(&sim).vitals.poise,
        guard(&sim),
    );
    tick(
        &mut sim,
        Controls {
            jab_pressed: true,
            ..idle()
        },
    );
    for _ in 0..30 {
        tick(&mut sim, idle());
    }
    assert_eq!(phase(&sim), "guard", "{:?}", lobster(&sim));
    let lost = h - lobster(&sim).vitals.health;
    assert!(lost > 0.5 && lost < 1.6, "a jab on the guard lost {lost}");
    assert_eq!(lobster(&sim).vitals.poise, p, "its own poise untouched");
    assert!(guard(&sim) < g - 6.0, "guard {g} -> {}", guard(&sim));
}

#[test]
fn a_jab_is_not_parried_but_a_smash_is_in_the_counter_beat() {
    let mut sim = arena();
    to_pause(&mut sim);
    tick(
        &mut sim,
        Controls {
            jab_pressed: true,
            ..idle()
        },
    );
    for _ in 0..20 {
        assert_ne!(phase(&sim), "counter");
        tick(&mut sim, idle());
    }
    let mut sim = arena();
    to_pause(&mut sim);
    let h = lobster(&sim).vitals.health;
    let (ch, cp) = (crab_health(&sim), sim.crab(P).unwrap().fighter.vitals.poise);
    tick(
        &mut sim,
        Controls {
            smash_pressed: true,
            ..idle()
        },
    );
    assert_eq!(phase(&sim), "counter", "{:?}", lobster(&sim));
    let mut shoved = false;
    for _ in 0..40 {
        tick(&mut sim, idle());
        shoved |= sim.crab(P).unwrap().fighter.vitals.poise <= cp - lob::COUNTER_POISE + 1.0;
    }
    assert!(shoved, "the crab was not shoved");
    assert_eq!(crab_health(&sim), ch, "the shove costs no health");
    assert_eq!(lobster(&sim).vitals.health, h, "the parry costs it nothing");
}

#[test]
fn an_open_guard_takes_full_damage_and_loses_poise() {
    let mut sim = arena();
    to_pause(&mut sim);
    // Claws just reopened after the beat: unguarded.
    {
        let e = &mut sim.enemies_mut()[0];
        e.state = State::Notice;
        e.state_tick = 1;
        e.state_len = lob::COUNTER_REOPEN;
        e.creature = Extra::Lobster(state_of(Sub::Reopen));
        e.reach = lob::REOPEN_REACH;
    }
    let h = lobster(&sim).vitals.health;
    tick(
        &mut sim,
        Controls {
            smash_pressed: true,
            ..idle()
        },
    );
    for _ in 0..25 {
        tick(&mut sim, idle());
    }
    assert!(
        h - lobster(&sim).vitals.health >= 10.0,
        "lost {}",
        h - lobster(&sim).vitals.health
    );
}

#[test]
fn jabs_break_the_guard_and_poise_breaks_the_lobster() {
    let mut sim = arena();
    to_pause(&mut sim);
    if let Extra::Lobster(s) = &mut sim.enemies_mut()[0].creature {
        s.guard = 5.0;
    }
    tick(
        &mut sim,
        Controls {
            jab_pressed: true,
            ..idle()
        },
    );
    for _ in 0..20 {
        tick(&mut sim, idle());
    }
    assert_eq!(phase(&sim), "broken", "{:?}", lobster(&sim));
    // The guard is whole again after the stagger.
    for _ in 0..(lob::GUARD_BREAK_TICKS + 5) {
        tick(&mut sim, idle());
    }
    assert!(guard(&sim) >= lob::GUARD_POISE - 0.1);

    // Poise: in a recovery (no guard) a smash breaks a worn poise.
    let mut sim = arena();
    to_pause(&mut sim);
    {
        let e = &mut sim.enemies_mut()[0];
        e.state = State::Recover;
        e.state_tick = 1;
        e.state_len = 200;
        e.vitals.poise = 10.0;
        e.reach = 0.0;
        e.creature = Extra::Lobster(state_of(Sub::Snip));
    }
    tick(
        &mut sim,
        Controls {
            smash_pressed: true,
            ..idle()
        },
    );
    for _ in 0..25 {
        tick(&mut sim, idle());
    }
    assert_eq!(phase(&sim), "stagger", "{:?}", lobster(&sim));
}

/// A scripted crab that plays the duel: parry a tell, counter, jab the guard,
/// smash the open claws.
fn bot(sim: &Sim, n: u32) -> Controls {
    let c = sim.crab(P).unwrap();
    let e = lobster(sim);
    let head = e.pos.x + e.facing * 95.0;
    let d = (c.mover.pos.x - head) * e.facing;
    let toward = (e.pos.x - c.mover.pos.x).signum();
    let open = matches!(
        phase(sim),
        "recover" | "stagger" | "parried" | "broken" | "reopen" | "spent"
    );
    let mut k = Controls::default();
    if e.state == State::Tell && e.state_tick + 4 >= e.state_len {
        k.block_held = true;
        k.block_pressed = true;
    } else if c.fighter.counter > 0 && d < 100.0 {
        k.smash_pressed = true;
    } else if open {
        if d > 70.0 {
            k.walk = toward;
        } else {
            k.smash_pressed = true;
        }
    } else if d > 118.0 {
        k.walk = toward * 0.6;
    } else if d < 100.0 {
        k.walk = -toward * 0.6;
    } else if n.is_multiple_of(30) {
        k.jab_pressed = true;
    } else {
        k.walk = toward * 0.02;
    }
    k
}

#[test]
fn a_scripted_crab_beats_or_drives_it_off() {
    let mut sim = arena();
    let mut drove = false;
    let mut sent_home = false;
    for n in 0..(120 * 90) {
        let k = bot(&sim, n);
        tick(&mut sim, k);
        drove |= lobster(&sim).fled;
        sent_home |= phase(&sim) == "den";
        assert!(crab_health(&sim) > 0.0, "the crab died at {n}");
        if sent_home {
            break;
        }
    }
    assert!(drove, "it never took the flip: {:?}", lobster(&sim));
    assert!(sent_home);
}

#[test]
fn the_tail_flip_at_35_percent_goes_home_backward_and_it_holds_the_den() {
    let mut sim = arena();
    let home = lobster(&sim).home;
    // Draw it out, then hurt it to 35%.
    to_pause(&mut sim);
    assert!(
        (lobster(&sim).pos.x - home.x).abs() > 40.0,
        "it did not walk out"
    );
    let face = lobster(&sim).facing;
    {
        let e = &mut sim.enemies_mut()[0];
        e.state = State::Recover;
        e.state_tick = 1;
        e.state_len = 60;
    }
    tick(&mut sim, idle());
    sim.enemies_mut()[0].vitals.health = 40.0;
    let (mut flipped, mut max_gap) = (false, 0.0f32);
    for _ in 0..600 {
        tick(&mut sim, idle());
        if phase(&sim) == "flip" {
            flipped = true;
            assert_eq!(lobster(&sim).facing, face, "it flips tail first");
            max_gap = max_gap.max((lobster(&sim).pos.x - home.x).abs());
        }
        if phase(&sim) == "den" {
            break;
        }
    }
    assert!(flipped, "{:?}", lobster(&sim));
    assert_eq!(phase(&sim), "den");
    assert_eq!(lobster(&sim).pos.x, home.x);
    // Holding the den it snips what comes close, never crushes, never leaves.
    let mut crush = false;
    for _ in 0..4000 {
        let k = if (sim.crab(P).unwrap().mover.pos.x - home.x).abs() > 160.0 {
            Controls {
                walk: (home.x - sim.crab(P).unwrap().mover.pos.x).signum(),
                ..idle()
            }
        } else {
            Controls::default()
        };
        tick(&mut sim, k);
        crush |= phase(&sim) == "crush-tell";
        assert_eq!(lobster(&sim).pos.x, home.x, "it left its den");
        if crab_health(&sim) < 30.0 {
            break;
        }
    }
    assert!(!crush);
}

#[test]
fn it_never_follows_past_its_leash_and_can_be_escaped() {
    let mut sim = arena();
    let home = lobster(&sim).home;
    for n in 0..(120 * 40) {
        let walk = if n < 600 { 0.0 } else { -1.0 };
        tick(&mut sim, Controls { walk, ..idle() });
        assert!(
            (lobster(&sim).pos.x - home.x).abs() <= lob::LEASH + 25.0,
            "past the leash at {n}"
        );
    }
    assert!(crab_health(&sim) > 0.0);
    // Far away, it has gone back to resting at home.
    assert_eq!(phase(&sim), "rest");
    assert!((lobster(&sim).pos.x - home.x).abs() < 2.0);
}

#[test]
fn it_is_deterministic() {
    let run = || {
        let mut sim = arena();
        let mut hashes = Vec::new();
        for n in 0..(120 * 20) {
            let k = bot(&sim, n);
            tick(&mut sim, k);
            if n.is_multiple_of(60) {
                hashes.push(sim.state_hash());
            }
        }
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn a_remote_crab_is_never_changed_by_the_lobster() {
    let mut sim = arena();
    sim.crab_mut(P).unwrap().remote = true;
    let before = sim.crab(P).unwrap().fighter.clone();
    for _ in 0..1500 {
        sim.tick();
    }
    assert_eq!(sim.crab(P).unwrap().fighter, before);
}

#[test]
fn it_lives_at_a_landmark_in_the_test_level() {
    let level = Level::test_level();
    let spots: Vec<_> = level
        .foes
        .iter()
        .filter(|(k, _)| *k == Kind::Lobster)
        .collect();
    assert_eq!(spots.len(), 1, "one lobster den");
    let at = spots[0].1;
    let marks = level.landmarks();
    let water = marks
        .iter()
        .find(|l| l.name == "canyon deep water")
        .unwrap();
    assert!(water.rect.contains(at + Vec2::Y * 2.0), "{at}");
    assert_eq!(at.y, level.floor_height(at.x));
    for l in &marks {
        if l.kind == crabsim::level::landmarks::Kind::Refuge {
            let d = at.distance(at.clamp(l.rect.min, l.rect.max));
            assert!(d >= 400.0, "{} is {d} away", l.name);
        }
        if matches!(
            l.kind,
            crabsim::level::landmarks::Kind::Tunnel | crabsim::level::landmarks::Kind::Mouth
        ) {
            assert!(!l.rect.contains(at));
        }
    }
    assert!(Tuning::default().crusher.damage > 0.0);
}

// --- Co-op -----------------------------------------------------------------

const HOST: PlayerId = PlayerId(0);
const GUEST: PlayerId = PlayerId(1);
const FAR: Vec2 = Vec2::new(2300.0, 35.0);

fn world(spawn: bool) -> Sim {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    if spawn {
        let lx = X + 300.0;
        sim.spawn_enemy(Kind::Lobster, Vec2::new(lx, sim.level.floor_height(lx)));
    }
    sim
}

#[test]
fn a_blow_is_judged_by_the_guest_defender_once_and_both_sims_agree_on_a_parry() {
    let at = Vec2::new(X, Level::test_level().floor_height(X));
    let mut host = world(true);
    host.add_crab(HOST, FAR);
    host.add_remote_crab(GUEST, at);
    host.go_online(true);
    let mut guest = world(true);
    guest.add_remote_crab(HOST, FAR);
    guest.add_crab(GUEST, at);
    guest.go_online(false);
    let mut link = FakeLink::new(3, 0, 0.0, 7);
    let initial = guest.crab(GUEST).unwrap().fighter.vitals.health;
    let (mut parried, mut both) = (false, false);
    let play = Replay::parse("").unwrap();
    let mut pb = play.playback();
    for _ in 0..1500 {
        let e = guest.enemies()[0].clone();
        let press = e.state == State::Tell && e.state_tick + 4 >= e.state_len;
        let mut c = pb.next_controls();
        c.block_held = press;
        c.block_pressed = press;
        guest.set_controls(GUEST, c);
        host.set_controls(HOST, Controls::default());
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        parried |= guest.crab(GUEST).unwrap().events.parried;
        both |= parried
            && host.enemies()[0].state == State::Stagger
            && guest.enemies()[0].state == State::Stagger;
        assert_eq!(host.crab(HOST).unwrap().fighter.vitals.health, 100.0);
        assert!(guest.crab(GUEST).unwrap().fighter.vitals.health >= initial - 0.01);
    }
    assert!(parried && both, "parried={parried} both={both}");

    // An undefended blow hurts once, and the host sees the same crab.
    let mut host = world(true);
    host.add_crab(HOST, FAR);
    host.add_remote_crab(GUEST, at);
    host.go_online(true);
    let mut guest = world(true);
    guest.add_remote_crab(HOST, FAR);
    guest.add_crab(GUEST, at);
    guest.go_online(false);
    let mut link = FakeLink::new(3, 0, 0.0, 7);
    let (mut hurt, mut reflected) = (false, false);
    let mut last = initial;
    for _ in 0..1200 {
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        let local = guest.crab(GUEST).unwrap().fighter.vitals.health;
        hurt |= local < initial;
        reflected |= hurt && host.crab(GUEST).unwrap().fighter.vitals.health == local;
        assert!(last - local <= lob::CRUSH_DAMAGE + 0.01, "duplicate damage");
        last = local;
    }
    assert!(hurt && reflected);
}

fn state_of(sub: Sub) -> lob::State {
    let mut s = lob::State::default();
    s.sub = sub;
    s
}
