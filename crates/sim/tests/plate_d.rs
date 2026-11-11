//! The Plate D geometry batch (decisions.md, 2026-11-10): the world steps
//! down to the right, with sediment pockets in the banks under the steps.
//! Seed-free checks here; the ones that walk the baked cells are marked.
use bevy_math::{Rect, Vec2};
use crabsim::Level;
use crabsim::level::landmarks::Kind;

#[test]
fn the_world_steps_down_to_the_right() {
    let level = Level::test_level();
    let steps = [
        ("home shelf", 1000.0),
        ("basin", 3500.0),
        ("doorstep shelf", 5700.0),
        ("far rim", 8300.0),
        ("eastern step", 16500.0),
        ("lower canyon", 19000.0),
    ];
    let mut last = f32::MAX;
    for (name, x) in steps {
        let y = level.floor_height(x);
        println!("{name}: floor {y} at x {x}");
        assert!(
            y < last,
            "{name} ({y}) is not below the step before ({last})"
        );
        last = y;
    }
    // The basin pit is filled flat, the canyon floor sits above the lower canyon.
    for x in (2801..=5200).step_by(25) {
        assert_eq!(level.floor_height(x as f32), -700.0, "basin floor at {x}");
    }
    let canyon = level.floor_height(7000.0);
    assert_eq!(canyon, -6000.0);
    assert!(canyon > level.floor_height(19000.0));
    // The home segment is exactly as it was.
    assert_eq!(level.floor_height(1450.0), -260.0);
    assert_eq!(level.floor_height(2750.0), 860.0);
}

fn place(level: &Level, name: &str) -> (Rect, Kind) {
    let landmarks = level.landmarks();
    let l = landmarks
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no landmark {name}"));
    (l.rect, l.kind)
}

#[test]
fn every_plate_d_pocket_exists_by_name_with_its_kind() {
    let level = Level::test_level();
    let tunnels = [
        "home burrow",
        "mantis mouth",
        "clay tunnel",
        "seep shaft",
        "pebble warren",
        "cathedral seep",
        "long chimney",
        "chimney return",
        "chimney hall",
        "chimney mouth",
        "crust vault",
    ];
    let chambers = ["bone hall", "clay cathedral", "seep garden", "warm seam"];
    let frames = ["odd tunnel", "Plunge shaft", "Plunge crack", "angler stage"];
    for name in tunnels {
        assert_eq!(place(&level, name).1, Kind::Tunnel, "{name}");
    }
    for name in chambers {
        assert!(
            matches!(place(&level, name).1, Kind::Chamber { .. }),
            "{name}"
        );
    }
    for name in frames {
        assert_eq!(place(&level, name).1, Kind::Frame, "{name}");
    }
    let pockets = level.pockets();
    for name in tunnels.iter().chain(&chambers) {
        assert!(pockets.iter().any(|p| p.name == *name), "pocket {name}");
    }
    // Every pocket has a Mouth landmark per door, and the long chimney's top
    // is a brittle crust plug.
    for pocket in &pockets {
        let mouths = level
            .landmarks()
            .iter()
            .filter(|l| l.kind == Kind::Mouth && l.name.starts_with(pocket.name))
            .count();
        assert_eq!(mouths, pocket.mouths.len(), "{}", pocket.name);
    }
    let (plug, _) = place(&level, "long chimney crust plug");
    assert!(level.brittle.iter().any(|b| b.rect == plug));
    assert!(place(&level, "hanging garden").1 == Kind::Refuge);
    // landmarks() never panics on a level that lacks authored parts.
    let mut bare = Level::test_level();
    bare.foes.clear();
    bare.eggs.clear();
    bare.currents.clear();
    bare.secrets.clear();
    assert!(!bare.landmarks().is_empty());
}

fn open_at(level: &Level, p: Vec2) -> bool {
    level
        .strata()
        .iter()
        .any(|b| b.cell_at(p.x, p.y) == Some(crabsim::underwater::Cell::Water))
}

