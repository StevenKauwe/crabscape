//! "Every way to play pays" (the owner's three economy rules):
//!
//! 1. Low-attention gathering near home is safe, close to the door and its
//!    stack stays useful.
//! 2. Farming creatures you have outgrown pays: their drops stay useful for
//!    crafting, cooking and building at every level.
//! 3. Building takes part in other activities: core and rare building
//!    materials come from different activities, and no single loop supplies
//!    them all.
//!
//! The pacing of the low-attention session is in `economy_pacing.rs`.

use std::collections::BTreeSet;

use bevy_math::Vec2;
use crabsim::economy::items::drops_of;
use crabsim::economy::place::nodes;
use crabsim::economy::recipes::EconRecipe;
use crabsim::economy::{ECON_RECIPES, Item, NODES, NodeKind, Output};
use crabsim::enemy::KINDS;
use crabsim::{Level, Skill, Tuning};

/// A repeatable sink: something a crab does again and again at any level
/// (mending, eating, planting).
fn is_sink(r: &EconRecipe) -> bool {
    matches!(
        r.makes,
        Output::Repair(_) | Output::Portions(..) | Output::Bed(_)
    )
}

fn uses(item: Item) -> Vec<&'static EconRecipe> {
    ECON_RECIPES
        .iter()
        .filter(|r| r.needs.iter().any(|&(i, _)| i == item))
        .collect()
}

/// Every item a beaten creature leaves.
fn all_drops() -> Vec<Item> {
    let mut v: Vec<Item> = KINDS.into_iter().flat_map(drops_of).collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn every_creature_drop_stays_useful_the_whole_way() {
    let drops = all_drops();
    assert!(drops.len() >= 17, "{} drops", drops.len());
    for item in drops {
        let rs = uses(item);
        assert!(
            rs.iter().any(|r| r.level >= 40 || is_sink(r)),
            "{item:?} has no high-level use and no repeatable sink"
        );
        assert!(
            rs.iter().any(|r| r.level < 20),
            "{item:?} has no recipe below level 20"
        );
    }
}

#[test]
fn the_snapper_claw_re_tips_a_tool_of_every_tier_and_the_meat_is_food() {
    for tier in 1..=5 {
        let word = format!("retip-{tier}");
        assert!(ECON_RECIPES.iter().any(|r| r.word == word), "{word}");
    }
    let meat = uses(Item::ShrimpMeat);
    assert!(
        meat.iter()
            .any(|r| r.word == "snapper-broth" && r.level == 10)
    );
    assert!(
        meat.iter()
            .any(|r| r.word == "shrimp-paste-feast" && r.level >= 40)
    );
}

#[test]
fn every_tier_one_item_stays_useful() {
    let tier_one: Vec<Item> = NODES
        .iter()
        .filter(|n| n.tier == 1)
        .filter_map(|n| n.yields)
        .collect();
    assert_eq!(tier_one.len(), 5);
    for item in tier_one {
        let rs = uses(item);
        assert!(
            rs.iter().any(|r| r.level >= 40 || is_sink(r)),
            "{item:?} (tier 1) is a dead end at high level"
        );
    }
}

// ---- Building draws on other activities ----

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Prying,
    Harvesting,
    Foraging,
    Culturing,
    Excavating,
    Fighting,
    Diving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Activity {
    Gathering,
    Fighting,
    Diving,
    Digging,
}

fn activity(s: Source) -> Activity {
    match s {
        Source::Prying | Source::Harvesting | Source::Foraging | Source::Culturing => {
            Activity::Gathering
        }
        Source::Excavating => Activity::Digging,
        Source::Fighting => Activity::Fighting,
        Source::Diving => Activity::Diving,
    }
}

/// Where an item comes from; an item may have several sources (a deep
/// node's find is its skill's and a dive's; a processed item is made from
/// the sources of what it is made of, by its first recipe).
fn sources(item: Item, depth: u32) -> BTreeSet<Source> {
    let mut out = BTreeSet::new();
    for n in NODES.iter().filter(|n| n.yields == Some(item)) {
        out.insert(match n.skill {
            Skill::Prying => Source::Prying,
            Skill::Harvesting => Source::Harvesting,
            Skill::Foraging => Source::Foraging,
            Skill::Culturing => Source::Culturing,
            Skill::Excavating => Source::Excavating,
            other => panic!("{other:?} node"),
        });
        if n.tier >= 4 {
            out.insert(Source::Diving);
        }
    }
    match item {
        // Shucked or pried bivalves leave them.
        Item::Valve | Item::Pearl => {
            out.insert(Source::Prying);
        }
        // The deepest foraging.
        Item::Scale => {
            out.insert(Source::Foraging);
            out.insert(Source::Diving);
        }
        _ => {}
    }
    if all_drops().contains(&item) {
        out.insert(Source::Fighting);
        if item.tier() >= 4 {
            out.insert(Source::Diving);
        }
    }
    if out.is_empty() && depth < 8 {
        let made = ECON_RECIPES
            .iter()
            .find(|r| matches!(r.makes, Output::Items(o) if o.iter().any(|&(i, _)| i == item)));
        if let Some(r) = made {
            for &(i, _) in r.needs {
                out.extend(sources(i, depth + 1));
            }
        }
    }
    out
}

fn building() -> Vec<&'static EconRecipe> {
    ECON_RECIPES
        .iter()
        .filter(|r| r.skill == Skill::Building)
        .collect()
}

