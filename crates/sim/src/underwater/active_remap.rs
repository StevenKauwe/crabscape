//! Move the retained rectangle without allocating another full active grid.
use super::region::CellRegion;

pub(super) fn remap<T: Copy>(
    source: &mut Vec<T>,
    old: CellRegion,
    next: CellRegion,
    blank: T,
) -> Vec<T> {
    let left = old.x.max(next.x);
    let right = (old.x + old.width as i32).min(next.x + next.width as i32);
    let bottom = old.y.max(next.y);
    let top = (old.y + old.height as i32).min(next.y + next.height as i32);
    if old.width == next.width && old.height == next.height {
        // Rows move in the memmove direction. Each row's copy_within handles
        // horizontal overlap; row order protects the next vertical source.
        if left < right && bottom < top {
            let count = (right - left) as usize;
            for row in 0..(top - bottom) {
                let y = if next.y < old.y {
                    top - 1 - row
                } else {
                    bottom + row
                };
                let from = (y - old.y) as usize * old.width + (left - old.x) as usize;
                let to = (y - next.y) as usize * next.width + (left - next.x) as usize;
                source.copy_within(from..from + count, to);
            }
        }
        // Entering strips are overwritten by their archived/authored cells.
        return std::mem::take(source);
    }
    let mut out = vec![blank; next.width * next.height];
    if left < right && bottom < top {
        let count = (right - left) as usize;
        for y in bottom..top {
            let from = (y - old.y) as usize * old.width + (left - old.x) as usize;
            let to = (y - next.y) as usize * next.width + (left - next.x) as usize;
            out[to..to + count].copy_from_slice(&source[from..from + count]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_moves_preserve_absolute_cells_in_every_direction_and_reuse_allocation() {
        let old = CellRegion {
            x: 70,
            y: -190,
            width: 96,
            height: 80,
        };
        for dx in [-120, -95, -64, -3, 0, 1, 32, 95, 120] {
            for dy in [-100, -79, -16, -1, 0, 3, 64, 79, 100] {
                for size in [(96, 80), (80, 96), (128, 128)] {
                    let next = CellRegion {
                        x: old.x + dx,
                        y: old.y + dy,
                        width: size.0,
                        height: size.1,
                    };
                    let mut source: Vec<_> = (0..old.width * old.height).collect();
                    let allocation = source.as_ptr();
                    let out = remap(&mut source, old, next, usize::MAX);
                    if size == (old.width, old.height) {
                        assert_eq!(allocation, out.as_ptr());
                    }
                    for y in 0..next.height {
                        for x in 0..next.width {
                            let (gx, gy) = (next.x + x as i32, next.y + y as i32);
                            if old.contains(gx, gy) {
                                assert_eq!(
                                    out[y * next.width + x],
                                    (gy - old.y) as usize * old.width + (gx - old.x) as usize,
                                    "move {dx},{dy} to {size:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
