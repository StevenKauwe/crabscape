//! Machine checks for the legacy world's three fear registers (#456).
//! Rock-dependent failures remain executable under explicit W1.3 ignores.
use bevy_math::{Rect, Vec2};
use crabsim::Level;
use crabsim::level::Growth;
use crabsim::level::landmarks::{Kind, Needs};
use std::collections::HashSet;

#[test]
fn every_landmark_has_a_unique_name_and_positive_area() {
    let level = Level::test_level();
    assert!(!level.landmarks().is_empty());
    let mut names = HashSet::new();
    for landmark in &level.landmarks() {
        assert!(!landmark.name.trim().is_empty());
        assert!(names.insert(&landmark.name), "duplicate {}", landmark.name);
        assert!(landmark.rect.min.is_finite() && landmark.rect.max.is_finite());
        assert!(landmark.rect.width() > 0.0 && landmark.rect.height() > 0.0);
    }
}

#[test]
fn every_gate_landmark_covers_an_existing_net_brittle_or_bare_rock() {
    let level = Level::test_level();
    let expected: Vec<_> = level
        .nets
        .iter()
        .map(|n| (n.rect, Needs::Scissors))
        .chain(level.brittle.iter().map(|b| (b.rect, Needs::Crusher)))
        .chain(
            level
                .climbs
                .iter()
                .filter(|c| c.kind == Growth::Rock)
                .map(|c| {
                    (
                        Rect::new(c.x - 60.0, c.bottom, c.x + 60.0, c.top),
                        Needs::GlassGrips,
                    )
                }),
        )
        .collect();
    let actual: Vec<_> = level
        .landmarks()
        .iter()
        .filter_map(|l| match l.kind {
            Kind::Gate { needs } => Some((l.rect, needs)),
            _ => None,
        })
        .collect();
    assert_eq!(actual.len(), expected.len());
    for gate in &expected {
        assert_eq!(actual.iter().filter(|g| *g == gate).count(), 1, "{gate:?}");
    }
    println!("gates tagged {}/{}", actual.len(), expected.len());
}

#[test]
fn home_and_every_crevice_are_refuges() {
    let level = Level::test_level();
    let expected: Vec<_> = std::iter::once(level.home)
        .chain(level.crevices.iter().map(|c| c.door))
        .chain(
            level
                .climbs
                .iter()
                .filter(|c| c.kind != Growth::Rock)
                .map(|c| Rect::new(c.x - 60.0, c.bottom, c.x + 60.0, c.top)),
        )
        .collect();
    for rect in &expected {
        assert_eq!(
            level
                .landmarks()
                .iter()
                .filter(|l| l.kind == Kind::Refuge && l.rect == *rect)
                .count(),
            1
        );
    }
    println!("refuges tagged {}/{}", expected.len(), expected.len());
}

#[test]
fn landmarks_are_derived_the_same_every_time() {
    let level = Level::test_level();
    assert_eq!(level.landmarks(), Level::test_level().landmarks());
    assert_eq!(level.landmarks(), level.clone().landmarks());
}

#[test]
fn authored_chambers_and_chimney_are_tagged() {
    let level = Level::test_level();
    for (name, rect, prize) in [
        ("octopus den", level.den, Vec2::new(8650.0, -1896.0)),
        ("far-wall cave", level.secrets[1], level.eggs[4]),
    ] {
        let all = level.landmarks();
        let landmark = all
            .iter()
            .find(|l| l.name == name)
            .expect("authored chamber");
        assert_eq!(landmark.rect, rect);
        let Kind::Chamber {
            door,
            prize: actual,
        } = landmark.kind
        else {
            panic!("{name} must be a chamber")
        };
        assert_eq!(actual, prize);
        assert!(rect.contains(prize));
        assert_eq!(door.min.x, rect.min.x);
        assert_eq!(door.min.y, rect.min.y);
        assert_eq!(door.max.y, rect.max.y);
        assert!(door.width() > 0.0 && door.width() <= 60.0);
        println!(
            "{name} door-to-prize {:.1} units",
            door.center().distance(prize)
        );
    }
    assert!(
        level
            .landmarks()
            .iter()
            .any(|l| l.kind == Kind::Tunnel && l.rect.contains(Vec2::new(5440.0, -1000.0)))
    );
    for point in [
        Vec2::new(7000.0, -2000.0),
        Vec2::new(7000.0, -9000.0),
        Vec2::new(3800.0, -8000.0),
        Vec2::new(3000.0, -600.0),
        Vec2::new(4800.0, -600.0),
    ] {
        assert!(
            level
                .landmarks()
                .iter()
                .any(|l| l.kind == Kind::OpenWater && l.rect.contains(point)),
            "{point:?}"
        );
    }
}

