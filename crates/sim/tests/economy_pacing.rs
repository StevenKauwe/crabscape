//! Pacing: how fast the gathering skills level on real ticks near home, and
//! a model of the deeper tiers from measured gather times and route times.
//! See the spec's "Pacing targets".

mod common;

use bevy_math::Vec2;
use crabsim::economy::place::{SPOTS, band_index, nodes};
use crabsim::economy::{Item, NodeKind, Tool, ToolKind, xp};
use crabsim::{Controls, Level, PlayerId, Sim, Skill, Tuning};

const P0: PlayerId = PlayerId(0);

/// Past here the floor climbs to the plateau: not on foot.
const FOOT_X: f32 = 600.0;

/// A scripted crab on real ticks: gathers the nearest ready node on foot,
/// walks home when the shell is full, walks into the doorway to put the
/// load away, walks out and goes again.
struct Bot {
    sim: Sim,
    skill: Skill,
    kinds: Vec<usize>,
}

fn level_of_skill(sim: &Sim, skill: Skill) -> u32 {
    xp::level_for(sim.crab(P0).unwrap().econ.skills.get(skill))
}

impl Bot {
    fn new(skill: Skill, kind: NodeKind) -> Bot {
        Bot::on(skill, &[kind])
    }

    /// A bot working every foot node of these kinds.
    fn on(skill: Skill, wanted: &[NodeKind]) -> Bot {
        Bot::with_foes(skill, wanted, false)
    }

    /// The same, with the level's own foes in place if `foes`.
    fn with_foes(skill: Skill, wanted: &[NodeKind], foes: bool) -> Bot {
        let mut level = Level::test_level();
        if !foes {
            level.foes.clear();
        }
        level.currents.clear();
        // The old world's clams and grit lie on the way home; not this test.
        level.clams.clear();
        level.grit.clear();
        let mut tuning = Tuning::default();
        tuning.memory.step_ticks = u32::MAX;
        let mut sim = Sim::new(tuning, level);
        let home = sim.level.home;
        let x = home.max.x + 20.0;
        sim.add_crab(P0, Vec2::new(x, sim.level.floor_height(x)));
        // The old world's clam beds, teeth and sponges are not this test.
        let crab = sim.crab_mut(P0).unwrap();
        crab.eaten.iter_mut().for_each(|e| *e = true);
        crab.teeth_taken.iter_mut().for_each(|e| *e = true);
        crab.sponges_taken.iter_mut().for_each(|e| *e = true);
        crab.cultures_taken.iter_mut().for_each(|e| *e = true);
        let kinds = sim
            .placed_nodes()
            .iter()
            .enumerate()
            .filter(|(_, n)| wanted.contains(&n.kind) && n.pos.x < FOOT_X)
            .map(|(i, _)| i)
            .collect();
        Bot { sim, skill, kinds }
    }

    fn step(&mut self, c: Controls) {
        self.sim.set_controls(P0, c);
        self.sim.tick();
    }

    fn secs(&self) -> f64 {
        self.sim.tick_count() as f64 / 120.0
    }

