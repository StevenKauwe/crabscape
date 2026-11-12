//! Promises about crafting chains: every kind of output works, the reasons
//! are right, cooking can spoil (deterministically, less with skill), tools
//! and plating wear and are mended, food buffs run out, culture beds grow,
//! and a replay with craft words plays the same.

use std::collections::BTreeSet;

use bevy_math::Vec2;
use crabsim::craft::CantMakeEcon;
use crabsim::economy::food::{BUFF_TICKS, FEAST_TICKS};
use crabsim::economy::gear::{BED_LEVELS, TOOL_USES};
use crabsim::economy::recipes::{Bench, StrainDef};
use crabsim::economy::{
    Buff, ECON_RECIPES, Economy, Food, Gear, Item, NODES, Output, Station, ToolKind, xp,
};
use crabsim::prepared_food::PortionKind;
use crabsim::{Controls, PlayerId, Sim, Skill, Stash};

const P0: PlayerId = PlayerId(0);

fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.tick();
    }
}

/// A crab tucked in at home with `stash` in the bank.
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

fn set_level(sim: &mut Sim, skill: Skill, level: u32) {
    sim.crab_mut(P0)
        .unwrap()
        .econ
        .skills
        .set(skill, xp::TABLE[level as usize]);
}

fn stash(sim: &Sim, item: Item) -> u32 {
    sim.crab(P0).unwrap().stash.count(item)
}

fn econ(sim: &Sim) -> &Economy {
    &sim.crab(P0).unwrap().econ
}

#[test]
fn items_and_the_xp_they_earn() {
    let mut sim = at_home_with(&[(Item::Kelp, 5)]);
    assert_eq!(sim.make_econ(P0, "kelp-cord"), Ok(()));
    assert_eq!(stash(&sim, Item::Kelp), 3);
    assert_eq!(stash(&sim, Item::KelpCord), 1);
    assert_eq!(econ(&sim).skills.get(Skill::Weaving), 8);
    assert_eq!(econ(&sim).crafts_done, 1);
}

#[test]
fn a_tool_replaces_the_fitted_one_with_all_its_uses() {
    let mut sim = at_home_with(&[(Item::ShellBlank, 1), (Item::KelpCord, 1)]);
    assert_eq!(econ(&sim).tool(ToolKind::Pry).tier, 0);
    assert_eq!(sim.make_econ(P0, "pry-shell"), Ok(()));
    let t = *econ(&sim).tool(ToolKind::Pry);
    assert_eq!((t.tier, t.uses_left), (1, TOOL_USES[1]));
    // Other kinds are untouched, and a tool you already have is not made again.
    assert_eq!(econ(&sim).tool(ToolKind::Scoop).tier, 0);
    sim.crab_mut(P0).unwrap().stash.add(Item::ShellBlank, 1);
    sim.crab_mut(P0).unwrap().stash.add(Item::KelpCord, 1);
    assert_eq!(sim.make_econ(P0, "pry-shell"), Err(CantMakeEcon::Made));
}

#[test]
fn gear_is_fitted_in_its_slot() {
    let mut sim = at_home_with(&[
        (Item::KelpCord, 5),
        (Item::ShellBlank, 3),
        (Item::BlackCoral, 2),
        (Item::BoneSplint, 1),
    ]);
    set_level(&mut sim, Skill::Weaving, 8);
    set_level(&mut sim, Skill::Shellcraft, 55);
    sim.crab_mut(P0)
        .unwrap()
        .econ
        .stations
        .insert(Station::Grindstone);
    assert_eq!(sim.make_econ(P0, "kelp-sack"), Ok(()));
    assert_eq!(econ(&sim).gear.bag, Some(Gear::KelpSack));
    assert_eq!(econ(&sim).shell_slots_bonus(), 2);
    assert_eq!(sim.make_econ(P0, "shell-plating"), Ok(()));
    assert_eq!(econ(&sim).gear.body, Some((Gear::ShellPlating, 40)));
    assert_eq!(sim.make_econ(P0, "coral-tips"), Ok(()));
    assert_eq!(econ(&sim).gear.claws, Some(Gear::BlackCoralTips));
    assert_eq!(sim.make_econ(P0, "coral-tips"), Err(CantMakeEcon::Made));
}