mod common;
// Compile the production line-of-sight helper here without widening the sim API.
#[path = "../src/navigation.rs"]
mod navigation;

use common::route::{self, Kit};
use crabsim::{Controls, Tuning};

fn visible(level: &Level, from: Vec2, to: Vec2) -> bool {
    let solids: Vec<_> = level.solids().collect();
    navigation::clear_line(from, to, &|p| {
        p.y < level.floor_height(p.x) || solids.iter().any(|r| r.contains(p))
    })
}

#[test]
fn every_chamber_shows_its_prize_from_the_door() {
    let level = Level::test_level();
    let mut count = 0;
    for landmark in level.landmarks() {
        if let Kind::Chamber { door, prize } = landmark.kind {
            count += 1;
            let distance = door.center().distance(prize);
            println!("{} door-to-prize {distance:.1} units", landmark.name);
            assert!(distance <= 800.0, "{}: {distance:.1} > 800", landmark.name);
            assert!(
                visible(&level, door.center(), prize),
                "{}: prize hidden from {:?}",
                landmark.name,
                door.center()
            );
        }
    }
    assert!(count > 0);
}

#[test]
fn no_hunter_roams_inside_a_tunnel() {
    let level = Level::test_level();
    let tuning = Tuning::default();
    let landmarks = level.landmarks();
    let mut count = 0;
    for tunnel in landmarks.iter().filter(|l| l.kind == Kind::Tunnel) {
        count += 1;
        for &(kind, home) in &level.foes {
            let k = kind.tuning(&tuning);
            assert!(
                !tunnel.rect.contains(home),
                "{kind:?} spawns in {} at {home}",
                tunnel.name
            );
            // Conservative envelope of chase centres plus body and lunge.
            // Include every floor height in a walker's leash, even across
            // cliffs it cannot step over, so slopes cannot hide an intrusion.
            let swimming = matches!(
                kind,
                crabsim::enemy::Kind::Moray
                    | crabsim::enemy::Kind::Triggerfish
                    | crabsim::enemy::Kind::AlarmJelly
            );
            let leash = crabsim::enemy::Enemy::new(0, kind, home, &tuning).home_leash(&tuning);
            let chase = leash + k.approach_speed * k.tell_ticks as f32 * route::DT as f32;
            let (bottom, top) = if swimming {
                (home.y - chase, home.y + chase)
            } else {
                level
                    .floor
                    .iter()
                    .filter(|p| (p.x - home.x).abs() <= chase)
                    .map(|p| p.y)
                    .chain([
                        home.y,
                        level.floor_height(home.x - chase),
                        level.floor_height(home.x + chase),
                    ])
                    .fold((home.y, home.y), |(low, high), y| (low.min(y), high.max(y)))
            };
            let centres = Rect::new(home.x - chase, bottom, home.x + chase, top);
            let horizontal = chase + k.body.0 * 0.5 + k.strike.lunge;
            let envelope = Rect::new(
                home.x - horizontal,
                bottom,
                home.x + horizontal,
                top + k.body.1,
            );
            let intrusion = envelope.intersect(tunnel.rect);
            let centres_enter = centres.min.x < tunnel.rect.max.x
                && centres.max.x > tunnel.rect.min.x
                && centres.min.y <= tunnel.rect.max.y
                && centres.max.y >= tunnel.rect.min.y;
            let body_reach = Rect::new(
                centres.min.x - k.body.0 * 0.5,
                bottom,
                centres.max.x + k.body.0 * 0.5,
                top + k.body.1,
            );
            let mouth_only = !centres_enter
                && body_reach.contains(intrusion.min)
                && body_reach.contains(intrusion.max)
                && landmarks.iter().any(|l| {
                    l.kind == Kind::Mouth
                        && l.rect.contains(intrusion.min)
                        && l.rect.contains(intrusion.max)
                });
            assert!(
                intrusion.is_empty() || mouth_only,
                "{kind:?} at {home}: leash {envelope:?} overlaps {}",
                tunnel.name
            );
        }
    }
    assert!(count > 0);
    println!("{count} tunnels checked; zero hunter spawns or roaming intrusions");
}

