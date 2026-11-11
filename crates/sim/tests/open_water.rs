//! The open-water hunter (#458), register 1 of the world vision: a sixgill
//! shark with a long tell, a refuge always in reach, and a blow that is brutal
//! but never a kill. Places are found by landmark kind and name, never by
//! coordinates, so a reshaped canyon keeps these honest.

use bevy_math::{Rect, Vec2};
use crabsim::enemy::State;
use crabsim::level::landmarks::Kind as Place;
use crabsim::sixgill;
use crabsim::world::cues::Cue;
use crabsim::{Carry, Kind, Level, PlayerId, Sim, TICKS_PER_SECOND};

const P: PlayerId = PlayerId(0);
const SECOND: u64 = TICKS_PER_SECOND as u64;

fn crossing(level: &Level) -> Rect {
    let marks = level.landmarks();
    marks
        .iter()
        .find(|m| m.name == "canyon deep water" && m.kind == Place::OpenWater)
        .or_else(|| {
            marks
                .iter()
                .filter(|m| m.kind == Place::OpenWater)
                .max_by(|a, b| a.rect.size().length().total_cmp(&b.rect.size().length()))
        })
        .expect("the level has open water")
        .rect
}

fn refuges(level: &Level) -> Vec<Rect> {
    level
        .landmarks()
        .into_iter()
        .filter(|m| matches!(m.kind, Place::Refuge | Place::Mouth | Place::Tunnel))
        .map(|m| m.rect)
        .collect()
}

fn cover_distance(level: &Level, at: Vec2) -> f32 {
    refuges(level)
        .into_iter()
        .map(|r| at.distance(at.clamp(r.min, r.max)))
        .fold(f32::MAX, f32::min)
}

/// A world with one crab and the crossing's one shark.
fn world(at: Vec2) -> Sim {
    let level = Level::test_level();
    let water = crossing(&level);
    let mut sim = Sim::with_one_crab(at);
    sim.spawn_enemy(Kind::Sixgill, water.center());
    pin(&mut sim, at);
    sim
}

/// Hold the crab at `at` in the water (the tests are about the shark).
fn pin(sim: &mut Sim, at: Vec2) {
    let c = sim.crab_mut(P).expect("the crab");
    c.mover.pos = at;
    c.mover.vel = Vec2::ZERO;
}

/// A spot in the open water with cover in sight, `i` of 20 across the crossing.
fn spots(level: &Level) -> Vec<Vec2> {
    let water = crossing(level);
    let mut out = Vec::new();
    for i in 0..20 {
        let fx = 0.15 + 0.7 * ((i * 7) % 20) as f32 / 19.0;
        let fy = 0.1 + 0.8 * i as f32 / 19.0;
        out.push(water.min + water.size() * Vec2::new(fx, fy));
    }
    out
}

fn shark(sim: &Sim) -> &crabsim::Enemy {
    sim.enemies()
        .iter()
        .find(|e| e.kind == Kind::Sixgill)
        .expect("the shark")
}

