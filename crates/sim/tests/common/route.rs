//! Shared fixed-kit route search for the Plate D map.

use bevy_math::Vec2;
use crabsim::moult::Feat;
use crabsim::{Controls, Feel, Level, PlayerId, Sim, Tuning};
use std::collections::HashMap;

pub const ME: PlayerId = PlayerId(0);
pub const DT: f64 = 1.0 / 120.0;

/// What a crab is given before a run.
#[derive(Clone, Copy, Debug, Default)]
pub struct Kit {
    pub eggs: u32,
    pub scissors: bool,
    pub glass: bool,
    pub crusher: bool,
    pub dactyls: bool,
}

impl Kit {
    pub const fn eggs(eggs: u32) -> Self {
        Kit {
            eggs,
            scissors: false,
            glass: false,
            crusher: false,
            dactyls: false,
        }
    }
    pub fn name(&self) -> String {
        let mut s = format!("{} eggs", self.eggs);
        for (on, n) in [
            (self.scissors, "scissors"),
            (self.glass, "glass"),
            (self.crusher, "crusher"),
            (self.dactyls, "dactyls"),
        ] {
            if on {
                s += " + ";
                s += n;
            }
        }
        s
    }
}

/// The key places, as boxes (x0, x1, y0, y1) on the Plate D map: the world
/// steps down to the right (home shelf, basin, the hub behind the net, the
/// canyon's floor at -6,000, the far rim 2,000 lower than it was, the eastern
/// step and Coral Rise, the lower canyon deepest). The first box that holds a
/// point names it.
pub const PLACES: &[(&str, [f32; 4])] = &[
    ("surface", [0.0, 1300.0, 4600.0, 6000.0]),
    ("trench floor", [1301.0, 1819.0, -300.0, -200.0]),
    ("far ledge", [1820.0, 2716.0, 0.0, 400.0]),
    ("cliff crest", [2717.0, 2800.0, 850.0, 1000.0]),
    ("basin", [2800.0, 3300.0, -720.0, 1000.0]),
    ("basin floor", [3300.0, 4354.0, -720.0, -500.0]),
    ("hub", [4490.0, 5371.0, -720.0, 1500.0]),
    ("ridge crest", [4300.0, 4370.0, 2990.0, 3200.0]),
    ("doorstep", [5371.0, 5900.0, -1520.0, -1000.0]),
    ("notch", [5500.0, 5700.0, 540.0, 800.0]),
    ("canyon shelf", [5900.0, 6150.0, -3420.0, -3000.0]),
    ("canyon floor", [6150.0, 7900.0, -6020.0, -5600.0]),
    ("far-wall cave", [7700.0, 7901.0, -5210.0, -5030.0]),
    ("far rim", [7900.0, 8800.0, -3920.0, 1100.0]),
    ("eastern step", [15800.0, 17600.0, -5200.0, -3000.0]),
    ("slump dam", [17600.0, 17781.0, -4600.0, -4400.0]),
    ("lower canyon", [17781.0, 30000.0, -10100.0, -4400.0]),
    ("east of the rim", [8800.0, 17600.0, -20000.0, 20000.0]),
];

pub fn place_of(p: Vec2) -> Option<&'static str> {
    PLACES
        .iter()
        .find(|(_, [x0, x1, y0, y1])| p.x >= *x0 && p.x <= *x1 && p.y >= *y0 && p.y <= *y1)
        .map(|(n, _)| *n)
}

/// Home's side of the trench, on the seabed: where a trip ends.
fn home_side(p: Vec2) -> bool {
    p.x < 1300.0 && p.y > -10.0 && p.y < 1000.0
}

/// One move, as a schedule of controls from its start.
#[derive(Clone, Copy, Debug)]
pub enum Move {
    /// A directed test edge, with an instantaneous reserve charge.
    Wormhole { to: Vec2, reserve_cost: f32 },
    /// Hold a direction this long (on and off edges, until it lands).
    Walk(f32, f32),
    /// Jump (or, on a stalk, kick off) the way `dir`, then `strokes` paddle
    /// strokes on the rhythm, holding `dir` until `hold_for` seconds, then
    /// `after` until it lands.
    Jump {
        dir: f32,
        strokes: u32,
        hold_for: f32,
        after: f32,
    },
    /// On a stalk: climb up (or down) this long.
    Climb(bool, f32),
    /// Stand still this long (the legs rest, the tide turns).
    Wait(f32),
}

