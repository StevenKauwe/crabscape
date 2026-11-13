//! The first-hour guide (`economy/guide.rs`): a short, skippable chain that
//! walks a brand-new crab through all thirteen skills. The bot test in
//! `first_hours.rs` follows it on real ticks and measures the minutes; these
//! tests hold the rules: every skill has a step, each step can be done by a
//! level 1 crab with what the world has, a new crab starts on step 1, a step
//! already met is done at once, skipping and resuming work, the state is
//! saved, and a remote crab is never touched.

use bevy_math::Vec2;
use crabsim::economy::guide::{Cond, GUIDE, Guide, Note};
use crabsim::economy::{ECON_RECIPES, Economy, Item, NodeKind, place};
use crabsim::{Kind, Level, PlayerId, Sim, Skill, Tuning};

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

const THIRTEEN: [Skill; 13] = [
    Skill::Climbing,
    Skill::Swimming,
    Skill::Fighting,
    Skill::Foraging,
    Skill::Prying,
    Skill::Excavating,
    Skill::Harvesting,
    Skill::Culturing,
    Skill::Cooking,
    Skill::Shellcraft,
    Skill::Weaving,
    Skill::Building,
    Skill::Survival,
];

fn sim() -> Sim {
    let mut s = Sim::new(Tuning::default(), Level::test_level());
    s.add_crab(P0, Vec2::new(180.0, 0.0));
    s.populate();
    s
}

fn index(s: &Sim) -> usize {
    s.guide_status(P0).expect("a local crab").index
}

fn set_level(s: &mut Sim, skill: Skill, level: u32) {
    if skill.economy_index().is_some() {
        s.crab_mut(P0)
            .expect("crab")
            .econ
            .skills
            .set(skill, crabsim::economy::xp::xp_for_level(level));
    } else {
        // Climbing, Swimming and Fighting: plenty of experience.
        s.earn(P0, skill, 1.0e6);
    }
}

fn step_of(skill: Skill) -> usize {
    GUIDE
        .iter()
        .position(|s| s.skill == skill)
        .expect("a step for the skill")
}

#[test]
fn the_chain_has_a_step_for_every_skill_and_ends_on_the_wants_board() {
    assert!(
        (13..=16 + 2).contains(&GUIDE.len()),
        "{} steps",
        GUIDE.len()
    );
    for skill in THIRTEEN {
        assert!(
            GUIDE.iter().any(|s| s.skill == skill),
            "{} has no guide step",
            skill.name()
        );
    }
    assert_eq!(GUIDE.last().expect("steps").title, "Pick a want");
    assert_eq!(GUIDE.last().expect("steps").cond, Cond::Pinned);
}

#[test]
fn gathers_come_first_then_crafts_then_the_first_piece_then_moving() {
    let pos =
        |pred: &dyn Fn(&Cond) -> bool| GUIDE.iter().position(|s| pred(&s.cond)).expect("a step");
    let last_gather = GUIDE
        .iter()
        .rposition(|s| matches!(s.cond, Cond::Gather(..)))
        .expect("gathers");
    let first_make = pos(&|c| matches!(c, Cond::Make(..)));
    let valve_step = pos(&|c| matches!(c, Cond::Build(_)));
    let first_move = pos(&|c| matches!(c, Cond::Beat(_)));
    assert!(last_gather < first_make, "gathers first");
    assert!(first_make < valve_step, "then the crafts");
    assert!(valve_step < first_move, "then the first home piece");
    assert!(step_of(Skill::Climbing) > valve_step);
    assert!(step_of(Skill::Swimming) > step_of(Skill::Climbing));
    assert!(
        GUIDE.iter().any(|s| s.cond == Cond::Band(1)),
        "a first dive"
    );
}