#[test]
fn stations_are_built_and_beds_in_order() {
    let mut sim = at_home_with(&[
        (Item::Quill, 4),
        (Item::KelpCord, 4),
        (Item::Grit, 6),
        (Item::Valve, 2),
        (Item::Clay, 3),
    ]);
    set_level(&mut sim, Skill::Building, 12);
    assert_eq!(sim.make_econ(P0, "loom"), Ok(()));
    assert!(econ(&sim).has_station(Station::Loom));
    assert_eq!(sim.make_econ(P0, "loom"), Err(CantMakeEcon::Made));
    // Building XP: 40 * level / 5, at least 40.
    assert_eq!(econ(&sim).skills.get(Skill::Building), xp::TABLE[12] + 96);
    // The second bed needs the first.
    set_level(&mut sim, Skill::Building, 20);
    assert_eq!(
        sim.make_econ(P0, "bed-2"),
        Err(CantMakeEcon::NeedsEarlierBed)
    );
    assert_eq!(sim.make_econ(P0, "bed-1"), Ok(()));
    assert_eq!(econ(&sim).culture_beds, 1);
    assert_eq!(econ(&sim).beds.len(), 1);
    assert_eq!(sim.make_econ(P0, "bed-1"), Err(CantMakeEcon::Made));
    assert_eq!(BED_LEVELS, [5, 20, 40, 60]);
}

#[test]
fn four_beds_at_most_at_the_building_levels() {
    let mut sim = at_home_with(&[
        (Item::Grit, 20),
        (Item::Valve, 4),
        (Item::Clay, 20),
        (Item::WhaleBone, 1),
        (Item::GreatMantisClub, 1),
        (Item::Nodule, 2),
        (Item::JellyMesoglea, 2),
    ]);
    for (n, word) in ["bed-1", "bed-2", "bed-3", "bed-4"].into_iter().enumerate() {
        let need = BED_LEVELS[n];
        set_level(&mut sim, Skill::Building, need - 1);
        assert_eq!(
            sim.make_econ(P0, word),
            Err(CantMakeEcon::Level(Skill::Building, need))
        );
        set_level(&mut sim, Skill::Building, need);
        assert_eq!(sim.make_econ(P0, word), Ok(()), "{word}");
    }
    assert_eq!(econ(&sim).culture_beds, 4);
    assert_eq!(sim.make_econ(P0, "bed-4"), Err(CantMakeEcon::Made));
}

#[test]
fn shucked_mussels_make_two_portions_and_a_valve() {
    let mut sim = at_home_with(&[(Item::Mussel, 2)]);
    assert_eq!(sim.make_econ(P0, "shucked-mussels"), Ok(()));
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.prepared_food.held(), 2);
    assert!(
        crab.mover
            .prepared_food
            .slots
            .iter()
            .flatten()
            .all(|p| p.kind == PortionKind::Food(Food::ShuckedMussels))
    );
    assert_eq!(crab.stash.count(Item::Valve), 1);
    assert_eq!(crab.econ.skills.get(Skill::Cooking), 10);
    // Two batches fill the four slots; a third has no room.
    sim.crab_mut(P0).unwrap().stash.add(Item::Mussel, 4);
    assert_eq!(sim.make_econ(P0, "shucked-mussels"), Ok(()));
    assert_eq!(
        sim.make_econ(P0, "shucked-mussels"),
        Err(CantMakeEcon::NoRoom)
    );
}

#[test]
fn the_older_foods_still_use_the_larder_and_now_earn_cooking() {
    let mut sim = at_home_with(&[]);
    assert_eq!(
        sim.make_econ(P0, "shell-food"),
        Err(CantMakeEcon::Missing(Item::Clam, 1))
    );
    sim.crab_mut(P0).unwrap().larder = 1;
    assert_eq!(sim.make_econ(P0, "shell-food"), Ok(()));
    let crab = sim.crab(P0).unwrap();
    assert_eq!((crab.larder, crab.stash.count(Item::Valve)), (0, 2));
    assert_eq!(crab.econ.skills.get(Skill::Cooking), 12);
    assert!(
        crab.mover
            .prepared_food
            .slots
            .iter()
            .flatten()
            .all(|p| p.kind == PortionKind::Shell)
    );
}

