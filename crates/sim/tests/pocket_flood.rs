//! Lane V, pockets as data: a flood fill over the baked cells proves, for
//! every pocket of the Plate D map, that its frame is sealed bedrock but for
//! its doors (a dig from inside never reaches a place the doors don't), and
//! that from every sampled point of it at least two exits (or one exit and a
//! free ride out) are within a bounded way, counting the water that swims and
//! the sediment that digs.
use bevy_math::{Rect, Vec2};
use crabsim::Level;
use crabsim::level::strata::Pocket;
use crabsim::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Flood grid: 16 units, four game cells.
const G: f32 = 16.0;
/// How far outside a door still counts as the open ground.
const APRON: f32 = 60.0;
/// What a unit of sediment costs against a unit of water (digging is slower
/// than swimming).
const DIG: u32 = 4;

type Node = (i32, i32);

fn centre(n: Node) -> Vec2 {
    Vec2::new((n.0 as f32 + 0.5) * G, (n.1 as f32 + 0.5) * G)
}

fn node_at(p: Vec2) -> Node {
    ((p.x / G).floor() as i32, (p.y / G).floor() as i32)
}

/// The cost of entering a node: None for bedrock or anything a crab's body
/// could not pass (a probe in each direction must also be free).
fn cost(level: &Level, n: Node) -> Option<u32> {
    let c = centre(n);
    let mut worst = 1;
    for d in [
        Vec2::ZERO,
        Vec2::X * 6.0,
        -Vec2::X * 6.0,
        Vec2::Y * 6.0,
        -Vec2::Y * 6.0,
    ] {
        let p = c + d;
        match initial_terrain_cell(
            level,
            (p.x / GAME_CELL).floor() as i32,
            ((p.y - GAME_BOTTOM) / GAME_CELL).floor() as i32,
        ) {
            Cell::Rock => return None,
            Cell::Water => {}
            Cell::Sand | Cell::Mud => worst = DIG,
        }
    }
    Some(worst)
}

/// The open ground at a door: a little to each side and below, and a short
/// way up (a door through the step's sand has a water shaft above it).
fn apron_of(door: Rect) -> Rect {
    Rect {
        min: door.min - Vec2::splat(APRON),
        max: door.max + Vec2::new(APRON, 240.0),
    }
}

struct Complex {
    pockets: Vec<usize>,
    /// Frames and shafts: the pocket's own ground.
    inside: Vec<Rect>,
    /// Doors that open to the open ground, and the pocket each belongs to.
    exits: Vec<(Rect, usize)>,
}

fn complexes(level: &Level, pockets: &[Pocket]) -> Vec<Complex> {
    let inflate = |r: Rect, by: f32| Rect {
        min: r.min - Vec2::splat(by),
        max: r.max + Vec2::splat(by),
    };
    // Pockets joined by a door that lands in another's frame are one complex.
    let mut parent: Vec<usize> = (0..pockets.len()).collect();
    fn root(parent: &mut [usize], i: usize) -> usize {
        if parent[i] != i {
            let r = root(parent, parent[i]);
            parent[i] = r;
        }
        parent[i]
    }
    let mut internal = vec![vec![false; 4]; pockets.len()];
    for (i, p) in pockets.iter().enumerate() {
        for (m, mouth) in p.mouths.iter().enumerate() {
            for (j, q) in pockets.iter().enumerate() {
                if i != j && !inflate(q.frame, 25.0).intersect(*mouth).is_empty() {
                    let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                    parent[a] = b;
                    internal[i][m] = true;
                }
            }
        }
    }
    let mut out: HashMap<usize, Complex> = HashMap::new();
    for i in 0..pockets.len() {
        let r = root(&mut parent, i);
        let c = out.entry(r).or_insert_with(|| Complex {
            pockets: vec![],
            inside: vec![],
            exits: vec![],
        });
        c.pockets.push(i);
        c.inside.push(pockets[i].frame);
        for body in level
            .strata()
            .iter()
            .filter(|b| b.frame == pockets[i].frame)
        {
            // Gaps, shafts (the sediment up to the floor) and voids.
            c.inside.extend(body.mouths.iter().map(|m| {
                Rect::new(
                    m.min.x,
                    m.min.y,
                    m.max.x,
                    m.max.y.min(-4.0).max(m.min.y + 1.0),
                )
            }));
        }
        // The doors themselves (the bridges over the bedrock between frames).
        c.inside.extend(pockets[i].mouths.iter().copied());
        for (m, mouth) in pockets[i].mouths.iter().enumerate() {
            if !internal[i][m] {
                c.exits.push((*mouth, i));
            }
        }
    }
    let mut v: Vec<_> = out.into_values().collect();
    v.sort_by_key(|c| c.pockets[0]);
    v
}