impl Move {
    fn controls(&self, t: f64) -> Controls {
        let press = |at: f64| t >= at && t < at + DT * 0.99;
        match *self {
            Move::Walk(dir, secs) => Controls {
                walk: if t < f64::from(secs) { dir } else { 0.0 },
                ..Default::default()
            },
            Move::Jump {
                dir,
                strokes,
                hold_for,
                after,
            } => {
                let mut pressed = press(0.0);
                for k in 0..strokes {
                    pressed |= press(0.45 + 0.31 * f64::from(k));
                }
                Controls {
                    walk: if t < f64::from(hold_for) { dir } else { after },
                    jump_pressed: pressed,
                    jump_held: t < 0.45 + 0.31 * f64::from(strokes),
                    ..Default::default()
                }
            }
            Move::Climb(up, secs) => Controls {
                jump_held: up && t < f64::from(secs),
                down: !up && t < f64::from(secs),
                ..Default::default()
            },
            Move::Wait(_) | Move::Wormhole { .. } => Controls::default(),
        }
    }
    fn active(&self) -> f64 {
        match *self {
            Move::Walk(_, s) | Move::Climb(_, s) | Move::Wait(s) => f64::from(s),
            Move::Jump { strokes, .. } => 0.45 + 0.31 * f64::from(strokes),
            Move::Wormhole { .. } => 0.0,
        }
    }
    /// What it holds once its schedule is over and the crab is still moving.
    fn trailing(&self) -> f32 {
        match *self {
            Move::Walk(dir, _) => dir,
            Move::Jump { after, .. } => after,
            _ => 0.0,
        }
    }
}

/// A crab at rest somewhere, and how it got there.
#[derive(Clone)]
pub struct Node {
    pub sim: Sim,
    parent: Option<usize>,
    how: Option<Move>,
    /// Seconds since the start.
    pub time: f64,
    /// The lowest the reserve has been on the way (1 = full).
    pub min_reserve: f32,
    /// Paddle strokes made on the way.
    pub strokes: u32,
    /// Seconds with tired paddle legs (less than one stroke left) while
    /// swimming on the way.
    pub dry: f64,
    lowest: f32,
}

impl Node {
    pub fn mover(&self) -> &crabsim::Mover {
        &self.sim.crab(ME).expect("the crab").mover
    }

    /// This crab, at rest where it is, as the start of a new survey.
    pub fn restart(&self) -> Node {
        Node {
            parent: None,
            how: None,
            ..self.clone()
        }
    }
}

pub fn world(kit: Kit, start: Vec2, foes: bool) -> Sim {
    let mut level = Level::test_level();
    if !foes {
        level.foes.clear();
    }
    // Mapping is observational and cannot affect locomotion. Avoid rebuilding
    // personal map pages on every candidate route. Keep all physics unchanged.
    let mut tuning = Tuning::default();
    tuning.memory.step_ticks = u32::MAX;
    let mut sim = Sim::new(tuning, level);
    let tuning = sim.tuning.clone();
    sim.set_feel(Feel::Current);
    sim.add_crab(ME, start);
    let crab = sim.crab_mut(ME).expect("the crab");
    // Nothing on the way changes the kit: eggs eaten, finds taken.
    crab.eaten.iter_mut().for_each(|e| *e = true);
    crab.teeth_taken.iter_mut().for_each(|e| *e = true);
    crab.sponges_taken.iter_mut().for_each(|e| *e = true);
    crab.cultures_taken.iter_mut().for_each(|e| *e = true);
    let m = &mut crab.mover;
    m.upgrades = kit.eggs;
    m.scissor_claw = kit.scissors;
    m.glass_grips = kit.glass;
    if kit.crusher {
        m.feats.push(Feat::HeavyCrusher);
    }
    if kit.dactyls {
        m.feats.push(Feat::GrippingDactyls);
    }
    m.stamina = m.strokes(&tuning) as f32;
    assert!(foes || sim.enemies().is_empty());
    sim
}

fn at_rest(m: &crabsim::Mover) -> bool {
    (m.grounded || m.climbing) && m.vel.length() < 8.0
}