/// Every step can be done by a level 1 crab with what the world has: the
/// nodes lie in the home pockets, the recipes and pieces ask a level that an
/// earlier step has already reached, the creature stands near home, and the
/// dive has ground to stand on.
#[test]
fn every_step_can_be_done_by_a_level_1_crab_in_the_world_as_it_is() {
    let s = sim();
    // The highest level of each skill the earlier steps are sure to reach.
    let mut sure = [1u32; 13];
    let idx = |skill: Skill| THIRTEEN.iter().position(|&k| k == skill).expect("skill");
    for (i, step) in GUIDE.iter().enumerate() {
        let have = |skill: Skill, sure: &[u32; 13]| sure[idx(skill)];
        match step.cond {
            Cond::Gather(kind, _) => {
                let def = kind.def();
                assert!(def.level <= have(def.skill, &sure), "step {} gate", i + 1);
                assert!(
                    s.placed_nodes()
                        .iter()
                        .any(|n| n.kind == kind && n.pos.x < 600.0),
                    "step {}: no {} in the home pockets",
                    i + 1,
                    def.name
                );
            }
            Cond::Make(word, _) => {
                let r = ECON_RECIPES
                    .iter()
                    .find(|r| r.word == word)
                    .expect("a recipe");
                assert!(r.level <= have(r.skill, &sure), "step {} gate", i + 1);
                // Its inputs are tier 1 items.
                for &(item, _) in r.needs {
                    assert!(item.tier() <= 1, "step {} needs {item:?}", i + 1);
                }
            }
            Cond::Build(piece) => {
                assert!(
                    piece.level() <= have(Skill::Building, &sure),
                    "step {}: {} asks Building {}",
                    i + 1,
                    piece.name(),
                    piece.level()
                );
                for &(item, _) in piece.needs() {
                    assert!(item.tier() <= 1, "step {} needs {item:?}", i + 1);
                }
            }
            Cond::Level(skill, level) => {
                sure[idx(skill)] = sure[idx(skill)].max(level);
            }
            Cond::Beat(kind) => {
                assert!(
                    s.enemies()
                        .iter()
                        .any(|e| e.kind == kind && e.home.x < 700.0),
                    "step {}: no {kind:?} near home",
                    i + 1
                );
            }
            Cond::Band(band) => {
                let g = s
                    .level
                    .ground_below(Vec2::new(1300.0, -821.0))
                    .expect("ground to dive to");
                assert!(place::band_index(&s.level, Vec2::new(1300.0, g + 2.0)) >= band);
            }
            Cond::Pinned => {}
        }
        // A step's own act raises its skill's gate for the later steps.
        if matches!(step.cond, Cond::Make(..) | Cond::Gather(..)) {
            sure[idx(step.skill)] = sure[idx(step.skill)].max(2);
        }
    }
    assert!(s.level.bands().len() >= 2);
}

#[test]
fn rewards_are_small_tier_1_things() {
    for step in GUIDE {
        for &(item, n) in step.reward.items {
            assert!(item.tier() <= 1, "{}: {item:?} is not tier 1", step.title);
            assert!(n <= 4, "{}: {n} of {item:?} is a lot", step.title);
        }
        if let Some((_, xp)) = step.reward.xp {
            // One level 1 gather pays 11.
            assert!(xp <= 11, "{}: {xp} XP", step.title);
        }
        assert!(!step.reward.line().is_empty());
    }
    // Nothing is padded: no step needs a count over a dozen.
    for step in GUIDE {
        if let Cond::Gather(_, n) | Cond::Make(_, n) = step.cond {
            assert!(n <= 12, "{}: {n}", step.title);
        }
    }
}

#[test]
fn a_new_crab_starts_on_step_one() {
    let mut s = sim();
    let st = s.guide_status(P0).expect("status");
    assert_eq!(st.index, 0);
    assert!(!st.skipped);
    assert_eq!(st.step.expect("a step").title, GUIDE[0].title);
    // Ticking with nothing done changes nothing.
    for _ in 0..240 {
        s.tick();
    }
    assert_eq!(index(&s), 0);
}

