//! Test-only protocol model; never used for live network verification.
//! Stores a complete rollback snapshot and deliberately serializes metadata.

use super::{Dirty, World, bytes};
use crate::underwater::{HEIGHT, TERRAIN_CHUNK_CELLS, WIDTH};
use std::collections::BTreeMap;

fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn tile_hash(world: &World, tile: (usize, usize)) -> u64 {
    let side = TERRAIN_CHUNK_CELLS as usize;
    let mut cells = Vec::new();
    for y in tile.1 * side..((tile.1 + 1) * side).min(HEIGHT) {
        for x in tile.0 * side..((tile.0 + 1) * side).min(WIDTH) {
            let i = y * WIDTH + x;
            cells.push((
                world.cells[i],
                world.flow[i].0.to_bits(),
                world.flow[i].1.to_bits(),
                world.grain[i],
                world.suspended[i],
                world.pebble_owner[i],
            ));
        }
    }
    hash(&bytes(&cells).expect("bounded fixture tile serializes"))
}

fn globals_hash(world: &World) -> u64 {
    let mut copy = world.clone();
    copy.cells.clear();
    copy.flow.clear();
    copy.grain.clear();
    copy.suspended.clear();
    copy.pebble_owner.clear();
    copy.sleeping.clear();
    hash(&bytes(&copy.checkpoint_ref()).expect("fixture globals serialize"))
}

fn sleeping_hash(world: &World, key: (i32, i32)) -> u64 {
    hash(&bytes(&world.sleeping.get(&key)).expect("fixture sleeping chunk serializes"))
}

/// Frame carries authoritative hashes for marked state, and the exact full
/// checkpoint digest on each checkpoint boundary. It does not carry changes.
pub(crate) struct Frame {
    tiles: BTreeMap<(usize, usize), u64>,
    sleeping: BTreeMap<(i32, i32), u64>,
    globals: Option<u64>,
    full: Option<u64>,
}

fn covers(marked: &Dirty, changed: &Dirty) -> bool {
    changed.active.is_subset(&marked.active)
        && changed.sleeping.is_subset(&marked.sleeping)
        && (!changed.global || marked.global)
}

impl Frame {
    pub(crate) fn capture_checked(
        before: &World,
        host: &World,
        changes: &Dirty,
        checkpoint: bool,
    ) -> Option<Self> {
        let actual = super::dirty(before, host).ok()?;
        covers(changes, &actual).then(|| Self::capture(host, changes, checkpoint))
    }

    fn capture(host: &World, changes: &Dirty, checkpoint: bool) -> Self {
        Self {
            tiles: changes
                .active
                .iter()
                .map(|&id| (id, tile_hash(host, id)))
                .collect(),
            sleeping: changes
                .sleeping
                .iter()
                .map(|&id| (id, sleeping_hash(host, id)))
                .collect(),
            globals: changes.global.then(|| globals_hash(host)),
            full: checkpoint.then(|| crate::net::cell_join::lockstep::digest(host).unwrap()),
        }
    }
}

pub(crate) struct Probe {
    world: World,
    verified: World,
    sequence: u64,
    verified_sequence: u64,
    period: u64,
}

