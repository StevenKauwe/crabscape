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
    assert!(enemy.octopus().unwrap().arms.iter().any(|a| a.state == ArmState::Slide));
    let msg = crabsim::net::Msg::Enemy { at: 40, enemy };
    let bytes = msg.to_bytes();
    assert!(bytes.len() <= crabsim::net::MAX_BYTES, "{} bytes", bytes.len());
    assert_eq!(crabsim::net::Msg::from_bytes(&bytes), Some(msg));
}
