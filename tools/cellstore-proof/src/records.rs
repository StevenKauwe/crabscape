//! Fixed-layout metadata for the isolated IndexedDB experiment.
//! Lengths are checked before allocation; this is not a game save format.
use sha2::{Digest, Sha256};
const MAX_PLAYER: usize = 64 * 1024;
const MAX_PARTS: usize = 512;
const DESCRIPTOR: usize = 36;
const STAGE_HEADER: usize = 40;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    pub epoch: u64,
    pub revision: u64,
    pub next_id: u64,
    pub head_id: u64,
    pub stage_id: u64,
    pub pin_token: u64,
    pub head_guarded: bool,
    pub pin_guarded: bool,
    pub stage_guarded: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub digest: [u8; 32],
    pub len: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stage {
    pub epoch: u64,
    pub base: u64,
    pub id: u64,
    pub tick: u64,
    pub player: Vec<u8>,
    pub parts: Vec<Part>,
}
impl Meta {
    pub const ENCODED_LEN: usize = 51 + 32;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = [
            self.epoch.to_le_bytes(),
            self.revision.to_le_bytes(),
            self.next_id.to_le_bytes(),
            self.head_id.to_le_bytes(),
            self.stage_id.to_le_bytes(),
            self.pin_token.to_le_bytes(),
        ]
        .concat();
        b.push(u8::from(self.head_guarded));
        b.push(u8::from(self.pin_guarded));
        b.push(u8::from(self.stage_guarded));
        b.extend_from_slice(&digest(&b));
        b
    }
    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() != Self::ENCODED_LEN
            || digest(&b[..51]) != b[51..]
            || b[48] > 1
            || b[49] > 1
            || b[50] > 1
        {
            return None;
        }
        Some(Self {
            epoch: u64::from_le_bytes(b[0..8].try_into().ok()?),
            revision: u64::from_le_bytes(b[8..16].try_into().ok()?),
            next_id: u64::from_le_bytes(b[16..24].try_into().ok()?),
            head_id: u64::from_le_bytes(b[24..32].try_into().ok()?),
            stage_id: u64::from_le_bytes(b[32..40].try_into().ok()?),
            pin_token: u64::from_le_bytes(b[40..48].try_into().ok()?),
            head_guarded: b[48] == 1,
            pin_guarded: b[49] == 1,
            stage_guarded: b[50] == 1,
        })
    }
}
impl Stage {
    pub const MAX_ENCODED: usize = STAGE_HEADER + MAX_PLAYER + MAX_PARTS * DESCRIPTOR + 32;
    pub fn encode(&self) -> Option<Vec<u8>> {
        if self.player.len() > MAX_PLAYER || self.parts.len() > MAX_PARTS {
            return None;
        }
        let mut b = Vec::with_capacity(
            STAGE_HEADER + self.player.len() + self.parts.len() * DESCRIPTOR + 32,
        );
        for n in [self.epoch, self.base, self.id, self.tick] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        b.extend_from_slice(&(self.player.len() as u32).to_le_bytes());
        b.extend_from_slice(&(self.parts.len() as u32).to_le_bytes());
        b.extend_from_slice(&self.player);
        for part in &self.parts {
            b.extend_from_slice(&part.digest);
            b.extend_from_slice(&part.len.to_le_bytes());
        }
        b.extend_from_slice(&digest(&b));
        Some(b)
    }
    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() < STAGE_HEADER + 32 || b.len() > Self::MAX_ENCODED {
            return None;
        }
        let read_u64 = |i: usize| Some(u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?));
        let plen = u32::from_le_bytes(b[32..36].try_into().ok()?) as usize;
        let count = u32::from_le_bytes(b[36..40].try_into().ok()?) as usize;
        if plen > MAX_PLAYER
            || count > MAX_PARTS
            || b.len() != STAGE_HEADER + plen + count * DESCRIPTOR + 32
        {
            return None;
        }
        let payload_len = b.len() - 32;
        if digest(&b[..payload_len]) != b[payload_len..] {
            return None;
        }
        let player = b[STAGE_HEADER..STAGE_HEADER + plen].to_vec();
        let mut parts = Vec::with_capacity(count);
        for chunk in b[STAGE_HEADER + plen..payload_len]
            .as_chunks::<DESCRIPTOR>()
            .0
        {
            parts.push(Part {
                digest: chunk[..32].try_into().ok()?,
                len: u32::from_le_bytes(chunk[32..36].try_into().ok()?),
            });
        }
        Some(Self {
            epoch: read_u64(0)?,
            base: read_u64(8)?,
            id: read_u64(16)?,
            tick: read_u64(24)?,
            player,
            parts,
        })
    }
}
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_reject_truncation_oversized_lengths_and_trailing_bytes() {
        let stage = Stage {
            epoch: 2,
            base: 3,
            id: 4,
            tick: 5,
            player: vec![1, 2],
            parts: vec![Part {
                digest: digest(b"x"),
                len: 1,
            }],
        };
        let raw = stage.encode().unwrap();
        assert_eq!(Stage::decode(&raw), Some(stage));
        for n in 0..raw.len() {
            assert!(Stage::decode(&raw[..n]).is_none());
        }
        let mut bad = raw.clone();
        bad[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Stage::decode(&bad).is_none());
        let mut extra = raw;
        extra.push(0);
        assert!(Stage::decode(&extra).is_none());
        assert!(Meta::decode(&[0; 23]).is_none());
        let meta = Meta {
            epoch: 1,
            revision: 2,
            next_id: 3,
            head_id: 2,
            stage_id: 3,
            pin_token: 0,
            head_guarded: false,
            pin_guarded: false,
            stage_guarded: false,
        };
        let mut encoded = meta.encode();
        assert_eq!(Meta::decode(&encoded), Some(meta));
        encoded[8] ^= 1;
        assert!(Meta::decode(&encoded).is_none());
    }
}
