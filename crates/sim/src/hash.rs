//! A tiny, fixed hash function (FNV-1a, 64 bit).
//!
//! Used for "fingerprints": a short number that changes when the data
//! changes. Rust's own `HashMap` hasher is deliberately different on every
//! run, and may change between Rust versions, so it can't be written into a
//! file or compared between computers. This one never changes.

/// Hash some bytes.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        // From the FNV reference test vectors.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