fn recipe_sources(r: &EconRecipe) -> BTreeSet<Source> {
    r.needs.iter().flat_map(|&(i, _)| sources(i, 0)).collect()
}

fn recipe_activities(r: &EconRecipe) -> BTreeSet<Activity> {
    recipe_sources(r).into_iter().map(activity).collect()
}

#[test]
fn every_item_a_building_recipe_needs_has_a_source() {
    for r in building() {
        for &(item, _) in r.needs {
            assert!(!sources(item, 0).is_empty(), "{}: {item:?}", r.word);
        }
    }
}

#[test]
fn building_above_level_ten_draws_on_two_activities() {
    println!("building recipe        lvl  activities");
    for r in building() {
        let acts = recipe_activities(r);
        println!("{:<22} {:>3}  {acts:?}", r.word, r.level);
        if r.level > 10 {
            assert!(acts.len() >= 2, "{} draws on {acts:?}", r.word);
        }
    }
}

#[test]
fn no_single_loop_supplies_every_building_recipe() {
    let recipes = building();
    for lone in [
        Source::Prying,
        Source::Harvesting,
        Source::Foraging,
        Source::Culturing,
        Source::Excavating,
        Source::Fighting,
        Source::Diving,
    ] {
        // Every item of every recipe from this one source alone?
        let all = recipes
            .iter()
            .all(|r| r.needs.iter().all(|&(i, _)| sources(i, 0).contains(&lone)));
        assert!(!all, "{lone:?} alone supplies every building recipe");
        // And the recipes above level 10 each need something it cannot give.
        let missing = recipes
            .iter()
            .filter(|r| r.level > 10)
            .filter(|r| r.needs.iter().any(|&(i, _)| !sources(i, 0).contains(&lone)))
            .count();
        assert!(missing > 0, "{lone:?}");
    }
}

#[test]
fn the_building_set_needs_all_four_activities() {
    let union: BTreeSet<Activity> = building().into_iter().flat_map(recipe_activities).collect();
    assert_eq!(
        union,
        BTreeSet::from([
            Activity::Gathering,
            Activity::Fighting,
            Activity::Diving,
            Activity::Digging
        ])
    );
    // Rare materials come from fighting, diving and digging, the core ones
    // from gathering and digging.
    let rare: BTreeSet<Activity> = building()
        .into_iter()
        .filter(|r| r.level >= 40)
        .flat_map(recipe_activities)
        .collect();
    assert!(rare.contains(&Activity::Fighting) && rare.contains(&Activity::Diving));
}

// ---- Low-attention gathering is safe and close ----

