//! Feature-gated migration proof. Coordinates are absolute terrain cells;
//! nothing here reads, writes, or installs a save.
use super::*;

#[derive(Clone, Copy)]
pub struct Rect {
    pub left: i64,
    pub bottom: i64,
    pub right: i64,
    pub top: i64,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
}

/// Validate ownership completely before cloning or altering anything. A
/// pebble's square must be represented exactly once across active and sleeping
/// cells. Keep all sediment and flow in place; normalize selected pebble
/// grains to ordinary sand and remap the remaining owner indices.
struct Plan {
    remap: Vec<u16>,
    kept: Vec<Pebble>,
    motion: Vec<(i8, i8, u8)>,
    removed: usize,
}

fn prepare(source: &World, bounds: Rect) -> Result<Plan, Error> {
    let n = source.pebbles.len();
    if bounds.left >= bounds.right
        || bounds.bottom >= bounds.top
        || n > u16::MAX as usize
        || source.pebble_motion.len() != n
        || source.cells.len() != WIDTH * HEIGHT
        || source.grain.len() != WIDTH * HEIGHT
        || source.pebble_owner.len() != WIDTH * HEIGHT
        || source.pebbles.iter().any(|p| p.side == 0 || p.side > 64)
    {
        return Err(Error::Invalid);
    }
    let mut counts = vec![0usize; n];
    let mut check = |owner: u16, cell: Cell, x: i64, y: i64| -> Result<(), Error> {
        if owner == 0 {
            return Ok(());
        }
        let p = source
            .pebbles
            .get(owner as usize - 1)
            .ok_or(Error::Invalid)?;
        let px = i64::from(source.origin.0) + i64::from(p.x);
        let py = i64::from(source.origin.1) + i64::from(p.y);
        if cell != Cell::Sand
            || x < px
            || x >= px + i64::from(p.side)
            || y < py
            || y >= py + i64::from(p.side)
        {
            return Err(Error::Invalid);
        }
        counts[owner as usize - 1] += 1;
        Ok(())
    };
    for (i, &owner) in source.pebble_owner.iter().enumerate() {
        check(
            owner,
            source.cells[i],
            i64::from(source.origin.0) + (i % WIDTH) as i64,
            i64::from(source.origin.1) + (i / WIDTH) as i64,
        )?;
    }
    let side = i64::from(TERRAIN_CHUNK_CELLS);
    for (&(cx, cy), chunk) in &source.sleeping {
        if chunk.cells.len() != (side * side) as usize {
            return Err(Error::Invalid);
        }
        for (i, cell) in chunk.cells.iter().enumerate() {
            if let Some(cell) = cell {
                let x = i64::from(cx) * side + i as i64 % side;
                let y = i64::from(cy) * side + i as i64 / side;
                // A stored owner inside the active window can duplicate an
                // active footprint and is not a valid complete checkpoint.
                if cell.owner != 0
                    && x >= i64::from(source.origin.0)
                    && x < i64::from(source.origin.0) + WIDTH as i64
                    && y >= i64::from(source.origin.1)
                    && y < i64::from(source.origin.1) + HEIGHT as i64
                {
                    return Err(Error::Invalid);
                }
                check(cell.owner, cell.cell, x, y)?;
            }
        }
    }
    if source
        .pebbles
        .iter()
        .zip(counts)
        .any(|(p, c)| c != usize::from(p.side).pow(2))
    {
        return Err(Error::Invalid);
    }
    let mut remap = vec![0u16; n + 1];
    let mut kept = Vec::with_capacity(n);
    let mut motion = Vec::with_capacity(n);
    let mut removed = 0;
    for (i, (&p, &m)) in source.pebbles.iter().zip(&source.pebble_motion).enumerate() {
        let x = i64::from(source.origin.0) + i64::from(p.x);
        let y = i64::from(source.origin.1) + i64::from(p.y);
        if x >= bounds.left
            && y >= bounds.bottom
            && x + i64::from(p.side) <= bounds.right
            && y + i64::from(p.side) <= bounds.top
        {
            for value in [x, y, x + i64::from(p.side) - 1, y + i64::from(p.side) - 1] {
                i32::try_from(value).map_err(|_| Error::Invalid)?;
            }
            removed += 1;
        } else {
            kept.push(p);
            motion.push(m);
            remap[i + 1] = kept.len() as u16;
        }
    }
    Ok(Plan {
        remap,
        kept,
        motion,
        removed,
    })
}

