//! The economy's rules on the world: earning experience and the hooks the
//! other rules read. A child of `world` so it can reach the world's own
//! state; the gather and crafting runtimes build on these.

use crate::SimCrab;
use crate::economy::gather::{self, GatherBlocked, GatherProgress, GatherState};
use crate::economy::{Food, Item, LevelUp, NodeKind, PlacedNode};
use crate::home::Shelter;
use crate::net::{Msg, Net, To};
use crate::physics::{Carry, STEP};
use crate::shared::{Shared, Thing};
use crate::skills::Skill;
use crate::tuning::Tuning;
use crate::world::cues::{Cue, Cues};
use crate::world::{PlayerId, Sim};

/// What a crab's hold did this tick, for the world to finish.
enum Act {
    /// A work stroke in this skill.
    Stroke(Skill),
    /// A gather finished here (the host or solo took the charge): node.
    Done(usize),
    /// The vent pot cooked this food.
    Cooked(Food),
}

/// Experience for one block of sediment dug: a block is 4 by 4 cells
/// (16 world units) under the claws. Tuned in `tests/economy_dig.rs` so a
/// minute of steady tunnelling pays about what a minute of tier 1 gathering
/// does.
pub const DIG_BLOCK_XP: f32 = 1.0;
/// Keeps the second-item roll apart from the rare-find roll.
const DOUBLE_SALT: u64 = 0x5eed_d0b1_e000_0001;
/// How many dug blocks a crab remembers: digging a refilled spot again, or
/// digging in place, pays nothing while the block is remembered.
const DIG_MEMORY: usize = 96;

/// The blocks a crab dug lately and the new ones not yet paid.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct DigLedger {
    recent: Vec<(i32, i32)>,
    next: usize,
    owed: u32,
}

impl DigLedger {
    /// The claws at cell (`x`, `y`) dig toward (`dx`, `dy`) into sediment:
    /// a block not dug lately is owed experience.
    pub(crate) fn credit(&mut self, x: f32, y: f32, dx: i32, dy: i32) {
        let key = (
            ((x + dx as f32 * 3.0) / 4.0).floor() as i32,
            ((y + dy as f32 * 3.0) / 4.0).floor() as i32,
        );
        if self.recent.contains(&key) {
            return;
        }
        if self.recent.len() < DIG_MEMORY {
            self.recent.push(key);
        } else {
            self.recent[self.next] = key;
            self.next = (self.next + 1) % DIG_MEMORY;
        }
        self.owed = self.owed.saturating_add(1);
    }
}

impl Sim {
    /// Pay every local crab for the new blocks it dug this tick: Excavating
    /// by the unit, deterministic (no chance).
    pub(crate) fn dig_step(&mut self) {
        let tick = self.tick + 1;
        for c in self.crabs.iter_mut().filter(|c| !c.remote) {
            if c.dig.owed > 0 {
                let blocks = std::mem::take(&mut c.dig.owed);
                earn_crab(
                    c,
                    Skill::Excavating,
                    blocks as f32 * DIG_BLOCK_XP,
                    &self.tuning,
                    &mut self.sound_cues,
                    tick,
                );
            }
        }
    }

    /// Crab `id` earns `xp` experience in `skill`. Returns the last level
    /// it reached, if it went up (one [`Cue::LevelUp`] is sounded for every
    /// level crossed, and the crab's next events say the last one). Only a
    /// crab of this device earns: a remote crab, an unknown one or a bad
    /// amount changes nothing.
    pub fn earn(&mut self, id: PlayerId, skill: Skill, xp: f32) -> Option<LevelUp> {
        if !xp.is_finite() || xp <= 0.0 {
            return None;
        }
        let tick = self.tick + 1;
        let crab = self.crabs.iter_mut().find(|c| c.id == id && !c.remote)?;
        earn_crab(crab, skill, xp, &self.tuning, &mut self.sound_cues, tick)
    }

    /// The nodes in the level, in the order the shared node state and
    /// co-op messages name them.
    pub fn placed_nodes(&self) -> &[PlacedNode] {
        &self.placed_nodes
    }

    /// Percent faster gathering for crab `id` (from food).
    pub fn gather_speed_bonus(&self, id: PlayerId) -> u32 {
        self.crab(id).map_or(0, |c| c.econ.gather_speed_bonus())
    }

    /// What blows on crab `id` are multiplied by (plating).
    pub fn damage_taken_factor(&self, id: PlayerId) -> f32 {
        self.crab(id).map_or(1.0, |c| c.econ.damage_taken_factor())
    }

