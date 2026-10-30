//! Codes (`crabsim::codes`): typed in the game, they change the local crab
//! between two ticks, and a replay carries them so it plays back exactly.

use bevy_math::Vec2;
use crabsim::replay::ticks;
use crabsim::{Code, Feel, Hit, Kind, PlayerId, Replay, SPAWN, Sim, Source, play};

const P0: PlayerId = PlayerId(0);

fn sim() -> Sim {
    let mut sim = Sim::with_one_crab(SPAWN);
    sim.set_feel(Feel::Current);
    sim
}

/// The crab smashes once at the fixture standing in reach; how much health
/// the fixture lost, and how fast the blow threw it.
fn one_smash(codes: &[Code]) -> (f32, f32) {
    let mut sim = sim();
    for &code in codes {
        assert!(sim.enter_code(P0, code), "{code:?} reaches the crab");
    }
    sim.spawn_enemy(Kind::Fixture, Vec2::new(SPAWN.x + 100.0, 0.0));
    let replay = Replay::parse("t=0 smash").unwrap();
    let mut thrown: f32 = 0.0;
    play(&mut sim, &replay, ticks(0.35), |s| {
        thrown = thrown.max(s.enemies()[0].vel_x.abs());
    });
    (40.0 - sim.enemies()[0].vitals.health, thrown)
}

#[test]
fn hulk_smashes_three_times_as_hard_and_throws_further() {
    let (plain, plain_thrown) = one_smash(&[]);
    assert_eq!(plain, 12.0, "a plain smash on the fixture");
    let (hulk, hulk_thrown) = one_smash(&[Code::Hulk]);
    assert_eq!(hulk, 3.0 * plain, "hulk triples the smash");
    assert!(
        hulk_thrown > 2.0 * plain_thrown,
        "much bigger knockback: {hulk_thrown} vs {plain_thrown}"
    );
}

#[test]
fn hulk_still_smashes_down_to_a_creature_low_on_the_floor() {
    // A snapping shrimp is only 12 tall; out of its burrow (a crab walked
    // up) it can be hit. In the game's own feel.
    for codes in [&[][..], &[Code::Hulk]] {
        let mut sim = Sim::with_one_crab(SPAWN);
        for &code in codes {
            sim.enter_code(P0, code);
        }
        sim.spawn_enemy(Kind::Snapper, Vec2::new(340.0, 0.0));
        let replay =
            Replay::parse("t=0 walk=right; t=0.25 walk=stop; t=0.9 smash; t=1.6 smash").unwrap();
        let mut struck = false;
        play(&mut sim, &replay, ticks(2.2), |s| {
            struck |= s.crabs()[0].events.struck
        });
        assert!(struck, "{codes:?} smash reaches the shrimp");
    }
}

#[test]
fn hulk_again_turns_it_off() {
    let mut sim = sim();
    sim.enter_code(P0, Code::Hulk);
    assert!(sim.crab(P0).unwrap().mover.hulk);
    sim.enter_code(P0, Code::Hulk);
    assert!(!sim.crab(P0).unwrap().mover.hulk);
    assert_eq!(
        one_smash(&[Code::Hulk, Code::Hulk]).0,
        12.0,
        "back to plain"
    );
}

#[test]
fn hulk_poise_is_hard_to_break_and_it_walks_a_little_slower() {
    let blow = Hit::new(0.0, 60.0, Source::World);
    let mut plain = sim();
    plain.hit_crab(P0, &blow);
    let mut hulk = sim();
    hulk.enter_code(P0, Code::Hulk);
    hulk.hit_crab(P0, &blow);
    let poise = |s: &Sim| s.crab(P0).unwrap().fighter.vitals.poise;
    assert_eq!(poise(&plain), 40.0);
    assert!(poise(&hulk) > 80.0, "hulk poise {}", poise(&hulk));

    let walk = Replay::parse("t=0 walk=right").unwrap();
    let speed = |s: &mut Sim| {
        play(s, &walk, ticks(1.0), |_| {});
        s.crab(P0).unwrap().mover.vel.x
    };
    let (mut plain, mut hulk) = (sim(), sim());
    hulk.enter_code(P0, Code::Hulk);
    let (p, h) = (speed(&mut plain), speed(&mut hulk));
    assert!(h < p && h > 0.6 * p, "a little slower: {h} vs {p}");
}

