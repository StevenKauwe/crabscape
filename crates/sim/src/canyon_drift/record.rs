//! Fixed, independently owned spatial records for one renewable Canyon anchor.
use super::{
    DriftBody, DriftCatcher,
    run::{Band, CanyonRun, PracticePhase, Slot, SlotPhase, UPPER_INLET},
};
use crate::underwater::{GAME_BOTTOM, GAME_CELL};
use bevy_math::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnchorRecord {
    pub world: u64,
    pub band: Band,
    pub last_active_tick: Option<u64>,
    pub last_admission_tick: Option<u64>,
    pub deck_cursor: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlotRecord {
    pub world: u64,
    pub index: u8,
    pub slot: Slot,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatcherRecord {
    pub world: u64,
    pub catcher: DriftCatcher,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PracticeRecord {
    pub world: u64,
    pub body: DriftBody,
    pub phase: PracticePhase,
    pub revision: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanyonRegionRecord {
    pub schema: u16,
    pub region_id: (i32, i32),
    pub anchor: Option<AnchorRecord>,
    pub slots: [Option<SlotRecord>; 2],
    pub catcher: Option<CatcherRecord>,
    pub practice: Option<PracticeRecord>,
}
fn key(p: Vec2) -> (i32, i32) {
    (
        (p.x / (GAME_CELL * 64.0)).floor() as i32,
        ((p.y - GAME_BOTTOM) / (GAME_CELL * 64.0)).floor() as i32,
    )
}
impl CanyonRegionRecord {
    pub fn valid(&self) -> bool {
        if self.schema != 1
            || (self.anchor.is_none()
                && self.catcher.is_none()
                && self.practice.is_none()
                && self.slots.iter().all(Option::is_none))
        {
            return false;
        }
        if self.anchor.is_some() && key(UPPER_INLET) != self.region_id {
            return false;
        }
        if self
            .catcher
            .is_some_and(|r| key(r.catcher.position) != self.region_id)
            || self.practice.is_some_and(|r| {
                let p = if r.phase == PracticePhase::Caught {
                    self.catcher.map(|c| c.catcher.position)
                } else {
                    Some(r.body.position)
                };
                p.is_none_or(|p| key(p) != self.region_id)
            })
            || self.slots.iter().enumerate().any(|(i, r)| {
                r.is_some_and(|r| {
                    let p = if matches!(r.slot.phase, SlotPhase::Caught(_)) {
                        self.catcher.map(|c| c.catcher.position)
                    } else {
                        Some(r.slot.location)
                    };
                    r.index as usize != i || p.is_none_or(|p| key(p) != self.region_id)
                })
            })
        {
            return false;
        }
        let mut run = CanyonRun::new(0);
        run.has_anchor = false;
        run.owned_slots = [false; 2];
        run.has_catcher = false;
        run.has_practice = false;
        let mut world = None;
        let mut accept_world = |id: u64| {
            if world.is_some_and(|prior| prior != id) {
                false
            } else {
                world = Some(id);
                true
            }
        };
        if let Some(a) = self.anchor {
            if !accept_world(a.world) {
                return false;
            }
            run.has_anchor = true;
            run.band = a.band;
            run.last_active_tick = a.last_active_tick;
            run.last_admission_tick = a.last_admission_tick;
            run.deck_cursor = a.deck_cursor;
        }
        if let Some(c) = self.catcher {
            if !accept_world(c.world) {
                return false;
            }
            run.has_catcher = true;
            run.catcher = c.catcher;
        }
        if let Some(p) = self.practice {
            if !accept_world(p.world) {
                return false;
            }
            run.has_practice = true;
            run.practice = p.body;
            run.practice_phase = p.phase;
            run.practice_revision = p.revision;
        }
        for (i, slot) in self.slots.iter().enumerate() {
            if let Some(slot) = slot {
                if !accept_world(slot.world) {
                    return false;
                }
                run.owned_slots[i] = true;
                run.slots[i] = slot.slot;
            }
        }
        run.world_id = world.unwrap_or_default();
        run.valid()
    }
}
impl CanyonRun {
    pub fn records(&self) -> Vec<CanyonRegionRecord> {
        let mut pages: Vec<CanyonRegionRecord> = Vec::new();
        let mut page = |region_id| {
            if let Some(i) = pages.iter().position(|p| p.region_id == region_id) {
                return i;
            }
            pages.push(CanyonRegionRecord {
                schema: 1,
                region_id,
                anchor: None,
                slots: [None; 2],
                catcher: None,
                practice: None,
            });
            pages.len() - 1
        };
        let anchor = self.has_anchor.then(|| page(key(UPPER_INLET)));
        let catcher = self.has_catcher.then(|| page(key(self.catcher.position)));
        let practice = self.has_practice.then(|| {
            page(key(if self.practice_phase == PracticePhase::Caught {
                self.catcher.position
            } else {
                self.practice.position
            }))
        });
        let slots = std::array::from_fn::<_, 2, _>(|i| {
            self.owned_slots[i].then(|| {
                page(key(
                    if matches!(self.slots[i].phase, SlotPhase::Caught(_)) {
                        self.catcher.position
                    } else {
                        self.slots[i].location
                    },
                ))
            })
        });
        if let Some(i) = anchor {
            pages[i].anchor = Some(AnchorRecord {
                world: self.world_id,
                band: self.band,
                last_active_tick: self.last_active_tick,
                last_admission_tick: self.last_admission_tick,
                deck_cursor: self.deck_cursor,
            });
        }
        if let Some(i) = catcher {
            pages[i].catcher = Some(CatcherRecord {
                world: self.world_id,
                catcher: self.catcher,
            });
        }
        if let Some(i) = practice {
            pages[i].practice = Some(PracticeRecord {
                world: self.world_id,
                body: self.practice,
                phase: self.practice_phase,
                revision: self.practice_revision,
            });
        }
        for (index, page) in slots.into_iter().enumerate() {
            if let Some(i) = page {
                pages[i].slots[index] = Some(SlotRecord {
                    world: self.world_id,
                    index: index as u8,
                    slot: self.slots[index],
                });
            }
        }
        pages
    }
    pub fn from_records(records: &[CanyonRegionRecord]) -> Option<Self> {
        if records.is_empty() || records.len() > 5 || records.iter().any(|r| !r.valid()) {
            return None;
        }
        if records
            .iter()
            .enumerate()
            .any(|(i, r)| records[..i].iter().any(|p| p.region_id == r.region_id))
        {
            return None;
        }
        let mut run = CanyonRun::new(0);
        run.has_anchor = false;
        run.owned_slots = [false; 2];
        run.has_catcher = false;
        run.has_practice = false;
        let mut world = None;
        let mut accept_world = |id: u64| {
            if world.is_some_and(|prior| prior != id) {
                false
            } else {
                world = Some(id);
                true
            }
        };
        for record in records {
            if let Some(a) = record.anchor {
                if run.has_anchor || !accept_world(a.world) {
                    return None;
                }
                run.has_anchor = true;
                run.band = a.band;
                run.last_active_tick = a.last_active_tick;
                run.last_admission_tick = a.last_admission_tick;
                run.deck_cursor = a.deck_cursor;
            }
            if let Some(c) = record.catcher {
                if run.has_catcher || !accept_world(c.world) {
                    return None;
                }
                run.has_catcher = true;
                run.catcher = c.catcher;
            }
            if let Some(p) = record.practice {
                if run.has_practice || !accept_world(p.world) {
                    return None;
                }
                run.has_practice = true;
                run.practice = p.body;
                run.practice_phase = p.phase;
                run.practice_revision = p.revision;
            }
            for (i, slot) in record.slots.iter().enumerate() {
                if let Some(slot) = slot {
                    if run.owned_slots[i] || !accept_world(slot.world) {
                        return None;
                    }
                    run.owned_slots[i] = true;
                    run.slots[i] = slot.slot;
                }
            }
        }
        run.world_id = world?;
        run.complete().then_some(run)
    }
}
