//! The Plunge (#470, #471, #472): the odd tunnel, the fall, the crack, the
//! vent. Built on open water from the real frames' rects, so the rules are
//! proved whatever shape the world's geometry gives the frames.
use bevy_math::{Rect, Vec2};
use crabsim::combat::Hit;
use crabsim::plunge::{self, Phase, Places};
use crabsim::world::cues::Cue;
use crabsim::{Carry, Controls, Kind, Level, PlayerId, STEP, Sim, Tuning};

const A: PlayerId = PlayerId(0);
const B: PlayerId = PlayerId(1);
const HZ: u32 = 120;

fn places() -> Places {
    Places::from_landmarks(&Level::test_level().landmarks()).expect("the Plunge's frames")
}

/// Open water over a far floor, with a floor slab under the odd tunnel and one
/// under the crack to stand on.
fn level(p: &Places) -> Level {
    let mut level = Level::test_level();
    level.foes.clear();
    level.rocks.clear();
    level.currents.clear();
    level.upwelling.clear();
    level.floor = vec![Vec2::new(-5000.0, -50000.0), Vec2::new(40000.0, -50000.0)];
    level.min_x = -5000.0;
    level.max_x = 40000.0;
    for r in [p.tunnel, p.crack] {
        level.rocks.push(Rect::new(
            r.min.x - 50.0,
            r.min.y - 40.0,
            r.max.x + 50.0,
            r.min.y,
        ));
    }
    level
}

fn sim(crabs: &[(PlayerId, Vec2)]) -> Sim {
    let p = places();
    let mut sim = Sim::new(Tuning::default(), level(&p));
    for &(id, at) in crabs {
        sim.add_crab(id, at);
        sim.crab_mut(id).expect("crab").mover.act_three = true;
    }
    sim
}

fn at_hole(dx: f32) -> Vec2 {
    let h = places().hole();
    Vec2::new(h.x + dx, h.y)
}

fn go(sim: &mut Sim, id: PlayerId, c: Controls, ticks: u32) {
    for _ in 0..ticks {
        sim.set_controls(id, c);
        sim.tick();
    }
}

fn phase(sim: &Sim, id: PlayerId) -> Phase {
    sim.plunge(id).expect("crab").phase
}

fn pos(sim: &Sim, id: PlayerId) -> Vec2 {
    sim.crab(id).expect("crab").mover.pos
}

fn broken(sim: &mut Sim, id: PlayerId) {
    sim.crab_mut(id).expect("crab").plunge.dig = plunge::DIG_BREAK;
}

/// Tick (holding `c`) until `done`, at most `max` ticks; the ticks it took.
fn until(sim: &mut Sim, id: PlayerId, c: Controls, max: u32, done: impl Fn(&Sim) -> bool) -> u32 {
    for n in 0..max {
        if done(sim) {
            return n;
        }
        sim.set_controls(id, c);
        sim.tick();
    }
    panic!("not done in {max} ticks");
}

fn in_crack(s: &Sim) -> bool {
    s.plunge(A).is_some_and(|p| p.in_crack())
}

/// Into the crack by the fall, from a broken way.
fn fall(sim: &mut Sim, c: Controls) -> u32 {
    broken(sim, A);
    until(sim, A, c, RIDE_MAX, |s| {
        matches!(phase(s, A), Phase::Ride { .. })
    });
    until(sim, A, c, RIDE_MAX, in_crack)
}

const RIDE_MAX: u32 = 60 * HZ;

fn walk(dir: f32) -> Controls {
    Controls {
        walk: dir,
        ..Controls::default()
    }
}

#[test]
fn the_odd_tunnel_has_no_tells_before_act_3() {
    let mut s = sim(&[(A, at_hole(0.0))]);
    s.crab_mut(A).expect("crab").mover.act_three = false;
    assert_eq!(s.odd_tunnel_tells(A), None, "no tells before act three");
    s.crab_mut(A).expect("crab").mover.act_three = true;
    let tells = s.odd_tunnel_tells(A).expect("tells with act three");
    assert_eq!(tells.smell, crabsim::level::bands::Smell::Metallic);
    assert!(tells.snow > 0.9 && tells.cold > 0.9, "{tells:?}");
    // And none far from the tunnel.
    let mut far = sim(&[(A, at_hole(6000.0))]);
    far.crab_mut(A).expect("crab").mover.act_three = true;
    assert_eq!(far.odd_tunnel_tells(A), None);
    // Digging without act three never breaks the way.
    let mut early = sim(&[(A, at_hole(100.0))]);
    early.crab_mut(A).expect("crab").mover.act_three = false;
    go(
        &mut early,
        A,
        Controls {
            dig_held: true,
            ..Controls::default()
        },
        plunge::DIG_BREAK + 10,
    );
    assert_eq!(early.plunge(A).expect("crab").dig, 0);
}

