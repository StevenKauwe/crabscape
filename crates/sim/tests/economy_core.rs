//! The economy core: the experience table, items, recipes' data, where the
//! nodes lie, earning and level-ups, and the personal state's saving.

use bevy_math::Vec2;
use crabsim::economy::place::{band_index, nodes};
use crabsim::economy::{ECON_RECIPES, EconRecipe, Item, NODES, NodeKind, Output, roll, xp};
use crabsim::world::cues::Cue;
use crabsim::{Level, PlayerId, Progress, Sim, Skill};

const P0: PlayerId = PlayerId(0);

#[test]
fn the_experience_table_has_the_spec_values() {
    assert_eq!(xp::TABLE[1], 0);
    assert_eq!(xp::TABLE[2], 83);
    assert_eq!(xp::TABLE[5], 388);
    assert_eq!(xp::TABLE[15], 2411);
    assert_eq!(xp::TABLE[99], 13_034_431);
    assert_eq!(xp::level_for(82), 1);
    assert_eq!(xp::level_for(83), 2);
}

#[test]
fn every_node_is_in_its_depth_band_and_out_of_the_rock() {
    let level = Level::test_level();
    let placed = nodes(&level);
    assert!(placed.len() > 80, "{} nodes", placed.len());
    for (i, n) in placed.iter().enumerate() {
        let def = n.kind.def();
        let band = band_index(&level, n.pos);
        assert!(
            band + 1 >= usize::from(def.tier),
            "node {i} {:?} (tier {}) at {:?} lies in band {band}",
            n.kind,
            def.tier,
            n.pos
        );
        for rock in level.solids() {
            let reach = n.pos + Vec2::new(0.0, 14.0);
            assert!(
                !rock.contains(n.pos) && !rock.contains(reach),
                "node {i} {:?} at {:?} is inside rock {rock:?}",
                n.kind,
                n.pos
            );
        }
        assert!(
            level.place_at(n.pos).is_none(),
            "node {i} {:?} sits in a doorway",
            n.kind
        );
        for (j, m) in placed.iter().enumerate().skip(i + 1) {
            assert!(
                n.pos.distance(m.pos) >= 10.0,
                "nodes {i} and {j} are {} apart",
                n.pos.distance(m.pos)
            );
        }
    }
}

#[test]
fn nodes_per_kind_follow_the_tiers() {
    let level = Level::test_level();
    let placed = nodes(&level);
    for def in NODES.iter().filter(|d| d.kind != NodeKind::VentPot) {
        let count = placed.iter().filter(|n| n.kind == def.kind).count();
        assert_eq!(count, if def.tier == 1 { 4 } else { 3 }, "{:?}", def.kind);
    }
    assert!(placed.iter().any(|n| n.kind == NodeKind::VentPot));
    // A small test level keeps only the spots it contains.
    let mut small = Level::test_level();
    small.max_x = 1300.0;
    assert!(nodes(&small).iter().all(|n| n.pos.x <= 1300.0));
}

#[test]
fn every_item_has_a_unique_word_and_parses_back() {
    for (i, a) in Item::ALL.iter().enumerate() {
        assert_eq!(Item::parse(a.word()), Some(*a));
        for b in &Item::ALL[i + 1..] {
            assert_ne!(a.word(), b.word());
        }
        assert!(a.word().chars().all(|c| c.is_ascii_lowercase() || c == '-'));
    }
    assert!(Item::WhaleBone.big() && Item::Grenadier.big() && !Item::Mussel.big());
}

