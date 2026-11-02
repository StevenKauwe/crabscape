//! An owner-reserved airborne sediment delivery at an ordered state boundary.
use super::*;
use crate::underwater::FallPacket;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Arrival {
    pub(crate) id: RegionId,
    pub(crate) epoch: RegionEpoch,
    pub(crate) revision: u64,
    pub(crate) delivery: u64,
    pub(crate) source_tick: u64,
    pub(crate) bounds: CellRegion,
    #[serde(deserialize_with = "read_packets")]
    pub(crate) packets: Vec<FallPacket>,
    #[serde(deserialize_with = "read_bottom")]
    pub(crate) bottom: Vec<u8>,
}
fn read_packets<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<FallPacket>, D::Error> {
    read_list::<D, _, 64>(d)
}
fn read_bottom<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    read_list::<D, _, 256>(d)
}
impl Arrival {
    pub fn id(&self) -> RegionId {
        self.id
    }
    pub fn epoch(&self) -> RegionEpoch {
        self.epoch
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn delivery(&self) -> u64 {
        self.delivery
    }
    pub fn source_tick(&self) -> u64 {
        self.source_tick
    }
    pub fn packets(&self) -> &[FallPacket] {
        &self.packets
    }
    pub fn bounds(&self) -> CellRegion {
        self.bounds
    }
    pub(crate) fn pack_bottom(mask: &[bool]) -> Vec<u8> {
        let mut out = vec![0; mask.len().div_ceil(8)];
        for (i, b) in mask.iter().enumerate() {
            if *b {
                out[i / 8] |= 1 << (i % 8);
            }
        }
        out
    }
    pub(crate) fn bottom_mask(&self) -> Vec<bool> {
        (0..self.bounds.width)
            .map(|i| self.bottom[i / 8] & (1 << (i % 8)) != 0)
            .collect()
    }
}
impl RegionKernel {
    pub fn receive_falls_view(
        &mut self,
        view: &mut Sim,
        arrival: &Arrival,
    ) -> Result<(), RegionError> {
        std::mem::swap(&mut self.sim, view);
        let result = self.receive_falls(arrival);
        std::mem::swap(&mut self.sim, view);
        result
    }
    pub fn receive_falls(&mut self, arrival: &Arrival) -> Result<(), RegionError> {
        if arrival.id != self.id
            || arrival.epoch != self.epoch
            || arrival.revision != self.revision
            || arrival.bounds != self.bounds()
        {
            return Err(RegionError::InvalidState);
        }
        if arrival.delivery <= self.arrivals_delivered {
            return Ok(());
        }
        if self.arrivals_delivered.checked_add(1) != Some(arrival.delivery)
            || arrival.packets.len() > 64
            || arrival.bottom.len() != arrival.bounds.width.div_ceil(8)
            || arrival.packets.iter().any(|p| {
                p.advanced_tick > arrival.source_tick
                    || !(arrival.bounds.contains(p.x, p.y)
                        || arrival.bounds.contains(p.x, p.y.saturating_sub(1)))
            })
        {
            return Err(RegionError::InvalidState);
        }
        let world = self
            .sim
            .level
            .cells
            .as_mut()
            .ok_or(RegionError::InvalidState)?;
        let now = world.tick_count();
        let packets: Vec<_> = arrival
            .packets
            .iter()
            .map(|p| {
                let mut p = *p;
                let debt = arrival.source_tick.saturating_sub(p.advanced_tick);
                p.advanced_tick = now.saturating_sub(debt);
                p.emitted_tick = p.advanced_tick;
                p
            })
            .collect();
        if !world.enqueue_falls(&packets) || !world.set_bottom_open(arrival.bottom_mask()) {
            return Err(RegionError::InvalidState);
        }
        self.arrivals_delivered = arrival.delivery;
        self.state_revision += 1;
        // The next material step advances the delivery within owned terrain.
        // No write outside the lease occurs during receipt.

        Ok(())
    }
}