#[test]
fn digging_breaks_in_between_20_and_40_s() {
    let mut s = sim(&[(A, at_hole(100.0))]);
    let dig = Controls {
        dig_held: true,
        ..Controls::default()
    };
    let n = until(&mut s, A, dig, 60 * HZ, |s| {
        s.plunge(A).expect("crab").broken()
    });
    let secs = n as f32 * STEP;
    println!("the way breaks after {secs:.1} s of digging");
    assert!((20.0..=40.0).contains(&secs), "{secs}");
    // Silt first, then the roar.
    let mut silt = None;
    let mut roar = None;
    let mut s = sim(&[(A, at_hole(100.0))]);
    for _ in 0..plunge::DIG_BREAK {
        s.set_controls(A, dig);
        s.tick();
        for e in s.cues().iter().filter(|e| e.tick == s.tick_count()) {
            match e.cue {
                Cue::PlungeSilt { .. } => silt = silt.or(Some(e.tick)),
                Cue::PlungeRoar { .. } => roar = roar.or(Some(e.tick)),
                _ => {}
            }
        }
    }
    let (silt, roar) = (silt.expect("silt trickles"), roar.expect("a roar"));
    assert!(silt < roar, "silt {silt} then roar {roar}");
}

#[test]
fn the_lip_lets_a_crab_walk_back_out() {
    let start = places().tunnel.width() / 2.0 - 30.0;
    let mut s = sim(&[(A, at_hole(start))]);
    broken(&mut s, A);
    go(&mut s, A, walk(1.0), 5 * HZ);
    assert_eq!(phase(&s, A), Phase::Walking, "walked away from the hole");
    assert!(pos(&s, A).x > at_hole(start).x, "and outwards");
    // Standing still at the lip, it is drawn in slowly (not at once).
    let mut s = sim(&[(A, at_hole(start))]);
    broken(&mut s, A);
    let n = until(&mut s, A, Controls::default(), 60 * HZ, |s| {
        matches!(phase(s, A), Phase::Ride { .. })
    });
    let secs = n as f32 * STEP;
    println!("the lip draws a still crab in after {secs:.1} s");
    assert!(secs > 2.0, "time to turn back: {secs}");
}

#[test]
fn the_ride_always_ends_at_the_landing() {
    let p = places();
    let scripts: Vec<Controls> = vec![
        Controls::default(),
        walk(1.0),
        walk(-1.0),
        Controls {
            jump_held: true,
            ..walk(1.0)
        },
        Controls {
            down: true,
            ..walk(-1.0)
        },
        Controls {
            jump_held: true,
            ..Controls::default()
        },
        Controls {
            down: true,
            ..Controls::default()
        },
        Controls {
            dodge_held: true,
            dig_held: true,
            ..walk(1.0)
        },
        Controls {
            smash_held: true,
            block_held: true,
            ..walk(-1.0)
        },
        Controls {
            jump_held: true,
            down: true,
            ..walk(1.0)
        },
    ];
    let mut runs = 0;
    for (i, c) in scripts.iter().enumerate() {
        for dx in [-70.0, 60.0] {
            let mut s = sim(&[(A, at_hole(dx))]);
            broken(&mut s, A);
            let c = *c;
            // Hold the script through the lip too, then through the ride.
            for _ in 0..RIDE_MAX {
                if matches!(phase(&s, A), Phase::Ride { .. }) {
                    break;
                }
                s.set_controls(A, c);
                s.tick();
            }
            if !matches!(phase(&s, A), Phase::Ride { .. }) {
                continue;
            }
            until(&mut s, A, c, RIDE_MAX, in_crack);
            let at = pos(&s, A);
            assert!(
                at.distance(p.crack_landing()) < 1.0,
                "script {i} dx {dx}: ended at {at}, landing {}",
                p.crack_landing()
            );
            runs += 1;
        }
    }
    println!("{runs} rides, all ended at the landing");
    assert!(runs >= 8);
    // Entries by the friend rule (digging beside), from every direction held.
    for c in scripts {
        let mut s = sim(&[(A, at_hole(0.0)), (B, at_hole(80.0))]);
        broken(&mut s, A);
        s.crab_mut(B).expect("crab").plunge.dig = 5;
        let dig = Controls {
            dig_held: true,
            ..c
        };
        for _ in 0..(RIDE_MAX + 10) {
            s.set_controls(A, c);
            s.set_controls(B, dig);
            s.tick();
            if s.plunge(B).is_some_and(|p| p.in_crack()) {
                break;
            }
        }
        assert!(pos(&s, B).distance(p.crack_landing()) < 1.0);
    }
}

