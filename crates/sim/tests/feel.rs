//! The feel presets (`crabsim::feel`): each one's timings and extras do what
//! its row in the table says, and switching feels replays exactly.

use bevy_math::Vec2;
use crabsim::feel::{ALL, Feel};
use crabsim::replay::ticks;
use crabsim::{Controls, Kind, PlayerId, Replay, SPAWN, Sim, play};

const P0: PlayerId = PlayerId(0);

/// The crab's fight status after each tick of `script`, in `feel`.
fn statuses(feel: Feel, script: &str, seconds: f64) -> Vec<String> {
    let replay = Replay::parse(&format!("@0 feel={}\n{script}", feel.name())).unwrap();
    let mut sim = Sim::with_one_crab(replay.start);
    let mut out = Vec::new();
    play(&mut sim, &replay, ticks(seconds), |sim| {
        out.push(sim.snapshots().next().unwrap().combat);
    });
    out
}

/// How many ticks in a row each status lasts.
fn runs(statuses: &[String]) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    for s in statuses {
        match out.last_mut() {
            Some((last, n)) if last == s => *n += 1,
            _ => out.push((s.clone(), 1)),
        }
    }
    out
}

fn phases(claw: &str, (w, a, r): (u32, u32, u32)) -> Vec<(String, u32)> {
    vec![
        (format!("{claw} windup"), w),
        (format!("{claw} active"), a),
        (format!("{claw} recovery"), r),
    ]
}

#[test]
fn every_preset_swings_with_its_own_timings() {
    for feel in ALL {
        let spec = feel.spec();
        let smash = runs(&statuses(feel, "@10 smash", 1.0));
        assert_eq!(
            smash[1..4],
            phases("smash", spec.crusher.ticks),
            "{feel:?} {smash:?}"
        );
        let jab = runs(&statuses(feel, "@10 jab", 1.0));
        assert_eq!(
            jab[1..4],
            phases("jab", spec.cutter.ticks),
            "{feel:?} {jab:?}"
        );
    }
}

#[test]
fn the_presets_are_really_different() {
    let t = |f: Feel| f.tuning();
    // Heavy winds up longest; Snappy and Silksong are quick.
    assert!(t(Feel::Heavy).crusher.windup_ticks > t(Feel::Current).crusher.windup_ticks);
    assert!(t(Feel::Snappy).crusher.windup_ticks < 10);
    assert_eq!(t(Feel::Silksong).cutter.windup_ticks, 1, "an instant jab");
    // Heavy freezes longest on a plain smash.
    let longest = ALL
        .into_iter()
        .max_by_key(|f| t(*f).crusher.hitstop_ticks)
        .unwrap();
    assert_eq!(longest, Feel::Heavy);
    // Only Momentum charges and cares about speed.
    for f in ALL {
        assert_eq!(t(f).charge_ticks > 0, f == Feel::Momentum, "{f:?}");
        assert_eq!(t(f).momentum > 0.0, f == Feel::Momentum, "{f:?}");
    }
    // Expedition: the tightest parry and the biggest counter.
    for f in ALL {
        if f != Feel::Expedition {
            assert!(t(Feel::Expedition).parry_window_ticks < t(f).parry_window_ticks);
            assert!(t(Feel::Expedition).counter_damage > t(f).counter_damage);
        }
    }
}

/// Where the crab is after each tick of `script` in `feel`.
fn xs(feel: Feel, script: &str, n: u64) -> Vec<f32> {
    let replay = Replay::parse(&format!("@0 feel={}\n{script}", feel.name())).unwrap();
    let mut sim = Sim::with_one_crab(replay.start);
    let mut out = Vec::new();
    play(&mut sim, &replay, n, |sim| {
        out.push(sim.crabs()[0].mover.pos.x)
    });
    out
}

#[test]
fn a_lunge_throws_the_body_into_the_blow_and_the_feet_brake_it() {
    let moved = |f: Feel| {
        let x = xs(f, "@10 smash", 90);
        x[89] - x[0]
    };
    assert_eq!(moved(Feel::Current), 0.0, "planted, as before");
    let heavy = moved(Feel::Heavy);
    assert!(
        (20.0..45.0).contains(&heavy),
        "a step, not a slide: {heavy}"
    );
    assert!(moved(Feel::Snappy) < heavy);
    // Facing left, it lunges left.
    let x = xs(Feel::Heavy, "@0 walk=left; @3 walk=stop; @30 smash", 120);
    assert!(x[119] < x[29] - 20.0, "{} -> {}", x[29], x[119]);
}