/// Play `mv` from `from`; the node it comes to rest at, if it does.
fn play(from: &Node, at: usize, mv: Move, visit: &mut impl FnMut(Vec2)) -> Option<Node> {
    let mut node = Node {
        sim: from.sim.clone(),
        parent: Some(at),
        how: Some(mv),
        time: from.time,
        min_reserve: from.min_reserve,
        strokes: from.strokes,
        dry: from.dry,
        lowest: from.lowest,
    };
    if let Move::Wormhole { to, reserve_cost } = mv {
        let m = &mut node.sim.crab_mut(ME)?.mover;
        m.pos = to;
        m.vel = Vec2::ZERO;
        m.grounded = false;
        m.climbing = false;
        m.reserve = (m.reserve - reserve_cost).max(0.0);
        node.min_reserve = node.min_reserve.min(m.reserve);
        node.lowest = node.lowest.min(to.y);
        return Some(node);
    }
    let (mut strokes, mut min_reserve, mut lowest, mut dry) =
        (node.strokes, node.min_reserve, node.lowest, node.dry);
    let sim = &mut node.sim;
    let mut t = 0.0;
    let mut still = 0;
    let limit = mv.active() + 30.0;
    while t < limit {
        let c = if t < mv.active() {
            mv.controls(t)
        } else {
            Controls {
                walk: mv.trailing(),
                ..Default::default()
            }
        };
        sim.set_controls(ME, c);
        sim.tick();
        t += DT;
        let crab = sim.crab(ME)?;
        let m = &crab.mover;
        visit(m.pos);
        if crab.events.paddled {
            strokes += 1;
        }
        min_reserve = min_reserve.min(m.reserve);
        lowest = lowest.min(m.pos.y);
        if !m.grounded && !m.climbing && m.stamina < 1.0 && m.upgrades > 0 {
            dry += DT;
        }
        if t >= mv.active() && at_rest(m) {
            still += 1;
            if still >= 12 {
                break;
            }
        } else {
            still = 0;
        }
    }
    if still < 12 {
        return None;
    }
    // A breath on the floor rests the paddle legs before the next move.
    if sim.crab(ME)?.mover.grounded {
        for _ in 0..120 {
            sim.set_controls(ME, Controls::default());
            sim.tick();
            t += DT;
        }
    }
    node.time += t;
    node.strokes = strokes;
    node.min_reserve = min_reserve;
    node.lowest = lowest;
    node.dry = dry;
    Some(node)
}

fn moves(m: &crabsim::Mover, strokes: u32) -> Vec<Move> {
    let mut out = Vec::new();
    if m.climbing {
        for secs in [1.0, 4.0, 40.0] {
            out.push(Move::Climb(true, secs));
            out.push(Move::Climb(false, secs));
        }
    } else {
        for dir in [-1.0, 1.0] {
            out.push(Move::Walk(dir, 0.4));
            out.push(Move::Walk(dir, 2.0));
        }
        // The tide only matters in the canyon.
        if (5900.0..7900.0).contains(&m.pos.x) {
            out.push(Move::Wait(8.0));
            out.push(Move::Wait(20.0));
        }
    }
    // Straight up first, then steer (onto a ledge overhead).
    if !m.climbing {
        for dir in [-1.0, 1.0] {
            for wait in [0.3, 0.6] {
                out.push(Move::Jump {
                    dir: 0.0,
                    strokes,
                    hold_for: wait,
                    after: dir,
                });
            }
        }
    }
    for dir in [-1.0f32, 0.0, 1.0] {
        if m.climbing && dir == 0.0 {
            continue;
        }
        for k in 0..=strokes {
            out.push(Move::Jump {
                dir,
                strokes: k,
                hold_for: 60.0,
                after: dir,
            });
            if dir != 0.0 {
                // Push off, then let the glide go: straight down.
                out.push(Move::Jump {
                    dir,
                    strokes: k,
                    hold_for: 0.45 + 0.31 * k as f32 + 0.3,
                    after: 0.0,
                });
            }
        }
    }
    out
}

fn key(m: &crabsim::Mover) -> (i32, i32, bool) {
    (
        (m.pos.x / 80.0).round() as i32,
        (m.pos.y / 80.0).round() as i32,
        m.climbing,
    )
}

pub struct Survey {
    wormholes: Vec<Wormhole>,
    pub nodes: Vec<Node>,
}

impl Survey {
    pub fn reached(&self, place: &str) -> Option<usize> {
        // The quickest arrival.
        (0..self.nodes.len())
            .filter(|&i| place_of(self.nodes[i].mover().pos) == Some(place))
            .min_by(|&a, &b| self.nodes[a].time.total_cmp(&self.nodes[b].time))
    }
    fn route(&self, mut i: usize) -> Vec<(Move, Vec2)> {
        let mut out = Vec::new();
        while let (Some(p), Some(mv)) = (self.nodes[i].parent, self.nodes[i].how) {
            out.push((mv, self.nodes[i].mover().pos));
            i = p;
        }
        out.reverse();
        out
    }
    pub fn describe(&self, i: usize) -> String {
        self.route(i)
            .iter()
            .map(|(mv, p)| {
                let at = place_of(*p).unwrap_or("home side");
                format!("{mv:?} -> {at} ({:.0}, {:.0})", p.x, p.y)
            })
            .collect::<Vec<_>>()
            .join("\n      ")
    }
}