#[test]
fn feast_fills_the_shell_and_the_reserve() {
    let mut sim = sim();
    sim.hit_crab(P0, &Hit::new(40.0, 0.0, Source::World));
    let crab = sim.crab_mut(P0).unwrap();
    crab.mover.reserve = 0.1;
    crab.mover.stamina = 0.0;
    assert!(sim.enter_code(P0, Code::Feast));
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.fighter.vitals.health, sim.tuning.crab.health);
    assert_eq!(crab.mover.reserve, 1.0);
    assert_eq!(
        crab.mover.stamina,
        crab.mover.strokes(&sim.tuning) as f32,
        "rested"
    );
}

#[test]
fn glow_lights_the_lantern_to_its_brightest_and_back() {
    let mut sim = sim();
    let lantern = |s: &Sim| s.crab(P0).unwrap().mover.lantern_glow();
    assert_eq!(lantern(&sim), 0);
    sim.enter_code(P0, Code::Glow);
    assert_eq!(lantern(&sim), crabsim::codes::GLOW_CULTURES);
    assert_eq!(sim.crab(P0).unwrap().mover.lantern, 0, "nothing earned");
    sim.enter_code(P0, Code::Glow);
    assert_eq!(lantern(&sim), 0);
}

#[test]
fn an_unknown_code_changes_nothing() {
    assert_eq!(Code::parse("banana"), None);
    assert_eq!(Code::parse(""), None);
    assert_eq!(Code::parse("  HuLk "), Some(Code::Hulk), "case and spaces");
    let before = sim();
    let mut after = sim();
    assert!(!after.enter_typed(P0, "banana"));
    assert_eq!(after.state_hash(), before.state_hash());
    assert!(
        Replay::parse("t=1 code=banana")
            .unwrap_err()
            .contains("banana")
    );
}

#[test]
fn the_joke_codes_parse_however_they_are_typed() {
    assert_eq!(Code::parse("pizza"), Some(Code::Pizza));
    assert_eq!(Code::parse("  MONKEYS "), Some(Code::Monkeys));
}

/// Walk, smash and dodge about with or without the pizza look; the crab's
/// whole state each 20 ticks, with the look set aside.
fn run_with_pizza(pizza: bool) -> (Vec<u64>, bool) {
    let mut sim = sim();
    sim.spawn_enemy(Kind::Fixture, Vec2::new(340.0, 0.0));
    if pizza {
        assert!(sim.enter_typed(P0, "pizza"));
    }
    let script = Replay::parse("t=0 walk=right; t=0.3 walk=stop; t=0.8 smash; t=1.6 jab").unwrap();
    let mut playback = script.playback();
    let mut seen = Vec::new();
    for n in 0..600 {
        sim.set_controls(P0, playback.next_controls());
        sim.tick();
        if n % 20 == 0 {
            let mut c = sim.crab(P0).unwrap().clone();
            c.mover.pizza = false;
            seen.push(crabsim::hash::fnv1a(
                format!("{c:?} {:?}", sim.enemies()).as_bytes(),
            ));
        }
    }
    (seen, sim.crab(P0).unwrap().mover.pizza)
}

#[test]
fn pizza_toggles_the_look_and_changes_nothing_in_play() {
    let (plain, plain_look) = run_with_pizza(false);
    let (pizza, pizza_look) = run_with_pizza(true);
    assert!(!plain_look && pizza_look);
    assert_eq!(plain, pizza, "the crab and the creature play out the same");
    let mut sim = sim();
    sim.enter_typed(P0, "pizza");
    assert!(sim.crab(P0).unwrap().mover.pizza);
    sim.enter_typed(P0, "pizza");
    assert!(!sim.crab(P0).unwrap().mover.pizza, "again: back");
}

#[test]
fn monkeys_drop_in_near_the_crab_and_cannot_be_hit_while_falling() {
    let mut sim = sim();
    assert!(sim.enemies().is_empty());
    assert!(sim.enter_typed(P0, "monkeys"));
    let found = sim.enemies();
    assert_eq!(found.len(), 3);
    assert!(found.iter().all(|e| e.kind == Kind::Monkey));
    assert!(found.iter().all(|e| (e.pos.x - SPAWN.x).abs() < 400.0));
    assert!(found.iter().all(|e| e.dropping()));
    assert!(found.iter().all(|e| !e.hittable(&sim.tuning)));
    for _ in 0..crabsim::codes::MONKEY_DROP {
        sim.tick();
    }
    assert!(sim.enemies().iter().all(|e| !e.dropping()));
    assert!(sim.enemies().iter().all(|e| e.hittable(&sim.tuning)));
}