#[test]
fn a_step_done_pays_its_reward_and_says_so() {
    let mut s = sim();
    let Cond::Gather(kind, n) = GUIDE[0].cond else {
        panic!("step 1 is a gather");
    };
    let item = kind.def().yields.expect("an item");
    for _ in 0..n {
        s.crab_mut(P0)
            .expect("crab")
            .econ
            .guide
            .note(Note::Gathered(item));
    }
    s.tick();
    assert_eq!(s.crab(P0).expect("crab").events.guide_done, Some(0));
    assert_eq!(index(&s), 1);
    for &(reward, k) in GUIDE[0].reward.items {
        assert!(s.crab(P0).expect("crab").stash.count(reward) >= k);
    }
    // The next tick says nothing more.
    s.tick();
    assert_eq!(s.crab(P0).expect("crab").events.guide_done, None);
    // Another kind of gather counts for nothing.
    s.crab_mut(P0)
        .expect("crab")
        .econ
        .guide
        .note(Note::Gathered(Item::Mussel));
    assert_eq!(s.crab(P0).expect("crab").econ.guide.count, 0);
}

#[test]
fn steps_already_met_finish_at_once_and_silently() {
    let mut s = sim();
    // A skilled crab: the three gather skills of the first steps are past 2.
    for skill in GUIDE[..3].iter().map(|s| s.skill) {
        set_level(&mut s, skill, 3);
    }
    s.tick();
    assert_eq!(index(&s), 3, "the first three steps are skipped");
    assert_eq!(
        s.crab(P0).expect("crab").events.guide_done,
        None,
        "a skipped step pays nothing and shows no card"
    );
    // Every later step stays ahead of a crab that has met none of it.
    for _ in 0..120 {
        s.tick();
    }
    assert_eq!(index(&s), 3);
}

#[test]
fn a_crab_with_everything_met_finishes_the_chain_at_once() {
    let mut s = sim();
    for skill in THIRTEEN {
        set_level(&mut s, skill, 10);
    }
    s.tick();
    assert!(s.crab(P0).expect("crab").econ.guide.finished());
    assert!(s.guide_status(P0).expect("status").step.is_none());
    assert_eq!(s.crab(P0).expect("crab").events.guide_done, None);
}

#[test]
fn the_last_step_waits_for_a_pinned_want() {
    let mut s = sim();
    for skill in THIRTEEN {
        set_level(&mut s, skill, 5);
    }
    s.tick();
    assert_eq!(index(&s), GUIDE.len() - 1, "only the last step is left");
    for _ in 0..60 {
        s.tick();
    }
    assert_eq!(index(&s), GUIDE.len() - 1);
    assert!(s.pin_want(P0, "kelp-cord"));
    s.tick();
    assert_eq!(
        s.crab(P0).expect("crab").events.guide_done,
        Some((GUIDE.len() - 1) as u8)
    );
    assert!(s.guide_status(P0).expect("status").step.is_none());
}

#[test]
fn skip_then_on_resumes_at_the_first_step_not_done() {
    let mut s = sim();
    assert!(s.guide_skip(P0));
    let at = index(&s);
    assert!(s.guide_status(P0).expect("status").skipped);
    // While skipped nothing moves, whatever the crab does.
    for skill in GUIDE[..2].iter().map(|s| s.skill) {
        set_level(&mut s, skill, 3);
    }
    for _ in 0..120 {
        s.tick();
    }
    assert_eq!(index(&s), at);
    assert_eq!(s.crab(P0).expect("crab").events.guide_done, None);
    // On again: the steps it has met go by at once, then it waits.
    assert!(s.guide_on(P0));
    s.tick();
    let st = s.guide_status(P0).expect("status");
    assert!(!st.skipped);
    assert_eq!(st.index, 2, "the first step not yet done");
    // Skip again from there, and on again with nothing new: same place.
    assert!(s.guide_skip(P0));
    assert!(s.guide_on(P0));
    s.tick();
    assert_eq!(index(&s), 2);
}

