//! The bone hall's one threat: a clock (`docs/design/world-vision-brief.md`,
//! register 3). The whale fall's enriched mud and bones release sulfide in
//! pulses; standing in the haze during a pulse drains the reserve.
//!
//! Biology: sulfide around whale-fall bones comes from the sulfophilic stage
//! (`ocean-shaped.md` row 9, [WP-WF]). Pulsing on a nine-second clock, and
//! the reserve drain, are design calls, not measurements.
//!
//! The clock is a pure function of the tick, so nothing is stored or saved and
//! every device agrees. The haze is derived from the hall's landmark each time
//! (never stored), so it follows the hall if the map moves. It sits on the
//! far side of the bones from the door and never touches the door, the way in
//! to the prize, or any tunnel or mouth. It never hurts health and never
//! kills; it only costs reserve, and only a crab on its own device.
use crate::Tuning;
use crate::level::Level;
use crate::level::landmarks::{Kind, Landmark};
use crate::physics::STEP;
use crate::world::SimCrab;
use bevy_math::{Rect, Vec2};

/// Ticks between the starts of two pulses (9 s).
pub const PERIOD: u64 = 1080;
/// How long a pulse lasts (2 s).
pub const PULSE: u64 = 240;
/// The hiss and shimmer start this long before a pulse (1.5 s).
pub const TELL: u64 = 180;
/// Tick of the first pulse (its tell starts at `FIRST - TELL`).
pub const FIRST: u64 = 1080;

/// Where the clock is, for the sim and for drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Quiet,
    /// Before a pulse: 0 at the hiss, near 1 as the pulse starts.
    Tell(f32),
    /// A pulse: 0 at its start, near 1 at its end.
    Pulse(f32),
}

pub fn phase(tick: u64) -> Phase {
    if tick + TELL >= FIRST {
        let t = tick + TELL - FIRST;
        let into_tell = t % PERIOD;
        if into_tell < TELL {
            return Phase::Tell(into_tell as f32 / TELL as f32);
        }
        let into_pulse = into_tell - TELL;
        if into_pulse < PULSE {
            return Phase::Pulse(into_pulse as f32 / PULSE as f32);
        }
    }
    Phase::Quiet
}

pub fn pulse_active(tick: u64) -> bool {
    matches!(phase(tick), Phase::Pulse(_))
}

/// The tick a tell (and its hiss) begins on, if `tick` is one.
pub fn tell_starts(tick: u64) -> bool {
    tick + TELL >= FIRST && (tick + TELL - FIRST).is_multiple_of(PERIOD)
}

/// A hall found by its landmark: the room, its door and its prize.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hall {
    pub rect: Rect,
    pub door: Rect,
    pub prize: Vec2,
}

/// The bone hall, found by name.
pub fn find(level: &Level) -> Option<Hall> {
    find_in(&landmarks_of(level)?)
}

/// The level's landmarks, if it is an authored level (a synthetic test level
/// has none, and the landmark builder insists on the authored one's parts).
fn landmarks_of(level: &Level) -> Option<Vec<Landmark>> {
    let authored = !level.currents.is_empty()
        && level
            .foes
            .iter()
            .any(|(k, _)| *k == crate::enemy::Kind::Octopus)
        && level
            .eggs
            .get(4)
            .is_some_and(|egg| level.secrets.iter().any(|r| r.contains(*egg)));
    authored.then(|| level.landmarks())
}

fn find_in(marks: &[Landmark]) -> Option<Hall> {
    marks.iter().find_map(|l| match l.kind {
        Kind::Chamber { door, prize } if l.name == "bone hall" => Some(Hall {
            rect: l.rect,
            door,
            prize,
        }),
        _ => None,
    })
}

/// How far the haze keeps from the walk in from the door, the prize and the
/// door itself.
const KEEP_CLEAR: f32 = 160.0;
/// How far the haze keeps from any tunnel or mouth.
const KEEP_FROM_TUNNEL: f32 = 80.0;
/// The haze keeps this far inside the hall's own walls.
const INSET: f32 = 50.0;
/// Smaller than this and there is no haze to speak of.
const MIN_SIDE: f32 = 120.0;

