//! Culture beds at home (the spec's "Culturing beds"): plant a strain, wait
//! while the crab is out (sim ticks), harvest `3 + level/20` of it, or feed
//! the bed spoiled batches to halve the time left, once.
//!
//! Planting is the `plant-*` recipes in [`super::recipes::ECON_RECIPES`]
//! (made through `Sim::make_econ`); harvesting and feeding are the two bed
//! actions here, noted in a replay as `craft=harvest-bed` and
//! `craft=feed-bed`.

use crate::craft::CantMakeEcon;
use crate::economy::recipes::StrainDef;
use crate::economy::{Bed, Economy, Item};
use crate::physics::TICKS_PER_SECOND;
use crate::skills::Skill;
use crate::world::{PlayerId, Sim};

/// A bed action a replay can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BedAction {
    /// Harvest the first ripe bed.
    Harvest,
    /// Feed one spoiled batch to the growing bed with the most time left.
    Feed,
}

impl BedAction {
    pub const ALL: [BedAction; 2] = [BedAction::Harvest, BedAction::Feed];

    /// The replay word.
    pub fn word(self) -> &'static str {
        match self {
            BedAction::Harvest => "harvest-bed",
            BedAction::Feed => "feed-bed",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.word() == word.trim())
    }
}

impl Economy {
    /// The index of a bed ready to harvest at `tick`.
    pub fn ripe_bed(&self, tick: u64) -> Option<usize> {
        self.beds
            .iter()
            .position(|b| b.is_some_and(|b| b.ready_at_tick <= tick))
    }

    /// The index of the unfed, still growing bed with the most time left.
    pub fn feedable_bed(&self, tick: u64) -> Option<usize> {
        self.beds
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                b.filter(|b| !b.fed && b.ready_at_tick > tick)
                    .map(|b| (i, b))
            })
            .max_by_key(|(i, b)| (b.ready_at_tick, std::cmp::Reverse(*i)))
            .map(|(i, _)| i)
    }

    /// Plant `strain` in the first free bed, ready after its grow time.
    /// False if there is no free bed or no such strain.
    pub fn plant(&mut self, strain: Item, tick: u64) -> bool {
        let Some(def) = StrainDef::of(strain) else {
            return false;
        };
        let Some(slot) = self.beds.iter_mut().find(|b| b.is_none()) else {
            return false;
        };
        let ticks = u64::from(def.grow_secs) * u64::from(TICKS_PER_SECOND);
        *slot = Some(Bed {
            strain,
            ready_at_tick: tick.saturating_add(ticks),
            fed: false,
        });
        true
    }
}

impl Sim {
    /// Crab `id` harvests its first ripe bed: `3 + level/20` of the strain
    /// into the bank and the harvest XP. Only a local crab tucked in at home.
    pub fn harvest_bed(&mut self, id: PlayerId) -> Result<(), CantMakeEcon> {
        self.at_home(id)?;
        let tick = self.tick_count();
        let level = self
            .crab(id)
            .map_or(1, |c| c.level(Skill::Culturing, &self.tuning));
        let crab = self.crab_mut(id).ok_or(CantMakeEcon::NotAtHome)?;
        let i = crab.econ.ripe_bed(tick).ok_or(CantMakeEcon::NothingReady)?;
        let bed = crab.econ.beds[i].take().ok_or(CantMakeEcon::NothingReady)?;
        let xp = StrainDef::of(bed.strain).map_or(0, |d| d.harvest_xp);
        crab.stash.add(bed.strain, StrainDef::harvest_count(level));
        crab.econ.crafts_done += 1;
        self.earn(id, Skill::Culturing, xp as f32);
        self.record_craft(id, crate::craft::Craft::Bed(BedAction::Harvest));
        Ok(())
    }

    /// Crab `id` feeds a spoiled batch to a growing bed: half the time left,
    /// once per planting.
    pub fn feed_bed(&mut self, id: PlayerId) -> Result<(), CantMakeEcon> {
        self.at_home(id)?;
        let tick = self.tick_count();
        let crab = self.crab_mut(id).ok_or(CantMakeEcon::NotAtHome)?;
        let i = crab
            .econ
            .feedable_bed(tick)
            .ok_or(CantMakeEcon::NothingGrowing)?;
        if crab.stash.count(Item::Spoiled) == 0 {
            return Err(CantMakeEcon::Missing(Item::Spoiled, 1));
        }
        crab.stash.take(&[(Item::Spoiled, 1)]);
        if let Some(bed) = crab.econ.beds[i].as_mut() {
            bed.ready_at_tick = tick + (bed.ready_at_tick - tick) / 2;
            bed.fed = true;
        }
        self.record_craft(id, crate::craft::Craft::Bed(BedAction::Feed));
        Ok(())
    }
}
