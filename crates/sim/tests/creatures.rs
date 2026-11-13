//! The snapping shrimp and the snowflake moray (`docs/design/creatures/`):
//! each one's scenario tests from its page.

use bevy_math::Vec2;
use crabsim::enemy::State;
use crabsim::{Controls, Kind, Replay, Sim, play};

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

/// Real sizes (cm) at crab scale (1 cm = 6.25 units), every animal's hurtbox on
/// the research's own measure (`docs/design/creatures/README.md`, "Sizes at crab
/// scale"; the drawings are checked against the same table in `src/sizes.rs`).
#[test]
fn every_body_is_its_real_size_at_crab_scale() {
    let t = crabsim::Tuning::default();
    let cm = |u: f32| u / 6.25;
    let long = |k: Kind| cm(k.tuning(&t).body.0);
    let sea = crabsim::tuning::SEA_MORAY_SCALE;
    // (animal, its hurtbox along the measure, the measure, the real range in cm).
    let rows = [
        // Width between the lateral spines; drawn 13.6 cm tip to tip (females
        // mature at 11.3-13.0 cm tip to tip).
        (
            "crab",
            cm(2.0 * t.body_half_width),
            "carapace width without spines",
            10.0..=13.0,
        ),
        (
            "snapping shrimp",
            long(Kind::Snapper),
            "total length",
            3.0..=5.5,
        ),
        (
            "mantis shrimp",
            long(Kind::Mantis),
            "total length",
            3.0..=18.0,
        ),
        // Its head out of the crevice: a 100 cm moray in the game's sea (common
        // 50, max 100), 60 cm in the plain test level.
        ("moray", long(Kind::Moray) * sea, "head length", 10.0..=14.0),
        (
            "moray",
            cm(t.moray.body.1) * sea,
            "depth at the gills",
            100.0 / 23.0..=100.0 / 14.0,
        ),
        (
            "triggerfish",
            long(Kind::Triggerfish),
            "total length",
            30.0..=60.0,
        ),
        (
            "octopus",
            long(Kind::Octopus),
            "mantle and head",
            15.0..=25.0,
        ),
        (
            "alarm jelly",
            long(Kind::AlarmJelly),
            "bell diameter",
            2.0..=17.0,
        ),
        // A carapace of 85-100 cm and the head.
        (
            "loggerhead turtle",
            long(Kind::LoggerheadTurtle),
            "carapace and head",
            90.0..=115.0,
        ),
        // An 80 cm squid (common 50-80 cm total length) without its arms.
        (
            "humboldt squid",
            long(Kind::HumboldtSquid),
            "mantle and head",
            40.0..=80.0,
        ),
        (
            "viperfish",
            long(Kind::Viperfish),
            "standard length",
            20.0..=35.0,
        ),
        (
            "goliath grouper",
            long(Kind::GoliathGrouper),
            "total length",
            150.0..=250.0,
        ),
        (
            "american lobster",
            long(Kind::Lobster),
            "body without claws",
            20.0..=61.0,
        ),
        // A 10 cm hermit (10-12 cm long), its shell and head.
        (
            "hermit crab",
            long(Kind::HermitCrab),
            "shell and head",
            6.0..=10.0,
        ),
        // A juvenile green crab (adults reach about 9 cm across the carapace;
        // not verified), its hurtbox across the carapace.
        (
            "shore crab",
            long(Kind::ShoreCrab),
            "carapace width",
            2.0..=9.0,
        ),
        (
            "bobbit worm",
            long(Kind::BobbitWorm),
            "body width",
            1.0..=2.6,
        ),
        (
            "giant isopod",
            long(Kind::GiantIsopod),
            "body length",
            19.0..=36.0,
        ),
        // A 2 m sixgill (up to 4.8 m: [FLMNH-SG]; the 65 cm low end not verified).
        (
            "bluntnose sixgill",
            long(Kind::Sixgill),
            "total length",
            65.0..=480.0,
        ),
        // Six 50 cm fish (30-63.5 cm), coiled into a knot.
        (
            "hagfish",
            long(Kind::Hagfish),
            "the knot's width",
            15.0..=35.0,
        ),
    ];
    for (animal, size, measure, real) in &rows {
        assert!(
            real.contains(size),
            "{animal} {measure}: {size:.1} cm, real {real:?}"
        );
    }
    // Every kind is in the table but three: the space monkey (a joke code), the
    // fixture (a test dummy) and the amphipods (a cloud of 2 cm animals, drawn
    // to size in src/sizes.rs).
    let mut animals: Vec<&str> = rows.iter().map(|r| r.0).filter(|a| *a != "crab").collect();
    animals.dedup();
    assert_eq!(
        animals.len() + 3,
        crabsim::enemy::KINDS.len(),
        "a new animal needs a row here and in src/sizes.rs"
    );
}