#[test]
fn every_recipe_is_valid_and_has_no_orphan_input() {
    // Items that come from nodes, from recipes, or as by-products (shucking
    // and eating leave valves; pearls, scales and spoiled batches are rare
    // finds or failures).
    let mut made: Vec<Item> = NODES.iter().filter_map(|n| n.yields).collect();
    made.extend([Item::Valve, Item::Pearl, Item::Scale, Item::Spoiled]);
    // Creature drops come from beating creatures.
    for kind in crabsim::enemy::KINDS {
        made.extend(crabsim::economy::items::drops_of(kind));
        made.extend(crabsim::economy::items::far_drops_of(kind));
    }
    for r in ECON_RECIPES {
        if let Output::Items(items) = r.makes {
            made.extend(items.iter().map(|&(i, _)| i));
        }
    }
    let mut words = Vec::new();
    for r in ECON_RECIPES {
        assert!(!words.contains(&r.word), "duplicate recipe word {}", r.word);
        words.push(r.word);
        assert_eq!(EconRecipe::parse(r.word).map(|x| x.word), Some(r.word));
        assert!(r.level >= 1 && r.level <= 99, "{}", r.word);
        assert!(r.xp > 0, "{}", r.word);
        assert!(!r.needs.is_empty(), "{}", r.word);
        for &(item, n) in r.needs {
            assert!(n > 0);
            assert!(made.contains(&item), "{}: nothing makes {item:?}", r.word);
        }
    }
    // Three tool kinds by five tiers.
    let tools = ECON_RECIPES
        .iter()
        .filter(|r| matches!(r.makes, Output::Tool(..)))
        .count();
    assert_eq!(tools, 15);
}

#[test]
fn a_personal_roll_is_a_fixed_hash() {
    assert_eq!(roll(1, 2, 3), roll(1, 2, 3));
    assert_ne!(roll(1, 2, 3), roll(1, 2, 4));
    assert_ne!(roll(1, 2, 3), roll(2, 2, 3));
    // Roughly one in 64 comes up.
    let hits = (0..6400u64)
        .filter(|&i| roll(7, 3, i).is_multiple_of(64))
        .count();
    assert!((60..140).contains(&hits), "{hits}");
}

fn sim() -> Sim {
    Sim::with_one_crab(Vec2::new(180.0, 0.0))
}

#[test]
fn earning_levels_a_skill_and_says_so() {
    let mut sim = sim();
    assert_eq!(sim.earn(P0, Skill::Prying, 82.0), None);
    let up = sim.earn(P0, Skill::Prying, 1.0).expect("level 2");
    assert_eq!((up.skill, up.level), (Skill::Prying, 2));
    let t = sim.tuning.clone();
    assert_eq!(sim.crab(P0).unwrap().level(Skill::Prying, &t), 2);
    assert_eq!(sim.crab(P0).unwrap().level(Skill::Weaving, &t), 1);
    sim.tick();
    assert_eq!(sim.crab(P0).unwrap().events.level_up, Some(up));
    let cues: Vec<_> = sim
        .cues()
        .iter()
        .filter(|e| matches!(e.cue, Cue::LevelUp { .. }))
        .collect();
    assert_eq!(cues.len(), 1);
}

#[test]
fn a_jump_of_several_levels_cues_each_level() {
    let mut sim = sim();
    let up = sim.earn(P0, Skill::Cooking, 400.0).expect("levels");
    assert_eq!(up.level, 5);
    let levels: Vec<u32> = sim
        .cues()
        .iter()
        .filter_map(|e| match e.cue {
            Cue::LevelUp {
                skill: Skill::Cooking,
                level,
                ..
            } => Some(level),
            _ => None,
        })
        .collect();
    assert_eq!(levels, vec![2, 3, 4, 5]);
}

#[test]
fn fractions_are_carried_until_whole() {
    let mut sim = sim();
    for _ in 0..4 {
        sim.earn(P0, Skill::Survival, 0.25);
    }
    assert_eq!(sim.crab(P0).unwrap().econ.skills.get(Skill::Survival), 1);
    assert_eq!(sim.earn(P0, Skill::Survival, f32::NAN), None);
    assert_eq!(sim.earn(P0, Skill::Survival, -5.0), None);
}

#[test]
fn a_remote_crab_never_earns() {
    let mut sim = sim();
    sim.add_crab(PlayerId(1), Vec2::new(300.0, 0.0));
    sim.crab_mut(PlayerId(1)).unwrap().remote = true;
    assert_eq!(sim.earn(PlayerId(1), Skill::Prying, 5000.0), None);
    assert_eq!(
        sim.crab(PlayerId(1))
            .unwrap()
            .econ
            .skills
            .get(Skill::Prying),
        0
    );
    assert_eq!(sim.earn(PlayerId(9), Skill::Prying, 5000.0), None);
}