struct Field {
    /// The nodes the flood is allowed to visit and what each costs.
    cells: HashMap<Node, u32>,
    leaks: Vec<Vec2>,
}

/// Every non-bedrock node reachable from `from` that lies inside the complex
/// or an apron of one of its exits; a reachable node anywhere else is a leak.
fn explore(level: &Level, c: &Complex, from: &[Node]) -> Field {
    let inside = |p: Vec2| c.inside.iter().any(|r| r.contains(p));
    let apron = |p: Vec2| c.exits.iter().any(|(m, _)| apron_of(*m).contains(p));
    let mut cells = HashMap::new();
    let mut leaks = Vec::new();
    let mut stack: Vec<Node> = from.to_vec();
    while let Some(n) = stack.pop() {
        if cells.contains_key(&n) {
            continue;
        }
        let Some(w) = cost(level, n) else { continue };
        if !inside(centre(n)) {
            if apron(centre(n)) {
                // The open ground at a door: the flood ends here.
                cells.insert(n, w);
            } else {
                leaks.push(centre(n));
                if leaks.len() > 5 {
                    break;
                }
            }
            continue;
        }
        cells.insert(n, w);
        for d in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            stack.push((n.0 + d.0, n.1 + d.1));
        }
    }
    Field { cells, leaks }
}

fn dijkstra(field: &Field, seeds: &[Node]) -> HashMap<Node, u32> {
    let mut dist: HashMap<Node, u32> = HashMap::new();
    let mut heap = BinaryHeap::new();
    for &s in seeds {
        if field.cells.contains_key(&s) {
            dist.insert(s, 0);
            heap.push(Reverse((0, s)));
        }
    }
    while let Some(Reverse((d, n))) = heap.pop() {
        if dist.get(&n).is_some_and(|&b| b < d) {
            continue;
        }
        for dd in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let m = (n.0 + dd.0, n.1 + dd.1);
            let Some(&w) = field.cells.get(&m) else {
                continue;
            };
            let nd = d + w;
            if dist.get(&m).is_none_or(|&b| nd < b) {
                dist.insert(m, nd);
                heap.push(Reverse((nd, m)));
            }
        }
    }
    dist
}

/// How far (weighted: a unit of sediment costs [`DIG`] units of water) a crab
/// has to go from any point of a pocket to an exit, in the pockets; the long
/// ducts, whose length is the point of them, are bounded by their length.
const WAY_OUT: f32 = 4000.0;
const DUCT_WAY_OUT: f32 = 40_000.0;

/// An exit that opens only into another sealed frame (the odd tunnel, the
/// Plunge's dead end) or behind a brittle crust is not an open door.
fn open_door(level: &Level, frames: &[Rect], door: Rect) -> bool {
    let gated = level
        .brittle
        .iter()
        .any(|b| !b.rect.intersect(door).is_empty());
    let dead_end = frames
        .iter()
        .any(|f| !f.intersect(apron_of(door)).is_empty());
    !gated && !dead_end
}

struct Report {
    leaks: usize,
    open_doors: usize,
    worst_second: u32,
    samples: usize,
}