#[test]
fn repairs_restore_uses_and_cost_grit_and_cord() {
    let mut sim = at_home_with(&[(Item::QuillRod, 1), (Item::KelpCord, 4), (Item::Grit, 5)]);
    set_level(&mut sim, Skill::Shellcraft, 15);
    assert_eq!(sim.make_econ(P0, "pry-quill"), Ok(()));
    // Nothing to mend while whole.
    assert_eq!(
        sim.make_econ(P0, "repair-tool-2"),
        Err(CantMakeEcon::NothingToMend)
    );
    let kelp_cord = stash(&sim, Item::KelpCord);
    sim.crab_mut(P0).unwrap().econ.tools[ToolKind::Pry.index()].uses_left = 0;
    assert!(econ(&sim).tool(ToolKind::Pry).cracked());
    // The wrong tier does not mend it.
    assert_eq!(
        sim.make_econ(P0, "repair-tool-3"),
        Err(CantMakeEcon::NothingToMend)
    );
    assert_eq!(sim.make_econ(P0, "repair-tool-2"), Ok(()));
    assert_eq!(econ(&sim).tool(ToolKind::Pry).uses_left, TOOL_USES[2]);
    assert_eq!(stash(&sim, Item::Grit), 3, "grit x tier");
    assert_eq!(stash(&sim, Item::KelpCord), kelp_cord - 1);
}

#[test]
fn plating_cuts_damage_until_it_cracks_and_a_repair_restores_it() {
    let mut sim = at_home_with(&[
        (Item::ShellBlank, 3),
        (Item::KelpCord, 1),
        (Item::Grit, 1),
        (Item::Valve, 1),
    ]);
    set_level(&mut sim, Skill::Shellcraft, 5);
    assert_eq!(econ(&sim).damage_taken_factor(), 1.0);
    assert_eq!(sim.make_econ(P0, "shell-plating"), Ok(()));
    idle(&mut sim, 1);
    assert!((econ(&sim).damage_taken_factor() - 0.9).abs() < 1e-6);
    assert!((sim.crab(P0).unwrap().mover.econ.damage_factor - 0.9).abs() < 1e-6);
    assert!((sim.damage_taken_factor(P0) - 0.9).abs() < 1e-6);
    let crab = sim.crab_mut(P0).unwrap();
    for i in 0..40 {
        let cracked = crab.econ.wear_plating();
        assert_eq!(cracked, i == 39, "blow {i}");
    }
    assert_eq!(crab.econ.damage_taken_factor(), 1.0, "cracked: nothing");
    assert!(!crab.econ.wear_plating());
    idle(&mut sim, 1);
    assert_eq!(sim.crab(P0).unwrap().mover.econ.damage_factor, 1.0);
    assert_eq!(sim.make_econ(P0, "repair-plating-1"), Ok(()));
    assert_eq!(econ(&sim).gear.body, Some((Gear::ShellPlating, 40)));
    assert_eq!(stash(&sim, Item::Valve), 0);
}

