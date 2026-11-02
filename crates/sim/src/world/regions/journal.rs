//! A delegated region's checkpoint and acknowledged ordered input history.
//! Receiving inputs does not run its physics on the lobby owner's browser.
use super::*;

const MAX_TICKS: u64 = 120 * 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Run {
    first: u64,
    count: u32,
    #[serde(deserialize_with = "read_controls")]
    controls: Vec<(PlayerId, Controls)>,
    #[serde(deserialize_with = "read_actions")]
    actions: Vec<(PlayerId, CellSessionAction)>,
}

#[derive(Serialize, Deserialize)]
pub struct RegionJournal {
    id: RegionId,
    epoch: RegionEpoch,
    authority: PlayerId,
    #[serde(deserialize_with = "read_members")]
    members: Vec<PlayerId>,
    base_tick: u64,
    next_tick: u64,
    #[serde(deserialize_with = "read_checkpoint")]
    checkpoint: Vec<u8>,
    #[serde(deserialize_with = "read_runs")]
    runs: Vec<Run>,
}
fn read_members<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<PlayerId>, D::Error> {
    read_list::<D, _, 8>(d)
}
fn read_checkpoint<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    read_list::<D, _, { 16 * 1024 * 1024 }>(d)
}
fn read_runs<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Run>, D::Error> {
    read_list::<D, _, { MAX_TICKS as usize }>(d)
}
impl RegionJournal {
    pub fn new(kernel: &RegionKernel, authority: PlayerId) -> Result<Self, RegionError> {
        let members = kernel.members();
        if members.is_empty() || members.len() > 8 {
            return Err(RegionError::InvalidRoster);
        }
        Ok(Self {
            id: kernel.id,
            epoch: kernel.epoch,
            authority,
            members,
            base_tick: kernel.sim.tick_count(),
            next_tick: kernel.sim.tick_count(),
            checkpoint: kernel.checkpoint()?,
            runs: Vec::new(),
        })
    }
    pub fn id(&self) -> RegionId {
        self.id
    }
    pub fn epoch(&self) -> RegionEpoch {
        self.epoch
    }
    pub fn acknowledged_tick(&self) -> u64 {
        self.next_tick
    }
    pub fn pending_ticks(&self) -> u64 {
        self.next_tick - self.base_tick
    }