#[test]
fn the_ride_never_damages_or_kills() {
    let mut s = sim(&[(A, at_hole(0.0))]);
    for kind in [
        Kind::Mantis,
        Kind::Snapper,
        Kind::Moray,
        Kind::Octopus,
        Kind::Triggerfish,
    ] {
        s.spawn_enemy(kind, Vec2::ZERO);
    }
    let whole = s.crab(A).expect("crab").fighter.vitals.health;
    broken(&mut s, A);
    let mut ticks = 0;
    let mut seen_ride = false;
    for _ in 0..RIDE_MAX {
        let at = pos(&s, A);
        for e in s.enemies_mut() {
            e.pos = at; // every hunter on top of it
        }
        s.set_controls(A, Controls::default());
        s.tick();
        let c = s.crab(A).expect("crab");
        assert_eq!(c.fighter.vitals.health, whole, "damage at tick {ticks}");
        assert!(c.fighter.dead.is_none());
        if matches!(phase(&s, A), Phase::Ride { .. }) {
            seen_ride = true;
            ticks += 1;
        }
        if c.plunge.in_crack() {
            break;
        }
    }
    assert!(seen_ride);
    let secs = ticks as f32 * STEP;
    println!("ride length {secs:.1} s");
    assert!((25.0..=40.0).contains(&secs));
    let c = s.crab(A).expect("crab");
    assert_eq!(c.strain, 1.0, "strain goes to full");
    assert!(c.scars >= 4, "cracks stay: {}", c.scars);
}

#[test]
fn the_landing_is_a_tunnel_with_no_hunters() {
    let mut s = sim(&[(A, at_hole(0.0))]);
    fall(&mut s, Controls::default());
    for kind in [Kind::Mantis, Kind::Snapper, Kind::Moray, Kind::Octopus] {
        s.spawn_enemy(kind, Vec2::ZERO);
    }
    let whole = s.crab(A).expect("crab").fighter.vitals.health;
    for _ in 0..(30 * HZ) {
        let at = pos(&s, A);
        for e in s.enemies_mut() {
            e.pos = at;
        }
        s.set_controls(A, Controls::default());
        s.tick();
        assert_eq!(s.crab(A).expect("crab").fighter.vitals.health, whole);
        assert!(!s.threatened(A));
    }
}

/// From the broken way to still in the crack, then the scene to its end.
fn watch(s: &mut Sim) {
    until(s, A, Controls::default(), 10 * HZ, |s| {
        s.plunge(A).is_some_and(|p| p.scene().is_some())
    });
    until(s, A, Controls::default(), 30 * HZ, |s| {
        s.plunge(A).is_some_and(|p| p.scene().is_none())
    });
}

/// Walk to `x` (feet) in the crack.
fn walk_to(s: &mut Sim, x: f32, stop: impl Fn(&Sim) -> bool) {
    for _ in 0..(10 * HZ) {
        if stop(s) {
            return;
        }
        let d = x - pos(s, A).x;
        s.set_controls(A, walk(d.signum()));
        s.tick();
    }
}

#[test]
fn the_flash_happens_once_per_crab() {
    let p = places();
    let mut s = sim(&[(A, at_hole(0.0))]);
    fall(&mut s, Controls::default());
    assert!(!s.plunge(A).expect("crab").flash_seen);
    watch(&mut s);
    assert!(s.plunge(A).expect("crab").flash_seen);
    // Out by the vent, down again, still again: no second scene.
    walk_to(&mut s, p.vent().x, |s| {
        s.plunge(A).is_some_and(|p| p.rise().is_some())
    });
    until(&mut s, A, Controls::default(), 30 * HZ, |s| {
        phase(s, A) == Phase::Walking
    });
    fall(&mut s, Controls::default());
    for _ in 0..(10 * HZ) {
        s.tick();
        assert!(s.plunge(A).expect("crab").scene().is_none(), "played twice");
    }
}

