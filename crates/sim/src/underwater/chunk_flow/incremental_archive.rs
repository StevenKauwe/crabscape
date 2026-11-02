//! Bounded-work archive validation experiment. The immutable borrow is the
//! proof's lifetime: no mutable world access is possible while it is scanned.
//! This does not yet authorize later writes, other worlds, or game integration.
#[cfg(test)]
mod owned;

use super::{Cell, HEIGHT, SIDE, TERRAIN_CHUNK_CELLS, WIDTH, World};
use crate::underwater::SleepingChunk;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Pending,
    Valid,
    Invalid,
}

type Pages<'a> = std::collections::btree_map::Iter<'a, (i32, i32), SleepingChunk>;

struct Validator<'a> {
    world: &'a World,
    pages: Pages<'a>,
    page: Option<((i32, i32), &'a SleepingChunk, usize)>,
    counts: Vec<u16>,
    expected: Vec<u16>,
    index: usize,
    phase: u8,
    status: Status,
}

impl<'a> Validator<'a> {
    fn new(world: &'a World) -> Self {
        let valid_len = world.pebbles.len() <= u16::MAX as usize;
        let count = if valid_len { world.pebbles.len() } else { 0 };
        Self {
            world,
            pages: world.sleeping.iter(),
            page: None,
            counts: vec![0; count],
            expected: vec![0; count],
            index: 0,
            phase: 0,
            status: if valid_len {
                Status::Pending
            } else {
                Status::Invalid
            },
        }
    }
    // Each unit examines at most one pebble or archived cell; allocation is
    // capped at two 65,535-entry u16 arrays, independently of archive size.
    fn advance(&mut self, budget: usize) -> Status {
        const MAX_ITEMS_PER_CALL: usize = 4096;
        for _ in 0..budget.min(MAX_ITEMS_PER_CALL) {
            if self.status != Status::Pending {
                break;
            }
            match self.phase {
                0 => {
                    if self.index == self.world.pebbles.len() {
                        self.phase = 1;
                        continue;
                    }
                    let p = self.world.pebbles[self.index];
                    let side = i32::from(p.side);
                    let (Some(right), Some(top)) = (p.x.checked_add(side), p.y.checked_add(side))
                    else {
                        self.status = Status::Invalid;
                        break;
                    };
                    if !(1..=64).contains(&side) {
                        self.status = Status::Invalid;
                        break;
                    }
                    let intersects =
                        right > 0 && p.x < WIDTH as i32 && top > 0 && p.y < HEIGHT as i32;
                    if !intersects {
                        self.expected[self.index] = (side * side) as u16;
                    }
                    self.index += 1;
                }
                1 => {
                    if self.page.is_none() {
                        let Some((&key, page)) = self.pages.next() else {
                            self.phase = 2;
                            self.index = 0;
                            continue;
                        };
                        if page.cells.len() != SIDE * SIDE {
                            self.status = Status::Invalid;
                            break;
                        }
                        self.page = Some((key, page, 0));
                    }
                    let Some((key, page, slot)) = &mut self.page else {
                        unreachable!()
                    };
                    if let Some(cell) = &page.cells[*slot]
                        && cell.owner != 0
                    {
                        let index = usize::from(cell.owner) - 1;
                        let Some(p) = self.world.pebbles.get(index) else {
                            self.status = Status::Invalid;
                            break;
                        };
                        let Some(x) = key
                            .0
                            .checked_mul(TERRAIN_CHUNK_CELLS)
                            .and_then(|v| v.checked_add((*slot % SIDE) as i32))
                            .and_then(|v| v.checked_sub(self.world.origin.0))
                        else {
                            self.status = Status::Invalid;
                            break;
                        };
                        let Some(y) = key
                            .1
                            .checked_mul(TERRAIN_CHUNK_CELLS)
                            .and_then(|v| v.checked_add((*slot / SIDE) as i32))
                            .and_then(|v| v.checked_sub(self.world.origin.1))
                        else {
                            self.status = Status::Invalid;
                            break;
                        };
                        let side = i32::from(p.side);
                        let (Some(right), Some(top)) =
                            (p.x.checked_add(side), p.y.checked_add(side))
                        else {
                            self.status = Status::Invalid;
                            break;
                        };
                        if cell.cell != Cell::Sand
                            || (0..WIDTH as i32).contains(&x) && (0..HEIGHT as i32).contains(&y)
                            || x < p.x
                            || x >= right
                            || y < p.y
                            || y >= top
                        {
                            self.status = Status::Invalid;
                            break;
                        }
                        let Some(count) = self.counts[index].checked_add(1) else {
                            self.status = Status::Invalid;
                            break;
                        };
                        self.counts[index] = count;
                    }
                    *slot += 1;
                    if *slot == SIDE * SIDE {
                        self.page = None;
                    }
                }
                _ => {
                    if self.index == self.counts.len() {
                        self.status = Status::Valid;
                        break;
                    }
                    if self.counts[self.index] != self.expected[self.index] {
                        self.status = Status::Invalid;
                        break;
                    }
                    self.index += 1;
                }
            }
        }
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::underwater::chunk_flow::ChunkFlow;

    fn run(world: &World, budget: usize) -> Status {
        let mut v = Validator::new(world);
        let mut result = v.advance(0);
        assert_eq!(result, Status::Pending);
        while result == Status::Pending {
            result = v.advance(budget);
        }
        result
    }

    fn fixture() -> World {
        let mut world = World::new();
        world.game = true;
        world.origin = (0, 0);
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        assert!(world.test_add_pebble(100, 228, 3));
        assert!(world.test_add_pebble(320, 228, 3));
        world.shift_window(&crate::Level::test_level(), (256, 0));
        world
    }

    #[test]
    fn bounded_steps_match_existing_guard_and_reject_corruption() {
        let world = fixture();
        assert!(ChunkFlow::validate_sleeping_pebbles(&world));
        for budget in [1, 7, 4096, 100_000] {
            assert_eq!(run(&world, budget), Status::Valid);
        }
        let mut bad = world.clone();
        let key = (
            100_i32.div_euclid(TERRAIN_CHUNK_CELLS),
            228_i32.div_euclid(TERRAIN_CHUNK_CELLS),
        );
        let slot = (228_i32.rem_euclid(TERRAIN_CHUNK_CELLS) * TERRAIN_CHUNK_CELLS
            + 100_i32.rem_euclid(TERRAIN_CHUNK_CELLS)) as usize;
        bad.sleeping.get_mut(&key).unwrap().cells[slot] = None;
        assert!(!ChunkFlow::validate_sleeping_pebbles(&bad));
        assert_eq!(run(&bad, 17), Status::Invalid);
    }

    #[test]
    fn rejects_orphans_duplicates_bad_material_and_truncated_pages() {
        let world = fixture();
        let key = (1, 3);
        let slot = 36 * SIDE + 36; // world (100, 228)
        for kind in 0..5 {
            let mut bad = world.clone();
            match kind {
                0 => {
                    bad.sleeping.get_mut(&key).unwrap().cells[slot]
                        .as_mut()
                        .unwrap()
                        .owner = 0
                }
                1 => {
                    bad.sleeping.get_mut(&key).unwrap().cells[slot]
                        .as_mut()
                        .unwrap()
                        .cell = Cell::Rock
                }
                2 => {
                    let mut cell = bad.sleeping.get(&key).unwrap().cells[slot].unwrap();
                    cell.owner = u16::MAX;
                    bad.sleeping.get_mut(&key).unwrap().cells[slot + 10] = Some(cell);
                }
                3 => {
                    let mut cell = bad.sleeping.get(&key).unwrap().cells[slot].unwrap();
                    cell.owner = 2;
                    bad.sleeping.entry((5, 3)).or_default().cells[36 * SIDE] = Some(cell);
                }
                _ => {
                    bad.sleeping.get_mut(&key).unwrap().cells.truncate(slot);
                }
            }
            assert_eq!(run(&bad, 257), Status::Invalid, "case {kind}");
        }
    }

    #[test]
    #[ignore = "manual release-mode incremental validation profile"]
    fn incremental_archive_profile() {
        let mut world = fixture();
        for x in 0..80 {
            world.sleeping.insert((x, -100), Default::default());
        }
        let mut validator = Validator::new(&world);
        let mut times = Vec::new();
        loop {
            let start = std::time::Instant::now();
            let status = validator.advance(4096);
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            if status != Status::Pending {
                assert_eq!(status, Status::Valid);
                break;
            }
        }
        times.sort_by(f64::total_cmp);
        eprintln!(
            "{} calls, median {:.4} ms, max {:.4} ms; 4096 items/call, existing fixture plus 80 empty pages; excludes World creation and all game/network work",
            times.len(),
            times[times.len() / 2],
            times[times.len() - 1]
        );
    }

    #[test]
    fn larger_archive_validates_in_bounded_slices_without_a_page_cap() {
        let mut world = fixture();
        for x in 0..80 {
            world.sleeping.insert((x, -100), Default::default());
        }
        assert!(!ChunkFlow::validate_sleeping_pebbles(&world));
        let mut bounded = Validator::new(&world);
        assert_eq!(bounded.advance(usize::MAX), Status::Pending);
        assert_eq!(run(&world, 137), Status::Valid);
    }
}