fn prove(level: &Level, pockets: &[Pocket], c: &Complex, ducts: bool) -> Report {
    let mut seeds = Vec::new();
    for r in &c.inside {
        let (a, b) = (node_at(r.min), node_at(r.max));
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                seeds.push((x, y));
            }
        }
    }
    let field = explore(level, c, &seeds);
    // A silt shaft in the floor's own sand is open to it on both sides by
    // design (the home burrow's); only the frames below are bedrock.
    let frames: Vec<Rect> = c.pockets.iter().map(|&i| pockets[i].frame).collect();
    let leaks = field
        .leaks
        .iter()
        .filter(|p| !frames.iter().any(|f| p.y > f.max.y))
        .count();
    let others: Vec<Rect> = level
        .landmarks()
        .into_iter()
        .filter(|l| matches!(l.kind, crabsim::level::landmarks::Kind::Frame))
        // Open ground that is only named (the shrimp row, the shelf...) is not
        // a sealed frame.
        .filter(|l| {
            !crabsim::level::shallows::GROUND
                .iter()
                .any(|(n, _)| *n == l.name)
        })
        .map(|l| l.rect)
        .collect();
    let doors: Vec<Rect> = c
        .exits
        .iter()
        .map(|(m, _)| *m)
        .filter(|m| open_door(level, &others, *m))
        .collect();
    let dists: Vec<HashMap<Node, u32>> = doors
        .iter()
        .map(|m| {
            let door = Rect {
                min: m.min - Vec2::splat(8.0),
                max: m.max + Vec2::splat(8.0),
            };
            let seeds: Vec<Node> = field
                .cells
                .keys()
                .copied()
                .filter(|&n| door.contains(centre(n)))
                .collect();
            dijkstra(&field, &seeds)
        })
        .collect();
    let bound = if ducts { DUCT_WAY_OUT } else { WAY_OUT };
    let (mut worst_second, mut samples) = (0, 0);
    for &i in &c.pockets {
        let f = pockets[i].frame;
        for (&n, &w) in &field.cells {
            let p = centre(n);
            // A coarse grid of the water inside the pocket.
            if w != 1 || !f.contains(p) || n.0 % 6 != 0 || n.1 % 6 != 0 {
                continue;
            }
            samples += 1;
            let mut ways: Vec<u32> = dists.iter().filter_map(|d| d.get(&n).copied()).collect();
            ways.sort_unstable();
            // A dead end (one door) is held to its one way; the rest to two.
            let need = doors.len().min(2);
            let way = ways.get(need - 1).copied().unwrap_or(u32::MAX);
            let units = way as f32 * G;
            worst_second = worst_second.max(units as u32);
            assert!(
                units <= bound,
                "{}: from {p:?} way out number {need} is {units} units (bound {bound})",
                pockets[i].name
            );
        }
    }
    Report {
        leaks,
        open_doors: doors.len(),
        worst_second,
        samples,
    }
}

/// Every pocket of the map: a dig from inside never gets out but by a door
/// (the flood meets bedrock everywhere else), and from every sampled water
/// point two open doors are within a bounded way (path length in units, the
/// sediment counted at four times).
#[test]
fn every_pocket_is_sealed_but_for_its_doors_and_has_two_ways_out() {
    let level = Level::test_level();
    let pockets = level.pockets();
    assert_eq!(pockets.len(), 17);
    let mut dead_ends = Vec::new();
    for c in complexes(&level, &pockets) {
        let names: Vec<_> = c.pockets.iter().map(|&i| pockets[i].name).collect();
        let ducts = names
            .iter()
            .any(|n| matches!(*n, "long chimney" | "chimney return"));
        let r = prove(&level, &pockets, &c, ducts);
        println!(
            "{names:?}: {} open doors, {} leaks, {} samples, worst second way out {} units",
            r.open_doors, r.leaks, r.samples, r.worst_second
        );
        assert_eq!(r.leaks, 0, "{names:?} leaks through its bedrock");
        assert!(r.samples > 0, "{names:?} has no water to stand in");
        if r.open_doors < 2 {
            dead_ends.push(names);
        }
    }
    // No dead ends. (The crust vault and the warm seam behind its crust had
    // a single door on the vent field's floor; the vault's duct and shaft are
    // its second. The seam stays behind the crust: no door of its own.)
    assert!(dead_ends.is_empty(), "{dead_ends:?}");
}

/// The proof can fail: told the bone hall is smaller than it is, the flood
/// walks out through the water it is not allowed to enter.
#[test]
fn the_flood_notices_a_hole() {
    let level = Level::test_level();
    let pockets = level.pockets();
    let mut c = complexes(&level, &pockets)
        .into_iter()
        .find(|c| c.pockets.iter().any(|&i| pockets[i].name == "bone hall"))
        .expect("the bone hall complex");
    let hall = pockets
        .iter()
        .find(|p| p.name == "bone hall")
        .expect("hall")
        .frame;
    let seeds: Vec<Node> = vec![node_at(hall.center())];
    assert_eq!(explore(&level, &c, &seeds).leaks.len(), 0);
    c.inside.retain(|r| *r != hall);
    c.inside
        .retain(|r| r.min.y > hall.max.y || r.max.y < hall.min.y);
    assert!(!explore(&level, &c, &seeds).leaks.is_empty());
}
