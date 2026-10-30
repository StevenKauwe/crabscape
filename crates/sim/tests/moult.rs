//! Promises about moulting (`docs/design/world.md`, Moulting and feats): a
//! skill at its milestone level makes a feat ready, but the body only
//! changes at a moult. Well fed and tucked in at home, the crab moults to
//! grow one ready feat; it is soft for a while (half the shell, weaker claws,
//! no eating), hardening faster tucked in; then the feat is its own, in a
//! bigger shell. The moult goes through the sim like a code, so replays
//! carry it, and it is saved.

use bevy_math::Vec2;
use crabsim::moult::{self, Feat};
use crabsim::skills::xp_for;
use crabsim::{Controls, Hit, PlayerId, Progress, Replay, Shelter, Sim, Skill, play};

const P0: PlayerId = PlayerId(0);

/// Run `n` ticks with no controls.
fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.tick();
    }
}

/// Put crab 0 at `level` in `skill`.
fn set_level(sim: &mut Sim, skill: Skill, level: u32) {
    let step = skill.step(&sim.tuning);
    let m = &mut sim.crab_mut(P0).unwrap().mover;
    let xp = xp_for(level, step);
    match skill {
        Skill::Climbing => m.climb_xp = xp,
        Skill::Swimming => m.swim_xp = xp,
        Skill::Fighting => m.fight_xp = xp,
    }
}

/// A crab tucked right into its home shell, well fed, with Climbing at the
/// milestone: ready to moult.
fn ready_at_home() -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(95.0, 0.0));
    let den = sim.level.home_den;
    let fed = sim.tuning.food.well_fed_ticks;
    let milestone = sim.tuning.moult.feat_level;
    {
        let m = &mut sim.crab_mut(P0).unwrap().mover;
        m.pos = den;
        m.shelter = Shelter::In;
        m.tucked = 1.0;
        m.well_fed = fed;
    }
    set_level(&mut sim, Skill::Climbing, milestone);
    sim.tick();
    assert!(sim.crab(P0).unwrap().mover.tucked_in());
    sim
}

fn mover(sim: &Sim) -> &crabsim::Mover {
    &sim.crab(P0).unwrap().mover
}

#[test]
fn each_skill_has_one_feat_and_each_feat_is_a_new_move() {
    assert_eq!(moult::FEATS.len(), Skill::ALL.len());
    for skill in Skill::ALL {
        let feat = Feat::of(skill);
        assert_eq!(feat.skill(), skill);
        assert!(!feat.name().is_empty() && !feat.does().is_empty());
    }
    assert_eq!(Feat::of(Skill::Climbing).name(), "gripping dactyls");
    assert_eq!(Feat::of(Skill::Swimming).name(), "broad paddles");
    assert_eq!(Feat::of(Skill::Fighting).name(), "heavy crusher");
}

#[test]
fn a_feat_is_ready_at_the_milestone_level_and_the_note_says_so() {
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    let t = sim.tuning.clone();
    let milestone = t.moult.feat_level;
    set_level(&mut sim, Skill::Climbing, milestone - 1);
    assert!(mover(&sim).ready_feats(&t).is_empty());
    set_level(&mut sim, Skill::Climbing, milestone);
    assert_eq!(mover(&sim).ready_feats(&t), vec![Feat::GrippingDactyls]);
    assert_eq!(
        moult::ready_note(Skill::Climbing, 5),
        "Climbing level 5! A feat is ready: moult to grow it."
    );
    // Reaching it by earning experience says so with the level up.
    set_level(&mut sim, Skill::Swimming, milestone);
    let step = Skill::Swimming.step(&t);
    sim.crab_mut(P0).unwrap().mover.swim_xp = xp_for(milestone, step) - 0.5;
    let up = sim
        .crab_mut(P0)
        .unwrap()
        .mover
        .earn(Skill::Swimming, 1.0, &t);
    assert_eq!(up, Some(Skill::Swimming));
    assert!(mover(&sim).feat_ready(Skill::Swimming, &t));
}

