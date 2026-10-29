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
        text.contains("code=glow") && text.contains("code=feast"),
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
