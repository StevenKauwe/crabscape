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
