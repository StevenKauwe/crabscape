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

/// A wind-up after a turn: the turn, then the wind-up, overlapping by up
/// to half of each.
fn turned(windup: u32, turn: u32) -> u32 {
    windup + turn - (windup / 2).min(turn / 2)
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
        // Facing right, the crusher (on the crab's right, screen-left) is on
        // the wrong side: the body turns round first, adding to the wind-up.
        let (w, a, r) = spec.crusher.ticks;
        let smash = runs(&statuses(feel, "@10 smash", 1.0));
        assert_eq!(
            smash[1..4],
            phases("smash", (turned(w, spec.turn), a, r)),
            "{feel:?} {smash:?}"
        );
        // Facing left, it is already on the right side: no turn.
        let left = runs(&statuses(
            feel,
            "@0 walk=left; @2 walk=stop; @10 smash",
            1.0,
        ));
        let windup = left.iter().find(|(n, _)| n == "smash windup").unwrap().1;
        assert_eq!(windup, w, "{feel:?} {left:?}");
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
    // Heavy freezes (at least as) long as any on a plain smash.
    let longest = ALL.into_iter().map(|f| t(f).crusher.hitstop_ticks).max();
    assert_eq!(Some(t(Feel::Heavy).crusher.hitstop_ticks), longest);
    // Charging: Momentum and the hybrids built on it; run speed: Momentum, G.
    let charges = [Feel::Momentum, Feel::Hybrid, Feel::Brawler];
    for f in ALL {
        assert_eq!(t(f).charge_ticks > 0, charges.contains(&f), "{f:?}");
        let speed = [Feel::Momentum, Feel::Hybrid].contains(&f);
        assert_eq!(t(f).momentum > 0.0, speed, "{f:?}");
        // Every preset but A crushes on a counter.
        assert_eq!(t(f).crush.is_some(), f != Feel::Current, "{f:?}");
    }
    // Expedition: the tightest parry, the biggest counter. G keeps its counter
    // but its parry is relaxed a fraction (the owner found 6 ticks too tight).
    for f in ALL {
        assert!(t(Feel::Expedition).parry_window_ticks <= t(f).parry_window_ticks);
        assert!(t(Feel::Expedition).counter_damage >= t(f).counter_damage);
    }
    assert_eq!(t(Feel::Hybrid).parry_window_ticks, 9);
    // G really is D + E + F: momentum and charge, E's jab and hop, F's parry.
    let g = t(Feel::Hybrid);
    assert_eq!(g.momentum, t(Feel::Momentum).momentum);
    assert_eq!(g.cutter.windup_ticks, t(Feel::Silksong).cutter.windup_ticks);
    assert!(g.crusher.hop > 0.0 && g.cutter.recoil > 0.0);
    assert_eq!(g.counter_damage, t(Feel::Expedition).counter_damage);
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
    // Facing left the crusher is on the near side: no turn, only the lunge.
    let moved = |f: Feel| {
        let x = xs(f, "@0 walk=left; @3 walk=stop; @30 smash", 150);
        x[29] - x[149]
    };
    assert_eq!(moved(Feel::Current), 0.0, "planted, as before");
    let heavy = moved(Feel::Heavy);
    assert!(
        (20.0..45.0).contains(&heavy),
        "a step, not a slide: {heavy}"
    );
    assert!(moved(Feel::Snappy) < heavy);
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
    assert_eq!(
        s[1],
        ("smash windup".to_string(), turned(12, 12) + 60),
        "turn, then the wind-up and the charge: {s:?}"
    );
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
    let sim = duel(Feel::Expedition, &script, h as u64 + 200, |s| {
        if s.snapshots().next().unwrap().combat == "smash windup" {
            windup += 1;
        }
    });
    assert_eq!(
        windup,
        turned(16 - 10, 14),
        "a counter comes out fast (after its turn)"
    );
    // It clamps on and crushes: three cracks of 2, then the 12 x 3 blow.
    assert_eq!(
        1000.0 - sim.enemies()[0].vitals.health,
        42.0,
        "12 x 3 + 3 x 2"
    );
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

#[test]
fn a_counter_clamps_on_cracks_three_times_then_lets_go_hard() {
    for feel in ALL.into_iter().skip(1) {
        let h = impact(feel);
        let script = format!("@{} parry; @{} smash", h - 3, h + 2);
        let (mut seized, mut cracked, mut crushed, mut crushing) = (0, 0, 0, 0);
        let mut held_still = true;
        let mut at = None;
        let sim = duel(feel, &script, h as u64 + 240, |s| {
            let ev = s.crabs()[0].events;
            seized += u32::from(ev.seized);
            cracked += u32::from(ev.cracked);
            crushed += u32::from(ev.crushed);
            if s.crabs()[0].fighter.crush.is_some() {
                crushing += 1;
                let x = s.enemies()[0].pos.x;
                held_still &= *at.get_or_insert(x) == x;
            }
        });
        assert_eq!((seized, cracked, crushed), (1, 3, 1), "{feel:?}");
        assert!(crushing >= 54, "{feel:?}: held {crushing} ticks");
        assert!(held_still, "{feel:?}: the target stays in the claw");
        assert_eq!(sim.enemies()[0].cracks, 3);
        assert!(!sim.enemies()[0].held, "{feel:?}: let go");
    }
    // A has no crush: a plain counter.
    let h = impact(Feel::Current);
    let mut seized = false;
    duel(
        Feel::Current,
        &format!("@{} parry; @{} smash", h - 3, h + 2),
        h as u64 + 120,
        |s| seized |= s.crabs()[0].events.seized,
    );
    assert!(!seized);
}

#[test]
fn the_crab_turns_round_to_strike_across_and_never_swaps_claws() {
    use crabsim::combat::Claw;
    // The claws' sides: from the front the crusher is screen-left, from
    // behind screen-right; the cutter the other way. Always opposite.
    for back in [false, true] {
        assert_eq!(
            Claw::Crusher.screen_side(back),
            -Claw::Cutter.screen_side(back)
        );
    }
    assert_eq!(Claw::Crusher.screen_side(false), -1.0);
    let feel = Feel::Hybrid;
    let t = feel.tuning();
    let replay = Replay::parse(&format!(
        "@0 feel={}\n@10 smash; @80 jab; @140 walk=left; @142 walk=stop; @150 jab; @200 smash",
        feel.name()
    ))
    .unwrap();
    let mut sim = Sim::with_one_crab(SPAWN);
    let mut back = Vec::new();
    let mut turns = Vec::new();
    play(&mut sim, &replay, 700, |s| {
        let c = &s.crabs()[0];
        back.push(c.fighter.back);
        if c.events.turned {
            turns.push(s.tick_count());
        }
        // Whatever it does, the striking claw is on the side of the blow.
        if let Some(sw) = &c.fighter.swing
            && c.fighter.turn == 0
        {
            assert_eq!(sw.claw.screen_side(c.fighter.back), c.fighter.facing);
        }
    });
    // Smash right: turns (back). Jab right: the cutter is now screen-left:
    // turns to the front. Jab left from the front: turns (back). Smash left
    // from behind: the crusher is screen-right: turns to the front.
    assert_eq!(turns.len(), 4, "{turns:?}");
    assert!(back[20] && !back[100] && back[160] && !back[260]);
    // Out of the fight with its back shown, it turns to the front again.
    let mut sim = Sim::with_one_crab(SPAWN);
    sim.set_feel(feel);
    let replay = Replay::parse("@10 smash").unwrap();
    let mut when = None;
    play(&mut sim, &replay, 600, |s| {
        if !s.crabs()[0].fighter.back && s.tick_count() > 20 && when.is_none() {
            when = Some(s.tick_count());
        }
    });
    let swing_end = 10 + (turned(12, t.turn_ticks) + 5 + 20) as u64;
    let w = when.expect("turns back");
    assert!((swing_end + 230..swing_end + 260).contains(&w), "{w}");
}

#[test]
fn the_hitboxes_reach_as_far_as_the_longer_claws() {
    let t = Feel::Current.tuning();
    assert_eq!((t.crusher.reach, t.cutter.reach), (44.0, 38.0));
    assert!(t.cutter.reach < t.crusher.reach);
}

#[test]
fn a_turn_into_a_blow_is_a_readable_step_that_covers_ground() {
    for feel in ALL.into_iter().skip(1) {
        let t = feel.tuning();
        assert!(
            (10..=18).contains(&t.turn_ticks),
            "{feel:?}: {} ticks",
            t.turn_ticks
        );
        // Smash to the right from the front: it pivots and steps that way.
        let x = xs(feel, "@10 smash", 10 + t.turn_ticks as u64 + 2);
        let step = x[x.len() - 1] - x[9];
        assert!(
            step > t.turn_travel * 0.75 && step < t.turn_travel * 1.6,
            "{feel:?}: stepped {step} for {}",
            t.turn_travel
        );
        assert!((20.0..=40.0).contains(&t.turn_travel));
    }
    // A press during the turn is kept and comes out after the swing.
    let s = statuses(Feel::Heavy, "@10 smash; @14 jab", 2.0);
    assert!(s.iter().any(|x| x == "jab windup"), "{:?}", runs(&s));
}