#[test]
fn moulting_needs_a_ready_feat_a_meal_and_the_home() {
    // Ready, fed and home: it moults.
    let mut sim = ready_at_home();
    assert!(sim.can_moult(P0));
    assert!(sim.moult(P0, Skill::Climbing));
    assert!(sim.crab(P0).unwrap().events.moulted || mover(&sim).is_soft());

    // A feat that isn't ready: no.
    let mut sim = ready_at_home();
    assert!(!sim.moult(P0, Skill::Fighting), "Fighting is level 1");

    // Hungry: no.
    let mut sim = ready_at_home();
    sim.crab_mut(P0).unwrap().mover.well_fed = 0;
    assert!(!sim.can_moult(P0));
    assert!(!sim.moult(P0, Skill::Climbing));

    // Out in the open: no.
    let mut sim = ready_at_home();
    {
        let m = &mut sim.crab_mut(P0).unwrap().mover;
        m.shelter = Shelter::Out;
        m.tucked = 0.0;
        m.pos = Vec2::new(300.0, 0.0);
    }
    assert!(!sim.moult(P0, Skill::Climbing));

    // Tucked into a crevice out in the level, not home: no.
    let mut sim = ready_at_home();
    let den = sim.level.crevices[0].den;
    sim.crab_mut(P0).unwrap().mover.pos = den;
    assert!(!sim.moult(P0, Skill::Climbing));
}

#[test]
fn a_moult_uses_the_meal_and_leaves_the_crab_soft() {
    let mut sim = ready_at_home();
    let t = sim.tuning.clone();
    let before_shell = mover(&sim).max_shell(&t);
    let before_claws = mover(&sim).claw_strength(&t);
    assert!(sim.moult(P0, Skill::Climbing));
    sim.tick();
    let m = mover(&sim);
    assert!(m.is_soft());
    assert_eq!(m.well_fed, 0, "the moult used up the meal");
    assert_eq!(m.growing, Some(Feat::GrippingDactyls));
    assert!(!m.has(Feat::GrippingDactyls), "not until it hardens");
    // Fed, the shell held more; soft, it holds half the plain shell of its
    // new size (it grew at the moult: `tests/growth.rs`), and its bigger
    // claws hit weaker.
    let (plain, grown) = (t.crab.health, m.grown(&t));
    assert!(before_shell > plain && grown > 1.0);
    assert_eq!(m.max_shell(&t), plain * grown * t.moult.soft_shell);
    assert!(sim.crab(P0).unwrap().fighter.vitals.health <= m.max_shell(&t));
    assert_eq!(
        m.claw_strength(&t),
        before_claws * grown * t.moult.soft_claws
    );
    // Only one moult at a time.
    set_level(&mut sim, Skill::Swimming, t.moult.feat_level);
    sim.crab_mut(P0).unwrap().mover.well_fed = t.food.well_fed_ticks;
    assert!(!sim.moult(P0, Skill::Swimming), "still soft");
}

#[test]
fn a_soft_crab_eats_nothing() {
    let mut sim = ready_at_home();
    assert!(sim.moult(P0, Skill::Climbing));
    // A clam in the larder, resting at home: not eaten while soft.
    sim.crab_mut(P0).unwrap().larder = 2;
    idle(&mut sim, 240);
    assert_eq!(sim.crab(P0).unwrap().larder, 2);
    assert_eq!(mover(&sim).well_fed, 0);
    // An egg it touches stays where it is.
    let egg = sim.level.eggs[0];
    {
        let m = &mut sim.crab_mut(P0).unwrap().mover;
        m.shelter = Shelter::Out;
        m.tucked = 0.0;
        m.pos = egg - Vec2::Y * 20.0;
        m.grounded = false;
    }
    let mut too_soft = false;
    for _ in 0..10 {
        sim.tick();
        too_soft |= sim.crab(P0).unwrap().events.too_soft;
    }
    assert!(!sim.crab(P0).unwrap().eaten[0]);
    assert!(too_soft, "it says why");
}