    /// Play until `skill` reaches each of `targets`; sim seconds for each.
    fn run(&mut self, targets: &[u32], limit_secs: f64) -> Vec<Option<f64>> {
        let mut out = vec![None; targets.len()];
        let mut going_home = false;
        while self.secs() < limit_secs && out.iter().any(Option::is_none) {
            let lvl = level_of_skill(&self.sim, self.skill);
            for (k, t) in targets.iter().enumerate() {
                if out[k].is_none() && lvl >= *t {
                    out[k] = Some(self.secs());
                }
            }
            let crab = self.sim.crab(P0).unwrap();
            let m = &crab.mover;
            let full = m.clinging.len() >= m.carries(&self.sim.tuning);
            if m.tucked_in() {
                // Deposited (the shell empties when tucked in): walk out.
                if !m
                    .clinging
                    .iter()
                    .any(|c| matches!(c, crabsim::Carry::Item(_)))
                {
                    going_home = false;
                    self.step(Controls {
                        walk: 1.0,
                        ..Controls::default()
                    });
                } else {
                    self.step(Controls::default());
                }
                continue;
            }
            if m.shelter != crabsim::home::Shelter::Out {
                self.step(Controls {
                    walk: if going_home { -1.0 } else { 1.0 },
                    ..Controls::default()
                });
                continue;
            }
            if full {
                going_home = true;
            }
            if going_home {
                self.step(Controls {
                    walk: -1.0,
                    ..Controls::default()
                });
                continue;
            }
            // Nearest ready node.
            let px = m.pos.x;
            let target = self
                .kinds
                .iter()
                .copied()
                .filter(|&i| self.sim.shared.nodes[i].charges > 0)
                .min_by(|&a, &b| {
                    let (xa, xb) = (
                        self.sim.placed_nodes()[a].pos.x,
                        self.sim.placed_nodes()[b].pos.x,
                    );
                    (xa - px).abs().total_cmp(&(xb - px).abs())
                });
            let Some(i) = target else {
                // All spent: wait where we are.
                self.step(Controls::default());
                continue;
            };
            let dx = self.sim.placed_nodes()[i].pos.x - px;
            if dx.abs() > 6.0 {
                self.step(Controls {
                    walk: dx.signum(),
                    ..Controls::default()
                });
            } else {
                self.step(Controls {
                    gather_held: true,
                    ..Controls::default()
                });
            }
        }
        out
    }
}

/// The five skills with a tier 1 node on the home side, and its kind.
const SKILLS: [(Skill, NodeKind); 5] = [
    (Skill::Prying, NodeKind::MusselClump),
    (Skill::Harvesting, NodeKind::KelpStand),
    (Skill::Excavating, NodeKind::GritHeap),
    (Skill::Foraging, NodeKind::SnailRocks),
    (Skill::Culturing, NodeKind::ShoreGlow),
];

/// Sim seconds to levels 2, 5, 10, 15 for each skill, by the bot.
fn bot_times() -> Vec<(Skill, Vec<Option<f64>>)> {
    SKILLS
        .iter()
        .map(|&(skill, kind)| {
            let mut bot = Bot::new(skill, kind);
            (skill, bot.run(&[2, 5, 10, 15], 3000.0))
        })
        .collect()
}

#[test]
fn a_new_crab_levels_every_gathering_skill_at_the_spec_pace() {
    // Spec bounds in seconds: first level, 5, 10, 15.
    let bounds = [90.0, 6.0 * 60.0, 15.0 * 60.0, 35.0 * 60.0];
    println!("skill        L2     L5     L10    L15 (sim seconds, bot on real ticks)");
    for (skill, times) in bot_times() {
        let shown: Vec<String> = times
            .iter()
            .map(|t| t.map_or("never".into(), |t| format!("{t:6.0}")))
            .collect();
        println!("{skill:<12?} {}", shown.join(" "));
        for (k, (t, bound)) in times.iter().zip(bounds).enumerate() {
            let level = [2, 5, 10, 15][k];
            let t = t.unwrap_or_else(|| panic!("{skill:?} never reached level {level}"));
            assert!(
                t <= bound,
                "{skill:?} level {level}: {t:.0} s > {bound:.0} s"
            );
        }
    }
}

use common::route::{self, Kit};

const FULL_KIT: Kit = Kit {
    eggs: 6,
    scissors: true,
    glass: true,
    crusher: true,
    dactyls: true,
};

/// Trips from home, measured with the route solver and the full kit: per
/// tier, the seconds out and the seconds back to the nearest node of the tier
/// (where a player farms), and the seconds out to its median node (the
/// ordering check). Tier 5 (the vent field, past the narrows and the slump
/// dam) is beyond the solver: it is extrapolated at the pace the solver
/// measured for tier 4 over the extra distance. Re-measure with
/// `cargo test --release -p crabsim --test economy_pacing -- --ignored --nocapture measure_routes`
/// (about 15 minutes) and paste the printed `TRIPS` line.
const TRIPS: [(f64, f64, f64); 5] = [
    (0.0, 0.0, 4.83),
    (24.17, 69.93, 27.22),
    (96.42, 66.89, 110.33),
    (110.67, 75.73, 113.72),
    (0.0, 0.0, 0.0),
];