#[test]
fn reasons_are_right() {
    let mut sim = at_home_with(&[]);
    assert_eq!(sim.make_econ(P0, "no-such"), Err(CantMakeEcon::Unknown));
    assert_eq!(
        sim.make_econ(P0, "kelp-cord"),
        Err(CantMakeEcon::Missing(Item::Kelp, 2))
    );
    sim.crab_mut(P0).unwrap().stash.add(Item::Kelp, 1);
    assert_eq!(
        sim.make_econ(P0, "kelp-cord"),
        Err(CantMakeEcon::Missing(Item::Kelp, 1)),
        "only the shortfall"
    );
    assert_eq!(
        sim.make_econ(P0, "quill-rod"),
        Err(CantMakeEcon::Level(Skill::Shellcraft, 15))
    );
    set_level(&mut sim, Skill::Weaving, 20);
    sim.crab_mut(P0).unwrap().stash.add(Item::KelpCord, 3);
    assert_eq!(
        sim.make_econ(P0, "braided-cord"),
        Err(CantMakeEcon::Station(Station::Loom))
    );
    assert_eq!(
        sim.make_econ(P0, "vent-poached-seep-mussels"),
        Err(CantMakeEcon::Level(Skill::Cooking, 50))
    );
    set_level(&mut sim, Skill::Cooking, 50);
    assert_eq!(
        sim.make_econ(P0, "vent-poached-seep-mussels"),
        Err(CantMakeEcon::NotAtVentPot)
    );
    assert_eq!(
        sim.make_econ(P0, "plant-shore"),
        Err(CantMakeEcon::Station(Station::CultureBed))
    );
    // A failed make changes nothing.
    assert_eq!(stash(&sim, Item::Kelp), 1);
    assert_eq!(econ(&sim).crafts_done, 0);
}

#[test]
fn making_is_only_at_home_and_only_for_a_local_crab() {
    let mut out = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    out.crab_mut(P0).unwrap().stash = Stash::of(&[(Item::Kelp, 4)]);
    assert_eq!(out.make_econ(P0, "kelp-cord"), Err(CantMakeEcon::NotAtHome));
    assert_eq!(out.harvest_bed(P0), Err(CantMakeEcon::NotAtHome));
    assert_eq!(out.feed_bed(P0), Err(CantMakeEcon::NotAtHome));

    let mut sim = at_home_with(&[(Item::Kelp, 4)]);
    assert_eq!(sim.can_make_econ(P0, "kelp-cord"), Ok(()));
    sim.crab_mut(P0).unwrap().remote = true;
    assert_eq!(sim.make_econ(P0, "kelp-cord"), Err(CantMakeEcon::NotAtHome));
    assert_eq!(stash(&sim, Item::Kelp), 4);
    assert_eq!(
        sim.make_econ(PlayerId(9), "kelp-cord"),
        Err(CantMakeEcon::NotAtHome)
    );
}

/// Make worm paste (a spoilable cure) `n` times at Cooking `lvl`, clearing
/// the slots each time; says how many batches spoiled.
fn spoiled_in(n: u32, lvl: u32) -> Vec<bool> {
    let mut sim = at_home_with(&[]);
    sim.crab_mut(P0)
        .unwrap()
        .econ
        .stations
        .insert(Station::BrinePot);
    let mut out = Vec::new();
    for _ in 0..n {
        set_level(&mut sim, Skill::Cooking, lvl);
        let crab = sim.crab_mut(P0).unwrap();
        crab.mover.prepared_food = Default::default();
        crab.stash = Stash::of(&[(Item::Worm, 2), (Item::Kelp, 1)]);
        assert_eq!(sim.make_econ(P0, "worm-paste"), Ok(()));
        let spoiled = stash(&sim, Item::Spoiled) == 1;
        let held = sim.crab(P0).unwrap().mover.prepared_food.held();
        assert_eq!(held, if spoiled { 0 } else { 2 });
        out.push(spoiled);
    }
    out
}

#[test]
fn spoiling_is_deterministic_and_falls_with_skill() {
    let low = spoiled_in(300, 12);
    assert_eq!(low, spoiled_in(300, 12), "the same crab spoils the same");
    let mid = spoiled_in(300, 20);
    let high = spoiled_in(300, 27);
    let count = |v: &[bool]| v.iter().filter(|s| **s).count();
    // 30% at the requirement, 14% eight levels on, none at 15 levels over.
    assert!((60..=120).contains(&count(&low)), "{}", count(&low));
    assert!((20..=65).contains(&count(&mid)), "{}", count(&mid));
    assert_eq!(count(&high), 0);
    assert!(count(&low) > count(&mid) && count(&mid) > count(&high));
    let r = crabsim::economy::EconRecipe::parse("worm-paste").unwrap();
    assert_eq!(
        [
            r.spoil_percent(12),
            r.spoil_percent(20),
            r.spoil_percent(27)
        ],
        [30, 14, 0]
    );
    // The workbench's own foods never spoil.
    assert_eq!(
        crabsim::economy::EconRecipe::parse("shucked-mussels")
            .unwrap()
            .spoil_percent(1),
        0
    );
}