    /// Validate the complete batch before appending any of it. Epoch and
    /// authority checks prevent a departed browser writing after handover.
    pub fn append(
        &mut self,
        authority: PlayerId,
        epoch: RegionEpoch,
        frames: &[RegionFrame],
    ) -> Result<(), RegionError> {
        if authority != self.authority
            || epoch != self.epoch
            || frames.len() > 4
            || self.pending_ticks() + frames.len() as u64 > MAX_TICKS
        {
            return Err(RegionError::InvalidFrame);
        }
        for (i, frame) in frames.iter().enumerate() {
            if frame.tick != self.next_tick + i as u64
                || frame.controls.len() != self.members.len()
                || frame
                    .controls
                    .iter()
                    .zip(&self.members)
                    .any(|((id, c), expected)| {
                        id != expected || !c.walk.is_finite() || !(-1.0..=1.0).contains(&c.walk)
                    })
                || frame.actions.len() > 32
                || frame.actions.iter().any(|(id, a)| {
                    !self.members.contains(id) || matches!(a, CellSessionAction::Home)
                })
            {
                return Err(RegionError::InvalidFrame);
            }
        }
        for frame in frames {
            if let Some(run) = self
                .runs
                .last_mut()
                .filter(|r| r.controls == frame.controls && r.actions == frame.actions)
            {
                run.count += 1;
            } else {
                self.runs.push(Run {
                    first: frame.tick,
                    count: 1,
                    controls: frame.controls.clone(),
                    actions: frame.actions.clone(),
                });
            }
            self.next_tick += 1;
        }
        Ok(())
    }
    /// Reconstruct only when this region is needed for a visit, merge,
    /// recovery or a new checkpoint. Normal receipt remains inexpensive.
    pub fn materialize(&self) -> Result<RegionKernel, RegionError> {
        let mut kernel = RegionKernel::from_checkpoint(&self.checkpoint)?;
        for run in &self.runs {
            for offset in 0..run.count {
                kernel.tick(&RegionFrame {
                    tick: run.first + u64::from(offset),
                    controls: run.controls.clone(),
                    actions: run.actions.clone(),
                })?;
            }
        }
        if kernel.sim.tick_count() != self.next_tick {
            return Err(RegionError::InvalidState);
        }
        Ok(kernel)
    }
    pub fn compact(&mut self) -> Result<(), RegionError> {
        let kernel = self.materialize()?;
        let checkpoint = kernel.checkpoint()?;
        self.checkpoint = checkpoint;
        self.base_tick = self.next_tick;
        self.runs.clear();
        Ok(())
    }
    pub fn replace_checkpoint(
        &mut self,
        authority: PlayerId,
        epoch: RegionEpoch,
        bytes: Vec<u8>,
    ) -> Result<(), RegionError> {
        if authority != self.authority || epoch != self.epoch {
            return Err(RegionError::InvalidFrame);
        }
        let state = RegionKernel::from_checkpoint(&bytes)?;
        if state.id != self.id
            || state.epoch != self.epoch
            || state.sim.tick_count() != self.next_tick
            || state.members() != self.members
        {
            return Err(RegionError::InvalidState);
        }
        self.checkpoint = bytes;
        self.base_tick = self.next_tick;
        self.runs.clear();
        Ok(())
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, RegionError> {
        if postcard::experimental::serialized_size(self).map_err(|_| RegionError::InvalidState)?
            > 24 * 1024 * 1024
        {
            return Err(RegionError::TooLarge);
        }
        let raw = postcard::to_allocvec(self).map_err(|_| RegionError::InvalidState)?;
        let mut out = b"CRBJNL01".to_vec();
        out.extend(miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1));
        Ok(out)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RegionError> {
        if !bytes.starts_with(b"CRBJNL01") || bytes.len() > 24 * 1024 * 1024 {
            return Err(RegionError::InvalidState);
        }
        let raw =
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[8..], 24 * 1024 * 1024)
                .map_err(|_| RegionError::InvalidState)?;
        let restored: Self = postcard::from_bytes(&raw).map_err(|_| RegionError::InvalidState)?;
        let base = RegionKernel::from_checkpoint(&restored.checkpoint)?;
        if base.id != restored.id
            || base.epoch != restored.epoch
            || base.members() != restored.members
            || base.sim.tick_count() != restored.base_tick
            || restored.next_tick < restored.base_tick
            || restored.next_tick - restored.base_tick > MAX_TICKS
        {
            return Err(RegionError::InvalidState);
        }
        let mut next = restored.base_tick;
        for run in &restored.runs {
            if run.first != next
                || run.count == 0
                || run.controls.len() != restored.members.len()
                || run
                    .controls
                    .iter()
                    .zip(&restored.members)
                    .any(|((id, c), expected)| {
                        id != expected || !c.walk.is_finite() || !(-1.0..=1.0).contains(&c.walk)
                    })
                || run.actions.iter().any(|(id, a)| {
                    !restored.members.contains(id) || matches!(a, CellSessionAction::Home)
                })
            {
                return Err(RegionError::InvalidState);
            }
            next = next
                .checked_add(u64::from(run.count))
                .ok_or(RegionError::InvalidState)?;
        }
        if next != restored.next_tick {
            return Err(RegionError::InvalidState);
        }
        Ok(restored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::underwater::region_store::TerrainStore;
    #[test]
    fn acknowledged_inputs_recover_exactly_after_authority_loss_and_bad_batches_are_atomic() {
        let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
        sim.enable_underwater_cells();
        sim.populate();
        let mut terrain = TerrainStore::new(sim.level.cells.take().unwrap()).unwrap();
        let mut archive = EntityArchive::new(sim).unwrap();
        let (epoch, world) = terrain
            .grant(
                RegionId(1),
                CellRegion {
                    x: 0,
                    y: 0,
                    width: 256,
                    height: 256,
                },
                &archive.sim.level,
            )
            .unwrap();
        let mut live = archive
            .extract(RegionId(1), epoch, &[PlayerId(0)], world)
            .unwrap();
        let mut journal = RegionJournal::new(&live, PlayerId(0)).unwrap();
        for n in 0..60 {
            let mut frames = Vec::new();
            for offset in 0..4 {
                let tick = n * 4 + offset;
                let frame = RegionFrame {
                    tick,
                    controls: vec![(
                        PlayerId(0),
                        Controls {
                            walk: if n < 30 { 0.25 } else { -0.25 },
                            dig_held: n % 10 > 5,
                            ..Default::default()
                        },
                    )],
                    actions: vec![],
                };
                live.tick(&frame).unwrap();
                frames.push(frame);
            }
            journal.append(PlayerId(0), epoch, &frames).unwrap();
        }
        assert_eq!(journal.acknowledged_tick(), 240);
        let recovered = journal.materialize().unwrap();
        assert_eq!(live.state_hash(), recovered.state_hash());
        let saved = journal.to_bytes().unwrap();
        let reloaded = RegionJournal::from_bytes(&saved).unwrap();
        assert_eq!(
            live.state_hash(),
            reloaded.materialize().unwrap().state_hash()
        );
        assert!(RegionJournal::from_bytes(&saved[..saved.len() - 1]).is_err());
        let invalid = RegionFrame {
            tick: 241,
            controls: vec![(PlayerId(0), Controls::default())],
            actions: vec![],
        };
        let valid = RegionFrame {
            tick: 240,
            ..invalid.clone()
        };
        assert!(
            journal
                .append(
                    PlayerId(0),
                    epoch,
                    &[valid.clone(), invalid.clone(), invalid]
                )
                .is_err()
        );
        assert_eq!(journal.acknowledged_tick(), 240);
        assert!(
            journal
                .append(PlayerId(7), epoch, &[valid.clone()])
                .is_err()
        );
        assert!(
            journal
                .append(PlayerId(0), RegionEpoch(epoch.0 + 1), &[valid])
                .is_err()
        );
        journal.compact().unwrap();
        assert_eq!(journal.pending_ticks(), 0);
        assert_eq!(
            live.state_hash(),
            journal.materialize().unwrap().state_hash()
        );
    }
}