/// Solve the routes: one full kit survey from home, the seconds out to every
/// node, the way back from the nearest node of each tier. Every node west of
/// the vent field must be reached.
#[test]
#[ignore = "about 15 minutes of route solving; prints TRIPS"]
fn measure_routes() {
    let placed = nodes(&Level::test_level());
    let kit = FULL_KIT;
    let s = route::survey(kit);
    let out_time = |p: Vec2| {
        (0..s.nodes.len())
            .filter(|&j| s.nodes[j].mover().pos.distance(p) <= 70.0)
            .map(|j| (s.nodes[j].time, j))
            .min_by(|a, b| a.0.total_cmp(&b.0))
    };
    let mut trips = [(0.0f64, 0.0f64, 0.0f64); 5];
    let mut nearest = Vec::new();
    let mut unreached = Vec::new();
    for tier in 1..=5u8 {
        let mut reached = Vec::new();
        for n in &placed {
            let d = n.kind.def();
            if d.tier != tier || d.skill == Skill::Cooking {
                continue;
            }
            match out_time(n.pos) {
                Some((t, j)) => reached.push((t, j, n.pos.x, n.kind)),
                None => unreached.push((n.kind, n.pos.x)),
            }
        }
        reached.sort_by(|a, b| a.0.total_cmp(&b.0));
        println!("tier {tier}: solver reaches {}", reached.len());
        if let Some(&(t, j, x, kind)) = reached.first() {
            let median = reached[reached.len() / 2].0;
            println!("  nearest: {kind:?} x{x:.0} out {t:.1}; median out {median:.1}");
            trips[tier as usize - 1] = (t, t, median);
            nearest.push((tier, j, t));
        }
    }
    // The way back from the nearest node of each tier (tier 1 lies on the
    // home side: the same way back).
    let backs: Vec<_> = std::thread::scope(|sc| {
        let jobs: Vec<_> = nearest
            .iter()
            .filter(|n| n.0 > 1)
            .map(|&(tier, j, t)| {
                let s = &s;
                sc.spawn(move || (tier, route::way_home(s, j, kit).map(|h| h.time - t)))
            })
            .collect();
        jobs.into_iter()
            .map(|j| j.join().expect("a way home"))
            .collect()
    });
    for (tier, back) in backs {
        println!("  tier {tier} way back: {back:?}");
        trips[tier as usize - 1].1 = back.unwrap_or(f64::NAN);
    }
    println!("TRIPS = {trips:?}");
    println!("not reached: {unreached:?}");
    assert!(
        unreached.iter().all(|(_, x)| *x >= 17400.0),
        "nodes the solver cannot reach with the full kit: {unreached:?}"
    );
}

// ---- The model for the deeper tiers ----

/// The Prying row of each tier (every skill shares the numbers but Culturing's
/// experience).
const TIER_KINDS: [NodeKind; 5] = [
    NodeKind::MusselClump,
    NodeKind::ClamBed,
    NodeKind::ScallopBed,
    NodeKind::SeepMusselBed,
    NodeKind::VentClamBed,
];
/// Shell slots of a size 1 crab: `Tuning::cling`.
const SHELL: u32 = 4;
const KELP_SACK: u32 = 2;
const WOVEN_PACK: u32 = 4;
/// Pry, scoop and cutter tips of nodules: tool tier 5.
const NODULE_TOOL: u8 = 5;