/// How near to a foe a crab standing or passing is in danger (the owner's
/// rule: danger guards the way, never the harvest, so a node is out of reach
/// of every foe). A turret that does not move (the snapping shrimp, the
/// moray in its crevice, the bobbit worm) strikes only within its strike
/// range, so keep that and a margin; the octopus is kept its notice range
/// off; a tethered walker (hermit crab, giant isopod, lobster) wanders and
/// hunts anywhere inside its leash, so keep the leash, its bite and a margin
/// (measured: an isopod shoved a crab 400 from its burrow); the hagfish
/// keeps a leash and an ink cloud; any other hunter that leaves its place
/// (the mantis, the open-water and midwater creatures, the amphipods) is
/// kept 600 units off, the brief's number (none has a smaller patrol radius
/// in the tuning).
fn danger(kind: crabsim::Kind, t: &Tuning) -> f32 {
    use crabsim::Kind;
    let e = kind.tuning(t);
    match kind {
        Kind::Octopus => e.notice_range + 20.0,
        Kind::HermitCrab | Kind::GiantIsopod | Kind::Lobster => {
            e.give_up_range + e.strike_range + 20.0
        }
        Kind::Hagfish => e.give_up_range + 140.0,
        Kind::GoliathGrouper | Kind::LoggerheadTurtle | Kind::HumboldtSquid => 600.0,
        _ if e.approach_speed == 0.0 => e.strike_range + 20.0,
        _ => 600.0,
    }
}

/// The tier 1 nodes on foot from home.
fn home_cluster() -> Vec<(NodeKind, f32)> {
    let level = Level::test_level();
    nodes(&level)
        .into_iter()
        .filter(|n| n.kind.def().tier == 1 && n.pos.x < 600.0)
        .map(|n| (n.kind, n.pos.x))
        .collect()
}

#[test]
fn no_foe_lies_near_the_home_cluster() {
    let level = Level::test_level();
    let t = Tuning::default();
    let cluster = home_cluster();
    assert!(cluster.len() >= 10, "{} nodes on foot", cluster.len());
    let mut foes: Vec<(crabsim::Kind, f32)> = level.foes.iter().map(|&(k, p)| (k, p.x)).collect();
    foes.extend(
        crabsim::creatures::spots(&level)
            .into_iter()
            .map(|(k, p)| (k, p.x)),
    );
    assert!(foes.len() >= 7);
    for (kind, fx) in foes {
        let keep = danger(kind, &t);
        for &(node, nx) in &cluster {
            assert!(
                (nx - fx).abs() >= keep,
                "{node:?} at x {nx} is {} from a {kind:?} at x {fx} (keep {keep})",
                (nx - fx).abs()
            );
        }
    }
}

#[test]
fn the_home_cluster_is_a_short_walk_from_the_door() {
    let level = Level::test_level();
    let t = Tuning::default();
    let door = level.home.max.x;
    let farthest = home_cluster()
        .iter()
        .map(|&(_, x)| x - door)
        .fold(0.0f32, f32::max);
    let secs = farthest / t.walk_speed;
    println!("farthest home node {farthest:.0} units from the door: {secs:.1} s");
    assert!(secs <= 40.0, "{secs:.1} s");
    // All five skills have a node in each pocket.
    for (skill_kind, _) in [
        (NodeKind::MusselClump, 0),
        (NodeKind::KelpStand, 0),
        (NodeKind::GritHeap, 0),
        (NodeKind::SnailRocks, 0),
        (NodeKind::ShoreGlow, 0),
    ] {
        let n = home_cluster().iter().filter(|c| c.0 == skill_kind).count();
        assert!(n >= 2, "{skill_kind:?}: {n} on foot");
    }
}

// ---- Danger guards the way, never the harvest ----

/// Every foe on the real level, from every spawn list the sim has: the
/// level's own (`foes` holds the older route, the bestiary, the exploration
/// habitat, the canyon's jellies and the octopus), the new creatures'
/// spots, and the moray crevices' dens. (The sixgill is a deep-band
/// resident named in `bands.rs`, not a spawn; the midwater creatures are in
/// `creatures::spots` and keep the 600 default.)
fn every_foe(level: &Level) -> Vec<(crabsim::Kind, bevy_math::Vec2)> {
    let mut foes: Vec<_> = level.foes.clone();
    foes.extend(crabsim::creatures::spots(level));
    foes.extend(level.crevices.iter().map(|c| (crabsim::Kind::Moray, c.den)));
    foes
}