#[test]
fn monkeys_swing_at_the_crab_weakly() {
    let mut sim = sim();
    sim.enter_typed(P0, "monkeys");
    let start = sim.crab(P0).unwrap().fighter.vitals.health;
    let (mut hits, mut biggest) = (0, 0.0_f32);
    let mut before = start;
    for _ in 0..ticks(12.0) {
        sim.tick();
        let now = sim.crab(P0).unwrap().fighter.vitals.health;
        if now < before {
            hits += 1;
            biggest = biggest.max(before - now);
        }
        before = now;
    }
    assert!(hits > 0, "they hit");
    assert!(
        biggest <= 0.05 * start,
        "each blow is weak: {biggest} of {start}"
    );
}

#[test]
fn a_monkey_can_be_beaten_and_leaves_no_body() {
    let mut sim = sim();
    sim.enter_typed(P0, "monkeys");
    for _ in 0..crabsim::codes::MONKEY_DROP {
        sim.tick();
    }
    let hit = Hit {
        damage: 100.0,
        source: Source::Crab(P0),
        ..Hit::default()
    };
    let id = sim.enemies()[0].id;
    let t = sim.tuning.clone();
    let e = sim.enemies_mut().iter_mut().find(|e| e.id == id).unwrap();
    let took = e.takes(&hit, e.pos.x - 10.0, &t).unwrap();
    assert!(took.died);
    assert_eq!(e.state, crabsim::enemy::State::Gone);
    for _ in 0..crabsim::codes::MONKEY_LINGER + 10 {
        sim.tick();
    }
    assert!(sim.enemies().iter().all(|e| e.id != id), "gone for good");
    assert_eq!(sim.enemies().len(), 2);
}

#[test]
fn monkeys_leave_after_a_while() {
    let mut sim = sim();
    sim.enter_typed(P0, "monkeys");
    for _ in 0..crabsim::codes::MONKEY_STAY + crabsim::codes::MONKEY_LINGER + 600 {
        sim.tick();
    }
    assert!(sim.enemies().is_empty());
}

#[test]
fn a_recording_with_the_monkeys_code_replays_to_the_same_hash() {
    let mut live = sim();
    live.start_recording();
    for frame in 0..900 {
        if frame == 100 {
            assert!(live.enter_typed(P0, "monkeys"));
        }
        if frame == 400 {
            assert!(live.enter_typed(P0, "pizza"));
        }
        live.set_controls(P0, Default::default());
        live.tick();
    }
    assert_eq!(live.enemies().len(), 3);
    let text = live.recording().unwrap().to_text();
    assert!(text.contains("code=monkeys"), "{text}");
    assert!(!text.contains("spawn="), "the code carries them: {text}");
    let replay = Replay::parse(&text).unwrap();
    let mut again = sim();
    play(&mut again, &replay, replay.length(), |_| {});
    assert_eq!(again.state_hash(), live.state_hash());
}

#[test]
fn every_code_has_its_own_hint() {
    assert_eq!(Code::Hulk.hint(), "bulk up (again: back)");
    assert_eq!(Code::Pizza.hint(), "a crab of many pizzas (just for fun)");
    assert_eq!(
        Code::Monkeys.hint(),
        "monkeys, from outer space (just for fun)"
    );
    for a in crabsim::codes::ALL {
        for b in crabsim::codes::ALL {
            assert_eq!(a == b, a.hint() == b.hint(), "{a:?} and {b:?}");
        }
    }
}

#[test]
fn a_friends_crab_is_never_changed_here() {
    let mut sim = sim();
    sim.add_remote_crab(PlayerId(1), SPAWN);
    assert!(!sim.enter_code(PlayerId(1), Code::Hulk));
    assert!(!sim.crab(PlayerId(1)).unwrap().mover.hulk);
}

