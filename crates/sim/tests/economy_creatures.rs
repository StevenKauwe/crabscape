//! Promises about creature drops in the economy: every drop name is an item,
//! eating a beaten creature's body puts its drops on the shell (room
//! permitting), Fighting experience scales with the creature's health, and
//! each drop recipe makes what it says.

use bevy_math::Vec2;
use crabsim::craft::CantMakeEcon;
use crabsim::creatures;
use crabsim::economy::items::from_drop;
use crabsim::economy::{
    Buff, ECON_RECIPES, Economy, Effect, Food, Gear, Item, Output, Station, win_xp, xp,
};
use crabsim::enemy::State;
use crabsim::physics::Carry;
use crabsim::{Controls, Kind, PlayerId, Sim, Skill, Stash};

const P0: PlayerId = PlayerId(0);

const TEN: [Kind; 10] = [
    Kind::LoggerheadTurtle,
    Kind::HumboldtSquid,
    Kind::Viperfish,
    Kind::GoliathGrouper,
    Kind::Lobster,
    Kind::HermitCrab,
    Kind::BobbitWorm,
    Kind::GiantIsopod,
    Kind::Hagfish,
    Kind::AmphipodSwarm,
];

#[test]
fn every_drop_name_of_every_creature_is_an_item() {
    let mut seen = 0;
    for kind in TEN {
        for name in creatures::drops(kind) {
            assert!(
                from_drop(name).is_some(),
                "{kind:?} drops {name:?}: no item"
            );
            seen += 1;
        }
    }
    assert_eq!(seen, 10, "ten drops in all");
    assert_eq!(from_drop("not a drop"), None);
}

/// A crab on the floor with a creature of `kind` lying beaten on top of it.
fn beaten(kind: Kind) -> Sim {
    let at = Vec2::new(1000.0, 110.0);
    let mut sim = Sim::with_one_crab(at);
    let id = sim.spawn_enemy(kind, at);
    let e = sim
        .enemies_mut()
        .iter_mut()
        .find(|e| e.id == id)
        .expect("spawned");
    e.state = State::Gone;
    e.deaths = 1;
    e.pos = at;
    sim
}

fn shell(sim: &Sim) -> Vec<Carry> {
    sim.crab(P0).unwrap().mover.clinging.clone()
}

#[test]
fn eating_a_squid_body_gives_its_beak() {
    let mut sim = beaten(Kind::HumboldtSquid);
    for _ in 0..20 {
        sim.tick();
    }
    assert!(
        shell(&sim).contains(&Carry::Item(Item::SquidBeak)),
        "{:?}",
        shell(&sim)
    );
    // Once: the body is gone for the next tick.
    for _ in 0..20 {
        sim.tick();
    }
    let beaks = shell(&sim)
        .iter()
        .filter(|c| **c == Carry::Item(Item::SquidBeak))
        .count();
    assert_eq!(beaks, 1);
}

#[test]
fn a_full_shell_leaves_the_drops_behind_and_says_so() {
    let mut sim = beaten(Kind::HumboldtSquid);
    let slots = sim.crab(P0).unwrap().mover.carries(&sim.tuning);
    sim.crab_mut(P0).unwrap().mover.clinging = vec![Carry::Grit; slots];
    let mut full = false;
    for _ in 0..20 {
        sim.tick();
        full |= sim.crab(P0).unwrap().events.shell_full;
    }
    assert!(full, "the shell says it is full");
    assert!(!shell(&sim).contains(&Carry::Item(Item::SquidBeak)));
}

#[test]
fn two_drops_come_off_one_body() {
    let mut sim = beaten(Kind::Viperfish);
    for _ in 0..20 {
        sim.tick();
    }
    let s = shell(&sim);
    assert!(s.contains(&Carry::Item(Item::ViperfishFang)), "{s:?}");
    assert!(s.contains(&Carry::Item(Item::Photophore)), "{s:?}");
}

#[test]
fn an_amphipod_swarm_scooped_up_gives_amphipods() {
    let mut sim = beaten(Kind::AmphipodSwarm);
    for _ in 0..20 {
        sim.tick();
    }
    assert!(shell(&sim).contains(&Carry::Item(Item::Amphipods)));
}

