//! A low, diggable connection between the two banks east of Ribbed Bank.
//! Terrain is the durable state: there is no additional reward or progress bit.
use super::{Cell, GAME_BOTTOM, GAME_CELL, World};

impl World {
    /// Seed once, when creating a fresh opening. The overland route remains
    /// open; the sand beneath the rock roof can be excavated from either end.
    pub fn seed_shallow_shortcut(&mut self) {
        for gx in 125..=134 {
            for gy in 90..=139 {
                let Some(i) = self.index_at(gx - self.origin.0, gy - self.origin.1) else {
                    continue;
                };
                if self.pebble_owner[i] != 0 {
                    continue;
                }
                let y = GAME_BOTTOM + (gy as f32 + 0.5) * GAME_CELL;
                self.cells[i] = if y < 80.0 {
                    Cell::Sand
                } else if y < 96.0 {
                    Cell::Rock
                } else {
                    Cell::Water
                };
                self.flow[i] = (0.0, 0.0);
            }
        }
    }
}