/// Seconds of one round trip to tier `tier` (1 to 5): the measured out and
/// back, plus tucking in and out.
fn trip_secs(tier: u8) -> f64 {
    let t = Tuning::default();
    let tuck = f64::from(t.home_in_time + t.home_out_time);
    if tier == 1 {
        // On foot from the den to the middle of the home side cluster (the
        // solver starts at x 300, not at the door).
        let mut xs: Vec<f32> = SPOTS
            .iter()
            .filter(|s| {
                s.kind.def().tier == 1 && s.kind.def().skill != Skill::Cooking && s.x < FOOT_X
            })
            .map(|s| s.x)
            .collect();
        xs.sort_by(f32::total_cmp);
        let way = xs[xs.len() / 2] - Level::test_level().home_den.x;
        return 2.0 * f64::from(way / t.walk_speed) + tuck;
    }
    let (out, back, _) = TRIPS[tier as usize - 1];
    if tier < 5 {
        return out + back + tuck;
    }
    // Beyond the solver: the pace of tier 4 over the longer way.
    let mid = |tier: u8| {
        let mut xs: Vec<f32> = SPOTS
            .iter()
            .filter(|s| s.kind.def().tier == tier && s.kind.def().skill != Skill::Cooking)
            .map(|s| s.x)
            .collect();
        xs.sort_by(f32::total_cmp);
        f64::from(xs[xs.len() / 2] - 150.0)
    };
    let (out4, back4, _) = TRIPS[3];
    (out4 + back4) * mid(5) / mid(4) + tuck
}

/// Seconds of one gather at the tier's node for a crab of `level` with a tool
/// of `tool_tier` (the function the sim uses).
fn gather_secs(tier: u8, level: u32, tool_tier: u8) -> f64 {
    let def = TIER_KINDS[tier as usize - 1].def();
    f64::from(def.gather_ticks(level, tool_tier, 0)) / 120.0
}

/// Round-trip experience per minute, and items per minute, gathering at
/// `tier` with `slots` on the shell.
fn round_trip(tier: u8, level: u32, tool_tier: u8, slots: u32) -> (f64, f64) {
    let def = TIER_KINDS[tier as usize - 1].def();
    let cap = f64::from(slots);
    let secs = trip_secs(tier) + cap * gather_secs(tier, level, tool_tier);
    (cap * 60.0 / secs, cap * f64::from(def.xp) * 60.0 / secs)
}

/// Tier 1 "power-gathering": items dropped as they come, so only the nodes
/// bind: `nodes` of one skill's tier 1 kind, all charges worked, then the
/// wait for the last to regrow.
fn power_gather(level: u32, tool_tier: u8, nodes: u32) -> f64 {
    let def = TIER_KINDS[0].def();
    let g = gather_secs(1, level, tool_tier);
    let charges = f64::from(nodes) * f64::from(def.charges);
    let cycle = (charges * g).max(f64::from(def.respawn_secs) + f64::from(def.charges) * g);
    charges * f64::from(def.xp) * 60.0 / cycle
}

/// How many tier 1 nodes of one kind can be reached on foot from home.
fn foot_nodes() -> u32 {
    SKILLS
        .iter()
        .map(|&(_, kind)| {
            SPOTS
                .iter()
                .filter(|s| s.kind == kind && s.x < FOOT_X)
                .count() as u32
        })
        .max()
        .unwrap_or(0)
}

/// The best bag at a stage: none for tier 1, the kelp sack for 2, the woven
/// pack for 3 and up.
fn bag_for(tier: u8) -> u32 {
    match tier {
        1 => 0,
        2 => KELP_SACK,
        _ => WOVEN_PACK,
    }
}

