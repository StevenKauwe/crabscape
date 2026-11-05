//! The single rock batch: gates hold before future strata and rides open them.
mod common;
use bevy_math::{Rect, Vec2};
use common::route::{self, Kit};
use crabsim::level::landmarks::Kind;
use crabsim::{Controls, Level};

const FULL: Kit = Kit {
    eggs: 6,
    scissors: true,
    glass: true,
    crusher: true,
    dactyls: true,
};
const FRAMES: &[&str] = &[
    "home burrow",
    "mantis mouth",
    "clay tunnel",
    "bone hall",
    "seep shaft",
    "seep outlet",
    "long chimney",
    "chimney return",
    "chimney hall",
    "chimney mouth",
    "odd tunnel",
    "Plunge shaft",
    "Plunge crack",
    "angler stage",
];

fn place(name: &str) -> Rect {
    Level::test_level()
        .landmarks()
        .iter()
        .find(|l| l.name == name)
        .expect(name)
        .rect
}

#[test]
fn existing_floor_heights_are_unchanged_west_of_17400() {
    let level = Level::test_level();
    let mut digest = 0xcbf29ce484222325_u64;
    // Every unit, including both sides of the old vertical edges.
    for x in 0..=17600 {
        for byte in level.floor_height(x as f32).to_bits().to_le_bytes() {
            digest = (digest ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    assert_eq!(
        digest, 0x888447f7e3f4962c,
        "baseline floor digest: {digest:#x}"
    );
}

#[test]
fn slump_dam_is_one_way_without_a_tether() {
    let dam = place("slump dam");
    let below = Vec2::new(
        dam.max.x + 60.0,
        Level::test_level().floor_height(dam.max.x + 60.0),
    );
    let survey = route::survey_observed(
        route::start_node(FULL, below, false),
        FULL,
        |n| (n.mover().pos.x - below.x).abs() > 250.0,
        150,
        &[],
        &mut |p| assert!(!(p.x < dam.max.x && p.y >= dam.max.y), "climbed back: {p}"),
    );
    assert!(survey.nodes.len() > 1);
    let mut sim = route::world(
        Kit::default(),
        Vec2::new(dam.max.x - 60.0, dam.max.y),
        false,
    );
    let health = sim.crab(route::ME).expect("crab").fighter.vitals.health;
    for _ in 0..120 * 30 {
        sim.set_controls(
            route::ME,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
    }
    let crab = sim.crab(route::ME).expect("crab");
    assert_eq!(crab.fighter.vitals.health, health, "falls do no damage");
    assert!(
        crab.mover.pos.x > dam.max.x && crab.mover.pos.y < dam.max.y - 1500.0,
        "landed at {:?}",
        crab.mover.pos
    );
}

#[test]
fn narrows_need_the_heavy_crusher() {
    let gate = place("narrows lock");
    for eggs in 0..=6 {
        for bits in 0..16 {
            let kit = Kit {
                eggs,
                scissors: bits & 1 != 0,
                glass: bits & 2 != 0,
                crusher: bits & 4 != 0,
                dactyls: bits & 8 != 0,
            };
            let at = Vec2::new(gate.min.x - 65.0, gate.min.y);
            route::survey_observed(
                route::start_node(kit, at, false),
                kit,
                |n| n.mover().pos.x < at.x - 100.0,
                100,
                &[],
                &mut |p| assert!(p.x < gate.max.x, "bypass {} at {p}", kit.name()),
            );
            let mut sim = route::world(kit, at, false);
            for tick in 0..480 {
                sim.set_controls(
                    route::ME,
                    Controls {
                        walk: 1.0,
                        smash_pressed: tick == 0,
                        smash_held: tick < 120,
                        ..Default::default()
                    },
                );
                sim.tick();
            }
            let rock = sim
                .level
                .brittle
                .iter()
                .find(|b| b.rect == gate)
                .expect("lock");
            assert_eq!(rock.cracked, kit.crusher, "{}", kit.name());
            assert_eq!(
                sim.crab(route::ME).expect("crab").mover.pos.x > gate.max.x + 60.0,
                kit.crusher,
                "{}",
                kit.name()
            );
        }
    }
}

#[test]
fn digging_cannot_bypass_the_narrows() {
    let gate = place("narrows lock");
    for (x, walk) in [(gate.min.x - 65.0, 1.0), (gate.max.x + 65.0, -1.0)] {
        for (down, up) in [(false, false), (true, false), (false, true)] {
            let mut sim = route::world(FULL, Vec2::new(x, gate.min.y), false);
            // Raw-substrate check while the old canonical seed awaits its bake.
            sim.level.max_x += 1.0;
            sim.enable_underwater_cells();
            for tick in 0..120 * 20 {
                sim.set_controls(
                    route::ME,
                    Controls {
                        walk,
                        down: down || tick < 360,
                        dig_held: true,
                        dig_up: up,
                        ..Default::default()
                    },
                );
                sim.tick();
                let p = sim.crab(route::ME).expect("crab").mover.pos;
                assert!(
                    if walk > 0.0 {
                        p.x < gate.max.x
                    } else {
                        p.x > gate.min.x
                    },
                    "dug through at {p}"
                );
            }
            assert!(
                !sim.level
                    .brittle
                    .iter()
                    .find(|b| b.rect == gate)
                    .expect("lock")
                    .cracked
            );
        }
    }
}

#[test]
fn frames_are_sealed() {
    let level = Level::test_level();
    let landmarks = level.landmarks();
    assert_eq!(
        landmarks.iter().filter(|l| l.kind == Kind::Frame).count(),
        FRAMES.len()
    );
    for &name in FRAMES {
        let r = place(name);
        assert_eq!(
            landmarks.iter().find(|l| l.name == name).expect(name).kind,
            Kind::Frame
        );
        assert!(
            !level.rocks.iter().any(|rock| rock.contains(r.center())),
            "authored rock fills {name}"
        );
        assert!(
            landmarks
                .iter()
                .filter(|l| l.kind == Kind::Plug && l.name.starts_with(name))
                .count()
                >= 2,
            "{name} needs two reserved exits"
        );
        // A continuous rock boundary, including corners, rules out every
        // approach. The crack's 12-unit viewing slit cannot fit a crab.
        for i in 0..=200 {
            let t = i as f32 / 200.0;
            for p in [
                Vec2::new(r.min.x, r.min.y + r.height() * t),
                Vec2::new(r.max.x, r.min.y + r.height() * t),
                Vec2::new(r.min.x + r.width() * t, r.min.y),
                Vec2::new(r.min.x + r.width() * t, r.max.y),
            ] {
                let slit = ((name == "Plunge crack" && p.x == r.max.x)
                    || (name == "angler stage" && p.x == r.min.x))
                    && (p.y + 14025.0).abs() < 6.0;
                assert!(
                    // Permanent rock also seals the chimney after its brittle
                    // crust has cracked: no early crusher access to a frame.
                    slit || level.rocks.iter().any(|rock| rock.contains(p)),
                    "{name} open at {p}"
                );
            }
        }
        // Start on reachable ground directly above each reserved body. Observe
        // trajectories too, not only resting nodes; terminate after local moves.
        // The bone hall's midpoint is below an old basin pillar. Pick clear
        // ground beside that pillar rather than spawning inside solid rock.
        let at = [r.center().x, r.center().x - 100.0, r.center().x + 100.0]
            .into_iter()
            .map(|x| Vec2::new(x, level.floor_height(x)))
            .find(|p| {
                let body = Rect::new(p.x - 40.0, p.y + 1.0, p.x + 40.0, p.y + 60.0);
                level.solids().all(|rock| rock.intersect(body).is_empty())
            })
            .expect("clear reachable ground above each frame");
        let inner = Rect {
            min: r.min + Vec2::splat(1.0),
            max: r.max - Vec2::splat(1.0),
        };
        route::survey_observed(
            route::start_node(FULL, at, false),
            FULL,
            |n| n.time > 0.0,
            200,
            &[],
            &mut |p| assert!(!inner.contains(p), "route entered {name}"),
        );
        // Same real-cell dig sequence as dig_gates: down first, then each
        // digging direction. Bedrock must survive, including with full kit.
        for (walk, down, up) in [
            (1.0, false, false),
            (-1.0, false, false),
            (0.0, true, false),
            (0.0, false, true),
        ] {
            let mut sim = route::world(FULL, at, false);
            // Exercise the raw substrate as well as the geometric route solver.
            // Changing only the distant boundary opts out of the committed seed.
            sim.level.max_x += 1.0;
            sim.enable_underwater_cells();
            for tick in 0..600 {
                sim.set_controls(
                    route::ME,
                    Controls {
                        walk,
                        down: down || tick < 360,
                        dig_held: true,
                        dig_up: up,
                        ..Default::default()
                    },
                );
                sim.tick();
                assert!(
                    !inner.contains(sim.crab(route::ME).expect("crab").mover.pos),
                    "dig entered {name}"
                );
            }
        }
    }
}

#[test]
fn plunge_crack_has_only_a_narrow_view_into_the_stage() {
    let level = Level::test_level();
    let crack = place("Plunge crack");
    let stage = place("angler stage");
    assert_eq!(crack.size(), Vec2::new(300.0, 150.0));
    assert_eq!(stage.min.x - crack.max.x, 100.0);
    assert!(crabsim::Tuning::default().body_height > 12.0);
    for x in (crack.max.x as i32)..=(stage.min.x as i32) {
        assert!(
            !level
                .rocks
                .iter()
                .any(|rock| rock.contains(Vec2::new(x as f32, -14025.0)))
        );
        for y in [-14032.0, -14018.0] {
            assert!(
                level
                    .rocks
                    .iter()
                    .any(|rock| rock.contains(Vec2::new(x as f32, y)))
            );
        }
    }
}

#[test]
fn vent_field_is_wide_deep_and_open() {
    let level = Level::test_level();
    let r = place("vent field");
    assert!(r.min.x > 17600.0 && r.width() >= 2500.0);
    assert_eq!(level.band_at(r.center()).name, "Deep");
    for i in 0..=100 {
        let x = r.min.x + r.width() * i as f32 / 100.0;
        assert_eq!(level.floor_height(x), r.min.y);
        assert!(
            !level
                .rocks
                .iter()
                .any(|rock| rock.contains(Vec2::new(x, r.max.y - 1.0)))
        );
    }
    let coral: Vec<_> = level.climbs.iter().filter(|c| c.x >= r.min.x).collect();
    assert!(coral.len() >= 3);
    assert!(coral.iter().all(|c| c.top - c.bottom <= 200.0));
    assert!(coral.windows(2).all(|w| w[1].x - w[0].x > 1020.0));
    println!("vent width {}, open-water height {}", r.width(), r.height());
}