#[test]
fn a_press_late_in_a_recovery_chains_and_an_early_one_waits() {
    // Snappy jabs chain from recovery tick 2: jab (3 + 3) then the press.
    let s = statuses(Feel::Snappy, "@10 jab; @18 jab", 1.0);
    let starts: Vec<usize> = (1..s.len())
        .filter(|&i| s[i] == "jab windup" && s[i - 1] != "jab windup")
        .collect();
    assert_eq!(starts.len(), 2, "{:?}", runs(&s));
    // The first jab: windup from tick index 10, active from 13, recovery
    // from 16; the second comes out on its 2nd recovery tick.
    assert_eq!(starts[1] - starts[0], 3 + 3 + 2, "{:?}", runs(&s));
    // In Current nothing chains: the second jab waits for the whole swing.
    let s = statuses(Feel::Current, "@10 jab; @25 jab", 1.0);
    let starts: Vec<usize> = (1..s.len())
        .filter(|&i| s[i] == "jab windup" && s[i - 1] != "jab windup")
        .collect();
    assert_eq!(starts[1] - starts[0], 6 + 4 + 12, "{:?}", runs(&s));
}

/// The crab at the spawn against the fixture, close enough to hit at once.
/// Returns the sim after `n` ticks of `script`, with every tick's crab
/// state passed to `each`.
fn duel(feel: Feel, script: &str, n: u64, mut each: impl FnMut(&Sim)) -> Sim {
    let replay = Replay::parse(script).unwrap();
    let mut sim = Sim::with_one_crab(SPAWN);
    sim.set_feel(feel);
    sim.tuning.fixture.vitals.poise = 1000.0; // no stagger: a plain bag
    sim.tuning.fixture.vitals.health = 1000.0;
    sim.spawn_enemy(Kind::Fixture, Vec2::new(240.0, 0.0));
    play(&mut sim, &replay, n, |s| each(s));
    sim
}

#[test]
fn silksong_jabs_bounce_back_and_smash_hits_hop_up() {
    let mut first_x = None;
    let mut back = 0.0_f32;
    duel(Feel::Silksong, "@1 jab", 60, |s| {
        let x = s.crabs()[0].mover.pos.x;
        let start = *first_x.get_or_insert(x);
        back = back.min(x - start);
    });
    assert!(back < -8.0, "the jab bounced the crab back ({back})");
    let mut peak = 0.0_f32;
    duel(Feel::Silksong, "@1 smash", 120, |s| {
        peak = peak.max(s.crabs()[0].mover.pos.y);
    });
    assert!(peak > 15.0, "the smash hit hopped it up ({peak})");
    // No bounce or hop in Current.
    let mut peak = 0.0_f32;
    duel(Feel::Current, "@1 smash", 120, |s| {
        peak = peak.max(s.crabs()[0].mover.pos.y)
    });
    assert_eq!(peak, 0.0);
}

/// Damage the fixture took from one smash in Momentum, started after
/// `run_ticks` of running right (sprinting if `sprint`), held for `hold`.
fn momentum_smash(run_ticks: u64, sprint: bool, hold: u64) -> (f32, u32) {
    let mut sim = Sim::with_one_crab(SPAWN);
    sim.set_feel(Feel::Momentum);
    let t = &mut sim.tuning.fixture;
    (t.vitals.health, t.vitals.poise, t.notice_range) = (1000.0, 1000.0, 0.0);
    // Far enough that the run never reaches it: it gets placed in reach
    // once the smash comes out.
    let bag = sim.spawn_enemy(Kind::Fixture, Vec2::new(2000.0, 0.0));
    let run = Controls {
        walk: 1.0,
        dodge_pressed: sprint,
        dodge_held: sprint,
        ..Controls::default()
    };
    for i in 0..run_ticks {
        sim.set_controls(
            P0,
            Controls {
                dodge_pressed: sprint && i == 0,
                ..run
            },
        );
        sim.tick();
    }
    let smash = Controls {
        smash_pressed: true,
        smash_held: hold > 0,
        ..Controls::default()
    };
    sim.set_controls(P0, smash);
    let mut freeze = 0;
    for i in 0..200 {
        sim.tick();
        sim.set_controls(
            P0,
            Controls {
                smash_held: i + 1 < hold,
                ..Controls::default()
            },
        );
        let crab = &sim.crabs()[0];
        // Keep the bag just in front of the claw.
        let at = crab.mover.pos + Vec2::X * crab.fighter.facing * 90.0;
        let e = sim.enemies_mut().iter_mut().find(|e| e.id == bag).unwrap();
        if e.vitals.health == 1000.0 {
            e.pos = at;
        } else {
            freeze = freeze.max(e.vitals.hitstop);
        }
    }
    let hp = sim.enemies()[0].vitals.health;
    (1000.0 - hp, freeze)
}