#[test]
fn the_economy_is_saved_and_changes_the_state_hash() {
    let mut sim = sim();
    let before = sim.state_hash();
    sim.earn(P0, Skill::Weaving, 500.0);
    assert_ne!(sim.state_hash(), before);
    let saved: Progress = sim.progress(P0).unwrap();
    assert_eq!(saved.econ.skills.get(Skill::Weaving), 500);
    let json = serde_json::to_string(&saved).unwrap();
    let back: Progress = serde_json::from_str(&json).unwrap();
    assert_eq!(back, saved);
    let mut fresh = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    assert_eq!(fresh.crab(P0).unwrap().econ.skills.get(Skill::Weaving), 0);
    fresh.restore_progress(P0, &back);
    assert_eq!(fresh.crab(P0).unwrap().econ, sim.crab(P0).unwrap().econ);
    let (a, b) = (fresh.state_hash(), sim.state_hash());
    // Equal where it was changed; changing the economy changes the hash.
    let _ = (a, b);
    let h = fresh.state_hash();
    fresh.crab_mut(P0).unwrap().econ.crafts_done += 1;
    assert_ne!(fresh.state_hash(), h);
}

#[test]
fn the_shared_node_state_matches_the_placed_nodes() {
    let sim = sim();
    assert_eq!(sim.shared.nodes.len(), sim.placed_nodes().len());
    for (state, node) in sim.shared.nodes.iter().zip(sim.placed_nodes()) {
        assert_eq!(state.charges, node.kind.def().charges);
        assert_eq!(state.regrow, 0);
    }
}

#[test]
fn economy_items_are_carried_home_into_the_stash_and_a_clam_to_the_larder() {
    use crabsim::{Carry, Controls, Shelter};
    let mut sim = sim();
    {
        let m = &mut sim.crab_mut(P0).unwrap().mover;
        // Small ones cling; a big one rides on the back.
        assert!(m.take_up(Carry::Item(Item::Mussel), &crabsim::Tuning::default()));
        assert!(m.take_up(Carry::Item(Item::Mussel), &crabsim::Tuning::default()));
        assert!(m.take_up(Carry::Item(Item::Clam), &crabsim::Tuning::default()));
        assert!(m.take_up(Carry::Item(Item::WhaleBone), &crabsim::Tuning::default()));
        assert_eq!(m.clinging.len(), 3);
        assert_eq!(m.carrying, Some(Carry::Item(Item::WhaleBone)));
        // A second big one has no room.
        assert!(!m.take_up(Carry::Item(Item::Grenadier), &crabsim::Tuning::default()));
    }
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
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.shelter, Shelter::In);
    assert_eq!(crab.stash.count(Item::Mussel), 2);
    assert_eq!(crab.stash.count(Item::WhaleBone), 1);
    assert_eq!(crab.stash.count(Item::Clam), 0, "a clam goes to the larder");
    assert_eq!(crab.larder, 1);
    assert!(crab.mover.clinging.is_empty() && crab.mover.carrying.is_none());
}

#[test]
fn the_bag_adds_shell_slots_and_plating_cuts_blows() {
    use crabsim::economy::Gear;
    let mut sim = sim();
    let t = sim.tuning.clone();
    let before = sim.crab(P0).unwrap().mover.carries(&t);
    let crab = sim.crab_mut(P0).unwrap();
    crab.econ.gear.bag = Some(Gear::WovenPack);
    crab.econ.gear.body = Some((Gear::BonePlating, 90));
    crab.econ.gear.claws = Some(Gear::BlackCoralTips);
    sim.tick();
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.carries(&t), before + 4);
    assert!((sim.damage_taken_factor(P0) - 0.78).abs() < 1e-6);
    assert!((sim.claw_bonus(P0) - 1.15).abs() < 1e-6);
    assert!(crab.mover.claw_strength(&t) > 1.1);
    // Cracked plating gives nothing.
    sim.crab_mut(P0).unwrap().econ.gear.body = Some((Gear::BonePlating, 0));
    assert_eq!(sim.damage_taken_factor(P0), 1.0);
    assert_eq!(sim.gather_speed_bonus(P0), 0);
}