#[test]
fn fighting_experience_scales_with_the_creatures_health() {
    let t = Sim::with_one_crab(Vec2::ZERO).tuning.clone();
    // Half its health, at least 15.
    assert_eq!(win_xp(Kind::HumboldtSquid, &t), 70.0);
    assert_eq!(win_xp(Kind::Lobster, &t), 60.0);
    assert_eq!(win_xp(Kind::Viperfish, &t), 17.5);
    assert_eq!(win_xp(Kind::BobbitWorm, &t), 20.0);
    assert!(win_xp(Kind::Hagfish, &t) > win_xp(Kind::Viperfish, &t));
    assert!(win_xp(Kind::HumboldtSquid, &t) > win_xp(Kind::HermitCrab, &t));
    // The older creatures keep the flat number.
    assert_eq!(win_xp(Kind::Fixture, &t), t.fight_xp_per_win);
}

#[test]
fn beating_a_creature_pays_its_health_based_win() {
    let mut sim = Sim::with_one_crab(Vec2::new(1000.0, 110.0));
    sim.spawn_enemy(Kind::Viperfish, Vec2::new(1060.0, 110.0));
    for _ in 0..30 {
        sim.tick();
    }
    let before = sim.crab(P0).unwrap().mover.fight_xp;
    {
        let e = sim
            .enemies_mut()
            .iter_mut()
            .find(|e| e.kind == Kind::Viperfish)
            .unwrap();
        e.vitals.health = 1.0;
        e.state = State::Stagger;
        e.state_tick = 0;
        e.state_len = 300;
    }
    sim.set_controls(
        P0,
        Controls {
            smash_pressed: true,
            ..Controls::default()
        },
    );
    sim.tick();
    for _ in 0..60 {
        sim.set_controls(P0, Controls::default());
        sim.tick();
    }
    let gained = sim.crab(P0).unwrap().mover.fight_xp - before;
    let dead = sim
        .enemies()
        .iter()
        .any(|e| e.kind == Kind::Viperfish && e.deaths == 1);
    assert!(dead, "the viperfish was beaten");
    assert!((17.0..22.0).contains(&gained), "gained {gained}");
}

// Recipes ------------------------------------------------------------------

fn at_home_with(items: &[(Item, u32)]) -> Sim {
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
        if sim.crab(P0).unwrap().mover.tucked_in() {
            break;
        }
    }
    sim.set_controls(P0, Controls::default());
    assert!(sim.crab(P0).unwrap().mover.tucked_in(), "got home");
    sim.crab_mut(P0).unwrap().stash = Stash::of(items);
    sim
}

fn skilled(sim: &mut Sim, skill: Skill, level: u32) {
    sim.crab_mut(P0)
        .unwrap()
        .econ
        .skills
        .set(skill, xp::TABLE[level as usize]);
}

fn build(sim: &mut Sim, st: Station) {
    sim.crab_mut(P0).unwrap().econ.stations.insert(st);
}

fn econ(sim: &Sim) -> &Economy {
    &sim.crab(P0).unwrap().econ
}

/// Make recipe `word` from exactly what it needs, at its level and bench.
fn make(word: &str) -> Sim {
    let r = ECON_RECIPES.iter().find(|r| r.word == word).expect("row");
    let mut sim = at_home_with(r.needs);
    skilled(&mut sim, r.skill, r.level);
    if let crabsim::economy::recipes::Bench::At(st) = r.at {
        build(&mut sim, st);
    }
    // One short of the level is refused.
    skilled(&mut sim, r.skill, r.level - 1);
    assert_eq!(
        sim.make_econ(P0, word),
        Err(CantMakeEcon::Level(r.skill, r.level)),
        "{word}"
    );
    skilled(&mut sim, r.skill, r.level);
    assert_eq!(sim.make_econ(P0, word), Ok(()), "{word}");
    for &(item, _) in r.needs {
        assert_eq!(sim.crab(P0).unwrap().stash.count(item), 0, "{word} used it");
    }
    sim
}

#[test]
fn claw_tips_and_guards_fit_the_claw_slot() {
    for (word, gear, percent) in [
        ("beak-tips", Gear::BeakTips, 10),
        ("fang-tips", Gear::FangTips, 18),
        ("crusher-guards", Gear::CrusherGuards, 8),
        ("jaw-tips", Gear::JawTips, 12),
    ] {
        let sim = make(word);
        assert_eq!(econ(&sim).gear.claws, Some(gear), "{word}");
        assert_eq!(gear.claw_percent(), percent);
        assert!((econ(&sim).claw_bonus() - (1.0 + percent as f32 / 100.0)).abs() < 1e-6);
    }
}