#[test]
fn the_state_round_trips_through_a_save_and_old_economy_data_loads() {
    let e = Economy {
        guide: Guide {
            step: 4,
            skipped: true,
            count: 3,
            vetted: true,
        },
        ..Economy::default()
    };
    let json = serde_json::to_string(&e).expect("serialises");
    let back: Economy = serde_json::from_str(&json).expect("loads");
    assert_eq!(back.guide, e.guide);
    // An economy saved before the guide existed has no `guide` at all.
    let mut value: serde_json::Value = serde_json::from_str(&json).expect("json");
    value.as_object_mut().expect("an object").remove("guide");
    let old: Economy = serde_json::from_value(value).expect("an old save loads");
    assert_eq!(old.guide, Guide::default());
    assert_eq!(old.guide.step, 0);
    assert!(!old.guide.skipped);
    // And a crab loaded from such a save starts on step 1.
    let mut s = sim();
    s.crab_mut(P0).expect("crab").econ = old;
    s.tick();
    assert_eq!(index(&s), 0);
}

#[test]
fn a_remote_crabs_guide_never_changes() {
    let mut s = sim();
    s.add_remote_crab(P1, Vec2::new(200.0, 0.0));
    let before = s.crab(P1).expect("remote").econ.guide.clone();
    set_level_of(&mut s, P1);
    for _ in 0..240 {
        s.tick();
    }
    assert_eq!(s.crab(P1).expect("remote").econ.guide, before);
    assert!(s.guide_status(P1).is_none());
    assert!(!s.guide_skip(P1));
    assert!(!s.guide_on(P1));
    assert_eq!(s.crab(P1).expect("remote").events.guide_done, None);
    assert_eq!(s.crab(P1).expect("remote").econ.guide, before);
}

fn set_level_of(s: &mut Sim, id: PlayerId) {
    let c = s.crab_mut(id).expect("crab");
    c.econ.skills.set(Skill::Prying, 5_000);
    c.econ.guide.note(Note::Beat(Kind::Snapper));
}

#[test]
fn a_node_kind_step_counts_only_its_own_nodes() {
    // Guard against a step whose kind yields nothing.
    for step in GUIDE {
        if let Cond::Gather(kind, _) = step.cond {
            assert!(kind.def().yields.is_some());
            assert!(matches!(
                kind,
                NodeKind::MusselClump
                    | NodeKind::KelpStand
                    | NodeKind::GritHeap
                    | NodeKind::SnailRocks
                    | NodeKind::ShoreGlow
            ));
        }
    }
}

#[test]
fn the_replay_words_skip_and_on_round_trip() {
    let mut s = sim();
    let text = "@0 guide=skip\n@5 guide=on\n";
    let replay = crabsim::Replay::parse(text).expect("parses");
    assert_eq!(replay.guides, vec![(0, false), (5, true)]);
    assert!(replay.to_text().contains("guide=skip"));
    assert!(replay.to_text().contains("guide=on"));
    let _ = &mut s;
}

#[test]
fn the_guide_points_at_the_nearest_seen_node_of_its_step() {
    let mut s = sim();
    let Cond::Gather(kind, _) = GUIDE[0].cond else {
        panic!("step 1 is a gather");
    };
    for _ in 0..240 {
        s.tick();
    }
    let at = s
        .guide_target(P0, 1200.0)
        .expect("the first step's node is in sight of home");
    let placed = s.placed_nodes();
    let node = placed.iter().find(|n| n.pos == at).expect("a placed node");
    assert_eq!(node.kind, kind);
    // Nothing once the guide is skipped.
    assert!(s.guide_skip(P0));
    assert_eq!(s.guide_target(P0, 1200.0), None);
}