impl Probe {
    pub(crate) fn new(world: World, period: u64) -> Self {
        assert!(period > 0);
        Self {
            verified: world.clone(),
            world,
            sequence: 0,
            verified_sequence: 0,
            period,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn retained_payload_lower_bound(&self) -> usize {
        super::retained_payload_lower_bound(&self.world)
            + super::retained_payload_lower_bound(&self.verified)
    }

    pub(crate) fn apply_checked(&mut self, trial: World, frame: &Frame) -> bool {
        let marked = Dirty {
            active: frame.tiles.keys().copied().collect(),
            sleeping: frame.sleeping.keys().copied().collect(),
            global: frame.globals.is_some(),
        };
        let complete =
            super::dirty(&self.world, &trial).is_ok_and(|actual| covers(&marked, &actual));
        if !complete {
            self.world = self.verified.clone();
            self.sequence = self.verified_sequence;
            return false;
        }
        self.apply(trial, frame)
    }

    fn apply(&mut self, trial: World, frame: &Frame) -> bool {
        let Some(next) = self.sequence.checked_add(1) else {
            return false;
        };
        let checkpoint = next.is_multiple_of(self.period);
        if checkpoint != frame.full.is_some()
            || frame
                .tiles
                .iter()
                .any(|(&id, &expected)| tile_hash(&trial, id) != expected)
            || frame
                .sleeping
                .iter()
                .any(|(&id, &expected)| sleeping_hash(&trial, id) != expected)
            || frame
                .globals
                .is_some_and(|expected| globals_hash(&trial) != expected)
            || frame.full.is_some_and(|expected| {
                crate::net::cell_join::lockstep::digest(&trial) != Some(expected)
            })
        {
            // Undo all frames since the last verified full checkpoint.
            self.world = self.verified.clone();
            self.sequence = self.verified_sequence;
            return false;
        }
        self.world = trial;
        self.sequence = next;
        if checkpoint {
            self.verified = self.world.clone();
            self.verified_sequence = next;
        }
        true
    }
}

#[test]
fn marked_mismatch_rewinds_and_full_digest_catches_omitted_mark() {
    use crate::underwater::Cell;
    let initial = World::new();
    let mut host = initial.clone();
    let mut guest = Probe::new(initial.clone(), 3);
    let i = 100 * WIDTH + 80;
    host.cells[i] = Cell::Sand;
    let marks = super::dirty(&initial, &host).unwrap();
    let frame = Frame::capture(&host, &marks, false);
    let mut bad = host.clone();
    bad.cells[i] = Cell::Mud;
    assert!(!guest.apply(bad, &frame));
    assert_eq!(guest.world, initial);
    assert_eq!(guest.sequence, 0);
    assert!(guest.apply(host.clone(), &frame));
    let confirmed_before = guest.world.clone();
    let mut silently_wrong = host.clone();
    silently_wrong.cells[i + 1] = Cell::Mud;
    let missing = Dirty::default();
    let incomplete = Frame::capture(&host, &missing, false);
    assert!(guest.apply(silently_wrong.clone(), &incomplete)); // Delayed detection is explicit.
    let checkpoint = Frame::capture(&host, &missing, true);
    assert!(!guest.apply(silently_wrong, &checkpoint));
    assert_eq!(guest.world, initial);
    assert_ne!(guest.world, confirmed_before);
    assert_eq!(guest.sequence, 0);
}

#[test]
fn full_checkpoint_advances_rollback_point_and_metadata_is_checked() {
    let initial = World::new();
    let mut host = initial.clone();
    let mut guest = Probe::new(initial, 2);
    host.opening_edge_opened = true;
    let marks = super::dirty(&guest.world, &host).unwrap();
    assert!(marks.global);
    let frame = Frame::capture(&host, &marks, false);
    let bad = guest.world.clone();
    assert!(!guest.apply(bad, &frame));
    assert!(guest.apply(host.clone(), &frame));
    let full = Frame::capture(&host, &Dirty::default(), true);
    assert!(guest.apply(host.clone(), &full));
    let snapshot = guest.world.clone();
    let mut bad = host.clone();
    bad.tick += 1;
    let marks = super::dirty(&host, &bad).unwrap();
    let frame = Frame::capture(&host, &marks, false);
    assert!(!guest.apply(bad, &frame));
    assert_eq!(guest.world, snapshot);
    assert_eq!(guest.sequence, 2);
}

#[test]
fn real_upward_steps_match_for_two_worlds_and_checkpoint() {
    use crate::underwater::{Cell, HostDig, HostPlayerForce};
    let level = crate::Level::test_level();
    let tuning = crate::Tuning::default();
    let force = |player, x| HostPlayerForce {
        player: crate::PlayerId(player),
        x,
        y: 0.0,
        half_width: 12.0,
        half_height: 8.0,
        wake_x: 0.0,
        wake_y: 0.0,
        walking: 0,
        grounded: false,
        jumped: false,
        dig: Some(HostDig {
            dx: 0,
            dy: 1,
            force: 48,
        }),
    };
    let (host_force, guest_force) = (force(0, 400.0), force(1, 700.0));
    let mut host = World::from_level_settled(&level);
    host.cells.fill(Cell::Water);
    host.pebble_owner.fill(0);
    host.pebbles.clear();
    host.pebble_motion.clear();
    host.follow_player(&level, host_force.x, host_force.y - host_force.half_height);
    let mut guest = Probe::new(host.clone(), 3);
    for n in 1..=3 {
        for world in [&mut host, &mut guest.world] {
            for p in [&host_force, &guest_force] {
                let (x, y) = world.local_cell(p.x, p.y);
                assert!(world.test_place_loose_sand(x as i32, y as i32 + 5));
            }
        }
        let before = host.clone();
        let marked = super::trace_world(|| {
            assert!(host.step_up_pair(&level, &tuning, &host_force, &guest_force, false))
        });
        let actual = super::dirty(&before, &host).unwrap();
        assert!(actual.active.is_subset(&marked.active));
        assert!(actual.sleeping.is_subset(&marked.sleeping));
        assert!(!actual.global || marked.global);
        let frame = Frame::capture_checked(&before, &host, &marked, n == 3).unwrap();
        let mut trial = guest.world.clone();
        assert!(trial.step_up_pair(&level, &tuning, &host_force, &guest_force, false));
        assert!(guest.apply_checked(trial, &frame));
        assert_eq!(guest.world, host);
    }
    assert_eq!(guest.verified_sequence, 3);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "manual release-mode performance measurement"]
fn incremental_vs_full_verification_profile() {
    use std::time::Instant;
    let world = World::from_level_settled(&crate::Level::test_level());
    let mut marks = Dirty::default();
    marks.active.extend([(1, 1), (2, 1)]);
    marks.global = true;
    let mut incremental = Vec::new();
    let mut full = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        std::hint::black_box(Frame::capture(&world, &marks, false));
        incremental.push(start.elapsed());
        let start = Instant::now();
        std::hint::black_box(crate::net::cell_join::lockstep::digest(&world).unwrap());
        full.push(start.elapsed());
    }
    incremental.sort_unstable();
    full.sort_unstable();
    eprintln!(
        "two tile + globals capture {:?}; full checkpoint digest {:?}; n=20 each",
        incremental[10], full[10]
    );
}

#[test]
fn marked_sleeping_changes_are_checked_and_rewound() {
    use crate::underwater::SleepingChunk;
    let initial = World::new();
    let mut host = initial.clone();
    host.sleeping.insert((-3, 2), SleepingChunk::default());
    let marks = super::dirty(&initial, &host).unwrap();
    let frame = Frame::capture(&host, &marks, false);
    let mut probe = Probe::new(initial.clone(), 2);
    assert!(!probe.apply(initial.clone(), &frame));
    assert_eq!(probe.world, initial);
    assert!(probe.apply(host, &frame));
}

#[test]
fn off_material_window_shift_is_marked_and_validated() {
    use crate::underwater::HostPlayerForce;
    let level = crate::Level::test_level();
    let mut host = World::from_level_settled(&level);
    let force = |player, x| HostPlayerForce {
        player: crate::PlayerId(player),
        x,
        y: 100.0,
        half_width: 12.0,
        half_height: 8.0,
        wake_x: 0.0,
        wake_y: 0.0,
        walking: 0,
        grounded: false,
        jumped: false,
        dig: None,
    };
    let (h, g) = (force(0, 4400.0), force(1, 4700.0));
    let initial = host.clone();
    let trace = super::trace_world(|| {
        assert!(host.step_up_pair(&level, &crate::Tuning::default(), &h, &g, false))
    });
    let expected = super::dirty(&initial, &host).unwrap();
    assert!(expected.active.is_subset(&trace.active));
    assert!(expected.sleeping.is_subset(&trace.sleeping));
    assert!(expected.global && trace.global);
    assert!(!expected.sleeping.is_empty());
    let frame = Frame::capture(&host, &trace, false);
    let mut probe = Probe::new(initial.clone(), 2);
    let mut guest_world = initial;
    assert!(guest_world.step_up_pair(&level, &crate::Tuning::default(), &h, &g, false));
    assert!(probe.apply(guest_world, &frame));
    assert_eq!(probe.world, host);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "manual release-mode performance measurement"]
fn incremental_host_and_guest_profile() {
    use std::time::Instant;
    let world = World::from_level_settled(&crate::Level::test_level());
    let mut marks = Dirty::default();
    marks.active.extend([(1, 1), (2, 1)]);
    marks.global = true;
    let mut host = Vec::new();
    let mut guest = Vec::new();
    let mut clones = Vec::new();
    let mut probe = Probe::new(world.clone(), u64::MAX);
    for _ in 0..20 {
        let start = Instant::now();
        let frame = Frame::capture(&world, &marks, false);
        host.push(start.elapsed());
        let start = Instant::now();
        let trial = world.clone();
        clones.push(start.elapsed());
        let start = Instant::now();
        assert!(probe.apply(trial, &frame));
        guest.push(start.elapsed());
    }
    host.sort_unstable();
    guest.sort_unstable();
    clones.sort_unstable();
    eprintln!(
        "host capture {:?}, guest verify/apply {:?}, trial clone {:?}; n=20 each",
        host[10], guest[10], clones[10]
    );
}

#[test]
fn exhaustive_guard_rejects_missing_marks_before_commit_on_both_sides() {
    use crate::underwater::Cell;
    let initial = World::new();
    let mut changed = initial.clone();
    changed.cells[WIDTH * 100 + 80] = Cell::Mud;
    let omitted = Dirty::default();
    assert!(Frame::capture_checked(&initial, &changed, &omitted, false).is_none());
    let frame = Frame::capture(&initial, &omitted, false);
    let mut probe = Probe::new(initial.clone(), 2);
    assert!(!probe.apply_checked(changed, &frame));
    assert_eq!(probe.world, initial);
    assert_eq!(probe.sequence, 0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "manual release-mode performance measurement"]
fn exhaustive_guard_profile() {
    use std::time::Instant;
    let before = World::from_level_settled(&crate::Level::test_level());
    let mut after = before.clone();
    let index = 100 * WIDTH + 80;
    after.cells[index] = if before.cells[index] == crate::underwater::Cell::Sand {
        crate::underwater::Cell::Mud
    } else {
        crate::underwater::Cell::Sand
    };
    let marks = super::dirty(&before, &after).unwrap();
    assert_eq!(marks.active.len(), 1);
    let mut host = Vec::new();
    let mut guest = Vec::new();
    let mut probe = Probe::new(before.clone(), u64::MAX);
    for _ in 0..20 {
        let start = Instant::now();
        let frame = Frame::capture_checked(&before, &after, &marks, false).unwrap();
        host.push(start.elapsed());
        let start = Instant::now();
        assert!(probe.apply_checked(after.clone(), &frame));
        guest.push(start.elapsed());
        probe.world = before.clone();
    }
    host.sort_unstable();
    guest.sort_unstable();
    eprintln!(
        "audited host capture {:?}, audited guest verify incl trial clone {:?}; n=20 each",
        host[10], guest[10]
    );
}

#[test]
fn varied_upward_fixtures_preserve_oracle_coverage_and_atomic_failure() {
    use crate::underwater::{Cell, HostDig, HostPlayerForce};
    let level = crate::Level::test_level();
    let tuning = crate::Tuning::default();
    let mut successful_with_changes = 0;
    for case in 0..24 {
        let force = |player, x, dig: bool| HostPlayerForce {
            player: crate::PlayerId(player),
            x,
            y: 0.0,
            half_width: 12.0,
            half_height: 8.0,
            wake_x: 0.0,
            wake_y: 0.0,
            walking: 0,
            grounded: false,
            jumped: false,
            dig: dig.then_some(HostDig {
                dx: 0,
                dy: 1,
                force: 48,
            }),
        };
        let (h, g) = (
            force(0, 256.0, case % 3 != 0),
            force(1, 700.0, case % 3 != 1),
        );
        let mut world = World::from_level_settled(&level);
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.follow_player(&level, h.x, h.y - h.half_height);
        world.tick = case as u64;
        for p in [&h, &g] {
            let (x, y) = world.local_cell(p.x, p.y);
            for offset in 0..=(case % 5) {
                let i = World::index(x as i32 + offset - 2, y as i32 + 5).unwrap();
                world.cells[i] = if case % 2 == 0 { Cell::Sand } else { Cell::Mud };
                world.grain[i] = (case * 7) as u8;
            }
            if case % 7 == 0 {
                let i = World::index(x as i32, y as i32 + 4).unwrap();
                world.cells[i] = Cell::Rock;
            }
        }
        let before = world.clone();
        let material = case % 2 == 0;
        let mut accepted = false;
        let marks = super::trace_world(|| {
            accepted = world.step_up_pair(&level, &tuning, &h, &g, material);
        });
        let actual = super::dirty(&before, &world).unwrap();
        assert!(covers(&marks, &actual), "missing mark in case {case}");
        if !accepted {
            assert_eq!(world, before, "failed case {case} changed the world");
            continue;
        }
        if !actual.active.is_empty() {
            successful_with_changes += 1;
        }
        let frame = Frame::capture_checked(&before, &world, &marks, false).unwrap();
        let mut replay = before.clone();
        assert!(replay.step_up_pair(&level, &tuning, &h, &g, material));
        let mut verifier = Probe::new(before, 3);
        assert!(verifier.apply_checked(replay, &frame));
        assert_eq!(verifier.world, world);
    }
    assert!(successful_with_changes >= 8);
}