fn grown(r: Rect, by: f32) -> Rect {
    Rect::new(r.min.x - by, r.min.y - by, r.max.x + by, r.max.y + by)
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

fn area(r: Rect) -> f32 {
    r.width().max(0.0) * r.height().max(0.0)
}

/// The biggest piece of `r` that clears `cut` (empty if none is left).
fn carve(r: Rect, cut: Rect) -> Rect {
    if !overlaps(r, cut) {
        return r;
    }
    // Plain structs: `Rect::new` would flip a backwards rect into a real one.
    let piece = |x0: f32, y0: f32, x1: f32, y1: f32| Rect {
        min: Vec2::new(x0, y0),
        max: Vec2::new(x1, y1),
    };
    [
        piece(r.min.x, r.min.y, cut.min.x, r.max.y),
        piece(cut.max.x, r.min.y, r.max.x, r.max.y),
        piece(r.min.x, r.min.y, r.max.x, cut.min.y),
        piece(r.min.x, cut.max.y, r.max.x, r.max.y),
    ]
    .into_iter()
    .max_by(|a, b| area(*a).total_cmp(&area(*b)))
    .unwrap_or(r)
}

/// The haze of the hall: the half of the room away from the door, beyond the
/// prize, shrunk to clear the door, the walk in and every tunnel or mouth.
/// `None` when nothing is left.
pub fn haze(level: &Level, hall: &Hall) -> Option<Rect> {
    haze_in(&landmarks_of(level)?, hall)
}

fn haze_in(marks: &[Landmark], hall: &Hall) -> Option<Rect> {
    let room = hall.rect;
    let door_x = (hall.door.min.x + hall.door.max.x) * 0.5;
    let inner = Rect::new(
        room.min.x + INSET,
        room.min.y + INSET,
        room.max.x - INSET,
        room.max.y - INSET,
    );
    // The far side of the room from the door, beyond the prize.
    let mut haze = if door_x <= room.center().x {
        Rect::new(
            (hall.prize.x + KEEP_CLEAR).max(inner.min.x),
            inner.min.y,
            inner.max.x,
            inner.max.y,
        )
    } else {
        Rect::new(
            inner.min.x,
            inner.min.y,
            (hall.prize.x - KEEP_CLEAR).min(inner.max.x),
            inner.max.y,
        )
    };
    let door_centre = hall.door.center();
    let walk = Rect::from_corners(door_centre, hall.prize);
    haze = carve(haze, grown(hall.door, KEEP_CLEAR));
    haze = carve(haze, grown(walk, KEEP_CLEAR));
    haze = carve(
        haze,
        grown(Rect::from_center_size(hall.prize, Vec2::ZERO), KEEP_CLEAR),
    );
    for l in marks {
        if matches!(l.kind, Kind::Tunnel | Kind::Mouth) {
            haze = carve(haze, grown(l.rect, KEEP_FROM_TUNNEL));
        }
    }
    (haze.max.x - haze.min.x >= MIN_SIDE && haze.max.y - haze.min.y >= MIN_SIDE).then_some(haze)
}

/// Is this point in the haze right now (the pulse is on)?
pub fn drains_at(tick: u64, haze: Rect, p: Vec2) -> bool {
    pulse_active(tick) && haze.contains(p)
}

/// The hall and its haze, worked out once from the level (building the
/// landmarks is costly, and a running world never changes them). Derived, so
/// never hashed or saved.
#[derive(Clone, Debug, Default)]
pub struct Cache(Option<Option<Rect>>);

/// Hiss and shimmer are only worth cueing for a crab within this of the haze.
const EARSHOT: f32 = 2400.0;

/// The hall's clock for the tick just finished (`tick` is the new count): the
/// reserve drain for each crab on this device that stands in the haze during
/// a pulse (health is never touched). Returns where the hiss comes from when a
/// tell starts on this tick and a crab on this device is within earshot.
pub fn step(
    tick: u64,
    level: &Level,
    tuning: &Tuning,
    crabs: &mut [SimCrab],
    cache: &mut Cache,
) -> Option<Vec2> {
    let (starts, pulsing) = (tell_starts(tick), pulse_active(tick));
    if !starts && !pulsing {
        return None;
    }
    let haze = (*cache.0.get_or_insert_with(|| {
        let marks = landmarks_of(level)?;
        haze_in(&marks, &find_in(&marks)?)
    }))?;
    if pulsing {
        for c in crabs
            .iter_mut()
            .filter(|c| !c.remote && c.fighter.dead.is_none())
        {
            if haze.contains(c.mover.pos) {
                let cost = tuning.sulfide_drain * STEP / c.mover.reserve_strokes(tuning);
                c.mover.reserve = (c.mover.reserve - cost).max(0.0);
            }
        }
    }
    let heard = crabs
        .iter()
        .any(|c| !c.remote && c.mover.pos.distance(haze.center()) < EARSHOT);
    (starts && heard).then(|| haze.center())
}
