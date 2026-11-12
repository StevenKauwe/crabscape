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
fn existing_floor_heights_are_unchanged_west_of_2800() {
    let level = Level::test_level();
    let mut digest = 0xcbf29ce484222325_u64;
    // Every unit, including both sides of the old vertical edges.
    for x in 0..=2800 {
        // The sea floor's line (the Silt Galleries' mouth is a hole in what
        // `floor_height` reports, and nowhere in the line).
        let height = crabsim::level::layers::polyline_at(&level.floor, x as f32);
        for byte in height.to_bits().to_le_bytes() {
            digest = (digest ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    assert_eq!(
        digest, 0x05046833a83dc0c6,
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
    // Only the Plunge's four frames stay sealed (the rest are open pockets).
    assert_eq!(
        landmarks
            .iter()
            .filter(|l| l.kind == Kind::Frame && FRAMES.contains(&l.name.as_str()))
            .count(),
        4
    );
    for &name in FRAMES {
        if !matches!(
            name,
            "odd tunnel" | "Plunge shaft" | "Plunge crack" | "angler stage"
        ) {
            continue;
        }
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
                .filter(|l| matches!(l.kind, Kind::Plug | Kind::Mouth) && l.name.starts_with(name))
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
                // The odd tunnel's dead end opens onto the chimney hall.
                let doorway = landmarks.iter().any(|l| {
                    l.kind == Kind::Mouth && l.name.starts_with(name) && l.rect.contains(p)
                });
                assert!(
                    // Permanent rock also seals the chimney after its brittle
                    // crust has cracked: no early crusher access to a frame.
                    slit || doorway || level.rocks.iter().any(|rock| rock.contains(p)),
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

#[test]
fn tether_starts_after_the_first_visit_and_is_shared() {
    use crabsim::{PlayerId, net::FakeLink};
    let mut host = route::world(FULL, Vec2::new(17860.0, -10000.0), false);
    let mut guest = crabsim::Sim::new(host.tuning.clone(), host.level.clone());
    host.add_remote_crab(PlayerId(1), Vec2::new(18380.0, -10000.0));
    guest.add_remote_crab(route::ME, Vec2::new(17860.0, -10000.0));
    guest.add_crab(PlayerId(1), Vec2::new(18380.0, -10000.0));
    host.go_online(true);
    guest.go_online(false);
    assert!(!host.level.tether_open());
    let marker = host
        .level
        .nets
        .iter()
        .find(|n| n.rect.is_empty())
        .expect("event marker")
        .rect
        .center();
    assert!(
        host.level
            .rocks
            .iter()
            .any(|r| r.contains(marker - Vec2::new(50.0, 70.0))
                && r.contains(marker + Vec2::new(50.0, 70.0))),
        "the switch cannot be reached with scissors through the sill"
    );
    host.tick();
    assert!(
        !host.level.tether_open(),
        "remote bodies do not open the tether"
    );
    let mut link = FakeLink::steady(0);
    for _ in 0..120 {
        guest.tick();
        host.tick();
        link.carry(&mut [&mut host, &mut guest]);
    }
    assert!(host.level.tether_open());
    assert!(guest.level.tether_open());
    let mut late = route::world(FULL, route::HOME, false);
    late.go_online(false);
    late.receive(host.world());
    assert!(late.level.tether_open());
    let floor = host.level.floor.clone();
    let rocks = host.level.rocks.clone();
    for (sim, id) in [(&mut host, route::ME), (&mut guest, PlayerId(1))] {
        let m = &mut sim.crab_mut(id).expect("local crab").mover;
        m.pos = Vec2::new(17840.0, -10000.0);
        m.vel = Vec2::ZERO;
        for tick in 0..120 * 5 {
            sim.set_controls(
                id,
                Controls {
                    jump_pressed: tick == 0,
                    ..Default::default()
                },
            );
            sim.tick();
        }
        assert!(
            sim.crab(id).expect("local crab").mover.pos.y > -8500.0,
            "both devices ride the opened current"
        );
        assert_eq!(sim.level.floor, floor);
        assert_eq!(sim.level.rocks, rocks);
        assert_eq!(sim.level.currents, late.level.currents);
    }
}

#[test]
fn deep_food_is_visible_and_only_adapts_when_eaten_at_home() {
    let mut sim = route::world(FULL, Vec2::new(18450.0, -10000.0), false);
    let before = sim.crab(route::ME).expect("crab").adapted_depth;
    let key = sim.level.deep_food().expect("vent food");
    assert!(
        Level::test_level()
            .landmarks()
            .iter()
            .any(|l| l.kind == Kind::Refuge && l.rect.contains(Vec2::new(18380.0, -9970.0)))
    );
    assert!(key.distance(Vec2::new(18380.0, -9970.0)) < 800.0);
    for i in 0..=100 {
        let p = Vec2::new(18380.0, -9970.0).lerp(key, i as f32 / 100.0);
        assert!(!sim.level.solids().any(|r| r.contains(p)));
    }
    sim.tick();
    assert_eq!(
        sim.crab(route::ME).expect("crab").mover.clinging,
        vec![crabsim::physics::Carry::Clam]
    );
    assert_eq!(sim.crab(route::ME).expect("crab").adapted_depth, before);
    sim.crab_mut(route::ME)
        .expect("crab")
        .mover
        .prepared_food
        .auto_eat = false;
    sim.go_home(route::ME);
    for _ in 0..sim.tuning.food.eat_after_ticks + 2 {
        sim.tick();
    }
    assert_eq!(
        sim.crab(route::ME).expect("crab").adapted_depth,
        before,
        "storing is not eating"
    );
    sim.crab_mut(route::ME)
        .expect("crab")
        .mover
        .prepared_food
        .auto_eat = true;
    sim.tick();
    assert_eq!(
        sim.crab(route::ME).expect("crab").adapted_depth,
        before - 1500.0
    );
    for _ in 0..120 {
        sim.tick();
    }
    assert_eq!(
        sim.crab(route::ME).expect("crab").adapted_depth,
        before - 1500.0
    );
}

#[test]
fn deep_food_survives_dropping_other_items_and_death() {
    use crabsim::{
        physics::Carry,
        world::{DeepFood, ShellAction},
    };
    let at = Vec2::new(18450.0, -10000.0);
    let mut sim = route::world(FULL, at, false);
    sim.crab_mut(route::ME)
        .expect("crab")
        .mover
        .clinging
        .push(Carry::Grit);
    sim.tick();
    assert_eq!(
        sim.crab(route::ME).expect("crab").deep_food,
        DeepFood::Carried(1)
    );
    assert!(sim.shell_action(route::ME, ShellAction::Drop(0)));
    assert_eq!(
        sim.crab(route::ME).expect("crab").deep_food,
        DeepFood::Carried(0)
    );
    assert!(sim.shell_action(route::ME, ShellAction::Drop(0)));
    sim.tick();
    assert!(sim.crab(route::ME).expect("crab").mover.clinging.is_empty());
    sim.crab_mut(route::ME).expect("crab").mover.pos.x += 300.0;
    sim.tick();
    sim.crab_mut(route::ME).expect("crab").mover.pos = at;
    sim.tick();
    assert_eq!(
        sim.crab(route::ME).expect("crab").deep_food,
        DeepFood::Carried(1)
    );
    sim.crab_mut(route::ME).expect("crab").fighter.dead = Some(sim.tuning.respawn_ticks);
    sim.tick();
    assert!(matches!(
        sim.crab(route::ME).expect("crab").deep_food,
        DeepFood::Dropped(_)
    ));
    assert!(sim.crab(route::ME).expect("crab").mover.clinging.is_empty());
    let crab = sim.crab_mut(route::ME).expect("crab");
    crab.mover.pos = at;
    crab.mover.shelter = crabsim::home::Shelter::Out;
    crab.mover.tucked = 0.0;
    sim.tick();
    assert_eq!(
        sim.crab(route::ME).expect("crab").deep_food,
        DeepFood::Carried(1)
    );
}

#[test]
fn guest_receives_deep_food_from_the_host_and_remote_crabs_do_not_eat_it() {
    use crabsim::{PlayerId, net::FakeLink, world::DeepFood};
    let guest_id = PlayerId(1);
    let at = Vec2::new(18450.0, -10000.0);
    let mut host = route::world(FULL, route::HOME, false);
    host.add_remote_crab(guest_id, at);
    let mut guest = crabsim::Sim::new(host.tuning.clone(), host.level.clone());
    guest.add_crab(guest_id, at);
    guest.add_remote_crab(route::ME, route::HOME);
    host.go_online(true);
    guest.go_online(false);
    let mut link = FakeLink::steady(3);
    for _ in 0..60 {
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
    }
    assert_eq!(
        guest.crab(guest_id).expect("guest").deep_food,
        DeepFood::Carried(0)
    );
    let bed = host
        .level
        .clams
        .iter()
        .position(|p| *p == host.level.deep_food().expect("food"))
        .expect("bed");
    assert!(host.shared.clams[bed] > 0 && guest.shared.clams[bed] > 0);
    let before = guest.crab(guest_id).expect("guest").adapted_depth;
    guest.go_home(guest_id);
    for _ in 0..guest.tuning.food.eat_after_ticks + 2 {
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
    }
    assert_eq!(
        guest.crab(guest_id).expect("guest").adapted_depth,
        before - 1500.0
    );
    assert_eq!(
        host.crab(guest_id).expect("remote").deep_food,
        DeepFood::Available
    );
    assert_eq!(host.crab(route::ME).expect("host").adapted_depth, before);
}