#[test]
fn the_shell_hardens_in_time_and_faster_tucked_in() {
    let t = crabsim::Tuning::default();
    let soft = t.moult.soft_ticks;
    // Out in the open, it takes the whole time.
    let mut out = ready_at_home();
    assert!(out.moult(P0, Skill::Climbing));
    {
        let m = &mut out.crab_mut(P0).unwrap().mover;
        m.shelter = Shelter::Out;
        m.tucked = 0.0;
        m.pos = Vec2::new(300.0, 0.0);
    }
    idle(&mut out, soft - 2);
    assert!(mover(&out).is_soft());
    let mut hardened = None;
    for _ in 0..4 {
        out.tick();
        hardened = hardened.or(out.crab(P0).unwrap().events.hardened);
    }
    assert_eq!(hardened, Some(Feat::GrippingDactyls));
    assert!(!mover(&out).is_soft());
    // Tucked in at home, much sooner.
    let mut home = ready_at_home();
    assert!(home.moult(P0, Skill::Climbing));
    let mut ticks = 0;
    while mover(&home).is_soft() {
        home.tick();
        ticks += 1;
        assert!(ticks < soft, "hardened no sooner tucked in");
    }
    assert!(ticks <= soft / t.moult.harden_tucked + 1, "{ticks}");
}

#[test]
fn hardening_grows_the_feat_and_a_bigger_shell() {
    let mut sim = ready_at_home();
    let t = sim.tuning.clone();
    let plain = t.crab.health;
    assert!(sim.moult(P0, Skill::Climbing));
    let mut hardened = None;
    while mover(&sim).is_soft() {
        sim.tick();
        hardened = hardened.or(sim.crab(P0).unwrap().events.hardened);
    }
    assert_eq!(hardened, Some(Feat::GrippingDactyls));
    let m = mover(&sim);
    assert!(m.has(Feat::GrippingDactyls));
    assert_eq!(m.growing, None);
    assert_eq!(m.moults, 1);
    // The shell grew with the crab (`tests/growth.rs` has the size).
    assert!(m.grown(&t) > 1.0);
    assert_eq!(m.max_shell(&t), plain * m.grown(&t));
    assert!(!m.feat_ready(Skill::Climbing, &t), "grown, not ready again");
    assert_eq!(
        moult::hardened_note(Feat::GrippingDactyls),
        "Your new shell has hardened: gripping dactyls!"
    );
    // Rested at home the whole time: the bigger shell is whole.
    assert_eq!(sim.crab(P0).unwrap().fighter.vitals.health, m.max_shell(&t));
}

#[test]
fn one_feat_per_moult() {
    let mut sim = ready_at_home();
    let t = sim.tuning.clone();
    set_level(&mut sim, Skill::Swimming, t.moult.feat_level);
    assert_eq!(mover(&sim).ready_feats(&t).len(), 2);
    assert!(sim.moult(P0, Skill::Swimming));
    while mover(&sim).is_soft() {
        sim.tick();
    }
    let m = mover(&sim);
    assert!(m.has(Feat::BroadPaddles));
    assert!(!m.has(Feat::GrippingDactyls));
    assert_eq!(m.ready_feats(&t), vec![Feat::GrippingDactyls]);
}

#[test]
fn soft_claws_hit_weaker() {
    let mut sim = ready_at_home();
    let t = sim.tuning.clone();
    let hard = mover(&sim).claw_strength(&t);
    assert!(sim.moult(P0, Skill::Climbing));
    assert!(mover(&sim).claw_strength(&t) < hard);
}

#[test]
fn dying_soft_wakes_still_soft_and_still_growing() {
    let mut sim = ready_at_home();
    assert!(sim.moult(P0, Skill::Climbing));
    {
        let m = &mut sim.crab_mut(P0).unwrap().mover;
        m.shelter = Shelter::Out;
        m.tucked = 0.0;
        m.pos = Vec2::new(300.0, 0.0);
    }
    sim.hit_crab(
        P0,
        &Hit {
            damage: 1000.0,
            ..Hit::default()
        },
    );
    let mut woke = false;
    for _ in 0..600 {
        sim.tick();
        woke |= sim.crab(P0).unwrap().events.respawned;
        if woke {
            break;
        }
    }
    assert!(woke);
    let m = mover(&sim);
    assert!(m.is_soft());
    assert_eq!(m.growing, Some(Feat::GrippingDactyls));
}