#[test]
fn experience_per_minute_rises_with_tier_and_beats_power_gathering() {
    println!("tier  trip s  items/min  xp/min   (at the unlock level, tier tool, best bag)");
    let mut at_unlock = Vec::new();
    for tier in 1..=5u8 {
        let def = TIER_KINDS[tier as usize - 1].def();
        let (items, xp) = round_trip(tier, def.level, tier - 1, SHELL + bag_for(tier));
        println!(
            "{tier:>4}  {:>6.0}  {items:>9.1}  {xp:>6.0}",
            trip_secs(tier)
        );
        at_unlock.push(xp);
    }
    println!("tier  trip s  items/min  xp/min   (level 60, nodule tools, woven pack)");
    let mut at_sixty = Vec::new();
    for tier in 1..=5u8 {
        let (items, xp) = round_trip(tier, 60, NODULE_TOOL, SHELL + WOVEN_PACK);
        println!(
            "{tier:>4}  {:>6.0}  {items:>9.1}  {xp:>6.0}",
            trip_secs(tier)
        );
        at_sixty.push(xp);
    }
    let power_new = power_gather(1, 0, foot_nodes());
    let power_sixty = power_gather(60, NODULE_TOOL, foot_nodes());
    println!(
        "power-gathering tier 1 (node-bound, {} nodes): {power_new:.0} xp/min new, {power_sixty:.0} at level 60",
        foot_nodes()
    );
    for xp in [&at_unlock, &at_sixty] {
        assert!(xp.windows(2).all(|w| w[0] < w[1]), "not rising: {xp:?}");
    }
    let t3 = round_trip(3, 30, 2, SHELL + WOVEN_PACK).1;
    assert!(power_new < t3, "{power_new} vs {t3}");
    assert!(
        power_sixty < at_sixty[2],
        "{power_sixty} vs {}",
        at_sixty[2]
    );
}

/// Estimated hours to levels 30, 45, 60, 99 at the best tier open at each
/// level (tier tool; nodule tools from 60; the best bag then).
#[test]
fn estimated_hours_to_the_milestones_are_printed() {
    let mut secs = 0.0;
    let mut last = 0.0;
    let mut out = Vec::new();
    for level in 1..99u32 {
        let tier = (1..=5u8)
            .rev()
            .find(|&t| TIER_KINDS[t as usize - 1].def().level <= level)
            .unwrap_or(1);
        let tool = if level >= 60 { NODULE_TOOL } else { tier - 1 };
        let (_, xp_min) = round_trip(tier, level, tool, SHELL + bag_for(tier));
        let need = f64::from(xp::xp_for_level(level + 1) - xp::xp_for_level(level));
        secs += need / xp_min * 60.0;
        if [30, 45, 60, 99].contains(&(level + 1)) {
            out.push((level + 1, secs / 3600.0));
            assert!(secs > last);
            last = secs;
        }
    }
    println!("hours to level (one gathering skill, best tier open):");
    for (level, hours) in &out {
        println!("  {level:>3}: {hours:>7.1} h");
    }
}

#[test]
fn every_node_lies_in_a_band_no_shallower_than_its_tier_and_trips_lengthen_with_tier() {
    let level = Level::test_level();
    for n in nodes(&level) {
        let def = n.kind.def();
        if def.skill == Skill::Cooking {
            continue;
        }
        assert!(
            band_index(&level, n.pos) >= usize::from(def.tier) - 1,
            "{:?} (tier {}) at {:?} lies in band {}",
            n.kind,
            def.tier,
            n.pos,
            band_index(&level, n.pos)
        );
    }
    let trips: Vec<f64> = (1..=5).map(trip_secs).collect();
    assert!(trips.windows(2).all(|w| w[0] < w[1]), "{trips:?}");
    let medians: Vec<f64> = TRIPS.iter().take(4).map(|t| t.2).collect();
    assert!(medians.windows(2).all(|w| w[0] < w[1]), "{medians:?}");
}

// ---- Two hours of low-attention gathering near home ----

/// What a session at the home cluster earned.
struct Session {
    xp: u32,
    levels: u32,
    items: u32,
}