fn apply(mut candidate: World, plan: Plan) -> (World, usize) {
    let Plan {
        remap,
        kept,
        motion,
        removed,
    } = plan;
    let side = i64::from(TERRAIN_CHUNK_CELLS);
    for (i, owner) in candidate.pebble_owner.iter_mut().enumerate() {
        let mapped = remap[*owner as usize];
        if *owner != 0 && mapped == 0 {
            let x = i64::from(candidate.origin.0) + (i % WIDTH) as i64;
            let y = i64::from(candidate.origin.1) + (i / WIDTH) as i64;
            candidate.grain[i] = initial_grain(x as i32, y as i32);
        }
        *owner = mapped;
    }
    for (&(cx, cy), chunk) in &mut candidate.sleeping {
        for (i, cell) in chunk.cells.iter_mut().enumerate() {
            let Some(cell) = cell else { continue };
            let mapped = remap[cell.owner as usize];
            if cell.owner != 0 && mapped == 0 {
                let x = i64::from(cx) * side + i as i64 % side;
                let y = i64::from(cy) * side + i as i64 / side;
                cell.grain = initial_grain(x as i32, y as i32);
            }
            cell.owner = mapped;
        }
    }
    candidate.pebbles = kept;
    candidate.pebble_motion = motion;
    (candidate, removed)
}

/// Preserve a borrowed source by cloning only after validation succeeds.
pub fn transform(source: &World, bounds: Rect) -> Result<(World, usize), Error> {
    let plan = prepare(source, bounds)?;
    Ok(apply(source.clone(), plan))
}

