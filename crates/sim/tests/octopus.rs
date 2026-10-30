//! The common octopus (`docs/design/creatures/common-octopus.md`), a boss in
//! its den: hidden as part of the rock, its pounce, and the arm tips it
//! probes with. Each scenario test from its page that these cover, and the
//! numbers of its implementation spec.

use bevy_math::Vec2;
use crabsim::enemy::{Part, State};
use crabsim::octopus::ArmState;
use crabsim::{Hit, Kind, Replay, Sim, Source, Tuning, play};

/// Its den: a fixture on the flat floor near the start (it lives in no
/// level yet).
const DEN: f32 = 480.0;

/// What one fight did, tick by tick.
struct Run {
    /// The crab's x after each tick.
    x: Vec<f32>,
    health: Vec<f32>,
    status: Vec<String>,
    hurt: Vec<bool>,
    parried: bool,
    evaded: bool,
    blocked: bool,
    /// The octopus's state and phase.
    enemy: Vec<(State, &'static str)>,
    /// Each arm's state.
    arms: Vec<Vec<ArmState>>,
    sim: Sim,
}

impl Run {
    fn first_hurt(&self) -> usize {
        self.hurt.iter().position(|h| *h).expect("hurt")
    }

    /// Ticks arm `i` spent in `state` before tick `before`.
    fn arm_ticks(&self, i: usize, state: ArmState, before: usize) -> usize {
        self.arms[..before].iter().filter(|a| a[i] == state).count()
    }
}

/// A crab at `crab_x` and the octopus in its den; `revealed`: it has already
/// been found out (the fight is on), else it lies hidden.
fn fight(crab_x: f32, revealed: bool, script: &str, ticks: u64) -> Run {
    fight_with(crab_x, revealed, script, ticks, |_| {})
}

fn fight_with(
    crab_x: f32,
    revealed: bool,
    script: &str,
    ticks: u64,
    set_up: impl FnOnce(&mut Sim),
) -> Run {
    let replay = Replay::parse(script).unwrap();
    let mut sim = Sim::with_one_crab(Vec2::new(crab_x, 0.0));
    sim.spawn_enemy(Kind::Octopus, Vec2::new(DEN, 0.0));
    if revealed {
        sim.enemies_mut()[0].octopus_mut().unwrap().revealed = true;
    }
    set_up(&mut sim);
    let mut r = Run {
        x: Vec::new(),
        health: Vec::new(),
        status: Vec::new(),
        hurt: Vec::new(),
        parried: false,
        evaded: false,
        blocked: false,
        enemy: Vec::new(),
        arms: Vec::new(),
        sim: sim.clone(),
    };
    play(&mut sim, &replay, ticks, |s| {
        let c = &s.crabs()[0];
        r.x.push(c.mover.pos.x);
        r.health.push(c.fighter.vitals.health);
        r.status.push(c.fighter.status(&s.tuning));
        r.hurt.push(c.events.hurt);
        r.parried |= c.events.parried;
        r.evaded |= c.events.evaded;
        r.blocked |= c.events.blocked;
        let e = &s.enemies()[0];
        r.enemy.push((e.state, e.phase(&s.tuning)));
        let o = e.octopus().unwrap();
        r.arms.push(o.arms.iter().map(|a| a.state).collect());
    });
    r.sim = sim;
    r
}

// --- Hidden, and the pounce ---------------------------------------------------

/// Walking right, past the hidden octopus.
const WALK_PAST: &str = "t=0 walk=right";

#[test]
fn hidden_it_pounces_on_a_crab_walking_past_after_only_a_dark_flush() {
    let r = fight(200.0, false, WALK_PAST, 300);
    let h = r.first_hurt();
    assert_eq!(r.enemy[0].1, "hidden", "a rock that looks back");
    let flush = r.enemy[..h].iter().filter(|e| e.1 == "flush").count();
    assert_eq!(flush, 30, "the tell: a dark flush of the skin");
    // Hidden until the flush, nothing between.
    let first_flush = r.enemy.iter().position(|e| e.1 == "flush").unwrap();
    assert!(r.enemy[..first_flush].iter().all(|e| e.1 == "hidden"));
    assert_eq!(100.0 - r.health[h], 20.0, "pounce 20");
    assert_eq!(r.status[h], "grabbed", "and it holds on");
    assert_eq!(r.enemy[h + 1].0, State::Grab);
    // Where the pounce started: within 140 of its front.
    let front = DEN - r.sim.tuning.octopus.base.body.0 / 2.0;
    let gap = front - r.x[first_flush];
    assert!(
        gap <= 140.0 && gap > 130.0,
        "as soon as it passed 140: {gap}"
    );
}

#[test]
fn a_crab_further_off_is_watched_never_pounced() {
    // Standing 280 from its den (202 from its front): seen (sight 400), not
    // close enough to pounce on.
    let r = fight(200.0, false, "", 600);
    assert!(r.enemy.iter().all(|e| e.1 == "hidden"), "it stayed a rock");
    assert!(r.hurt.iter().all(|h| !h));
    let e = &r.sim.enemies()[0];
    assert_eq!(e.target, Some(r.sim.crabs()[0].id), "but it saw the crab");
}

#[test]
fn the_pounce_can_be_dodged_or_parried() {
    let h = fight(200.0, false, WALK_PAST, 300).first_hurt();
    let dodge = fight(
        200.0,
        false,
        &format!("{WALK_PAST}; @{} walk=left dodge", h - 3),
        h as u64 + 40,
    );
    assert!(dodge.evaded && dodge.hurt.iter().all(|x| !x), "dodged");
    assert!(dodge.status.iter().all(|s| s != "grabbed"));
    let parry = fight(
        200.0,
        false,
        &format!("{WALK_PAST}; @{} parry", h - 3),
        h as u64 + 40,
    );
    assert!(parry.parried && parry.hurt.iter().all(|x| !x), "parried");
    assert!(
        parry.enemy[h..].iter().any(|e| e.0 == State::Stagger),
        "a parried pounce staggers it"
    );
}

#[test]
fn struck_while_hidden_it_is_found_out_and_its_eye_counts_double() {
    let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    sim.spawn_enemy(Kind::Octopus, Vec2::new(DEN, 0.0));
    let t = sim.tuning.clone();
    let e = &mut sim.enemies_mut()[0];
    assert!(
        e.hidden(&t) && e.hittable(&t),
        "hidden, but a claw still finds it"
    );
    e.takes_at(Part::Eye, &Hit::new(10.0, 0.0, Source::World), 300.0, &t);
    assert_eq!(e.vitals.health, 400.0 - 2.0 * 10.0, "the eye: x2");
    assert!(e.octopus().unwrap().revealed, "found out");
    assert!(!e.hidden(&t));
    sim.tick();
    assert_eq!(sim.enemies()[0].state, State::Probe, "the crab is in reach");
}

#[test]
fn a_claw_finds_the_eye_before_the_mantle_and_the_mantle_before_an_arm() {
    let sim = {
        let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
        sim.spawn_enemy(Kind::Octopus, Vec2::new(DEN, 0.0));
        sim
    };
    let t = &sim.tuning;
    let e = &sim.enemies()[0];
    let eye = e.eye(t);
    let tiny = |p: Vec2| bevy_math::Rect::from_center_size(p, Vec2::splat(2.0));
    assert_eq!(e.meets(tiny(eye), t), Some(Part::Eye));
    let middle = e.pos + Vec2::new(0.0, t.octopus.base.body.1 / 2.0);
    assert_eq!(e.meets(tiny(middle), t), Some(Part::Body));
    // Outside the ellipse, by its corner: nothing.
    let (w, h) = t.octopus.base.body;
    let corner = e.pos + Vec2::new(w / 2.0 - 4.0, h - 4.0);
    assert_eq!(e.meets(tiny(corner), t), None, "an ellipse, not a box");
}

// --- Probing arms -------------------------------------------------------------

/// The fight is on: the crab stands 280 from its den, in its arms' reach.
fn probe(script: &str, ticks: u64) -> Run {
    fight(200.0, true, script, ticks)
}

#[test]
fn each_arm_tip_slides_in_for_60_ticks_then_grabs_on_contact() {
    let r = probe("", 400);
    let h = r.first_hurt();
    assert_eq!(r.enemy[0].0, State::Probe, "the fight is on: it probes");
    let start = r.arms.iter().position(|a| a[0] == ArmState::Slide).unwrap();
    assert_eq!(h - start + 1, 60, "the tell: it touches on its 60th tick");
    assert_eq!(r.arm_ticks(0, ArmState::Slide, h), 59, "sliding till then");
    let t = &r.sim.tuning;
    assert_eq!(100.0 - r.health[h], t.octopus.arms.damage);
    assert_eq!(r.status[h], "grabbed", "grabbed on contact");
    assert_eq!(r.arms[h][0], ArmState::Hold);
    // Never more than two arms out at once.
    for a in &r.arms {
        let out = a
            .iter()
            .filter(|s| matches!(s, ArmState::Slide | ArmState::Feel))
            .count();
        assert!(out <= 2, "{a:?}");
    }
    assert!(
        r.arms[..h].iter().any(|a| a[1] == ArmState::Slide),
        "a second arm follows the first"
    );
}

#[test]
fn a_parried_probe_tip_recoils_without_grabbing() {
    let h = probe("", 400).first_hurt();
    let r = probe(&format!("@{} parry", h - 3), h as u64 + 60);
    assert!(r.parried, "parried");
    assert!(r.hurt[..=h].iter().all(|x| !x), "no damage");
    assert!(r.status.iter().all(|s| s != "grabbed"), "never grabbed");
    assert_eq!(r.arms[h][0], ArmState::Recoil, "the tip recoils");
    assert_ne!(r.enemy[h].0, State::Grab);
}

#[test]
fn a_guard_pushes_tips_back_until_its_poise_gives_out() {
    let r = probe("t=0 guard", 600);
    assert!(r.blocked);
    assert!(r.arms.iter().any(|a| a[0] == ArmState::Back), "pushed back");
    // Each blocked touch drains poise; held up long enough, the guard
    // breaks and the next tip gets through. Not before.
    let staggered = r.status.iter().position(|s| s == "staggered");
    let grabbed = r.status.iter().position(|s| s == "grabbed");
    let (Some(staggered), Some(grabbed)) = (staggered, grabbed) else {
        panic!("turtling holds forever: {staggered:?} {grabbed:?}");
    };
    assert!(staggered > 240, "it takes a while: {staggered}");
    assert!(grabbed > staggered);
}

#[test]
fn an_arm_at_0_health_stops_probing() {
    // Arm 0 cut off at the start; the crab guards for a long while.
    let r = fight_with(200.0, true, "t=0 guard", 2400, |sim| {
        let t = sim.tuning.clone();
        let e = &mut sim.enemies_mut()[0];
        e.takes_at(Part::Arm(0), &Hit::new(40.0, 0.0, Source::World), 200.0, &t);
        assert_eq!(e.octopus().unwrap().arms[0].health, 0.0);
        // Tough enough to outlast its holds and bites the whole time (dead,
        // the crab wakes at home and the octopus, left alone, mends).
        let id = sim.crabs()[0].id;
        sim.crab_mut(id).unwrap().fighter.vitals.health = 10_000.0;
    });
    assert!(r.arms.iter().all(|a| a[0] == ArmState::Lost), "never again");
    for i in 1..8 {
        assert!(
            r.arms.iter().any(|a| a[i] == ArmState::Slide),
            "arm {i} still probes"
        );
    }
}

#[test]
fn a_hurt_arm_withdraws() {
    let r = probe("", 400);
    let start = r.arms.iter().position(|a| a[0] == ArmState::Slide).unwrap();
    let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    sim.spawn_enemy(Kind::Octopus, Vec2::new(DEN, 0.0));
    sim.enemies_mut()[0].octopus_mut().unwrap().revealed = true;
    for _ in 0..start + 20 {
        sim.tick();
    }
    let t = sim.tuning.clone();
    let e = &mut sim.enemies_mut()[0];
    assert_eq!(e.octopus().unwrap().arms[0].state, ArmState::Slide);
    e.takes_at(Part::Arm(0), &Hit::new(5.0, 0.0, Source::World), 200.0, &t);
    let arm = &e.octopus().unwrap().arms[0];
    assert_eq!(arm.health, 35.0);
    assert_eq!(arm.state, ArmState::Back, "pulled back");
    assert_eq!(
        e.vitals.health, 400.0,
        "the arm's own health, not the mantle's"
    );
}

/// The crab jabs as arm 0's tip comes within its reach (a few ticks before
/// it would touch).
fn jab_the_tip(scissors: bool) -> (f32, ArmState) {
    let r = probe("", 400);
    let start = r.arms.iter().position(|a| a[0] == ArmState::Slide).unwrap();
    let windup = Tuning::default().cutter.windup_ticks as usize;
    let press = start + 55 - windup;
    let r = fight_with(
        200.0,
        true,
        &format!("@{press} jab"),
        press as u64 + 12,
        |sim| {
            let id = sim.crabs()[0].id;
            sim.crab_mut(id).unwrap().mover.scissor_claw = scissors;
        },
    );
    let arm = &r.sim.enemies()[0].octopus().unwrap().arms[0];
    (arm.health, arm.state)
}

#[test]
fn a_claw_cuts_a_probing_arm_and_scissor_claws_cut_twice_as_deep() {
    let jab = Tuning::default().cutter.damage;
    let (plain, state) = jab_the_tip(false);
    assert_eq!(plain, 40.0 - jab, "a jab on the tip");
    assert_eq!(state, ArmState::Back, "it withdraws");
    let (scissors, _) = jab_the_tip(true);
    assert_eq!(scissors, 40.0 - 2.0 * jab, "scissor claws: x2 on arms");
}

#[test]
fn a_held_crab_struggles_free_ten_ticks_a_press() {
    let h = probe("", 400).first_hurt();
    let held = probe("", h as u64 + 300);
    let ticks = held.status[h..]
        .iter()
        .take_while(|s| *s == "grabbed")
        .count();
    assert!((238..=262).contains(&ticks), "held up to 240: {ticks}");
    let mut script = String::new();
    for t in (h + 2..h + 240).step_by(4) {
        script += &format!("@{t} dodge; @{} slow\n", t + 1);
    }
    let s = probe(&script, h as u64 + 300);
    let ticks = s.status[h..].iter().take_while(|s| *s == "grabbed").count();
    // Each press takes 10 off, on top of the tick: about 240 / 3.5.
    assert!((55..=85).contains(&ticks), "struggled free in {ticks}");
    // Let go: the arm withdraws and it probes again.
    assert!(s.arms[h + ticks + 2][0] == ArmState::Back);
}

// --- Size ---------------------------------------------------------------------

/// Real sizes (cm) at crab scale (1 cm = 6.25 units).
#[test]
fn its_body_is_its_real_size_at_crab_scale() {
    let t = Tuning::default();
    let cm = |u: f32| u / 6.25;
    // Mantle to 25 cm, arms to 1 m: about four times the mantle.
    let mantle = cm(t.octopus.base.body.0);
    assert!((24.0..=25.0).contains(&mantle), "mantle {mantle}");
    let arm = cm(t.octopus.arms.length);
    assert!((95.0..=100.0).contains(&arm), "arm {arm}");
    assert_eq!(t.octopus.arms.count, 8);
}

// --- Co-op --------------------------------------------------------------------

#[test]
fn an_octopus_mid_probe_fits_in_one_co_op_message() {
    let r = probe("", 40);
    let enemy = r.sim.enemies()[0].clone();
    assert!(
        enemy
            .octopus()
            .unwrap()
            .arms
            .iter()
            .any(|a| a.state == ArmState::Slide)
    );
    let msg = crabsim::net::Msg::Enemy { at: 40, enemy };
    let bytes = msg.to_bytes();
    assert!(
        bytes.len() <= crabsim::net::MAX_BYTES,
        "{} bytes",
        bytes.len()
    );
    assert_eq!(crabsim::net::Msg::from_bytes(&bytes), Some(msg));
}

// --- Webover, the hold, the bite and autotomy ---------------------------------

/// A fight where the crab stands close, under its web: 200 from its middle.
const CLOSE: f32 = DEN - 200.0;

/// What a close fight did, tick by tick (the crab's hold and venom too).
struct Close {
    run: Run,
    /// Ticks the crab has been held so far (0 when free).
    held: Vec<u32>,
    /// Doses of venom in it.
    venom: Vec<usize>,
    /// Walking legs dropped.
    legs: Vec<u32>,
    dropped: bool,
}

fn close(script: &str, ticks: u64, set_up: impl FnOnce(&mut Sim)) -> Close {
    let replay = Replay::parse(script).unwrap();
    let mut sim = Sim::with_one_crab(Vec2::new(CLOSE, 0.0));
    sim.spawn_enemy(Kind::Octopus, Vec2::new(DEN, 0.0));
    sim.enemies_mut()[0].octopus_mut().unwrap().revealed = true;
    set_up(&mut sim);
    let (mut held, mut venom, mut legs, mut dropped) = (vec![], vec![], vec![], false);
    let mut run = Run {
        x: vec![],
        health: vec![],
        status: vec![],
        hurt: vec![],
        parried: false,
        evaded: false,
        blocked: false,
        enemy: vec![],
        arms: vec![],
        sim: sim.clone(),
    };
    play(&mut sim, &replay, ticks, |s| {
        let c = &s.crabs()[0];
        run.x.push(c.mover.pos.x);
        run.health.push(c.fighter.vitals.health);
        run.status.push(c.fighter.status(&s.tuning));
        run.hurt.push(c.events.hurt);
        run.evaded |= c.events.evaded;
        held.push(c.fighter.grabbed.map_or(0, |g| g.held));
        venom.push(c.fighter.venom.len());
        legs.push(c.fighter.legs_lost);
        dropped |= c.events.dropped_leg;
        let e = &s.enemies()[0];
        run.enemy.push((e.state, e.phase(&s.tuning)));
        run.arms
            .push(e.octopus().unwrap().arms.iter().map(|a| a.state).collect());
    });
    run.sim = sim;
    Close {
        run,
        held,
        venom,
        legs,
        dropped,
    }
}

/// The tick the web starts to drop on a crab standing still under it.
fn drop_tick() -> usize {
    let c = close("", 400, |_| {});
    c.run
        .enemy
        .iter()
        .position(|e| e.1 == "drop")
        .expect("it drops its web")
}

#[test]
fn close_under_its_web_it_rises_then_drops_it_and_caught_crabs_are_grabbed() {
    let c = close("", 400, |_| {});
    let r = &c.run;
    let rise = r.enemy.iter().position(|e| e.1 == "rise").expect("rises");
    let drop = drop_tick();
    assert_eq!(drop - rise, 84, "the tell: it rises and spreads its web");
    let caught = r
        .status
        .iter()
        .position(|s| s == "grabbed")
        .expect("caught");
    assert!(
        caught > drop && caught < drop + 10,
        "under the web as it closes"
    );
    assert_eq!(r.enemy[caught].0, State::Grab);
    // Not even probing while it rises: its arms gather in.
    assert!(r.arms[rise + 45].iter().all(|a| *a == ArmState::Curled));
}

#[test]
fn dodging_sideways_out_of_the_drop_takes_nothing() {
    let d = drop_tick();
    let c = close(
        &format!("@{} walk=left dodge", d + 2),
        d as u64 + 40,
        |_| {},
    );
    let r = &c.run;
    assert!(r.evaded, "the dodge's moment of invulnerability");
    assert!(r.hurt.iter().all(|h| !h), "took nothing");
    assert!(r.status.iter().all(|s| s != "grabbed"));
    // Missed: it lies with its arms spread on the floor, open.
    assert_eq!(r.enemy[d + 12].1, "spread");
    assert!(r.arms[d + 12].iter().all(|a| *a == ArmState::Spread));
}

#[test]
fn a_missed_webover_leaves_its_arms_out_where_hits_count_half_again() {
    let d = drop_tick();
    let mut c = close(
        &format!("@{} walk=left dodge", d + 2),
        d as u64 + 15,
        |_| {},
    );
    let t = c.run.sim.tuning.clone();
    let e = &mut c.run.sim.enemies_mut()[0];
    e.takes_at(Part::Arm(3), &Hit::new(10.0, 0.0, Source::World), 300.0, &t);
    assert_eq!(e.octopus().unwrap().arms[3].health, 40.0 - 15.0, "x1.5");
    let recover = c.run.enemy.iter().filter(|e| e.1 == "spread").count();
    assert!(recover > 0);
    let whole = close(
        &format!("@{} walk=left dodge", d + 2),
        d as u64 + 120,
        |_| {},
    );
    let spread = whole.run.enemy.iter().filter(|e| e.1 == "spread").count();
    assert_eq!(spread, 70, "recovering 70 ticks");
}

#[test]
fn it_cant_webover_in_a_low_tunnel() {
    // A roof of rock just over the crab's head where it stands.
    let c = close("", 600, |sim| {
        sim.level.rocks.push(bevy_math::Rect::new(
            CLOSE - 80.0,
            120.0,
            CLOSE + 80.0,
            160.0,
        ));
    });
    assert!(c.run.enemy.iter().all(|e| e.1 != "rise"), "never rises");
    assert!(
        c.run.arms.iter().any(|a| a.contains(&ArmState::Slide)),
        "it probes instead"
    );
}

#[test]
fn the_venom_bite_lands_at_tick_120_of_a_grab_not_before() {
    let c = close("", 900, |_| {});
    let r = &c.run;
    let caught = r.status.iter().position(|s| s == "grabbed").unwrap();
    let t = &r.sim.tuning;
    // Hurt by the web, then not again until the bite.
    let bites: Vec<usize> = (caught + 1..r.hurt.len()).filter(|&i| r.hurt[i]).collect();
    let venom = bites[0];
    assert_eq!(c.held[venom], 120, "the venom bite, at 120 held");
    assert_eq!(c.venom[venom - 1], 0);
    assert_eq!(c.venom[venom], 1, "venom in");
    let before = r.health[venom - 1];
    assert!((before - r.health[venom] - t.octopus.bite.venom_damage).abs() < 0.1);
    let heavy = bites[1];
    assert_eq!(c.held[heavy], 200, "the heavy bite, at 200");
    let lost = r.health[heavy - 1] - r.health[heavy];
    assert!((lost - 30.0).abs() < 0.1, "30 ({lost})");
}

#[test]
fn venom_hurts_2_a_second_for_5_seconds_and_stacks() {
    let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    let id = sim.crabs()[0].id;
    let bite = Hit {
        venom: true,
        ..Hit::new(0.0, 0.0, Source::World)
    };
    sim.hit_crab(id, &bite);
    sim.hit_crab(id, &bite);
    for _ in 0..120 {
        sim.tick();
    }
    let f = &sim.crabs()[0].fighter;
    assert_eq!(f.venom.len(), 2, "two doses");
    assert!(
        (100.0 - f.vitals.health - 4.0).abs() < 0.05,
        "{}",
        f.vitals.health
    );
    for _ in 0..500 {
        sim.tick();
    }
    let f = &sim.crabs()[0].fighter;
    assert!(f.venom.is_empty(), "worn off after 5 s");
    assert!(
        (100.0 - f.vitals.health - 20.0).abs() < 0.1,
        "{}",
        f.vitals.health
    );
}

/// How far the crab walks in a second from standing, with `set_up` done.
fn walks(set_up: impl FnOnce(&mut Sim)) -> f32 {
    let mut sim = Sim::with_one_crab(Vec2::new(100.0, 0.0));
    set_up(&mut sim);
    play(
        &mut sim,
        &Replay::parse("t=0 walk=right").unwrap(),
        120,
        |_| {},
    );
    sim.crabs()[0].mover.pos.x - 100.0
}

#[test]
fn venom_slows_the_crab_to_half() {
    let plain = walks(|_| {});
    let slowed = walks(|sim| {
        let id = sim.crabs()[0].id;
        let bite = Hit {
            venom: true,
            ..Hit::new(0.0, 0.0, Source::World)
        };
        sim.hit_crab(id, &bite);
    });
    let share = slowed / plain;
    assert!((0.45..=0.55).contains(&share), "{share}");
}

#[test]
fn a_bite_on_raised_claws_carries_no_venom() {
    let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    let id = sim.crabs()[0].id;
    play(&mut sim, &Replay::parse("@0 guard").unwrap(), 5, |_| {});
    let bite = Hit {
        venom: true,
        ..Hit::new(10.0, 0.0, Source::World)
    };
    let out = sim.hit_crab(id, &bite);
    assert!(matches!(out, crabsim::Outcome::Blocked(_)), "{out:?}");
    assert!(sim.crabs()[0].fighter.venom.is_empty(), "the claw took it");
}

#[test]
fn autotomy_frees_at_once() {
    let caught = close("", 400, |_| {})
        .run
        .status
        .iter()
        .position(|s| s == "grabbed")
        .unwrap();
    // Down + dodge: a walking leg dropped, free on that tick.
    let at = caught + 20;
    let c = close(&format!("@{at} down=on dodge"), at as u64 + 5, |_| {});
    assert_eq!(c.run.status[at - 1], "grabbed");
    assert_ne!(c.run.status[at], "grabbed", "free at once");
    assert!(c.dropped);
    assert_eq!(c.legs[at], 1);
    // Or (on a phone, with no down) holding dodge for half a second.
    let held = close(&format!("@{at} sprint"), at as u64 + 80, |_| {});
    let free = held.run.status[at..]
        .iter()
        .position(|s| s != "grabbed")
        .unwrap();
    let hold = Tuning::default().autotomy.hold_ticks as usize;
    assert!((hold - 1..=hold + 1).contains(&free), "after {free}");
    assert_eq!(*held.legs.last().unwrap(), 1);
}

#[test]
fn each_dropped_leg_costs_8_percent_of_walking_until_the_next_moult() {
    let plain = walks(|_| {});
    let two = walks(|sim| {
        let id = sim.crabs()[0].id;
        sim.crab_mut(id).unwrap().fighter.legs_lost = 2;
    });
    let share = two / plain;
    assert!((0.82..=0.86).contains(&share), "{share}");
    // Dying is the moult (`world.md`): it wakes with every leg.
    let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    let id = sim.crabs()[0].id;
    sim.crab_mut(id).unwrap().fighter.legs_lost = 2;
    sim.hit_crab(id, &Hit::new(1000.0, 0.0, Source::World));
    for _ in 0..400 {
        sim.tick();
    }
    assert_eq!(sim.crabs()[0].fighter.legs_lost, 0, "regrown");
}

// --- Ink and jet, fleeing, and its den -----------------------------------------

/// Beat on its mantle, 10 at a time whenever it can be hit, until it is gone
/// or `ticks` run out. The crab stands in its arms' reach, made tough.
/// Returns the run and the mantle's health each time it inked.
fn beat(ticks: u64) -> (Run, Vec<f32>) {
    let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    sim.spawn_enemy(Kind::Octopus, Vec2::new(DEN, 0.0));
    sim.enemies_mut()[0].octopus_mut().unwrap().revealed = true;
    let id = sim.crabs()[0].id;
    sim.crab_mut(id).unwrap().fighter.vitals.health = 100_000.0;
    let t = sim.tuning.clone();
    let mut r = fight(200.0, true, "", 0);
    let mut inked = Vec::new();
    let mut before = State::Idle;
    for i in 0..ticks {
        if i % 20 == 0 {
            let e = &mut sim.enemies_mut()[0];
            if e.hittable(&t) {
                e.takes_at(Part::Body, &Hit::new(10.0, 0.0, Source::World), 200.0, &t);
            }
        }
        sim.tick();
        let e = &sim.enemies()[0];
        if e.state == State::Flee && before != State::Flee {
            inked.push(e.vitals.health);
        }
        before = e.state;
        r.enemy.push((e.state, e.phase(&t)));
        r.x.push(e.pos.x);
    }
    r.sim = sim;
    (r, inked)
}

#[test]
fn ink_at_66_and_33_percent_exactly_once_each() {
    let (r, inked) = beat(9000);
    assert_eq!(inked.len(), 3, "two inks, then fleeing: {inked:?}");
    assert!(
        inked[0] <= 0.66 * 400.0 && inked[0] > 0.66 * 400.0 - 10.0,
        "{inked:?}"
    );
    assert!(
        inked[1] <= 0.33 * 400.0 && inked[1] > 0.33 * 400.0 - 10.0,
        "{inked:?}"
    );
    assert_eq!(inked[2], 0.0, "at 0 it flees");
    let ink = r.enemy.iter().filter(|e| e.1 == "ink").count();
    assert_eq!(ink, 2 * 30, "each ink and jet lasts 30 ticks");
}

#[test]
fn after_inking_it_reappears_600_or_more_away() {
    let (r, _) = beat(9000);
    let first = r.enemy.iter().position(|e| e.1 == "ink").unwrap();
    let back = first + r.enemy[first..].iter().position(|e| e.1 != "ink").unwrap();
    let moved = (r.x[back] - DEN).abs();
    assert!(moved >= 600.0, "jetted {moved}");
    let level = &r.sim.level;
    let e_y = level.floor_height(r.x[back]);
    assert!(e_y.is_finite());
    // It fights on from there.
    assert!(
        r.enemy[back..]
            .iter()
            .any(|e| e.0 == State::Probe || e.0 == State::Idle),
        "it carries on"
    );
}

#[test]
fn the_ink_cloud_blinds_a_crab_inside_it() {
    let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    sim.spawn_enemy(Kind::Octopus, Vec2::new(DEN, 0.0));
    let t = sim.tuning.clone();
    sim.enemies_mut()[0].octopus_mut().unwrap().ink = Some(crabsim::octopus::Ink {
        at: Vec2::new(250.0, 30.0),
        left: t.octopus.ink.cloud_ticks,
    });
    sim.tick();
    assert!(sim.crabs()[0].fighter.blinded, "inside the cloud");
    for _ in 0..t.octopus.ink.cloud_ticks {
        sim.tick();
    }
    assert!(!sim.crabs()[0].fighter.blinded, "the cloud is gone");
    assert!(sim.enemies()[0].octopus().unwrap().ink.is_none());
}

#[test]
fn inside_the_ink_a_sight_hunting_enemy_loses_the_crab() {
    // A mantis (a sight hunter) closes in on the crab...
    let hunt = |ink: bool, ticks: u32| {
        let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
        sim.spawn_enemy(Kind::Mantis, Vec2::new(420.0, 0.0));
        sim.enemies_mut()[0].pos.x = 440.0; // out of its burrow
        sim.spawn_enemy(Kind::Octopus, Vec2::new(1000.0, 0.0));
        for _ in 0..30 {
            sim.tick();
        }
        assert!(sim.enemies()[0].target.is_some(), "it has seen the crab");
        if ink {
            let left = sim.tuning.octopus.ink.cloud_ticks;
            sim.enemies_mut()[1].octopus_mut().unwrap().ink = Some(crabsim::octopus::Ink {
                at: Vec2::new(200.0, 30.0),
                left,
            });
        }
        let mut hurt = false;
        for _ in 0..ticks {
            sim.tick();
            hurt |= sim.crabs()[0].events.hurt;
        }
        (sim.enemies()[0].target, hurt)
    };
    let (target, hurt) = hunt(false, 250);
    assert!(target.is_some() && hurt, "out of the ink it strikes");
    // (While the cloud lasts: 180 ticks.)
    let (target, hurt) = hunt(true, 170);
    assert!(target.is_none(), "...and loses it in the ink");
    assert!(!hurt, "no strike at a crab it can't see");
}

#[test]
fn at_0_health_it_jets_off_leaving_no_body_and_its_den_open() {
    let (r, _) = beat(9000);
    let e = &r.sim.enemies()[0];
    assert_eq!(e.state, State::Gone);
    assert!(e.octopus().unwrap().fled, "it got away");
    assert_eq!(r.enemy.last().unwrap().1, "gone");
    assert!(!e.hittable(&r.sim.tuning));
    // No body to eat where it was.
    let mut sim = r.sim.clone();
    let id = sim.crabs()[0].id;
    let mut fed = false;
    let x = e.pos.x;
    sim.crab_mut(id).unwrap().mover.pos.x = x - 50.0;
    for _ in 0..200 {
        sim.tick();
        fed |= sim.crabs()[0].events.fed;
    }
    assert!(!fed, "no body parts: it got away");
}

#[test]
fn its_den_then_holds_a_midden_of_shell_grit() {
    let (r, _) = beat(9000);
    let mut sim = r.sim.clone();
    let id = sim.crabs()[0].id;
    let t = sim.tuning.clone();
    let grit = |sim: &Sim| {
        let m = &sim.crabs()[0].mover;
        m.load().filter(|c| *c == crabsim::Carry::Grit).count() as u32
    };
    let mut got = 0;
    for _ in 0..t.octopus.den.midden + 2 {
        let c = sim.crab_mut(id).unwrap();
        c.mover.clinging.clear();
        c.mover.pos = Vec2::new(DEN, 0.0);
        sim.tick();
        got += grit(&sim);
    }
    assert_eq!(got, t.octopus.den.midden, "a few loads, then it's bare");
    // Before it has fled, the den gives nothing.
    let mut sim = Sim::with_one_crab(Vec2::new(DEN, 0.0));
    sim.spawn_enemy(Kind::Octopus, Vec2::new(DEN, 0.0));
    sim.tick();
    assert_eq!(grit(&sim), 0);
}

#[test]
fn it_lives_hidden_in_its_den_at_the_end_of_the_far_rim() {
    let level = crabsim::Level::test_level();
    let t = Tuning::default();
    let (_, at) = level
        .foes
        .iter()
        .find(|(k, _)| *k == Kind::Octopus)
        .expect("an octopus in the level");
    let den = level.den;
    let (w, _) = t.octopus.base.body;
    // All of it inside the chamber, on its floor, facing out of it (the wall at its back).
    assert!(
        at.x - w / 2.0 >= den.min.x && at.x + w / 2.0 <= den.max.x,
        "{at}"
    );
    assert_eq!(level.floor_height(at.x), den.min.y);
    // Room over a crab in the chamber to webover.
    assert!(den.max.y - den.min.y >= t.octopus.webover.headroom);
    let mut sim = Sim::new(Tuning::default(), level.clone());
    sim.populate();
    let e = sim
        .enemies()
        .iter()
        .find(|e| e.kind == Kind::Octopus)
        .unwrap();
    assert!(e.facing < 0.0, "faces the way in");
    assert!(e.hidden(&sim.tuning));
}

// --- Always there: back in its den, whatever happened ----------------------------

/// The level as the game starts it: every animal in place, the crab at `at`.
fn level_world(at: Vec2) -> Sim {
    let mut sim = Sim::with_one_crab(at);
    sim.populate();
    sim
}

/// The level's octopus.
fn the_octopus(sim: &Sim) -> &crabsim::Enemy {
    sim.enemies()
        .iter()
        .find(|e| e.kind == Kind::Octopus)
        .expect("the level's octopus")
}

/// Where the level's octopus lives.
fn its_den(sim: &Sim) -> Vec2 {
    let (_, at) = *sim
        .level
        .foes
        .iter()
        .find(|(k, _)| *k == Kind::Octopus)
        .unwrap();
    Vec2::new(at.x, sim.level.floor_height(at.x))
}

/// It is there as a crab coming in would find it: hidden in its den, whole.
fn at_home(sim: &Sim) -> bool {
    let e = the_octopus(sim);
    let o = e.octopus().unwrap();
    e.state == State::Idle
        && e.pos == its_den(sim)
        && e.hidden(&sim.tuning)
        && !o.fled
        && e.vitals.health == sim.tuning.octopus.base.vitals.health
}

/// Beat it with blows on its mantle until it has jetted off.
fn beat_it(sim: &mut Sim) {
    let t = sim.tuning.clone();
    let id = sim.crabs()[0].id;
    sim.crab_mut(id).unwrap().fighter.vitals.health = 100_000.0;
    for i in 0..20_000 {
        let e = sim
            .enemies_mut()
            .iter_mut()
            .find(|e| e.kind == Kind::Octopus)
            .unwrap();
        if e.state == State::Gone {
            return;
        }
        if i % 20 == 0 && e.hittable(&t) {
            let x = e.pos.x - 100.0;
            e.takes_at(Part::Body, &Hit::new(40.0, 0.0, Source::World), x, &t);
        }
        sim.tick();
    }
    panic!("never beaten");
}

/// Run `secs` seconds of nothing in particular.
fn wait(sim: &mut Sim, secs: u32) {
    for _ in 0..secs * 120 {
        sim.tick();
    }
}

#[test]
fn beaten_it_comes_back_to_its_den_once_crabs_have_left_it_alone() {
    let den = {
        let sim = level_world(Vec2::ZERO);
        its_den(&sim)
    };
    let mut sim = level_world(den - Vec2::new(200.0, 0.0));
    beat_it(&mut sim);
    assert!(the_octopus(&sim).octopus().unwrap().fled, "it got away");
    // While a crab stays by its den, the den is open (its midden is there
    // to take) and it stays away, however long.
    wait(&mut sim, 6 * 60);
    assert_eq!(the_octopus(&sim).state, State::Gone, "away while watched");
    // The crab goes home; a while later the octopus is back, hidden in its
    // den, whole again, its midden guarded.
    let id = sim.crabs()[0].id;
    sim.crab_mut(id).unwrap().mover.pos = crabsim::SPAWN;
    let away = sim.tuning.octopus.base.respawn_ticks;
    assert!(away > 0, "it comes back");
    wait(&mut sim, away / 120 + 1);
    assert!(at_home(&sim), "{:?}", the_octopus(&sim));
}

#[test]
fn a_crab_dying_in_its_den_leaves_it_there() {
    let mut sim = level_world(Vec2::ZERO);
    let den = its_den(&sim);
    let id = sim.crabs()[0].id;
    sim.crab_mut(id).unwrap().mover.pos = den - Vec2::new(200.0, 0.0);
    let mut died = false;
    for _ in 0..60 * 120 {
        sim.tick();
        died |= sim.crabs()[0].fighter.dead.is_some();
    }
    assert!(died, "it caught the crab");
    assert!(
        sim.crabs()[0].mover.pos.distance(den) > 2000.0,
        "woke at home"
    );
    wait(&mut sim, 10);
    assert!(at_home(&sim), "{:?}", the_octopus(&sim));
}

#[test]
fn it_is_there_again_when_the_game_is_loaded() {
    // Beaten and its midden taken, then the game reloads: the save brings
    // the crab back, and the level's animals are put in afresh.
    let mut sim = level_world(Vec2::ZERO);
    let den = its_den(&sim);
    let id = sim.crabs()[0].id;
    sim.crab_mut(id).unwrap().mover.pos = den - Vec2::new(200.0, 0.0);
    beat_it(&mut sim);
    sim.crab_mut(id).unwrap().mover.pos = den;
    sim.tick();
    let saved = sim.progress(id).unwrap();
    let mut loaded = Sim::new(Tuning::default(), crabsim::Level::test_level());
    loaded.add_crab(id, crabsim::SPAWN);
    loaded.restore_progress(id, &saved);
    loaded.start_recording();
    loaded.populate();
    assert!(at_home(&loaded));
}

#[test]
fn monkeys_dropping_into_its_den_leave_it_there() {
    let mut sim = level_world(Vec2::ZERO);
    let den = its_den(&sim);
    let id = sim.crabs()[0].id;
    sim.crab_mut(id).unwrap().mover.pos = den - Vec2::new(500.0, 0.0);
    sim.crab_mut(id).unwrap().fighter.vitals.health = 100_000.0;
    assert!(sim.enter_typed(id, "monkeys"));
    wait(&mut sim, 5);
    // Every monkey visits and goes; the crab goes home.
    let stay = (crabsim::codes::MONKEY_STAY + crabsim::codes::MONKEY_LINGER) / 120;
    wait(&mut sim, stay + 2);
    assert!(sim.enemies().iter().all(|e| e.kind != Kind::Monkey));
    sim.crab_mut(id).unwrap().mover.pos = crabsim::SPAWN;
    wait(&mut sim, 10);
    assert!(at_home(&sim), "{:?}", the_octopus(&sim));
}

#[test]
fn a_hulk_crab_it_cannot_hold_leaves_it_there() {
    let mut sim = level_world(Vec2::ZERO);
    let den = its_den(&sim);
    let id = sim.crabs()[0].id;
    assert!(sim.enter_typed(id, "hulk"));
    sim.crab_mut(id).unwrap().mover.pos = den - Vec2::new(200.0, 0.0);
    // It pounces, webs and probes at a crab it can't hold, for a minute.
    for _ in 0..60 * 120 {
        sim.tick();
        assert!(
            sim.crabs()[0].fighter.grabbed.is_none(),
            "nothing holds hulk"
        );
    }
    assert_eq!(the_octopus(&sim).pos, den, "still in its den");
    assert_ne!(the_octopus(&sim).state, State::Gone);
    sim.crab_mut(id).unwrap().mover.pos = crabsim::SPAWN;
    wait(&mut sim, 10);
    assert!(at_home(&sim), "{:?}", the_octopus(&sim));
}