/// A bot working `wanted` for `secs` sim seconds from skill level `start`
/// (every gathering skill at that level, tools of tier `tool`), watching
/// only the full shell. The XP is summed over the five gathering skills.
fn session(start: u32, tool: u8, wanted: &[NodeKind], secs: f64) -> Session {
    let mut bot = Bot::on(Skill::Prying, wanted);
    let from = xp::TABLE[start as usize];
    let crab = bot.sim.crab_mut(P0).expect("crab");
    for (skill, _) in SKILLS {
        crab.econ.skills.set(skill, from);
    }
    if tool > 0 {
        for kind in ToolKind::ALL {
            crab.econ.tools[kind.index()] = Tool::new(kind, tool);
        }
    }
    bot.run(&[99], secs);
    let crab = bot.sim.crab(P0).expect("crab");
    let book = &crab.econ.skills;
    let total: u32 = SKILLS.iter().map(|&(s, _)| book.get(s) - from).sum();
    let levels: u32 = SKILLS
        .iter()
        .map(|&(s, _)| xp::level_for(book.get(s)) - start)
        .sum();
    let items: u32 = Item::ALL.iter().map(|&i| crab.stash.count(i)).sum();
    Session {
        xp: total,
        levels,
        items,
    }
}

const SESSION_SECS: f64 = 2.0 * 3600.0;

#[test]
fn two_hours_beside_the_door_is_never_pointless() {
    let mussels = [NodeKind::MusselClump];
    let every: Vec<NodeKind> = SKILLS.iter().map(|&(_, k)| k).collect();
    let hours = SESSION_SECS / 3600.0;
    println!("2 h at the home cluster (watching only the full shell):");
    println!("crab        nodes        xp/h    levels  items/h");
    let new = session(1, 0, &mussels, SESSION_SECS);
    let new_all = session(1, 0, &every, SESSION_SECS);
    let old = session(60, NODULE_TOOL, &mussels, SESSION_SECS);
    let old_all = session(60, NODULE_TOOL, &every, SESSION_SECS);
    for (name, nodes, s) in [
        ("level 1", "mussels", &new),
        ("level 1", "all five", &new_all),
        ("level 60", "mussels", &old),
        ("level 60", "all five", &old_all),
    ] {
        println!(
            "{name:<10}  {nodes:<9} {:>7.0}  {:>6}  {:>7.0}",
            f64::from(s.xp) / hours,
            s.levels,
            f64::from(s.items) / hours
        );
    }
    // A new crab on one node kind still climbs far: level 20 (4,470 xp).
    let reached = xp::level_for(xp::TABLE[1] + new.xp);
    assert!(reached >= 20, "level {reached} after 2 h on mussels");
    // The old crab keeps a quarter of what a dive to tier 3 pays a minute.
    let t3 = round_trip(3, 60, NODULE_TOOL, SHELL + WOVEN_PACK).1;
    let per_min = f64::from(old.xp) / (SESSION_SECS / 60.0);
    println!("level 60: {per_min:.0} xp/min on mussels vs {t3:.0} at tier 3");
    assert!(per_min >= 0.25 * t3, "{per_min:.0} vs {t3:.0}");
    // But never as much as the attentive dive: tier 1 stays below tier 3.
    assert!(
        f64::from(old_all.xp) / (SESSION_SECS / 60.0) / 5.0 < t3,
        "per skill"
    );
    assert!(per_min < t3, "{per_min:.0} vs {t3:.0}");
    // Something is banked either way.
    assert!(new.items > 100 && old.items > 100);
}

#[test]
fn the_real_foes_never_touch_a_crab_working_the_home_cluster() {
    let every: Vec<NodeKind> = SKILLS.iter().map(|&(_, k)| k).collect();
    let mut bot = Bot::with_foes(Skill::Prying, &every, true);
    assert!(!bot.sim.level.foes.is_empty());
    bot.sim.populate();
    assert!(bot.sim.enemies().len() >= 7, "foes spawned");
    let full = bot.sim.crab(P0).expect("crab").fighter.vitals.health;
    // Half an hour, a minute at a time, the shell watched between.
    for minute in 1..=30 {
        bot.run(&[99], f64::from(minute) * 60.0);
        let crab = bot.sim.crab(P0).expect("crab");
        assert!(
            crab.fighter.vitals.health >= full,
            "hit in minute {minute}: {} of {full}",
            crab.fighter.vitals.health
        );
    }
    let book = &bot.sim.crab(P0).expect("crab").econ.skills;
    assert!(
        SKILLS.iter().all(|&(s, _)| book.get(s) > 0),
        "all five worked"
    );
}
