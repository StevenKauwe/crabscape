//! The RuneScape experience table, one for every skill but Data-Driven:
//! `xp(L) = floor( sum_{l=1}^{L-1} floor(l + 300 * 2^(l/7)) / 4 )`.
//! Level 2 is 83, level 99 is 13,034,431.

/// The highest level.
pub const MAX_LEVEL: u32 = 99;

/// Experience needed for level `i` (index = level; index 0 is unused and 0).
/// `TABLE[1] = 0`, `TABLE[2] = 83`, `TABLE[99] = 13_034_431`.
pub const TABLE: [u32; 100] = [
    0, 0, 83, 174, 276, 388, 512, 650, 801, 969, 1154, 1358, 1584, 1833, 2107, 2411, 2746, 3115,
    3523, 3973, 4470, 5018, 5624, 6291, 7028, 7842, 8740, 9730, 10824, 12031, 13363, 14833, 16456,
    18247, 20224, 22406, 24815, 27473, 30408, 33648, 37224, 41171, 45529, 50339, 55649, 61512,
    67983, 75127, 83014, 91721, 101333, 111945, 123660, 136594, 150872, 166636, 184040, 203254,
    224466, 247886, 273742, 302288, 333804, 368599, 407015, 449428, 496254, 547953, 605032, 668051,
    737627, 814445, 899257, 992895, 1096278, 1210421, 1336443, 1475581, 1629200, 1798808, 1986068,
    2192818, 2421087, 2673114, 2951373, 3258594, 3597792, 3972294, 4385776, 4842295, 5346332,
    5902831, 6517253, 7195629, 7944614, 8771558, 9684577, 10692629, 11805606, 13034431,
];

/// The table, recomputed with the formula (for the test that checks the
/// literal above).
pub fn computed() -> [u32; 100] {
    let mut table = [0u32; 100];
    let mut points = 0.0f64;
    for level in 1..99usize {
        points += (level as f64 + 300.0 * (level as f64 / 7.0).exp2()).floor();
        table[level + 1] = (points / 4.0).floor() as u32;
    }
    table
}

/// The level `xp` reaches (1 to 99).
pub fn level_for(xp: u32) -> u32 {
    let mut level = 1;
    while level < MAX_LEVEL && xp >= TABLE[level as usize + 1] {
        level += 1;
    }
    level
}

/// How far through its current level `xp` is, 0 to 1.
pub fn progress(xp: u32) -> f32 {
    let level = level_for(xp);
    if level >= MAX_LEVEL {
        return 1.0;
    }
    let (from, to) = (TABLE[level as usize], TABLE[level as usize + 1]);
    ((xp - from) as f32 / (to - from) as f32).clamp(0.0, 1.0)
}

/// Experience still needed for the next level (0 at 99).
pub fn xp_to_next(xp: u32) -> u32 {
    let level = level_for(xp);
    if level >= MAX_LEVEL {
        0
    } else {
        TABLE[level as usize + 1] - xp
    }
}

/// The experience at which `level` begins.
pub fn xp_for_level(level: u32) -> u32 {
    TABLE[level.clamp(1, MAX_LEVEL) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_literal_matches_the_formula() {
        assert_eq!(computed(), TABLE);
    }

    #[test]
    fn spot_values_from_the_spec() {
        for (level, xp) in [
            (1, 0),
            (2, 83),
            (5, 388),
            (10, 1154),
            (15, 2411),
            (20, 4470),
            (30, 13363),
            (45, 61512),
            (60, 273742),
            (92, 6517253),
            (99, 13034431),
        ] {
            assert_eq!(TABLE[level], xp, "level {level}");
            assert_eq!(level_for(xp), level as u32);
            assert_eq!(level_for(xp.saturating_sub(1)), (level as u32).max(2) - 1);
        }
    }

    #[test]
    fn progress_and_next() {
        assert_eq!(xp_to_next(0), 83);
        assert_eq!(xp_to_next(13_034_431), 0);
        assert_eq!(level_for(u32::MAX), 99);
        assert!((progress(41) - 41.0 / 83.0).abs() < 1e-6);
        assert_eq!(progress(u32::MAX), 1.0);
    }
}
