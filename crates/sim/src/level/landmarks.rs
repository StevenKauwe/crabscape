//! Static authoring labels for world-shape checks. These never drive gameplay.
//! Broad water boxes describe crossings; refuge, tunnel and chamber boxes
//! describe their local sheltered spaces. Gates annotate traversal requirements.
use super::{Growth, Level};
use crate::enemy;
use bevy_math::{Rect, Vec2};

/// A named place in the authored level, in world coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Landmark {
    pub name: String,
    pub rect: Rect,
    pub kind: Kind,
}

/// Fear register or a door/gate used to measure routes between registers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Refuge,
    OpenWater,
    Tunnel,
    Mouth,
    Chamber { door: Rect, prize: Vec2 },
    Gate { needs: Needs },
}

/// Labels for the existing passage checks, spanning crafted kit and feats.
/// Scissors = scissor claws, Crusher = HeavyCrusher, GlassGrips = glass grips.
/// Strokes counts permanent swim upgrades. These labels grant no abilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Needs {
    Scissors,
    Crusher,
    GlassGrips,
    Strokes(u32),
}

/// Build only after all existing level extensions have authored their data.
pub(super) fn existing_level(level: &Level) -> Vec<Landmark> {
    let mut landmarks = Vec::new();
    let mut add = |name: String, rect, kind| landmarks.push(Landmark { name, rect, kind });
    add("home".into(), level.home, Kind::Refuge);
    for (i, crevice) in level.crevices.iter().enumerate() {
        add(format!("crevice {i}"), crevice.door, Kind::Refuge);
    }
    for (i, climb) in level.climbs.iter().enumerate() {
        let (name, kind) = match climb.kind {
            Growth::Kelp => ("kelp stand", Kind::Refuge),
            Growth::Coral => ("coral stand", Kind::Refuge),
            Growth::Rock => (
                "bare-rock band",
                Kind::Gate {
                    needs: Needs::GlassGrips,
                },
            ),
        };
        add(
            format!("{name} {i}"),
            Rect::new(climb.x - 60.0, climb.bottom, climb.x + 60.0, climb.top),
            kind,
        );
    }
    for (i, net) in level.nets.iter().enumerate() {
        add(
            format!("ghost net {i}"),
            net.rect,
            Kind::Gate {
                needs: Needs::Scissors,
            },
        );
    }
    for (i, brittle) in level.brittle.iter().enumerate() {
        add(
            format!("brittle rock {i}"),
            brittle.rect,
            Kind::Gate {
                needs: Needs::Crusher,
            },
        );
    }

    // The first tide column is the canyon. Widen it to each wall, stepping
    // inward below the shelf and far rim so these boxes stay out of bedrock.
    let tide = level.currents[0];
    for (name, rect) in [
        (
            "canyon doorstep crossing",
            Rect::new(5901.0, -1900.0, level.den.min.x, tide.max.y),
        ),
        (
            "canyon shelf crossing",
            Rect::new(5901.0, -3400.0, 7700.0, -1900.0),
        ),
        (
            "canyon deep water",
            Rect::new(6151.0, tide.min.y, 7700.0, -3400.0),
        ),
        ("basin", Rect::new(2801.0, -700.0, 3300.0, 860.0)),
        ("far basin", Rect::new(4490.0, -700.0, 5370.0, 560.0)),
    ] {
        add(name.into(), rect, Kind::OpenWater);
    }
    let (left, right, bottom) = level.pit();
    add(
        "pit open water".into(),
        Rect::new(left, bottom, right, -700.0),
        Kind::OpenWater,
    );
    add(
        "crack chimney".into(),
        Rect::new(5371.0, -1500.0, 5500.0, level.crack.y),
        Kind::Tunnel,
    );
    add(
        "crack mouth".into(),
        Rect::new(5371.0, level.crack.y, 5500.0, level.crack.y + 40.0),
        Kind::Mouth,
    );

    let (_, octopus_den) = level
        .foes
        .iter()
        .find(|(kind, _)| *kind == enemy::Kind::Octopus)
        .expect("the authored level has an octopus den");
    // The pickup uses the octopus's home position plus four units (world.rs).
    let midden = *octopus_den + Vec2::new(0.0, 4.0);
    let egg = level.eggs[4];
    let cave = *level
        .secrets
        .iter()
        .find(|rect| rect.contains(egg))
        .expect("the fifth egg is inside the far-wall cave");
    for (name, rect, prize) in [
        ("octopus den", level.den, midden),
        ("far-wall cave", cave, egg),
    ] {
        let door = Rect::new(rect.min.x, rect.min.y, rect.min.x + 20.0, rect.max.y);
        add(name.into(), rect, Kind::Chamber { door, prize });
    }
    landmarks
}