    /// What crab `id`'s claw blows are multiplied by (claw tips, food).
    pub fn claw_bonus(&self, id: PlayerId) -> f32 {
        self.crab(id).map_or(1.0, |c| c.econ.claw_bonus())
    }
}

/// Award already-validated local experience without borrowing unrelated world
/// state. Batch callers can keep crab order without an allocation or ID search.
fn earn_crab(
    crab: &mut SimCrab,
    skill: Skill,
    xp: f32,
    tuning: &Tuning,
    cues: &mut Cues,
    tick: u64,
) -> Option<LevelUp> {
    let (before, after) = if skill.economy_index().is_some() {
        crab.econ.skills.add(skill, xp)
    } else {
        let before = crab.mover.level(skill, tuning);
        crab.mover.earn(skill, xp, tuning);
        (before, crab.mover.level(skill, tuning))
    };
    let mut last = None;
    for level in before + 1..=after {
        cues.emit(
            tick,
            Cue::LevelUp {
                crab: crab.id,
                skill,
                level,
            },
        );
        last = Some(LevelUp { skill, level });
    }
    if last.is_some() {
        crab.pending.level_up = last;
    }
    last
}

impl Sim {
    /// The gather crab `id` is working, for the ring around the node.
    pub fn gather_state(&self, id: PlayerId) -> Option<GatherState> {
        let g = &self.crab(id)?.gather;
        let node = g.node?;
        (g.need > 0).then(|| GatherState {
            node,
            progress: (g.ticks as f32 / g.need as f32).clamp(0.0, 1.0),
        })
    }

    /// One tick of every local crab's gathering and of Survival.
    pub(crate) fn gather_step(&mut self) {
        let tick = self.tick + 1;
        let mut acts = Vec::new();
        {
            let Self {
                crabs,
                shared,
                placed_nodes,
                net,
                tuning,
                ..
            } = self;
            for c in crabs.iter_mut().filter(|c| !c.remote) {
                if let Some(act) = gather_crab(c, shared, placed_nodes, net, tuning) {
                    acts.push((c.id, act));
                }
            }
        }
        for (id, act) in acts {
            match act {
                Act::Stroke(skill) => {
                    self.sound_cues.emit(tick, Cue::Gather { crab: id, skill });
                }
                Act::Done(node) => self.finish_node(id, node, true),
                Act::Cooked(food) => {
                    let xp = gather::cook_xp(food);
                    if let Some(c) = self.crabs.iter_mut().find(|c| c.id == id && !c.remote) {
                        c.gather.reset();
                        c.gather.blocked_shown = false;
                        let item = match food {
                            Food::VentClamFeast => Item::VentClam,
                            _ => Item::SeepMussel,
                        };
                        c.events.gathered = Some((item, xp));
                    }
                    self.earn(id, Skill::Cooking, xp as f32);
                    self.sound_cues.emit(tick, Cue::Gathered { crab: id });
                }
            }
        }
        self.survival_step();
    }

    /// Survival: per second in a band of twilight or deeper, band index
    /// minus one experience, twice that while the shell strains hard.
    fn survival_step(&mut self) {
        let mut earned = Vec::new();
        for c in self.crabs.iter().filter(|c| !c.remote) {
            if c.fighter.dead.is_some() {
                continue;
            }
            let band = crate::economy::place::band_index(&self.level, c.mover.pos);
            if band >= 2 {
                let hard = if c.strain > 0.5 { 2.0 } else { 1.0 };
                // The first hours pay Survival more (`ladder::survival_xp_percent`).
                let level = crate::economy::xp::level_for(c.econ.skills.get(Skill::Survival));
                let early = crate::economy::ladder::survival_xp_percent(level) as f32 / 100.0;
                earned.push((c.id, (band - 1) as f32 * hard * STEP * early));
            }
        }
        for (id, xp) in earned {
            self.earn(id, Skill::Survival, xp);
        }
    }