/// Validate first, then transform an owned candidate without cloning the World.
/// On validation failure the unchanged original is returned to the caller.
pub fn transform_owned(source: World, bounds: Rect) -> Result<(World, usize), Box<(Error, World)>> {
    let plan = match prepare(&source, bounds) {
        Ok(plan) => plan,
        Err(error) => return Err(Box::new((error, source))),
    };
    Ok(apply(source, plan))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> World {
        let mut world = World::new();
        world.game = true;
        for (id, (x, y)) in [(10, 20), (100, 358), (200, 400)].into_iter().enumerate() {
            let owner = (id + 1) as u16;
            world.pebbles.push(Pebble {
                x,
                y,
                side: 4,
                shade: 250 + id as u8,
            });
            world.pebble_motion.push((id as i8 - 1, 1, 2));
            for yy in y..y + 4 {
                for xx in x..x + 4 {
                    if let Some(i) = World::index(xx, yy) {
                        world.cells[i] = Cell::Sand;
                        world.pebble_owner[i] = owner;
                        world.grain[i] = 250 + id as u8;
                    } else {
                        let side = TERRAIN_CHUNK_CELLS;
                        let id = (xx.div_euclid(side), yy.div_euclid(side));
                        let index = (yy.rem_euclid(side) * side + xx.rem_euclid(side)) as usize;
                        world.sleeping.entry(id).or_default().cells[index] = Some(SleepingCell {
                            cell: Cell::Sand,
                            flow: (0.25, -0.5),
                            suspended: 2,
                            grain: 250 + (owner - 1) as u8,
                            owner,
                        });
                    }
                }
            }
        }
        world
    }

    #[test]
    fn active_split_and_sleeping_aggregates_remap_without_moving_material() {
        let original = fixture();
        let bounds = Rect {
            left: 0,
            bottom: 0,
            right: 150,
            top: 362,
        };
        let (actual, removed) = transform(&original, bounds).unwrap();
        assert_eq!(removed, 2);
        let (again, count) = transform(&original, bounds).unwrap();
        assert_eq!((actual.clone(), removed), (again, count));
        let mut expected = original.clone();
        for (i, owner) in expected.pebble_owner.iter_mut().enumerate() {
            if matches!(*owner, 1 | 2) {
                expected.grain[i] = initial_grain((i % WIDTH) as i32, (i / WIDTH) as i32);
            }
            *owner = match *owner {
                1 | 2 => 0,
                3 => 1,
                other => other,
            };
        }
        for (&(cx, cy), chunk) in &mut expected.sleeping {
            for (i, cell) in chunk.cells.iter_mut().enumerate() {
                let Some(cell) = cell else { continue };
                if matches!(cell.owner, 1 | 2) {
                    cell.grain =
                        initial_grain(cx * 64 + (i % 64) as i32, cy * 64 + (i / 64) as i32);
                }
                cell.owner = match cell.owner {
                    1 | 2 => 0,
                    3 => 1,
                    other => other,
                };
            }
        }
        expected.pebbles = vec![original.pebbles[2]];
        expected.pebble_motion = vec![original.pebble_motion[2]];
        assert_eq!(actual, expected);
        assert_eq!(original, fixture(), "input must remain unchanged");
        assert_eq!(transform(&actual, bounds).unwrap(), (actual, 0));
    }

    #[test]
    fn boundary_crossers_and_unselected_pebbles_remain_unchanged() {
        let original = fixture();
        let bounds = Rect {
            left: 0,
            bottom: 0,
            right: 102,
            top: 362,
        };
        let (actual, removed) = transform(&original, bounds).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(actual.pebbles, original.pebbles[1..]);
        assert_eq!(actual.pebble_motion, original.pebble_motion[1..]);
        assert_eq!(actual.cells, original.cells);
        assert_eq!(actual.flow, original.flow);
        for (i, (&before, &after)) in original.grain.iter().zip(&actual.grain).enumerate() {
            if original.pebble_owner[i] == 1 {
                assert_eq!(after, initial_grain((i % WIDTH) as i32, (i / WIDTH) as i32));
                assert!(after < 248);
            } else {
                assert_eq!(after, before);
            }
        }
        assert_eq!(actual.suspended, original.suspended);
    }

    #[test]
    fn owned_transform_matches_borrowed_without_replacing_terrain_buffers() {
        let source = fixture();
        let bounds = Rect {
            left: 0,
            bottom: 0,
            right: 150,
            top: 362,
        };
        let expected = transform(&source, bounds).unwrap();
        let cells = source.cells.as_ptr();
        let flow = source.flow.as_ptr();
        let grain = source.grain.as_ptr();
        let owners = source.pebble_owner.as_ptr();
        let sleeping = source.sleeping[&(1, 5)].cells.as_ptr();
        let (actual, removed) = transform_owned(source, bounds).unwrap();
        assert_eq!((actual.clone(), removed), expected);
        assert_eq!(actual.cells.as_ptr(), cells);
        assert_eq!(actual.flow.as_ptr(), flow);
        assert_eq!(actual.grain.as_ptr(), grain);
        assert_eq!(actual.pebble_owner.as_ptr(), owners);
        assert_eq!(actual.sleeping[&(1, 5)].cells.as_ptr(), sleeping);
    }

    #[test]
    fn owned_transform_returns_unchanged_input_on_validation_failure() {
        let mut invalid = fixture();
        invalid.pebble_owner[World::index(10, 20).unwrap()] = 0;
        let expected = invalid.clone();
        let cells = invalid.cells.as_ptr();
        let bounds = Rect {
            left: 0,
            bottom: 0,
            right: 150,
            top: 362,
        };
        let rejected = transform_owned(invalid, bounds).unwrap_err();
        assert_eq!(rejected.0, Error::Invalid);
        assert_eq!(rejected.1, expected);
        assert_eq!(rejected.1.cells.as_ptr(), cells);
    }

    #[test]
    fn malformed_missing_duplicate_and_non_sand_ownership_fail_closed() {
        let bounds = Rect {
            left: 0,
            bottom: 0,
            right: 150,
            top: 362,
        };
        for case in 0..4 {
            let mut source = fixture();
            match case {
                0 => source.pebble_owner[World::index(10, 20).unwrap()] = 0,
                1 => source.pebble_owner[World::index(20, 20).unwrap()] = 1,
                2 => source.cells[World::index(10, 20).unwrap()] = Cell::Water,
                _ => source.sleeping.get_mut(&(1, 5)).unwrap().cells[40 * 64 + 36] = None,
            }
            let before = source.clone();
            assert_eq!(transform(&source, bounds), Err(Error::Invalid));
            assert_eq!(source, before);
        }
    }

    #[test]
    fn old_bake_round_trips_a_streamed_checkpoint_before_cleanup() {
        let level = crate::Level::test_level();
        let mut source = World::opening_pebble_fixture(&level);
        assert!(source.follow_player(&level, 4400.0, -700.0));
        let bytes = postcard::to_allocvec(&source.checkpoint()).unwrap();
        let saved: WorldCheckpoint = postcard::from_bytes(&bytes).unwrap();
        let restored = saved.into_world().unwrap();
        assert_eq!(restored, source);
        let bounds = Rect {
            left: 0,
            bottom: 0,
            right: 640,
            top: 360,
        };
        let (candidate, count) = transform(&restored, bounds).unwrap();
        assert!(count > 0);
        assert_eq!(candidate.cells, restored.cells);
        assert_eq!(candidate.flow, restored.flow);
        assert_eq!(candidate.tick, restored.tick);
        assert_eq!(candidate.suspended, restored.suspended);
        for (i, (&before, &after)) in restored.grain.iter().zip(&candidate.grain).enumerate() {
            if restored.pebble_owner[i] != 0 && candidate.pebble_owner[i] == 0 {
                assert!(after < 248);
            } else {
                assert_eq!(after, before);
            }
        }
        assert_eq!(transform(&candidate, bounds).unwrap(), (candidate, 0));
        assert_eq!(restored, source);
    }

    #[test]
    fn transformed_checkpoint_starts_a_replay_of_the_transformed_world() {
        let level = crate::Level::test_level();
        let source = World::opening_pebble_fixture(&level);
        let mut old_sim = crate::Sim::new(crate::Tuning::default(), level.clone());
        assert!(old_sim.restore_cellular_terrain(source.clone(), 17));
        let old_raw = old_sim.recording_terrain_bytes().unwrap();
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&old_raw, 6);
        let old_token = crate::VerifiedCellStart::from_compressed(&packed, 17).unwrap();
        let bounds = Rect {
            left: 0,
            bottom: 0,
            right: 640,
            top: 360,
        };
        let (candidate, count) = transform(&source, bounds).unwrap();
        assert!(count > 0);
        let mut sim = crate::Sim::new(crate::Tuning::default(), level);
        assert!(sim.restore_cellular_terrain(candidate, 17));
        sim.add_crab(crate::PlayerId(0), crate::SPAWN);
        let mut progress = sim.progress(crate::PlayerId(0)).unwrap();
        progress.lantern = 1;
        sim.restore_progress(crate::PlayerId(0), &progress);
        assert_eq!(sim.progress(crate::PlayerId(0)).unwrap(), progress);
        assert!(!sim.start_recording_with_verified_terrain(&old_token));
        sim.start_recording();
        assert!(sim.is_recording());
        for _ in 0..8 {
            sim.tick();
        }
        let replay = sim.recording().unwrap();
        let parsed = crate::Replay::parse(&replay.to_text()).unwrap();
        let mut playback = crate::Sim::with_one_crab(parsed.start);
        crate::play(&mut playback, &parsed, parsed.length(), |_| {});
        assert_eq!(playback.state_hash(), sim.state_hash());
    }
}