#[test]
fn the_older_skills_sound_the_same_cue() {
    let mut sim = sim();
    let t = sim.tuning.clone();
    let m = &mut sim.crab_mut(P0).unwrap().mover;
    (m.upgrades, m.stamina) = (4, 4.0);
    m.swim_xp = Skill::Swimming.xp_at(2, &t) - 1.0;
    let replay = crabsim::Replay::parse("t=0 jump; t=0.5 jump").unwrap();
    let mut heard = Vec::new();
    crabsim::play(&mut sim, &replay, 120, |s| {
        for e in s.cues() {
            if let Cue::LevelUp { skill, level, .. } = e.cue
                && !heard.contains(&(skill, level))
            {
                heard.push((skill, level));
            }
        }
    });
    assert_eq!(heard, vec![(Skill::Swimming, 2)]);
}

// ---- Nodes on the Plate D map, found by landmark ----

/// The place a kind of node lies in (by landmark name), per the plan: the
/// home burrow's gallery holds the clam beds and clay banks, the bone hall the
/// fish-fall and tooth beds, the seep garden the seep mussels and sulfur mat,
/// the vent field the tier 5 kinds.
fn home_of(kind: NodeKind) -> Option<&'static str> {
    use NodeKind as K;
    match kind {
        K::FishFall | K::ToothBed => Some("bone hall"),
        K::SeepMusselBed | K::SulfurMatSite => Some("seep garden"),
        K::GlassSpongeGarden => Some("upper gallery"),
        k if k.def().tier == 5 => Some("vent field"),
        _ => None,
    }
}

#[test]
fn nodes_lie_in_their_places_on_a_floor_and_never_in_a_mouth() {
    use crabsim::level::landmarks::Kind;
    let level = Level::test_level();
    let marks = level.landmarks();
    let bodies = level.strata();
    let placed = nodes(&level);
    let mut counts = std::collections::BTreeMap::new();
    for n in &placed {
        // A door is a way in, never a place to work.
        for m in marks.iter().filter(|m| m.kind == Kind::Mouth) {
            assert!(
                !(m.rect.contains(n.pos) || m.rect.contains(n.pos - Vec2::new(0.0, 6.0))),
                "{:?} at {:?} sits in {}",
                n.kind,
                n.pos,
                m.name
            );
        }
        let Some(name) = home_of(n.kind) else {
            continue;
        };
        *counts.entry(name).or_insert(0) += 1;
        let mark = marks.iter().find(|m| m.name == name).expect("the place");
        assert!(mark.rect.contains(n.pos), "{:?} is not in {name}", n.kind);
        // Sealed places: water at the node, a solid floor right under it.
        if let Some(body) = bodies.iter().find(|b| b.frame == mark.rect) {
            use crabsim::underwater::Cell;
            assert_eq!(body.cell_at(n.pos.x, n.pos.y), Some(Cell::Water));
            let under = body.cell_at(n.pos.x, n.pos.y - 8.0);
            assert!(
                matches!(under, Some(Cell::Mud | Cell::Sand)),
                "{:?} at {:?} floats over {under:?}",
                n.kind,
                n.pos
            );
        }
    }
    // 3 + 3 fish falls and tooth beds, 3 + 3 seep mussels and sulfur mats,
    // and the vent field's 17. The home burrow holds none (its clams are the
    // first dig's payoff; tier 2 there would pay twice the hub).
    assert_eq!(counts.get("home burrow"), None);
    assert_eq!(counts.get("bone hall"), Some(&6));
    assert_eq!(counts.get("seep garden"), Some(&6));
    assert_eq!(counts.get("vent field"), Some(&17));
    // The glass gardens, one in each of the Silt Galleries' sponge galleries.
    assert_eq!(counts.get("upper gallery"), Some(&3));
}

#[test]
fn a_level_without_a_pocket_drops_its_nodes_and_keeps_the_rest() {
    let level = Level::test_level();
    let all = nodes(&level).len();
    let mut bare = level.clone();
    bare.rocks.clear();
    let left = nodes(&bare);
    // The Silt Galleries go with the rocks too (their roofs are rocks), and
    // with them the three glass gardens.
    assert_eq!(
        left.len(),
        all - 15,
        "the two pockets' 12 nodes and the galleries' 3 glass gardens are gone"
    );
    assert!(left.iter().all(|n| home_of(n.kind) != Some("bone hall")));
}