#[test]
fn only_one_claw_gear_at_a_time() {
    let mut sim = make("beak-tips");
    sim.crab_mut(P0).unwrap().stash = Stash::of(&[(Item::BobbitJaw, 2), (Item::BraidedCord, 1)]);
    skilled(&mut sim, Skill::Shellcraft, 40);
    assert_eq!(sim.make_econ(P0, "jaw-tips"), Ok(()));
    assert_eq!(econ(&sim).gear.claws, Some(Gear::JawTips));
}

#[test]
fn isopod_plating_sits_between_bone_and_nodule() {
    let sim = make("isopod-plating");
    assert_eq!(econ(&sim).gear.body, Some((Gear::IsopodPlating, 140)));
    assert!(
        Gear::BonePlating.damage_cut_percent() < Gear::IsopodPlating.damage_cut_percent()
            && Gear::IsopodPlating.damage_cut_percent() < Gear::NodulePlating.damage_cut_percent()
    );
    assert!((econ(&sim).damage_taken_factor() - 0.75).abs() < 1e-6);
}

#[test]
fn slime_cord_makes_braided_cord() {
    let sim = make("slime-cord");
    assert_eq!(sim.crab(P0).unwrap().stash.count(Item::BraidedCord), 3);
}

#[test]
fn a_photophore_makes_the_lantern_brighter() {
    let mut sim = at_home_with(&[(Item::Photophore, 1), (Item::TwilightStrain, 1)]);
    skilled(&mut sim, Skill::Culturing, 40);
    let before = sim.crab(P0).unwrap().mover.lantern;
    assert_eq!(sim.make_econ(P0, "photophore-lantern"), Ok(()));
    assert_eq!(sim.crab(P0).unwrap().mover.lantern, before + 1);
    assert!(matches!(
        ECON_RECIPES
            .iter()
            .find(|r| r.word == "photophore-lantern")
            .unwrap()
            .makes,
        Output::Effect(Effect::Lantern)
    ));
}

#[test]
fn the_whelk_rooms_grow_the_home_one_stage_at_a_time() {
    let mut sim = at_home_with(&[
        (Item::WhelkShell, 6),
        (Item::Clay, 12),
        (Item::OctopusInk, 1),
        (Item::BraidedCord, 2),
        (Item::Nodule, 2),
        (Item::BlackCoral, 2),
    ]);
    skilled(&mut sim, Skill::Building, 62);
    let t = sim.tuning.clone();
    // In order only.
    assert_eq!(
        sim.make_econ(P0, "whelk-hall"),
        Err(CantMakeEcon::NeedsEarlierRoom)
    );
    for (stage, word) in [(1, "whelk-room"), (2, "whelk-hall"), (3, "whelk-keep")] {
        assert_eq!(sim.make_econ(P0, word), Ok(()));
        assert_eq!(sim.crab(P0).unwrap().home_stage(&t), stage);
        assert_eq!(sim.make_econ(P0, word), Err(CantMakeEcon::Made));
    }
}

#[test]
fn anemone_guards_fit_the_clam_guards() {
    let sim = make("anemone-guards");
    assert!(sim.crab(P0).unwrap().mover.clam_guards);
    let mut sim = sim;
    sim.crab_mut(P0).unwrap().stash = Stash::of(&[(Item::Anemone, 1), (Item::Valve, 2)]);
    assert_eq!(sim.make_econ(P0, "anemone-guards"), Err(CantMakeEcon::Made));
}

#[test]
fn amphipod_broth_cuts_pressure_and_gives_a_stroke() {
    let sim = make("amphipod-broth");
    assert_eq!(sim.crab(P0).unwrap().mover.prepared_food.held(), 2);
    assert_eq!(Food::AmphipodBroth.reserve_strokes(), 1);
    let e = Economy {
        buff: Food::AmphipodBroth.buff(),
        ..Economy::default()
    };
    assert!(matches!(e.buff, Some((Buff::Pressure20, _))));
    assert!((e.pressure_factor() - 0.8).abs() < 1e-6);
}

