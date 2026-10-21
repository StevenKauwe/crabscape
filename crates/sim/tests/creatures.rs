//! The snapping shrimp and the snowflake moray (`docs/design/creatures/`):
//! each one's scenario tests from its page.

use bevy_math::Vec2;
use crabsim::enemy::State;
use crabsim::{Kind, Replay, Sim, play};

struct Run {
    /// Crab health after each tick.
    health: Vec<f32>,
    /// The crab's fight status after each tick.
    status: Vec<String>,
    /// The enemy's state and phase after each tick.
    enemy: Vec<(State, &'static str)>,
    hurt: Vec<bool>,
    evaded: bool,
    blocked: bool,
    sim: Sim,
}

fn run(kind: Kind, crab_x: f32, enemy_x: f32, script: &str, ticks: u64) -> Run {
    let replay = Replay::parse(script).unwrap();
    let mut sim = Sim::with_one_crab(Vec2::new(crab_x, 0.0));
    sim.spawn_enemy(kind, Vec2::new(enemy_x, 0.0));
    let mut r = Run {
        health: Vec::new(),
        status: Vec::new(),
        enemy: Vec::new(),
        hurt: Vec::new(),
        evaded: false,
        blocked: false,
        sim: sim.clone(),
    };
    play(&mut sim, &replay, ticks, |s| {
        let c = &s.crabs()[0];
        r.health.push(c.fighter.vitals.health);
        r.status.push(c.fighter.status(&s.tuning));
        r.hurt.push(c.events.hurt);
        r.evaded |= c.events.evaded;
        r.blocked |= c.events.blocked;
        let e = &s.enemies()[0];
        r.enemy.push((e.state, e.phase(&s.tuning)));
    });
    r.sim = sim;
    r
}

// --- Snapping shrimp ---------------------------------------------------------

/// Walk up to the shrimp at 300 and stop in reach of its snap.
const WALK_UP: &str = "t=0 walk=right; t=0.3 walk=stop";

fn first_hurt(r: &Run) -> usize {
    r.hurt.iter().position(|h| *h).expect("hurt")
}

#[test]
fn a_snapping_shrimp_feels_steps_cocks_and_snaps_along_the_floor() {
    let r = run(Kind::Snapper, 160.0, 300.0, WALK_UP, 300);
    let h = first_hurt(&r);
    assert_eq!(100.0 - r.health[h], 6.0, "6 damage");
    // It was in its burrow, then guarded, then cocked its claw and held.
    assert!(r.enemy.iter().any(|e| e.1 == "burrowed"));
    let cocked = r.enemy[..h].iter().filter(|e| e.1 == "cocked").count();
    assert_eq!(cocked, 30, "the held claw before the snap");
    let tell = r.enemy[..h].iter().filter(|e| e.0 == State::Tell).count();
    assert_eq!(tell, 54);
}

#[test]
fn jump_or_dodge_the_shock_but_a_parry_does_nothing() {
    let h = first_hurt(&run(Kind::Snapper, 160.0, 300.0, WALK_UP, 300));
    let jump = run(
        Kind::Snapper,
        160.0,
        300.0,
        &format!("{WALK_UP}; @{} jump", h - 18),
        h as u64 + 20,
    );
    assert!(
        jump.hurt.iter().all(|x| !x),
        "a jump clears the shock along the floor"
    );
    let dodge = run(
        Kind::Snapper,
        160.0,
        300.0,
        &format!("{WALK_UP}; @{} walk=right dodge", h - 3),
        h as u64 + 20,
    );
    assert!(
        dodge.evaded && dodge.hurt.iter().all(|x| !x),
        "dodged through it"
    );
    let parry = run(
        Kind::Snapper,
        160.0,
        300.0,
        &format!("{WALK_UP}; @{} parry", h - 4),
        h as u64 + 20,
    );
    assert!(parry.hurt.iter().any(|x| *x), "a shock can't be parried");
    let block = run(
        Kind::Snapper,
        160.0,
        300.0,
        &format!("{WALK_UP}; t=0.35 block"),
        h as u64 + 20,
    );
    let lost = 100.0 - block.health.last().unwrap();
    assert!(
        block.blocked && (lost - 1.2).abs() < 0.01,
        "blocked: x0.2 ({lost})"
    );
}

#[test]
fn creeping_past_a_snapping_shrimp_goes_unnoticed() {
    // Walking at 40% (slower than half speed) right past it.
    let r = run(
        Kind::Snapper,
        160.0,
        300.0,
        "t=0 walk=0.4; t=1.4 walk=stop",
        400,
    );
    assert!(r.enemy.iter().all(|e| e.1 == "burrowed"), "never came out");
    assert!(r.hurt.iter().all(|x| !x));
}

// --- Snowflake moray ---------------------------------------------------------

#[test]
fn a_moray_smells_a_crab_that_lingers_and_never_one_out_of_range() {
    let near = run(Kind::Moray, 160.0, 400.0, "", 400);
    let noticed = near
        .enemy
        .iter()
        .position(|e| e.0 == State::Notice)
        .expect("it smells the crab");
    assert!(
        (118..=124).contains(&noticed),
        "after ~120 ticks of scent ({noticed})"
    );
    let far = run(Kind::Moray, 100.0, 400.0, "", 600);
    assert!(
        far.enemy.iter().all(|e| e.1 == "burrowed"),
        "out of range: never"
    );
}

/// The crab in reach of the moray's lunge from its crevice at 400.
fn moray(script: &str, ticks: u64) -> Run {
    run(Kind::Moray, 290.0, 400.0, script, ticks)
}

fn bite_tick() -> usize {
    first_hurt(&moray("", 600))
}

#[test]
fn an_unblocked_lunge_grabs_and_holds_on_until_struggled_free() {
    let b = bite_tick();
    let r = moray("", b as u64 + 200);
    assert_eq!(100.0 - r.health[b], 15.0, "the bite");
    assert_eq!(r.status[b], "grabbed");
    assert!(r.enemy[b + 1..b + 40].iter().all(|e| e.0 == State::Grab));
    // Held: it bites again every 30 ticks (4 each); it lets go after 150.
    let held = r.status[b..].iter().take_while(|s| *s == "grabbed").count();
    // (150 ticks of hold; the bites' freezes don't count.)
    assert!((148..=175).contains(&held), "held {held}");
    let end = r.health[b + held - 1];
    assert!(
        end == 100.0 - 15.0 - 4.0 * 4.0 || end == 100.0 - 15.0 - 4.0 * 5.0,
        "{end}"
    );
    // Struggling (a dodge press every 4 ticks) frees the crab in about 50.
    let mut script = String::new();
    for t in (b + 2..b + 160).step_by(4) {
        script += &format!("@{t} dodge; @{} slow\n", t + 1);
    }
    let s = moray(&script, b as u64 + 200);
    let held = s.status[b..].iter().take_while(|s| *s == "grabbed").count();
    assert!((40..=60).contains(&held), "struggled free in {held}");
}

#[test]
fn a_block_takes_a_little_and_is_never_a_grab_and_a_parry_staggers_it() {
    let b = bite_tick();
    let block = moray("@0 block", b as u64 + 60);
    assert!(block.blocked);
    assert!(block.status.iter().all(|s| s != "grabbed"), "never a grab");
    assert_eq!(100.0 - block.health.last().unwrap(), 3.0, "15 x 0.2");
    let parry = moray(&format!("@{} parry", b - 3), b as u64 + 30);
    assert!(parry.hurt.iter().all(|x| !x));
    assert!(
        parry.enemy.iter().any(|e| e.0 == State::Stagger),
        "parried: staggered"
    );
    let _ = parry.sim;
}

#[test]
fn a_morays_head_out_of_its_crevice_is_what_a_claw_hits() {
    let b = bite_tick();
    let r = moray("", b as u64 + 1);
    let (e, c) = (&r.sim.enemies()[0], &r.sim.crabs()[0]);
    let t = &r.sim.tuning;
    let crevice = e.kind.tuning(t).body.0;
    let body = e.hurtbox(t);
    // Holding on: its box runs out of the crevice to the crab it holds.
    assert!(e.reach > 0.0 && body.width() > crevice, "{body:?}");
    let gap = body.min.x - (c.mover.pos.x + t.body_half_width);
    assert!(gap < 2.0 * e.kind.tuning(t).strike.radius, "gap {gap}");
    // Then the head draws back in.
    let later = moray("@0 block", b as u64 + 300);
    let e = &later.sim.enemies()[0];
    assert_eq!(e.reach, 0.0);
    assert_eq!(e.hurtbox(t).width(), crevice);
}

/// Real sizes (cm) at crab scale (1 cm = 6.25 units; `docs/design/creatures/README.md`).
#[test]
fn every_body_is_its_real_size_at_crab_scale() {
    let t = crabsim::Tuning::default();
    let cm = |u: f32| u / 6.25;
    // Crab: a 12 cm carapace, a blue crab just grown up (females mature at
    // 11.3-13.0 cm, up to 23 cm).
    let crab = cm(2.0 * t.body_half_width);
    assert!((11.0..=13.0).contains(&crab), "crab {crab}");
    // Big-clawed snapping shrimp: up to 5.5 cm long.
    let snapper = cm(t.snapper.body.0);
    assert!((3.0..=5.5).contains(&snapper), "snapper {snapper}");
    // Peacock mantis shrimp: 3-18 cm long.
    let mantis = cm(t.mantis.body.0);
    assert!((3.0..=18.0).contains(&mantis), "mantis {mantis}");
    // Snowflake moray, 60 cm: deep at the gill opening a 14th to a 23rd of that.
    let depth = cm(t.moray.body.1);
    assert!(
        (60.0 / 23.0..=60.0 / 14.0).contains(&depth),
        "moray {depth}"
    );
}
