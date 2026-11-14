//! The trap-finder (#750): nowhere a crab can stand in the first-hour world holds it. A crab is set down on every
//! floor found in x 0 to 3,400 (the shallows, the trench and the Silt Galleries' upper gallery), every 20 units,
//! and must get at least 120 units away by one of the things a player can do: walk either way, hop either way,
//! climb (hold jump), or dig (up, down or sideways). Going home (the Home button) always works, so it isn't
//! counted. Foes are cleared: this is about the ground, not the hunters.
//!
//! Both run in CI (the required tests): the plain level (about 360 spots, under a second) and the cell world, where
//! sand moves and digging matters (about 210 spots, about 11 s with the sim optimised). Spots where a crab's box
//! would overlap rock or buried sediment are skipped: a crab can't be there. Each trap found is printed with how far
//! the best way out got.
use bevy_math::Vec2;
use crabsim::underwater::Cell;
use crabsim::{Controls, Level, PlayerId, Sim, Tuning};

const P: PlayerId = PlayerId(0);
/// How far a crab must get from where it was set down.
const OUT: f32 = 120.0;
/// Seconds per way out tried.
const TRY_SECS: u32 = 3;

/// Every way out a player has, as held controls (a hop is pressed every half second).
fn ways() -> Vec<(&'static str, Controls)> {
    let walk = |w: f32| Controls {
        walk: w,
        ..Default::default()
    };
    vec![
        ("walk left", walk(-1.0)),
        ("walk right", walk(1.0)),
        (
            "hop left",
            Controls {
                jump_pressed: true,
                jump_held: true,
                ..walk(-1.0)
            },
        ),
        (
            "hop right",
            Controls {
                jump_pressed: true,
                jump_held: true,
                ..walk(1.0)
            },
        ),
        (
            "climb",
            Controls {
                jump_held: true,
                jump_pressed: true,
                ..Default::default()
            },
        ),
        (
            "dig up",
            Controls {
                dig_held: true,
                dig_up: true,
                ..Default::default()
            },
        ),
        (
            "dig down",
            Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            },
        ),
        (
            "dig left",
            Controls {
                dig_held: true,
                ..walk(-1.0)
            },
        ),
        (
            "dig right",
            Controls {
                dig_held: true,
                ..walk(1.0)
            },
        ),
    ]
}

/// Every floor in the first-hour world: from a column of heights every 30 units, every 20 across, the ground straight below each.
fn floors(level: &Level) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::new();
    let mut x = 20.0;
    while x < 3400.0 {
        let mut y = 1500.0;
        while y > -1100.0 {
            if let Some(g) = level.ground_below(Vec2::new(x, y))
                && g > -1100.0
                && !out
                    .iter()
                    .any(|p| (p.x - x).abs() < 1.0 && (p.y - g).abs() < 30.0)
            {
                out.push(Vec2::new(x, g));
            }
            y -= 30.0;
        }
        x += 20.0;
    }
    out
}

/// Spots a crab set down there can't leave, with where it settled.
/// A trap: where the crab was set down, where it settled, and the farthest the best way out got (and which).
type Trap = (Vec2, Vec2, (f32, &'static str));

fn traps(base: &Sim) -> (usize, Vec<Trap>) {
    let ways = ways();
    let ticks = TRY_SECS * 120;
    let mut found = Vec::new();
    let spots = floors(&base.level);
    let mut tried = 0;
    for spot in spots {
        let mut settled = base.clone();
        settled.add_crab(P, spot + Vec2::Y);
        // Let it land (a spot on a slope or a lip slides a little).
        for _ in 0..60 {
            settled.tick();
        }
        let Some(start) = settled.crab(P).map(|c| c.mover.pos) else {
            continue;
        };
        // Set down overlapping rock or buried sediment (a floor under an overhang's lip, a filled shaft): not a
        // place a crab can be, so not a place it can be trapped.
        if !clear(&settled, start) {
            continue;
        }
        tried += 1;
        let mut best = (0.0_f32, "");
        let out = ways.iter().any(|(name, c)| {
            let mut sim = settled.clone();
            for i in 0..ticks {
                let mut c = *c;
                c.jump_pressed &= i % 60 == 0;
                sim.set_controls(P, c);
                sim.tick();
                let d = sim.crab(P).map_or(0.0, |k| k.mover.pos.distance(start));
                if d > best.0 {
                    best = (d, *name);
                }
                if d > OUT {
                    return true;
                }
            }
            false
        });
        if !out {
            found.push((spot, start, best));
        }
    }
    (tried, found)
}

/// Is the crab's whole box at `at` open water (nothing solid in it, in the cells where there are cells)?
fn clear(sim: &Sim, at: Vec2) -> bool {
    let Some(m) = sim.crab(P).map(|c| &c.mover) else {
        return false;
    };
    let (half, height) = (m.half_width(&sim.tuning) - 2.0, m.height(&sim.tuning));
    let mut y = at.y + 3.0;
    while y < at.y + height {
        let mut x = at.x - half;
        while x <= at.x + half {
            let p = Vec2::new(x, y);
            let solid = match sim.level.cells.as_ref() {
                Some(w) if w.contains_world(x, y) => {
                    let (cx, cy) = w.local_cell(x, y);
                    w.cell(cx.floor() as i32, cy.floor() as i32) != Cell::Water
                }
                _ => sim.level.solid_at(p),
            };
            if solid {
                return false;
            }
            x += 4.0;
        }
        y += 4.0;
    }
    true
}

fn world(cells: bool) -> Sim {
    let mut level = Level::test_level();
    level.foes.clear();
    let mut sim = Sim::new(Tuning::default(), level);
    if cells {
        sim.enable_underwater_cells();
    }
    sim
}

fn check(cells: bool) {
    let started = std::time::Instant::now();
    let (tried, found) = traps(&world(cells));
    println!(
        "{} spots tried ({}), {} traps, {:.1} s",
        tried,
        if cells { "cell world" } else { "plain level" },
        found.len(),
        started.elapsed().as_secs_f32()
    );
    for (spot, at, (far, how)) in &found {
        println!("  trap: set down at {spot}, settled at {at}; got {far:.0} away at best ({how})");
    }
    assert!(tried > 100, "too few spots tried: {tried}");
    assert!(found.is_empty(), "{} spots hold a crab", found.len());
}

#[test]
fn no_floor_in_the_first_hour_world_traps_a_crab() {
    check(false);
}

#[test]
fn no_floor_in_the_first_hour_cell_world_traps_a_crab() {
    check(true);
}