#[test]
fn egg_four_lies_in_the_bone_hall_in_sight_of_its_door() {
    let level = Level::test_level();
    let (hall, kind) = place(&level, "bone hall");
    let Kind::Chamber { door, prize } = kind else {
        panic!("the bone hall is a chamber");
    };
    assert_eq!(prize, level.eggs[3], "egg 4 is the hall's prize");
    assert!(hall.contains(prize));
    assert!(
        level
            .teeth
            .iter()
            .chain(&level.clues)
            .filter(|p| p.x < 5000.0)
            .all(|p| hall.contains(*p) && p.distance(prize) < 200.0)
    );
    let distance = door.center().distance(prize);
    println!("bone hall door to prize {distance:.1}");
    assert!(distance <= 800.0);
    // The straight line from the door to the prize runs through open water.
    let steps = (distance / 4.0) as usize;
    for i in 0..=steps {
        let p = door.center().lerp(prize, i as f32 / steps as f32);
        assert!(open_at(&level, p), "the hall is blocked at {p}");
    }
    // The mantis lies buried beside the mantis mouth's door, never inside.
    let (_, mantis) = level
        .foes
        .iter()
        .find(|(k, _)| *k == crabsim::enemy::Kind::Mantis)
        .copied()
        .expect("the basin mantis");
    let tuning = crabsim::Tuning::default();
    let e = crabsim::enemy::Enemy::new(0, crabsim::enemy::Kind::Mantis, mantis, &tuning);
    assert!(
        e.mouth_tell(&level, mantis + Vec2::X * 100.0, &tuning)
            .is_some(),
        "mouth_tell finds the buried mantis"
    );
}

fn water_fraction(level: &Level, r: Rect) -> f32 {
    use crabsim::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};
    let (mut water, mut all) = (0, 0);
    let mut y = r.min.y + 2.0;
    while y < r.max.y {
        let mut x = r.min.x + 2.0;
        while x < r.max.x {
            let c = initial_terrain_cell(
                level,
                (x / GAME_CELL).floor() as i32,
                ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32,
            );
            all += 1;
            water += usize::from(c == Cell::Water);
            x += GAME_CELL;
        }
        y += GAME_CELL;
    }
    water as f32 / all as f32
}

#[test]
fn baked_cavities_stay_open_water() {
    let level = Level::test_level();
    for body in level.strata().iter().filter(|b| b.frame.min.y > -10400.0) {
        for void in &body.voids {
            let f = water_fraction(&level, *void);
            println!("{:?}: {:.0}% water", void, f * 100.0);
        }
    }
    // The walking corridor between the doors (the water shaft's foot is
    // where the floor's sand lands).
    let gallery = Rect::new(240.0, -532.0, 540.0, -460.0);
    assert!(water_fraction(&level, gallery) >= 0.8, "gallery filled in");
}

use crabsim::{Controls, PlayerId, Sim, Tuning};

fn dig_sim(at: Vec2) -> Sim {
    let mut level = Level::test_level();
    level.foes.clear();
    let mut sim = Sim::new(Tuning::default(), level);
    sim.add_crab(PlayerId(0), at);
    sim.enable_underwater_cells();
    sim.crab_mut(PlayerId(0)).unwrap().mover.pos = at;
    let mut cells = sim.level.cells.take().expect("cells");
    cells.follow_player(&sim.level, at.x, at.y);
    {
        use crabsim::underwater::Cell;
        let mut col = String::new();
        let mut y = 40.0;
        while y > -480.0 {
            let (cx, cy) = cells.local_cell(at.x, y);
            col.push(match cells.cell(cx as i32, cy as i32) {
                Cell::Water => '.',
                Cell::Rock => '#',
                Cell::Sand => 's',
                Cell::Mud => 'm',
            });
            y -= 10.0;
        }
        eprintln!("window column: {col}");
    }
    sim.level.cells = Some(cells);
    sim
}

/// What the game does every frame: the cell window follows the crab.
fn follow(sim: &mut Sim) {
    let p = sim.crab(PlayerId(0)).unwrap().mover.pos;
    let mut cells = sim.level.cells.take().expect("cells");
    cells.follow_player(&sim.level, p.x, p.y);
    sim.level.cells = Some(cells);
}