#[test]
fn far_rim_landing_can_hold_a_crab_still() {
    let level = Level::test_level();
    let landmarks = level.landmarks();
    let landing = landmarks
        .iter()
        .find(|l| l.name == "far-rim landing")
        .expect("rim refuge");
    let half = Tuning::default().body_half_width;
    for x in [
        landing.rect.min.x + half,
        landing.rect.center().x,
        landing.rect.max.x - half,
    ] {
        let start = Vec2::new(x, landing.rect.min.y);
        let mut sim = route::world(Kit::default(), start, false);
        for _ in 0..240 {
            sim.tick();
        }
        let mover = &sim.crab(route::ME).expect("resting crab").mover;
        assert!(
            mover.grounded && mover.pos.distance(start) < 1.0,
            "rim refuge cannot hold a crab at {start}: {}",
            mover.pos
        );
    }
}

#[test]
fn every_chamber_has_a_retreat_within_6s() {
    chamber_retreats(false);
}

/// W1.3 https://github.com/StevenKauwe/crabscape/issues/459:
/// the far-wall cave's west door (7700..7720, -5200..-5030) opens
/// directly onto a fall. Add a supported escape to shelter, within six
/// walking seconds of the prize at (7810, -5170).
#[test]
#[ignore = "needs rock: W1.3 #459"]
fn far_wall_cave_has_a_retreat_within_6s() {
    chamber_retreats(true);
}

fn chamber_retreats(cave: bool) {
    let level = Level::test_level();
    let landmarks = level.landmarks();
    for chamber in &landmarks {
        if (chamber.name == "far-wall cave") != cave {
            continue;
        }
        let Kind::Chamber { door, prize } = chamber.kind else {
            continue;
        };
        // Walk through the doorway before accepting shelter. Every tick is
        // measured, including acceleration and any fall on the way out.
        let mut sim = route::world(Kit::default(), prize, false);
        let direction = (door.center().x - prize.x).signum();
        let mut through_door = false;
        let mut retreat = None;
        for tick in 0..=720 {
            let p = sim.crab(route::ME).expect("retreat crab").mover.pos;
            through_door |= door.contains(p);
            if through_door
                && landmarks
                    .iter()
                    .any(|l| matches!(l.kind, Kind::Tunnel | Kind::Refuge) && l.rect.contains(p))
            {
                retreat = Some(f64::from(tick) * route::DT);
                break;
            }
            sim.set_controls(
                route::ME,
                Controls {
                    walk: direction,
                    ..Default::default()
                },
            );
            sim.tick();
        }
        println!("{} walking retreat {retreat:?} s (limit 6.0)", chamber.name);
        assert!(
            retreat.is_some(),
            "{} has no walking retreat via {:?} within 6.0 s",
            chamber.name,
            door
        );
    }
}

const FULL_KIT: Kit = Kit {
    eggs: 6,
    scissors: true,
    glass: true,
    crusher: true,
    dactyls: true,
};

// Keep an actual occupied position per 50-unit cell, never a snapped point
// inside rock. Include all resting nodes as well as intermediate trajectories.
/// `far`: survey to the octopus roof (slow: the ignored canyon test). Otherwise
/// stop at the basin's far edge, which keeps the always-run test quick on CI.
fn water_samples(far: bool) -> &'static Vec<Vec2> {
    static NEAR: std::sync::OnceLock<Vec<Vec2>> = std::sync::OnceLock::new();
    static FAR: std::sync::OnceLock<Vec<Vec2>> = std::sync::OnceLock::new();
    let limit = if far { 8800.0 } else { 3400.0 };
    (if far { &FAR } else { &NEAR }).get_or_init(|| {
        let mut cells = std::collections::BTreeMap::new();
        let s = route::survey_observed(
            route::start_node(FULL_KIT, route::HOME, false),
            FULL_KIT,
            // The water landmarks end before the octopus roof. Do not expand
            // the separate eastern reef beyond that roof's far edge.
            |n| n.mover().pos.x > limit,
            route::BUDGET,
            &[],
            &mut |p| {
                cells
                    .entry(((p.x / 50.0).floor() as i32, (p.y / 50.0).floor() as i32))
                    .or_insert(p);
            },
        );
        let mut points: Vec<_> = cells.into_values().collect();
        points.extend(s.nodes.iter().map(|n| n.mover().pos));
        points
    })
}

