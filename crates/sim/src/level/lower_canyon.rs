//! One rock batch for the lower canyon and the future strata bodies.
//! Frames retain procedural substrate until W3.0; none is a playable tunnel yet.
use super::{Brittle, Climb, Growth, Level, WATER_SURFACE_Y};
use bevy_math::{Rect, Vec2};

pub(super) const DAM: Rect = Rect::new(17600.0, -10000.0, 17781.0, -2520.0);
pub(super) const NARROWS: Rect = Rect::new(18000.0, -10000.0, 18080.0, -9800.0);
pub(super) const VENTS: Rect = Rect::new(18300.0, -10000.0, 20800.0, -9000.0);
pub(super) const GARDEN: Rect = Rect::new(5640.0, -1100.0, 5810.0, -930.0);

/// Interiors, separated by forty units of bedrock. Long connections are
/// reserved under the heightmap, never roofs replacing an existing floor.
pub(super) fn frames() -> Vec<(&'static str, Rect)> {
    vec![
        ("home burrow", Rect::new(80.0, -360.0, 680.0, -60.0)),
        ("mantis mouth", Rect::new(4760.0, -900.0, 4960.0, -720.0)),
        ("clay tunnel", Rect::new(4760.0, -2100.0, 4960.0, -940.0)),
        ("bone hall", Rect::new(4500.0, -2640.0, 5700.0, -2140.0)),
        ("seep shaft", Rect::new(5240.0, -2100.0, 5360.0, -740.0)),
        // The seep's upper landing is inside the existing near-wall pillar.
        // W4.1's wormhole joins the two ends without blocking the doorstep.
        ("seep outlet", Rect::new(5520.0, -1250.0, 5620.0, -1020.0)),
        ("long chimney", Rect::new(720.0, -12800.0, 880.0, -60.0)),
        (
            "chimney return",
            Rect::new(920.0, -12800.0, 18360.0, -12600.0),
        ),
        (
            "chimney hall",
            Rect::new(18400.0, -12800.0, 19000.0, -12400.0),
        ),
        (
            "chimney mouth",
            Rect::new(18840.0, -12360.0, 19000.0, -10020.0),
        ),
        (
            "odd tunnel",
            Rect::new(18700.0, -13000.0, 18900.0, -12840.0),
        ),
        (
            "Plunge shaft",
            Rect::new(18700.0, -14100.0, 18900.0, -13040.0),
        ),
        (
            "Plunge crack",
            Rect::new(18940.0, -14100.0, 19240.0, -13950.0),
        ),
        (
            "angler stage",
            Rect::new(19340.0, -14200.0, 20040.0, -13800.0),
        ),
    ]
}

/// Explicit removable closures for W3.0. These are separate rocks so opening
/// a mouth later does not require moving the outline or rebaking it.
pub(super) fn plugs(name: &str, r: Rect) -> Vec<Rect> {
    let mouths = match name {
        "home burrow" => vec![(0, 200.0), (3, -210.0)],
        "bone hall" => vec![(0, 4860.0), (0, 5300.0)],
        "long chimney" => vec![(0, 800.0), (2, -210.0), (3, -12700.0)],
        "chimney return" => vec![(2, -12700.0), (3, -12700.0)],
        "chimney hall" => vec![(2, -12700.0), (0, 18920.0), (1, 18800.0)],
        "Plunge shaft" => vec![(0, 18800.0), (3, -14025.0)],
        "Plunge crack" => vec![(2, -14025.0), (0, 19090.0)],
        "seep outlet" => vec![(1, 5570.0), (3, -1070.0)],
        _ => vec![(0, r.center().x), (1, r.center().x)],
    };
    mouths
        .into_iter()
        .map(|(side, c)| match side {
            0 => Rect::new(c - 50.0, r.max.y, c + 50.0, r.max.y + 20.0),
            1 => Rect::new(c - 50.0, r.min.y - 20.0, c + 50.0, r.min.y),
            2 => Rect::new(r.min.x - 20.0, c - 50.0, r.min.x, c + 50.0),
            _ => Rect::new(r.max.x, c - 50.0, r.max.x + 20.0, c + 50.0),
        })
        .collect()
}

