//! Next goals (`economy/goals.rs`): what pulls a crab on once the first-hour
//! guide is done. These tests hold the rules: nothing while the guide runs,
//! up to three goals after, a goal met pays a tenth of a level and the next
//! comes at once, the pinned want is a goal, the state is saved, and a
//! remote crab is never touched. `first_hours.rs` measures how long goals
//! stay open in three hours of mixed play.

use bevy_math::Vec2;
use crabsim::economy::goals::{Aim, Goals, OPEN, REFILL_TICKS, bonus_xp};
use crabsim::economy::{Economy, Item, xp};
use crabsim::{Controls, PlayerId, Sim, Skill, Stash};

const P0: PlayerId = PlayerId(0);

fn ticks(sim: &mut Sim, n: u64) {
    for _ in 0..n {
        sim.tick();
    }
}

fn open(sim: &Sim) -> Vec<Aim> {
    sim.crab(P0)
        .expect("crab")
        .econ
        .goals
        .open
        .iter()
        .map(|g| g.aim.clone())
        .collect()
}

/// A crab tucked in at home, the guide skipped.
fn at_home() -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    for _ in 0..1200 {
        sim.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        sim.tick();
        if sim.crab(P0).expect("crab").mover.tucked_in() {
            break;
        }
    }
    sim.set_controls(P0, Controls::default());
    assert!(sim.crab(P0).expect("crab").mover.tucked_in(), "got home");
    assert!(sim.guide_skip(P0));
    sim
}

#[test]
fn no_goals_while_the_guide_runs_then_three() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    ticks(&mut sim, REFILL_TICKS * 3);
    assert!(open(&sim).is_empty(), "the guide is the goal for now");
    assert!(sim.goal_status(P0).is_empty());
    assert!(sim.guide_skip(P0));
    ticks(&mut sim, REFILL_TICKS);
    let goals = open(&sim);
    assert_eq!(goals.len(), OPEN, "{goals:?}");
    // One goal per skill and per recipe.
    let mut skills: Vec<Skill> = goals
        .iter()
        .filter_map(|a| match a {
            Aim::Level { skill, .. } => Some(*skill),
            Aim::Make { .. } => None,
        })
        .collect();
    let n = skills.len();
    skills.sort_by_key(|s| s.name());
    skills.dedup();
    assert_eq!(skills.len(), n, "{goals:?}");
    // Every goal says what it opens or what is short, and what it pays.
    for g in sim.goal_status(P0) {
        assert!(!g.title.is_empty() && !g.line.is_empty(), "{g:?}");
        assert!(g.reward.starts_with("Pays "), "{g:?}");
    }
    assert!(sim.goal_line(P0).is_some());
}

#[test]
fn a_level_goal_pays_a_tenth_of_a_level_and_the_next_comes_at_once() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    // Prying nearly at level 2: the nearest skill is offered.
    sim.crab_mut(P0)
        .expect("crab")
        .econ
        .skills
        .set(Skill::Prying, xp::TABLE[2] - 1);
    assert!(sim.guide_skip(P0));
    ticks(&mut sim, REFILL_TICKS);
    let (skill, level) = open(&sim)
        .into_iter()
        .find_map(|a| match a {
            Aim::Level { skill, level } if skill.economy_index().is_some() => Some((skill, level)),
            _ => None,
        })
        .expect("a level goal in an economy skill");
    assert_eq!(
        (skill, level),
        (Skill::Prying, 2),
        "the nearest level first"
    );
    let at = xp::xp_for_level(level);
    sim.crab_mut(P0).expect("crab").econ.skills.set(skill, at);
    sim.tick();
    let crab = sim.crab(P0).expect("crab");
    let done = crab.events.goal_done.expect("the goal says it is done");
    assert_eq!(done.skill, skill);
    assert_eq!(done.level, Some(level));
    assert_eq!(done.xp, bonus_xp(level));
    assert_eq!(crab.econ.skills.get(skill), at + bonus_xp(level));
    assert_eq!(crab.econ.goals.done, 1);
    assert!(
        bonus_xp(level) * 10 <= xp::level_step(level),
        "a tenth of a level at most"
    );
    ticks(&mut sim, REFILL_TICKS);
    assert_eq!(open(&sim).len(), OPEN, "the next goal is offered");
}

#[test]
fn the_pinned_want_is_a_goal_made_it_pays_unpinned_it_leaves() {
    let mut sim = at_home();
    assert!(sim.pin_want(P0, "kelp-cord"));
    ticks(&mut sim, REFILL_TICKS);
    let pinned = Aim::Make {
        word: "kelp-cord".into(),
    };
    assert!(open(&sim).contains(&pinned), "{:?}", open(&sim));
    // Unpinning it, unmade, takes it off the board.
    sim.unpin(P0);
    sim.tick();
    assert!(!open(&sim).contains(&pinned));
    // Pinned again and made: it pays.
    assert!(sim.pin_want(P0, "kelp-cord"));
    ticks(&mut sim, REFILL_TICKS);
    assert!(open(&sim).contains(&pinned));
    sim.crab_mut(P0).expect("crab").stash = Stash::of(&[(Item::Kelp, 10)]);
    let r = crabsim::economy::EconRecipe::parse("kelp-cord").expect("kelp cord");
    sim.make_econ(P0, r).expect("made");
    sim.tick();
    let done = sim
        .crab(P0)
        .expect("crab")
        .events
        .goal_done
        .expect("made it");
    assert_eq!(done.skill, Skill::Weaving);
    assert_eq!(done.title(), "Made kelp cord");
    assert!(!open(&sim).contains(&pinned));
}

#[test]
fn a_goal_one_thing_away_names_what_is_short() {
    let mut sim = at_home();
    // Kelp cord wants kelp: with none, it is one kind away.
    ticks(&mut sim, REFILL_TICKS);
    let status = sim.goal_status(P0);
    if let Some(g) = status.iter().find(|g| g.title.starts_with("Make ")) {
        assert!(g.line.starts_with("Still short of ") || g.ready, "{g:?}");
        assert!(g.tag.contains('/') || g.ready, "{g:?}");
    }
}

#[test]
fn the_state_round_trips_through_a_save_and_old_economy_data_loads() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    assert!(sim.guide_skip(P0));
    ticks(&mut sim, REFILL_TICKS);
    let econ = sim.crab(P0).expect("crab").econ.clone();
    let text = serde_json::to_string(&econ).expect("save");
    let back: Economy = serde_json::from_str(&text).expect("load");
    assert_eq!(back.goals, econ.goals);
    assert_eq!(back.goals.open.len(), OPEN);
    // Economy data from before goals loads with none.
    let mut old: serde_json::Value = serde_json::from_str(&text).expect("json");
    old.as_object_mut().expect("object").remove("goals");
    let back: Economy = serde_json::from_value(old).expect("old data loads");
    assert_eq!(back.goals, Goals::default());
}

#[test]
fn a_remote_crabs_goals_never_change() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    sim.add_remote_crab(PlayerId(1), Vec2::new(300.0, 0.0));
    if let Some(c) = sim.crab_mut(PlayerId(1)) {
        c.econ.guide.skipped = true;
    }
    ticks(&mut sim, REFILL_TICKS * 2);
    let remote = sim.crab(PlayerId(1)).expect("remote crab");
    assert!(remote.econ.goals.open.is_empty());
    assert!(sim.goal_status(PlayerId(1)).is_empty());
}