#[test]
fn the_treasure_cannot_be_taken_twice() {
    let p = places();
    let mut s = sim(&[(A, at_hole(0.0))]);
    fall(&mut s, Controls::default());
    walk_to(&mut s, p.treasure().x, |s| {
        s.plunge(A).expect("crab").treasure_taken
    });
    let wild = |s: &Sim| {
        s.crab(A)
            .expect("crab")
            .mover
            .clinging
            .iter()
            .filter(|c| **c == Carry::Item(crabsim::Item::WildLight))
            .count()
    };
    assert_eq!(wild(&s), 1);
    // The workbench keeps it as the blue-green culture.
    assert_eq!(
        crabsim::Material::kept(Carry::Item(crabsim::Item::WildLight)),
        Some(crabsim::Item::WildLight)
    );
    assert!(crabsim::Recipe::parse("blue-lantern").is_some());
    // Leave (after the answer), come back, touch the spot again.
    go(&mut s, A, Controls::default(), 12 * HZ);
    walk_to(&mut s, p.vent().x, |s| {
        s.plunge(A).is_some_and(|p| p.rise().is_some())
    });
    until(&mut s, A, Controls::default(), 30 * HZ, |s| {
        phase(s, A) == Phase::Walking
    });
    fall(&mut s, Controls::default());
    walk_to(&mut s, p.treasure().x, |_| false);
    assert_eq!(wild(&s), 1, "only one ever");
}

#[test]
fn nothing_in_the_scene_can_hurt_the_crab() {
    let mut s = sim(&[(A, at_hole(0.0))]);
    fall(&mut s, Controls::default());
    s.spawn_enemy(Kind::Mantis, Vec2::ZERO);
    let whole = s.crab(A).expect("crab").fighter.vitals.health;
    let blow = Hit {
        damage: 60.0,
        ..Hit::default()
    };
    let mut scene_ticks = 0;
    for _ in 0..(40 * HZ) {
        let at = pos(&s, A);
        for e in s.enemies_mut() {
            e.pos = at;
        }
        s.hit_crab(A, &blow);
        s.tick();
        if s.plunge(A).expect("crab").scene().is_some() {
            scene_ticks += 1;
        }
        assert_eq!(s.crab(A).expect("crab").fighter.vitals.health, whole);
    }
    assert!(scene_ticks as f32 * STEP > 15.0, "the scene played");
}

#[test]
fn the_answer_blinks_twice_after_the_treasure_and_never_again() {
    let p = places();
    let blinks = |s: &mut Sim, ticks: u32| {
        let (mut edges, mut was) = (0, false);
        for _ in 0..ticks {
            s.set_controls(A, Controls::default());
            s.tick();
            let lit = s
                .plunge(A)
                .expect("crab")
                .answer()
                .is_some_and(plunge::answer_lit);
            if lit && !was {
                edges += 1;
            }
            was = lit;
        }
        edges
    };
    let mut s = sim(&[(A, at_hole(0.0))]);
    fall(&mut s, Controls::default());
    // Nothing answers before the treasure.
    assert_eq!(blinks(&mut s, 8 * HZ), 0);
    walk_to(&mut s, p.treasure().x, |s| {
        s.plunge(A).expect("crab").treasure_taken
    });
    assert_eq!(blinks(&mut s, 10 * HZ), 2, "two blinks");
    assert!(s.plunge(A).expect("crab").answered);
    // Home and back: never again.
    walk_to(&mut s, p.vent().x, |s| {
        s.plunge(A).is_some_and(|p| p.rise().is_some())
    });
    until(&mut s, A, Controls::default(), 30 * HZ, |s| {
        phase(s, A) == Phase::Walking
    });
    fall(&mut s, Controls::default());
    assert_eq!(blinks(&mut s, 12 * HZ), 0);
}

#[test]
fn the_vent_always_lands_in_the_start_tunnel() {
    let p = places();
    for shored in [false, true] {
        let mut s = sim(&[(A, at_hole(0.0))]);
        if shored {
            // The tunnel dug away or shored: rock over the landing.
            let l = p.home_landing();
            s.level
                .rocks
                .push(Rect::new(l.x - 30.0, l.y - 5.0, l.x + 30.0, l.y + 100.0));
        }
        fall(&mut s, Controls::default());
        watch(&mut s);
        walk_to(&mut s, p.vent().x, |s| {
            s.plunge(A).is_some_and(|p| p.rise().is_some())
        });
        let n = until(&mut s, A, walk(-1.0), 60 * HZ, |s| {
            phase(s, A) == Phase::Walking
        });
        let secs = n as f32 * STEP;
        println!("vent ride {secs:.1} s (shored: {shored})");
        assert!((15.0..=25.0).contains(&secs));
        assert!(
            pos(&s, A).distance(p.home_landing()) < 1.0,
            "{}",
            pos(&s, A)
        );
        assert!(p.tunnel.contains(pos(&s, A) + Vec2::Y));
        let c = s.crab(A).expect("crab");
        assert_eq!(c.fighter.vitals.health, c.mover.max_shell(&s.tuning));
        assert_eq!(c.plunge.dig, 0, "the way is closed again");
    }
}

