//! Rectangle access with one archive lookup/borrow per page. The callback is
//! cell-local; traversal order is not simulation order and carries no forces.
use super::*;

impl World {
    pub(super) fn visit_archived_region<E>(
        &self,
        bounds: CellRegion,
        mut visit: impl FnMut(i32, i32, Option<SleepingCell>) -> Result<(), E>,
    ) -> Result<(), E> {
        let side = TERRAIN_CHUNK_CELLS;
        let active = self.region();
        for cy in bounds.y.div_euclid(side)..=(bounds.y + bounds.height as i32 - 1).div_euclid(side)
        {
            for cx in
                bounds.x.div_euclid(side)..=(bounds.x + bounds.width as i32 - 1).div_euclid(side)
            {
                let left = bounds.x.max(cx * side);
                let right = (bounds.x + bounds.width as i32).min((cx + 1) * side);
                let bottom = bounds.y.max(cy * side);
                let top = (bounds.y + bounds.height as i32).min((cy + 1) * side);
                let inside = left >= active.x
                    && bottom >= active.y
                    && right <= active.x + active.width as i32
                    && top <= active.y + active.height as i32;
                // Looking up an entirely active page would create a spurious
                // cold-page request for data already in the active arrays.
                let page = if inside {
                    None
                } else {
                    self.sleeping.get(&(cx, cy))
                };
                for gy in bottom..top {
                    for gx in left..right {
                        let value = if let Some(i) =
                            self.index_at(gx - self.origin.0, gy - self.origin.1)
                        {
                            Some(SleepingCell {
                                cell: self.cells[i],
                                flow: self.flow[i],
                                suspended: self.suspended[i],
                                grain: self.grain[i],
                                owner: self.pebble_owner[i],
                            })
                        } else {
                            let slot = (gy.rem_euclid(side) * side + gx.rem_euclid(side)) as usize;
                            page.and_then(|p| p.cells[slot])
                        };
                        visit(gx, gy, value)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn map_archived_region(
        &mut self,
        bounds: CellRegion,
        mut value: impl FnMut(i32, i32, Option<SleepingCell>) -> SleepingCell,
    ) {
        let active = self.region();
        let left = bounds.x.max(active.x);
        let right = (bounds.x + bounds.width as i32).min(active.x + active.width as i32);
        let bottom = bounds.y.max(active.y);
        let top = (bounds.y + bounds.height as i32).min(active.y + active.height as i32);
        for gy in bottom..top {
            for gx in left..right {
                let i = (gy - active.y) as usize * self.width + (gx - active.x) as usize;
                let old = SleepingCell {
                    cell: self.cells[i],
                    flow: self.flow[i],
                    suspended: self.suspended[i],
                    grain: self.grain[i],
                    owner: self.pebble_owner[i],
                };
                let v = value(gx, gy, Some(old));
                self.cells[i] = v.cell;
                self.flow[i] = v.flow;
                self.suspended[i] = v.suspended;
                self.grain[i] = v.grain;
                self.pebble_owner[i] = v.owner;
            }
        }
        let side = TERRAIN_CHUNK_CELLS;
        for strip in bounds.difference(active) {
            for cy in
                strip.y.div_euclid(side)..=(strip.y + strip.height as i32 - 1).div_euclid(side)
            {
                for cx in
                    strip.x.div_euclid(side)..=(strip.x + strip.width as i32 - 1).div_euclid(side)
                {
                    let left = strip.x.max(cx * side);
                    let right = (strip.x + strip.width as i32).min((cx + 1) * side);
                    let bottom = strip.y.max(cy * side);
                    let top = (strip.y + strip.height as i32).min((cy + 1) * side);
                    let slots = self
                        .sleeping
                        .entry((cx, cy))
                        .or_default()
                        .cells
                        .as_mut_slice();
                    for gy in bottom..top {
                        for gx in left..right {
                            let slot = (gy.rem_euclid(side) * side + gx.rem_euclid(side)) as usize;
                            slots[slot] = Some(value(gx, gy, slots[slot]));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bulk_reads_and_edits_match_scalar_access_at_active_and_sleeping_seams() {
        let mut source = World::new();
        source.origin = (65, -100);
        for (gx, gy) in [(0, -128), (704, -98), (705, 200), (63, -101), (600, 310)] {
            source.put_archived_cell(
                gx,
                gy,
                SleepingCell {
                    cell: Cell::Mud,
                    flow: (0.3, -0.7),
                    suspended: 17,
                    grain: 92,
                    owner: 0,
                },
            );
        }
        for bounds in [
            CellRegion {
                x: 600,
                y: -120,
                width: 128,
                height: 128,
            },
            CellRegion {
                x: 64,
                y: -128,
                width: 128,
                height: 128,
            },
            CellRegion {
                x: 700,
                y: -128,
                width: 128,
                height: 128,
            },
            CellRegion {
                x: 0,
                y: 220,
                width: 256,
                height: 128,
            },
        ] {
            let mut visited = BTreeMap::new();
            source
                .visit_archived_region(bounds, |x, y, v| {
                    visited.insert((x, y), v);
                    Ok::<_, ()>(())
                })
                .unwrap();
            assert_eq!(visited.len(), bounds.width * bounds.height);
            for (&(x, y), &v) in &visited {
                assert_eq!(v, source.archived_cell(x, y));
            }
            let change = |x: i32, y: i32, old: Option<SleepingCell>| SleepingCell {
                cell: if (x ^ y) & 1 == 0 {
                    Cell::Sand
                } else {
                    Cell::Mud
                },
                flow: old.map_or((0.0, 0.0), |s| s.flow),
                grain: (x + y).rem_euclid(248) as u8,
                suspended: 3,
                owner: 0,
            };
            let mut scalar = source.clone();
            let mut bulk = source.clone();
            let held = bulk.clone();
            for y in bounds.y..bounds.y + bounds.height as i32 {
                for x in bounds.x..bounds.x + bounds.width as i32 {
                    scalar.put_archived_cell(x, y, change(x, y, scalar.archived_cell(x, y)));
                }
            }
            bulk.map_archived_region(bounds, change);
            assert!(
                scalar == bulk,
                "bulk edit differed from scalar access at {bounds:?}"
            );
            assert!(held == source, "editing a page changed a held snapshot");
        }
    }
}