/// Every place the crab can come to rest from `start`, breadth first.
pub fn survey_from(start: Node, kit: Kit, stop: impl Fn(&Node) -> bool, budget: usize) -> Survey {
    survey_from_with_wormholes(start, kit, stop, budget, &[])
}

fn survey_from_with_wormholes(
    start: Node,
    kit: Kit,
    stop: impl Fn(&Node) -> bool,
    budget: usize,
    wormholes: &[Wormhole],
) -> Survey {
    survey_observed(start, kit, stop, budget, wormholes, &mut |_| {})
}

pub fn survey_observed(
    start: Node,
    kit: Kit,
    stop: impl Fn(&Node) -> bool,
    budget: usize,
    wormholes: &[Wormhole],
    visit: &mut impl FnMut(Vec2),
) -> Survey {
    visit(start.mover().pos);
    let mut nodes = vec![start];
    let mut seen: HashMap<(i32, i32, bool), usize> = HashMap::new();
    seen.insert(key(nodes[0].mover()), 0);
    let mut next = 0;
    while next < nodes.len() && nodes.len() < budget {
        let here = nodes[next].clone();
        if stop(&here) {
            next += 1;
            continue;
        }
        let edges = wormholes
            .iter()
            .filter(|edge| {
                let [x0, x1, y0, y1] = edge.from;
                let p = here.mover().pos;
                p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1 && (edge.needs)(&kit)
            })
            .map(|edge| Move::Wormhole {
                to: edge.to,
                reserve_cost: edge.reserve_cost,
            });
        for mv in moves(here.mover(), kit.eggs).into_iter().chain(edges) {
            if let Some(n) = play(&here, next, mv, visit) {
                let k = key(n.mover());
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(k) {
                    e.insert(nodes.len());
                    nodes.push(n);
                }
            }
        }
        next += 1;
    }
    assert!(
        next == nodes.len(),
        "route survey exhausted its {budget}-node budget"
    );
    Survey {
        nodes,
        wormholes: wormholes.to_vec(),
    }
}

pub fn start_node(kit: Kit, at: Vec2, foes: bool) -> Node {
    Node {
        sim: world(kit, at, foes),
        parent: None,
        how: None,
        time: 0.0,
        min_reserve: 1.0,
        strokes: 0,
        dry: 0.0,
        lowest: at.y,
    }
}

pub const HOME: Vec2 = Vec2::new(300.0, 0.0);
pub const BUDGET: usize = 4000;

pub fn survey(kit: Kit) -> Survey {
    survey_from(start_node(kit, HOME, false), kit, |_| false, BUDGET)
}

/// The way back home from where node `i` of `s` came to rest.
pub fn way_home(s: &Survey, i: usize, kit: Kit) -> Option<Node> {
    let mut start = s.nodes[i].clone();
    start.parent = None;
    start.how = None;
    let back = survey_from_with_wormholes(
        start,
        kit,
        |n| home_side(n.mover().pos),
        BUDGET,
        &s.wormholes,
    );
    (0..back.nodes.len())
        .filter(|&j| home_side(back.nodes[j].mover().pos))
        .min_by(|&a, &b| back.nodes[a].time.total_cmp(&back.nodes[b].time))
        .map(|j| back.nodes[j].clone())
}

/// A directed, instantaneous connection. Reserve cost is a fraction of a full
/// reserve, clamped at zero on arrival, as with ordinary reserve expenditure.
/// Add a separate edge for a return connection. Destinations are arrival positions.
#[derive(Clone, Copy)]
pub struct Wormhole {
    pub from: [f32; 4],
    pub to: Vec2,
    pub reserve_cost: f32,
    pub needs: fn(&Kit) -> bool,
}

pub fn survey_with_wormholes(kit: Kit, wormholes: &[Wormhole]) -> Survey {
    assert!(
        wormholes
            .iter()
            .all(|edge| edge.reserve_cost.is_finite() && edge.reserve_cost >= 0.0)
    );
    survey_from_with_wormholes(
        start_node(kit, HOME, false),
        kit,
        |_| false,
        BUDGET,
        wormholes,
    )
}