#[test]
fn a_moray_hunts_a_crab_again_after_it_died_and_came_back() {
    // It wakes at home, then walks back past the moray for its things.
    let mut sim = Sim::with_one_crab(Vec2::new(290.0, 0.0));
    sim.spawn_enemy(Kind::Moray, Vec2::new(400.0, 0.0));
    let id = sim.crabs()[0].id;
    sim.crab_mut(id).unwrap().fighter.vitals.health = 20.0;
    let (mut died, mut back, mut struck_after) = (None, None, 0);
    let mut states = Vec::new();
    for tick in 0..4000u64 {
        let walk = if back.is_some() { 1.0 } else { 0.0 };
        sim.set_controls(
            id,
            Controls {
                walk,
                ..Default::default()
            },
        );
        sim.tick();
        let c = &sim.crabs()[0];
        if c.events.died && died.is_none() {
            died = Some(tick);
        }
        if c.events.respawned && back.is_none() {
            back = Some(tick);
        }
        let e = &sim.enemies()[0];
        if states.last() != Some(&e.state) {
            states.push(e.state);
            if back.is_some() && e.state == State::Attack {
                struck_after += 1;
            }
        }
    }
    assert!(
        died.is_some() && back.is_some(),
        "died {died:?}, back {back:?}"
    );
    assert!(struck_after > 0, "never struck again: {states:?}");
}

#[test]
fn a_moray_knocked_out_of_its_crevice_goes_back_and_hunts_again() {
    // A claw's knockback shoved it 30 out of its crevice at 400, then the
    // fight ended (the crab died or left): it must get home and smell again.
    let mut sim = Sim::with_one_crab(Vec2::new(290.0, 0.0));
    sim.spawn_enemy(Kind::Moray, Vec2::new(400.0, 0.0));
    sim.enemies_mut()[0].pos.x = 430.0;
    let mut hurt = false;
    play(&mut sim, &Replay::parse("").unwrap(), 900, |s| {
        hurt |= s.crabs()[0].events.hurt;
    });
    let e = &sim.enemies()[0];
    assert!(hurt, "never noticed the crab: {:?} at {}", e.state, e.pos.x);
}