#[test]
fn momentum_hits_harder_at_speed_and_a_held_smash_charges() {
    let (standing, still_freeze) = momentum_smash(0, false, 0);
    assert_eq!(standing, 12.0, "plain from a standstill");
    let (walking, _) = momentum_smash(60, false, 0);
    let (sprinting, sprint_freeze) = momentum_smash(60, true, 0);
    assert!(
        walking > standing && sprinting > walking,
        "{standing} {walking} {sprinting}"
    );
    assert!(sprinting <= 12.0 * 1.8 + 0.01);
    assert!(sprint_freeze > still_freeze, "harder blows freeze longer");
    // Held for the whole charge: twice as hard. A tap: the plain blow.
    let (charged, _) = momentum_smash(0, false, 200);
    assert!((charged - 24.0).abs() < 0.01, "full charge: {charged}");
    let (half, _) = momentum_smash(0, false, 12 + 30);
    assert!(half > 12.0 && half < 24.0, "half a charge: {half}");
    // Charging holds the wind-up: a full charge is 12 + 60 ticks.
    let s = runs(&statuses(Feel::Momentum, "@10 charge; @200 loose", 2.0));
    assert_eq!(s[1], ("smash windup".to_string(), 72), "{s:?}");
    let s = runs(&statuses(Feel::Momentum, "@10 charge; @40 loose", 2.0));
    assert_eq!(
        s[1],
        ("smash windup".to_string(), 30),
        "let go at 30: {s:?}"
    );
}

/// Tick of the fixture's first blow on a crab that does nothing (from tick 1).
fn impact(feel: Feel) -> usize {
    let mut hurt = Vec::new();
    duel(feel, "", 120, |s| hurt.push(s.crabs()[0].events.hurt));
    1 + hurt.iter().position(|h| *h).unwrap()
}

#[test]
fn expedition_parries_are_tighter_and_its_counter_bigger_and_faster() {
    let parried = |f: Feel, early: usize| {
        let h = impact(f);
        let mut any = false;
        duel(
            f,
            &format!("@{} parry", h - 1 - early),
            h as u64 + 10,
            |s| any |= s.crabs()[0].events.parried,
        );
        any
    };
    assert!(parried(Feel::Current, 8));
    assert!(!parried(Feel::Expedition, 8), "8 ticks early is too early");
    assert!(parried(Feel::Expedition, 5));
    // Parry, then smash: the counter's wind-up is shorter and it hits 3x.
    let h = impact(Feel::Expedition);
    let script = format!("@{} parry; @{} smash", h - 3, h + 2);
    let mut windup = 0;
    let sim = duel(Feel::Expedition, &script, h as u64 + 80, |s| {
        if s.snapshots().next().unwrap().combat == "smash windup" {
            windup += 1;
        }
    });
    assert_eq!(windup, 16 - 10, "a counter comes out fast");
    assert_eq!(1000.0 - sim.enemies()[0].vitals.health, 36.0, "12 x 3");
}

#[test]
fn a_replay_that_switches_feels_plays_back_exactly() {
    let mut sim = Sim::with_one_crab(SPAWN);
    sim.set_feel(Feel::Snappy); // before the recording: noted at its start
    sim.start_recording();
    sim.spawn_enemy(Kind::Fixture, Vec2::new(300.0, 0.0));
    for i in 0..400u32 {
        if i == 150 {
            sim.set_feel(Feel::Momentum);
        }
        let c = Controls {
            walk: if i < 40 { 1.0 } else { 0.0 },
            smash_pressed: i % 50 == 10,
            smash_held: i % 50 >= 10 && i % 50 < 30,
            jab_pressed: i % 50 == 35,
            ..Controls::default()
        };
        sim.set_controls(P0, c);
        sim.tick();
    }
    let text = sim.recording().unwrap().to_text();
    assert!(
        text.contains("@0 feel=snappy") && text.contains("@150 feel=momentum"),
        "{text}"
    );
    let replay = Replay::parse(&text).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    play(&mut again, &replay, replay.length(), |_| {});
    assert_eq!(again.feel(), Feel::Momentum);
    assert_eq!(Some(again.state_hash()), replay.hash);
}