#[test]
fn no_foe_lies_near_any_node_on_the_real_level() {
    let level = Level::test_level();
    let t = Tuning::default();
    let foes = every_foe(&level);
    let placed = nodes(&level);
    assert!(placed.len() >= 80, "{} nodes", placed.len());
    assert!(foes.len() >= 30, "{} foes", foes.len());
    let mut bad = Vec::new();
    for n in &placed {
        for &(kind, at) in &foes {
            let keep = danger(kind, &t);
            let d = n.pos.distance(at);
            if d < keep {
                bad.push(format!(
                    "{:?} at {:.0},{:.0} is {d:.0} from a {kind:?} at {:.0},{:.0} (keep {keep})",
                    n.kind, n.pos.x, n.pos.y, at.x, at.y
                ));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} calm-spot failures:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// Nothing in the gather rules interrupts a harvest except the crab's own
/// movement (walking, jumping, dodging, letting go, leaving the floor) and a
/// hit it takes (`economy/sim.rs`, the `cancel` list; the bar emptying on
/// each is shown in `economy_gather.rs`). No foe, timer or world event is on
/// that list. So with the real foes alive and the crab standing at each
/// placed node and holding Gather, nothing may touch it and the hold may
/// never be cancelled by anything but the crab.
#[test]
fn a_harvest_at_every_node_is_calm_with_the_real_foes_alive() {
    use crabsim::{Controls, PlayerId, Sim};
    let mut level = Level::test_level();
    // The vent field's plumes carry a crab; they are weather, not foes.
    level.currents.clear();
    let probe = Sim::new(Tuning::default(), level.clone());
    let count = probe.placed_nodes().len();
    assert!(count >= 80);
    for i in 0..count {
        let at = probe.placed_nodes()[i];
        let mut sim = Sim::new(Tuning::default(), level.clone());
        sim.populate();
        assert!(sim.enemies().len() >= 7, "foes spawned");
        let id = PlayerId(0);
        // A node in a sealed pocket (the burrow's gallery, the bone hall, the
        // seep garden) is worked from inside it: the cell window follows the
        // crab there, as in the game, and the crab must stand on its floor.
        let sealed = sim.level.pocket_at(at.pos).is_some();
        if sealed {
            sim.add_crab(id, at.pos);
            sim.enable_underwater_cells();
            sim.crab_mut(id).expect("crab").mover.pos = at.pos;
        } else {
            sim.add_crab(id, Vec2::new(at.pos.x, sim.level.floor_height(at.pos.x)));
        }
        let full = sim.crab(id).expect("crab").fighter.vitals.health;
        sim.set_controls(
            id,
            Controls {
                gather_held: true,
                ..Controls::default()
            },
        );
        // Twelve seconds of holding at the node.
        for _ in 0..1440 {
            if sealed {
                let p = sim.crab(id).expect("crab").mover.pos;
                let mut cells = sim.level.cells.take().expect("cells");
                cells.follow_player(&sim.level, p.x, p.y);
                sim.level.cells = Some(cells);
            }
            sim.tick();
            let c = sim.crab(id).expect("crab");
            assert!(
                !sealed || c.mover.pos.distance(at.pos) < 30.0,
                "node {i} {:?}: the crab left the pocket floor for {:?} (node at {:?})",
                at.kind,
                c.mover.pos,
                at.pos
            );
            assert!(
                c.fighter.vitals.health >= full && !c.events.hurt && !c.events.staggered,
                "node {i} {:?} at {:.0}: a harvest was touched (health {} of {full}, foes near: {:?})",
                at.kind,
                at.pos.x,
                c.fighter.vitals.health,
                sim.enemies()
                    .iter()
                    .filter(|e| e.pos.distance(at.pos) < 800.0)
                    .map(|e| (e.kind, e.pos.x as i32, e.pos.y as i32, e.phase(&sim.tuning)))
                    .collect::<Vec<_>>()
            );
        }
    }
}