#[test]
fn home_burrow_has_two_exits_and_no_hunters() {
    let level = Level::test_level();
    let pocket = level
        .pockets()
        .into_iter()
        .find(|p| p.name == "home burrow")
        .expect("home burrow");
    assert!(pocket.mouths.len() >= 2);
    for mouth in &pocket.mouths {
        // Each door is a silt shaft's top on the ground.
        assert!(mouth.max.y >= level.floor_height(mouth.center().x) - 30.0);
        assert!(level.strata().iter().any(|b| {
            b.shafts
                .iter()
                .chain(&b.voids)
                .any(|s| s.contains(mouth.center() - Vec2::Y * 30.0))
        }));
    }
    let tuning = Tuning::default();
    for interior in level.tunnel_interiors().filter(|r| r.max.x < 700.0) {
        for &(kind, home) in &level.foes {
            assert!(!interior.contains(home), "{kind:?} lives in {interior:?}");
            let e = crabsim::enemy::Enemy::new(0, kind, home, &tuning);
            assert!(!e.can_reach_tunnel_point(&level, interior.center(), &tuning));
        }
    }
}

#[test]
fn first_dig_finds_a_clam_within_10s() {
    let level = Level::test_level();
    let clam = level
        .clams
        .iter()
        .copied()
        .find(|c| (c.x - 172.0).abs() < 30.0 && c.y > -100.0 && c.y < 0.0)
        .expect("first-dig clam by the home shell");
    let mut sim = dig_sim(Vec2::new(clam.x, 40.0));
    let mut got = None;
    for tick in 0..120 * 10 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            },
        );
        sim.tick();
        if !sim.crab(PlayerId(0)).unwrap().mover.clinging.is_empty() {
            got = Some(tick);
            break;
        }
    }
    let tick = got.expect("no clam within 10 s");
    println!("first-dig clam after {:.1} s", tick as f32 / 120.0);
}

/// In one door, along the gallery, out the other: under 25 s, under two
/// reserve strokes.
#[test]
fn home_burrow_round_trip_is_cheap() {
    let mut sim = dig_sim(Vec2::new(172.0, 40.0));
    let mut min_reserve = 1.0_f32;
    let mut phase = 0;
    for tick in 0..120 * 40 {
        let p = sim.crab(PlayerId(0)).unwrap().mover.pos;
        if tick % 240 == 0 {
            eprintln!(
                "t{tick} {p:?} in {} vel {:?} load {}",
                sim.level.cells.as_ref().unwrap().contains_world(p.x, p.y),
                sim.crab(PlayerId(0)).unwrap().mover.vel,
                sim.crab(PlayerId(0)).unwrap().mover.dig_load
            );
        }
        min_reserve = min_reserve.min(sim.crab(PlayerId(0)).unwrap().mover.reserve);
        let controls = match phase {
            0 if p.y < -520.0 => {
                phase = 1;
                Controls::default()
            }
            0 => Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            },
            1 if p.x > 586.0 => {
                phase = 2;
                Controls::default()
            }
            1 => Controls {
                walk: 1.0,
                dig_held: true,
                ..Default::default()
            },
            _ if p.y >= -4.0 => {
                println!(
                    "round trip {:.1} s, lowest reserve {min_reserve:.2}",
                    tick as f32 / 120.0
                );
                return;
            }
            _ => Controls {
                jump_held: true,
                jump_pressed: tick % 40 == 0,
                ..Default::default()
            },
        };
        sim.set_controls(PlayerId(0), controls);
        follow(&mut sim);
        sim.tick();
    }
    panic!(
        "no round trip in 25 s: phase {phase}, ended at {:?}",
        sim.crab(PlayerId(0)).unwrap().mover.pos
    );
}

#[test]
#[ignore]
fn dump_col() {
    use crabsim::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};
    let mut level = Level::test_level();
    level.max_x += 1.0;
    for x in [480.0, 520.0, 560.0, 600.0, 630.0] {
        let mut row = String::new();
        let mut y = 40.0;
        while y > -480.0 {
            let c = initial_terrain_cell(
                &level,
                (x / GAME_CELL).floor() as i32,
                ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32,
            );
            row.push(match c {
                Cell::Water => '.',
                Cell::Rock => '#',
                Cell::Sand => 's',
                Cell::Mud => 'm',
            });
            y -= 10.0;
        }
        println!("{x}: {row}");
    }
}

#[test]
#[ignore]
fn dump_rocks() {
    let level = Level::test_level();
    for r in level
        .rocks
        .iter()
        .filter(|r| r.min.x < 400.0 && r.max.x > 100.0 && r.max.y > -300.0)
    {
        println!("ROCK {r:?}");
    }
}
