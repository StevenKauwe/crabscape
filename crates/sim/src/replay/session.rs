//! Exact bounded recordings of canonical two-player cellular ticks.
use crate::{Controls, PlayerId, Sim};
use serde::{Deserialize, Serialize};

pub const MAX_CELL_SESSION_TEXT: usize = 4 * 1024 * 1024 - 128;
const MAX_RAW: usize = 8 * 1024 * 1024;
const MAX_RUNS: usize = 16_384;
const MAX_TICKS: u64 = 120 * 60 * 60;
const PREFIX: &str = "cell_session_v1 ";

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CellSessionAction {
    Home,
    Code(crate::codes::Code),
    Moult(crate::skills::Skill),
    Craft(crate::craft::Recipe),
    Salvage(crate::salvage::SalvageAction),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellSessionTick {
    pub controls: [Controls; 2],
    pub actions: [Option<(PlayerId, CellSessionAction)>; 4],
}
impl CellSessionTick {
    fn valid(&self, host: PlayerId, guest: PlayerId) -> bool {
        self.controls
            .iter()
            .all(|c| c.walk.is_finite() && (-1.0..=1.0).contains(&c.walk))
            && self
                .actions
                .iter()
                .flatten()
                .all(|(id, _)| *id == host || *id == guest)
    }
    fn apply(&self, sim: &mut Sim, host: PlayerId, guest: PlayerId) {
        for (id, action) in self.actions.iter().flatten() {
            match action {
                CellSessionAction::Home => {
                    sim.go_home(*id);
                }
                CellSessionAction::Code(code) => {
                    sim.enter_code(*id, *code);
                }
                CellSessionAction::Moult(skill) => {
                    sim.moult(*id, *skill);
                }
                CellSessionAction::Craft(recipe) => {
                    let _ = sim.make(*id, *recipe);
                }
                CellSessionAction::Salvage(action) => {
                    sim.salvage_action(*id, *action);
                }
            }
        }
        sim.set_controls(host, self.controls[0]);
        sim.set_controls(guest, self.controls[1]);
    }
}

impl Sim {
    /// The canonical live and replay step. Invalid identities/controls do not tick.
    pub fn tick_cell_session(
        &mut self,
        host: PlayerId,
        guest: PlayerId,
        frame: &CellSessionTick,
    ) -> bool {
        if host == guest
            || self.crab(host).is_none()
            || self.crab(guest).is_none()
            || self.level.cells.is_none()
            || !frame.valid(host, guest)
        {
            return false;
        }
        let mut points = [
            self.crab(host).expect("validated host").mover.pos,
            self.crab(guest).expect("validated guest").mover.pos,
        ];
        for (id, action) in frame.actions.iter().flatten() {
            if matches!(action, CellSessionAction::Home) {
                points[usize::from(*id == guest)] =
                    self.level.door_and_den(crate::level::Place::Home).1;
            }
        }
        let Some(mut bounds) = crate::underwater::region::CellRegion::around_points(&points, 128)
        else {
            return false;
        };
        let Some(mut terrain) = self.level.cells.take() else {
            return false;
        };
        let mut fitted = false;
        for _ in 0..4 {
            if terrain.fit_region(&self.level, bounds).is_ok() {
                fitted = true;
                break;
            }
            let x = bounds.x.saturating_sub(64).max(0);
            bounds = crate::underwater::region::CellRegion {
                x,
                y: bounds.y - 64,
                width: bounds.width + (bounds.x - x) as usize + 64,
                height: bounds.height + 128,
            };
            if !bounds.valid() {
                break;
            }
        }
        self.level.cells = Some(terrain);
        if !fitted {
            return false;
        }
        frame.apply(self, host, guest);
        self.tick_fixed_terrain();
        true
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Run {
    count: u32,
    frame: CellSessionTick,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SessionReplay {
    checkpoint: String,
    host: PlayerId,
    guest: PlayerId,
    start_tick: u64,
    length: u64,
    runs: Vec<Run>,
}
impl SessionReplay {
    pub(super) fn parse(text: &str) -> Result<super::Replay, String> {
        if text.len() > MAX_CELL_SESSION_TEXT {
            return Err("cell session replay exceeds download cap".into());
        }
        let mut lines = text.trim().lines();
        let encoded = lines
            .next()
            .and_then(|line| line.strip_prefix(PREFIX))
            .ok_or("invalid cell session header")?;
        let mut hash = None;
        for line in lines {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if hash.is_some() {
                return Err("duplicate session hash".into());
            }
            hash = Some(
                u64::from_str_radix(
                    line.strip_prefix("hash ")
                        .ok_or("unexpected session replay line")?
                        .trim(),
                    16,
                )
                .map_err(|_| "invalid session hash")?,
            );
        }
        let packed = crate::memory::unbase64(encoded).ok_or("invalid cell session encoding")?;
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, MAX_RAW)
            .map_err(|_| "cell session replay exceeds decode limit")?;
        let session: Self =
            serde_json::from_slice(&raw).map_err(|_| "invalid cell session replay")?;
        if session.runs.len() > MAX_RUNS
            || session.length > MAX_TICKS
            || session
                .runs
                .iter()
                .any(|r| r.count == 0 || !r.frame.valid(session.host, session.guest))
            || session.runs.iter().map(|r| u64::from(r.count)).sum::<u64>() != session.length
        {
            return Err("invalid cell session frames".into());
        }
        session.restore()?;
        Ok(super::Replay {
            cellular: true,
            end: Some(session.length),
            hash,
            session: Some(session),
            ..Default::default()
        })
    }
    pub(super) fn recognizes(text: &str) -> bool {
        text.trim_start().starts_with("cell_session_v1")
    }
    pub(super) fn restore(&self) -> Result<Sim, String> {
        if self.checkpoint.len() > MAX_CELL_SESSION_TEXT || self.host == self.guest {
            return Err("invalid session checkpoint".into());
        }
        let packed =
            crate::memory::unbase64(&self.checkpoint).ok_or("invalid checkpoint encoding")?;
        let sim =
            Sim::from_session_checkpoint(&packed).map_err(|_| "invalid session checkpoint")?;
        if sim.tick_count() != self.start_tick
            || sim.crabs().len() != 2
            || sim.crab(self.host).is_none()
            || sim.crab(self.guest).is_none()
            || self.start_tick.checked_add(self.length).is_none()
        {
            return Err("invalid session actors or clock".into());
        }
        Ok(sim)
    }
    fn frame_at(&self, tick: u64) -> Option<&CellSessionTick> {
        let relative = tick.checked_sub(self.start_tick)?;
        if relative >= self.length {
            return None;
        }
        let mut end = 0;
        for run in &self.runs {
            end += u64::from(run.count);
            if relative < end {
                return Some(&run.frame);
            }
        }
        None
    }
    pub(super) fn apply_at(&self, sim: &mut Sim) {
        if let Some(frame) = self.frame_at(sim.tick_count()) {
            frame.apply(sim, self.host, self.guest);
        }
    }
    pub(super) fn step(&self, sim: &mut Sim) -> bool {
        self.frame_at(sim.tick_count())
            .is_some_and(|frame| sim.tick_cell_session(self.host, self.guest, frame))
    }
    pub(super) fn to_text(&self) -> Result<String, String> {
        let raw = serde_json::to_vec(self).map_err(|_| "could not encode session replay")?;
        if raw.len() > MAX_RAW {
            return Err("session replay omitted: raw recording cap exceeded".into());
        }
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        let text = format!("{PREFIX}{}\n", crate::memory::base64(&packed));
        if text.len() > MAX_CELL_SESSION_TEXT {
            return Err("session replay omitted: download cap exceeded".into());
        }
        Ok(text)
    }
}

/// Small RLE append-only recording. Failure is sticky: never emit a misleading
/// partial replay after missing a frame, invalid input, or exceeding a bound.
#[derive(Clone, Debug)]
pub struct CellSessionRecording {
    replay: SessionReplay,
    omitted: bool,
}
impl CellSessionRecording {
    pub fn new(packed: &[u8], host: PlayerId, guest: PlayerId) -> Result<Self, String> {
        if packed.len() > MAX_CELL_SESSION_TEXT * 3 / 4 {
            return Err("session replay omitted: starting checkpoint exceeds download cap".into());
        }
        let sim =
            Sim::from_session_checkpoint(packed).map_err(|_| "invalid recording checkpoint")?;
        let replay = SessionReplay {
            checkpoint: crate::memory::base64(packed),
            host,
            guest,
            start_tick: sim.tick_count(),
            length: 0,
            runs: Vec::new(),
        };
        if host == guest
            || sim.crabs().len() != 2
            || sim.crab(host).is_none()
            || sim.crab(guest).is_none()
        {
            return Err("invalid recording actors".into());
        }
        Ok(Self {
            replay,
            omitted: false,
        })
    }
    /// Packed starting bytes, useful for declining a smaller report attachment
    /// before formatting the full downloadable recording.
    pub fn checkpoint_len(&self) -> usize {
        // base64 has no padding in the shared codec.
        self.replay.checkpoint.len() * 3 / 4
    }

    /// Call immediately before applying this canonical tick to the live Sim.
    pub fn note(&mut self, absolute_tick: u64, frame: CellSessionTick) -> bool {
        if self.omitted
            || absolute_tick != self.replay.start_tick + self.replay.length
            || self.replay.length >= MAX_TICKS
            || !frame.valid(self.replay.host, self.replay.guest)
        {
            self.omitted = true;
            return false;
        }
        if let Some(run) = self.replay.runs.last_mut().filter(|r| r.frame == frame) {
            run.count += 1;
        } else if self.replay.runs.len() < MAX_RUNS {
            self.replay.runs.push(Run { count: 1, frame });
        } else {
            self.omitted = true;
            return false;
        }
        self.replay.length += 1;
        true
    }
    pub fn to_text(&self) -> Result<String, String> {
        if self.omitted {
            Err("session replay omitted: incomplete recording or recording cap exceeded".into())
        } else {
            self.replay.to_text()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Level, Replay, Tuning};
    fn initial() -> Sim {
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        sim.enable_underwater_cells();
        sim.add_crab(PlayerId(0), crate::level::SPAWN);
        sim.add_crab(
            PlayerId(91),
            crate::level::SPAWN + bevy_math::Vec2::X * 100.0,
        );
        sim.populate();
        for _ in 0..12 {
            sim.tick();
        }
        sim
    }
    #[test]
    fn session_replays_salvage_actions_and_identity_exactly() {
        use crate::salvage::SalvageAction;
        let mut sim = Sim::with_one_crab(bevy_math::Vec2::new(300.0, 30.0));
        sim.enable_underwater_cells();
        sim.add_crab(PlayerId(91), bevy_math::Vec2::new(150.0, 30.0));
        sim.crab_mut(PlayerId(0)).unwrap().salvage_identity = 10;
        sim.crab_mut(PlayerId(91)).unwrap().salvage_identity = 11;
        sim.salvage.as_mut().unwrap().world_id = 77;
        let packed = sim.session_checkpoint().unwrap();
        let mut recording = CellSessionRecording::new(&packed, PlayerId(0), PlayerId(91)).unwrap();
        for tick in 0..20 {
            let mut frame = CellSessionTick::default();
            if tick == 0 {
                frame.actions[0] = Some((
                    PlayerId(0),
                    CellSessionAction::Salvage(SalvageAction::LiftValve),
                ));
            }
            if tick == 10 {
                frame.actions[0] = Some((
                    PlayerId(0),
                    CellSessionAction::Salvage(SalvageAction::DropValve),
                ));
            }
            assert!(recording.note(sim.tick_count(), frame));
            assert!(sim.tick_cell_session(PlayerId(0), PlayerId(91), &frame));
            if tick == 0 {
                assert_eq!(
                    sim.salvage.as_ref().unwrap().valve_phase,
                    crate::salvage::ValvePhase::Carried(PlayerId(0))
                );
            }
            if tick == 10 {
                assert_ne!(
                    sim.salvage.as_ref().unwrap().valve_phase,
                    crate::salvage::ValvePhase::Carried(PlayerId(0))
                );
            }
        }
        let replay = Replay::parse(&recording.to_text().unwrap()).unwrap();
        let mut restored = Sim::with_one_crab(crate::level::SPAWN);
        assert!(replay.prepare_start(&mut restored));
        for _ in 0..replay.length() {
            assert!(replay.step_cell_session(&mut restored));
        }
        assert_eq!(sim.state_hash(), restored.state_hash());
        assert_eq!(
            sim.session_checkpoint().unwrap(),
            restored.session_checkpoint().unwrap()
        );
    }
    #[test]
    fn session_text_replays_both_actors_actions_and_absolute_clock_exactly() {
        let mut sim = initial();
        let packed = sim.session_checkpoint().unwrap();
        let mut recording = CellSessionRecording::new(&packed, PlayerId(0), PlayerId(91)).unwrap();
        for tick in 0..360 {
            let mut frame = CellSessionTick::default();
            frame.controls[0] = Controls {
                walk: if tick < 180 { -1.0 } else { 1.0 },
                jump_pressed: tick % 53 == 0,
                jump_held: tick % 53 < 20,
                jab_pressed: tick % 37 == 0,
                ..Default::default()
            };
            frame.controls[1] = Controls {
                walk: if tick < 180 { 1.0 } else { -1.0 },
                dig_held: tick % 40 < 20,
                dig_up: tick % 80 < 20,
                down: tick % 80 >= 60,
                block_held: tick % 21 < 4,
                dodge_pressed: tick % 67 == 0,
                ..Default::default()
            };
            frame.actions[0] = match tick {
                5 => Some((
                    PlayerId(91),
                    CellSessionAction::Code(crate::codes::Code::Glow),
                )),
                50 => Some((PlayerId(0), CellSessionAction::Home)),
                60 => Some((
                    PlayerId(0),
                    CellSessionAction::Craft(crate::craft::Recipe::Lantern),
                )),
                80 => Some((
                    PlayerId(91),
                    CellSessionAction::Moult(crate::skills::Skill::Swimming),
                )),
                140 => Some((
                    PlayerId(91),
                    CellSessionAction::Code(crate::codes::Code::Feast),
                )),
                _ => None,
            };
            assert!(recording.note(sim.tick_count(), frame));
            assert!(sim.tick_cell_session(PlayerId(0), PlayerId(91), &frame));
        }
        let text = format!(
            "{}hash {:016x}\n",
            recording.to_text().unwrap(),
            sim.state_hash()
        );
        let replay = Replay::parse(&text).unwrap();
        assert!(replay.is_cell_session());
        assert_eq!(replay.length(), 360);
        let mut restored = Sim::with_one_crab(crate::level::SPAWN);
        assert!(replay.prepare_start(&mut restored));
        assert_eq!(restored.tick_count(), 12);
        for _ in 0..replay.length() {
            assert!(replay.step_cell_session(&mut restored));
        }
        assert!(!replay.step_cell_session(&mut restored));
        assert_eq!(restored.tick_count(), 372);
        assert_eq!(replay.hash, Some(restored.state_hash()));
        assert_eq!(
            sim.session_checkpoint().unwrap(),
            restored.session_checkpoint().unwrap()
        );
        assert_eq!(Replay::parse(&replay.to_text()).unwrap(), replay);
    }
    #[test]
    fn bounded_recording_rejects_bad_order_and_malformed_data() {
        for text in [
            "cell_session_v1 ?".into(),
            "cell_session_v1 ".to_owned() + &"A".repeat(MAX_CELL_SESSION_TEXT),
        ] {
            assert!(Replay::parse(&text).is_err());
        }
        let sim = initial();
        let packed = sim.session_checkpoint().unwrap();
        let mut recording = CellSessionRecording::new(&packed, PlayerId(0), PlayerId(91)).unwrap();
        assert!(!recording.note(sim.tick_count() + 1, CellSessionTick::default()));
        assert!(recording.to_text().unwrap_err().contains("omitted"));
        let mut recording = CellSessionRecording::new(&packed, PlayerId(0), PlayerId(91)).unwrap();
        let mut invalid = CellSessionTick::default();
        invalid.actions[0] = Some((PlayerId(99), CellSessionAction::Home));
        assert!(!recording.note(sim.tick_count(), invalid));
        assert!(recording.to_text().is_err());
        let mut bounded = CellSessionRecording::new(&packed, PlayerId(0), PlayerId(91)).unwrap();
        for tick in 0..MAX_RUNS {
            let mut frame = CellSessionTick::default();
            frame.controls[0].walk = (tick % 2) as f32;
            assert!(bounded.note(sim.tick_count() + tick as u64, frame));
        }
        assert!(!bounded.note(
            sim.tick_count() + MAX_RUNS as u64,
            CellSessionTick::default()
        ));
        assert!(bounded.to_text().is_err());
        let mut session = recording.replay;
        session.runs = vec![Run {
            count: 0,
            frame: CellSessionTick::default(),
        }];
        assert!(Replay::parse(&session.to_text().unwrap()).is_err());
        assert!(
            CellSessionRecording::new(&vec![0; MAX_CELL_SESSION_TEXT], PlayerId(0), PlayerId(91))
                .is_err()
        );
    }
}
