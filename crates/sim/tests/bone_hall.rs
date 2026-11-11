//! The bone hall's sulfide clock (#467): one threat, a readable tell, a haze
//! that stays clear of every way in and out, and a cost in reserve only.
use bevy_math::{Rect, Vec2};
use crabsim::bone_hall::{self, FIRST, Hall, PERIOD, PULSE, TELL};
use crabsim::level::landmarks::Kind;
use crabsim::world::cues::Cue;
use crabsim::{Level, PlayerId, Sim, Tuning};

fn real_hall(level: &Level) -> (Hall, Rect) {
    let hall = bone_hall::find(level).expect("the level has a bone hall");
    let haze = bone_hall::haze(level, &hall).expect("the hall has a haze");
    (hall, haze)
}

fn seeps(sim: &Sim) -> usize {
    sim.cues()
        .iter()
        .filter(|e| e.tick == sim.tick_count() && matches!(e.cue, Cue::Seep { .. }))
        .count()
}

#[test]
fn every_pulse_has_a_1_5s_tell() {
    assert_eq!(TELL, 180, "1.5 s at 120 ticks a second");
    // A crab in earshot: the hiss is only cued for a crab near the hall.
    let level = Level::test_level();
    let (_, haze) = real_hall(&level);
    let mut sim = Sim::with_one_crab(haze.center() + Vec2::new(0.0, 900.0));
    let (mut hisses, mut starts) = (Vec::new(), Vec::new());
    let mut was_on = false;
    for _ in 0..FIRST + PERIOD * 3 + PULSE {
        sim.tick();
        let t = sim.tick_count();
        if seeps(&sim) > 0 {
            hisses.push(t);
        }
        let on = bone_hall::pulse_active(t);
        if on && !was_on {
            starts.push(t);
        }
        was_on = on;
    }
    println!("hisses {hisses:?} pulse starts {starts:?}");
    assert_eq!(starts.len(), 4);
    // Far away, nothing is cued at all.
    let mut far = Sim::with_one_crab(Vec2::new(500.0, 0.0));
    for _ in 0..FIRST + PULSE {
        far.tick();
        assert_eq!(seeps(&far), 0);
    }
    assert_eq!(hisses.len(), starts.len(), "one hiss per pulse");
    for (hiss, start) in hisses.iter().zip(&starts) {
        assert_eq!(start - hiss, TELL, "the hiss comes 1.5 s before the pulse");
    }
    // The shimmer's tell is the same 1.5 s: every tell tick is followed by
    // the pulse's first tick exactly the rest of the tell later.
    for t in 0..FIRST + PERIOD * 4 {
        if let bone_hall::Phase::Tell(p) = bone_hall::phase(t) {
            let left = TELL - (p * TELL as f32).round() as u64;
            assert_eq!(bone_hall::phase(t + left), bone_hall::Phase::Pulse(0.0));
        }
    }
}

fn clear_of(haze: Rect, other: Rect, margin: f32) -> bool {
    haze.max.x + margin <= other.min.x
        || other.max.x + margin <= haze.min.x
        || haze.max.y + margin <= other.min.y
        || other.max.y + margin <= haze.min.y
}

#[test]
fn the_haze_never_enters_a_tunnel_or_the_door() {
    let level = Level::test_level();
    let (hall, haze) = real_hall(&level);
    println!("haze {haze:?} in hall {:?}", hall.rect);
    assert!(
        haze.min.x >= hall.rect.min.x
            && haze.max.x <= hall.rect.max.x
            && haze.min.y >= hall.rect.min.y
            && haze.max.y <= hall.rect.max.y,
        "the haze lies inside the hall"
    );
    let check = |hall: &Hall, haze: Rect| {
        assert!(clear_of(haze, hall.door, 0.0), "the door");
        assert!(!haze.contains(hall.prize), "the prize");
        let walk = Rect::from_corners(hall.door.center(), hall.prize);
        assert!(clear_of(haze, walk, 0.0), "the way in to the prize");
        for l in level.landmarks() {
            if matches!(l.kind, Kind::Tunnel | Kind::Mouth) {
                assert!(clear_of(haze, l.rect, 0.0), "{}", l.name);
            }
        }
    };
    check(&hall, haze);
    // A synthetic hall laid across the tunnels and mouths must still carve
    // its haze clear of every one of them.
    let tunnels: Vec<_> = level
        .landmarks()
        .into_iter()
        .filter(|l| matches!(l.kind, Kind::Tunnel | Kind::Mouth))
        .collect();
    for l in tunnels {
        let rect = Rect::new(
            l.rect.center().x - 700.0,
            l.rect.center().y - 300.0,
            l.rect.center().x + 700.0,
            l.rect.center().y + 300.0,
        );
        let synthetic = Hall {
            rect,
            door: Rect::new(rect.min.x, rect.min.y, rect.min.x + 20.0, rect.max.y),
            prize: rect.center() - Vec2::new(300.0, 0.0),
        };
        if let Some(haze) = bone_hall::haze(&level, &synthetic) {
            check(&synthetic, haze);
        }
    }
}