pub(super) fn shape(level: &mut Level) {
    level.max_x = 21000.0;
    level.floor.extend([
        Vec2::new(17700.0, -2520.0),
        Vec2::new(17780.0, -2520.0),
        Vec2::new(17781.0, -10000.0),
        Vec2::new(21000.0, -10000.0),
    ]);
    // A sediment heap rises eighty units beyond Coral Rise, then collapses
    // into a sheer face. The old heightmap and its outer bank stay intact.
    // The lock is a slot in a buttress rising to the water surface, like the
    // shallow sea arch: no high glide or excavated sill bypasses the crusher.
    level.rocks.extend([
        Rect::new(17960.0, -10300.0, 18120.0, -10000.0),
        Rect::new(17960.0, -9800.0, 18120.0, WATER_SURFACE_Y),
    ]);
    level.brittle.push(Brittle {
        rect: NARROWS,
        cracked: false,
    });
    for x in [18380.0, 19550.0, 20720.0] {
        level.climbs.push(Climb {
            x,
            bottom: -10000.0,
            top: -9800.0,
            kind: Growth::Coral,
        });
    }
    for (name, r) in frames() {
        if name == "seep outlet" {
            // Hollow only this sealed interior inside the old pillar. Its
            // outer collision surfaces, including the underpass, stay put.
            level.rocks = level
                .rocks
                .iter()
                .flat_map(|rock| {
                    let cut = rock.intersect(r);
                    if cut.is_empty() {
                        return vec![*rock];
                    }
                    [
                        Rect::new(rock.min.x, rock.min.y, cut.min.x, rock.max.y),
                        Rect::new(cut.max.x, rock.min.y, rock.max.x, rock.max.y),
                        Rect::new(cut.min.x, rock.min.y, cut.max.x, cut.min.y),
                        Rect::new(cut.min.x, cut.max.y, cut.max.x, rock.max.y),
                    ]
                    .into_iter()
                    .filter(|piece| !piece.is_empty())
                    .collect()
                })
                .collect();
        }
        frame(level, name, r);
    }
    // The crack has a 100-unit thick viewing wall, with a twelve-unit slit
    // into a separately sealed dark stage. A crab cannot fit through it.
    for (x0, x1, y0, y1) in [
        (19240.0, 19260.0, -14120.0, -13930.0),
        (19320.0, 19340.0, -14220.0, -13780.0),
    ] {
        level.rocks.retain(|r| *r != Rect::new(x0, y0, x1, y1));
        level.rocks.extend([
            Rect::new(x0, y0, 19340.0, -14031.0),
            Rect::new(x0, -14019.0, 19340.0, y1),
        ]);
    }
    // A small ledge attached to the existing near wall, above the doorstep.
    level
        .rocks
        .push(Rect::new(5640.0, -1130.0, 5810.0, -1100.0));
    level.climbs.push(Climb {
        x: 5700.0,
        bottom: -1100.0,
        top: -930.0,
        kind: Growth::Coral,
    });
}

fn frame(level: &mut Level, name: &str, r: Rect) {
    let mouths = plugs(name, r);
    for (wall, horizontal) in [
        (
            Rect::new(r.min.x - 20.0, r.min.y - 20.0, r.min.x, r.max.y + 20.0),
            false,
        ),
        (
            Rect::new(r.max.x, r.min.y - 20.0, r.max.x + 20.0, r.max.y + 20.0),
            false,
        ),
        (Rect::new(r.min.x, r.min.y - 20.0, r.max.x, r.min.y), true),
        (Rect::new(r.min.x, r.max.y, r.max.x, r.max.y + 20.0), true),
    ] {
        let mut pieces = vec![wall];
        for mouth in &mouths {
            pieces = pieces
                .into_iter()
                .flat_map(|piece| {
                    let cut = piece.intersect(*mouth);
                    if cut.is_empty() {
                        return vec![piece];
                    }
                    let mut before = piece;
                    let mut after = piece;
                    if horizontal {
                        before.max.x = cut.min.x;
                        after.min.x = cut.max.x;
                    } else {
                        before.max.y = cut.min.y;
                        after.min.y = cut.max.y;
                    }
                    [before, after]
                        .into_iter()
                        .filter(|r| !r.is_empty())
                        .collect()
                })
                .collect();
        }
        level.rocks.extend(pieces);
    }
    for (i, mouth) in mouths.into_iter().enumerate() {
        if name == "long chimney" && i == 0 {
            // Backed by a solid cap until W3.0: cracking crust is not early access.
            level.brittle.push(Brittle {
                rect: mouth,
                cracked: false,
            });
            level.rocks.push(Rect::new(
                mouth.min.x,
                mouth.min.y - 20.0,
                mouth.max.x,
                mouth.min.y,
            ));
        } else {
            level.rocks.push(mouth);
        }
    }
}