#[test]
fn a_spoiled_batch_gives_a_spoiled_item_and_half_the_xp() {
    let mut sim = at_home_with(&[]);
    sim.crab_mut(P0)
        .unwrap()
        .econ
        .stations
        .insert(Station::BrinePot);
    // Find a craft number that spoils at Cooking 12 (30%).
    for _ in 0..40 {
        set_level(&mut sim, Skill::Cooking, 12);
        let before = econ(&sim).skills.get(Skill::Cooking);
        let crab = sim.crab_mut(P0).unwrap();
        crab.mover.prepared_food = Default::default();
        crab.stash = Stash::of(&[(Item::Worm, 2), (Item::Kelp, 1)]);
        sim.make_econ(P0, "worm-paste").unwrap();
        if stash(&sim, Item::Spoiled) == 1 {
            assert_eq!(econ(&sim).skills.get(Skill::Cooking), before + 15);
            assert_eq!(stash(&sim, Item::Worm), 0, "the inputs are gone");
            return;
        }
        assert_eq!(econ(&sim).skills.get(Skill::Cooking), before + 30);
    }
    panic!("40 batches at 30% never spoiled");
}

#[test]
fn buffs_run_out_and_a_new_one_replaces_the_old() {
    let mut e = Economy {
        buff: Some((Buff::Gather10, BUFF_TICKS)),
        ..Economy::default()
    };
    assert_eq!(e.gather_speed_bonus(), 10);
    for _ in 0..BUFF_TICKS - 1 {
        e.tick(0);
    }
    assert_eq!(e.gather_speed_bonus(), 10);
    e.tick(0);
    assert_eq!(e.buff, None);
    assert_eq!(e.gather_speed_bonus(), 0);

    // The feast: four minutes, everything at once.
    e.buff = Some((Buff::Feast, FEAST_TICKS));
    assert_eq!(e.gather_speed_bonus(), 15);
    assert!((e.claw_bonus() - 1.15).abs() < 1e-6);
    assert!((e.pressure_factor() - 0.7).abs() < 1e-6);
    assert!((e.reserve_refill_factor() - 1.25).abs() < 1e-6);
    for _ in 0..FEAST_TICKS - 1 {
        e.tick(0);
    }
    assert!(e.buff.is_some());
    e.tick(0);
    assert!(e.buff.is_none());
    assert_eq!(FEAST_TICKS, 4 * 60 * 120);

    // Claws multiply: tips on top of food.
    e.gear.claws = Some(Gear::BlackCoralTips);
    e.buff = Some((Buff::Claws15, BUFF_TICKS));
    assert!((e.claw_bonus() - 1.15 * 1.15).abs() < 1e-6);
}

#[test]
fn eating_applies_the_food_and_a_new_buff_replaces_the_old() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.mover
            .prepared_food
            .add_batch(PortionKind::of(Food::WormPaste), 2);
        crab.mover
            .prepared_food
            .add_batch(PortionKind::of(Food::SnailWrap), 2);
        crab.mover.reserve = 0.2;
    }
    idle(&mut sim, 120);
    let eat = Controls {
        eat_held: true,
        ..Controls::default()
    };
    sim.set_controls(P0, eat);
    idle(&mut sim, 240);
    sim.set_controls(P0, Controls::default());
    let (b, left) = econ(&sim).buff.expect("worm paste buff");
    assert_eq!(b, Buff::Gather10);
    assert!(left > BUFF_TICKS - 5);
    assert_eq!(sim.gather_speed_bonus(P0), 10);
    // Release, then eat the next portion (the second worm paste).
    idle(&mut sim, 5);
    sim.set_controls(P0, eat);
    idle(&mut sim, 240);
    sim.set_controls(P0, Controls::default());
    idle(&mut sim, 5);
    sim.set_controls(P0, eat);
    idle(&mut sim, 240);
    // The snail wrap replaced the paste and gave a reserve stroke at once.
    assert_eq!(econ(&sim).buff.map(|b| b.0), Some(Buff::ReserveQuick));
    assert!(sim.crab(P0).unwrap().mover.reserve > 0.2);
    assert_eq!(sim.gather_speed_bonus(P0), 0);
}