fn refuge_visible(level: &Level, feet: Vec2, rect: Rect) -> bool {
    let eye = feet + Vec2::Y * Tuning::default().body_height;
    // A rectangle can be partly occluded: also try its interior and corners,
    // rather than rejecting a shelter because its nearest edge touches rock.
    let inner = Rect {
        min: rect.min + Vec2::ONE,
        max: rect.max - Vec2::ONE,
    };
    [
        eye.clamp(inner.min, inner.max),
        inner.center(),
        inner.min,
        inner.max,
        Vec2::new(inner.min.x, inner.max.y),
        Vec2::new(inner.max.x, inner.min.y),
    ]
    .into_iter()
    .any(|target| visible(level, eye, target))
}

#[test]
fn open_water_always_has_a_visible_refuge_within_600() {
    open_water_refuges(false);
}

/// Missing shelter, not permission to raise the 600-unit limit:
/// https://github.com/StevenKauwe/crabscape/issues/459.
/// Keep these named regions executable and strict as W1.3 adds rock/refuges.
#[test]
#[ignore = "needs rock: W1.3 #459"]
fn canyon_far_basin_and_pit_have_visible_refuges_within_600() {
    open_water_refuges(true);
}

fn open_water_refuges(needs_rock: bool) {
    let level = Level::test_level();
    let landmarks = level.landmarks();
    let mut failures = Vec::new();
    for water in landmarks.iter().filter(|l| l.kind == Kind::OpenWater) {
        let blocked = matches!(
            water.name.as_str(),
            "canyon doorstep crossing"
                | "canyon shelf crossing"
                | "canyon deep water"
                | "far basin"
                | "pit open water"
        );
        if blocked != needs_rock {
            continue;
        }

        let mut worst = (0.0_f32, Vec2::ZERO);
        let mut count = 0;
        for &p in water_samples(needs_rock)
            .iter()
            .filter(|p| water.rect.contains(**p))
        {
            count += 1;
            let mut refuges: Vec<_> = landmarks
                .iter()
                .filter(|l| matches!(l.kind, Kind::Refuge | Kind::Mouth))
                .map(|l| (p.distance(p.clamp(l.rect.min, l.rect.max)), l.rect))
                .collect();
            refuges.sort_by(|a, b| a.0.total_cmp(&b.0));
            let distance = refuges
                .into_iter()
                .find(|(_, rect)| refuge_visible(&level, p, *rect))
                .map_or(f32::INFINITY, |(distance, _)| distance);
            if distance > worst.0 {
                worst = (distance, p);
            }
        }
        println!(
            "{}: {count} occupied samples, worst {:.1} at {:?}",
            water.name, worst.0, worst.1
        );
        assert!(
            count > 0,
            "{} was not sampled by the full-kit solver",
            water.name
        );
        if worst.0 > 600.0 {
            failures.push((water.name.clone(), worst));
        }
    }
    assert!(failures.is_empty(), "missing visible refuge: {failures:?}");
}

#[test]
fn every_pocket_has_two_exits() {
    let level = Level::test_level();
    let landmarks = level.landmarks();
    for pocket in landmarks
        .iter()
        .filter(|l| matches!(l.kind, Kind::Tunnel | Kind::Chamber { .. }))
    {
        let exits: Vec<_> = landmarks
            .iter()
            .filter(|l| {
                matches!(l.kind, Kind::Mouth | Kind::OpenWater | Kind::Refuge)
                    && l.rect.min.x <= pocket.rect.max.x + 40.0
                    && l.rect.max.x >= pocket.rect.min.x - 40.0
                    && l.rect.min.y <= pocket.rect.max.y + 40.0
                    && l.rect.max.y >= pocket.rect.min.y - 40.0
            })
            .map(|l| l.name.as_str())
            .collect();
        let required = if pocket.kind == Kind::Tunnel { 2 } else { 1 };
        println!(
            "{}: {} exits {exits:?}, requires {required}",
            pocket.name,
            exits.len()
        );
        assert!(exits.len() >= required, "{} has too few exits", pocket.name);
    }
}

