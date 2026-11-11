//! Derived place labels for world-shape checks and sanctuary navigation.
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
    /// Sealed bedrock outline reserved for future strata.
    Frame,
    /// One-way overworld descent.
    Drop,
    /// Separately authored future mouth closure.
    Plug,
    Refuge,
    OpenWater,
    Tunnel,
    Mouth,
    Chamber {
        door: Rect,
        prize: Vec2,
    },
    Gate {
        needs: Needs,
    },
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
    // The existing rim is a broad, level landing outside the octopus's
    // doorway. A crab can stop here after backing out of the den.
    let rim = level.den.min.y;
    add(
        "far-rim landing".into(),
        Rect::new(7940.0, rim, level.den.min.x, rim + 60.0),
        Kind::Refuge,
    );
    // The four existing return ledges on the cliff's basin face are real
    // resting places, not new terrain. Their tops provide visible shelter.
    for (i, rock) in level
        .rocks
        .iter()
        .filter(|r| r.min.x >= 2810.0 && r.max.x <= 3050.0 && r.min.y >= -420.0 && r.max.y <= 540.0)
        .enumerate()
    {
        add(
            format!("basin return ledge {i}"),
            Rect::new(rock.min.x, rock.max.y, rock.max.x, rock.max.y + 60.0),
            Kind::Refuge,
        );
    }
    for (i, crevice) in level.crevices.iter().enumerate() {
        add(format!("crevice {i}"), crevice.door, Kind::Refuge);
    }
    for (i, climb) in level.climbs.iter().enumerate() {
        let (name, kind) = match climb.kind {
            Growth::Kelp => ("kelp stand", Kind::Refuge),
            Growth::Coral => ("coral stand", Kind::Refuge),
            Growth::Rope => ("royal rope", Kind::Refuge),
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
    // A zero-area net is an event marker (the lower canyon's tether switch),
    // not a gate.
    for (i, net) in level
        .nets
        .iter()
        .enumerate()
        .filter(|(_, n)| !n.rect.is_empty())
    {
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
            if brittle.rect == super::lower_canyon::NARROWS {
                "narrows lock".into()
            } else {
                format!("brittle rock {i}")
            },
            brittle.rect,
            Kind::Gate {
                needs: Needs::Crusher,
            },
        );
    }

    // The first tide column is the canyon. Widen it to each wall, stepping
    // inward below the shelf and far rim so these boxes stay out of bedrock.
    if let Some(&tide) = level.currents.first() {
        for (name, rect) in [
            (
                "canyon doorstep crossing",
                Rect::new(5901.0, -1900.0, level.den.min.x, tide.max.y),
            ),
            // From the shelf to the far rim's (lower) landing.
            (
                "canyon shelf crossing",
                Rect::new(5901.0, level.den.min.y, level.den.min.x, -1900.0),
            ),
            (
                "canyon deep water",
                Rect::new(6151.0, tide.min.y, 7700.0, -3400.0),
            ),
            ("basin", Rect::new(2801.0, -700.0, 3300.0, 860.0)),
            // The filled pit's stretch of the basin: its doors are the
            // mantis mouth's and the seep shaft's.
            ("basin floor", Rect::new(3300.0, -700.0, 4300.0, -450.0)),
            ("far basin", Rect::new(4370.0, -700.0, 5370.0, 560.0)),
        ] {
            add(name.into(), rect, Kind::OpenWater);
        }
    }
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

    // The octopus's den and the far-wall cave (a missing part is skipped:
    // other lanes call this on synthetic levels).
    let octopus_den = level
        .foes
        .iter()
        .find(|(kind, _)| *kind == enemy::Kind::Octopus)
        .map(|(_, den)| *den);
    if let Some(den) = octopus_den {
        // The pickup uses the octopus's home position plus four units (world.rs).
        let midden = den + Vec2::new(0.0, 4.0);
        let door = Rect::new(
            level.den.min.x,
            level.den.min.y,
            level.den.min.x + 20.0,
            level.den.max.y,
        );
        add(
            "octopus den".into(),
            level.den,
            Kind::Chamber {
                door,
                prize: midden,
            },
        );
    }
    if let Some(&egg) = level.eggs.get(4)
        && let Some(&cave) = level.secrets.iter().find(|rect| rect.contains(egg))
    {
        let door = Rect::new(cave.min.x, cave.min.y, cave.min.x + 20.0, cave.max.y);
        add(
            "far-wall cave".into(),
            cave,
            Kind::Chamber { door, prize: egg },
        );
    }
    for (name, rect, kind) in [
        ("slump dam", super::lower_canyon::DAM, Kind::Drop),
        ("vent field", super::lower_canyon::VENTS, Kind::OpenWater),
    ] {
        landmarks.push(Landmark {
            name: name.into(),
            rect,
            kind,
        });
    }
    let garden = super::lower_canyon::HANGING_GARDEN;
    landmarks.push(Landmark {
        name: "hanging garden".into(),
        rect: Rect::new(garden.x - 60.0, garden.bottom, garden.x + 60.0, garden.top),
        kind: Kind::Refuge,
    });
    for spec in super::lower_canyon::specs() {
        let pocket = level.pocket_refs().find(|p| p.name == spec.name);
        landmarks.push(Landmark {
            name: spec.name.into(),
            rect: spec.rect,
            kind: match pocket.map(|p| p.kind) {
                Some(super::strata::PocketKind::Tunnel) => Kind::Tunnel,
                Some(super::strata::PocketKind::Chamber) => {
                    let (door, prize) = spec.chamber.unwrap_or((spec.rect, spec.rect.center()));
                    Kind::Chamber { door, prize }
                }
                None => Kind::Frame,
            },
        });
        // A pocket's mouths are its doors; any other frame lists its gaps
        // (open ones as mouths, sealed ones as plugs).
        let doors: Vec<(Rect, bool)> = match pocket {
            Some(p) => p.mouths.iter().map(|m| (*m, true)).collect(),
            None => spec.gaps.iter().map(|g| (g.rect, g.open)).collect(),
        };
        for (i, (rect, open)) in doors.into_iter().enumerate() {
            let crust = spec.name == "long chimney" && i == 0;
            landmarks.push(Landmark {
                name: if crust {
                    "long chimney crust plug".into()
                } else if open {
                    format!("{} mouth {i}", spec.name)
                } else {
                    format!("{} plug {i}", spec.name)
                },
                // A door on the ground reaches up out of it, so a hunted
                // crab in the open water above sees it.
                rect: if open && rect.max.y + 30.0 >= level.floor_height(rect.center().x) {
                    Rect::new(
                        rect.min.x,
                        rect.min.y,
                        rect.max.x,
                        rect.max.y.max(level.floor_height(rect.center().x)) + 40.0,
                    )
                } else {
                    rect
                },
                kind: if open { Kind::Mouth } else { Kind::Plug },
            });
        }
    }
    landmarks
}