#[test]
fn a_moult_goes_into_the_replay_and_plays_back_the_same() {
    // Fed and ready out by the home shell; recorded walking in and moulting.
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let t = sim.tuning.clone();
    sim.crab_mut(P0).unwrap().mover.well_fed = t.food.well_fed_ticks;
    set_level(&mut sim, Skill::Climbing, t.moult.feat_level);
    // As a reload leaves it: whole.
    let whole = mover(&sim).max_shell(&t);
    sim.crab_mut(P0).unwrap().fighter.vitals.health = whole;
    let start_progress = sim.progress(P0).unwrap();
    sim.start_recording();
    let left = Controls {
        walk: -1.0,
        ..Controls::default()
    };
    while !mover(&sim).tucked_in() {
        sim.set_controls(P0, left);
        sim.tick();
        assert!(sim.tick_count() < 1200, "got home");
    }
    sim.set_controls(P0, Controls::default());
    let at = sim.tick_count();
    assert!(sim.moult(P0, Skill::Climbing));
    idle(&mut sim, 200);
    let replay = sim.recording().unwrap();
    let text = replay.to_text();
    assert!(text.contains("moult=climbing"), "{text}");
    let replay = Replay::parse(&text).unwrap();
    assert_eq!(replay.moults, vec![(at, Skill::Climbing)]);
    assert_eq!(replay.progress, Some(start_progress));

    // Played back from the same start, it ends in the same state.
    let mut again = Sim::with_one_crab(replay.start);
    play(&mut again, &replay, replay.length(), |_| {});
    assert!(mover(&again).is_soft());
    assert_eq!(replay.hash, Some(again.state_hash()));
}

#[test]
fn a_friends_crab_is_never_moulted_here() {
    let mut sim = ready_at_home();
    let friend = PlayerId(7);
    sim.add_remote_crab(friend, sim.level.home_den);
    assert!(!sim.moult(friend, Skill::Climbing));
    assert!(!sim.can_moult(friend));
}

#[test]
fn a_moult_and_the_feats_are_saved() {
    let mut sim = ready_at_home();
    let t = sim.tuning.clone();
    set_level(&mut sim, Skill::Swimming, t.moult.feat_level);
    assert!(sim.moult(P0, Skill::Swimming));
    while mover(&sim).is_soft() {
        sim.tick();
    }
    sim.crab_mut(P0).unwrap().mover.well_fed = t.food.well_fed_ticks;
    assert!(sim.moult(P0, Skill::Climbing));
    idle(&mut sim, 30);
    let saved = sim.progress(P0).unwrap();
    let text = serde_json::to_string(&saved).unwrap();
    let saved: Progress = serde_json::from_str(&text).unwrap();
    let mut fresh = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    fresh.restore_progress(P0, &saved);
    let m = mover(&fresh);
    assert_eq!(m.feats, vec![Feat::BroadPaddles]);
    assert_eq!(m.growing, Some(Feat::GrippingDactyls));
    assert_eq!(m.moults, 1);
    assert_eq!(m.soft, mover(&sim).soft);
    assert_eq!(fresh.progress(P0).unwrap(), saved);
    // A save from before moulting loads with none of it.
    let mut json = serde_json::to_value(&saved).unwrap();
    let fields = json.as_object_mut().unwrap();
    for f in ["feats", "moults", "soft", "growing"] {
        assert!(fields.remove(f).is_some(), "{f}");
    }
    let old: Progress = serde_json::from_value(json).expect("an old save loads");
    assert!(old.feats.is_empty() && old.growing.is_none() && old.soft == 0);
}

#[test]
fn walking_out_soft_is_allowed() {
    // Going out early is the player's risk, not a rule.
    let mut sim = ready_at_home();
    assert!(sim.moult(P0, Skill::Climbing));
    let mut out = false;
    for _ in 0..240 {
        sim.set_controls(
            P0,
            Controls {
                walk: 1.0,
                ..Controls::default()
            },
        );
        sim.tick();
        out |= sim.crab(P0).unwrap().mover.shelter == Shelter::Out;
    }
    assert!(out);
    assert!(mover(&sim).is_soft());
}
