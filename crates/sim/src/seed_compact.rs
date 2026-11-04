//! Offline lossless-for-material compaction of a completed bake. No re-simulation.
//! Empty-water grain bytes are swapped scratch values, never sediment density;
//! the material kernel and water shader do not read them.
use crate::{
    Level,
    underwater::{
        Cell,
        settled_seed::{self, Seed},
    },
};
use std::path::Path;

pub fn compact(input: &Path, output: &Path) {
    let bytes = std::fs::read(input).expect("read completed seed");
    let bytes =
        miniz_oxide::inflate::decompress_to_vec_zlib(&bytes).expect("completed seed compression");
    let mut seed: Seed = postcard::from_bytes(&bytes).expect("completed seed format");
    let level = Level::test_level();
    assert_eq!(
        seed.fingerprint,
        settled_seed::fingerprint(&level),
        "only compact a current, verified completed bake"
    );
    assert_eq!(seed.ticks, 9000);
    let before: usize = seed.columns.iter().map(Vec::len).sum();
    let mut empty = 0;
    for (x, column) in seed.columns.iter_mut().enumerate() {
        let gx = seed.origin.0 + x as i32;
        column.retain_mut(|g| {
            if g.cell != Cell::Water {
                return true;
            }
            // Keep every actual material change (including emptied bed cells),
            // all sediment identities and all suspension. Canonicalize only
            // the unused grain byte of an empty water cell.
            g.grain = settled_seed::raw_grain(gx, g.y);
            if g.suspended == 0 && settled_seed::raw_cell(&level, gx, g.y) == Cell::Water {
                empty += 1;
                false
            } else {
                true
            }
        });
    }
    let encoded = postcard::to_allocvec(&seed).expect("encode compact seed");
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&encoded, 9);
    std::fs::write(output, &compressed).expect("write compact seed");
    println!(
        "{before} patches → {} (removed{empty} empty-water identity-only patches); {} compressed bytes; sediment={} unchanged",
        seed.columns.iter().map(Vec::len).sum::<usize>(),
        compressed.len(),
        seed.sediment
    );
}