/// A crab on this device standing at `at`. (The legacy level is solid rock
/// around the hall, so a ticking sim would rescue it out; the drain is the
/// same function the tick calls, fed crabs directly.)
fn crab_at(at: Vec2, id: u32, remote: bool) -> crabsim::world::SimCrab {
    let mut crab = Sim::with_one_crab(at).crabs()[0].clone();
    crab.id = PlayerId(id);
    crab.remote = remote;
    crab
}

#[test]
fn haze_drains_reserve_but_never_health() {
    let level = Level::test_level();
    let (hall, haze) = real_hall(&level);
    let tuning = Tuning::default();
    let mut crabs = [
        crab_at(haze.center(), 0, false),
        crab_at(Vec2::new(500.0, 0.0), 1, false),
        crab_at(hall.door.center(), 2, false),
        crab_at(haze.center(), 3, true),
    ];
    let health: Vec<_> = crabs.iter().map(|c| c.fighter.vitals.health).collect();
    let mut cache = bone_hall::Cache::default();
    let mut hisses = 0;
    for tick in 1..=FIRST + PULSE + 10 {
        let before = crabs[0].mover.reserve;
        hisses +=
            usize::from(bone_hall::step(tick, &level, &tuning, &mut crabs, &mut cache).is_some());
        if !bone_hall::pulse_active(tick) {
            assert_eq!(crabs[0].mover.reserve, before, "no drain between pulses");
        }
    }
    let reserve: Vec<_> = crabs.iter().map(|c| c.mover.reserve).collect();
    println!("reserve after one pulse: {reserve:?}; hisses {hisses}");
    let expected =
        1.0 - tuning.sulfide_drain * PULSE as f32 / 120.0 / tuning.stamina_reserve as f32;
    assert!(
        (reserve[0] - expected).abs() < 0.01,
        "{} vs {expected}",
        reserve[0]
    );
    assert_eq!(reserve[1..], [1.0, 1.0, 1.0], "away, at the door, remote");
    let after: Vec<_> = crabs.iter().map(|c| c.fighter.vitals.health).collect();
    assert_eq!(after, health, "health never changes");
    assert_eq!(hisses, 1);
    // Many pulses can empty the reserve but nothing more.
    for tick in FIRST + PULSE + 11..FIRST + PERIOD * 6 {
        bone_hall::step(tick, &level, &tuning, &mut crabs, &mut cache);
    }
    assert!(crabs[0].mover.reserve >= 0.0);
    assert_eq!(crabs[0].fighter.vitals.health, health[0]);
}

#[test]
fn prize_visible_from_the_door_within_800() {
    let level = Level::test_level();
    let (hall, _) = real_hall(&level);
    let door = hall.door.center();
    let d = door.distance(hall.prize);
    println!("door to prize {d:.0}");
    assert!(d <= 800.0);
    assert!(hall.rect.contains(hall.prize));
    for rock in &level.rocks {
        assert!(
            !segment_hits(door, hall.prize, *rock),
            "rock {rock:?} stands between the door and the prize"
        );
    }
    // The haze keeps clear of that line too, so nothing hides the prize.
    let (_, haze) = real_hall(&level);
    assert!(!segment_hits(door, hall.prize, haze));
}

fn segment_hits(a: Vec2, b: Vec2, r: Rect) -> bool {
    (0..=200).any(|i| r.contains(a.lerp(b, i as f32 / 200.0)))
}

#[test]
fn pulses_are_deterministic_and_coop_consistent() {
    let run = || {
        let mut sim = Sim::with_one_crab(Vec2::new(500.0, 0.0));
        sim.add_crab(PlayerId(1), Vec2::new(520.0, 0.0));
        if let Some(c) = sim.crab_mut(PlayerId(1)) {
            c.remote = true;
        }
        for _ in 0..FIRST + PERIOD * 2 {
            sim.tick();
        }
        sim
    };
    let (a, b) = (run(), run());
    assert_eq!(a.state_hash(), b.state_hash());
    let cues = |s: &Sim| {
        s.cues()
            .iter()
            .filter(|e| matches!(e.cue, Cue::Seep { .. }))
            .map(|e| e.tick)
            .collect::<Vec<_>>()
    };
    assert_eq!(cues(&a), cues(&b));
    // The pure drain agrees from the same state, for host and guest alike.
    let level = Level::test_level();
    let (_, haze) = real_hall(&level);
    let tuning = Tuning::default();
    let mut cache = bone_hall::Cache::default();
    let mut host = [
        crab_at(haze.center(), 0, false),
        crab_at(haze.center(), 1, true),
    ];
    let mut guest = [
        crab_at(haze.center(), 0, true),
        crab_at(haze.center(), 1, false),
    ];
    for tick in 1..=FIRST + PERIOD * 2 {
        bone_hall::step(tick, &level, &tuning, &mut host, &mut cache);
        bone_hall::step(tick, &level, &tuning, &mut guest, &mut cache);
    }
    assert_eq!(host[0].mover.reserve, guest[1].mover.reserve);
    assert_eq!(host[1].mover.reserve, 1.0);
    assert_eq!(guest[0].mover.reserve, 1.0);
}
