//! A delegated region's checkpoint and acknowledged ordered input history.
//! Receiving inputs does not run its physics on the lobby owner's browser.
use super::*;

const MAX_TICKS: u64 = 120 * 60;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum StateChange {
    Move {
        revision: u64,
        #[serde(deserialize_with = "read_checkpoint")]
        bytes: Vec<u8>,
    },
    FallsCommitted(u64),
    Arrival(super::falling::Arrival),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct StateEvent {
    tick: u64,
    change: StateChange,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Run {
    first: u64,
    count: u32,
    #[serde(deserialize_with = "read_controls")]
    controls: Vec<(PlayerId, Controls)>,
    #[serde(deserialize_with = "read_actions")]
    actions: Vec<(PlayerId, CellSessionAction)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegionJournal {
    base_revision: u64,
    base_fall_committed: u64,
    base_arrivals_delivered: u64,
    base_state_revision: u64,
    #[serde(deserialize_with = "read_events")]
    events: Vec<StateEvent>,
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
fn read_events<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<StateEvent>, D::Error> {
    read_list::<D, _, { MAX_TICKS as usize }>(d)
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
            base_revision: kernel.revision(),
            base_fall_committed: kernel.fall_committed(),
            base_arrivals_delivered: kernel.arrivals_delivered(),
            base_state_revision: kernel.state_revision(),
            events: Vec::new(),
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
    /// Start a journal and transfer the same freshly encoded base checkpoint.
    /// The returned bytes are owned independently; receivers still validate them.
    pub fn new_with_checkpoint(
        kernel: &RegionKernel,
        authority: PlayerId,
    ) -> Result<(Self, Vec<u8>), RegionError> {
        let journal = Self::new(kernel, authority)?;
        let checkpoint = journal.checkpoint.clone();
        Ok((journal, checkpoint))
    }

    fn metadata_at(&self, state: u64) -> (u64, u64, u64) {
        let mut result = (
            self.base_revision,
            self.base_fall_committed,
            self.base_arrivals_delivered,
        );
        for event in self
            .events
            .iter()
            .take(state.saturating_sub(self.base_state_revision) as usize)
        {
            match &event.change {
                StateChange::Move { revision, .. } => result.0 = *revision,
                StateChange::FallsCommitted(n) => result.1 = *n,
                StateChange::Arrival(a) => result.2 = a.delivery(),
            }
        }
        result
    }
    pub fn state_revision(&self) -> u64 {
        self.base_state_revision + self.events.len() as u64
    }
    pub fn revision(&self) -> u64 {
        self.metadata_at(self.state_revision()).0
    }
    pub fn fall_committed(&self) -> u64 {
        self.metadata_at(self.state_revision()).1
    }
    pub fn arrivals_delivered(&self) -> u64 {
        self.metadata_at(self.state_revision()).2
    }
    fn note(
        &mut self,
        authority: PlayerId,
        tick: u64,
        change: StateChange,
    ) -> Result<(), RegionError> {
        if authority != self.authority
            || tick != self.next_tick
            || self.events.len() >= MAX_TICKS as usize
            || self.state_revision() == u64::MAX
        {
            return Err(RegionError::InvalidFrame);
        }
        self.events.push(StateEvent { tick, change });
        Ok(())
    }
    pub fn note_falls_committed(
        &mut self,
        authority: PlayerId,
        tick: u64,
        committed: u64,
    ) -> Result<(), RegionError> {
        if committed <= self.fall_committed() {
            return Err(RegionError::InvalidFrame);
        }
        self.note(authority, tick, StateChange::FallsCommitted(committed))
    }
    pub fn note_arrival(
        &mut self,
        authority: PlayerId,
        tick: u64,
        input: super::falling::Arrival,
    ) -> Result<(), RegionError> {
        if input.id() != self.id
            || input.epoch() != self.epoch
            || input.revision() != self.revision()
            || self.arrivals_delivered().checked_add(1) != Some(input.delivery())
        {
            return Err(RegionError::InvalidFrame);
        }
        self.note(authority, tick, StateChange::Arrival(input))
    }
    pub fn note_move(
        &mut self,
        authority: PlayerId,
        tick: u64,
        input: super::moving::MoveIn,
    ) -> Result<(), RegionError> {
        if input.id() != self.id
            || input.epoch() != self.epoch
            || self.revision().checked_add(1) != Some(input.revision())
        {
            return Err(RegionError::InvalidFrame);
        }
        self.note(
            authority,
            tick,
            StateChange::Move {
                revision: input.revision(),
                bytes: input.to_bytes()?,
            },
        )
    }
    fn apply_change(kernel: &mut RegionKernel, change: &StateChange) -> Result<(), RegionError> {
        match change {
            StateChange::Move { bytes, .. } => kernel
                .rewindow(&super::moving::MoveIn::from_bytes(bytes)?)
                .map(drop),
            StateChange::FallsCommitted(n) => kernel.commit_falls(*n),
            StateChange::Arrival(a) => kernel.receive_falls(a),
        }
    }
    fn apply_events(&self, kernel: &mut RegionKernel) -> Result<(), RegionError> {
        for (i, event) in self.events.iter().enumerate() {
            let order = self.base_state_revision + i as u64 + 1;
            if event.tick == kernel.sim.tick_count() && order > kernel.state_revision() {
                Self::apply_change(kernel, &event.change)?;
                if kernel.state_revision() != order {
                    return Err(RegionError::InvalidState);
                }
            }
        }
        Ok(())
    }
    fn reset_base(&mut self, kernel: &RegionKernel) {
        self.base_revision = kernel.revision();
        self.base_fall_committed = kernel.fall_committed();
        self.base_arrivals_delivered = kernel.arrivals_delivered();
        self.base_state_revision = kernel.state_revision();
        self.events.clear();
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
        self.apply_events(&mut kernel)?;
        for run in &self.runs {
            for offset in 0..run.count {
                kernel.tick(&RegionFrame {
                    tick: run.first + u64::from(offset),
                    controls: run.controls.clone(),
                    actions: run.actions.clone(),
                })?;
                self.apply_events(&mut kernel)?;
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
        self.reset_base(&kernel);
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
            || state.revision() != self.revision()
            || state.fall_committed() != self.fall_committed()
            || state.state_revision() != self.state_revision()
            || state.sim.tick_count() != self.next_tick
            || state.members() != self.members
        {
            return Err(RegionError::InvalidState);
        }
        self.reset_base(&state);
        self.checkpoint = bytes;
        self.base_tick = self.next_tick;
        self.runs.clear();
        Ok(())
    }
    /// Compact from the authority's live kernel without decoding the bytes
    /// that were just encoded. External checkpoints must still use the
    /// validating byte-based replacement/advance methods.
    pub fn replace_from_local(
        &mut self,
        authority: PlayerId,
        kernel: &RegionKernel,
    ) -> Result<(), RegionError> {
        if authority != self.authority || kernel.epoch != self.epoch {
            return Err(RegionError::InvalidFrame);
        }
        if kernel.id != self.id
            || kernel.revision() != self.revision()
            || kernel.fall_committed() != self.fall_committed()
            || kernel.state_revision() != self.state_revision()
            || kernel.sim.tick_count() != self.next_tick
            || kernel.members() != self.members
        {
            return Err(RegionError::InvalidState);
        }
        // Encode before changing the retained base, so a failed capture keeps
        // the previous checkpoint and complete acknowledged journal intact.
        let checkpoint = kernel.checkpoint()?;
        self.reset_base(kernel);
        self.checkpoint = checkpoint;
        self.base_tick = self.next_tick;
        self.runs.clear();
        Ok(())
    }

    /// The installed browser view holds the actual simulation; the kernel
    /// contains its ownership metadata and an empty placeholder simulation.
    pub fn replace_from_local_view(
        &mut self,
        authority: PlayerId,
        kernel: &mut RegionKernel,
        view: &mut Sim,
    ) -> Result<(), RegionError> {
        std::mem::swap(&mut kernel.sim, view);
        let result = self.replace_from_local(authority, kernel);
        std::mem::swap(&mut kernel.sim, view);
        result
    }
    pub fn advance_checkpoint(
        &mut self,
        authority: PlayerId,
        epoch: RegionEpoch,
        bytes: Vec<u8>,
    ) -> Result<(), RegionError> {
        self.advance_checkpoint_retain(authority, epoch, bytes)
            .map(drop)
    }
    pub fn advance_checkpoint_retain(
        &mut self,
        authority: PlayerId,
        epoch: RegionEpoch,
        bytes: Vec<u8>,
    ) -> Result<RegionKernel, RegionError> {
        if authority != self.authority || epoch != self.epoch {
            return Err(RegionError::InvalidFrame);
        }
        let state = RegionKernel::from_checkpoint(&bytes)?;
        let tick = state.sim.tick_count();
        if state.id != self.id
            || state.epoch != self.epoch
            || state.members() != self.members
            || state.state_revision() < self.base_state_revision
            || state.state_revision() > self.state_revision()
            || self.metadata_at(state.state_revision())
                != (
                    state.revision(),
                    state.fall_committed(),
                    state.arrivals_delivered(),
                )
            || self.events.iter().enumerate().any(|(i, e)| {
                let order = self.base_state_revision + i as u64 + 1;
                (order <= state.state_revision() && e.tick > tick)
                    || (order > state.state_revision() && e.tick < tick)
            })
            || tick < self.base_tick
            || tick > self.next_tick
        {
            return Err(RegionError::InvalidState);
        }
        let mut runs = Vec::new();
        for run in &self.runs {
            let end = run.first + u64::from(run.count);
            if end <= tick {
                continue;
            }
            let mut kept = run.clone();
            if kept.first < tick {
                kept.count = (end - tick) as u32;
                kept.first = tick;
            }
            runs.push(kept);
        }
        let kept = self
            .events
            .split_off((state.state_revision() - self.base_state_revision) as usize);
        self.reset_base(&state);
        self.events = kept;
        self.checkpoint = bytes;
        self.base_tick = tick;
        self.runs = runs;
        Ok(state)
    }
    pub fn base_kernel(&self) -> Result<RegionKernel, RegionError> {
        RegionKernel::from_checkpoint(&self.checkpoint)
    }
    pub fn to_text(&self) -> Result<String, RegionError> {
        Ok(format!(
            "region_session_v2 {}\n",
            crate::memory::base64(&self.to_bytes()?)
        ))
    }
    pub fn from_text(text: &str) -> Result<Self, RegionError> {
        if text.len() > 32 * 1024 * 1024 {
            return Err(RegionError::TooLarge);
        }
        let encoded = text
            .trim()
            .strip_prefix("region_session_v2 ")
            .ok_or(RegionError::InvalidState)?;
        let bytes = crate::memory::unbase64(encoded).ok_or(RegionError::InvalidState)?;
        Self::from_bytes(&bytes)
    }
    pub fn prepare_replay(&self, sim: &mut Sim) -> bool {
        match self.base_kernel() {
            Ok(mut kernel) => {
                if self.apply_events(&mut kernel).is_err() {
                    return false;
                }
                *sim = kernel.sim;
                true
            }
            Err(_) => false,
        }
    }
    pub fn step_replay(&self, sim: &mut Sim) -> bool {
        let tick = sim.tick_count();
        let i = self.runs.partition_point(|r| r.first <= tick);
        let Some(run) = i
            .checked_sub(1)
            .and_then(|i| self.runs.get(i))
            .filter(|r| tick < r.first + u64::from(r.count))
        else {
            return false;
        };
        for (id, action) in &run.actions {
            match action {
                CellSessionAction::Home => return false,
                CellSessionAction::Code(code) => {
                    sim.enter_code(*id, *code);
                }
                CellSessionAction::Moult(skill) => {
                    sim.moult(*id, *skill);
                }
                CellSessionAction::Craft(recipe) => {
                    let _ = sim.make(*id, *recipe);
                }
                CellSessionAction::Shell(action) => {
                    sim.shell_action(*id, *action);
                }
                CellSessionAction::Salvage(action) => {
                    sim.salvage_action(*id, *action);
                }
                CellSessionAction::Canyon(action) => {
                    sim.canyon_action(*id, *action);
                }
            }
        }
        for (id, control) in &run.controls {
            sim.set_controls(*id, *control);
        }
        sim.tick_fixed_terrain();
        self.apply_replay_events(sim)
    }
    fn apply_replay_events(&self, sim: &mut Sim) -> bool {
        let tick = sim.tick_count();
        for (i, event) in self
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.tick == tick)
        {
            let state = self.base_state_revision + i as u64;
            let (revision, fall_committed, arrivals_delivered) = self.metadata_at(state);
            let placeholder = Sim::new(sim.tuning.clone(), crate::Level::test_level());
            let mut kernel = RegionKernel {
                id: self.id,
                epoch: self.epoch,
                revision,
                fall_committed,
                arrivals_delivered,
                state_revision: state,
                sim: std::mem::replace(sim, placeholder),
            };
            let result = Self::apply_change(&mut kernel, &event.change);
            *sim = kernel.sim;
            if result.is_err() {
                return false;
            }
        }
        true
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
            || base.revision() != restored.base_revision
            || base.fall_committed() != restored.base_fall_committed
            || base.arrivals_delivered() != restored.base_arrivals_delivered
            || base.state_revision() != restored.base_state_revision
            || restored
                .base_state_revision
                .checked_add(restored.events.len() as u64)
                .is_none()
            || base.epoch != restored.epoch
            || base.members() != restored.members
            || base.sim.tick_count() != restored.base_tick
            || restored.next_tick < restored.base_tick
            || restored.next_tick - restored.base_tick > MAX_TICKS
        {
            return Err(RegionError::InvalidState);
        }
        let mut window = restored.base_revision;
        let mut committed = restored.base_fall_committed;
        let mut arrivals = restored.base_arrivals_delivered;
        let mut event_tick = restored.base_tick;
        for event in &restored.events {
            if event.tick < event_tick || event.tick > restored.next_tick {
                return Err(RegionError::InvalidState);
            }
            event_tick = event.tick;
            match &event.change {
                StateChange::Move { revision, bytes } => {
                    let input = super::moving::MoveIn::from_bytes(bytes)?;
                    if window.checked_add(1) != Some(*revision)
                        || input.revision() != *revision
                        || input.id() != restored.id
                        || input.epoch() != restored.epoch
                    {
                        return Err(RegionError::InvalidState);
                    }
                    window = *revision;
                }
                StateChange::FallsCommitted(n) => {
                    if *n <= committed {
                        return Err(RegionError::InvalidState);
                    }
                    committed = *n;
                }
                StateChange::Arrival(a) => {
                    if a.id() != restored.id
                        || a.epoch() != restored.epoch
                        || a.revision() != window
                        || arrivals.checked_add(1) != Some(a.delivery())
                    {
                        return Err(RegionError::InvalidState);
                    }
                    arrivals = a.delivery();
                }
            }
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
    fn reused_initial_checkpoint_and_compaction_keep_future_history_exact() {
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
        let (mut journal, mut checkpoint) =
            RegionJournal::new_with_checkpoint(&live, PlayerId(0)).unwrap();
        assert_eq!(
            RegionKernel::from_checkpoint(&checkpoint)
                .unwrap()
                .sim
                .state_hash(),
            live.sim.state_hash()
        );
        checkpoint.fill(0);
        assert_eq!(
            journal.materialize().unwrap().sim.state_hash(),
            live.sim.state_hash()
        );
        for tick in 0..12 {
            let frame = RegionFrame {
                tick,
                controls: vec![(PlayerId(0), Controls::default())],
                actions: vec![],
            };
            live.tick(&frame).unwrap();
            journal.append(PlayerId(0), epoch, &[frame]).unwrap();
        }
        let before = journal.clone();
        assert!(journal.replace_from_local(PlayerId(7), &live).is_err());
        let old_id = live.id;
        live.id = RegionId(99);
        assert!(journal.replace_from_local(PlayerId(0), &live).is_err());
        live.id = old_id;
        live.epoch = RegionEpoch(epoch.0 + 1);
        assert!(journal.replace_from_local(PlayerId(0), &live).is_err());
        live.epoch = epoch;
        assert!(
            journal
                .replace_from_local(PlayerId(0), &before.base_kernel().unwrap())
                .is_err()
        );
        assert!(
            journal
                .replace_checkpoint(PlayerId(0), epoch, vec![0; 32])
                .is_err()
        );
        assert_eq!(
            journal, before,
            "failed captures must not discard acknowledged inputs"
        );
        let checkpoint = live.checkpoint().unwrap();
        let mut validated = journal.clone();
        validated
            .replace_checkpoint(PlayerId(0), epoch, checkpoint)
            .unwrap();
        journal.replace_from_local(PlayerId(0), &live).unwrap();
        assert_eq!(journal, validated);

        let mut view = Sim::with_one_crab(Vec2::ZERO);
        let mut installed = live.install_view(&mut view);
        let view_hash = view.state_hash();
        let mut captured_view = before.clone();
        assert!(
            captured_view
                .replace_from_local_view(PlayerId(7), &mut installed, &mut view)
                .is_err()
        );
        assert_eq!(
            view.state_hash(),
            view_hash,
            "failed view capture restores the installed simulation"
        );
        assert_eq!(captured_view, before);
        captured_view
            .replace_from_local_view(PlayerId(0), &mut installed, &mut view)
            .unwrap();
        assert_eq!(captured_view, journal);
        assert_eq!(view.state_hash(), view_hash);
        for tick in 12..24 {
            let frame = RegionFrame {
                tick,
                controls: vec![(
                    PlayerId(0),
                    Controls {
                        walk: 0.25,
                        ..Controls::default()
                    },
                )],
                actions: vec![],
            };
            installed.tick_view(&mut view, &frame).unwrap();
            captured_view.append(PlayerId(0), epoch, &[frame]).unwrap();
        }
        assert_eq!(captured_view.pending_ticks(), 12);
        assert_eq!(
            captured_view.materialize().unwrap().sim.state_hash(),
            view.state_hash()
        );
        assert_eq!(
            RegionJournal::from_bytes(&captured_view.to_bytes().unwrap()).unwrap(),
            captured_view
        );
    }

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
        let mut halfway = None;
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
            if n == 29 {
                halfway = Some(live.checkpoint().unwrap());
            }
        }
        assert_eq!(journal.acknowledged_tick(), 240);
        let recovered = journal.materialize().unwrap();
        assert_eq!(live.state_hash(), recovered.state_hash());
        let replay = crate::Replay::parse(&journal.to_text().unwrap()).unwrap();
        let mut played = Sim::with_one_crab(replay.start);
        assert!(replay.prepare_start(&mut played));
        for _ in 0..replay.length() {
            assert!(replay.step_cell_session(&mut played));
        }
        assert_eq!(
            live.sim.state_hash(),
            played.state_hash(),
            "exported regional replay retains NPC random state"
        );
        let mut advanced = journal.clone();
        advanced
            .advance_checkpoint(PlayerId(0), epoch, halfway.unwrap())
            .unwrap();
        assert_eq!(advanced.acknowledged_tick(), 240);
        assert_eq!(advanced.pending_ticks(), 120);
        assert_eq!(
            live.state_hash(),
            advanced.materialize().unwrap().state_hash()
        );
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
                .append(PlayerId(7), epoch, std::slice::from_ref(&valid))
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