#[test]
fn the_original_creatures_leave_their_drops_on_the_shell() {
    let table = [
        (Kind::Snapper, vec![Item::SnapperClaw, Item::ShrimpMeat]),
        (Kind::Mantis, vec![Item::MantisClub]),
        (Kind::Moray, vec![Item::MorayTooth]),
        (Kind::Triggerfish, vec![Item::TriggerPlate]),
        (Kind::AlarmJelly, vec![Item::JellyMesoglea]),
        (Kind::Octopus, vec![Item::OctopusInk]),
    ];
    for (kind, drops) in table {
        assert_eq!(crabsim::economy::items::drops_of(kind), drops, "{kind:?}");
        let mut sim = beaten(kind);
        for _ in 0..20 {
            sim.tick();
        }
        let s = shell(&sim);
        for item in drops {
            assert!(
                s.contains(&Carry::Item(item)),
                "{kind:?} gave no {item:?}: {s:?}"
            );
        }
    }
}

#[test]
fn a_monkey_and_the_test_fixture_leave_nothing() {
    assert!(crabsim::economy::items::drops_of(Kind::Monkey).is_empty());
    assert!(crabsim::economy::items::drops_of(Kind::Fixture).is_empty());
}

#[test]
fn a_snapper_claw_re_tips_a_worn_tool_of_its_tier_for_grit_by_tier() {
    use crabsim::economy::ToolKind;
    for tier in 1u8..=5 {
        let word = format!("retip-{tier}");
        let r = ECON_RECIPES.iter().find(|r| r.word == word).expect("row");
        assert_eq!(r.level, 1, "usable from the start");
        assert_eq!(
            r.needs,
            &[(Item::SnapperClaw, 1), (Item::Grit, u32::from(tier))]
        );
        let mut sim = at_home_with(r.needs);
        sim.crab_mut(P0).unwrap().econ.tools[ToolKind::Pry.index()] =
            crabsim::economy::Tool::new(ToolKind::Pry, tier);
        // Whole: nothing to mend.
        assert_eq!(
            sim.make_econ(P0, word.as_str()),
            Err(CantMakeEcon::NothingToMend)
        );
        sim.crab_mut(P0).unwrap().econ.tools[ToolKind::Pry.index()].uses_left = 0;
        assert_eq!(sim.make_econ(P0, word.as_str()), Ok(()));
        assert!(!econ(&sim).tool(ToolKind::Pry).cracked());
    }
}

#[test]
fn a_creature_part_mends_whatever_has_worn() {
    let mut sim = at_home_with(&[(Item::SquidBeak, 1), (Item::Grit, 2)]);
    sim.crab_mut(P0).unwrap().econ.tools[0] =
        crabsim::economy::Tool::new(crabsim::economy::ToolKind::Pry, 3);
    sim.crab_mut(P0).unwrap().econ.tools[0].uses_left = 3;
    assert_eq!(sim.make_econ(P0, "mend-beak"), Ok(()));
    assert_eq!(
        econ(&sim).tool(crabsim::economy::ToolKind::Pry).uses_left,
        crabsim::economy::gear::TOOL_USES[3]
    );
}

#[test]
fn the_new_foods_make_portions() {
    for word in [
        "snapper-broth",
        "amphipod-mash",
        "jelly-salad",
        "inked-kelp",
    ] {
        let sim = make(word);
        assert_eq!(
            sim.crab(P0).unwrap().mover.prepared_food.held(),
            2,
            "{word}"
        );
    }
    // The feast is made at the crock (spoilage can fail a batch: more skill).
    let r = ECON_RECIPES
        .iter()
        .find(|r| r.word == "shrimp-paste-feast")
        .expect("row");
    let mut sim = at_home_with(r.needs);
    skilled(&mut sim, Skill::Cooking, 99);
    build(&mut sim, Station::Crock);
    assert_eq!(sim.make_econ(P0, "shrimp-paste-feast"), Ok(()));
    assert_eq!(sim.crab(P0).unwrap().mover.prepared_food.held(), 2);
    assert!(Food::ShrimpPasteFeast.buff().is_some());
}

#[test]
fn club_and_moray_tips_fit_the_claw_slot() {
    let sim = make("club-tips");
    assert_eq!(econ(&sim).gear.claws, Some(Gear::ClubTips));
    let sim = make("moray-tips");
    assert_eq!(econ(&sim).gear.claws, Some(Gear::MorayTips));
    assert!(Gear::ClubTips.claw_percent() < Gear::MorayTips.claw_percent());
}

#[test]
fn slime_twine_and_a_photophore_seed_work_early() {
    let sim = make("slime-twine");
    assert_eq!(sim.crab(P0).unwrap().stash.count(Item::BraidedCord), 1);
    let sim = make("glow-seed");
    assert_eq!(sim.crab(P0).unwrap().stash.count(Item::Culture), 2);
}