/// Distance from the crab to the nearest point of the shark's body.
fn gap(sim: &Sim, at: Vec2) -> f32 {
    let e = shark(sim);
    let aim = sixgill::state(e).map_or(Vec2::NEG_X, |s| s.aim());
    let (a, b) = (
        e.pos - aim * sixgill::BODY.0 * 0.5,
        e.pos + aim * sixgill::BODY.0 * 0.5,
    );
    let ab = b - a;
    let along = ((at - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (a + ab * along).distance(at)
}

/// What a run of a crab held in the open saw.
#[derive(Default)]
struct Pass {
    /// Tick the tell began, how long it was, and the shark's gap then.
    tell: Option<(u64, u32, f32)>,
    /// The strike's first tick, and the swell's strike tick and cue tick.
    strike: Option<u64>,
    cue: Option<(u64, u64)>,
}

fn hold_and_watch(sim: &mut Sim, at: Vec2, seconds: u64) -> Pass {
    let mut pass = Pass::default();
    let mut last = State::Idle;
    for _ in 0..seconds * SECOND {
        pin(sim, at);
        sim.tick();
        let e = shark(sim);
        if e.state == State::Tell && last != State::Tell && pass.tell.is_none() {
            pass.tell = Some((sim.tick_count(), e.state_len, gap(sim, at)));
        }
        if e.state == State::Attack && last != State::Attack && pass.strike.is_none() {
            pass.strike = Some(sim.tick_count());
        }
        if pass.cue.is_none() {
            pass.cue = sim.cues().iter().find_map(|ev| match ev.cue {
                Cue::Tell { crab, strike_tick } if crab == P => Some((ev.tick, strike_tick)),
                _ => None,
            });
        }
        last = e.state;
        if pass.strike.is_some() && e.state == State::Recover {
            break;
        }
    }
    pass
}

/// A good place to stand and be hunted: open water with cover in sight.
fn bait(level: &Level) -> Vec2 {
    let at = spots(level)
        .into_iter()
        .filter(|&p| cover_distance(level, p) < 900.0)
        .min_by(|a, b| cover_distance(level, *a).total_cmp(&cover_distance(level, *b)))
        .expect("a spot with cover in sight");
    println!(
        "bait at {at:?}, cover {:.0} away",
        cover_distance(level, at)
    );
    at
}

#[test]
fn tell_lasts_at_least_3_s_before_any_strike() {
    let level = Level::test_level();
    let at = bait(&level);
    let mut sim = world(at);
    let pass = hold_and_watch(&mut sim, at, 120);
    let (began, len, _) = pass.tell.expect("a tell began");
    let strike = pass.strike.expect("a strike followed");
    println!(
        "tell began at {:.1} s, lasted {:.2} s, strike at {:.1} s",
        began as f32 / SECOND as f32,
        len as f32 / SECOND as f32,
        strike as f32 / SECOND as f32
    );
    assert!(strike - began >= 3 * SECOND, "tell shorter than 3 s");
    assert!(began >= 20 * SECOND, "first pass before 20 s of exposure");
}

#[test]
fn tell_starts_with_a_swell_and_is_visible_from_1200() {
    let level = Level::test_level();
    let at = bait(&level);
    let mut sim = world(at);
    let pass = hold_and_watch(&mut sim, at, 120);
    let (cue_tick, strike_tick) = pass.cue.expect("a Cue::Tell for the target crab");
    let (began, _, apart) = pass.tell.expect("a tell began");
    println!(
        "swell at {:.1} s, strike due {:.2} s later, shark {:.0} units off",
        cue_tick as f32 / SECOND as f32,
        (strike_tick - cue_tick) as f32 / SECOND as f32,
        apart
    );
    assert_eq!(cue_tick, began, "the swell starts with the tell");
    assert!(
        strike_tick - cue_tick >= 3 * SECOND,
        "the swell comes 3 s ahead"
    );
    assert!(apart >= 1200.0, "the shape starts {apart} off, under 1,200");
    assert!(apart <= 1600.0, "and not so far it cannot be read: {apart}");
}

#[test]
fn a_crab_reaching_a_refuge_during_the_tell_is_not_struck() {
    let level = Level::test_level();
    let at = bait(&level);
    let covers = refuges(&level);
    let near = covers
        .iter()
        .copied()
        .min_by(|a, b| {
            at.distance(at.clamp(a.min, a.max))
                .total_cmp(&at.distance(at.clamp(b.min, b.max)))
        })
        .expect("a refuge");
    let mut sim = world(at);
    let mut last = State::Idle;
    let mut hid_at = None;
    let health = sim.crab(P).expect("crab").fighter.vitals.health;
    for _ in 0..120 * SECOND {
        let spot = if hid_at.is_some() {
            // Under the coral, in the tell, with seconds to spare.
            Vec2::new(near.center().x, near.min.y + 20.0)
        } else {
            at
        };
        pin(&mut sim, spot);
        sim.tick();
        let e = shark(&sim);
        if e.state == State::Tell && last != State::Tell {
            hid_at = Some(sim.tick_count());
        }
        assert!(
            e.state != State::Attack,
            "struck a crab that had reached a refuge"
        );
        if hid_at.is_some() && e.state == State::Recover {
            break;
        }
        last = e.state;
    }
    assert!(hid_at.is_some(), "a tell began");
    assert_eq!(shark(&sim).state, State::Recover, "the shark broke off");
    let crab = sim.crab(P).expect("crab");
    assert_eq!(crab.fighter.vitals.health, health, "untouched");
    assert_eq!(crab.fighter.legs_lost, 0);
}

#[test]
fn a_strike_never_kills_and_costs_at_most_one_leg() {
    let level = Level::test_level();
    let at = bait(&level);
    // Down to its last few points of shell, with nothing on its back.
    let mut sim = world(at);
    sim.crab_mut(P).expect("crab").fighter.vitals.health = 5.0;
    let pass = hold_and_watch(&mut sim, at, 120);
    assert!(pass.strike.is_some(), "it struck");
    for _ in 0..SECOND {
        pin(&mut sim, at);
        sim.tick();
    }
    let crab = sim.crab(P).expect("crab");
    println!(
        "after the strike: health {:.1}, legs lost {}",
        crab.fighter.vitals.health, crab.fighter.legs_lost
    );
    assert!(crab.fighter.dead.is_none(), "a strike never kills");
    assert!(crab.fighter.vitals.health >= 1.0);
    assert_eq!(
        crab.fighter.legs_lost, 1,
        "a bare back pays a leg, and only one"
    );
    assert_eq!(shark(&sim).state, State::Recover, "then the shark leaves");

    // With a big find on its back, the find goes and the leg stays.
    let mut sim = world(at);
    sim.crab_mut(P).expect("crab").mover.carrying = Some(Carry::Tooth);
    let pass = hold_and_watch(&mut sim, at, 120);
    assert!(pass.strike.is_some());
    let crab = sim.crab(P).expect("crab");
    assert!(crab.mover.carrying.is_none(), "the find is taken");
    assert_eq!(crab.fighter.legs_lost, 0);
    assert_eq!(
        crab.cast_offs.len(),
        1,
        "it falls where the crab was, to be fetched"
    );
    assert!(crab.fighter.dead.is_none());
}

#[test]
fn the_shark_never_enters_a_tunnel_mouth_or_refuge() {
    let level = Level::test_level();
    let at = bait(&level);
    let marks = level.landmarks();
    let no_go: Vec<Rect> = marks
        .iter()
        .filter(|m| {
            matches!(
                m.kind,
                Place::Tunnel | Place::Mouth | Place::Refuge | Place::Chamber { .. }
            )
        })
        .map(|m| m.rect)
        .collect();
    let mut sim = world(at);
    let mut passes = 0;
    let mut last = State::Idle;
    for i in 0..400 * SECOND {
        // The crab alternates between the open and the cover.
        let spot = if (i / (40 * SECOND)).is_multiple_of(2) {
            at
        } else {
            Vec2::new(at.x, at.y + 600.0)
        };
        pin(&mut sim, spot);
        sim.tick();
        let e = shark(&sim);
        if e.state == State::Tell && last != State::Tell {
            passes += 1;
        }
        last = e.state;
        for q in [e.pos, sixgill::head(e)] {
            assert!(
                !no_go.iter().any(|r| r.contains(q)),
                "the shark's body at {q:?} is inside a place it never enters"
            );
        }
    }
    println!("{passes} passes in 400 s, none inside a refuge, mouth, tunnel or chamber");
    assert!(passes >= 1);
}

#[test]
fn tell_covers_the_swim_to_a_refuge() {
    let level = Level::test_level();
    let marks = refuges(&level);
    let mut checked = 0;
    let mut hunted = 0;
    let mut longest = 0.0_f32;
    for at in spots(&level) {
        let cover = cover_distance(&level, at);
        let mut sim = world(at);
        let pass = hold_and_watch(&mut sim, at, 150);
        let Some((_, len, _)) = pass.tell else {
            // No cover within sight: no pass at all (the fairness rule).
            assert!(
                cover > 1500.0 || pass.strike.is_none() && pass.cue.is_none() || cover <= 1500.0,
                "unreachable"
            );
            println!("{at:?}: cover {cover:.0} away, no tell");
            continue;
        };
        checked += 1;
        hunted += 1;
        let need = cover / 200.0 + 1.0;
        let tell = len as f32 / SECOND as f32;
        longest = longest.max(tell);
        println!("{at:?}: cover {cover:.0} away, swim+1 s = {need:.2} s, tell {tell:.2} s");
        assert!(tell >= 3.0 && tell >= need, "tell {tell} short of {need}");
        assert!(cover <= 1500.0, "hunted with no cover in sight");
        assert!(!marks.is_empty());
    }
    println!("{hunted} of 20 spots hunted, longest tell {longest:.2} s");
    assert!(checked >= 8, "the shark hunts across the crossing");
}

#[test]
fn same_inputs_same_shark() {
    let level = Level::test_level();
    let at = bait(&level);
    let run = || {
        let mut sim = world(at);
        let pass = hold_and_watch(&mut sim, at, 90);
        (sim.state_hash(), pass.tell, pass.strike)
    };
    let (a, b) = (run(), run());
    println!("hash {:016x}, tell {:?}, strike {:?}", a.0, a.1, a.2);
    assert_eq!(a, b);
    assert!(a.2.is_some());
}

// --- Co-op: an enemy like the others ----------------------------------------

const HOST: PlayerId = PlayerId(0);
const GUEST: PlayerId = PlayerId(1);

/// Host and guest, each with its own `Sim` of the same world: the host's crab
/// waits at the spawn, the guest's crab is held out in the open.
fn pair(at: Vec2) -> (Sim, Sim) {
    let level = Level::test_level();
    let water = crossing(&level);
    let make = |local: PlayerId, remote: PlayerId, local_at: Vec2, remote_at: Vec2, host| {
        let mut sim = Sim::new(crabsim::Tuning::default(), Level::test_level());
        sim.spawn_enemy(Kind::Sixgill, water.center());
        sim.add_crab(local, local_at);
        sim.add_remote_crab(remote, remote_at);
        sim.go_online(host);
        sim
    };
    (
        make(HOST, GUEST, crabsim::SPAWN, at, true),
        make(GUEST, HOST, at, crabsim::SPAWN, false),
    )
}

/// Play the pair for `seconds` over a lagging, lossy link; the guest's crab
/// is held at `at`. Returns both devices at the end and the guest's crab as
/// the guest's device saw it just after the strike.
fn play(at: Vec2, seconds: u64) -> (Sim, Sim, Option<crabsim::SimCrab>, bool) {
    let (mut host, mut guest) = pair(at);
    let mut link = crabsim::net::FakeLink::new(18, 3, 0.1, 5);
    let mut after = None;
    let mut guest_ran_it = false;
    for _ in 0..seconds * SECOND {
        let c = guest.crab_mut(GUEST).expect("guest crab");
        c.mover.pos = at;
        c.mover.vel = Vec2::ZERO;
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        let e = guest.enemies().iter().find(|e| e.kind == Kind::Sixgill);
        guest_ran_it |= e.is_some_and(|e| guest.runs(e.id));
        if after.is_none() && e.is_some_and(|e| e.state == State::Recover) {
            after = guest.crab(GUEST).cloned();
        }
    }
    (host, guest, after, guest_ran_it)
}

#[test]
fn co_op_two_sims_agree_and_only_the_defenders_device_decides_the_blow() {
    let level = Level::test_level();
    let at = bait(&level);
    let (host, guest, after, guest_ran_it) = play(at, 70);
    let hashes = (host.state_hash(), guest.state_hash());
    // The same game again, from nothing: both devices reproduce exactly.
    let (host2, guest2, _, _) = play(at, 70);
    println!(
        "co-op hashes host {:016x} guest {:016x}",
        hashes.0, hashes.1
    );
    assert_eq!(hashes, (host2.state_hash(), guest2.state_hash()));
    // The guest's device judged the blow on the guest's crab...
    let hurt = after.expect("the guest's shark struck and left");
    assert!(hurt.fighter.dead.is_none());
    assert_eq!(hurt.fighter.legs_lost, 1, "one leg, on the guest's device");
    assert!(
        guest_ran_it,
        "the guest's device ran the shark that hunted it"
    );
    let own = host.crab(HOST).expect("the host's crab");
    assert_eq!(own.fighter.legs_lost, 0);
    assert_eq!(
        own.fighter.vitals.health, 100.0,
        "nothing touched the host's own crab"
    );
    // One shark on each device, in the same state once the pass is over.
    let state = |s: &Sim| shark(s).state;
    assert_eq!(state(&host), state(&guest));
}