#[test]
fn beds_grow_while_the_crab_is_out_and_harvest_gives_the_strain() {
    let mut sim = at_home_with(&[(Item::ShoreStrain, 2), (Item::Spoiled, 2)]);
    assert_eq!(
        sim.make_econ(P0, "plant-shore"),
        Err(CantMakeEcon::Station(Station::CultureBed))
    );
    {
        let e = &mut sim.crab_mut(P0).unwrap().econ;
        e.stations.insert(Station::CultureBed);
        e.culture_beds = 1;
        e.beds.push(None);
    }
    assert_eq!(sim.harvest_bed(P0), Err(CantMakeEcon::NothingReady));
    assert_eq!(sim.feed_bed(P0), Err(CantMakeEcon::NothingGrowing));
    let start = sim.tick_count();
    assert_eq!(sim.make_econ(P0, "plant-shore"), Ok(()));
    assert_eq!(econ(&sim).skills.get(Skill::Culturing), 5, "plant XP");
    assert_eq!(stash(&sim, Item::ShoreStrain), 1);
    let bed = econ(&sim).beds[0].unwrap();
    assert_eq!(bed.strain, Item::ShoreStrain);
    assert_eq!(bed.ready_at_tick, start + 120 * 120);
    // Its one bed is busy.
    assert_eq!(
        sim.make_econ(P0, "plant-shore"),
        Err(CantMakeEcon::NoFreeBed)
    );
    // Feeding halves the time left, once.
    assert_eq!(sim.feed_bed(P0), Ok(()));
    let fed = econ(&sim).beds[0].unwrap();
    assert!(fed.fed);
    assert_eq!(fed.ready_at_tick, start + 60 * 120);
    assert_eq!(stash(&sim, Item::Spoiled), 1);
    assert_eq!(sim.feed_bed(P0), Err(CantMakeEcon::NothingGrowing));
    // Not ripe until its time.
    idle(&mut sim, 60 * 120 - 5);
    assert_eq!(sim.harvest_bed(P0), Err(CantMakeEcon::NothingReady));
    idle(&mut sim, 10);
    assert_eq!(sim.harvest_bed(P0), Ok(()));
    assert_eq!(
        stash(&sim, Item::ShoreStrain),
        1 + StrainDef::harvest_count(1)
    );
    assert_eq!(StrainDef::harvest_count(1), 3);
    assert_eq!(StrainDef::harvest_count(40), 5);
    assert_eq!(econ(&sim).skills.get(Skill::Culturing), 25, "5 + 20");
    assert!(econ(&sim).beds[0].is_none());
    assert_eq!(sim.harvest_bed(P0), Err(CantMakeEcon::NothingReady));
}

#[test]
fn a_replay_with_craft_words_reproduces() {
    let mut live = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let mut saved = live.progress(P0).unwrap();
    saved.stash = Stash::of(&[
        (Item::Kelp, 6),
        (Item::ShellBlank, 1),
        (Item::Grit, 6),
        (Item::Valve, 2),
        (Item::ShoreStrain, 1),
        (Item::Spoiled, 1),
        (Item::Mussel, 2),
    ]);
    saved.econ.skills.set(Skill::Building, xp::TABLE[5]);
    live.restore_progress(P0, &saved);
    live.start_recording();
    for _ in 0..1200 {
        live.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        live.tick();
        if live.crab(P0).unwrap().mover.tucked_in() {
            break;
        }
    }
    live.set_controls(P0, Controls::default());
    idle(&mut live, 10);
    for word in [
        "kelp-cord",
        "kelp-cord",
        "pry-shell",
        "shucked-mussels",
        "bed-1",
    ] {
        idle(&mut live, 5);
        assert_eq!(live.make_econ(P0, word), Ok(()), "{word}");
    }
    idle(&mut live, 5);
    assert_eq!(live.make_econ(P0, "plant-shore"), Ok(()));
    assert_eq!(live.feed_bed(P0), Ok(()));
    idle(&mut live, 30);

    let text = live.recording().unwrap().to_text();
    for word in [
        "craft=kelp-cord",
        "craft=pry-shell",
        "craft=bed-1",
        "craft=plant-shore",
        "craft=feed-bed",
    ] {
        assert!(text.contains(word), "{word} in {text}");
    }
    let replay = crabsim::Replay::parse(&text).unwrap();
    assert_eq!(replay.crafts.len(), 7);
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(again.progress(P0), live.progress(P0));
    assert_eq!(econ(&again).beds, econ(&live).beds);
    assert_eq!(econ(&again).tool(ToolKind::Pry).tier, 1);
    assert_eq!(replay.hash, Some(again.state_hash()), "{text}");
}