    /// Crab `id`'s gather of node `node` is done and the charge is its
    /// own (`in_tick`: it finished in this tick; otherwise the host's word
    /// came between ticks): the item, the experience, the wear of the tool,
    /// a rare find.
    pub(crate) fn finish_node(&mut self, id: PlayerId, node: usize, in_tick: bool) {
        let Some(def) = self.placed_nodes.get(node).map(|n| n.kind.def()) else {
            return;
        };
        let tuning = &self.tuning;
        let Some(crab) = self.crabs.iter_mut().find(|c| c.id == id && !c.remote) else {
            return;
        };
        crab.gather.reset();
        crab.gather.blocked_shown = false;
        let cracked = gather::wear_tool(crab, def);
        let identity = crab.salvage_identity.wrapping_add(u64::from(crab.id.0));
        let done = crab.econ.gathers_done;
        crab.econ.gathers_done = done.saturating_add(1);
        let level = crab.level(def.skill, tuning);
        let xp_paid = crate::economy::ladder::node_xp(def, level);
        let rare = gather::rare_find_at(def, identity, node, done, level);
        // A second item from a tier 1 gather: a personal roll, and the
        // node's shared charge is untouched.
        let second = def.tier == 1
            && def.yields.is_some()
            && crate::economy::roll(identity ^ DOUBLE_SALT, node as u64, done) % 100
                < u64::from(crate::economy::ladder::double_yield_percent(
                    def.skill, level,
                ));
        let ev = if in_tick {
            &mut crab.events
        } else {
            &mut crab.pending
        };
        if let Some(item) = def.yields {
            let carry = Carry::Item(item);
            if crab.mover.take_up(carry, tuning) {
                ev.picked_up = ev.picked_up.or(Some(carry));
            } else {
                ev.shell_full |= carry.small();
            }
            ev.gathered = Some((item, xp_paid));
        }
        if let Some(item) = def.yields {
            crab.econ
                .guide
                .note(crate::economy::guide::Note::Gathered(item));
        }
        if second && let Some(item) = def.yields {
            // Room permitting; otherwise the extra is simply not taken.
            let _ = crab.mover.take_up(Carry::Item(item), tuning);
        }
        if let Some(item) = rare {
            let carry = Carry::Item(item);
            if crab.mover.take_up(carry, tuning) {
                ev.picked_up = ev.picked_up.or(Some(carry));
            }
        }
        ev.tool_cracked = ev.tool_cracked.or(cracked);
        let tick = self.tick + 1;
        if cracked.is_some() {
            self.sound_cues.emit(tick, Cue::ToolCracked { crab: id });
        }
        self.earn(id, def.skill, xp_paid as f32);
        self.sound_cues.emit(tick, Cue::Gathered { crab: id });
    }
}

/// One local crab's hold, one tick.
fn gather_crab(
    c: &mut SimCrab,
    shared: &mut Shared,
    placed: &[PlacedNode],
    net: &mut Option<Net>,
    t: &Tuning,
) -> Option<Act> {
    let health = c.fighter.vitals.health;
    let hurt = c.gather.last_health > 0.0 && health < c.gather.last_health - 1.0e-4;
    c.gather.last_health = health;
    let ctl = c.controls;
    if !ctl.gather_held || c.fighter.dead.is_some() {
        c.gather.reset();
        c.gather.blocked_shown = false;
        return None;
    }
    let m = &c.mover;
    let ev = c.events;
    let standing = (m.grounded || m.climbing)
        && !m.is_soft()
        && m.shelter == Shelter::Out
        && c.fighter.vitals.hitstop == 0;
    let cancel = !standing
        || hurt
        || ev.hurt
        || ev.staggered
        || ev.jumped
        || ev.dodged
        || ctl.jump_held
        || ctl.dodge_held
        || m.dodge_left > 0.0
        || ctl.walk.abs() > 0.2;
    if cancel {
        c.gather.reset();
        return None;
    }
    // The nearest ready node within reach of its feet.
    let mut best: Option<(usize, f32)> = None;
    let mut best_workable = false;
    let mut spent_near = false;
    for (i, p) in placed.iter().enumerate() {
        let d = p.pos.distance(m.pos);
        if d > gather::REACH {
            continue;
        }
        match shared.nodes.get(i) {
            Some(n) if n.charges > 0 => {
                // A node the crab can work beats a nearer one it cannot (a
                // level 8 node beside a level 1 one): so the pockets can hold
                // both without the near one blocking the hold.
                let workable = gather::check(c, p.kind, t).is_ok();
                if workable && !best_workable {
                    best = Some((i, d));
                    best_workable = true;
                } else if workable == best_workable && best.is_none_or(|b| d < b.1) {
                    best = Some((i, d));
                }
            }
            Some(_) => spent_near = true,
            None => {}
        }
    }
    let Some((i, _)) = best else {
        c.gather.reset();
        if spent_near && !c.gather.blocked_shown {
            c.gather.blocked_shown = true;
            c.events.gather_blocked = Some(GatherBlocked::Spent);
        }
        return None;
    };
    let kind = placed[i].kind;
    let thing = Thing::Node(i);
    if net
        .as_ref()
        .is_some_and(|n| !n.host && n.asked.contains(&(c.id, thing)))
    {
        // Asked the host; the bar waits full for its word.
        return None;
    }
    if c.gather.node != Some(i) {
        match gather::check(c, kind, t) {
            Err(why) => {
                c.gather.reset();
                if !c.gather.blocked_shown {
                    c.gather.blocked_shown = true;
                    c.events.gather_blocked = Some(why);
                }
            }
            Ok(plan) => {
                c.gather = GatherProgress {
                    node: Some(i),
                    ticks: 0,
                    need: plan.need,
                    cook: plan.cook,
                    ..c.gather
                };
            }
        }
        return None;
    }
    c.gather.ticks += 1;
    if c.gather.ticks < c.gather.need {
        return c
            .gather
            .ticks
            .is_multiple_of(gather::STROKE_TICKS)
            .then(|| Act::Stroke(kind.def().skill));
    }
    // Done. Is it still as it was when the hold started?
    let plan = match gather::check(c, kind, t) {
        Ok(plan) => plan,
        Err(why) => {
            c.gather.reset();
            c.gather.blocked_shown = true;
            c.events.gather_blocked = Some(why);
            return None;
        }
    };
    if kind == NodeKind::VentPot {
        // The station never runs dry: cook what the shell holds.
        let food = plan.cook.filter(|&food| gather::cook(c, food))?;
        return Some(Act::Cooked(food));
    }
    if !shared.there(thing, t) {
        c.gather.reset();
        return None;
    }
    match net {
        Some(n) if !n.host => {
            n.asked.push((c.id, thing));
            n.send(To::Host, Msg::Take { by: c.id, thing });
            None
        }
        _ => {
            shared.take(thing, t);
            if let Some(n) = net {
                n.send(
                    To::Everyone,
                    Msg::Taken {
                        by: Some(c.id),
                        thing,
                    },
                );
            }
            Some(Act::Done(i))
        }
    }
}