#[test]
fn a_crab_cannot_just_walk_past_a_mantis() {
    // The owner: "no reason to kill them, just run past". On the plateau,
    // walking past: the mantis turns after it, darts after it and strikes.
    // Only a sprint (costing the reserve) or a fight gets past.
    let mut sim = Sim::with_one_crab(Vec2::new(830.0, 110.0));
    sim.set_feel(crabsim::Feel::Hybrid);
    sim.spawn_enemy(Kind::Mantis, Vec2::new(950.0, 110.0));
    let id = sim.crabs()[0].id;
    let mut hurt = false;
    // Until just before the trench's edge (1300).
    while sim.crabs()[0].mover.pos.x < 1270.0 && !hurt {
        sim.set_controls(
            id,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
        hurt |= sim.crabs()[0].events.hurt;
    }
    assert!(hurt, "walked past unhurt");
}

#[test]
fn a_sprinting_crab_gets_past_a_mantis() {
    // Faster than its dart, at the cost of the reserve.
    let mut sim = Sim::with_one_crab(Vec2::new(830.0, 110.0));
    sim.set_feel(crabsim::Feel::Hybrid);
    sim.spawn_enemy(Kind::Mantis, Vec2::new(950.0, 110.0));
    let id = sim.crabs()[0].id;
    let mut hurt = false;
    let mut tick = 0;
    while sim.crabs()[0].mover.pos.x < 1270.0 && tick < 600 {
        sim.set_controls(
            id,
            Controls {
                walk: 1.0,
                dodge_pressed: tick == 0,
                dodge_held: true,
                ..Default::default()
            },
        );
        sim.tick();
        tick += 1;
        hurt |= sim.crabs()[0].events.hurt;
    }
    assert!(!hurt, "caught while sprinting");
    assert!(sim.crabs()[0].mover.pos.x >= 1270.0, "got past");
}

#[test]
fn eating_a_beaten_creature_mends_the_shell_once() {
    let mut sim = Sim::with_one_crab(Vec2::new(250.0, 0.0));
    sim.spawn_enemy(Kind::Mantis, Vec2::new(400.0, 0.0));
    let id = sim.crabs()[0].id;
    sim.crab_mut(id).unwrap().fighter.vitals.health = 30.0;
    sim.crab_mut(id).unwrap().mover.reserve = 0.2;
    // Beaten (as by a last smash).
    let tuning = sim.tuning.clone();
    let blow = crabsim::Hit {
        damage: 1000.0,
        ..Default::default()
    };
    sim.enemies_mut()[0].pos.x = 420.0; // out of its burrow
    sim.enemies_mut()[0].takes(&blow, 380.0, &tuning);
    assert_eq!(sim.enemies()[0].state, State::Gone);
    let mut fed = 0;
    for _ in 0..240 {
        sim.set_controls(
            id,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
        fed += sim.crabs()[0].events.fed as u32;
    }
    let crab = &sim.crabs()[0];
    assert_eq!(fed, 1, "one body, one meal");
    assert_eq!(crab.fighter.vitals.health, 80.0, "half a shell mended");
    assert!(crab.mover.reserve > 0.99, "reserve filled");
}

#[test]
fn a_living_creature_is_no_meal() {
    let mut sim = Sim::with_one_crab(Vec2::new(250.0, 0.0));
    sim.spawn_enemy(Kind::Fixture, Vec2::new(300.0, 0.0));
    let id = sim.crabs()[0].id;
    let mut fed = false;
    for _ in 0..240 {
        sim.tick();
        fed |= sim.crabs()[id.0 as usize].events.fed;
    }
    assert!(!fed);
}

#[test]
fn a_vacant_habitat_waits_for_its_season_and_the_last_living_crab() {
    let home = Vec2::new(1800.0, 0.0);
    let visitor = crabsim::PlayerId(7);
    for kind in [Kind::Snapper, Kind::Mantis, Kind::Moray] {
        let mut sim = Sim::with_one_crab(Vec2::ZERO);
        sim.add_remote_crab(visitor, Vec2::ZERO);
        let id = sim.spawn_enemy(kind, home);
        let home = sim.enemies()[0].home;
        let tuning = sim.tuning.clone();
        let e = &mut sim.enemies_mut()[0];
        e.pos.x += 80.0; // away from the burrow and hittable
        e.takes(
            &crabsim::Hit {
                damage: 10000.0,
                ..Default::default()
            },
            home.x,
            &tuning,
        );
        assert_eq!(e.state, State::Gone);
        assert!(
            e.state_len >= 3 * 60 * 120,
            "a quick circuit cannot refill the pocket"
        );
        e.state_tick = e.state_len - 1;
        sim.tick();
        assert_eq!(
            sim.enemies()[0].state,
            State::Gone,
            "too early, even unwatched"
        );
        sim.crab_mut(visitor).unwrap().mover.pos = home;
        sim.tick();
        assert_eq!(
            sim.enemies()[0].state,
            State::Gone,
            "a friend is still watching"
        );
        sim.crab_mut(visitor).unwrap().fighter.dead = Some(0);
        sim.tick();
        let replacement = &sim.enemies()[0];
        assert_ne!(replacement.state, State::Gone);
        assert_eq!((replacement.id, replacement.deaths), (id, 1));
        assert_eq!(replacement.home, home);
        assert_eq!(
            replacement.vitals.health,
            kind.tuning(&tuning).vitals.health
        );
    }
}