/// The "no soft lock" proof: starting with nothing, following only what the
/// rules allow, every recipe's inputs become reachable and every tool tier
/// can be made.
#[test]
fn no_soft_lock_fixed_point() {
    let mut items: BTreeSet<Item> = BTreeSet::new();
    let mut tools: BTreeSet<(ToolKind, u8)> = BTreeSet::new();
    let mut stations: BTreeSet<Station> = BTreeSet::new();
    let mut made: BTreeSet<&str> = BTreeSet::new();
    loop {
        let before = (items.len(), tools.len(), stations.len(), made.len());
        // Nodes whose tool is in hand give their item (tier 0 always is).
        for n in NODES {
            let tool_ok = match n.tool {
                None => true,
                Some(_) if n.tool_tier == 0 => true,
                Some(k) => tools.iter().any(|&(tk, t)| tk == k && t >= n.tool_tier),
            };
            if tool_ok && let Some(item) = n.yields {
                items.insert(item);
            }
        }
        // By-products: shucked bivalves leave valves; a pearl and a scale
        // are rare finds from prying and from the deepest foraging.
        if items.contains(&Item::Clam) || items.contains(&Item::Mussel) {
            items.insert(Item::Valve);
            items.insert(Item::Pearl);
        }
        if items.contains(&Item::ScalyFoot) {
            items.insert(Item::Scale);
        }
        // Creature drops: reached by beating the creatures (Fighting).
        for kind in crabsim::enemy::KINDS {
            items.extend(crabsim::economy::items::drops_of(kind));
            items.extend(crabsim::economy::items::far_drops_of(kind));
        }
        for r in ECON_RECIPES {
            let bench_ok = match r.at {
                Bench::Workbench => true,
                Bench::At(st) => stations.contains(&st),
                // A node in the vent field, which the deep kit reaches.
                Bench::VentPot => items.contains(&Item::VentMat),
            };
            if !bench_ok || !r.needs.iter().all(|(i, _)| items.contains(i)) {
                continue;
            }
            made.insert(r.word);
            match r.makes {
                Output::Items(out) => items.extend(out.iter().map(|(i, _)| *i)),
                Output::Tool(k, t) => {
                    tools.insert((k, t));
                }
                Output::Station(st) => {
                    stations.insert(st);
                }
                Output::Portions(f, _) => items.extend(f.extra().iter().map(|(i, _)| *i)),
                Output::Gear(_) | Output::Bed(_) | Output::Repair(_) | Output::Effect(_) => {}
            }
        }
        if before == (items.len(), tools.len(), stations.len(), made.len()) {
            break;
        }
    }
    for r in ECON_RECIPES {
        assert!(
            made.contains(r.word),
            "soft lock: {} is never makeable",
            r.word
        );
    }
    for kind in ToolKind::ALL {
        for tier in 1..=5 {
            assert!(tools.contains(&(kind, tier)), "no {kind:?} tier {tier}");
        }
    }
    for n in NODES {
        if let Some(item) = n.yields {
            assert!(items.contains(&item), "{item:?} is never reachable");
        }
    }
    for st in Station::ALL {
        assert!(stations.contains(&st), "{st:?} is never built");
    }
}