#[test]
fn a_recording_with_codes_replays_to_the_same_hash() {
    let script = Replay::parse(
        "t=0 walk=right; t=0.3 walk=stop; t=0.8 smash; t=1.6 jab; t=2 walk=left; t=2.4 walk=right; t=2.8 smash",
    )
    .unwrap();
    let run = |codes: bool| {
        let mut live = sim();
        live.start_recording();
        live.spawn_enemy(Kind::Fixture, Vec2::new(340.0, 0.0));
        let mut playback = script.playback();
        for frame in 0..500 {
            if codes {
                match frame {
                    60 => assert!(live.enter_typed(P0, "hulk")),
                    200 => assert!(live.enter_typed(P0, "glow")),
                    260 => assert!(live.enter_typed(P0, "feast")),
                    300 => assert!(live.enter_typed(P0, "pizza")),
                    330 => assert!(live.enter_typed(P0, "hulk")),
                    _ => {}
                }
            }
            live.set_controls(P0, playback.next_controls());
            for _ in 0..[1, 0, 2, 1][frame % 4] {
                live.tick();
            }
        }
        live
    };
    let live = run(true);
    let text = live.recording().unwrap().to_text();
    assert_eq!(text.matches("code=hulk").count(), 2, "{text}");
    assert!(
        text.contains("code=glow") && text.contains("code=feast") && text.contains("code=pizza"),
        "{text}"
    );
    let replay = Replay::parse(&text).unwrap();
    assert_eq!(Replay::parse(&replay.to_text()).unwrap(), replay);
    let mut again = sim();
    play(&mut again, &replay, replay.length(), |_| {});
    assert_eq!(again.state_hash(), live.state_hash());
    assert_eq!(replay.hash, Some(live.state_hash()));
    assert_ne!(
        run(false).state_hash(),
        live.state_hash(),
        "the codes made a difference"
    );
}

#[test]
fn the_hint_lists_every_code_by_the_word_to_type() {
    let hints = crabsim::codes::hints();
    for code in crabsim::codes::ALL {
        let line = format!("{}: {}", code.name(), code.hint());
        assert!(
            hints.lines().any(|l| l == line),
            "{line:?} missing from {hints:?}"
        );
        assert_eq!(Code::parse(code.name()), Some(code));
    }
}

#[test]
fn hulk_has_ten_times_the_shell_and_loses_it_on_toggle_off() {
    let mut sim = sim();
    let usual = sim.crab(P0).unwrap().mover.max_shell(&sim.tuning);
    assert!(sim.enter_code(P0, Code::Hulk));
    let hulk = sim.crab(P0).unwrap().mover.max_shell(&sim.tuning);
    assert_eq!(hulk, 10.0 * usual);
    let health = sim.crab(P0).unwrap().fighter.vitals.health;
    assert_eq!(health, 10.0 * usual, "a whole shell stays whole");
    // Hurt in hulk mode, then back to normal: never more than whole.
    assert!(sim.enter_code(P0, Code::Hulk));
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.max_shell(&sim.tuning), usual);
    assert_eq!(crab.fighter.vitals.health, usual);
}

#[test]
fn a_hurt_hulk_crab_keeps_its_share_of_shell() {
    let mut sim = sim();
    let usual = sim.crab(P0).unwrap().mover.max_shell(&sim.tuning);
    sim.crab_mut(P0).unwrap().fighter.vitals.health = usual / 2.0;
    sim.enter_code(P0, Code::Hulk);
    assert_eq!(sim.crab(P0).unwrap().fighter.vitals.health, 5.0 * usual);
}

/// The crab in reach of the moray's lunge, with the code on or not; whether
/// it was ever held.
fn held_by_moray(hulk: bool) -> bool {
    let mut sim = Sim::with_one_crab(Vec2::new(290.0, 0.0));
    sim.set_feel(Feel::Current);
    if hulk {
        sim.enter_code(P0, Code::Hulk);
    }
    sim.spawn_enemy(Kind::Moray, Vec2::new(400.0, 0.0));
    let mut held = false;
    play(
        &mut sim,
        &Replay::parse("t=0 walk=stop").unwrap(),
        ticks(5.0),
        |s| {
            held |= s.crabs()[0].fighter.grabbed.is_some();
        },
    );
    held
}

#[test]
fn a_grab_does_not_hold_a_hulk_crab() {
    assert!(held_by_moray(false), "a plain crab is held");
    assert!(!held_by_moray(true), "a hulk crab shrugs it off");
}