#[cfg(test)]
mod payout_tests {
    use super::*;
    use bevy_math::Vec2;

    fn fixture() -> Sim {
        let mut sim = Sim::with_one_crab(Vec2::ZERO);
        for id in 1..8 {
            sim.add_crab(PlayerId(id), Vec2::ZERO);
        }
        sim.crabs[3].remote = true;
        for (i, c) in sim.crabs.iter_mut().enumerate() {
            c.dig.owed = [0, 1, 83, 400, 2000, 0, 90, 1][i];
        }
        sim
    }

    // The former two-stage algorithm is the behavioral oracle: all debts
    // cleared first, then awards in crab order, with remote debt untouched.
    fn reference(sim: &mut Sim) {
        let mut pay = Vec::new();
        for c in sim.crabs.iter_mut().filter(|c| !c.remote) {
            if c.dig.owed > 0 {
                pay.push((c.id, c.dig.owed));
                c.dig.owed = 0;
            }
        }
        for (id, blocks) in pay {
            sim.earn(id, Skill::Excavating, blocks as f32 * DIG_BLOCK_XP);
        }
    }

    #[test]
    fn dig_payout_preserves_order_remote_debt_and_exactly_once_awards() {
        let mut actual = fixture();
        let mut expected = fixture();
        for _ in 0..2 {
            actual.dig_step();
            reference(&mut expected);
            assert_eq!(actual.state_hash(), expected.state_hash());
            assert_eq!(actual.cues(), expected.cues());
            for (a, e) in actual.crabs.iter().zip(&expected.crabs) {
                assert_eq!(a.dig, e.dig);
                assert_eq!(a.pending.level_up, e.pending.level_up);
            }
        }
        assert_eq!(actual.crabs[3].dig.owed, 400);
    }

    #[test]
    #[ignore = "manual matched release microbenchmark, not a timing gate"]
    fn dig_payout_budget() {
        use std::hint::black_box;
        use std::time::Instant;
        for count in [1, 8] {
            for sample in 0..5 {
                for (name, payout) in [
                    ("reference", reference as fn(&mut Sim)),
                    ("borrowed", Sim::dig_step),
                ] {
                    let mut sim = fixture();
                    sim.crabs.truncate(count);
                    let start = Instant::now();
                    for _ in 0..100_000 {
                        for c in &mut sim.crabs {
                            c.dig.owed = black_box(1);
                        }
                        payout(black_box(&mut sim));
                    }
                    println!(
                        "{count} crabs {sample} {name}: {:.1} ns/payout hash={:x}",
                        start.elapsed().as_nanos() as f64 / 100_000.0,
                        sim.state_hash()
                    );
                }
            }
        }
    }
}