/// Exhaust every subset of the four permanent abilities and 0..=6 eggs.
/// One survey serves all gates whose ability is missing. Digging attacks on
/// the frames are separately covered by dig_gates.rs; hub.rs covers the
/// one-sided window and dive_budget.rs covers the early stroke barriers.
#[test]
#[ignore = "exhaustive 98-kit route matrix exceeds 180 s; run explicitly with --ignored --nocapture"]
fn no_kit_skips_a_gate() {
    let level = Level::test_level();
    let landmarks = level.landmarks();
    let gates: Vec<_> = landmarks
        .iter()
        .filter_map(|l| {
            let Kind::Gate { needs } = l.kind else {
                return None;
            };
            let beyond = match l.name.as_str() {
                // Passage beyond the lower net; hub beyond the ridge window.
                "ghost net 0" => Rect::new(4357.0, -4380.0, 4490.0, -4230.0),
                "ghost net 1" => Rect::new(4490.0, -700.0, 5370.0, 560.0),
                // Sealed culture inside the brittle lump, not the walk around it.
                // Standing on its top is legal; the protected prize is inside.
                "brittle rock 0" | "brittle rock 1" => Rect {
                    min: l.rect.min + Vec2::ONE,
                    max: l.rect.max - Vec2::ONE,
                },
                // Near-wall shelf, notch, and ridge crest, respectively.
                "bare-rock band 6" => Rect::new(5900.0, -3400.0, 6150.0, -3300.0),
                "bare-rock band 7" => Rect::new(5500.0, 540.0, 5700.0, 800.0),
                "bare-rock band 23" => Rect::new(4300.0, 2990.0, 4370.0, 3200.0),
                name => panic!("define the far side for new gate {name} ({needs:?})"),
            };
            Some((l.name.as_str(), needs, beyond))
        })
        .collect();
    // Strongest kits first so an accidental bypass is reported early.
    for eggs in (0..=6).rev() {
        for bits in (0..16).rev() {
            let kit = Kit {
                eggs,
                scissors: bits & 1 != 0,
                glass: bits & 2 != 0,
                crusher: bits & 4 != 0,
                dactyls: bits & 8 != 0,
            };
            let lacks = |need| match need {
                Needs::Scissors => !kit.scissors,
                Needs::GlassGrips => !kit.glass,
                Needs::Crusher => !kit.crusher,
                Needs::Strokes(n) => kit.eggs < n,
            };
            let checked: Vec<_> = gates.iter().filter(|(_, need, _)| lacks(*need)).collect();
            if checked.is_empty() {
                continue;
            }
            let s = route::survey(kit);
            println!(
                "gate search {}: {} resting places",
                kit.name(),
                s.nodes.len()
            );
            for (name, _, beyond) in checked {
                for (i, node) in s.nodes.iter().enumerate() {
                    assert!(
                        !beyond.contains(node.mover().pos),
                        "{} skips {name} at {:?}: {}",
                        kit.name(),
                        node.mover().pos,
                        s.describe(i)
                    );
                }
            }
        }
    }
}

#[test]
fn basin_ledge_refuges_can_hold_a_crab_still() {
    let level = Level::test_level();
    let landmarks = level.landmarks();
    for i in 0..4 {
        let name = format!("basin return ledge {i}");
        let ledge = landmarks
            .iter()
            .find(|l| l.name == name)
            .expect("existing basin ledge tagged as refuge");
        assert_eq!(ledge.kind, Kind::Refuge);
        let half = Tuning::default().body_half_width;
        for x in [
            ledge.rect.min.x + half,
            ledge.rect.center().x,
            ledge.rect.max.x - half,
        ] {
            let at = Vec2::new(x, ledge.rect.min.y);
            let mut sim = route::world(Kit::default(), at, false);
            for _ in 0..240 {
                sim.tick();
            }
            let m = &sim.crab(route::ME).expect("resting crab").mover;
            assert!(
                m.grounded && m.pos.distance(at) < 1.0,
                "{name} cannot hold at {at}: {}",
                m.pos
            );
        }
    }
}
