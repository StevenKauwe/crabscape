//! The simulation's only source of randomness: a small seeded generator
//! (SplitMix64) stored in the [`Sim`](crate::Sim) and part of its state hash.
//! Same seed, same numbers, on every computer and in the browser.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A whole number from `lo` to `hi`, both included.
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() % u64::from(hi - lo + 1)) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_numbers_and_in_range() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..100 {
            let n = a.range(3, 9);
            assert_eq!(n, b.range(3, 9));
            assert!((3..=9).contains(&n));
        }
        // SplitMix64's first output for seed 0 (reference value).
        assert_eq!(Rng::new(0).next_u64(), 0xe220_a839_7b1d_cdaf);
    }
}
