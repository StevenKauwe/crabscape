//! The browser's `mulberry` generator, with explicit serializable replay state.
//! Live gameplay currently uses Math.random; comparison runs must inject the same
//! draws on both sides. This does not by itself reproduce an old live mission.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Mulberry32 {
    pub state: u32,
}
impl Mulberry32 {
    pub fn next_unit(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6D2B79F5);
        let mut t = (self.state ^ (self.state >> 15)).wrapping_mul(1 | self.state);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t));
        f64::from(t ^ (t >> 14)) / 4294967296.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_restores_next_draw() {
        let mut rng = Mulberry32 { state: 42 };
        for _ in 0..17 {
            rng.next_unit();
        }
        let mut copy = rng;
        for _ in 0..1000 {
            assert_eq!(rng.next_unit(), copy.next_unit());
        }
    }
}