#[test]
fn same_inputs_same_ride() {
    let run = || {
        let mut s = sim(&[(A, at_hole(40.0))]);
        broken(&mut s, A);
        let mut hashes = Vec::new();
        for t in 0..(45 * HZ) {
            s.set_controls(
                A,
                Controls {
                    walk: ((t / 200) % 3) as f32 - 1.0,
                    jump_held: (t / 350) % 2 == 0,
                    ..Controls::default()
                },
            );
            s.tick();
            hashes.push(s.state_hash());
        }
        (hashes, pos(&s, A))
    };
    assert_eq!(run(), run());
}

#[test]
fn a_friend_beside_comes_too() {
    let mut s = sim(&[(A, at_hole(0.0)), (B, at_hole(100.0))]);
    broken(&mut s, A);
    let dig = Controls {
        dig_held: true,
        ..Controls::default()
    };
    for _ in 0..4 {
        s.set_controls(A, Controls::default());
        s.set_controls(B, dig);
        s.tick();
    }
    assert!(matches!(phase(&s, A), Phase::Ride { .. }));
    assert!(matches!(phase(&s, B), Phase::Ride { .. }), "digging beside");
    // One merely standing beside does not.
    let mut s = sim(&[(A, at_hole(0.0)), (B, at_hole(100.0))]);
    broken(&mut s, A);
    go(&mut s, A, Controls::default(), 4);
    assert!(matches!(phase(&s, A), Phase::Ride { .. }));
    assert_eq!(phase(&s, B), Phase::Walking);
    // One too far away does not.
    let mut s = sim(&[(A, at_hole(0.0)), (B, at_hole(-190.0))]);
    broken(&mut s, A);
    for _ in 0..4 {
        s.set_controls(B, dig);
        s.tick();
    }
    assert_eq!(phase(&s, B), Phase::Walking);
}

/// The same trip on the real, built map (its terrain, creatures and cells as
/// shipped, not an open-water stand-in): the frames are open water there, and
/// the ride, the crack and the vent still keep their promises.
#[test]
fn the_whole_plunge_works_on_the_real_map() {
    let p = places();
    for dx in [-60.0, 0.0, 70.0] {
        let mut s = Sim::new(Tuning::default(), Level::test_level());
        s.add_crab(A, at_hole(dx));
        // The real ground, with its cells: the frame is sealed inside the
        // rock, so set the crab down in it.
        s.enable_underwater_cells();
        let mut cells = s.level.cells.take().expect("cells");
        cells.follow_player(&s.level, p.hole().x, p.hole().y);
        s.level.cells = Some(cells);
        s.crab_mut(A).expect("crab").mover.pos = at_hole(dx);
        s.crab_mut(A).expect("crab").mover.act_three = true;
        let full = s.crab(A).expect("crab").fighter.vitals.health;
        let n = fall(&mut s, Controls::default());
        println!("real map: fall {:.1} s from dx {dx}", n as f32 * STEP);
        assert!(p.crack.contains(pos(&s, A) + Vec2::Y), "{}", pos(&s, A));
        watch(&mut s);
        walk_to(&mut s, p.vent().x, |s| {
            s.plunge(A).is_some_and(|p| p.rise().is_some())
        });
        let n = until(&mut s, A, walk(-1.0), 60 * HZ, |s| {
            phase(s, A) == Phase::Walking
        });
        let secs = n as f32 * STEP;
        println!("real map: vent ride {secs:.1} s");
        assert!((15.0..=25.0).contains(&secs));
        assert!(pos(&s, A).distance(p.home_landing()) < 1.0, "{}", pos(&s, A));
        let c = s.crab(A).expect("crab");
        assert_eq!(c.fighter.vitals.health, full, "never hurt on the way");
        assert!(c.fighter.dead.is_none());
    }
}
